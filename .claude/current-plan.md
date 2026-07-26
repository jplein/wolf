# Plan: macOS pane resize by mouse + full pane-layout persistence

## Status: COMPLETE ✅ (all green, reviewer-approved, docs synced) — UNCOMMITTED

Final state: WolfKeyMapTests + state_layout + state_unit + rebuild_layout (real nvim) +
ffi state + pane_status + bridge_smoke PASS; 62/62 regression; lint all exit 0;
//crates/wolf-macos:Wolf builds; reviewer APPROVED all areas A–F; all §6 READMEs +
architecture.md synced and prettier-clean (2 files reformatted). NOT COMMITTED — includes
regenerated crates/wolf-ffi/generated/wolf-ffi.{swift,h} which MUST be committed together
with the source change. Awaiting user direction on commit.

## Progress log

- [x] Architect plan complete (authoritative plan appended below).
- [x] Tests written (test-writer). Files: WolfKeyMapTests/{PaneTreeWeights,DividerHitTest,
      DividerDragSeam,PaneLayoutCodec}Tests.swift (new) + PaneTreeTests/RestoreSeamTests
      (modified); wolf-core/tests/{state_layout.rs (new, no-nvim), rebuild_layout.rs (new,
      real nvim)}; state_unit.rs + wolf-ffi/tests/state.rs (extended). Bazel: WolfKeyMapTests
      srcs += 4; wolf-core targets += state_layout, rebuild_layout.
- [x] Implementation + tests pass (coder). ALL GREEN: WolfKeyMapTests PASS; state_layout,
      state_unit, rebuild_layout (real nvim) PASS; ffi state + bridge_smoke PASS; 62/62
      regression; lint (swift_format, rustfmt_core, rustfmt_ffi, clippy) exit 0; bindings
      regenerated (UNCOMMITTED); //crates/wolf-macos:Wolf builds. Coder finalized:
      SavedPaneNode is Hashable (superset of Equatable, forced by PlannedTab: Hashable) +
      added SavedPaneNode.leafCount; PaneStatus.abs_path added in pane_status.rs.
- [x] Test hardening pass: test-writer added direct FfiPaneStatus.abs_path() assertion to
      //crates/wolf-ffi:pane_status; coder ran it → PASSED (1/1). abs_path = canonical
      buffer path, contract confirmed. SKIPPED #1 (FFI write→rebuild loop) — both halves
      already covered separately.
- [x] Review (reviewer) — APPROVED all areas A–F. Verified child-0-first DFS on BOTH Lua
      (`rightbelow vsplit`/`split`) and Swift (`PaneTree.rebuilt`) sides (load-bearing handle
      correspondence); weight preservation, min-clamp NaN/range safety, back-compat, no
      mouse-event leak, cursor-rect invalidation coverage all sound. Only outstanding item:
      docs sync. Non-blocking nit: drag uses same overhead for leading+trailing side of a
      horizontal pair (marginally conservative, never out-of-range) — acceptable per plan.
- [ ] Docs verified (technical-writer). Coder did NOT touch READMEs/architecture.md; several
      now describe old interfaces (e.g. PaneStatus fields in wolf-core/src/README.md).

## API contracts the coder MUST match (from test-writer)

- `PaneTree.Divider`: nested `public struct Divider: Equatable` with public stored props
  `orientation, path:[Int], boundary:Int, lineRect:Rect, pairStart, pairSpan, before:PaneId,
  after:PaneId, fraction` AND a public memberwise init (hit-test tests hand-build dividers).
- `lineRect` is ZERO-thickness: vertical → w==0, h==splitHeight, x==boundary x; horizontal →
  h==0, w==splitWidth, y==boundary y. Tolerance supplies clickable width.
- `pairStart`/`pairSpan` describe only the two adjacent children; `fraction ==
  w[boundary]/(w[boundary]+w[boundary+1])`; for two leaf children before/after are those ids.
- `DividerDragSeam.clampedLeftFraction(pairStart:pairSpan:pointerPos:orientation:cellWidth:
  cellHeight:minCols:minRows:leadingOverhead:trailingOverhead:) -> Double`, `minCols`/`minRows`
  are `Int`; per-side min px = minCells*cellSize + overhead; clamp `[minLeft/span, 1-minRight/
  span]`; degenerate → 0.5.
