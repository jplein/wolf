//! Where the window's tab bar sits: in a row of its own below the header bar, or inside the
//! header bar as its title widget, by `gtk-titlebar-style` and the tab view's page count.
//!
//! [`tab_bar_placement`] is the pure decision and [`TabBarSlot`] the GTK glue that applies it
//! at build and again as pages come and go. Why the header's height is load-bearing for the
//! pointer smokes is `notes/wolf-gnome/window-structure.md`'s.
//!
//! ## One tab is always the row
//!
//! [`TabBarPlacement::BelowHeader`] puts the bar in the window's content box between the header
//! bar and the split, where it autohides at one page, so a one-tab window's geometry is the
//! same under either style. `tabs` at one tab answers `BelowHeader` rather than "nowhere": an
//! autohidden bar still shows while a tab is being dragged, which is how a tab is dropped into
//! a one-tab window.
//!
//! ## The title widget, centred
//!
//! [`TabBarPlacement::InHeader`] makes the bar `adw::HeaderBar`'s expanding title widget, so
//! the window title is not drawn. Packed at the header's start it would get only its natural
//! width, since libadwaita aligns its start box to the start. GTK centres an expanding centre
//! child between the wider of the two side groups, so the bar starts after the start controls
//! and stops short of the menu by the groups' difference, which is header that still drags the
//! window; two tabs meet at the header's centre.
//!
//! ## The in-header stylesheet holds the header's height
//!
//! The bar's natural height is its 34 px tab box plus 6 px of padding above and below and 1 px
//! below its box, which as the title widget would make the header taller and move every pane
//! origin. [`IN_HEADER_CSS`] strips those paddings and the box's inner shade, under a
//! `headerbar` only, so the row bar keeps its height under either style. It sets geometry
//! alone: [`crate::titlebar_background`]'s per-window provider colours the bar in both places.
//!
//! ## The move is deferred and idempotent
//!
//! `notify::n-pages` fires inside libadwaita's own close and drop paths, and reparenting the
//! bar there would unroot a tab mid-gesture. So the notify queues an idle, which reads the page
//! count when it runs and moves the bar only when the answer differs from the applied one. The
//! count is the view's, not the tab registry's, because a tab dragged to or from another window
//! changes the view alone.
//!
//! ## Focus is sampled before the move
//!
//! Unparenting the toplevel's focus widget leaves the window with none, and window-level key
//! dispatch then dies. The move samples the focus first, and returns it to the selected tab's
//! command-line strip when it was inside the bar or there was none.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::glib;
use wolf_core::GtkTitlebarStyle;

use crate::tab_view::TabsHandle;

/// Where the tab bar sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabBarPlacement {
    /// In the window's content box between the header bar and the split, autohiding at one
    /// page.
    BelowHeader,
    /// The header bar's title widget, in place of the window title.
    InHeader,
}

/// Where the tab bar sits under `style` while the tab view holds `pages` pages.
#[must_use]
pub fn tab_bar_placement(style: GtkTitlebarStyle, pages: usize) -> TabBarPlacement {
    match style {
        GtkTitlebarStyle::Tabs if pages >= 2 => TabBarPlacement::InHeader,
        GtkTitlebarStyle::Tabs | GtkTitlebarStyle::Native => TabBarPlacement::BelowHeader,
    }
}

/// The static class [`TabBarSlot::install`] gives the tab bar, which [`IN_HEADER_CSS`] selects.
pub const TAB_BAR_CSS_CLASS: &str = "wolf-tab-bar";

/// The stylesheet that keeps the in-header bar at a tab's own height, so the header keeps its
/// one-tab height. Its rules match only under a `headerbar`, so the row bar is untouched.
pub const IN_HEADER_CSS: &str = "
headerbar tabbar.wolf-tab-bar tabbox {
  padding-top: 0;
  padding-bottom: 0;
}

headerbar tabbar.wolf-tab-bar > revealer > box.box {
  padding: 0;
  box-shadow: none;
}
";

