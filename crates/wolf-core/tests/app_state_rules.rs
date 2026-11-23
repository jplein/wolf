//! Headless tests of the pure rules in `wolf_core::app_state`, from
//! `specs/application-state.md`:
//!
//! - §4 "Which tab becomes a directory's project state", through `project_tab` and
//!   `project_states`: the frontmost window whose active tab is in the directory, else the most
//!   recently selected tab of the frontmost window holding one. Recency is input, a list of tab
//!   indices per window, most recent first, and a tab it does not list ranks after the listed
//!   ones in tab order.
//! - §5's pane rules, through `restored_tab`: a missing edit file or preview source restores as
//!   an empty edit pane in the same position, a zoom on such a pane or one that would hide
//!   nothing is dropped, and a focused pane keeps the focus.
//! - The document `tab_rebuild_spec` renders for the captive Neovim's `wolf_rebuild_layout`:
//!   its leaves numbered depth-first through nested layouts, as a `PaneRef` counts them, a
//!   non-finite weight rendered as `0.0`, so the split keeps its shape, and a path holding `"`,
//!   `\` and a newline kept byte for byte, at the root and under a layout.
//!
//! The existence checks run against real files in a tempdir. Windows are built in code, with
//! directories named after their role (`D` the one asked about, `X` and `Y` others).

use std::path::{Path, PathBuf};

use serde_json::Value;
use wolf_core::app_state::{
    project_states, project_tab, restored_tab, tab_rebuild_spec, Cursor, Drawer, Frame, Layout,
    Node, Pane, PaneOrientation, PaneRef, ProjectState, Sidebar, Tab, TabContent, Window,
    WindowState,
};

/// The directory each selection case asks about.
const D: &str = "/work/asked";
/// Directories of other tabs, which the rule must pass over.
const X: &str = "/work/other-x";
const Y: &str = "/work/other-y";

/// A frame whose every field is distinct from [`FRAME_1`]'s, so a mixed-up window shows.
const FRAME_0: Frame = Frame {
    x: Some(10.0),
    y: Some(20.0),
    width: 800.0,
    height: 600.0,
};
const FRAME_1: Frame = Frame {
    x: Some(900.0),
    y: Some(50.0),
    width: 1200.0,
    height: 700.0,
};

/// A tab in `directory` whose lone edit pane names `marker`, so two tabs in one directory differ.
fn tab(directory: &str, marker: &str) -> Tab {
    Tab {
        directory: directory.to_owned(),
        content: content(Some(edit(&format!("{directory}/{marker}.md")))),
    }
}

/// An empty tab: no directory, no root.
fn empty_tab() -> Tab {
    Tab {
        directory: String::new(),
        content: TabContent::default(),
    }
}

/// Tab content with `root` and every other field at its default.
fn content(root: Option<Node>) -> TabContent {
    TabContent {
        root,
        ..TabContent::default()
    }
}

/// A window with `frame`, `tabs` and the active tab `active`.
fn window(frame: Frame, active: usize, tabs: Vec<Tab>) -> Window {
    Window {
        frame,
        active_tab: active,
        minimized: false,
        tabs,
    }
}

/// A window at [`FRAME_0`] whose tabs are in `dirs`, each tab marked with its index.
fn window_of(active: usize, dirs: &[&str]) -> Window {
    let tabs = dirs
        .iter()
        .enumerate()
        .map(|(i, dir)| {
            if dir.is_empty() {
                empty_tab()
            } else {
                tab(dir, &format!("tab{i}"))
            }
        })
        .collect();
    window(FRAME_0, active, tabs)
}

fn state(windows: Vec<Window>) -> WindowState {
    WindowState { windows }
}

/// An edit pane on `path` with no cursor.
fn edit(path: &str) -> Node {
    Node::Pane(Pane::Edit {
        path: path.to_owned(),
        cursor: None,
        top_line: None,
    })
}

/// An edit pane on `path` with a saved cursor and top line.
fn edit_at(path: &str, line: u32, col: u32, top_line: u32) -> Node {
    Node::Pane(Pane::Edit {
        path: path.to_owned(),
        cursor: Some(Cursor { line, col }),
        top_line: Some(top_line),
    })
}

