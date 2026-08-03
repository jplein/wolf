//! Headless coverage for the NEW per-tab PANE-LAYOUT persistence (feature plan
//! §4.1): `TabState.layout: Option<PaneNode>` plus the `PaneNode` / `PaneOrientation`
//! value types (all re-exported at the crate root), round-tripped through
//! `StateStore::set_snapshot` -> `flush` -> `load_state`, with the on-disk serde
//! shape pinned and the v1 back-compat + unknown-field tolerance guarded.
//!
//! ## Why this lives in `tests/` rather than `src/state.rs`'s `#[cfg(test)]`
//!
//! Tests are written before the coder adds the schema. This file exercises ONLY
//! the planned public, crate-root-re-exported API. The coder may keep it as-is or
//! fold the cases into `src/state.rs`'s test module.
//!
//! ## What is asserted
//!
//!   * A `WolfState` whose tabs carry nested `PaneNode`s (leaf, 2-child split,
//!     n-ary split, perpendicular nesting, and a `""`-path leaf) round-trips
//!     value-for-value.
//!   * BACK-COMPAT: a v1 `state.json` with NO `layout` field loads with
//!     `layout: None` (the field is `#[serde(default)]`; STATE_VERSION stays 1).
//!   * TOLERANCE: a file carrying an unknown extra field still loads.
//!   * ON-DISK SHAPE: orientation serializes as `"horizontal"` / `"vertical"` and
//!     each node's tag is `"type": "leaf"` / `"type": "split"`.
//!   * PREVIEW (ROUND 2): a leaf's `preview: bool` round-trips (`"preview": true`
//!     on disk for a preview leaf); a v1 leaf object with NO `preview` key loads as
//!     `preview: false` (the field is `#[serde(default)]`; STATE_VERSION stays 1).
//!
//! Needs NO nvim and no network: everything is driven through explicit `tempfile`
//! tempdirs passed to `StateStore::open` / `load_state`.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use tempfile::tempdir;
use wolf_core::state::{rebuild_layout_spec, STATE_FILE_NAME};
use wolf_core::{
    load_state, PaneNode, PaneOrientation, StateStore, TabState, WindowState, WolfState,
};

/// Generous upper bound for `StateStore::flush` in tests.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

// -- Distinctive window geometry (named per mnd) -------------------------------
const WIN_X: f64 = 128.0;
const WIN_Y: f64 = 64.0;
const WIN_WIDTH: f64 = 1180.0;
const WIN_HEIGHT: f64 = 760.0;

/// Reads a file to a `String`, panicking with context on failure (errcheck).
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Writes `contents` to `path`, panicking with context on failure (errcheck).
fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
}

/// A (non-preview) leaf pane node bound to `path`.
fn leaf(path: &str) -> PaneNode {
    PaneNode::Leaf {
        path: path.to_string(),
        preview: false,
    }
}

/// A PREVIEW leaf pane node whose `path` is its SOURCE markdown file (ROUND 2 —
/// preview restore). `preview: true` marks it so `wolf_rebuild_layout` recreates a
/// preview of the source instead of `:edit`-ing a blank buffer.
fn preview_leaf(path: &str) -> PaneNode {
    PaneNode::Leaf {
        path: path.to_string(),
        preview: true,
    }
}

/// A vertical split of an editor leaf and a PREVIEW leaf that share the SAME source
/// markdown file (the canonical "editor + live preview of this file" layout).
fn editor_and_preview_split(path: &str) -> PaneNode {
    PaneNode::Split {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.5, 0.5],
        children: vec![leaf(path), preview_leaf(path)],
    }
}

/// A tab with the given cwd + optional pane layout (sidebar always visible here).
fn tab(cwd: &str, layout: Option<PaneNode>) -> TabState {
    TabState {
        cwd: cwd.to_string(),
        sidebar_visible: true,
        layout,
    }
}

/// A 2-child vertical split of two named leaves (0.5 / 0.5).
fn two_child_split() -> PaneNode {
    PaneNode::Split {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.5, 0.5],
        children: vec![leaf("/proj/a.md"), leaf("/proj/b.md")],
    }
}

/// An n-ary horizontal split (three children, unequal weights).
fn nary_split() -> PaneNode {
    PaneNode::Split {
        orientation: PaneOrientation::Horizontal,
        weights: vec![0.5, 0.25, 0.25],
        children: vec![leaf("/proj/a.md"), leaf("/proj/b.md"), leaf("/proj/c.md")],
    }
}