- `LayoutToken` internal shape untested (only postOrderTokens↔decodeTokens round-trip + token
  count == node count asserted) — struct-vs-enum is the coder's choice.
- `specJSON` uses `"v"`/`"h"` for orientation and JSON keys `leaf`/`path`/`orientation`/
  `weights`/`children` (matching `init.lua`'s `wolf_rebuild_layout`).
- `FfiPaneNode.orientation() -> u8` (0=horizontal,1=vertical); `weight(u32)`/`child(u32)`/
  `child_count()->u32`; `is_leaf`/`leaf_path`.
- `WolfCore::rebuild_layout(&str) -> Vec<i64>` returns handles in DFS/leaf order (positional
  correspondence is load-bearing + asserted).
- `wolf_core` re-exports `PaneNode`/`PaneOrientation` at crate root; `TabState`/`SavedTab`/
  `PlannedTab` gain a `layout` field (labeled `layout:` in inits).
- `rebuild_layout.rs` verifies per-window buffer via `grid_for_win`→`pane_status`; empty-path
  leaf → `[No Name]`.

## Feature request

Add the ability to resize panes on macOS by dragging the interior dividers with
the mouse, plus full pane-layout persistence.

## Confirmed decisions (from the user)

1. **Divider drag-to-resize**: dragging an interior divider between two adjacent
   panes reweights those two panes (transfers weight between the adjacent
   children of the owning split node). Show an appropriate resize cursor
   (`resizeLeftRight` / `resizeUpDown`) on hover over a divider.
2. **Minimum pane size when dragging**: 3 rows and 10 columns — clamp the drag so
   no pane goes below this.
3. **Full layout persistence**: persist each tab's ENTIRE pane-split tree
   (topology + per-child weights) into `state.json` (new schema in `wolf-core` +
   `wolf-ffi` projections + restore seam), and rebuild the nvim window topology +
   pane sizes on launch, so both splits AND their sizes survive quit/relaunch.

## Established architecture facts (docs are source of truth)

- Front-end `PaneTree` (`crates/wolf-macos/Sources/WolfKeyMap/PaneTree.swift`, a
  pure headlessly-tested port of `wolf-gnome/src/layout.rs`) is the macOS source
  of truth for pane layout. Splits are currently ALWAYS equal-weighted; there is
  no weight-mutation API; header explicitly defers divider drag-to-resize. It
  re-equalizes on every split/remove (`normalize()`).
- `PaneContainerView.swift` paints all panes in one flat `NSView` via
  `tree.paneRects(w:h:)`, draws interior dividers in `drawDividers()`, and pushes
  each pane rect -> nvim grid size via `syncGridSizes()` (converges, no feedback
  loop; layout is NOT taken from nvim). `mouseDown/Dragged/Up` currently forward
  clicks to nvim as cursor/selection events; there is no divider hit-testing.
- Today's persisted state (`wolf-core/src/state.rs`) is only
  `TabState { cwd, sidebar_visible }` per tab; split topology is NOT persisted.
  Restore is planned by `RestoreSeam.swift` (pure). `FfiWindowState` projects
  `wolf_core::WindowState`.
- Min pane size of 3 rows / 10 columns should map through `PaneGeometry` / grid
  cell sizes.

## Constraints

- Session persistence (weights surviving in-session splits/removes/tab-switches
  instead of re-equalizing) is a prerequisite of restart persistence and must
  also work.
- Prefer end-to-end/integration tests that start a real Neovim (bundled pinned
  nvim; build/test with **bazelisk**, not cargo).
- Keep pure seams (PaneTree, and any new pure persistence/rebuild seams)
  headlessly unit-tested.
- Every source-directory README must stay in sync with the implementation.

---

# Architect's plan (authoritative)

## 1. Summary

Add two capabilities to the macOS front-end. First, interior pane dividers become
draggable: hovering shows a resize cursor and dragging reweights the two adjacent
children of the owning split node, clamped so no pane's grid drops below 3 rows /
10 columns. Second, each tab's entire pane-split tree (topology + per-child
weights + per-leaf buffer path) is persisted into `state.json` and rebuilt on
launch, so splits and their sizes survive quit/relaunch. As a prerequisite, the
pure `PaneTree` stops re-equalizing on every split/remove and instead preserves
existing (user-resized) weights, so a resized layout survives in-session sibling
splits/removes and tab switches. The pure layout/geometry decisions stay in the
headlessly-tested `WolfKeyMap` seams; AppKit glue (`PaneContainerView`) and nvim
topology reconstruction are the only non-headless pieces.

