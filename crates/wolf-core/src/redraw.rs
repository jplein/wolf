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
    Cell, Cmdline, CmdlineChunk, CursorShape, DefaultColors, HlAttr, MarkdownCursor, Message,
    ModeInfo, PendingConfirm, PreviewInfo, Pum, PumItem, Rgb, SharedSnapshot, Snapshot, TabFiles,
    ThemeColors, ThemeGroup, ToastChunk, Viewport, WinBuffer, WinInfo,
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
/// snapshot `seq`. The input pump reads it synchronously to scope the global MRU
/// store by nvim's live cwd when recording opened files; `None` means "not yet
/// reported".
pub type TrackedCwd = Arc<parking_lot::RwLock<Option<PathBuf>>>;

/// A shared cell holding every pane's navigation history (back/forward).
///
/// Owned by [`crate::api::WolfCore`] (read and moved by its `history_*` methods)
/// and cloned into the [`RedrawHandler`], which FOLDS a derived per-window
/// [`NavTarget`](crate::nav_history::NavTarget) into it after every `wolf_buffers`
/// / `wolf_preview` emit and PRUNES a closed window's stack on `win_close`. Like
/// [`IntentQueue`] / [`TrackedCwd`] it is a sibling of the render snapshot (NOT
/// part of it): history is out-of-band app state, mutated from FFI accessors that
/// also do blocking RPC, so it must not ride on the `Clone`-per-frame
/// [`Snapshot`]. Updating it does not bump the snapshot `seq`.
///
/// LOCK ORDER: the fold takes the snapshot write lock FIRST and this mutex second,
/// with no `.await` or RPC between; the `WolfCore::history_*` side takes this mutex
/// ALONE (never while holding the snapshot lock, never across a `block_on_rpc`), so
/// there is no lock-order cycle.
///
/// A restart clones the SAME handler, so history survives a respawn by
/// construction — acceptable, because the new nvim issues fresh window handles and
/// stale panes are never queried.
pub type NavHistoryCell = Arc<Mutex<crate::nav_history::NavHistory>>;

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

/// Name of the out-of-band notification carrying what every window is showing,
/// emitted by the `WolfBuffers` autocmds in `runtime/nvim/init.lua` (and once
/// immediately at setup). The payload is a single msgpack array of
/// `[win_id, buf_path, bufnr, is_terminal, is_drawer]` tuples for ALL windows in the
/// current tabpage (preview scratch panes are SKIPPED emit-side, which keeps this
/// map DISJOINT from [`Snapshot::previews`]); core WHOLESALE-REPLACES BOTH
/// [`Snapshot::buffers`] (named windows only, path-valued — unchanged) and the
/// parallel [`Snapshot::win_buffers`] (every window, [`WinBuffer`]-valued) with the
/// reported set. Routed separately from `redraw` in
/// [`RedrawHandler::handle_notify`]. A legacy 2-element tuple still decodes, with
/// `bufnr: 0` and both flags `false`.
const BUFFERS_EVENT: &str = "wolf_buffers";

/// Name of the out-of-band notification carrying the per-window full-width flags,
/// emitted by the `WolfFullWidth` autocmds in `runtime/nvim/init.lua` (and once
/// immediately at setup). The payload is a single msgpack array of
/// `[win_id, full_width]` pairs for ALL windows in the current tabpage; core
/// WHOLESALE-REPLACES [`Snapshot::full_width`] with the reported set. Routed
/// separately from `redraw` in [`RedrawHandler::handle_notify`].
const FULL_WIDTH_EVENT: &str = "wolf_full_width";

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

/// Name of the out-of-band notification carrying a `[source_bufnr, line,
/// char_col]` triple for a Markdown source buffer whose cursor moved OR which was
/// just entered, emitted by the `CursorMoved`/`CursorMovedI`/`BufEnter`/
/// `BufWinEnter` autocmds in `runtime/nvim/init.lua`. `line` is 1-based
/// (`line('.')`); `char_col` is a 0-based CHARACTER column (`charcol('.') - 1`),
/// NOT a byte column. Core UPSERTS the pair into [`Snapshot::markdown_cursors`].
/// Routed separately from `redraw` in [`RedrawHandler::handle_notify`].
///
/// The two BUFFER-ENTRY events are load-bearing for freshness (`:edit foo.md` fires
/// `CursorMoved` BEFORE filetype detection, so the markdown guard bails and nothing
/// further fires until the user moves), and they are exactly why the emitter also
/// guards on `win_gettype() == ""`: `vim.fn.bufload()` fires them inside nvim's
/// synthetic AUTOCMD window, where the buffer is genuinely current but the cursor is
/// a fabricated line 1 belonging to no user pane. Without that guard a
/// background-loaded source would report a bogus `(1, 0)` instead of the `line == 0`
/// "cursor unknown" sentinel the front-end relies on. See
/// [`crate::nvim::NvimProcess::setup_preview`] for the full guard list.
const MARKDOWN_CURSOR_EVENT: &str = "wolf_markdown_cursor";

/// Name of the out-of-band notification carrying every tabpage's tab-local
/// working directory, emitted by the `WolfTabs` autocmds in
/// `runtime/nvim/init.lua` (and once immediately at setup). The payload is a
/// single msgpack array of `[tabpage_handle, cwd]` pairs for ALL tabpages; core
/// WHOLESALE-REPLACES [`Snapshot::tab_cwds`] with the reported set. Routed
/// separately from `redraw` in [`RedrawHandler::handle_notify`].
const TAB_CWDS_EVENT: &str = "wolf_tab_cwds";

/// Name of the out-of-band notification carrying the Tier-1 open-file mirror,
/// emitted by the `WolfOpenFiles` autocmds in `runtime/nvim/init.lua` (and once
/// immediately at setup). The payload is a single msgpack array of
/// `[tabpage_handle, active_path, files[]]` triples for ALL tabpages; core
/// WHOLESALE-REPLACES [`Snapshot::open_files`] with the reported set. Routed
/// separately from `redraw` in [`RedrawHandler::handle_notify`].
const OPEN_FILES_EVENT: &str = "wolf_open_files";

/// `nvim-rs` [`Handler`] that applies redraw batches to a [`SharedSnapshot`].
#[derive(Clone)]
pub struct RedrawHandler {
    snapshot: SharedSnapshot,
    intents: IntentQueue,
    commands: CommandQueue,
    cwd: TrackedCwd,
    recent_dirs: RecentDirs,
    nav_history: NavHistoryCell,
    wake: WakeFn,
}

