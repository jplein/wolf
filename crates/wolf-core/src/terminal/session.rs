//! One terminal session: its emulator state under a lock, its published frame, and the
//! reader and writer threads that move bytes between the pty and the emulator.

use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use wolf_vt::{
    Dirty, Effect, KeyCode, KeyInput, Mods, MouseAction, MouseInput, ScrollViewport, Terminal,
};

use super::pty;
use super::router::{Route, Router, CTRL_W};
use super::{
    TerminalColours, TerminalCommand, TerminalCursor, TerminalFrame, TerminalId,
    TerminalMouseEvent, TerminalMouseKind, TerminalRegistry, TerminalScrollEvent,
    TerminalScrollbar, DEFAULT_CELL_PX, DEFAULT_SIZE, TERMINAL_HANGUP_GRACE,
    TERMINAL_PUBLISH_INTERVAL, TERMINAL_RENDER_HOLD_MAX,
};
use crate::input::{Key, KeyEvent, MouseButton, ScrollDirection};
use crate::redraw::WakeFn;
use crate::render::Rgb;

/// The longest the reader sleeps before it checks whether the shell exited, so a shell whose
/// background job keeps the pty open is still reported.
const EXIT_POLL_CAP: Duration = Duration::from_millis(500);

/// How often the reader checks for the exit once the pty has closed.
const EOF_POLL: Duration = Duration::from_millis(10);

/// How often a hang-up checks whether the shell is gone, within the grace.
const REAP_POLL: Duration = Duration::from_millis(10);

/// How long a writer blocked on a full pty waits before it checks whether to stop.
const WRITER_POLL_MS: libc::c_int = 100;

/// One read from the pty.
const READ_CHUNK: usize = 64 * 1024;

/// How much output the reader feeds before it next considers publishing.
const READ_BUDGET: usize = 1024 * 1024;

/// A running or exited session. Its threads hold an `Arc` of it and a `Weak` to the
/// registry, so nothing they hold keeps the registry alive.
pub(super) struct Session {
    pub(super) id: TerminalId,
    pub(super) command: TerminalCommand,
    /// The basename of the program.
    pub(super) shell: String,
    pid: u32,
    state: Mutex<TermState>,
    frame: RwLock<Arc<TerminalFrame>>,
    /// The writer thread's queue; dropped by the hang-up, which ends the thread.
    writer: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    writer_stop: Arc<AtomicBool>,
    /// The master for `TIOCSWINSZ`, closed by the hang-up under this lock so a resize never
    /// uses a closed descriptor.
    control: Mutex<Option<OwnedFd>>,
    /// The self-pipe that wakes the reader. Both ends live as long as the session, so a
    /// write never meets a closed read end.
    wake_r: OwnedFd,
    wake_w: OwnedFd,
    hangup: AtomicBool,
    removed: AtomicBool,
    exited: AtomicBool,
    reader: Mutex<Option<JoinHandle<()>>>,
    wake: WakeFn,
}

/// What the session lock guards: the emulator and everything derived from it.
struct TermState {
    vt: Terminal,
    /// libghostty-vt's built-in palette, which the theme's 16 colours are laid over.
    base_palette: [wolf_vt::Rgb; 256],
    size: (u16, u16),
    cell_px: (u32, u32),
    version: u64,
    row_versions: Vec<u64>,
    /// Something changed since the last publish.
    dirty: bool,
    /// The next publish stamps every row.
    full: bool,
    /// When the program's synchronized update began, while it defers publishing.
    hold_since: Option<Instant>,
    router: Router,
    /// The cell a selecting drag started from, while the left button is down.
    anchor: Option<(u16, u16)>,
    dragged: bool,
    title: String,
    pwd: Option<PathBuf>,
    exited: Option<i32>,
}

