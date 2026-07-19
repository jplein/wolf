# Plan: Stop painting pane grids occluded by preview web views

## Problem statement

Wolf's typing latency is far worse than terminal Neovim. Frame-time instrumentation
(landed in the previous pass, see "Instrumentation" below) shows `PaneContainerView.draw`
is the dominant main-thread cost in every configuration measured. This pass targets one
specific, cheap hypothesis about why `draw` gets dramatically worse when a Markdown
preview is open.

## Measured data (real captures, this machine — a FAST Mac)

Large file (`~/Git/jplein/Notepad/commonplace-book.md`), typing near the top.
Steady-state 5 s windows, from `test-1-no-preview` and `test-2-with-preview` in the repo root.

| metric              | no preview            | with preview          |
| ------------------- | --------------------- | --------------------- |
| `draw`              | 77 x 26.2 ms = 2002 ms | 56 x 47.6 ms = 2663 ms |
| `prev.reload`       | —                     | 31 x 30.8 ms = 956 ms  |
| `prev.render`       | —                     | 31 x 28.4 ms = 880 ms  |
| `prev.html`         | —                     | 31 x 2.4 ms = 74 ms    |
| wakes               | 135 x 4.2 ms = 560 ms  | 159 x 4.8 ms = 768 ms  |
| **main-thread busy** | **~2.6 s / 5 s (51%)** | **~4.4 s / 5 s (88%)** |
| per keystroke       | ~55 ms                | ~81 ms                |
| wakes/key           | ~2.9–3.35             | ~2.7–2.9              |

`draw` is 78% of busy time without a preview and 61% with one. Nothing else is close.

### Corrections to earlier conclusions (both were WRONG — do not reuse them)

1. **Wake amplification is REAL.** An earlier 22-keystroke smoke capture showed
   `wakes/key = 1.00` and this was wrongly recorded as falsifying the hypothesis. Both
   real captures show **~2.9**. The sample was too small to conclude from.
2. **`loadHTMLString` is NOT the preview problem.** `prev.html` is only **2.4 ms**. The
   previously-planned "replace `loadHTMLString` with incremental DOM patching" work is
   **CANCELLED** — it would buy ~2 ms. The preview cost is `prev.render` (28 ms), the
   `block_on` RPC + whole-document `pulldown-cmark` render.

## THE HYPOTHESIS TO TEST IN THIS PASS

`draw` nearly DOUBLES when a preview opens: **26.2 ms -> 47.6 ms** — even though the
editor pane becomes NARROWER and should therefore paint FEWER cells.

**Primary hypothesis (occlusion):** `PaneContainerView.draw` paints every pane's grid,
including the preview pane's nvim scratch-buffer grid, which is then entirely hidden
behind the `WKWebView` stacked on top of that pane's rect. That is ~20 ms per frame spent
rasterizing cells no user can ever see.

**Secondary hypothesis (compositing): FALSIFIED — the claim below was factually wrong.**

~~`PaneContainerView` is the only view in the tree that never sets `wantsLayer`/`isOpaque`.~~
**`PaneContainerView.init:275` ALREADY sets `wantsLayer = true`**, with an explanatory
comment. Independently, `FrameMetrics.end(.draw, ...)` is a `defer` inside `drawRect`
(`:1152-1153`), so the `draw` bucket measures ONLY the drawing callback body — layer
compositing against the `WKWebView`'s layer happens later in the CA commit and window
server, entirely outside the measured span, and therefore cannot inflate the `draw` mean.
`isOpaque` is still not overridden (defaults `false`), but it is NOT the cause; do not
bundle it into this pass.

**Third hypothesis (cost moved to the editor pane): PARTIALLY TRUE, and it repairs a
false premise above.** `maxGridCols = 90` (`:75`) caps and centers the grid, so on a wide
window the editor pane is ALREADY at the cap; splitting for the preview halves its pixel
width but its column count lands just under the cap. **The editor pane does not get
cheaper.** A second, similarly-sized grid then appears → predicted ~2x = ~52 ms vs
measured 47.6 ms. Good fit, accounts for essentially the whole delta.

