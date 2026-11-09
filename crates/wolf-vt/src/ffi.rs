//! Hand-written declarations of the libghostty-vt C API subset `lib.rs` uses.
//!
//! Every name, value and layout here is checked against `include/ghostty/vt/*.h` in the
//! pinned 1.0.0 release tarball (`types.h`, `color.h`, `build_info.h`, `terminal.h`,
//! `render.h`, `screen.h`, `style.h`, `modes.h`, `point.h`, `grid_ref.h`, `selection.h`,
//! `formatter.h`, `key/event.h`, `key/encoder.h`, `mouse/event.h`, `mouse/encoder.h`,
//! `paste.h`, `focus.h`). `GhosttyResult` and every `GHOSTTY_ENUM_TYPED` enum are C `int`,
//! so they appear as `c_int` constants. Handles are opaque pointers. A struct whose first
//! field is `size` is a sized struct: the caller sets `size` to the struct's size.

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
opaque!(GhosttyFormatterImpl);
opaque!(GhosttyKeyEncoderImpl);
opaque!(GhosttyKeyEventImpl);
opaque!(GhosttyMouseEncoderImpl);
opaque!(GhosttyMouseEventImpl);

pub type GhosttyTerminal = *mut GhosttyTerminalImpl;
pub type GhosttyRenderState = *mut GhosttyRenderStateImpl;
pub type GhosttyRenderStateRowIterator = *mut GhosttyRenderStateRowIteratorImpl;
pub type GhosttyRenderStateRowCells = *mut GhosttyRenderStateRowCellsImpl;
pub type GhosttyFormatter = *mut GhosttyFormatterImpl;
pub type GhosttyKeyEncoder = *mut GhosttyKeyEncoderImpl;
pub type GhosttyKeyEvent = *mut GhosttyKeyEventImpl;
pub type GhosttyMouseEncoder = *mut GhosttyMouseEncoderImpl;
pub type GhosttyMouseEvent = *mut GhosttyMouseEventImpl;

/// `GhosttyCell` (`screen.h`): a packed cell value, read through `ghostty_cell_get`.
pub type GhosttyCell = u64;

/// `GhosttyMods` (`key/event.h`): a modifier bitmask.
pub type GhosttyMods = u16;

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

/// `GhosttyString` (`types.h`): a borrowed byte string.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyString {
    pub ptr: *const u8,
    pub len: usize,
}

/// `GhosttyTerminalScrollViewportValue` (`terminal.h`).
#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyTerminalScrollViewportValue {
    pub delta: isize,
    pub padding: [u64; 2],
}

/// `GhosttyTerminalScrollViewport` (`terminal.h`), passed by value.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyTerminalScrollViewport {
    pub tag: c_int,
    pub value: GhosttyTerminalScrollViewportValue,
}

/// `GhosttyTerminalScrollbar` (`terminal.h`).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GhosttyTerminalScrollbar {
    pub total: u64,
    pub offset: u64,
    pub len: u64,
}

/// `GhosttyTerminalModeConfig` (`terminal.h`); `mode` is a packed `GhosttyMode` (`modes.h`).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GhosttyTerminalModeConfig {
    pub mode: u16,
    pub value: bool,
}

/// `GhosttyRenderStateRowSelection` (`render.h`), a sized struct.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GhosttyRenderStateRowSelection {
    pub size: usize,
    pub start_x: u16,
    pub end_x: u16,
}

/// `GhosttyRenderStateCursor` (`render.h`), a sized struct.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GhosttyRenderStateCursor {
    pub size: usize,
    pub viewport_has_value: bool,
    pub viewport_x: u16,
    pub viewport_y: u16,
    pub wide_tail: bool,
    pub visible: bool,
    pub blinking: bool,
    pub password_input: bool,
    pub visual_style: c_int,
}

/// `GhosttyRenderStateColors` (`render.h`), a sized struct.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyRenderStateColors {
    pub size: usize,
    pub background: GhosttyColorRgb,
    pub foreground: GhosttyColorRgb,
    pub cursor: GhosttyColorRgb,
    pub cursor_has_value: bool,
    pub palette: [GhosttyColorRgb; 256],
}

