//! The async, streaming file-list traversal and its [`FileSearch`] session.
//!
//! A [`FileSearch`] is the picker's view of "all the files under the search
//! root". It is created by [`crate::api::WolfCore::start_file_search`], which
//! synchronously seeds the most-recently-used files (so a reader sees them
//! immediately) and then spawns a background walk that streams the rest in.
//!
//! ## Shape (mirrors the render-state snapshot)
//!
//! The collected results live in a [`FileSearchState`] behind a single
//! [`parking_lot::RwLock`]. The traversal task takes the *write* lock only to
//! append a finished batch (and bump [`FileSearchState::version`]), never holding
//! it across IO or the wake callback. Readers take an *owned* read guard via
//! [`FileSearch::read`] (owning an `Arc` clone of the lock), so the guard's
//! lifetime is independent of the [`FileSearch`].
//!
//! ## Traversal rules
//!
//! The walk uses the `ignore` crate (ripgrep's walker) configured to behave like
//! `git ls-files --others --exclude-standard` inside a repo and a plain recursive
//! walk outside one: it honors `.gitignore` (in the cwd or any ancestor up to the
//! repo root), the global git ignore, and git's `info/exclude`, shows dotfiles
//! (only `.gitignore` filters), does not follow symlinks (so symlink loops cannot
//! hang it), and never descends into [`EXCLUDED_DIR_NAMES`] (`.git/` and our own
//! `.wolf/` cache), which content search excludes too. Each entry is classified
//! by [`is_listable_file`]: plain files are kept, a symlink is kept only when its
//! target is a plain file, and directories / broken or directory symlinks are
//! dropped.
//!
//! ## The filesystem watcher
//!
//! [`SearchRegistry`] runs a `notify` watcher on its own `wolf-fs-watch`
//! thread. It watches the root of every live session started with the watcher
//! on, judges each event's paths by the walk's own rules ([`WatchFilter`]), and
//! forwards the survivors to [`SearchRegistry::notify_fs_changed`], so a change
//! made outside Neovim reaches the sessions with no nvim event at all.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::{ArcRwLockReadGuard, Mutex, RawRwLock, RwLock};

use crate::mru::MruStore;
use crate::redraw::WakeFn;

/// Number of kept files accumulated before a batch is appended to the shared
/// state (and a wake fired). Keeps lock hold-times short while still streaming
/// results to the UI promptly.
const TRAVERSAL_BATCH: usize = 256;

/// The directory names every traversal prunes: the picker walk here and the
/// content search's `rg` exclusion globs in `search.rs`, so the two cannot drift.
pub(crate) const EXCLUDED_DIR_NAMES: [&str; 2] = [".git", ".wolf"];

/// Default cap on the number of listable files a single traversal will stream
/// before it stops early (marking the result [`FileSearchState::truncated`]).
/// Generous enough not to hurt real projects; overridable via
/// `WOLF_SEARCH_MAX_ENTRIES`.
const SEARCH_MAX_ENTRIES_DEFAULT: usize = 50_000;

/// Default wall-clock budget for a single traversal. Once elapsed the walk stops
/// early (marking the result [`FileSearchState::truncated`]). Overridable via
/// `WOLF_SEARCH_BUDGET_MS`.
const SEARCH_BUDGET_DEFAULT: Duration = Duration::from_secs(5);

/// Default maximum directory depth for a single traversal (`ignore`'s
/// convention: the root is depth 0, so a file directly in the root is depth 1).
/// Overridable via `WOLF_SEARCH_MAX_DEPTH`. Depth pruning is SILENT (invisible in
/// the `ignore` API), so it does NOT set [`FileSearchState::truncated`].
const SEARCH_MAX_DEPTH_DEFAULT: usize = 64;

/// Default debounce for the fs-event-driven rescan (see [`SearchRegistry`]): a
/// burst of `wolf_fs_changed` notifications inside this window coalesces into
/// one rescan decision. Overridable via `WOLF_RESCAN_DEBOUNCE_MS`.
const RESCAN_DEBOUNCE_DEFAULT: Duration = Duration::from_millis(200);

/// Whether a new search's session is watched by the filesystem watcher:
/// `WOLF_FS_WATCH` trimmed equal to `0` turns it off, and anything else, unset
/// included, leaves it on. Read once per search on the calling thread, like
/// [`SearchBounds::from_env`]. Off, the session sees an external change only
/// through an nvim `wolf_fs_changed` trigger.
pub(crate) fn fs_watch_from_env() -> bool {
    !matches!(std::env::var("WOLF_FS_WATCH"), Ok(v) if v.trim() == "0")
}

/// Poll cadence while a rescan waits for a session's INITIAL traversal to
/// finish (a swap must never race [`append_batch`]). The wait is bounded by the
/// session's [`SearchBounds::budget`], which also bounds the walk itself.
const RESCAN_COMPLETE_POLL: Duration = Duration::from_millis(25);

/// The bounds that keep a single file-search traversal from running away (e.g. a
/// fallback walk of the entire `$HOME` tree): an entry cap, a wall-clock budget,
/// a max depth, and a set of absolute directories to skip outright.
///
/// Resolved once (synchronously, on the calling thread) per search via
/// [`SearchBounds::from_env`] and moved into the traversal closure. `skip_dirs`
/// starts empty; the caller populates it with pathological trees (e.g.
/// `~/Library`, `~/.cache`) that should never be walked.
///
/// `Clone` because a long-lived session's [`SessionEntry`] keeps its own copy
/// (the rescan re-walks under the SAME caps as the initial traversal).
#[derive(Clone)]
pub(crate) struct SearchBounds {
    /// Stop streaming after this many listable files (then set `truncated`).
    pub(crate) max_entries: usize,
    /// Stop the walk once this much wall-clock time has elapsed (then set
    /// `truncated`).
    pub(crate) budget: Duration,
    /// Maximum directory depth (`ignore`'s root=depth-0 convention).
    pub(crate) max_depth: usize,
    /// Absolute directories to prune outright (never descended into).
    pub(crate) skip_dirs: HashSet<PathBuf>,
}

impl SearchBounds {
    /// Reads the caps from the environment, falling back to the defaults on any
    /// unset / unparsable / zero value (the `> 0` guard avoids a zero cap or a
    /// `Duration::ZERO` instant-truncation). Mirrors the `WOLF_RPC_TIMEOUT_MS`
    /// guard pattern in `api.rs`. `skip_dirs` starts empty (caller populates).
    pub(crate) fn from_env() -> Self {
        let max_entries = std::env::var("WOLF_SEARCH_MAX_ENTRIES")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(SEARCH_MAX_ENTRIES_DEFAULT);
        let budget = std::env::var("WOLF_SEARCH_BUDGET_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|&ms| ms > 0)
            .map(Duration::from_millis)
            .unwrap_or(SEARCH_BUDGET_DEFAULT);
        let max_depth = std::env::var("WOLF_SEARCH_MAX_DEPTH")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(SEARCH_MAX_DEPTH_DEFAULT);
        Self {
            max_entries,
            budget,
            max_depth,
            skip_dirs: HashSet::new(),
        }
    }
}

/// A file known to the picker: an absolute `path` (used to open the file), a
/// `display` string (the path relative to the search root, what the UI shows and
/// ranks), and an optional `mru_rank` (`Some(0)` = most recently opened).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Absolute path to the file (used to issue `InputEvent::Open`).
    pub path: PathBuf,
    /// Path relative to the search root (the cwd), what the UI shows and ranks.
    pub display: String,
    /// MRU rank: `Some(0)` is the most recently opened file; `None` if the entry
    /// is not in the MRU cache.
    pub mru_rank: Option<u32>,
}

