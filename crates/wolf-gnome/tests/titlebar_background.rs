//! Table-driven tests for `wolf_gnome::titlebar_background`: the rule that picks the header
//! bar's and the tab bar's background from `WolfTitlebar` and `Normal`, the stylesheet that
//! carries it, and the change detection that decides when the stylesheet is rewritten.
//!
//! Pure: no display, no Neovim and no compositor. Every expected value is written out by
//! hand, never computed by the function under test, except where T7 states that the
//! stylesheet it expects is `titlebar_css`'s, which T4 to T6 pin. Whether the window paints
//! the answer is the titlebar smokes' question, in `smoke_headless/titlebar.rs`.

use wolf_core::Rgb;
use wolf_gnome::titlebar_background::{
    stylesheet_update, titlebar_background, titlebar_css, TitlebarBackground,
};

/// The `WolfTitlebar` background of every case. Its three channels differ from each other
/// and from [`NORMAL`]'s, so a swapped argument or a swapped channel reads as a wrong value.
const TITLEBAR: u32 = 0x7a_2b_5c;

/// The `Normal` background of every case.
const NORMAL: u32 = 0x1b_3c_d4;

/// A second background, for a change from one colour to another.
const OTHER: u32 = 0x40_a0_10;

/// The class every stylesheet case is built for. No other word of a stylesheet contains it.
const CLASS: &str = "wolf-titlebar-zq7";

fn rgb(packed: u32) -> Rgb {
    Rgb::from_packed(packed)
}

/// T1, falsifier C1.
#[test]
fn a_set_wolf_titlebar_beats_normal() {
    assert_eq!(
        titlebar_background(Some(rgb(TITLEBAR)), Some(rgb(NORMAL))),
        TitlebarBackground::Colour(rgb(TITLEBAR)),
        "with both WolfTitlebar and Normal set, the bars must take WolfTitlebar's background. \
         Colour(#1b3cd4) means Normal was tested first; Libadwaita means a set colour was \
         dropped"
    );
    assert_eq!(
        titlebar_background(Some(rgb(TITLEBAR)), None),
        TitlebarBackground::Colour(rgb(TITLEBAR)),
        "with WolfTitlebar set and Normal unknown, the bars must still take WolfTitlebar's \
         background. Libadwaita means WolfTitlebar is consulted only when Normal is known"
    );
}

/// T2, falsifier C2.
#[test]
fn normal_fills_in_when_wolf_titlebar_is_unset() {
    assert_eq!(
        titlebar_background(None, Some(rgb(NORMAL))),
        TitlebarBackground::Colour(rgb(NORMAL)),
        "with WolfTitlebar unset, the bars must take Normal's background, the colour the grid \
         fills with, so the two match. Libadwaita means the Normal fallback is missing, which \
         is the case of every colorscheme that leaves WolfTitlebar unset, Lighthouse included"
    );
}

/// T3, falsifier C3.
#[test]
fn libadwaita_keeps_the_bar_while_neither_is_known() {
    assert_eq!(
        titlebar_background(None, None),
        TitlebarBackground::Libadwaita,
        "before the first frame neither colour is known, and the bars must keep libadwaita's \
         own colour. Colour(#000000) means the grid's fallback background was substituted for \
         an unknown Normal"
    );
}

/// T4, falsifier C4.
#[test]
fn the_stylesheet_carries_the_colour_in_rgb_order() {
    let css = titlebar_css(CLASS, TitlebarBackground::Colour(rgb(0x12_34_56)));
    assert!(
        css.contains(CLASS),
        "the stylesheet for a colour must name the window's class, or its rule matches no \
         widget, or every window's: {css:?}"
    );
    assert!(
        css.contains("#123456"),
        "the stylesheet must write #123456 for Rgb {{ r: 0x12, g: 0x34, b: 0x56 }}, as \
         lowercase #rrggbb. #563412 means the channels are written in b, g, r order; no hex at \
         all means another notation: {css:?}"
    );
    assert!(
        !css.contains("#563412"),
        "the stylesheet carries #563412, the channels of #123456 reversed: {css:?}"
    );
}

/// T5, falsifier C5.
#[test]
fn the_stylesheet_sets_no_foreground() {
    let css = titlebar_css(CLASS, TitlebarBackground::Colour(rgb(TITLEBAR)));
    assert!(
        css.contains("background-color:"),
        "the stylesheet for a colour must declare background-color, or this test's search for \
         a foreground declaration below proves nothing: {css:?}"
    );
    let rest = css.replace("background-color:", "");
    assert!(
        !rest.contains("color:"),
        "the stylesheet must set the background alone. A color declaration overrides \
         libadwaita's header bar foreground, which must stay the resolved scheme's, so the \
         title, the icons and the tab text would take a colour from the colorscheme: {css:?}"
    );
}

/// T6, falsifier C6.
#[test]
fn libadwaita_takes_an_empty_stylesheet() {
    assert_eq!(
        titlebar_css(CLASS, TitlebarBackground::Libadwaita),
        "",
        "Libadwaita must give an empty stylesheet, so libadwaita's own header bar and tab bar \
         rules apply. A rule here, such as one for #000000, paints the bars before the first \
         frame says what colour they are"
    );
}

/// T7, falsifiers C7a and C7b. The rewrites cover each kind of change: from libadwaita's
/// colour to a colour, from one colour to another, and from a colour back to libadwaita's.
#[test]
fn only_a_changed_background_rewrites_the_stylesheet() {
    let colour = TitlebarBackground::Colour(rgb(NORMAL));
    let other = TitlebarBackground::Colour(rgb(OTHER));
    let libadwaita = TitlebarBackground::Libadwaita;

    for same in [libadwaita, colour] {
        assert_eq!(
            stylesheet_update(same, same, CLASS),
            None,
            "applied and wanted are both {same:?}, so the stylesheet must not be rewritten. \
             Some means every render wake reloads the provider's CSS, and GTK restyles both \
             bars, for a colour that did not change"
        );
    }

    for (applied, wanted) in [(libadwaita, colour), (colour, other), (colour, libadwaita)] {
        assert_eq!(
            stylesheet_update(applied, wanted, CLASS),
            Some(titlebar_css(CLASS, wanted)),
            "applied is {applied:?} and wanted is {wanted:?}, so the stylesheet must be \
             rewritten to the wanted one. None means a live :colorscheme or :set background \
             leaves the bars at the old colour; Some of another stylesheet means the rewrite \
             carries the applied colour, or another class"
        );
    }
}
