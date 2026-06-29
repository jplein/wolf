//! The `redraw` event handler: folds Neovim's redraw stream into the shared
//! [`Snapshot`](crate::render::Snapshot).
//!
//! Implements [`nvim_rs::Handler`] over the child-process writer. On each
//! `redraw` notification it takes the snapshot write lock **once** for the whole
//! batch, applies every event, and — on `flush` — bumps the sequence number,
//! **drops the lock**, and only then fires the front-end wake callback. The lock
//! is never held across an `.await`.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use nvim_rs::compat::tokio::Compat;
use nvim_rs::{Handler, Neovim};
use parking_lot::{Mutex, RwLockWriteGuard};
use rmpv::Value;
use tokio::process::ChildStdin;

use crate::layout::LayoutIntent;
use crate::render::{
    Cell, CursorShape, DefaultColors, HlAttr, ModeInfo, Rgb, SharedSnapshot, Snapshot, Viewport,
    WinInfo,
};

/// A shared, drainable queue of [`LayoutIntent`]s.
///
/// Owned by [`crate::api::WolfCore`] (drained via `drain_intents`) and cloned
/// into the [`RedrawHandler`], which pushes onto it when a `wolf_layout`
/// notification arrives. It is a sibling of the render snapshot (NOT part of it):
/// intents are out-of-band events, not redraw frames, so pushing one does not
/// bump the snapshot `seq`.
pub type IntentQueue = Arc<Mutex<VecDeque<LayoutIntent>>>;

/// The front-end wake callback, invoked once per `flush` (after the lock drops).
///
/// May run on a tokio worker thread, hence `Send + Sync`; the front-end is
/// responsible for marshalling onto its UI thread.
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// Name of the out-of-band notification carrying the active buffer's absolute
/// path (`expand('%:p')`), emitted by the `WolfCurrentFile` autocmd in
/// `runtime/nvim/init.lua`. Routed separately from `redraw` in
/// [`RedrawHandler::handle_notify`].
const CURRENT_FILE_EVENT: &str = "wolf_current_file";

/// Name of the out-of-band notification carrying the active window handle,
/// emitted by the `WolfLayout` `WinEnter` autocmd in `runtime/nvim/init.lua`.
/// Routed separately from `redraw` in [`RedrawHandler::handle_notify`].
const ACTIVE_WIN_EVENT: &str = "wolf_active_win";

/// Name of the out-of-band notification carrying a layout intent payload (a
/// split or a focus-direction move), emitted by the chord/Ex-command overrides
/// in `runtime/nvim/init.lua`. Routed separately from `redraw`.
const LAYOUT_EVENT: &str = "wolf_layout";

/// `nvim-rs` [`Handler`] that applies redraw batches to a [`SharedSnapshot`].
#[derive(Clone)]
pub struct RedrawHandler {
    snapshot: SharedSnapshot,
    intents: IntentQueue,
    wake: WakeFn,
}

impl RedrawHandler {
    /// Creates a handler writing into `snapshot`, pushing layout intents onto
    /// `intents`, and waking via `wake`.
    #[must_use]
    pub fn new(snapshot: SharedSnapshot, intents: IntentQueue, wake: WakeFn) -> Self {
        Self {
            snapshot,
            intents,
            wake,
        }
    }
}

#[async_trait]
impl Handler for RedrawHandler {
    type Writer = Compat<ChildStdin>;

    async fn handle_notify(
        &self,
        name: String,
        args: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) {
        match name.as_str() {
            "redraw" => {
                let fired = self.apply_redraw(args).await;
                if fired {
                    (self.wake)();
                }
            }
            CURRENT_FILE_EVENT => {
                // `apply_current_file` acquires and DROPS the write lock before
                // returning, so the wake fires strictly after the lock is
                // released — same wake-after-unlock invariant as the redraw path.
                self.apply_current_file(args).await;
                (self.wake)();
            }
            ACTIVE_WIN_EVENT => {
                // `apply_active_win` acquires and DROPS the write lock before
                // returning; wake fires after the lock is released (same
                // wake-after-unlock invariant). This is NOT a redraw frame, so
                // `seq` is left unchanged.
                self.apply_active_win(args).await;
                (self.wake)();
            }
            LAYOUT_EVENT => {
                // A layout intent is out-of-band: parse it, push onto the sibling
                // intent queue (a brief Mutex, no snapshot lock, never held across
                // an `.await`), and wake so the front-end drains it. No `seq` bump.
                if let Some(intent) = args.first().and_then(LayoutIntent::from_payload) {
                    self.intents.lock().push_back(intent);
                }
                (self.wake)();
            }
            _ => {}
        }
    }
}

