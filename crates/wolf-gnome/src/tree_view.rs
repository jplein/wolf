//! The GNOME file-tree sidebar UI for `wolf-gnome`.
//!
//! This is the GTK glue that turns the pure hierarchical projection in
//! [`crate::tree`] into a live `gtk::ListView` driven by a `gtk::TreeListModel`.
//! It mirrors [`crate::picker`]'s patterns: a long-lived `wolf_core::FileSearch`,
//! an `async-channel` + `glib::spawn_future_local` drain that refreshes on each
//! traversal batch, `Rc`/`RefCell` interior mutability on the single GTK main
//! thread, and a `Weak` upgrade in the drain task to avoid a reference cycle.
//!
//! ## When the search starts, and where it is rooted
//!
//! [`build`] takes a `start` flag: the search begins at build time for a normal
//! project launch, but NOT in the empty state (no project open — see
//! [`crate::workspace`]), where the sidebar is hidden and there is nothing to
//! walk. [`TreeHandle::reroot`] then starts (or restarts) it, driven from
//! [`crate::shell`]'s ONE settle point over the pure
//! [`crate::workspace::reroot_decision`]. That covers both a front-end directory
//! change (the Open Folder dialog, an Open Recent palette row) and a `:cd` typed
//! into Neovim. This module reads no cwd of its own: the shell knows where the
//! sidebar is rooted and passes the label in.
//!
//! ## Model
//!
//! Each [`crate::tree::TreeNode`] is wrapped in a `glib::BoxedAnyObject` (no custom
//! GObject subclass). The root `gio::ListStore` holds exactly ONE item: the
//! synthetic root from [`root_node`], a `Dir` named after the working directory
//! whose `rel_path` is the empty cwd sentinel. A `gtk::TreeListModel` (passthrough =
//! false, autoexpand = false) lazily materializes each directory's children through
//! [`children_for`]; files return `None` (no children). A `gtk::SingleSelection`
//! wraps the tree model and a `gtk::SignalListItemFactory` binds each row into a
//! `gtk::TreeExpander` + label.
//!
//! `remove_all()` destroys the root's `TreeListRow` and with it its expanded state,
//! so the root is RE-EXPANDED after every root-store rebuild. macOS gets expansion
//! preservation free from `NSOutlineView`'s identity-keyed `reloadData()` and
//! carries a `didInitialExpand` flag instead; there is no such flag here and none is
//! needed.
//!
//! **Rule R1 governs every borrow of the shared `entries` and `memo`** — see
//! [`children_for`], which states it. A `RefCell` borrow held across a GTK call that
//! re-enters the create-func is a panic.
//!
//! ## Refresh: what is coalesced, what is gated
//!
//! A traversal batch goes through [`crate::refresh_coalescer`], so a burst collapses
//! into one trailing rebuild. Everything else refreshes immediately: the initial
//! build, a render wake, a re-root, and the visibility catch-up.
//!
//! A refresh then passes two gates ([`crate::tree_gate`]): it is refused outright
//! while the sidebar is hidden (and owed once when it returns), and it rebuilds only
//! when the snapshot VERSION advanced. A pending reveal is retried either way,
//! because a skipped rebuild still leaves selection and scroll to settle.
//!
//! **Present limitation: manual expansion does not survive a version advance.** A
//! current-file change advances no version, so expansion survives opening a file,
//! switching buffers and every ordinary redraw; each streaming batch and each
//! re-root still rebuilds the root store and collapses it. Preserving it across a
//! version advance means owning every expanded directory's `gio::ListStore` in a
//! registry and diffing children into it per snapshot — a redesign of the model
//! layer rather than a memo, and GTK has no `NSOutlineView` equivalent to get it
//! from.
//!
//! ## Highlight + reveal
//!
//! The currently-edited file (from `Snapshot::current_file`, delivered on the
//! existing render wake) is highlighted via the [`CURRENT_FILE_CSS`] class and
//! revealed by expanding the synthetic root and then its ancestor directories,
//! selecting its row, and scrolling it into view with `ListView::scroll_to` (the gtk
//! `v4_12` feature). Because the current file can arrive before the tree has
//! streamed in, the requested reveal is stashed in `pending_reveal` and retried on
//! each refresh.
//!
//! The bold is moved by a REGISTRY rather than by a rebuild, and that is what the
//! version gate costs. `connect_bind` records each file row's label under its
//! rel-path and `connect_unbind` drops it, so a current-file change toggles the class
//! on exactly two labels. The registry is load-bearing precisely because the gate
//! suppresses the rebuild: a current-file change advances no version, so it re-binds
//! no row, and nothing else would move the bold.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::{gio, glib};

