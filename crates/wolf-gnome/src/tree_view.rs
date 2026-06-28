//! The GNOME file-tree sidebar UI for `wolf-gnome`.
//!
//! This is the GTK glue that turns the pure hierarchical projection in
//! [`crate::tree`] into a live `gtk::ListView` driven by a `gtk::TreeListModel`.
//! It mirrors [`crate::picker`]'s patterns: a long-lived `wolf_core::FileSearch`
//! started at build time, an `async-channel` + `glib::spawn_future_local` drain
//! that refreshes on each traversal batch, `Rc`/`RefCell` interior mutability on
//! the single GTK main thread, and a `Weak` upgrade in the drain task to avoid a
//! reference cycle.
//!
//! ## Model
//!
//! Each [`crate::tree::TreeNode`] is wrapped in a `glib::BoxedAnyObject` (no custom
//! GObject subclass). The root `gio::ListStore` holds the cwd's immediate children
//! (`children_of(entries, "")`). A `gtk::TreeListModel` (passthrough = false,
//! autoexpand = false) lazily materializes each directory's children via a
//! create-func that re-runs `children_of` against the latest shared entries; files
//! return `None` (no children). A `gtk::SingleSelection` wraps the tree model and a
//! `gtk::SignalListItemFactory` binds each row into a `gtk::TreeExpander` + label.
//!
//! ## Highlight + reveal
//!
//! The currently-edited file (from `Snapshot::current_file`, delivered on the
//! existing render wake) is highlighted via the [`CURRENT_FILE_CSS`] class and
//! revealed by expanding its ancestor directories then selecting its row. Because
//! the current file can arrive before the tree has streamed in, the requested
//! reveal is stashed in `pending_reveal` and retried on each refresh.
//!
//! ## v1 limitations
//!
//! Rebuilding the root store on each refresh collapses manual expansion (the reveal
//! re-applies afterward). Reveal expands the file's ancestor directories, selects its
//! row, and scrolls it into view via `ListView::scroll_to` (enabled by the gtk
//! `v4_12` feature).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::{gio, glib};

use wolf_core::{FileEntry, FileSearch, InputEvent, WakeFn, WolfCore};

use crate::tree::{self, TreeNode};

/// CSS class applied to the label of the row whose file is the current buffer.
const CURRENT_FILE_CSS: &str = "wolf-current-file";

/// The CSS that defines [`CURRENT_FILE_CSS`]. Loaded once into the default display.
const CURRENT_FILE_CSS_DATA: &str = ".wolf-current-file { font-weight: bold; }";

/// Capacity of the tree's wake channel. One slot is enough: it coalesces a burst of
/// traversal-batch wakes into a single queued refresh (a dropped "full" send just
/// means a refresh is already pending), mirroring the picker's wake channel.
const TREE_WAKE_CHANNEL_CAP: usize = 1;

/// The tree's shared, interior-mutable state. All access happens on the single GTK
/// main thread, so `RefCell` (not a `Mutex`) is sufficient.
struct TreeInner {
    /// The latest flat file list, shared with the `TreeListModel` create-func so
    /// lazily-materialized directory children reflect the most recent traversal.
    entries: Rc<RefCell<Vec<FileEntry>>>,
    /// The long-lived file search; dropping it cancels the background traversal.
    search: RefCell<Option<FileSearch>>,
    /// The root list store (cwd's immediate children), rebuilt on each refresh.
    root_store: gio::ListStore,
    /// The lazy hierarchical model over `root_store`.
    tree_model: gtk::TreeListModel,
    /// The selection model wrapping `tree_model` (drives row selection on reveal).
    selection: gtk::SingleSelection,
    /// The scrolled window holding the list view (the sidebar's root widget).
    scroller: gtk::ScrolledWindow,
    /// The list view; retained so `reveal` can scroll a revealed row into view.
    list_view: gtk::ListView,
    /// The live core, shared with the rest of the app.
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The editor drawing area, re-focused after a single-click open.
    area: gtk::DrawingArea,
    /// The wake closure handed to `start_file_search`; firing it queues a refresh.
    wake: WakeFn,
    /// A reveal target that could not yet be resolved (the file had not streamed in
    /// when the current-file change arrived); retried on each refresh.
    pending_reveal: RefCell<Option<PathBuf>>,
    /// The last current-file path acted on, so `on_render_wake` only reveals when
    /// the current buffer actually changes.
    last_current: RefCell<Option<PathBuf>>,
}

