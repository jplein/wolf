//! Terminal sessions owned by core: a pty, a shell, a libghostty-vt emulator and the frame
//! each publishes, named by [`TerminalId`] and shown in Neovim placeholder windows.
//!
//! `specs/terminal-panes.md` is authoritative for what a session is and does;
//! [`crate::api::WolfCore`] carries the session API this module backs.

mod pty;
mod router;
mod session;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use parking_lot::{Mutex, RwLock};

use crate::files::SearchRegistry;
use crate::input::{KeyEvent, MouseButton, Nvim, ScrollDirection};
use crate::layout::{LayoutIntent, SplitOrientation};
use crate::pump::PumpTarget;
use crate::redraw::{IntentQueue, TrackedCwd, WakeFn};
use crate::render::{Rgb, SharedSnapshot};

use router::Route;
use session::Session;

pub use wolf_vt::{
    CellWidth as TerminalCellWidth, CursorStyle as TerminalCursorStyle,
    Underline as TerminalUnderline,
};

/// The shortest time between two publishes of one session's frame; the exit publish is
/// exempt.
pub const TERMINAL_PUBLISH_INTERVAL: Duration = Duration::from_millis(8);

/// The longest a synchronized update (mode 2026) defers publishing before the hold counts
/// as released.
pub const TERMINAL_RENDER_HOLD_MAX: Duration = Duration::from_secs(1);

/// How long a held `<C-w>` waits for the key after it before it is sent on its own, as
/// Neovim's `timeoutlen`.
pub const TERMINAL_PREFIX_TIMEOUT: Duration = Duration::from_secs(1);

/// How long a hung-up shell has between `SIGHUP` and `SIGKILL`.
pub const TERMINAL_HANGUP_GRACE: Duration = Duration::from_secs(1);

/// A session's size before the first `resize_terminal`.
const DEFAULT_SIZE: (u16, u16) = (80, 24);

/// The cell size in pixels before the first `resize_terminal`.
const DEFAULT_CELL_PX: (u32, u32) = (8, 16);

/// The variable a front-end reads Wolf's terminfo directory from; packaging sets it as a
/// default, so a developer's export wins.
pub const TERMINFO_DIR_ENV: &str = "WOLF_TERMINFO_DIR";

/// The non-empty value of [`TERMINFO_DIR_ENV`], for [`crate::CoreConfig::terminfo_dir`].
pub fn terminfo_dir_from_env() -> Option<PathBuf> {
    std::env::var_os(TERMINFO_DIR_ENV)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

/// Names a terminal session: allocated from 1 per core, never reused, and stable across
/// Neovim restarts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalId(pub u64);

/// Where a session's placeholder goes. A `win` of `0` is Neovim's current window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalTarget {
    /// Show the session in `win`, replacing its buffer.
    Window(i64),
    /// Split `win` and show the session in the new window, reported as a `Split` intent.
    Split {
        win: i64,
        orientation: SplitOrientation,
    },
}

/// The program a session runs. `None` where one is taken means the user's shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// One cell of a [`TerminalFrame`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalCell {
    /// The grapheme cluster, `""` for an empty cell or a wide character's spacer.
    pub text: String,
    /// The resolved foreground, `None` for the frame's default. Not swapped for `inverse`.
    pub fg: Option<Rgb>,
    /// The resolved background, as `fg`.
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub faint: bool,
    pub italic: bool,
    pub underline: TerminalUnderline,
    pub strikethrough: bool,
    /// The painter swaps the foreground and background.
    pub inverse: bool,
    /// The painter draws no glyph.
    pub invisible: bool,
    pub blink: bool,
    pub width: TerminalCellWidth,
}

/// The cursor of a [`TerminalFrame`], in viewport cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalCursor {
    pub col: u16,
    pub row: u16,
    pub visible: bool,
    pub blinking: bool,
    pub style: TerminalCursorStyle,
}

/// The scrollable area of a [`TerminalFrame`], in rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalScrollbar {
    pub total: u64,
    /// The viewport's first row; `total - len` at the bottom.
    pub offset: u64,
    pub len: u64,
}