fn preview(path: &str, scroll: f64) -> Node {
    Node::Pane(Pane::Preview {
        path: path.to_owned(),
        scroll: Some(scroll),
    })
}

fn terminal(path: &str) -> Node {
    Node::Pane(Pane::Terminal {
        path: path.to_owned(),
    })
}

/// What a missing file restores as: an empty edit pane with nothing to restore.
fn empty_pane() -> Node {
    edit("")
}

/// A layout of `children` along `orientation`, with weights `weights`.
fn layout(orientation: PaneOrientation, weights: Vec<f64>, children: Vec<Node>) -> Node {
    Node::Layout(Layout {
        orientation,
        weights,
        children,
    })
}

/// A tempdir holding `existing.md` and `other.md`, and the path of a file it does not hold.
struct Files {
    _dir: tempfile::TempDir,
    existing: String,
    other: String,
    missing: String,
}

fn files() -> Files {
    let dir = tempfile::tempdir().expect("failed to create the fixture tempdir");
    let path = |name: &str| -> String {
        dir.path()
            .join(name)
            .to_str()
            .expect("the tempdir path is UTF-8")
            .to_owned()
    };
    let existing = path("existing.md");
    let other = path("other.md");
    let missing = path("missing.md");
    std::fs::write(&existing, "# existing\n").expect("failed to write existing.md");
    std::fs::write(&other, "# other\n").expect("failed to write other.md");
    assert!(
        Path::new(&existing).is_file() && !Path::new(&missing).exists(),
        "premise: existing.md must exist and missing.md must not"
    );
    Files {
        _dir: dir,
        existing,
        other,
        missing,
    }
}

/// A directory path that does not exist, for a terminal pane.
fn missing_dir(files: &Files) -> String {
    let dir = PathBuf::from(&files.missing).with_extension("d");
    assert!(!dir.exists(), "premise: {} must not exist", dir.display());
    dir.to_str().expect("the tempdir path is UTF-8").to_owned()
}

/// The children of `node`, which must be a layout.
fn children(node: &Option<Node>) -> &[Node] {
    match node {
        Some(Node::Layout(layout)) => &layout.children,
        other => panic!("the restored root must still be a layout; it is {other:?}"),
    }
}

#[test]
fn a_single_active_tab_in_the_directory_is_chosen() {
    let windows = state(vec![window_of(1, &[D, D])]);
    // Recency names tab 0 first, so only rule 1 picks tab 1.
    assert_eq!(
        project_tab(&windows, &[vec![0, 1]], D),
        Some((0, 1)),
        "the active tab is in D, so rule 1 must choose it over the more recent tab 0; (0, 0) \
         means rule 1 was skipped and rule 2 chose by recency"
    );
}

#[test]
fn several_active_tabs_choose_the_frontmost_window() {
    let windows = state(vec![window_of(1, &[X, D]), window_of(0, &[D])]);
    assert_eq!(
        project_tab(&windows, &[vec![1, 0], vec![0]], D),
        Some((0, 1)),
        "both windows' active tabs are in D, so the frontmost window, the first, must win; \
         (1, 0) means the windows are searched from the back"
    );
}

#[test]
fn no_active_tab_falls_back_to_the_frontmost_window_s_most_recent_tab() {
    let windows = state(vec![window_of(0, &[X, D, D]), window_of(0, &[Y, D])]);
    assert_eq!(
        project_tab(&windows, &[vec![0, 2, 1], vec![0, 1]], D),
        Some((0, 2)),
        "no active tab is in D, so rule 2 must take the frontmost window holding D and its most \
         recently selected D tab, tab 2; (0, 1) means tab order replaced recency, and (1, 1) \
         means the windows are searched from the back"
    );
}

#[test]
fn an_active_tab_anywhere_beats_a_recent_tab_in_a_front_window() {
    let windows = state(vec![window_of(0, &[X, D]), window_of(0, &[D])]);
    assert_eq!(
        project_tab(&windows, &[vec![0, 1], vec![0]], D),
        Some((1, 0)),
        "window 1's active tab is in D, so rule 1 must choose it before rule 2 looks at the \
         front window's tabs; (0, 1) means rule 2 ran first"
    );
}