**The `draw` count drop (77 -> 56 per 5 s) is NOT more invalidation.** Every path sets
`needsDisplay = true` and AppKit coalesces to at most one display pass per run-loop
iteration; at 88% busy instead of 51%, more wakes fold into each pass. Fewer draws of
unchanged cost. So the honest headline is the PRODUCT: total `draw` time went
2002 ms -> 2663 ms (+33%), not +82%.

## Scope of THIS pass

1. **Verify the hypothesis** using the existing instrumentation, cheaply and explicitly.
2. **If occlusion is confirmed:** skip painting grid content for any pane fully covered by
   an opaque preview web view.
3. **Re-measure** and record the before/after `draw` numbers.

Deliberately NOT in this pass (all still queued, in priority order):

- glyph run coalescing / glyph cache / `drawGlyph` rework, and the redundant
  `read_frame_for_grid` calls + triple cell-vector clone (this is the real fix for the
  26 ms no-preview floor — the NEXT pass)
- moving `prev.render` off the main thread (third)
- wake amplification, ~2.9 wakes/key at ~4.8 ms each (fourth)
- dirty-rect tracking in `draw`
- anything in Rust (`wolf-core` / `wolf-ffi`). No Rust changes; no FFI surface changes.

## Correctness constraints (this pass DOES change render behavior)

Unlike the instrumentation pass, skipping paint work is a real behavior change. It must be
correct in every case, not just the common one:

- Only skip when the web view genuinely, fully covers the pane's grid rect AND is opaque.
  A partially-covering or transparent web view must still paint underneath.
- A preview pane whose `WKWebView` does not exist yet (`previewWebViews[win] == nil`, the
  documented "web view does not exist yet" path in `reloadDirtyPreviews`) must still paint.
- A web view that exists but has not yet loaded content (blank/white) must not produce a
  visibly empty or garbage pane.
- Hidden / zero-size / detached web views must not count as occluding.
- Pane geometry changes (split, resize, close, preview close) must restore painting
  immediately — no stale "occluded" state surviving the web view's removal.
- Cursor, scrollbars, dividers and overlays must be unaffected except where they are
  themselves genuinely occluded.

## Instrumentation available (from the previous pass — already landed)

Env-gated, off by default. `WOLF_FRAME_STATS=1`, `WOLF_FRAME_STATS_INTERVAL=<seconds>`,
`WOLF_FRAME_STATS_FILE=<path>`. Buckets: `ctrl.wake`, `cont.wake`, `draw`, `prev.reload`,
`prev.render`, `prev.html`, plus `keys` / `wakes` / `wakes/key`.

Seam: `crates/wolf-macos/Sources/WolfKeyMap/FrameStats.swift` (pure, unit-tested).
Glue: `crates/wolf-macos/Sources/WolfMacOS/FrameMetrics.swift`.

Run it (NOTE the `Wolf_archive-root` path — `bazel-bin/crates/wolf-macos/Wolf.app` does
NOT exist; `macos_application` emits `Wolf.zip` plus the unzipped bundle under
`Wolf_archive-root/`):

```
bazel build //crates/wolf-macos:Wolf
WOLF_FRAME_STATS=1 ./bazel-bin/crates/wolf-macos/Wolf_archive-root/Wolf.app/Contents/MacOS/Wolf
```

## Success criteria

`draw` with a preview open drops materially from its 47.6 ms mean, toward (or below) the
26 ms no-preview figure, with no visual regression in the preview or editor panes. If the
occlusion hypothesis is disproven, that is a perfectly good outcome — record what the
evidence actually showed and stop, rather than optimizing on a wrong theory.

## Architect's plan (APPROVED — note the STAGED GATE at phase A step 4)

### Why occlusion is strongly supported (from code reading)

- `draw` (`:1149`) loops `paneRects()` and calls `paintPane` for EVERY pane with a bound
  grid. No preview check. `paintPane` (`:1190`) calls `handle.read_frame_for_grid(grid)`
  (whole cell-vector across the FFI) then `paintGrid` (`:1210`) → fill, `paintCells`,
  `drawCursor`. `syncGridSizes` (`:654`) resizes the preview pane's grid to FULL geometry
  — it is a full-size grid, not a stub.