Verified architecture facts:
- `PaneTree.swift` is pure, Foundation-only, `Double`-based; today `equalize()`s
  on every same-orientation split insert (`splitIn`, ~line 278) and after every
  remove (`normalize`, ~line 353). No weight-mutation API, no divider concept.
- `PaneContainerView` paints all panes in one flat `NSView`; `mouseDown/Dragged/Up`
  (~587-626) forward to nvim with no divider hit-testing; `drawDividers` (~1714)
  strokes 1px separators; `syncGridSizes` (~733) pushes each pane rect → nvim grid
  size. Splits applied from nvim intents in `applySplit` (~1253) via `tree.split`.
- Runtime splits: `runtime/nvim/init.lua` `_G.wolf_do_split(orient)` (~626) does the
  real `:split`/`:vsplit` and emits `wolf_layout{action='split',...}`; front-end
  builds `PaneTree` from these; nvim is NOT the layout source of truth.
- State: `wolf-core/src/state.rs` has `TabState{cwd, sidebar_visible}`,
  `WindowState{x,y,width,height,tabs}`, `WolfState{windows}`, on-disk
  `StateFile{version, windows}`, `STATE_VERSION = 1`; `load_state` strict-but-
  tolerant (no `deny_unknown_fields`). Write is whole-tree `StateStore::set_snapshot`.
- FFI: read side `FfiState`/`FfiWindowState`/`FfiTabState` opaque projections +
  `wolf_ffi_load_state`. Write side incremental scratch builder
  `wolf_ffi_state_snapshot_begin/add_window/add_tab/commit/flush`. macOS save is
  `AppDelegate.snapshotNow()` (~533); restore is `RestoreSeam.plan` →
  `materializeProjects` → `assembleWindows`. `RestoredTabState` carries only
  `{cwd, sidebarVisible}`.

## 2. New files

All new Swift seams import ONLY Foundation, live in
`crates/wolf-macos/Sources/WolfKeyMap/`, and MUST be added to the enumerated
`srcs` of `wolf_key_map` AND `WolfKeyMapTests` in `crates/wolf-macos/BUILD.bazel`.

- **`WolfKeyMap/DividerHitTest.swift`** — `public enum DividerHitTest` with
  `static func hit(_ dividers: [PaneTree.Divider], point: (x: Double, y: Double),
  tolerance: Double) -> PaneTree.Divider?` returning the nearest divider whose
  line is within `tolerance` (and within span), else `nil`. Enumeration lives on
  `PaneTree` (needs the private `Frame`); picking is pure over the public list.
- **`WolfKeyMap/DividerDragSeam.swift`** — `public enum DividerDragSeam` with
  `static func clampedLeftFraction(pairStart:pairSpan:pointerPos:orientation:
  cellWidth:cellHeight:minCols:minRows:leadingOverhead:trailingOverhead:) -> Double`.
  Raw fraction `(pointerPos-pairStart)/pairSpan`; per-side min pixels =
  `minCols*cellWidth+overhead` (vertical) / `minRows*cellHeight+overhead`
  (horizontal); clamp to `[minLeft/span, 1-minRight/span]`; degenerate too-small
  pair → midpoint (0.5). The clamp lives HERE (needs cell size + pixel spans),
  NOT in pixel-free `PaneTree`.
- **`WolfKeyMap/PaneLayoutCodec.swift`** — pure `PaneLayout`<->serializable value
  tree + post-order token emission for the FFI push-builder:
  - `public enum SavedPaneNode: Equatable { case leaf(path: String); case
    split(orientation: Orientation, weights: [Double], children: [SavedPaneNode]) }`
  - `enum PaneLayoutCodec`: `encode(_ layout:PaneLayout, paths:[PaneId:String]) ->
    SavedPaneNode`; `struct LayoutToken` + `postOrderTokens(_:) -> [LayoutToken]`
    (`.leaf(path,weight)` / `.split(orientation,childCount,weight)`, root weight
    1.0); `decodeTokens(_:) -> SavedPaneNode?` (inverse); `specJSON(_ node:
    SavedPaneNode) -> String` (single-String rebuild input, see §4.4).
