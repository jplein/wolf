//! The `redraw` event handler: folds Neovim's redraw stream into the shared
//! [`Snapshot`](crate::render::Snapshot).
//!
//! Implements [`nvim_rs::Handler`] over the child-process writer. On each
//! `redraw` notification it takes the snapshot write lock **once** for the whole
//! batch, applies every event, and — on `flush` — bumps the sequence number,
//! **drops the lock**, and only then fires the front-end wake callback. The lock
//! is never held across an `.await`.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use nvim_rs::compat::tokio::Compat;
use nvim_rs::{Handler, Neovim};
use parking_lot::{Mutex, RwLockWriteGuard};
use rmpv::Value;
use tokio::process::ChildStdin;

use crate::command::Command;
use crate::layout::LayoutIntent;
use crate::recent_dirs::RecentDirs;
use crate::render::{
    Cell, CursorShape, DefaultColors, HlAttr, ModeInfo, PreviewInfo, Rgb, SharedSnapshot, Snapshot,
    ThemeColors, ThemeGroup, Viewport, WinInfo,
};

/// A shared, drainable queue of [`LayoutIntent`]s.
///
/// Owned by [`crate::api::WolfCore`] (drained via `drain_intents`) and cloned
/// into the [`RedrawHandler`], which pushes onto it when a `wolf_layout`
/// notification arrives. It is a sibling of the render snapshot (NOT part of it):
/// intents are out-of-band events, not redraw frames, so pushing one does not
/// bump the snapshot `seq`.
pub type IntentQueue = Arc<Mutex<VecDeque<LayoutIntent>>>;

/// A shared, drainable queue of [`Command`]s.
///
/// Owned by [`crate::api::WolfCore`] (drained via `drain_commands`) and cloned
/// into the [`RedrawHandler`], which pushes onto it when a `wolf_command`
/// notification arrives. Like [`IntentQueue`] it is a sibling of the render
/// snapshot (NOT part of it): commands are out-of-band, user-triggered events, so
/// pushing one does not bump the snapshot `seq`.
pub type CommandQueue = Arc<Mutex<VecDeque<Command>>>;

/// A shared cell holding the captive Neovim's current working directory.
///
/// Owned by [`crate::api::WolfCore`] and cloned into the [`RedrawHandler`], which
/// writes it on each `wolf_cwd` notification (the `WolfDir` `DirChanged` autocmd
/// in `runtime/nvim/init.lua`). It is a sibling of the render snapshot (NOT part
/// of it): nvim's cwd is not render state, so updating it does not bump the
/// snapshot `seq`. The input pump reads it synchronously to pick the per-cwd MRU
/// store when recording opened files; `None` means "not yet reported".
pub type TrackedCwd = Arc<parking_lot::RwLock<Option<PathBuf>>>;

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

/// Name of the out-of-band notification carrying the per-window buffer paths,
/// emitted by the `WolfBuffers` autocmds in `runtime/nvim/init.lua` (and once
/// immediately at setup). The payload is a single msgpack array of
/// `[win_id, buf_path]` pairs for ALL windows in the current tabpage; core
/// WHOLESALE-REPLACES [`Snapshot::buffers`] with the reported set. Routed
/// separately from `redraw` in [`RedrawHandler::handle_notify`].
const BUFFERS_EVENT: &str = "wolf_buffers";

/// Name of the out-of-band notification carrying the active window handle,
/// emitted by the `WolfLayout` `WinEnter` autocmd in `runtime/nvim/init.lua`.
/// Routed separately from `redraw` in [`RedrawHandler::handle_notify`].
const ACTIVE_WIN_EVENT: &str = "wolf_active_win";

/// Name of the out-of-band notification carrying the current TABPAGE handle,
/// emitted by the `wolf_setup_tabs` `TabEnter`/`TabNewEntered` autocmds in
/// `runtime/nvim/init.lua` (and once immediately at setup). Routed separately
/// from `redraw` in [`RedrawHandler::handle_notify`] into
/// [`Snapshot::current_tabpage`]. The front-end uses it as the cheap refocus
/// gate (mirrors [`ACTIVE_WIN_EVENT`]).
const TABPAGE_EVENT: &str = "wolf_tabpage";

