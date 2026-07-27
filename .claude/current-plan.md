# Plan: Persist & restore the open file for single-pane tabs

## Problem statement

When only one pane is open in a tab, the file open in that pane is not stored in
`state.json`, and on relaunch it is not reopened. Multi-pane tabs work correctly
because a tab's open buffer paths are persisted **only** through its pane-split
layout, and by design that layout is `nil`/`None` for a single unsplit pane.

### Reproduction
- Launch Wolf, reduce to a single window with a single pane that has a file open.
- Quit and relaunch.
- Actual: `state.json` records no open file; the file is not reopened.
- Expected: `state.json` records the open file; it is reopened on relaunch.

### Root cause (confirmed during investigation)

**Save side**
- `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift:1410` —
  `layoutSnapshot()` has `guard tree.leaves().count > 1 else { return nil }`, so a
  single pane produces no layout.
- `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift:569` — `snapshotNow()`
  only pushes leaf paths when `layoutSnapshot()` is non-nil. Otherwise only
  cwd + sidebar are recorded (`snapshot_add_tab`).
- Result: `TabState.layout` (`crates/wolf-core/src/state.rs:127`) stays `None`;
  `state.json` never records the single pane's buffer path.

**Restore side**
- `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift:208` —
  `if let saved = restoredTab?.layout, saved.leafCount > 1` ignores a single-leaf
  layout.

**Already works for us**
- The nvim rebuild helper `wolf_rebuild_layout` (`runtime/nvim/init.lua:938-951`)
  already handles a single leaf: it `:edit`s the file (or recreates a preview) in
  the tab's one window and returns `{ win }`.
- `TabState.layout` / `SavedPaneNode` already carry a `.leaf(path:)` shape, so no
  schema change is needed.

### Proposed fix (reuse the single-leaf layout; no schema change)
1. `layoutSnapshot()` — emit a single-`.leaf` snapshot for a lone pane too;
   return `nil` only when that leaf has an empty path and is not a preview (so
   genuinely empty-state tabs stay layout-less).
2. `build()` restore guard — allow a single-leaf layout (`leafCount >= 1`) to be
   rebuilt (which just opens the file).
3. Tests — add a real-nvim persistence test proving a single-pane tab round-trips
   its file path.
4. Docs — update comments/READMEs that say "None/nil for a single unsplit pane"
   (CLAUDE.md treats docs as source of truth).

## Architect's plan

### Approach decision
Reuse the single-leaf layout & `requestLayoutRebuild` for a single leaf — do NOT
special-case `handle.open(path)` (a `:edit` special-case would break the preview
single-pane case; the Lua `wolf_rebuild_layout` and `PaneTree.rebuilt` already
handle a lone leaf, including previews). No schema change; `STATE_VERSION` stays 1.

### Code changes
1. **`crates/wolf-macos/Sources/WolfKeyMap/PaneLayoutCodec.swift`** — add
   `public var isEmptyLeaf: Bool` on `SavedPaneNode`, `true` iff
   `.leaf(path: "", preview: false)` (a preview leaf or non-empty path is NOT
   empty; a `.split` is never empty). Pure/headlessly testable seam.
2. **`crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`** —
   `layoutSnapshot()` (~1407-1414): compute `leafPaths()` + `encode(...)` into
   `saved`, then `return saved.isEmptyLeaf ? nil : saved`. Emits `.leaf(path,false)`
   for a lone editor pane and `.leaf(sourcePath,true)` for a lone preview; suppresses
   ONLY a lone empty non-preview leaf (splits unaffected). Update doc comment.
3. **`crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift`** — `build()`
   (~206-210): relax guard from `saved.leafCount > 1` to just
   `if let saved = restoredTab?.layout {` → `requestLayoutRebuild(saved)`. Update
   comments (line 20, 206-207, 727).
4. **`crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift`** — `snapshotNow()`
   comment only (~567-568): no code change (line 569 guard still correct).

### Trace / safety (confirmed by architect)
- `onRenderWake()` single-leaf restore mirrors the multi-pane branch:
  `ensureInitialPane()` (no-op once a pane exists) then `rebuildLayout(from:saved)`
  which replaces tree/panes; `handle.rebuild_layout` returns exactly 1 handle,
  guard `handles.count == saved.leafCount` (1==1) passes. No bootstrap conflict.
- `rebuildLayout` fires `onLayoutChanged?()` → debounced re-snapshot; idempotent.

