//! Headless serde tests of the application-state model, `wolf_core::app_state`, against the
//! JSON shapes of `specs/application-state.md` §2 ("The state model").
//!
//! They pin:
//!
//! - The spec's JSON example parses, means what the spec says, and serializes back to the same
//!   document: the same keys, the same array lengths, and the same numbers.
//! - The fields a front-end may not capture (`cursor`, `top_line`, `scroll`, `focused`, the
//!   sidebar's `mode`, `query` and toggles, and `drawer`) read as `None` when absent and are not
//!   written back as `null`.
//! - `frame.x` and `frame.y`, a tab's `root` and its `zoomed` are always written, `null` when
//!   unset, as the spec writes them.
//! - A window's `minimized` is written only when `true`, and reads `false` when absent.
//! - A pane reference is a depth-first pane index or the string `"drawer"`.
//! - A project state is `{"frame": …, "tab": …}`, and its tab carries no `directory`.
//!
//! Numbers are compared by value through [`same_json`], because the spec writes `120` where the
//! model holds an `f64`, and `serde_json` keeps `120` and `120.0` as different `Number`s.

use serde_json::Value;
use wolf_core::app_state::{
    Cursor, Drawer, Frame, Layout, Node, Pane, PaneOrientation, PaneRef, ProjectState, Sidebar,
    SidebarMode, Tab, TabContent, Window, WindowState,
};

/// The JSON example of `specs/application-state.md` §2, copied verbatim.
const EXAMPLE: &str = r#"{
  "windows": [
    {
      "frame": { "x": 120, "y": 80, "width": 1400, "height": 900 },
      "active_tab": 0,
      "tabs": [
        {
          "directory": "/home/user/dev/project",
          "root": {
            "type": "layout",
            "orientation": "vertical",
            "weights": [0.5, 0.5],
            "children": [
              {
                "type": "pane",
                "kind": "edit",
                "path": "/home/user/dev/project/README.md",
                "cursor": { "line": 12, "col": 4 },
                "top_line": 1
              },
              {
                "type": "pane",
                "kind": "preview",
                "path": "/home/user/dev/project/README.md",
                "scroll": 0.25
              }
            ]
          },
          "focused": 0,
          "zoomed": null,
          "sidebar": {
            "visible": true,
            "mode": "search",
            "query": "TODO",
            "case_sensitive": false,
            "use_regex": false,
            "all_files": false
          },
          "drawer": { "open": true, "share": 0.33 }
        }
      ]
    }
  ]
}"#;

/// A window state whose tab omits every field that is optional when read: an edit pane with no
/// `cursor` or `top_line`, a preview pane with no `scroll`, no `focused`, a sidebar holding only
/// `visible`, and no `drawer`.
const SPARSE: &str = r#"{
  "windows": [
    {
      "frame": { "x": 10, "y": 20, "width": 800, "height": 600 },
      "active_tab": 0,
      "tabs": [
        {
          "directory": "/srv/sparse",
          "root": {
            "type": "layout",
            "orientation": "horizontal",
            "weights": [0.5, 0.5],
            "children": [
              { "type": "pane", "kind": "edit", "path": "/srv/sparse/notes.md" },
              { "type": "pane", "kind": "preview", "path": "/srv/sparse/notes.md" }
            ]
          },
          "zoomed": null,
          "sidebar": { "visible": false }
        }
      ]
    }
  ]
}"#;

/// Compares two JSON documents: objects must have the same keys, arrays the same lengths, numbers
/// the same `f64` value, and everything else must be equal. Returns the first difference, named
/// by its path from the root.
fn same_json(actual: &Value, expected: &Value, at: &str) -> Result<(), String> {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            let mut a_keys: Vec<&String> = a.keys().collect();
            let mut e_keys: Vec<&String> = e.keys().collect();
            a_keys.sort();
            e_keys.sort();
            if a_keys != e_keys {
                return Err(format!(
                    "{at}: the keys are {a_keys:?}, where the expected document has {e_keys:?}"
                ));
            }
            for (key, e_value) in e {
                same_json(&a[key], e_value, &format!("{at}.{key}"))?;
            }
            Ok(())
        }
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                return Err(format!(
                    "{at}: the array has {} elements, where the expected document has {}",
                    a.len(),
                    e.len()
                ));
            }
            for (i, (a_item, e_item)) in a.iter().zip(e).enumerate() {
                same_json(a_item, e_item, &format!("{at}[{i}]"))?;
            }
            Ok(())
        }
        (Value::Number(a), Value::Number(e)) if a.as_f64() == e.as_f64() => Ok(()),
        _ if actual == expected => Ok(()),
        _ => Err(format!(
            "{at}: the value is {actual}, where the expected document has {expected}"
        )),
    }
}

