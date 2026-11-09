//! A safe binding over libghostty-vt, Ghostty's terminal-emulation library: program
//! output goes in as bytes, and the screen comes back out as cells, a cursor and colours.
//! Input goes the other way, through the key, mouse, paste and focus encoders.
//!
//! The crate is GUI-free: libghostty-vt parses VT sequences and keeps screen state, and
//! draws nothing. It holds every `unsafe` line and all the link wiring between Wolf and
//! the library. How the library is pinned, built and linked on each platform is in
//! `specs/terminal-panes.md#building-libghostty-vt`.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::fmt;
use std::mem;
use std::ptr;

mod ffi;

/// The linked libghostty-vt's version, such as `"1.0.0"`, read at run time from
/// `ghostty_version()`. A development build carries a suffix such as `"-dev+abc1234"`.
pub fn version() -> String {
    // SAFETY: ghostty_version returns a static NUL-terminated string the library owns.
    unsafe { CStr::from_ptr(ffi::ghostty_version()) }
        .to_string_lossy()
        .into_owned()
}

/// A 24-bit colour, laid out as libghostty-vt's `GhosttyColorRgb`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<ffi::GhosttyColorRgb> for Rgb {
    fn from(c: ffi::GhosttyColorRgb) -> Self {
        Rgb {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

impl From<Rgb> for ffi::GhosttyColorRgb {
    fn from(c: Rgb) -> Self {
        ffi::GhosttyColorRgb {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

/// The cursor, 0-based in the active area. `visible` is DEC mode 25.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub col: u16,
    pub row: u16,
    pub visible: bool,
}

/// A cell's underline style, from SGR 4 and its `4:n` sub-parameters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// How a cell takes part in a wide character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CellWidth {
    /// An ordinary one-column cell.
    #[default]
    Narrow,
    /// The first column of a two-column character; it holds the grapheme.
    Wide,
    /// The second column of a wide character; it holds no text and is not painted.
    SpacerTail,
    /// The last column of a soft-wrapped row whose wide character moved to the next row.
    SpacerHead,
}

/// The cursor's shape, from `DECSCUSR` (`CSI Ps SP q`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CursorStyle {
    #[default]
    Block,
    Bar,
    Underline,
    BlockHollow,
}

/// What changed since the previous [`Terminal::screen`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Dirty {
    /// Nothing changed.
    #[default]
    Clean,
    /// Some rows changed; [`Screen::row_dirty`] names them.
    Partial,
    /// Global state changed, such as a resize or a colour; every row needs repainting.
    Full,
}

/// One cell of the viewport.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// The cell's grapheme cluster: `""` for an empty cell or a wide character's spacer.
    pub text: String,
    /// The resolved foreground, palette indices looked up; `None` when the cell has no
    /// colour of its own and takes the terminal's default. Not swapped for `inverse`.
    pub fg: Option<Rgb>,
    /// The resolved background, as `fg`.
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub faint: bool,
    pub italic: bool,
    pub strikethrough: bool,
    /// SGR 7: the painter swaps the foreground and background.
    pub inverse: bool,
    /// SGR 8: the painter draws no glyph.
    pub invisible: bool,
    pub blink: bool,
    pub underline: Underline,
    pub width: CellWidth,
}

/// The cursor as the viewport shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenCursor {
    /// 0-based viewport column.
    pub col: u16,
    /// 0-based viewport row.
    pub row: u16,
    /// DEC mode 25.
    pub visible: bool,
    pub blinking: bool,
    pub style: CursorStyle,
}

/// The viewport's cells at one moment, row-major, with what changed since the previous
/// [`Terminal::screen`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub cols: u16,
    pub rows: u16,
    /// What changed since the previous [`Terminal::screen`].
    pub dirty: Dirty,
    /// The cursor, or `None` while it is outside the viewport (scrolled into history).
    pub cursor: Option<ScreenCursor>,
    /// The default foreground cells without a colour of their own take.
    pub foreground: Rgb,
    /// The default background, as `foreground`.
    pub background: Rgb,
    /// The cursor colour, `None` when neither the program nor the embedder set one.
    pub cursor_color: Option<Rgb>,
    cells: Vec<Cell>,
    row_dirty: Vec<bool>,
    selection: Vec<Option<(u16, u16)>>,
}

impl Screen {
    /// The cell at `col`, `row` (0-based), or `None` outside the screen.
    pub fn cell(&self, col: u16, row: u16) -> Option<&Cell> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        self.cells
            .get(usize::from(row) * usize::from(self.cols) + usize::from(col))
    }

    /// Row `row`'s text: the cells' text in order, an empty cell as one space, with
    /// trailing spaces trimmed. A wide character's spacer cell reads as a space too.
    /// `""` past the last row.
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

    /// Whether row `row` changed since the previous [`Terminal::screen`]. Every row is
    /// dirty when [`dirty`](Self::dirty) is [`Dirty::Full`], none when it is
    /// [`Dirty::Clean`]; `false` past the last row.
    pub fn row_dirty(&self, row: u16) -> bool {
        match self.dirty {
            Dirty::Clean => false,
            Dirty::Full => row < self.rows,
            Dirty::Partial => self.row_dirty.get(usize::from(row)).copied() == Some(true),
        }
    }

    /// The selected columns of row `row`, inclusive at both ends, or `None` when the
    /// selection does not reach the row.
    pub fn row_selection(&self, row: u16) -> Option<(u16, u16)> {
        self.selection.get(usize::from(row)).copied().flatten()
    }
}

/// A libghostty-vt result code other than success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error {
    code: c_int,
}

impl Error {
    /// The `GhosttyResult` value, such as `-2` for `GHOSTTY_INVALID_VALUE`.
    pub fn code(&self) -> i32 {
        self.code
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.code {
            ffi::GHOSTTY_OUT_OF_MEMORY => "GHOSTTY_OUT_OF_MEMORY",
            ffi::GHOSTTY_INVALID_VALUE => "GHOSTTY_INVALID_VALUE",
            ffi::GHOSTTY_OUT_OF_SPACE => "GHOSTTY_OUT_OF_SPACE",
            ffi::GHOSTTY_NO_VALUE => "GHOSTTY_NO_VALUE",
            ffi::GHOSTTY_IO_ERROR => "GHOSTTY_IO_ERROR",
            ffi::GHOSTTY_LIMIT_EXCEEDED => "GHOSTTY_LIMIT_EXCEEDED",
            ffi::GHOSTTY_REJECTED => "GHOSTTY_REJECTED",
            ffi::GHOSTTY_UNSUPPORTED => "GHOSTTY_UNSUPPORTED",
            _ => "unknown result",
        };
        write!(f, "libghostty-vt returned {name} ({})", self.code)
    }
}

