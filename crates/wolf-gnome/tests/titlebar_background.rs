//! Table-driven tests for `wolf_gnome::titlebar_background`: the rule that picks the header
//! bar's and the tab bar's background from `WolfTitlebar` and `Normal`, the rule that picks an
//! empty tab's view's background from `Normal` alone, the stylesheets that carry them, and the
//! change detection that decides when a stylesheet is rewritten.
//!
//! Pure: no display, no Neovim and no compositor. Every expected value is written out by
//! hand, never computed by the function under test, except where T7 states that the
//! stylesheet it expects is the output of the stylesheet function it passes in, which T4 to
//! T6 and T10 to T12 pin. Whether the window paints the answer is the titlebar smokes'
//! question, in `smoke_headless/titlebar.rs`, the empty-view smoke's included.

use wolf_core::Rgb;
use wolf_gnome::titlebar_background::{
    empty_state_background, empty_state_css, stylesheet_update, titlebar_background, titlebar_css,
    TitlebarBackground,
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

/// The class every empty-state stylesheet case is built for. No other word of a stylesheet
/// contains it.
const EMPTY_CLASS: &str = "wolf-empty-state-zq7";

/// The signature `stylesheet_update` takes its stylesheet function as.
type Stylesheet = fn(&str, TitlebarBackground) -> String;

/// The stylesheet functions T7 runs, each with the name its messages give it.
const STYLESHEETS: [(&str, Stylesheet); 2] = [
    ("titlebar_css", titlebar_css),
    ("empty_state_css", empty_state_css),
];

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

/// T7, falsifiers C7a, C7b and ES6. The rewrites cover each kind of change: from
/// libadwaita's colour to a colour, from one colour to another, and from a colour back to
/// libadwaita's. Each row runs once per stylesheet function, which it passes in and whose
/// own output it expects.
#[test]
fn only_a_changed_background_rewrites_the_stylesheet() {
    let colour = TitlebarBackground::Colour(rgb(NORMAL));
    let other = TitlebarBackground::Colour(rgb(OTHER));
    let libadwaita = TitlebarBackground::Libadwaita;

    for (name, css) in STYLESHEETS {
        for same in [libadwaita, colour] {
            assert_eq!(
                stylesheet_update(same, same, CLASS, css),
                None,
                "with {name}, applied and wanted are both {same:?}, so the stylesheet must not \
                 be rewritten. Some means every render wake reloads the provider's CSS, and GTK \
                 restyles the provider's widgets, for a colour that did not change"
            );
        }

        for (applied, wanted) in [(libadwaita, colour), (colour, other), (colour, libadwaita)] {
            assert_eq!(
                stylesheet_update(applied, wanted, CLASS, css),
                Some(css(CLASS, wanted)),
                "with {name}, applied is {applied:?} and wanted is {wanted:?}, so the \
                 stylesheet must be rewritten to {name}'s wanted one. None means a live \
                 :colorscheme or :set background leaves the widgets at the old colour; Some of \
                 another stylesheet means the rewrite carries the applied colour, another \
                 class, or another function's rule, such as the header bar's for the empty \
                 view"
            );
        }
    }
}

/// T8, falsifier ES1.
#[test]
fn the_empty_state_takes_normals_background() {
    assert_eq!(
        empty_state_background(Some(rgb(NORMAL))),
        TitlebarBackground::Colour(rgb(NORMAL)),
        "with Normal known, an empty tab's view must take Normal's background, the colour the \
         grid fills with. Libadwaita means the view keeps the window background after the \
         first frame; another colour means the view was given something other than Normal"
    );
}

/// T9, falsifier ES2.
#[test]
fn the_empty_state_keeps_libadwaita_while_normal_is_unknown() {
    assert_eq!(
        empty_state_background(None),
        TitlebarBackground::Libadwaita,
        "before the first frame Normal is unknown, and an empty tab's view must keep \
         libadwaita's window background. Colour(#000000) means the grid's fallback background \
         was substituted for an unknown Normal"
    );
}

/// T10, falsifier ES3.
#[test]
fn the_empty_state_stylesheet_carries_the_colour_in_rgb_order() {
    let css = empty_state_css(EMPTY_CLASS, TitlebarBackground::Colour(rgb(0x12_34_56)));
    assert!(
        css.contains(&format!(".{EMPTY_CLASS}")),
        "the empty-state stylesheet for a colour must select the tab's class, .{EMPTY_CLASS}, \
         or its rule matches no widget, or every tab's view: {css:?}"
    );
    assert!(
        css.contains("#123456"),
        "the empty-state stylesheet must write #123456 for Rgb {{ r: 0x12, g: 0x34, b: 0x56 }}, \
         as lowercase #rrggbb. #563412 means the channels are written in b, g, r order; no hex \
         at all means another notation: {css:?}"
    );
    assert!(
        !css.contains("#563412"),
        "the empty-state stylesheet carries #563412, the channels of #123456 reversed: {css:?}"
    );
}

/// T11, falsifier ES4. A declaration is a non-blank `;`-separated part of the rule's body.
#[test]
fn the_empty_state_stylesheet_declares_the_background_alone() {
    let css = empty_state_css(EMPTY_CLASS, TitlebarBackground::Colour(rgb(TITLEBAR)));
    assert_eq!(
        (css.matches('{').count(), css.matches('}').count()),
        (1, 1),
        "the empty-state stylesheet for a colour must be one rule, so that its body below is \
         the whole of what it declares: {css:?}"
    );
    let body = css
        .split_once('{')
        .and_then(|(_, rest)| rest.split_once('}'))
        .map_or("", |(body, _)| body);
    let declarations: Vec<&str> = body
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let properties: Vec<&str> = declarations
        .iter()
        .map(|declaration| declaration.split(':').next().unwrap_or("").trim())
        .collect();
    assert_eq!(
        properties,
        ["background-color"],
        "the empty-state rule must declare background-color alone. A color declaration \
         overrides libadwaita's foreground, which the view's button and text keep; a margin, \
         padding or size moves the view's geometry; no declaration means the colour is not \
         applied. Declarations: {declarations:?}, in {css:?}"
    );
}

/// T12, falsifier ES5.
#[test]
fn libadwaita_gives_the_empty_state_an_empty_stylesheet() {
    assert_eq!(
        empty_state_css(EMPTY_CLASS, TitlebarBackground::Libadwaita),
        "",
        "Libadwaita must give an empty stylesheet, so the view's background class keeps its \
         window background. A rule here, such as one for #000000, paints the view before the \
         first frame says what colour Normal is"
    );
}
