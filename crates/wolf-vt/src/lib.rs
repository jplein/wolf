//! A safe binding over libghostty-vt, Ghostty's terminal-emulation library: program
//! output goes in as bytes, and the screen comes back out as cells, a cursor and colours.
//!
//! The crate is GUI-free: libghostty-vt parses VT sequences and keeps screen state, and
//! draws nothing. It holds every `unsafe` line and all the link wiring between Wolf and
//! the library. How the library is pinned, built and linked on each platform is in
//! `specs/terminal-panes.md#building-libghostty-vt`.

use std::ffi::{c_int, c_void, CStr};
use std::fmt;
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

/// One cell of the viewport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The cell's grapheme cluster: `""` for an empty cell or a wide character's spacer.
    pub text: String,
    /// The resolved foreground, palette indices looked up; `None` when the cell has no
    /// colour of its own and takes the terminal's default.
    pub fg: Option<Rgb>,
    /// The resolved background, as `fg`.
    pub bg: Option<Rgb>,
}

/// The viewport's cells at one moment, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub cols: u16,
    pub rows: u16,
    cells: Vec<Cell>,
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

/// A terminal emulator: its screen, scrollback, modes and colours.
///
/// Every `unsafe` call in this impl relies on one invariant: the four handles are non-null,
/// were made by their `*_new` functions, are owned by this value alone and are freed only
/// by `Drop`. Mutating calls take `&mut self`, since libghostty-vt is not internally
/// synchronized. Each out-pointer passed to a `*_get` points at the type the header names
/// for that key.
pub struct Terminal {
    terminal: ffi::GhosttyTerminal,
    render_state: ffi::GhosttyRenderState,
    rows: ffi::GhosttyRenderStateRowIterator,
    cells: ffi::GhosttyRenderStateRowCells,
}

// SAFETY: libghostty-vt handles have no thread affinity. Not Sync: nothing synchronizes them.
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
        };
        // SAFETY: each out-pointer is a field of `term`, whose Drop frees whatever was made,
        // and every free function accepts NULL for a handle that was not.
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
        }
        Ok(term)
    }

    /// Feeds program output through the VT parser. Malformed input never fails; queries
    /// that need a reply, such as a device status report, are dropped.
    pub fn feed(&mut self, bytes: &[u8]) {
        // SAFETY: `bytes` is valid for `bytes.len()` bytes for the duration of the call.
        unsafe { ffi::ghostty_terminal_vt_write(self.terminal, bytes.as_ptr(), bytes.len()) }
    }

    /// Resizes the grid, reflowing the primary screen, and records the cell size in pixels
    /// for size reports. Both `cols` and `rows` must be above zero.
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
        })
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
        self.set_color(ffi::GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND, foreground)?;
        self.set_color(ffi::GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND, background)
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

    /// The viewport's cells, read through a render-state update. The update consumes the
    /// terminal's dirty state, which nothing else in this crate reads.
    pub fn screen(&mut self) -> Result<Screen, Error> {
        let mut cols = 0u16;
        let mut rows = 0u16;
        // SAFETY: the Terminal invariant. `self.rows` and `self.cells` are repopulated from
        // the render state just updated, and read before it is updated again.
        unsafe {
            check(ffi::ghostty_render_state_update(
                self.render_state,
                self.terminal,
            ))?;
            check(ffi::ghostty_render_state_get(
                self.render_state,
                ffi::GHOSTTY_RENDER_STATE_DATA_COLS,
                ptr::addr_of_mut!(cols).cast::<c_void>(),
            ))?;
            check(ffi::ghostty_render_state_get(
                self.render_state,
                ffi::GHOSTTY_RENDER_STATE_DATA_ROWS,
                ptr::addr_of_mut!(rows).cast::<c_void>(),
            ))?;
            check(ffi::ghostty_render_state_get(
                self.render_state,
                ffi::GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                ptr::addr_of_mut!(self.rows).cast::<c_void>(),
            ))?;
        }

        let empty = Cell {
            text: String::new(),
            fg: None,
            bg: None,
        };
        let mut cells = Vec::with_capacity(usize::from(cols) * usize::from(rows));
        for _ in 0..rows {
            // SAFETY: as above; the iterator was populated from the updated render state.
            if !unsafe { ffi::ghostty_render_state_row_iterator_next(self.rows) } {
                break;
            }
            // SAFETY: as above; the iterator is positioned on a row.
            unsafe {
                check(ffi::ghostty_render_state_row_get(
                    self.rows,
                    ffi::GHOSTTY_RENDER_STATE_ROW_DATA_CELLS,
                    ptr::addr_of_mut!(self.cells).cast::<c_void>(),
                ))?;
            }
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
        Ok(Screen { cols, rows, cells })
    }

    /// The cell `self.cells` is positioned on.
    fn current_cell(&self) -> Result<Cell, Error> {
        Ok(Cell {
            text: self.current_text()?,
            fg: self.current_color(ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR)?,
            bg: self.current_color(ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR)?,
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

    fn set_color(&mut self, option: c_int, color: Rgb) -> Result<(), Error> {
        let color = ffi::GhosttyColorRgb::from(color);
        // SAFETY: the Terminal invariant; `option` is a colour option, whose input is a
        // GhosttyColorRgb, which the library copies.
        check(unsafe {
            ffi::ghostty_terminal_set(self.terminal, option, ptr::addr_of!(color).cast::<c_void>())
        })
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
        // SAFETY: the Terminal invariant; each handle is freed once, NULL is accepted, and
        // the cells and rows go before the render state they borrow from.
        unsafe {
            ffi::ghostty_render_state_row_cells_free(self.cells);
            ffi::ghostty_render_state_row_iterator_free(self.rows);
            ffi::ghostty_render_state_free(self.render_state);
            ffi::ghostty_terminal_free(self.terminal);
        }
    }
}
