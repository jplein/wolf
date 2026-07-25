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

use parking_lot::{ArcRwLockReadGuard, RawRwLock, RwLock};

use crate::redraw::WakeFn;

/// Number of kept files accumulated before a batch is appended to the shared
/// state (and a wake fired). Keeps lock hold-times short while still streaming
/// results to the UI promptly.
const TRAVERSAL_BATCH: usize = 256;

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
            version: 1,
            seen,
        }
    }
}

/// An owned read guard over a [`FileSearchState`].
///
/// Owns an `Arc` clone of the lock (so its lifetime is independent of the
/// [`FileSearch`]); `Deref`s to [`FileSearchState`] so callers read
/// `guard.entries`, `guard.complete`, and `guard.version` directly.
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
pub(crate) fn spawn_traversal(
    root: PathBuf,
    state: Arc<RwLock<FileSearchState>>,
    cancel: Arc<AtomicBool>,
    wake: WakeFn,
    handle: tokio::runtime::Handle,
) {
    handle.spawn_blocking(move || {
        tracing::debug!(root = %root.display(), "spawn_traversal walking");
        // Local dedup set, seeded from the synchronous MRU seed. The traversal is
        // the only producer after start, so a local set avoids locking per entry.
        let mut seen: HashSet<PathBuf> = state.read().seen.clone();
        let mut batch: Vec<FileEntry> = Vec::new();

        let walker = ignore::WalkBuilder::new(&root)
            .parents(true)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .ignore(false)
            .require_git(false)
            .follow_links(false)
            .filter_entry(|entry| {
                let name = entry.file_name();
                name != OsStr::new(".git") && name != OsStr::new(".wolf")
            })
            .build();

        for result in walker {
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

            if batch.len() >= TRAVERSAL_BATCH {
                append_batch(&state, &mut batch, false);
                wake();
            }
        }

        let complete = !cancel.load(Ordering::SeqCst);
        append_batch(&state, &mut batch, complete);
        wake();
    });
}

/// Appends `batch` (draining it) to the shared state, bumps `version`, and (when
/// `complete`) marks the search finished. The write lock is held only for this
/// short, IO-free critical section and never across a wake.
fn append_batch(state: &Arc<RwLock<FileSearchState>>, batch: &mut Vec<FileEntry>, complete: bool) {
    let mut guard = state.write();
    guard.entries.append(batch);
    guard.version += 1;
    if complete {
        guard.complete = true;
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
