//! The header bar's sidebar control: an `adw::ToggleGroup` of a Files toggle and a Search
//! toggle, which shows the sidebar's page, switches it, and hides the sidebar.
//!
//! [`active_toggle`], [`toggle_click`] and [`toggle_at`] are pure; [`SidebarToggles`] is the
//! GTK glue, as [`crate::appearance`] separates decision from applier. The group's state is
//! set only by [`crate::sidebar`]'s relay, from [`active_toggle`], so every path that shows,
//! hides or switches the sidebar moves it.
//!
//! ## The group's own activation is bypassed
//!
//! `adw::ToggleGroup` emits nothing for a click on the toggle that is already active, and
//! that click must hide the sidebar. So a capture-phase `gtk::GestureClick` on the group
//! claims every primary press before the toggles see it, and on release maps the pointer's
//! half of the homogeneous group to a page with [`toggle_at`]. The claim costs the toggles
//! their pressed look.
//!
//! Nothing connects to the group's `active` or `active-name` notify, so the relay's
//! [`SidebarToggles::show_state`] cannot re-enter the click path: the gesture sees only the
//! pointer. The group never takes focus, so Space and Enter stay with Neovim, and the
//! keyboard reaches the sidebar through its chords.

use adw::prelude::*;

use crate::sidebar::SidebarPage;

/// The group's name for the Files toggle.
const FILES_TOGGLE: &str = "files";

/// The group's name for the Search toggle.
const SEARCH_TOGGLE: &str = "search";

/// What a click on one of the toggles does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleClick {
    /// Shows the sidebar on the page.
    Show(SidebarPage),
    /// Hides the sidebar.
    Hide,
}

/// The toggle that shows as active: the sidebar's page while it is shown, none while it is
/// hidden.
#[must_use]
pub fn active_toggle(shown: bool, page: SidebarPage) -> Option<SidebarPage> {
    shown.then_some(page)
}

/// What a click on `clicked` does while the sidebar is `shown` on `page`: the active toggle
/// hides the sidebar, and any other click shows it on the clicked page.
#[must_use]
pub fn toggle_click(shown: bool, page: SidebarPage, clicked: SidebarPage) -> ToggleClick {
    if shown && page == clicked {
        ToggleClick::Hide
    } else {
        ToggleClick::Show(clicked)
    }
}

/// The toggle under a group-local `x` in a group `width` wide. Files is the start half,
/// which is the right half when `rtl`; this holds because the group is homogeneous.
#[must_use]
pub fn toggle_at(x: f64, width: f64, rtl: bool) -> SidebarPage {
    let start_half = if rtl {
        x >= width / 2.0
    } else {
        x < width / 2.0
    };
    if start_half {
        SidebarPage::Files
    } else {
        SidebarPage::Search
    }
}

/// The toggle group's name for `page`.
fn toggle_name(page: SidebarPage) -> &'static str {
    match page {
        SidebarPage::Files => FILES_TOGGLE,
        SidebarPage::Search => SEARCH_TOGGLE,
    }
}

/// A handle to the built toggle group, owned by [`crate::sidebar`] and packed into the header
/// bar by [`crate::window`].
#[derive(Clone)]
pub struct SidebarToggles {
    group: adw::ToggleGroup,
}

impl SidebarToggles {
    /// Builds the group with no toggle active; the sidebar's relay sets it.
    #[must_use]
    pub fn build() -> SidebarToggles {
        let group = adw::ToggleGroup::builder()
            .homogeneous(true)
            .can_focus(false)
            .build();
        group.add(
            adw::Toggle::builder()
                .name(FILES_TOGGLE)
                .icon_name("folder-symbolic")
                .tooltip("Files")
                .build(),
        );
        group.add(
            adw::Toggle::builder()
                .name(SEARCH_TOGGLE)
                .icon_name("system-search-symbolic")
                .tooltip("Search (Ctrl+Shift+F)")
                .build(),
        );
        group.set_active_name(None);
        SidebarToggles { group }
    }

    /// The group, for the header bar's `pack_start`.
    #[must_use]
    pub fn widget(&self) -> gtk::Widget {
        self.group.clone().upcast()
    }

    /// Shows the sidebar's state: the active toggle is [`active_toggle`]'s answer.
    pub fn show_state(&self, shown: bool, page: SidebarPage) {
        self.group
            .set_active_name(active_toggle(shown, page).map(toggle_name));
    }

    /// Dims the group and refuses its clicks, as in the empty state.
    pub fn set_sensitive(&self, sensitive: bool) {
        self.group.set_sensitive(sensitive);
    }

    /// Calls `f` with the page of the toggle a primary click lands on. The group's own
    /// activation never runs; see the module docs.
    pub fn connect_click(&self, f: impl Fn(SidebarPage) + 'static) {
        let gesture = gtk::GestureClick::builder()
            .button(gtk::gdk::BUTTON_PRIMARY)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        gesture.connect_pressed(|gesture, _n_press, _x, _y| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        gesture.connect_released(move |gesture, _n_press, x, y| {
            let Some(widget) = gesture.widget() else {
                return;
            };
            if !widget.contains(x, y) {
                return;
            }
            let rtl = widget.direction() == gtk::TextDirection::Rtl;
            f(toggle_at(x, f64::from(widget.width()), rtl));
        });
        self.group.add_controller(gesture);
    }
}