impl std::error::Error for Error {}

/// Maps a `GhosttyResult` to `Ok` on `GHOSTTY_SUCCESS`.
fn check(code: c_int) -> Result<(), Error> {
    if code == ffi::GHOSTTY_SUCCESS {
        Ok(())
    } else {
        Err(Error { code })
    }
}

/// Something the terminal asked of its embedder while it processed a call, queued for
/// [`Terminal::take_effects`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Bytes to write back to the pty, such as the reply to a device status report.
    WritePty(Vec<u8>),
    /// The title changed; [`Terminal::title`] reads it.
    TitleChanged,
    /// The pwd changed; [`Terminal::pwd`] reads it.
    PwdChanged,
    /// Synchronized output (mode 2026) began (`true`) or ended (`false`).
    RenderHold(bool),
}

/// Where [`Terminal::scroll_viewport`] moves the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollViewport {
    /// The top of the scrollback.
    Top,
    /// The active area.
    Bottom,
    /// By this many rows; up is negative.
    Delta(isize),
}

/// The scrollable area, in rows, for drawing a scrollbar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scrollbar {
    /// The scrollback and the active area together.
    pub total: u64,
    /// The viewport's first row within `total`; `total - len` at the bottom.
    pub offset: u64,
    /// The viewport's height.
    pub len: u64,
}

/// Modifiers held during a key or mouse event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Command on macOS, the Windows or Super key elsewhere.
    pub super_key: bool,
}

impl Mods {
    fn bits(self) -> ffi::GhosttyMods {
        let mut bits = 0;
        if self.shift {
            bits |= ffi::GHOSTTY_MODS_SHIFT;
        }
        if self.ctrl {
            bits |= ffi::GHOSTTY_MODS_CTRL;
        }
        if self.alt {
            bits |= ffi::GHOSTTY_MODS_ALT;
        }
        if self.super_key {
            bits |= ffi::GHOSTTY_MODS_SUPER;
        }
        bits
    }
}

/// The physical key of a [`KeyInput`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Unidentified,
    /// The US-layout key that types this character: a letter in either case, a digit,
    /// one of `` ` - = [ ] \ ; ' , . / `` or space. Any other character is
    /// [`Unidentified`](Self::Unidentified).
    Ascii(char),
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    /// `F1` to `F25`; any other number is unidentified.
    F(u8),
}

impl KeyCode {
    fn ghostty_key(self) -> c_int {
        match self {
            KeyCode::Unidentified => ffi::GHOSTTY_KEY_UNIDENTIFIED,
            KeyCode::Ascii(c) => ascii_key(c),
            KeyCode::Enter => ffi::GHOSTTY_KEY_ENTER,
            KeyCode::Escape => ffi::GHOSTTY_KEY_ESCAPE,
            KeyCode::Tab => ffi::GHOSTTY_KEY_TAB,
            KeyCode::Backspace => ffi::GHOSTTY_KEY_BACKSPACE,
            KeyCode::Delete => ffi::GHOSTTY_KEY_DELETE,
            KeyCode::Insert => ffi::GHOSTTY_KEY_INSERT,
            KeyCode::Up => ffi::GHOSTTY_KEY_ARROW_UP,
            KeyCode::Down => ffi::GHOSTTY_KEY_ARROW_DOWN,
            KeyCode::Left => ffi::GHOSTTY_KEY_ARROW_LEFT,
            KeyCode::Right => ffi::GHOSTTY_KEY_ARROW_RIGHT,
            KeyCode::Home => ffi::GHOSTTY_KEY_HOME,
            KeyCode::End => ffi::GHOSTTY_KEY_END,
            KeyCode::PageUp => ffi::GHOSTTY_KEY_PAGE_UP,
            KeyCode::PageDown => ffi::GHOSTTY_KEY_PAGE_DOWN,
            KeyCode::F(n) => {
                let key = ffi::GHOSTTY_KEY_F1 + c_int::from(n) - 1;
                if n >= 1 && key <= ffi::GHOSTTY_KEY_F25 {
                    key
                } else {
                    ffi::GHOSTTY_KEY_UNIDENTIFIED
                }
            }
        }
    }
}

/// The `GhosttyKey` of the US-layout key that types `c`.
fn ascii_key(c: char) -> c_int {
    match c {
        'a'..='z' => ffi::GHOSTTY_KEY_A + (c as c_int - 'a' as c_int),
        'A'..='Z' => ffi::GHOSTTY_KEY_A + (c as c_int - 'A' as c_int),
        '0'..='9' => ffi::GHOSTTY_KEY_DIGIT_0 + (c as c_int - '0' as c_int),
        '`' => ffi::GHOSTTY_KEY_BACKQUOTE,
        '\\' => ffi::GHOSTTY_KEY_BACKSLASH,
        '[' => ffi::GHOSTTY_KEY_BRACKET_LEFT,
        ']' => ffi::GHOSTTY_KEY_BRACKET_RIGHT,
        ',' => ffi::GHOSTTY_KEY_COMMA,
        '=' => ffi::GHOSTTY_KEY_EQUAL,
        '-' => ffi::GHOSTTY_KEY_MINUS,
        '.' => ffi::GHOSTTY_KEY_PERIOD,
        '\'' => ffi::GHOSTTY_KEY_QUOTE,
        ';' => ffi::GHOSTTY_KEY_SEMICOLON,
        '/' => ffi::GHOSTTY_KEY_SLASH,
        ' ' => ffi::GHOSTTY_KEY_SPACE,
        _ => ffi::GHOSTTY_KEY_UNIDENTIFIED,
    }
}

/// One key press for [`Terminal::encode_key`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyInput {
    pub code: KeyCode,
    /// The text the key types on the current layout, before Ctrl or Alt change it, such
    /// as `"c"` for Ctrl+C; empty for a named key. Control characters are dropped.
    pub text: String,
    /// The character the key types with no modifier, for the kitty protocol's base key.
    pub unshifted: Option<char>,
    pub mods: Mods,
}

/// What a mouse event did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

/// A mouse button; the wheel reports as presses of its two buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    WheelUp,
    WheelDown,
}

/// One mouse event for [`Terminal::encode_mouse`], at a pixel position from the top-left
/// of the terminal's content area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseInput {
    pub action: MouseAction,
    /// `None` for motion with no button held.
    pub button: Option<MouseButton>,
    pub mods: Mods,
    pub x: f32,
    pub y: f32,
}

