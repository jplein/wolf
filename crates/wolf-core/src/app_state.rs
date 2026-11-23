//! The application-state model of `specs/application-state.md` §2, the rule of §4
//! that picks a directory's project state, and the restore-side helpers of §5.
//!
//! The types serialize to the spec's JSON. These fields are always written, as
//! `null` when unset: [`Frame::x`], [`Frame::y`], [`TabContent::root`] and
//! [`TabContent::zoomed`]. These are optional when read, an absent one meaning
//! "nothing to restore", and are not written when `None`: a pane's `cursor`,
//! `top_line` and `scroll`, a tab's `focused` and `drawer`, and the sidebar's
//! `mode`, `query` and three toggles. [`Window::minimized`] is written only when
//! `true`, and an absent one reads `false`.
//!
//! A [`PaneRef`] names a pane by its index among the root's panes in depth-first
//! order, or the drawer. That is safe across a restore because [`restored_tab`]
//! replaces a pane in place, so no index shifts, and it drops a zoom on a pane it
//! replaced.
//!
//! The rebuild document the captive Neovim's `_G.wolf_rebuild_layout` decodes is a
//! wire format of its own, separate from this model: the crate-private `leaf_spec`
//! and `layout_spec` build it for both [`tab_rebuild_spec`] and
//! [`crate::state::rebuild_layout_spec`]. It keeps `preview` and `terminal`
//! booleans rather than `kind`, so the two producers share one path in `init.lua`.
//!
//! See `specs/application-state.md#2-the-state-model`,
//! `specs/application-state.md#which-tab-becomes-a-directorys-project-state` and
//! `specs/application-state.md#5-restoring-at-launch`.

use std::collections::HashSet;
use std::path::Path;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use crate::state::PaneOrientation;

/// Every open window, in stacking order, frontmost first, with minimized windows
/// behind all others.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// The windows, frontmost first.
    pub windows: Vec<Window>,
}

/// One window: its normal frame, its tabs in tab-bar order, and the selected one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    /// The window's un-maximized, non-fullscreen geometry, in the writing
    /// front-end's coordinates.
    pub frame: Frame,
    /// The index into `tabs` of the selected tab.
    pub active_tab: usize,
    /// The window's tabs, in tab-bar order.
    pub tabs: Vec<Tab>,
    /// Whether the window is minimized. Written only when `true`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub minimized: bool,
}

/// A window's normal frame. `x` and `y` are `None` on a platform that cannot
/// report a window's position, which GNOME always is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// The origin's x coordinate, or `None` when unknown.
    pub x: Option<f64>,
    /// The origin's y coordinate, or `None` when unknown.
    pub y: Option<f64>,
    /// The width.
    pub width: f64,
    /// The height.
    pub height: f64,
}

/// One tab of a window: its directory plus its [`TabContent`], flattened into one
/// JSON object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    /// The absolute canonical path of the tab's Neovim's working directory, or
    /// `""` for an empty tab, whose `root` is `None`.
    pub directory: String,
    /// Everything else the tab holds, which is also a project state's tab.
    #[serde(flatten)]
    pub content: TabContent,
}

/// What a tab holds besides its directory: the tab of a [`ProjectState`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TabContent {
    /// The root node, a layout or a lone pane; `None` for an empty tab.
    pub root: Option<Node>,
    /// The focused pane or the drawer; `None` when there is nothing to restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused: Option<PaneRef>,
    /// The zoomed pane or the drawer; `None` when nothing is zoomed.
    pub zoomed: Option<PaneRef>,
    /// The sidebar's visibility, mode and search panel.
    pub sidebar: Sidebar,
    /// The terminal drawer; `None` when there is nothing to restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawer: Option<Drawer>,
}

/// A node of a tab's tree, tagged by `"type"`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    /// A split of its children.
    Layout(Layout),
    /// A leaf.
    Pane(Pane),
}

/// A split of its area along `orientation`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    /// The axis the children are laid out along.
    pub orientation: PaneOrientation,
    /// The per-child weights, parallel to `children`.
    pub weights: Vec<f64>,
    /// The children, left to right or top to bottom.
    pub children: Vec<Node>,
}

/// A leaf of a tab's tree, tagged by `"kind"`. What `path` means depends on the
/// kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pane {
    /// An editor window.
    Edit {
        /// The buffer's absolute file path, or `""` for a buffer that is not a
        /// file.
        path: String,
        /// The cursor, as `nvim_win_get_cursor` reports it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<Cursor>,
        /// The 1-based first visible line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        top_line: Option<u32>,
    },
    /// A Markdown preview.
    Preview {
        /// The absolute path of the Markdown source.
        path: String,
        /// The fraction of the page's scrollable height above the viewport, from
        /// `0` to `1`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scroll: Option<f64>,
    },
    /// A terminal pane, which restores as a fresh shell.
    Terminal {
        /// The directory the shell starts in.
        path: String,
    },
}

