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

use crate::browser_preview::{self, BrowserPreview};
use crate::buffer_picker::BufInfo;
use crate::command::Command;
use crate::command_mru::CommandMru;
use crate::files::{self, FileSearch, FileSearchState};
use crate::input::{self, InputEvent, MouseEvent, Nvim, ScrollEvent};
use crate::layout::LayoutIntent;
use crate::mru::{FileEntry, MruError, MruStore};
use crate::nvim::{self, NvimConfig, NvimError, NvimProcess};
use crate::recent_dirs::RecentDirs;
use crate::redraw::{CommandQueue, IntentQueue, RedrawHandler, TrackedCwd, WakeFn};
use crate::render::{OwnedSnapshotReadGuard, SharedSnapshot, Snapshot};
use crate::state::state_dir;

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
    /// Working directory: the FALLBACK search root + the MRU cache location
    /// (`<cwd>/.wolf/mru.sqlite`). The captive Neovim is also launched here. The
    /// picker's LIVE search root is the out-of-band tracked `live_cwd()`; this
    /// configured cwd is used only before nvim first reports its cwd.
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
    /// and the MRU cache location. The live search root is the tracked
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

        // Per-cwd MRU stores are opened lazily (graceful) instead of one eager
        // open at cfg.cwd; `tracked_cwd` follows nvim's live cwd out-of-band.
        let mru_cache: MruCache = Arc::new(Mutex::new(HashMap::new()));
        let tracked_cwd: TrackedCwd = Arc::new(RwLock::new(None));

        let snapshot = SharedSnapshot::new(cfg.cols as usize, cfg.rows as usize);
        let intents: IntentQueue = Arc::new(Mutex::new(VecDeque::new()));
        let commands: CommandQueue = Arc::new(Mutex::new(VecDeque::new()));

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

        let handler = RedrawHandler::new(
            snapshot.clone_handle(),
            intents.clone(),
            commands.clone(),
            tracked_cwd.clone(),
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
            proc.setup_full_width().await?;
            proc.setup_layout().await?;
            proc.setup_dir_tracking().await?;
            proc.setup_theme_tracking().await?;
            proc.setup_commands().await?;
            proc.setup_tabs().await?;
            proc.setup_view_tracking().await?;
            proc.setup_preview().await?;
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
        let pump_snapshot = snapshot.clone_handle();
        runtime.spawn(async move {
            while let Some(first) = input_rx.recv().await {
                // Let any siblings enqueued in the same synchronous burst arrive.
                tokio::time::sleep(INPUT_COALESCE_DELAY).await;

                let mut batch = vec![first];
                while let Ok(ev) = input_rx.try_recv() {
                    batch.push(ev);
                }

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
            recent_dirs,
            command_mru,
            cwd,
            browser_preview: Mutex::new(None),
            rpc_timeout,
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
        match self.block_on_rpc("yank_visual_selection", self.nvim.yank_visual_selection()) {
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
        match self.block_on_rpc("list_buffers", self.nvim.list_buffers()) {
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
        match self.block_on_rpc("render_preview", self.nvim.render_preview(source_bufnr)) {
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
        // browser_preview lock, so the lock is never held across a block_on.
        let bufnr =
            match self.block_on_rpc("current_markdown_bufnr", self.nvim.current_markdown_bufnr()) {
                Ok(bufnr) => bufnr,
                Err(e) => {
                    tracing::warn!("open_in_browser: current_markdown_bufnr failed: {e}");
                    return String::new();
                }
            };
        if bufnr <= 0 {
            return String::new();
        }

        let pinned_raw = self
            .block_on_rpc("buffer_path", self.nvim.buffer_path(bufnr))
            .unwrap_or_default();
        let cwd = self
            .block_on_rpc("current_dir", self.nvim.current_dir())
            .ok();
        let target = browser_preview::build_target(bufnr, cwd, &pinned_raw);

        let mut slot = self.browser_preview.lock();
        let port = if let Some(server) = slot.as_ref() {
            server.set_target(target.clone());
            server.port()
        } else {
            match browser_preview::spawn(
                &self.runtime,
                self.nvim.handle(),
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

    /// Opens (or reuses) the Markdown preview pane for the active source buffer and
    /// returns the preview scratch buffer's number (`0` on error or when the active
    /// buffer is not Markdown).
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to run the `wolf_open_preview` helper (see
    /// [`crate::nvim::NvimProcess::open_preview`]).
    /// `mode` is chosen by the front-end via `PreviewSeam.placementMode`:
    /// `"split_right"` (single pane → new pane on the right), `"active"` (multi
    /// pane → replace the active pane's buffer), or `"reuse_preview_pane"` (exactly
    /// two panes with the OTHER pane already a preview → bind into that OTHER preview
    /// window, leaving the active editor untouched; falls back to `"active"` when
    /// none). The
    /// front-end learns the new pane + the source↔preview mapping on the next wake
    /// (via the `wolf_preview` notification), so no synchronous window handle is
    /// needed. An RPC error (including a [`NvimError::Timeout`] on a wedged nvim) is
    /// logged and returns `0`.
    #[must_use]
    pub fn open_preview(&self, mode: &str) -> i64 {
        match self.block_on_rpc("open_preview", self.nvim.open_preview(mode)) {
            Ok(bufnr) => bufnr,
            Err(e) => {
                tracing::warn!("open_preview({mode}) failed: {e}");
                0
            }
        }
    }

    /// Navigates from the active PREVIEW pane back to its source file and returns
    /// the source buffer's number (`0` on error or when the active buffer is not a
    /// preview pane).
    ///
    /// Blocks briefly on the captive nvim via
    /// [`block_on_rpc`](Self::block_on_rpc) (bounded by
    /// [`rpc_timeout`](Self::rpc_timeout) — [`RPC_TIMEOUT_DEFAULT`],
    /// `WOLF_RPC_TIMEOUT_MS`) to run the `wolf_edit_source` helper (see
    /// [`crate::nvim::NvimProcess::edit_source`]).
    /// `mode` is chosen by the front-end via `EditSeam.placementMode`:
    /// `"split_left"` (single pane → new editor pane on the LEFT, preview stays
    /// right), `"active"` (replace the active preview's buffer with the source), or
    /// `"reuse_editor_pane"` (exactly two panes with the OTHER pane an editor → bind
    /// the source into that OTHER editor window and focus it; falls back to
    /// `"active"` when none). The front-end learns the new pane + updated snapshot on
    /// the next wake (via the `wolf_preview` / `wolf_current_file` notifications), so
    /// no synchronous window handle is needed. An RPC error (including a
    /// [`NvimError::Timeout`] on a wedged nvim) is logged and returns `0`.
    #[must_use]
    pub fn edit_source(&self, mode: &str) -> i64 {
        match self.block_on_rpc("edit_source", self.nvim.edit_source(mode)) {
            Ok(bufnr) => bufnr,
            Err(e) => {
                tracing::warn!("edit_source({mode}) failed: {e}");
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
        match self.block_on_rpc("preview_file", self.nvim.preview_file(win, path)) {
            Ok(bufnr) => bufnr,
            Err(e) => {
                tracing::warn!("preview_file({win}, {path}) failed: {e}");
                0
            }
        }
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
        match self.block_on_rpc("create_tabpage", self.nvim.create_tabpage(&cwd)) {
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
        match self.block_on_rpc("rebuild_layout", self.nvim.rebuild_layout(spec_json)) {
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
        match self.block_on_rpc("current_tabpage", self.nvim.current_tabpage()) {
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
    /// The MRU seed FOLLOWS Neovim's live cwd: it is taken from the per-cwd store
    /// at `<resolved-root>/.wolf` (the SAME resolved search root the traversal uses,
    /// via [`mru_for`]), so the seed's `display` strings relativize against exactly
    /// the directory being listed (root-relative, never absolute, and never relative
    /// to a stale configured cwd). When nvim is `:cd`'d into a different project, the
    /// seed comes from that project's store — per-project MRU isolation. A failed
    /// store open is graceful (empty seed).
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

        let seed = mru_for(&self.mru_cache, &root)
            .map(|s| s.recent())
            .unwrap_or_default();
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
        let WolfCore { runtime, nvim, .. } = self;
        runtime.block_on(async move { nvim.shutdown().await })?;
        Ok(())
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
    fn block_on_rpc<F, T>(&self, what: &'static str, fut: F) -> Result<T, NvimError>
    where
        F: std::future::Future<Output = Result<T, NvimError>>,
    {
        timeout_block_on(&self.runtime, self.rpc_timeout, what, fut)
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
fn timeout_block_on<F, T>(
    runtime: &Runtime,
    timeout: Duration,
    what: &'static str,
    fut: F,
) -> Result<T, NvimError>
where
    F: std::future::Future<Output = Result<T, NvimError>>,
{
    // The `tokio::time::timeout` future must be CONSTRUCTED inside the runtime
    // context (its timer registers with the reactor on creation), so build it in
    // the async block rather than passing it as an already-created argument to
    // `block_on` (which would evaluate it outside any runtime → "no reactor").
    let start = std::time::Instant::now();
    match runtime.block_on(async move { tokio::time::timeout(timeout, fut).await }) {
        Ok(inner) => {
            let elapsed = start.elapsed();
            tracing::debug!(rpc = what, ?elapsed, "nvim RPC completed");
            inner
        }
        Err(_elapsed) => {
            let elapsed = start.elapsed();
            tracing::warn!(
                rpc = what,
                ?elapsed,
                "nvim RPC '{what}' timed out after {timeout:?}"
            );
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