/// One published picture of a session, which [`crate::WolfCore::terminal_frame`] returns.
/// A front-end repaints the rows whose `row_versions` entry is newer than the version it
/// last painted.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalFrame {
    /// Monotonic per session.
    pub version: u64,
    pub cols: u16,
    pub rows: u16,
    /// Row-major, `cols * rows`.
    pub cells: Vec<TerminalCell>,
    /// Per row, the frame version at which the row last changed.
    pub row_versions: Vec<u64>,
    /// `None` while the cursor is scrolled out of view.
    pub cursor: Option<TerminalCursor>,
    pub default_fg: Rgb,
    pub default_bg: Rgb,
    pub cursor_color: Option<Rgb>,
    /// Per row, the selected columns, inclusive.
    pub selection: Vec<Option<(u16, u16)>>,
    pub scrollbar: TerminalScrollbar,
    /// Whether the program tracks the mouse, so clicks go to it rather than selecting.
    pub mouse_tracking: bool,
    pub title: String,
    /// The directory the shell last reported, decoded.
    pub pwd: Option<PathBuf>,
    /// The basename of the session's program, such as `zsh`.
    pub shell: String,
    /// The exit code, or 128 plus the signal for a signal death; `None` while running.
    pub exited: Option<i32>,
}

impl TerminalFrame {
    /// The cell at `col`, `row`, or `None` outside the frame.
    #[must_use]
    pub fn cell(&self, col: u16, row: u16) -> Option<&TerminalCell> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        self.cells
            .get(usize::from(row) * usize::from(self.cols) + usize::from(col))
    }

    /// Row `row`'s text, an empty cell or a spacer as one space, trailing spaces trimmed.
    #[must_use]
    pub fn row_text(&self, row: u16) -> String {
        if row >= self.rows {
            return String::new();
        }
        let mut text: String = (0..self.cols)
            .filter_map(|col| self.cell(col, row))
            .map(|cell| {
                if cell.text.is_empty() {
                    " "
                } else {
                    cell.text.as_str()
                }
            })
            .collect();
        text.truncate(text.trim_end_matches(' ').len());
        text
    }
}

/// What a mouse event does, for [`TerminalMouseEvent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalMouseKind {
    Press,
    Release,
    Motion,
}

/// A mouse event over a terminal pane, in pixels from the top-left of its content area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalMouseEvent {
    pub kind: TerminalMouseKind,
    /// `None` for motion with no button held.
    pub button: Option<MouseButton>,
    pub x: f32,
    pub y: f32,
    /// Bypasses the program's mouse tracking, so a drag selects.
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// A wheel scroll over a terminal pane, at a pixel position as [`TerminalMouseEvent`].
/// `Left` and `Right` are ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalScrollEvent {
    pub direction: ScrollDirection,
    pub lines: u16,
    pub x: f32,
    pub y: f32,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// The colours sessions take from the editor: its default foreground and background, the
/// `WolfCursor` background, and `g:terminal_color_0` to `_15`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TerminalColours {
    pub(crate) fg: Option<Rgb>,
    pub(crate) bg: Option<Rgb>,
    pub(crate) cursor: Option<Rgb>,
    pub(crate) ansi: [Option<Rgb>; 16],
}

/// Every session of one core, and what the sessions need from the rest of it.
pub(crate) struct TerminalRegistry {
    me: Weak<TerminalRegistry>,
    sessions: Mutex<BTreeMap<TerminalId, Arc<Session>>>,
    /// The drawer's session, which may have exited; see [`drawer`](Self::drawer).
    drawer: Mutex<Option<TerminalId>>,
    next_id: AtomicU64,
    colours: Mutex<TerminalColours>,
    shutting_down: AtomicBool,
    wake: WakeFn,
    runtime: tokio::runtime::Handle,
    snapshot: SharedSnapshot,
    intents: IntentQueue,
    search_registry: Arc<SearchRegistry>,
    tracked_cwd: TrackedCwd,
    fallback_cwd: PathBuf,
    terminfo_dir: Option<PathBuf>,
    /// The pump's current Neovim handle, which a restart swaps; set once after spawn.
    rpc: OnceLock<Arc<RwLock<PumpTarget>>>,
}

/// What [`TerminalRegistry::new`] takes from the core.
pub(crate) struct RegistryParts {
    pub(crate) wake: WakeFn,
    pub(crate) runtime: tokio::runtime::Handle,
    pub(crate) snapshot: SharedSnapshot,
    pub(crate) intents: IntentQueue,
    pub(crate) search_registry: Arc<SearchRegistry>,
    pub(crate) tracked_cwd: TrackedCwd,
    pub(crate) fallback_cwd: PathBuf,
    pub(crate) terminfo_dir: Option<PathBuf>,
}

