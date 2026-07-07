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

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::command::Command;
use crate::command_mru::CommandMru;
use crate::files::{self, FileSearch, FileSearchState};
use crate::input::{self, InputEvent, MouseEvent, Nvim, ScrollEvent};
use crate::layout::LayoutIntent;
use crate::mru::{FileEntry, MruError, MruStore};
use crate::nvim::{self, NvimConfig, NvimError, NvimProcess};
use crate::recent_dirs::RecentDirs;
use crate::redraw::{
    CommandQueue, IntentQueue, RedrawHandler, SharedStateStore, TrackedCwd, WakeFn,
};
use crate::render::{OwnedSnapshotReadGuard, SharedSnapshot, Snapshot};
use crate::state::{state_dir, StateStore};

/// A lazily-populated, per-cwd cache of [`MruStore`]s, keyed by CANONICAL cwd.
///
/// Replaces the single startup `MruStore`: both the picker seed (read) and the
/// input pump's record (write) resolve the store for nvim's LIVE cwd through
/// [`mru_for`], so the MRU follows `:cd`/`:lcd`/`:tcd`. A `parking_lot::Mutex`
/// guards the map; its critical section spans only the synchronous
/// [`MruStore::open`] and is never held across an `.await`.
type MruCache = Arc<Mutex<HashMap<PathBuf, MruStore>>>;

/// Resolves the [`MruStore`] for `cwd`, opening and caching it on first use.
///
/// The key is canonicalized with the SAME expression
/// [`resolve_search_root`](WolfCore::resolve_search_root) uses (canonicalize,
/// falling back to the raw path), so the seed store and the traversal root line up
/// and the same canonical cwd maps to a single cached store. On a cache hit a cheap
/// `Arc`-backed clone is returned; on a miss the store is opened at
/// `<cwd>/.wolf/mru.sqlite`, inserted, and cloned. An open failure is logged via
/// `tracing::warn!` and returns `None` (GRACEFUL — a cache hiccup never breaks the
/// picker seed or the input pump). The brief map `Mutex` is held across the
/// synchronous open but NEVER across an `.await`.
fn mru_for(cache: &MruCache, cwd: &Path) -> Option<MruStore> {
    let key = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let mut map = cache.lock();
    if let Some(store) = map.get(&key) {
        return Some(store.clone());
    }
    match MruStore::open(&key) {
        Ok(store) => {
            map.insert(key, store.clone());
            Some(store)
        }
        Err(e) => {
            tracing::warn!("failed to open MRU store at {}: {e}", key.display());
            None
        }
    }
}

/// How long the input pump waits after the first event of a burst for siblings
/// to enqueue, so a synchronous run of `send_input` calls coalesces into one
/// applied batch (one RPC for fused keys, hence one redraw flush).
const INPUT_COALESCE_DELAY: Duration = Duration::from_millis(8);