- **Test files** (in `crates/wolf-macos/Tests/WolfKeyMapTests/`):
  `PaneTreeWeightsTests.swift`, `DividerHitTestTests.swift`,
  `DividerDragSeamTests.swift`, `PaneLayoutCodecTests.swift`.
- **`crates/wolf-core/tests/state_layout.rs`** — round-trip + back-compat (no
  nvim). New `rust_test` target `state_layout` in `crates/wolf-core/BUILD.bazel`.
- **`crates/wolf-core/tests/rebuild_layout.rs`** — real bundled nvim; new
  `rust_test` target `rebuild_layout`.

## 3. Front-end pure layer

### 3.1 `PaneTree.swift`
**Weight-policy change (intentional divergence from the gnome 1:1 port; TOPOLOGY
contract unchanged):**
- `splitIn` same-orientation branch (~275-278): instead of inserting `0.0` then
  `equalize`, carve the new pane's share from the TARGET child only —
  `weights[i]` and inserted `weights[i+1]` each = `weights[i]/2`; other children
  untouched. Fresh 2-child `split(...)` stays `0.5/0.5`.
- `normalize` (~322-357): after FLATTEN (keep proportional grandchild scaling) and
  1-child COLLAPSE, replace `equalize(&weights)` (~353) with proportional
  RE-NORMALIZE to sum 1 (`w[i] /= sum`). Removing a sibling redistributes
  proportionally, not equally.
- Keep `equalize` for the fresh-split default only (or inline).

**New public divider API:**
- `public struct Divider: Equatable { orientation: Orientation; path: [Int];
  boundary: Int; lineRect: Rect; pairStart: Double; pairSpan: Double; before:
  PaneId; after: PaneId; fraction: Double }` — `path` = child-index path root→
  owning split; `boundary` = index `i` between child `i` and `i+1`;
  `fraction = weights[i]/(weights[i]+weights[i+1])`.
- `public func dividers(width:height:) -> [Divider]` — recurse mirroring
  `collectRects` (~374); one Divider per interior boundary with the correct pixel
  line (`.vertical` → vertical line at boundary x over split y-range; `.horizontal`
  → horizontal line at boundary y).
- `public func setPairFraction(path:[Int], boundary:Int, leftFraction:Double)` —
  navigate to split at `path`; `pairSum = w[i]+w[i+1]`; clamp `leftFraction` to
  `[eps,1-eps]`; `w[i]=leftFraction*pairSum`, `w[i+1]=(1-leftFraction)*pairSum`.
  Global sum + other children unchanged. No-op on invalid path/boundary.
- `public static func rebuilt(from saved:SavedPaneNode, firstLeafId:PaneId) ->
  (tree:PaneTree, leafIds:[PaneId])` — build `Frame`s directly from `saved`
  (topology + weights), assign leaf ids sequentially in DFS order from
  `firstLeafId`, return tree + ordered leaf-id list (same DFS order as `leaves()`).

**Header comment (1-27, 38-39):** remove "always equal-weighted / not written
back / deferred" language; document the new API, weight-preserving split/remove
(fresh 2-child still 0.5/0.5), and the explicit gnome divergence (topology matches
exactly; only weight policy + divider API are macOS-only).

### 3.2 `PaneTreeTests.swift` (modify)
Update assertions that expected re-equalization after same-orientation split or
remove (now expect weight preservation). Keep all topological assertions.

### 3.3 `RestoreSeam.swift` (modify)
Extend `SavedTab`, `PlannedTab` with `public let layout: SavedPaneNode?` (threaded
through `plan` unchanged — grouping still by cwd; layout is passthrough). Update
inits + the `plan` construction (~122-140) to carry `tab.layout` into `PlannedTab`.

## 4. Core / FFI / nvim / macOS glue

### 4.1 `wolf-core/src/state.rs`
- `#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)] #[serde(rename_all=
  "snake_case")] pub enum PaneOrientation { Horizontal, Vertical }` (strings in JSON).
- `#[derive(...)] #[serde(tag="type", rename_all="snake_case")] pub enum PaneNode {
  Leaf { path: String }, Split { orientation: PaneOrientation, weights: Vec<f64>,
  children: Vec<PaneNode> } }`.
