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
use std::sync::{Arc, OnceLock};

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

/// The outer/global grid id. Under `ext_multigrid` grid 1 always exists (Neovim
/// resizes it; the command line and messages live there), but editor window
/// content lives on grids `>= 2`. Grid 1 is never painted as window content and
/// is never the active grid while a window grid exists.
pub(crate) const OUTER_GRID_ID: u64 = 1;

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

    /// Resizes the grid to `cols * rows`, PRESERVING the content of every cell that
    /// is within both the old and new bounds. Newly exposed cells are blank.
    ///
    /// This matches the Neovim `ui-linegrid` `grid_resize` contract: a resize is not
    /// a clear (there is a separate `grid_clear` event). Neovim does not re-send
    /// `grid_line` for rows it considers unchanged after a resize, so a UI that
    /// cleared on resize would blank surviving content (e.g. the still-open pane
    /// after a split or close). The overlapping region is therefore copied across.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let mut next = vec![Cell::default(); cols * rows];
        let copy_cols = cols.min(self.cols);
        let copy_rows = rows.min(self.rows);
        for row in 0..copy_rows {
            for col in 0..copy_cols {
                next[row * cols + col] = self.cells[row * self.cols + col].clone();
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.cells = next;
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
    /// The grid id the cursor currently lives on (the active window grid). Set by
    /// `grid_cursor_goto`; [`Snapshot::new`] points it at [`OUTER_GRID_ID`].
    pub grid: u64,
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
    /// First buffer line visible in the window (0-based buffer line index, as
    /// `win_viewport` reports).
    pub topline: u64,
    /// One past the last visible buffer line (0-based, exclusive).
    pub botline: u64,
    /// The cursor's buffer line (0-based buffer line index).
    pub curline: u64,
    /// Total number of lines in the buffer.
    pub linecount: u64,
}

/// Per-window layout + viewport info, learned from `win_pos`/`win_viewport`.
///
/// Keyed in [`Snapshot::windows`] by the window's grid id. The window-handle
/// (`win`) is stored as an `i64` — the same form used for `nvim_set_current_win`
/// / `nvim_win_close` later — decoded from the redraw stream by
/// [`crate::redraw`] (a `win` arrives either as a plain integer or as a
/// msgpack-`Ext` wrapping an encoded integer).
#[derive(Clone, Copy, Debug, Default)]
pub struct WinInfo {
    /// The grid id this window draws into (`>= 2` for editor windows).
    pub grid: u64,
    /// The Neovim window handle, as an `i64` (see the struct docs).
    pub win: i64,
    /// Window top edge, in outer-grid rows (from `win_pos`).
    pub start_row: usize,
    /// Window left edge, in outer-grid columns (from `win_pos`).
    pub start_col: usize,
    /// Window width in columns (from `win_pos`).
    pub width: usize,
    /// Window height in rows (from `win_pos`).
    pub height: usize,
    /// This window's scroll viewport (from `win_viewport`).
    pub viewport: Viewport,
    /// Historically "whether the window is hidden (from `win_hide`)", but now
    /// effectively ALWAYS `false` and unused (OD8). On a `win_hide` (most notably
    /// a tabpage switch, which hides every window of the leaving tabpage) the
    /// handler REMOVES the window from `Snapshot.windows` entirely rather than
    /// flagging it here, so `windows`/`grid_for_win` reflect only the current
    /// tabpage. The field is retained only to avoid churning the FFI + Swift
    /// surface (`FfiWinInfo::hidden()` / the `!w.hidden()` filters still compile
    /// and behave correctly, since a live window is never hidden).
    pub hidden: bool,
}

/// The effective colors of one app-level theme highlight group (`WolfCursor` /
/// `WolfTitlebar`), as reported by the `wolf_theme` notification.
///
/// Both channels are `Option` because a colorscheme may leave either unset (or
/// define no group at all / clear it), in which case the front-end falls back to
/// its default behavior. The all-`None` default means "group absent".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThemeGroup {
    /// Foreground color, if the group sets one.
    pub fg: Option<Rgb>,
    /// Background color, if the group sets one.
    pub bg: Option<Rgb>,
}