#[test]
fn tabs_missing_from_recency_rank_after_listed_ones() {
    let windows = state(vec![window_of(0, &[X, D, D])]);
    let none: &[Vec<usize>] = &[];
    let empty: &[Vec<usize>] = &[Vec::new()];
    for (recency, what) in [
        (none, "no recency list for the window"),
        (empty, "an empty recency list"),
    ] {
        assert_eq!(
            project_tab(&windows, recency, D),
            Some((0, 1)),
            "with {what}, rule 2 must rank the D tabs in tab order and take tab 1; None means \
             only tabs listed in recency are considered, which breaks the empty recency at launch"
        );
    }
    assert_eq!(
        project_tab(&windows, &[vec![0, 2]], D),
        Some((0, 2)),
        "recency lists tab 2 and not tab 1, so the listed tab must rank first"
    );
}

#[test]
fn no_tab_or_an_empty_directory_chooses_nothing() {
    let windows = state(vec![window_of(0, &["", X]), window_of(0, &[""])]);
    assert_eq!(
        project_tab(&windows, &[], D),
        None,
        "no tab is in D, so nothing must be chosen"
    );
    assert_eq!(
        project_tab(&windows, &[], ""),
        None,
        "an empty tab never produces a project state, so \"\" must choose nothing even though \
         both windows' active tabs are empty"
    );
}

#[test]
fn project_states_has_one_entry_per_open_directory_with_its_window_s_frame() {
    let d1 = "/work/first";
    let d2 = "/work/second";
    let w0 = window(FRAME_0, 0, vec![tab(d1, "front")]);
    let w1 = window(
        FRAME_1,
        0,
        vec![tab(d2, "back"), tab(d1, "behind"), empty_tab()],
    );
    let expected = vec![
        (
            d1.to_owned(),
            ProjectState {
                frame: FRAME_0,
                tab: w0.tabs[0].content.clone(),
            },
        ),
        (
            d2.to_owned(),
            ProjectState {
                frame: FRAME_1,
                tab: w1.tabs[0].content.clone(),
            },
        ),
    ];
    let windows = state(vec![w0, w1]);
    assert_eq!(
        project_states(&windows, &[vec![0], vec![0, 1, 2]]),
        expected,
        "there must be one entry per non-empty directory, in order of first appearance, each \
         with the chosen tab and the frame of the window it came from. D2 with window 0's frame \
         means the frame is not taken from the chosen tab's window; an entry for \"\" means \
         empty tabs are not skipped"
    );
}

#[test]
fn a_missing_edit_file_restores_as_an_empty_pane() {
    let files = files();
    let gone_dir = missing_dir(&files);
    let unnamed = Node::Pane(Pane::Edit {
        path: String::new(),
        cursor: Some(Cursor { line: 4, col: 0 }),
        top_line: Some(2),
    });
    let saved = content(Some(layout(
        PaneOrientation::Horizontal,
        vec![0.125, 0.25, 0.375, 0.25],
        vec![
            edit_at(&files.existing, 3, 1, 2),
            edit_at(&files.missing, 9, 2, 5),
            terminal(&gone_dir),
            unnamed.clone(),
        ],
    )));

    let restored = restored_tab(&saved);
    let expected = content(Some(layout(
        PaneOrientation::Horizontal,
        vec![0.125, 0.25, 0.375, 0.25],
        vec![
            edit_at(&files.existing, 3, 1, 2),
            empty_pane(),
            terminal(&gone_dir),
            unnamed,
        ],
    )));
    assert_eq!(
        restored, expected,
        "only the edit pane on a missing file may change, into an empty edit pane with no cursor \
         in the same position. Pane 1 unchanged means missing files are kept; pane 2 replaced \
         means a terminal's directory was checked as a file; pane 3 losing its cursor means an \
         unnamed buffer was replaced"
    );
}

#[test]
fn a_missing_preview_source_restores_as_an_empty_pane() {
    let files = files();
    let saved = content(Some(layout(
        PaneOrientation::Vertical,
        vec![0.5, 0.5],
        vec![preview(&files.existing, 0.5), preview(&files.missing, 0.25)],
    )));

    let restored = restored_tab(&saved);
    assert_eq!(
        children(&restored.root),
        [preview(&files.existing, 0.5), empty_pane()],
        "a preview of a missing source must restore as an empty edit pane, and a preview of an \
         existing source must keep its kind, path and scroll"
    );
}