impl TreeInner {
    /// Reads the current `FileSearch` view, copies its entries into the shared list,
    /// rebuilds the root store from the cwd's immediate children, then retries any
    /// pending reveal. No-op if the search is gone.
    fn refresh(&self) {
        {
            let search = self.search.borrow();
            let Some(fs) = search.as_ref() else {
                return;
            };
            let view = fs.read();
            let mut entries = self.entries.borrow_mut();
            entries.clear();
            entries.extend(view.entries.iter().cloned());
        }

        // Rebuild the root store from the latest entries. (This collapses manual
        // expansion; the reveal below re-applies the current-file path.)
        let roots = tree::children_of(&self.entries.borrow(), "");
        self.root_store.remove_all();
        for node in roots {
            self.root_store.append(&glib::BoxedAnyObject::new(node));
        }

        let pending = self.pending_reveal.borrow().clone();
        if let Some(abs) = pending {
            self.reveal(&abs);
        }
    }

    /// Handles a render wake: if `Snapshot::current_file` changed since last time,
    /// records it, stashes it as the reveal target, and refreshes (rebuilding the
    /// store so highlight CSS re-applies, then revealing the new current file).
    fn on_render_wake(&self) {
        let current = {
            let core_ref = self.core.borrow();
            core_ref
                .as_ref()
                .and_then(|c| c.read_snapshot().current_file.clone())
        };
        if *self.last_current.borrow() == current {
            return;
        }
        *self.last_current.borrow_mut() = current.clone();
        *self.pending_reveal.borrow_mut() = current.clone();
        // Rebuild + reveal: fresh binds pick up the highlight for the new current
        // file (and drop it from the old one); reveal re-expands the path to it.
        self.refresh();
    }

    /// Reveals `abs` in the tree: expands its ancestor directories (top-down,
    /// re-scanning after each expansion since positions shift) and selects its row.
    /// Leaves `pending_reveal` set if the file has not streamed in yet (so the next
    /// refresh retries); clears it on success.
    fn reveal(&self, abs: &Path) {
        let rel = {
            let entries = self.entries.borrow();
            tree::rel_path_for(&entries, abs)
        };
        let Some(rel) = rel else {
            // Not resolvable yet; leave pending_reveal set for the next refresh.
            return;
        };

        for ancestor in tree::ancestors_to_reveal(&rel) {
            if let Some(pos) = self.find_row(&ancestor) {
                if let Some(row) = self.row_at(pos) {
                    row.set_expanded(true);
                }
            }
        }

        if let Some(pos) = self.find_row(&rel) {
            self.selection.set_selected(pos);
            // Scroll the (possibly deeply-nested, off-screen) revealed row into
            // view. Requires the gtk `v4_12` feature.
            self.list_view
                .scroll_to(pos, gtk::ListScrollFlags::NONE, None);
            *self.pending_reveal.borrow_mut() = None;
        }
    }

    /// Single-click activation at flattened position `pos`: open a file (then return
    /// focus to the editor) or toggle a directory's expansion.
    fn activate(&self, pos: u32) {
        let Some(row) = self.row_at(pos) else {
            return;
        };
        let Some(boxed) = row.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        // Compute the action in a scope so the node borrow is dropped before any
        // send_input (never hold a borrow across the core call).
        let open_path = {
            let node = boxed.borrow::<TreeNode>();
            match &*node {
                TreeNode::File { abs_path, .. } => Some(abs_path.clone()),
                TreeNode::Dir { .. } => None,
            }
        };
        match open_path {
            Some(path) => {
                if let Some(core) = self.core.borrow().as_ref() {
                    core.send_input(InputEvent::Open { path });
                }
                self.area.grab_focus();
            }
            None => {
                let expanded = row.is_expanded();
                row.set_expanded(!expanded);
            }
        }
    }