/// The collected state of a [`FileSearch`]: the entries found so far, whether the
/// traversal has finished, and a monotonic `version` bumped on every change.
///
/// `seen` is the dedup set of absolute paths already emitted (seeded with the MRU
/// paths so a rediscovered MRU file is not listed twice); it is internal.
pub struct FileSearchState {
    /// All entries collected so far (MRU seed first, then streamed traversal
    /// results in discovery order).
    pub entries: Vec<FileEntry>,
    /// `true` once the background traversal has finished (or will never run).
    pub complete: bool,
    /// `true` when the walk stopped early at the entry cap or the time budget —
    /// the result is complete but not exhaustive (more files exist under the root
    /// than were streamed). Depth pruning does NOT set this (it is invisible in
    /// the `ignore` API), so it reflects the entry-cap + time-budget caps only.
    pub truncated: bool,
    /// Monotonic version, bumped on each append so readers can detect changes.
    pub version: u64,
    /// Absolute paths already emitted, for dedup across the MRU seed + traversal.
    seen: HashSet<PathBuf>,
}

impl FileSearchState {
    /// Builds the initial state from the synchronous MRU `seed`, deriving the
    /// dedup set from the seeded entries' paths. Starts at `version = 1`,
    /// `complete = false`.
    pub(crate) fn seeded(seed: Vec<FileEntry>) -> Self {
        let seen = seed.iter().map(|e| e.path.clone()).collect();
        Self {
            entries: seed,
            complete: false,
            truncated: false,
            version: 1,
            seen,
        }
    }
}

/// An owned read guard over a [`FileSearchState`].
///
/// Owns an `Arc` clone of the lock (so its lifetime is independent of the
/// [`FileSearch`]); `Deref`s to [`FileSearchState`] so callers read
/// `guard.entries`, `guard.complete`, `guard.truncated`, and `guard.version`
/// directly.
pub struct FileSearchReadGuard {
    guard: ArcRwLockReadGuard<RawRwLock, FileSearchState>,
}

impl std::ops::Deref for FileSearchReadGuard {
    type Target = FileSearchState;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// A live file-search session: the shared result state plus a cancel flag.
///
/// Dropping a [`FileSearch`] cancels its background traversal (so closing the
/// picker, or starting a new search, stops the old walk promptly).
pub struct FileSearch {
    state: Arc<RwLock<FileSearchState>>,
    cancel: Arc<AtomicBool>,
}

impl FileSearch {
    /// Assembles a [`FileSearch`] from its shared state and cancel flag (the
    /// traversal having already been spawned against the same handles).
    pub(crate) fn from_parts(state: Arc<RwLock<FileSearchState>>, cancel: Arc<AtomicBool>) -> Self {
        Self { state, cancel }
    }

    /// Reads the current search state (entries / complete / version).
    #[must_use]
    pub fn read(&self) -> FileSearchReadGuard {
        FileSearchReadGuard {
            guard: self.state.read_arc(),
        }
    }

    /// Cancels the background traversal. Idempotent; also fired on `Drop`.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

impl Drop for FileSearch {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// Spawns the streaming traversal of `root` onto the tokio runtime `handle`.
///
/// The `ignore` walk is blocking IO, so it runs on `spawn_blocking`. Found files
/// are appended to `state` in [`TRAVERSAL_BATCH`]-sized chunks (bumping
/// `version` and calling `wake` after the write lock is dropped); `cancel` is
/// checked each iteration. On completion the remainder is appended, `complete`
/// is set (unless cancelled), and a final `wake` fires.
///
/// The walk is BOUNDED by `bounds` (see [`SearchBounds`]) so it cannot run away
/// on a large root (e.g. a fallback walk of the entire `$HOME` tree): it stops
/// early at the entry cap or the wall-clock budget (logging a `warn!` and marking
/// [`FileSearchState::truncated`] — the result stays `complete`), never descends
/// past `max_depth`, and prunes any `skip_dirs` subtree outright. A search
/// truncated by a cap still ends `complete` (only a real `cancel` leaves it
/// incomplete); depth pruning is silent and does NOT set `truncated`.
pub(crate) fn spawn_traversal(
    root: PathBuf,
    state: Arc<RwLock<FileSearchState>>,
    cancel: Arc<AtomicBool>,
    wake: WakeFn,
    handle: tokio::runtime::Handle,
    bounds: SearchBounds,
) {
    handle.spawn_blocking(move || {
        tracing::debug!(root = %root.display(), "spawn_traversal walking");
        // Local dedup set, seeded from the synchronous MRU seed. The traversal is
        // the only producer after start, so a local set avoids locking per entry.
        let mut seen: HashSet<PathBuf> = state.read().seen.clone();
        let mut batch: Vec<FileEntry> = Vec::new();

        let truncated = walk_files(&root, &bounds, &cancel, &mut seen, |entry| {
            batch.push(entry);
            if batch.len() >= TRAVERSAL_BATCH {
                append_batch(&state, &mut batch, false, false);
                wake();
            }
        });

        let complete = !cancel.load(Ordering::SeqCst);
        append_batch(&state, &mut batch, complete, truncated);
        wake();
    });
}

/// The ONE bounded walk of `root` under `bounds` (the module-doc traversal
/// rules), shared by the initial streaming traversal ([`spawn_traversal`]) and
/// the registry rescan so the two can never drift.
///
/// Each accepted, deduped file is handed to `emit` in discovery order. `seen`
/// is the dedup set (pre-seeded by the caller, grown in place); `cancel` is
/// checked each iteration so a dropped session stops the walk promptly. Returns
/// `true` when the walk stopped early at the entry cap or the wall-clock budget
/// (the caller's `truncated`); depth pruning stays silent, per
/// [`SearchBounds`].
fn walk_files(
    root: &Path,
    bounds: &SearchBounds,
    cancel: &AtomicBool,
    seen: &mut HashSet<PathBuf>,
    mut emit: impl FnMut(FileEntry),
) -> bool {
    let started = Instant::now();
    let mut accepted: usize = 0;
    let mut truncated = false;

    // Absolute directories to prune outright, moved into the `filter_entry`
    // closure (kept `Send + Sync` so `ignore` can share it across threads).
    let skip = bounds.skip_dirs.clone();
    let walker = ignore::WalkBuilder::new(root)
        .parents(true)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(false)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(bounds.max_depth))
        .filter_entry(move |entry| {
            let name = entry.file_name();
            !EXCLUDED_DIR_NAMES.iter().any(|ex| name == OsStr::new(ex))
                && !skip.contains(entry.path())
        })
        .build();

