//! Application-state persistence (`state.json`) + platform state-dir resolution.
//!
//! This is the GUI-toolkit-free state module: it resolves the per-platform
//! application-state directory (macOS `~/Library/Application Support/Wolf`,
//! Linux `~/.local/state/wolf`), reads a pretty-printed `state.json` holding a
//! TREE of windows — each window carries geometry plus a LIST of tabs, and each
//! tab carries its own working directory, sidebar visibility, and (optionally) its
//! full pane-split [`layout`](TabState::layout) tree — and writes it back through a
//! single dedicated writer thread.
//!
//! ## Loading (infallible, silently-ignore)
//!
//! [`load_state`] never creates, modifies, or deletes the file: an absent file
//! is a silent `None`; ANY read/parse/shape/version failure is a
//! `tracing::warn!` + `None` with the on-disk bytes preserved verbatim
//! (all-or-nothing, matching the `config` module's precedent). Unknown extra
//! fields are tolerated (no `deny_unknown_fields`) so newer writers do not
//! break older readers.
//!
//! ## Writing (coalescing, latest-wins, atomic)
//!
//! A [`StateStore`] holds the in-memory [`WolfState`] tree under a mutex and a
//! generation counter; the front-end submits the WHOLE tree at once via
//! [`StateStore::set_snapshot`], which swaps it in and bumps the generation
//! ONLY when the submitted tree differs from the current one (an identical
//! snapshot is a no-op, so a KVO echo storm produces no disk churn). A
//! dedicated writer thread (named `wolf-state-writer`) sleeps a short coalesce
//! delay, clones the LATEST state, serializes with
//! `serde_json::to_string_pretty`, writes a sibling tmp file, and renames it
//! over `state.json` — so a crash never leaves a partial file and rapid
//! snapshot bursts produce one write. [`StateStore::flush`] blocks until
//! everything pending has been attempted.
//!
//! Concurrent Wolf PROCESSES are out of scope: the last writer wins (each
//! write is still atomic, so the file is never torn).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

/// The on-disk state file name read/written in the platform state dir.
pub const STATE_FILE_NAME: &str = "state.json";

/// The sibling temp file the writer thread writes before the atomic rename.
const STATE_TMP_FILE_NAME: &str = "state.json.tmp";

/// The state-file schema version this build reads/writes. A file carrying any
/// OTHER version is silently ignored (treated as absent).
const STATE_VERSION: u64 = 1;

/// How long the writer thread sleeps after being woken before it snapshots and
/// writes, so a rapid burst of mutations coalesces into one disk write.
const WRITE_COALESCE_DELAY: Duration = Duration::from_millis(100);

/// The writer thread's idle re-check cadence: while nothing is pending it wakes
/// this often to notice that every front-end [`StateStore`] handle is gone (and
/// then exits). Purely a cleanup path — mutations wake it immediately.
const WRITER_IDLE_POLL: Duration = Duration::from_millis(500);

/// The axis along which a [`PaneNode::Split`] divides its bounds.
///
/// Serialized as the lowercase strings `"horizontal"` / `"vertical"`. A
/// `.horizontal` split stacks its panes top/bottom (dividing HEIGHT); a
/// `.vertical` split places them side by side (dividing WIDTH) — matching the
/// macOS `PaneTree` / `wolf-ffi` orientation table (horizontal == 0, vertical == 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneOrientation {
    /// Panes stacked top/bottom; the divider runs horizontally and splits HEIGHT.
    Horizontal,
    /// Panes side by side; the divider runs vertically and splits WIDTH.
    Vertical,
}

/// One node of a tab's persisted pane-split tree: either a leaf bound to a buffer
/// path, or an n-ary split of child nodes along an [`PaneOrientation`] with a
/// parallel per-child `weights` vector.
///
/// Serialized as an internally-tagged enum (`"type": "leaf"` / `"type": "split"`),
/// mirroring the front-end `SavedPaneNode`. A `Leaf`'s `path` is `""` for an
/// unnamed buffer; when `preview` is `true` the pane is a Markdown preview and
/// `path` is the SOURCE Markdown file's ABS path (restored via `wolf_preview_file`,
/// not `:edit`). `preview` is `#[serde(default)]` so an old v1 file without it loads
/// with `preview = false`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaneNode {
    /// A single pane showing the buffer at `path` (`""` for an unnamed buffer), or a
    /// Markdown preview of the source file at `path` when `preview` is `true`.
    Leaf {
        /// The absolute buffer path, or `""` for an unnamed buffer.
        path: String,
        /// Whether this leaf is a Markdown preview of the source file at `path`.
        #[serde(default)]
        preview: bool,
    },
    /// An n-ary split of `children` along `orientation`, with parallel per-child
    /// `weights`.
    Split {
        /// The axis this node splits along.
        orientation: PaneOrientation,
        /// The per-child weights (parallel to `children`).
        weights: Vec<f64>,
        /// The child nodes, in DFS (left-to-right / top-to-bottom) order.
        children: Vec<PaneNode>,
    },
}