/// `GhosttyStyleColor` (`style.h`): a one-byte tag and a three-byte value.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GhosttyStyleColor {
    pub tag: u8,
    pub value: [u8; 3],
}

/// `GhosttyStyle` (`style.h`), 16 bytes with a frozen layout.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GhosttyStyle {
    pub fg_color: GhosttyStyleColor,
    pub bg_color: GhosttyStyleColor,
    pub underline_color: GhosttyStyleColor,
    pub flags: u16,
    pub reserved: u16,
}

/// `GhosttyPointCoordinate` (`point.h`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyPointCoordinate {
    pub x: u16,
    pub y: u32,
}

/// `GhosttyPointValue` (`point.h`).
#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyPointValue {
    pub coordinate: GhosttyPointCoordinate,
    pub padding: [u64; 2],
}

/// `GhosttyPoint` (`point.h`), passed by value.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyPoint {
    pub tag: c_int,
    pub value: GhosttyPointValue,
}

/// `GhosttyGridRef` (`grid_ref.h`): an untracked reference, valid until the next mutation.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyGridRef {
    pub node: *mut c_void,
    pub x: u16,
    pub y: u16,
}

/// `GhosttySelection` (`selection.h`), with a frozen layout.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttySelection {
    pub start: GhosttyGridRef,
    pub end: GhosttyGridRef,
    pub rectangle: bool,
}

/// `GhosttyMousePosition` (`mouse/event.h`), passed by value.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyMousePosition {
    pub x: f32,
    pub y: f32,
}

/// `GhosttyMouseEncoderSize` (`mouse/encoder.h`), a sized struct.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GhosttyMouseEncoderSize {
    pub size: usize,
    pub screen_width: u32,
    pub screen_height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub padding_top: u32,
    pub padding_bottom: u32,
    pub padding_right: u32,
    pub padding_left: u32,
}

/// `GhosttyTerminalWritePtyFn` (`terminal.h`).
pub type GhosttyTerminalWritePtyFn =
    extern "C" fn(terminal: GhosttyTerminal, userdata: *mut c_void, data: *const u8, len: usize);
/// `GhosttyTerminalTitleChangedFn` and `GhosttyTerminalPwdChangedFn` (`terminal.h`).
pub type GhosttyTerminalNotifyFn = extern "C" fn(terminal: GhosttyTerminal, userdata: *mut c_void);
/// `GhosttyTerminalRenderHoldFn` (`terminal.h`).
pub type GhosttyTerminalRenderHoldFn =
    extern "C" fn(terminal: GhosttyTerminal, userdata: *mut c_void, held: bool);

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

// GhosttyCursorStyle (types.h).
pub const GHOSTTY_CURSOR_STYLE_BAR: c_int = 0;
pub const GHOSTTY_CURSOR_STYLE_BLOCK: c_int = 1;
pub const GHOSTTY_CURSOR_STYLE_UNDERLINE: c_int = 2;
pub const GHOSTTY_CURSOR_STYLE_BLOCK_HOLLOW: c_int = 3;

// GhosttyTerminalScrollViewportTag (terminal.h).
pub const GHOSTTY_TERMINAL_SCROLL_VIEWPORT_TOP: c_int = 0;
pub const GHOSTTY_TERMINAL_SCROLL_VIEWPORT_BOTTOM: c_int = 1;
pub const GHOSTTY_TERMINAL_SCROLL_VIEWPORT_DELTA: c_int = 2;

// GhosttyTerminalScreen (terminal.h).
pub const GHOSTTY_TERMINAL_SCREEN_ALTERNATE: c_int = 1;