/// A perpendicular NESTED tree: a vertical split whose second child is itself a
/// horizontal split, and whose FIRST leaf has an EMPTY ("") path (an unnamed
/// buffer pane).
fn perpendicular_split() -> PaneNode {
    PaneNode::Split {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.6, 0.4],
        children: vec![
            leaf(""),
            PaneNode::Split {
                orientation: PaneOrientation::Horizontal,
                weights: vec![0.5, 0.5],
                children: vec![leaf("/proj/b.md"), leaf("/proj/c.md")],
            },
        ],
    }
}

/// The fixture tree: one window with four tabs, each carrying a different layout
/// shape (a bare leaf, a 2-child split, an n-ary split, and a perpendicular nest).
fn sample_tree() -> WolfState {
    WolfState {
        windows: vec![WindowState {
            x: WIN_X,
            y: WIN_Y,
            width: WIN_WIDTH,
            height: WIN_HEIGHT,
            tabs: vec![
                tab("/proj", Some(leaf("/proj/only.md"))),
                tab("/proj", Some(two_child_split())),
                tab("/proj", Some(nary_split())),
                tab("/proj", Some(perpendicular_split())),
            ],
        }],
    }
}

// =========================================================================
// Full round-trip: set_snapshot -> flush -> load_state == the whole tree
// =========================================================================

#[test]
fn pane_layout_round_trips_through_store_and_load() {
    let dir = tempdir().expect("create temp state dir");
    let tree = sample_tree();

    let store = StateStore::open(dir.path());
    store.set_snapshot(tree.clone());
    assert!(
        store.flush(FLUSH_TIMEOUT),
        "flush after set_snapshot should complete within {FLUSH_TIMEOUT:?}"
    );

    let loaded = load_state(dir.path()).expect("load_state should parse the written file");
    assert_eq!(
        loaded, tree,
        "the entire loaded tree (including every tab's nested PaneNode + weights) must round-trip"
    );

    // Spot-check the layout shapes survived structurally.
    let tabs = &loaded.windows[0].tabs;
    assert_eq!(tabs.len(), 4, "four tabs");
    assert_eq!(
        tabs[0].layout,
        Some(leaf("/proj/only.md")),
        "tab 0 keeps its single-leaf layout"
    );
    assert_eq!(
        tabs[3].layout,
        Some(perpendicular_split()),
        "tab 3 keeps its perpendicular-nested layout (with the empty-path leaf)"
    );

    // On-disk serde shape: node tags + orientation strings.
    let text = read(&dir.path().join(STATE_FILE_NAME));
    assert!(
        text.contains("\"type\": \"leaf\""),
        "a leaf node must serialize with `\"type\": \"leaf\"`, got:\n{text}"
    );
    assert!(
        text.contains("\"type\": \"split\""),
        "a split node must serialize with `\"type\": \"split\"`, got:\n{text}"
    );
    assert!(
        text.contains("\"vertical\""),
        "a vertical orientation must serialize as the string `\"vertical\"`, got:\n{text}"
    );
    assert!(
        text.contains("\"horizontal\""),
        "a horizontal orientation must serialize as the string `\"horizontal\"`, got:\n{text}"
    );
}

// =========================================================================
// Back-compat: a v1 file with NO `layout` field loads with layout: None
// =========================================================================

#[test]
fn v1_file_without_layout_loads_with_none() {
    let dir = tempdir().expect("create temp state dir");
    let path = dir.path().join(STATE_FILE_NAME);

    // A valid v1 tree whose tabs carry only cwd + sidebar_visible (the pre-layout
    // shape). `layout` is `#[serde(default)]`, so this must load with None — and
    // STATE_VERSION stays 1 (no version bump, no discard of existing state).
    let no_layout = r#"{
  "version": 1,
  "windows": [
    {
      "x": 128.0,
      "y": 64.0,
      "width": 1180.0,
      "height": 760.0,
      "tabs": [
        { "cwd": "/proj", "sidebar_visible": true },
        { "cwd": "/other", "sidebar_visible": false }
      ]
    }
  ]
}
"#;
    write(&path, no_layout);

    let loaded = load_state(dir.path()).expect("a v1 file without `layout` must still load");
    let tabs = &loaded.windows[0].tabs;
    assert_eq!(tabs.len(), 2, "both tabs load");
    assert!(
        tabs.iter().all(|t| t.layout.is_none()),
        "a tab with no on-disk `layout` field must default to layout: None"
    );
}