impl TerminalRegistry {
    pub(crate) fn new(parts: RegistryParts) -> Arc<Self> {
        Arc::new_cyclic(|me| TerminalRegistry {
            me: me.clone(),
            sessions: Mutex::new(BTreeMap::new()),
            drawer: Mutex::new(None),
            next_id: AtomicU64::new(1),
            colours: Mutex::new(TerminalColours::default()),
            shutting_down: AtomicBool::new(false),
            wake: parts.wake,
            runtime: parts.runtime,
            snapshot: parts.snapshot,
            intents: parts.intents,
            search_registry: parts.search_registry,
            tracked_cwd: parts.tracked_cwd,
            fallback_cwd: parts.fallback_cwd,
            terminfo_dir: parts.terminfo_dir,
            rpc: OnceLock::new(),
        })
    }

    /// Gives the registry the pump's handle cell, which its tasks call Neovim through.
    pub(crate) fn attach_rpc(&self, target: Arc<RwLock<PumpTarget>>) {
        let _ = self.rpc.set(target);
    }

    /// The current Neovim handle, `None` before [`attach_rpc`](Self::attach_rpc).
    fn nvim(&self) -> Option<Nvim> {
        self.rpc.get().map(|target| target.read().nvim.clone())
    }

    /// The cwd a session opened beside window `tab`'s tabpage starts in: that tabpage's cwd,
    /// else Neovim's live cwd, else the core's configured one.
    pub(crate) fn tab_cwd(&self, tab: Option<i64>) -> PathBuf {
        tab.and_then(|tab| self.snapshot.read().tab_cwds.get(&tab).cloned())
            .or_else(|| self.tracked_cwd.read().clone())
            .unwrap_or_else(|| self.fallback_cwd.clone())
    }

    /// Spawns `command` (the user's shell for `None`) in `cwd` and registers the session,
    /// its first frame published. Fails when the pty or the program cannot be started.
    pub(crate) fn spawn(
        &self,
        command: Option<TerminalCommand>,
        cwd: &Path,
    ) -> std::io::Result<TerminalId> {
        let id = TerminalId(self.next_id.fetch_add(1, Ordering::SeqCst));
        let command = command.unwrap_or_else(pty::default_command);
        let colours = *self.colours.lock();
        let session = Session::start(
            id,
            command,
            cwd,
            self.terminfo_dir.as_deref(),
            &colours,
            self.wake.clone(),
            self.me.clone(),
        )?;
        self.sessions.lock().insert(id, session);
        Ok(id)
    }

    /// The drawer's running session, or a new one of the user's shell in tabpage `tab`'s
    /// cwd when it has none. Spawning is `openpty` and fork/exec, never an RPC.
    pub(crate) fn drawer_session(&self, tab: Option<i64>) -> std::io::Result<TerminalId> {
        let mut drawer = self.drawer.lock();
        if let Some(id) = drawer.filter(|id| self.is_live(*id)) {
            return Ok(id);
        }
        let id = self.spawn(None, &self.tab_cwd(tab))?;
        *drawer = Some(id);
        Ok(id)
    }

    /// The drawer's session while it runs.
    pub(crate) fn drawer(&self) -> Option<TerminalId> {
        let drawer = *self.drawer.lock();
        drawer.filter(|id| self.is_live(*id))
    }

    /// The directory session `id`'s shell is in: its `OSC 7` report, else the operating
    /// system's view. `None` for an unknown session or an unreadable cwd.
    pub(crate) fn shell_cwd(&self, id: TerminalId) -> Option<PathBuf> {
        self.get(id)?.shell_cwd()
    }

    fn get(&self, id: TerminalId) -> Option<Arc<Session>> {
        self.sessions.lock().get(&id).cloned()
    }

    pub(crate) fn frame(&self, id: TerminalId) -> Option<Arc<TerminalFrame>> {
        self.get(id).map(|session| session.frame())
    }

    /// Whether `id` names a running session.
    pub(crate) fn is_live(&self, id: TerminalId) -> bool {
        self.get(id).is_some_and(|session| !session.has_exited())
    }

    /// The running sessions, ascending.
    pub(crate) fn live(&self) -> Vec<TerminalId> {
        let sessions: Vec<Arc<Session>> = self.sessions.lock().values().cloned().collect();
        sessions
            .into_iter()
            .filter(|session| !session.has_exited())
            .map(|session| session.id)
            .collect()
    }

    /// The session's shell name, for a terminal pane's status bar.
    pub(crate) fn shell_name(&self, id: TerminalId) -> Option<String> {
        self.get(id).map(|session| session.shell.clone())
    }