    for result in walker {
        if started.elapsed() >= bounds.budget {
            truncated = true;
            tracing::warn!(
                root = %root.display(),
                budget_ms = bounds.budget.as_millis() as u64,
                accepted,
                "file search truncated: time budget exceeded"
            );
            break;
        }
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let Ok(entry) = result else {
            // A per-entry error (e.g. a permission denial) skips just that
            // entry; the walk continues.
            continue;
        };
        let path = entry.path();
        if path == root {
            continue;
        }
        if has_excluded_component(path) {
            continue;
        }
        if !is_listable_file(path) {
            continue;
        }

        let abs = path.to_path_buf();
        if seen.contains(&abs) {
            continue;
        }
        seen.insert(abs.clone());

        emit(FileEntry {
            path: abs,
            display: display_for(root, path),
            mru_rank: None,
        });
        accepted += 1;
        if accepted >= bounds.max_entries {
            truncated = true;
            tracing::warn!(
                root = %root.display(),
                cap = bounds.max_entries,
                "file search truncated: entry cap reached"
            );
            break;
        }
    }

    truncated
}

/// Appends `batch` (draining it) to the shared state, bumps `version`, marks the
/// search finished when `complete`, and marks it `truncated` when the walk
/// stopped early at a cap. The write lock is held only for this short, IO-free
/// critical section and never across a wake.
///
/// The batch's paths are also folded into [`FileSearchState::seen`], so the
/// shared set stays the FULL membership set (MRU seed + every streamed entry).
/// [`SearchRegistry::notify_fs_changed`] reads it to decide whether a
/// `BufWritePost` path is already listed; only the seed would make every `:w`
/// of a walk-discovered file trigger a spurious rescan.
fn append_batch(
    state: &Arc<RwLock<FileSearchState>>,
    batch: &mut Vec<FileEntry>,
    complete: bool,
    truncated: bool,
) {
    let mut guard = state.write();
    for entry in batch.iter() {
        guard.seen.insert(entry.path.clone());
    }
    guard.entries.append(batch);
    guard.version += 1;
    if complete {
        guard.complete = true;
    }
    if truncated {
        guard.truncated = true;
    }
}

/// True if any component of `path` is one of [`EXCLUDED_DIR_NAMES`] (a defensive
/// backstop in addition to the walker's `filter_entry` pruning of those subtrees).
fn has_excluded_component(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(c, Component::Normal(name) if EXCLUDED_DIR_NAMES.iter().any(|ex| name == OsStr::new(ex)))
    })
}

/// Builds the [`FileEntry::display`] for `path`: relative to `root` when under it
/// (using the platform separator), otherwise the full path string as a fallback.
fn display_for(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Classifies a traversal entry: keep plain files, keep a symlink only if its
/// target is a plain file, and drop directories / broken or directory symlinks.
fn is_listable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    let file_type = meta.file_type();
    if file_type.is_file() {
        true
    } else if file_type.is_symlink() {
        // `metadata` follows the link; keep only if it resolves to a plain file.
        std::fs::metadata(path)
            .map(|m| m.is_file())
            .unwrap_or(false)
    } else {
        false
    }
}

/// The pure rescan decision: should a batch of `wolf_fs_changed` paths trigger
/// a re-walk of the session rooted at `root`?
///
/// `force` (a `BufFilePost` rename, a `BufDelete`, a watcher remove or rename,
/// or a `.gitignore` edit) always rescans — the filesystem may have LOST a
/// listed file, which no membership check can see. Otherwise (`BufWritePost`,
/// or a watcher create or modify) a rescan runs only when some path lies under
/// `root` AND is absent from the session's `seen` set, so a `:w` of an
/// already-listed file costs nothing. `paths` and `seen` hold absolute,
/// already-canonicalized paths; a canonicalization mismatch upstream fails open
/// to an extra (harmless) walk.
pub(crate) fn needs_rescan(
    force: bool,
    paths: &[PathBuf],
    root: &Path,
    seen: &HashSet<PathBuf>,
) -> bool {
    if force {
        return true;
    }
    paths
        .iter()
        .any(|p| p.starts_with(root) && !seen.contains(p))
}

/// One live [`FileSearch`] session as the [`SearchRegistry`] sees it: strong
/// handles to the session's shared state and cancel flag, plus everything a
/// rescan needs to re-walk it (root, bounds, wake, MRU store).
///
/// The strong `Arc`s are safe because the session's OWN liveness signal is the
/// `cancel` flag ([`FileSearch::drop`] sets it), which the registry treats as
/// "dead, prune me" on every notify — a dropped session never lingers past the
/// next fs event, and a pruned entry drops the state with it.
pub(crate) struct SessionEntry {
    /// The session's shared result state (the same lock [`FileSearch::read`]
    /// guards).
    state: Arc<RwLock<FileSearchState>>,
    /// The session's cancel flag: set by [`FileSearch::cancel`]/`Drop`. Doubles
    /// as the registry's liveness test AND the mid-walk abort check.
    cancel: Arc<AtomicBool>,
    /// The traversal root the session was started with.
    root: PathBuf,
    /// The caps the initial traversal ran under; a rescan re-walks under the
    /// SAME caps so the swapped list is comparable to the original.
    bounds: SearchBounds,
    /// The session's wake, fired once per swap (after the write lock drops).
    wake: WakeFn,
    /// The global MRU store (cheap `Arc`-backed clone). A rescan re-seeds from
    /// `recent(&root)`, which also prunes rows for since-deleted files.
    mru: MruStore,
    /// `true` while a rescan walk is running for this session (only one runs at
    /// a time).
    rescan_running: AtomicBool,
    /// Set when a rescan is requested; a walk that finishes with this still set
    /// re-runs once, so a request landing mid-walk is never lost.
    rescan_dirty: AtomicBool,
    /// `true` when the filesystem watcher keeps `root` watched for this session
    /// ([`fs_watch_from_env`]). Off, the session still receives every event the
    /// registry is notified of, but its root adds no watch.
    watch: bool,
}

impl SessionEntry {
    /// Assembles a registry entry for a just-started session (the traversal
    /// having been spawned against the same `state`/`cancel` handles). `watch`
    /// asks the registry's watcher to watch `root` while the session lives.
    pub(crate) fn new(
        state: Arc<RwLock<FileSearchState>>,
        cancel: Arc<AtomicBool>,
        root: PathBuf,
        bounds: SearchBounds,
        wake: WakeFn,
        mru: MruStore,
        watch: bool,
    ) -> Self {
        Self {
            state,
            cancel,
            root,
            bounds,
            wake,
            mru,
            rescan_running: AtomicBool::new(false),
            rescan_dirty: AtomicBool::new(false),
            watch,
        }
    }
}

/// Coalesced `wolf_fs_changed` state accumulated while the debounce timer runs.
#[derive(Default)]
struct Pending {
    /// The (canonicalized-where-possible) absolute paths reported so far.
    paths: Vec<PathBuf>,
    /// `true` when any event in the burst was a rename/delete (`force`).
    force: bool,
    /// `true` while a debounce flush task is scheduled (at most one at a time).
    scheduled: bool,
}

