//! Headless, deterministic unit coverage for `FfiFrame`'s snapshot -> frame
//! packing.
//!
//! Unlike `tests/bridge_smoke.rs` — which needs a real, staged Neovim and loudly
//! SKIPS without one — this test builds a [`wolf_core::Snapshot`] BY HAND (no nvim,
//! no `WolfCore`, no Swift) and asserts exactly what [`FfiFrame::from_snapshot`]
//! produces: color packing, the style-flag bitset, reverse-video resolution, cursor
//! shape/visibility, the reverse-video cursor fill/glyph, the default colors, and
//! the flat `cells()` grid. It is real, non-mock coverage of the resolution/packing
//! logic that needs no live `WolfCore`.
//!
//! The `//crates/wolf-ffi:frame_pack` target is self-contained (no `common/`) and
//! names `//crates/wolf-core:wolf_core` directly so the test can build a
//! `wolf_core::Snapshot`.

use std::collections::HashMap;

use wolf_core::{
    Cell, Cursor, CursorShape, DefaultColors, Grid, HighlightTable, HlAttr, ModeInfo, Rgb,
    Snapshot, ThemeGroup,
};
use wolf_ffi::FfiFrame;

/// The (window) grid id the hand-built snapshots paint onto. Under
/// `ext_multigrid` editor window content lives on grids `>= 2`.
const GRID_ID: u64 = 2;
/// Columns in the default test grid.
const COLS: usize = 4;
/// Rows in the default test grid.
const ROWS: usize = 2;

// Style-flag bit layout, copied verbatim from `src/lib.rs`'s `mod flag` (the
// single source of truth the test asserts against — do not diverge).
/// Bold bit.
const BOLD: u8 = 1 << 0;
/// Italic bit.
const ITALIC: u8 = 1 << 1;
/// Underline bit.
const UNDERLINE: u8 = 1 << 2;
/// Undercurl bit.
const UNDERCURL: u8 = 1 << 3;
/// Strikethrough bit.
const STRIKETHROUGH: u8 = 1 << 4;

// Cursor-shape `u8` discriminants, copied verbatim from `src/lib.rs`'s
// `mod cursor_shape`.
/// Block cursor discriminant.
const SHAPE_BLOCK: u8 = 0;
/// Horizontal-bar cursor discriminant.
const SHAPE_HORIZONTAL: u8 = 1;
/// Vertical-bar cursor discriminant.
const SHAPE_VERTICAL: u8 = 2;

/// Builds a cursor on grid `grid` at `(row, col)`, mode index 0.
fn cursor_on(grid: u64, row: usize, col: usize) -> Cursor {
    Cursor {
        grid,
        row,
        col,
        mode_idx: 0,
    }
}

/// Assembles a single-grid snapshot from explicit parts (every other `Snapshot`
/// field defaults: no windows, no active win, seq 0).
#[allow(
    clippy::field_reassign_with_default,
    reason = "clippy's suggested `Snapshot { .., ..Default::default() }` does not compile \
              here: `Snapshot` carries a crate-private field (`pending_confirm`), and \
              functional-update syntax cannot fill a private field from another crate \
              (E0451). Build-then-assign is the only shape available."
)]
fn snapshot(
    grid_id: u64,
    grid: Grid,
    highlights: HighlightTable,
    cursor: Cursor,
    mode_infos: Vec<ModeInfo>,
) -> Snapshot {
    let mut grids = HashMap::new();
    grids.insert(grid_id, grid);
    // Default plus field assignment, not `..Snapshot::default()`: the crate-private
    // `pending_confirm` field makes functional-update syntax fail with E0451.
    let mut snap = Snapshot::default();
    snap.grids = grids;
    snap.highlights = highlights;
    snap.cursor = cursor;
    snap.mode_infos = mode_infos;
    snap
}

