//! Hand-written declarations of the libghostty-vt C API subset `lib.rs` uses.
//!
//! Every name, value and layout here is checked against `include/ghostty/vt/*.h` in the
//! pinned 1.0.0 release tarball (`types.h`, `color.h`, `build_info.h`, `terminal.h`,
//! `render.h`). `GhosttyResult` and every `GHOSTTY_ENUM_TYPED` enum are C `int`, so they
//! appear as `c_int` constants. Handles are opaque pointers.

use std::ffi::{c_char, c_int, c_void};

/// An opaque C struct, only ever behind a pointer.
macro_rules! opaque {
    ($name:ident) => {
        #[repr(C)]
        pub struct $name {
            _private: [u8; 0],
        }
    };
}

opaque!(GhosttyAllocator);
opaque!(GhosttyTerminalImpl);
opaque!(GhosttyRenderStateImpl);
opaque!(GhosttyRenderStateRowIteratorImpl);
opaque!(GhosttyRenderStateRowCellsImpl);

pub type GhosttyTerminal = *mut GhosttyTerminalImpl;
pub type GhosttyRenderState = *mut GhosttyRenderStateImpl;
pub type GhosttyRenderStateRowIterator = *mut GhosttyRenderStateRowIteratorImpl;
pub type GhosttyRenderStateRowCells = *mut GhosttyRenderStateRowCellsImpl;

/// `GhosttyColorRgb` (`color.h`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GhosttyColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// `GhosttyBuffer` (`types.h`): a caller-owned byte buffer. On `GHOSTTY_OUT_OF_SPACE`,
/// `len` is the capacity the value needs.
#[repr(C)]
pub struct GhosttyBuffer {
    pub ptr: *mut u8,
    pub cap: usize,
    pub len: usize,
}

// GhosttyResult (types.h).
pub const GHOSTTY_SUCCESS: c_int = 0;
pub const GHOSTTY_OUT_OF_MEMORY: c_int = -1;
pub const GHOSTTY_INVALID_VALUE: c_int = -2;
pub const GHOSTTY_OUT_OF_SPACE: c_int = -3;
pub const GHOSTTY_NO_VALUE: c_int = -4;
pub const GHOSTTY_IO_ERROR: c_int = -5;
pub const GHOSTTY_LIMIT_EXCEEDED: c_int = -6;
pub const GHOSTTY_REJECTED: c_int = -7;
pub const GHOSTTY_UNSUPPORTED: c_int = -8;

// GhosttyTerminalOption (terminal.h).
pub const GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND: c_int = 12;
pub const GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND: c_int = 13;

// GhosttyTerminalData (terminal.h).
pub const GHOSTTY_TERMINAL_DATA_COLS: c_int = 1;
pub const GHOSTTY_TERMINAL_DATA_ROWS: c_int = 2;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_X: c_int = 3;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_Y: c_int = 4;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE: c_int = 7;
pub const GHOSTTY_TERMINAL_DATA_COLOR_FOREGROUND: c_int = 18;
pub const GHOSTTY_TERMINAL_DATA_COLOR_BACKGROUND: c_int = 19;
pub const GHOSTTY_TERMINAL_DATA_COLOR_PALETTE: c_int = 21;

// GhosttyRenderStateData (render.h).
pub const GHOSTTY_RENDER_STATE_DATA_COLS: c_int = 1;
pub const GHOSTTY_RENDER_STATE_DATA_ROWS: c_int = 2;
pub const GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR: c_int = 4;

// GhosttyRenderStateRowData (render.h).
pub const GHOSTTY_RENDER_STATE_ROW_DATA_CELLS: c_int = 3;

// GhosttyRenderStateRowCellsData (render.h).
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR: c_int = 5;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR: c_int = 6;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8: c_int = 9;

extern "C" {
    // build_info.h
    pub fn ghostty_version() -> *const c_char;

    // terminal.h
    pub fn ghostty_terminal_new(
        allocator: *const GhosttyAllocator,
        terminal: *mut GhosttyTerminal,
        cols: u16,
        rows: u16,
    ) -> c_int;
    pub fn ghostty_terminal_free(terminal: GhosttyTerminal);
    pub fn ghostty_terminal_resize(
        terminal: GhosttyTerminal,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> c_int;
    pub fn ghostty_terminal_set(
        terminal: GhosttyTerminal,
        option: c_int,
        value: *const c_void,
    ) -> c_int;
    pub fn ghostty_terminal_vt_write(terminal: GhosttyTerminal, data: *const u8, len: usize);
    pub fn ghostty_terminal_get(terminal: GhosttyTerminal, data: c_int, out: *mut c_void) -> c_int;

    // render.h
    pub fn ghostty_render_state_new(
        allocator: *const GhosttyAllocator,
        state: *mut GhosttyRenderState,
    ) -> c_int;
    pub fn ghostty_render_state_free(state: GhosttyRenderState);
    pub fn ghostty_render_state_update(
        state: GhosttyRenderState,
        terminal: GhosttyTerminal,
    ) -> c_int;
    pub fn ghostty_render_state_get(
        state: GhosttyRenderState,
        data: c_int,
        out: *mut c_void,
    ) -> c_int;
    pub fn ghostty_render_state_row_iterator_new(
        allocator: *const GhosttyAllocator,
        out_iterator: *mut GhosttyRenderStateRowIterator,
    ) -> c_int;
    pub fn ghostty_render_state_row_iterator_free(iterator: GhosttyRenderStateRowIterator);
    pub fn ghostty_render_state_row_iterator_next(iterator: GhosttyRenderStateRowIterator) -> bool;
    pub fn ghostty_render_state_row_get(
        iterator: GhosttyRenderStateRowIterator,
        data: c_int,
        out: *mut c_void,
    ) -> c_int;
    pub fn ghostty_render_state_row_cells_new(
        allocator: *const GhosttyAllocator,
        out_cells: *mut GhosttyRenderStateRowCells,
    ) -> c_int;
    pub fn ghostty_render_state_row_cells_next(cells: GhosttyRenderStateRowCells) -> bool;
    pub fn ghostty_render_state_row_cells_get(
        cells: GhosttyRenderStateRowCells,
        data: c_int,
        out: *mut c_void,
    ) -> c_int;
    pub fn ghostty_render_state_row_cells_free(cells: GhosttyRenderStateRowCells);
}