/// The registry of live [`FileSearch`] sessions, driving the fs-event rescan.
///
/// Owned by `WolfCore` (one per core) and cloned into the redraw handler, which
/// calls [`notify_fs_changed`](Self::notify_fs_changed) for every
/// `wolf_fs_changed` notification; the registry's own filesystem watcher calls
/// it too, for each event the walk could list. Sessions self-register from
/// `WolfCore::start_file_search`; an entry whose `cancel` flag is set is dead
/// and pruned on every notify. Notifications are DEBOUNCED registry-wide
/// (default 200 ms, `WOLF_RESCAN_DEBOUNCE_MS`), then each surviving session
/// decides via [`needs_rescan`] and re-walks with walk-then-swap: collect into
/// a local `Vec`, then replace `entries`/`seen`/`truncated` and bump `version`
/// under one short write lock, firing the session wake after the lock drops.
pub struct SearchRegistry {
    /// The core's tokio runtime handle: the debounce timer runs on it and each
    /// rescan walk runs on its blocking pool.
    handle: tokio::runtime::Handle,
    /// The coalescing window between the first notify of a burst and the rescan
    /// decision.
    debounce: Duration,
    /// The live sessions. A brief `Mutex`, never held across IO or a wake.
    sessions: Mutex<Vec<Arc<SessionEntry>>>,
    /// The burst being coalesced. A brief `Mutex`, never held across IO.
    pending: Mutex<Pending>,
    /// The watcher thread's inbox; `None` when the thread could not be spawned.
    watch_tx: Option<Sender<WatchMsg>>,
}

impl SearchRegistry {
    /// Creates an empty registry driving rescans on `handle`, with the debounce
    /// window read from `WOLF_RESCAN_DEBOUNCE_MS` (unset / unparsable / zero
    /// falls back to [`RESCAN_DEBOUNCE_DEFAULT`] — the `> 0` guard mirrors
    /// [`SearchBounds::from_env`]).
    ///
    /// Also spawns the `wolf-fs-watch` thread, which watches the root of every
    /// live session registered with `watch` set (`WOLF_FS_WATCH`, read per
    /// search) and exits when the registry drops. The thread holds only a
    /// `Weak`, so it never keeps the registry alive. If it cannot be spawned,
    /// one warning is logged and the nvim `wolf_fs_changed` triggers remain the
    /// only source of rescans.
    pub(crate) fn new(handle: tokio::runtime::Handle) -> Arc<Self> {
        let debounce = std::env::var("WOLF_RESCAN_DEBOUNCE_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|&ms| ms > 0)
            .map(Duration::from_millis)
            .unwrap_or(RESCAN_DEBOUNCE_DEFAULT);
        Arc::new_cyclic(|registry| {
            let (tx, rx) = mpsc::channel();
            let thread_registry = registry.clone();
            let callback_tx = tx.clone();
            let spawned = std::thread::Builder::new()
                .name("wolf-fs-watch".into())
                .spawn(move || watch_thread(thread_registry, rx, callback_tx));
            let watch_tx = match spawned {
                Ok(_) => Some(tx),
                Err(err) => {
                    tracing::warn!(
                        %err,
                        "fs watcher thread failed to spawn; external changes wait for an nvim trigger"
                    );
                    None
                }
            };
            Self {
                handle,
                debounce,
                sessions: Mutex::new(Vec::new()),
                pending: Mutex::new(Pending::default()),
                watch_tx,
            }
        })
    }

    /// Registers a live session, pruning dead ones (cancel flag set) under the
    /// same lock. Register is the only hook that runs on picker-open, so it
    /// must prune here: without fs events, [`Self::notify_fs_changed`] never
    /// fires, and a browse-only session would otherwise accumulate dead
    /// entries — each holding a strong `Arc` to its search state — unboundedly.
    ///
    /// After the lock drops it asks the watcher thread to reconcile its watched
    /// roots with the live entries; the watch itself happens on that thread.
    pub(crate) fn register(&self, entry: SessionEntry) {
        {
            let mut sessions = self.sessions.lock();
            sessions.retain(|s| !s.cancel.load(Ordering::SeqCst));
            sessions.push(Arc::new(entry));
        }
        self.send_watch(WatchMsg::Reconcile);
    }

    /// Folds one `wolf_fs_changed` event into the pending burst and (on the
    /// first event of a burst) schedules the debounce flush.
    ///
    /// `path` is the reported absolute path (canonicalized by the caller where
    /// possible); `force` is `true` for renames/deletes (`BufFilePost` /
    /// `BufDelete`, or a watcher remove or rename) and for a `.gitignore` edit,
    /// which rescan unconditionally. Dead sessions (cancel flag set) are pruned
    /// here, on every notify, so a closed picker/tree stops costing walks after
    /// at most one more event; a prune that removes an entry asks the watcher
    /// to reconcile, which unwatches a root no live session needs. Each mutex
    /// guard is bound to its own short scope; nothing slow runs under either
    /// lock.
    pub(crate) fn notify_fs_changed(self: &Arc<Self>, path: PathBuf, force: bool) {
        let pruned = {
            let mut sessions = self.sessions.lock();
            let before = sessions.len();
            sessions.retain(|s| !s.cancel.load(Ordering::SeqCst));
            sessions.len() != before
        };
        if pruned {
            self.send_watch(WatchMsg::Reconcile);
        }

        let schedule = {
            let mut pending = self.pending.lock();
            pending.paths.push(path);
            pending.force |= force;
            if pending.scheduled {
                false
            } else {
                pending.scheduled = true;
                true
            }
        };

        if schedule {
            let registry = Arc::clone(self);
            let debounce = self.debounce;
            self.handle.spawn(async move {
                tokio::time::sleep(debounce).await;
                registry.flush_pending();
            });
        }
    }

    /// Drains the coalesced burst and requests a rescan of every live session
    /// whose [`needs_rescan`] says the burst could have changed its listing.
    ///
    /// Runs on the tokio runtime after the debounce sleep. The pending state is
    /// taken (and `scheduled` cleared) under one brief lock, so an event
    /// arriving after the take starts a fresh burst with its own flush. The
    /// per-session decision reads the session's `seen` under a read guard bound
    /// to a local, dropped before the rescan is requested.
    fn flush_pending(self: &Arc<Self>) {
        let (paths, force) = {
            let mut pending = self.pending.lock();
            pending.scheduled = false;
            (
                std::mem::take(&mut pending.paths),
                std::mem::replace(&mut pending.force, false),
            )
        };

        let sessions: Vec<Arc<SessionEntry>> = self.sessions.lock().clone();
        for entry in sessions {
            let decision = {
                let guard = entry.state.read();
                needs_rescan(force, &paths, &entry.root, &guard.seen)
                // Read guard drops here, before any rescan work.
            };
            if decision {
                request_rescan(entry, &self.handle);
            }
        }
    }

    /// The roots the watcher should cover: each live entry with `watch` set,
    /// grouped by root, with the bounds merged most permissively (largest
    /// `max_depth`, intersection of `skip_dirs`) so no session on a root misses
    /// an event its own walk would list. Takes only the `sessions` lock.
    fn watched_roots(&self) -> HashMap<PathBuf, SearchBounds> {
        let sessions = self.sessions.lock();
        let mut roots: HashMap<PathBuf, SearchBounds> = HashMap::new();
        for entry in sessions.iter() {
            if !entry.watch || entry.cancel.load(Ordering::SeqCst) {
                continue;
            }
            match roots.get_mut(&entry.root) {
                Some(merged) => {
                    merged.max_depth = merged.max_depth.max(entry.bounds.max_depth);
                    merged
                        .skip_dirs
                        .retain(|d| entry.bounds.skip_dirs.contains(d));
                }
                None => {
                    roots.insert(entry.root.clone(), entry.bounds.clone());
                }
            }
        }
        roots
    }

    /// Hands `msg` to the watcher thread. A send never blocks; it fails only
    /// once the thread has exited, which leaves nothing to tell.
    fn send_watch(&self, msg: WatchMsg) {
        if let Some(tx) = &self.watch_tx {
            let _ = tx.send(msg);
        }
    }
}

impl Drop for SearchRegistry {
    /// Stops the watcher thread. Its channel never disconnects on its own,
    /// because the thread keeps a sender for the watcher's callback.
    fn drop(&mut self) {
        self.send_watch(WatchMsg::Shutdown);
    }
}

/// Requests a rescan of `entry`, spawning the walk on the blocking pool unless
/// one is already running (in which case the running walk re-runs once at the
/// end — that is what `rescan_dirty` records).
///
/// The dirty flag is set BEFORE the running CAS, so a request that loses the
/// race to a finishing walk is still observed: the finisher re-checks dirty
/// after clearing `rescan_running` and loops if it can re-acquire the flag.
fn request_rescan(entry: Arc<SessionEntry>, handle: &tokio::runtime::Handle) {
    entry.rescan_dirty.store(true, Ordering::SeqCst);
    if entry
        .rescan_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    handle.spawn_blocking(move || loop {
        while entry.rescan_dirty.swap(false, Ordering::SeqCst) {
            rescan_session(&entry);
        }
        entry.rescan_running.store(false, Ordering::SeqCst);
        // A request landing between the dirty check and the store spawned nothing, so re-check;
        // whoever wins the CAS on the run flag owns the next iteration.
        if entry.rescan_dirty.load(Ordering::SeqCst)
            && entry
                .rescan_running
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            continue;
        }
        break;
    });
}

