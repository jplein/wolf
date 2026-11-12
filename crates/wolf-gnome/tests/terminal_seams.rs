//! Table-driven tests for the pure seams behind a terminal pane: `terminal::style`,
//! `terminal::damage`, `terminal::geometry`, `terminal::input`, the smoke hook's parse,
//! `scrollbar::terminal_viewport`, `clipboard_ui::copy_decision`'s terminal arm,
//! `liveness_ui::terminal_restore`, and `pane_layout`'s terminal leaf.
//!
//! No display, no Neovim and no session: every input is a hand-built `TerminalCell`,
//! `TerminalFrame` or literal, and every expected value is written out by hand, never
//! computed by the function under test. What the widgets do with these answers is
//! `terminal_paint.rs`'s and the terminal-pane cage smoke's.

use std::collections::HashMap;
use std::path::PathBuf;

use gdk::ModifierType;

use wolf_core::{
    Key, KeyEvent, Liveness, MouseButton, PaneNode, PaneOrientation, ResolvedStyle, Rgb,
    TerminalCell, TerminalCellWidth, TerminalCommand, TerminalCursor, TerminalCursorStyle,
    TerminalFrame, TerminalId, TerminalScrollbar, TerminalUnderline,
};
use wolf_gnome::clipboard_ui::{copy_decision, CopyPress};
use wolf_gnome::editor_inset::GridRect;
use wolf_gnome::layout::{Orientation, PaneId, PaneLayout};
use wolf_gnome::liveness_ui::{terminal_restore, TerminalRestore};
use wolf_gnome::pane_layout::{encode, is_empty_leaf, resolve_leaves, WinContent};
use wolf_gnome::scrollbar::terminal_viewport;
use wolf_gnome::terminal::damage::{effective_painted_version, rows_to_repaint, PaintKey};
use wolf_gnome::terminal::geometry::{pointer_pixels, size, sizer, TerminalSize};
use wolf_gnome::terminal::input::{
    commit_route, focus_reports, mouse_button, pointer_mods, FocusReport, PointerMods,
    TerminalCommit,
};
use wolf_gnome::terminal::style::{
    cursor_colors, cursor_span, cursor_style, is_selected, resolve, CellPaint,
};
use wolf_gnome::terminal::{smoke_terminal_command, SMOKE_TERMINAL_ENV};

const DEFAULT_FG: Rgb = Rgb {
    r: 220,
    g: 221,
    b: 222,
};
const DEFAULT_BG: Rgb = Rgb {
    r: 10,
    g: 11,
    b: 12,
};
/// A set foreground, unlike either default.
const SET_FG: Rgb = Rgb {
    r: 200,
    g: 100,
    b: 0,
};
/// A set background, unlike either default and `SET_FG`.
const SET_BG: Rgb = Rgb { r: 0, g: 0, b: 40 };

/// A plain one-letter cell with every attribute off.
fn letter() -> TerminalCell {
    TerminalCell {
        text: "a".to_owned(),
        ..Default::default()
    }
}

/// The style `resolve` must give a cell with no attribute, painted `fg` on `bg`.
fn plain_style(fg: Rgb, bg: Rgb) -> ResolvedStyle {
    ResolvedStyle {
        fg: Some(fg),
        bg: Some(bg),
        special: None,
        bold: false,
        italic: false,
        underline: false,
        undercurl: false,
        strikethrough: false,
    }
}

/// A `cols` by `rows` frame of blank cells with every field set, so a case changes only
/// what it is about.
fn frame(cols: u16, rows: u16) -> TerminalFrame {
    TerminalFrame {
        version: 1,
        cols,
        rows,
        cells: vec![TerminalCell::default(); usize::from(cols) * usize::from(rows)],
        row_versions: vec![1; usize::from(rows)],
        cursor: Some(TerminalCursor {
            col: 0,
            row: 0,
            visible: true,
            blinking: false,
            style: TerminalCursorStyle::Block,
        }),
        default_fg: DEFAULT_FG,
        default_bg: DEFAULT_BG,
        cursor_color: None,
        selection: vec![None; usize::from(rows)],
        scrollbar: TerminalScrollbar::default(),
        mouse_tracking: false,
        title: String::new(),
        pwd: None,
        shell: "sh".to_owned(),
        exited: None,
    }
}

