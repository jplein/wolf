//! The front-end-facing surface of `wolf-core`.
//!
//! [`WolfCore`] owns the tokio runtime, the captive Neovim process, and the
//! shared render-state snapshot. Its API is intentionally FFI-shaped (plain
//! structs, handles, a single wake callback) so the same surface serves both the
//! GTK front-end (direct crate) and the macOS front-end (via swift-bridge later).
//!
//! Output is a snapshot the front-end reads on its UI thread after a wake; input
//! is plain calls that core hands to the tokio side to issue RPC. There is one
//! wake callback, fired on `flush`; no per-event callbacks cross the boundary.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::browser_preview::{self, BrowserPreview};
use crate::buffer_picker::BufInfo;
use crate::command::Command;
use crate::command_mru::CommandMru;
use crate::files::{self, FileSearch, FileSearchState};
use crate::input::{self, InputEvent, MouseEvent, Nvim, ScrollEvent};
use crate::layout::LayoutIntent;
use crate::liveness::{self, Liveness};
use crate::mru::{FileEntry, MruStore};
use crate::nav_history::{MenuRow, NavMove, NavTarget, MAX_MENU_ROWS};
use crate::nvim::{self, NvimConfig, NvimError, NvimProcess};
use crate::recent_dirs::RecentDirs;
use crate::redraw::{CommandQueue, IntentQueue, NavHistoryCell, RedrawHandler, TrackedCwd, WakeFn};
use crate::render::{OwnedSnapshotReadGuard, SharedSnapshot, Snapshot, TabFiles};
use crate::search::{self, SearchOptions, SearchSession, SearchState};
use crate::state::{spell_dir, state_dir, swap_dir_for};

/// How long the input pump waits after the first event of a burst for siblings
/// to enqueue, so a synchronous run of `send_input` calls coalesces into one
/// applied batch (one RPC for fused keys, hence one redraw flush).
const INPUT_COALESCE_DELAY: Duration = Duration::from_millis(8);

/// Default budget for a single interactive synchronous nvim RPC issued through
/// [`WolfCore::block_on_rpc`]. An RPC that exceeds this elapses to
/// [`NvimError::Timeout`], converting an unbounded main-thread hang into a
/// bounded stall that degrades to each call site's documented fallback.
///
/// Matches `SHUTDOWN_TIMEOUT`. Overridable at [`WolfCore::start`] via the
/// `WOLF_RPC_TIMEOUT_MS` env var (invalid / unset / zero → this default).
const RPC_TIMEOUT_DEFAULT: Duration = Duration::from_secs(5);

/// Configuration to start a [`WolfCore`].
#[derive(Clone, Debug)]
pub struct CoreConfig {
    /// Path to the in-repo `runtime/nvim/init.lua`.
    pub init_lua: PathBuf,
    /// Initial grid width in columns.
    pub cols: u16,
    /// Initial grid height in rows.
    pub rows: u16,
    /// Working directory: the FALLBACK search root (and the FALLBACK `directory`
    /// scope for MRU records, until nvim first reports its cwd). The captive Neovim
    /// is also launched here. The picker's LIVE search root is the out-of-band
    /// tracked `live_cwd()`; this configured cwd is used only before nvim first
    /// reports its cwd.
    pub cwd: PathBuf,
}

