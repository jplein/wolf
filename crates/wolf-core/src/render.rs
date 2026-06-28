//! Render-state model: the toolkit-agnostic, in-memory description of what should
//! be drawn on screen.
//!
//! `wolf-core` folds Neovim's `redraw` event stream into a [`Snapshot`] held
//! behind a single [`SharedSnapshot`] lock. Front-ends read the snapshot on their
//! UI thread (after a wake) and paint it natively. Nothing here draws pixels or
//! references any GUI toolkit.
//!
//! ## Locking invariant
//!
//! There is exactly one [`parking_lot::RwLock`] guarding the snapshot. The redraw
//! handler takes the *write* lock once for a whole `redraw` batch, never holds it
//! across an `.await`, and fires the wake callback only *after* the lock is
//! dropped (on `flush`). Readers take the *read* lock for the duration of a paint.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::{
    ArcRwLockReadGuard, RawRwLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
};

/// An owned read guard over the shared snapshot.
///
/// Unlike [`RwLockReadGuard`], this owns an `Arc` clone of the lock, so it does
/// not borrow the [`SharedSnapshot`] (or anything containing it). This lets the
/// front-end-facing [`crate::api::WolfCore::read_snapshot`] return a guard whose
/// lifetime is independent of `&self`.
pub type OwnedSnapshotReadGuard = ArcRwLockReadGuard<RawRwLock, Snapshot>;

/// Bit shift to extract the red channel from a packed `0xRRGGBB` color.
const RED_SHIFT: u32 = 16;
/// Bit shift to extract the green channel from a packed `0xRRGGBB` color.
const GREEN_SHIFT: u32 = 8;
/// Mask for a single 8-bit color channel.
const CHANNEL_MASK: u32 = 0xFF;

/// An RGB color, unpacked from Neovim's packed `0xRRGGBB` integer form.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red channel (0–255).
    pub r: u8,
    /// Green channel (0–255).
    pub g: u8,
    /// Blue channel (0–255).
    pub b: u8,
}

impl Rgb {
    /// Unpacks a Neovim `0xRRGGBB` integer into an [`Rgb`].
    #[must_use]
    pub fn from_packed(packed: u32) -> Self {
        Self {
            r: ((packed >> RED_SHIFT) & CHANNEL_MASK) as u8,
            g: ((packed >> GREEN_SHIFT) & CHANNEL_MASK) as u8,
            b: (packed & CHANNEL_MASK) as u8,
        }
    }
}

impl std::fmt::Debug for Rgb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// A single highlight attribute set, as defined by an `hl_attr_define` event.
///
/// Colors are `Option` because a group may leave fg/bg/special unset, in which
/// case the [`DefaultColors`] apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HlAttr {
    /// Foreground color, if this group sets one.
    pub fg: Option<Rgb>,
    /// Background color, if this group sets one.
    pub bg: Option<Rgb>,
    /// Special color (used for underline/undercurl), if set.
    pub special: Option<Rgb>,
    /// Bold text.
    pub bold: bool,
    /// Italic text.
    pub italic: bool,
    /// Underlined text.
    pub underline: bool,
    /// Curly underline.
    pub undercurl: bool,
    /// Strikethrough text.
    pub strikethrough: bool,
    /// Reverse video (swap fg/bg).
    pub reverse: bool,
}

/// The editor's default foreground/background/special colors, from
/// `default_colors_set`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DefaultColors {
    /// Default foreground color.
    pub fg: Option<Rgb>,
    /// Default background color.
    pub bg: Option<Rgb>,
    /// Default special color.
    pub special: Option<Rgb>,
}

/// A highlight resolved against the defaults, ready to paint.
///
/// `reverse` is applied (fg/bg swapped) so front-ends paint `fg`/`bg` directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResolvedStyle {
    /// Effective foreground color.
    pub fg: Option<Rgb>,
    /// Effective background color.
    pub bg: Option<Rgb>,
    /// Effective special color.
    pub special: Option<Rgb>,
    /// Bold text.
    pub bold: bool,
    /// Italic text.
    pub italic: bool,
    /// Underlined text.
    pub underline: bool,
    /// Curly underline.
    pub undercurl: bool,
    /// Strikethrough text.
    pub strikethrough: bool,
}

/// The highlight table: the `hl_id -> attributes` map plus the editor defaults.
#[derive(Clone, Debug, Default)]
pub struct HighlightTable {
    /// Default colors from `default_colors_set`.
    pub defaults: DefaultColors,
    attrs: HashMap<u64, HlAttr>,
}

impl HighlightTable {
    /// Creates an empty highlight table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Defines (or replaces) the attributes for `hl_id`.
    pub fn define(&mut self, hl_id: u64, attr: HlAttr) {
        self.attrs.insert(hl_id, attr);
    }