#[test]
fn resolve_takes_the_defaults_for_unset_colours_and_keeps_set_ones() {
    let unset = resolve(&letter(), false, DEFAULT_FG, DEFAULT_BG);
    assert_eq!(
        unset,
        CellPaint {
            background: DEFAULT_BG,
            foreground: DEFAULT_FG,
            draws_glyph: true,
            style: plain_style(DEFAULT_FG, DEFAULT_BG),
        },
        "a cell with no colour of its own must paint the frame's defaults; another background \
         means the unset arm reads something other than `default_bg`, another foreground the \
         same for `default_fg`, and a set `special` or attribute means `style` is not built from \
         the cell"
    );

    let set = TerminalCell {
        fg: Some(SET_FG),
        bg: Some(SET_BG),
        ..letter()
    };
    assert_eq!(
        resolve(&set, false, DEFAULT_FG, DEFAULT_BG),
        CellPaint {
            background: SET_BG,
            foreground: SET_FG,
            draws_glyph: true,
            style: plain_style(SET_FG, SET_BG),
        },
        "a cell's own colours must win over the defaults; the defaults here mean the set colours \
         are ignored, and swapped colours mean the swap fires with neither inverse nor selection"
    );
}

#[test]
fn inverse_and_selection_each_swap_and_together_cancel() {
    let coloured = TerminalCell {
        fg: Some(SET_FG),
        bg: Some(SET_BG),
        ..letter()
    };
    let inverse = TerminalCell {
        inverse: true,
        ..coloured.clone()
    };
    // (case, cell, selected, expected (foreground, background))
    let cases = [
        ("neither", &coloured, false, (SET_FG, SET_BG)),
        ("inverse alone", &inverse, false, (SET_BG, SET_FG)),
        ("selected alone", &coloured, true, (SET_BG, SET_FG)),
        ("inverse and selected", &inverse, true, (SET_FG, SET_BG)),
    ];
    for (case, cell, selected, (fg, bg)) in cases {
        let paint = resolve(cell, selected, DEFAULT_FG, DEFAULT_BG);
        assert_eq!(
            (paint.foreground, paint.background),
            (fg, bg),
            "{case}: the colours must swap exactly when one of `inverse` and `selected` holds. \
             Unswapped colours under one of them mean that term is missing from the swap; swapped \
             colours under both mean the two are or-ed rather than xor-ed; swapped colours under \
             neither mean the condition is inverted"
        );
        assert_eq!(
            (paint.style.fg, paint.style.bg),
            (Some(fg), Some(bg)),
            "{case}: `style` must carry the colours after the swap, since the glyph painter reads \
             its colour from there"
        );
    }

    let unset_selected = resolve(&letter(), true, DEFAULT_FG, DEFAULT_BG);
    assert_eq!(
        (unset_selected.foreground, unset_selected.background),
        (DEFAULT_BG, DEFAULT_FG),
        "a selected cell with no colours must swap the defaults, so a selection over plain text \
         is visible; the defaults unswapped mean the swap happens before the defaults are taken"
    );
}

#[test]
fn a_faint_foreground_is_blended_halfway_to_the_background() {
    let on_black = Rgb { r: 0, g: 0, b: 0 };
    let faint = TerminalCell {
        fg: Some(SET_FG),
        bg: Some(on_black),
        faint: true,
        ..letter()
    };
    let halfway = Rgb {
        r: 100,
        g: 50,
        b: 0,
    };
    let paint = resolve(&faint, false, DEFAULT_FG, DEFAULT_BG);
    assert_eq!(
        (paint.foreground, paint.style.fg, paint.background),
        (halfway, Some(halfway), on_black),
        "a faint (200,100,0) on (0,0,0) must paint its glyph in (100,50,0) and keep its \
         background (FAINT_ALPHA 0.5); the unblended foreground means faint is ignored, and a \
         blended background means the blend was applied to the wrong colour"
    );

    let bright = TerminalCell {
        faint: false,
        ..faint
    };
    assert_eq!(
        resolve(&bright, false, DEFAULT_FG, DEFAULT_BG).foreground,
        SET_FG,
        "a cell that is not faint must keep its foreground; a blended one means every cell is \
         dimmed"
    );
}

