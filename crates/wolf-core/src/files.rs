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
//! hang it), and never descends into `.git/` or our own `.wolf/` cache. Each
//! entry is classified by [`is_listable_file`]: plain files are kept, a symlink is
//! kept only when its target is a plain file, and directories / broken or
//! directory symlinks are dropped.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{ArcRwLockReadGuard, Mutex, RawRwLock, RwLock};

use crate::mru::MruStore;
use crate::redraw::WakeFn;

/// Number of kept files accumulated before a batch is appended to the shared
/// state (and a wake fired). Keeps lock hold-times short while still streaming
/// results to the UI promptly.
const TRAVERSAL_BATCH: usize = 256;

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
            name != OsStr::new(".git")
                && name != OsStr::new(".wolf")
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

/// True if any component of `path` is `.git` or `.wolf` (a defensive backstop in
/// addition to the walker's `filter_entry` pruning of those subtrees).
fn has_excluded_component(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(c, Component::Normal(name) if name == OsStr::new(".git") || name == OsStr::new(".wolf"))
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
/// `force` (a `BufFilePost` rename or a `BufDelete`) always rescans — the
/// filesystem may have LOST a listed file, which no membership check can see.
/// Otherwise (`BufWritePost`) a rescan runs only when some path lies under
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
}

impl SessionEntry {
    /// Assembles a registry entry for a just-started session (the traversal
    /// having been spawned against the same `state`/`cancel` handles).
    pub(crate) fn new(
        state: Arc<RwLock<FileSearchState>>,
        cancel: Arc<AtomicBool>,
        root: PathBuf,
        bounds: SearchBounds,
        wake: WakeFn,
        mru: MruStore,
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
/// `wolf_fs_changed` notification. Sessions self-register from
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
}

impl SearchRegistry {
    /// Creates an empty registry driving rescans on `handle`, with the debounce
    /// window read from `WOLF_RESCAN_DEBOUNCE_MS` (unset / unparsable / zero
    /// falls back to [`RESCAN_DEBOUNCE_DEFAULT`] — the `> 0` guard mirrors
    /// [`SearchBounds::from_env`]).
    pub(crate) fn new(handle: tokio::runtime::Handle) -> Self {
        let debounce = std::env::var("WOLF_RESCAN_DEBOUNCE_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|&ms| ms > 0)
            .map(Duration::from_millis)
            .unwrap_or(RESCAN_DEBOUNCE_DEFAULT);
        Self {
            handle,
            debounce,
            sessions: Mutex::new(Vec::new()),
            pending: Mutex::new(Pending::default()),
        }
    }

    /// Registers a live session, pruning dead ones (cancel flag set) under the
    /// same lock. Register is the only hook that runs on picker-open, so it
    /// must prune here: without fs events, [`Self::notify_fs_changed`] never
    /// fires, and a browse-only session would otherwise accumulate dead
    /// entries — each holding a strong `Arc` to its search state — unboundedly.
    pub(crate) fn register(&self, entry: SessionEntry) {
        let mut sessions = self.sessions.lock();
        sessions.retain(|s| !s.cancel.load(Ordering::SeqCst));
        sessions.push(Arc::new(entry));
    }

    /// Folds one `wolf_fs_changed` event into the pending burst and (on the
    /// first event of a burst) schedules the debounce flush.
    ///
    /// `path` is the reported absolute path (canonicalized by the caller where
    /// possible); `force` is `true` for renames/deletes (`BufFilePost` /
    /// `BufDelete`), which rescan unconditionally. Dead sessions (cancel flag
    /// set) are pruned here, on every notify, so a closed picker/tree stops
    /// costing walks after at most one more event. Each mutex guard is bound to
    /// its own short scope; nothing slow runs under either lock.
    pub(crate) fn notify_fs_changed(self: &Arc<Self>, path: PathBuf, force: bool) {
        {
            let mut sessions = self.sessions.lock();
            sessions.retain(|s| !s.cancel.load(Ordering::SeqCst));
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
