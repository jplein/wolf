//! Tests for `wolf_gnome::preview_width`: the inline style on a preview page's root element
//! that caps and centres its content, the document that carries it in its `<html>` tag, and the
//! script that sets it live.
//!
//! Pure: no display, no Neovim and no compositor. The content box is laid out from the style by
//! [`content_box`], a model of CSS block layout for the root element of a `border-box` page in a
//! viewport of the pane's width: the viewport's width capped by any `max-width`, centred by two
//! `auto` side margins, then inset by the padding. Every expected rect is
//! `editor_inset::grid_rect`'s, so the tests hold the page's content on the grid an editor pane
//! of that size paints. Whether WebKitGTK lays the page out so is the margins smoke's question,
//! in `smoke_headless/preview_margins.rs`.
//!
//! | ID | Test | Kind |
//! | --- | --- | --- |
//! | G1 | [`capped_content_box_is_the_editors_capped_grid_rect`] | layout |
//! | G2 | [`full_width_style_pads_and_caps_nothing`] | layout |
//! | G3 | [`styled_document_puts_the_style_on_the_root_tag_alone`] | document |
//! | G3 | [`styled_document_leaves_a_document_it_cannot_style`] | document |
//! | G3 | [`apply_root_style_js_sets_the_roots_style_to_exactly_the_style`] | script |

use std::collections::HashMap;

use wolf_core::markdown::{render_html, PreviewTheme, WidthMode};
use wolf_gnome::editor_inset::{column_cap, grid_rect, GridRect, EDITOR_PADDING_PX, MAX_GRID_COLS};
use wolf_gnome::preview_width::{apply_root_style_js, root_style, styled_document};

/// A cell width that is no whole number of pixels, and whose cap leaves an odd margin to centre,
/// so the floor in `grid_rect` matters.
const CELL_W: f64 = 8.5;

/// The statusbar strip's height. The web view starts below it.
const STRIP_H: f64 = 33.0;

/// The pane height.
const PANE_H: f64 = 700.0;

/// A pane wider than the capped content: its centring offset is a half pixel.
const WIDE_W: f64 = 1000.0;

/// A pane narrower than the capped content.
const NARROW_W: f64 = 600.0;

/// The style's declarations, by name. Panics on a malformed one.
fn declarations(style: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in style.split(';') {
        let (name, value) = part.split_once(':').unwrap_or_else(|| {
            panic!(
                "{style:?} holds {part:?}, which is no name:value declaration: the format is \
                 name:value joined by ;, with no spaces and no trailing ;"
            )
        });
        assert!(
            !name.is_empty() && !value.is_empty() && !name.contains(' ') && !value.contains(' '),
            "{style:?}: {part:?} is no name:value declaration with no spaces"
        );
        let previous = out.insert(name.to_owned(), value.to_owned());
        assert!(previous.is_none(), "{style:?} declares {name} twice");
    }
    out
}