/// Configuration to start a [`WolfCore`].
#[derive(Clone, Debug)]
pub struct CoreConfig {
    /// Path to the in-repo `runtime/nvim/init.lua`.
    pub init_lua: PathBuf,
    /// Initial grid width in columns.
    pub cols: u16,
    /// Initial grid height in rows.
    pub rows: u16,
    /// Working directory: the FALLBACK search root + the MRU cache location
    /// (`<cwd>/.wolf/mru.sqlite`). The captive Neovim is also launched here. The
    /// picker's LIVE search root is the captive Neovim's `getcwd()`; this is used
    /// only when that query fails or returns an unusable path.
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
    /// An error from the MRU cache layer.
    ///
    /// Retained as part of the public surface, but `start` no longer produces it:
    /// the MRU store is now opened lazily per-cwd via [`mru_for`], which is
    /// graceful (logs + returns `None`) rather than fatal, so a cache hiccup never
    /// fails startup or the picker.
    #[error(transparent)]
    Mru(#[from] MruError),
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
    nvim: NvimProcess,
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
    /// The per-cwd MRU store cache. Both the picker seed (read) and the input
    /// pump's record (write) resolve the store for nvim's LIVE cwd via [`mru_for`],
    /// so the MRU follows `:cd`/`:lcd`/`:tcd`. Cloned (cheaply, it is `Arc`-backed)
    /// into the input pump so opens record into the right per-cwd store.
    mru_cache: MruCache,
    /// The captive Neovim's live working directory, tracked out-of-band via the
    /// `wolf_cwd` notification (the `WolfDir` `DirChanged` autocmd). Read
    /// synchronously by the input pump to pick the per-cwd MRU store when recording
    /// an opened file (`block_on(getcwd)` is impossible there — the pump runs on a
    /// tokio worker thread). `None` until nvim first reports its cwd; the pump then
    /// falls back to [`cwd`](Self::cwd).
    tracked_cwd: TrackedCwd,
    /// The optionally-attached application-state store, paired with this core's
    /// window index. Empty at start; filled by [`WolfCore::attach_state_store`].
    /// Cloned into the redraw handler, which mirrors each `wolf_cwd`
    /// notification into the store at that window index so cwd changes persist
    /// with no front-end involvement.
    state_store: SharedStateStore,
    /// The always-on GLOBAL recent-directories MRU (`<state_dir>/folder-mru.sqlite`).
    /// Constructed once at [`WolfCore::start`] from [`state_dir`] (or
    /// [`RecentDirs::disabled`] when it cannot be resolved) and cloned into the
    /// redraw handler, which records each `wolf_cwd` directory change into it. Read
    /// back via [`WolfCore::recent_dirs`].
    recent_dirs: RecentDirs,
    /// The always-on GLOBAL command-palette MRU (`<state_dir>/command-mru.sqlite`).
    /// Constructed once at [`WolfCore::start`] from [`state_dir`] (or
    /// [`CommandMru::disabled`] when it cannot be resolved). Unlike
    /// [`recent_dirs`](Self::recent_dirs) it is NOT wired into the redraw handler —
    /// nothing auto-records into it; only an explicit palette activation records via
    /// the handle handed out by [`WolfCore::command_mru`].
    command_mru: CommandMru,
    /// The configured working directory: the file picker's FALLBACK search root
    /// and the MRU cache location. The live search root is the captive Neovim's
    /// `getcwd()` (see [`WolfCore::start_file_search`]); this path is used only
    /// when that query fails or returns an unusable path.
    cwd: PathBuf,
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

        // Per-cwd MRU stores are opened lazily (graceful) instead of one eager
        // open at cfg.cwd; `tracked_cwd` follows nvim's live cwd out-of-band.
        let mru_cache: MruCache = Arc::new(Mutex::new(HashMap::new()));
        let tracked_cwd: TrackedCwd = Arc::new(RwLock::new(None));

        let snapshot = SharedSnapshot::new(cfg.cols as usize, cfg.rows as usize);
        let intents: IntentQueue = Arc::new(Mutex::new(VecDeque::new()));
        let commands: CommandQueue = Arc::new(Mutex::new(VecDeque::new()));
        let state_store: SharedStateStore = Arc::new(RwLock::new(None));

        // The global recent-directories MRU is ALWAYS-ON (not attached/opt-in like
        // the StateStore): construct it once from state_dir(), or a no-op disabled
        // store when that cannot be resolved. Cloned into the redraw handler, which
        // records each cwd change into it.
        let recent_dirs = state_dir()
            .map(|d| RecentDirs::open(&d))
            .unwrap_or_else(RecentDirs::disabled);

        // The global command-palette MRU is ALSO always-on, constructed the same
        // way from state_dir(). Unlike recent_dirs it is NOT cloned into the redraw
        // handler: nothing auto-records into it (only palette activation does).
        let command_mru = state_dir()
            .map(|d| CommandMru::open(&d))
            .unwrap_or_else(CommandMru::disabled);

        let handler = RedrawHandler::new(
            snapshot.clone_handle(),
            intents.clone(),
            commands.clone(),
            tracked_cwd.clone(),
            state_store.clone(),
            recent_dirs.clone(),
            wake,
        );

        let nvim_cfg = NvimConfig::from_env(cfg.init_lua, cwd.clone(), cfg.cols, cfg.rows)
            .map_err(CoreError::Nvim)?;

        let nvim = runtime.block_on(async {
            let proc = nvim::spawn(&nvim_cfg, handler).await?;
            proc.ui_attach(cfg.cols, cfg.rows).await?;
            proc.setup_current_file_tracking().await?;
            proc.setup_buffer_tracking().await?;
            proc.setup_layout().await?;
            proc.setup_dir_tracking().await?;
            proc.setup_theme_tracking().await?;
            proc.setup_commands().await?;
            Ok::<NvimProcess, NvimError>(proc)
        })?;

        // Pre-warm the MRU store for the configured cwd so `<cfg.cwd>/.wolf` exists
        // right after start (preserving the ".wolf-exists-after-start" invariant the
        // tests rely on). Graceful: a failed open is logged, not fatal.
        let _ = mru_for(&mru_cache, &cfg.cwd);

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
        let pump_nvim: Nvim = nvim.handle();
        let pump_mru_cache = mru_cache.clone();
        let pump_tracked_cwd = tracked_cwd.clone();
        let pump_fallback_cwd = cwd.clone();
        runtime.spawn(async move {
            while let Some(first) = input_rx.recv().await {
                // Let any siblings enqueued in the same synchronous burst arrive.
                tokio::time::sleep(INPUT_COALESCE_DELAY).await;

                let mut batch = vec![first];
                while let Ok(ev) = input_rx.try_recv() {
                    batch.push(ev);
                }
                input::apply_batch(&pump_nvim, &batch).await;

                // Record every opened file in the per-cwd MRU store for nvim's LIVE
                // cwd (the tracked cell, falling back to the configured cwd until
                // nvim first reports). Resolving here, not at startup, makes the
                // record path follow `:cd`/`:lcd`/`:tcd`. Reading the cell clones
                // out the path so the brief RwLock guard is not held across the
                // store resolution. Fire-and-forget; a None store is skipped.
                for ev in &batch {
                    if let InputEvent::Open { path } = ev {
                        let record_cwd = pump_tracked_cwd
                            .read()
                            .clone()
                            .unwrap_or_else(|| pump_fallback_cwd.clone());
                        if let Some(store) = mru_for(&pump_mru_cache, &record_cwd) {
                            store.record(path);
                        }
                    }
                }
            }
        });

        Ok(Self {
            runtime,
            nvim,
            snapshot,
            intents,
            commands,
            input_tx,
            mru_cache,
            tracked_cwd,
            state_store,
            recent_dirs,
            command_mru,
            cwd,
        })
    }