- `syncPreviewWebViews` (`:816`) sets `web.frame = gridRect(ofPane: entry.1)` — the
  IDENTICAL expression `paintGrid` is handed. Coverage is exact, not approximate.
  `web.isHidden = false` each wake. The rendered doc sets an opaque
  `body.wolf-markdown` background (`#1b1b1b` dark / `#f4f4f4` light) which per CSS
  propagates to the viewport canvas → opaque over its whole frame once loaded.
- **The scratch grid is NOT cheap.** `paintCells` (`:1344`) does `setFillColor` + `fill`
  for EVERY cell unconditionally; only the glyph is conditional. Blank cells are not
  empty — `grid_line` stores nvim's literal `" "` (`wolf-core/src/redraw.rs:780-813`);
  only wide-char trailing placeholders and never-painted cells are `""`
  (`wolf-core/src/render.rs:187-198`). So a blank cell has `text == " "` → non-empty →
  `drawGlyph` runs → `CTLineCreateWithAttributedString` + `CTLineDraw` per cell, plus
  `FfiCell::text()`'s `self.text.clone()` Rust String alloc per cell
  (`wolf-ffi/src/lib.rs:2160`). A blank preview grid costs nearly what a full one does.
- Residual uncertainty (the reason to MEASURE, not assume): `Grid::clear()` resets cells
  to `Cell::default()` (empty text), so between a `grid_clear` and the following
  `grid_line` a grid IS cheap. Steady state should re-send `grid_line`, but confirm.

The statusbar strip is OUTSIDE `gridRect` and is NOT covered — only the grid paint may be
skipped.

### PHASE A — verification (land this FIRST, then STOP)

Add two buckets to `FrameStatsBucket`, dense/ascending raw values:
`drawPaneEditor = 6` label `"draw.pane"`, `drawPanePreview = 7` label `"draw.prev"`.
In `draw`'s pane loop, explicit `begin`/`end` around `paintPane` (not `defer`, mirroring
the `previewRender` site), selecting `.drawPanePreview` when `previewNamesByGrid[grid] != nil`
else `.drawPaneEditor`. Keep these buckets PERMANENTLY — they are the before/after
evidence here and the instrument for the next (glyph) pass.

`FrameStatsTests.swift` pins bucket discriminants/labels and asserts `allCases.count`
against the pinned list (~:110-134) — add the two rows and update the enum recap comment
(~:19-25). The `format` ordering tests (~:598-630) iterate `allCases` and need no edit.

**GATE — decision rule after measuring, before writing any fix:**

- `draw.prev` mean >~ 15 ms with a preview open → occlusion CONFIRMED, proceed to phase B.
- `draw.prev` mean <~ 5 ms → occlusion DISPROVEN. Record the numbers, STOP, hand the pass
  to the glyph work. **Do not build the fix.**
- In between → report both buckets plus the editor pane's column count and re-decide.

### PHASE B — the fix (ONLY if the gate confirms)

**CREATE `Sources/WolfKeyMap/PaneOcclusion.swift`** — pure, Foundation-only, reuses `Rect`
from `PaneTree.swift` as `EditorInset.swift`/`PickerPanelSeam.swift` do.

- `public struct OccluderGeometry` — `frame: Rect`, `isHidden: Bool`, `isAttached: Bool`,
  `alpha: Double`, `hasLoadedContent: Bool`, public memberwise init (matching the
  `FrameStats.BucketSummary` convention so tests build fixtures directly).
- `public enum PaneOcclusion` with
  `static func isFullyOccluded(gridRect: Rect, by occluder: OccluderGeometry?) -> Bool`.

ALL rules must hold or the result is `false` (i.e. PAINT):

1. `occluder != nil` — "web view does not exist yet" (`previewWebViews[win] == nil`)
2. `hasLoadedContent` — exists but has not loaded content yet
3. `isAttached` — detached
4. `!isHidden` — hidden / hidden ancestor
5. `alpha >= 1.0` — transparent
6. `frame.w > 0 && frame.h > 0` — zero-size
7. `gridRect.w > 0 && gridRect.h > 0` — degenerate grid rect reported NOT occluded
   (conservative; nothing to save anyway)