impl RedrawHandler {
    /// Applies a whole `redraw` batch under a single write lock and returns
    /// whether a `flush` was seen (so the caller can wake AFTER the lock drops).
    ///
    /// The write lock is acquired via [`SharedSnapshot::write_async`] so a held
    /// reader cannot pin this worker thread; once acquired it is NOT held across
    /// any `.await` (the whole batch is applied synchronously below).
    async fn apply_redraw(&self, batch: Vec<Value>) -> bool {
        let mut snap = self.snapshot.write_async().await;
        let mut flushed = false;

        for event in batch {
            let Value::Array(parts) = event else { continue };
            let mut it = parts.into_iter();
            let Some(Value::String(name)) = it.next() else {
                continue;
            };
            let Some(name) = name.into_str() else { continue };

            // The remaining items are one-or-more argument tuples, each an array.
            for call in it {
                let Value::Array(params) = call else { continue };
                if apply_event(&mut snap, &name, params) {
                    flushed = true;
                }
            }
        }

        if flushed {
            snap.seq = snap.seq.wrapping_add(1);
        }
        // Lock drops here as `snap` goes out of scope at function return.
        flushed
    }

    /// Applies a `wolf_current_file` notification: sets the snapshot's
    /// `current_file` to the canonicalized active-buffer path, or `None` for an
    /// unnamed buffer (empty/absent argument).
    ///
    /// The first argument is `expand('%:p')`. An empty string (unnamed buffer)
    /// maps to `None`; otherwise the path is canonicalized to match the
    /// canonical paths the file picker emits, falling back to the raw path if
    /// canonicalization fails (e.g. the file no longer exists).
    ///
    /// The write lock is acquired via [`SharedSnapshot::write_async`] and is NOT
    /// held across any `.await`; it is dropped before returning so the caller's
    /// wake fires after the lock is released.
    async fn apply_current_file(&self, args: Vec<Value>) {
        let target: Option<PathBuf> = match args.first().and_then(Value::as_str) {
            Some(p) if !p.is_empty() => {
                Some(std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p)))
            }
            _ => None,
        };

        let mut snap = self.snapshot.write_async().await;
        snap.current_file = target;
        // A current_file change is NOT a redraw frame, so `seq` (the redraw-frame
        // counter, advanced only on `flush`) is intentionally left unchanged. The
        // caller still fires the wake after this lock drops so the front-end can
        // re-read the snapshot.
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_active_win` notification: sets the snapshot's `active_win`
    /// to the reported window handle.
    ///
    /// The first argument is `nvim_api_nvim_get_current_win()` from the `WinEnter`
    /// autocmd. It arrives as a plain integer (a Lua number) or, defensively, as a
    /// msgpack-`Ext`-wrapped integer; either decodes via [`ext_handle`]. A handle
    /// we cannot decode leaves `active_win` unchanged.
    ///
    /// The write lock is acquired via [`SharedSnapshot::write_async`], is NOT held
    /// across any `.await`, and is dropped before returning so the caller's wake
    /// fires after the lock is released. This is NOT a redraw frame, so `seq` is
    /// left unchanged.
    async fn apply_active_win(&self, args: Vec<Value>) {
        let Some(win) = args.first().and_then(ext_handle) else {
            return;
        };
        let mut snap = self.snapshot.write_async().await;
        snap.active_win = Some(win);
        // Lock drops here as `snap` goes out of scope at function return.
    }
}

/// Applies one redraw event tuple. Returns `true` only for `flush`.
///
/// Multigrid routing: `grid_resize`/`grid_clear`/`grid_line`/`grid_scroll`/
/// `grid_cursor_goto` carry their target grid id in `params[0]`; the window
/// events (`win_pos`/`win_viewport`/`win_hide`/`win_close`/`grid_destroy`)
/// carry it likewise. Float and message-grid placement (`win_float_pos`,
/// `win_external_pos`, `msg_set_pos`) are intentionally ignored: floats and
/// external windows are not rendered, and the message/command-line content lives
/// on the outer grid (grid 1), which the front-end paints as a cmdline strip.
fn apply_event(snap: &mut RwLockWriteGuard<'_, Snapshot>, name: &str, params: Vec<Value>) -> bool {
    match name {
        "grid_resize" => grid_resize(snap, &params),
        "grid_clear" => grid_clear(snap, &params),
        "grid_line" => grid_line(snap, params),
        "grid_scroll" => grid_scroll(snap, &params),
        "grid_destroy" => grid_destroy(snap, &params),
        "hl_attr_define" => hl_attr_define(snap, params),
        "default_colors_set" => default_colors_set(snap, &params),
        "mode_info_set" => mode_info_set(snap, params),
        "mode_change" => mode_change(snap, &params),
        "grid_cursor_goto" => grid_cursor_goto(snap, &params),
        "win_pos" => win_pos(snap, &params),
        "win_viewport" => win_viewport(snap, &params),
        "win_hide" => win_hide(snap, &params),
        "win_close" => win_close(snap, &params),
        "flush" => return true,
        _ => {}
    }
    false
}

fn as_u64(v: &Value) -> Option<u64> {
    v.as_u64()
}

fn as_i64(v: &Value) -> Option<i64> {
    v.as_i64()
}

/// Decodes a Neovim window/buffer handle, which `nvim-rs` delivers either as a
/// plain `Value::Integer` or as a `Value::Ext` whose payload is a msgpack-encoded
/// integer. Returns the handle as an `i64` (the same form used for
/// `nvim_set_current_win`/`nvim_win_close` later), or `None` if it is neither.
fn ext_handle(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(_) => v.as_i64(),
        Value::Ext(_type, data) => {
            let mut bytes: &[u8] = data;
            rmpv::decode::read_value(&mut bytes)
                .ok()
                .and_then(|inner| inner.as_i64())
        }
        _ => None,
    }
}

fn grid_resize(snap: &mut Snapshot, params: &[Value]) {
    // [grid, width, height]
    let (Some(grid), Some(width), Some(height)) = (
        params.first().and_then(as_u64),
        params.get(1).and_then(as_u64),
        params.get(2).and_then(as_u64),
    ) else {
        return;
    };
    snap.grid_mut(grid).resize(width as usize, height as usize);
}

fn grid_clear(snap: &mut Snapshot, params: &[Value]) {
    // [grid]
    let Some(grid) = params.first().and_then(as_u64) else {
        return;
    };
    // Drop quietly for an unknown grid (avoid materializing a phantom grid).
    if let Some(g) = snap.grids.get_mut(&grid) {
        g.clear();
    }
}

fn grid_destroy(snap: &mut Snapshot, params: &[Value]) {
    // [grid]
    let Some(grid) = params.first().and_then(as_u64) else {
        return;
    };
    snap.grids.remove(&grid);
    snap.windows.remove(&grid);
}

fn grid_scroll(snap: &mut Snapshot, params: &[Value]) {
    // [grid, top, bot, left, right, rows, cols]
    let grid = params.first().and_then(as_u64);
    let top = params.get(1).and_then(as_u64);
    let bot = params.get(2).and_then(as_u64);
    let left = params.get(3).and_then(as_u64);
    let right = params.get(4).and_then(as_u64);
    let rows = params.get(5).and_then(as_i64);
    let (Some(grid), Some(top), Some(bot), Some(left), Some(right), Some(rows)) =
        (grid, top, bot, left, right, rows)
    else {
        return;
    };
    // Drop quietly for an unknown grid (avoid materializing a phantom grid).
    let Some(g) = snap.grids.get_mut(&grid) else {
        return;
    };
    g.scroll(
        top as usize,
        bot as usize,
        left as usize,
        right as usize,
        rows,
    );
}

fn grid_line(snap: &mut Snapshot, params: Vec<Value>) {
    // [grid, row, col_start, cells, wrap]
    let mut it = params.into_iter();
    let Some(grid) = it.next().as_ref().and_then(as_u64) else {
        return;
    };
    let Some(row) = it.next().as_ref().and_then(as_u64) else {
        return;
    };
    let Some(col_start) = it.next().as_ref().and_then(as_u64) else {
        return;
    };
    let Some(Value::Array(cells)) = it.next() else {
        return;
    };

    // Drop quietly for an unknown grid (avoid materializing a phantom grid).
    let Some(g) = snap.grids.get_mut(&grid) else {
        return;
    };

    let row = row as usize;
    let mut col = col_start as usize;
    // Highlight id carries forward when a cell tuple omits it.
    let mut current_hl: u64 = 0;

    for cell in cells {
        let Value::Array(parts) = cell else { continue };
        // parts: [text] | [text, hl_id] | [text, hl_id, repeat]
        let text = match parts.first() {
            Some(Value::String(s)) => s.as_str().unwrap_or("").to_string(),
            _ => continue,
        };
        if let Some(hl) = parts.get(1).and_then(as_u64) {
            current_hl = hl;
        }
        let repeat = parts.get(2).and_then(as_u64).unwrap_or(1).max(1);

        // Each cell tuple maps to exactly one grid column. Neovim already emits
        // the trailing half of a double-width glyph as its own cell with empty
        // text (verified against the protocol), so we must NOT synthesize an
        // extra placeholder here — doing so would shift following cells right by
        // one column. Storing the empty text verbatim satisfies the render spec
        // ("text = '' for the trailing wide cell").
        for _ in 0..repeat {
            g.set_cell(
                row,
                col,
                Cell {
                    text: text.clone(),
                    hl_id: current_hl,
                },
            );
            col += 1;
        }
    }
}

fn hl_attr_define(snap: &mut Snapshot, params: Vec<Value>) {
    // [id, rgb_attrs, cterm_attrs, info]
    let mut it = params.into_iter();
    let Some(id) = it.next().as_ref().and_then(as_u64) else {
        return;
    };
    let Some(Value::Map(rgb)) = it.next() else {
        return;
    };

    let mut attr = HlAttr::default();
    for (k, v) in rgb {
        let Some(key) = k.as_str() else { continue };
        match key {
            "foreground" => attr.fg = v.as_u64().map(|p| Rgb::from_packed(p as u32)),
            "background" => attr.bg = v.as_u64().map(|p| Rgb::from_packed(p as u32)),
            "special" => attr.special = v.as_u64().map(|p| Rgb::from_packed(p as u32)),
            "bold" => attr.bold = v.as_bool().unwrap_or(false),
            "italic" => attr.italic = v.as_bool().unwrap_or(false),
            "underline" => attr.underline = v.as_bool().unwrap_or(false),
            "undercurl" => attr.undercurl = v.as_bool().unwrap_or(false),
            "strikethrough" => attr.strikethrough = v.as_bool().unwrap_or(false),
            "reverse" => attr.reverse = v.as_bool().unwrap_or(false),
            _ => {}
        }
    }
    snap.highlights.define(id, attr);
}

fn default_colors_set(snap: &mut Snapshot, params: &[Value]) {
    // [rgb_fg, rgb_bg, rgb_sp, cterm_fg, cterm_bg]
    let fg = params.first().and_then(as_u64);
    let bg = params.get(1).and_then(as_u64);
    let sp = params.get(2).and_then(as_u64);
    snap.highlights.set_defaults(DefaultColors {
        fg: fg.map(|p| Rgb::from_packed(p as u32)),
        bg: bg.map(|p| Rgb::from_packed(p as u32)),
        special: sp.map(|p| Rgb::from_packed(p as u32)),
    });
}

fn mode_info_set(snap: &mut Snapshot, params: Vec<Value>) {
    // [cursor_style_enabled, mode_info_array]
    let mut it = params.into_iter();
    let _enabled = it.next();
    let Some(Value::Array(modes)) = it.next() else {
        return;
    };

    let mut infos = Vec::with_capacity(modes.len());
    for m in modes {
        let Value::Map(entries) = m else { continue };
        let mut info = ModeInfo::default();
        for (k, v) in entries {
            let Some(key) = k.as_str() else { continue };
            match key {
                "name" => {
                    if let Some(s) = v.as_str() {
                        info.name = s.to_string();
                    }
                }
                "cursor_shape" => {
                    info.shape = match v.as_str() {
                        Some("vertical") => CursorShape::Vertical,
                        Some("horizontal") => CursorShape::Horizontal,
                        _ => CursorShape::Block,
                    };
                }
                "cell_percentage" => {
                    info.cell_percentage = v.as_u64().map(|n| n as u32);
                }
                "attr_id" => {
                    info.attr_id = v.as_u64();
                }
                _ => {}
            }
        }
        infos.push(info);
    }
    snap.mode_infos = infos;
}

fn mode_change(snap: &mut Snapshot, params: &[Value]) {
    // [mode_name, mode_idx]
    if let Some(idx) = params.get(1).and_then(as_u64) {
        snap.cursor.mode_idx = idx as usize;
    }
}

fn grid_cursor_goto(snap: &mut Snapshot, params: &[Value]) {
    // [grid, row, col]
    let (Some(grid), Some(row), Some(col)) = (
        params.first().and_then(as_u64),
        params.get(1).and_then(as_u64),
        params.get(2).and_then(as_u64),
    ) else {
        return;
    };
    snap.cursor.grid = grid;
    snap.cursor.row = row as usize;
    snap.cursor.col = col as usize;
}

fn win_pos(snap: &mut Snapshot, params: &[Value]) {
    // [grid, win, start_row, start_col, width, height]
    let Some(grid) = params.first().and_then(as_u64) else {
        return;
    };
    let win = params.get(1).and_then(ext_handle).unwrap_or_default();
    let start_row = params.get(2).and_then(as_u64).unwrap_or_default() as usize;
    let start_col = params.get(3).and_then(as_u64).unwrap_or_default() as usize;
    let width = params.get(4).and_then(as_u64).unwrap_or_default() as usize;
    let height = params.get(5).and_then(as_u64).unwrap_or_default() as usize;

    let entry = snap.windows.entry(grid).or_insert_with(|| WinInfo {
        grid,
        ..WinInfo::default()
    });
    entry.grid = grid;
    entry.win = win;
    entry.start_row = start_row;
    entry.start_col = start_col;
    entry.width = width;
    entry.height = height;
    entry.hidden = false;
}

fn win_hide(snap: &mut Snapshot, params: &[Value]) {
    // [grid]
    let Some(grid) = params.first().and_then(as_u64) else {
        return;
    };
    if let Some(win) = snap.windows.get_mut(&grid) {
        win.hidden = true;
    }
}

fn win_close(snap: &mut Snapshot, params: &[Value]) {
    // [grid]
    //
    // Remove BOTH the window entry and its grid. Neovim also emits `grid_destroy`
    // for the same grid on a window close; removing the grid here too makes the
    // two paths idempotent so the grid is gone regardless of event ordering.
    let Some(grid) = params.first().and_then(as_u64) else {
        return;
    };
    snap.windows.remove(&grid);
    snap.grids.remove(&grid);
}

fn win_viewport(snap: &mut Snapshot, params: &[Value]) {
    // [grid, win, topline, botline, curline, curcol, line_count, ...]
    let Some(grid) = params.first().and_then(as_u64) else {
        return;
    };
    let win = params.get(1).and_then(ext_handle).unwrap_or_default();
    let viewport = Viewport {
        topline: params.get(2).and_then(as_u64).unwrap_or_default(),
        botline: params.get(3).and_then(as_u64).unwrap_or_default(),
        curline: params.get(4).and_then(as_u64).unwrap_or_default(),
        linecount: params.get(6).and_then(as_u64).unwrap_or_default(),
    };

    let entry = snap.windows.entry(grid).or_insert_with(|| WinInfo {
        grid,
        win,
        ..WinInfo::default()
    });
    if entry.win == 0 {
        entry.win = win;
    }
    entry.viewport = viewport;
}