impl Session {
    /// Spawns `command` in `cwd`, publishes the first frame and starts the reader and
    /// writer.
    pub(super) fn start(
        id: TerminalId,
        command: TerminalCommand,
        cwd: &Path,
        colours: &TerminalColours,
        wake: WakeFn,
        registry: Weak<TerminalRegistry>,
    ) -> io::Result<Arc<Session>> {
        let (cols, rows) = DEFAULT_SIZE;
        let mut vt = Terminal::new(cols, rows).map_err(io::Error::other)?;
        vt.resize(cols, rows, DEFAULT_CELL_PX.0, DEFAULT_CELL_PX.1)
            .map_err(io::Error::other)?;
        let base_palette = vt.palette();
        let shell = command
            .program
            .file_name()
            .unwrap_or(command.program.as_os_str())
            .to_string_lossy()
            .into_owned();
        let mut state = TermState {
            vt,
            base_palette,
            size: DEFAULT_SIZE,
            cell_px: DEFAULT_CELL_PX,
            version: 0,
            row_versions: Vec::new(),
            dirty: true,
            full: true,
            hold_since: None,
            router: Router::default(),
            anchor: None,
            dragged: false,
            title: String::new(),
            pwd: None,
            exited: None,
        };
        state.apply_colours(colours);
        let _ = state.vt.take_effects();
        let first = state
            .build_frame(&shell)
            .ok_or_else(|| io::Error::other("libghostty-vt could not read the first frame"))?;
        let (wake_r, wake_w) = pty::pipe()?;

        let spawned = pty::spawn(&command, cwd, DEFAULT_SIZE, DEFAULT_CELL_PX)?;
        let pid = spawned.child.id();
        let child = spawned.child;
        let master = spawned.master;
        let threads = (|| {
            let control = master.try_clone()?;
            let writer_fd = master.try_clone()?;
            let (tx, rx) = mpsc::channel();
            let writer_stop = Arc::new(AtomicBool::new(false));
            let session = Arc::new(Session {
                id,
                command,
                shell,
                pid,
                state: Mutex::new(state),
                frame: RwLock::new(Arc::new(first)),
                writer: Mutex::new(Some(tx)),
                writer_stop: writer_stop.clone(),
                control: Mutex::new(Some(control)),
                wake_r,
                wake_w,
                hangup: AtomicBool::new(false),
                removed: AtomicBool::new(false),
                exited: AtomicBool::new(false),
                reader: Mutex::new(None),
                wake,
            });
            std::thread::Builder::new()
                .name(format!("wolf-term-{}-w", id.0))
                .spawn(move || write_loop(&writer_fd, &rx, &writer_stop))?;
            Ok::<_, io::Error>(session)
        })();
        let session = match threads {
            Ok(session) => session,
            Err(e) => {
                pty::signal_group(pid, libc::SIGKILL);
                return Err(e);
            }
        };
        let reader = Reader {
            session: session.clone(),
            registry,
            child,
            master: Some(master),
            eof: false,
            last_publish: None,
        };
        match std::thread::Builder::new()
            .name(format!("wolf-term-{}", id.0))
            .spawn(move || reader.run())
        {
            Ok(handle) => *session.reader.lock() = Some(handle),
            Err(e) => {
                pty::signal_group(pid, libc::SIGKILL);
                return Err(e);
            }
        }
        Ok(session)
    }

    pub(super) fn frame(&self) -> Arc<TerminalFrame> {
        self.frame.read().clone()
    }

    pub(super) fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    pub(super) fn mark_removed(&self) {
        self.removed.store(true, Ordering::SeqCst);
    }

    /// Asks the reader to hang the shell up; it does so on its own thread.
    pub(super) fn request_hangup(&self) {
        self.hangup.store(true, Ordering::SeqCst);
        self.poke();
    }

    /// Waits for the reader, which returns once the shell is reaped.
    pub(super) fn join_reader(&self) {
        let handle = self.reader.lock().take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }

    /// Routes a key through the `<C-w>` prefix and sends what it encodes. Returns the
    /// split, close or focus a chord asks for, which the registry performs.
    pub(super) fn key(&self, key: KeyEvent) -> Option<Route> {
        let (route, bytes) = {
            let mut state = self.state.lock();
            match state.router.route(&key, Instant::now()) {
                Route::Send(keys) => {
                    let bytes = state.encode_keys(&keys);
                    (None, bytes)
                }
                Route::Hold => (Some(Route::Hold), Vec::new()),
                other => (Some(other), Vec::new()),
            }
        };
        if !bytes.is_empty() {
            self.send_input(bytes);
        }
        match route {
            Some(Route::Hold) => {
                self.poke();
                None
            }
            other => other,
        }
    }

    /// Sends text as typed, after a held `<C-w>`.
    pub(super) fn text(&self, text: &str) {
        let bytes = {
            let mut state = self.state.lock();
            let mut bytes = state.release_prefix();
            bytes.extend_from_slice(text.as_bytes());
            bytes
        };
        self.send_input(bytes);
    }

    /// Pastes text, bracketed when the program set mode 2004.
    pub(super) fn paste(&self, text: &str) {
        let bytes = {
            let mut state = self.state.lock();
            let mut bytes = state.release_prefix();
            let bracketed = state.vt.dec_mode(2004);
            bytes.extend(wolf_vt::paste_encode(text, bracketed));
            bytes
        };
        self.send_input(bytes);
    }

    /// Reports a mouse event to a tracking program unless Shift is held; otherwise a
    /// left-button drag selects and a press clears the selection.
    pub(super) fn mouse(&self, event: TerminalMouseEvent) {
        let bytes = {
            let mut state = self.state.lock();
            if state.vt.mouse_tracking() && !event.shift {
                let input = MouseInput {
                    action: match event.kind {
                        TerminalMouseKind::Press => MouseAction::Press,
                        TerminalMouseKind::Release => MouseAction::Release,
                        TerminalMouseKind::Motion => MouseAction::Motion,
                    },
                    button: event.button.map(|button| match button {
                        MouseButton::Left => wolf_vt::MouseButton::Left,
                        MouseButton::Right => wolf_vt::MouseButton::Right,
                        MouseButton::Middle => wolf_vt::MouseButton::Middle,
                    }),
                    mods: mods(event.shift, event.ctrl, event.alt),
                    x: event.x,
                    y: event.y,
                };
                state.vt.encode_mouse(&input)
            } else {
                state.select(&event);
                Vec::new()
            }
        };
        if bytes.is_empty() {
            self.poke();
        } else {
            self.send(bytes);
        }
    }

    /// Wheel lines: reported to a tracking program, arrows on the alternate screen with mode
    /// 1007, else a scroll through the scrollback.
    pub(super) fn scroll(&self, event: TerminalScrollEvent) {
        let (wheel, arrow, delta) = match event.direction {
            ScrollDirection::Up => (wolf_vt::MouseButton::WheelUp, Key::Up, -1),
            ScrollDirection::Down => (wolf_vt::MouseButton::WheelDown, Key::Down, 1),
            ScrollDirection::Left | ScrollDirection::Right => return,
        };
        let bytes = {
            let mut state = self.state.lock();
            let mut bytes = Vec::new();
            if state.vt.mouse_tracking() {
                let input = MouseInput {
                    action: MouseAction::Press,
                    button: Some(wheel),
                    mods: mods(event.shift, event.ctrl, event.alt),
                    x: event.x,
                    y: event.y,
                };
                for _ in 0..event.lines {
                    bytes.extend(state.vt.encode_mouse(&input));
                }
            } else if state.vt.alternate_screen() && state.vt.dec_mode(1007) {
                let arrow = KeyEvent {
                    key: arrow,
                    ctrl: false,
                    shift: false,
                    alt: false,
                    meta: false,
                };
                for _ in 0..event.lines {
                    bytes.extend(state.encode_keys(&[arrow]));
                }
            } else {
                state.vt.scroll_viewport(ScrollViewport::Delta(
                    delta * isize::try_from(event.lines).unwrap_or(isize::MAX),
                ));
                state.changed_all();
            }
            bytes
        };
        if bytes.is_empty() {
            self.poke();
        } else {
            self.send(bytes);
        }
    }

