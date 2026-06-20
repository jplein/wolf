//! The `redraw` event handler: folds Neovim's redraw stream into the shared
//! [`Snapshot`](crate::render::Snapshot).
//!
//! Implements [`nvim_rs::Handler`] over the child-process writer. On each
//! `redraw` notification it takes the snapshot write lock **once** for the whole
//! batch, applies every event, and — on `flush` — bumps the sequence number,
//! **drops the lock**, and only then fires the front-end wake callback. The lock
//! is never held across an `.await`.

use std::sync::Arc;

use async_trait::async_trait;
use nvim_rs::compat::tokio::Compat;
use nvim_rs::{Handler, Neovim};
use parking_lot::RwLockWriteGuard;
use rmpv::Value;
use tokio::process::ChildStdin;

use crate::render::{
    Cell, CursorShape, DefaultColors, HlAttr, ModeInfo, Rgb, SharedSnapshot, Snapshot,
};

/// The front-end wake callback, invoked once per `flush` (after the lock drops).
///
/// May run on a tokio worker thread, hence `Send + Sync`; the front-end is
/// responsible for marshalling onto its UI thread.
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// `nvim-rs` [`Handler`] that applies redraw batches to a [`SharedSnapshot`].
#[derive(Clone)]
pub struct RedrawHandler {
    snapshot: SharedSnapshot,
    wake: WakeFn,
}

impl RedrawHandler {
    /// Creates a handler writing into `snapshot` and waking via `wake`.
    #[must_use]
    pub fn new(snapshot: SharedSnapshot, wake: WakeFn) -> Self {
        Self { snapshot, wake }
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
        if name != "redraw" {
            return;
        }
        let fired = self.apply_redraw(args).await;
        if fired {
            (self.wake)();
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
}

/// Applies one redraw event tuple. Returns `true` only for `flush`.
fn apply_event(snap: &mut RwLockWriteGuard<'_, Snapshot>, name: &str, params: Vec<Value>) -> bool {
    match name {
        "grid_resize" => grid_resize(snap, &params),
        "grid_clear" => snap.grid.clear(),
        "grid_line" => grid_line(snap, params),
        "grid_scroll" => grid_scroll(snap, &params),
        "hl_attr_define" => hl_attr_define(snap, params),
        "default_colors_set" => default_colors_set(snap, &params),
        "mode_info_set" => mode_info_set(snap, params),
        "mode_change" => mode_change(snap, &params),
        "grid_cursor_goto" => grid_cursor_goto(snap, &params),
        "win_viewport" => win_viewport(snap, &params),
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

fn grid_resize(snap: &mut Snapshot, params: &[Value]) {
    // [grid, width, height]
    let (Some(width), Some(height)) = (params.get(1).and_then(as_u64), params.get(2).and_then(as_u64))
    else {
        return;
    };
    snap.grid.resize(width as usize, height as usize);
}

fn grid_scroll(snap: &mut Snapshot, params: &[Value]) {
    // [grid, top, bot, left, right, rows, cols]
    let top = params.get(1).and_then(as_u64);
    let bot = params.get(2).and_then(as_u64);
    let left = params.get(3).and_then(as_u64);
    let right = params.get(4).and_then(as_u64);
    let rows = params.get(5).and_then(as_i64);
    let (Some(top), Some(bot), Some(left), Some(right), Some(rows)) = (top, bot, left, right, rows)
    else {
        return;
    };
    snap.grid.scroll(
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
    let _grid = it.next();
    let Some(row) = it.next().as_ref().and_then(as_u64) else {
        return;
    };
    let Some(col_start) = it.next().as_ref().and_then(as_u64) else {
        return;
    };
    let Some(Value::Array(cells)) = it.next() else {
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
            snap.grid.set_cell(
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
    let (Some(row), Some(col)) = (params.get(1).and_then(as_u64), params.get(2).and_then(as_u64))
    else {
        return;
    };
    snap.cursor.row = row as usize;
    snap.cursor.col = col as usize;
}

fn win_viewport(snap: &mut Snapshot, params: &[Value]) {
    // [grid, win, topline, botline, curline, curcol, line_count, ...]
    let topline = params.get(2).and_then(as_u64);
    let botline = params.get(3).and_then(as_u64);
    let curline = params.get(4).and_then(as_u64);
    let linecount = params.get(6).and_then(as_u64);
    if let Some(topline) = topline {
        snap.viewport.topline = topline;
    }
    if let Some(botline) = botline {
        snap.viewport.botline = botline;
    }
    if let Some(curline) = curline {
        snap.viewport.curline = curline;
    }
    if let Some(linecount) = linecount {
        snap.viewport.linecount = linecount;
    }
}
