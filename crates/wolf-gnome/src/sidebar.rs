//! The window's SIDEBAR container for `wolf-gnome`: a `gtk::StackSwitcher` over a
//! `gtk::Stack` of two pages, Files ([`crate::tree_view`]) and Search
//! ([`crate::search_panel`]).
//!
//! Both pages stay built for the window's lifetime, so switching keeps the tree's
//! expansion and the panel's query, rows and running `rg`. Only the CURRENT page of
//! a SHOWN sidebar is live: [`panel_visibility`] decides, and every change — a page
//! switch, by the switcher or by [`SidebarHandle::show_page`], and the sidebar
//! being shown or hidden — is relayed into each page's refresh gate, so the page
//! nobody can see rebuilds nothing and catches up once when it returns.
//!
//! Switching to Search focuses the panel's field, as the switcher's search button
//! does on the other front-end. The page is not persisted.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;

use crate::search_panel::SearchPanelHandle;
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
    /// The switcher above the stack, as one vertical box.
    root: gtk::Box,
    /// The two pages.
    stack: gtk::Stack,
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

    /// Relays [`panel_visibility`] into both pages' refresh gates.
    fn relay(&self) {
        let live = panel_visibility(self.shown.get(), self.page());
        self.tree.set_sidebar_visible(live.files);
        self.search.set_sidebar_visible(live.search);
    }
}

/// A handle to the built sidebar, used by the window to place it and by the shell
/// to switch its page and relay its visibility.
#[derive(Clone)]
pub struct SidebarHandle {
    inner: Rc<SidebarInner>,
}

impl SidebarHandle {
    /// The sidebar's root widget, the `gtk::Paned`'s start child.
    #[must_use]
    pub fn widget(&self) -> gtk::Widget {
        self.inner.root.clone().upcast()
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

    /// Whether keyboard focus is inside the sidebar, either page or the switcher.
    /// `false` while the sidebar is not rooted.
    #[must_use]
    pub fn focus_is_inside(&self) -> bool {
        let Some(root) = self.inner.root.root() else {
            return false;
        };
        let Some(focus) = root.focus() else {
            return false;
        };
        focus.is_ancestor(&self.inner.root) || &focus == self.inner.root.upcast_ref::<gtk::Widget>()
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
    stack
        .add_titled(&tree.widget(), Some(FILES_PAGE), "Files")
        .set_icon_name("folder-symbolic");
    stack
        .add_titled(&search.widget(), Some(SEARCH_PAGE), "Search")
        .set_icon_name("system-search-symbolic");
    stack.set_visible_child_name(FILES_PAGE);

    let switcher = gtk::StackSwitcher::builder().stack(&stack).build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&switcher);
    root.append(&stack);

    let inner = Rc::new(SidebarInner {
        root,
        stack: stack.clone(),
        tree: tree.clone(),
        search: search.clone(),
        shown: Cell::new(false),
    });

    // Both switch paths, the switcher and `show_page`, land here; a `Weak`, so the stack does not
    // keep the sidebar alive.
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