impl Pane {
    /// The pane's `path`, whatever its kind.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Pane::Edit { path, .. } | Pane::Preview { path, .. } | Pane::Terminal { path } => path,
        }
    }
}

impl Node {
    /// The panes under this node, in depth-first order: the order a [`PaneRef`]
    /// indexes.
    #[must_use]
    pub fn panes(&self) -> Vec<&Pane> {
        fn walk<'a>(node: &'a Node, out: &mut Vec<&'a Pane>) {
            match node {
                Node::Pane(pane) => out.push(pane),
                Node::Layout(layout) => layout.children.iter().for_each(|c| walk(c, out)),
            }
        }
        let mut out = Vec::new();
        walk(self, &mut out);
        out
    }
}

/// An edit pane's cursor: `line` 1-based, `col` a 0-based byte column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// The 1-based line.
    pub line: u32,
    /// The 0-based byte column.
    pub col: u32,
}

/// A reference to a pane of a tab, for `focused` and `zoomed`.
///
/// Serialized as the pane's bare depth-first index, or as the string `"drawer"`;
/// any other string fails to parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneRef {
    /// The pane at this index among the root's panes, in depth-first order.
    Pane(usize),
    /// The terminal drawer.
    Drawer,
}

/// The string a [`PaneRef::Drawer`] serializes as.
const DRAWER_REF: &str = "drawer";

impl Serialize for PaneRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            PaneRef::Pane(index) => serializer.serialize_u64(*index as u64),
            PaneRef::Drawer => serializer.serialize_str(DRAWER_REF),
        }
    }
}

impl<'de> Deserialize<'de> for PaneRef {
    /// Goes through `deserialize_any`, which also works inside the flattened
    /// [`TabContent`], where serde buffers the value before this sees it.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PaneRefVisitor;

        impl Visitor<'_> for PaneRefVisitor {
            type Value = PaneRef;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "a pane index or \"{DRAWER_REF}\"")
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<PaneRef, E> {
                usize::try_from(v)
                    .map(PaneRef::Pane)
                    .map_err(|_| E::invalid_value(de::Unexpected::Unsigned(v), &self))
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<PaneRef, E> {
                u64::try_from(v)
                    .map_err(|_| E::invalid_value(de::Unexpected::Signed(v), &self))
                    .and_then(|v| self.visit_u64(v))
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<PaneRef, E> {
                if v == DRAWER_REF {
                    Ok(PaneRef::Drawer)
                } else {
                    Err(E::invalid_value(de::Unexpected::Str(v), &self))
                }
            }
        }

        deserializer.deserialize_any(PaneRefVisitor)
    }
}

/// A tab's sidebar. Everything but `visible` is `None` when there is nothing to
/// restore; the toggles mirror [`crate::SearchOptions`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sidebar {
    /// Whether the sidebar is visible.
    pub visible: bool,
    /// Which panel the sidebar shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<SidebarMode>,
    /// The search panel's query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// The search panel's case-sensitivity toggle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_sensitive: Option<bool>,
    /// The search panel's regex toggle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_regex: Option<bool>,
    /// The search panel's all-files toggle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_files: Option<bool>,
}

impl Default for Sidebar {
    /// A visible sidebar with nothing else to restore.
    fn default() -> Self {
        Self {
            visible: true,
            mode: None,
            query: None,
            case_sensitive: None,
            use_regex: None,
            all_files: None,
        }
    }
}

/// The panel a sidebar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidebarMode {
    /// The file tree.
    Tree,
    /// The content-search panel.
    Search,
}

/// A tab's terminal drawer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Drawer {
    /// Whether the drawer is open.
    pub open: bool,
    /// The fraction of the pane area's height the drawer takes.
    pub share: f64,
}

/// What Open Folder restores for one directory: one window holding one tab.
///
/// Its JSON is `{"frame": …, "tab": …}`. The tab has no `directory`, because the
/// directory is the row's key, and no `active_tab`, which a one-tab window does
/// not need.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectState {
    /// The frame of the window the tab came from.
    pub frame: Frame,
    /// The tab.
    pub tab: TabContent,
}