use wolf_core::tree::TreeIndexCache;
use wolf_core::{FileEntry, FileSearch, InputEvent, WakeFn, WolfCore};

use crate::picker::FocusTarget;
use crate::refresh_coalescer::{RefreshCoalescer, REFRESH_INTERVAL};
use crate::tree::{self, TreeNode};
use crate::tree_gate::TreeRefreshGate;

/// CSS class applied to the label of the row whose file is the current buffer.
const CURRENT_FILE_CSS: &str = "wolf-current-file";

/// The sidebar's single top-level node: a synthetic directory named `label` whose
/// `rel_path` is the CWD SENTINEL — the empty string, the same one macOS uses.
///
/// A `Dir`, which is what makes `TreeInner::activate` structurally unable to reach
/// `InputEvent::Open` for the root row: only the `File` arm sends one. The empty
/// `rel_path` is what lets the ordinary create-func resolve the root's children to
/// the cwd's own top level with no special-casing.
#[must_use]
pub(crate) fn root_node(label: &str) -> TreeNode {
    TreeNode::Dir {
        name: label.to_string(),
        rel_path: String::new(),
    }
}

/// The immediate children of `rel_path` at snapshot `version`, through the
/// version-keyed `memo` rather than a fresh full rescan per query.
///
/// **Rule R1 — the borrow rule this function exists to make keepable.** GTK invokes
/// the `TreeListModel` create-func RE-ENTRANTLY: from `TreeListRow::set_expanded`,
/// from `TreeExpander::set_list_row` inside `connect_bind`, and while deciding
/// whether a row draws an expander arrow. So `memo` and `entries` are borrowed ONLY
/// inside the single call statement of this function, which contains no GTK call of
/// any kind, and every caller binds the returned `Vec` to a local. A `borrow_mut`
/// held across `set_expanded`, `append` or `set_list_row` is a `RefCell` PANIC, not
/// a warning — the same force `crate::persistence` gives its core-borrow rule.
/// Because the borrow scope contains no GTK call, a nested create-func invocation
/// cannot occur inside it: the borrow is uncontended by construction, not by luck.
#[must_use]
pub(crate) fn children_for(
    memo: &mut TreeIndexCache,
    version: u64,
    entries: &[FileEntry],
    rel_path: &str,
) -> Vec<TreeNode> {
    memo.index_for(version, entries)
        .children_of(rel_path)
        .to_vec()
}