    /// Reports a focus change to a program that set mode 1004.
    pub(super) fn focus(&self, gained: bool) {
        let bytes = {
            let state = self.state.lock();
            if state.vt.dec_mode(1004) {
                wolf_vt::focus_encode(gained)
            } else {
                Vec::new()
            }
        };
        self.send(bytes);
    }

    /// Resizes the emulator, then the pty. A zero size is ignored, and a zero cell size
    /// keeps the previous one. A size and cell size the session already has do nothing,
    /// neither repainting every row nor sending `SIGWINCH`, because front-ends resend a
    /// size on wakes until the frame shows it.
    pub(super) fn resize(&self, (cols, rows): (u16, u16), cell_px: (u32, u32)) {
        if cols == 0 || rows == 0 {
            return;
        }
        let (bytes, cell_px) = {
            let mut state = self.state.lock();
            let cell_px = if cell_px.0 == 0 || cell_px.1 == 0 {
                state.cell_px
            } else {
                cell_px
            };
            if (state.size, state.cell_px) == ((cols, rows), cell_px) {
                return;
            }
            if state.vt.resize(cols, rows, cell_px.0, cell_px.1).is_err() {
                return;
            }
            state.size = (cols, rows);
            state.cell_px = cell_px;
            state.changed_all();
            (state.absorb_effects(Instant::now()), cell_px)
        };
        self.send(bytes);
        if let Some(master) = self.control.lock().as_ref() {
            if let Err(e) = pty::set_window_size(master, (cols, rows), cell_px) {
                tracing::warn!("resizing terminal {} failed: {e}", self.id.0);
            }
        }
        self.poke();
    }

    pub(super) fn selection_text(&self) -> Option<String> {
        self.state.lock().vt.selection_text()
    }

    pub(super) fn set_colours(&self, colours: &TerminalColours) {
        self.state.lock().apply_colours(colours);
        self.poke();
    }

    /// The directory a split of this session starts in: the reported pwd when it exists,
    /// else the shell process's cwd.
    pub(super) fn shell_cwd(&self) -> Option<PathBuf> {
        let reported = self.state.lock().pwd.clone();
        reported
            .filter(|dir| dir.is_dir())
            .or_else(|| pty::process_cwd(self.pid).filter(|dir| dir.is_dir()))
    }

    /// Sends typed bytes, first snapping the viewport back to the bottom.
    fn send_input(&self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        let snapped = self.state.lock().snap_to_bottom();
        self.send(bytes);
        if snapped {
            self.poke();
        }
    }

    /// Queues bytes for the writer thread.
    fn send(&self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        if let Some(tx) = self.writer.lock().as_ref() {
            let _ = tx.send(bytes);
        }
    }

    /// Wakes the reader, which then publishes what changed and rearms its deadlines.
    fn poke(&self) {
        let byte = 1u8;
        // SAFETY: the write end is open for the session's life and the buffer is one byte. A
        // full pipe is an ignored EAGAIN: the reader is already due to wake.
        unsafe {
            libc::write(
                self.wake_w.as_raw_fd(),
                std::ptr::addr_of!(byte).cast::<libc::c_void>(),
                1,
            );
        }
    }

    fn store(&self, frame: TerminalFrame) {
        *self.frame.write() = Arc::new(frame);
    }
}

impl TermState {
    /// The bytes a list of keys encodes to, from the current modes.
    fn encode_keys(&mut self, keys: &[KeyEvent]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for key in keys {
            bytes.extend(self.vt.encode_key(&key_input(key)));
        }
        bytes
    }

    /// The `^W` of a held prefix, released ahead of text or a paste.
    fn release_prefix(&mut self) -> Vec<u8> {
        if self.router.take_held() {
            self.encode_keys(&[CTRL_W])
        } else {
            Vec::new()
        }
    }