/// Name of the out-of-band notification carrying a layout intent payload (a
/// split or a focus-direction move), emitted by the chord/Ex-command overrides
/// in `runtime/nvim/init.lua`. Routed separately from `redraw`.
const LAYOUT_EVENT: &str = "wolf_layout";

/// Name of the out-of-band notification carrying a command id (a user-triggered
/// `:Wolf<Pascal>` Ex-command or `require('wolf').<snake>()` Lua call), emitted by
/// the command registrations in `runtime/nvim/init.lua`. Routed separately from
/// `redraw`; folded into the sibling [`CommandQueue`].
const COMMAND_EVENT: &str = "wolf_command";

/// Name of the out-of-band notification carrying the captive Neovim's current
/// working directory (`getcwd()`), emitted by the `WolfDir` `DirChanged` autocmd
/// in `runtime/nvim/init.lua` (and once immediately at setup). Routed separately
/// from `redraw` in [`RedrawHandler::handle_notify`] into the [`TrackedCwd`] cell.
const CWD_EVENT: &str = "wolf_cwd";

/// Name of the out-of-band notification carrying the effective colors of the
/// optional `WolfCursor` / `WolfTitlebar` app-theme highlight groups, emitted by
/// the `WolfTheme` `ColorScheme` autocmd in `runtime/nvim/init.lua` (and once
/// immediately at setup). The payload is FOUR flat integers
/// (`cursor_fg, cursor_bg, titlebar_fg, titlebar_bg`), each a packed `0xRRGGBB`
/// color or `-1` for an unset channel. Routed separately from `redraw` in
/// [`RedrawHandler::handle_notify`] into [`Snapshot::theme`].
const THEME_EVENT: &str = "wolf_theme";

/// Name of the out-of-band notification carrying the set of preview windows and
/// their source Markdown buffers, emitted by the `WolfPreview` window-scan
/// autocmd in `runtime/nvim/init.lua` (and once immediately at setup). The payload
/// is a single msgpack array of `[preview_win, source_bufnr, source_abs_path]`
/// triples; core WHOLESALE-REPLACES [`Snapshot::previews`] with the reported set.
/// Routed separately from `redraw` in [`RedrawHandler::handle_notify`].
const PREVIEW_EVENT: &str = "wolf_preview";

/// Name of the out-of-band notification carrying a `[source_bufnr, changedtick]`
/// pair for a Markdown source buffer that changed, emitted by the
/// `TextChanged`/`TextChangedI`/`BufWritePost` autocmds in
/// `runtime/nvim/init.lua`. Core UPSERTS the tick into
/// [`Snapshot::markdown_ticks`]. Routed separately from `redraw` in
/// [`RedrawHandler::handle_notify`].
const MARKDOWN_DIRTY_EVENT: &str = "wolf_markdown_dirty";

/// `nvim-rs` [`Handler`] that applies redraw batches to a [`SharedSnapshot`].
#[derive(Clone)]
pub struct RedrawHandler {
    snapshot: SharedSnapshot,
    intents: IntentQueue,
    commands: CommandQueue,
    cwd: TrackedCwd,
    recent_dirs: RecentDirs,
    wake: WakeFn,
}

impl RedrawHandler {
    /// Creates a handler writing into `snapshot`, pushing layout intents onto
    /// `intents`, pushing user-triggered commands onto `commands`, tracking nvim's
    /// cwd into `cwd` (recording it into the always-on global `recent_dirs` MRU),
    /// and waking via `wake`.
    #[must_use]
    pub fn new(
        snapshot: SharedSnapshot,
        intents: IntentQueue,
        commands: CommandQueue,
        cwd: TrackedCwd,
        recent_dirs: RecentDirs,
        wake: WakeFn,
    ) -> Self {
        Self {
            snapshot,
            intents,
            commands,
            cwd,
            recent_dirs,
            wake,
        }
    }
}

#[async_trait]
impl Handler for RedrawHandler {
    type Writer = Compat<ChildStdin>;