/// The CSS that defines [`CURRENT_FILE_CSS`]. Loaded into the default display by
/// [`install_css`], once per window.
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
    /// Where focus returns after a single-click open: the SELECTED tab's cmdline
    /// strip, resolved at call time. See [`crate::picker::FocusTarget`].
    area: FocusTarget,
    /// The wake closure handed to `start_file_search`; firing it queues a refresh.
    wake: WakeFn,
    /// A reveal target that could not yet be resolved (the file had not streamed in
    /// when the current-file change arrived); retried on each refresh.
    pending_reveal: RefCell<Option<PathBuf>>,
    /// The last current-file path acted on, so `on_render_wake` only reveals when
    /// the current buffer actually changes.
    last_current: RefCell<Option<PathBuf>>,
    /// The synthetic root row's displayed name; replaced on each re-root.
    root_label: RefCell<String>,
    /// The snapshot version the shared `entries` hold. Shared with the create-func,
    /// which keys the memo on it.
    entries_version: Rc<Cell<u64>>,
    /// The per-snapshot index behind every per-directory query. Shared with the
    /// create-func; REPLACED outright on a re-root, because a fresh `FileSearch`
    /// restarts its version at 1.
    memo: Rc<RefCell<TreeIndexCache>>,
    /// The visibility + version gate consulted by every refresh.
    gate: RefCell<TreeRefreshGate>,
    /// The trailing-refresh state machine behind the traversal-batch wake.
    coalescer: RefCell<RefreshCoalescer>,
    /// The current buffer's absolute path, shared with the bind closure so a bound
    /// row can decide its highlight without taking a snapshot lock.
    current_abs: Rc<RefCell<Option<PathBuf>>>,
    /// Every currently-bound FILE row's label, keyed by `rel_path`. This is what
    /// lets the current-file bold move without a rebuild; with the version gate in
    /// place, a current-file change re-binds nothing.
    bound_labels: Rc<RefCell<HashMap<String, gtk::Label>>>,
}

impl TreeInner {
    /// Refreshes the sidebar: consults both gates, and on a version advance copies
    /// the search's entries into the shared list, rebuilds the root store from the
    /// single synthetic root and re-expands it. A pending reveal is retried EITHER
    /// WAY — a skipped rebuild leaves the structure alone but selection and scroll
    /// still have to settle. No-op if the search is gone.
    fn refresh(&self) {
        // No search at all: the empty state before its first re-root, or a closed
        // sidebar. Nothing to read and nothing to draw.
        if self.search.borrow().is_none() {
            return;
        }

        // The VISIBILITY half, in its own statement: a refresh refused here is
        // recorded and owed back when the sidebar returns.
        let may_refresh = self.gate.borrow_mut().request_refresh();
        if !may_refresh {
            return;
        }

        // ONE read of the search view per refresh: the VERSION half of the gate and
        // the entry copy both come out of it, so they cannot disagree. Every guard
        // taken here is released before the first widget call below.
        let rebuild = {
            let search = self.search.borrow();
            let Some(fs) = search.as_ref() else {
                return;
            };
            let view = fs.read();
            // Own statement, as the visibility half above.
            let rebuild = self.gate.borrow_mut().should_rebuild(view.version);
            if rebuild {
                let mut entries = self.entries.borrow_mut();
                entries.clear();
                entries.extend(view.entries.iter().cloned());
                // The shared entries and the version the memo keys on move
                // together, or a query would be answered out of the wrong index.
                self.entries_version.set(view.version);
            }
            rebuild
        };

        if rebuild {
            // The root store holds exactly ONE item: the synthetic cwd root. Its
            // children are materialized lazily by the create-func.
            let label = self.root_label.borrow().clone();
            self.root_store.remove_all();
            self.root_store
                .append(&glib::BoxedAnyObject::new(root_node(&label)));
            self.expand_root();
        }

        let pending = self.pending_reveal.borrow().clone();
        if let Some(abs) = pending {
            self.reveal(&abs);
        }
    }