/// Encodes `text` as a paste into a program: unsafe control bytes (ESC among them)
/// become spaces, and the text is wrapped in `CSI 200~` and `CSI 201~` when `bracketed`
/// (mode 2004), or has each newline turned into a carriage return when not.
pub fn paste_encode(text: &str, bracketed: bool) -> Vec<u8> {
    let mut data = text.as_bytes().to_vec();
    encode_with(|buf, cap, written| {
        // SAFETY: `data` is valid for its length and the library may rewrite it in place;
        // `buf` is valid for `cap` bytes, and `written` is a valid out-pointer.
        unsafe {
            ffi::ghostty_paste_encode(
                data.as_mut_ptr().cast::<c_char>(),
                data.len(),
                bracketed,
                buf,
                cap,
                written,
            )
        }
    })
}

/// Encodes a focus report, `CSI I` when `gained` and `CSI O` when not, for a program
/// that set mode 1004.
pub fn focus_encode(gained: bool) -> Vec<u8> {
    let event = if gained {
        ffi::GHOSTTY_FOCUS_GAINED
    } else {
        ffi::GHOSTTY_FOCUS_LOST
    };
    // SAFETY: `buf` is valid for `cap` bytes and `written` is a valid out-pointer.
    encode_with(|buf, cap, written| unsafe { ffi::ghostty_focus_encode(event, buf, cap, written) })
}

/// Runs an encoder that writes into a caller buffer, growing the buffer once when the
/// library asks for more. An error encodes nothing.
fn encode_with(mut encode: impl FnMut(*mut u8, usize, *mut usize) -> c_int) -> Vec<u8> {
    let mut out = vec![0u8; 128];
    loop {
        let mut written = 0usize;
        let code = encode(out.as_mut_ptr(), out.len(), &mut written);
        if code == ffi::GHOSTTY_OUT_OF_SPACE && written > out.len() {
            out.resize(written, 0);
            continue;
        }
        if code != ffi::GHOSTTY_SUCCESS {
            return Vec::new();
        }
        out.truncate(written);
        return out;
    }
}

/// The effect queue the library's callbacks push onto. Its address is the terminal's
/// userdata, so it lives in its own allocation that moves with nothing.
type EffectQueue = Vec<Effect>;

/// Recovers the effect queue from a callback's userdata.
///
/// # Safety
///
/// `userdata` is the `EffectQueue` a live [`Terminal`] registered, and the callback runs
/// inside a call on that terminal, which holds it exclusively.
unsafe fn queue_of<'a>(userdata: *mut c_void) -> Option<&'a mut EffectQueue> {
    userdata.cast::<EffectQueue>().as_mut()
}

extern "C" fn on_write_pty(
    _terminal: ffi::GhosttyTerminal,
    userdata: *mut c_void,
    data: *const u8,
    len: usize,
) {
    if data.is_null() {
        return;
    }
    // SAFETY: see queue_of; `data` is valid for `len` bytes for the duration of the call.
    unsafe {
        if let Some(queue) = queue_of(userdata) {
            queue.push(Effect::WritePty(
                std::slice::from_raw_parts(data, len).to_vec(),
            ));
        }
    }
}

extern "C" fn on_title_changed(_terminal: ffi::GhosttyTerminal, userdata: *mut c_void) {
    // SAFETY: see queue_of.
    if let Some(queue) = unsafe { queue_of(userdata) } {
        queue.push(Effect::TitleChanged);
    }
}

extern "C" fn on_pwd_changed(_terminal: ffi::GhosttyTerminal, userdata: *mut c_void) {
    // SAFETY: see queue_of.
    if let Some(queue) = unsafe { queue_of(userdata) } {
        queue.push(Effect::PwdChanged);
    }
}

extern "C" fn on_render_hold(_terminal: ffi::GhosttyTerminal, userdata: *mut c_void, held: bool) {
    // SAFETY: see queue_of.
    if let Some(queue) = unsafe { queue_of(userdata) } {
        queue.push(Effect::RenderHold(held));
    }
}

/// A terminal emulator: its screen, scrollback, modes and colours, and the encoders that
/// turn input into the bytes its program reads.
///
/// Every `unsafe` call in this impl relies on one invariant: the handles are non-null,
/// were made by their `*_new` functions, are owned by this value alone and are freed only
/// by `Drop`, and `effects` is the allocation registered as the terminal's userdata.
/// Mutating calls take `&mut self`, since libghostty-vt is not internally synchronized.
/// Each out-pointer passed to a `*_get` points at the type the header names for that key.
pub struct Terminal {
    terminal: ffi::GhosttyTerminal,
    render_state: ffi::GhosttyRenderState,
    rows: ffi::GhosttyRenderStateRowIterator,
    cells: ffi::GhosttyRenderStateRowCells,
    key_encoder: ffi::GhosttyKeyEncoder,
    key_event: ffi::GhosttyKeyEvent,
    mouse_encoder: ffi::GhosttyMouseEncoder,
    mouse_event: ffi::GhosttyMouseEvent,
    effects: *mut EffectQueue,
    /// The cell size of the last `resize`, which the mouse encoder converts pixels with.
    cell_px: (u32, u32),
    /// Whether a mouse button is down, which the mouse encoder reports drags by.
    button_down: bool,
}

// SAFETY: libghostty-vt handles have no thread affinity, and the effect queue is owned by
// this value. Not Sync: nothing synchronizes them.
unsafe impl Send for Terminal {}

