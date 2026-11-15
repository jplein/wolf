//! The window's header bar and tab bar background: which colour they take, the stylesheet
//! that carries it, and the per-window provider that applies it.
//!
//! The rule is [`titlebar_background`]: `WolfTitlebar`'s background when the colorscheme
//! sets it, else the `Normal` background the grid fills with, so the bars and the grid
//! match, else libadwaita's own colour while neither is known, before the first frame.
//! [`titlebar_css`] and [`stylesheet_update`] are pure as well; [`TitlebarStyle`] is the
//! GTK glue, which [`crate::appearance`]'s split between decision and applier shapes.
//!
//! ## The stylesheet sets the background alone
//!
//! The title, the header bar's icons and the tabs' text keep libadwaita's foreground for
//! the scheme [`crate::appearance`] resolves: `ColorScheme::ForceDark` and `ForceLight`
//! make libadwaita draw the forced scheme's `--headerbar-fg-color` whatever the desktop
//! says, and `Default` follows the desktop that `background` is pushed from. A `color`, a
//! `filter` or a scheme-dependent value here would override that, so the rule declares
//! `background-color` and nothing else.
//!
//! ## No `:backdrop` selector
//!
//! libadwaita swaps an inactive window's bars to `--headerbar-backdrop-color` with its own
//! `:backdrop` rules, and dims their contents with a filter. GTK takes each property from
//! the highest-priority provider with a matching rule before it compares specificity, so
//! one rule at `STYLE_PROVIDER_PRIORITY_APPLICATION` that matches in every state keeps the
//! themed background in backdrop too, while libadwaita's dimming of the contents stays.
//!
//! ## One provider and one class per window
//!
//! Each window may run a different colorscheme, so each has its own provider, scoped by a
//! class that only its header bar and tab bar carry. The class names the [`WindowId`],
//! which [`crate::coordinator::next_free_id`] can mint again once its window closes, so
//! [`TitlebarStyle::uninstall`] removes the provider at close.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use wolf_core::Rgb;

use crate::coordinator::WindowId;

/// The background the header bar and the tab bar take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitlebarBackground {
    /// A colour from the colorscheme, drawn in sRGB as the grid is.
    Colour(Rgb),
    /// libadwaita's own header bar colour, focused or backdrop.
    Libadwaita,
}

/// Picks the bars' background from `WolfTitlebar`'s background (`titlebar`) and `Normal`'s
/// (`normal`).
///
/// `WolfTitlebar` comes first because a colorscheme that defines it is asking for it.
/// `normal` is the resolved default background, before the grid's fallback colour: an
/// unknown `Normal` keeps libadwaita's colour rather than painting the fallback.
#[must_use]
pub fn titlebar_background(titlebar: Option<Rgb>, normal: Option<Rgb>) -> TitlebarBackground {
    match (titlebar, normal) {
        (Some(colour), _) | (None, Some(colour)) => TitlebarBackground::Colour(colour),
        (None, None) => TitlebarBackground::Libadwaita,
    }
}

/// The stylesheet that gives the header bar and the tab bar carrying `class` the
/// `background`, or `""` for [`TitlebarBackground::Libadwaita`].
///
/// The colour is written as lowercase `#rrggbb`. The two selectors are the nodes libadwaita
/// paints: `headerbar` itself, and the tab bar's `box.box`, the revealer's child that its
/// `tabbar .box` rule colours, since the `tabbar` node paints nothing.
#[must_use]
pub fn titlebar_css(class: &str, background: TitlebarBackground) -> String {
    let TitlebarBackground::Colour(Rgb { r, g, b }) = background else {
        return String::new();
    };
    format!(
        "headerbar.{class},\ntabbar.{class} > revealer > box.box {{\n  \
         background-color: #{r:02x}{g:02x}{b:02x};\n}}\n"
    )
}

/// The stylesheet to load when `wanted` replaces `applied`, or `None` when they are equal,
/// so a render wake that changes nothing does no CSS work.
#[must_use]
pub fn stylesheet_update(
    applied: TitlebarBackground,
    wanted: TitlebarBackground,
    class: &str,
) -> Option<String> {
    (applied != wanted).then(|| titlebar_css(class, wanted))
}

/// One window's titlebar provider: the GTK glue over [`stylesheet_update`].
///
/// Deliberately untested headlessly, for [`crate::appearance::apply_at_launch`]'s reason:
/// GTK objects assert an initialized main thread. The titlebar smokes exercise it.
#[derive(Clone)]
pub struct TitlebarStyle {
    provider: gtk::CssProvider,
    display: gtk::gdk::Display,
    class: Rc<str>,
    applied: Rc<Cell<TitlebarBackground>>,
}

impl TitlebarStyle {
    /// Gives `header` and `tab_bar` the window's class and adds an empty provider to their
    /// display at `STYLE_PROVIDER_PRIORITY_APPLICATION`.
    ///
    /// The class goes on the two widgets, never on the window: a descendant selector under
    /// the window would also match a header bar that a child widget brings.
    #[must_use]
    pub fn install(id: WindowId, header: &adw::HeaderBar, tab_bar: &gtk::Widget) -> Self {
        let class: Rc<str> = format!("wolf-titlebar-{id}").into();
        header.add_css_class(&class);
        tab_bar.add_css_class(&class);
        let provider = gtk::CssProvider::new();
        let display = header.display();
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        Self {
            provider,
            display,
            class,
            applied: Rc::new(Cell::new(TitlebarBackground::Libadwaita)),
        }
    }

    /// Applies `wanted`, replacing the provider's data only when it differs from what is
    /// applied.
    pub fn apply(&self, wanted: TitlebarBackground) {
        if let Some(css) = stylesheet_update(self.applied.get(), wanted, &self.class) {
            self.provider.load_from_string(&css);
            self.applied.set(wanted);
        }
    }

    /// Removes the provider from the display, so a later window given the same id does not
    /// inherit this one's colour.
    pub fn uninstall(&self) {
        gtk::style_context_remove_provider_for_display(&self.display, &self.provider);
    }
}
