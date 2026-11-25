//! The inline style on a preview page's root element that caps and centres its content, the
//! document that carries it in its `<html>` tag, and the script that sets it live.
//!
//! GTK-free, so the whole rulebook runs in the fast unit suite. The widget work lives in
//! [`crate::pane_view`], whose web view spans its pane below the statusbar strip: the page's
//! scrollbar then sits at the pane's edge, and its margins scroll and click as the page. The
//! column cap is the page's, a `max-width` with auto side margins, never the view's frame.
//!
//! ## The root, never the body
//!
//! The style goes on `<html>` because the preview stylesheet (`base.css` in wolf-core) owns the
//! body's box: its zero padding is what the first-block margin rules and the baseline grid rest
//! on. A style on `<body>` would override both.
//!
//! ## The content can sit up to a pixel right of an editor's grid
//!
//! [`crate::editor_inset::grid_rect`] floors its centring offset, and the page centres at the
//! exact, possibly fractional, offset. So a capped page's content starts up to 1 px right of the
//! column a capped editor of the same pane would start at.

use crate::editor_inset::ColumnCap;

/// The inline style of a preview page's root element: `name:value` declarations joined by `;`,
/// with no spaces and no trailing `;`.
///
/// It always pads the page by `padding` on every side, inside a `border-box`. When `cap` caps
/// anything, which [`crate::editor_inset::grid_rect`] decides the same way (`Some(n)` with
/// `n > 0` and a finite positive cell width), it adds a `max-width` of the capped columns plus both
/// side paddings, centred by auto side margins. The content box then lands where the pane's
/// capped grid rect does, give or take the floor the module doc describes.
#[must_use]
pub fn root_style(cap: ColumnCap, padding: f64) -> String {
    let mut style = format!("box-sizing:border-box;padding:{padding}px");
    if let Some(max_cols) = cap.max_cols {
        if max_cols > 0 && cap.cell_w.is_finite() && cap.cell_w > 0.0 {
            let max_width = f64::from(max_cols) * cap.cell_w + 2.0 * padding;
            style.push_str(&format!(
                ";max-width:{max_width}px;margin-left:auto;margin-right:auto"
            ));
        }
    }
    style
}

/// `html` with ` style="<root_style>"` inserted directly after its first `<html`, which must
/// precede `<head>`; `&` and `"` are escaped.
///
/// Everything from `<head>` on is untouched, so the head stays byte-identical across renders
/// and its offset is the raw document's plus the attribute. `html` comes back unchanged when
/// `root_style` is empty or no `<html` precedes `<head>`.
#[must_use]
pub fn styled_document(html: &str, root_style: &str) -> String {
    let Some(head) = html.find("<head>") else {
        return html.to_owned();
    };
    if root_style.is_empty() {
        return html.to_owned();
    }
    let prologue = &html[..head];
    let mut from = 0;
    while let Some(found) = prologue[from..].find("<html") {
        let end = from + found + "<html".len();
        if matches!(
            prologue.as_bytes().get(end),
            Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
        ) {
            let value = root_style.replace('&', "&amp;").replace('"', "&quot;");
            return format!("{} style=\"{value}\"{}", &html[..end], &html[end..]);
        }
        from = end;
    }
    html.to_owned()
}

/// A script that sets the page's root element's `style` attribute to exactly `root_style`, the
/// value carried as a JSON string literal rather than interpolated raw.
#[must_use]
pub fn apply_root_style_js(root_style: &str) -> String {
    let literal = serde_json::Value::String(root_style.to_owned());
    format!("document.documentElement.setAttribute(\"style\",{literal});")
}