### Edge cases
- Preview lone pane → restored via `wolf_preview_file`, never `:edit`.
- Empty/unnamed lone buffer → `isEmptyLeaf` true → stays layout-less.
- Empty-state "no folder" tab (`hasFolder==false`) → lone unnamed buffer → `""` →
  `layoutSnapshot()==nil` → `has_layout()` stays false (critical invariant).
- Abs paths always persisted; restore `:edit`s abs path regardless of restored cwd.

### Tests (all target already-wired Bazel targets; confirm each actually runs)
- **4a** `crates/wolf-macos/Tests/WolfKeyMapTests/PaneLayoutCodecTests.swift`
  (wired `BUILD.bazel:256`): 4 methods for `isEmptyLeaf` — true for empty
  non-preview leaf; false for named leaf, empty PREVIEW leaf, and any `.split`.
- **4b** `crates/wolf-core/tests/rebuild_layout.rs` (wired
  `crates/wolf-core/BUILD.bazel:392`): 3 real-nvim fns modeled on existing ones,
  reusing `poll_win_filename`/`poll_preview_source`/`poll_win_filename_suffix`/
  `common::temp_md`/`start_core_sized`: `rebuild_single_leaf_editor`,
  `rebuild_single_leaf_preview`, `rebuild_single_empty_leaf`.
- **4c** `crates/wolf-ffi/tests/state.rs` (wired `crates/wolf-ffi/BUILD.bazel:864`):
  the target is deliberately ONE `#[test]` (mutates process-global state) — ADD a
  new step INSIDE `snapshot_builder_round_trips_the_whole_tree` (do NOT add a 2nd
  `#[test]`): push a lone `layout_push_leaf(SOLO_PATH,false,ROOT_WEIGHT)` +
  `commit_to_tab()` (no split), commit, reload, assert `has_layout()`,
  `layout().is_leaf()`, `!is_preview()`, `leaf_path()==SOLO_PATH`.

### Docs to reconcile (technical-writer; run `prettier --write` after .md edits)
- `crates/wolf-core/src/state.rs:119,127`; `crates/wolf-core/src/README.md:45`;
  `crates/wolf-core/README.md:182`; `crates/wolf-ffi/src/lib.rs:767-768,1420-1421`;
  `PaneContainerView.swift:1407-1409`; `WolfWindowController.swift:20,206-207,727`;
  `AppDelegate.swift:568`; `RestoreSeam.swift:35`; `crates/wolf-macos/README.md:95`;
  `crates/wolf-macos/Sources/README.md` (~line 21 & 60). New phrasing: "None/nil
  ONLY for an empty-state (unnamed, non-preview) lone pane; a lone pane with a file
  or preview persists a single leaf."

### Out of scope (flagged, do not implement)
Opening a file in an already-folder single-pane tab does not itself schedule a
snapshot; it's captured at the next trigger (reliably at clean quit — the repro
path). No new buffer-change snapshot trigger in this fix.

## Status
- Architect plan complete.
- Test-writer: added 4a (Swift `isEmptyLeaf` ×4), 4b (real-nvim ×3), 4c (FFI
  round-trip step inside existing test). No BUILD/source changes. All 3 files
  confirmed wired.
- Coder: implemented all 4 code changes. Builds green (bazelisk). Tests GREEN:
  WolfKeyMapTests ✓; rebuild_layout 6/6 (incl. 3 new) ✓; ffi:state ✓ (no-cache);
  ffi:multi_window ✓. Linters GREEN: swift_format, rustfmt_core, rustfmt_ffi,
  clippy. No test changes recommended.
- Reviewer: APPROVED. Predicate, save/restore wiring, single-leaf rebuild flow,
  no-loop, FFI builder, and regression scan all PASS. Two stale Swift doc-comments
  flagged for the doc pass: `WolfWindowController.swift:20` and `:727-728` (both
  already on the reconcile list). Note (b) empty-state-with-named-file edge is
  informational (consistent with pre-existing multi-pane-in-empty-state), no action.
- Technical-writer: reconciled all stale "None/nil for a single unsplit pane"
  claims (state.rs, lib.rs ×2, WolfWindowController.swift:20/727, RestoreSeam.swift,
  + 5 READMEs) to the new invariant, documented `isEmptyLeaf`, and added the 4 new
  Swift test methods to Tests/README.md. No schema version bump. No code/test logic
  changed.
- Orchestrator: ran `prettier --write` on all 5 edited READMEs (all clean/unchanged);
  re-verified swift_format + rustfmt_core + rustfmt_ffi PASS after the doc-comment
  edits.

## COMPLETE ✓
Fix implemented, tested (bazelisk), reviewed (APPROVED), documented, formatters green.