#[test]
fn a_zoom_on_a_pane_that_restores_empty_is_dropped() {
    let files = files();
    let root = layout(
        PaneOrientation::Vertical,
        vec![0.5, 0.5],
        vec![edit(&files.existing), edit(&files.missing)],
    );

    let mut on_missing = content(Some(root.clone()));
    on_missing.zoomed = Some(PaneRef::Pane(1));
    assert_eq!(
        restored_tab(&on_missing).zoomed,
        None,
        "pane 1 restores empty, so its zoom must be dropped"
    );

    let mut on_existing = content(Some(root));
    on_existing.zoomed = Some(PaneRef::Pane(0));
    assert_eq!(
        restored_tab(&on_existing).zoomed,
        Some(PaneRef::Pane(0)),
        "control: pane 0's file exists and a sibling would be hidden, so its zoom must be kept; \
         None means every zoom is dropped"
    );
}

#[test]
fn a_zoom_that_would_hide_nothing_is_dropped() {
    let files = files();
    let closed = Some(Drawer {
        open: false,
        share: 0.25,
    });
    let open = Some(Drawer {
        open: true,
        share: 0.25,
    });

    let mut lone = content(Some(edit(&files.existing)));
    lone.zoomed = Some(PaneRef::Pane(0));
    assert_eq!(
        restored_tab(&lone).zoomed,
        None,
        "a lone pane with no drawer hides nothing when zoomed, so the zoom must be dropped"
    );

    lone.drawer = open;
    assert_eq!(
        restored_tab(&lone).zoomed,
        Some(PaneRef::Pane(0)),
        "a lone pane zoomed over an open drawer hides the drawer, so the zoom must be kept; None \
         means the lone-pane rule ignores the drawer"
    );

    let mut drawer = content(Some(layout(
        PaneOrientation::Vertical,
        vec![0.5, 0.5],
        vec![edit(&files.existing), edit(&files.other)],
    )));
    drawer.zoomed = Some(PaneRef::Drawer);
    drawer.drawer = closed;
    assert_eq!(
        restored_tab(&drawer).zoomed,
        None,
        "a zoom on a closed drawer must be dropped"
    );

    drawer.drawer = open;
    assert_eq!(
        restored_tab(&drawer).zoomed,
        Some(PaneRef::Drawer),
        "a zoom on an open drawer hides both panes, so it must be kept"
    );
}

#[test]
fn focus_stays_on_a_pane_that_restores_empty() {
    let files = files();
    let mut saved = content(Some(layout(
        PaneOrientation::Vertical,
        vec![0.5, 0.5],
        vec![edit(&files.existing), edit(&files.missing)],
    )));
    saved.focused = Some(PaneRef::Pane(1));
    assert_eq!(
        restored_tab(&saved).focused,
        Some(PaneRef::Pane(1)),
        "a focused pane that restores empty keeps its position, so it must keep the focus"
    );
}

/// The object `value` is, panicking with `what` otherwise.
fn object<'a>(value: &'a Value, what: &str) -> &'a serde_json::Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{what} must be a JSON object; it is {value}"))
}