8. Strict containment, **tolerance 0.0**: `frame.x <= gridRect.x`, `frame.y <= gridRect.y`,
   `frame.maxX >= gridRect.maxX`, `frame.maxY >= gridRect.maxY`

Zero tolerance is correct and SAFE because `syncPreviewWebViews` assigns the web view's
frame from the SAME `gridRect(ofPane:)` expression — values are bit-identical when in
sync, and when OUT of sync (mid-live-resize, post-split, pre-close) containment fails and
the grid paints. That satisfies "geometry changes restore painting immediately"
STRUCTURALLY rather than by bookkeeping. Document that partial coverage returns `false`.

**MODIFY `PaneContainerView.swift`:**

- Add `private var previewLoadedWins: Set<Int64> = []` beside the other per-window preview
  maps. Documented: set when a web view first reports `didFinish`; deliberately NOT
  cleared on a subsequent reload, because WebKit keeps displaying the previous document
  while the next loads — clearing it would cause paint/unpaint flicker.
- `previewDidFinishLoad(win:)` — insert `win` FIRST; the existing
  `guard let fragment = ... else { return }` returns early.
- `removePreviewWebView(_:)` — remove `win` alongside the other per-window teardown. This
  is the "preview close restores painting immediately" guarantee.
- `private func previewOccluder(forWin:) -> OccluderGeometry?` — `nil` when
  `previewWebViews[win] == nil`; else built from the live view:
  `isHidden: web.isHiddenOrHasHiddenAncestor` (preferred over bare `isHidden`),
  `isAttached: web.superview === self && web.window != nil`,
  `alpha: Double(web.alphaValue)`, `hasLoadedContent: previewLoadedWins.contains(win)`.
- `paintPane(grid:paneRect:active:paintsGrid:ctx:)` — when `paintsGrid == false`, paint the
  statusbar as today then `return` WITHOUT `handle.read_frame_for_grid(grid)` and without
  `paintGrid`. **Skipping the FFI frame read is a material part of the saving — do not
  hoist it above the branch.**
- In `draw`'s pane loop pass `paintsGrid: !occluded`. Requires reading the full
  `PaneBinding` (for `win`), not just `boundGrid`. Add a small private `CGRect` →
  `WolfKeyMap.Rect` helper (none exists; `paneRects()` does the inverse inline).
- Leave `drawDividers`, `drawScrollbars`, `refreshOverlays` and the `ctx.fill(bounds)`
  editor-background fill COMPLETELY unchanged. `drawScrollbars` already suppresses the
  synthetic scrollbar over preview panes via an independent `previewNamesByGrid` check —
  do NOT couple it to occlusion.
- `drawCursor` is only reachable from `paintGrid`, so it is skipped for an occluded pane.
  That is CORRECT — a preview pane's cursor is already invisible under the web view — but
  state it in a code comment so a reviewer does not flag it.

### Tests

`PaneOcclusionTests.swift`, one case per correctness constraint, all asserting `false`
(paint) unless stated: exact-equality containment → TRUE; strictly-larger container →
TRUE; `nil` occluder; `hasLoadedContent == false`; `isAttached == false`; `isHidden`;
alpha 0.0/0.5/0.999 → false and 1.0 → true; zero width, zero height, zero-by-zero;
partial coverage short on left/right/top/bottom and inset on all four sides (x5);
off-by-a-hair `maxX` short by 0.5 (pins tolerance 0.0); occluder entirely elsewhere;
degenerate `gridRect`; and flag-interaction cases proving the rules AND rather than
short-circuiting on geometry alone.

**No macOS UI test target exists** (`BUILD.bazel` has only `WolfKeyMapTests` +
`bundle_layout_test.sh`). Compile-verified + manual-smoke only: `previewOccluder`'s live
`WKWebView` reads, the `previewLoadedWins` lifecycle (first load, reload, `previewFile`
repoint, pane close), and the `paintPane` branch.