    /// Requests a refresh through the trailing coalescer: a burst of traversal
    /// batches arms at most one `glib` timeout per
    /// [`crate::refresh_coalescer::REFRESH_INTERVAL`].
    ///
    /// The armed closure reads the shared state at FIRE time, so it always sees
    /// everything appended up to that instant; and because `fire` disarms before it
    /// returns, a batch arriving after a fire arms a new one rather than being
    /// dropped.
    fn refresh_coalesced(self: &Rc<Self>) {
        // Own statement: the borrow is gone before anything else runs.
        let armed = self.coalescer.borrow_mut().request();
        let Some(generation) = armed else {
            return;
        };
        // A Weak, like the drain task's, so an armed fire cannot keep the sidebar
        // alive past the window.
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(REFRESH_INTERVAL, move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            // Own statement: `refresh` rebuilds widgets, and no coalescer borrow
            // may be live across that.
            let performs = inner.coalescer.borrow_mut().fire(generation);
            if performs {
                inner.refresh();
            }
        });
    }

    /// Expands the synthetic root row, provided it has children.
    ///
    /// Called after EVERY root-store rebuild: `remove_all()` destroys the root's
    /// `TreeListRow` along with its expanded state, so without this every streamed
    /// file sits under a collapsed row.
    fn expand_root(&self) {
        // Rule R1: both borrows live inside this one call statement, whose callee
        // touches no widget. The result is a local, so nothing is borrowed when
        // `set_expanded` re-enters the create-func below.
        let children = children_for(
            &mut self.memo.borrow_mut(),
            self.entries_version.get(),
            &self.entries.borrow(),
            "",
        );
        if children.is_empty() {
            return;
        }
        if let Some(row) = self.row_at(0) {
            row.set_expanded(true);
        }
    }

    /// Handles a render wake: if `Snapshot::current_file` changed since last time,
    /// records it, moves the bold to the new row, stashes it as the reveal target,
    /// and refreshes.
    fn on_render_wake(&self) {
        let current = {
            let core_ref = self.core.borrow();
            core_ref
                .as_ref()
                .and_then(|c| c.read_snapshot().current_file.clone())
        };
        // Bound to a local so the borrow is gone before the write below.
        let unchanged = *self.last_current.borrow() == current;
        if unchanged {
            return;
        }
        let previous = self.last_current.borrow().clone();
        *self.last_current.borrow_mut() = current.clone();
        *self.pending_reveal.borrow_mut() = current.clone();
        *self.current_abs.borrow_mut() = current.clone();

        // A current-file change advances no snapshot version, so the gate suppresses
        // the rebuild and no row is re-bound: the highlight has to be moved
        // explicitly.
        let (old_rel, new_rel) = {
            let entries = self.entries.borrow();
            (
                previous
                    .as_deref()
                    .and_then(|abs| tree::rel_path_for(&entries, abs)),
                current
                    .as_deref()
                    .and_then(|abs| tree::rel_path_for(&entries, abs)),
            )
        };
        self.apply_current_highlight(old_rel.as_deref(), new_rel.as_deref());
        self.refresh();
    }

    /// Moves the [`CURRENT_FILE_CSS`] class from the row bound to `old_rel` to the
    /// one bound to `new_rel`.
    ///
    /// Both labels are resolved and CLONED under one short `borrow()` which is then
    /// dropped, because a `gtk::Label` CSS change can run a style pass and nothing
    /// may hold the registry borrow across it.
    fn apply_current_highlight(&self, old_rel: Option<&str>, new_rel: Option<&str>) {
        let (old_label, new_label) = {
            let bound = self.bound_labels.borrow();
            (
                old_rel.and_then(|rel| bound.get(rel).cloned()),
                new_rel.and_then(|rel| bound.get(rel).cloned()),
            )
        };
        if let Some(label) = old_label {
            label.remove_css_class(CURRENT_FILE_CSS);
        }
        if let Some(label) = new_label {
            label.add_css_class(CURRENT_FILE_CSS);
        }
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

        // The SYNTHETIC ROOT first. Every ancestor row is materialized underneath
        // it, so without this expansion no `find_row` below can ever match and every
        // reveal silently fails. (macOS's `expandItem(rootItem)`.)
        if let Some(row) = self.row_at(0) {
            row.set_expanded(true);
        }

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
                if let Some(area) = (self.area)() {
                    area.grab_focus();
                }
            }
            None => {
                let expanded = row.is_expanded();
                row.set_expanded(!expanded);
            }
        }
    }

    /// Drops the file search, cancelling the background traversal, and disarms any
    /// trailing refresh so a late fire cannot touch a torn-down sidebar.
    fn close(&self) {
        self.coalescer.borrow_mut().cancel();
        let _ = self.search.borrow_mut().take();
    }

    /// Records the sidebar's visibility, refreshing once if a refresh was deferred
    /// while it was hidden.
    ///
    /// Relayed from `shell::apply_sidebar_visibility`, the ONE site that calls
    /// `sidebar.set_visible`.
    fn set_sidebar_visible(&self, visible: bool) {
        // Own statement: `refresh` consults the same gate.
        let owed = self.gate.borrow_mut().set_visible(visible);
        if owed {
            self.refresh();
        }
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

    /// Re-roots the sidebar at the core's CURRENT live cwd, drawing `root_label` on
    /// the synthetic root row: cancels the existing traversal, clears every cached
    /// projection, then starts a fresh search and refreshes.
    ///
    /// Driven only from [`crate::shell`]'s settle point over
    /// [`crate::workspace::reroot_decision`], which fires once nvim has reported the
    /// directory: `start_file_search` resolves its root from `live_cwd()`, so
    /// re-rooting earlier would re-root at the OLD cwd.
    ///
    /// The pending reveal is CLEARED rather than re-armed (macOS re-arms). Re-arming
    /// would make an unresolvable reveal — the ordinary case after a `:cd` away from
    /// the current file — retry an O(N) row scan on every later refresh forever.
    fn reroot(&self, root_label: &str) {
        // Disarm FIRST. A fire armed against the old root would otherwise refresh
        // the sidebar from the directory the user just left.
        self.coalescer.borrow_mut().cancel();
        // Drop the old search next, so its in-flight traversal (at the old root)
        // is cancelled before the new one starts.
        let _ = self.search.borrow_mut().take();

        // The three that move TOGETHER, before any GTK call. `entries`, the memo and
        // `entries_version` are one triple: `refresh` states the invariant that the
        // shared entries and the version the memo keys on advance together, and every
        // instant between these statements and the next GTK call is an instant a
        // re-entrant create-func could answer a query. Clearing `entries` while the
        // memo still holds the PREVIOUS project's index at the old version would
        // answer that query out of the wrong index. A fresh `FileSearch` restarts
        // `version` at 1, so a version-keyed memo is only ever valid within one
        // search session.
        self.entries.borrow_mut().clear();
        *self.memo.borrow_mut() = TreeIndexCache::new();
        self.entries_version.set(0);

        self.root_store.remove_all();
        self.bound_labels.borrow_mut().clear();
        *self.pending_reveal.borrow_mut() = None;
        // Forget the last current file so the next wake re-reveals it under the new
        // root rather than treating it as unchanged.
        *self.last_current.borrow_mut() = None;
        *self.root_label.borrow_mut() = root_label.to_string();
        // The gate's version tracking is the memo's counterpart and is reset for the
        // same reason: version 1 of the new search shares nothing but its number with
        // version 1 of the old one.
        self.gate.borrow_mut().reset_version_tracking();
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

    /// Re-roots the sidebar at the core's live cwd, drawing `root_label` on the
    /// synthetic root row. Called from the shell's one settle point, for both a
    /// front-end directory change and a `:cd` typed into Neovim.
    pub fn reroot(&self, root_label: &str) {
        self.inner.reroot(root_label);
    }

    /// Relays the sidebar's visibility into the refresh gate, catching up once if a
    /// refresh was deferred while it was hidden.
    pub fn set_sidebar_visible(&self, visible: bool) {
        self.inner.set_sidebar_visible(visible);
    }
}

/// Ensures the [`CURRENT_FILE_CSS`] class is defined, by adding a display-global
/// provider carrying its CSS.
///
/// [`build`] calls this, so a second window adds a SECOND identical provider to
/// the same default display. Two identical providers are inert — the rule
/// resolves the same either way — and the call is deliberately NOT hoisted into
/// [`crate::coordinator`]: `crates/wolf-gnome/tests/README.md` records that the
/// current-file bold reaches no pixel in either suite, so the hoist would be an
/// unverifiable edit to the startup path.
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
/// directory's children through [`children_for`] against the shared memo.
///
/// GTK calls this closure RE-ENTRANTLY, which is what rule R1 exists for: every
/// borrow it takes ends inside one statement whose callee touches no widget.
fn build_tree_model(
    root_store: &gio::ListStore,
    entries: &Rc<RefCell<Vec<FileEntry>>>,
    entries_version: &Rc<Cell<u64>>,
    memo: &Rc<RefCell<TreeIndexCache>>,
) -> gtk::TreeListModel {
    let entries = entries.clone();
    let entries_version = entries_version.clone();
    let memo = memo.clone();
    gtk::TreeListModel::new(root_store.clone(), false, false, move |obj| {
        let boxed = obj.downcast_ref::<glib::BoxedAnyObject>()?;
        // The node borrow ends here, before the store below is populated.
        let dir = {
            let node = boxed.borrow::<TreeNode>();
            match &*node {
                TreeNode::Dir { rel_path, .. } => Some(rel_path.clone()),
                TreeNode::File { .. } => None,
            }
        };
        let rel_path = dir?;
        // Rule R1: both borrows live inside this one call statement.
        let children = children_for(
            &mut memo.borrow_mut(),
            entries_version.get(),
            &entries.borrow(),
            &rel_path,
        );
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        for child in children {
            store.append(&glib::BoxedAnyObject::new(child));
        }
        Some(store.upcast::<gio::ListModel>())
    })
}

/// Builds the list-item factory: a `TreeExpander` + `Label` per row, bound to the
/// row's node (name + current-file highlight) on each (re)bind.
///
/// The highlight is decided from the shared `current_abs` rather than from a
/// per-bind `read_snapshot()`, which removes a snapshot-lock take per bound row and
/// makes bind and [`TreeInner::apply_current_highlight`] read ONE source of truth.
/// Each bound FILE row registers its label in `bound_labels` and `connect_unbind`
/// drops it, which is what lets the bold move with no rebuild.
fn build_factory(
    current_abs: &Rc<RefCell<Option<PathBuf>>>,
    bound_labels: &Rc<RefCell<HashMap<String, gtk::Label>>>,
) -> gtk::SignalListItemFactory {
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

    let current_abs = current_abs.clone();
    let bind_labels = bound_labels.clone();
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
        // FIRST, and with nothing borrowed: computing a row's expandability
        // re-enters the `TreeListModel` create-func.
        expander.set_list_row(Some(&row));

        let Some(boxed) = row.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        // The node borrow ends here; every widget call below is outside it.
        let (name, file) = {
            let node = boxed.borrow::<TreeNode>();
            match &*node {
                TreeNode::Dir { name, .. } => (name.clone(), None),
                TreeNode::File {
                    name,
                    rel_path,
                    abs_path,
                } => (name.clone(), Some((rel_path.clone(), abs_path.clone()))),
            }
        };
        label.set_text(&name);

        let Some((rel_path, abs_path)) = file else {
            // A directory row (the synthetic root included) is never the current
            // file and is never registered.
            label.remove_css_class(CURRENT_FILE_CSS);
            return;
        };
        bind_labels.borrow_mut().insert(rel_path, label.clone());
        // Bound to a local so the borrow is gone before the CSS change.
        let is_current = current_abs.borrow().as_deref() == Some(abs_path.as_path());
        if is_current {
            label.add_css_class(CURRENT_FILE_CSS);
        } else {
            label.remove_css_class(CURRENT_FILE_CSS);
        }
    });

    let unbind_labels = bound_labels.clone();
    factory.connect_unbind(move |_factory, list_item| {
        let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        // Resolved exactly as `connect_bind` resolves it, because the removal below
        // is an IDENTITY test against this widget and not against the rel-path.
        let Some(expander) = list_item.child().and_downcast::<gtk::TreeExpander>() else {
            return;
        };
        let Some(label) = expander.child().and_downcast::<gtk::Label>() else {
            return;
        };
        let Some(row) = list_item.item().and_downcast::<gtk::TreeListRow>() else {
            return;
        };
        let Some(boxed) = row.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let rel_path = {
            let node = boxed.borrow::<TreeNode>();
            match &*node {
                TreeNode::File { rel_path, .. } => Some(rel_path.clone()),
                TreeNode::Dir { .. } => None,
            }
        };
        let Some(rel_path) = rel_path else {
            return;
        };
        // **Remove only our own registration.** GTK does not order bind and unbind
        // across recycled widgets: collapsing a directory shifts rows UP, so the same
        // file is BOUND onto the surviving list item before the surplus one is
        // unbound, and both carry that file's rel_path. A blind `remove(&rel_path)`
        // would delete the live registration the bind just made, leaving that row
        // bold with no way to clear it — two bold rows until the next version
        // advance. `gtk::Label`'s `PartialEq` is a glib pointer comparison, so this
        // asks the only question that distinguishes them: is the mapped label THIS
        // widget?
        //
        // Rule R1: the borrow lives in its own statement, and the removal below takes
        // a fresh one, so neither is held across a widget call.
        let is_ours = unbind_labels.borrow().get(&rel_path) == Some(&label);
        if is_ours {
            unbind_labels.borrow_mut().remove(&rel_path);
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
///
/// `root_label` is the text the synthetic root row draws — the launch working
/// directory's basename, from [`crate::workspace::root_label`]. This module reads no
/// cwd; the shell passes it in here and again on each [`TreeHandle::reroot`].
///
/// `area` resolves the focus-return widget at CALL time — the SELECTED tab's
/// cmdline strip. The sidebar is per WINDOW while the strip is per TAB, so the two
/// cannot be bound to each other once and for all; see
/// [`crate::picker::FocusTarget`].
#[must_use]
pub fn build(
    core: &Rc<RefCell<Option<WolfCore>>>,
    area: FocusTarget,
    start: bool,
    root_label: &str,
) -> TreeHandle {
    install_css();

    let entries: Rc<RefCell<Vec<FileEntry>>> = Rc::new(RefCell::new(Vec::new()));
    let entries_version: Rc<Cell<u64>> = Rc::new(Cell::new(0));
    let memo: Rc<RefCell<TreeIndexCache>> = Rc::new(RefCell::new(TreeIndexCache::new()));
    let current_abs: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let bound_labels: Rc<RefCell<HashMap<String, gtk::Label>>> =
        Rc::new(RefCell::new(HashMap::new()));

    let root_store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let tree_model = build_tree_model(&root_store, &entries, &entries_version, &memo);
    let selection = gtk::SingleSelection::new(Some(tree_model.clone()));
    // Don't auto-highlight row 0 at startup; a real selection only happens when a
    // current-file reveal lands.
    selection.set_autoselect(false);
    let factory = build_factory(&current_abs, &bound_labels);

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
        area,
        wake,
        pending_reveal: RefCell::new(None),
        last_current: RefCell::new(None),
        root_label: RefCell::new(root_label.to_string()),
        entries_version,
        memo,
        // Seeded VISIBLE and corrected by the first `set_empty_state`, which
        // `window::build` makes after `install_actions` and before `present()` —
        // so the gate is right before anything renders.
        gate: RefCell::new(TreeRefreshGate::new(true)),
        coalescer: RefCell::new(RefreshCoalescer::new()),
        current_abs,
        bound_labels,
    });

    // Start the long-lived file search feeding the tree — unless this is the empty
    // state, where there is no project to walk yet.
    if start {
        inner.start_search();
    }

    // Drain the wake channel on the GTK main thread. Each traversal batch goes
    // through the COALESCER rather than refreshing directly, so a burst collapses
    // into one trailing rebuild. A Weak avoids a reference cycle (inner owns the
    // wake sender; the task holds only a weak ref so dropping the handle ends the
    // task).
    {
        let inner_weak = Rc::downgrade(&inner);
        glib::spawn_future_local(async move {
            while wake_rx.recv().await.is_ok() {
                if let Some(inner) = inner_weak.upgrade() {
                    inner.refresh_coalesced();
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