/// Serializes `value` to a `serde_json::Value`, panicking with `what` on failure.
fn to_value<T: serde::Serialize>(value: &T, what: &str) -> Value {
    serde_json::to_value(value).unwrap_or_else(|err| panic!("failed to serialize {what}: {err}"))
}

/// The JSON object `value` is, panicking with `what` when it is something else.
fn object<'a>(value: &'a Value, what: &str) -> &'a serde_json::Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{what} must serialize to a JSON object; it is {value}"))
}

/// The spec's example parses as a `WindowState` and serializes back to the same document.
#[test]
fn the_spec_example_round_trips() {
    let parsed: WindowState = serde_json::from_str(EXAMPLE).unwrap_or_else(|err| {
        panic!("the spec's §2 example must parse as a WindowState; serde says: {err}")
    });
    let expected: Value = serde_json::from_str(EXAMPLE).expect("the spec's example is JSON");
    let written = to_value(&parsed, "the parsed spec example");
    if let Err(diff) = same_json(&written, &expected, "$") {
        panic!(
            "the parsed spec example must serialize back to the spec's document. A key the spec \
             lacks means a field is renamed or written though absent; a missing key means a \
             field the spec writes, such as \"zoomed\": null, is skipped. {diff}\nWritten: \
             {written:#}"
        );
    }
}

/// The spec's example parses to the values the spec gives it, field by field.
#[test]
fn the_spec_example_parses_to_its_meaning() {
    let parsed: WindowState = serde_json::from_str(EXAMPLE).unwrap_or_else(|err| {
        panic!(
            "the spec's §2 example must parse as a WindowState; an unknown variant here means an \
             enum such as the sidebar's mode is not spelled as the spec spells it: {err}"
        )
    });
    let readme = "/home/user/dev/project/README.md".to_owned();
    let expected = WindowState {
        windows: vec![Window {
            frame: Frame {
                x: Some(120.0),
                y: Some(80.0),
                width: 1400.0,
                height: 900.0,
            },
            active_tab: 0,
            minimized: false,
            tabs: vec![Tab {
                directory: "/home/user/dev/project".to_owned(),
                content: TabContent {
                    root: Some(Node::Layout(Layout {
                        orientation: PaneOrientation::Vertical,
                        weights: vec![0.5, 0.5],
                        children: vec![
                            Node::Pane(Pane::Edit {
                                path: readme.clone(),
                                cursor: Some(Cursor { line: 12, col: 4 }),
                                top_line: Some(1),
                            }),
                            Node::Pane(Pane::Preview {
                                path: readme,
                                scroll: Some(0.25),
                            }),
                        ],
                    })),
                    focused: Some(PaneRef::Pane(0)),
                    zoomed: None,
                    sidebar: Sidebar {
                        visible: true,
                        mode: Some(SidebarMode::Search),
                        query: Some("TODO".to_owned()),
                        case_sensitive: Some(false),
                        use_regex: Some(false),
                        all_files: Some(false),
                    },
                    drawer: Some(Drawer {
                        open: true,
                        share: 0.33,
                    }),
                },
            }],
        }],
    };
    assert_eq!(
        parsed, expected,
        "the spec's example must parse to the window, layout, panes, references, sidebar and \
         drawer it describes; the first differing field names the serde mapping that is wrong"
    );
}

