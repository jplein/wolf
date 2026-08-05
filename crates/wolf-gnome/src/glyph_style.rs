//! The pure, widget-free rulebook for the three text decorations a cell carries
//! as PANGO ATTRIBUTES — underline, undercurl and strikethrough — plus the colour
//! the first two are drawn in. The Rust counterpart of
//! `WolfKeyMap/GlyphStyle.swift`, named for it so the parity obligation stays
//! legible.
//!
//! Data in, data out: a [`wolf_core::ResolvedStyle`] arrives and a
//! [`GlyphDecoration`] leaves. `crate::render::draw_glyph` needs a live Cairo
//! context, so a rule left inside it could only ever be executed by the
//! golden-render target — the same argument [`crate::cell_metrics_math`] records
//! for the metrics arithmetic, and the reason this is a separate file rather than
//! three `if`s in the paint loop.
//!
//! ## Attributes only, and that IS the whole port
//!
//! macOS's counterpart also resolves the FACE. Here weight and style ride on the
//! cloned [`pango::FontDescription`] (`set_weight` / `set_style`) rather than on
//! attributes, so `draw_glyph` keeps its two `font.set_*` calls and this module
//! covers exactly the three attribute-borne decorations. Nothing is half-done.
//!
//! ## Pango draws the wave; nothing here strokes a Cairo path
//!
//! `undercurl` maps to [`pango::Underline::Error`], Pango's wavy underline, for
//! three reasons. A hand-stroked Cairo wave would give `draw_glyph` a SECOND
//! drawing mechanism with its own thickness, amplitude and vertical placement,
//! which is exactly what `crates/wolf-gnome/DECISIONS.md`'s one-glyph-path rule
//! exists to prevent. The placement is Pango's to know in any case: the underline
//! position and thickness come from the face's own `OS/2` / `post` metrics, which
//! Pango already reads and this crate does not, so a hand-stroked wave would need
//! either a second metrics probe or a magic constant that drifts. And one integer
//! attribute costs the hot paint loop nothing, where a per-cell path construction
//! does not.
//!
//! ## Undercurl wins over a plain underline, because Pango cannot draw both
//!
//! Neovim can set `underline` and `undercurl` on one highlight group. Pango's
//! underline attribute is SINGLE-VALUED over a range — there is one
//! `PANGO_ATTR_UNDERLINE` per run — so [`pango::Underline::Single`] and
//! [`pango::Underline::Error`] cannot coexist and one of them has to lose.
//! Undercurl wins: it is the more specific request, and it is what every terminal
//! GUI draws for `cterm=underline,undercurl`. That rule is the one a later reader
//! is most likely to "simplify" by reordering the arms, so it carries its own
//! named test rather than riding inside the table.
//!
//! ## The strikethrough colour is deliberately never named
//!
//! `pango::AttrColor::new_strikethrough_color` exists and is deliberately not
//! called: with no such attribute Pango strikes in the LAYOUT's foreground, which
//! is exactly what macOS does (`GlyphStyleResolver.resolve` sets
//! `strikethroughColor: fg` unconditionally, while `underlineColor` is `special`
//! when set and `fg` otherwise). Not calling it buys that parity for free and one
//! fewer attribute per decorated cell, which is why [`GlyphDecoration`] carries no
//! strikethrough-colour field at all. Adding one is a change to argue for, not an
//! oversight to fix.
//!
//! ## Naming a `pango::Underline` in a pure module is deliberate
//!
//! [`crate::actions`] sets the precedent, and states the reason, for `gdk::Key`
//! and `gdk::ModifierType`: these are plain VALUE types — no widget, no display,
//! no initialized main thread — so naming one costs nothing in testability and
//! lets the seam say exactly what the widget layer will hand to Pango. The whole
//! of this module, its tests included, runs in the fast unit suite.
//!
//! ## Divergence from macOS
//!
//! macOS folds `undercurl` into a STRAIGHT underline; this front-end draws the
//! wave Neovim asked for. `macos-followups.md` records it as a portable
//! improvement rather than a Linux quirk.