// GhosttyTerminalOption (terminal.h).
pub const GHOSTTY_TERMINAL_OPT_USERDATA: c_int = 1;
pub const GHOSTTY_TERMINAL_OPT_WRITE_PTY: c_int = 2;
pub const GHOSTTY_TERMINAL_OPT_TITLE_CHANGED: c_int = 6;
pub const GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND: c_int = 12;
pub const GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND: c_int = 13;
pub const GHOSTTY_TERMINAL_OPT_COLOR_CURSOR: c_int = 14;
pub const GHOSTTY_TERMINAL_OPT_COLOR_PALETTE: c_int = 15;
pub const GHOSTTY_TERMINAL_OPT_SELECTION: c_int = 22;
pub const GHOSTTY_TERMINAL_OPT_PWD_CHANGED: c_int = 26;
pub const GHOSTTY_TERMINAL_OPT_RENDER_HOLD: c_int = 42;

// GhosttyTerminalData (terminal.h).
pub const GHOSTTY_TERMINAL_DATA_COLS: c_int = 1;
pub const GHOSTTY_TERMINAL_DATA_ROWS: c_int = 2;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_X: c_int = 3;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_Y: c_int = 4;
pub const GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN: c_int = 6;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE: c_int = 7;
pub const GHOSTTY_TERMINAL_DATA_SCROLLBAR: c_int = 9;
pub const GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING: c_int = 11;
pub const GHOSTTY_TERMINAL_DATA_TITLE: c_int = 12;
pub const GHOSTTY_TERMINAL_DATA_PWD: c_int = 13;
pub const GHOSTTY_TERMINAL_DATA_COLOR_FOREGROUND: c_int = 18;
pub const GHOSTTY_TERMINAL_DATA_COLOR_BACKGROUND: c_int = 19;
pub const GHOSTTY_TERMINAL_DATA_COLOR_CURSOR: c_int = 20;
pub const GHOSTTY_TERMINAL_DATA_COLOR_PALETTE: c_int = 21;
pub const GHOSTTY_TERMINAL_DATA_SELECTION: c_int = 31;
pub const GHOSTTY_TERMINAL_DATA_VIEWPORT_ACTIVE: c_int = 32;
pub const GHOSTTY_TERMINAL_DATA_MODE: c_int = 37;

// GhosttyRenderStateDirty (render.h).
pub const GHOSTTY_RENDER_STATE_DIRTY_FALSE: c_int = 0;
pub const GHOSTTY_RENDER_STATE_DIRTY_PARTIAL: c_int = 1;
pub const GHOSTTY_RENDER_STATE_DIRTY_FULL: c_int = 2;

// GhosttyRenderStateData (render.h).
pub const GHOSTTY_RENDER_STATE_DATA_COLS: c_int = 1;
pub const GHOSTTY_RENDER_STATE_DATA_ROWS: c_int = 2;
pub const GHOSTTY_RENDER_STATE_DATA_DIRTY: c_int = 3;
pub const GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR: c_int = 4;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR: c_int = 18;
pub const GHOSTTY_RENDER_STATE_DATA_COLORS: c_int = 19;

// GhosttyRenderStateRowData (render.h).
pub const GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY: c_int = 1;
pub const GHOSTTY_RENDER_STATE_ROW_DATA_CELLS: c_int = 3;
pub const GHOSTTY_RENDER_STATE_ROW_DATA_SELECTION: c_int = 4;

// GhosttyRenderStateRowCellsData (render.h).
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW: c_int = 1;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE: c_int = 2;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR: c_int = 5;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR: c_int = 6;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8: c_int = 9;

// GhosttyCellData and GhosttyCellWide (screen.h).
pub const GHOSTTY_CELL_DATA_WIDE: c_int = 3;
pub const GHOSTTY_CELL_WIDE_NARROW: c_int = 0;
pub const GHOSTTY_CELL_WIDE_WIDE: c_int = 1;
pub const GHOSTTY_CELL_WIDE_SPACER_TAIL: c_int = 2;
pub const GHOSTTY_CELL_WIDE_SPACER_HEAD: c_int = 3;