    /// Sets the default colors (from `default_colors_set`).
    pub fn set_defaults(&mut self, defaults: DefaultColors) {
        self.defaults = defaults;
    }

    /// Resolves `hl_id` into a paint-ready [`ResolvedStyle`], applying defaults
    /// for any unset color and honoring `reverse` by swapping fg/bg.
    ///
    /// `hl_id == 0` is the default highlight and resolves to the defaults.
    #[must_use]
    pub fn resolve(&self, hl_id: u64) -> ResolvedStyle {
        let attr = self.attrs.get(&hl_id).copied().unwrap_or_default();

        let mut fg = attr.fg.or(self.defaults.fg);
        let mut bg = attr.bg.or(self.defaults.bg);
        let special = attr.special.or(self.defaults.special);

        if attr.reverse {
            std::mem::swap(&mut fg, &mut bg);
        }

        ResolvedStyle {
            fg,
            bg,
            special,
            bold: attr.bold,
            italic: attr.italic,
            underline: attr.underline,
            undercurl: attr.undercurl,
            strikethrough: attr.strikethrough,
        }
    }
}

/// A single grid cell: one grapheme cluster of text plus its highlight id.
///
/// For a double-width character, the leading cell carries the text and the
/// immediately following cell is a placeholder with empty `text`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// The displayed text (one grapheme cluster), or empty for the trailing
    /// placeholder cell of a double-width character.
    pub text: String,
    /// The highlight id resolved via [`HighlightTable`].
    pub hl_id: u64,
}

/// The cell grid: a `rows * cols` matrix in row-major order.
#[derive(Clone, Debug, Default)]
pub struct Grid {
    /// Number of columns.
    pub cols: usize,
    /// Number of rows.
    pub rows: usize,
    cells: Vec<Cell>,
}

impl Grid {
    /// Creates a blank grid of the given dimensions.
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
        }
    }

    /// Resizes the grid to `cols * rows`, clearing all cells.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.cells = vec![Cell::default(); cols * rows];
    }

    /// Clears every cell to the default (empty text, hl_id 0).
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
    }

    /// Returns the cell at `(row, col)`, or `None` if out of bounds.
    #[must_use]
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.index(row, col).map(|i| &self.cells[i])
    }

    /// Sets the cell at `(row, col)`. Out-of-bounds writes are ignored.
    pub fn set_cell(&mut self, row: usize, col: usize, cell: Cell) {
        if let Some(i) = self.index(row, col) {
            self.cells[i] = cell;
        }
    }

    /// Scrolls a rectangular region by `rows` (positive = up, negative = down),
    /// per the `grid_scroll` event semantics.
    ///
    /// `top`/`bot` are the inclusive-exclusive row bounds and `left`/`right` the
    /// column bounds of the affected region. Rows that move in from outside the
    /// region are left untouched (Neovim repaints them with `grid_line`).
    pub fn scroll(&mut self, top: usize, bot: usize, left: usize, right: usize, rows: i64) {
        if rows == 0 {
            return;
        }
        let left = left.min(self.cols);
        let right = right.min(self.cols);
        let top = top.min(self.rows);
        let bot = bot.min(self.rows);
        if left >= right || top >= bot {
            return;
        }

        if rows > 0 {
            // Content moves up: dst row = src row - rows.
            let rows = rows as usize;
            let mut dst = top;
            while dst + rows < bot {
                self.copy_row_range(dst + rows, dst, left, right);
                dst += 1;
            }
        } else {
            // Content moves down: dst row = src row - rows (rows negative).
            let mag = (-rows) as usize;
            if bot < mag {
                return;
            }
            let mut dst = bot;
            while dst > top + mag {
                dst -= 1;
                self.copy_row_range(dst - mag, dst, left, right);
            }
        }
    }

    /// Returns the concatenated text of `row` (one column per grapheme; the
    /// trailing placeholder of a wide char contributes nothing). Trailing spaces
    /// for empty cells are rendered as a single space each so column alignment is
    /// preserved for callers that slice the string.
    #[must_use]
    pub fn row_text(&self, row: usize) -> String {
        let mut s = String::new();
        if row >= self.rows {
            return s;
        }
        for col in 0..self.cols {
            // Safe: row/col are in bounds here.
            let cell = &self.cells[row * self.cols + col];
            if cell.text.is_empty() {
                // Empty text means either a never-painted cell or the trailing
                // placeholder of a wide char. Render a single space so columns
                // line up; wide-char trailing cells are already accounted for by
                // the leading wide glyph occupying its own column.
                s.push(' ');
            } else {
                s.push_str(&cell.text);
            }
        }
        s
    }

    fn index(&self, row: usize, col: usize) -> Option<usize> {
        if row < self.rows && col < self.cols {
            Some(row * self.cols + col)
        } else {
            None
        }
    }

    fn copy_row_range(&mut self, src_row: usize, dst_row: usize, left: usize, right: usize) {
        for col in left..right {
            let cell = self.cells[src_row * self.cols + col].clone();
            self.cells[dst_row * self.cols + col] = cell;
        }
    }
}