use wolf_core::{ResolvedStyle, Rgb};

/// Multiplier that widens an 8-bit colour channel to Pango's 16-bit one.
///
/// 257, never 256: `255 * 257 == 65535` reaches the top of the range EXACTLY,
/// while `255 * 256 == 65280` lands a visibly off-white on every decoration drawn
/// in a pure channel. The error is invisible to any test of the wave's shape,
/// which is why [`pango_channels`] is pinned by value.
const PANGO_CHANNEL_SCALE: u16 = 257;

/// The decorations one cell's style asks Pango for.
///
/// `underline` is [`pango::Underline::None`] when the cell wants no underline at
/// all, so the widget layer inserts an underline attribute only when it is not
/// that. `underline_color` of `None` means "leave Pango's default", which is the
/// layout's own foreground — it is never a colour for a decoration that is not
/// drawn, because it is `None` whenever `underline` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphDecoration {
    /// Which underline Pango draws under the run, if any.
    pub underline: pango::Underline,
    /// Whether the run is struck through. The colour is deliberately not carried;
    /// see this module's docs.
    pub strikethrough: bool,
    /// The colour the underline is drawn in, or `None` to leave it to Pango.
    pub underline_color: Option<Rgb>,
}

impl GlyphDecoration {
    /// Whether this is no decoration at all: no underline and no strikethrough.
    ///
    /// `draw_glyph` builds no `AttrList` when this answers `true`, which is what
    /// keeps the undecorated cell — nearly every cell of nearly every frame —
    /// allocating exactly what it allocated before decorations existed.
    ///
    /// `underline_color` is not consulted and cannot matter: it is `None`
    /// whenever `underline` is [`pango::Underline::None`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.underline == pango::Underline::None && !self.strikethrough
    }
}

/// Resolves one cell's [`ResolvedStyle`] into the decorations Pango is asked for.
///
/// The rule is total by construction:
///
/// | `undercurl` | `underline` | `underline` field | `underline_color` |
/// | --- | --- | --- | --- |
/// | true | either | [`pango::Underline::Error`] | `style.special` |
/// | false | true | [`pango::Underline::Single`] | `style.special` |
/// | false | false | [`pango::Underline::None`] | `None` |
///
/// `strikethrough` copies `style.strikethrough` in every row. `underline_color`
/// is `None` whenever no underline is drawn, and it NEVER falls back to
/// `style.fg`: an unset `special` is a request to leave the colour to Pango,
/// which draws the decoration in the layout foreground anyway, so naming the
/// foreground here would only make the seam claim a colour it was not given.
///
/// Takes `&ResolvedStyle` to match `draw_glyph`'s own parameter.
#[must_use]
pub fn glyph_decoration(style: &ResolvedStyle) -> GlyphDecoration {
    // `undercurl` is tested FIRST, and that order is the rule: Pango carries one
    // underline attribute per run, so a style with both bits set has to lose one
    // and the wave is the one Neovim asked for.
    let underline = if style.undercurl {
        pango::Underline::Error
    } else if style.underline {
        pango::Underline::Single
    } else {
        pango::Underline::None
    };

    GlyphDecoration {
        underline,
        strikethrough: style.strikethrough,
        underline_color: if underline == pango::Underline::None {
            None
        } else {
            style.special
        },
    }
}