#[test]
fn explicit_highlight_colors_pack_to_set_rgb() {
    const FG: u32 = 0x0011_2233;
    const BG: u32 = 0x0044_5566;
    const HL: u64 = 1;

    // No defaults set, so the highlight is the only color source: an explicit
    // fg/bg must resolve to set=true with the exact 0x00RRGGBB packing.
    let mut highlights = HighlightTable::new();
    highlights.define(
        HL,
        HlAttr {
            fg: Some(Rgb::from_packed(FG)),
            bg: Some(Rgb::from_packed(BG)),
            ..HlAttr::default()
        },
    );

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        0,
        0,
        Cell {
            text: "A".to_string(),
            hl_id: HL,
        },
    );

    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 1, 1),
        Vec::new(),
    );
    let frame = FfiFrame::from_snapshot(&snap);

    let cells = frame.cells();
    let cell = &cells[0];
    assert_eq!(cell.text(), "A", "spot-checked cell text");

    let fg = cell.fg();
    assert!(fg.set, "an explicit highlight fg must be set");
    assert_eq!(fg.rgb, FG, "fg should pack to 0x00RRGGBB exactly");

    let bg = cell.bg();
    assert!(bg.set, "an explicit highlight bg must be set");
    assert_eq!(bg.rgb, BG, "bg should pack to 0x00RRGGBB exactly");
}

#[test]
fn special_color_packs_from_resolved_style() {
    const SPECIAL: u32 = 0x0000_FF00;
    const HL: u64 = 1;
    const PLAIN_HL: u64 = 2;

    // An explicit `special` color plus underline packs to a set 0x00RRGGBB `FfiColor` and sets the
    // UNDERLINE bit.
    let mut highlights = HighlightTable::new();
    highlights.define(
        HL,
        HlAttr {
            special: Some(Rgb::from_packed(SPECIAL)),
            underline: true,
            ..HlAttr::default()
        },
    );
    // A plain/default highlight sets no special color, so it must pack set=false.
    highlights.define(PLAIN_HL, HlAttr::default());

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        0,
        0,
        Cell {
            text: "S".to_string(),
            hl_id: HL,
        },
    );
    grid.set_cell(
        0,
        1,
        Cell {
            text: "p".to_string(),
            hl_id: PLAIN_HL,
        },
    );

    // Cursor parked off both cells under test so the cursor reverse-video logic
    // does not touch them.
    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 1, 3),
        Vec::new(),
    );
    let frame = FfiFrame::from_snapshot(&snap);

    let cells = frame.cells();

    let styled = &cells[0];
    let special = styled.special();
    assert!(
        special.set,
        "an explicit highlight special color must be set"
    );
    assert_eq!(
        special.rgb, SPECIAL,
        "special should pack to 0x00RRGGBB exactly"
    );
    assert!(
        styled.flags() & UNDERLINE != 0,
        "the underline attribute must set the UNDERLINE flag bit"
    );

    let plain = &cells[1];
    assert!(
        !plain.special().set,
        "a highlight with no special color must pack special.set == false"
    );
    assert_eq!(plain.special().rgb, 0, "an unset special color packs rgb=0");
}

#[test]
fn unset_highlight_yields_unset_colors() {
    // hl_id 0 with unset defaults resolves to None, so the packed color is unset and the front-end
    // falls back to its own defaults.
    let highlights = HighlightTable::new();

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        0,
        0,
        Cell {
            text: "x".to_string(),
            hl_id: 0,
        },
    );

    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 1, 1),
        Vec::new(),
    );
    let frame = FfiFrame::from_snapshot(&snap);

    let cells = frame.cells();
    let cell = &cells[0];

    let fg = cell.fg();
    assert!(
        !fg.set,
        "unset fg (no highlight, no defaults) must be set=false"
    );
    assert_eq!(fg.rgb, 0, "an unset color packs rgb=0");

    let bg = cell.bg();
    assert!(
        !bg.set,
        "unset bg (no highlight, no defaults) must be set=false"
    );
    assert_eq!(bg.rgb, 0, "an unset color packs rgb=0");
}