**Manual smoke checklist (reviewer must confirm no visual regression):** 1. preview
renders, statusbar reads `Preview: <name>`, no synthetic scrollbar. 2. typing updates it
live. 3. **live-resize with a preview open** — grid must repaint under the web view while
frames are out of sync; a brief flash of scratch grid at the seam is acceptable, a BLACK
or GARBAGE region is a bug. 4. split / close pane / close preview repaints immediately.
5. two preview panes. 6. cross-file link repoint stays occluded. 7. Cmd-+/Cmd-- font size.
8. tab away and back (`markRefocusRebind`). 9. system light/dark toggle.

### BUILD.bazel

`wolf_key_map.srcs` += `Sources/WolfKeyMap/PaneOcclusion.swift` (alphabetical, between
`PaneGeometry.swift` and `PaneTree.swift`); `WolfKeyMapTests.srcs` +=
`Tests/WolfKeyMapTests/PaneOcclusionTests.swift`. No change to `wolf_macos_lib` (globs) or
`swift_format_srcs` (globs).

### Docs

`Sources/README.md` (PaneOcclusion bullet + file-count phrase + FrameStats "six buckets"
→ eight, enumerating what the two new ones measure); `crates/wolf-macos/README.md`
(occlusion behavior; **fix the stale "the blank scratch grid paints underneath (occluded,
cheap)" claim — it was never cheap**); `Tests/README.md` (PaneOcclusionTests);
**`PaneContainerView.swift:159-166` header comment — the same "(occluded, cheap)" claim.
This is the documentation bug that seeded the whole problem.** Also correct the wrong
bundle path in `FrameMetrics.swift`'s header comment (`:23-24`). `prettier --write` all.

### Success criteria

`draw.prev` mean drops to near zero; `draw` mean falls from 47.6 ms toward ~26 ms; total
`draw` time in a 5 s window falls from ~2663 ms toward ~1470 ms. **Report BOTH the mean and
the count x mean product** — the count is load-dependent (see the coalescing note above) so
the mean alone is misleading. The remaining ~26 ms floor is expected and is the NEXT pass's
target, not this one's failure.

### Explicitly NOT changing

Glyph run coalescing / glyph cache / `drawGlyph` / `paintCells` rework (incl. the per-cell
`CTLine` and per-cell Rust String clone) — next pass. The redundant `read_frame_for_grid`
calls and triple cell clone (beyond the one call skipped as a side effect here). Moving
`prev.render` off the main thread. Wake amplification. Dirty-rect tracking. **Any Rust**
(`wolf-core`, `wolf-ffi`, FFI surface, `markdown.rs`, preview CSS). `loadHTMLString` →
incremental DOM (cancelled; 2.4 ms). **`isOpaque`** — not the cause, and bundling an
unmeasured change into a measured pass would contaminate the before/after; queue separately.
Preview web-view z-order vs overlays. `drawScrollbars`' existing preview suppression.

## GATE RESULT: OCCLUSION CONFIRMED — phase B is unlocked

Capture: `test-3-with-preview-after-phase-a` (repo root). Steady-state windows #3-#6,
same file/position/window size as `test-2-with-preview`.

| bucket               | mean         | count        |
| -------------------- | ------------ | ------------ |
| `draw`               | 45.6–46.0 ms | 47–56        |
| `draw.pane` (editor) | 22.1–22.3 ms | == draw count |
| `draw.prev` (preview)| **22.0–22.2 ms** | == draw count |

- `draw.prev` = **22 ms**, far above the >~15 ms confirm threshold. Occlusion CONFIRMED.
- `draw.prev` ≈ `draw.pane` — the blank preview scratch grid costs the SAME as the
  text-full editor grid, exactly as predicted from the `" "`-not-`""` cell analysis.
- The two panes sum to ~44.2 ms of the ~45.8 ms `draw`; background fill + dividers +
  scrollbars are only ~1.6 ms. The pane loop is essentially all of `draw`.
- Both counts equal the `draw` count in steady state → every draw paints both panes.