    async fn handle_notify(&self, name: String, args: Vec<Value>, _neovim: Neovim<Self::Writer>) {
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
            BUFFERS_EVENT => {
                // `apply_buffers` builds the new map BEFORE taking the write
                // guard, then assigns it and DROPS the lock before returning; the
                // wake fires strictly after the lock is released (same
                // wake-after-unlock invariant). This is NOT a redraw frame, so
                // `seq` is left unchanged.
                self.apply_buffers(args).await;
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
            TABPAGE_EVENT => {
                // `apply_tabpage` acquires and DROPS the write lock before
                // returning; wake fires after the lock is released (same
                // wake-after-unlock invariant). This is NOT a redraw frame, so
                // `seq` is left unchanged.
                self.apply_tabpage(args).await;
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
            COMMAND_EVENT => {
                // A command is out-of-band and user-triggered: resolve the id to a
                // `Command`, push onto the sibling command queue (a brief Mutex, no
                // snapshot lock, never held across an `.await`), and wake so the
                // front-end drains it. An unknown id is dropped. No `seq` bump.
                if let Some(command) = args
                    .first()
                    .and_then(Value::as_str)
                    .and_then(Command::from_id)
                {
                    self.commands.lock().push_back(command);
                }
                (self.wake)();
            }
            THEME_EVENT => {
                // `apply_theme` acquires and DROPS the write lock before
                // returning; wake fires after the lock is released (same
                // wake-after-unlock invariant). This is NOT a redraw frame, so
                // `seq` is left unchanged.
                self.apply_theme(args).await;
                (self.wake)();
            }
            PREVIEW_EVENT => {
                // `apply_previews` builds the new map BEFORE taking the write
                // guard, then assigns it wholesale and DROPS the lock before
                // returning; the wake fires strictly after the lock is released
                // (same wake-after-unlock invariant). This is NOT a redraw frame,
                // so `seq` is left unchanged.
                self.apply_previews(args).await;
                (self.wake)();
            }
            MARKDOWN_DIRTY_EVENT => {
                // `apply_markdown_dirty` acquires and DROPS the write lock before
                // returning; wake fires after the lock is released (same
                // wake-after-unlock invariant). This is NOT a redraw frame, so
                // `seq` is left unchanged.
                self.apply_markdown_dirty(args).await;
                (self.wake)();
            }
            CWD_EVENT => {
                // nvim's cwd is NOT render state, so there is intentionally NO
                // wake here: the `:cd`/`:lcd`/`:tcd` that triggered this already
                // produces its own redraw flush (and hence wake) through the
                // normal `redraw` path. `apply_cwd` only updates the sibling
                // `TrackedCwd` cell (a brief RwLock, no snapshot lock, no `seq`
                // bump), which the input pump reads to pick the per-cwd MRU store.
                self.apply_cwd(args);
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
            let Some(name) = name.into_str() else {
                continue;
            };

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
    /// `current_file` to the canonicalized active-buffer path (or `None` for an
    /// unnamed buffer) and its `current_filetype` to the reported `vim.bo.filetype`
    /// (or empty).
    ///
    /// The first argument is `expand('%:p')`. An empty string (unnamed buffer)
    /// maps to `None`; otherwise the path is canonicalized to match the
    /// canonical paths the file picker emits, falling back to the raw path if
    /// canonicalization fails (e.g. the file no longer exists). The (new) second
    /// argument is `vim.bo.filetype`; the autocmd now also fires on `FileType` so
    /// the filetype tracks the active buffer. A missing 2nd arg maps to the empty
    /// string (unknown filetype), which drives [`Snapshot::is_markdown_active`] to
    /// `false`.
    ///
    /// The (new) third argument is a boolean: whether the active buffer is a PREVIEW
    /// buffer (it carries `b:wolf_preview_source`). A missing 3rd arg maps to
    /// `false`, which drives [`Snapshot::is_preview_active`] to `false`.
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
        let filetype = args
            .get(1)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let is_preview = args.get(2).and_then(Value::as_bool).unwrap_or(false);

        let mut snap = self.snapshot.write_async().await;
        snap.current_file = target;
        snap.current_filetype = filetype;
        snap.current_is_preview = is_preview;
        // A current_file change is NOT a redraw frame, so `seq` (the redraw-frame
        // counter, advanced only on `flush`) is intentionally left unchanged. The
        // caller still fires the wake after this lock drops so the front-end can
        // re-read the snapshot.
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_preview` notification: WHOLESALE-REPLACES the snapshot's
    /// per-preview-window source map ([`Snapshot::previews`]) with the reported
    /// set.
    ///
    /// The single argument is a msgpack array of
    /// `[preview_win, source_bufnr, source_abs_path]` triples, one per window
    /// showing a `b:wolf_preview_source` scratch buffer. Each `preview_win` /
    /// `source_bufnr` decodes via [`ext_handle`] (a plain integer, or defensively a
    /// msgpack-`Ext`-wrapped integer); each `source_abs_path` is the absolute path
    /// of the source Markdown file. A triple missing either handle is SKIPPED.
    ///
    /// The new map is built in full BEFORE the write guard is taken, then assigned
    /// wholesale. The lock is acquired via [`SharedSnapshot::write_async`], is NOT
    /// held across any `.await`, and is dropped before returning so the caller's
    /// wake fires after the lock is released. This is NOT a redraw frame, so `seq`
    /// is left unchanged; the wholesale replacement drops any closed preview
    /// windows.
    async fn apply_previews(&self, args: Vec<Value>) {
        let mut map: HashMap<i64, PreviewInfo> = HashMap::new();
        if let Some(triples) = args.first().and_then(Value::as_array) {
            for triple in triples {
                let Some(triple) = triple.as_array() else {
                    continue;
                };
                let Some(win) = triple.first().and_then(ext_handle) else {
                    continue;
                };
                let Some(source_bufnr) = triple.get(1).and_then(ext_handle) else {
                    continue;
                };
                let source_path = triple
                    .get(2)
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_default();
                map.insert(
                    win,
                    PreviewInfo {
                        source_bufnr,
                        source_path,
                    },
                );
            }
        }

        let mut snap = self.snapshot.write_async().await;
        snap.previews = map;
        // A previews change is NOT a redraw frame, so `seq` is intentionally left
        // unchanged. The caller fires the wake after this lock drops.
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_markdown_dirty` notification: UPSERTS the reported
    /// `[source_bufnr, changedtick]` into [`Snapshot::markdown_ticks`].
    ///
    /// The payload is a source buffer number (decoded via [`ext_handle`]) and its
    /// current `changedtick` (a `u64`). A payload missing either is dropped. The
    /// front-end joins a preview's `source_bufnr` to its tick to decide when to
    /// re-pull rendered HTML.
    ///
    /// The write lock is acquired via [`SharedSnapshot::write_async`], is NOT held
    /// across any `.await`, and is dropped before returning so the caller's wake
    /// fires after the lock is released. This is NOT a redraw frame, so `seq` is
    /// left unchanged.
    async fn apply_markdown_dirty(&self, args: Vec<Value>) {
        let Some(bufnr) = args.first().and_then(ext_handle) else {
            return;
        };
        let Some(tick) = args.get(1).and_then(as_u64) else {
            return;
        };
        let mut snap = self.snapshot.write_async().await;
        snap.markdown_ticks.insert(bufnr, tick);
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_buffers` notification: WHOLESALE-REPLACES the snapshot's
    /// per-window buffer map ([`Snapshot::buffers`]) with the reported set.
    ///
    /// The single argument is a msgpack array of `[win_id, buf_path]` pairs for
    /// ALL windows in the current tabpage. Each `win_id` decodes via
    /// [`ext_handle`] (a plain integer, or defensively a msgpack-`Ext`-wrapped
    /// integer); each `buf_path` is `nvim_buf_get_name(buf)` — absolute for a file
    /// buffer, empty for an unnamed one. Empty paths are SKIPPED (unnamed windows
    /// are absent from the map); non-empty paths are canonicalized (to match the
    /// canonical cwds/paths the rest of core keys on), falling back to the raw
    /// path if canonicalization fails.
    ///
    /// The new map is built in full BEFORE the write guard is taken (so no
    /// filesystem work happens under the lock, sidestepping
    /// `clippy::significant_drop_in_scrutinee`), then assigned wholesale. The lock
    /// is acquired via [`SharedSnapshot::write_async`], is NOT held across any
    /// `.await`, and is dropped before returning so the caller's wake fires after
    /// the lock is released. This is NOT a redraw frame, so `seq` is left
    /// unchanged; the wholesale replacement drops any stale/closed windows.
    async fn apply_buffers(&self, args: Vec<Value>) {
        let mut map: HashMap<i64, PathBuf> = HashMap::new();
        if let Some(pairs) = args.first().and_then(Value::as_array) {
            for pair in pairs {
                let Some(pair) = pair.as_array() else {
                    continue;
                };
                let Some(win) = pair.first().and_then(ext_handle) else {
                    continue;
                };
                let Some(path) = pair.get(1).and_then(Value::as_str) else {
                    continue;
                };
                if path.is_empty() {
                    continue;
                }
                let canon = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
                map.insert(win, canon);
            }
        }

        let mut snap = self.snapshot.write_async().await;
        snap.buffers = map;
        // A buffers change is NOT a redraw frame, so `seq` is intentionally left
        // unchanged. The caller fires the wake after this lock drops.
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

    /// Applies a `wolf_tabpage` notification: stores the reported current
    /// TABPAGE handle in [`Snapshot::current_tabpage`].
    ///
    /// The first argument is `nvim_get_current_tabpage()` from the `TabEnter` /
    /// `TabNewEntered` autocmd (plus one initial emit at setup). It arrives as a
    /// plain integer or, defensively, as a msgpack-`Ext`-wrapped integer; either
    /// decodes via [`ext_handle`]. A handle we cannot decode leaves
    /// `current_tabpage` unchanged.
    ///
    /// The write lock is acquired via [`SharedSnapshot::write_async`], is NOT
    /// held across any `.await`, and is dropped before returning so the caller's
    /// wake fires after the lock is released. This is NOT a redraw frame, so
    /// `seq` is left unchanged.
    async fn apply_tabpage(&self, args: Vec<Value>) {
        let Some(tabpage) = args.first().and_then(ext_handle) else {
            return;
        };
        let mut snap = self.snapshot.write_async().await;
        snap.current_tabpage = tabpage;
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_theme` notification: stores the reported `WolfCursor` /
    /// `WolfTitlebar` colors in [`Snapshot::theme`].
    ///
    /// The payload is four flat integers — `[cursor_fg, cursor_bg, titlebar_fg,
    /// titlebar_bg]` — each a packed `0xRRGGBB` color or `-1` for an unset
    /// channel (the `WolfTheme` autocmd in `runtime/nvim/init.lua` sends
    /// `h.fg or -1` etc.). A value is accepted only when it is a valid 24-bit
    /// color (`0..=0xFF_FFFF`); anything else — including the `-1` sentinel and
    /// a missing argument — maps to `None`.
    ///
    /// The args are parsed BEFORE the write lock is taken, so the guard is held
    /// only for the bare assignment. The lock is acquired via
    /// [`SharedSnapshot::write_async`], is NOT held across any `.await`, and is
    /// dropped before returning so the caller's wake fires after the lock is
    /// released. This is NOT a redraw frame, so `seq` is left unchanged.
    async fn apply_theme(&self, args: Vec<Value>) {
        /// Maps one flat payload scalar to a color: in-range 24-bit values are
        /// colors; `-1` (and anything else) means unset.
        fn channel(v: Option<&Value>) -> Option<Rgb> {
            let v = v.and_then(Value::as_i64)?;
            if (0..=0x00FF_FFFF).contains(&v) {
                Some(Rgb::from_packed(v as u32))
            } else {
                None
            }
        }

        let theme = ThemeColors {
            cursor: ThemeGroup {
                fg: channel(args.first()),
                bg: channel(args.get(1)),
            },
            titlebar: ThemeGroup {
                fg: channel(args.get(2)),
                bg: channel(args.get(3)),
            },
        };

        let mut snap = self.snapshot.write_async().await;
        snap.theme = theme;
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_cwd` notification: stores the captive nvim's reported
    /// working directory in the sibling [`TrackedCwd`] cell.
    ///
    /// The first argument is `getcwd()` from the `WolfDir` `DirChanged` autocmd. A
    /// non-empty value is canonicalized (to match the canonical cwds the MRU cache
    /// keys on, falling back to the raw path if canonicalization fails) and stored;
    /// an empty/missing value leaves the tracked cwd unchanged. The canonicalize is
    /// done BEFORE taking the `RwLock` write guard, so the guard is held only for
    /// the bare assignment (never across filesystem work) — this also sidesteps
    /// `clippy::significant_drop_in_scrutinee`. This touches only the cwd cell, not
    /// the snapshot lock, and does NOT bump `seq` (cwd is not a redraw frame).
    ///
    /// The (already-canonical) target is then recorded into the always-on
    /// global [`RecentDirs`] MRU, so every directory the user changes into — via
    /// the "open folder" command OR a typed `:cd` — is moved-or-added to the top of
    /// `folder-mru.sqlite`. That record is fire-and-forget (a brief internal mutex,
    /// self-healing on corruption) and never bumps `seq` or wakes the front-end.
    ///
    /// The one-shot INITIAL `wolf_cwd` emit (the startup/launch cwd, before any
    /// real `:cd`) is flagged with a `true` second arg by `init.lua`. For that
    /// emit the tracked cwd cell is STILL updated (so `live_cwd` reflects nvim's
    /// working directory), but the [`RecentDirs`] record is skipped: the passive
    /// launch cwd is not a directory the user opened, so it must not pollute the
    /// recent-dirs MRU. Only genuine `DirChanged` events (unflagged) record.
    ///
    /// Per-tab cwd is NO LONGER mirrored core-side into a state store: the
    /// front-end reads each tab's live cwd (via the FFI `cwd()` accessor) when it
    /// walks the whole window/tab tree at snapshot time.
    fn apply_cwd(&self, args: Vec<Value>) {
        let target: Option<PathBuf> = match args.first().and_then(Value::as_str) {
            Some(p) if !p.is_empty() => {
                Some(std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p)))
            }
            // Empty/missing: leave the tracked cwd unchanged.
            _ => None,
        };
        let is_initial = matches!(args.get(1).and_then(Value::as_bool), Some(true));
        if let Some(target) = target {
            *self.cwd.write() = Some(target.clone());
            // The passive launch/initial cwd is tracked (above) but never
            // recorded in the MRU — only real directory changes are.
            if is_initial {
                return;
            }
            self.recent_dirs.record(&target);
        }
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
pub(crate) fn ext_handle(v: &Value) -> Option<i64> {
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
        // `repeat` counts how many columns the cell occupies, INCLUDING the
        // first — so an explicit `repeat` of 0 is a zero-width no-op cell.
        // Neovim really emits those: a trailing `[" ", 0, 0]` clear-to-EOL cell
        // when there is nothing left to clear. Clamping it up to 1 (as this
        // once did) writes a spurious default-highlight space ONE column past
        // the event's real span; Neovim's shadow grid never has that cell, so
        // it is never repainted and the corruption persists on screen (single
        // wrong-background cells just past end-of-line).
        let repeat = parts.get(2).and_then(as_u64).unwrap_or(1);

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
    //
    // A window is hidden when it stops being displayed — most importantly, when
    // its TABPAGE is no longer current (Neovim emits `win_hide` for every window
    // of the tabpage being left on a tabpage switch, and re-emits `win_pos` for
    // them on the way back; it does NOT `win_close`/`grid_destroy` them). Remove
    // the WINDOW entry so `Snapshot.windows` (and therefore
    // [`Snapshot::grid_for_win`]) reflects ONLY the current tabpage's windows —
    // the project/tabpage-per-view contract the front-end's refocus rebind relies
    // on. The GRID is intentionally KEPT (Neovim reuses the same grid id when the
    // window is shown again, and may re-`win_pos` without re-`grid_resize`ing), so
    // a rebound window always resolves to a live, paintable grid.
    let Some(grid) = params.first().and_then(as_u64) else {
        return;
    };
    snap.windows.remove(&grid);
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