#[test]
fn attribute_flags_pack_exact_bits() {
    struct Case {
        attr: HlAttr,
        want: u8,
    }

    let cases = [
        // bold + italic + underline together.
        Case {
            attr: HlAttr {
                bold: true,
                italic: true,
                underline: true,
                ..HlAttr::default()
            },
            want: BOLD | ITALIC | UNDERLINE,
        },
        // undercurl alone (its own bit, distinct from underline).
        Case {
            attr: HlAttr {
                undercurl: true,
                ..HlAttr::default()
            },
            want: UNDERCURL,
        },
        // strikethrough alone.
        Case {
            attr: HlAttr {
                strikethrough: true,
                ..HlAttr::default()
            },
            want: STRIKETHROUGH,
        },
        // all five set.
        Case {
            attr: HlAttr {
                bold: true,
                italic: true,
                underline: true,
                undercurl: true,
                strikethrough: true,
                ..HlAttr::default()
            },
            want: BOLD | ITALIC | UNDERLINE | UNDERCURL | STRIKETHROUGH,
        },
        // none set.
        Case {
            attr: HlAttr::default(),
            want: 0,
        },
    ];

    // One cell per case along row 0; hl_id = index + 1.
    let mut highlights = HighlightTable::new();
    let mut grid = Grid::new(cases.len(), 1);
    for (i, case) in cases.iter().enumerate() {
        let id = u64::try_from(i).expect("case index fits in u64") + 1;
        highlights.define(id, case.attr);
        grid.set_cell(
            0,
            i,
            Cell {
                text: String::new(),
                hl_id: id,
            },
        );
    }

    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 0, 0),
        Vec::new(),
    );
    let frame = FfiFrame::from_snapshot(&snap);

    let cells = frame.cells();
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(
            cells[i].flags(),
            case.want,
            "flag bitset for case {i} should be exactly {:#07b}",
            case.want
        );
    }
}

#[test]
fn reverse_video_is_resolved_once_not_double_applied() {
    const ORIG_FG: u32 = 0x0011_1111;
    const ORIG_BG: u32 = 0x0022_2222;
    const HL: u64 = 1;

    // `HighlightTable::resolve` already swaps fg/bg for `reverse`, so the bridge stores the colors
    // as given and must not swap again.
    let mut highlights = HighlightTable::new();
    highlights.define(
        HL,
        HlAttr {
            fg: Some(Rgb::from_packed(ORIG_FG)),
            bg: Some(Rgb::from_packed(ORIG_BG)),
            reverse: true,
            ..HlAttr::default()
        },
    );

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        0,
        0,
        Cell {
            text: "r".to_string(),
            hl_id: HL,
        },
    );

    // Cursor parked away from (0,0) so the cursor reverse-video logic does not
    // touch the cell under test.
    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 1, 1),
        Vec::new(),
    );
    let frame = FfiFrame::from_snapshot(&snap);

    let cells = frame.cells();
    let cell = &cells[0];
    assert_eq!(
        cell.fg().rgb,
        ORIG_BG,
        "reverse: the cell fg should be the ORIGINAL bg (swapped exactly once)"
    );
    assert_eq!(
        cell.bg().rgb,
        ORIG_FG,
        "reverse: the cell bg should be the ORIGINAL fg (swapped exactly once)"
    );
}

#[test]
fn cursor_shape_maps_to_u8_and_is_visible_on_active_grid() {
    const PCT: u32 = 25;
    const CURSOR_ROW: usize = 1;
    const CURSOR_COL: usize = 2;

    let cases = [
        (CursorShape::Block, SHAPE_BLOCK),
        (CursorShape::Horizontal, SHAPE_HORIZONTAL),
        (CursorShape::Vertical, SHAPE_VERTICAL),
    ];

    for (shape, want) in cases {
        let highlights = HighlightTable::new();
        let grid = Grid::new(COLS, ROWS);
        let mode = ModeInfo {
            name: "m".to_string(),
            shape,
            cell_percentage: Some(PCT),
            attr_id: None,
        };
        let snap = snapshot(
            GRID_ID,
            grid,
            highlights,
            cursor_on(GRID_ID, CURSOR_ROW, CURSOR_COL),
            vec![mode],
        );
        let frame = FfiFrame::from_snapshot(&snap);

        assert_eq!(frame.cursor_shape(), want, "shape {shape:?} -> u8");
        assert!(
            frame.cursor_visible(),
            "the cursor lives on the active grid, so it must be visible"
        );
        assert_eq!(
            frame.cursor_row(),
            u16::try_from(CURSOR_ROW).expect("row fits u16")
        );
        assert_eq!(
            frame.cursor_col(),
            u16::try_from(CURSOR_COL).expect("col fits u16")
        );
        assert_eq!(frame.cursor_cell_percentage(), PCT);
    }
}