**The instrument validated itself:** window #1, before the preview was opened, shows
`draw.pane` with NO `draw.prev` bucket. Bucket selection is correct, not inverted or stuck.

**Projected saving:** `draw` 45.8 -> ~23.8 ms (-48%). Per 5 s window,
56 x 45.6 = 2554 ms -> 56 x 23.8 = 1333 ms, i.e. **~1.2 s per 5 s reclaimed, ~25% of the
entire main thread**, plus one fewer `read_frame_for_grid` per frame. With-preview busy
should fall from ~88% toward ~64%.

## Status

- [x] architect plan
- [x] PHASE A: buckets + per-pane timing (tests green, swift_format clean)
- [x] PHASE A: MEASURE + evaluate gate — **CONFIRMED, proceed to phase B**
- [x] PHASE B: tests — `PaneOcclusionTests.swift` (all 8 rules + interaction matrix)
- [x] PHASE B: implementation — `PaneOcclusion.swift`, `previewLoadedWins`,
      `previewOccluder`, `paintPane(paintsGrid:)`, `draw` pane loop
- [x] review — APPROVED, no blocking findings
- [x] docs — `Sources/README.md`, `README.md`, `Tests/README.md`, `architecture.md`,
      and the seed bug: `PaneContainerView.swift:161` "(occluded, cheap)" corrected
- [ ] **re-measure and record before/after** ← ONLY REMAINING STEP

### Review notes worth keeping

- **Worst case (a permanently blank pane) is structurally unreachable.** `previewOccluder`
  short-circuits on `previewWebViews[win] == nil` BEFORE consulting `previewLoadedWins`, so
  a stale set entry alone can never suppress a paint. `removePreviewWebView` is the SOLE
  removal path for a web view (only call site `syncPreviewWebViews:871-873`) and clears the
  set, so a reused `win` handle always gets a fresh `hasLoadedContent: false`.
- **Cross-file repoint (`previewFile`) is correct** — it never touches `previewLoadedWins`,
  so the win stays in the set while the new document loads. Right, not a hazard: WebKit
  keeps the previous document composited until the next commits, so the view stays opaque.
  Clearing it would cause exactly the flicker the header comment warns about.
- **The plan's zero-tolerance argument was stated too strongly and is now corrected in
  `PaneOcclusion.swift`'s header.** "Out of sync -> containment fails -> paints" is NOT
  universal: if a pane SHRINKS and `draw` runs before `syncPreviewWebViews`, the stale
  LARGER frame still contains the smaller `gridRect`, so the paint IS skipped. Still safe,
  but the real invariant is **"the web view covers the skipped region"**, not "the frames
  are in sync". A future reader over-trusting the sync argument could introduce a real bug.
- **Frame perturbation is the silent failure mode.** `autoresizingMask` is `.none` and
  `NSView` does not backing-align frames unless opted in, so this should not happen — but
  **if re-measurement does NOT show `draw.prev` collapsing toward zero, frame perturbation
  is the first suspect.** The failure is silent (optimization disabled), not visual.
- Verified: no Rust changed (`git status crates/wolf-core crates/wolf-ffi` clean); FFI
  `read_frame_for_grid` genuinely below the `guard paintsGrid`, not hoisted; phase-A timing
  still brackets exactly `paintPane`, so numbers stay comparable to the gate.
- Tests: `WolfKeyMapTests` PASS, `bundle_layout_test` PASS, `//tools/lint:swift_format`
  PASS, `bazel build //crates/wolf-macos:Wolf` succeeds.

### Manual smoke checklist (NOT yet run — needs a human at the app)

1. preview renders; statusbar reads `Preview: <name>`; no synthetic scrollbar over it
2. typing in the source updates the preview live
3. **live-resize the window with a preview open** — brief flash of scratch grid at the seam
   is acceptable; a BLACK or GARBAGE region is a bug
4. split / close a pane / close the preview — pane underneath repaints immediately
5. two preview panes open at once
6. cross-file link repoint stays occluded
7. Cmd-+ / Cmd-- font size change with a preview open
8. tab away and back (`markRefocusRebind` path)
9. system light/dark toggle with a preview open