/// The app-level theme colors driven by the optional `WolfCursor` /
/// `WolfTitlebar` highlight groups (see [`crate::redraw`]'s `wolf_theme`
/// handling). All-`None` by default; front-ends fall back to their existing
/// behavior for any unset channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThemeColors {
    /// `WolfCursor`: bg drives the app cursor fill, fg the glyph color.
    pub cursor: ThemeGroup,
    /// `WolfTitlebar`: bg drives the macOS titlebar background; fg is carried
    /// for future title-text coloring.
    pub titlebar: ThemeGroup,
}

/// Where a preview pane's source Markdown lives, learned from the `wolf_preview`
/// notification.
///
/// A preview pane is backed by a real Neovim scratch buffer named
/// `Preview: <basename>` carrying `b:wolf_preview_source` (OD1 = Design S). The
/// `wolf_preview` autocmd scans windows for that buffer variable and reports, per
/// preview window (keyed in [`Snapshot::previews`] by the preview WINDOW handle),
/// the SOURCE buffer's number and absolute path so the front-end can pull rendered
/// HTML via [`crate::api::WolfCore::render_preview`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewInfo {
    /// The Neovim buffer number of the SOURCE Markdown buffer (NOT the scratch
    /// preview buffer). Keys into [`Snapshot::markdown_ticks`] for the live tick.
    pub source_bufnr: i64,
    /// Absolute path of the source Markdown file.
    pub source_path: PathBuf,
}

/// A complete, paint-ready frame of render state.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// All live cell grids, keyed by grid id. Always contains [`OUTER_GRID_ID`]
    /// (the global/outer grid); editor window content lives on grids `>= 2`.
    pub grids: HashMap<u64, Grid>,
    /// Per-window layout + viewport info, keyed by the window's grid id.
    pub windows: HashMap<u64, WinInfo>,
    /// The highlight table (resolves `hl_id`s).
    pub highlights: HighlightTable,
    /// The cursor position and mode (including the grid it lives on).
    pub cursor: Cursor,
    /// Cached mode descriptors from `mode_info_set`.
    pub mode_infos: Vec<ModeInfo>,
    /// The currently active Neovim window handle, or `None` if not yet known.
    /// Updated out-of-band from the redraw stream via the `wolf_active_win`
    /// notification, emitted by the `WolfLayout` `WinEnter` autocmd in
    /// `runtime/nvim/init.lua` (see [`crate::redraw`]); the front-end uses it to
    /// mark the active pane.
    pub active_win: Option<i64>,
    /// The current Neovim TABPAGE handle (as an `i64`), or `0` when not yet
    /// known. Updated out-of-band from the redraw stream via the `wolf_tabpage`
    /// notification, emitted by the `wolf_setup_tabs` `TabEnter`/`TabNewEntered`
    /// autocmds in `runtime/nvim/init.lua` (see [`crate::redraw`]). Because it is
    /// not a redraw frame, updating it does NOT bump `seq`. The macOS front-end
    /// uses it as the cheap "is my tabpage current" refocus-rebind gate (mirrors
    /// `active_win`); the authoritative bind-time value is
    /// [`crate::api::WolfCore::current_tabpage`].
    pub current_tabpage: i64,
    /// Absolute, canonicalized path of the buffer currently being edited in
    /// nvim, or `None` for an unnamed buffer. Updated out-of-band from the
    /// redraw stream via the `wolf_current_file` notification (see
    /// [`crate::redraw`]); defaults to `None`.
    pub current_file: Option<PathBuf>,
    /// Per-window buffer paths, keyed by the Neovim WINDOW handle (`WinInfo.win`,
    /// an `i64`), each an absolute canonicalized buffer path. Unnamed windows are
    /// ABSENT from the map. Updated out-of-band from the redraw stream via the
    /// `wolf_buffers` notification (see [`crate::redraw`]): the whole map is
    /// WHOLESALE-REPLACED on each emit, so stale/closed windows vanish. Because it
    /// is not a redraw frame, updating it does NOT bump `seq`; the front-end joins
    /// a grid → window → buffer path via [`crate::api::WolfCore::pane_status`].
    /// Defaults to empty.
    pub buffers: HashMap<i64, PathBuf>,
    /// The app-level theme colors from the optional `WolfCursor` /
    /// `WolfTitlebar` highlight groups. Updated out-of-band from the redraw
    /// stream via the `wolf_theme` notification (a `ColorScheme` autocmd plus
    /// an emit-once installed by `runtime/nvim/init.lua`; see
    /// [`crate::redraw`]); all-`None` by default and whenever a group is
    /// absent or cleared.
    pub theme: ThemeColors,
    /// The filetype (`vim.bo.filetype`) of the buffer currently being edited in
    /// nvim, or the empty string when unknown. Updated out-of-band from the redraw
    /// stream via the `wolf_current_file` notification (a 2nd argument added to the
    /// existing current-file autocmd, which now also fires on `FileType`; see
    /// [`crate::redraw`]). Because it is not a redraw frame, updating it does NOT
    /// bump `seq`. Drives [`Snapshot::is_markdown_active`], the Preview command's
    /// enable gate. Defaults to empty.
    pub current_filetype: String,
    /// Whether the buffer currently being edited in nvim is a PREVIEW buffer (it
    /// carries `b:wolf_preview_source`). Updated out-of-band from the redraw stream
    /// via the `wolf_current_file` notification (a 3rd boolean argument added to the
    /// existing current-file autocmd emit; see [`crate::redraw`]). Because it is not
    /// a redraw frame, updating it does NOT bump `seq`. Drives
    /// [`Snapshot::is_preview_active`], the Edit command's enable gate. Defaults to
    /// `false`.
    pub current_is_preview: bool,
    /// Per-preview-window source info, keyed by the preview WINDOW handle (an
    /// `i64`). Updated out-of-band from the redraw stream via the `wolf_preview`
    /// notification (see [`crate::redraw`]): the whole map is WHOLESALE-REPLACED on
    /// each emit, so closed preview windows vanish. Because it is not a redraw
    /// frame, updating it does NOT bump `seq`. Defaults to empty.
    pub previews: HashMap<i64, PreviewInfo>,
    /// Per-source-buffer `changedtick`, keyed by the SOURCE Markdown buffer number
    /// (an `i64`). Updated out-of-band from the redraw stream via the
    /// `wolf_markdown_dirty` notification (see [`crate::redraw`]): each emit upserts
    /// one `[bufnr, tick]` entry. The front-end joins a preview's `source_bufnr` to
    /// its tick to decide when to re-pull rendered HTML. Because it is not a redraw
    /// frame, updating it does NOT bump `seq`. Defaults to empty.
    pub markdown_ticks: HashMap<i64, u64>,
    /// Monotonic frame sequence number, bumped on each `flush`.
    pub seq: u64,
}