impl Terminal {
    /// A terminal of `cols` by `rows` cells with libghostty-vt's defaults. Both must be
    /// above zero.
    pub fn new(cols: u16, rows: u16) -> Result<Terminal, Error> {
        let mut term = Terminal {
            terminal: ptr::null_mut(),
            render_state: ptr::null_mut(),
            rows: ptr::null_mut(),
            cells: ptr::null_mut(),
            key_encoder: ptr::null_mut(),
            key_event: ptr::null_mut(),
            mouse_encoder: ptr::null_mut(),
            mouse_event: ptr::null_mut(),
            effects: Box::into_raw(Box::default()),
            cell_px: (1, 1),
            button_down: false,
        };
        // SAFETY: out-pointers are fields of `term`, whose Drop frees what was made (NULL included).
        // Callbacks and userdata go in as the pointers themselves, as terminal.h requires.
        unsafe {
            check(ffi::ghostty_terminal_new(
                ptr::null(),
                &mut term.terminal,
                cols,
                rows,
            ))?;
            check(ffi::ghostty_render_state_new(
                ptr::null(),
                &mut term.render_state,
            ))?;
            check(ffi::ghostty_render_state_row_iterator_new(
                ptr::null(),
                &mut term.rows,
            ))?;
            check(ffi::ghostty_render_state_row_cells_new(
                ptr::null(),
                &mut term.cells,
            ))?;
            check(ffi::ghostty_key_encoder_new(
                ptr::null(),
                &mut term.key_encoder,
            ))?;
            check(ffi::ghostty_key_event_new(ptr::null(), &mut term.key_event))?;
            check(ffi::ghostty_mouse_encoder_new(
                ptr::null(),
                &mut term.mouse_encoder,
            ))?;
            check(ffi::ghostty_mouse_event_new(
                ptr::null(),
                &mut term.mouse_event,
            ))?;

            let set = |option, value: *const c_void| {
                check(ffi::ghostty_terminal_set(term.terminal, option, value))
            };
            set(
                ffi::GHOSTTY_TERMINAL_OPT_USERDATA,
                term.effects.cast::<c_void>(),
            )?;
            let write_pty: ffi::GhosttyTerminalWritePtyFn = on_write_pty;
            set(
                ffi::GHOSTTY_TERMINAL_OPT_WRITE_PTY,
                write_pty as *const c_void,
            )?;
            let title: ffi::GhosttyTerminalNotifyFn = on_title_changed;
            set(
                ffi::GHOSTTY_TERMINAL_OPT_TITLE_CHANGED,
                title as *const c_void,
            )?;
            let pwd: ffi::GhosttyTerminalNotifyFn = on_pwd_changed;
            set(ffi::GHOSTTY_TERMINAL_OPT_PWD_CHANGED, pwd as *const c_void)?;
            let hold: ffi::GhosttyTerminalRenderHoldFn = on_render_hold;
            set(ffi::GHOSTTY_TERMINAL_OPT_RENDER_HOLD, hold as *const c_void)?;
        }
        Ok(term)
    }

    /// Feeds program output through the VT parser. Malformed input never fails. What the
    /// output asks of the embedder, such as the reply to a device status report, is
    /// queued for [`take_effects`](Self::take_effects).
    pub fn feed(&mut self, bytes: &[u8]) {
        // SAFETY: `bytes` is valid for `bytes.len()` bytes for the duration of the call.
        unsafe { ffi::ghostty_terminal_vt_write(self.terminal, bytes.as_ptr(), bytes.len()) }
    }