    /// Attaches an application-state store bound to `window_index` (this core's
    /// slot in `state.json`'s `windows` list): from now on every genuine
    /// `DirChanged` `wolf_cwd` notification (a real `:cd`/`:lcd`/`:tcd`, including
    /// the Open-Folder chdir) is mirrored into `store` as this window's cwd.
    ///
    /// The passive launch/initial cwd is DELIBERATELY never persisted: the
    /// setup-time `wolf_cwd` emit is flagged and skipped by
    /// [`apply_cwd`](crate::redraw), and no tracked-cwd is pushed here on attach.
    /// So a fresh empty-state window persists `cwd == ""`; a real `:cd` persists
    /// via `apply_cwd`; and a restored window's cwd comes from
    /// [`StateStore::open`]'s disk load — not from this attach.
    pub fn attach_state_store(&self, store: StateStore, window_index: usize) {
        *self.state_store.write() = Some((store, window_index));
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
    /// [`crate::nvim::NvimProcess::yank_visual_selection`]) via `runtime.block_on`
    /// (same pattern as [`resolve_search_root`](Self::resolve_search_root)). An
    /// RPC error, a `None` result (not in visual mode), or an empty selection all
    /// map to `None`, so the front-end only writes real text to the clipboard.
    #[must_use]
    pub fn copy_visual_selection(&self) -> Option<String> {
        match self.runtime.block_on(self.nvim.yank_visual_selection()) {
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
    /// The store is the one for nvim's LIVE cwd (the tracked cell, falling back to
    /// the configured `cwd` until nvim first reports), so the entries' `display`
    /// strings relativize against the same directory the picker is rooted at. A
    /// failed store open is graceful (empty result).
    #[must_use]
    pub fn mru_recent(&self) -> Vec<FileEntry> {
        let cwd = self
            .tracked_cwd
            .read()
            .clone()
            .unwrap_or_else(|| self.cwd.clone());
        mru_for(&self.mru_cache, &cwd)
            .map(|s| s.recent())
            .unwrap_or_default()
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

    /// Starts a streaming file search rooted at the captive Neovim's live working
    /// directory (`getcwd()`), falling back to the configured cwd.
    ///
    /// The traversal root is resolved via [`resolve_search_root`](Self::resolve_search_root):
    /// it follows wherever Neovim currently is (`:cd`/`:lcd`/`:tcd`), and only if
    /// that query fails or returns an unusable path does it fall back to the
    /// configured `self.cwd`.
    ///
    /// The MRU cache is seeded SYNCHRONOUSLY before returning (so the very first
    /// [`FileSearch::read`] already shows recent files), then the async traversal
    /// is spawned on the core's tokio runtime; it streams discovered files into
    /// the shared state, bumping `version` and calling `wake` after each batch and
    /// once more when `complete`. Dropping (or [`FileSearch::cancel`]-ing) the
    /// returned handle stops the traversal.
    ///
    /// The MRU seed FOLLOWS Neovim's live cwd: it is taken from the per-cwd store
    /// at `<resolved-root>/.wolf` (the SAME resolved search root the traversal uses,
    /// via [`mru_for`]), so the seed's `display` strings relativize against exactly
    /// the directory being listed (root-relative, never absolute, and never relative
    /// to a stale configured cwd). When nvim is `:cd`'d into a different project, the
    /// seed comes from that project's store — per-project MRU isolation. A failed
    /// store open is graceful (empty seed).
    #[must_use]
    pub fn start_file_search(&self, wake: WakeFn) -> FileSearch {
        let root = self.resolve_search_root();

        let seed = mru_for(&self.mru_cache, &root)
            .map(|s| s.recent())
            .unwrap_or_default();
        let state = Arc::new(RwLock::new(FileSearchState::seeded(seed)));
        let cancel = Arc::new(AtomicBool::new(false));

        files::spawn_traversal(
            root,
            state.clone(),
            cancel.clone(),
            wake,
            self.runtime.handle().clone(),
        );

        FileSearch::from_parts(state, cancel)
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
    pub fn shutdown(self) -> Result<(), CoreError> {
        let WolfCore {
            runtime, nvim, ..
        } = self;
        runtime.block_on(async move { nvim.shutdown().await })?;
        Ok(())
    }

    /// Resolves the file-search traversal root: the captive Neovim's live working
    /// directory (`getcwd()`), falling back to the configured `self.cwd`.
    ///
    /// A brief blocking `getcwd()` RPC is issued via `runtime.block_on` (the same
    /// pattern `start`/`shutdown` use — one sub-ms round-trip). On success the
    /// returned path is canonicalized (keeping the raw path if canonicalization
    /// fails — a successful `getcwd()` is a real directory) so it lines up with the
    /// canonical paths the MRU stores. On failure it logs and falls back to
    /// canonicalizing `self.cwd`, the configured root.
    fn resolve_search_root(&self) -> PathBuf {
        match self.runtime.block_on(self.nvim.current_dir()) {
            Ok(p) => std::fs::canonicalize(&p).unwrap_or(p),
            Err(e) => {
                tracing::warn!(
                    "getcwd() query failed ({e}); falling back to configured cwd"
                );
                std::fs::canonicalize(&self.cwd).unwrap_or_else(|_| self.cwd.clone())
            }
        }
    }
}