/// Returns a shared, empty `0x0` grid used as the [`Snapshot::active_grid`]
/// fallback when no grid is present (so callers never deref a missing grid).
fn empty_grid() -> &'static Grid {
    static EMPTY: OnceLock<Grid> = OnceLock::new();
    EMPTY.get_or_init(|| Grid::new(0, 0))
}

impl Snapshot {
    /// Creates an empty snapshot sized to `cols * rows`.
    ///
    /// Inserts the outer grid ([`OUTER_GRID_ID`]) at `cols * rows` and points the
    /// cursor at it. The `windows` map starts empty and `active_win` is `None`.
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        let mut grids = HashMap::new();
        grids.insert(OUTER_GRID_ID, Grid::new(cols, rows));
        Self {
            grids,
            cursor: Cursor {
                grid: OUTER_GRID_ID,
                ..Cursor::default()
            },
            ..Self::default()
        }
    }

    /// Returns the id of the active grid (the cursor's window grid).
    ///
    /// Resolution order: the cursor's grid if it exists; else the highest
    /// non-outer grid id present; else the outer grid if present; else `0`.
    #[must_use]
    pub fn active_grid_id(&self) -> u64 {
        if self.grids.contains_key(&self.cursor.grid) {
            return self.cursor.grid;
        }
        if let Some(&id) = self.grids.keys().filter(|&&id| id != OUTER_GRID_ID).max() {
            return id;
        }
        if self.grids.contains_key(&OUTER_GRID_ID) {
            return OUTER_GRID_ID;
        }
        0
    }

    /// Returns `true` when the current cursor mode is Neovim visual mode.
    ///
    /// Resolves `mode_infos[cursor.mode_idx]` and matches `name == "visual"`.
    /// Neovim reports a single `"visual"` cursor-mode-info for all of `v` / `V`
    /// / `<C-v>` (charwise/linewise/blockwise); this deliberately excludes
    /// `"visual_select"` (select mode).
    #[must_use]
    pub fn is_visual_mode(&self) -> bool {
        self.mode_infos
            .get(self.cursor.mode_idx)
            .is_some_and(|mode| mode.name == "visual")
    }

    /// Returns `true` when the buffer currently being edited is a Markdown file.
    ///
    /// Resolves `current_filetype == "markdown"` (mirrors [`is_visual_mode`]'s
    /// "resolve one field, match a string" shape). This is the enable gate for the
    /// Preview command (front-end `validateMenuItem` / the palette row).
    ///
    /// [`is_visual_mode`]: Snapshot::is_visual_mode
    #[must_use]
    pub fn is_markdown_active(&self) -> bool {
        self.current_filetype == "markdown"
    }

    /// Returns `true` when the buffer currently being edited is a PREVIEW buffer.
    ///
    /// Resolves `current_is_preview` (mirrors [`is_markdown_active`]'s "resolve one
    /// field" shape). This is the enable gate for the Edit command (front-end
    /// `validateMenuItem`).
    ///
    /// [`is_markdown_active`]: Snapshot::is_markdown_active
    #[must_use]
    pub fn is_preview_active(&self) -> bool {
        self.current_is_preview
    }

    /// Returns the active grid (see [`Snapshot::active_grid_id`]), or a shared
    /// empty `0x0` grid if no grid is present.
    #[must_use]
    pub fn active_grid(&self) -> &Grid {
        self.grids
            .get(&self.active_grid_id())
            .unwrap_or_else(|| empty_grid())
    }

    /// Returns the grid with id `id`, if present.
    #[must_use]
    pub fn grid(&self, id: u64) -> Option<&Grid> {
        self.grids.get(&id)
    }

    /// Returns a mutable reference to the grid with id `id`, creating an empty
    /// `0x0` grid (which a subsequent `grid_resize` will size) if it is absent.
    pub fn grid_mut(&mut self, id: u64) -> &mut Grid {
        self.grids.entry(id).or_insert_with(|| Grid::new(0, 0))
    }

    /// Returns the active grid's viewport (the active window's scroll state), or
    /// the default viewport if the active grid has no associated window.
    #[must_use]
    pub fn active_viewport(&self) -> Viewport {
        self.windows
            .get(&self.active_grid_id())
            .map_or_else(Viewport::default, |w| w.viewport)
    }

    /// Returns the grid id bound to window handle `win`, if any window maps to it.
    #[must_use]
    pub fn grid_for_win(&self, win: i64) -> Option<u64> {
        self.windows
            .values()
            .find(|w| w.win == win)
            .map(|w| w.grid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid width used to construct a fresh [`Snapshot`] under test. Any positive
    /// size works; `is_markdown_active` does not depend on grid dimensions.
    const TEST_COLS: usize = 80;
    /// A grid height used to construct a fresh [`Snapshot`] under test.
    const TEST_ROWS: usize = 24;

    /// `is_markdown_active()` is `true` IFF `current_filetype == "markdown"`
    /// (mirrors `is_visual_mode`'s "resolve one field, match a string" shape). A
    /// fresh snapshot has an empty filetype (not markdown); setting it to
    /// `"markdown"` flips it on; any other filetype flips it off again.
    #[test]
    fn is_markdown_active_true_only_for_markdown_filetype() {
        let mut snap = Snapshot::new(TEST_COLS, TEST_ROWS);
        assert!(
            !snap.is_markdown_active(),
            "a fresh snapshot has an empty filetype and is not markdown"
        );

        snap.current_filetype = "markdown".to_string();
        assert!(
            snap.is_markdown_active(),
            "current_filetype == \"markdown\" must report markdown active"
        );

        snap.current_filetype = "rust".to_string();
        assert!(
            !snap.is_markdown_active(),
            "a non-markdown filetype must report markdown inactive"
        );
    }

    /// `is_preview_active()` is `true` IFF `current_is_preview` is set (mirrors
    /// `is_markdown_active`'s "resolve one field" shape). A fresh snapshot has
    /// `current_is_preview == false` (not a preview); setting it flips it on.
    #[test]
    fn is_preview_active_true_only_when_current_is_preview_set() {
        let mut snap = Snapshot::new(TEST_COLS, TEST_ROWS);
        assert!(
            !snap.is_preview_active(),
            "a fresh snapshot has current_is_preview == false and is not a preview"
        );

        snap.current_is_preview = true;
        assert!(
            snap.is_preview_active(),
            "current_is_preview == true must report a preview active"
        );
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
