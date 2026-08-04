//! The pure projection of a Neovim window's `win_viewport` onto a pane-local
//! scrollbar thumb and its colour.
//!
//! No cairo, no pango, no widget: the whole rulebook is a function of values, so it
//! runs in the fast unit suite the way [`crate::editor_inset`] and
//! [`crate::statusbar_layout`] do. `pane_view::draw_pane` fills the rectangle this
//! module returns and decides nothing itself.
//!
//! ## A scrollbar here is a projection, not a control
//!
//! `architecture.md` decision 5 makes the scrollbar a rendering of Neovim's own
//! viewport rather than an input surface, and this crate takes that literally: the
//! thumb is painted, and there is no `gtk::Scrollbar`. A `gtk::Scrollbar` carries a
//! `gtk::Adjustment` and is interactive, so shipping one would invite a thumb drag
//! with no feedback path into Neovim — and a control that visibly refuses input is
//! worse than an indicator that never claimed to accept any.
//!
//! Two further reasons hold independently of that one. The gutter the thumb sits in
//! is painted in the editor's own background, a per-frame value out of the snapshot,
//! while a widget's background is theme-owned — the same reason the statusbar strip
//! is painted, per `DECISIONS.md` "Window structure". And a pane leaf must stay a
//! bare `gtk::DrawingArea`: `ACTIVE_PANE_CSS` is an `outline` precisely so it
//! consumes no layout space and never enters the inset arithmetic, and a widget
//! child would need a `gtk::Overlay` or `gtk::Fixed` around every leaf, rebuilt and
//! re-parented on every layout change.
//!
//! ## The thumb sits in the pane's right PADDING GUTTER
//!
//! [`scrollbar_thumb`] measures `x` from the PANE's right edge, not from the grid
//! rectangle's. macOS's `drawScrollbars` documents the former and computes the
//! latter, so its bar overlaps the last text column in any pane narrower than the
//! column cap; this module implements the documented rule. With
//! [`SCROLLBAR_THICKNESS_PX`] plus [`SCROLLBAR_MARGIN_PX`] no wider than
//! [`crate::editor_inset::EDITOR_PADDING_PX`], the thumb lies wholly inside padding
//! that has already been subtracted, so it never covers a glyph at any pane width,
//! capped or not. The margin also clears the two-pixel active-pane outline, which
//! `outline-offset: -2px` draws over the pane's last four pixels.
//!
//! **The thumb therefore does not enter `grid_size_for_pane`, and that is the
//! load-bearing half.** If it consumed grid width, a pane's column count would drop
//! by one the instant its buffer overflowed the window and rise again when it fit —
//! a text reflow triggered by scrolling, and a `resize_grid` issued on the scroll
//! path.
//!
//! ## Only the thumb is drawn, and there is no track
//!
//! macOS fades its overlay bar out on a repeating 30 Hz timer. `DECISIONS.md`
//! "Threading and the wake" states that nothing polls and no timer is periodic, so
//! this crate cannot port the fade, and the bar is therefore always visible. A
//! permanently visible full-height trough is a GNOME-unidiomatic ornament, and it is
//! a tall thin high-contrast mark on every OCR page whose buffer overflows — an
//! avoidable risk for no signal. GNOME's own idiom is an overlay indicator with no
//! visible trough at rest, which is macOS's steady state minus the fade.
//!
//! ## `Viewport::curline` is deliberately unread
//!
//! The thumb's position comes from `topline` and its height from
//! `botline - topline`, both against `linecount`. Where the CURSOR sits inside that
//! range is not a scrollbar's subject — macOS ignores it too — and reading it would
//! make the thumb jump on a cursor move that scrolled nothing.

// `the_thumb_lies_wholly_inside_the_right_gutter` opens by asserting a RELATION
// BETWEEN this module's constants and `editor_inset::EDITOR_PADDING_PX` — the thumb
// fits in the gutter — which clippy const-folds to a literal and reports as an
// assertion with a constant value. That relation is the point: its whole value is
// the message it carries the day someone widens the bar past the padding it is
// supposed to hide in. Scoped to `cfg(test)` so production code in this module is
// still linted normally.
#![cfg_attr(
    test,
    allow(
        clippy::assertions_on_constants,
        reason = "the gutter invariant is deliberately a relation between production constants"
    )
)]