    /// Moves the viewport back to the active area, returning whether it moved.
    fn snap_to_bottom(&mut self) -> bool {
        if self.vt.viewport_at_bottom() {
            return false;
        }
        self.vt.scroll_viewport(ScrollViewport::Bottom);
        self.changed_all();
        true
    }

    fn changed_all(&mut self) {
        self.dirty = true;
        self.full = true;
    }

    /// Lays the editor's colours over the emulator's defaults.
    fn apply_colours(&mut self, colours: &TerminalColours) {
        let fg = colours.fg.map(vt_rgb).or_else(|| self.vt.foreground());
        let bg = colours.bg.map(vt_rgb).or_else(|| self.vt.background());
        if let (Some(fg), Some(bg)) = (fg, bg) {
            let _ = self.vt.set_default_colors(fg, bg);
        }
        self.vt.set_cursor_color(colours.cursor.map(vt_rgb));
        let mut palette = self.base_palette;
        for (entry, colour) in palette.iter_mut().zip(colours.ansi) {
            if let Some(colour) = colour {
                *entry = vt_rgb(colour);
            }
        }
        self.vt.set_palette(Some(&palette));
        self.changed_all();
    }

    /// The selection a mouse event makes without tracking: a left press anchors and clears,
    /// motion with the anchor set selects up to the pointer, a release ends the drag.
    fn select(&mut self, event: &TerminalMouseEvent) {
        let cell = self.cell_at(event.x, event.y);
        let left = event.button == Some(MouseButton::Left);
        match event.kind {
            TerminalMouseKind::Press if left => {
                self.anchor = Some(cell);
                self.dragged = false;
                let _ = self.vt.clear_selection();
            }
            TerminalMouseKind::Motion => {
                let Some(anchor) = self.anchor else { return };
                self.dragged = true;
                let _ = self.vt.select(anchor, cell);
            }
            TerminalMouseKind::Release if left => {
                if let (Some(anchor), true) = (self.anchor.take(), self.dragged) {
                    let _ = self.vt.select(anchor, cell);
                }
                self.dragged = false;
            }
            _ => return,
        }
        self.changed_all();
    }

    /// The viewport cell under a pixel position, clamped to the grid.
    fn cell_at(&self, x: f32, y: f32) -> (u16, u16) {
        let along = |px: f32, cell: u32, cells: u16| -> u16 {
            let index = (px.max(0.0) / cell.max(1) as f32) as u32;
            u16::try_from(index)
                .unwrap_or(u16::MAX)
                .min(cells.saturating_sub(1))
        };
        (
            along(x, self.cell_px.0, self.size.0),
            along(y, self.cell_px.1, self.size.1),
        )
    }

    /// Handles what the emulator asked for during the last mutating call, returning the
    /// bytes to write back to the pty.
    fn absorb_effects(&mut self, now: Instant) -> Vec<u8> {
        let mut out = Vec::new();
        for effect in self.vt.take_effects() {
            match effect {
                Effect::WritePty(bytes) => out.extend_from_slice(&bytes),
                Effect::TitleChanged => {
                    self.title = self.vt.title();
                    self.dirty = true;
                }
                Effect::PwdChanged => {
                    self.pwd = pty::decode_pwd(&self.vt.pwd());
                    self.dirty = true;
                }
                Effect::RenderHold(true) => {
                    self.hold_since.get_or_insert(now);
                }
                Effect::RenderHold(false) => {
                    self.hold_since = None;
                    self.dirty = true;
                }
            }
        }
        out
    }