#[test]
fn the_rebuild_document_carries_kind_cursor_and_zoom() {
    let tab = TabContent {
        root: Some(layout(
            PaneOrientation::Vertical,
            vec![0.25, 0.25, 0.5],
            vec![
                edit_at("/srv/doc/left.md", 5, 2, 3),
                preview("/srv/doc/middle.md", 0.5),
                terminal("/srv/doc/shell"),
            ],
        )),
        focused: Some(PaneRef::Pane(0)),
        zoomed: Some(PaneRef::Pane(2)),
        sidebar: Sidebar::default(),
        drawer: None,
    };

    let spec = tab_rebuild_spec(&tab).expect("a tab with a root must render a rebuild document");
    let doc: Value = serde_json::from_str(&spec)
        .unwrap_or_else(|err| panic!("the rebuild document must be JSON: {err}\n{spec}"));
    let split = object(&doc, "the rebuild document's root");
    assert_eq!(
        split.get("orientation").and_then(Value::as_str),
        Some("v"),
        "a vertical layout must render orientation \"v\"; document: {doc}"
    );
    let weights: Option<Vec<f64>> = split
        .get("weights")
        .and_then(Value::as_array)
        .map(|w| w.iter().filter_map(Value::as_f64).collect());
    assert_eq!(
        weights,
        Some(vec![0.25, 0.25, 0.5]),
        "the split must carry the layout's weights; document: {doc}"
    );
    let leaves = split
        .get("children")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("the split must carry children; document: {doc}"));
    assert_eq!(leaves.len(), 3, "one leaf per pane; document: {doc}");
    let leaf = |i: usize| object(&leaves[i], "a leaf");
    let flag = |i: usize, key: &str| leaf(i).get(key).and_then(Value::as_bool);

    assert_eq!(
        leaf(0).get("path").and_then(Value::as_str),
        Some("/srv/doc/left.md"),
        "leaf 0 must carry its pane's path"
    );
    assert_eq!(
        (
            flag(0, "leaf"),
            flag(0, "preview"),
            flag(0, "terminal"),
            flag(0, "zoomed")
        ),
        (Some(true), Some(false), Some(false), Some(false)),
        "the edit leaf must be a plain, unzoomed leaf (leaf, preview, terminal, zoomed); \
         document: {doc}"
    );
    assert_eq!(
        leaf(0).get("cursor"),
        Some(&serde_json::json!({"line": 5, "col": 2})),
        "the edit leaf must carry its saved cursor as {{line, col}}; document: {doc}"
    );
    assert_eq!(
        leaf(0).get("top_line").and_then(Value::as_u64),
        Some(3),
        "the edit leaf must carry its saved top line; document: {doc}"
    );

    assert_eq!(
        (flag(1, "preview"), flag(1, "terminal"), flag(1, "zoomed")),
        (Some(true), Some(false), Some(false)),
        "the preview leaf must render preview true (preview, terminal, zoomed); false means the \
         helper would :edit its source. Document: {doc}"
    );
    assert!(
        !leaf(1).contains_key("cursor") && !leaf(1).contains_key("top_line"),
        "a preview leaf carries no cursor or top line; document: {doc}"
    );

    assert_eq!(
        (flag(2, "preview"), flag(2, "terminal"), flag(2, "zoomed")),
        (Some(false), Some(true), Some(true)),
        "the terminal leaf must render terminal true, and it is pane 2, the zoomed one \
         (preview, terminal, zoomed); zoomed false means the zoom is not rendered. Document: {doc}"
    );

    assert_eq!(
        tab_rebuild_spec(&TabContent::default()),
        None,
        "premise: a tab with no root has no rebuild document"
    );
}

/// The leaves of the rebuild document `node`, depth-first, as `(path, zoomed)`.
fn doc_leaves(node: &Value, out: &mut Vec<(String, Option<bool>)>) {
    let map = object(node, "a rebuild document node");
    if map.get("leaf").and_then(Value::as_bool) == Some(true) {
        let path = map
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("<no path>");
        out.push((path.to_owned(), map.get("zoomed").and_then(Value::as_bool)));
        return;
    }
    let children = map
        .get("children")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("a split must carry children; node: {node}"));
    for child in children {
        doc_leaves(child, out);
    }
}