/// Errors from starting or driving the core.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Failed to build the tokio runtime.
    #[error("failed to build tokio runtime: {0}")]
    Runtime(#[source] std::io::Error),
    /// An error from the captive Neovim layer.
    #[error(transparent)]
    Nvim(#[from] NvimError),
}

/// A read guard over the current render-state [`Snapshot`].
///
/// The front-end holds this for the duration of a paint, then drops it. It
/// `Deref`s to [`Snapshot`]. The guard owns an `Arc` clone of the underlying
/// lock, so its lifetime is independent of the [`WolfCore`] it came from.
pub struct SnapshotReadGuard {
    guard: OwnedSnapshotReadGuard,
}

impl std::ops::Deref for SnapshotReadGuard {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// The live core: tokio runtime + captive Neovim + shared snapshot.
pub struct WolfCore {
    runtime: Runtime,
    /// The captive Neovim process. Behind a [`RefCell`] because
    /// [`restart_nvim`](WolfCore::restart_nvim) swaps in a fresh process by value
    /// through `&self` (the bridge only ever hands the front-end a `&self`).
    /// `WolfCore` is `!Send`, main-thread only, so the `RefCell` is never contended
    /// across threads; every borrow is bound to a LOCAL so no guard crosses an
    /// `.await` (satisfies `clippy::await_holding_refcell_ref`).
    nvim: RefCell<NvimProcess>,
    snapshot: SharedSnapshot,
    /// The drainable layout-intent queue. The redraw handler pushes parsed
    /// `wolf_layout` intents here; the front-end consumes them via
    /// [`WolfCore::drain_intents`] on wake.
    intents: IntentQueue,
    /// The drainable command queue. The redraw handler pushes user-triggered
    /// `wolf_command` commands (`:Wolf<Pascal>` Ex-commands / `require('wolf')`
    /// Lua calls) here; the front-end consumes them via
    /// [`WolfCore::drain_commands`] on wake.
    commands: CommandQueue,
    /// Ordered input queue. Every input event is enqueued here and applied by a
    /// single consumer task in FIFO order, so keystrokes never reorder (which
    /// would garble typed text).
    input_tx: UnboundedSender<InputEvent>,
    /// The always-on GLOBAL recent-files MRU (`<state_dir>/file-mru.sqlite`).
    /// Constructed once at [`WolfCore::start`] from [`state_dir`] (or
    /// [`MruStore::disabled`] when it cannot be resolved). Every row carries the
    /// project `directory` it was recorded under; both the picker seed (read) and
    /// the input pump's record (write) pass nvim's LIVE cwd as that directory, so
    /// the recent list follows `:cd`/`:lcd`/`:tcd`. Cloned (cheaply, it is
    /// `Arc`-backed) into the input pump so opens record under the right directory.
    mru: MruStore,
    /// The captive Neovim's live working directory, tracked out-of-band via the
    /// `wolf_cwd` notification (the `WolfDir` `DirChanged` autocmd). Read
    /// synchronously by the input pump to scope the MRU record (the `directory`
    /// column) when recording an opened file (`block_on(getcwd)` is impossible
    /// there — the pump runs on a tokio worker thread). `None` until nvim first
    /// reports its cwd; the pump then falls back to [`cwd`](Self::cwd).
    tracked_cwd: TrackedCwd,
    /// The always-on GLOBAL recent-directories MRU (`<state_dir>/folder-mru.sqlite`).
    /// Constructed once at [`WolfCore::start`] from [`state_dir`] (or
    /// [`RecentDirs::disabled`] when it cannot be resolved) and cloned into the
    /// redraw handler, which records each `wolf_cwd` directory change into it. Read
    /// back via [`WolfCore::recent_dirs`].
    recent_dirs: RecentDirs,
    /// Every pane's navigation history (back/forward), keyed by nvim WINDOW handle.
    /// Created once at [`WolfCore::start`] and cloned into the redraw handler, which
    /// FOLDS a derived target into it after every `wolf_buffers` / `wolf_preview`
    /// emit and PRUNES a closed window's stack on `win_close`. Read and moved by the
    /// six `history_*` methods.
    ///
    /// LOCK DISCIPLINE: this mutex is NEVER held across a
    /// [`block_on_rpc`](Self::block_on_rpc) and NEVER held while the snapshot lock is
    /// held from this side. The redraw fold takes them in the OPPOSITE order
    /// (snapshot then nav) but has no `.await`/RPC in between, so there is no
    /// lock-order cycle.
    nav_history: NavHistoryCell,
    /// The always-on GLOBAL command-palette MRU (`<state_dir>/command-mru.sqlite`).
    /// Constructed once at [`WolfCore::start`] from [`state_dir`] (or
    /// [`CommandMru::disabled`] when it cannot be resolved). Unlike
    /// [`recent_dirs`](Self::recent_dirs) it is NOT wired into the redraw handler —
    /// nothing auto-records into it; only an explicit palette activation records via
    /// the handle handed out by [`WolfCore::command_mru`].
    command_mru: CommandMru,
    /// The configured working directory: the file picker's FALLBACK search root
    /// and the FALLBACK `directory` scope for MRU records/reads. The live search
    /// root (and MRU directory scope) is the tracked
    /// [`live_cwd`](Self::live_cwd) (see [`WolfCore::start_file_search`]); this path
    /// is used only before nvim first reports its cwd.
    cwd: PathBuf,
    /// The lazily-started, loopback-only browser-preview HTTP server (one per
    /// core). `None` until the user first runs the "Preview in Browser" command
    /// ([`WolfCore::open_in_browser`]); once started it is reused (and re-pointed
    /// at the active markdown buffer) on subsequent invocations. Guarded by a
    /// `Mutex` that is NEVER held across the slow/contended nvim RPC `block_on`
    /// (bufnr resolution runs before the lock is taken); the only `block_on`
    /// under the lock is the trivial, self-contained TCP bind during lazy spawn.
    browser_preview: Mutex<Option<BrowserPreview>>,
    /// The per-RPC timeout budget for interactive synchronous nvim calls routed
    /// through [`block_on_rpc`](Self::block_on_rpc). Resolved ONCE at
    /// [`WolfCore::start`] from `WOLF_RPC_TIMEOUT_MS` (falling back to
    /// [`RPC_TIMEOUT_DEFAULT`]), so every guarded call site shares one bound.
    rpc_timeout: Duration,
    /// The captive nvim's liveness state (`Alive` / `NotResponding` / `Dead`),
    /// backed by an atomic. The supervisor marks `Dead` on an unexpected death;
    /// [`block_on_rpc`](WolfCore::block_on_rpc) escalates a `Timeout` to
    /// `NotResponding` and auto-clears it back to `Alive` on a subsequent
    /// successful RPC; [`restart_nvim`](WolfCore::restart_nvim) resets it to
    /// `Alive`. Read cheaply (no RPC) by [`liveness`](WolfCore::liveness).
    liveness: Arc<AtomicU8>,
    /// A clone of the same front-end wake fired on liveness transitions, so the
    /// front-end can re-read [`liveness`](WolfCore::liveness) and paint/recover.
    wake: WakeFn,
    /// The current RPC handle shared with the input pump. The pump reads a clone
    /// per batch; [`restart_nvim`](WolfCore::restart_nvim) swaps in the fresh
    /// nvim's handle so the pump keeps working after a respawn (a fixed captured
    /// handle would go stale).
    shared_handle: Arc<RwLock<Nvim>>,
    /// The redraw handler (it is `Clone`, carrying the SAME snapshot / intents /
    /// commands / tracked-cwd / recent-dirs / wake). Stored so a restart clones a
    /// fresh handler for the respawned nvim's io loop.
    handler: RedrawHandler,
    /// The nvim launch config, reused VERBATIM by a restart (same hermetic XDG
    /// `state_root`, same durable `swap_dir`, same cwd + initial grid size).
    nvim_cfg: NvimConfig,
    /// Reentrancy guard for [`restart_nvim`](WolfCore::restart_nvim): a concurrent
    /// or nested restart request is a no-op while one is in flight.
    restarting: AtomicBool,
    /// The live file-search session registry (see [`files::SearchRegistry`]).
    /// Created once at [`WolfCore::start`] and cloned into the redraw handler,
    /// which routes each `wolf_fs_changed` notification into it;
    /// [`start_file_search`](WolfCore::start_file_search) registers every new
    /// session so the debounced rescan keeps long-lived sessions (the tree
    /// sidebar) tracking writes/renames/deletes made through the captive nvim.
    search_registry: Arc<files::SearchRegistry>,
}

/// The process's canonicalized home directory, resolved at most once.
///
/// Mirrors [`crate::pane_status::pane_status`]'s expectation of a canonical home
/// so `strip_prefix` lines up with the canonical buffer path: takes
/// [`dirs::home_dir`], then [`std::fs::canonicalize`] with a graceful fallback to
/// the raw (uncanonicalized) home path when canonicalize fails, and `None` when
/// there is no home directory. Home is immutable for the process lifetime, so the
/// result (including the `realpath` syscall) is cached in a `OnceLock` rather than
/// recomputed on every per-pane repaint.
fn canonical_home() -> Option<&'static Path> {
    static HOME: OnceLock<Option<PathBuf>> = OnceLock::new();
    HOME.get_or_init(|| dirs::home_dir().map(|h| std::fs::canonicalize(&h).unwrap_or(h)))
        .as_deref()
}

impl WolfCore {
    /// Starts the core: builds a multi-thread tokio runtime, spawns the captive
    /// Neovim, wires `wake` into the redraw handler, and attaches as an
    /// `ext_linegrid` + `ext_multigrid` UI at the configured size.
    pub fn start(cfg: CoreConfig, wake: WakeFn) -> Result<Self, CoreError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(CoreError::Runtime)?;

        let cwd = cfg.cwd.clone();

        // Resolve the interactive-RPC timeout budget ONCE. An invalid, unset, or
        // zero `WOLF_RPC_TIMEOUT_MS` falls back to the default (the `> 0` guard
        // avoids a `Duration::ZERO` instant-timeout).
        let rpc_timeout = std::env::var("WOLF_RPC_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|&ms| ms > 0)
            .map(Duration::from_millis)
            .unwrap_or(RPC_TIMEOUT_DEFAULT);

        let tracked_cwd: TrackedCwd = Arc::new(RwLock::new(None));

        let snapshot = SharedSnapshot::new(cfg.cols as usize, cfg.rows as usize);
        let intents: IntentQueue = Arc::new(Mutex::new(VecDeque::new()));
        let commands: CommandQueue = Arc::new(Mutex::new(VecDeque::new()));
        let nav_history: NavHistoryCell =
            Arc::new(Mutex::new(crate::nav_history::NavHistory::default()));

        // The global recent-directories MRU is ALWAYS-ON: construct it once from
        // state_dir(), or a no-op disabled store when that cannot be resolved.
        // Cloned into the redraw handler, which records each cwd change into it.
        let recent_dirs = state_dir()
            .map(|d| RecentDirs::open(&d))
            .unwrap_or_else(RecentDirs::disabled);

        // The global command-palette MRU is ALSO always-on, constructed the same
        // way from state_dir(). Unlike recent_dirs it is NOT cloned into the redraw
        // handler: nothing auto-records into it (only palette activation does).
        let command_mru = state_dir()
            .map(|d| CommandMru::open(&d))
            .unwrap_or_else(CommandMru::disabled);

        // The global recent-files MRU is ALSO always-on, constructed the same way
        // from state_dir(). Every row carries the project `directory` it was
        // recorded under; `recent(directory)` filters to that directory, so a single
        // global DB keeps every project's recent list isolated. Cloned into the
        // input pump (below) so opens record under nvim's live cwd.
        let mru = state_dir()
            .map(|d| MruStore::open(&d))
            .unwrap_or_else(MruStore::disabled);

        // The file-search session registry: `start_file_search` registers each
        // live session; the redraw handler routes `wolf_fs_changed` events into
        // it, and it debounces + re-walks the registered sessions (see
        // `files::SearchRegistry`). Built on this runtime's handle so rescan
        // walks share the traversal's blocking pool.
        let search_registry = Arc::new(files::SearchRegistry::new(runtime.handle().clone()));

        let handler = RedrawHandler::new(
            snapshot.clone_handle(),
            intents.clone(),
            commands.clone(),
            tracked_cwd.clone(),
            recent_dirs.clone(),
            nav_history.clone(),
            search_registry.clone(),
            wake.clone(),
        );

        // The liveness cell starts Alive; the supervisor / block_on_rpc / restart
        // transition it. `wake` is cloned so liveness transitions can signal the
        // front-end (they do not bump `seq`, so the front-end observes them via the
        // raw-wake seam, not a flush).
        let liveness: Arc<AtomicU8> = Arc::new(AtomicU8::new(Liveness::Alive.to_u8()));

        // The durable, per-project swap dir (STABLE across respawns of this cwd).
        // Canonicalize the cwd first so the hash matches a caller that passes an
        // already-canonical cwd; `None` (unresolvable state_dir) makes NvimConfig
        // fall back to an ephemeral swap dir under its temp state root. The durable
        // spell dir takes NO cwd: it is global by design.
        let canonical_cwd = std::fs::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
        let swap_dir = swap_dir_for(&canonical_cwd);
        let spell_dir = spell_dir();

        let nvim_cfg = NvimConfig::from_env(
            cfg.init_lua,
            cwd.clone(),
            swap_dir,
            spell_dir,
            cfg.cols,
            cfg.rows,
        )
        .map_err(CoreError::Nvim)?;

        let nvim = runtime.block_on(async {
            let proc =
                nvim::spawn(&nvim_cfg, handler.clone(), liveness.clone(), wake.clone()).await?;
            proc.ui_attach(cfg.cols, cfg.rows).await?;
            proc.setup_current_file_tracking().await?;
            proc.setup_buffer_tracking().await?;
            proc.setup_full_width().await?;
            proc.setup_layout().await?;
            proc.setup_dir_tracking().await?;
            proc.setup_theme_tracking().await?;
            proc.setup_commands().await?;
            proc.setup_tabs().await?;
            proc.setup_open_files().await?;
            proc.setup_fs_events().await?;
            proc.setup_view_tracking().await?;
            proc.setup_preview().await?;
            Ok::<NvimProcess, NvimError>(proc)
        })?;

        // Spawn the single ordered input consumer. It owns its own RPC handle and
        // applies events strictly in FIFO order, so keystrokes never reorder.
        //
        // To match how a front-end delivers a burst (and how nvim coalesces its
        // redraws), the pump drains all currently-queued events before applying,
        // and fuses a run of consecutive `Key` events into one `nvim_input` call.
        // A burst of keys then produces a single RPC and a single redraw `flush`,
        // which is what callers that "send N keys, then wait for one flush" rely
        // on. A brief settle yield lets a synchronous burst fully enqueue first.
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<InputEvent>();
        // The pump reads a FRESH handle clone per batch from the shared cell, so a
        // restart (which swaps in the respawned nvim's handle) keeps it working; a
        // once-captured handle would go stale after a respawn.
        let shared_handle: Arc<RwLock<Nvim>> = Arc::new(RwLock::new(nvim.handle()));
        let pump_shared_handle = shared_handle.clone();
        let pump_mru = mru.clone();
        let pump_tracked_cwd = tracked_cwd.clone();
        let pump_fallback_cwd = cwd.clone();
        let pump_snapshot = snapshot.clone_handle();
        runtime.spawn(async move {
            while let Some(first) = input_rx.recv().await {
                // Let any siblings enqueued in the same synchronous burst arrive.
                tokio::time::sleep(INPUT_COALESCE_DELAY).await;

                let mut batch = vec![first];
                while let Ok(ev) = input_rx.try_recv() {
                    batch.push(ev);
                }

                // Read the CURRENT handle for this batch (survives a restart swap).
                let pump_nvim: Nvim = pump_shared_handle.read().clone();

                // Toast-clear-on-input (OD8): if this batch carries any dismissing
                // input (Key/Mouse/Scroll) and the toast is not a real prompt,
                // clear it BEFORE applying — so a fresh `msg_show` produced by that
                // same input repopulates the toast afterward. The write guard is
                // taken cooperatively and dropped immediately, never across the
                // apply `.await`.
                if batch.iter().any(InputEvent::is_toast_dismissing) {
                    let mut snap = pump_snapshot.write_async().await;
                    if !snap.toast.is_prompt() {
                        snap.clear_toast();
                    }
                    drop(snap);
                }

                input::apply_batch(&pump_nvim, &batch).await;

                // Record every opened file into the global MRU under nvim's LIVE
                // cwd (the tracked cell, falling back to the configured cwd until
                // nvim first reports) as the `directory` scope. Resolving here, not
                // at startup, makes the record path follow `:cd`/`:lcd`/`:tcd`.
                // Reading the cell clones out the path so the brief RwLock guard is
                // not held across the record. Fire-and-forget.
                for ev in &batch {
                    if let InputEvent::Open { path } | InputEvent::OpenAt { path, .. } = ev {
                        let record_cwd = pump_tracked_cwd
                            .read()
                            .clone()
                            .unwrap_or_else(|| pump_fallback_cwd.clone());
                        pump_mru.record(&record_cwd, path);
                    }
                }
            }
        });

        Ok(Self {
            runtime,
            nvim: RefCell::new(nvim),
            snapshot,
            intents,
            commands,
            input_tx,
            mru,
            tracked_cwd,
            recent_dirs,
            nav_history,
            command_mru,
            cwd,
            browser_preview: Mutex::new(None),
            rpc_timeout,
            liveness,
            wake,
            shared_handle,
            handler,
            nvim_cfg,
            restarting: AtomicBool::new(false),
            search_registry,
        })
    }

    /// Reads the current render-state snapshot (for painting on the UI thread).
    #[must_use]
    pub fn read_snapshot(&self) -> SnapshotReadGuard {
        SnapshotReadGuard {
            guard: self.snapshot.read_arc(),
        }
    }

    /// Forwards an input event to Neovim without blocking the caller.
    ///
    /// The event is enqueued on the ordered input channel and applied by the
    /// single consumer task in FIFO order; errors are logged (the front-end gets
    /// no synchronous result, matching the FFI-shaped contract).
    pub fn send_input(&self, event: InputEvent) {
        if self.input_tx.send(event).is_err() {
            tracing::warn!("input dropped: core input pump is gone");
        }
    }

    /// Copies the current visual-mode selection to text, or `None` if there is
    /// nothing to copy.
    ///
    /// Blocks briefly on the captive nvim to yank the selection (see
    /// [`crate::nvim::NvimProcess::yank_visual_selection`]) via
    /// [`block_on_rpc`](Self::block_on_rpc) (same pattern as
    /// [`resolve_search_root`](Self::resolve_search_root)), bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) ([`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`). An RPC error, a [`NvimError::Timeout`] on a wedged
    /// nvim, a `None` result (not in visual mode), or an empty selection all map
    /// to `None`, so the front-end only writes real text to the clipboard.
    #[must_use]
    pub fn copy_visual_selection(&self) -> Option<String> {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("yank_visual_selection", nvim.yank_visual_selection()) {
            Ok(Some(text)) if !text.is_empty() => Some(text),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!("copy_visual_selection failed: {e}");
                None
            }
        }
    }

    /// Pastes `text` into the active document (fire-and-forget via
    /// [`InputEvent::Paste`]).
    pub fn paste(&self, text: String) {
        self.send_input(InputEvent::Paste { text });
    }

    /// Opens `path` AND jumps the cursor to `line`/`col` (fire-and-forget via
    /// [`InputEvent::OpenAt`]).
    ///
    /// Used by the sidebar search panel when a matching line is clicked. `line`
    /// is 1-based (ripgrep's `line_number`); `col` is a 0-based byte offset
    /// (ripgrep's first-submatch start). The core's input pump records the open
    /// into the global MRU, exactly like [`InputEvent::Open`].
    pub fn open_at(&self, path: PathBuf, line: u64, col: u64) {
        self.send_input(InputEvent::OpenAt { path, line, col });
    }

    /// Reads the captive nvim's global `ignorecase` option (a one-shot seed for
    /// the sidebar search panel's "case sensitive" toggle).
    ///
    /// Blocks briefly on the captive nvim via [`block_on_rpc`](Self::block_on_rpc)
    /// (the same bounded pattern [`copy_visual_selection`](Self::copy_visual_selection)
    /// uses), bounded by [`rpc_timeout`](Self::rpc_timeout). An RPC error or a
    /// [`NvimError::Timeout`] on a wedged nvim degrades to `false`, so the toggle
    /// simply defaults to case-sensitive rather than hanging.
    #[must_use]
    pub fn ignorecase(&self) -> bool {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("ignorecase", nvim.ignorecase()) {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!("ignorecase query failed: {e}");
                false
            }
        }
    }

    /// Sets the global `background` option (fire-and-forget via
    /// [`InputEvent::SetBackground`]).
    ///
    /// `dark: true` selects `background=dark`, `false` selects `background=light`.
    /// The macOS front-end calls this with the resolved appearance at launch and,
    /// in `auto` mode, live when the OS toggles. Enqueued on the ordered input
    /// channel, so it applies in FIFO order relative to pending keystrokes.
    pub fn set_background(&self, dark: bool) {
        self.send_input(InputEvent::SetBackground { dark });
    }

    /// Snapshots the captive nvim's listed buffers (for seeding the buffer picker).
    ///
    /// Blocks briefly on the captive nvim to query `getbufinfo({'buflisted': 1})`
    /// (see [`crate::nvim::NvimProcess::list_buffers`]) via
    /// [`block_on_rpc`](Self::block_on_rpc), the same bounded blocking pattern
    /// [`copy_visual_selection`](Self::copy_visual_selection) and
    /// [`resolve_search_root`](Self::resolve_search_root) use — bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) ([`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`). An RPC error (including a [`NvimError::Timeout`] on a
    /// wedged nvim) is logged and yields an empty list (the picker just shows no
    /// candidates rather than failing).
    #[must_use]
    pub fn list_buffers(&self) -> Vec<BufInfo> {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("list_buffers", nvim.list_buffers()) {
            Ok(buffers) => buffers,
            Err(e) => {
                tracing::warn!("list_buffers failed: {e}");
                Vec::new()
            }
        }
    }

    /// Renders the Markdown SOURCE buffer `source_bufnr` to a themed HTML preview
    /// document (empty string on error).
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (the same bounded pattern
    /// [`list_buffers`](Self::list_buffers) uses, bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to read the buffer lines and render them via
    /// [`crate::markdown::render_html`] (see
    /// [`crate::nvim::NvimProcess::render_preview`]). An RPC error (e.g. a
    /// closed/unloaded buffer, a [`NvimError::Timeout`] on a wedged nvim, or after
    /// shutdown) is logged and yields an empty string, which the front-end web view
    /// loads as a blank document rather than failing.
    #[must_use]
    pub fn render_preview(&self, source_bufnr: i64) -> String {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("render_preview", nvim.render_preview(source_bufnr)) {
            Ok(html) => html,
            Err(e) => {
                tracing::warn!("render_preview({source_bufnr}) failed: {e}");
                String::new()
            }
        }
    }

    /// Opens (or reuses) the loopback-only browser-preview server rooted at the
    /// project (a tiny static site) and returns the loopback URL of the pinned
    /// Markdown buffer — `http://127.0.0.1:<port>/<relpath>` (or `.../` when the
    /// buffer is unnamed) — or the EMPTY string when the active buffer is not
    /// Markdown / on error.
    ///
    /// All captive-nvim queries run via [`block_on_rpc`](Self::block_on_rpc)
    /// FIRST — OUTSIDE the `browser_preview` lock — so no lock is ever held across
    /// a `block_on` (the crate's locking invariant), each bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) ([`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) so a wedged nvim degrades to `""` (or, for the cwd
    /// query, no root) on [`NvimError::Timeout`] rather than hanging: the pinned
    /// bufnr
    /// ([`crate::nvim::NvimProcess::current_markdown_bufnr`], non-positive → `""`),
    /// the pinned file's absolute path
    /// ([`crate::nvim::NvimProcess::buffer_path`]), and the captive nvim's cwd
    /// ([`crate::nvim::NvimProcess::current_dir`]). These build the
    /// [`browser_preview::Target`] (root + relpath) via
    /// [`browser_preview::build_target`]. Then the `browser_preview` slot is
    /// locked: an existing server is re-pointed at the new target and reused (same
    /// port); otherwise a server is lazily started (bound `127.0.0.1:0`, random
    /// port). A bind failure is logged and yields `""`. The front-end opens the
    /// returned URL in the system default browser; the browser resolves relative
    /// links against it, and the server serves the linked files.
    #[must_use]
    pub fn open_in_browser(&self) -> String {
        // Resolve everything from the captive nvim BEFORE taking the
        // browser_preview lock, so the lock is never held across a block_on. Each
        // block_on_rpc binds the borrow to a LOCAL that drops before the next, so
        // no `Ref` guard is ever held across the `.await`.
        let bufnr = {
            let nvim = self.nvim.borrow();
            match self.block_on_rpc("current_markdown_bufnr", nvim.current_markdown_bufnr()) {
                Ok(bufnr) => bufnr,
                Err(e) => {
                    tracing::warn!("open_in_browser: current_markdown_bufnr failed: {e}");
                    return String::new();
                }
            }
        };
        if bufnr <= 0 {
            return String::new();
        }

        let pinned_raw = {
            let nvim = self.nvim.borrow();
            self.block_on_rpc("buffer_path", nvim.buffer_path(bufnr))
                .unwrap_or_default()
        };
        let cwd = {
            let nvim = self.nvim.borrow();
            self.block_on_rpc("current_dir", nvim.current_dir()).ok()
        };
        let target = browser_preview::build_target(bufnr, cwd, &pinned_raw);

        let mut slot = self.browser_preview.lock();
        let port = if let Some(server) = slot.as_ref() {
            server.set_target(target.clone());
            server.port()
        } else {
            match browser_preview::spawn(
                &self.runtime,
                self.shared_handle.read().clone(),
                self.snapshot.clone_handle(),
                target.clone(),
            ) {
                Ok(server) => {
                    let port = server.port();
                    *slot = Some(server);
                    port
                }
                Err(e) => {
                    tracing::warn!("open_in_browser: failed to start preview server: {e}");
                    return String::new();
                }
            }
        };

        if target.pinned_relpath.is_empty() {
            format!("http://127.0.0.1:{port}/")
        } else {
            format!(
                "http://127.0.0.1:{port}/{}",
                browser_preview::encode_url_path(&target.pinned_relpath)
            )
        }
    }

    /// Previews the active source buffer in place — the active pane becomes the
    /// preview pane — and returns the preview scratch buffer's number (`0` on error
    /// or when the active buffer is not Markdown).
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to run the `wolf_open_preview` helper (see
    /// [`crate::nvim::NvimProcess::open_preview`]). The bind never creates a pane;
    /// the front-end learns the source↔preview mapping on the next wake (via the
    /// `wolf_preview` notification), so no synchronous window handle is needed. An
    /// RPC error (including a [`NvimError::Timeout`] on a wedged nvim) is logged
    /// and returns `0`.
    #[must_use]
    pub fn open_preview(&self) -> i64 {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("open_preview", nvim.open_preview()) {
            Ok(bufnr) => bufnr,
            Err(e) => {
                tracing::warn!("open_preview() failed: {e}");
                0
            }
        }
    }

    /// Navigates from the active PREVIEW pane back to its source file in place —
    /// the active pane becomes an edit pane — and returns the source buffer's
    /// number (`0` on error or when the active buffer is not a preview pane).
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to run the `wolf_edit_source` helper (see
    /// [`crate::nvim::NvimProcess::edit_source`]). The bind never creates a pane;
    /// the front-end learns the updated snapshot on the next wake (via the
    /// `wolf_preview` / `wolf_current_file` notifications), so no synchronous
    /// window handle is needed. An RPC error (including a [`NvimError::Timeout`] on
    /// a wedged nvim) is logged and returns `0`.
    #[must_use]
    pub fn edit_source(&self) -> i64 {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("edit_source", nvim.edit_source()) {
            Ok(bufnr) => bufnr,
            Err(e) => {
                tracing::warn!("edit_source() failed: {e}");
                0
            }
        }
    }

    /// Repoints the preview window `win` at the Markdown file at absolute `path`
    /// (a linked file the user clicked in the preview), replacing that pane's preview
    /// with the linked file's, and returns the linked file's SOURCE bufnr.
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to run the `wolf_preview_file` helper (see
    /// [`crate::nvim::NvimProcess::preview_file`]).
    /// `path` is ABSOLUTE — resolved by the front-end from WebKit's `file://` URL;
    /// no relative resolution happens here. Returns the linked file's source bufnr
    /// (`> 0`) on success; `0` when `path` is empty, unreadable, or not Markdown, or
    /// on RPC error (including a [`NvimError::Timeout`] on a wedged nvim, logged). The front-end applies its own O3 fallback on `0` (hand
    /// an existing-but-non-previewable file to the OS default app; no-op if missing).
    #[must_use]
    pub fn preview_file(&self, win: i64, path: &str) -> i64 {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("preview_file", nvim.preview_file(win, path)) {
            Ok(bufnr) => bufnr,
            Err(e) => {
                tracing::warn!("preview_file({win}, {path}) failed: {e}");
                0
            }
        }
    }

    /// Previews the already-open buffer `bufnr` in window `win` (`0` = the current
    /// window), replacing that pane's buffer with the source's preview scratch, and
    /// returns the source bufnr — the preview-mode buffer-picker activation path.
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to run the `wolf_preview_buffer` helper (see
    /// [`crate::nvim::NvimProcess::preview_buffer`]). Returns the source bufnr
    /// (`> 0`) on success; `0` when `bufnr` is invalid, unnamed, or not Markdown,
    /// or on RPC error (including a [`NvimError::Timeout`] on a wedged nvim,
    /// logged). The front-end falls back to a plain buffer switch on `0` (the pane
    /// converts to edit mode).
    #[must_use]
    pub fn preview_buffer(&self, win: i64, bufnr: i64) -> i64 {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("preview_buffer", nvim.preview_buffer(win, bufnr)) {
            Ok(source) => source,
            Err(e) => {
                tracing::warn!("preview_buffer({win}, {bufnr}) failed: {e}");
                0
            }
        }
    }

    /// Previews the active pane's Markdown file in ANOTHER pane — binding the
    /// preview scratch into `target_win` when it is a usable other pane, else
    /// splitting a new pane to the right — and returns the SOURCE bufnr (`0` on
    /// error or when the active buffer is not Markdown).
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to run the `wolf_preview_in_other` helper (see
    /// [`crate::nvim::NvimProcess::preview_in_other`]). The helper gates before
    /// any split, so a rejected call never changes the layout; a split surfaces
    /// as an ordinary vertical Split layout intent on the next wake. Focus stays
    /// in the source-editing pane. An RPC error (including a
    /// [`NvimError::Timeout`] on a wedged nvim) is logged and returns `0`.
    #[must_use]
    pub fn preview_in_other(&self, target_win: i64) -> i64 {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("preview_in_other", nvim.preview_in_other(target_win)) {
            Ok(bufnr) => bufnr,
            Err(e) => {
                tracing::warn!("preview_in_other({target_win}) failed: {e}");
                0
            }
        }
    }

    /// Edits the active preview pane's SOURCE file in ANOTHER pane — binding the
    /// source into `target_win` when it is a usable other pane, else splitting
    /// (the scratch keeps the new right window, the source lands in the
    /// original) — and returns the bufnr bound into that pane (`0` on error or
    /// when the active pane is not a preview).
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to run the `wolf_edit_in_other` helper (see
    /// [`crate::nvim::NvimProcess::edit_in_other`]). The helper gates before any
    /// split and FOCUSES the pane now editing the source. An RPC error
    /// (including a [`NvimError::Timeout`] on a wedged nvim) is logged and
    /// returns `0`.
    #[must_use]
    pub fn edit_in_other(&self, target_win: i64) -> i64 {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("edit_in_other", nvim.edit_in_other(target_win)) {
            Ok(bufnr) => bufnr,
            Err(e) => {
                tracing::warn!("edit_in_other({target_win}) failed: {e}");
                0
            }
        }
    }

    /// Whether pane `win` has an older entry to go Back to.
    ///
    /// A cheap, LOCK-FREE-of-the-snapshot read of the sibling navigation-history
    /// cell (a brief mutex, no RPC) — same cost class as [`Self::cwd`]. `false` for
    /// an unknown window, a never-navigated pane, or the terminal DRAWER (which is
    /// EXCLUDED from history entirely).
    #[must_use]
    pub fn history_can_back(&self, win: i64) -> bool {
        self.nav_history.lock().can_back(win)
    }

    /// Whether pane `win` has a newer entry to go Forward to. See
    /// [`history_can_back`](Self::history_can_back).
    #[must_use]
    pub fn history_can_forward(&self, win: i64) -> bool {
        self.nav_history.lock().can_forward(win)
    }

    /// Moves pane `win` one entry BACK in its navigation history and returns
    /// whether the pane actually moved.
    ///
    /// The cursor moves (and the pane's feedback-loop `expected` is armed) BEFORE
    /// the nvim RPC is issued, so a redraw fold racing that RPC's autocmds already
    /// sees it and ABSORBS the resulting `wolf_buffers` emit instead of pushing a
    /// duplicate entry. When the nvim-side helper reports failure (a `:bwipeout`ed
    /// buffer, a deleted or non-Markdown preview source) the cursor is REVERTED —
    /// see [`crate::nav_history`].
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout)). Returns `false` at the bottom of the
    /// stack, for an unknown window, and on RPC error.
    pub fn history_back(&self, win: i64) -> bool {
        // The nav mutex is taken, used, and RELEASED before the RPC below; it is
        // never held across `block_on_rpc` and never held with the snapshot lock.
        let mv = self.nav_history.lock().back(win);
        match mv {
            Some(mv) => self.apply_nav(win, &mv),
            None => false,
        }
    }

    /// Moves pane `win` one entry FORWARD in its navigation history and returns
    /// whether the pane actually moved. Mirrors [`history_back`](Self::history_back)
    /// exactly.
    pub fn history_forward(&self, win: i64) -> bool {
        let mv = self.nav_history.lock().forward(win);
        match mv {
            Some(mv) => self.apply_nav(win, &mv),
            None => false,
        }
    }

    /// Moves pane `win` to the STACK index `index` (a [`MenuRow::index`] handed back
    /// from [`history_entries`](Self::history_entries)) and returns whether the pane
    /// actually moved.
    ///
    /// Returns `false` — a true no-op — for an out-of-range index and for the
    /// CURSOR'S OWN index, so clicking the checkmarked History-menu row does
    /// nothing. Otherwise identical to [`history_back`](Self::history_back).
    pub fn history_jump(&self, win: i64, index: usize) -> bool {
        let mv = self.nav_history.lock().jump(win, index);
        match mv {
            Some(mv) => self.apply_nav(win, &mv),
            None => false,
        }
    }

    /// Renders pane `win`'s History-menu rows: NEWEST-FIRST, capped at
    /// [`MAX_MENU_ROWS`] DISTINCT targets, with finished disambiguated display
    /// strings (home already abbreviated to `~`) and at most one `is_current` row.
    ///
    /// Each [`MenuRow::index`] is a real STACK index, usable verbatim with
    /// [`history_jump`](Self::history_jump). Empty for an unknown window, a
    /// never-navigated pane, or the terminal DRAWER. A brief mutex, no RPC.
    #[must_use]
    pub fn history_entries(&self, win: i64) -> Vec<MenuRow> {
        self.nav_history
            .lock()
            .menu_rows(win, MAX_MENU_ROWS, canonical_home())
    }

    /// Issues the nvim RPC that actually moves pane `win` to `mv.target`, and
    /// reverts the cursor when it fails.
    ///
    /// The mechanism is per-target: an [`Edit`](NavTarget::Edit) goes through the
    /// window-scoped `_G.wolf_history_open`, a [`Preview`](NavTarget::Preview)
    /// re-uses the existing `_G.wolf_preview_file` link-follow path, and a
    /// [`Terminal`](NavTarget::Terminal) / [`Untitled`](NavTarget::Untitled) goes
    /// through `_G.wolf_history_set_buf`. All three are window-scoped
    /// `nvim_win_set_buf` (never `:edit`), so no disk re-read and no
    /// `current_file`/file-MRU churn.
    ///
    /// The `RefCell` borrow is bound to a LOCAL and dropped before the nav mutex is
    /// taken (`clippy::await_holding_refcell_ref`), and the nav mutex is NEVER held
    /// across the RPC. A helper result of `0` (or an RPC error) reverts the cursor —
    /// but only if a racing fold has not already reconciled the pane — and returns
    /// `false`.
    fn apply_nav(&self, win: i64, mv: &NavMove) -> bool {
        let nvim = self.nvim.borrow();
        let outcome = match &mv.target {
            NavTarget::Edit { path } => {
                let path = path.to_string_lossy().into_owned();
                self.block_on_rpc("history_open", nvim.history_open(win, &path))
            }
            NavTarget::Preview { source } => {
                let path = source.to_string_lossy().into_owned();
                self.block_on_rpc("preview_file", nvim.preview_file(win, &path))
            }
            NavTarget::Terminal { bufnr } | NavTarget::Untitled { bufnr } => {
                self.block_on_rpc("history_set_buf", nvim.history_set_buf(win, *bufnr))
            }
        };
        drop(nvim);

        let bound = match outcome {
            Ok(bufnr) => bufnr,
            Err(e) => {
                tracing::warn!("history navigation for win {win} failed: {e}");
                0
            }
        };
        if bound > 0 {
            return true;
        }
        // The move did not land: put the cursor back where it was (a no-op if a
        // racing `observe` already reconciled this pane).
        self.nav_history.lock().revert(win, mv.to, mv.from);
        false
    }

    /// Returns whether the buffer currently being edited is a Markdown file.
    ///
    /// A cheap, synchronous main-thread read of [`Snapshot::is_markdown_active`]
    /// (`current_filetype == "markdown"`, fed out-of-band via `wolf_current_file`).
    /// The front-end gates the Preview command on this (the Copy/visual-mode
    /// precedent).
    #[must_use]
    pub fn is_markdown_active(&self) -> bool {
        self.read_snapshot().is_markdown_active()
    }

    /// Returns whether the buffer currently being edited is a PREVIEW buffer.
    ///
    /// A cheap, synchronous main-thread read of [`Snapshot::is_preview_active`]
    /// (`current_is_preview`, fed out-of-band via `wolf_current_file`'s 3rd arg).
    /// The front-end gates the Edit command on this (mirrors `is_markdown_active`).
    #[must_use]
    pub fn is_preview_active(&self) -> bool {
        self.read_snapshot().is_preview_active()
    }

    /// Returns whether the buffer currently being edited is in FULL-WIDTH mode.
    ///
    /// A cheap, synchronous main-thread read of [`Snapshot::is_full_width_active`]
    /// (`current_is_full_width`, fed out-of-band via `wolf_current_file`'s 4th arg).
    /// The front-end drives the View-menu "Use full width for content" checkmark on
    /// this (mirrors `is_preview_active`).
    #[must_use]
    pub fn is_full_width_active(&self) -> bool {
        self.read_snapshot().is_full_width_active()
    }

    /// Returns whether the window with handle `win` is in FULL-WIDTH mode.
    ///
    /// A cheap, synchronous main-thread read of [`Snapshot::full_width_for_win`]
    /// (the per-window `full_width` map, fed out-of-band via `wolf_full_width`). The
    /// front-end threads this into a pane's geometry (`maxCols: nil` for full width,
    /// `90` for the ~90-char default).
    #[must_use]
    pub fn full_width_for_win(&self, win: i64) -> bool {
        self.read_snapshot().full_width_for_win(win)
    }

    /// Creates a new nvim TABPAGE (optionally rooted at `cwd` via `:tcd`) and
    /// returns its stable HANDLE (`0` on error).
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (the same bounded pattern
    /// [`list_buffers`](Self::list_buffers) uses, bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to run the `wolf_create_tabpage` helper. The macOS
    /// front-end calls this to add a view (a new tabpage) to a project's shared
    /// nvim. An RPC error (including a [`NvimError::Timeout`] on a wedged nvim) is
    /// logged and returns `0` so the caller can drop the would-be view.
    #[must_use]
    pub fn create_tabpage(&self, cwd: Option<&Path>) -> i64 {
        let cwd = cwd
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("create_tabpage", nvim.create_tabpage(&cwd)) {
            Ok(handle) => handle,
            Err(e) => {
                tracing::warn!("create_tabpage failed: {e}");
                0
            }
        }
    }

    /// Realizes the saved pane-split tree described by `spec_json` in the current
    /// tabpage's single starting window (via `:split`/`:vsplit` + `:edit`) and
    /// returns the resulting window handles in DFS/leaf order — the SAME order as
    /// the front-end `PaneTree.leaves()` / `PaneTree.rebuilt`.
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (the same bounded pattern
    /// [`create_tabpage`](Self::create_tabpage) uses, bounded by
    /// [`rpc_timeout`](Self::rpc_timeout)) to run the `wolf_rebuild_layout` helper.
    /// nvim produces only the window COUNT + TOPOLOGY + per-window BUFFER; sizes
    /// come from the front-end's weights. An RPC error (including a
    /// [`NvimError::Timeout`] on a wedged nvim) is logged and returns `vec![]`.
    #[must_use]
    pub fn rebuild_layout(&self, spec_json: &str) -> Vec<i64> {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("rebuild_layout", nvim.rebuild_layout(spec_json)) {
            Ok(handles) => handles,
            Err(e) => {
                tracing::warn!("rebuild_layout failed: {e}");
                Vec::new()
            }
        }
    }

    /// Returns the CANONICALIZED tab-local working directory of tabpage `handle`,
    /// or `None` for an unknown/closed handle (or one nvim has not yet reported).
    ///
    /// A cheap, lock-free SNAPSHOT READ with NO nvim RPC — it does not block.
    /// [`Snapshot::tab_cwds`] is maintained out-of-band by the `wolf_tab_cwds`
    /// notification (the `WolfTabs` autocmds in `runtime/nvim/init.lua`), which
    /// wholesale-replaces the map on every `DirChanged`/`TabNewEntered`/`TabClosed`
    /// (plus one initial emit), with each cwd already canonicalized in
    /// [`apply_tab_cwds`](crate::redraw::RedrawHandler::apply_tab_cwds). This is
    /// AUTHORITATIVE even for a NON-current view's cwd — the gap that
    /// [`live_cwd`](Self::live_cwd) (current view only) cannot cover — without the
    /// blocking round-trip the old `wolf_tab_cwd` RPC required. `None` until nvim
    /// first reports the tab (the same neutral fallback the old timeout failure
    /// produced), and after a `TabClosed` prunes the handle from the map.
    #[must_use]
    pub fn cwd_for_tabpage(&self, handle: i64) -> Option<PathBuf> {
        self.read_snapshot().tab_cwds.get(&handle).cloned()
    }

    /// Returns the current tabpage's stable HANDLE (`0` on error) — the
    /// authoritative first-view bind.
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`, degrading to `0` on [`NvimError::Timeout`]) via
    /// `nvim_get_current_tabpage`. Contrast [`Snapshot::current_tabpage`] (the
    /// cheap live gate, updated out-of-band via `wolf_tabpage`): this is the
    /// authoritative value read at bind time.
    #[must_use]
    pub fn current_tabpage(&self) -> i64 {
        let nvim = self.nvim.borrow();
        match self.block_on_rpc("current_tabpage", nvim.current_tabpage()) {
            Ok(handle) => handle,
            Err(e) => {
                tracing::warn!("current_tabpage failed: {e}");
                0
            }
        }
    }

    /// Makes tabpage `handle` current (fire-and-forget via
    /// [`InputEvent::SetTabpage`]).
    ///
    /// Enqueued on the same ordered input channel as keystrokes so it never races
    /// ahead of or behind pending input; applied via `nvim_set_current_tabpage`.
    pub fn set_current_tabpage(&self, handle: i64) {
        self.send_input(InputEvent::SetTabpage { handle });
    }

    /// Closes tabpage `handle` (fire-and-forget via [`InputEvent::CloseTabpage`]).
    ///
    /// Enqueued on the same ordered input channel as keystrokes so it never races
    /// ahead of or behind pending input; applied via the init.lua
    /// `wolf_close_tabpage` helper (`<n>tabclose` under `pcall`). Used for a
    /// non-last view close; the front-end shuts the core down for the last view.
    pub fn close_tabpage(&self, handle: i64) {
        self.send_input(InputEvent::CloseTabpage { handle });
    }

    /// Switches window `win` to buffer `bufnr` (fire-and-forget via
    /// [`InputEvent::SwitchBuffer`]).
    ///
    /// Enqueued on the same ordered input channel as keystrokes so it never races
    /// ahead of or behind pending input; applied via the init.lua
    /// `wolf_switch_buffer` helper (window-scoped `nvim_win_set_buf` + per-pane
    /// view restoration).
    pub fn switch_buffer(&self, win: i64, bufnr: i64) {
        self.send_input(InputEvent::SwitchBuffer { win, bufnr });
    }

    /// Toggles the per-project terminal drawer (fire-and-forget via
    /// [`InputEvent::ToggleTerminal`]).
    ///
    /// Enqueued on the same ordered input channel as keystrokes; applied via the
    /// init.lua `wolf_toggle_terminal` helper, which opens a bottom `:terminal`
    /// split in the current tabpage or hides it if already open (the shared
    /// terminal buffer/job persists across toggles).
    pub fn toggle_terminal(&self) {
        self.send_input(InputEvent::ToggleTerminal);
    }

    /// Toggles the active buffer's content width between the ~90-char default and
    /// full pane width (fire-and-forget via [`InputEvent::ToggleFullWidth`]).
    ///
    /// Enqueued on the same ordered input channel as keystrokes; applied via the
    /// init.lua `wolf_toggle_full_width` helper, which flips `b:wolf_full_width` on
    /// the effective buffer (the SOURCE buffer when the active pane is a preview
    /// scratch — OQ-1) and re-emits the current-file / per-window full-width /
    /// previews notifications.
    pub fn toggle_full_width(&self) {
        self.send_input(InputEvent::ToggleFullWidth);
    }

    /// Drains and returns all pending layout intents (in arrival order).
    ///
    /// The front-end calls this on each wake to consume any `wolf_layout` intents
    /// (splits / focus-direction moves) the redraw handler has pushed since the
    /// last drain. Returns an empty `Vec` if none are pending.
    #[must_use]
    pub fn drain_intents(&self) -> Vec<LayoutIntent> {
        let mut queue = self.intents.lock();
        queue.drain(..).collect()
    }

    /// Drains and returns all pending commands (in arrival order).
    ///
    /// The front-end calls this on each wake to consume any user-triggered
    /// `wolf_command` commands (`:Wolf<Pascal>` Ex-commands / `require('wolf')`
    /// Lua calls) the redraw handler has pushed since the last drain, mapping each
    /// to a native action. Returns an empty `Vec` if none are pending — which,
    /// unlike layout intents, is always the case right after attach (commands are
    /// user-triggered only; there is no emit-on-attach).
    #[must_use]
    pub fn drain_commands(&self) -> Vec<Command> {
        let mut queue = self.commands.lock();
        queue.drain(..).collect()
    }

    /// Focuses a specific Neovim window (e.g. click-to-focus, or after resolving a
    /// [`LayoutIntent::FocusDir`] into a concrete neighbour window).
    ///
    /// Enqueued on the same ordered input channel as keystrokes so it never races
    /// ahead of or behind pending input; applied via `nvim_set_current_win`.
    pub fn focus_win(&self, win: i64) {
        self.send_input(InputEvent::FocusWin { win });
    }

    /// Forwards the OS window/app focus state to the captive Neovim UI via
    /// `nvim_ui_set_focus`, so `FocusGained` / `FocusLost` fire in the running app.
    ///
    /// A front-end reports focus GAINED (`true`) on app/window activation and focus
    /// LOST (`false`) on resign. `FocusGained` drives the init.lua `WolfAutoRead`
    /// augroup's `:checktime`, reloading an externally-changed unmodified buffer
    /// when the user returns to Wolf (e.g. Dock reactivation). Both edges are
    /// reported because nvim only fires `FocusGained` on a real unfocused→focused
    /// transition. Enqueued on the same ordered input channel as keystrokes so it
    /// never races pending input; applied via `nvim_ui_set_focus`.
    pub fn set_ui_focus(&self, gained: bool) {
        self.send_input(InputEvent::SetUiFocus { gained });
    }

    /// Forwards a mouse event (click / drag / release) to Neovim.
    ///
    /// Enqueued on the same ordered input channel as keystrokes so it never races
    /// ahead of or behind pending input; applied via `nvim_input_mouse` targeting
    /// [`MouseEvent::grid`] with grid-local row/col.
    pub fn send_mouse(&self, ev: MouseEvent) {
        self.send_input(InputEvent::Mouse(ev));
    }

    /// Forwards a scroll-wheel event to Neovim.
    ///
    /// Enqueued on the same ordered input channel as keystrokes so it never races
    /// ahead of or behind pending input; applied via `nvim_input_mouse("wheel", …)`
    /// targeting [`ScrollEvent::grid`] with grid-local row/col.
    pub fn send_scroll(&self, ev: ScrollEvent) {
        self.send_input(InputEvent::Scroll(ev));
    }

    /// Returns the most-recently-opened files (newest first), pruning any whose
    /// backing file no longer exists. See [`MruStore::recent`].
    ///
    /// The rows are filtered to nvim's LIVE cwd (the tracked cell, falling back to
    /// the configured `cwd` until nvim first reports) as the `directory` scope, so
    /// the entries' `display` strings relativize against the same directory the
    /// picker is rooted at. A disabled/unreadable store yields an empty result.
    #[must_use]
    pub fn mru_recent(&self) -> Vec<FileEntry> {
        let cwd = self
            .tracked_cwd
            .read()
            .clone()
            .unwrap_or_else(|| self.cwd.clone());
        self.mru.recent(&cwd)
    }

    /// Returns the global recent directories (newest first), capped at
    /// [`crate::RECENT_DIRS_LIMIT`]. See [`RecentDirs::recent`].
    ///
    /// These are the directories the captive Neovim has changed into (via the
    /// "open folder" command or a typed `:cd`), recorded from the `wolf_cwd`
    /// notification into the always-on `<state_dir>/folder-mru.sqlite`. Returns an
    /// empty `Vec` when the store is disabled or unreadable.
    #[must_use]
    pub fn recent_dirs(&self) -> Vec<PathBuf> {
        self.recent_dirs.recent()
    }

    /// Returns a clone of the global command-palette MRU handle
    /// (`<state_dir>/command-mru.sqlite`, or a disabled no-op store).
    ///
    /// The clone is cheap (`Arc`-backed). The palette front-end reads
    /// [`CommandMru::recent`] to float recently-used commands to the top and calls
    /// [`CommandMru::record`] on activation. Nothing else records into this store.
    #[must_use]
    pub fn command_mru(&self) -> CommandMru {
        self.command_mru.clone()
    }

    /// The captive Neovim's LIVE working directory, or `None` until nvim first
    /// reports it.
    ///
    /// A cheap, synchronous main-thread read of the SAME `tracked_cwd` cell
    /// [`mru_recent`](Self::mru_recent) reads (a clone-out of the `RwLock` guard,
    /// no `getcwd()` RPC). The value is ALREADY canonicalized (the redraw handler
    /// canonicalizes it in `apply_cwd` before storing), so callers get a clean
    /// canonical path with no further work. Tracks `:cd`/`:lcd`/`:tcd` via the
    /// `wolf_cwd`/`DirChanged` notification; `None` until nvim first reports.
    #[must_use]
    pub fn live_cwd(&self) -> Option<PathBuf> {
        self.tracked_cwd.read().clone()
    }

    /// Computes the per-pane statusbar strings for the window drawing into `grid`.
    ///
    /// Joins `grid` → [`WinInfo::win`](crate::render::WinInfo) → the per-window
    /// buffer path in [`Snapshot::buffers`](crate::render::Snapshot::buffers), then
    /// formats it via the pure [`crate::pane_status`] over the live cwd
    /// ([`live_cwd`](Self::live_cwd)) and the user's home directory. A window with
    /// no matching entry (an unnamed buffer, or a grid with no window) yields an
    /// empty buffer path, which the pure function maps to
    /// [`NO_NAME`](crate::pane_status::NO_NAME) with an empty display path.
    ///
    /// The snapshot read guard is DROPPED before reading `live_cwd` (which takes
    /// its own lock), mirroring [`mru_recent`](Self::mru_recent). The cwd is
    /// already canonical; the home directory is canonicalized here (falling back to
    /// the raw path) so `strip_prefix` lines up with the canonical buffer path.
    #[must_use]
    pub fn pane_status(&self, grid: u64) -> crate::pane_status::PaneStatus {
        let buf: String = {
            let snap = self.read_snapshot();
            snap.windows
                .get(&grid)
                .and_then(|w| snap.buffers.get(&w.win))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
            // Read guard drops here, before `live_cwd()` takes its own lock.
        };

        let cwd = self.live_cwd();
        crate::pane_status::pane_status(&buf, cwd.as_deref(), canonical_home())
    }

    /// Formats `abs_path` into the cwd/home-relative display path shown on a
    /// pane's secondary statusbar line, reusing the pure
    /// [`crate::pane_status::pane_status`] over the live cwd
    /// ([`live_cwd`](Self::live_cwd)) and the user's home directory. This is the
    /// display path only (the [`PaneStatus::display_path`](crate::pane_status::PaneStatus)
    /// field); the filename is derived separately by the caller. An empty
    /// `abs_path` yields an empty display path.
    #[must_use]
    pub fn display_path(&self, abs_path: &str) -> String {
        crate::pane_status::pane_status(abs_path, self.live_cwd().as_deref(), canonical_home())
            .display_path
    }

    /// Starts a streaming file search rooted at the captive Neovim's live working
    /// directory, falling back to the configured cwd.
    ///
    /// The traversal root is resolved OUT-OF-BAND via
    /// [`resolve_search_root`](Self::resolve_search_root) — a cheap synchronous read
    /// of the tracked [`live_cwd`](Self::live_cwd), no nvim RPC — so it follows
    /// wherever Neovim currently is (`:cd`/`:lcd`/`:tcd`) and never blocks on a
    /// wedged nvim. Only before nvim first reports its cwd does it fall back to the
    /// configured `self.cwd`.
    ///
    /// The MRU cache is seeded SYNCHRONOUSLY before returning (so the very first
    /// [`FileSearch::read`] already shows recent files), then the async traversal
    /// is spawned on the core's tokio runtime; it streams discovered files into
    /// the shared state, bumping `version` and calling `wake` after each batch and
    /// once more when `complete`. Dropping (or [`FileSearch::cancel`]-ing) the
    /// returned handle stops the traversal.
    ///
    /// The MRU seed FOLLOWS Neovim's live cwd: it is the global store's rows scoped
    /// to the resolved search root (`self.mru.recent(&root)` — the SAME resolved
    /// search root the traversal uses as the `directory` filter), so the seed's
    /// `display` strings relativize against exactly the directory being listed
    /// (root-relative, never absolute, and never relative to a stale configured
    /// cwd). When nvim is `:cd`'d into a different project, the seed comes from that
    /// directory's rows — per-project MRU isolation. A disabled/unreadable store
    /// yields an empty seed.
    ///
    /// The traversal is BOUNDED so it cannot run away on a large root (e.g. a
    /// fallback walk of the whole `$HOME` tree): [`files::SearchBounds`] caps the
    /// number of streamed entries, imposes a wall-clock budget, limits the walk
    /// depth, and prunes pathological home subtrees (`~/Library`, `~/.Trash`,
    /// `~/.cache`) outright. The three caps are tunable via `WOLF_SEARCH_MAX_ENTRIES`,
    /// `WOLF_SEARCH_BUDGET_MS`, and `WOLF_SEARCH_MAX_DEPTH` (defaults 50_000 / 5s /
    /// 64), read SYNCHRONOUSLY here on the calling thread — once per search.
    #[must_use]
    pub fn start_file_search(&self, wake: WakeFn) -> FileSearch {
        let root = self.resolve_search_root();

        let seed = self.mru.recent(&root);
        tracing::debug!(root = %root.display(), seed = seed.len(), "starting file search");
        let state = Arc::new(RwLock::new(FileSearchState::seeded(seed)));
        let cancel = Arc::new(AtomicBool::new(false));

        // Resolve the traversal caps synchronously on the calling thread (so tests
        // that set the env vars around this call observe them deterministically),
        // then prune the known pathological home subtrees so a fallback $HOME walk
        // never even descends into `~/Library` & friends.
        let mut bounds = files::SearchBounds::from_env();
        if let Some(home) = canonical_home() {
            bounds.skip_dirs.insert(home.join("Library"));
            bounds.skip_dirs.insert(home.join(".Trash"));
            bounds.skip_dirs.insert(home.join(".cache"));
        }

        // Register the session with the fs-event rescan registry BEFORE the
        // traversal spawns, so no `wolf_fs_changed` event can slip between the
        // walk and the registration. The entry shares the session's `state` /
        // `cancel` handles; dropping the returned `FileSearch` sets `cancel`,
        // which is the registry's cue to prune the entry.
        self.search_registry.register(files::SessionEntry::new(
            state.clone(),
            cancel.clone(),
            root.clone(),
            bounds.clone(),
            wake.clone(),
            self.mru.clone(),
        ));

        files::spawn_traversal(
            root,
            state.clone(),
            cancel.clone(),
            wake,
            self.runtime.handle().clone(),
            bounds,
        );

        FileSearch::from_parts(state, cancel)
    }

    /// Starts a streaming, ripgrep-backed CONTENT search under the current search
    /// root, returning a live [`SearchSession`].
    ///
    /// The search root is resolved OUT-OF-BAND via
    /// [`resolve_search_root`](Self::resolve_search_root) (the tracked
    /// [`live_cwd`](Self::live_cwd), no nvim RPC), so it follows `:cd`/`:lcd`/`:tcd`
    /// and never blocks on a wedged nvim — exactly like the file picker.
    ///
    /// The pinned `rg` binary is located via `WOLF_RG_BIN` (set by the front-end
    /// from the bundled binary; the Nix devShell exports the Nixpkgs `ripgrep`).
    /// When it is unset/empty the search degrades GRACEFULLY to an
    /// immediately-complete session carrying an `error` (no results), rather than
    /// panicking. Otherwise the rg subprocess is spawned on the core's tokio
    /// runtime; it streams matches into the shared state, bumping `version` and
    /// calling `wake` after each batch and once more when `complete`. Dropping (or
    /// [`SearchSession::cancel`]-ing) the returned handle kills the rg subprocess.
    #[must_use]
    pub fn start_search(&self, opts: SearchOptions, wake: WakeFn) -> SearchSession {
        let rg_bin = std::env::var("WOLF_RG_BIN")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let Some(rg_bin) = rg_bin else {
            tracing::warn!("WOLF_RG_BIN unset; content search unavailable");
            let handles = search::SearchHandles {
                state: Arc::new(RwLock::new(SearchState::errored(
                    "WOLF_RG_BIN unset".to_string(),
                ))),
                cancel: Arc::new(AtomicBool::new(false)),
                child: Arc::new(Mutex::new(None)),
            };
            wake();
            return SearchSession::from_parts(handles);
        };

        let root = self.resolve_search_root();
        tracing::debug!(root = %root.display(), query = %opts.query, "starting content search");
        let handles = search::SearchHandles {
            state: Arc::new(RwLock::new(SearchState::new())),
            cancel: Arc::new(AtomicBool::new(false)),
            child: Arc::new(Mutex::new(None)),
        };

        search::spawn_search(
            root,
            opts,
            PathBuf::from(rg_bin),
            &handles,
            wake,
            self.runtime.handle().clone(),
        );

        SearchSession::from_parts(handles)
    }

    /// Requests a grid resize. The authoritative size arrives via `grid_resize`.
    /// Enqueued on the same ordered channel as input so it never races ahead of
    /// or behind pending keystrokes.
    pub fn resize(&self, cols: u16, rows: u16) {
        self.send_input(InputEvent::Resize { cols, rows });
    }

    /// Requests a per-grid resize of a single window `grid` (in cells). The
    /// authoritative size arrives via `grid_resize`. Enqueued on the same ordered
    /// channel as input so it never races ahead of or behind pending keystrokes.
    pub fn resize_grid(&self, grid: u64, cols: u16, rows: u16) {
        self.send_input(InputEvent::ResizeGrid { grid, cols, rows });
    }

    /// Cleanly shuts down the captive Neovim and tears down the runtime.
    ///
    /// Reaps the child even when it is WEDGED (the `shutdown` reap force-kills after
    /// a bounded `child.wait()`) or already dead (a crashed core still returns
    /// `Ok`).
    pub fn shutdown(self) -> Result<(), CoreError> {
        let WolfCore { runtime, nvim, .. } = self;
        let nvim = nvim.into_inner();
        runtime.block_on(async move { nvim.shutdown().await })?;
        Ok(())
    }

    /// The captive nvim's current [`Liveness`] — a cheap, lock-free atomic read
    /// (NOT an RPC).
    ///
    /// `Alive` normally; `NotResponding` after a synchronous RPC elapsed the
    /// timeout budget (a stall — self-clears to `Alive` on the next successful
    /// RPC); `Dead` after the supervisor observed an unexpected process death
    /// (terminal until [`restart_nvim`](Self::restart_nvim)).
    #[must_use]
    pub fn liveness(&self) -> Liveness {
        liveness::get(&self.liveness)
    }

    /// The Tier-1 open-file mirror for tabpage `handle` (its file-backed open
    /// buffers + active buffer), or `None` for an unknown/closed tabpage.
    ///
    /// A cheap, lock-free SNAPSHOT READ with NO nvim RPC. [`Snapshot::open_files`]
    /// is maintained out-of-band by the `wolf_open_files` notification. The
    /// front-end CAPTURES this per view BEFORE a restart (a respawn wipes the
    /// mirror) and reopens the files afterward.
    #[must_use]
    pub fn open_files_for_tabpage(&self, handle: i64) -> Option<TabFiles> {
        self.read_snapshot().open_files.get(&handle).cloned()
    }

    /// Restarts the captive Neovim IN PLACE after a crash or stall: force-kills the
    /// old process, respawns a fresh nvim in the same cwd, re-runs the full setup
    /// chain, and swaps the new process + handle in.
    ///
    /// Returns whether an ACTUAL respawn happened:
    /// - `Ok(true)` — the old process was killed, a fresh nvim was spawned, the
    ///   full setup chain succeeded, and the new process + handle were swapped in.
    ///   Post-`Ok(true)` contract: [`liveness`](Self::liveness) is `Alive`,
    ///   [`current_tabpage`](Self::current_tabpage) is the fresh nvim's tabpage-1,
    ///   and [`list_buffers`](Self::list_buffers) is empty (the respawn starts
    ///   clean). Reopening the Tier-1 files + rebinding views is the FRONT-END's job
    ///   (off the mirror captured before the restart) — this method reopens nothing.
    /// - `Ok(false)` — a NO-OP: either the current liveness is already `Alive` (a
    ///   redundant restart must NOT disturb a live nvim) or a restart is already in
    ///   flight (the `restarting` CAS was set). The process is left untouched.
    /// - `Err(_)` — the respawn or setup chain FAILED. Liveness is marked `Dead`
    ///   (never left stale-`Alive`) so the front-end keeps its crash overlay, the
    ///   `restarting` guard is released, and the error is returned.
    pub fn restart_nvim(&self) -> Result<bool, CoreError> {
        // Reentrancy guard: only one restart proceeds at a time. A concurrent
        // request is a no-op (`Ok(false)`), NOT an actual restart.
        if self
            .restarting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(false);
        }
        // Ensure the guard is cleared on every exit path.
        let result = self.restart_nvim_inner();
        self.restarting.store(false, Ordering::SeqCst);
        result
    }

    /// The body of [`restart_nvim`](Self::restart_nvim), run under the `restarting`
    /// guard. Returns `Ok(true)` on an actual respawn, `Ok(false)` on the Alive
    /// no-op, `Err` on a respawn/setup failure (with liveness marked `Dead`).
    fn restart_nvim_inner(&self) -> Result<bool, CoreError> {
        // A redundant restart on a HEALTHY (Alive) core is a safe no-op: there is
        // nothing to recover, and tearing down a live nvim would needlessly drop
        // its open buffers. The real crash/stall flow calls restart ONLY when
        // liveness is Dead (auto) or NotResponding (the manual button), so this
        // skip never blocks a genuine recovery.
        if liveness::get(&self.liveness) == Liveness::Alive {
            return Ok(false);
        }

        // Force-kill + reap the OLD process (works crashed-or-wedged). Its
        // `shutting_down` is set first, so the old supervisor will NOT re-fire Dead.
        // The `RefMut` is bound to a LOCAL so the guard lives in this SYNC frame,
        // never held across an `.await` inside an async block (satisfies
        // `clippy::await_holding_refcell_ref`).
        {
            let mut old = self.nvim.borrow_mut();
            self.runtime.block_on(old.kill_in_place());
        }

        // Leave the terminal Dead state for the restart (Dead -> Alive), waking the
        // front-end. This is the ONLY exit from Dead (unconditional store-Alive).
        liveness::reset_alive(&self.liveness, &self.wake);

        // Respawn on the runtime, reusing the config verbatim (same XDG state_root,
        // same durable swap_dir, same cwd), then re-run ui_attach + the full setup
        // chain. The grid uses the configured initial size; the next front-end
        // resize re-establishes the live size.
        let cols = self.nvim_cfg.initial_cols;
        let rows = self.nvim_cfg.initial_rows;
        let handler = self.handler.clone();
        let liveness_arc = self.liveness.clone();
        let wake = self.wake.clone();
        let spawned = self.runtime.block_on(async {
            let proc = nvim::spawn(&self.nvim_cfg, handler, liveness_arc, wake).await?;
            proc.ui_attach(cols, rows).await?;
            proc.setup_current_file_tracking().await?;
            proc.setup_buffer_tracking().await?;
            proc.setup_full_width().await?;
            proc.setup_layout().await?;
            proc.setup_dir_tracking().await?;
            proc.setup_theme_tracking().await?;
            proc.setup_commands().await?;
            proc.setup_tabs().await?;
            proc.setup_open_files().await?;
            proc.setup_fs_events().await?;
            proc.setup_view_tracking().await?;
            proc.setup_preview().await?;
            Ok::<NvimProcess, NvimError>(proc)
        });

        let new_proc = match spawned {
            Ok(proc) => proc,
            Err(e) => {
                // A failed respawn must NOT leave liveness stale-`Alive` (the
                // `reset_alive` above optimistically cleared it): mark it `Dead` so
                // the front-end keeps its crash overlay (and its manual-retry
                // button). The old process is already reaped; the stale `self.nvim`
                // handle is inert (its io loop is gone).
                liveness::mark_dead(&self.liveness, &self.wake);
                return Err(CoreError::Nvim(e));
            }
        };

        // Swap in the fresh process + its handle (so the input pump picks it up).
        let new_handle = new_proc.handle();
        *self.nvim.borrow_mut() = new_proc;
        *self.shared_handle.write() = new_handle;
        Ok(true)
    }

    /// Resolves the file-search traversal root OUT-OF-BAND: the captive Neovim's
    /// live working directory via [`live_cwd`](Self::live_cwd) (the `tracked_cwd`
    /// cell, fed by the `wolf_cwd`/`DirChanged` notification), falling back to the
    /// configured `self.cwd`.
    ///
    /// No nvim RPC is issued on this path. [`live_cwd`](Self::live_cwd) is a cheap
    /// synchronous read of the tracked-cwd cell, ALREADY canonicalized (the redraw
    /// handler canonicalizes it in `apply_cwd` before storing), so it is returned
    /// as-is with no further work. It follows `:cd`/`:lcd`/`:tcd`, and uses the
    /// last-known value if a directory change is still mid-flight (the notification
    /// has not landed yet). Only before nvim's first `wolf_cwd` emit — when
    /// [`live_cwd`](Self::live_cwd) is still `None` — does it fall back to
    /// canonicalizing the configured `self.cwd` (keeping the raw path if
    /// canonicalization fails). Because it never blocks on nvim, the picker is
    /// wedge-independent: it opens instantly from the last-known cwd even while a
    /// window's nvim is wedged.
    fn resolve_search_root(&self) -> PathBuf {
        if let Some(cwd) = self.live_cwd() {
            tracing::debug!(
                root = %cwd.display(),
                source = "live_cwd",
                "resolved file-search root"
            );
            return cwd;
        }
        let fallback = std::fs::canonicalize(&self.cwd).unwrap_or_else(|_| self.cwd.clone());
        tracing::debug!(
            root = %fallback.display(),
            source = "configured_cwd",
            "resolved file-search root"
        );
        fallback
    }

    /// Runs an interactive synchronous nvim RPC under the core's configured
    /// [`rpc_timeout`](Self::rpc_timeout) budget, delegating to the module-level
    /// [`timeout_block_on`].
    ///
    /// The central chokepoint for every interactive `block_on` on `self.nvim`:
    /// on elapse it logs a `warn!` and returns [`NvimError::Timeout`], which each
    /// call site already degrades to its documented fallback (`""`, `0`, `None`,
    /// an empty `Vec`, …) — so a wedged nvim yields a bounded stall instead of an
    /// unbounded main-thread hang. Deliberately NOT used for startup or shutdown
    /// (those have their own bounds).
    ///
    /// Liveness is folded in here (the stall detector): a [`NvimError::Timeout`]
    /// escalates the shared liveness to `NotResponding` (via
    /// [`liveness::mark_not_responding`], `Alive -> NotResponding` only — never
    /// downgrading `Dead`); a SUCCESSFUL result auto-clears a transient stall
    /// (`compare_exchange(NotResponding -> Alive)` ONLY — it must NEVER touch
    /// `Dead`, so a crashed core stays `Dead` until [`restart_nvim`](Self::restart_nvim)).
    fn block_on_rpc<F, T>(&self, what: &'static str, fut: F) -> Result<T, NvimError>
    where
        F: std::future::Future<Output = Result<T, NvimError>> + Send + 'static,
        T: Send + 'static,
    {
        let result = timeout_block_on(&self.runtime, self.rpc_timeout, what, fut);
        match &result {
            Ok(_) => {
                // RPC-success auto-clear: NotResponding -> Alive ONLY (never Dead).
                // Fire the wake on an actual change so the overlay self-dismisses.
                if self
                    .liveness
                    .compare_exchange(
                        Liveness::NotResponding.to_u8(),
                        Liveness::Alive.to_u8(),
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    (self.wake)();
                }
            }
            Err(NvimError::Timeout(_, _)) => {
                liveness::mark_not_responding(&self.liveness, &self.wake);
            }
            Err(_) => {}
        }
        result
    }
}

/// Runs `fut` to completion on `runtime`, bounded by `timeout`.
///
/// Extracted as a free fn (rather than only a `&self` method) so the timeout
/// mechanism is unit-testable WITHOUT spawning a live nvim: a never-resolving
/// future under a short budget must map to [`NvimError::Timeout`] promptly. On
/// elapse it logs a `warn!` naming the wedged RPC and returns
/// `NvimError::Timeout(what, timeout)`; otherwise the inner `Result` (whether
/// `Ok` or a real RPC `Err`) passes through verbatim.
///
/// `fut` is SPAWNED as a runtime task (hence the `Send + 'static` bounds) rather
/// than driven inline, and on a timeout the task's [`JoinHandle`] is DROPPED —
/// which DETACHES the task, letting it run to completion in the background. This
/// is load-bearing for stall recovery: an nvim-rs call whose response channel
/// receiver is dropped mid-flight makes the io loop fail with "Could not send
/// response" and DIE (which the supervisor would misread as a crash). By detaching
/// (not aborting/dropping the future), the receiver stays alive until nvim
/// eventually answers, so a WEDGED-then-recovered nvim's io loop survives and the
/// next call succeeds (auto-clearing the transient `NotResponding`).
fn timeout_block_on<F, T>(
    runtime: &Runtime,
    timeout: Duration,
    what: &'static str,
    fut: F,
) -> Result<T, NvimError>
where
    F: std::future::Future<Output = Result<T, NvimError>> + Send + 'static,
    T: Send + 'static,
{
    let start = std::time::Instant::now();
    // Spawn the RPC as a task, then wait on its JoinHandle bounded by `timeout`.
    // The `tokio::time::timeout` future is CONSTRUCTED inside the runtime context
    // (its timer registers with the reactor on creation).
    let task = runtime.spawn(fut);
    match runtime.block_on(async move { tokio::time::timeout(timeout, task).await }) {
        Ok(Ok(inner)) => {
            let elapsed = start.elapsed();
            tracing::debug!(rpc = what, ?elapsed, "nvim RPC completed");
            inner
        }
        Ok(Err(join_err)) => {
            // The spawned RPC task panicked (should not happen); surface it as an
            // RPC error rather than a timeout.
            Err(NvimError::Rpc(format!(
                "nvim RPC '{what}' task failed: {join_err}"
            )))
        }
        Err(_elapsed) => {
            let elapsed = start.elapsed();
            tracing::warn!(
                rpc = what,
                ?elapsed,
                "nvim RPC '{what}' timed out after {timeout:?}"
            );
            // Dropping `timeout(..)` here drops the inner `JoinHandle`, which
            // DETACHES the task (it keeps running to completion in the background)
            // so the pending response is delivered to a LIVE receiver and the io
            // loop is not killed.
            Err(NvimError::Timeout(what, timeout))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the RPC-timeout chokepoint mechanism.
    //!
    //! These exercise the private free fn `timeout_block_on` DIRECTLY, with NO
    //! live nvim: the point of extracting the mechanism into a free fn (rather than
    //! only a `&self` method on `WolfCore`) is that its timeout behaviour can be
    //! pinned deterministically and fast, without spawning Neovim. The end-to-end
    //! behaviour against a real, wedged nvim is covered by
    //! `tests/rpc_timeout.rs`.
    //!
    //! NOTE to coder: these are written to the signature the plan specifies for the
    //! not-yet-existing items. `timeout_block_on` is expected to be:
    //!
    //! ```ignore
    //! fn timeout_block_on<F, T>(
    //!     runtime: &tokio::runtime::Runtime,
    //!     timeout: std::time::Duration,
    //!     what: &'static str,
    //!     fut: F,
    //! ) -> Result<T, NvimError>
    //! where
    //!     F: std::future::Future<Output = Result<T, NvimError>>;
    //! ```
    //!
    //! and `NvimError::Timeout(&'static str, Duration)` a new variant. If the coder
    //! diverges from these, align the source (not the test) with the plan.

    use std::future;
    use std::time::{Duration, Instant};

    use tokio::runtime::Builder;

    use super::{timeout_block_on, NvimError};

    /// A short timeout for the never-resolving case: long enough to be a genuine
    /// bound, short enough to keep the test fast.
    const SHORT_TIMEOUT: Duration = Duration::from_millis(50);

    /// A generous timeout for the pass-through cases, where the future is already
    /// resolved so the timeout must NEVER fire.
    const GENEROUS_TIMEOUT: Duration = Duration::from_secs(30);

    /// Upper bound on how long `timeout_block_on` may take to return on a future
    /// that never resolves: comfortably above the 50ms timeout plus scheduling
    /// slack, and far below a real unbounded hang. Proves the call is bounded.
    const RETURN_BOUND: Duration = Duration::from_secs(2);

    /// The value carried by the ready-Ok pass-through future.
    const READY_VALUE: i32 = 7;

    /// Builds the current-thread runtime the unit tests drive `timeout_block_on`
    /// on. `enable_all()` is required so `tokio::time::timeout`'s timer is armed.
    fn rt() -> tokio::runtime::Runtime {
        Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build current-thread runtime")
    }

    /// A future that never resolves must elapse the timeout and map to
    /// `NvimError::Timeout(what, timeout)` — returning PROMPTLY (bounded), never
    /// parking the caller indefinitely.
    #[test]
    fn timeout_block_on_times_out_on_never_resolving_future() {
        let rt = rt();

        let start = Instant::now();
        let result: Result<(), NvimError> = timeout_block_on(
            &rt,
            SHORT_TIMEOUT,
            "wedged",
            future::pending::<Result<(), NvimError>>(),
        );
        let elapsed = start.elapsed();

        match result {
            Err(NvimError::Timeout(what, dur)) => {
                assert_eq!(what, "wedged", "the timeout error must carry the RPC label");
                assert_eq!(
                    dur, SHORT_TIMEOUT,
                    "the timeout error must carry the configured duration"
                );
            }
            other => panic!("expected Err(NvimError::Timeout(..)), got {other:?}"),
        }

        assert!(
            elapsed < RETURN_BOUND,
            "timeout_block_on must return promptly (bounded); took {elapsed:?}"
        );
    }

    /// An already-ready `Ok` future passes straight through under a generous
    /// timeout: the timeout is a backstop, not a delay.
    #[test]
    fn timeout_block_on_passes_through_ready_ok() {
        let rt = rt();

        let result: Result<i32, NvimError> = timeout_block_on(
            &rt,
            GENEROUS_TIMEOUT,
            "ready",
            future::ready(Ok(READY_VALUE)),
        );

        assert_eq!(
            result.expect("a ready Ok future must pass through as Ok"),
            READY_VALUE
        );
    }

    /// An inner `Err` is returned VERBATIM (not remapped to `Timeout`): the
    /// chokepoint only converts an ELAPSED timeout, never a real RPC failure.
    #[test]
    fn timeout_block_on_passes_through_inner_err() {
        let rt = rt();

        let result: Result<(), NvimError> = timeout_block_on(
            &rt,
            GENEROUS_TIMEOUT,
            "boom",
            future::ready(Err(NvimError::Rpc("boom".into()))),
        );

        match result {
            Err(NvimError::Rpc(msg)) => {
                assert_eq!(
                    msg, "boom",
                    "the inner Rpc error must pass through verbatim"
                );
            }
            other => panic!("expected the inner Err(NvimError::Rpc(\"boom\")), got {other:?}"),
        }
    }
}