#[test]
fn cursor_not_visible_when_on_different_grid_than_active() {
    let highlights = HighlightTable::new();
    let grid = Grid::new(COLS, ROWS);

    // The cursor names an absent grid, so it differs from the active grid and is not visible.
    let cursor = Cursor {
        grid: GRID_ID + 1,
        row: 0,
        col: 0,
        mode_idx: 0,
    };
    let snap = snapshot(GRID_ID, grid, highlights, cursor, Vec::new());
    let frame = FfiFrame::from_snapshot(&snap);

    assert!(
        !frame.cursor_visible(),
        "a cursor on a different grid than the active grid must be invisible"
    );
}

#[test]
fn cursor_fill_glyph_are_reverse_video_of_the_cell() {
    const CELL_FG: u32 = 0x00AA_AAAA;
    const CELL_BG: u32 = 0x00BB_BBBB;
    const HL: u64 = 1;

    // No mode cursor attr, so the cursor is plain reverse video of the cell under
    // it: fill = the cell's fg, glyph = the cell's bg.
    let mut highlights = HighlightTable::new();
    highlights.define(
        HL,
        HlAttr {
            fg: Some(Rgb::from_packed(CELL_FG)),
            bg: Some(Rgb::from_packed(CELL_BG)),
            ..HlAttr::default()
        },
    );

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        0,
        0,
        Cell {
            text: "c".to_string(),
            hl_id: HL,
        },
    );

    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 0, 0),
        Vec::new(),
    );
    let frame = FfiFrame::from_snapshot(&snap);

    let fill = frame.cursor_fill();
    let glyph = frame.cursor_glyph();
    assert!(
        fill.set && glyph.set,
        "cursor colors are always set (reverse video)"
    );
    assert_eq!(fill.rgb, CELL_FG, "fill = cell fg (reverse video)");
    assert_eq!(glyph.rgb, CELL_BG, "glyph = cell bg (reverse video)");
}

#[test]
fn cursor_fill_glyph_use_mode_cursor_attr_when_distinct() {
    const CELL_FG: u32 = 0x00AA_AAAA;
    const CELL_BG: u32 = 0x00BB_BBBB;
    const ATTR_FG: u32 = 0x0001_0203;
    const ATTR_BG: u32 = 0x000A_0B0C;
    const CELL_HL: u64 = 1;
    const CURSOR_HL: u64 = 5;

    // A mode cursor attr whose bg differs from the cell bg and its own fg overrides reverse video:
    // fill = attr bg, glyph = attr fg.
    let mut highlights = HighlightTable::new();
    highlights.define(
        CELL_HL,
        HlAttr {
            fg: Some(Rgb::from_packed(CELL_FG)),
            bg: Some(Rgb::from_packed(CELL_BG)),
            ..HlAttr::default()
        },
    );
    highlights.define(
        CURSOR_HL,
        HlAttr {
            fg: Some(Rgb::from_packed(ATTR_FG)),
            bg: Some(Rgb::from_packed(ATTR_BG)),
            ..HlAttr::default()
        },
    );

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        0,
        0,
        Cell {
            text: "c".to_string(),
            hl_id: CELL_HL,
        },
    );

    let mode = ModeInfo {
        name: "i".to_string(),
        shape: CursorShape::Vertical,
        cell_percentage: None,
        attr_id: Some(CURSOR_HL),
    };
    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 0, 0),
        vec![mode],
    );
    let frame = FfiFrame::from_snapshot(&snap);

    assert_eq!(
        frame.cursor_fill().rgb,
        ATTR_BG,
        "fill should be the mode-cursor bg"
    );
    assert_eq!(
        frame.cursor_glyph().rgb,
        ATTR_FG,
        "glyph should be the mode-cursor fg"
    );
}