#[test]
fn spacers_and_invisible_cells_draw_no_glyph() {
    let wide = |width| TerminalCell { width, ..letter() };
    let cases = [
        ("narrow", wide(TerminalCellWidth::Narrow), true),
        ("wide lead", wide(TerminalCellWidth::Wide), true),
        ("spacer tail", wide(TerminalCellWidth::SpacerTail), false),
        ("spacer head", wide(TerminalCellWidth::SpacerHead), false),
        (
            "invisible",
            TerminalCell {
                invisible: true,
                ..letter()
            },
            false,
        ),
    ];
    for (case, cell, draws) in cases {
        assert_eq!(
            resolve(&cell, false, DEFAULT_FG, DEFAULT_BG).draws_glyph,
            draws,
            "{case}: `draws_glyph` must be {draws}. A spacer that draws would paint over the right \
             half of its wide lead; an invisible cell that draws shows hidden text; a narrow or \
             wide lead that does not draws nothing at all"
        );
    }
}

#[test]
fn style_bits_map_the_terminal_attributes() {
    let with_underline = |underline| TerminalCell {
        underline,
        ..letter()
    };
    // (case, cell, (bold, italic, underline, undercurl, strikethrough))
    let cases = [
        (
            "bold",
            TerminalCell {
                bold: true,
                ..letter()
            },
            (true, false, false, false, false),
        ),
        (
            "italic",
            TerminalCell {
                italic: true,
                ..letter()
            },
            (false, true, false, false, false),
        ),
        (
            "strikethrough",
            TerminalCell {
                strikethrough: true,
                ..letter()
            },
            (false, false, false, false, true),
        ),
        (
            "single underline",
            with_underline(TerminalUnderline::Single),
            (false, false, true, false, false),
        ),
        (
            "double underline",
            with_underline(TerminalUnderline::Double),
            (false, false, true, false, false),
        ),
        (
            "dotted underline",
            with_underline(TerminalUnderline::Dotted),
            (false, false, true, false, false),
        ),
        (
            "dashed underline",
            with_underline(TerminalUnderline::Dashed),
            (false, false, true, false, false),
        ),
        (
            "curly underline",
            with_underline(TerminalUnderline::Curly),
            (false, false, false, true, false),
        ),
        (
            "no underline",
            with_underline(TerminalUnderline::None),
            (false, false, false, false, false),
        ),
    ];
    for (case, cell, expected) in cases {
        let style = resolve(&cell, false, DEFAULT_FG, DEFAULT_BG).style;
        assert_eq!(
            (
                style.bold,
                style.italic,
                style.underline,
                style.undercurl,
                style.strikethrough
            ),
            expected,
            "{case}: (bold, italic, underline, undercurl, strikethrough) is wrong. Curly must be \
             the undercurl alone and the four other styles the straight underline alone; a style \
             reading both or neither has lost its arm in the mapping"
        );
        assert_eq!(
            style.special, None,
            "{case}: a terminal cell has no special colour, so the undercurl paints in the \
             foreground"
        );
    }

    let blinking = TerminalCell {
        blink: true,
        ..letter()
    };
    assert_eq!(
        resolve(&blinking, false, DEFAULT_FG, DEFAULT_BG),
        resolve(&letter(), false, DEFAULT_FG, DEFAULT_BG),
        "blink must change nothing: blinking text draws steady, since this crate runs no periodic \
         timer"
    );
}

#[test]
fn is_selected_is_inclusive_and_false_outside_the_selection() {
    let mut selected = frame(8, 3);
    selected.selection = vec![Some((2, 5)), None, Some((0, 0))];
    // (case, col, row, expected)
    let cases = [
        ("first selected column", 2, 0, true),
        ("inside", 3, 0, true),
        ("last selected column", 5, 0, true),
        ("before the start", 1, 0, false),
        ("past the end", 6, 0, false),
        ("a row with no selection", 3, 1, false),
        ("a one-cell selection", 0, 2, true),
        ("a row past the frame", 0, 3, false),
        ("a column past the frame", 9, 0, false),
    ];
    for (case, col, row, expected) in cases {
        assert_eq!(
            is_selected(&selected, col, row),
            expected,
            "{case}: is_selected({col}, {row}) over selection {:?}. The selection's end column is \
             inclusive, so `false` at an end means a half-open range; `true` on a `None` row or \
             past the frame means the row is not looked up",
            selected.selection
        );
    }
}

