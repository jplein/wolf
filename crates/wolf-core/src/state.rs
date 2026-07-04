//! Application-state persistence (`state.json`) + platform state-dir resolution.
//!
//! This is the GUI-toolkit-free state module: it resolves the per-platform
//! application-state directory (macOS `~/Library/Application Support/Wolf`,
//! Linux `~/.local/state/wolf`), reads a pretty-printed `state.json` holding a
//! LIST of per-window entries (geometry, cwd, sidebar visibility, optional
//! toolbar display mode), and writes it back through a single dedicated writer
//! thread.
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
//! A [`StateStore`] merges every mutation into an in-memory [`WolfState`]
//! under a mutex and bumps a generation counter; a dedicated writer thread
//! (named `wolf-state-writer`) sleeps a short coalesce delay, clones the
//! LATEST state, serializes with `serde_json::to_string_pretty`, writes a
//! sibling tmp file, and renames it over `state.json` — so a crash never
//! leaves a partial file and rapid mutation bursts produce one write.
//! [`StateStore::flush`] blocks until everything pending has been attempted.
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

/// The macOS toolbar display mode (icon + text / icon only / text only).
///
/// A macOS-only facet carried in the shared schema as an OPTIONAL per-window
/// field (`toolbar_display_mode`, omitted when unset) so other front-ends
/// (GNOME) can ignore it gracefully. Serialized in snake_case
/// (`"icon_and_text"` / `"icon_only"` / `"text_only"`); the variant ORDER is
/// the cross-language discriminant contract mirrored by wolf-ffi's
/// `mod toolbar_mode` and the Swift `StateSeam.ToolbarMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarMode {
    /// Toolbar items show their icon with the label beneath it.
    IconAndText,
    /// Toolbar items show only their icon.
    IconOnly,
    /// Toolbar items show only their text label.
    TextOnly,
}

/// One window's persisted state.
///
/// Geometry is in `f64` points, interpreted by the WRITING front-end (macOS
/// stores `NSWindow.frame`: AppKit bottom-left global coordinates). A
/// non-positive `width`/`height` is the "unset" sentinel — restore treats the
/// geometry as absent. `cwd` is the absolute canonicalized path nvim reported,
/// or `""` when it has not been reported yet; restore validates it is a
/// non-empty, existing directory before use.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WindowState {
    /// The window origin's x coordinate, in points.
    pub x: f64,
    /// The window origin's y coordinate, in points.
    pub y: f64,
    /// The window width, in points (`<= 0` == unset).
    pub width: f64,
    /// The window height, in points (`<= 0` == unset).
    pub height: f64,
    /// The embedded Neovim's working directory (`""` == not yet reported).
    pub cwd: String,
    /// Whether the file-tree sidebar is visible (defaults to `true`).
    pub sidebar_visible: bool,
    /// The macOS toolbar display mode; `None` (omitted on disk) when unset or
    /// on front-ends without the concept (GNOME).
    #[serde(
        rename = "toolbar_display_mode",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub toolbar_mode: Option<ToolbarMode>,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            cwd: String::new(),
            sidebar_visible: true,
            toolbar_mode: None,
        }
    }
}

/// The whole persisted application state: a LIST of windows (future-proof for
/// multi-window; the current front-ends only populate index 0).
#[derive(Clone, Debug, Default)]
pub struct WolfState {
    /// The per-window entries, in window order.
    pub windows: Vec<WindowState>,
}

/// The on-disk file shape: a `version` header wrapping the window list.
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
/// serializes ALL disk writes: every setter merges into the in-memory
/// [`WolfState`] under a brief mutex and wakes the writer, which coalesces,
/// serializes, and atomically replaces the file. All operations are infallible
/// (log-not-fail); clones share the same store. The writer thread exits on its
/// own once every handle is dropped and nothing is pending.
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

    /// Records window `idx`'s frame geometry (points; the writing front-end's
    /// coordinate convention). Grows the window list with defaults up to `idx`.
    pub fn set_geometry(&self, idx: usize, x: f64, y: f64, width: f64, height: f64) {
        self.mutate(idx, |w| {
            let changed = (w.x, w.y, w.width, w.height) != (x, y, width, height);
            w.x = x;
            w.y = y;
            w.width = width;
            w.height = height;
            changed
        });
    }

    /// Records window `idx`'s working directory (stored lossily as a UTF-8
    /// string). Grows the window list with defaults up to `idx`.
    pub fn set_cwd(&self, idx: usize, cwd: &Path) {
        let cwd = cwd.to_string_lossy().into_owned();
        self.mutate(idx, |w| {
            let changed = w.cwd != cwd;
            w.cwd = cwd;
            changed
        });
    }

    /// Records window `idx`'s sidebar visibility. Grows the window list with
    /// defaults up to `idx`.
    pub fn set_sidebar_visible(&self, idx: usize, visible: bool) {
        self.mutate(idx, |w| {
            let changed = w.sidebar_visible != visible;
            w.sidebar_visible = visible;
            changed
        });
    }

    /// Records window `idx`'s toolbar display mode (`None` clears it — the
    /// on-disk field is then omitted). Grows the window list with defaults up
    /// to `idx`.
    pub fn set_toolbar_mode(&self, idx: usize, mode: Option<ToolbarMode>) {
        self.mutate(idx, |w| {
            let changed = w.toolbar_mode != mode;
            w.toolbar_mode = mode;
            changed
        });
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

    /// Truncates the `windows` list to `len` entries, dropping any trailing
    /// (higher-index) slots. Used at terminate time to prune ghost windows left
    /// by closed windows once the surviving windows have been re-pushed at dense
    /// indices. A no-op when the list is already `<= len`; otherwise bumps the
    /// generation and wakes the writer so the shrink reaches disk. The mutex is
    /// held only for the in-memory truncation, never across serialization or IO.
    pub fn truncate_windows(&self, len: usize) {
        let mut cell = self.inner.cell.lock();
        if cell.state.windows.len() <= len {
            return;
        }
        cell.state.windows.truncate(len);
        cell.generation += 1;
        self.inner.cond.notify_all();
    }

    /// The shared setter body: under the cell mutex, grows `windows` with
    /// defaults up to `idx`, applies `apply` (which reports whether the field
    /// actually changed), and — when the list grew or the field changed — bumps
    /// the generation and wakes the writer. Unchanged writes are skipped so
    /// repeated identical pushes (e.g. KVO echoes) cause no disk churn. The
    /// mutex is held only for the in-memory merge, never across serialization
    /// or IO.
    fn mutate(&self, idx: usize, apply: impl FnOnce(&mut WindowState) -> bool) {
        let mut cell = self.inner.cell.lock();
        let grew = cell.state.windows.len() <= idx;
        if grew {
            cell.state
                .windows
                .resize_with(idx + 1, WindowState::default);
        }
        let changed = apply(&mut cell.state.windows[idx]);
        if grew || changed {
            cell.generation += 1;
            self.inner.cond.notify_all();
        }
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
                let timed_out = inner
                    .cond
                    .wait_for(&mut cell, WRITER_IDLE_POLL)
                    .timed_out();
                // The writer's own Arc is the only one left: every StateStore
                // handle is gone, nothing is pending, so exit. The pending
                // check (`generation <= written`) must be re-read here: a last
                // handle can mutate and drop between the wait timing out and
                // the lock being reacquired, and that mutation must still be
                // written.
                if timed_out
                    && Arc::strong_count(inner) == 1
                    && cell.generation <= cell.written
                {
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