/// Which tab §4 stores as `directory`'s project state, as `(window, tab)` indices
/// into `state`, or `None` for `""` and for a directory with no tab open.
///
/// 1. The active tab of the first window, in `state`'s frontmost-first order,
///    whose active tab is in `directory`. An out-of-range `active_tab` counts as
///    no active tab.
/// 2. Otherwise, of the first window holding a tab in `directory`, the first of
///    those tabs in `recency[w]`, that window's tab indices with the most recently
///    selected first. Tabs `recency[w]` does not list, and every tab when it is
///    missing, rank after the listed ones, in tab order.
///
/// Empty recency is therefore correct at launch: §3 starts it with the active tab
/// first, which never matters to rule 2, because rule 2 runs only when no window's
/// active tab is in `directory`. It picks the saved project state, and §6's tab to
/// focus. Directories compare as strings, since the front-ends write canonical
/// absolute paths.
#[must_use]
pub fn project_tab(
    state: &WindowState,
    recency: &[Vec<usize>],
    directory: &str,
) -> Option<(usize, usize)> {
    if directory.is_empty() {
        return None;
    }
    let in_directory = |window: &Window, t: usize| {
        window
            .tabs
            .get(t)
            .is_some_and(|tab| tab.directory == directory)
    };

    let active = state
        .windows
        .iter()
        .enumerate()
        .find(|(_, window)| in_directory(window, window.active_tab));
    if let Some((w, window)) = active {
        return Some((w, window.active_tab));
    }

    let (w, window) = state
        .windows
        .iter()
        .enumerate()
        .find(|(_, window)| window.tabs.iter().any(|tab| tab.directory == directory))?;
    let listed = recency.get(w).map_or(&[][..], Vec::as_slice);
    listed
        .iter()
        .copied()
        .find(|&t| in_directory(window, t))
        .or_else(|| (0..window.tabs.len()).find(|&t| in_directory(window, t)))
        .map(|t| (w, t))
}

/// The project state of every distinct non-empty directory with a tab open in
/// `state`, in order of first appearance (window order, then tab order). Each is
/// the tab [`project_tab`] picks, with the frame of the window it came from.
#[must_use]
pub fn project_states(state: &WindowState, recency: &[Vec<usize>]) -> Vec<(String, ProjectState)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for tab in state.windows.iter().flat_map(|w| &w.tabs) {
        // `project_tab` gives an empty tab's `""` no project state.
        let directory = tab.directory.as_str();
        if !seen.insert(directory) {
            continue;
        }
        if let Some((w, t)) = project_tab(state, recency, directory) {
            let window = &state.windows[w];
            out.push((
                directory.to_owned(),
                ProjectState {
                    frame: window.frame,
                    tab: window.tabs[t].content.clone(),
                },
            ));
        }
    }
    out
}

/// `content` as §5 restores it, ahead of [`tab_rebuild_spec`].
///
/// - An edit pane whose non-empty `path` is not an existing regular file, and a
///   preview pane whose source is not one, become an empty edit pane in the same
///   position. A terminal pane and an empty-path edit pane are kept: a terminal
///   falls back to the tabpage cwd in [`crate::WolfCore::rebuild_layout`].
/// - `zoomed` is dropped when it names a replaced pane or an out-of-range one,
///   when it names a pane of a lone-pane root with no open drawer, where it would
///   hide nothing, when it names a drawer that is not open, and when there is no
///   root.
/// - `focused` is dropped when it names an out-of-range pane or a drawer that is
///   not open, or when there is no root. A replaced pane keeps the focus.
///
/// A zoom is dropped only on a pane this replaced, never on one saved with an
/// empty path, since that pane restores as it was saved.
#[must_use]
pub fn restored_tab(content: &TabContent) -> TabContent {
    let mut out = content.clone();
    let Some(root) = out.root.as_mut() else {
        out.focused = None;
        out.zoomed = None;
        return out;
    };
    let mut replaced = Vec::new();
    let count = replace_missing(root, 0, &mut replaced);
    let drawer_open = out.drawer.is_some_and(|d| d.open);
    let names_something = |r: PaneRef| match r {
        PaneRef::Pane(i) => i < count,
        PaneRef::Drawer => drawer_open,
    };
    out.focused = out.focused.filter(|&r| names_something(r));
    out.zoomed = out.zoomed.filter(|&r| {
        names_something(r)
            && match r {
                PaneRef::Pane(i) => !replaced.contains(&i) && (count > 1 || drawer_open),
                PaneRef::Drawer => true,
            }
    });
    out
}