/// One tab's persisted state (a tab is its own captive Neovim: buffers, splits,
/// scroll, sidebar, and working directory).
///
/// `cwd` is the absolute canonicalized path nvim reported, or `""` when it has
/// not been reported yet (a fresh empty-state tab); restore validates it is a
/// non-empty, existing directory before use. `sidebar_visible` records whether
/// the file-tree sidebar was showing (defaults to `true`). `layout` is the tab's
/// full pane-split tree (topology + per-child weights + per-leaf buffer path); it
/// is `None` ONLY for an empty-state lone pane (an unnamed, non-preview buffer,
/// e.g. the "no folder" tab) — a lone pane showing a file or a preview persists a
/// single `leaf`. It is `#[serde(default)]` so an old v1 file without it loads
/// with `layout: None` (no version bump).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TabState {
    /// The embedded Neovim's working directory (`""` == not yet reported).
    pub cwd: String,
    /// Whether the file-tree sidebar is visible (defaults to `true`).
    pub sidebar_visible: bool,
    /// The tab's pane-split layout, `None` ONLY for an empty-state (unnamed,
    /// non-preview) lone pane; a lone pane showing a file or preview persists a
    /// single `leaf`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<PaneNode>,
}

impl Default for TabState {
    fn default() -> Self {
        Self {
            cwd: String::new(),
            sidebar_visible: true,
            layout: None,
        }
    }
}

/// One window's persisted state: geometry plus its ordered list of tabs.
///
/// Geometry is in `f64` points, interpreted by the WRITING front-end (macOS
/// stores `NSWindow.frame`: AppKit bottom-left global coordinates). A
/// non-positive `width`/`height` is the "unset" sentinel — restore treats the
/// geometry as absent. `tabs` is the window's tab bar, in tab order; there is
/// NO `#[serde(default)]` on it, so an OLD flat (pre-tabs) file that carries a
/// window-level `cwd`/`sidebar_visible` and no `tabs` fails deserialization and
/// is silently ignored (no migration path).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowState {
    /// The window origin's x coordinate, in points.
    pub x: f64,
    /// The window origin's y coordinate, in points.
    pub y: f64,
    /// The window width, in points (`<= 0` == unset).
    pub width: f64,
    /// The window height, in points (`<= 0` == unset).
    pub height: f64,
    /// The window's tabs, in tab order (each its own captive Neovim).
    pub tabs: Vec<TabState>,
}

/// The whole persisted application state: a TREE of windows, each with geometry
/// and its own list of tabs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WolfState {
    /// The per-window entries, in window order.
    pub windows: Vec<WindowState>,
}

/// The on-disk file shape: a `version` header wrapping the window tree.
#[derive(serde::Serialize, serde::Deserialize)]
struct StateFile {
    version: u64,
    windows: Vec<WindowState>,
}

/// Resolves the per-platform application-state directory that holds
/// `state.json`.
///
/// The `WOLF_STATE_DIR` environment variable overrides everything (a
/// test/determinism hook); otherwise it is macOS
/// `~/Library/Application Support/Wolf` (via [`dirs::data_dir`]) or Linux
/// `~/.local/state/wolf` (via [`dirs::state_dir`], falling back to
/// [`dirs::data_dir`]). Returns `None` only when the platform dir cannot be
/// resolved (no override and no home directory).
#[must_use]
pub fn state_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("WOLF_STATE_DIR") {
        return Some(PathBuf::from(dir));
    }
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir().map(|d| d.join("Wolf"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::state_dir()
            .or_else(dirs::data_dir)
            .map(|d| d.join("wolf"))
    }
}