/// The shape of the cursor for a given mode, from `mode_info_set`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    /// A full-cell block cursor (typically normal mode).
    #[default]
    Block,
    /// A horizontal bar cursor (typically replace mode).
    Horizontal,
    /// A vertical bar cursor (typically insert mode).
    Vertical,
}

/// Cached descriptor for one cursor mode, from `mode_info_set`.
#[derive(Clone, Debug, Default)]
pub struct ModeInfo {
    /// The mode's short name (e.g. `"normal"`, `"insert"`).
    pub name: String,
    /// The cursor shape used in this mode.
    pub shape: CursorShape,
    /// Cell coverage percentage for bar cursors (1–100), if specified.
    pub cell_percentage: Option<u32>,
    /// Highlight attribute id used for the cursor in this mode, if specified.
    pub attr_id: Option<u64>,
}

/// The cursor's grid position and current mode index.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cursor {
    /// Cursor row (grid coordinate).
    pub row: usize,
    /// Cursor column (grid coordinate).
    pub col: usize,
    /// Index into [`Snapshot::mode_infos`] for the current mode.
    pub mode_idx: usize,
}

/// Viewport info for synthesizing a native scrollbar, from `win_viewport`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Viewport {
    /// First buffer line visible in the window (1-based, as nvim reports).
    pub topline: u64,
    /// One past the last visible buffer line.
    pub botline: u64,
    /// The cursor's buffer line.
    pub curline: u64,
    /// Total number of lines in the buffer.
    pub linecount: u64,
}

/// A complete, paint-ready frame of render state.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// The cell grid.
    pub grid: Grid,
    /// The highlight table (resolves `hl_id`s).
    pub highlights: HighlightTable,
    /// The cursor position and mode.
    pub cursor: Cursor,
    /// Cached mode descriptors from `mode_info_set`.
    pub mode_infos: Vec<ModeInfo>,
    /// Viewport info for the scrollbar.
    pub viewport: Viewport,
    /// Absolute, canonicalized path of the buffer currently being edited in
    /// nvim, or `None` for an unnamed buffer. Updated out-of-band from the
    /// redraw stream via the `wolf_current_file` notification (see
    /// [`crate::redraw`]); defaults to `None`.
    pub current_file: Option<PathBuf>,
    /// Monotonic frame sequence number, bumped on each `flush`.
    pub seq: u64,
}

impl Snapshot {
    /// Creates an empty snapshot sized to `cols * rows`.
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            grid: Grid::new(cols, rows),
            ..Self::default()
        }
    }
}

/// A shared, lock-guarded [`Snapshot`].
///
/// Cloning shares the same underlying lock (it is an `Arc`).
#[derive(Clone)]
pub struct SharedSnapshot {
    inner: Arc<RwLock<Snapshot>>,
}

impl SharedSnapshot {
    /// Creates a shared snapshot sized to `cols * rows`.
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Snapshot::new(cols, rows))),
        }
    }

    /// Acquires the read lock. Held for the duration of a paint.
    pub fn read(&self) -> RwLockReadGuard<'_, Snapshot> {
        self.inner.read()
    }

    /// Acquires an owned read guard (owning an `Arc` clone of the lock), so the
    /// guard's lifetime is independent of `&self`.
    pub fn read_arc(&self) -> OwnedSnapshotReadGuard {
        self.inner.read_arc()
    }

    /// Acquires the write lock. Per the locking invariant, callers must NOT hold
    /// this across an `.await`.
    pub fn write(&self) -> RwLockWriteGuard<'_, Snapshot> {
        self.inner.write()
    }

    /// Acquires the write lock from async code WITHOUT blocking the tokio worker
    /// thread while a reader is held.
    ///
    /// A naive `self.inner.write()` is a *synchronous* blocking call: if a
    /// front-end is holding a read guard (e.g. painting, or across a shutdown),
    /// it would pin the worker thread until the reader drops, which can starve the
    /// runtime. Instead we retry `try_write`, yielding to the scheduler between
    /// attempts, so the worker stays free to make other progress. Once acquired,
    /// the returned guard must still NOT be held across an `.await` (the redraw
    /// handler applies a whole batch synchronously and drops it before returning).
    pub async fn write_async(&self) -> RwLockWriteGuard<'_, Snapshot> {
        loop {
            if let Some(guard) = self.inner.try_write() {
                return guard;
            }
            tokio::task::yield_now().await;
        }
    }

    /// Clones the handle (shares the same underlying lock).
    #[must_use]
    pub fn clone_handle(&self) -> Self {
        self.clone()
    }
}