/// Every field that is optional when read reads as `None` when absent, and is not written back.
#[test]
fn optional_fields_read_as_nothing_to_restore() {
    let parsed: WindowState = serde_json::from_str(SPARSE).unwrap_or_else(|err| {
        panic!(
            "a tab without cursor, top_line, scroll, focused, sidebar mode, query, toggles or \
             drawer must parse, since a front-end may not capture them: {err}"
        )
    });
    let tab = &parsed.windows[0].tabs[0].content;
    let Some(Node::Layout(layout)) = &tab.root else {
        panic!(
            "the sparse tab's root must parse as a layout; it is {:?}",
            tab.root
        );
    };
    assert_eq!(
        layout.children,
        vec![
            Node::Pane(Pane::Edit {
                path: "/srv/sparse/notes.md".to_owned(),
                cursor: None,
                top_line: None,
            }),
            Node::Pane(Pane::Preview {
                path: "/srv/sparse/notes.md".to_owned(),
                scroll: None,
            }),
        ],
        "an edit pane without cursor and top_line, and a preview pane without scroll, must read \
         them as None, meaning nothing to restore"
    );
    assert_eq!(tab.focused, None, "an absent focused must read as None");
    assert_eq!(tab.drawer, None, "an absent drawer must read as None");
    assert_eq!(
        tab.sidebar,
        Sidebar {
            visible: false,
            mode: None,
            query: None,
            case_sensitive: None,
            use_regex: None,
            all_files: None,
        },
        "a sidebar holding only visible must read visible and leave mode, query and the three \
         toggles None"
    );

    let expected: Value = serde_json::from_str(SPARSE).expect("the sparse fixture is JSON");
    let written = to_value(&parsed, "the parsed sparse window state");
    if let Err(diff) = same_json(&written, &expected, "$") {
        panic!(
            "the sparse window state must serialize back without the fields it lacks; an extra \
             key holding null means an optional field is written when None. {diff}\nWritten: \
             {written:#}"
        );
    }
}

/// A frame whose position is unknown reads `x` and `y` as `None` and writes both as `null`.
#[test]
fn a_frame_without_a_position_writes_null_x_and_y() {
    let frame: Frame =
        serde_json::from_str(r#"{"x": null, "y": null, "width": 1024, "height": 768}"#)
            .unwrap_or_else(|err| panic!("a frame with null x and y must parse: {err}"));
    assert_eq!(
        frame,
        Frame {
            x: None,
            y: None,
            width: 1024.0,
            height: 768.0,
        },
        "null x and y must read as None, with width and height kept"
    );

    let written = to_value(&frame, "a position-less frame");
    let fields = object(&written, "a frame");
    for key in ["x", "y"] {
        assert_eq!(
            fields.get(key),
            Some(&Value::Null),
            "a position-less frame must write {key:?} as null, as the spec does for GNOME; \
             None here means the key is skipped. Written: {written}"
        );
    }
}

/// A window's `minimized` is written only when `true`, and a window without the key reads `false`.
#[test]
fn a_window_writes_minimized_only_when_true() {
    let window = |minimized: bool| Window {
        frame: Frame {
            x: Some(10.0),
            y: Some(20.0),
            width: 800.0,
            height: 600.0,
        },
        active_tab: 0,
        minimized,
        tabs: Vec::new(),
    };

    let shown = to_value(&window(false), "a window that is not minimized");
    assert_eq!(
        object(&shown, "a window").get("minimized"),
        None,
        "a window that is not minimized must write no \"minimized\" key, so §2's example and \
         every saved row stay byte-identical; Some means the field is written though false. \
         Written: {shown}"
    );
    let hidden = to_value(&window(true), "a minimized window");
    assert_eq!(
        object(&hidden, "a window").get("minimized"),
        Some(&Value::Bool(true)),
        "a minimized window must write \"minimized\": true; None means the field is never \
         written, so §5 cannot restore it minimized. Written: {hidden}"
    );

    let absent: Window = serde_json::from_str(
        r#"{"frame": {"x": 1, "y": 2, "width": 3, "height": 4}, "active_tab": 0, "tabs": []}"#,
    )
    .unwrap_or_else(|err| {
        panic!("a window without \"minimized\" must parse, as every older row does: {err}")
    });
    assert!(
        !absent.minimized,
        "a window without \"minimized\" must read false"
    );
    let present: Window = serde_json::from_str(
        r#"{"frame": {"x": 1, "y": 2, "width": 3, "height": 4}, "active_tab": 0,
            "minimized": true, "tabs": []}"#,
    )
    .unwrap_or_else(|err| panic!("a window with \"minimized\": true must parse: {err}"));
    assert!(
        present.minimized,
        "a window with \"minimized\": true must read true; false means the key is ignored on read"
    );
}