- On `TabState` (~68-74): `#[serde(default, skip_serializing_if="Option::is_none")]
  pub layout: Option<PaneNode>`; `Default` → `layout: None`.
- KEEP `STATE_VERSION = 1` (layout is `serde(default)`; old files load `layout:
  None`). A v2 bump would wrongly discard existing state — rejected.
- Re-export `PaneNode`/`PaneOrientation` at crate root. Update module + `TabState` docs.

### 4.2 `wolf-ffi/src/lib.rs` — WRITE side
Post-order push-builder (scalar/String-only bridge). In `extern "Rust"` (~724-739):
`wolf_ffi_state_layout_push_leaf(path:String, weight:f64)`,
`wolf_ffi_state_layout_push_split(orientation:u8, child_count:u32, weight:f64)`,
`wolf_ffi_state_layout_commit_to_tab()`. Impl uses a process-wide
`static LAYOUT_STACK: OnceLock<Mutex<Vec<(PaneNode,f64)>>>`: push_leaf pushes a
leaf; push_split pops the top `child_count`, wraps into a Split (orientation u8
0→Horizontal,1→Vertical), pushes; commit_to_tab pops the single remaining node
onto the scratch tree's LAST window's LAST tab `layout`, clears the stack.
Also: extend `FfiPaneStatus` with `abs_path(&self) -> String` (+ its `extern`
decl); if `wolf_core::PaneStatus` lacks the abs path, extend it + `WolfCore::
pane_status`. (Reuse `pane_status_for_grid`, the established grid→buffer join.)

### 4.3 `wolf-ffi/src/lib.rs` — READ side
Opaque `pub struct FfiPaneNode { node: PaneNode }` with `extern "Rust"`:
`is_leaf()->bool`, `leaf_path()->String`, `orientation()->u8`, `child_count()->
u32`, `weight(index:u32)->f64`, `child(index:u32)->FfiPaneNode` (recursion via
repeated opaque returns). Extend `FfiTabState` with `has_layout()->bool` +
`layout()->FfiPaneNode` (empty leaf when None) + decls; `FfiTabState::from_tab`
carries `t.layout.clone()`.

### 4.4 REBUILD API (`init.lua` + `nvim.rs` + `api.rs` + ffi)
- `runtime/nvim/init.lua`: `_G.wolf_rebuild_layout(spec)` where spec is nested
  `{leaf=true,path=".."}` / `{orientation="h"|"v",weights={...},children={...}}`.
  In the current tabpage (single starting window): realize topology via
  `:split`/`:vsplit` (create `child_count-1` windows per split, recurse), `:edit
  <fnameescape(path)>` for a non-empty leaf path else leave empty; collect
  resulting window handles in DFS order (SAME order as `PaneTree.leaves()`/
  `rebuilt`) and return list + count. nvim only produces the right WINDOW COUNT +
  TOPOLOGY + BUFFER per window; sizes come from front-end weights via `syncGridSizes`.
- `nvim.rs`: `fn rebuild_layout(&self, spec_json:&str) -> RpcFuture` invoking the
  Lua helper (pass a JSON string the Lua side `vim.json.decode`s), returning the
  ordered handles.
- `api.rs`: `pub fn rebuild_layout(&self, spec_json:&str) -> Vec<i64>` via
  `block_on_rpc` (bounded, like `create_tabpage` ~699); `vec![]` on error.
- `wolf-ffi`: `pub fn rebuild_layout(&self, spec_json:String) -> Vec<i64>` on
  `WolfCoreHandle` + `extern` decl. Swift builds `spec_json` via
  `PaneLayoutCodec.specJSON`.

**Regenerate + commit bindings after ALL bridge changes:**
`bazelisk run //crates/wolf-ffi:gen_bindings` → commit
`crates/wolf-ffi/generated/wolf-ffi.swift` + `.h`.

### 4.5 macOS glue
**`PaneContainerView.swift`:**
- State: `private var activeDivider: PaneTree.Divider?`; `var onLayoutChanged:
  (() -> Void)?` (→ `coordinator.scheduleStateSnapshot`).