use crate::editor_inset::GridRect;
use crate::statusbar_layout::blend;
use wolf_core::{Rgb, Viewport};

/// Width of the scrollbar thumb, in device pixels.
pub const SCROLLBAR_THICKNESS_PX: f64 = 8.0;

/// Gap between the thumb's right edge and the pane's, in device pixels.
pub const SCROLLBAR_MARGIN_PX: f64 = 6.0;

/// Floor under the thumb's height, so a huge buffer still shows something
/// grabbable.
pub const SCROLLBAR_MIN_THUMB_PX: f64 = 24.0;

/// The thumb's alpha against the editor background.
pub const SCROLLBAR_THUMB_ALPHA: f64 = 0.45;

/// The neutral grey the thumb is blended from, on every channel.
pub const SCROLLBAR_GREY: u8 = 128;

/// A pane-local rectangle: the scrollbar's thumb.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarThumb {
    /// Left edge, measured from the pane's own left edge.
    pub x: f64,
    /// Top edge, measured from the pane's own top edge.
    pub y: f64,
    /// Width (always [`SCROLLBAR_THICKNESS_PX`]).
    pub w: f64,
    /// Height.
    pub h: f64,
}

/// Widens one `win_viewport` field to `f64`.
///
/// **Every field is widened BEFORE any subtraction**, and that ordering is the
/// rule rather than a style: `botline - topline` taken in `u64` on a malformed
/// viewport is an overflow, which is a debug-build PANIC and not merely a wrapped
/// value. macOS has the same rule for a weaker reason.
#[allow(
    clippy::cast_precision_loss,
    reason = "a buffer line count above 2^53 is unreachable, and the alternative — subtracting in \
              u64 — panics on a malformed viewport"
)]
fn as_lines(v: u64) -> f64 {
    v as f64
}

/// The thumb for a pane `pane_w` device pixels wide whose grid paints into `grid`,
/// projected from `viewport`.
///
/// `None` when nothing is visible, when the whole buffer fits its window, when the
/// pane is too narrow to hold the gutter, or when any input is degenerate. The
/// thumb spans the GRID rectangle vertically, so it never rides up over the
/// statusbar strip, and it sits in the pane's right padding gutter horizontally —
/// see the module docs for both.
#[must_use]
pub fn scrollbar_thumb(pane_w: f64, grid: GridRect, viewport: Viewport) -> Option<ScrollbarThumb> {
    if !pane_w.is_finite()
        || !grid.x.is_finite()
        || !grid.y.is_finite()
        || !grid.w.is_finite()
        || !grid.h.is_finite()
    {
        return None;
    }

    let total = as_lines(viewport.linecount);
    if total <= 0.0 {
        return None;
    }
    // `curline` is deliberately unread; see the module docs.
    let visible = (as_lines(viewport.botline) - as_lines(viewport.topline)).max(0.0);
    if visible <= 0.0 || visible >= total {
        return None;
    }

    let x = pane_w - SCROLLBAR_MARGIN_PX - SCROLLBAR_THICKNESS_PX;
    if x < 0.0 {
        return None;
    }

    let track_y = grid.y;
    let track_h = grid.h;
    if track_h <= 0.0 {
        return None;
    }

    // The `.min(track_h)` is a deliberate divergence from macOS, which omits it and
    // so draws a thumb taller than its own track in a pane shorter than the floor.
    let h = (track_h * (visible / total).min(1.0))
        .max(SCROLLBAR_MIN_THUMB_PX)
        .min(track_h);
    let raw = track_y + (as_lines(viewport.topline) / total) * track_h;
    let y = raw.clamp(track_y, (track_y + track_h - h).max(track_y));

    Some(ScrollbarThumb {
        x,
        y,
        w: SCROLLBAR_THICKNESS_PX,
        h,
    })
}