// GhosttyStyleFlags (style.h); the underline style is a GhosttySgrUnderline (sgr.h).
pub const GHOSTTY_STYLE_FLAG_BOLD: u16 = 1 << 0;
pub const GHOSTTY_STYLE_FLAG_ITALIC: u16 = 1 << 1;
pub const GHOSTTY_STYLE_FLAG_FAINT: u16 = 1 << 2;
pub const GHOSTTY_STYLE_FLAG_BLINK: u16 = 1 << 3;
pub const GHOSTTY_STYLE_FLAG_INVERSE: u16 = 1 << 4;
pub const GHOSTTY_STYLE_FLAG_INVISIBLE: u16 = 1 << 5;
pub const GHOSTTY_STYLE_FLAG_STRIKETHROUGH: u16 = 1 << 6;
pub const GHOSTTY_STYLE_UNDERLINE_SHIFT: u16 = 8;
pub const GHOSTTY_STYLE_UNDERLINE_MASK: u16 = 0x7 << GHOSTTY_STYLE_UNDERLINE_SHIFT;
pub const GHOSTTY_SGR_UNDERLINE_SINGLE: u16 = 1;
pub const GHOSTTY_SGR_UNDERLINE_DOUBLE: u16 = 2;
pub const GHOSTTY_SGR_UNDERLINE_CURLY: u16 = 3;
pub const GHOSTTY_SGR_UNDERLINE_DOTTED: u16 = 4;
pub const GHOSTTY_SGR_UNDERLINE_DASHED: u16 = 5;

// GhosttyPointTag (point.h).
pub const GHOSTTY_POINT_TAG_VIEWPORT: c_int = 1;

// GhosttyFormatterOption (formatter.h).
pub const GHOSTTY_FORMATTER_OPT_UNWRAP: c_int = 2;
pub const GHOSTTY_FORMATTER_OPT_TRIM: c_int = 3;
pub const GHOSTTY_FORMATTER_OPT_SELECTION: c_int = 4;

// GhosttyKeyAction (key/event.h).
pub const GHOSTTY_KEY_ACTION_PRESS: c_int = 1;

// GhosttyMods (key/event.h).
pub const GHOSTTY_MODS_SHIFT: GhosttyMods = 1 << 0;
pub const GHOSTTY_MODS_CTRL: GhosttyMods = 1 << 1;
pub const GHOSTTY_MODS_ALT: GhosttyMods = 1 << 2;
pub const GHOSTTY_MODS_SUPER: GhosttyMods = 1 << 3;

// GhosttyKey (key/event.h).
pub const GHOSTTY_KEY_UNIDENTIFIED: c_int = 0;
pub const GHOSTTY_KEY_BACKQUOTE: c_int = 1;
pub const GHOSTTY_KEY_BACKSLASH: c_int = 2;
pub const GHOSTTY_KEY_BRACKET_LEFT: c_int = 3;
pub const GHOSTTY_KEY_BRACKET_RIGHT: c_int = 4;
pub const GHOSTTY_KEY_COMMA: c_int = 5;
pub const GHOSTTY_KEY_DIGIT_0: c_int = 6;
pub const GHOSTTY_KEY_EQUAL: c_int = 16;
pub const GHOSTTY_KEY_A: c_int = 20;
pub const GHOSTTY_KEY_MINUS: c_int = 46;
pub const GHOSTTY_KEY_PERIOD: c_int = 47;
pub const GHOSTTY_KEY_QUOTE: c_int = 48;
pub const GHOSTTY_KEY_SEMICOLON: c_int = 49;
pub const GHOSTTY_KEY_SLASH: c_int = 50;
pub const GHOSTTY_KEY_BACKSPACE: c_int = 53;
pub const GHOSTTY_KEY_ENTER: c_int = 58;
pub const GHOSTTY_KEY_SPACE: c_int = 63;
pub const GHOSTTY_KEY_TAB: c_int = 64;
pub const GHOSTTY_KEY_DELETE: c_int = 68;
pub const GHOSTTY_KEY_END: c_int = 69;
pub const GHOSTTY_KEY_HOME: c_int = 71;
pub const GHOSTTY_KEY_INSERT: c_int = 72;
pub const GHOSTTY_KEY_PAGE_DOWN: c_int = 73;
pub const GHOSTTY_KEY_PAGE_UP: c_int = 74;
pub const GHOSTTY_KEY_ARROW_DOWN: c_int = 75;
pub const GHOSTTY_KEY_ARROW_LEFT: c_int = 76;
pub const GHOSTTY_KEY_ARROW_RIGHT: c_int = 77;
pub const GHOSTTY_KEY_ARROW_UP: c_int = 78;
pub const GHOSTTY_KEY_ESCAPE: c_int = 120;
pub const GHOSTTY_KEY_F1: c_int = 121;
pub const GHOSTTY_KEY_F25: c_int = 145;