/// Widens an [`Rgb`] to the 16-bit channel triple Pango's colour attributes take.
///
/// `255` must reach `65535` exactly; see [`PANGO_CHANNEL_SCALE`].
#[must_use]
pub fn pango_channels(color: Rgb) -> (u16, u16, u16) {
    (
        u16::from(color.r) * PANGO_CHANNEL_SCALE,
        u16::from(color.g) * PANGO_CHANNEL_SCALE,
        u16::from(color.b) * PANGO_CHANNEL_SCALE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use pango::Underline;

    /// The fixture's foreground.
    ///
    /// LOAD-BEARING that it is `Some` and distinct from [`FIXTURE_SPECIAL`]: the
    /// revert these cases exist to catch is `underline_color: style.special
    /// .or(style.fg)`, and against a fixture whose `fg` is `None` that revert
    /// returns exactly what the correct rule returns. A style built with
    /// `..ResolvedStyle::default()` would have that hole.
    const FIXTURE_FG: Rgb = Rgb {
        r: 0xee,
        g: 0xee,
        b: 0xee,
    };
    /// The fixture's background. Distinct from the other two so a rule that
    /// reached for the wrong field names itself.
    const FIXTURE_BG: Rgb = Rgb {
        r: 0x11,
        g: 0x11,
        b: 0x11,
    };
    /// The fixture's `special` colour — the one value a correct
    /// `underline_color` may ever carry.
    const FIXTURE_SPECIAL: Rgb = Rgb {
        r: 0xd0,
        g: 0x20,
        b: 0x30,
    };

    /// A style whose `special` is unset.
    const NO_SPECIAL: Option<Rgb> = None;
    /// A style whose `special` is set, spelled once so the table rows stay
    /// readable at a glance.
    const SPECIAL: Option<Rgb> = Some(FIXTURE_SPECIAL);

    /// One row's inputs: `(underline, undercurl, strikethrough, special)`.
    type Inputs = (bool, bool, bool, Option<Rgb>);
    /// One row's expected output:
    /// `(underline, strikethrough, underline_color, is_empty)`.
    type Expected = (Underline, bool, Option<Rgb>, bool);

    /// Builds a style carrying the three decoration bits and a `special`.
    ///
    /// Every field is spelled out rather than filled from
    /// `..ResolvedStyle::default()`, so a fourth decoration field appearing in
    /// `wolf_core::ResolvedStyle` is a compile error here — the alternative is a
    /// new field this table silently never exercises.
    fn style_with(
        underline: bool,
        undercurl: bool,
        strikethrough: bool,
        special: Option<Rgb>,
    ) -> ResolvedStyle {
        ResolvedStyle {
            fg: Some(FIXTURE_FG),
            bg: Some(FIXTURE_BG),
            special,
            bold: false,
            italic: false,
            underline,
            undercurl,
            strikethrough,
        }
    }

    /// The WHOLE domain of [`glyph_decoration`]: all eight combinations of the
    /// three decoration bits, each with `special` present and absent, pinning all
    /// three output fields plus [`GlyphDecoration::is_empty`].
    ///
    /// `is_empty` rides here rather than in a fifth case because it is a
    /// projection of the same row — asserting it separately would restate the
    /// table with two of its columns hidden.
    ///
    /// Every row is evaluated and the failures are reported together, rather than
    /// aborting at the first one: which SUBSET of the sixteen reddens is the
    /// diagnostic. Dropping the strikethrough copy (`strikethrough: false`
    /// always) reddens the eight strikethrough rows and nothing else; swapping the
    /// two underline arms reddens the four `underline && undercurl` rows;
    /// `underline_color: style.special.or(style.fg)` reddens the eight
    /// `special: None` rows that draw an underline.
    #[test]
    fn glyph_decoration_table() {
        let rows: [(Inputs, Expected); 16] = [
            (
                (false, false, false, NO_SPECIAL),
                (Underline::None, false, NO_SPECIAL, true),
            ),
            (
                (false, false, false, SPECIAL),
                (Underline::None, false, NO_SPECIAL, true),
            ),
            (
                (false, false, true, NO_SPECIAL),
                (Underline::None, true, NO_SPECIAL, false),
            ),
            (
                (false, false, true, SPECIAL),
                (Underline::None, true, NO_SPECIAL, false),
            ),
            (
                (false, true, false, NO_SPECIAL),
                (Underline::Error, false, NO_SPECIAL, false),
            ),
            (
                (false, true, false, SPECIAL),
                (Underline::Error, false, SPECIAL, false),
            ),
            (
                (false, true, true, NO_SPECIAL),
                (Underline::Error, true, NO_SPECIAL, false),
            ),
            (
                (false, true, true, SPECIAL),
                (Underline::Error, true, SPECIAL, false),
            ),
            (
                (true, false, false, NO_SPECIAL),
                (Underline::Single, false, NO_SPECIAL, false),
            ),
            (
                (true, false, false, SPECIAL),
                (Underline::Single, false, SPECIAL, false),
            ),
            (
                (true, false, true, NO_SPECIAL),
                (Underline::Single, true, NO_SPECIAL, false),
            ),
            (
                (true, false, true, SPECIAL),
                (Underline::Single, true, SPECIAL, false),
            ),
            (
                (true, true, false, NO_SPECIAL),
                (Underline::Error, false, NO_SPECIAL, false),
            ),
            (
                (true, true, false, SPECIAL),
                (Underline::Error, false, SPECIAL, false),
            ),
            (
                (true, true, true, NO_SPECIAL),
                (Underline::Error, true, NO_SPECIAL, false),
            ),
            (
                (true, true, true, SPECIAL),
                (Underline::Error, true, SPECIAL, false),
            ),
        ];

        let mut failures: Vec<String> = Vec::new();
        for (inputs, expected) in rows {
            let (underline, undercurl, strikethrough, special) = inputs;
            let (want_underline, want_strikethrough, want_color, want_empty) = expected;

            let got = glyph_decoration(&style_with(underline, undercurl, strikethrough, special));
            let row = format!(
                "underline: {underline}, undercurl: {undercurl}, \
                 strikethrough: {strikethrough}, special: {special:?}"
            );

            if got.underline != want_underline {
                failures.push(format!(
                    "[{row}] underline is {:?}, want {want_underline:?}",
                    got.underline
                ));
            }
            if got.strikethrough != want_strikethrough {
                failures.push(format!(
                    "[{row}] strikethrough is {}, want {want_strikethrough}",
                    got.strikethrough
                ));
            }
            if got.underline_color != want_color {
                failures.push(format!(
                    "[{row}] underline_color is {:?}, want {want_color:?}",
                    got.underline_color
                ));
            }
            if got.is_empty() != want_empty {
                failures.push(format!(
                    "[{row}] is_empty() is {}, want {want_empty}",
                    got.is_empty()
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "glyph_decoration disagreed with its own rule table on {} of the 64 field \
             assertions over 16 rows:\n{}\n\nThe fields fail for different reasons and the failing \
             subset names which: a wrong `underline` on the four rows where BOTH bits are set \
             means the arm order was swapped, so a straight line is drawn where Neovim asked for a \
             wave; a wrong `strikethrough` on all eight strikethrough rows means the copy was \
             dropped and no cell is ever struck; a wrong `underline_color` on a `special: None` \
             row means the rule fell back to `style.fg` ({FIXTURE_FG:?} in this fixture) and the \
             seam now names a colour it was never given; a wrong `underline_color` on an \
             `Underline::None` row means a colour is carried for a decoration that is not drawn at \
             all; a wrong `is_empty` alone means the hot path's skip disagrees with the decoration \
             beside it, which either allocates an AttrList for nothing or drops a decoration the \
             table resolved.",
            failures.len(),
            failures.join("\n")
        );
    }

    /// With BOTH bits set the answer is the wave, stated on its own so a
    /// regression names the rule rather than four table rows.
    ///
    /// Falsifier: swap the two arms in `glyph_decoration` so `underline` is tested
    /// first. This case reddens ahead of the table, and reports `Single` — which
    /// is precisely macOS's `hasUnderline = underline || undercurl`.
    #[test]
    fn undercurl_wins_over_a_plain_underline() {
        for special in [NO_SPECIAL, SPECIAL] {
            let got = glyph_decoration(&style_with(true, true, false, special));
            assert_eq!(
                got.underline,
                Underline::Error,
                "a style with `underline` AND `undercurl` set (special: {special:?}) resolved to \
                 {:?}. Pango's underline attribute is single-valued over a run, so exactly one of \
                 them can be drawn and undercurl is the one that wins. A `Single` here means the \
                 plain-underline arm was tested first and the wave Neovim asked for is drawn as a \
                 straight line; a `None` means neither arm ran.",
                got.underline
            );
        }
    }

    /// An unset `special` leaves the underline colour to Pango, and a style that
    /// draws no underline names no colour either.
    ///
    /// Falsifier: `underline_color: style.special.or(style.fg)`. Every row below
    /// then reports `Some(#eeeeee)` — the fixture's foreground is `Some` and
    /// distinct precisely so that revert is observable rather than
    /// indistinguishable from the correct answer.
    #[test]
    fn an_unset_special_leaves_the_underline_colour_to_pango() {
        let plain = glyph_decoration(&style_with(true, false, false, NO_SPECIAL));
        assert_eq!(
            plain.underline_color, NO_SPECIAL,
            "a plain underline with no `special` resolved to underline_color {:?}. It must be \
             None: Pango then draws the underline in the layout's own foreground, which is what \
             an unset `special` means. A Some({FIXTURE_FG:?}) is the `.or(style.fg)` fallback, \
             which makes the seam name a colour Neovim never sent.",
            plain.underline_color
        );

        let curl = glyph_decoration(&style_with(false, true, false, NO_SPECIAL));
        assert_eq!(
            curl.underline_color, NO_SPECIAL,
            "an undercurl with no `special` resolved to underline_color {:?}, not None. The \
             undercurl arm must take the colour from `style.special` alone, exactly as the plain \
             underline arm does — a fallback in one arm and not the other is how the two drift.",
            curl.underline_color
        );

        let undecorated = glyph_decoration(&style_with(false, false, true, SPECIAL));
        assert_eq!(
            undecorated.underline_color, NO_SPECIAL,
            "a style that draws NO underline but does carry a `special` resolved to \
             underline_color {:?}. It must be None: a colour for a decoration that is not drawn \
             is a value nothing can render, and it invites a later reader to insert the \
             AttrColor unconditionally.",
            undecorated.underline_color
        );
    }

    /// The 8-bit to 16-bit widening reaches the top of the range exactly.
    ///
    /// The expectations are LITERALS, never `255 * PANGO_CHANNEL_SCALE`: written
    /// that way they would move with the constant they exist to pin. The mid row
    /// carries three DIFFERENT channels, so a transposed return (`(b, g, r)`)
    /// fails it too.
    ///
    /// Falsifier: change the scale to 256. The white row then reports 65280, a
    /// visibly off-white for every decoration drawn in a saturated channel.
    #[test]
    fn pango_channels_scale_a_byte_to_the_full_range() {
        let white = pango_channels(Rgb {
            r: 255,
            g: 255,
            b: 255,
        });
        assert_eq!(
            white,
            (65535, 65535, 65535),
            "255 must widen to 65535 in every channel, but this gave {white:?}. A 65280 is \
             `* 256`, which never reaches the top of Pango's range: an underline asked for in \
             pure white is drawn a shade under it, and the same error scales every saturated \
             decoration colour."
        );

        let black = pango_channels(Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(
            black,
            (0, 0, 0),
            "0 must widen to 0 in every channel, but this gave {black:?}. A non-zero answer means \
             an offset was added to make the white row come out right."
        );

        let mid = pango_channels(Rgb {
            r: 128,
            g: 64,
            b: 32,
        });
        assert_eq!(
            mid,
            (32896, 16448, 8224),
            "(128, 64, 32) must widen to (32896, 16448, 8224) — each channel times 257 — but this \
             gave {mid:?}. (32768, 16384, 8192) is `* 256`; (8224, 16448, 32896) is the triple \
             returned in the wrong order, which would draw every underline in a colour with its \
             red and blue swapped."
        );
    }
}