// =========================================================================
// Tolerance: an unknown extra field still loads
// =========================================================================

#[test]
fn unknown_extra_field_is_tolerated() {
    let dir = tempdir().expect("create temp state dir");
    let path = dir.path().join(STATE_FILE_NAME);

    // A valid v1 tree carrying a `layout` AND an unknown extra field on both the
    // window and the tab. With no `deny_unknown_fields`, load_state must tolerate
    // it (newer writers must not break older readers).
    let with_extra = r#"{
  "version": 1,
  "windows": [
    {
      "x": 128.0,
      "y": 64.0,
      "width": 1180.0,
      "height": 760.0,
      "future_window_field": "ignored",
      "tabs": [
        {
          "cwd": "/proj",
          "sidebar_visible": true,
          "future_tab_field": 42,
          "layout": {
            "type": "split",
            "orientation": "vertical",
            "weights": [0.5, 0.5],
            "children": [
              { "type": "leaf", "path": "/proj/a.md" },
              { "type": "leaf", "path": "/proj/b.md" }
            ]
          }
        }
      ]
    }
  ]
}
"#;
    write(&path, with_extra);

    let loaded = load_state(dir.path()).expect("an unknown extra field must be tolerated");
    let tab0 = &loaded.windows[0].tabs[0];
    assert_eq!(
        tab0.layout,
        Some(two_child_split()),
        "the layout parses despite the unknown sibling fields"
    );
}

// =========================================================================
// PREVIEW round-trip: a preview leaf preserves `preview: true` (ROUND 2)
// =========================================================================

#[test]
fn preview_leaf_round_trips_preserving_the_preview_flag() {
    let dir = tempdir().expect("create temp state dir");
    let tree = WolfState {
        windows: vec![WindowState {
            x: WIN_X,
            y: WIN_Y,
            width: WIN_WIDTH,
            height: WIN_HEIGHT,
            tabs: vec![tab("/proj", Some(editor_and_preview_split("/proj/a.md")))],
        }],
    };

    let store = StateStore::open(dir.path());
    store.set_snapshot(tree.clone());
    assert!(
        store.flush(FLUSH_TIMEOUT),
        "flush after set_snapshot should complete within {FLUSH_TIMEOUT:?}"
    );

    let loaded = load_state(dir.path()).expect("load_state should parse the written file");
    assert_eq!(
        loaded, tree,
        "the whole tree (including the preview leaf's preview flag) must round-trip"
    );

    // Spot-check the split's children: editor (preview: false) + preview (true).
    match &loaded.windows[0].tabs[0].layout {
        Some(PaneNode::Split { children, .. }) => {
            assert_eq!(children.len(), 2, "the split has two children");
            assert_eq!(
                children[0],
                leaf("/proj/a.md"),
                "the editor child keeps preview: false"
            );
            assert_eq!(
                children[1],
                preview_leaf("/proj/a.md"),
                "the preview child keeps preview: true and the SAME source path"
            );
        }
        other => panic!("expected a split layout, got {other:?}"),
    }

    // On-disk shape: the preview leaf serializes `"preview": true`.
    let text = read(&dir.path().join(STATE_FILE_NAME));
    assert!(
        text.contains("\"preview\": true"),
        "a preview leaf must serialize with `\"preview\": true`, got:\n{text}"
    );
}

// =========================================================================
// Back-compat: a v1 layout leaf with NO `preview` key loads as preview: false
// =========================================================================