/// Resolves the durable, Wolf-owned, per-project swap directory for `cwd`.
///
/// Returns `state_dir()/swap/<stable-hex-hash-of-cwd>` — a STABLE, DETERMINISTIC
/// path derived from the (assumed already-canonical) project `cwd`, so the SAME
/// project reuses the SAME swap dir across respawns AND across separate Wolf
/// processes (letting a fresh nvim find a dead nvim's swap), while DISTINCT
/// projects get distinct dirs. The hash is a plain [`std::hash::DefaultHasher`]
/// rendered as hex — not cryptographic, just a stable, collision-unlikely name.
///
/// `None` only when [`state_dir`] cannot be resolved (no `WOLF_STATE_DIR`
/// override and no home directory); [`crate::api::WolfCore::start`] then falls
/// back to an ephemeral swap dir under the nvim state root (documented, no panic).
#[must_use]
pub fn swap_dir_for(cwd: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cwd.hash(&mut hasher);
    let name = format!("{:016x}", hasher.finish());
    state_dir().map(|d| d.join("swap").join(name))
}

/// Reads `dir/state.json`, or `None` when it is absent or unusable.
///
/// INFALLIBLE and STRICTLY read-only: it NEVER creates, rewrites, or deletes
/// the file. An absent file is a silent `None` (debug log only); any other
/// failure — unreadable, unparseable, wrong shape, missing required field, or
/// a `version` other than [`STATE_VERSION`] — logs a `tracing::warn!` and
/// returns `None`, leaving the on-disk bytes untouched (all-or-nothing, never
/// a partial parse). Unknown extra fields are tolerated.
#[must_use]
pub fn load_state(dir: &Path) -> Option<WolfState> {
    let path = dir.join(STATE_FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("wolf-core: no state file at {}", path.display());
            return None;
        }
        Err(e) => {
            tracing::warn!(
                "wolf-core: could not read {}: {e}; ignoring saved state",
                path.display()
            );
            return None;
        }
    };
    let file: StateFile = match serde_json::from_str(&text) {
        Ok(file) => file,
        Err(e) => {
            tracing::warn!(
                "wolf-core: malformed {}: {e}; ignoring saved state (file left untouched)",
                path.display()
            );
            return None;
        }
    };
    if file.version != STATE_VERSION {
        tracing::warn!(
            "wolf-core: {} has unsupported version {}; ignoring saved state",
            path.display(),
            file.version
        );
        return None;
    }
    Some(WolfState {
        windows: file.windows,
    })
}

/// The mutable heart of a [`StateStore`]: the in-memory truth plus the
/// generation counters the writer thread and [`StateStore::flush`] rendezvous
/// on. `generation` counts mutations; `written` is the highest generation whose
/// write has been ATTEMPTED (write errors still advance it, so `flush` cannot
/// hang on a read-only disk).
struct StateCell {
    state: WolfState,
    generation: u64,
    written: u64,
}

/// The shared innards behind every clone of a [`StateStore`].
struct StoreInner {
    /// The directory holding `state.json` (created lazily on first write).
    dir: PathBuf,
    /// The in-memory truth + generation counters.
    cell: Mutex<StateCell>,
    /// Wakes the writer on mutation and flushers on write completion.
    cond: Condvar,
}

/// A live, cloneable handle to the persisted application state.
///
/// Opened over an explicit directory (the injection seam — production callers
/// pass [`state_dir`]), it loads the existing `state.json` (defaults on any
/// failure, per [`load_state`]) and spawns one dedicated writer thread that
/// serializes ALL disk writes: [`set_snapshot`](StateStore::set_snapshot) swaps
/// the whole in-memory [`WolfState`] tree under a brief mutex (when it changed)
/// and wakes the writer, which coalesces, serializes, and atomically replaces
/// the file. All operations are infallible (log-not-fail); clones share the
/// same store. The writer thread exits on its own once every handle is dropped
/// and nothing is pending.
#[derive(Clone)]
pub struct StateStore {
    inner: Arc<StoreInner>,
}

impl StateStore {
    /// Opens the store over `dir`: loads `dir/state.json` (defaults when absent
    /// or unusable, per [`load_state`]) and spawns the `wolf-state-writer`
    /// thread. Never fails: a store whose writer could not spawn still serves
    /// reads and in-memory mutations (writes are logged as lost; `flush` then
    /// times out).
    #[must_use]
    pub fn open(dir: &Path) -> StateStore {
        let state = load_state(dir).unwrap_or_default();
        let inner = Arc::new(StoreInner {
            dir: dir.to_path_buf(),
            cell: Mutex::new(StateCell {
                state,
                generation: 0,
                written: 0,
            }),
            cond: Condvar::new(),
        });
        let writer_inner = Arc::clone(&inner);
        let spawned = std::thread::Builder::new()
            .name("wolf-state-writer".to_string())
            .spawn(move || writer_loop(&writer_inner));
        if let Err(e) = spawned {
            tracing::warn!(
                "wolf-core: could not spawn the state writer thread: {e}; \
                 state changes will not be persisted"
            );
        }
        StateStore { inner }
    }