#[test]
fn cursor_fill_glyph_honor_theme_cursor_fg_and_bg() {
    const CELL_FG: u32 = 0x00AA_AAAA;
    const CELL_BG: u32 = 0x00BB_BBBB;
    const THEME_FG: u32 = 0x0011_2233;
    const THEME_BG: u32 = 0x0044_5566;
    const HL: u64 = 1;

    // A WolfCursor theme group with both channels set overrides reverse video unconditionally:
    // fill = theme bg, glyph = theme fg.
    let mut highlights = HighlightTable::new();
    highlights.define(
        HL,
        HlAttr {
            fg: Some(Rgb::from_packed(CELL_FG)),
            bg: Some(Rgb::from_packed(CELL_BG)),
            ..HlAttr::default()
        },
    );

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        0,
        0,
        Cell {
            text: "t".to_string(),
            hl_id: HL,
        },
    );

    let mut snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 0, 0),
        Vec::new(),
    );
    snap.theme.cursor = ThemeGroup {
        fg: Some(Rgb::from_packed(THEME_FG)),
        bg: Some(Rgb::from_packed(THEME_BG)),
    };
    let frame = FfiFrame::from_snapshot(&snap);

    let fill = frame.cursor_fill();
    let glyph = frame.cursor_glyph();
    assert!(fill.set && glyph.set, "themed cursor colors are always set");
    assert_eq!(fill.rgb, THEME_BG, "theme: fill = WolfCursor bg");
    assert_eq!(glyph.rgb, THEME_FG, "theme: glyph = WolfCursor fg");
    // And NOT the reverse video of the underlying cell.
    assert_ne!(
        fill.rgb, CELL_FG,
        "the theme override must replace the reverse-video fill"
    );
}

#[test]
fn cursor_glyph_falls_back_to_cell_bg_when_theme_fg_unset() {
    const CELL_FG: u32 = 0x00AA_AAAA;
    const CELL_BG: u32 = 0x00BB_BBBB;
    const THEME_BG: u32 = 0x0044_5566;
    const HL: u64 = 1;

    // A bg-only WolfCursor group still sets the fill; the glyph falls back to the cell bg:
    // `theme.cursor.fg.unwrap_or(cell_bg)`.
    let mut highlights = HighlightTable::new();
    highlights.define(
        HL,
        HlAttr {
            fg: Some(Rgb::from_packed(CELL_FG)),
            bg: Some(Rgb::from_packed(CELL_BG)),
            ..HlAttr::default()
        },
    );

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        0,
        0,
        Cell {
            text: "t".to_string(),
            hl_id: HL,
        },
    );

    let mut snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 0, 0),
        Vec::new(),
    );
    snap.theme.cursor = ThemeGroup {
        fg: None,
        bg: Some(Rgb::from_packed(THEME_BG)),
    };
    let frame = FfiFrame::from_snapshot(&snap);

    assert_eq!(
        frame.cursor_fill().rgb,
        THEME_BG,
        "bg-only theme: fill = WolfCursor bg"
    );
    assert_eq!(
        frame.cursor_glyph().rgb,
        CELL_BG,
        "bg-only theme: glyph falls back to the cell bg (reverse-video glyph)"
    );
}