#[test]
fn v1_layout_leaf_without_preview_defaults_to_false() {
    let dir = tempdir().expect("create temp state dir");
    let path = dir.path().join(STATE_FILE_NAME);

    // A hand-crafted v1 file whose leaf objects carry `type`/`path` but NO `preview`
    // key (the round-1 shape). `preview` is `#[serde(default)]`, so each leaf must
    // load with `preview: false` — and the file must NOT be rejected (still v1, no
    // version bump).
    let no_preview = r#"{
  "version": 1,
  "windows": [
    {
      "x": 128.0,
      "y": 64.0,
      "width": 1180.0,
      "height": 760.0,
      "tabs": [
        {
          "cwd": "/proj",
          "sidebar_visible": true,
          "layout": {
            "type": "split",
            "orientation": "vertical",
            "weights": [0.5, 0.5],
            "children": [
              { "type": "leaf", "path": "/proj/a.md" },
              { "type": "leaf", "path": "/proj/b.md" }
            ]
          }
        }
      ]
    }
  ]
}
"#;
    write(&path, no_preview);

    let loaded =
        load_state(dir.path()).expect("a v1 layout without `preview` keys must still load");
    assert_eq!(
        loaded.windows.len(),
        1,
        "the v1 file is accepted, not rejected (still version 1)"
    );
    assert_eq!(
        loaded.windows[0].tabs[0].layout,
        Some(two_child_split()),
        "a leaf with no on-disk `preview` field must default to preview: false"
    );
}

// =========================================================================
// `rebuild_layout_spec` — the WIRE shape `_G.wolf_rebuild_layout` decodes
//
// This is NOT `PaneNode`'s serde shape. The Lua helper reads a terser document:
//
//   leaf  -> { "leaf": true, "path": "<abs or empty>", "preview": true|false }
//   split -> { "orientation": "v"|"h", "weights": [...], "children": [...] }
//
// Every assertion below PARSES the rendered string back with
// `serde_json::from_str::<Value>` and inspects fields, rather than comparing
// strings: key ORDER is not part of the contract (`vim.json.decode` is a JSON
// parser, not a text matcher), and a string comparison would turn a harmless
// serializer change into a red test while still missing a genuinely wrong VALUE.
// =========================================================================

/// Parses a rendered spec back into a `serde_json::Value`, panicking with the
/// offending text on failure.
///
/// A parse failure here is exactly what `wolf_rebuild_layout`'s
/// `pcall(vim.json.decode, …)` degrades to: it returns `{}`, `rebuild_layout`
/// returns no handles, and the restore silently does nothing.
fn parse_spec(node: &PaneNode) -> Value {
    let text = rebuild_layout_spec(node);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!(
            "rebuild_layout_spec produced text that is not valid JSON ({e}). \
             `wolf_rebuild_layout` wraps its decode in a pcall, so on the Lua side this failure is \
             SILENT: it returns {{}}, no window handles come back, and the saved layout is simply \
             never restored. Rendered text was:\n{text}"
        )
    })
}

/// Collects the `path` of every leaf object in `value`, in DOCUMENT order — the
/// order `wolf_rebuild_layout` realizes regions and returns handles in.
fn spec_leaf_paths(value: &Value, out: &mut Vec<String>) {
    if value.get("leaf").and_then(Value::as_bool) == Some(true) {
        out.push(
            value
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        );
        return;
    }
    if let Some(children) = value.get("children").and_then(Value::as_array) {
        for child in children {
            spec_leaf_paths(child, out);
        }
    }
}