    /// A clone of the current in-memory truth (mutations are visible here
    /// immediately, ahead of the coalesced disk write).
    #[must_use]
    pub fn state(&self) -> WolfState {
        self.inner.cell.lock().state.clone()
    }

    /// Replaces the whole in-memory tree with `next` — the front-end walks its
    /// live windows/tabs and submits the ENTIRE tree at once.
    ///
    /// Compares `next` against the current tree FIRST: an identical snapshot is
    /// a no-op (the generation is NOT bumped and the writer never re-fires), so
    /// a redundant re-submit — e.g. the KVO/observer storm the coordinator's
    /// coalesced timer already absorbs — causes no disk churn. When it differs,
    /// the tree is swapped in, the generation is bumped, and the writer is woken.
    /// The mutex is held only for the compare-and-swap, never across
    /// serialization or IO.
    pub fn set_snapshot(&self, next: WolfState) {
        let mut cell = self.inner.cell.lock();
        if cell.state == next {
            return;
        }
        cell.state = next;
        cell.generation += 1;
        self.inner.cond.notify_all();
    }

    /// Blocks until every mutation made so far has had its disk write ATTEMPTED
    /// (write errors are logged but still count as attempted, so this cannot
    /// hang on a read-only disk), or until `timeout` elapses. Returns whether
    /// everything pending was flushed in time.
    #[must_use]
    pub fn flush(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut cell = self.inner.cell.lock();
        while cell.written < cell.generation {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            self.inner.cond.wait_for(&mut cell, deadline - now);
        }
        true
    }
}

/// The dedicated writer-thread body (`wolf-state-writer`).
///
/// Waits on the condvar for a pending generation, sleeps
/// [`WRITE_COALESCE_DELAY`] with the lock DROPPED (so a mutation burst merges
/// into one write), snapshots the latest state + target generation under a
/// brief lock, performs the atomic tmp+rename write with NO lock held, then
/// advances `written` and wakes any flusher. While idle it re-checks every
/// [`WRITER_IDLE_POLL`] whether it holds the last `Arc` (every front-end
/// [`StateStore`] dropped) and exits when so.
fn writer_loop(inner: &Arc<StoreInner>) {
    loop {
        {
            let mut cell = inner.cell.lock();
            while cell.generation <= cell.written {
                let timed_out = inner.cond.wait_for(&mut cell, WRITER_IDLE_POLL).timed_out();
                // The writer's own Arc is the only one left: every StateStore
                // handle is gone, nothing is pending, so exit. The pending
                // check (`generation <= written`) must be re-read here: a last
                // handle can mutate and drop between the wait timing out and
                // the lock being reacquired, and that mutation must still be
                // written.
                if timed_out && Arc::strong_count(inner) == 1 && cell.generation <= cell.written {
                    return;
                }
            }
        } // Lock dropped before the coalesce sleep.
        std::thread::sleep(WRITE_COALESCE_DELAY);
        let (state, target) = {
            let cell = inner.cell.lock();
            (cell.state.clone(), cell.generation)
        };
        write_state_file(&inner.dir, &state);
        let mut cell = inner.cell.lock();
        if target > cell.written {
            cell.written = target;
        }
        inner.cond.notify_all();
    }
}

/// Serializes `state` (pretty-printed, with the version header) and atomically
/// replaces `dir/state.json` via a sibling tmp file + rename, creating `dir`
/// first. INFALLIBLE: any error is logged, not returned (a full disk must
/// never take the editor down); the caller advances the written generation
/// regardless so `flush` cannot hang.
fn write_state_file(dir: &Path, state: &WolfState) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::warn!(
            "wolf-core: could not create state dir {}: {e}",
            dir.display()
        );
        return;
    }
    let file = StateFile {
        version: STATE_VERSION,
        windows: state.windows.clone(),
    };
    let json = match serde_json::to_string_pretty(&file) {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!("wolf-core: could not serialize state: {e}");
            return;
        }
    };
    let tmp = dir.join(STATE_TMP_FILE_NAME);
    let path = dir.join(STATE_FILE_NAME);
    if let Err(e) = std::fs::write(&tmp, json) {
        tracing::warn!("wolf-core: could not write {}: {e}", tmp.display());
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        tracing::warn!(
            "wolf-core: could not rename {} over {}: {e}",
            tmp.display(),
            path.display()
        );
        // Best-effort cleanup so a failed rename leaves no residue behind.
        let _ = std::fs::remove_file(&tmp);
    }
}