    /// Drops the file search, cancelling the background traversal.
    fn close(&self) {
        let _ = self.search.borrow_mut().take();
    }

    /// Returns the `TreeListRow` at flattened `pos` (items are rows since
    /// passthrough is false).
    fn row_at(&self, pos: u32) -> Option<gtk::TreeListRow> {
        self.tree_model.item(pos).and_downcast::<gtk::TreeListRow>()
    }

    /// Scans the materialized tree for the first row whose node `rel_path` equals
    /// `rel`, returning its flattened position.
    fn find_row(&self, rel: &str) -> Option<u32> {
        let n = self.tree_model.n_items();
        for pos in 0..n {
            let Some(row) = self.row_at(pos) else {
                continue;
            };
            let Some(boxed) = row.item().and_downcast::<glib::BoxedAnyObject>() else {
                continue;
            };
            let node = boxed.borrow::<TreeNode>();
            if tree::node_rel_path(&node) == rel {
                return Some(pos);
            }
        }
        None
    }
}

/// A handle to the built tree sidebar, used by the app to feed it render wakes,
/// place its widget, and close it on shutdown.
#[derive(Clone)]
pub struct TreeHandle {
    inner: Rc<TreeInner>,
}

impl TreeHandle {
    /// Returns the sidebar's root widget (the scrolled window) for placement in the
    /// app's `gtk::Paned`.
    #[must_use]
    pub fn widget(&self) -> gtk::Widget {
        self.inner.scroller.clone().upcast::<gtk::Widget>()
    }

    /// Reacts to a render wake: reveals + highlights the current file if it changed.
    pub fn on_render_wake(&self) {
        self.inner.on_render_wake();
    }

    /// Drops the file search, cancelling any in-flight traversal.
    pub fn close(&self) {
        self.inner.close();
    }
}

/// Ensures the [`CURRENT_FILE_CSS`] class is defined by loading its CSS once into
/// the default display.
fn install_css() {
    if let Some(display) = gdk::Display::default() {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CURRENT_FILE_CSS_DATA);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// Builds the `TreeListModel` over `root_store`, lazily materializing each
/// directory's children from `entries` via `children_of`.
fn build_tree_model(
    root_store: &gio::ListStore,
    entries: &Rc<RefCell<Vec<FileEntry>>>,
) -> gtk::TreeListModel {
    let entries = entries.clone();
    gtk::TreeListModel::new(root_store.clone(), false, false, move |obj| {
        let boxed = obj.downcast_ref::<glib::BoxedAnyObject>()?;
        let node = boxed.borrow::<TreeNode>();
        match &*node {
            TreeNode::Dir { rel_path, .. } => {
                let store = gio::ListStore::new::<glib::BoxedAnyObject>();
                for child in tree::children_of(&entries.borrow(), rel_path) {
                    store.append(&glib::BoxedAnyObject::new(child));
                }
                Some(store.upcast::<gio::ListModel>())
            }
            TreeNode::File { .. } => None,
        }
    })
}

/// Builds the list-item factory: a `TreeExpander` + `Label` per row, bound to the
/// row's node (name + current-file highlight) on each (re)bind. `core` is read at
/// bind time to decide the highlight.
fn build_factory(core: &Rc<RefCell<Option<WolfCore>>>) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_factory, list_item| {
        let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::builder().halign(gtk::Align::Start).build();
        let expander = gtk::TreeExpander::new();
        expander.set_child(Some(&label));
        list_item.set_child(Some(&expander));
    });

    let core = core.clone();
    factory.connect_bind(move |_factory, list_item| {
        let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(expander) = list_item.child().and_downcast::<gtk::TreeExpander>() else {
            return;
        };
        let Some(label) = expander.child().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(row) = list_item.item().and_downcast::<gtk::TreeListRow>() else {
            return;
        };
        expander.set_list_row(Some(&row));

        let Some(boxed) = row.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let node = boxed.borrow::<TreeNode>();
        let (name, is_current) = match &*node {
            TreeNode::Dir { name, .. } => (name.clone(), false),
            TreeNode::File { name, abs_path, .. } => {
                let core_ref = core.borrow();
                let current = core_ref.as_ref().is_some_and(|c| {
                    c.read_snapshot().current_file.as_deref() == Some(abs_path.as_path())
                });
                (name.clone(), current)
            }
        };
        label.set_text(&name);
        if is_current {
            label.add_css_class(CURRENT_FILE_CSS);
        } else {
            label.remove_css_class(CURRENT_FILE_CSS);
        }
    });

    factory
}