#[test]
fn cursor_colours_and_span() {
    let mut themed = frame(4, 1);
    let cursor = Rgb {
        r: 255,
        g: 128,
        b: 1,
    };
    themed.cursor_color = Some(cursor);
    assert_eq!(
        cursor_colors(&themed),
        (cursor, DEFAULT_BG),
        "a frame with a cursor colour must fill the cursor in it and draw the glyph in the default \
         background; the default foreground as the fill means `cursor_color` is ignored"
    );
    assert_eq!(
        cursor_colors(&frame(4, 1)),
        (DEFAULT_FG, DEFAULT_BG),
        "a frame with no cursor colour must fill the cursor in the default foreground"
    );

    let cases = [
        (TerminalCellWidth::Wide, 2),
        (TerminalCellWidth::Narrow, 1),
        (TerminalCellWidth::SpacerTail, 1),
        (TerminalCellWidth::SpacerHead, 1),
    ];
    for (width, span) in cases {
        assert_eq!(
            cursor_span(width),
            span,
            "a cursor on a {width:?} cell must cover {span} cell(s); a block over a wide lead \
             that covers one leaves the glyph's right half uncovered"
        );
    }
}

#[test]
fn cursor_style_is_hollow_unless_focused() {
    use TerminalCursorStyle::{Bar, Block, BlockHollow, Underline};
    let cases = [
        (Block, true, Block),
        (Bar, true, Bar),
        (Underline, true, Underline),
        (BlockHollow, true, BlockHollow),
        (Block, false, BlockHollow),
        (Bar, false, BlockHollow),
        (Underline, false, BlockHollow),
        (BlockHollow, false, BlockHollow),
    ];
    for (style, focused, expected) in cases {
        assert_eq!(
            cursor_style(style, focused),
            expected,
            "a {style:?} cursor with focused {focused} must paint {expected:?}: a focused pane \
             paints the frame's style and any other pane a hollow block. The frame's style \
             unfocused means focus is ignored; a hollow block focused means the style is ignored"
        );
    }
}

/// The paint key every `effective_painted_version` case varies one field of.
const KEY: PaintKey = PaintKey {
    cols: 80,
    rows: 24,
    default_bg: DEFAULT_BG,
    cell_w: 9.5,
    cell_h: 19.0,
    scale: 1.0,
};

#[test]
fn a_changed_paint_key_forgets_the_painted_version() {
    assert_eq!(
        effective_painted_version(Some(KEY), KEY, Some(5)),
        Some(5),
        "an unchanged key must keep the painted version, or every update repaints every row"
    );
    assert_eq!(
        effective_painted_version(Some(KEY), KEY, None),
        None,
        "nothing painted stays nothing painted"
    );
    assert_eq!(
        effective_painted_version(None, KEY, Some(5)),
        None,
        "a canvas with no previous key has painted nothing that can be kept"
    );
    let changed = [
        ("cols", PaintKey { cols: 81, ..KEY }),
        ("rows", PaintKey { rows: 25, ..KEY }),
        (
            "default_bg",
            PaintKey {
                default_bg: DEFAULT_FG,
                ..KEY
            },
        ),
        (
            "cell_w",
            PaintKey {
                cell_w: 9.75,
                ..KEY
            },
        ),
        (
            "cell_h",
            PaintKey {
                cell_h: 20.0,
                ..KEY
            },
        ),
        ("scale", PaintKey { scale: 2.0, ..KEY }),
    ];
    for (field, previous) in changed {
        assert_eq!(
            effective_painted_version(Some(previous), KEY, Some(5)),
            None,
            "a key differing only in `{field}` must forget the painted version, so every row is \
             repainted; `Some(5)` means a stale backing surface keeps rows painted under the old \
             {field}"
        );
    }
}