/// One rescan of a live session: walk-then-swap.
///
/// Waits (bounded poll, [`RESCAN_COMPLETE_POLL`] cadence, capped by the
/// session's [`SearchBounds::budget`]) for the INITIAL traversal to reach
/// `complete`, so a swap never races [`append_batch`]. Then re-seeds from
/// `MruStore::recent(&root)` (which prunes rows for since-deleted files),
/// re-walks into a LOCAL vec via [`walk_files`] (no lock held across IO), and
/// — unless the result equals the current listing (a no-op, e.g. `:bdelete` of
/// a still-existing file) — replaces `entries`/`seen`/`truncated` and bumps
/// `version` under one short write lock, keeping `complete = true`. The
/// session wake fires once, strictly after the lock drops.
fn rescan_session(entry: &SessionEntry) {
    // The initial walk is bounded by its own budget, so wait that plus slack; on elapse the
    // next fs event retries.
    let wait_deadline = Instant::now() + entry.bounds.budget + Duration::from_secs(1);
    loop {
        if entry.cancel.load(Ordering::SeqCst) {
            return;
        }
        let complete = {
            let guard = entry.state.read();
            guard.complete
            // Read guard drops here (never held across the sleep below).
        };
        if complete {
            break;
        }
        if Instant::now() >= wait_deadline {
            tracing::warn!(
                root = %entry.root.display(),
                "rescan skipped: initial traversal never completed"
            );
            return;
        }
        std::thread::sleep(RESCAN_COMPLETE_POLL);
    }

    tracing::debug!(root = %entry.root.display(), "rescanning file search");

    // Fresh MRU seed, then the shared walk, all into locals — the state lock is
    // untouched until the swap.
    let seed = entry.mru.recent(&entry.root);
    let mut seen: HashSet<PathBuf> = seed.iter().map(|e| e.path.clone()).collect();
    let mut entries = seed;
    let truncated = walk_files(
        &entry.root,
        &entry.bounds,
        &entry.cancel,
        &mut seen,
        |file| entries.push(file),
    );

    // A cancelled walk is partial; swapping it in would LOSE listed files.
    if entry.cancel.load(Ordering::SeqCst) {
        return;
    }

    let swapped = {
        let mut guard = entry.state.write();
        if guard.entries == entries && guard.truncated == truncated {
            // Equal listing: skip the swap AND the version bump, so a no-op
            // rescan causes no UI churn.
            false
        } else {
            guard.entries = entries;
            guard.seen = seen;
            guard.truncated = truncated;
            guard.version += 1;
            true
        }
        // Write lock drops here, before the wake.
    };
    if swapped {
        (entry.wake)();
    }
}

/// A message to the `wolf-fs-watch` thread.
enum WatchMsg {
    /// The live entries changed: re-derive the watched roots from them.
    Reconcile,
    /// One event from the `notify` watcher, passed on untouched by its callback.
    Event(notify::Result<notify::Event>),
    /// The registry dropped: exit.
    Shutdown,
}

/// The body of the `wolf-fs-watch` thread, which makes every `watch` and
/// `unwatch` call. A recursive inotify watch walks the whole tree, so it must
/// never run on a front-end's thread or under the registry's locks; and
/// notify's `watch` waits on the backend thread that runs the callback, which
/// is why the callback only forwards into this thread's channel.
///
/// The thread exits on [`WatchMsg::Shutdown`] or once `registry` no longer
/// upgrades, and upgrades it only for the brief registry calls, never across a
/// `watch`.
fn watch_thread(
    registry: Weak<SearchRegistry>,
    rx: Receiver<WatchMsg>,
    callback_tx: Sender<WatchMsg>,
) {
    let mut watching = Watching::new(callback_tx);
    while let Ok(msg) = rx.recv() {
        let live = match msg {
            WatchMsg::Shutdown => false,
            WatchMsg::Reconcile => watching.reconcile(&registry),
            WatchMsg::Event(res) => watching.on_event(&registry, res),
        };
        if !live {
            break;
        }
    }
}

/// Forwards each `(path, force)` to the registry's
/// [`notify_fs_changed`](SearchRegistry::notify_fs_changed); `false` once the
/// registry is gone.
fn forward(registry: &Weak<SearchRegistry>, changes: Vec<(PathBuf, bool)>) -> bool {
    if changes.is_empty() {
        return true;
    }
    let Some(live) = registry.upgrade() else {
        return false;
    };
    for (path, force) in changes {
        live.notify_fs_changed(path, force);
    }
    true
}

/// Everything the watcher thread owns. Only that thread touches it, so none of
/// it is locked.
struct Watching {
    /// The sender the watcher's callback clones, created lazily with the watcher.
    callback_tx: Sender<WatchMsg>,
    /// The backend watcher, created on the first root to watch.
    watcher: Option<RecommendedWatcher>,
    /// Set when the watcher could not be created; nothing is watched again.
    disabled: bool,
    /// The roots holding a live recursive watch.
    watched: HashSet<PathBuf>,
    /// Roots whose watch failed; not retried until they leave the desired set.
    failed: HashSet<PathBuf>,
    /// One filter per desired root, judging events by that root's walk rules.
    filters: HashMap<PathBuf, WatchFilter>,
    /// Set once an event error has been logged, so a failing backend warns once.
    warned_event_error: bool,
}

impl Watching {
    fn new(callback_tx: Sender<WatchMsg>) -> Self {
        Self {
            callback_tx,
            watcher: None,
            disabled: false,
            watched: HashSet::new(),
            failed: HashSet::new(),
            filters: HashMap::new(),
            warned_event_error: false,
        }
    }

