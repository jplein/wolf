//! The window's SIDEBAR container for `wolf-gnome`: a `gtk::Stack` of two pages, Files
//! ([`crate::tree_view`]) and Search ([`crate::search_panel`]). The page is chosen from the
//! header bar's toggle group ([`crate::sidebar_toggles`]), whose state the relay derives.
//!
//! Both pages stay built for the window's lifetime, so switching keeps the tree's
//! expansion and the panel's query, rows and running `rg`. Only the CURRENT page of
//! a SHOWN sidebar is live: [`panel_visibility`] decides, and every change — a page
//! switch by [`SidebarHandle::show_page`], and the sidebar being shown or hidden — is
//! relayed into each page's refresh gate, so the page nobody can see rebuilds nothing
//! and catches up once when it returns. The same relay sets the toggle group.
//!
//! Switching to Search focuses the panel's field. The selected tab records the page with the
//! panel's fields, and selecting a tab shows the page it recorded.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;

use crate::search_panel::SearchPanelHandle;
use crate::sidebar_toggles::SidebarToggles;
use crate::tree_view::TreeHandle;

/// The stack's name for the Files page.
const FILES_PAGE: &str = "files";

/// The stack's name for the Search page.
const SEARCH_PAGE: &str = "search";

/// One of the sidebar's two pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPage {
    /// The file tree.
    Files,
    /// The project-search panel.
    Search,
}

/// Which of the two pages is live, that is, may refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelVisibility {
    /// Whether the Files page is live.
    pub files: bool,
    /// Whether the Search page is live.
    pub search: bool,
}

/// Which pages are live: a page is live only when the sidebar is shown AND it is
/// the current page.
#[must_use]
pub fn panel_visibility(sidebar_shown: bool, page: SidebarPage) -> PanelVisibility {
    PanelVisibility {
        files: sidebar_shown && page == SidebarPage::Files,
        search: sidebar_shown && page == SidebarPage::Search,
    }
}

/// The sidebar's shared state. All access happens on the single GTK main thread.
struct SidebarInner {
    /// The two pages, and the sidebar's root widget.
    stack: gtk::Stack,
    /// The header bar's toggle group, which shows this sidebar's state.
    toggles: SidebarToggles,
    /// The Files page's handle.
    tree: TreeHandle,
    /// The Search page's handle.
    search: SearchPanelHandle,
    /// Whether the shell last relayed the sidebar as shown.
    shown: Cell<bool>,
}

impl SidebarInner {
    /// The current page, read off the stack.
    fn page(&self) -> SidebarPage {
        if self.stack.visible_child_name().as_deref() == Some(SEARCH_PAGE) {
            SidebarPage::Search
        } else {
            SidebarPage::Files
        }
    }

    /// Relays [`panel_visibility`] into both pages' refresh gates, and the shown flag and
    /// page into the toggle group. The one site that sets the group.
    fn relay(&self) {
        let live = panel_visibility(self.shown.get(), self.page());
        self.tree.set_sidebar_visible(live.files);
        self.search.set_sidebar_visible(live.search);
        self.toggles.show_state(self.shown.get(), self.page());
    }
}

/// A handle to the built sidebar, used by the window to place it and its toggle group, and
/// by the shell to switch its page, relay its visibility and take the group's clicks.
#[derive(Clone)]
pub struct SidebarHandle {
    inner: Rc<SidebarInner>,
}

impl SidebarHandle {
    /// The sidebar's root widget, the `gtk::Paned`'s start child.
    #[must_use]
    pub fn widget(&self) -> gtk::Widget {
        self.inner.stack.clone().upcast()
    }

    /// The header bar's toggle group, whose state this sidebar's relay sets.
    #[must_use]
    pub fn toggles(&self) -> SidebarToggles {
        self.inner.toggles.clone()
    }

    /// Whether the shell last relayed the sidebar as shown: the flag the toggle group shows.
    #[must_use]
    pub fn is_shown(&self) -> bool {
        self.inner.shown.get()
    }

    /// The current page.
    #[must_use]
    pub fn page(&self) -> SidebarPage {
        self.inner.page()
    }

    /// Switches to `page`. Switching to Search also focuses the panel's field.
    pub fn show_page(&self, page: SidebarPage) {
        let name = match page {
            SidebarPage::Files => FILES_PAGE,
            SidebarPage::Search => SEARCH_PAGE,
        };
        self.inner.stack.set_visible_child_name(name);
    }

    /// Records whether the sidebar is shown and relays it into both pages.
    ///
    /// Called from `shell::AppShell::apply_sidebar_visibility`, the ONE site that
    /// shows or hides the sidebar.
    pub fn set_sidebar_visible(&self, shown: bool) {
        self.inner.shown.set(shown);
        self.inner.relay();
    }

    /// Whether keyboard focus is inside the sidebar, on either page.
    /// `false` while the sidebar is not rooted.
    #[must_use]
    pub fn focus_is_inside(&self) -> bool {
        let Some(root) = self.inner.stack.root() else {
            return false;
        };
        let Some(focus) = root.focus() else {
            return false;
        };
        focus.is_ancestor(&self.inner.stack)
            || &focus == self.inner.stack.upcast_ref::<gtk::Widget>()
    }
}

/// Builds the sidebar over the two already-built pages. It opens on Files, and
/// both pages stay dormant until the shell relays that the sidebar is shown.
#[must_use]
pub fn build(tree: &TreeHandle, search: &SearchPanelHandle) -> SidebarHandle {
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::None)
        .vexpand(true)
        .build();
    stack.add_named(&tree.widget(), Some(FILES_PAGE));
    stack.add_named(&search.widget(), Some(SEARCH_PAGE));
    stack.set_visible_child_name(FILES_PAGE);

    let inner = Rc::new(SidebarInner {
        stack: stack.clone(),
        toggles: SidebarToggles::build(),
        tree: tree.clone(),
        search: search.clone(),
        shown: Cell::new(false),
    });

    // `show_page`, the only writer of the page, lands here; a `Weak`, so the stack does not keep
    // the sidebar alive.
    let weak = Rc::downgrade(&inner);
    stack.connect_visible_child_name_notify(move |_stack| {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        inner.relay();
        if inner.page() == SidebarPage::Search {
            inner.search.focus_entry();
        }
    });

    SidebarHandle { inner }
}
