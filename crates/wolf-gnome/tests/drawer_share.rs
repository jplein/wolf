//! Table-driven tests for `wolf_gnome::drawer::DrawerShare`, a tab's share of the pane area
//! that the terminal drawer's divider holds, and the rule that restores a saved one.
//!
//! The share is the drawer's fraction of the vertical paned's span; the paned's position is
//! the editor tree's height. Every expected position here is a literal worked out by hand,
//! never a value computed by the code under test.
//!
//! The limits model the smokes' geometry: a 176 px floor on both children and a one-pixel
//! handle, so a span `s` has `min_position` 176 and `max_position` `s - 1 - 176`.
//!
//! Pure: no display, no Neovim and no compositor. Whether the paned reads and writes the
//! share is `smoke_headless/drawer_divider.rs`'s question.

use wolf_gnome::drawer::{DrawerShare, DRAWER_WEIGHT};

/// The pane minimum both children are held to: `editor_inset::min_pane_size`'s height at
/// the smokes' cell, 44 px plus a 92 px statusbar plus two 20 px gutters.
const FLOOR: i32 = 176;

/// A round span for the hand arithmetic; the smokes' own pane area is 627 px.
const SPAN: i32 = 600;

/// The `max_position` GTK reports for a span: the span less the handle and the drawer's
/// floor.
const fn max_for(span: i32) -> i32 {
    span - 1 - FLOOR
}

/// A share built the way a drag builds one: a fresh share dragged to `position`.
fn dragged(position: i32, span: i32) -> DrawerShare {
    let mut share = DrawerShare::new();
    share.drag_to(position, span);
    share
}

#[test]
fn a_new_share_is_a_third() {
    assert_eq!(
        DRAWER_WEIGHT,
        1.0 / 3.0,
        "DRAWER_WEIGHT is {DRAWER_WEIGHT}, not one third. `runtime/nvim/init.lua` opens the \
         drawer's split at `math.max(1, math.floor(vim.o.lines / 3))` rows; changing the pixel \
         share without that expression makes the GTK strip and the Neovim split different sizes \
         on the frame the drawer opens."
    );
    assert_eq!(
        DrawerShare::new().get(),
        1.0 / 3.0,
        "a new DrawerShare holds {}, not one third: a new window's drawer must open at a third \
         of the pane area. A value of 2/3 means the share stores the EDITOR's fraction rather \
         than the drawer's; any other value means `new` no longer starts from DRAWER_WEIGHT.",
        DrawerShare::new().get()
    );
    assert_eq!(
        DrawerShare::default(),
        DrawerShare::new(),
        "DrawerShare::default() is {:?} where DrawerShare::new() is {:?}. A derived Default \
         gives a share of 0.0, a drawer of no height that the paned then clamps to its floor, \
         so a caller reaching for `default()` would open every drawer at its minimum.",
        DrawerShare::default(),
        DrawerShare::new()
    );
}

/// One [`position_table`] row.
struct PositionCase {
    desc: &'static str,
    share: DrawerShare,
    min: i32,
    max: i32,
    span: i32,
    expected: i32,
}

/// Pins `DrawerShare::position`: the editor's height is the span less the drawer's share,
/// clamped into GTK's `[min_position, max_position]`.
#[test]
fn position_table() {
    let cases = [
        PositionCase {
            desc: "a new share puts the handle two thirds down (600 * 2/3 = 400)",
            share: DrawerShare::new(),
            min: FLOOR,
            max: max_for(SPAN),
            span: SPAN,
            expected: 400,
        },
        PositionCase {
            desc: "a share dragged to the middle positions the handle there",
            share: dragged(300, SPAN),
            min: FLOOR,
            max: max_for(SPAN),
            span: SPAN,
            expected: 300,
        },
        PositionCase {
            desc: "a share dragged above the editor's floor clamps to min_position",
            share: dragged(60, SPAN),
            min: FLOOR,
            max: max_for(SPAN),
            span: SPAN,
            expected: FLOOR,
        },
        PositionCase {
            desc: "a share dragged below the drawer's floor clamps to max_position",
            share: dragged(570, SPAN),
            min: FLOOR,
            max: max_for(SPAN),
            span: SPAN,
            expected: 423,
        },
    ];
    for case in &cases {
        let got = case.share.position(case.min, case.max, case.span);
        assert_eq!(
            got,
            case.expected,
            "{}: position({}, {}, {}) of share {} is {got}, want {}. 200 on the first row \
             means the share is applied as the editor's fraction rather than the drawer's; \
             a value below {FLOOR} means min_position is not clamped; a value above {} means \
             max_position is not, so the drawer could be dragged under its floor.",
            case.desc,
            case.min,
            case.max,
            case.span,
            case.share.get(),
            case.expected,
            case.max
        );
    }
}