    /// The watcher, created on first use. A creation failure logs one warning
    /// and disables watching for the registry's lifetime. Symlinks are not
    /// followed, as the walk follows none, which also keeps inotify's recursive
    /// walk out of linked trees.
    fn watcher(&mut self) -> Option<&mut RecommendedWatcher> {
        if self.watcher.is_none() && !self.disabled {
            let tx = self.callback_tx.clone();
            let handler = move |res: notify::Result<notify::Event>| {
                let _ = tx.send(WatchMsg::Event(res));
            };
            let config = notify::Config::default().with_follow_symlinks(false);
            match RecommendedWatcher::new(handler, config) {
                Ok(watcher) => self.watcher = Some(watcher),
                Err(err) => {
                    tracing::warn!(
                        %err,
                        "fs watcher failed to start; external changes wait for an nvim trigger"
                    );
                    self.disabled = true;
                }
            }
        }
        self.watcher.as_mut()
    }

    /// Re-derives the watched roots from the registry's live entries
    /// ([`SearchRegistry::watched_roots`]) rather than counting registrations,
    /// so the entries stay the one source of truth and the order of watch and
    /// unwatch requests is immaterial. Returns `false` once the registry is
    /// gone.
    ///
    /// Only the outermost desired roots are watched, since a recursive watch
    /// covers the roots under it. Every stale watch is dropped before any new
    /// one is made. See notes/crates/wolf-core/src/files.rs.md#unwatch-before-watch
    ///
    /// A watch that goes live forwards each desired root under it once, as a
    /// non-force path: the session's walk may have started before the watch,
    /// and a change in that gap would otherwise be lost. A root is a directory,
    /// never in `seen`, so [`needs_rescan`] rescans the sessions rooted there.
    fn reconcile(&mut self, registry: &Weak<SearchRegistry>) -> bool {
        let Some(live) = registry.upgrade() else {
            return false;
        };
        let desired = live.watched_roots();
        drop(live);

        self.filters.retain(|root, _| desired.contains_key(root));
        for (root, bounds) in &desired {
            match self.filters.get_mut(root) {
                Some(filter) => filter.set_bounds(bounds),
                None => {
                    self.filters
                        .insert(root.clone(), WatchFilter::new(root.clone(), bounds));
                }
            }
        }
        self.failed.retain(|root| desired.contains_key(root));

        let mut catch_up: Vec<(PathBuf, bool)> = Vec::new();
        while !self.disabled {
            let effective = effective_roots(desired.keys().filter(|r| !self.failed.contains(*r)));
            let stale: Vec<PathBuf> = self.watched.difference(&effective).cloned().collect();
            for root in stale {
                self.watched.remove(&root);
                if let Some(watcher) = self.watcher.as_mut() {
                    let _ = watcher.unwatch(&root);
                }
            }
            let fresh: Vec<PathBuf> = effective.difference(&self.watched).cloned().collect();
            let mut newly_failed = false;
            for root in fresh {
                let result = match self.watcher() {
                    Some(watcher) => watcher.watch(&root, RecursiveMode::Recursive),
                    None => break,
                };
                match result {
                    Ok(()) => {
                        catch_up.extend(
                            desired
                                .keys()
                                .filter(|r| r.starts_with(&root))
                                .map(|r| (r.clone(), false)),
                        );
                        self.watched.insert(root);
                    }
                    Err(err) => {
                        tracing::warn!(
                            root = %root.display(),
                            %err,
                            "fs watch failed; external changes under this root wait for an nvim trigger"
                        );
                        // Releases the watches a recursive inotify watch made before failing.
                        if let Some(watcher) = self.watcher.as_mut() {
                            let _ = watcher.unwatch(&root);
                        }
                        self.failed.insert(root);
                        newly_failed = true;
                    }
                }
            }
            // A failed outer root leaves the roots under it uncovered; the next pass watches them.
            if !newly_failed {
                break;
            }
        }
        forward(registry, catch_up)
    }

    /// Judges one watcher event and forwards each path the walk could list.
    /// Returns `false` once the registry is gone.
    ///
    /// An event that asks for a rescan (inotify queue overflow, FSEvents
    /// dropped events) names no reliable path, so it forwards one desired root
    /// as `force`.
    fn on_event(
        &mut self,
        registry: &Weak<SearchRegistry>,
        res: notify::Result<notify::Event>,
    ) -> bool {
        let event = match res {
            Ok(event) => event,
            Err(err) => {
                if !self.warned_event_error {
                    self.warned_event_error = true;
                    tracing::warn!(%err, "fs watcher reported an error");
                }
                return true;
            }
        };
        if event.need_rescan() {
            let Some(root) = self.filters.keys().next().cloned() else {
                return true;
            };
            return forward(registry, vec![(root, true)]);
        }
        let Some(force) = event_force(&event.kind) else {
            return true;
        };
        let kind_dir = kind_is_dir(&event.kind);
        let changes: Vec<(PathBuf, bool)> = event
            .paths
            .into_iter()
            .filter_map(|path| self.judge(canonical_event_path(path), force, kind_dir))
            .collect();
        forward(registry, changes)
    }

    /// The verdict on one canonical event `path`: `Some((path, force))` when
    /// some filter admits it, `None` when every walk would skip it.
    ///
    /// A `.gitignore` a filter's walk reads changes what that walk lists, so it
    /// drops the cached rules of its directory and is forwarded as `force`. A
    /// force event (a remove or rename) also drops every cached rule at or
    /// under the path, since a moved directory takes its ignore files with it.
    /// A path that no longer exists takes its kind from the event, and when the
    /// event cannot say, either answer admits it.
    fn judge(
        &mut self,
        path: PathBuf,
        force: bool,
        kind_dir: Option<bool>,
    ) -> Option<(PathBuf, bool)> {
        if force {
            for filter in self.filters.values_mut() {
                filter.forget_under(&path);
            }
        }
        let is_dir = std::fs::symlink_metadata(&path)
            .ok()
            .map(|m| m.is_dir())
            .or(kind_dir);
        let is_gitignore = path.file_name() == Some(OsStr::new(".gitignore"));
        let mut admitted = false;
        let mut force = force;
        for filter in self.filters.values_mut() {
            if is_gitignore && filter.governs(&path) {
                if let Some(dir) = path.parent() {
                    filter.forget_dir(dir);
                }
                admitted = true;
                force = true;
            } else if !admitted {
                admitted = match is_dir {
                    Some(dir) => filter.admits(&path, dir),
                    None => filter.admits(&path, false) || filter.admits(&path, true),
                };
            }
        }
        admitted.then_some((path, force))
    }
}

/// Canonicalizes an event path the way `seen` holds the walk's paths: the
/// parent through `canonicalize`, the last component kept as is, so a
/// symlinked file keeps its own name. A path whose parent is gone stays as
/// reported.
fn canonical_event_path(path: PathBuf) -> PathBuf {
    let canonical = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent).ok().map(|p| p.join(name)),
        _ => None,
    };
    canonical.unwrap_or(path)
}

/// Maps an event kind to the `force` flag it is forwarded with, or `None` to
/// drop it.
///
/// A remove or rename may take a listed file away, which no `seen` check can
/// see, so it forces. A create or modify does not: [`needs_rescan`]'s `seen`
/// check already rescans for a path the listing lacks. An access is dropped,
/// because on inotify a walk's own directory and `.gitignore` opens report
/// `Access(Open)`, and forwarding them would make each rescan trigger the next.
fn event_force(kind: &EventKind) -> Option<bool> {
    match kind {
        EventKind::Access(_) => None,
        EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) => Some(true),
        _ => Some(false),
    }
}