/// Builds the tree sidebar widgets, starts a long-lived `FileSearch`, wires the
/// streaming refresh + single-click activation, and returns a [`TreeHandle`]. The
/// sidebar's root widget is obtained via [`TreeHandle::widget`].
#[must_use]
pub fn build(core: &Rc<RefCell<Option<WolfCore>>>, area: &gtk::DrawingArea) -> TreeHandle {
    install_css();

    let entries: Rc<RefCell<Vec<FileEntry>>> = Rc::new(RefCell::new(Vec::new()));

    let root_store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let tree_model = build_tree_model(&root_store, &entries);
    let selection = gtk::SingleSelection::new(Some(tree_model.clone()));
    // Don't auto-highlight row 0 at startup; a real selection only happens when a
    // current-file reveal lands.
    selection.set_autoselect(false);
    let factory = build_factory(core);

    let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list_view.set_single_click_activate(true);

    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&list_view)
        .build();

    // Wake plumbing: traversal batches fire `wake` from a tokio worker (Send +
    // Sync); it only pushes onto the channel. A dropped "full" send is correct (a
    // refresh is already pending). DROP-SAFE: ignore try_send errors.
    let (wake_tx, wake_rx) = async_channel::bounded::<()>(TREE_WAKE_CHANNEL_CAP);
    let wake: WakeFn = Arc::new(move || {
        let _ = wake_tx.try_send(());
    });

    let inner = Rc::new(TreeInner {
        entries,
        search: RefCell::new(None),
        root_store,
        tree_model,
        selection,
        scroller,
        list_view: list_view.clone(),
        core: core.clone(),
        area: area.clone(),
        wake,
        pending_reveal: RefCell::new(None),
        last_current: RefCell::new(None),
    });

    // Start the long-lived file search feeding the tree.
    let search = {
        let core_ref = inner.core.borrow();
        core_ref
            .as_ref()
            .map(|c| c.start_file_search(inner.wake.clone()))
    };
    *inner.search.borrow_mut() = search;

    // Drain the wake channel on the GTK main thread, refreshing on each batch. A
    // Weak avoids a reference cycle (inner owns the wake sender; the task holds only
    // a weak ref so dropping the handle ends the task).
    {
        let inner_weak = Rc::downgrade(&inner);
        glib::spawn_future_local(async move {
            while wake_rx.recv().await.is_ok() {
                if let Some(inner) = inner_weak.upgrade() {
                    inner.refresh();
                }
            }
        });
    }

    // Single-click / activation opens a file or toggles a directory.
    {
        let inner_weak = Rc::downgrade(&inner);
        list_view.connect_activate(move |_list_view, pos| {
            if let Some(inner) = inner_weak.upgrade() {
                inner.activate(pos);
            }
        });
    }

    // Show the MRU seed immediately.
    inner.refresh();

    TreeHandle { inner }
}