    /// Reads the emulator into a new frame, stamping the rows that changed with its version.
    fn build_frame(&mut self, shell: &str) -> Option<TerminalFrame> {
        let screen = self.vt.screen().ok()?;
        self.version += 1;
        let version = self.version;
        let rows = usize::from(screen.rows);
        let all = self.full || screen.dirty == Dirty::Full || self.row_versions.len() != rows;
        self.row_versions.resize(rows, version);
        for (row, stamp) in (0..screen.rows).zip(self.row_versions.iter_mut()) {
            if all || screen.row_dirty(row) {
                *stamp = version;
            }
        }
        self.dirty = false;
        self.full = false;

        let mut cells = Vec::with_capacity(rows * usize::from(screen.cols));
        for row in 0..screen.rows {
            for col in 0..screen.cols {
                let cell = screen.cell(col, row).cloned().unwrap_or_default();
                cells.push(super::TerminalCell {
                    text: cell.text,
                    fg: cell.fg.map(core_rgb),
                    bg: cell.bg.map(core_rgb),
                    bold: cell.bold,
                    faint: cell.faint,
                    italic: cell.italic,
                    underline: cell.underline,
                    strikethrough: cell.strikethrough,
                    inverse: cell.inverse,
                    invisible: cell.invisible,
                    blink: cell.blink,
                    width: cell.width,
                });
            }
        }
        let bar = self.vt.scrollbar();
        Some(TerminalFrame {
            version,
            cols: screen.cols,
            rows: screen.rows,
            cells,
            row_versions: self.row_versions.clone(),
            cursor: screen.cursor.map(|cursor| TerminalCursor {
                col: cursor.col,
                row: cursor.row,
                visible: cursor.visible,
                blinking: cursor.blinking,
                style: cursor.style,
            }),
            default_fg: core_rgb(screen.foreground),
            default_bg: core_rgb(screen.background),
            cursor_color: screen.cursor_color.map(core_rgb),
            selection: (0..screen.rows)
                .map(|row| screen.row_selection(row))
                .collect(),
            scrollbar: TerminalScrollbar {
                total: bar.total,
                offset: bar.offset,
                len: bar.len,
            },
            mouse_tracking: self.vt.mouse_tracking(),
            title: self.title.clone(),
            pwd: self.pwd.clone(),
            shell: shell.to_owned(),
            exited: self.exited,
        })
    }
}

/// The reader thread's state: it owns the shell process and the master's read side.
struct Reader {
    session: Arc<Session>,
    registry: Weak<TerminalRegistry>,
    child: Child,
    master: Option<OwnedFd>,
    /// The pty reported end of file, so only the exit is left to wait for.
    eof: bool,
    last_publish: Option<Instant>,
}