/// Whether an event's kind says its path is a directory, for a path that no
/// longer exists to be asked. `None` when the kind does not say.
///
/// A directory create is forwarded like a file create: notify's inotify
/// backend misses files made in a new directory before its watch lands, and
/// the directory, absent from `seen`, rescans them in.
fn kind_is_dir(kind: &EventKind) -> Option<bool> {
    match kind {
        EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder) => Some(true),
        EventKind::Create(CreateKind::File) | EventKind::Remove(RemoveKind::File) => Some(false),
        _ => None,
    }
}

/// The roots that need a watch of their own: each root with no strict ancestor
/// in the set, compared component-wise, so `/ab` is not under `/a`.
fn effective_roots<'a>(roots: impl IntoIterator<Item = &'a PathBuf>) -> HashSet<PathBuf> {
    let roots: Vec<&PathBuf> = roots.into_iter().collect();
    roots
        .iter()
        .filter(|root| {
            !roots
                .iter()
                .any(|other| other != *root && root.starts_with(other))
        })
        .map(|root| (*root).clone())
        .collect()
}

/// The walk's verdict on a single path under one watched root, without a walk.
///
/// [`walk_files`] decides top-down as it descends; an event arrives as one
/// path, so this replays the same decisions for it: the excluded names, the
/// depth cap, `skip_dirs`, and `ignore`'s gitignore chain for each directory
/// on the way down and for the path itself. A `cargo build` under an ignored
/// `target/` would otherwise cause a stream of re-walks, and the filter can
/// only ever drop an event for a path the walk would not list.
///
/// The per-directory rules are read on first use and cached until a remove,
/// a rename or a `.gitignore` edit invalidates them.
/// See notes/crates/wolf-core/src/files.rs.md#the-gitignore-chain
struct WatchFilter {
    /// The session root the walk starts from.
    root: PathBuf,
    /// The most permissive `max_depth` of the sessions on `root`.
    max_depth: usize,
    /// The `skip_dirs` every session on `root` shares.
    skip_dirs: HashSet<PathBuf>,
    /// The global git excludes, built as the walk builds them.
    global: Gitignore,
    /// Each directory's `.gitignore` and `.git/info/exclude` rules.
    dirs: HashMap<PathBuf, DirRules>,
}

/// One directory's ignore rules, as the walk reads them.
struct DirRules {
    /// The directory's `.gitignore`.
    gitignore: Gitignore,
    /// The directory's `.git/info/exclude`, read whether or not `.git` is a
    /// repository, as `require_git(false)` has the walk do.
    exclude: Gitignore,
}

impl DirRules {
    fn load(dir: &Path) -> Self {
        Self {
            gitignore: load_ignore_file(dir, &dir.join(".gitignore")),
            exclude: load_ignore_file(dir, &dir.join(".git").join("info").join("exclude")),
        }
    }
}