// GhosttyKeyEncoderOption and GhosttyOptionAsAlt (key/encoder.h).
pub const GHOSTTY_KEY_ENCODER_OPT_MACOS_OPTION_AS_ALT: c_int = 7;
pub const GHOSTTY_OPTION_AS_ALT_TRUE: c_int = 1;

// GhosttyMouseAction and GhosttyMouseButton (mouse/event.h).
pub const GHOSTTY_MOUSE_ACTION_PRESS: c_int = 0;
pub const GHOSTTY_MOUSE_ACTION_RELEASE: c_int = 1;
pub const GHOSTTY_MOUSE_ACTION_MOTION: c_int = 2;
pub const GHOSTTY_MOUSE_BUTTON_LEFT: c_int = 1;
pub const GHOSTTY_MOUSE_BUTTON_RIGHT: c_int = 2;
pub const GHOSTTY_MOUSE_BUTTON_MIDDLE: c_int = 3;
pub const GHOSTTY_MOUSE_BUTTON_FOUR: c_int = 4;
pub const GHOSTTY_MOUSE_BUTTON_FIVE: c_int = 5;

// GhosttyMouseEncoderOption (mouse/encoder.h).
pub const GHOSTTY_MOUSE_ENCODER_OPT_SIZE: c_int = 3;
pub const GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED: c_int = 4;

