//! The GNOME file-tree sidebar UI for `wolf-gnome`.
//!
//! This is the GTK glue that turns the pure hierarchical projection in
//! [`crate::tree`] into a live `gtk::ListView` driven by a `gtk::TreeListModel`.
//! It mirrors [`crate::picker`]'s patterns: a long-lived `wolf_core::FileSearch`,
//! an `async-channel` + `glib::spawn_future_local` drain that refreshes on each
//! traversal batch, `Rc`/`RefCell` interior mutability on the single GTK main
//! thread, and a `Weak` upgrade in the drain task to avoid a reference cycle.
//!
//! ## When the search starts
//!
//! [`build`] takes a `start` flag: the search begins at build time for a normal
//! project launch, but NOT in the empty state (no project open — see
//! [`crate::workspace`]), where the sidebar is hidden and there is nothing to
//! walk. [`TreeHandle::reroot`] then starts (or restarts) it once a front-end
//! directory change — the Open Folder dialog or an Open Recent palette row — has
//! settled, i.e. once nvim has reported the new cwd that
//! `WolfCore::start_file_search` resolves its root from.
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

    /// Starts (or restarts) the long-lived file search feeding the tree. The
    /// traversal root is resolved by the core from its LIVE cwd, so this must only
    /// run once nvim has reported the cwd the tree is meant to show.
    fn start_search(&self) {
        let search = {
            let core_ref = self.core.borrow();
            core_ref
                .as_ref()
                .map(|core| core.start_file_search(self.wake.clone()))
        };
        *self.search.borrow_mut() = search;
    }

    /// Re-roots the sidebar at the core's CURRENT live cwd: cancels the existing
    /// traversal, clears every cached projection, then starts a fresh search and
    /// refreshes.
    ///
    /// Driven only from a front-end directory change — the Open Folder dialog or an
    /// Open Recent palette row — and only once
    /// [`crate::workspace::reroot_ready`] holds: `start_file_search` resolves its
    /// root from `live_cwd()`, which only updates after nvim reports `DirChanged`,
    /// so re-rooting earlier would re-root at the OLD cwd.
    fn reroot(&self) {
        // Drop the old search FIRST so its in-flight traversal (at the old root)
        // is cancelled before the new one starts.
        let _ = self.search.borrow_mut().take();
        self.entries.borrow_mut().clear();
        self.root_store.remove_all();
        *self.pending_reveal.borrow_mut() = None;
        // Forget the last current file so the next wake re-reveals it under the new
        // root rather than treating it as unchanged.
        *self.last_current.borrow_mut() = None;
        self.start_search();
        self.refresh();
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

    /// Re-roots the sidebar at the core's live cwd (used after a front-end
    /// directory change — the Open Folder dialog or an Open Recent palette row —
    /// once nvim has reported the new directory).
    pub fn reroot(&self) {
        self.inner.reroot();
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

/// Builds the tree sidebar widgets, wires the streaming refresh + single-click
/// activation, and returns a [`TreeHandle`]. The sidebar's root widget is obtained
/// via [`TreeHandle::widget`].
///
/// `start` decides whether the long-lived `FileSearch` is started NOW. It is
/// `false` in the empty state (no project open — see [`crate::workspace`]), where
/// the sidebar is hidden and there is no project tree to walk; the search then
/// starts on the first [`TreeHandle::reroot`] instead. The wake-drain task is
/// spawned either way and is a no-op while there is no search (`refresh` early
/// returns).
#[must_use]
pub fn build(
    core: &Rc<RefCell<Option<WolfCore>>>,
    area: &gtk::DrawingArea,
    start: bool,
) -> TreeHandle {
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

    // Start the long-lived file search feeding the tree — unless this is the empty
    // state, where there is no project to walk yet.
    if start {
        inner.start_search();
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    use wolf_core::tree::TreeIndexCache;

    // --- The fixtures -------------------------------------------------------
    //
    // Every entry below is written out by hand, and so is every expected node:
    // nothing in this module derives an expectation by calling the projection it
    // is asserting on.

    /// The directory every fixture entry lives under.
    ///
    /// Deliberately unrelated to [`ROOT_LABEL`]: a fixture path that carried the
    /// asserted label would let a projection keyed on the WRONG string still
    /// return the right children.
    const FIXTURE_ROOT: &str = "/wsroot";

    /// The label handed to `root_node`. It names no directory in any fixture, so
    /// `children_of(entries, "myproj")` is empty — which is what makes the
    /// sentinel assertion below able to fail.
    const ROOT_LABEL: &str = "myproj";

    /// One fixture entry. CONVENTION: the absolute path is
    /// `{FIXTURE_ROOT}/{display}`, and the MRU rank is irrelevant to the
    /// projection, so it is always `None`.
    fn fe(display: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(format!("{FIXTURE_ROOT}/{display}")),
            display: display.to_string(),
            mru_rank: None,
        }
    }

    /// A hand-built expected `Dir` node. BOTH fields are stated by the caller;
    /// neither is derived from the other.
    fn dir(name: &str, rel_path: &str) -> TreeNode {
        TreeNode::Dir {
            name: name.to_string(),
            rel_path: rel_path.to_string(),
        }
    }

    /// A hand-built expected `File` node, whose `abs_path` follows [`fe`]'s
    /// convention.
    fn file(name: &str, rel_path: &str) -> TreeNode {
        TreeNode::File {
            name: name.to_string(),
            rel_path: rel_path.to_string(),
            abs_path: PathBuf::from(format!("{FIXTURE_ROOT}/{rel_path}")),
        }
    }

    // =========================================================================
    // root_node
    // =========================================================================

    /// The sidebar's top level is ONE synthetic directory node named after the
    /// working directory, whose rel-path is the cwd sentinel.
    ///
    /// Two independent claims, so a failure says which broke:
    ///
    /// 1. It is a `Dir`. That arm is what makes `activate` unable to reach
    ///    `InputEvent::Open` for the root row — `activate` matches the node and
    ///    only its `File` arm sends an `Open` — so "clicking the root must not
    ///    open a file" is satisfied by construction rather than by a guard.
    /// 2. Its `rel_path` is `""`, the same sentinel macOS uses, so the existing
    ///    create-func resolves the root's children to the cwd's top level with no
    ///    special-casing.
    ///
    /// Revert that must redden it: giving the root node the label as its
    /// `rel_path` instead of the empty sentinel — claim 1 stays green and the
    /// children assertion fails, since no fixture directory is named `myproj`.
    #[test]
    fn root_node_uses_the_cwd_sentinel() {
        let entries = [fe("src/main.rs"), fe("docs/intro.md"), fe("readme.md")];

        let node = root_node(ROOT_LABEL);

        let TreeNode::Dir { name, rel_path } = &node else {
            panic!(
                "root_node must return a Dir, but returned {node:?}. A File root would be \
                 openable: `activate` sends InputEvent::Open for exactly the File arm, so the \
                 first click on the root row would ask nvim to edit the working DIRECTORY as a \
                 buffer."
            );
        };

        assert_eq!(
            name.as_str(),
            ROOT_LABEL,
            "the synthetic root row must draw the label it was handed ({ROOT_LABEL:?}), which is \
             the working directory's basename; the shell is the one place that knows where the \
             sidebar is rooted and passes it in"
        );

        assert_eq!(
            rel_path.as_str(),
            "",
            "the synthetic root's rel_path must be the EMPTY cwd sentinel, but was \
             {rel_path:?} — the create-func resolves a Dir's children by calling the projection \
             with its rel_path, and only the empty string names the cwd's own top level"
        );

        let want = vec![
            dir("docs", "docs"),
            dir("src", "src"),
            file("readme.md", "readme.md"),
        ];
        assert_eq!(
            tree::children_of(&entries, rel_path),
            want,
            "resolving the synthetic root's rel_path against the fixture must yield the cwd's top \
             level (dirs first, case-insensitively by name, then files): {want:?}. An empty \
             result means the root's rel_path names a directory that does not exist, so the root \
             row would draw with no children and nothing under it would ever be reachable."
        );
    }

    // =========================================================================
    // children_for
    // =========================================================================

    /// Every per-directory query at one snapshot version shares ONE built index.
    ///
    /// This is the discriminator for adopting `TreeIndexCache`: the free
    /// `children_of` rebuilds the whole index per call, so a create-func that
    /// still called it would answer every child list correctly while doing O(N)
    /// work per expanded directory. The two counts below are the only assertions
    /// that can tell those two implementations apart.
    ///
    /// Both expected counts are literals — 1 after four call sites, 2 after the
    /// fifth — never a count of the calls the test made.
    ///
    /// Revert that must redden it: replacing `children_for`'s body with
    /// `tree::children_of(entries, rel_path)`, the pre-memo form. Every child-list
    /// assertion stays green and `build_count()` stays 0, so both count
    /// assertions fail — which is exactly "the memo is not adopted".
    #[test]
    fn children_for_builds_the_index_once_per_version() {
        let entries = [
            fe("src/main.rs"),
            fe("src/lib.rs"),
            fe("docs/intro.md"),
            fe("readme.md"),
            fe("Cargo.toml"),
        ];

        let mut memo = TreeIndexCache::new();

        // --- Four call sites, all at version 1 -------------------------------
        //
        // Each result is bound to a local so the `&mut memo` borrow ends at the
        // end of its own statement — rule R1, which is what keeps a re-entrant
        // create-func from hitting a RefCell panic in production.

        let root = children_for(&mut memo, 1, &entries, "");
        assert_eq!(
            root,
            vec![
                dir("docs", "docs"),
                dir("src", "src"),
                file("Cargo.toml", "Cargo.toml"),
                file("readme.md", "readme.md"),
            ],
            "query 1 (the cwd's top level) must project dirs first, case-insensitively by name, \
             then files the same way"
        );

        let src_first = children_for(&mut memo, 1, &entries, "src");
        assert_eq!(
            src_first,
            vec![file("lib.rs", "src/lib.rs"), file("main.rs", "src/main.rs")],
            "query 2 (a directory expanded for the first time) must list its two leaf files"
        );

        let src_again = children_for(&mut memo, 1, &entries, "src");
        assert_eq!(
            src_again,
            vec![file("lib.rs", "src/lib.rs"), file("main.rs", "src/main.rs")],
            "query 3 repeats query 2 — GTK re-asks for a row's children when it binds the row and \
             again when it decides whether to draw an expander arrow — and must answer identically"
        );

        let docs = children_for(&mut memo, 1, &entries, "docs");
        assert_eq!(
            docs,
            vec![file("intro.md", "docs/intro.md")],
            "query 4 (a second expanded directory) must list its single leaf file"
        );

        assert_eq!(
            memo.build_count(),
            1,
            "four queries at snapshot version 1 must have built the index exactly ONCE, but the \
             count was {}. A count of 0 means children_for never consults the memo at all and is \
             re-running the free children_of's full rescan per query; a count above 1 means the \
             memo is rebuilding per query, which is the same O(N) storm with an extra allocation.",
            memo.build_count()
        );

        // --- The version advances: one more build, and the new file appears ---
        let entries_v2 = [
            fe("src/main.rs"),
            fe("src/lib.rs"),
            fe("docs/intro.md"),
            fe("readme.md"),
            fe("Cargo.toml"),
            fe("src/streamed_late.rs"),
        ];

        let src_v2 = children_for(&mut memo, 2, &entries_v2, "src");
        assert_eq!(
            src_v2,
            vec![
                file("lib.rs", "src/lib.rs"),
                file("main.rs", "src/main.rs"),
                file("streamed_late.rs", "src/streamed_late.rs"),
            ],
            "a query at snapshot version 2 must see the entry that batch appended; serving the \
             version-1 index here would leave a streamed file permanently invisible in the sidebar"
        );

        assert_eq!(
            memo.build_count(),
            2,
            "the version advance must have rebuilt the index exactly once more, for a total of \
             2, but the count was {}",
            memo.build_count()
        );
    }
}
