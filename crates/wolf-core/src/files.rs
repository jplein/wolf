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
//! ## Traversal rules (per plan §D3/§D6)
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

use parking_lot::{ArcRwLockReadGuard, RawRwLock, RwLock};

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

/// The bounds that keep a single file-search traversal from running away (e.g. a
/// fallback walk of the entire `$HOME` tree): an entry cap, a wall-clock budget,
/// a max depth, and a set of absolute directories to skip outright.
///
/// Resolved once (synchronously, on the calling thread) per search via
/// [`SearchBounds::from_env`] and moved into the traversal closure. `skip_dirs`
/// starts empty; the caller populates it with pathological trees (e.g.
/// `~/Library`, `~/.cache`) that should never be walked.
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

        let started = Instant::now();
        let mut accepted: usize = 0;
        let mut truncated = false;

        // Absolute directories to prune outright, moved into the `filter_entry`
        // closure (kept `Send + Sync` so `ignore` can share it across threads).
        let skip = bounds.skip_dirs.clone();
        let walker = ignore::WalkBuilder::new(&root)
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

            batch.push(FileEntry {
                path: abs,
                display: display_for(&root, path),
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

            if batch.len() >= TRAVERSAL_BATCH {
                append_batch(&state, &mut batch, false, false);
                wake();
            }
        }

        let complete = !cancel.load(Ordering::SeqCst);
        append_batch(&state, &mut batch, complete, truncated);
        wake();
    });
}

/// Appends `batch` (draining it) to the shared state, bumps `version`, marks the
/// search finished when `complete`, and marks it `truncated` when the walk
/// stopped early at a cap. The write lock is held only for this short, IO-free
/// critical section and never across a wake.
fn append_batch(
    state: &Arc<RwLock<FileSearchState>>,
    batch: &mut Vec<FileEntry>,
    complete: bool,
    truncated: bool,
) {
    let mut guard = state.write();
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