#[test]
fn the_rebuild_document_numbers_nested_panes_depth_first() {
    let tab = TabContent {
        root: Some(layout(
            PaneOrientation::Vertical,
            vec![0.3, 0.4, 0.3],
            vec![
                edit("/srv/nest/a.md"),
                layout(
                    PaneOrientation::Horizontal,
                    vec![0.5, 0.5],
                    vec![edit("/srv/nest/b.md"), preview("/srv/nest/c.md", 0.0)],
                ),
                terminal("/srv/nest/d"),
            ],
        )),
        zoomed: Some(PaneRef::Pane(3)),
        ..TabContent::default()
    };

    let spec = tab_rebuild_spec(&tab).expect("a tab with a root must render a rebuild document");
    let doc: Value = serde_json::from_str(&spec)
        .unwrap_or_else(|err| panic!("the rebuild document must be JSON: {err}\n{spec}"));
    let mut leaves = Vec::new();
    doc_leaves(&doc, &mut leaves);
    let paths: Vec<&str> = leaves.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/srv/nest/a.md",
            "/srv/nest/b.md",
            "/srv/nest/c.md",
            "/srv/nest/d"
        ],
        "the document must hold the four panes in depth-first order, the nested split's two \
         between a and d; document: {doc}"
    );
    let zoomed: Vec<Option<bool>> = leaves.iter().map(|(_, z)| *z).collect();
    assert_eq!(
        zoomed,
        [Some(false), Some(false), Some(false), Some(true)],
        "zoomed: Pane(3) names d, the fourth pane depth-first, so d alone must be marked. No \
         leaf marked means the count restarts inside the nested split, or is not carried past \
         it; b or c marked means the nested panes are numbered from another base. Document: \
         {doc}"
    );
}

#[test]
fn a_non_finite_weight_renders_as_zero() {
    let tab = content(Some(layout(
        PaneOrientation::Horizontal,
        vec![f64::NAN, f64::INFINITY],
        vec![edit("/srv/weights/top.md"), edit("/srv/weights/bottom.md")],
    )));

    let spec = tab_rebuild_spec(&tab).expect("a tab with a root must render a rebuild document");
    let doc: Value = serde_json::from_str(&spec)
        .unwrap_or_else(|err| panic!("the rebuild document must be JSON: {err}\n{spec}"));
    let split = object(&doc, "the rebuild document's root");
    assert_eq!(
        split.get("weights"),
        Some(&serde_json::json!([0.0, 0.0])),
        "NaN and infinity must render as 0.0; nulls mean the weights went out unsanitized, \
         which JSON cannot hold, and a missing key means the split lost its weights. \
         Document: {doc}"
    );
    assert_eq!(
        split.get("orientation").and_then(Value::as_str),
        Some("h"),
        "the split must keep its orientation; document: {doc}"
    );
    let mut leaves = Vec::new();
    doc_leaves(&doc, &mut leaves);
    let paths: Vec<&str> = leaves.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        ["/srv/weights/top.md", "/srv/weights/bottom.md"],
        "the split must keep both panes in order; document: {doc}"
    );
}

/// The path of the rebuild document's root leaf, parsed back from `spec`.
fn root_leaf_path(spec: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(spec)
        .unwrap_or_else(|err| panic!("the rebuild document must be JSON: {err}\n{spec}"));
    object(&doc, "the rebuild document's root")
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[test]
fn the_rebuild_document_keeps_a_path_byte_for_byte() {
    // A Linux file name may hold `"`, `\` and a newline, and one unescaped quote makes
    // `vim.json.decode` reject the document and the whole restore abort.
    const NASTY: &str = "/proj/we\"ird\\path\nnewline.md";

    let spec = tab_rebuild_spec(&content(Some(edit(NASTY))))
        .expect("a tab with a root must render a rebuild document");
    assert_eq!(
        root_leaf_path(&spec).as_deref(),
        Some(NASTY),
        "a root edit pane's path holding `\"`, `\\` and a newline must parse back byte for byte; \
         a parse panic means the document is not valid JSON, so the path went out unescaped, and \
         a different path means a character was dropped or rewritten. Document: {spec}"
    );

    let nested = content(Some(layout(
        PaneOrientation::Vertical,
        vec![0.5, 0.5],
        vec![edit(NASTY), edit("/proj/plain.md")],
    )));
    let spec = tab_rebuild_spec(&nested).expect("a tab with a root must render a rebuild document");
    let doc: Value = serde_json::from_str(&spec)
        .unwrap_or_else(|err| panic!("the rebuild document must be JSON: {err}\n{spec}"));
    let mut leaves = Vec::new();
    doc_leaves(&doc, &mut leaves);
    let paths: Vec<&str> = leaves.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        [NASTY, "/proj/plain.md"],
        "under a layout the same path must parse back byte for byte, ahead of its untouched \
         sibling; a changed first path means the escaping differs inside a split, and a changed \
         second one means the first leaf's escaping spilled into the next. Document: {doc}"
    );
}