/// Replaces each unrestorable pane under `node` with an empty edit pane, pushing
/// its depth-first index, counted from `first`, onto `replaced`. Returns the index
/// after the last pane under `node`.
fn replace_missing(node: &mut Node, first: usize, replaced: &mut Vec<usize>) -> usize {
    match node {
        Node::Pane(pane) => {
            let missing = match pane {
                Pane::Edit { path, .. } => !path.is_empty() && !Path::new(path).is_file(),
                Pane::Preview { path, .. } => !Path::new(path).is_file(),
                Pane::Terminal { .. } => false,
            };
            if missing {
                *pane = Pane::Edit {
                    path: String::new(),
                    cursor: None,
                    top_line: None,
                };
                replaced.push(first);
            }
            first + 1
        }
        Node::Layout(layout) => layout
            .children
            .iter_mut()
            .fold(first, |next, child| replace_missing(child, next, replaced)),
    }
}

/// Renders `content`'s root into the JSON string `runtime/nvim/init.lua`'s
/// `_G.wolf_rebuild_layout` decodes, or `None` when there is no root.
///
/// It renders the tab as is, checking no file: pass it [`restored_tab`]'s output.
/// An edit leaf carries its `cursor` and `top_line`, which the helper applies with
/// the line clamped to the buffer, and the leaf `zoomed` names is marked zoomed. A
/// zoom on the drawer marks no leaf. Total, as
/// [`crate::state::rebuild_layout_spec`] is: a serialization error renders `"{}"`.
#[must_use]
pub fn tab_rebuild_spec(content: &TabContent) -> Option<String> {
    let root = content.root.as_ref()?;
    let zoomed = match content.zoomed {
        Some(PaneRef::Pane(i)) => Some(i),
        _ => None,
    };
    let mut next = 0;
    let value = node_spec(root, zoomed, &mut next);
    Some(serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_owned()))
}

/// The rebuild document of `node`, numbering its leaves from `*next` and marking
/// the leaf at depth-first index `zoomed`.
fn node_spec(node: &Node, zoomed: Option<usize>, next: &mut usize) -> serde_json::Value {
    match node {
        Node::Pane(pane) => {
            let z = zoomed == Some(*next);
            *next += 1;
            match pane {
                Pane::Edit {
                    path,
                    cursor,
                    top_line,
                } => leaf_spec(path, false, false, z, *cursor, *top_line),
                Pane::Preview { path, .. } => leaf_spec(path, true, false, z, None, None),
                Pane::Terminal { path } => leaf_spec(path, false, true, z, None, None),
            }
        }
        Node::Layout(layout) => {
            let children = layout
                .children
                .iter()
                .map(|child| node_spec(child, zoomed, next))
                .collect();
            layout_spec(layout.orientation, &layout.weights, children)
        }
    }
}

/// One leaf of the rebuild document: `leaf`, `path`, `preview`, `terminal` and
/// `zoomed` always, plus `"cursor": {"line", "col"}` and `"top_line"` only when
/// they are `Some`, so a document with neither is the one `init.lua` has always
/// read.
pub(crate) fn leaf_spec(
    path: &str,
    preview: bool,
    terminal: bool,
    zoomed: bool,
    cursor: Option<Cursor>,
    top_line: Option<u32>,
) -> serde_json::Value {
    let mut leaf = serde_json::json!({
        "leaf": true,
        "path": path,
        "preview": preview,
        "terminal": terminal,
        "zoomed": zoomed,
    });
    if let Some(map) = leaf.as_object_mut() {
        if let Some(cursor) = cursor {
            map.insert(
                "cursor".to_owned(),
                serde_json::json!({ "line": cursor.line, "col": cursor.col }),
            );
        }
        if let Some(top_line) = top_line {
            map.insert("top_line".to_owned(), serde_json::Value::from(top_line));
        }
    }
    leaf
}

/// One split of the rebuild document: `orientation` as `"v"` for
/// [`PaneOrientation::Vertical`] (side by side) or `"h"` for
/// [`PaneOrientation::Horizontal`] (stacked), `weights`, and `children`.
///
/// A non-finite weight is emitted as `0.0`: `serde_json::Number::from_f64` rejects
/// NaN and ±inf, and an unsanitized value would silently become `null` and change
/// the document's shape. The substitution is cosmetic, since `wolf_rebuild_layout`
/// builds the window count and topology only and never reads `weights`.
pub(crate) fn layout_spec(
    orientation: PaneOrientation,
    weights: &[f64],
    children: Vec<serde_json::Value>,
) -> serde_json::Value {
    let tag = match orientation {
        PaneOrientation::Vertical => "v",
        PaneOrientation::Horizontal => "h",
    };
    let weights: Vec<serde_json::Value> = weights
        .iter()
        .map(|w| serde_json::Value::from(if w.is_finite() { *w } else { 0.0 }))
        .collect();
    serde_json::json!({
        "orientation": tag,
        "weights": weights,
        "children": children,
    })
}