- `private func editorDividers() -> [PaneTree.Divider]` = `tree.dividers(width:
  Double(bounds.width), height: dl.editorHeight)` (same `DrawerLayout` as
  `paneRects()` ~375-388), view coords. Editor tree only (drawer is outside `tree`).
- `mouseDown`: BEFORE the pane-forward loop (~589), `DividerHitTest.hit(
  editorDividers(), point:, tolerance:~4pt)`. On hit: set `activeDivider`, DO NOT
  `focus_win`/`send_mouse`, return. Else existing behavior.
- `mouseDragged`: if `activeDivider != nil`, compute clamped fraction via
  `DividerDragSeam.clampedLeftFraction(...)` (`pairStart`/`pairSpan` from divider,
  `pointerPos` along axis, `cellWidth/Height` from `metrics`, `minCols:10`,
  `minRows:3`, overheads from `EditorInset.padding` + statusbar height), call
  `tree.setPairFraction(...)`, `syncGridSizes()`, invalidate cursor rects,
  `needsDisplay=true`, RETURN. Else existing.
- `mouseUp`: if `activeDivider != nil`, clear it, `onLayoutChanged?()`, return.
  Else existing.
- Constants: `dividerHitTolerance: CGFloat = 4`, `minPaneCols = 10`, `minPaneRows = 3`.
- Cursor feedback: override `resetCursorRects()` to `addCursorRect` (thin rect
  around each divider's `lineRect` widened by tolerance) with
  `.resizeLeftRight` (vertical) / `.resizeUpDown` (horizontal). Call
  `window?.invalidateCursorRects(for: self)` at end of `syncGridSizes()`,
  `setFrameSize`, `viewDidEndLiveResize`, and after `applyIntents()`/
  `detectCloses()` in `onRenderWake`.
- Save capture: `func layoutSnapshot() -> SavedPaneNode?` =
  `PaneLayoutCodec.encode(tree.layout(), paths: leafPaths())` where `leafPaths()`
  maps each editor leaf → bound grid → `handle.pane_status_for_grid(grid).
  abs_path()` (drawer excluded); return `nil` for a single unsplit leaf.
- Restore rebuild: `private var pendingLayoutRebuild: SavedPaneNode?`;
  `func requestLayoutRebuild(_ saved: SavedPaneNode)`. In `onRenderWake` (after
  `ensureInitialPane`, gated like `pendingRefocusRebind` ~790-804): consume once —
  `handles = handle.rebuild_layout(specJSON)`; guard `handles.count ==
  saved.leafCount` (else fall back to single pane + log); `(newTree, leafIds) =
  PaneTree.rebuilt(from: saved, firstLeafId: nextPaneId)`; replace `tree` (make it
  `var` + internal `replaceTree(_:)`, currently `let` ~152), advance `nextPaneId`,
  rebuild `panes` binding `leafIds[i]`→`PaneBinding(win: handles[i], boundGrid:
  nil)`, set `activePane`; weights come from `rebuilt` (no separate apply);
  `resolvePendingGrids()`, `syncGridSizes()`, invalidate cursor rects,
  `needsDisplay=true`.
- Header (16-19): replace "deferred / equal-weighted" with the hit-test→drag→
  reweight→syncGridSizes flow + cursor-rect feedback.

**`WolfWindowController.swift`:** extend `RestoredTabState` with `layout:
SavedPaneNode?`; in `build()` (~189) after creating `paneContainer` (~196) set
`onLayoutChanged` → `coordinator.scheduleStateSnapshot()`, and if
`restoredTab?.layout` is a multi-leaf split call `paneContainer.requestLayoutRebuild
(...)`; expose `func layoutSnapshot() -> SavedPaneNode?`.

**`AppDelegate.swift`:** `snapshotNow()` (~533) after
`wolf_ffi_state_snapshot_add_tab(...)` (~564) emits `PaneLayoutCodec.postOrderTokens`
into `push_leaf`/`push_split` then `commit_to_tab()` when `layoutSnapshot()` is
non-nil. `restoredGroups(from:)` (~699) reads `has_layout()`/`layout()` +
`FfiPaneNode` into `SavedPaneNode?` → `RestoredTabState`. `savedWindows(from:)`
(~686) carries `layout` into `RestoreSeam.SavedTab`. `materializeProjects` (~157)
passes planned tab `layout` into `RestoredTabState` (~184).

**`StateSeam.swift`:** no change (launch decision cwd-only).

## 5. Test plan (see architect detail; test-writer implements)
Pure Swift: `PaneTreeWeightsTests` (setPairFraction transfer/clamp/n-ary,
weight-preservation on split+remove, topology preserved), `DividerHitTestTests`
(within/beyond tolerance, nearest at intersection, empty→nil),
`DividerDragSeamTests` (middle passthrough, clamp both sides both orientations,
overhead, degenerate midpoint), `PaneLayoutCodecTests` (encode paths, token
round-trip, `rebuilt` topology+weights+DFS ids, specJSON shape). Modify
`PaneTreeTests` (preserve policy) + `RestoreSeamTests` (layout passthrough, cwd
grouping unaffected).
Rust: `state_layout.rs` (no nvim: snapshot→flush→load_state equality; v1-without-
layout back-compat → None; unknown-field tolerance; orientation string shape);
`wolf-ffi/tests/state.rs` modify (drive push-builder → load → FfiPaneNode
round-trip); `rebuild_layout.rs` (real nvim: handle count == leaf count, distinct
handles, per-window buffer path matches, empty-path leaf → unnamed buffer).
macOS-only manual-smoke (state in Tests READMEs): AppKit resize cursor on hover,
live drag visuals, end-to-end quit→relaunch restore.

## 6. Docs to update (prettier --write after each)
`PaneTree.swift` header, `PaneContainerView.swift` header,
`crates/wolf-macos/Sources/README.md`, `crates/wolf-macos/README.md`,
`crates/wolf-core/src/README.md` + `crates/wolf-core/README.md`,
`crates/wolf-ffi/README.md` + `crates/wolf-ffi/src/README.md`,
`crates/wolf-macos/Tests/README.md` + `crates/wolf-core/tests/README.md` +
`crates/wolf-ffi/tests/README.md`, `runtime/nvim/README.md` (if present),
`architecture.md` (pane-layout/persistence sections).

## 7. Implementation order
1. Pure `PaneTree` changes + `DividerHitTest` + `DividerDragSeam` +
   `PaneLayoutCodec` (+ BUILD srcs) + `RestoreSeam` passthrough (Foundation-only).
2. Pure Swift tests for step 1; update `PaneTreeTests`/`RestoreSeamTests`.
3. `PaneContainerView` drag + cursor rects + `layoutSnapshot`/`requestLayoutRebuild`
   + `onLayoutChanged`.
4. Core `state.rs` schema + `state_layout.rs`.
5. FFI read/write projections + rebuild API + regen/commit bindings; extend
   `wolf-ffi/tests/state.rs`; add `rebuild_layout.rs`.
6. macOS save path (`snapshotNow`, `layoutSnapshot`).
7. macOS restore/rebuild path (`RestoredTabState.layout` threading,
   `requestLayoutRebuild`, `onRenderWake` one-shot).
8. Docs + `prettier --write`.

## 8-11. Deps / lint / risks / commands
- No new deps. New WolfKeyMap seams import ONLY Foundation. FFI accessors
  `#[must_use]` + `///` docs. Don't hand-edit committed `generated/*`.
- Risks: (1) nvim topology rebuild — DFS handle order must match both sides; gate
  behind initial-pane-bound + tabpage-current; guard count mismatch → fall back to
  single pane; topology-first then `:edit` so a bad path degrades to empty window;
  sizes always from front-end weights. (2) weight-policy divergence — topology
  unchanged, dedicated tests + header note. (3) back-compat — keep version 1 +
  `serde(default)`. (4) stale cursor rects — invalidate on every layout mutation.
  (5) stray nvim mouse events during drag — hit-test BEFORE pane-forward, early
  return in all three handlers. (6) bridge fragility — no Vec/recursive-struct
  args; regen + commit; smoke/state tests guard.
- Commands (bazelisk): `run //crates/wolf-ffi:gen_bindings` (+commit generated);
  `build //crates/wolf-macos:Wolf`; `test //crates/wolf-macos:WolfKeyMapTests`;
  `test //crates/wolf-core:state_layout //crates/wolf-core:rebuild_layout` (+
  `//crates/wolf-core/...`); `test //crates/wolf-ffi:state //crates/wolf-ffi:
  bridge_smoke`; lint `test //tools/lint/...`.