// GhosttyFocusEvent (focus.h).
pub const GHOSTTY_FOCUS_GAINED: c_int = 0;
pub const GHOSTTY_FOCUS_LOST: c_int = 1;

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
    pub fn ghostty_terminal_scroll_viewport(
        terminal: GhosttyTerminal,
        behavior: GhosttyTerminalScrollViewport,
    ) -> c_int;
    pub fn ghostty_terminal_get(terminal: GhosttyTerminal, data: c_int, out: *mut c_void) -> c_int;
    pub fn ghostty_terminal_grid_ref(
        terminal: GhosttyTerminal,
        point: GhosttyPoint,
        out_ref: *mut GhosttyGridRef,
    ) -> c_int;

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
    pub fn ghostty_render_state_clean(state: GhosttyRenderState) -> c_int;
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

    // screen.h
    pub fn ghostty_cell_get(cell: GhosttyCell, data: c_int, out: *mut c_void) -> c_int;

    // formatter.h
    pub fn ghostty_formatter_terminal_new(
        allocator: *const GhosttyAllocator,
        formatter: *mut GhosttyFormatter,
        terminal: GhosttyTerminal,
    ) -> c_int;
    pub fn ghostty_formatter_set(
        formatter: GhosttyFormatter,
        option: c_int,
        value: *const c_void,
    ) -> c_int;
    pub fn ghostty_formatter_format_buf(
        formatter: GhosttyFormatter,
        buf: *mut u8,
        buf_len: usize,
        out_written: *mut usize,
    ) -> c_int;
    pub fn ghostty_formatter_free(formatter: GhosttyFormatter);

    // key/event.h
    pub fn ghostty_key_event_new(
        allocator: *const GhosttyAllocator,
        event: *mut GhosttyKeyEvent,
    ) -> c_int;
    pub fn ghostty_key_event_free(event: GhosttyKeyEvent);
    pub fn ghostty_key_event_set_action(event: GhosttyKeyEvent, action: c_int) -> c_int;
    pub fn ghostty_key_event_set_key(event: GhosttyKeyEvent, key: c_int) -> c_int;
    pub fn ghostty_key_event_set_mods(event: GhosttyKeyEvent, mods: GhosttyMods) -> c_int;
    pub fn ghostty_key_event_set_consumed_mods(event: GhosttyKeyEvent, mods: GhosttyMods) -> c_int;
    pub fn ghostty_key_event_set_utf8(event: GhosttyKeyEvent, utf8: GhosttyString) -> c_int;
    pub fn ghostty_key_event_set_unshifted_codepoint(event: GhosttyKeyEvent, cp: u32) -> c_int;

    // key/encoder.h
    pub fn ghostty_key_encoder_new(
        allocator: *const GhosttyAllocator,
        encoder: *mut GhosttyKeyEncoder,
    ) -> c_int;
    pub fn ghostty_key_encoder_free(encoder: GhosttyKeyEncoder);
    pub fn ghostty_key_encoder_set(
        encoder: GhosttyKeyEncoder,
        option: c_int,
        value: *const c_void,
    ) -> c_int;
    pub fn ghostty_key_encoder_set_from_terminal(
        encoder: GhosttyKeyEncoder,
        terminal: GhosttyTerminal,
    ) -> c_int;
    pub fn ghostty_key_encoder_encode(
        encoder: GhosttyKeyEncoder,
        event: GhosttyKeyEvent,
        buf: *mut u8,
        buf_len: usize,
        out_written: *mut usize,
    ) -> c_int;

    // mouse/event.h
    pub fn ghostty_mouse_event_new(
        allocator: *const GhosttyAllocator,
        event: *mut GhosttyMouseEvent,
    ) -> c_int;
    pub fn ghostty_mouse_event_free(event: GhosttyMouseEvent);
    pub fn ghostty_mouse_event_set_action(event: GhosttyMouseEvent, action: c_int) -> c_int;
    pub fn ghostty_mouse_event_set_button(event: GhosttyMouseEvent, button: c_int) -> c_int;
    pub fn ghostty_mouse_event_clear_button(event: GhosttyMouseEvent) -> c_int;
    pub fn ghostty_mouse_event_set_mods(event: GhosttyMouseEvent, mods: GhosttyMods) -> c_int;
    pub fn ghostty_mouse_event_set_position(
        event: GhosttyMouseEvent,
        position: GhosttyMousePosition,
    ) -> c_int;

    // mouse/encoder.h
    pub fn ghostty_mouse_encoder_new(
        allocator: *const GhosttyAllocator,
        encoder: *mut GhosttyMouseEncoder,
    ) -> c_int;
    pub fn ghostty_mouse_encoder_free(encoder: GhosttyMouseEncoder);
    pub fn ghostty_mouse_encoder_set(
        encoder: GhosttyMouseEncoder,
        option: c_int,
        value: *const c_void,
    ) -> c_int;
    pub fn ghostty_mouse_encoder_set_from_terminal(
        encoder: GhosttyMouseEncoder,
        terminal: GhosttyTerminal,
    ) -> c_int;
    pub fn ghostty_mouse_encoder_encode(
        encoder: GhosttyMouseEncoder,
        event: GhosttyMouseEvent,
        buf: *mut u8,
        buf_len: usize,
        out_written: *mut usize,
    ) -> c_int;

    // paste.h
    pub fn ghostty_paste_encode(
        data: *mut c_char,
        data_len: usize,
        bracketed: bool,
        buf: *mut u8,
        buf_len: usize,
        out_written: *mut usize,
    ) -> c_int;

    // focus.h
    pub fn ghostty_focus_encode(
        event: c_int,
        buf: *mut u8,
        buf_len: usize,
        out_written: *mut usize,
    ) -> c_int;
}
