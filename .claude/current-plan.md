# Plan: macOS pane resize by mouse + full pane-layout persistence

# MERGE FROM MAIN (user-requested) — verification in progress

Merged `origin/main` (9 commits: Neovim crash/stall handling + `:q`/`:qa`→close pane/window +
test-wiring doc updates) into `feat/macos-pane-dividers`. Conflicts resolved (9 files):
- `.claude/current-plan.md` → took ours.
- `crates/wolf-ffi/src/lib.rs` → CODE: `use` import union (ours PaneNode/PaneOrientation +
  main Liveness/TabFiles).
- 7 doc files (architecture.md, wolf-core/src+tests README, wolf-ffi/src README, wolf-macos
  README + Sources README, runtime/nvim README) → merged BOTH features' prose (kept our pane
  divider/persistence/preview text AND injected main's crash-stall/liveness/FfiOpenFiles text).
No markers remain; prettier clean. NOT yet committed (merge in progress).
CRITICAL: generated `wolf-ffi.{swift,h}` were TEXT-auto-merged; both sides changed the bridge
surface → MUST regenerate before committing. Verification pass (coder): regen bindings, build,
FULL test suite (incl. main's new nvim_liveness/nvim_restart/swap_recovery), lint.

- [x] Verify (coder): regen bindings (byte-identical to auto-merge → auto-merge was correct);
      fixed 2 real merge-seam breaks (our `rebuild_layout` vs main's `block_on_rpc` Send+'static
      + `self.nvim` RefCell change → mirror `create_tabpage`) + rustfmt drift. App builds; 72/72
      Rust/FFI tests (BOTH families: ours state_layout/rebuild_layout/ffi state/pane_status +
      main's nvim_liveness/nvim_restart/swap_recovery); WolfKeyMapTests; all 4 lint targets GREEN.
- [x] Review (reviewer): MERGE RESOLUTION SOUND — all 9 resolutions faithful unions, no dropped/
      duplicated content, no marker residue, obsolete "divider deferred" text gone everywhere,
      both rebuild_layout code fixes correct. Fixed 1 flagged pre-existing doc drift (wolf-macos
      README layoutSnapshot round-1→round-2 signature).
- [x] Merge committed.

---

# ROUND 2 (user follow-up, committed round 1)

## Status: ROUND 2 COMPLETE ✅ (all green, reviewer-approved, docs synced) — UNCOMMITTED

Round-2 final: all targets green; reviewer APPROVED (A–D); docs synced + prettier-clean
(Sources/README.md reformatted). UNCOMMITTED, includes regenerated wolf-ffi bindings
(push_leaf preview arg + FfiPaneNode$is_preview) that MUST be committed together.

Round-2 progress:
- [x] Architect plan (authoritative round-2 plan appended below).
- [x] Tests (test-writer). Modified: PaneLayoutCodecTests, PaneTreeWeightsTests, PaneTreeTests,
      RestoreSeamTests, state_layout.rs, wolf-ffi/tests/state.rs, rebuild_layout.rs. Kept the
      FULL preview-binding assertion in rebuild_layout (basename-matched via previews(), bounded
      by FLUSH_TIMEOUT — preview.rs proves the same helper is reliable). AUDIT found + flipped
      TWO extra carve-weight cases beyond the named list: testSetPairFractionNaryTouchesOnlyThePair
      + testDividersForFlatRow (both now thirds). API shapes the coder must match are recorded
      in the round-2 plan's tests notes.
- [x] Implementation + tests pass (coder). ALL GREEN: WolfKeyMapTests, state_layout,
      rebuild_layout, ffi state/bridge_smoke/pane_status PASS; 62/62 regression; lint (clippy,
      rustfmt_core, rustfmt_ffi, swift_format) PASS; bindings regenerated (UNCOMMITTED, carry
      push_leaf(path,preview,weight) + FfiPaneNode$is_preview); //crates/wolf-macos:Wolf builds.
      No test edits needed. Docs NOT touched (technical-writer step).
- [x] Review (reviewer) — APPROVED all areas A–D. DFS/preview-slot correspondence verified
      (leaf branch always returns { win }; wolf_preview_file only nvim_win_set_buf, no create/
      destroy). Back-compat, deleted-source graceful, rebuilt-ignores-split-policy all confirmed.
      Flagged stale doc: PaneTree.swift:367 splitIn free-helper comment still says "carving its
      share from the target child" (contradicts code — must flip to re-equalize).
- [ ] Docs (technical-writer). Stale: PaneTree.swift:367 splitIn comment; Sources/README
      (layoutSnapshot signature paths:previews: + split policy); wolf-core/src/README (+preview
      field); wolf-ffi READMEs (+is_preview, push_leaf preview arg); PaneTree/PaneContainerView
      headers; runtime/nvim/README (preview rebuild); Tests READMEs; architecture.md (split
      re-equalizes; previews persist).
- [x] Docs (technical-writer) — DONE. Fixed PaneTree.swift:367 comment + 10 READMEs/
      architecture.md; prettier reformatted Sources/README.md (rest clean).

Round 1 was COMMITTED by the user. Two follow-up items:

1. **BUG — preview panes not restored.** On restore, a preview pane comes back as a
   blank editor. Cause: `leafPaths()` saves each leaf via `pane_status_for_grid(grid).
   abs_path()`; a preview pane's bound buffer is a preview SCRATCH buffer with no real
   file path (→ `""`), so `wolf_rebuild_layout`'s `:edit <path>` yields an empty buffer
   and the preview-ness is lost. Core tracks previews via `previews()` →
   `FfiPreview{ win, source_bufnr, source_name, source_path (ABS), source_tick }`;
   `preview_file(win, path)` repoints a preview window, `open_preview(mode)` creates one.
   FIX shape: mark a leaf as a preview in the schema + carry its SOURCE abs path; on
   rebuild, recreate a preview of that source in the target window instead of `:edit`.
2. **CHANGE — splits re-equalize.** Per user: `:vsplit`/`:split` should re-distribute
   the row/column to (approximately) equal widths again (e.g. an unequal 2-pane row
   `|--|-------|` → 3 EQUAL panes on vsplit). This REVERTS the same-orientation split
   weight policy from round 1 (carve-from-target) back to `equalize`. Drag-to-resize and
   full persistence are UNCHANGED. DECISION (orchestrator default, stated to user):
   REMOVE stays PROPORTIONAL (survivors keep dragged relative sizes); only SPLIT
   equalizes. Persistence/restore uses `PaneTree.rebuilt` from saved weights, unaffected.

---

# ROUND 2 — Architect's plan (authoritative)

## ITEM 1 — Preview panes not restored

**1.1 Schema (`wolf-core/src/state.rs`):** extend `PaneNode::Leaf` to
`Leaf { path: String, #[serde(default)] preview: bool }`. For a preview leaf, `path` =
the SOURCE Markdown file's ABS path, `preview = true`. `#[serde(default)]` → round-1
files load `preview=false`; STATE_VERSION stays 1. Update `PaneNode` doc. Decision:
extend `Leaf` (not a new variant) — smaller serde/match surface.

**1.2 FFI write (`wolf-ffi/src/lib.rs`, bridge change → regen):**
`wolf_ffi_state_layout_push_leaf(path: String, preview: bool, weight: f64)` (insert
`preview` between path and weight); impl pushes `PaneNode::Leaf { path, preview }`.

**1.3 FFI read (bridge change → regen):** add `FfiPaneNode.is_preview() -> bool`
(`Leaf{preview,..}=>preview`, `Split=>false`) + extern decl + `#[must_use]`/doc. Fix the
two empty-leaf fallbacks (`FfiTabState::layout` + `FfiPaneNode::child`) to add
`preview: false`. `leaf_path()` unchanged (returns the source path for a preview leaf).

**1.4 nvim rebuild (`runtime/nvim/init.lua` `_G.wolf_rebuild_layout`), CHOSEN = option (b):**
in the leaf branch, if `node.preview` and non-empty `node.path` → `wolf_preview_file(win,
node.path)` (it does bufadd/bufload/markdown-gate + `nvim_win_set_buf(win, preview_buf)`,
converting ANY window into a preview — no pre-existing preview window needed, window handle
unchanged → DFS correspondence intact); elseif non-empty path → `:edit fnameescape(path)`;
else keep inherited unnamed buffer. Failure (missing/non-md/empty source) → 0 → blank pane,
handle still returned. NO `nvim.rs`/`api.rs`/`WolfCore::rebuild_layout` change (spec JSON
flows through untouched). Update the helper header for the `{leaf,path,preview}` shape.

**1.5 Swift codec (`WolfKeyMap/PaneLayoutCodec.swift`):**
`SavedPaneNode.leaf(path:String, preview:Bool)` (stays Hashable);
`LayoutToken.leaf(path:,preview:,weight:)`;
`encode(_ layout:PaneLayout, paths:[PaneId:String], previews:Set<PaneId>)` — leaf →
`.leaf(path: paths[id] ?? "", preview: previews.contains(id))` (explicit param, no default);
update `emit`, `decodeTokens`, `jsonObject` (leaf → `["leaf":true,"path":path,
"preview":preview]`). Update file header.

**1.6 Save path (`PaneContainerView.swift`):** `layoutSnapshot()` passes a `previews`
set; rework `leafPaths()` into one `handle.previews()` scan building
`previewByWin: [Int64:String]` = `preview.win()` → `preview.source_path().toString()`
(ABS source). For each `id in tree.leaves()`: if `panes[id]?.win` ∈ previewByWin →
`paths[id]=source path` + insert into `previewIds`; else editor → `pane_status_for_grid(
grid).abs_path()`. Return `(paths, previewIds)`. Match on `.win` (previews keyed by window,
like syncPreviewWebViews). Update doc comments.

**1.7 Restore wiring (`AppDelegate.swift`):** snapshot push arm →
`case .leaf(let path, let preview, let weight): wolf_ffi_state_layout_push_leaf(path,
preview, weight)`; `savedPaneNode(from:)` → `.leaf(path: node.leaf_path().toString(),
preview: node.is_preview())`. NO change to restoredGroups/RestoredTabState/
WolfWindowController/RestoreSeam/rebuildLayout/PaneTree.rebuilt (marker rides in
SavedPaneNode; rebuilt ignores leaf payload).

**1.8 WKWebView:** no extra work — `wolf_preview_file` re-emits previews, so
`syncPreviewWebViews`/`previewNamesByGrid` (onRenderWake) hosts the web view automatically.

**1.9 `.leaf` sites to update:** PaneLayoutCodec (case/token/encode/emit/decode/json),
AppDelegate 572 + 742, RestoreSeamTests, PaneLayoutCodecTests. `PaneTree.swift` `.leaf`
sites operate on internal Frame/PaneLayout/PaneId — UNCHANGED (rebuilt's `case .leaf:`
binds nothing, still matches).

## ITEM 2 — `:vsplit`/`:split` re-equalize

**2.1 `PaneTree.swift`:** RE-ADD private `equalize(_ weights: inout [Double])` (1/n each,
guard empty). Revert `splitIn` same-orientation branch to: insert `.leaf(newPane)` at
`i+1`, insert weight `0.0`, `equalize(&weights)`. Keep perpendicular NEST at 0.5/0.5; keep
root fresh two-child split 0.5/0.5; **do NOT touch `normalize`** — REMOVE stays PROPORTIONAL.

**2.2 Docs:** `splitIn` doc → "re-equalizes the row/column"; header "Intentional divergence"
rewrite → same-orientation split RE-EQUALIZES (matches gnome); fresh 2-child 0.5/0.5; REMOVE
proportional (the ONLY remaining gnome divergence); dividers/setPairFraction/rebuilt
unchanged. PaneContainerView header if it mentions split policy.

## TESTS
**Item 1 add/modify:** `PaneLayoutCodecTests` — all `.leaf(path:)` → `.leaf(path:,preview:)`
(false); add encode-with-previews-set, preview token round-trip, specJSON `"preview"` key.
`state_layout.rs` — `leaf` helper adds `preview:false`, add `preview_leaf`, preview round-trip
(preserves true), v1-without-preview back-compat → false, serialized `"preview":true` shape.
`wolf-ffi/tests/state.rs` — existing push_leaf calls add `preview` arg; add a preview-leaf
case asserting `is_preview()` true/false + `leaf_path()` == source. `rebuild_layout.rs`
(real nvim) — add a preview-leaf case (editor+preview of a bundled md): handle count 2,
distinct, preview window reports expected `source_path` via `previews()`; REDUCED-SCOPE
fallback (handle count/topology only) allowed if preview snapshot not observable in timeout
— flag to test-writer. Save-path preview detection is AppKit → manual-smoke.

**Item 2 MODIFY (weights flip; SETUPS that used carve-splits to make unequal weights must
switch to `setPairFraction` drags):**
- `PaneTreeWeightsTests.testSameOrientationSplitCarvesFromTargetOnly` → FLIP to equal thirds,
  rename `...ReEqualizesRow` (prior 0.8/0.2 drag discarded).
- `...testSplittingOneSiblingKeepsTheResizedSiblingsWeight` → FLIP to equal thirds, rename
  `testSplittingSiblingReEqualizesRow` (P1 not 0.8).
- `...testRemoveRenormalizesProportionallyNotEqually` → KEEP proportional assertion, REWRITE
  setup via drag: split p1/p2, split p1/p4 (equal thirds), `setPairFraction([],0,0.75)`
  (P1=0.5,P4=1/6,P2=1/3), remove p4 → [0.6,0.4]; reject 50/50.
- `...testCollapseKeepsParentWeightNotEqual` → UNCHANGED (perpendicular nest, 0.8 survives).
- `PaneTreeTests.testVsplitFlattensIntoRow` → FLIP to equal thirds (w/3 each), update comment.
- `...testVsplitTwiceStaysFlatWeightsPreserved` → FLIP to four equal quarters, rename
  `...EqualWeights`, rewrite weight comments.
- `...testRemoveProportionallyRenormalizesRow` → KEEP proportional, REWRITE setup via drag:
  split p1/p2, split p1/p3 (thirds), `setPairFraction([],1,0.25)` (P3=1/6,P2=1/2,P1=1/3),
  remove p3 → [0.4,0.6]; keep "NOT re-equalized".
- `...testRemoveCollapsesSingleChild`, perpendicular/neighbor/divider tests → UNCHANGED.
- **AUDIT DIRECTIVE:** grep both PaneTree test files for any remaining assertion expecting a
  same-orientation split to yield non-equal weights, or a pre-split drag to survive a
  same-orientation split — flip to equal or rebuild the unequal precondition via
  `setPairFraction`.
- `RestoreSeamTests` `.leaf(path:)` → add `preview:false`.
No new Bazel targets (all extend existing files).

## ORDER (avoid compile breaks)
1. Item 2 PaneTree revert + 6 test flips → build WolfKeyMapTests.
2. Item 1 pure Swift codec (`SavedPaneNode.leaf` breaks all sites — fix AppDelegate 572/742,
   RestoreSeamTests, PaneLayoutCodecTests same pass; do NOT wire new FFI arg yet).
3. Item 1 core state.rs + state_layout.rs.
4. Item 1 FFI (push_leaf preview + is_preview + empty-leaf fallbacks) + AppDelegate wiring →
   REGEN + commit bindings → extend wolf-ffi/tests/state.rs.
5. Item 1 nvim init.lua leaf branch + PaneContainerView save path → extend rebuild_layout.rs.
6. Docs + prettier.

## RISKS
Bridge change needs regenerated committed bindings (sequence step 4 as one unit). Split-
equalize breaks test SETUPS not just assertions (rebuild unequal preconditions via drags).
DFS handle correspondence intact under option (b) (preview_file binds buffer, no win create/
destroy). Deleted source → preview_file 0 → blank pane (graceful, handle retained). Shared
source editor+preview leaf → bufadd returns same bufnr (no conflict). Back-compat: round-1
files load preview=false, no version bump.

## COMMANDS (bazelisk)
`run //crates/wolf-ffi:gen_bindings` (+commit generated .swift/.h after step 4);
`build //crates/wolf-macos:Wolf`; `test //crates/wolf-macos:WolfKeyMapTests //crates/
wolf-core:state_layout //crates/wolf-core:rebuild_layout //crates/wolf-ffi:state //crates/
wolf-ffi:bridge_smoke //crates/wolf-ffi:pane_status` + `//crates/wolf-core/...`;
lint `test //tools/lint/...`.

---

# ROUND 1 — COMPLETE ✅ (committed by user)

## Status: COMPLETE ✅ (all green, reviewer-approved, docs synced) — COMMITTED

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