#[test]
fn rebuild_layout_spec_matches_the_lua_contract() {
    // --- a plain editor leaf ---------------------------------------------
    let doc = parse_spec(&leaf("/proj/only.md"));
    assert_eq!(
        doc.get("leaf"),
        Some(&Value::Bool(true)),
        "a leaf must carry `\"leaf\": true`; `wolf_rebuild_layout`'s `if node.leaf then` is the \
         ONLY thing that distinguishes a leaf from a split, and a missing/false flag makes it fall \
         through to the split branch with no children, creating one region and no `:edit`"
    );
    assert_eq!(
        doc.get("path").and_then(Value::as_str),
        Some("/proj/only.md"),
        "the leaf's `path` is what the Lua side `:edit`s (through `fnameescape`)"
    );
    assert_eq!(
        doc.get("preview"),
        Some(&Value::Bool(false)),
        "`preview` must be present and false for an editor leaf; the Lua branches on it before \
         `:edit`"
    );
    assert!(
        doc.get("orientation").is_none() && doc.get("children").is_none(),
        "a leaf must carry no split keys at all: {doc}"
    );

    // --- a preview leaf ---------------------------------------------------
    let preview_doc = parse_spec(&preview_leaf("/proj/only.md"));
    assert_eq!(
        preview_doc.get("preview"),
        Some(&Value::Bool(true)),
        "a preview leaf must render `\"preview\": true`, which routes the Lua side to \
         `wolf_preview_file` instead of `:edit`. wolf-gnome never produces one today, but the flag \
         is carried end to end so the preview unit adds a VALUE rather than a field."
    );

    // --- an empty-path leaf ------------------------------------------------
    let empty_doc = parse_spec(&leaf(""));
    assert_eq!(
        empty_doc.get("path").and_then(Value::as_str),
        Some(""),
        "an unnamed pane renders as an empty `path`, which the Lua side leaves as the inherited \
         (unnamed) buffer while STILL returning the window handle — the handle count must match \
         the leaf count regardless"
    );

    // --- the ORIENTATION TAG, both directions ------------------------------
    //
    // THE row most worth having. Inverting `v`/`h` transposes every restored
    // layout — a saved side-by-side pair comes back stacked — while every
    // count-based assertion (handles returned, leaves realized, panes bound)
    // stays green, because the COUNT is unchanged.
    let vertical = parse_spec(&two_child_split());
    assert_eq!(
        vertical.get("orientation").and_then(Value::as_str),
        Some("v"),
        "PaneOrientation::Vertical (side by side, divides WIDTH) must render as the tag \"v\", \
         which is the only value `wolf_rebuild_layout` tests for (`local vertical = \
         node.orientation == \"v\"`). Anything else falls through to `rightbelow split` and stacks \
         the panes instead."
    );
    let horizontal = parse_spec(&nary_split());
    assert_eq!(
        horizontal.get("orientation").and_then(Value::as_str),
        Some("h"),
        "PaneOrientation::Horizontal (stacked, divides HEIGHT) must render as the tag \"h\". Note \
         the Lua only tests for \"v\", so ANY other string stacks — which is why this row is \
         asserted as an exact string rather than merely `!= \"v\"`."
    );

    // --- the split's own shape ---------------------------------------------
    assert!(
        vertical.get("leaf").is_none(),
        "a split must NOT carry a `leaf` key: `if node.leaf then` would take the leaf branch and \
         the whole subtree would collapse into one window. Got: {vertical}"
    );
    assert_eq!(
        vertical
            .get("children")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(2),
        "the two-child split must render two children"
    );
    let want_vertical_weights = vec![Value::from(0.5_f64), Value::from(0.5_f64)];
    assert_eq!(
        vertical.get("weights").and_then(Value::as_array),
        Some(&want_vertical_weights),
        "weights render as a JSON array of numbers, parallel to `children`"
    );
    assert_eq!(
        horizontal
            .get("children")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(3),
        "the n-ary split must render all three children — the Lua creates `#children` regions"
    );
    let want_horizontal_weights = vec![
        Value::from(0.5_f64),
        Value::from(0.25_f64),
        Value::from(0.25_f64),
    ];
    assert_eq!(
        horizontal.get("weights").and_then(Value::as_array),
        Some(&want_horizontal_weights),
        "the n-ary split's UNEQUAL weights render verbatim and in order"
    );

    // --- nesting recurses ---------------------------------------------------
    let nested = parse_spec(&perpendicular_split());
    let outer_children = nested
        .get("children")
        .and_then(Value::as_array)
        .expect("the perpendicular tree's root is a split with children");
    assert_eq!(outer_children.len(), 2, "the outer split has two children");
    assert_eq!(
        outer_children[0].get("leaf"),
        Some(&Value::Bool(true)),
        "the outer split's first child is a leaf"
    );
    assert_eq!(
        outer_children[1].get("orientation").and_then(Value::as_str),
        Some("h"),
        "the outer split's second child is itself a HORIZONTAL split, so the renderer recurses \
         and tags each level independently"
    );
}