impl Reader {
    fn run(mut self) {
        let mut buf = vec![0u8; READ_CHUNK];
        loop {
            if self.session.hangup.load(Ordering::SeqCst) {
                let status = self.hang_up();
                self.finish(status, &mut buf);
                return;
            }
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    self.finish(status, &mut buf);
                    return;
                }
                Ok(None) => {}
                Err(_) => {
                    let status = self.hang_up();
                    self.finish(status, &mut buf);
                    return;
                }
            }
            let next = self.tick();
            self.wait(next, &mut buf);
        }
    }

    /// Flushes a held prefix that timed out and publishes when due, returning when the
    /// reader must next look.
    fn tick(&mut self) -> Instant {
        let now = Instant::now();
        let mut next = now + if self.eof { EOF_POLL } else { EXIT_POLL_CAP };
        let (bytes, frame) = {
            let mut state = self.session.state.lock();
            let mut bytes = Vec::new();
            if state.router.flush_due(now) {
                bytes = state.encode_keys(&[CTRL_W]);
                state.snap_to_bottom();
            }
            if let Some(deadline) = state.router.deadline() {
                next = next.min(deadline);
            }
            let mut frame = None;
            if state.dirty {
                let mut due = true;
                if let Some(since) = state.hold_since {
                    let end = since + TERMINAL_RENDER_HOLD_MAX;
                    if now < end {
                        due = false;
                        next = next.min(end);
                    } else {
                        state.hold_since = None;
                    }
                }
                if let Some(last) = self.last_publish.filter(|_| due) {
                    let at = last + TERMINAL_PUBLISH_INTERVAL;
                    if now < at {
                        due = false;
                        next = next.min(at);
                    }
                }
                if due {
                    frame = state.build_frame(&self.session.shell);
                }
            }
            (bytes, frame)
        };
        self.session.send(bytes);
        if let Some(frame) = frame {
            self.publish(frame);
        }
        next
    }

    fn publish(&mut self, frame: TerminalFrame) {
        self.session.store(frame);
        (self.session.wake)();
        // Taken after the wake, so the next publish waits a full interval after it.
        self.last_publish = Some(Instant::now());
    }

    /// Sleeps until `next`, the pty has output, or another thread pokes the session.
    fn wait(&mut self, next: Instant, buf: &mut [u8]) {
        let remaining = next.saturating_duration_since(Instant::now());
        let timeout =
            libc::c_int::try_from(remaining.as_micros().div_ceil(1000)).unwrap_or(libc::c_int::MAX);
        let master: RawFd = match (&self.master, self.eof) {
            (Some(master), false) => master.as_raw_fd(),
            _ => -1,
        };
        let mut fds = [
            libc::pollfd {
                fd: self.session.wake_r.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: master,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: `fds` holds two valid pollfd entries; a negative fd is ignored.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
        if ready <= 0 {
            return;
        }
        if fds[0].revents != 0 {
            self.drain_wake();
        }
        if fds[1].revents != 0 {
            self.read_output(buf);
        }
    }

    fn drain_wake(&self) {
        let mut sink = [0u8; 64];
        loop {
            // SAFETY: the read end is open for the session's life; `sink` is 64 bytes.
            let n = unsafe {
                libc::read(
                    self.session.wake_r.as_raw_fd(),
                    sink.as_mut_ptr().cast::<libc::c_void>(),
                    sink.len(),
                )
            };
            if n <= 0 {
                return;
            }
        }
    }

    /// Feeds what the pty has, up to [`READ_BUDGET`], marking end of file when it closes.
    fn read_output(&mut self, buf: &mut [u8]) {
        let Some(master) = self.master.as_ref().map(AsRawFd::as_raw_fd) else {
            return;
        };
        let mut fed = 0;
        while fed < READ_BUDGET {
            // SAFETY: `master` is open while `self.master` holds it; `buf` is writable.
            let n =
                unsafe { libc::read(master, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
            if n > 0 {
                let n = n.unsigned_abs();
                let replies = {
                    let mut state = self.session.state.lock();
                    state.vt.feed(&buf[..n]);
                    state.dirty = true;
                    state.absorb_effects(Instant::now())
                };
                self.session.send(replies);
                fed += n;
                continue;
            }
            if n == 0 {
                self.eof = true;
                return;
            }
            match io::Error::last_os_error().kind() {
                io::ErrorKind::WouldBlock => return,
                io::ErrorKind::Interrupted => {}
                // Linux reports a pty whose other side closed as EIO.
                _ => {
                    self.eof = true;
                    return;
                }
            }
        }
    }

    /// `SIGHUP` to the shell's group, the master closed, then `SIGKILL` after the grace.
    fn hang_up(&mut self) -> ExitStatus {
        let pid = self.session.pid;
        pty::signal_group(pid, libc::SIGHUP);
        self.close_master();
        let deadline = Instant::now() + TERMINAL_HANGUP_GRACE;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(status)) => return status,
                Ok(None) => std::thread::sleep(REAP_POLL),
                Err(_) => break,
            }
        }
        pty::signal_group(pid, libc::SIGKILL);
        let _ = self.child.kill();
        self.child
            .wait()
            .unwrap_or_else(|_| ExitStatus::from_raw(libc::SIGKILL))
    }

    /// Closes every copy of the master this session holds and stops the writer.
    fn close_master(&mut self) {
        self.master = None;
        self.session.control.lock().take();
        self.session.writer.lock().take();
        self.session.writer_stop.store(true, Ordering::SeqCst);
    }

    /// Publishes the exit at once, then has the registry close the session's panes.
    fn finish(&mut self, status: ExitStatus, buf: &mut [u8]) {
        if !self.eof {
            self.read_output(buf);
        }
        self.close_master();
        let frame = {
            let mut state = self.session.state.lock();
            state.exited = Some(pty::exit_code(status));
            state.hold_since = None;
            let _ = state.absorb_effects(Instant::now());
            state.build_frame(&self.session.shell)
        };
        self.session.exited.store(true, Ordering::SeqCst);
        if let Some(frame) = frame {
            self.publish(frame);
        }
        if !self.session.removed.load(Ordering::SeqCst) {
            if let Some(registry) = self.registry.upgrade() {
                registry.session_exited(self.session.id);
            }
        }
    }
}