#[test]
fn rows_to_repaint_picks_the_strictly_newer_rows() {
    let versions = [3, 7, 2, 9];
    // (case, painted version, expected rows)
    let cases: [(&str, Option<u64>, Vec<u16>); 4] = [
        ("nothing painted", None, vec![0, 1, 2, 3]),
        ("painted at 3", Some(3), vec![1, 3]),
        ("painted at 7", Some(7), vec![3]),
        ("painted at the newest", Some(9), vec![]),
    ];
    for (case, painted, expected) in cases {
        assert_eq!(
            rows_to_repaint(&versions, painted),
            expected,
            "{case}: rows of versions {versions:?}. A row whose version equals the painted one was \
             already painted, so its presence means `>=` where `>` belongs; a missing newer row \
             is lost damage; a descending list breaks the canvas's ascending contract"
        );
    }
    assert!(
        rows_to_repaint(&[], None).is_empty(),
        "a frame with no rows has nothing to repaint"
    );
}

#[test]
fn size_clamps_counts_and_rounds_pixel_sizes() {
    // (case, cols, rows, cell_w, cell_h, expected)
    let cases = [
        (
            "an ordinary size",
            80,
            24,
            8.4,
            17.6,
            TerminalSize {
                cols: 80,
                rows: 24,
                cell_w_px: 8,
                cell_h_px: 18,
            },
        ),
        (
            "zero counts",
            0,
            0,
            8.4,
            17.6,
            TerminalSize {
                cols: 1,
                rows: 1,
                cell_w_px: 8,
                cell_h_px: 18,
            },
        ),
        (
            "a cell under half a pixel",
            10,
            5,
            0.2,
            0.2,
            TerminalSize {
                cols: 10,
                rows: 5,
                cell_w_px: 1,
                cell_h_px: 1,
            },
        ),
        (
            "a non-finite cell",
            10,
            5,
            f64::NAN,
            f64::INFINITY,
            TerminalSize {
                cols: 10,
                rows: 5,
                cell_w_px: 1,
                cell_h_px: 1,
            },
        ),
        (
            "a cell past u32",
            10,
            5,
            1e12,
            1e12,
            TerminalSize {
                cols: 10,
                rows: 5,
                cell_w_px: u32::MAX,
                cell_h_px: u32::MAX,
            },
        ),
    ];
    for (case, cols, rows, cell_w, cell_h, expected) in cases {
        assert_eq!(
            size(cols, rows, cell_w, cell_h),
            expected,
            "{case}: size({cols}, {rows}, {cell_w}, {cell_h}). A zero count is a size core \
             ignores, so it must read 1; the pixel sizes round to nearest and are at least 1, \
             since a zero cell size keeps core's previous one"
        );
    }
}

#[test]
fn pointer_pixels_are_rect_relative_and_scaled_to_the_reported_cell() {
    let rect = GridRect {
        x: 20.0,
        y: 50.0,
        w: 500.0,
        h: 400.0,
    };
    let reported = TerminalSize {
        cols: 40,
        rows: 16,
        cell_w_px: 13,
        cell_h_px: 25,
    };
    // (case, x, y, expected)
    let cases = [
        ("the rect's corner", 20.0, 50.0, (0.0, 0.0)),
        ("two cells right and down", 45.0, 100.0, (26.0, 50.0)),
        ("left of and above the rect", 10.0, 40.0, (0.0, 0.0)),
        ("a non-finite point", f64::NAN, f64::NAN, (0.0, 0.0)),
    ];
    for (case, x, y, expected) in cases {
        assert_eq!(
            pointer_pixels(x, y, rect, 12.5, 25.0, reported),
            expected,
            "{case}: pointer_pixels({x}, {y}) over a rect at (20, 50) with 12.5x25 cells reported \
             as 13x25 px. (46.8, 200) at the second row means the rect origin was not \
             subtracted; a negative or NaN coordinate means the clamp is missing"
        );
    }
}

#[test]
fn sizer_prefers_the_active_then_the_previous_then_the_lowest_pane() {
    let showing: &[PaneId] = &[7, 3, 5];
    // (case, showing, active, previous, expected)
    let cases = [
        ("active showing", showing, Some(5), Some(7), Some(5)),
        ("active not showing", showing, Some(9), Some(7), Some(7)),
        ("neither showing", showing, Some(9), Some(8), Some(3)),
        ("no active and no previous", showing, None, None, Some(3)),
        ("nothing showing", &[], Some(5), Some(7), None),
    ];
    for (case, panes, active, previous, expected) in cases {
        assert_eq!(
            sizer(panes, active, previous),
            expected,
            "{case}: sizer({panes:?}, {active:?}, {previous:?}). The active pane sizes a session \
             it shows, then the previous sizer, then the lowest id (not the first listed); a \
             pane that is not showing never sizes it"
        );
    }
}