/// A matcher rooted at `dir` for the ignore file at `file`, empty when the file
/// is absent or fails to build, as `ignore`'s own `create_gitignore` does.
fn load_ignore_file(dir: &Path, file: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(dir);
    if file.exists() {
        // A bad line is skipped and the rest kept, as in the walk.
        let _ = builder.add(file);
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// A gitignore match as `Some(true)` for ignore, `Some(false)` for a whitelist
/// (`!pattern`), and `None` for no match.
fn verdict<T>(m: ignore::Match<T>) -> Option<bool> {
    match m {
        ignore::Match::None => None,
        ignore::Match::Ignore(_) => Some(true),
        ignore::Match::Whitelist(_) => Some(false),
    }
}

impl WatchFilter {
    fn new(root: PathBuf, bounds: &SearchBounds) -> Self {
        // The walk builds its global matcher relative to the process cwd.
        let global = std::env::current_dir().map_or_else(
            |_| Gitignore::empty(),
            |cwd| GitignoreBuilder::new(cwd).build_global().0,
        );
        Self {
            root,
            max_depth: bounds.max_depth,
            skip_dirs: bounds.skip_dirs.clone(),
            global,
            dirs: HashMap::new(),
        }
    }

    fn set_bounds(&mut self, bounds: &SearchBounds) {
        self.max_depth = bounds.max_depth;
        self.skip_dirs.clone_from(&bounds.skip_dirs);
    }

    /// `true` when a walk of `root` would list `path` if it were a file
    /// (`is_dir` false), or would descend into it (`is_dir` true).
    fn admits(&mut self, path: &Path, is_dir: bool) -> bool {
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return false;
        };
        let depth = rel.components().count();
        if depth == 0 || depth > self.max_depth || has_excluded_component(path) {
            return false;
        }
        // `path` and each of its ancestors strictly below the root, innermost first.
        let chain: Vec<PathBuf> = path
            .ancestors()
            .take(depth)
            .map(Path::to_path_buf)
            .collect();
        if chain.iter().any(|p| self.skip_dirs.contains(p)) {
            return false;
        }
        for dir in chain.iter().skip(1).rev() {
            if self.ignored(dir, true) {
                return false;
            }
        }
        !self.ignored(path, is_dir)
    }

    /// `true` when the walk of `root` reads `gitignore`: its directory is the
    /// root or one the walk descends into, within `max_depth`.
    fn governs(&mut self, gitignore: &Path) -> bool {
        let Some(dir) = gitignore.parent() else {
            return false;
        };
        let Ok(rel) = gitignore.strip_prefix(&self.root) else {
            return false;
        };
        if rel.components().count() > self.max_depth || has_excluded_component(gitignore) {
            return false;
        }
        dir == self.root || self.admits(dir, true)
    }

    /// `ignore`'s verdict on `path`: the deepest `.gitignore` match, else the
    /// deepest `.git/info/exclude` match, else the global excludes, over every
    /// directory from the path's parent up to `/`.
    fn ignored(&mut self, path: &Path, is_dir: bool) -> bool {
        let (mut gitignore, mut exclude) = (None, None);
        let mut dir = path.parent();
        while let Some(d) = dir {
            if gitignore.is_some() && exclude.is_some() {
                break;
            }
            let rules = self
                .dirs
                .entry(d.to_path_buf())
                .or_insert_with(|| DirRules::load(d));
            if gitignore.is_none() {
                gitignore = verdict(rules.gitignore.matched(path, is_dir));
            }
            if exclude.is_none() {
                exclude = verdict(rules.exclude.matched(path, is_dir));
            }
            dir = d.parent();
        }
        gitignore
            .or(exclude)
            .or_else(|| verdict(self.global.matched(path, is_dir)))
            == Some(true)
    }

    /// Drops the cached rules of every directory at or under `path`.
    fn forget_under(&mut self, path: &Path) {
        self.dirs.retain(|dir, _| !dir.starts_with(path));
    }

    /// Drops the cached rules of `dir`.
    fn forget_dir(&mut self, dir: &Path) {
        self.dirs.remove(dir);
    }
}

// Unit tests for [`needs_rescan`]: `force` always rescans, otherwise only a path under `root`
// that is absent from `seen` does.
#[cfg(test)]
mod rescan_decision_tests {
    use super::*;

    /// The session root every case uses; `paths` under it are "in scope".
    fn root() -> PathBuf {
        PathBuf::from("/root/project")
    }

    /// A `seen` set holding the given absolute paths.
    fn seen(paths: &[&str]) -> HashSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn force_always_rescans() {
        // Force (BufFilePost / BufDelete) short-circuits everything: even with
        // no paths at all, the rescan must run.
        assert!(
            needs_rescan(true, &[], &root(), &seen(&[])),
            "force must rescan even with an empty path list"
        );
        // …and a force with a path that would otherwise be skipped (already
        // seen) still rescans.
        assert!(
            needs_rescan(
                true,
                &[PathBuf::from("/root/project/known.md")],
                &root(),
                &seen(&["/root/project/known.md"]),
            ),
            "force must rescan even when every path is already seen"
        );
    }

    #[test]
    fn new_path_under_root_rescans() {
        assert!(
            needs_rescan(
                false,
                &[PathBuf::from("/root/project/fresh.md")],
                &root(),
                &seen(&["/root/project/known.md"]),
            ),
            "a path under the root and absent from `seen` must trigger a rescan"
        );
    }

    #[test]
    fn already_seen_path_skips() {
        assert!(
            !needs_rescan(
                false,
                &[PathBuf::from("/root/project/known.md")],
                &root(),
                &seen(&["/root/project/known.md"]),
            ),
            "a `:w` of an already-listed file must NOT trigger a rescan"
        );
    }

    #[test]
    fn path_outside_root_skips() {
        assert!(
            !needs_rescan(
                false,
                &[PathBuf::from("/elsewhere/other.md")],
                &root(),
                &seen(&[]),
            ),
            "a path outside the session root must NOT trigger a rescan"
        );
    }

    #[test]
    fn empty_paths_without_force_skips() {
        assert!(
            !needs_rescan(false, &[], &root(), &seen(&[])),
            "no force and no paths must NOT trigger a rescan"
        );
    }

    #[test]
    fn any_new_under_root_path_in_a_batch_rescans() {
        // A batch mixing a seen path, an out-of-root path and one new under-root path still
        // rescans: any qualifying path is enough.
        assert!(
            needs_rescan(
                false,
                &[
                    PathBuf::from("/root/project/known.md"),
                    PathBuf::from("/elsewhere/other.md"),
                    PathBuf::from("/root/project/sub/fresh.md"),
                ],
                &root(),
                &seen(&["/root/project/known.md"]),
            ),
            "one new under-root path in a batch must be enough to rescan"
        );
    }
}

// Unit tests for [`append_batch`]'s maintenance of [`FileSearchState::seen`].
#[cfg(test)]
mod append_batch_seen_tests {
    use super::*;

    #[test]
    fn append_batch_folds_paths_into_seen() {
        // `seen` must hold every streamed path, not just the MRU seed, or a `:w` of a
        // walk-discovered file would trigger a spurious rescan.
        let state = Arc::new(RwLock::new(FileSearchState::seeded(Vec::new())));
        let path = PathBuf::from("/root/project/discovered.md");
        let mut batch = vec![FileEntry {
            path: path.clone(),
            display: String::from("discovered.md"),
            mru_rank: None,
        }];

        append_batch(&state, &mut batch, false, false);

        let guard = state.read();
        assert!(
            guard.seen.contains(&path),
            "a streamed entry's path must be a member of `seen` after append_batch"
        );
    }
}

// Unit tests for the watcher's event judgement: [`WatchFilter`] against the walk, the force
// mapping, and the outermost-root rule.
#[cfg(test)]
mod watch_tests {
    use super::*;
    use notify::event::{AccessKind, AccessMode, DataChange, MetadataKind, RenameMode};

    /// Writes `contents` to `root/rel`, creating parent directories.
    fn put(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// Every file under `dir`, found with `std::fs` alone.
    fn all_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                all_files(&path, out);
            } else {
                out.push(path);
            }
        }
    }

    #[test]
    fn watch_filter_admits_exactly_what_the_walk_lists() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        put(&root, ".gitignore", "*.log\n!keep.log\nbuild/\n");
        put(&root, "sub/.gitignore", "local.md\n");
        put(&root, ".git/info/exclude", "excluded.md\n");
        for rel in [
            "a.md",
            "x.log",
            "keep.log",
            "build/out.o",
            "build/deep/y.md",
            "sub/b.md",
            "sub/local.md",
            "sub/z.log",
            "excluded.md",
            "d1/d2/f.md",
            "d1/d2/d3/deep.md",
            "skipme/s.md",
            ".wolf/c.json",
            ".git/HEAD",
        ] {
            put(&root, rel, "x");
        }
        let bounds = SearchBounds {
            max_entries: usize::MAX,
            budget: Duration::from_secs(60),
            max_depth: 3,
            skip_dirs: HashSet::from([root.join("skipme")]),
        };

        let mut listed = HashSet::new();
        let mut seen = HashSet::new();
        walk_files(&root, &bounds, &AtomicBool::new(false), &mut seen, |e| {
            listed.insert(e.path);
        });
        let mut files = Vec::new();
        all_files(&root, &mut files);
        assert!(
            listed.contains(&root.join("d1/d2/f.md")) && !listed.contains(&root.join("sub/z.log")),
            "the fixture must exercise the depth cap and the ancestor chain: {listed:?}"
        );

        let mut filter = WatchFilter::new(root.clone(), &bounds);
        let verdicts: Vec<(PathBuf, bool)> = files
            .iter()
            .map(|f| (f.clone(), filter.admits(f, false)))
            .collect();
        for (file, admitted) in &verdicts {
            assert_eq!(
                *admitted,
                listed.contains(file),
                "filter and walk disagree on {}",
                file.display()
            );
        }

        // A removal is judged by name, so deleting a file leaves its verdict as it was.
        std::fs::remove_file(root.join("a.md")).unwrap();
        std::fs::remove_file(root.join("x.log")).unwrap();
        for (file, admitted) in &verdicts {
            assert_eq!(
                filter.admits(file, false),
                *admitted,
                "verdict on {} changed after the removals",
                file.display()
            );
        }
    }

    #[test]
    fn event_force_maps_each_kind() {
        assert_eq!(event_force(&EventKind::Remove(RemoveKind::Any)), Some(true));
        assert_eq!(
            event_force(&EventKind::Modify(ModifyKind::Name(RenameMode::Any))),
            Some(true)
        );
        assert_eq!(
            event_force(&EventKind::Create(CreateKind::File)),
            Some(false)
        );
        assert_eq!(
            event_force(&EventKind::Modify(ModifyKind::Data(DataChange::Content))),
            Some(false)
        );
        assert_eq!(
            event_force(&EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any))),
            Some(false)
        );
        assert_eq!(
            event_force(&EventKind::Access(AccessKind::Close(AccessMode::Write))),
            None
        );
    }

    #[test]
    fn effective_roots_keeps_outermost() {
        let roots: Vec<PathBuf> = ["/a", "/a/b", "/ab", "/c"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let expected: HashSet<PathBuf> = ["/a", "/ab", "/c"].iter().map(PathBuf::from).collect();
        assert_eq!(effective_roots(&roots), expected);
    }
}