impl RedrawHandler {
    /// Creates a handler writing into `snapshot`, pushing layout intents onto
    /// `intents`, pushing user-triggered commands onto `commands`, tracking nvim's
    /// cwd into `cwd` (recording it into the always-on global `recent_dirs` MRU),
    /// folding per-pane navigation targets into `nav_history`, and waking via
    /// `wake`.
    #[must_use]
    pub fn new(
        snapshot: SharedSnapshot,
        intents: IntentQueue,
        commands: CommandQueue,
        cwd: TrackedCwd,
        recent_dirs: RecentDirs,
        nav_history: NavHistoryCell,
        wake: WakeFn,
    ) -> Self {
        Self {
            snapshot,
            intents,
            commands,
            cwd,
            recent_dirs,
            nav_history,
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
            TAB_CWDS_EVENT => {
                // `apply_tab_cwds` builds the new map BEFORE taking the write
                // guard, then assigns it and DROPS the lock before returning; the
                // wake fires strictly after the lock is released (same
                // wake-after-unlock invariant). This is NOT a redraw frame, so
                // `seq` is left unchanged.
                self.apply_tab_cwds(args).await;
                (self.wake)();
            }
            OPEN_FILES_EVENT => {
                // `apply_open_files` builds the new map BEFORE taking the write
                // guard, then assigns it wholesale and DROPS the lock before
                // returning; the wake fires strictly after the lock is released
                // (same wake-after-unlock invariant). This is NOT a redraw frame, so
                // `seq` is left unchanged.
                self.apply_open_files(args).await;
                (self.wake)();
            }
            FULL_WIDTH_EVENT => {
                // `apply_full_width` builds the new map BEFORE taking the write
                // guard, then assigns it wholesale and DROPS the lock before
                // returning; the wake fires strictly after the lock is released
                // (same wake-after-unlock invariant). This is NOT a redraw frame, so
                // `seq` is left unchanged.
                self.apply_full_width(args).await;
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
            MARKDOWN_CURSOR_EVENT => {
                // `apply_markdown_cursor` acquires and DROPS the write lock before
                // returning; wake fires after the lock is released (same
                // wake-after-unlock invariant). This is NOT a redraw frame, so
                // `seq` is left unchanged.
                self.apply_markdown_cursor(args).await;
                (self.wake)();
            }
            CWD_EVENT => {
                // nvim's cwd is NOT render state, so there is intentionally NO
                // wake here: the `:cd`/`:lcd`/`:tcd` that triggered this already
                // produces its own redraw flush (and hence wake) through the
                // normal `redraw` path. `apply_cwd` only updates the sibling
                // `TrackedCwd` cell (a brief RwLock, no snapshot lock, no `seq`
                // bump), which the input pump reads to scope the global MRU store
                // by nvim's live cwd.
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
    ///
    /// A `win_close` cannot report the closed WINDOW handle (its payload is just
    /// the grid), so `win_close` resolves it from `snap.windows` BEFORE removing
    /// the entry and pushes it onto a `closed_wins` collector threaded through
    /// [`apply_event`]. After the batch — still under the guard, with no `.await`
    /// in between — those panes' navigation histories are PRUNED.
    async fn apply_redraw(&self, batch: Vec<Value>) -> bool {
        let mut snap = self.snapshot.write_async().await;
        let mut flushed = false;
        let mut closed_wins: Vec<i64> = Vec::new();

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
                if apply_event(&mut snap, &name, params, &mut closed_wins) {
                    flushed = true;
                }
            }
        }

        if flushed {
            snap.seq = snap.seq.wrapping_add(1);
        }
        if !closed_wins.is_empty() {
            // No `.await` between here and the guard drop, so this brief mutex is
            // never held across a suspension point.
            self.nav_history.lock().prune_many(&closed_wins);
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
    /// The (new) fourth argument is a boolean: whether the active buffer is in
    /// FULL-WIDTH mode (it carries a truthy `b:wolf_full_width`; for a preview
    /// scratch this is the SOURCE buffer's flag — OQ-1). A missing 4th arg maps to
    /// `false`, which drives [`Snapshot::is_full_width_active`] to `false`.
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
        let is_full_width = args.get(3).and_then(Value::as_bool).unwrap_or(false);

        let mut snap = self.snapshot.write_async().await;
        snap.current_file = target;
        snap.current_filetype = filetype;
        snap.current_is_preview = is_preview;
        snap.current_is_full_width = is_full_width;
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
    /// `[preview_win, source_bufnr, source_abs_path, source_full_width]` tuples, one
    /// per window showing a `b:wolf_preview_source` scratch buffer. Each
    /// `preview_win` / `source_bufnr` decodes via [`ext_handle`] (a plain integer, or
    /// defensively a msgpack-`Ext`-wrapped integer); each `source_abs_path` is the
    /// absolute path of the source Markdown file; each `source_full_width` is the
    /// SOURCE buffer's `b:wolf_full_width` flag (a missing 4th element maps to
    /// `false`). A tuple missing either handle is SKIPPED.
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
                let source_full_width = triple.get(3).and_then(Value::as_bool).unwrap_or(false);
                map.insert(
                    win,
                    PreviewInfo {
                        source_bufnr,
                        source_path,
                        source_full_width,
                    },
                );
            }
        }

        let mut snap = self.snapshot.write_async().await;
        snap.previews = map;
        // The derived navigation target joins BOTH maps, and EITHER notification
        // can complete a transition, so the fold runs here as well as in
        // `apply_buffers`.
        self.fold_nav_targets(&snap);
        // A previews change is NOT a redraw frame, so `seq` is intentionally left
        // unchanged. The caller fires the wake after this lock drops.
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Folds one derived [`NavTarget`](crate::nav_history::NavTarget) per window
    /// into the sibling [`NavHistoryCell`], from the CURRENT snapshot state.
    ///
    /// Called at the tail of BOTH [`apply_buffers`](Self::apply_buffers) and
    /// [`apply_previews`](Self::apply_previews), under the snapshot write guard,
    /// because the derived target joins [`Snapshot::win_buffers`] and
    /// [`Snapshot::previews`] and either notification can complete a transition.
    ///
    /// The candidate windows are the UNION of the two maps' keys, materialized
    /// into a local `Vec` BEFORE the nav mutex is taken (so no `MutexGuard` is ever
    /// a `for`/`match` scrutinee — `clippy::significant_drop_in_scrutinee`). A
    /// window in NEITHER map is simply not in the union: **no entry, no push, no
    /// prune** — the locked rule realized structurally, which is what makes the two
    /// notifications' arrival order irrelevant during a preview transition.
    ///
    /// There is NO `.await` in this function, so `clippy::await_holding_lock` does
    /// not fire (mirroring the note on `apply_buffers`); the mutex is taken after
    /// the snapshot lock and released before returning, and the `WolfCore` side
    /// never takes them together, so there is no lock-order cycle.
    fn fold_nav_targets(&self, snap: &Snapshot) {
        let mut wins: Vec<i64> = snap.win_buffers.keys().copied().collect();
        wins.extend(
            snap.previews
                .keys()
                .copied()
                .filter(|w| !snap.win_buffers.contains_key(w)),
        );
        if wins.is_empty() {
            return;
        }

        let mut hist = self.nav_history.lock();
        for win in wins {
            // An empty source path means "no usable preview", not a preview of "".
            let preview_source = snap
                .previews
                .get(&win)
                .map(|p| p.source_path.as_path())
                .filter(|p| !p.as_os_str().is_empty());
            if let Some(target) =
                crate::nav_history::derive_target(snap.win_buffers.get(&win), preview_source)
            {
                hist.observe(win, target);
            }
        }
        // The mutex drops here; no `.await` was crossed.
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

    /// Applies a `wolf_markdown_cursor` notification: UPSERTS the reported
    /// `[source_bufnr, line, char_col]` triple into
    /// [`Snapshot::markdown_cursors`].
    ///
    /// The payload is a source buffer number (decoded via [`ext_handle`]), the
    /// 1-based cursor LINE (`line('.')`), and the 0-based CHARACTER column
    /// (`charcol('.') - 1` — a CHARACTER offset, NOT a byte offset). A payload
    /// missing any of the three is dropped. Each scalar saturates into a `u32`, so
    /// an absurd value can never truncate into a small one. The front-end joins a
    /// preview's `source_bufnr` to this position to scroll the corresponding
    /// `wl-{line}-{col}` marker into view.
    ///
    /// The write lock is acquired via [`SharedSnapshot::write_async`], is NOT held
    /// across any `.await`, and is dropped before returning so the caller's wake
    /// fires after the lock is released. This is NOT a redraw frame, so `seq` is
    /// left unchanged.
    async fn apply_markdown_cursor(&self, args: Vec<Value>) {
        let Some(bufnr) = args.first().and_then(ext_handle) else {
            return;
        };
        let Some(line) = args.get(1).and_then(as_u64) else {
            return;
        };
        let Some(col) = args.get(2).and_then(as_u64) else {
            return;
        };
        let cursor = MarkdownCursor {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            col: u32::try_from(col).unwrap_or(u32::MAX),
        };
        let mut snap = self.snapshot.write_async().await;
        snap.markdown_cursors.insert(bufnr, cursor);
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_buffers` notification: WHOLESALE-REPLACES BOTH the
    /// snapshot's per-window buffer map ([`Snapshot::buffers`]) and the parallel
    /// [`Snapshot::win_buffers`] with the reported set, then folds the derived
    /// navigation targets.
    ///
    /// The single argument is a msgpack array of
    /// `[win_id, buf_path, bufnr, is_terminal, is_drawer]` tuples for ALL windows in
    /// the current tabpage (preview scratch panes are skipped emit-side). Each
    /// `win_id` decodes via [`ext_handle`] (a plain integer, or defensively a
    /// msgpack-`Ext`-wrapped integer); each `buf_path` is `nvim_buf_get_name(buf)` —
    /// absolute for a file buffer, empty for an unnamed one.
    ///
    /// The two maps are filled in the SAME pre-guard pass but by DELIBERATELY
    /// different rules:
    ///
    /// * [`Snapshot::buffers`] is UNCHANGED from before the tuple grew — a missing
    ///   or non-string element 2 is skipped, an EMPTY path is skipped (so unnamed
    ///   windows are ABSENT), and a non-empty path is canonicalized with a raw
    ///   fallback. Its only production consumer is
    ///   [`crate::api::WolfCore::pane_status`], which must not regress.
    /// * [`Snapshot::win_buffers`] gets an entry for EVERY tuple that decodes a
    ///   `win`, including unnamed windows (with `path: None`); it reuses the SAME
    ///   canonicalized value (never canonicalizing twice) and decodes `bufnr` via
    ///   [`ext_handle`] (defaulting `0`) plus the two booleans (defaulting `false`).
    ///   A legacy 2-element tuple therefore yields `bufnr: 0` and both flags
    ///   `false`.
    ///
    /// Both maps are built in full BEFORE the write guard is taken (so no filesystem
    /// work happens under the lock, sidestepping
    /// `clippy::significant_drop_in_scrutinee`), then assigned wholesale. The lock
    /// is acquired via [`SharedSnapshot::write_async`], is NOT held across any
    /// `.await`, and is dropped before returning so the caller's wake fires after
    /// the lock is released. This is NOT a redraw frame, so `seq` is left
    /// unchanged; the wholesale replacement drops any stale/closed windows.
    async fn apply_buffers(&self, args: Vec<Value>) {
        let mut map: HashMap<i64, PathBuf> = HashMap::new();
        let mut win_map: HashMap<i64, WinBuffer> = HashMap::new();
        if let Some(pairs) = args.first().and_then(Value::as_array) {
            for pair in pairs {
                let Some(pair) = pair.as_array() else {
                    continue;
                };
                let Some(win) = pair.first().and_then(ext_handle) else {
                    continue;
                };
                // Canonicalize ONCE; both maps share the result.
                let canon: Option<PathBuf> = match pair.get(1).and_then(Value::as_str) {
                    Some(path) if !path.is_empty() => {
                        Some(std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path)))
                    }
                    _ => None,
                };
                if let Some(canon) = canon.clone() {
                    map.insert(win, canon);
                }
                win_map.insert(
                    win,
                    WinBuffer {
                        bufnr: pair.get(2).and_then(ext_handle).unwrap_or_default(),
                        path: canon,
                        is_terminal: pair.get(3).and_then(Value::as_bool).unwrap_or(false),
                        is_drawer: pair.get(4).and_then(Value::as_bool).unwrap_or(false),
                    },
                );
            }
        }

        let mut snap = self.snapshot.write_async().await;
        snap.buffers = map;
        snap.win_buffers = win_map;
        // The derived navigation target joins BOTH maps, and EITHER notification
        // can complete a transition, so the fold runs here as well as in
        // `apply_previews`.
        self.fold_nav_targets(&snap);
        // A buffers change is NOT a redraw frame, so `seq` is intentionally left
        // unchanged. The caller fires the wake after this lock drops.
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_tab_cwds` notification: WHOLESALE-REPLACES the snapshot's
    /// per-tabpage tab-local cwd map ([`Snapshot::tab_cwds`]) with the reported
    /// set.
    ///
    /// The single argument is a msgpack array of `[tabpage_handle, cwd]` pairs for
    /// ALL tabpages. Each `tabpage_handle` decodes via [`ext_handle`] (a plain
    /// integer, or defensively a msgpack-`Ext`-wrapped integer); each `cwd` is
    /// `getcwd(-1, <number>)` (the tab-local cwd, falling back to the global cwd
    /// for a tab never `:tcd`'d). Empty paths are SKIPPED; non-empty paths are
    /// canonicalized (to match the canonical cwds/paths the rest of core keys on),
    /// falling back to the raw path if canonicalization fails.
    ///
    /// The new map is built in full BEFORE the write guard is taken (so no
    /// filesystem work happens under the lock, sidestepping
    /// `clippy::await_holding_lock`), then assigned wholesale. The lock is acquired
    /// via [`SharedSnapshot::write_async`], is NOT held across any `.await`, and is
    /// dropped before returning so the caller's wake fires after the lock is
    /// released. This is NOT a redraw frame, so `seq` is left unchanged; the
    /// wholesale replacement drops any closed tabpages (a `TabClosed` cannot report
    /// the closed handle).
    async fn apply_tab_cwds(&self, args: Vec<Value>) {
        let mut map: HashMap<i64, PathBuf> = HashMap::new();
        if let Some(pairs) = args.first().and_then(Value::as_array) {
            for pair in pairs {
                let Some(pair) = pair.as_array() else {
                    continue;
                };
                let Some(tp) = pair.first().and_then(ext_handle) else {
                    continue;
                };
                let Some(dir) = pair.get(1).and_then(Value::as_str) else {
                    continue;
                };
                if dir.is_empty() {
                    continue;
                }
                let canon = std::fs::canonicalize(dir).unwrap_or_else(|_| PathBuf::from(dir));
                map.insert(tp, canon);
            }
        }

        let mut snap = self.snapshot.write_async().await;
        snap.tab_cwds = map;
        // A tab_cwds change is NOT a redraw frame, so `seq` is intentionally left
        // unchanged. The caller fires the wake after this lock drops.
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_open_files` notification: WHOLESALE-REPLACES the snapshot's
    /// Tier-1 open-file mirror ([`Snapshot::open_files`]) with the reported set.
    ///
    /// The single argument is a msgpack array of `[tabpage_handle, active_path,
    /// files]` triples, one per tabpage. Each `tabpage_handle` decodes via
    /// [`ext_handle`]; `active_path` is the tabpage's active buffer's name (empty →
    /// `None`); `files` is an array of the file-backed (non-empty, non-preview)
    /// buffer names open in the tabpage. Paths are canonicalized (to match the
    /// canonical paths the rest of core keys on), falling back to the raw path if
    /// canonicalization fails. A triple missing its handle is SKIPPED.
    ///
    /// The new map is built in full BEFORE the write guard is taken (so no
    /// filesystem work happens under the lock), then assigned wholesale. The lock is
    /// acquired via [`SharedSnapshot::write_async`], is NOT held across any
    /// `.await`, and is dropped before returning so the caller's wake fires after
    /// the lock is released. This is NOT a redraw frame, so `seq` is left unchanged;
    /// the wholesale replacement drops any closed tabpages.
    async fn apply_open_files(&self, args: Vec<Value>) {
        /// Canonicalizes a non-empty path string, raw fallback; empty → `None`.
        fn canon(s: &str) -> Option<PathBuf> {
            if s.is_empty() {
                None
            } else {
                Some(std::fs::canonicalize(s).unwrap_or_else(|_| PathBuf::from(s)))
            }
        }

        let mut map: HashMap<i64, TabFiles> = HashMap::new();
        if let Some(triples) = args.first().and_then(Value::as_array) {
            for triple in triples {
                let Some(triple) = triple.as_array() else {
                    continue;
                };
                let Some(tp) = triple.first().and_then(ext_handle) else {
                    continue;
                };
                let active = triple.get(1).and_then(Value::as_str).and_then(canon);
                let files = triple
                    .get(2)
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().and_then(canon))
                            .collect()
                    })
                    .unwrap_or_default();
                map.insert(tp, TabFiles { active, files });
            }
        }

        let mut snap = self.snapshot.write_async().await;
        snap.open_files = map;
        // An open_files change is NOT a redraw frame, so `seq` is intentionally left
        // unchanged. The caller fires the wake after this lock drops.
        // Lock drops here as `snap` goes out of scope at function return.
    }

    /// Applies a `wolf_full_width` notification: WHOLESALE-REPLACES the snapshot's
    /// per-window full-width map ([`Snapshot::full_width`]) with the reported set.
    ///
    /// The single argument is a msgpack array of `[win_id, full_width]` pairs for
    /// ALL windows in the current tabpage. Each `win_id` decodes via [`ext_handle`]
    /// (a plain integer, or defensively a msgpack-`Ext`-wrapped integer); each
    /// `full_width` is the window's buffer's `b:wolf_full_width` flag. A pair missing
    /// its handle is SKIPPED.
    ///
    /// The new map is built in full BEFORE the write guard is taken, then assigned
    /// wholesale. The lock is acquired via [`SharedSnapshot::write_async`], is NOT
    /// held across any `.await`, and is dropped before returning so the caller's wake
    /// fires after the lock is released. This is NOT a redraw frame, so `seq` is left
    /// unchanged; the wholesale replacement drops any stale/closed windows.
    async fn apply_full_width(&self, args: Vec<Value>) {
        let mut map: HashMap<i64, bool> = HashMap::new();
        if let Some(pairs) = args.first().and_then(Value::as_array) {
            for pair in pairs {
                let Some(pair) = pair.as_array() else {
                    continue;
                };
                let Some(win) = pair.first().and_then(ext_handle) else {
                    continue;
                };
                let full_width = pair.get(1).and_then(Value::as_bool).unwrap_or(false);
                map.insert(win, full_width);
            }
        }

        let mut snap = self.snapshot.write_async().await;
        snap.full_width = map;
        // A full_width change is NOT a redraw frame, so `seq` is intentionally left
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
/// carry it likewise.
///
/// The `ext_cmdline` / `ext_popupmenu` / `ext_messages` protocols are folded into
/// [`Snapshot::cmdline`] / [`Snapshot::pum`] / [`Snapshot::toast`]:
/// `cmdline_show`/`cmdline_pos`/`cmdline_hide`, `popupmenu_show`/
/// `popupmenu_select`/`popupmenu_hide`, `msg_show`/`msg_clear`/
/// `msg_history_show`.
///
/// Intentionally ignored (like `win_float_pos` — the surface is not rendered, or
/// the info is redundant under the current option set):
/// `win_float_pos` / `win_external_pos` / `msg_set_pos` (floats/external windows
/// are not rendered; message placement is handled natively);
/// `cmdline_special_char` (OD6 — the `?`/`^R` insert-hint char, v1 limitation);
/// `cmdline_block_show` / `cmdline_block_append` / `cmdline_block_hide` (OD5 —
/// interactive `:function` blocks, v1 limitation); `msg_showmode` /
/// `msg_showcmd` (showcmd off) / `msg_ruler` (ruler off).
///
/// `closed_wins` collects the nvim WINDOW handles of every `win_close`d grid (the
/// handle is only recoverable BEFORE the `snap.windows` entry is removed), so the
/// caller can prune those panes' navigation histories.
fn apply_event(
    snap: &mut RwLockWriteGuard<'_, Snapshot>,
    name: &str,
    params: Vec<Value>,
    closed_wins: &mut Vec<i64>,
) -> bool {
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
        "win_close" => win_close(snap, &params, closed_wins),
        "cmdline_show" => cmdline_show(snap, &params),
        "cmdline_pos" => cmdline_pos(snap, &params),
        "cmdline_hide" => cmdline_hide(snap),
        "popupmenu_show" => popupmenu_show(snap, &params),
        "popupmenu_select" => popupmenu_select(snap, &params),
        "popupmenu_hide" => popupmenu_hide(snap),
        "msg_show" => msg_show(snap, params),
        "msg_clear" => msg_clear(snap),
        "msg_history_show" => msg_history_show(snap, params),
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

fn win_close(snap: &mut Snapshot, params: &[Value], closed_wins: &mut Vec<i64>) {
    // [grid]
    //
    // Remove BOTH the window entry and its grid. Neovim also emits `grid_destroy`
    // for the same grid on a window close; removing the grid here too makes the
    // two paths idempotent so the grid is gone regardless of event ordering.
    let Some(grid) = params.first().and_then(as_u64) else {
        return;
    };
    // The payload carries only the GRID, so resolve the nvim WINDOW handle from
    // the entry we are about to remove and hand it to the caller's collector; that
    // pane's navigation history is pruned after the batch.
    //
    // KNOWN LIMIT (documented, deliberately not fixed): `win_hide` — fired for
    // every window of a tabpage being LEFT — already removes the `windows` entry,
    // so a later `win_close` for such a grid cannot recover the handle and that
    // pane's history is orphaned. nvim never reuses window handles, so an orphan is
    // only memory (bounded per pane per nvim-process lifetime). An "absent from
    // `wolf_buffers` for N emits" reaper would be WRONG: a window in a non-current
    // tabpage is absent from both maps by construction, and pruning it would
    // violate the locked no-prune rule.
    if let Some(win) = snap.windows.get(&grid).map(|w| w.win) {
        if win != 0 {
            closed_wins.push(win);
        }
    }
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

/// Parses a redraw "content" value — a list of styled `[attr, text, ...]` tuples
/// — into `(text, attr_id)` pairs, tolerating BOTH chunk forms:
///
/// * `msg_show` / `msg_history_show`: `[attr_id, text_chunk, hl_id]` — `attr_id`
///   (elem[0]) is an integer highlight-attr id (resolved against the
///   [`HighlightTable`](crate::render::HighlightTable) like a cell).
/// * `cmdline_show`: `[attrs, text]` — elem[0] may be an ATTRS-DICT (a map)
///   rather than an int; in that case the attr id defaults to 0.
///
/// So elem[0] is read via [`Value::as_u64`] with a `0` fallback (covering the
/// map form), and elem[1] is the text. Tuples with no text are skipped.
fn parse_content_chunks(content: &Value) -> Vec<(String, u64)> {
    let Some(chunks) = content.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let Some(parts) = chunk.as_array() else {
            continue;
        };
        // elem[0]: attr id as an int; a map (cmdline attrs-dict form) → 0.
        let attr_id = parts.first().and_then(Value::as_u64).unwrap_or(0);
        // elem[1]: the text.
        let Some(text) = parts.get(1).and_then(Value::as_str) else {
            continue;
        };
        out.push((text.to_string(), attr_id));
    }
    out
}

/// Splits parsed `(text, attr_id)` chunks into visual lines, breaking each
/// chunk's text on `"\n"` (a message may embed newlines, e.g. multi-line
/// `nvim_echo`). Returns one `Vec<ToastChunk>` per line. An empty chunk list
/// yields no lines.
fn split_content_into_lines(chunks: Vec<(String, u64)>) -> Vec<Vec<ToastChunk>> {
    if chunks.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<Vec<ToastChunk>> = vec![Vec::new()];
    for (text, attr_id) in chunks {
        for (i, seg) in text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if !seg.is_empty() {
                if let Some(last) = lines.last_mut() {
                    last.push(ToastChunk {
                        text: seg.to_string(),
                        attr_id,
                    });
                }
            }
        }
    }
    lines
}

/// Appends `lines` to the LAST message in `messages`, fusing the first appended
/// line onto the message's last line (`:echon` semantics). Returns `false` when
/// there is no message to append to (the caller then pushes a fresh message).
fn append_lines_to_last(messages: &mut [Message], lines: Vec<Vec<ToastChunk>>) -> bool {
    let Some(last) = messages.last_mut() else {
        return false;
    };
    let mut it = lines.into_iter();
    if let Some(first) = it.next() {
        if let Some(last_line) = last.lines.last_mut() {
            last_line.extend(first);
        } else {
            last.lines.push(first);
        }
    }
    for line in it {
        last.lines.push(line);
    }
    true
}

fn cmdline_show(snap: &mut Snapshot, params: &[Value]) {
    // [content, pos, firstc, prompt, indent, level]
    let content = params
        .first()
        .map(parse_content_chunks)
        .unwrap_or_default()
        .into_iter()
        .map(|(text, attr_id)| CmdlineChunk { text, attr_id })
        .collect();
    let pos = params.get(1).and_then(as_u64).unwrap_or(0) as usize;
    let firstc = params
        .get(2)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let prompt = params
        .get(3)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let indent = params.get(4).and_then(as_u64).unwrap_or(0) as usize;
    let level = params.get(5).and_then(as_u64).unwrap_or(0) as u32;

    snap.cmdline = Cmdline {
        active: true,
        firstc,
        prompt,
        content,
        pos,
        indent,
        level,
    };

    // If a confirm dialog was hidden on the last `cmdline_hide` (its messages
    // stashed in `pending_confirm`), decide whether THIS cmdline is that dialog
    // re-prompting after an invalid key or an unrelated cmdline. The hide batch is
    // byte-identical either way, so we distinguish by the prompt: a prompt-style
    // cmdline (empty `firstc`, non-empty `prompt`) whose prompt matches the stashed
    // options line is the confirm's own re-prompt → restore the toast. Anything
    // else (a `:`/`/` cmdline, or a different prompt like a fresh `input()`) means
    // the confirm resolved → drop the stash, leaving the toast cleared.
    if let Some(pending) = snap.pending_confirm.take() {
        let is_reprompt = snap.cmdline.firstc.is_empty()
            && !snap.cmdline.prompt.is_empty()
            && snap.cmdline.prompt == pending.prompt;
        if is_reprompt {
            snap.toast.messages = pending.messages;
            snap.toast.visible = true;
        }
        // else: `pending` drops here — stash discarded, toast stays cleared.
    }
}

fn cmdline_pos(snap: &mut Snapshot, params: &[Value]) {
    // [pos, level]
    if let Some(pos) = params.first().and_then(as_u64) {
        snap.cmdline.pos = pos as usize;
    }
    if let Some(level) = params.get(1).and_then(as_u64) {
        snap.cmdline.level = level as u32;
    }
}

fn cmdline_hide(snap: &mut Snapshot) {
    // [level, abort] — the (possibly nested) command line closed. v1 collapses
    // any recursion level to the single surface, so any hide clears it.
    //
    // Capture the options-line prompt BEFORE resetting the cmdline: a `confirm()`
    // dialog's options ride the cmdline while its prompt text rides the toast as a
    // `confirm` message. Neovim never emits `msg_clear` when the dialog resolves,
    // so if we left the toast alone the prompt would linger forever. But an INVALID
    // key produces a byte-identical hide+re-prompt, so we cannot clear outright:
    // instead STASH the confirm toast's messages (with the captured prompt) and
    // hide the toast, so a genuine resolve shows cleared immediately while a later
    // matching `cmdline_show` (see there) can restore it.
    let prompt = snap.cmdline.prompt.clone();
    snap.cmdline = Cmdline::default();
    if snap.toast.is_prompt() {
        let messages = std::mem::take(&mut snap.toast.messages);
        snap.toast.visible = false;
        snap.pending_confirm = Some(PendingConfirm { messages, prompt });
    }
}

fn popupmenu_show(snap: &mut Snapshot, params: &[Value]) {
    // [items, selected, row, col, grid]; item [word, kind, menu, info]
    let items = params
        .first()
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let parts = item.as_array()?;
                    let field = |i: usize| {
                        parts
                            .get(i)
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    };
                    Some(PumItem {
                        word: field(0),
                        kind: field(1),
                        menu: field(2),
                        info: field(3),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // `selected` is -1 for "no selection"; `grid` is -1 when anchored to the
    // command line — both need signed decoding.
    let selected = params.get(1).and_then(as_i64).unwrap_or(-1);
    let row = params.get(2).and_then(as_i64).unwrap_or(0);
    let col = params.get(3).and_then(as_i64).unwrap_or(0);
    let grid = params.get(4).and_then(as_i64).unwrap_or(0);

    snap.pum = Pum {
        visible: true,
        items,
        selected,
        row,
        col,
        grid,
    };
}

fn popupmenu_select(snap: &mut Snapshot, params: &[Value]) {
    // [selected]
    if let Some(selected) = params.first().and_then(as_i64) {
        snap.pum.selected = selected;
    }
}

fn popupmenu_hide(snap: &mut Snapshot) {
    // []
    snap.pum = Pum::default();
}

fn msg_show(snap: &mut Snapshot, params: Vec<Value>) {
    // A new message supersedes any confirm stashed mid-resolve (see
    // `cmdline_hide`): drop it so a later prompt-style `cmdline_show` cannot
    // resurrect a stale confirm toast underneath this message.
    snap.pending_confirm = None;
    // [kind, content, replace_last, history, append, id, trigger]
    let kind = params
        .first()
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // 1. `search_count` (`[1/3]`) is suppressed entirely.
    if kind == "search_count" {
        return;
    }

    let chunks = params.get(1).map(parse_content_chunks).unwrap_or_default();
    let text_empty = chunks.iter().all(|(t, _)| t.is_empty());

    // 2. an `empty` message with no content clears the toast.
    if kind == "empty" && text_empty {
        snap.clear_toast();
        return;
    }

    let replace_last = params.get(2).and_then(Value::as_bool).unwrap_or(false);
    let append = params.get(4).and_then(Value::as_bool).unwrap_or(false);
    // OD7: integer id only; a string/absent id is treated as 0 (no replace-by-id).
    let id = params.get(5).and_then(Value::as_i64).unwrap_or(0);

    let lines = split_content_into_lines(chunks);

    // 3/4. append onto the current message (`:echon`).
    if append && append_lines_to_last(&mut snap.toast.messages, lines.clone()) {
        snap.toast.visible = true;
        return;
    }

    // 5. replace the last message in place.
    if replace_last {
        if let Some(last) = snap.toast.messages.last_mut() {
            *last = Message { kind, id, lines };
            snap.toast.visible = true;
            return;
        }
    }

    // 6. replace an existing message sharing this (non-zero) id.
    if id != 0 {
        if let Some(existing) = snap.toast.messages.iter_mut().find(|m| m.id == id) {
            *existing = Message { kind, id, lines };
            snap.toast.visible = true;
            return;
        }
    }

    // 7. push a fresh message.
    snap.toast.messages.push(Message { kind, id, lines });
    snap.toast.visible = true;
}

fn msg_clear(snap: &mut Snapshot) {
    // []
    snap.clear_toast();
}

fn msg_history_show(snap: &mut Snapshot, params: Vec<Value>) {
    // A history render supersedes any confirm stashed mid-resolve (see
    // `cmdline_hide`): drop it so a later prompt-style `cmdline_show` cannot
    // resurrect a stale confirm toast underneath these messages.
    snap.pending_confirm = None;
    // [entries, prev_cmd]; entry [kind, content, append]
    let Some(entries) = params.first().and_then(Value::as_array) else {
        return;
    };

    let mut messages: Vec<Message> = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(parts) = entry.as_array() else {
            continue;
        };
        let kind = parts
            .first()
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let chunks = parts.get(1).map(parse_content_chunks).unwrap_or_default();
        let append = parts.get(2).and_then(Value::as_bool).unwrap_or(false);
        let lines = split_content_into_lines(chunks);

        if append && append_lines_to_last(&mut messages, lines.clone()) {
            continue;
        }
        messages.push(Message { kind, id: 0, lines });
    }

    snap.toast.messages = messages;
    snap.toast.visible = true;
}

#[cfg(test)]
mod tests {
    //! Direct unit tests for the `msg_show` fold's less-observable branches
    //! (steps 4–6 of the fold in [`msg_show`]): `append`, `replace_last`, and
    //! id-match replacement. These cannot be uniquely triggered by real-nvim
    //! keystrokes (a single keystroke that produces a same-frame `replace_last`
    //! / `append` / id-match is not drivable headlessly), so they are exercised
    //! deterministically by feeding synthetic `msg_show`-shaped params straight
    //! through the fold. The real-nvim `tests/messages.rs` covers the observable
    //! push + clear-on-input paths.

    use super::*;

    /// A message id used to pin id-match replacement (a named constant per the
    /// `mnd` lint's "no magic number >= 5" rule).
    const ID_TARGET: i64 = 42;
    /// A second, distinct message id that must be left untouched by an id-match
    /// replacement of [`ID_TARGET`].
    const ID_OTHER: i64 = 7;

    /// Builds a `msg_show` `content` value: one styled chunk `[attr_id, text,
    /// hl_id]` (the exact tuple order the fold parses via
    /// [`parse_content_chunks`]).
    fn content_one(text: &str) -> Value {
        Value::Array(vec![Value::Array(vec![
            Value::from(0u64),
            Value::from(text),
            Value::from(0u64),
        ])])
    }

    /// Builds `msg_show` params in the exact tuple order the coder implemented:
    /// `[kind, content, replace_last, history, append, id, trigger]`.
    fn msg_show_params(
        kind: &str,
        text: &str,
        replace_last: bool,
        append: bool,
        id: i64,
    ) -> Vec<Value> {
        vec![
            Value::from(kind),
            content_one(text),
            Value::from(replace_last),
            Value::from(false), // history (ignored by the fold)
            Value::from(append),
            Value::from(id),
            Value::from(false), // trigger (unused)
        ]
    }

    /// Builds prompt-style `cmdline_show` params (empty content, empty `firstc`,
    /// the given `prompt`) — the shape a confirm()/input() prompt cmdline arrives
    /// in. Tuple order matches `fn cmdline_show`: `[content, pos, firstc, prompt,
    /// indent, level]`.
    fn cmdline_show_params(prompt: &str) -> Vec<Value> {
        vec![
            Value::Array(vec![]), // content: no chunks
            Value::from(0u64),    // pos
            Value::from(""),      // firstc: empty => prompt-style
            Value::from(prompt),  // prompt
            Value::from(0u64),    // indent
            Value::from(1u64),    // level
        ]
    }

    /// Flattens a message's lines into one newline-joined string of its chunk
    /// texts, for content assertions.
    fn message_text(msg: &Message) -> String {
        msg.lines
            .iter()
            .map(|line| line.iter().map(|c| c.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Fold step 4: `append == true` fuses the new content onto the LAST line of
    /// the last message rather than starting a new message (`:echon` semantics).
    #[test]
    fn msg_show_append_fuses_onto_last_line() {
        let mut snap = Snapshot::default();
        msg_show(
            &mut snap,
            msg_show_params("echomsg", "hello", false, false, 0),
        );
        msg_show(
            &mut snap,
            msg_show_params("echomsg", " world", false, true, 0),
        );

        assert_eq!(
            snap.toast.messages.len(),
            1,
            "append must fuse onto the existing message, not start a new one"
        );
        let msg = &snap.toast.messages[0];
        assert_eq!(
            msg.lines.len(),
            1,
            "the appended chunk fuses onto the last line (no new line)"
        );
        assert_eq!(message_text(msg), "hello world");
        assert!(snap.toast.visible, "an append keeps the toast visible");
    }

    /// Fold step 5: `replace_last == true` replaces the last message in place —
    /// the message count is unchanged and the content is the new text.
    #[test]
    fn msg_show_replace_last_updates_in_place() {
        let mut snap = Snapshot::default();
        msg_show(
            &mut snap,
            msg_show_params("echomsg", "first", false, false, 0),
        );
        msg_show(
            &mut snap,
            msg_show_params("echomsg", "second", true, false, 0),
        );

        assert_eq!(
            snap.toast.messages.len(),
            1,
            "replace_last must replace in place, not append"
        );
        assert_eq!(message_text(&snap.toast.messages[0]), "second");
        assert!(snap.toast.visible, "a replace keeps the toast visible");
    }

    /// Fold step 6: a new message whose integer `id` matches a visible message
    /// replaces THAT message (and leaves the others untouched).
    #[test]
    fn msg_show_matching_id_replaces_that_message() {
        let mut snap = Snapshot::default();
        msg_show(
            &mut snap,
            msg_show_params("echomsg", "orig", false, false, ID_TARGET),
        );
        msg_show(
            &mut snap,
            msg_show_params("echomsg", "other", false, false, ID_OTHER),
        );
        // A fresh message re-using ID_TARGET replaces the first message in place.
        msg_show(
            &mut snap,
            msg_show_params("echomsg", "updated", false, false, ID_TARGET),
        );

        assert_eq!(
            snap.toast.messages.len(),
            2,
            "an id-match must replace the matching message, not append"
        );
        let text_for = |id: i64| {
            snap.toast
                .messages
                .iter()
                .find(|m| m.id == id)
                .map(message_text)
        };
        assert_eq!(
            text_for(ID_TARGET).as_deref(),
            Some("updated"),
            "the message sharing the id is replaced"
        );
        assert_eq!(
            text_for(ID_OTHER).as_deref(),
            Some("other"),
            "the non-matching message is left untouched"
        );
        assert!(snap.toast.visible, "an id-match keeps the toast visible");
    }

    /// `cmdline_hide` clears a lingering confirm PROMPT toast. A `confirm()`
    /// dialog's prompt arrives as a `msg_show` kind `"confirm"` (into `toast`),
    /// while its options line rides `cmdline`; when the dialog resolves Neovim
    /// emits ONLY `cmdline_hide` (never `msg_clear`), so the fold must clear the
    /// prompt toast here or it lingers forever.
    #[test]
    fn cmdline_hide_clears_a_confirm_prompt_toast() {
        let mut snap = Snapshot::default();
        msg_show(
            &mut snap,
            msg_show_params("confirm", "Choose", false, false, 0),
        );
        assert!(
            snap.toast.is_prompt(),
            "a `confirm` kind message is a prompt toast"
        );
        assert!(snap.toast.visible, "the prompt toast should be visible");

        cmdline_hide(&mut snap);

        assert!(
            !snap.toast.visible,
            "cmdline_hide must clear the resolved confirm prompt toast"
        );
        assert!(
            !snap.toast.is_prompt(),
            "no lingering confirm prompt should remain after the hide"
        );
        assert!(
            snap.toast.messages.is_empty(),
            "the cleared prompt toast should hold no messages"
        );
    }

    /// `cmdline_hide` leaves a NON-prompt toast intact — the load-bearing guard
    /// against over-clearing. An `echomsg` toast (kind not `"confirm"`) must
    /// survive an unrelated `cmdline_hide` unchanged.
    #[test]
    fn cmdline_hide_leaves_a_non_prompt_toast_intact() {
        let mut snap = Snapshot::default();
        msg_show(
            &mut snap,
            msg_show_params("echomsg", "hello", false, false, 0),
        );
        assert!(snap.toast.visible, "the echomsg toast should be visible");
        let before_len = snap.toast.messages.len();

        cmdline_hide(&mut snap);

        assert!(
            snap.toast.visible,
            "a non-prompt toast must survive an unrelated cmdline_hide"
        );
        assert_eq!(
            snap.toast.messages.len(),
            before_len,
            "the surviving toast's message count is unchanged"
        );
        assert_eq!(
            message_text(&snap.toast.messages[0]),
            "hello",
            "the surviving toast's content is unchanged"
        );
    }

    /// Stash-then-restore: a confirm prompt hidden on `cmdline_hide` (stashed
    /// into `pending_confirm`) is RESTORED when the dialog re-prompts with the
    /// SAME options-line prompt (the invalid-key path — Neovim re-prompts in a
    /// later flush after a non-answering key).
    #[test]
    fn cmdline_show_matching_reprompt_restores_stashed_confirm() {
        const OPTIONS_PROMPT: &str = "[y]es, (n)o: ";

        let mut snap = Snapshot::default();
        // The options line arrives as a prompt-style cmdline; the prompt text as
        // a `confirm` toast.
        cmdline_show(&mut snap, &cmdline_show_params(OPTIONS_PROMPT));
        msg_show(
            &mut snap,
            msg_show_params("confirm", "Choose", false, false, 0),
        );

        // The dialog "resolves" (from the fold's view) on cmdline_hide: the toast
        // is hidden and its messages are stashed pending a possible re-prompt.
        cmdline_hide(&mut snap);
        assert!(
            !snap.toast.visible,
            "cmdline_hide hides the confirm toast immediately"
        );
        assert!(
            snap.pending_confirm.is_some(),
            "the hidden confirm messages are stashed pending a re-prompt"
        );

        // The SAME options prompt re-appears (invalid key => nvim re-prompts).
        cmdline_show(&mut snap, &cmdline_show_params(OPTIONS_PROMPT));

        assert!(
            snap.toast.visible,
            "a matching re-prompt restores the stashed confirm toast"
        );
        assert!(
            snap.toast.is_prompt(),
            "the restored toast is again a confirm prompt"
        );
        assert!(
            snap.pending_confirm.is_none(),
            "a consumed stash is cleared after restore"
        );
        assert_eq!(
            message_text(&snap.toast.messages[0]),
            "Choose",
            "the restored toast carries the original prompt text"
        );
    }

    /// Staleness guard: a stashed confirm is DROPPED (not restored) when the next
    /// prompt-style `cmdline_show` carries a DIFFERENT prompt — e.g. the confirm
    /// resolved and an unrelated `input("Name: ")` prompt followed.
    #[test]
    fn cmdline_show_nonmatching_prompt_drops_stashed_confirm() {
        let mut snap = Snapshot::default();
        cmdline_show(&mut snap, &cmdline_show_params("[y]es, (n)o: "));
        msg_show(
            &mut snap,
            msg_show_params("confirm", "Choose", false, false, 0),
        );
        cmdline_hide(&mut snap);
        assert!(
            snap.pending_confirm.is_some(),
            "the confirm messages are stashed on hide"
        );

        // A DIFFERENT prompt appears: the stash is stale and must be discarded.
        cmdline_show(&mut snap, &cmdline_show_params("Name: "));

        assert!(
            !snap.toast.visible,
            "a non-matching prompt must not restore the stale confirm toast"
        );
        assert!(
            !snap.toast.is_prompt(),
            "no confirm prompt should remain after a non-matching cmdline_show"
        );
        assert!(
            snap.pending_confirm.is_none(),
            "the stale stash is dropped on a non-matching prompt"
        );
        assert!(
            snap.toast.messages.is_empty(),
            "the dropped confirm leaves no toast messages"
        );
    }
}