/// The thumb's colour over the editor's own default background.
///
/// [`SCROLLBAR_GREY`] on every channel, blended [`SCROLLBAR_THUMB_ALPHA`] of the
/// way toward `bg` through [`crate::statusbar_layout`]'s own blend rather than a
/// second copy of it: one rounding rule, so the strip's colours and this one cannot
/// drift. Blending toward the background rather than compositing with an alpha also
/// keeps this crate's "no `set_source_rgba` on the draw path" property — the thumb
/// is painted over an already-filled background, so the two produce the same pixels.
#[must_use]
pub fn scrollbar_thumb_color(bg: Rgb) -> Rgb {
    blend(
        Rgb {
            r: SCROLLBAR_GREY,
            g: SCROLLBAR_GREY,
            b: SCROLLBAR_GREY,
        },
        bg,
        SCROLLBAR_THUMB_ALPHA,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::editor_inset::{GridRect, EDITOR_PADDING_PX};
    use wolf_core::{Rgb, Viewport};

    /// The grid rectangle every thumb row below is projected onto, unless the row
    /// carries its own.
    ///
    /// A LITERAL rather than a [`crate::editor_inset::grid_rect`] call, for the
    /// reason that module's own `grid_local_table` states: a bug in `grid_rect` must
    /// not be able to move this table's inputs underneath it and leave every row
    /// green. It is the rectangle a 840x680 pane with a 32 px strip and 20 px
    /// padding produces.
    const GRID: GridRect = GridRect {
        x: 20.0,
        y: 52.0,
        w: 800.0,
        h: 608.0,
    };

    /// How far a computed field may sit from its literal expectation.
    ///
    /// **This is not slack.** Every expectation in this file is a literal that was
    /// written down, never an expression recomputing the production arithmetic — but
    /// `608.0 * (11.0 / 200.0)` and the literal `33.44` are two different roundings
    /// of the same real number and may differ in the last bit. The tolerance covers
    /// that and nothing else: the smallest difference any falsifying edit below
    /// produces is 14 px (row 8's 24 against its 10), fourteen orders of magnitude
    /// above this bound.
    const EPS: f64 = 1e-9;

    /// Whether a computed field matches its literal expectation; see [`EPS`].
    fn close(got: f64, want: f64) -> bool {
        (got - want).abs() <= EPS
    }

    /// One [`scrollbar_thumb`] row: the pane width, the grid the thumb is projected
    /// onto, the viewport, and the expected thumb as `(x, y, w, h)` or `None`.
    struct ThumbCase {
        desc: &'static str,
        pane_w: f64,
        grid: GridRect,
        viewport: Viewport,
        expected: Option<(f64, f64, f64, f64)>,
    }

    /// The four fields of a [`ScrollbarThumb`] in declaration order, so a row states
    /// its expectation as one tuple and a failure prints all four rather than
    /// stopping at the first that differs.
    fn fields(thumb: ScrollbarThumb) -> (f64, f64, f64, f64) {
        (thumb.x, thumb.y, thumb.w, thumb.h)
    }

    /// Pins [`scrollbar_thumb`]: where the thumb sits, how tall it is, and the five
    /// distinct inputs that must produce no thumb at all.
    ///
    /// Every `x` below is 826 — `840 - 6 - 8`, measured from the PANE's right edge —
    /// and never 806, which is what measuring from the grid rectangle's right edge
    /// (`20 + 800 - 6 - 8`) would give. That difference is the whole of the
    /// divergence from macOS's `drawScrollbars`, and it is one column of glyphs.
    ///
    /// ## The `None` rows are the interesting half
    ///
    /// Four of the five are ordinary degenerate viewports. **Row 6 is not**: a
    /// `botline` below its `topline` is what a `u64` subtraction panics on in a debug
    /// build, so the rule that every viewport field is widened to `f64` BEFORE any
    /// subtraction is stricter here than macOS's reason for the same rule. A malformed
    /// viewport must produce no thumb, not a crashed front-end.
    ///
    /// FALSIFYING EDITS. Each names the edit and the row it fires on.
    ///
    /// - Delete the `visible >= total` guard: row 5 returns `Some` for a buffer that
    ///   fits entirely in its window, so every pane draws a full-height thumb whether
    ///   or not there is anything to scroll.
    /// - Delete `.max(SCROLLBAR_MIN_THUMB_PX)`: row 7's `h` shrinks to about 0.07 px
    ///   — a mark too small to see, which is exactly the case the floor exists for.
    /// - Delete `.min(track_h)`: row 8's `h` becomes 24 in a 10 px grid, so the thumb
    ///   is taller than the track it is drawn in and spills over the pane below. This
    ///   is a deliberate divergence from macOS, which omits the clamp.
    /// - Delete the upper clamp on `y`: row 2's thumb runs past the grid's lower edge,
    ///   which the containment assertion in the loop reports alongside the literal.
    /// - Subtract in `u64` before widening to `f64`: row 6 **panics** with `attempt to
    ///   subtract with overflow` rather than failing. Record the verbatim panic as the
    ///   red run; a panicking test and a failing one are different evidence.
    /// - Measure `x` from the grid rectangle (`grid.x + grid.w - THICKNESS - MARGIN`):
    ///   every `Some` row's `x` becomes 806, and the first row fires.
    /// - Delete the `x < 0.0` guard: row 11 returns a thumb at `x = -4` for a pane too
    ///   narrow to hold the gutter, i.e. a mark drawn outside the pane it belongs to.
    /// - Position the thumb from `curline` instead of `topline`: row 12 fires while
    ///   row 1 stays green, because row 1's cursor happens to sit on its top line and
    ///   row 12's is deliberately nowhere near it.
    #[test]
    fn scrollbar_thumb_table() {
        let cases = [
            ThumbCase {
                // visible 11 of 200 lines: h = 608 * 11/200 = 33.44, above the 24 px
                // floor; topline 0 puts the thumb at the track's top, y = 52.
                desc: "1: a viewport at the top of a long buffer",
                pane_w: 840.0,
                grid: GRID,
                viewport: Viewport {
                    topline: 0,
                    botline: 11,
                    curline: 0,
                    linecount: 200,
                },
                expected: Some((826.0, 52.0, 8.0, 33.44)),
            },
            ThumbCase {
                // y = 52 + (189/200) * 608 = 626.56, and 626.56 + 33.44 = 660 is
                // exactly the grid's lower edge — the thumb ends flush with it rather
                // than hanging below.
                desc: "2: a viewport at the bottom is flush with the grid's lower edge",
                pane_w: 840.0,
                grid: GRID,
                viewport: Viewport {
                    topline: 189,
                    botline: 200,
                    curline: 195,
                    linecount: 200,
                },
                expected: Some((826.0, 626.56, 8.0, 33.44)),
            },
            ThumbCase {
                desc: "3: an empty buffer has nothing to project",
                pane_w: 840.0,
                grid: GRID,
                viewport: Viewport {
                    topline: 0,
                    botline: 0,
                    curline: 0,
                    linecount: 0,
                },
                expected: None,
            },
            ThumbCase {
                // A window showing zero lines — the shape a just-created or
                // zero-height window reports.
                desc: "4: a zero-height viewport has nothing to project",
                pane_w: 840.0,
                grid: GRID,
                viewport: Viewport {
                    topline: 5,
                    botline: 5,
                    curline: 5,
                    linecount: 200,
                },
                expected: None,
            },
            ThumbCase {
                // The whole buffer fits: there is nothing to scroll, so there is
                // nothing to indicate.
                desc: "5: a buffer that fits entirely in its window has no thumb",
                pane_w: 840.0,
                grid: GRID,
                viewport: Viewport {
                    topline: 0,
                    botline: 200,
                    curline: 0,
                    linecount: 200,
                },
                expected: None,
            },
            ThumbCase {
                // THE row `u64` arithmetic panics on. `botline - topline` is -60 as a
                // real number; taken in `u64` it is an overflow, which is a debug
                // panic rather than a wrapped value.
                desc: "6: a botline BELOW its topline yields no thumb and no panic",
                pane_w: 840.0,
                grid: GRID,
                viewport: Viewport {
                    topline: 100,
                    botline: 40,
                    curline: 100,
                    linecount: 200,
                },
                expected: None,
            },
            ThumbCase {
                // 608 * 11/100000 = 0.0669 px, which the floor lifts to 24. Without
                // it the thumb is invisible in exactly the buffers most in need of one.
                desc: "7: a huge buffer's thumb stops shrinking at the floor",
                pane_w: 840.0,
                grid: GRID,
                viewport: Viewport {
                    topline: 0,
                    botline: 11,
                    curline: 0,
                    linecount: 100_000,
                },
                expected: Some((826.0, 52.0, 8.0, 24.0)),
            },
            ThumbCase {
                // The floor would ask for 24 px of thumb in a 10 px track. The clamp
                // wins, and the thumb is exactly its track. macOS omits this clamp.
                desc: "8: a grid shorter than the floor clamps the thumb to the grid",
                pane_w: 840.0,
                grid: GridRect {
                    x: 20.0,
                    y: 52.0,
                    w: 800.0,
                    h: 10.0,
                },
                viewport: Viewport {
                    topline: 0,
                    botline: 11,
                    curline: 0,
                    linecount: 200,
                },
                expected: Some((826.0, 52.0, 8.0, 10.0)),
            },
            ThumbCase {
                desc: "9: a non-finite pane width yields no thumb",
                pane_w: f64::NAN,
                grid: GRID,
                viewport: Viewport {
                    topline: 0,
                    botline: 11,
                    curline: 0,
                    linecount: 200,
                },
                expected: None,
            },
            ThumbCase {
                desc: "10: a non-finite grid height yields no thumb",
                pane_w: 840.0,
                grid: GridRect {
                    x: 20.0,
                    y: 52.0,
                    w: 800.0,
                    h: f64::NAN,
                },
                viewport: Viewport {
                    topline: 0,
                    botline: 11,
                    curline: 0,
                    linecount: 200,
                },
                expected: None,
            },
            ThumbCase {
                // 10 - 6 - 8 = -4: a pane narrower than its own gutter. The thumb
                // would be drawn outside the pane, over whatever is to its left.
                desc: "11: a pane too narrow to hold the gutter yields no thumb",
                pane_w: 10.0,
                grid: GRID,
                viewport: Viewport {
                    topline: 0,
                    botline: 11,
                    curline: 0,
                    linecount: 200,
                },
                expected: None,
            },
            ThumbCase {
                // Row 1 with an absurd `curline` and nothing else changed. The cursor
                // line is deliberately unread, so the thumb must not move — a cursor
                // move that scrolls nothing must not make the indicator jump.
                desc: "12: curline is unread, so a wild cursor line moves nothing",
                pane_w: 840.0,
                grid: GRID,
                viewport: Viewport {
                    topline: 0,
                    botline: 11,
                    curline: 12_345,
                    linecount: 200,
                },
                expected: Some((826.0, 52.0, 8.0, 33.44)),
            },
        ];

        for case in &cases {
            let got = scrollbar_thumb(case.pane_w, case.grid, case.viewport);
            match (got, case.expected) {
                (Some(thumb), Some(want)) => {
                    let f = fields(thumb);
                    assert!(
                        close(f.0, want.0)
                            && close(f.1, want.1)
                            && close(f.2, want.2)
                            && close(f.3, want.3),
                        "case: {} — scrollbar_thumb({}, {:?}, {:?}) = {f:?} as \
                         (x, y, w, h), want {want:?}. An `x` of 806 means the thumb \
                         is measured from the GRID rectangle's right edge rather \
                         than the PANE's, which puts it over the last column of \
                         glyphs; an `h` below {SCROLLBAR_MIN_THUMB_PX} means the \
                         floor is gone, and one above the grid's own height means \
                         the clamp is.",
                        case.desc,
                        case.pane_w,
                        case.grid,
                        case.viewport
                    );
                    assert!(
                        thumb.y >= case.grid.y - EPS
                            && thumb.y + thumb.h <= case.grid.y + case.grid.h + EPS,
                        "case: {} — the thumb {thumb:?} is not contained by its own \
                         track, which runs from {} to {}. The thumb spans the GRID \
                         rectangle so it never rides up over the statusbar strip or \
                         down past the pane; a violation here means the clamp on `y` \
                         or the `.min(track_h)` on the height went missing.",
                        case.desc,
                        case.grid.y,
                        case.grid.y + case.grid.h
                    );
                }
                (None, None) => {}
                _ => panic!(
                    "case: {} — scrollbar_thumb({}, {:?}, {:?}) = {got:?}, want {:?}. \
                     A `Some` where `None` was expected means a degenerate viewport \
                     produced a mark; a `None` where a thumb was expected means an \
                     ordinary window draws no indicator at all.",
                    case.desc, case.pane_w, case.grid, case.viewport, case.expected
                ),
            }
        }
    }

    /// The thumb lies wholly inside the pane's right padding gutter, at every pane
    /// width, so it can never cover a glyph.
    ///
    /// The first two assertions are relations between constants and are what make
    /// the per-width loop meaningful rather than incidental: the gutter is
    /// [`crate::editor_inset::EDITOR_PADDING_PX`] wide and already subtracted from
    /// every grid rectangle, so a thumb that fits inside it costs the grid nothing.
    /// Widen the bar past the padding and the loop's own bound would still hold
    /// while the mark sat over text — which is why the const relation is asserted
    /// first and names the gutter in its message.
    ///
    /// The pane widths are inputs only; the grid rectangle is held fixed because
    /// `x` depends on the pane width alone. A row's rectangle is therefore not the
    /// rectangle that pane would really produce, and nothing here reads it.
    ///
    /// FALSIFYING EDIT: raise [`SCROLLBAR_THICKNESS_PX`] to 24. The const relation
    /// fires first, naming the gutter, before any thumb is computed.
    #[test]
    fn the_thumb_lies_wholly_inside_the_right_gutter() {
        assert!(
            SCROLLBAR_THICKNESS_PX + SCROLLBAR_MARGIN_PX <= EDITOR_PADDING_PX,
            "the thumb is {SCROLLBAR_THICKNESS_PX} px wide {SCROLLBAR_MARGIN_PX} px \
             in from the pane's right edge, so it needs \
             {} px of gutter — more than the {EDITOR_PADDING_PX} px of editor \
             padding there is. The bar would then be drawn over the last column of \
             glyphs in every pane. The padding is already subtracted from every grid \
             rectangle, which is what makes a thumb inside it cost the grid nothing; \
             widening the bar past the padding means either covering text or taking \
             a column away from Neovim, and the second is a reflow on every scroll.",
            SCROLLBAR_THICKNESS_PX + SCROLLBAR_MARGIN_PX
        );
        assert!(
            SCROLLBAR_MARGIN_PX >= 4.0,
            "the thumb sits {SCROLLBAR_MARGIN_PX} px in from the pane's right edge, \
             inside the active-pane focus ring. `ACTIVE_PANE_CSS_DATA` is \
             `outline: 2px solid; outline-offset: -2px`, so the ring covers the \
             pane's last four pixels; a margin below 4 puts the thumb under it and \
             the mark changes appearance with which pane has focus."
        );

        for pane_w in [100.0, 320.0, 840.0, 2500.0] {
            let viewport = Viewport {
                topline: 0,
                botline: 11,
                curline: 0,
                linecount: 200,
            };
            let Some(thumb) = scrollbar_thumb(pane_w, GRID, viewport) else {
                panic!(
                    "a {pane_w} px pane showing 11 of 200 lines produced NO thumb. \
                     Every width here is wider than the gutter, so the only ways to \
                     get here are a viewport guard that rejects an ordinary window \
                     or an `x < 0.0` guard that is measuring something other than \
                     the gutter."
                )
            };
            assert!(
                thumb.x >= pane_w - EDITOR_PADDING_PX,
                "a {pane_w} px pane put the thumb's left edge at {}, which is left \
                 of the gutter's own left edge at {}. Anything left of that is over \
                 the grid rectangle, i.e. over glyphs.",
                thumb.x,
                pane_w - EDITOR_PADDING_PX
            );
            assert!(
                thumb.x + thumb.w <= pane_w,
                "a {pane_w} px pane put the thumb's right edge at {}, outside the \
                 pane. Cairo will happily draw there, over whatever widget the pane \
                 is next to.",
                thumb.x + thumb.w
            );
        }
    }

    /// One [`scrollbar_thumb_color`] row: an editor background and the thumb colour
    /// blended over it.
    struct ColorCase {
        desc: &'static str,
        bg: Rgb,
        expected: Rgb,
    }

    /// The thumb is a neutral grey blended toward the EDITOR's own background, so it
    /// reads as an indicator on a dark colorscheme and on a light one alike.
    ///
    /// Blending toward the background rather than compositing with an alpha is what
    /// keeps the crate's "no `set_source_rgba` on the draw path" property: the thumb
    /// is painted over an already-filled background, so blending toward it produces
    /// exactly the pixels alpha compositing would. The blend itself is
    /// `statusbar_layout`'s, reused rather than reimplemented, because a second
    /// rounding rule lets the two drift.
    ///
    /// The LIGHT row is the discriminator and is listed first deliberately. The
    /// obvious wrong implementation — blending toward black instead of toward the
    /// background — moves it from `#c3c3c3` to `#3a3a3a`, while the dark row barely
    /// moves at all (`#4a4a4a` to `#3a3a3a`). Fail-fast ordering therefore reports
    /// the row whose failure names the blend TARGET rather than the one whose
    /// failure looks like a rounding wobble.
    ///
    /// FALSIFYING EDITS:
    ///
    /// - Blend toward black rather than toward `bg`: the light row fires, 137 units
    ///   out on every channel, while the dark row would have been out by 16.
    /// - Change [`SCROLLBAR_THUMB_ALPHA`] or [`SCROLLBAR_GREY`]: both rows fire, and
    ///   the pair of literals says which — a grey change moves the two rows in the
    ///   same direction, an alpha change moves them in opposite ones.
    #[test]
    fn scrollbar_thumb_color_table() {
        let cases = [
            ColorCase {
                // round(250 + (128 - 250) * 0.45) = round(195.1) = 195 = 0xc3.
                desc: "a light editor background darkens the grey toward it",
                bg: Rgb {
                    r: 0xfa,
                    g: 0xfa,
                    b: 0xfa,
                },
                expected: Rgb {
                    r: 0xc3,
                    g: 0xc3,
                    b: 0xc3,
                },
            },
            ColorCase {
                // round(30 + (128 - 30) * 0.45) = round(74.1) = 74 = 0x4a.
                desc: "a dark editor background lightens the grey toward it",
                bg: Rgb {
                    r: 0x1e,
                    g: 0x1e,
                    b: 0x1e,
                },
                expected: Rgb {
                    r: 0x4a,
                    g: 0x4a,
                    b: 0x4a,
                },
            },
        ];

        for case in &cases {
            let got = scrollbar_thumb_color(case.bg);
            assert_eq!(
                got, case.expected,
                "case: {} — scrollbar_thumb_color({:?}) = {got:?}, want {:?}. The \
                 thumb is {SCROLLBAR_GREY} on every channel blended \
                 {SCROLLBAR_THUMB_ALPHA} of the way toward the EDITOR's background: \
                 a result far from the background on the light row means the blend \
                 target is a fixed colour rather than `bg`, and a result equal to \
                 {SCROLLBAR_GREY} on both rows means the blend is not happening at \
                 all.",
                case.desc, case.bg, case.expected
            );
        }
    }
}