/// A modifier-free `Key::Char`, which is what `commit_route` answers for one printable.
fn plain_char(c: char) -> TerminalCommit {
    TerminalCommit::Key(KeyEvent {
        key: Key::Char(c),
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    })
}

#[test]
fn commit_route_sends_one_printable_as_a_key_and_other_text_cleaned() {
    let cases = [
        ("empty", "", TerminalCommit::Nothing),
        ("escape alone", "\u{1b}", TerminalCommit::Nothing),
        ("delete alone", "\u{7f}", TerminalCommit::Nothing),
        ("controls only", "\u{1b}\u{7f}\r", TerminalCommit::Nothing),
        ("a letter", "s", plain_char('s')),
        ("a space", " ", plain_char(' ')),
        ("a tilde", "~", plain_char('~')),
        (
            "a composed letter",
            "é",
            TerminalCommit::Text("é".to_owned()),
        ),
        ("two letters", "ab", TerminalCommit::Text("ab".to_owned())),
        (
            "delete inside text",
            "a\u{7f}b",
            TerminalCommit::Text("ab".to_owned()),
        ),
        (
            "escape before a letter",
            "\u{1b}x",
            TerminalCommit::Text("x".to_owned()),
        ),
    ];
    for (case, text, expected) in cases {
        assert_eq!(
            commit_route(text),
            expected,
            "{case}: commit_route({text:?}). One printable ASCII scalar must be a key, so the \
             `<C-w>` router sees `s` or `v`; other text loses its C0 and DEL scalars and goes as \
             text; nothing left sends nothing"
        );
    }
}

#[test]
fn focus_reports_send_the_loss_before_the_gain() {
    let a = TerminalId(1);
    let b = TerminalId(2);
    let gain = |session| FocusReport {
        session,
        gained: true,
    };
    let loss = |session| FocusReport {
        session,
        gained: false,
    };
    let cases = [
        ("no session either side", None, None, vec![]),
        ("the same session", Some(a), Some(a), vec![]),
        ("focus arrives", None, Some(a), vec![gain(a)]),
        ("focus leaves", Some(a), None, vec![loss(a)]),
        ("focus moves", Some(a), Some(b), vec![loss(a), gain(b)]),
    ];
    for (case, previous, current, expected) in cases {
        assert_eq!(
            focus_reports(previous, current),
            expected,
            "{case}: focus_reports({previous:?}, {current:?}). An unchanged session reports \
             nothing; a move reports the loss first, so a program never sees two sessions \
             focused at once"
        );
    }
}

#[test]
fn mouse_button_maps_the_three_gdk_buttons() {
    let cases = [
        (1, Some(MouseButton::Left)),
        (2, Some(MouseButton::Middle)),
        (3, Some(MouseButton::Right)),
        (0, None),
        (4, None),
        (8, None),
    ];
    for (button, expected) in cases {
        assert_eq!(
            mouse_button(button),
            expected,
            "GDK button {button}: 1 is primary, 2 middle and 3 secondary, and any other button \
             (a wheel or side button) sends nothing"
        );
    }
}

#[test]
fn pointer_mods_keep_shift_ctrl_and_alt_only() {
    let cases = [
        ("none", ModifierType::empty(), PointerMods::default()),
        (
            "shift",
            ModifierType::SHIFT_MASK,
            PointerMods {
                shift: true,
                ..PointerMods::default()
            },
        ),
        (
            "control",
            ModifierType::CONTROL_MASK,
            PointerMods {
                ctrl: true,
                ..PointerMods::default()
            },
        ),
        (
            "alt",
            ModifierType::ALT_MASK,
            PointerMods {
                alt: true,
                ..PointerMods::default()
            },
        ),
        ("super", ModifierType::SUPER_MASK, PointerMods::default()),
        ("meta", ModifierType::META_MASK, PointerMods::default()),
        ("hyper", ModifierType::HYPER_MASK, PointerMods::default()),
        ("caps lock", ModifierType::LOCK_MASK, PointerMods::default()),
        (
            "a held button",
            ModifierType::BUTTON1_MASK,
            PointerMods::default(),
        ),
        (
            "everything",
            ModifierType::SHIFT_MASK
                | ModifierType::CONTROL_MASK
                | ModifierType::ALT_MASK
                | ModifierType::SUPER_MASK
                | ModifierType::BUTTON1_MASK,
            PointerMods {
                shift: true,
                ctrl: true,
                alt: true,
            },
        ),
    ];
    for (case, state, expected) in cases {
        assert_eq!(
            pointer_mods(state),
            expected,
            "{case}: pointer_mods({state:?}). Shift bypasses mouse tracking, so a dropped Shift \
             makes a drag in a tracking program unselectable; any other bit must be dropped"
        );
    }
}

