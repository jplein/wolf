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

use tempfile::tempdir;
use wolf_core::state::STATE_FILE_NAME;
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