/// An empty tab writes `"directory": ""` and `"root": null`, and parses back equal.
#[test]
fn an_empty_tab_has_an_empty_directory_and_a_null_root() {
    let tab = Tab {
        directory: String::new(),
        content: TabContent::default(),
    };
    assert_eq!(
        tab.content.root, None,
        "premise: a default TabContent has no root"
    );

    let written = to_value(&tab, "an empty tab");
    let fields = object(&written, "a tab");
    assert_eq!(
        fields.get("directory"),
        Some(&Value::String(String::new())),
        "an empty tab must write \"directory\": \"\"; written: {written}"
    );
    assert_eq!(
        fields.get("root"),
        Some(&Value::Null),
        "an empty tab must write \"root\": null, as the spec says; None here means the key is \
         skipped. Written: {written}"
    );

    let back: Tab = serde_json::from_value(written.clone())
        .unwrap_or_else(|err| panic!("an empty tab's JSON must parse back: {err}\n{written}"));
    assert_eq!(back, tab, "an empty tab must parse back equal to itself");
}

/// `focused` and `zoomed` are pane references: an index, or the string `"drawer"`.
#[test]
fn pane_references_are_an_index_or_drawer() {
    let json = r#"{
        "directory": "/srv/refs",
        "root": { "type": "pane", "kind": "terminal", "path": "/srv/refs" },
        "focused": "drawer",
        "zoomed": 2,
        "sidebar": { "visible": true },
        "drawer": { "open": true, "share": 0.5 }
    }"#;
    let tab: Tab = serde_json::from_str(json).unwrap_or_else(|err| {
        panic!(
            "a tab whose focused is \"drawer\" and whose zoomed is 2 must parse; the pane \
             reference visitor fails inside the flattened TabContent: {err}"
        )
    });
    assert_eq!(
        tab.content.focused,
        Some(PaneRef::Drawer),
        "\"focused\": \"drawer\" must read as the drawer"
    );
    assert_eq!(
        tab.content.zoomed,
        Some(PaneRef::Pane(2)),
        "\"zoomed\": 2 must read as the pane at depth-first index 2"
    );

    let written = to_value(&tab, "a tab with pane references");
    let fields = object(&written, "a tab");
    assert_eq!(
        fields.get("focused"),
        Some(&Value::String("drawer".to_owned())),
        "the drawer reference must write as the string \"drawer\"; written: {written}"
    );
    assert_eq!(
        fields.get("zoomed").and_then(Value::as_u64),
        Some(2),
        "a pane reference must write as its bare index; written: {written}"
    );

    let misspelled = json.replace(r#""focused": "drawer""#, r#""focused": "Drawer""#);
    assert!(
        serde_json::from_str::<Tab>(&misspelled).is_err(),
        "a pane reference must be an index or exactly \"drawer\"; \"Drawer\" parsed, so the \
         visitor accepts other strings"
    );
}

/// Whether `value` holds a `directory` key at any depth.
fn has_directory_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.contains_key("directory") || map.values().any(has_directory_key),
        Value::Array(items) => items.iter().any(has_directory_key),
        _ => false,
    }
}

/// A project state is a frame and one tab, and nothing in it names a directory.
#[test]
fn a_project_state_is_a_frame_and_a_tab_without_a_directory() {
    let state = ProjectState {
        frame: Frame {
            x: Some(40.0),
            y: None,
            width: 900.0,
            height: 700.0,
        },
        tab: TabContent {
            root: Some(Node::Pane(Pane::Edit {
                path: "/srv/project/main.md".to_owned(),
                cursor: Some(Cursor { line: 3, col: 1 }),
                top_line: Some(2),
            })),
            focused: Some(PaneRef::Pane(0)),
            zoomed: None,
            sidebar: Sidebar::default(),
            drawer: Some(Drawer {
                open: false,
                share: 0.25,
            }),
        },
    };

    let written = to_value(&state, "a project state");
    let mut keys: Vec<&String> = object(&written, "a project state").keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["frame", "tab"],
        "a project state must be exactly {{\"frame\", \"tab\"}}; written: {written}"
    );
    assert!(
        !has_directory_key(&written),
        "a project state's tab has no directory, because the directory is the row's key; \
         written: {written}"
    );

    let back: ProjectState = serde_json::from_value(written.clone())
        .unwrap_or_else(|err| panic!("a project state's JSON must parse back: {err}\n{written}"));
    assert_eq!(
        back, state,
        "a project state must parse back equal to itself"
    );
}