    /// Drains the effects queued by earlier calls, oldest first.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        // SAFETY: the Terminal invariant; no library call is running, so nothing else
        // reaches the queue.
        unsafe { mem::take(&mut *self.effects) }
    }

    /// Resizes the grid, reflowing the primary screen, and records the cell size in pixels
    /// for size reports and the mouse encoder. Both `cols` and `rows` must be above zero.
    /// A resize ends a render hold.
    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), Error> {
        // SAFETY: the Terminal invariant; the call takes plain values.
        check(unsafe {
            ffi::ghostty_terminal_resize(self.terminal, cols, rows, cell_width_px, cell_height_px)
        })?;
        self.cell_px = (cell_width_px.max(1), cell_height_px.max(1));
        Ok(())
    }

    /// `(cols, rows)`.
    pub fn size(&self) -> (u16, u16) {
        (
            self.get_or(ffi::GHOSTTY_TERMINAL_DATA_COLS, 0u16),
            self.get_or(ffi::GHOSTTY_TERMINAL_DATA_ROWS, 0u16),
        )
    }

    /// The cursor's position in the active area and its DEC mode 25 visibility.
    pub fn cursor(&self) -> Cursor {
        Cursor {
            col: self.get_or(ffi::GHOSTTY_TERMINAL_DATA_CURSOR_X, 0u16),
            row: self.get_or(ffi::GHOSTTY_TERMINAL_DATA_CURSOR_Y, 0u16),
            visible: self.get_or(ffi::GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE, false),
        }
    }

    /// Sets the default foreground and background, which cells without a colour of their
    /// own take and which `OSC 10` and `OSC 11` report.
    pub fn set_default_colors(&mut self, foreground: Rgb, background: Rgb) -> Result<(), Error> {
        self.set_color(ffi::GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND, Some(foreground))?;
        self.set_color(ffi::GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND, Some(background))
    }

    /// Sets the default cursor colour, `None` to unset it.
    pub fn set_cursor_color(&mut self, color: Option<Rgb>) {
        // An invalid value is impossible for a colour or NULL.
        let _ = self.set_color(ffi::GHOSTTY_TERMINAL_OPT_COLOR_CURSOR, color);
    }

    /// The effective cursor colour, an `OSC 12` override included; `None` when none is set.
    pub fn cursor_color(&self) -> Option<Rgb> {
        self.get_color(ffi::GHOSTTY_TERMINAL_DATA_COLOR_CURSOR)
    }

    /// The effective default foreground, an `OSC 10` override included; `None` when none
    /// is set.
    pub fn foreground(&self) -> Option<Rgb> {
        self.get_color(ffi::GHOSTTY_TERMINAL_DATA_COLOR_FOREGROUND)
    }

    /// The effective default background, as `foreground`.
    pub fn background(&self) -> Option<Rgb> {
        self.get_color(ffi::GHOSTTY_TERMINAL_DATA_COLOR_BACKGROUND)
    }

    /// The current 256-colour palette, `OSC 4` overrides included.
    pub fn palette(&self) -> [Rgb; 256] {
        let palette = self.get_or(
            ffi::GHOSTTY_TERMINAL_DATA_COLOR_PALETTE,
            [ffi::GhosttyColorRgb::default(); 256],
        );
        palette.map(Rgb::from)
    }

    /// Sets the default 256-colour palette, `None` to restore the built-in one. Cells keep
    /// palette indices, so text already on screen takes the new colours.
    pub fn set_palette(&mut self, palette: Option<&[Rgb; 256]>) {
        let colors = palette.map(|p| p.map(ffi::GhosttyColorRgb::from));
        let value = colors
            .as_ref()
            .map_or(ptr::null(), |c| ptr::from_ref(c).cast::<c_void>());
        // SAFETY: the Terminal invariant; `value` is NULL or points at 256 colours, which
        // the library copies.
        unsafe {
            ffi::ghostty_terminal_set(
                self.terminal,
                ffi::GHOSTTY_TERMINAL_OPT_COLOR_PALETTE,
                value,
            );
        }
    }

    /// Whether DEC private mode `mode` (as in `CSI ? mode h`) is set; `false` for a mode
    /// the library does not know.
    pub fn dec_mode(&self, mode: u16) -> bool {
        let mut config = ffi::GhosttyTerminalModeConfig {
            mode: mode & 0x7FFF,
            value: false,
        };
        // SAFETY: the Terminal invariant; MODE reads and writes a GhosttyTerminalModeConfig.
        let code = unsafe {
            ffi::ghostty_terminal_get(
                self.terminal,
                ffi::GHOSTTY_TERMINAL_DATA_MODE,
                ptr::addr_of_mut!(config).cast::<c_void>(),
            )
        };
        code == ffi::GHOSTTY_SUCCESS && config.value
    }

    /// Whether any mouse tracking mode (9, 1000, 1002 or 1003) is set.
    pub fn mouse_tracking(&self) -> bool {
        self.get_or(ffi::GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING, false)
    }

    /// Whether the alternate screen is active.
    pub fn alternate_screen(&self) -> bool {
        self.get_or(ffi::GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN, 0 as c_int)
            == ffi::GHOSTTY_TERMINAL_SCREEN_ALTERNATE
    }

    /// The title the program set with `OSC 0` or `OSC 2`, `""` when none.
    pub fn title(&self) -> String {
        self.get_string(ffi::GHOSTTY_TERMINAL_DATA_TITLE)
    }

    /// The pwd the program reported with `OSC 7`, 9 or 1337, as it sent it: a `file://`
    /// URI or a bare path, `""` when none.
    pub fn pwd(&self) -> String {
        self.get_string(ffi::GHOSTTY_TERMINAL_DATA_PWD)
    }

    /// Scrolls the viewport through the scrollback.
    pub fn scroll_viewport(&mut self, to: ScrollViewport) {
        let (tag, delta) = match to {
            ScrollViewport::Top => (ffi::GHOSTTY_TERMINAL_SCROLL_VIEWPORT_TOP, 0),
            ScrollViewport::Bottom => (ffi::GHOSTTY_TERMINAL_SCROLL_VIEWPORT_BOTTOM, 0),
            ScrollViewport::Delta(rows) => (ffi::GHOSTTY_TERMINAL_SCROLL_VIEWPORT_DELTA, rows),
        };
        let mut value = ffi::GhosttyTerminalScrollViewportValue { padding: [0; 2] };
        value.delta = delta;
        // SAFETY: the Terminal invariant; the behaviour is passed by value.
        unsafe {
            ffi::ghostty_terminal_scroll_viewport(
                self.terminal,
                ffi::GhosttyTerminalScrollViewport { tag, value },
            );
        }
    }

    /// The scrollable area and the viewport's place in it.
    pub fn scrollbar(&self) -> Scrollbar {
        let bar = self.get_or(
            ffi::GHOSTTY_TERMINAL_DATA_SCROLLBAR,
            ffi::GhosttyTerminalScrollbar::default(),
        );
        Scrollbar {
            total: bar.total,
            offset: bar.offset,
            len: bar.len,
        }
    }

    /// Whether the viewport follows the active area rather than showing history.
    pub fn viewport_at_bottom(&self) -> bool {
        self.get_or(ffi::GHOSTTY_TERMINAL_DATA_VIEWPORT_ACTIVE, true)
    }

    /// Selects the text from viewport cell `start` to `end`, `(col, row)` and inclusive,
    /// as text flows, replacing any selection.
    pub fn select(&mut self, start: (u16, u16), end: (u16, u16)) -> Result<(), Error> {
        let selection = ffi::GhosttySelection {
            start: self.grid_ref(start)?,
            end: self.grid_ref(end)?,
            rectangle: false,
        };
        // SAFETY: the Terminal invariant; both references were made just now, with no
        // mutation in between, and the library copies the selection.
        check(unsafe {
            ffi::ghostty_terminal_set(
                self.terminal,
                ffi::GHOSTTY_TERMINAL_OPT_SELECTION,
                ptr::addr_of!(selection).cast::<c_void>(),
            )
        })
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self) -> Result<(), Error> {
        // SAFETY: the Terminal invariant; NULL clears the selection.
        check(unsafe {
            ffi::ghostty_terminal_set(
                self.terminal,
                ffi::GHOSTTY_TERMINAL_OPT_SELECTION,
                ptr::null(),
            )
        })
    }

    /// The selection's text as a copy would take it, soft-wrapped lines joined and
    /// trailing spaces trimmed; `None` when nothing is selected.
    pub fn selection_text(&mut self) -> Option<String> {
        let mut selection = ffi::GhosttySelection {
            start: ffi::GhosttyGridRef {
                node: ptr::null_mut(),
                x: 0,
                y: 0,
            },
            end: ffi::GhosttyGridRef {
                node: ptr::null_mut(),
                x: 0,
                y: 0,
            },
            rectangle: false,
        };
        // SAFETY: the Terminal invariant; SELECTION writes a GhosttySelection.
        let code = unsafe {
            ffi::ghostty_terminal_get(
                self.terminal,
                ffi::GHOSTTY_TERMINAL_DATA_SELECTION,
                ptr::addr_of_mut!(selection).cast::<c_void>(),
            )
        };
        if code != ffi::GHOSTTY_SUCCESS {
            return None;
        }
        let mut formatter: ffi::GhosttyFormatter = ptr::null_mut();
        // SAFETY: the Terminal invariant; the formatter borrows the terminal only within this call,
        // and the selection was read with no mutation since. Options point at formatter.h's types.
        let bytes = unsafe {
            if ffi::ghostty_formatter_terminal_new(ptr::null(), &mut formatter, self.terminal)
                != ffi::GHOSTTY_SUCCESS
            {
                return None;
            }
            let yes = true;
            let bool_ptr = ptr::addr_of!(yes).cast::<c_void>();
            ffi::ghostty_formatter_set(formatter, ffi::GHOSTTY_FORMATTER_OPT_UNWRAP, bool_ptr);
            ffi::ghostty_formatter_set(formatter, ffi::GHOSTTY_FORMATTER_OPT_TRIM, bool_ptr);
            ffi::ghostty_formatter_set(
                formatter,
                ffi::GHOSTTY_FORMATTER_OPT_SELECTION,
                ptr::addr_of!(selection).cast::<c_void>(),
            );
            let bytes = encode_with(|buf, cap, written| {
                ffi::ghostty_formatter_format_buf(formatter, buf, cap, written)
            });
            ffi::ghostty_formatter_free(formatter);
            bytes
        };
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Encodes a key press as the program expects it, from the terminal's current modes:
    /// cursor-key and keypad application modes, the kitty keyboard flags and
    /// modifyOtherKeys. Alt is the encoder's alt on every platform. Empty for a key that
    /// sends nothing.
    pub fn encode_key(&mut self, key: &KeyInput) -> Vec<u8> {
        let text: &str = if key.text.chars().any(char::is_control) {
            ""
        } else {
            &key.text
        };
        let consumed = if key.mods.shift && !text.is_empty() {
            ffi::GHOSTTY_MODS_SHIFT
        } else {
            0
        };
        let option_as_alt = ffi::GHOSTTY_OPTION_AS_ALT_TRUE;
        // SAFETY: the Terminal invariant. The event borrows `text` only until its text is
        // cleared below, before `text` goes out of scope.
        unsafe {
            ffi::ghostty_key_encoder_set_from_terminal(self.key_encoder, self.terminal);
            ffi::ghostty_key_encoder_set(
                self.key_encoder,
                ffi::GHOSTTY_KEY_ENCODER_OPT_MACOS_OPTION_AS_ALT,
                ptr::addr_of!(option_as_alt).cast::<c_void>(),
            );
            let event = self.key_event;
            ffi::ghostty_key_event_set_action(event, ffi::GHOSTTY_KEY_ACTION_PRESS);
            ffi::ghostty_key_event_set_key(event, key.code.ghostty_key());
            ffi::ghostty_key_event_set_mods(event, key.mods.bits());
            ffi::ghostty_key_event_set_consumed_mods(event, consumed);
            ffi::ghostty_key_event_set_unshifted_codepoint(
                event,
                key.unshifted.map_or(0, u32::from),
            );
            ffi::ghostty_key_event_set_utf8(
                event,
                ffi::GhosttyString {
                    ptr: text.as_ptr(),
                    len: text.len(),
                },
            );
            let bytes = encode_with(|buf, cap, written| {
                ffi::ghostty_key_encoder_encode(self.key_encoder, event, buf, cap, written)
            });
            ffi::ghostty_key_event_set_utf8(
                event,
                ffi::GhosttyString {
                    ptr: ptr::null(),
                    len: 0,
                },
            );
            bytes
        }
    }

    /// Encodes a mouse event as the program's tracking mode and format ask, converting
    /// its pixel position to a cell with the cell size of the last `resize`. Empty when
    /// the program tracks no mouse or the mode does not report this event.
    pub fn encode_mouse(&mut self, mouse: &MouseInput) -> Vec<u8> {
        let (cols, rows) = self.size();
        let (cell_w, cell_h) = self.cell_px;
        let size = ffi::GhosttyMouseEncoderSize {
            size: mem::size_of::<ffi::GhosttyMouseEncoderSize>(),
            screen_width: u32::from(cols) * cell_w,
            screen_height: u32::from(rows) * cell_h,
            cell_width: cell_w,
            cell_height: cell_h,
            ..Default::default()
        };
        let wheel = matches!(
            mouse.button,
            Some(MouseButton::WheelUp | MouseButton::WheelDown)
        );
        if !wheel && mouse.button.is_some() {
            match mouse.action {
                MouseAction::Press => self.button_down = true,
                MouseAction::Release => self.button_down = false,
                MouseAction::Motion => {}
            }
        }
        let action = match mouse.action {
            MouseAction::Press => ffi::GHOSTTY_MOUSE_ACTION_PRESS,
            MouseAction::Release => ffi::GHOSTTY_MOUSE_ACTION_RELEASE,
            MouseAction::Motion => ffi::GHOSTTY_MOUSE_ACTION_MOTION,
        };
        let button = mouse.button.map(|button| match button {
            MouseButton::Left => ffi::GHOSTTY_MOUSE_BUTTON_LEFT,
            MouseButton::Right => ffi::GHOSTTY_MOUSE_BUTTON_RIGHT,
            MouseButton::Middle => ffi::GHOSTTY_MOUSE_BUTTON_MIDDLE,
            MouseButton::WheelUp => ffi::GHOSTTY_MOUSE_BUTTON_FOUR,
            MouseButton::WheelDown => ffi::GHOSTTY_MOUSE_BUTTON_FIVE,
        });
        let button_down = self.button_down;
        // SAFETY: the Terminal invariant; each option points at the type mouse/encoder.h
        // names for it, and the encoder copies it.
        unsafe {
            ffi::ghostty_mouse_encoder_set_from_terminal(self.mouse_encoder, self.terminal);
            ffi::ghostty_mouse_encoder_set(
                self.mouse_encoder,
                ffi::GHOSTTY_MOUSE_ENCODER_OPT_SIZE,
                ptr::addr_of!(size).cast::<c_void>(),
            );
            ffi::ghostty_mouse_encoder_set(
                self.mouse_encoder,
                ffi::GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED,
                ptr::addr_of!(button_down).cast::<c_void>(),
            );
            let event = self.mouse_event;
            ffi::ghostty_mouse_event_set_action(event, action);
            match button {
                Some(button) => ffi::ghostty_mouse_event_set_button(event, button),
                None => ffi::ghostty_mouse_event_clear_button(event),
            };
            ffi::ghostty_mouse_event_set_mods(event, mouse.mods.bits());
            ffi::ghostty_mouse_event_set_position(
                event,
                ffi::GhosttyMousePosition {
                    x: mouse.x,
                    y: mouse.y,
                },
            );
            encode_with(|buf, cap, written| {
                ffi::ghostty_mouse_encoder_encode(self.mouse_encoder, event, buf, cap, written)
            })
        }
    }

    /// The viewport's cells, cursor and colours, read through a render-state update, with
    /// what changed since the previous call. Reading consumes that change state: the next
    /// call reports only what changes after this one.
    pub fn screen(&mut self) -> Result<Screen, Error> {
        let mut cols = 0u16;
        let mut rows = 0u16;
        let mut dirty = ffi::GHOSTTY_RENDER_STATE_DIRTY_FALSE;
        let mut cursor = ffi::GhosttyRenderStateCursor {
            size: mem::size_of::<ffi::GhosttyRenderStateCursor>(),
            ..Default::default()
        };
        let mut colors = ffi::GhosttyRenderStateColors {
            size: mem::size_of::<ffi::GhosttyRenderStateColors>(),
            background: ffi::GhosttyColorRgb::default(),
            foreground: ffi::GhosttyColorRgb::default(),
            cursor: ffi::GhosttyColorRgb::default(),
            cursor_has_value: false,
            palette: [ffi::GhosttyColorRgb::default(); 256],
        };
        // SAFETY: the Terminal invariant. `self.rows` and `self.cells` are refilled from the state
        // just updated and read before its next update; the sized structs carry their size.
        unsafe {
            check(ffi::ghostty_render_state_update(
                self.render_state,
                self.terminal,
            ))?;
            self.render_get(ffi::GHOSTTY_RENDER_STATE_DATA_COLS, &mut cols)?;
            self.render_get(ffi::GHOSTTY_RENDER_STATE_DATA_ROWS, &mut rows)?;
            self.render_get(ffi::GHOSTTY_RENDER_STATE_DATA_DIRTY, &mut dirty)?;
            self.render_get(ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR, &mut cursor)?;
            self.render_get(ffi::GHOSTTY_RENDER_STATE_DATA_COLORS, &mut colors)?;
            // The iterator handle itself is the output: the library fills the iterator it names.
            let mut rows_handle = self.rows;
            self.render_get(
                ffi::GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                &mut rows_handle,
            )?;
        }

        let empty = Cell::default();
        let mut cells = Vec::with_capacity(usize::from(cols) * usize::from(rows));
        let mut row_dirty = Vec::with_capacity(usize::from(rows));
        let mut selection = Vec::with_capacity(usize::from(rows));
        for _ in 0..rows {
            // SAFETY: as above; the iterator was populated from the updated render state.
            if !unsafe { ffi::ghostty_render_state_row_iterator_next(self.rows) } {
                break;
            }
            let mut this_dirty = false;
            let mut range = ffi::GhosttyRenderStateRowSelection {
                size: mem::size_of::<ffi::GhosttyRenderStateRowSelection>(),
                ..Default::default()
            };
            // SAFETY: as above; the iterator is positioned on a row, and each output points at
            // the type render.h names for its key.
            let selected = unsafe {
                check(ffi::ghostty_render_state_row_get(
                    self.rows,
                    ffi::GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY,
                    ptr::addr_of_mut!(this_dirty).cast::<c_void>(),
                ))?;
                check(ffi::ghostty_render_state_row_get(
                    self.rows,
                    ffi::GHOSTTY_RENDER_STATE_ROW_DATA_CELLS,
                    ptr::addr_of_mut!(self.cells).cast::<c_void>(),
                ))?;
                ffi::ghostty_render_state_row_get(
                    self.rows,
                    ffi::GHOSTTY_RENDER_STATE_ROW_DATA_SELECTION,
                    ptr::addr_of_mut!(range).cast::<c_void>(),
                ) == ffi::GHOSTTY_SUCCESS
            };
            row_dirty.push(this_dirty);
            selection.push(selected.then_some((range.start_x, range.end_x)));
            let row_start = cells.len();
            // SAFETY: as above; the cells were populated from the current row.
            while cells.len() - row_start < usize::from(cols)
                && unsafe { ffi::ghostty_render_state_row_cells_next(self.cells) }
            {
                cells.push(self.current_cell()?);
            }
            cells.resize(row_start + usize::from(cols), empty.clone());
        }
        cells.resize(usize::from(cols) * usize::from(rows), empty);
        row_dirty.resize(usize::from(rows), false);
        selection.resize(usize::from(rows), None);

        // SAFETY: the Terminal invariant; nothing read above outlives this call.
        check(unsafe { ffi::ghostty_render_state_clean(self.render_state) })?;

        let dirty = match dirty {
            ffi::GHOSTTY_RENDER_STATE_DIRTY_PARTIAL => Dirty::Partial,
            ffi::GHOSTTY_RENDER_STATE_DIRTY_FULL => Dirty::Full,
            _ => Dirty::Clean,
        };
        let cursor = cursor.viewport_has_value.then_some(ScreenCursor {
            col: cursor.viewport_x,
            row: cursor.viewport_y,
            visible: cursor.visible,
            blinking: cursor.blinking,
            style: match cursor.visual_style {
                ffi::GHOSTTY_CURSOR_STYLE_BAR => CursorStyle::Bar,
                ffi::GHOSTTY_CURSOR_STYLE_UNDERLINE => CursorStyle::Underline,
                ffi::GHOSTTY_CURSOR_STYLE_BLOCK_HOLLOW => CursorStyle::BlockHollow,
                ffi::GHOSTTY_CURSOR_STYLE_BLOCK => CursorStyle::Block,
                _ => CursorStyle::Block,
            },
        });
        Ok(Screen {
            cols,
            rows,
            dirty,
            cursor,
            foreground: colors.foreground.into(),
            background: colors.background.into(),
            cursor_color: colors.cursor_has_value.then(|| colors.cursor.into()),
            cells,
            row_dirty,
            selection,
        })
    }

    /// Reads render-state data `key` into `out`, whose type the header names for `key`.
    ///
    /// # Safety
    ///
    /// The Terminal invariant, and `T` is the type render.h names for `key`.
    unsafe fn render_get<T>(&self, key: c_int, out: &mut T) -> Result<(), Error> {
        check(ffi::ghostty_render_state_get(
            self.render_state,
            key,
            ptr::from_mut(out).cast::<c_void>(),
        ))
    }

    /// The cell `self.cells` is positioned on.
    fn current_cell(&self) -> Result<Cell, Error> {
        let mut style = ffi::GhosttyStyle::default();
        let mut raw: ffi::GhosttyCell = 0;
        let mut wide: c_int = ffi::GHOSTTY_CELL_WIDE_NARROW;
        // SAFETY: the Terminal invariant; STYLE writes a GhosttyStyle, RAW a GhosttyCell and
        // the cell's WIDE a GhosttyCellWide.
        unsafe {
            check(ffi::ghostty_render_state_row_cells_get(
                self.cells,
                ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                ptr::addr_of_mut!(style).cast::<c_void>(),
            ))?;
            check(ffi::ghostty_render_state_row_cells_get(
                self.cells,
                ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                ptr::addr_of_mut!(raw).cast::<c_void>(),
            ))?;
            check(ffi::ghostty_cell_get(
                raw,
                ffi::GHOSTTY_CELL_DATA_WIDE,
                ptr::addr_of_mut!(wide).cast::<c_void>(),
            ))?;
        }
        let flag = |bit: u16| style.flags & bit != 0;
        let underline = match (style.flags & ffi::GHOSTTY_STYLE_UNDERLINE_MASK)
            >> ffi::GHOSTTY_STYLE_UNDERLINE_SHIFT
        {
            ffi::GHOSTTY_SGR_UNDERLINE_SINGLE => Underline::Single,
            ffi::GHOSTTY_SGR_UNDERLINE_DOUBLE => Underline::Double,
            ffi::GHOSTTY_SGR_UNDERLINE_CURLY => Underline::Curly,
            ffi::GHOSTTY_SGR_UNDERLINE_DOTTED => Underline::Dotted,
            ffi::GHOSTTY_SGR_UNDERLINE_DASHED => Underline::Dashed,
            _ => Underline::None,
        };
        let width = match wide {
            ffi::GHOSTTY_CELL_WIDE_WIDE => CellWidth::Wide,
            ffi::GHOSTTY_CELL_WIDE_SPACER_TAIL => CellWidth::SpacerTail,
            ffi::GHOSTTY_CELL_WIDE_SPACER_HEAD => CellWidth::SpacerHead,
            _ => CellWidth::Narrow,
        };
        Ok(Cell {
            text: self.current_text()?,
            fg: self.current_color(ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR)?,
            bg: self.current_color(ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR)?,
            bold: flag(ffi::GHOSTTY_STYLE_FLAG_BOLD),
            faint: flag(ffi::GHOSTTY_STYLE_FLAG_FAINT),
            italic: flag(ffi::GHOSTTY_STYLE_FLAG_ITALIC),
            strikethrough: flag(ffi::GHOSTTY_STYLE_FLAG_STRIKETHROUGH),
            inverse: flag(ffi::GHOSTTY_STYLE_FLAG_INVERSE),
            invisible: flag(ffi::GHOSTTY_STYLE_FLAG_INVISIBLE),
            blink: flag(ffi::GHOSTTY_STYLE_FLAG_BLINK),
            underline,
            width,
        })
    }

    /// The current cell's grapheme cluster as UTF-8, retried once with the length the
    /// library asks for when the inline buffer is too small.
    fn current_text(&self) -> Result<String, Error> {
        let mut bytes = vec![0u8; 32];
        loop {
            let mut buffer = ffi::GhosttyBuffer {
                ptr: bytes.as_mut_ptr(),
                cap: bytes.len(),
                len: 0,
            };
            // SAFETY: the Terminal invariant; `buffer` describes `bytes`, which outlives the
            // call, and the library writes at most `cap` bytes into it.
            let code = unsafe {
                ffi::ghostty_render_state_row_cells_get(
                    self.cells,
                    ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
                    ptr::addr_of_mut!(buffer).cast::<c_void>(),
                )
            };
            if code == ffi::GHOSTTY_OUT_OF_SPACE && buffer.len > bytes.len() {
                bytes.resize(buffer.len, 0);
                continue;
            }
            check(code)?;
            bytes.truncate(buffer.len);
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }
    }

    /// The current cell's resolved colour for `key`, `None` on `GHOSTTY_NO_VALUE`.
    fn current_color(&self, key: c_int) -> Result<Option<Rgb>, Error> {
        let mut color = ffi::GhosttyColorRgb::default();
        // SAFETY: the Terminal invariant; `key` is a colour key, whose output is a
        // GhosttyColorRgb.
        let code = unsafe {
            ffi::ghostty_render_state_row_cells_get(
                self.cells,
                key,
                ptr::addr_of_mut!(color).cast::<c_void>(),
            )
        };
        if code == ffi::GHOSTTY_NO_VALUE {
            return Ok(None);
        }
        check(code)?;
        Ok(Some(color.into()))
    }

    /// A reference to viewport cell `(col, row)`, valid until the next mutation.
    fn grid_ref(&self, (col, row): (u16, u16)) -> Result<ffi::GhosttyGridRef, Error> {
        let point = ffi::GhosttyPoint {
            tag: ffi::GHOSTTY_POINT_TAG_VIEWPORT,
            value: ffi::GhosttyPointValue {
                coordinate: ffi::GhosttyPointCoordinate {
                    x: col,
                    y: u32::from(row),
                },
            },
        };
        let mut out = ffi::GhosttyGridRef {
            node: ptr::null_mut(),
            x: 0,
            y: 0,
        };
        // SAFETY: the Terminal invariant; the point is passed by value and `out` is a valid
        // GhosttyGridRef.
        check(unsafe { ffi::ghostty_terminal_grid_ref(self.terminal, point, &mut out) })?;
        Ok(out)
    }

    fn set_color(&mut self, option: c_int, color: Option<Rgb>) -> Result<(), Error> {
        let color = color.map(ffi::GhosttyColorRgb::from);
        let value = color
            .as_ref()
            .map_or(ptr::null(), |c| ptr::from_ref(c).cast::<c_void>());
        // SAFETY: the Terminal invariant; `option` is a colour option, whose input is NULL or
        // a GhosttyColorRgb, which the library copies.
        check(unsafe { ffi::ghostty_terminal_set(self.terminal, option, value) })
    }

    fn get_color(&self, key: c_int) -> Option<Rgb> {
        let mut color = ffi::GhosttyColorRgb::default();
        // SAFETY: the Terminal invariant; `key` is a colour key, whose output is a
        // GhosttyColorRgb.
        let code = unsafe {
            ffi::ghostty_terminal_get(
                self.terminal,
                key,
                ptr::addr_of_mut!(color).cast::<c_void>(),
            )
        };
        (code == ffi::GHOSTTY_SUCCESS).then(|| color.into())
    }

    /// Copies out the borrowed string terminal data `key` names.
    fn get_string(&self, key: c_int) -> String {
        let empty = ffi::GhosttyString {
            ptr: ptr::null(),
            len: 0,
        };
        let value = self.get_or(key, empty);
        if value.ptr.is_null() || value.len == 0 {
            return String::new();
        }
        // SAFETY: the library's string is valid for `len` bytes until the next mutating call,
        // and it is copied before this function returns.
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(value.ptr, value.len) })
            .into_owned()
    }

    /// Reads terminal data `key` into a `T`, or returns `default` when the library has no
    /// value. Callers pass the `T` the header names for `key`.
    fn get_or<T: Copy>(&self, key: c_int, default: T) -> T {
        let mut value = default;
        // SAFETY: the Terminal invariant; every caller pairs `key` with its header's type.
        let code = unsafe {
            ffi::ghostty_terminal_get(
                self.terminal,
                key,
                ptr::addr_of_mut!(value).cast::<c_void>(),
            )
        };
        if code == ffi::GHOSTTY_SUCCESS {
            value
        } else {
            default
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // SAFETY: the Terminal invariant; each handle is freed once (NULL accepted), cells and rows
        // before the render state they borrow, the queue after the terminal whose callbacks reach it.
        unsafe {
            ffi::ghostty_mouse_event_free(self.mouse_event);
            ffi::ghostty_mouse_encoder_free(self.mouse_encoder);
            ffi::ghostty_key_event_free(self.key_event);
            ffi::ghostty_key_encoder_free(self.key_encoder);
            ffi::ghostty_render_state_row_cells_free(self.cells);
            ffi::ghostty_render_state_row_iterator_free(self.rows);
            ffi::ghostty_render_state_free(self.render_state);
            ffi::ghostty_terminal_free(self.terminal);
            drop(Box::from_raw(self.effects));
        }
    }
}