#[test]
fn the_smoke_hook_reads_a_trimmed_program_and_nothing_else() {
    assert_eq!(
        SMOKE_TERMINAL_ENV, "WOLF_SMOKE_TERMINAL",
        "the cage smoke sets WOLF_SMOKE_TERMINAL by that literal name, so a renamed constant \
         leaves the smoke with no terminal pane"
    );
    for raw in [None, Some(""), Some("  ")] {
        assert_eq!(
            smoke_terminal_command(raw),
            None,
            "{raw:?} must open no terminal: the hook is off unless it names a program"
        );
    }
    assert_eq!(
        smoke_terminal_command(Some(" /bin/sh ")),
        Some(TerminalCommand {
            program: PathBuf::from("/bin/sh"),
            args: Vec::new(),
        }),
        "the program must be the trimmed text with no arguments; a program carrying spaces means \
         the trim is missing and the spawn fails"
    );
}

#[test]
fn terminal_viewport_maps_the_scrollbar() {
    let cases = [
        (
            "scrolled back",
            TerminalScrollbar {
                total: 500,
                offset: 120,
                len: 30,
            },
            (120, 150, 120, 500),
        ),
        (
            "at the bottom",
            TerminalScrollbar {
                total: 54,
                offset: 30,
                len: 24,
            },
            (30, 54, 30, 54),
        ),
        (
            "an offset at the top of u64",
            TerminalScrollbar {
                total: u64::MAX,
                offset: u64::MAX - 5,
                len: 10,
            },
            (u64::MAX - 5, u64::MAX, u64::MAX - 5, u64::MAX),
        ),
    ];
    for (case, bar, expected) in cases {
        let view = terminal_viewport(bar);
        assert_eq!(
            (view.topline, view.botline, view.curline, view.linecount),
            expected,
            "{case}: (topline, botline, curline, linecount) for {bar:?}. botline is offset plus \
             len, saturating; botline equal to topline means the thumb has no length"
        );
    }
}

#[test]
fn copy_in_a_terminal_pane_needs_a_core_but_not_a_live_neovim() {
    let id = TerminalId(4);
    for liveness in [Liveness::Alive, Liveness::NotResponding, Liveness::Dead] {
        assert_eq!(
            copy_decision(Some(liveness), Some(id)),
            CopyPress::Terminal(id),
            "a terminal pane under a {liveness:?} Neovim must copy the session's selection: \
             `terminal_selection_text` issues no RPC, so gating it on Neovim's liveness refuses \
             a copy that cannot block"
        );
    }
    assert!(
        matches!(copy_decision(None, Some(id)), CopyPress::Refuse(_)),
        "with no core there is no session to read, so a terminal pane must refuse; got {:?}",
        copy_decision(None, Some(id))
    );

    // With no terminal pane the answers are the editor's, unchanged.
    assert_eq!(
        copy_decision(Some(Liveness::Alive), None),
        CopyPress::Copy,
        "an editor pane under a live Neovim must yank"
    );
    for liveness in [None, Some(Liveness::NotResponding), Some(Liveness::Dead)] {
        assert!(
            matches!(copy_decision(liveness, None), CopyPress::Refuse(_)),
            "an editor pane under {liveness:?} must refuse the blocking yank; got {:?}",
            copy_decision(liveness, None)
        );
    }
}