/// Pins `DrawerShare::drag_to`: a position becomes the drawer's share of the span, and a
/// non-positive span leaves the share alone.
#[test]
fn drag_to_table() {
    let mut share = DrawerShare::new();

    share.drag_to(150, SPAN);
    assert_eq!(
        share.get(),
        0.75,
        "dragging the handle to 150 of 600 gave a share of {}, want 0.75: the drawer is the \
         450 px below the handle. 0.25 means drag_to stores the EDITOR's fraction, the \
         position over the span, rather than its complement.",
        share.get()
    );

    share.drag_to(400, SPAN);
    assert!(
        (share.get() - 1.0 / 3.0).abs() < 1e-12,
        "dragging the handle to 400 of 600 gave a share of {}, want one third: the drawer is \
         the 200 px below the handle. A share left at 0.75 means the second drag was ignored.",
        share.get()
    );

    let mut share = dragged(150, SPAN);
    for span in [0, -10] {
        share.drag_to(150, span);
        assert_eq!(
            share.get(),
            0.75,
            "drag_to(150, {span}) changed the share from 0.75 to {}. A paned with no span has \
             no position to read, so the share must be left as it was; 1.0 means a missing \
             fraction was read as zero, which hands the drawer the whole pane area on the next \
             re-apply.",
            share.get()
        );
    }
}

/// Pins that a drag survives the re-apply exactly: a position written by `drag_to` comes
/// back from `position` at the same span, at every pixel of three spans.
///
/// The limits are the whole span so the clamp cannot hide a drift. A half-pixel walk on
/// every allocation pass is what a mismatched denominator between the two directions gives.
#[test]
fn drag_then_position_round_trips() {
    for span in [600, 627, 1001] {
        let mut drifted = Vec::new();
        for p in 0..=span {
            let back = dragged(p, span).position(0, span, span);
            if back != p {
                drifted.push((p, back));
            }
        }
        assert!(
            drifted.is_empty(),
            "at span {span}, {} position(s) did not survive drag_to then position, as \
             (dragged, re-applied): first {:?}. A handle that moves on re-apply walks a little \
             further on every max-position notify, so the user's drag creeps. `(span, span - 1)` \
             means drag_to and position divide by different denominators.",
            drifted.len(),
            &drifted[..drifted.len().min(5)]
        );
    }
}

/// Pins that the share, not a pixel height, is what is remembered: a pane-area resize
/// keeps the drawer's fraction, clamped by the same limits at the new span.
#[test]
fn share_survives_a_pane_area_resize() {
    let share = dragged(200, SPAN);

    let rows = [
        (900, 300, "a taller pane area keeps the editor at a third"),
        (
            400,
            FLOOR,
            "a short pane area clamps the editor to its floor",
        ),
        (
            SPAN,
            200,
            "the original span gives back the dragged position",
        ),
    ];
    for (span, expected, desc) in rows {
        let got = share.position(FLOOR, max_for(span), span);
        assert_eq!(
            got, expected,
            "{desc}: a share dragged to 200 of {SPAN} positions the handle at {got} in a {span} \
             px pane area, want {expected}. 500 at span 900 means the drawer's PIXEL height was \
             stored rather than its share, so a window resize keeps the drawer's height and \
             hands the whole change to the editor."
        );
    }
}

/// E9a and E9b: a saved share comes back only when it is a fraction strictly between 0 and 1,
/// and anything else restores a new share.
#[test]
fn a_saved_share_restores_only_strictly_between_zero_and_one() {
    for saved in [0.5, 0.25] {
        assert_eq!(
            DrawerShare::restored(saved).get(),
            saved,
            "a saved share of {saved} restored as {}. It is a fraction strictly between 0 and 1, \
             so it must come back as saved; one third means every saved share is dropped and a \
             relaunched drawer opens at a new window's share.",
            DrawerShare::restored(saved).get()
        );
    }
    for saved in [
        0.0,
        1.0,
        -0.1,
        1.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let got = DrawerShare::restored(saved).get();
        assert_eq!(
            got, DRAWER_WEIGHT,
            "a saved share of {saved} restored as {got}, not DRAWER_WEIGHT. It names no drawer \
             the paned can show: 0 and below hide it, 1 and above hide the editor, and NaN makes \
             `position` answer the floor, so a corrupt saved state must restore a new share. \
             The saved value kept means `restored` checks nothing."
        );
    }
}
