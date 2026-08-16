//! The shared LOOK of the four transient overlays — the file picker, the command
//! palette, the buffer picker and the project search — as one CSS class installed
//! once per display.
//!
//! The target is a GNOME menu: the popover surface colour rather than the window
//! one, a menu's corner radius and drop shadow, and menu-sized rows. Every value
//! that has a libadwaita named colour uses it (`@popover_bg_color`,
//! `@popover_fg_color`) rather than a literal, so the overlays follow the
//! light/dark scheme [`crate::appearance`] resolves without this file knowing
//! which one is in effect.
//!
//! **This shares a stylesheet, not a widget.** [`crate::picker`]'s module docs
//! record why no common overlay TYPE is extracted; that judgement is unaffected
//! here, because a CSS class costs each call site one line and removes the four
//! copies of the appearance that a reader would otherwise have to keep in sync by
//! eye.
//!
//! The list, the viewport that scrolls it and their scroller are all forced
//! transparent because GTK gives a bare `list` the VIEW background, which would
//! punch a white (or near-black) panel through the popover surface the box
//! paints. The viewport is named because it is a node of that same chain: it
//! paints nothing under the current theme, so naming it costs no pixel and is
//! what keeps a theme that gives it a background from reintroducing the panel
//! one level down.

use std::cell::Cell;

use gtk::prelude::*;

/// The CSS class carried by an overlay's outermost box. Applied by [`apply`],
/// defined by [`OVERLAY_CSS_DATA`].
const OVERLAY_CSS_CLASS: &str = "wolf-overlay";

/// The stylesheet defining [`OVERLAY_CSS_CLASS`], loaded into the default display
/// by [`apply`].
///
/// The row padding is what sets the line height between items, and it is the one
/// number here a reader is likely to want: it is deliberately larger than the
/// GTK default so the lists read as a menu rather than as a dense table.
const OVERLAY_CSS_DATA: &str = "
.wolf-overlay {
  background-color: @popover_bg_color;
  color: @popover_fg_color;
  border-radius: 12px;
  box-shadow: 0 1px 5px 1px rgba(0, 0, 0, 0.18);
  padding: 6px;
}

.wolf-overlay > entry,
.wolf-overlay > label {
  margin-bottom: 6px;
}

.wolf-overlay scrolledwindow,
.wolf-overlay viewport,
.wolf-overlay list {
  background-color: transparent;
}

.wolf-overlay list > row {
  padding: 8px 10px;
  border-radius: 6px;
}
";

thread_local! {
    /// Whether [`OVERLAY_CSS_DATA`] has already been added to the default display.
    ///
    /// Four overlays times one window each would otherwise add four identical
    /// providers, and a second window four more. Identical providers are inert —
    /// [`crate::tree_view`] relies on exactly that — but there is no reason to
    /// accumulate them, and everything in this crate runs on the one GTK main
    /// thread, so a thread-local flag is the whole guard.
    static CSS_INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// Gives `overlay_box` the shared overlay chrome, installing the stylesheet on
/// first call.
///
/// Call it INSTEAD OF `add_css_class("background")`: the class this applies sets
/// both the background and the foreground colour, and it must win over
/// `.background`'s window colours rather than fight them.
pub(crate) fn apply(overlay_box: &gtk::Box) {
    CSS_INSTALLED.with(|installed| {
        if installed.get() {
            return;
        }
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let provider = gtk::CssProvider::new();
        provider.load_from_string(OVERLAY_CSS_DATA);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        installed.set(true);
    });
    overlay_box.add_css_class(OVERLAY_CSS_CLASS);
}