thread_local! {
    /// Whether [`IN_HEADER_CSS`] has been added to the default display, for
    /// `crate::overlay_style`'s reason: one provider serves every window.
    static CSS_INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// Adds [`IN_HEADER_CSS`] to the default display on first call.
fn install_css() {
    CSS_INSTALLED.with(|installed| {
        if installed.get() {
            return;
        }
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let provider = gtk::CssProvider::new();
        provider.load_from_string(IN_HEADER_CSS);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        installed.set(true);
    });
}

/// One window's tab bar placement: the GTK glue over [`tab_bar_placement`].
///
/// Hold it for the window's lifetime. The `n-pages` handler holds a `Weak`, because a strong
/// reference from a handler the view owns would leak the window. Deliberately untested
/// headlessly, for [`crate::appearance::apply_at_launch`]'s reason: the
/// `smoke_headless/titlebar_tabs.rs` smokes exercise it.
pub struct TabBarSlot {
    /// Held, never read: the handler reaches it through a `Weak`.
    _inner: Rc<SlotInner>,
}

/// The widgets a move touches, and the placement last applied.
struct SlotInner {
    style: GtkTitlebarStyle,
    header: adw::HeaderBar,
    content: gtk::Box,
    bar: gtk::Widget,
    tabs: TabsHandle,
    applied: Cell<Option<TabBarPlacement>>,
}

impl TabBarSlot {
    /// Gives `tabs`' bar its class and `hexpand`, installs [`IN_HEADER_CSS`] once, places the
    /// bar for the view's page count now, and re-places it after every page-count change.
    ///
    /// `header` must already be a child of `content`: the row placement inserts the bar right
    /// after it.
    #[must_use]
    pub fn install(
        style: GtkTitlebarStyle,
        header: &adw::HeaderBar,
        content: &gtk::Box,
        tabs: &TabsHandle,
    ) -> Self {
        let bar = tabs.bar_widget();
        bar.add_css_class(TAB_BAR_CSS_CLASS);
        bar.set_hexpand(true);
        install_css();
        let inner = Rc::new(SlotInner {
            style,
            header: header.clone(),
            content: content.clone(),
            bar,
            tabs: tabs.clone(),
            applied: Cell::new(None),
        });
        inner.apply(tab_bar_placement(style, tabs.n_pages()));
        let weak = Rc::downgrade(&inner);
        tabs.connect_n_pages(move || schedule(weak.clone()));
        Self { _inner: inner }
    }
}

/// Queues one re-placement for the main loop, outside the signal that asked for it.
fn schedule(weak: Weak<SlotInner>) {
    glib::idle_add_local_once(move || {
        let Some(slot) = weak.upgrade() else {
            return;
        };
        // No root means the window has closed.
        if slot.bar.root().is_none() {
            return;
        }
        slot.apply(tab_bar_placement(slot.style, slot.tabs.n_pages()));
    });
}

impl SlotInner {
    /// Moves the bar to `wanted` unless it is there already.
    fn apply(&self, wanted: TabBarPlacement) {
        let applied = self.applied.get();
        if applied == Some(wanted) {
            return;
        }
        // Sampled before the bar is unparented, after which a sample stops reporting it.
        let refocus = self.bar.root().map(|root| {
            root.focus()
                .is_none_or(|focus| focus == self.bar || focus.is_ancestor(&self.bar))
        });
        match applied {
            Some(TabBarPlacement::BelowHeader) => self.content.remove(&self.bar),
            Some(TabBarPlacement::InHeader) => self.header.set_title_widget(None::<&gtk::Widget>),
            None => {}
        }
        match wanted {
            TabBarPlacement::InHeader => self.header.set_title_widget(Some(&self.bar)),
            TabBarPlacement::BelowHeader => {
                self.content
                    .insert_child_after(&self.bar, Some(&self.header));
            }
        }
        self.applied.set(Some(wanted));
        if refocus == Some(true) {
            if let Some(tab) = self.tabs.selected() {
                tab.focus_area().grab_focus();
            }
        }
    }
}