    pub(crate) fn key(&self, id: TerminalId, key: KeyEvent) {
        let Some(session) = self.get(id) else { return };
        self.dismiss_toast();
        match session.key(key) {
            Some(Route::Split(orientation)) => self.router_split(session, orientation),
            Some(Route::Close) => self.router_close(id),
            Some(Route::Focus(dir)) => {
                if let Some(from) = self.placeholder_win(id) {
                    self.intents
                        .lock()
                        .push_back(LayoutIntent::FocusDir { dir, from });
                    (self.wake)();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn text(&self, id: TerminalId, text: &str) {
        if let Some(session) = self.get(id) {
            self.dismiss_toast();
            session.text(text);
        }
    }

    pub(crate) fn paste(&self, id: TerminalId, text: &str) {
        if let Some(session) = self.get(id) {
            self.dismiss_toast();
            session.paste(text);
        }
    }

    pub(crate) fn mouse(&self, id: TerminalId, event: TerminalMouseEvent) {
        if let Some(session) = self.get(id) {
            if event.kind == TerminalMouseKind::Press {
                self.dismiss_toast();
            }
            session.mouse(event);
        }
    }

    pub(crate) fn scroll(&self, id: TerminalId, event: TerminalScrollEvent) {
        if let Some(session) = self.get(id) {
            self.dismiss_toast();
            session.scroll(event);
        }
    }

    /// Reports focus to the program, and on a loss forces a path-less file-tree rescan:
    /// every exit from a terminal pane passes a focus loss, and its shell may have
    /// changed files.
    pub(crate) fn focus(&self, id: TerminalId, gained: bool) {
        if let Some(session) = self.get(id) {
            session.focus(gained);
        }
        if !gained {
            self.search_registry.notify_fs_changed(PathBuf::new(), true);
        }
    }

    pub(crate) fn resize(&self, id: TerminalId, size: (u16, u16), cell_px: (u32, u32)) {
        if let Some(session) = self.get(id) {
            session.resize(size, cell_px);
        }
    }

    pub(crate) fn selection_text(&self, id: TerminalId) -> Option<String> {
        self.get(id)?.selection_text()
    }

    /// Hangs a running session up, after which its exit closes its panes; removes an exited
    /// one.
    pub(crate) fn close(&self, id: TerminalId) {
        let Some(session) = self.get(id) else { return };
        if session.has_exited() {
            self.remove(id);
        } else {
            session.request_hangup();
        }
    }

    /// Takes a session out of the registry, hanging it up if it still runs.
    pub(crate) fn remove(&self, id: TerminalId) {
        let removed = self.sessions.lock().remove(&id);
        if let Some(session) = removed {
            session.mark_removed();
            if !session.has_exited() {
                session.request_hangup();
            }
        }
    }

    /// Neovim wiped a session's placeholder buffer.
    pub(crate) fn on_wiped(&self, id: TerminalId) {
        self.remove(id);
    }

    /// The editor's default colours changed; every session and every later one takes them.
    pub(crate) fn set_default_colours(&self, fg: Option<Rgb>, bg: Option<Rgb>) {
        let colours = {
            let mut colours = self.colours.lock();
            if (colours.fg, colours.bg) == (fg, bg) {
                return;
            }
            colours.fg = fg;
            colours.bg = bg;
            *colours
        };
        self.push_colours(&colours);
    }

    /// The theme's cursor and `ANSI` colours changed.
    pub(crate) fn set_theme_colours(&self, cursor: Option<Rgb>, ansi: [Option<Rgb>; 16]) {
        let colours = {
            let mut colours = self.colours.lock();
            if (colours.cursor, colours.ansi) == (cursor, ansi) {
                return;
            }
            colours.cursor = cursor;
            colours.ansi = ansi;
            *colours
        };
        self.push_colours(&colours);
    }

    fn push_colours(&self, colours: &TerminalColours) {
        let sessions: Vec<Arc<Session>> = self.sessions.lock().values().cloned().collect();
        for session in sessions {
            session.set_colours(colours);
        }
    }

    /// Hangs every session up and returns once every shell is reaped.
    pub(crate) fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let sessions: Vec<Arc<Session>> = std::mem::take(&mut *self.sessions.lock())
            .into_values()
            .collect();
        for session in &sessions {
            session.mark_removed();
            session.request_hangup();
        }
        for session in &sessions {
            session.join_reader();
        }
    }

    /// The window showing session `id`: the active window if it does, else one on screen,
    /// else any. `None` when no window shows it.
    fn placeholder_win(&self, id: TerminalId) -> Option<i64> {
        let snap = self.snapshot.read();
        let mut wins: Vec<i64> = snap
            .win_buffers
            .iter()
            .filter(|(_, buffer)| buffer.terminal == Some(id))
            .map(|(win, _)| *win)
            .collect();
        wins.sort_unstable();
        if let Some(active) = snap.active_win.filter(|active| wins.contains(active)) {
            return Some(active);
        }
        let on_screen = wins.iter().copied().find(|win| {
            snap.windows
                .values()
                .any(|info| info.win == *win && !info.hidden)
        });
        on_screen.or_else(|| wins.first().copied())
    }

    /// A typed `:terminal`: a new session of the user's shell in window `win`'s tabpage
    /// cwd, shown in `win` in place of its buffer. Runs on the runtime, as
    /// [`router_split`](Self::router_split) does.
    pub(crate) fn open_in_window(&self, win: i64) {
        let (Some(registry), Some(nvim)) = (self.me.upgrade(), self.nvim()) else {
            return;
        };
        self.runtime.spawn(async move {
            let tab = crate::nvim::win_tabpage(nvim.clone(), win).await.ok();
            let cwd = registry.tab_cwd(tab);
            let id = match registry.spawn(None, &cwd) {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!("`:terminal` could not start a session: {e}");
                    return;
                }
            };
            match crate::nvim::terminal_bind(nvim, id.0, win, "").await {
                Ok(bound) if bound != 0 => {}
                _ => registry.remove(id),
            }
        });
    }

    /// Dismisses a showing toast that is not a prompt, on the runtime: the calling front-end
    /// thread may hold a snapshot read guard, which a blocking write here would deadlock on.
    fn dismiss_toast(&self) {
        let snapshot = self.snapshot.clone_handle();
        let wake = self.wake.clone();
        self.runtime.spawn(async move {
            crate::pump::dismiss_toast_and_wake(&snapshot, &wake).await;
        });
    }

    /// `<C-w>s` or `<C-w>v`: a new session running the origin's program again, in the
    /// origin shell's directory, shown in a split of the origin's pane. Ignored in the
    /// drawer, which the front-ends keep outside their pane trees.
    fn router_split(&self, origin: Arc<Session>, orientation: SplitOrientation) {
        if *self.drawer.lock() == Some(origin.id) {
            return;
        }
        let (Some(registry), Some(win), Some(nvim)) = (
            self.me.upgrade(),
            self.placeholder_win(origin.id),
            self.nvim(),
        ) else {
            return;
        };
        self.runtime.spawn(async move {
            let cwd = match origin.shell_cwd() {
                Some(cwd) => cwd,
                None => {
                    let tab = crate::nvim::win_tabpage(nvim.clone(), win).await.ok();
                    registry.tab_cwd(tab)
                }
            };
            let Ok(id) = registry.spawn(Some(origin.command.clone()), &cwd) else {
                return;
            };
            let split = match orientation {
                SplitOrientation::Horizontal => "h",
                SplitOrientation::Vertical => "v",
            };
            match crate::nvim::terminal_bind(nvim, id.0, win, split).await {
                Ok(bound) if bound != 0 => {}
                _ => registry.remove(id),
            }
        });
    }

    /// `<C-w>c` or `<C-w>q`: closes the session's pane unless it is its tabpage's last.
    fn router_close(&self, id: TerminalId) {
        let (Some(win), Some(nvim)) = (self.placeholder_win(id), self.nvim()) else {
            return;
        };
        self.runtime.spawn(async move {
            if let Err(e) = crate::nvim::terminal_close(nvim, win).await {
                tracing::warn!("closing terminal pane {win} failed: {e}");
            }
        });
    }

    /// A session's shell exited: closes every window showing it, or removes the session
    /// when none does or Neovim cannot be reached.
    fn session_exited(&self, id: TerminalId) {
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        let (Some(registry), Some(nvim)) = (self.me.upgrade(), self.nvim()) else {
            self.remove(id);
            return;
        };
        self.runtime.spawn(async move {
            match crate::nvim::terminal_exited(nvim, id.0).await {
                Ok(closed) if closed > 0 => {}
                _ => registry.remove(id),
            }
        });
    }
}

impl Drop for TerminalRegistry {
    fn drop(&mut self) {
        for session in self.sessions.get_mut().values() {
            session.mark_removed();
            session.request_hangup();
        }
    }
}