/// A `px` length. Panics when `value` is none.
fn px(value: Option<&String>, name: &str) -> f64 {
    value
        .and_then(|v| v.strip_suffix("px"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or_else(|| panic!("{name} is {value:?}, not a decimal followed by px"))
}

/// The rect the page's content box lays out at under `style`, in a viewport `viewport_w` wide
/// whose top is `top`, in the pane's coordinates.
fn content_box(style: &str, viewport_w: f64, top: f64, viewport_h: f64) -> GridRect {
    let decls = declarations(style);
    assert_eq!(
        decls.get("box-sizing").map(String::as_str),
        Some("border-box"),
        "{style:?}: the root's box must be border-box, or a max-width caps the content alone and \
         the padding widens it past the grid"
    );
    let side = |name: &str| px(decls.get(name).or_else(|| decls.get("padding")), name);
    let (pt, pr, pb, pl) = (
        side("padding-top"),
        side("padding-right"),
        side("padding-bottom"),
        side("padding-left"),
    );
    let mut width = viewport_w;
    if decls.contains_key("width") {
        width = px(decls.get("width"), "width");
    }
    if decls.contains_key("max-width") {
        width = width.min(px(decls.get("max-width"), "max-width"));
    }
    let auto = |name: &str| decls.get(name).map(String::as_str) == Some("auto");
    let margin_left = if auto("margin-left") && auto("margin-right") {
        ((viewport_w - width) / 2.0).max(0.0)
    } else if decls.contains_key("margin-left") && !auto("margin-left") {
        px(decls.get("margin-left"), "margin-left")
    } else {
        0.0
    };
    GridRect {
        x: margin_left + pl,
        y: top + pt,
        w: width - pl - pr,
        h: viewport_h - pt - pb,
    }
}

/// Asserts the page's content box `css` is the editor's grid rect `grid`: the width, the top and
/// the height exactly, and the left edge at most one pixel right of the grid's, which
/// `grid_rect` floors and the page does not.
fn assert_on_grid(css: GridRect, grid: GridRect, case: &str) {
    let eps = 1e-9;
    assert!(
        (css.w - grid.w).abs() < eps,
        "{case}: the content is {} px wide where the editor's grid is {} px. Wider means the \
         max-width leaves out the padding; the viewport's width or more means a fixed width \
         that overflows a narrow pane",
        css.w,
        grid.w
    );
    let dx = css.x - grid.x;
    assert!(
        (-eps..1.0).contains(&dx),
        "{case}: the content starts at x {} where the editor's grid starts at {}. Only the floor \
         of a half-pixel centring offset may separate them, by less than one pixel; more means \
         the content is not centred or the side padding is wrong",
        css.x,
        grid.x
    );
    assert!(
        (css.y - grid.y).abs() < eps && (css.h - grid.h).abs() < eps,
        "{case}: the content's rows {}..{} must be the grid's {}..{}, the padding below the strip \
         and above the pane's bottom",
        css.y,
        css.y + css.h,
        grid.y,
        grid.y + grid.h
    );
}

/// G1.
#[test]
fn capped_content_box_is_the_editors_capped_grid_rect() {
    let capped = f64::from(MAX_GRID_COLS) * CELL_W;
    assert!(
        WIDE_W - 2.0 * EDITOR_PADDING_PX > capped && NARROW_W - 2.0 * EDITOR_PADDING_PX < capped,
        "premise: the wide pane is wider than the {capped}-px cap and the narrow one narrower"
    );
    assert!(
        ((WIDE_W - 2.0 * EDITOR_PADDING_PX - capped) / 2.0).fract() != 0.0,
        "premise: the wide pane's centring offset is fractional, so the floor is exercised"
    );
    let style = root_style(column_cap(false, CELL_W), EDITOR_PADDING_PX);
    for (name, pane_w) in [("a wide pane", WIDE_W), ("a narrow pane", NARROW_W)] {
        let css = content_box(&style, pane_w, STRIP_H, PANE_H - STRIP_H);
        let grid = grid_rect(
            pane_w,
            PANE_H,
            STRIP_H,
            EDITOR_PADDING_PX,
            column_cap(false, CELL_W),
        );
        assert_on_grid(css, grid, &format!("{name}, style {style:?}"));
    }
}

/// G2.
#[test]
fn full_width_style_pads_and_caps_nothing() {
    let style = root_style(column_cap(true, CELL_W), EDITOR_PADDING_PX);
    let decls = declarations(&style);
    assert!(
        !decls.contains_key("max-width") && !decls.contains_key("width"),
        "a full-width buffer's style must cap nothing, or the full-width flag leaves the page \
         capped: {style:?}"
    );
    for (name, pane_w) in [("a wide pane", WIDE_W), ("a narrow pane", NARROW_W)] {
        let css = content_box(&style, pane_w, STRIP_H, PANE_H - STRIP_H);
        let grid = grid_rect(
            pane_w,
            PANE_H,
            STRIP_H,
            EDITOR_PADDING_PX,
            column_cap(true, CELL_W),
        );
        assert_on_grid(css, grid, &format!("{name}, full width, style {style:?}"));
    }
}

/// The in-app preview's real document.
fn document() -> String {
    render_html(
        "# Probe\n\nOne paragraph.\n",
        PreviewTheme::Default,
        WidthMode::Full,
    )
}

/// G3.
#[test]
fn styled_document_puts_the_style_on_the_root_tag_alone() {
    let html = document();
    let head = html
        .find("<head>")
        .expect("premise: render_html's document has a <head>");
    assert!(
        html[..head].contains("<html"),
        "premise: render_html's <html tag precedes its <head>"
    );
    let style = root_style(column_cap(false, CELL_W), EDITOR_PADDING_PX);
    let styled = styled_document(&html, &style);
    let expected = html.replacen("<html", &format!("<html style=\"{style}\""), 1);
    assert!(
        styled == expected,
        "the style must be inserted directly after <html and nowhere else. The raw document back \
         means it was never inserted, and a style on <body> overrides base.css's body box. The \
         styled document starts {:?}",
        &styled[..styled.len().min(120)]
    );
    let styled_head = styled
        .find("<head>")
        .expect("the styled document keeps its <head>");
    assert!(
        styled[styled_head..] == html[head..],
        "everything from <head> on must be byte-identical to the raw document, or the body swap \
         and its head offset read a different document"
    );
}

/// G3, the two cases that leave the document alone.
#[test]
fn styled_document_leaves_a_document_it_cannot_style() {
    let html = document();
    assert!(
        styled_document(&html, "") == html,
        "an empty style must leave the document as it is, not add an empty attribute"
    );
    let headless = "<head><style>x</style></head><html><body>b</body></html>";
    assert_eq!(
        styled_document(headless, "padding:1px"),
        headless,
        "with no <html before <head> the document must be left as it is"
    );
}

/// The string literal `js` passes `setAttribute("style", …)` on the root element, decoded.
fn root_style_argument(js: &str) -> String {
    let call = "document.documentElement.setAttribute(";
    let start = js
        .find(call)
        .unwrap_or_else(|| panic!("{js:?} must call {call}"));
    let rest = js[start + call.len()..].trim_start();
    let rest = rest
        .strip_prefix("\"style\"")
        .unwrap_or_else(|| panic!("{js:?} must set the \"style\" attribute"))
        .trim_start();
    let rest = rest
        .strip_prefix(',')
        .unwrap_or_else(|| panic!("{js:?}: no second argument"))
        .trim_start();
    let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<String>();
    match stream.next() {
        Some(Ok(value)) => value,
        other => panic!("{js:?}: the value is no JSON string literal: {other:?}"),
    }
}

/// G3, the live script.
#[test]
fn apply_root_style_js_sets_the_roots_style_to_exactly_the_style() {
    let real = root_style(column_cap(false, CELL_W), EDITOR_PADDING_PX);
    for style in [real.as_str(), "a\"b\\c</script>\n"] {
        let js = apply_root_style_js(style);
        assert!(
            !js.contains("document.body"),
            "the script must style the root element, never the body, whose box base.css owns: \
             {js:?}"
        );
        assert_eq!(
            root_style_argument(&js),
            style,
            "the literal in {js:?} must decode to exactly the style; anything else is raw \
             interpolation or a different value"
        );
    }
}