#[test]
fn terminal_restore_puts_live_splits_in_order_then_the_drawer() {
    use TerminalRestore::{Drawer, Split};
    let t = TerminalId;
    struct Case {
        name: &'static str,
        splits: Vec<TerminalId>,
        drawer_open: bool,
        live: Vec<TerminalId>,
        want: Vec<TerminalRestore>,
    }
    let cases = [
        Case {
            name: "no sessions and no drawer restore nothing",
            splits: vec![],
            drawer_open: false,
            live: vec![],
            want: vec![],
        },
        Case {
            name: "splits come back in ascending id order; another order means they are not sorted",
            splits: vec![t(3), t(1), t(2)],
            drawer_open: false,
            live: vec![t(1), t(2), t(3)],
            want: vec![Split(t(1)), Split(t(2)), Split(t(3))],
        },
        Case {
            name: "a session captured twice is restored once",
            splits: vec![t(2), t(2), t(1)],
            drawer_open: false,
            live: vec![t(1), t(2)],
            want: vec![Split(t(1)), Split(t(2))],
        },
        Case {
            name: "a session that died across the restart is skipped (no live filter: Split(4))",
            splits: vec![t(1), t(4), t(2)],
            drawer_open: false,
            live: vec![t(1), t(2)],
            want: vec![Split(t(1)), Split(t(2))],
        },
        Case {
            name: "an open drawer comes back LAST, after every split, so a split never lands in it",
            splits: vec![t(2), t(1)],
            drawer_open: true,
            live: vec![t(1), t(2), t(5)],
            want: vec![Split(t(1)), Split(t(2)), Drawer],
        },
        Case {
            name: "an open drawer comes back with no live session: the toggle starts one",
            splits: vec![],
            drawer_open: true,
            live: vec![],
            want: vec![Drawer],
        },
    ];
    for case in cases {
        assert_eq!(
            terminal_restore(&case.splits, case.drawer_open, &case.live),
            case.want,
            "terminal_restore({:?}, {}, {:?}): {}",
            case.splits,
            case.drawer_open,
            case.live,
            case.name
        );
    }
}

#[test]
fn a_terminal_pane_persists_as_a_terminal_leaf() {
    let (editor, terminal, lone): (PaneId, PaneId, PaneId) = (1, 2, 3);
    let contents = resolve_leaves(&[
        (editor, WinContent::Buffer("/proj/a.md".to_owned())),
        (terminal, WinContent::Terminal("/proj/sub".to_owned())),
        (lone, WinContent::Terminal(String::new())),
    ])
    .expect("every pane resolves, so resolve_leaves must not refuse");
    let layout = PaneLayout::Split {
        orientation: Orientation::Vertical,
        children: vec![PaneLayout::Leaf(editor), PaneLayout::Leaf(terminal)],
        weights: vec![0.25, 0.75],
    };
    assert_eq!(
        encode(&layout, &contents),
        PaneNode::Split {
            orientation: PaneOrientation::Vertical,
            weights: vec![0.25, 0.75],
            children: vec![
                PaneNode::Leaf {
                    path: "/proj/a.md".to_owned(),
                    preview: false,
                    terminal: false,
                },
                PaneNode::Leaf {
                    path: "/proj/sub".to_owned(),
                    preview: false,
                    terminal: true,
                },
            ],
        },
        "a terminal pane must encode as a terminal leaf carrying its cwd, beside an editor leaf \
         that is not one; `terminal: false` on the second leaf means WinContent::Terminal \
         resolves like a buffer"
    );

    let lone_node = encode(&PaneLayout::Leaf(lone), &contents);
    assert_eq!(
        lone_node,
        PaneNode::Leaf {
            path: String::new(),
            preview: false,
            terminal: true,
        },
        "a lone terminal pane with no known cwd must encode as a terminal leaf with an empty path"
    );
    assert!(
        !is_empty_leaf(&lone_node),
        "a lone terminal leaf is not an empty-state pane and must persist; true means \
         is_empty_leaf ignores `terminal`"
    );
    let empty_contents: HashMap<PaneId, _> =
        resolve_leaves(&[(lone, WinContent::Buffer(String::new()))])
            .expect("an unnamed buffer resolves");
    assert!(
        is_empty_leaf(&encode(&PaneLayout::Leaf(lone), &empty_contents)),
        "control: a lone unnamed editor pane is still the empty leaf the predicate suppresses"
    );
}