#[test]
fn theme_cursor_wins_over_mode_cursor_attr() {
    const CELL_FG: u32 = 0x00AA_AAAA;
    const CELL_BG: u32 = 0x00BB_BBBB;
    const ATTR_FG: u32 = 0x0001_0203;
    const ATTR_BG: u32 = 0x000A_0B0C;
    const THEME_FG: u32 = 0x0011_2233;
    const THEME_BG: u32 = 0x0044_5566;
    const CELL_HL: u64 = 1;
    const CURSOR_HL: u64 = 5;

    // The mode attr is honorable, as in `cursor_fill_glyph_use_mode_cursor_attr_when_distinct`, but
    // with the theme also set, the theme branch runs first and wins.
    let mut highlights = HighlightTable::new();
    highlights.define(
        CELL_HL,
        HlAttr {
            fg: Some(Rgb::from_packed(CELL_FG)),
            bg: Some(Rgb::from_packed(CELL_BG)),
            ..HlAttr::default()
        },
    );
    highlights.define(
        CURSOR_HL,
        HlAttr {
            fg: Some(Rgb::from_packed(ATTR_FG)),
            bg: Some(Rgb::from_packed(ATTR_BG)),
            ..HlAttr::default()
        },
    );

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        0,
        0,
        Cell {
            text: "t".to_string(),
            hl_id: CELL_HL,
        },
    );

    let mode = ModeInfo {
        name: "i".to_string(),
        shape: CursorShape::Vertical,
        cell_percentage: None,
        attr_id: Some(CURSOR_HL),
    };
    let mut snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 0, 0),
        vec![mode],
    );
    snap.theme.cursor = ThemeGroup {
        fg: Some(Rgb::from_packed(THEME_FG)),
        bg: Some(Rgb::from_packed(THEME_BG)),
    };
    let frame = FfiFrame::from_snapshot(&snap);

    assert_eq!(
        frame.cursor_fill().rgb,
        THEME_BG,
        "theme beats mode attr: fill = WolfCursor bg, not the mode-cursor bg"
    );
    assert_eq!(
        frame.cursor_glyph().rgb,
        THEME_FG,
        "theme beats mode attr: glyph = WolfCursor fg, not the mode-cursor fg"
    );
    assert_ne!(
        frame.cursor_fill().rgb,
        ATTR_BG,
        "the mode-cursor bg must not leak through when the theme is set"
    );
}

#[test]
fn default_fg_bg_packed_from_snapshot_defaults() {
    const DEF_FG: u32 = 0x0012_3456;
    const DEF_BG: u32 = 0x0065_4321;

    let mut highlights = HighlightTable::new();
    highlights.set_defaults(DefaultColors {
        fg: Some(Rgb::from_packed(DEF_FG)),
        bg: Some(Rgb::from_packed(DEF_BG)),
        special: None,
    });

    let grid = Grid::new(COLS, ROWS);
    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 0, 0),
        Vec::new(),
    );
    let frame = FfiFrame::from_snapshot(&snap);

    let dfg = frame.default_fg();
    assert!(
        dfg.set,
        "default fg should be set when the snapshot sets it"
    );
    assert_eq!(dfg.rgb, DEF_FG);

    let dbg = frame.default_bg();
    assert!(
        dbg.set,
        "default bg should be set when the snapshot sets it"
    );
    assert_eq!(dbg.rgb, DEF_BG);
}

#[test]
fn default_fg_bg_unset_when_snapshot_has_no_defaults() {
    let highlights = HighlightTable::new();
    let grid = Grid::new(COLS, ROWS);
    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 0, 0),
        Vec::new(),
    );
    let frame = FfiFrame::from_snapshot(&snap);

    assert!(!frame.default_fg().set, "absent default fg packs set=false");
    assert!(!frame.default_bg().set, "absent default bg packs set=false");
}

#[test]
fn frame_dims_and_cell_count_match_grid() {
    const HL: u64 = 1;
    // Spot-check coordinate (row, col) and its row-major index.
    const SPOT_ROW: usize = 1;
    const SPOT_COL: usize = 3;

    let mut highlights = HighlightTable::new();
    highlights.define(HL, HlAttr::default());

    let mut grid = Grid::new(COLS, ROWS);
    grid.set_cell(
        SPOT_ROW,
        SPOT_COL,
        Cell {
            text: "Z".to_string(),
            hl_id: HL,
        },
    );

    let snap = snapshot(
        GRID_ID,
        grid,
        highlights,
        cursor_on(GRID_ID, 0, 0),
        Vec::new(),
    );
    let frame = FfiFrame::from_snapshot(&snap);

    assert_eq!(frame.cols(), u16::try_from(COLS).expect("cols fit u16"));
    assert_eq!(frame.rows(), u16::try_from(ROWS).expect("rows fit u16"));

    let cells = frame.cells();
    assert_eq!(
        cells.len(),
        COLS * ROWS,
        "cells() is a dense row-major grid of cols*rows entries"
    );

    let idx = SPOT_ROW * COLS + SPOT_COL;
    assert_eq!(
        cells[idx].text(),
        "Z",
        "the cell at (row {SPOT_ROW}, col {SPOT_COL}) sits at row-major index {idx}"
    );
}