/// The writer thread: every pty write, in order, from the session's queue.
fn write_loop(master: &OwnedFd, rx: &mpsc::Receiver<Vec<u8>>, stop: &AtomicBool) {
    while let Ok(bytes) = rx.recv() {
        let mut rest = &bytes[..];
        while !rest.is_empty() {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            // SAFETY: `master` is open for this thread's life; `rest` is readable.
            let n = unsafe {
                libc::write(
                    master.as_raw_fd(),
                    rest.as_ptr().cast::<libc::c_void>(),
                    rest.len(),
                )
            };
            if n > 0 {
                rest = &rest[n.unsigned_abs()..];
                continue;
            }
            match io::Error::last_os_error().kind() {
                io::ErrorKind::WouldBlock => {
                    let mut fd = libc::pollfd {
                        fd: master.as_raw_fd(),
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    // SAFETY: one valid pollfd.
                    unsafe { libc::poll(&mut fd, 1, WRITER_POLL_MS) };
                }
                io::ErrorKind::Interrupted => {}
                _ => return,
            }
        }
    }
}

/// The encoder's view of a core key press. A character is the US-layout key that types it,
/// with Shift for an uppercase letter.
fn key_input(key: &KeyEvent) -> KeyInput {
    let named = |code| KeyInput {
        code,
        text: String::new(),
        unshifted: None,
        mods: mods(key.shift, key.ctrl, key.alt),
    };
    match key.key {
        Key::Char(c) => KeyInput {
            code: KeyCode::Ascii(c),
            text: c.to_string(),
            unshifted: Some(if c.is_ascii_alphabetic() {
                c.to_ascii_lowercase()
            } else {
                c
            }),
            mods: mods(c.is_ascii_uppercase(), key.ctrl, key.alt),
        },
        Key::Enter => named(KeyCode::Enter),
        Key::Esc => named(KeyCode::Escape),
        Key::Tab => named(KeyCode::Tab),
        Key::Backspace => named(KeyCode::Backspace),
        Key::Delete => named(KeyCode::Delete),
        Key::Left => named(KeyCode::Left),
        Key::Right => named(KeyCode::Right),
        Key::Up => named(KeyCode::Up),
        Key::Down => named(KeyCode::Down),
        Key::Home => named(KeyCode::Home),
        Key::End => named(KeyCode::End),
        Key::PageUp => named(KeyCode::PageUp),
        Key::PageDown => named(KeyCode::PageDown),
        Key::F(n) => named(KeyCode::F(n)),
    }
}

fn mods(shift: bool, ctrl: bool, alt: bool) -> Mods {
    Mods {
        shift,
        ctrl,
        alt,
        super_key: false,
    }
}

fn vt_rgb(colour: Rgb) -> wolf_vt::Rgb {
    wolf_vt::Rgb {
        r: colour.r,
        g: colour.g,
        b: colour.b,
    }
}

fn core_rgb(colour: wolf_vt::Rgb) -> Rgb {
    Rgb {
        r: colour.r,
        g: colour.g,
        b: colour.b,
    }
}