#[test]
fn rebuild_layout_spec_leaf_order_is_dfs() {
    // `wolf_rebuild_layout` realizes regions child-0-first and gathers handles in
    // the same walk, so the DOCUMENT order of leaves IS the order the returned
    // handle vector is in. Every front-end zips its own DFS leaf ids onto that
    // vector by index, which is why the order is a contract and not an incidental.
    let mut paths = Vec::new();
    spec_leaf_paths(&parse_spec(&nary_split()), &mut paths);
    assert_eq!(
        paths,
        vec![
            "/proj/a.md".to_string(),
            "/proj/b.md".to_string(),
            "/proj/c.md".to_string()
        ],
        "a flat n-ary split emits its leaves left-to-right"
    );

    let mut nested_paths = Vec::new();
    spec_leaf_paths(&parse_spec(&perpendicular_split()), &mut nested_paths);
    assert_eq!(
        nested_paths,
        vec![
            String::new(),
            "/proj/b.md".to_string(),
            "/proj/c.md".to_string()
        ],
        "a NESTED tree emits depth-first, child-0 first: the outer split's empty-path leaf comes \
         before both leaves of the inner horizontal split. A breadth-first walk would produce the \
         same three paths in a different order and misbind every restored pane."
    );

    // A lone leaf is still a one-element sequence — the single-pane restore shape.
    let mut lone = Vec::new();
    spec_leaf_paths(&parse_spec(&leaf("/proj/only.md")), &mut lone);
    assert_eq!(
        lone,
        vec!["/proj/only.md".to_string()],
        "a lone leaf renders as a single leaf document, which `wolf_rebuild_layout` realizes into \
         exactly one handle"
    );
}

#[test]
fn rebuild_layout_spec_escapes_paths() {
    // THIS is why the renderer is not hand-rolled. A file name may legally contain
    // a double quote, a backslash and even a newline on Linux. An unescaped quote
    // makes `pcall(vim.json.decode, …)` fail, `wolf_rebuild_layout` return `{}`,
    // and the restore abort SILENTLY — no error, no handles, no layout.
    const NASTY: &str = "/proj/we\"ird\\path\nnewline.md";

    let doc = parse_spec(&leaf(NASTY));
    assert_eq!(
        doc.get("path").and_then(Value::as_str),
        Some(NASTY),
        "a path containing `\"`, `\\` and a newline must survive the render/parse round trip \
         BYTE FOR BYTE. If this fails, `serde_json` is no longer doing the escaping — a \
         hand-rolled writer would produce a document `vim.json.decode` rejects, and the restore \
         would fail silently rather than loudly."
    );

    // And nested inside a split, so the escaping is not special-cased at the root.
    let split = PaneNode::Split {
        orientation: PaneOrientation::Vertical,
        weights: vec![0.5, 0.5],
        children: vec![leaf(NASTY), leaf("/proj/plain.md")],
    };
    let mut paths = Vec::new();
    spec_leaf_paths(&parse_spec(&split), &mut paths);
    assert_eq!(
        paths,
        vec![NASTY.to_string(), "/proj/plain.md".to_string()],
        "the same path nested under a split must round-trip identically, and must not disturb its \
         sibling"
    );
}

#[test]
fn rebuild_layout_spec_tolerates_a_non_finite_weight() {
    // `serde_json::Number::from_f64` has no representation for NaN or ±inf, so the
    // renderer substitutes `0.0` and stays TOTAL. The substitution is purely
    // cosmetic: `wolf_rebuild_layout` produces the window COUNT and TOPOLOGY only
    // and never reads `weights` (pane sizes come from the front-end). What matters
    // is that the DOCUMENT still parses — an emitted `null` (or a panic) would take
    // the whole restore down over a value nobody reads.
    let poisoned = PaneNode::Split {
        orientation: PaneOrientation::Vertical,
        weights: vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.5],
        children: vec![
            leaf("/proj/a.md"),
            leaf("/proj/b.md"),
            leaf("/proj/c.md"),
            leaf("/proj/d.md"),
        ],
    };

    let doc = parse_spec(&poisoned);
    let weights = doc
        .get("weights")
        .and_then(Value::as_array)
        .expect("the split still renders a `weights` array");
    let want_weights = vec![
        Value::from(0.0_f64),
        Value::from(0.0_f64),
        Value::from(0.0_f64),
        Value::from(0.5_f64),
    ];
    assert_eq!(
        weights, &want_weights,
        "each NON-FINITE weight must be emitted as 0.0 while the finite one is untouched. A `null` \
         in this array would change the document's SHAPE, and Lua would read `node.weights[1]` as \
         nil."
    );

    // The topology — the part that actually matters — is unaffected.
    let mut paths = Vec::new();
    spec_leaf_paths(&doc, &mut paths);
    assert_eq!(
        paths,
        vec![
            "/proj/a.md".to_string(),
            "/proj/b.md".to_string(),
            "/proj/c.md".to_string(),
            "/proj/d.md".to_string()
        ],
        "a poisoned weight must not cost the restore its topology: all four leaves still render, \
         in order, so `wolf_rebuild_layout` still returns four handles"
    );
}
