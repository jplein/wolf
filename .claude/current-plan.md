# Plan: Reduce `draw.pane` — the per-pane paint floor

## Problem statement

After the occlusion fix (committed, `cd0230b`), `draw.pane` is the single largest
main-thread cost in Wolf and the floor under EVERY configuration, preview or not. It is
20.4 ms to paint one pane. Terminal Neovim's whole keystroke-to-pixel path is single-digit ms.

## Where we are (measured, this machine — a FAST Mac)

Steady state, large file (`~/Git/jplein/Notepad/commonplace-book.md`), typing near the top.
Captures `test-1`..`test-4` in the repo root are the full record.

| bucket        | before occlusion fix | after (current) |
| ------------- | -------------------- | --------------- |
| `draw`        | 45.6–46.0 ms         | 22.1–22.3 ms    |
| `draw.pane`   | 22.1–22.3 ms         | **20.4–20.5 ms** |
| `draw.prev`   | 22.0–22.2 ms         | 0.13 ms (fixed) |
| `prev.reload` | 28.7–29.4 ms         | 28.2–28.7 ms    |
| `prev.render` | 26.4–27.1 ms         | 25.8–26.3 ms    |
| `ctrl.wake`   | 4.71–4.77 ms         | 4.81–4.88 ms    |

Current main-thread budget per 5 s window (with preview open):

| source                                    | time / 5 s | share |
| ----------------------------------------- | ---------- | ----- |
| `draw` (almost entirely `draw.pane`)      | 1703 ms    | 47%   |
| `prev.reload` (of which `prev.render` 1127 ms) | 1233 ms | 34%   |
| wakes                                     | 723 ms     | 20%   |

Total busy ~73%. Per keystroke ~70 ms. Frame rate ~15.4 fps.

**Read products, not means.** `draw` COUNT rose 56 -> 77 after the occlusion fix, because a
less-busy main thread lets AppKit coalesce fewer display passes. Freed capacity is partly
reinvested as more frequent redraws and preview reloads rather than idle. Always report
count x mean alongside the mean.

## THE QUESTION THIS PASS MUST ANSWER FIRST

`draw.pane` = 20.4 ms for ONE pane. It is the sum of at least four different things, and
they have COMPLETELY DIFFERENT fixes. The split is unknown and must be measured before any
optimization is written.

1. **`handle.read_frame_for_grid(grid)` — FFI marshalling.**
   `from_snapshot_grid` (`crates/wolf-ffi/src/lib.rs:3456`) allocates `cols*rows` `FfiCell`s
   each with a `cell.text.clone()` (a Rust `String` heap alloc PER CELL). `cells()`
   (`:3375`) then does `self.cells.clone()` — a SECOND full deep clone. `drawCursor`
   (`PaneContainerView.swift`) calls `frame.cells()` again — a THIRD.
   → If this dominates, the fix is in **Rust/FFI** (return a borrowed view, or a flat
   packed buffer, instead of cloning `String`s per cell).
2. **`paintCells` — CoreText.** Per cell: `ctx.setFillColor` + `ctx.fill`, then for any
   non-empty cell a fresh `[NSAttributedString.Key: Any]` dict, a fresh
   `NSAttributedString`, and a fresh `CTLineCreateWithAttributedString` + `CTLineDraw`. No
   glyph cache, no run coalescing, no batched `CGContextShowGlyphsAtPositions`.
   Remember: blank cells hold nvim's literal `" "`, NOT `""`, so they are NOT skipped —
   this is what made the preview scratch grid cost a full 22 ms.
   → If this dominates, the fix is entirely in **Swift** (glyph cache + run coalescing).
3. **Swift-side per-cell `String` bridging** — `cell.text().toString()` allocates a Swift
   `String` per cell per frame.
4. **The statusbar paint** — known small (~0.13 ms, from the occluded-pane measurement,
   which is statusbar-only).

**Do not guess the ratio.** The last pass nearly optimized `loadHTMLString`, which turned
out to be 2.4 ms; only measurement caught it. The same discipline applies here.

## Approach: same staged gate that worked last time

### PHASE A — measure the split

Add sub-buckets to the existing `FrameStats` seam (it is already there, allocation-free,
env-gated, and unit-tested) splitting `draw.pane` into at minimum:

- the `read_frame_for_grid` FFI call
- `paintCells`
- `drawCursor`
- the statusbar paint

Then re-capture under the SAME protocol and report count x mean for each.

**GATE — decide the fix from the evidence, not from this plan's suspicions:**

- FFI marshalling >~50% of `draw.pane` → the primary fix is in Rust/FFI. This would be the
  first Rust change in this whole effort; scope it separately.
- CoreText >~50% → the primary fix is a Swift glyph cache + run coalescing.
- Roughly even → fix both, cheaper one first, re-measuring between so each is attributable.

### PHASE B — the fix, conditional on phase A

Not specified here on purpose. The architect specifies it AFTER the gate result is in.

## Constraints

- Preserve the phase-A/occlusion instrumentation exactly as-is — `draw`, `draw.pane`,
  `draw.prev` must stay comparable to `test-1`..`test-4`.
- Any new bucket must keep raw values dense/ascending and update the pinned test list in
  `FrameStatsTests.swift`.
- Sub-bucket timing must not itself distort the measurement: `draw.pane` is 20 ms and a
  clock read is tens of ns, but per-CELL timing would be fatal. Time PHASES, never cells.
- Disabled path stays free (one `swift_once` load + a branch).

## Explicitly NOT in this pass

- Moving `prev.render` off the main thread (now 34% of budget and the clear NEXT pass after
  this one — it rose in relative importance once `draw` halved).
- Wake amplification (~2.9 wakes/key at ~4.8 ms).
- Dirty-rect tracking in `draw`.
- `INPUT_COALESCE_DELAY` (8 ms, machine-independent).
- `isOpaque` (still not the cause of anything measured).

## Success criteria

Phase A: a trustworthy split of the 20.4 ms with count x mean per sub-bucket, sufficient to
pick the fix without guessing. A surprising result is a GOOD outcome.

Phase B: `draw.pane` materially below 20.4 ms with no visual regression. Terminal-parity is
not the bar for one pass; a large, verified reduction is.

## Architect's PHASE A plan (APPROVED — implement exactly this)

### Findings that drive the design (verified in code, do NOT re-derive)

Call chain per pane: `draw` (`:1208`) -> `paintPane` (`:1271`) -> `paintStatusbar` (`:1337`),
then `read_frame_for_grid` (`:1293`), then `paintGrid` (`:1305`) -> `paintCells` (`:1439`)
and `drawCursor` (`:1676`).

**FIVE deep copies of the cell data, located precisely:**

| # | where | cost |
|---|-------|------|
| A | `FfiFrame::from_snapshot_grid` (`wolf-ffi/src/lib.rs:3450`), `cell.text.clone()` `:3460` | `cols*rows` Rust String allocs, inside `read_frame_for_grid`. `core.read_snapshot()` is only a `read_arc()` guard (`wolf-core/src/api.rs:340`) — cheap. |
| B | `let cells = frame.cells()` at `PaneContainerView.swift:1442`, INSIDE `paintCells` -> `self.cells.clone()` (`lib.rs:3375`) | a second full `cols*rows` String alloc set |
| C | `frame.cells()` passed as an **ARGUMENT at the call site** `:1322` inside `paintGrid` | a third full clone — for a function that reads at most ONE cell |
| D | `cell.text()` at `:1458`/`:1698` -> `FfiCell::text` = `self.text.clone()` (`lib.rs:2160`), then `.toString()` | per cell: one Rust String alloc + one Swift String alloc |
| E | **`RustVec<FfiCell>` teardown** — nobody had named this. `cells` is a function-local; ARC release drops `cols*rows` `FfiCell`s = one `free` per cell, after the loop, before `paintCells` returns. |

Also per cell inside the glyph loop: `WolfColor.cgColor(cell.bg(), ...)` at `:1454`
allocates a `CGColor`. And because blank cells carry `" "`, `!text.isEmpty` is true for
essentially EVERY cell, so `drawGlyph` (`:1625`, `CTLineCreateWithAttributedString` +
`CTLineDraw`, no cache) runs `cols*rows` times.

**Consequences:** clone B is eager on the FIRST statement of `paintCells`, so a naive
`paintCells` bucket conflates marshalling with CoreText — the exact confusion to avoid.
**Clone C is evaluated at the CALL SITE, not inside `drawCursor` — a `defer` inside
`drawCursor` would MISS it entirely.** Teardown E is only capturable as a difference.

### The six new buckets (dense, ascending from `drawPanePreview = 7`)

| raw | case | label | measures |
|-----|------|-------|----------|
| 8  | `paneFrameRead` | `pane.read`   | `handle.read_frame_for_grid(grid)` — snapshot lock + clone A + `FfiFrame` construction |
| 9  | `paneCells`     | `pane.cells`  | the whole `paintCells(frame, ctx:)` call — **PARENT** of 10/11, and the only bucket containing teardown E |
| 10 | `paneCellsVec`  | `pane.vec`    | just `let cells = frame.cells()` — clone B in isolation |
| 11 | `paneGlyphLoop` | `pane.glyphs` | just the `for row`/`for col` double loop — bg `CGColor` + fill, per-cell text bridge D, `GlyphStyleResolver.resolve`, `drawGlyph` |
| 12 | `paneCursor`    | `pane.cursor` | the `drawCursor(...)` call expression in `paintGrid`, INCLUDING its `frame.cells()` argument — clone C + cursor fill + at most one glyph |
| 13 | `paneStatusbar` | `pane.status` | the `paintStatusbar(...)` call in `paintPane` |

`pane.vec` is the single most important bucket — without it `pane.cells` mixes a full deep
clone with all CoreText work. `pane.cells` exists solely to attribute teardown E without
relying on undefined `defer`-vs-ARC ordering. Longest label is 11 chars, inside the pinned
left-14 field, so `format` layout and the byte-exact tests are unaffected.

**DISJOINTNESS CONTRACT (document in the enum doc comment):**
`{pane.read, pane.cells, pane.cursor, pane.status}` are mutually disjoint, summing to at
most `draw.pane + draw.prev`. `{pane.vec, pane.glyphs}` partition the INTERIOR of
`pane.cells`, with a remainder. **The six do NOT sum to `draw.pane`; summing all six
double-counts.**

### Timer placement (all in `PaneContainerView.swift`) — explicit begin/end everywhere

Rule as before: `defer` for whole functions with early returns, explicit for statements.

- **`pane.status`** — `paintPane`, around the `paintStatusbar(...)` call `:1277-1279`.
- **`pane.read`** — `paintPane`, around exactly `let frame = handle.read_frame_for_grid(grid)`
  `:1293`. Must sit **BELOW the `guard paintsGrid` at `:1288`** so an occluded pane records
  no `pane.read` sample — preserves occlusion semantics and makes the count identities below
  meaningful.
- **`pane.cells`** — `paintGrid`, around the `paintCells(frame, ctx: ctx)` call `:1320`.
  At the call site, NOT `defer` inside `paintCells`: `defer` order relative to ARC release
  of function locals is **unspecified** by Swift; a call-site pair is guaranteed to enclose
  the release of `cells`.
- **`pane.vec`** — inside `paintCells`, around exactly `let cells = frame.cells()` `:1442`.
- **`pane.glyphs`** — inside `paintCells`, begin immediately before `for row in 0..<rows`
  `:1446`, end immediately after the outer loop's closing brace `:1466`. **NOT `defer`** —
  a `defer` registered before the loop fires at function exit and would swallow part of the
  teardown, destroying the separation `pane.cells` exists to expose.
- **`pane.cursor`** — `paintGrid`, begin immediately BEFORE the whole
  `drawCursor(ctx, frame:, cells: frame.cells(), cols:)` statement `:1322` (inside the
  `if frame.cursor_visible()` block), end immediately after. **The `begin()` MUST precede
  argument evaluation, because that is where clone C happens. A `defer` inside `drawCursor`
  would be WRONG and must not be used — it would report near-zero and silently hide a full
  deep clone. This is the most error-prone placement in the plan; add a comment at the site
  saying so.**

No timers inside any loop body. No changes to `drawGlyph`, `drawDecorations`,
`drawStatusbarText`, `drawDividers`, `drawScrollbars`.

### What `pane.glyphs` actually measures — READ THIS BEFORE INTERPRETING THE GATE

A clean split of the per-cell string bridging from CoreText is **NOT possible** without
per-cell timing, which is rejected (below). `pane.glyphs` is a composite, per cell, of:
`cell.bg()` + `WolfColor.cgColor` (a `CGColor` alloc); `setFillColor` + `fill`;
`cell.text()` (Rust String clone); `.toString()` (Swift String alloc);
`flags()`/`fg()`/`special()` + `GlyphStyleResolver.resolve`; and `drawGlyph` (attributes
dict + `NSAttributedString` + `CTLine` + `CTLineDraw`).

**Read `pane.glyphs` as "per-cell Swift-side work", NOT as "CoreText".**

If the gate lands here, phase B's FIRST action is a **discriminating probe, not a fix**: a
throwaway local build stubbing out `drawGlyph` (or the `cell.text()` bridge) and
re-measuring under the identical protocol. One A/B measurement, costs nothing, and is far
cheaper than building a glyph cache and discovering the bridging was the real cost.

### Measurement-distortion analysis

12 added clock reads per pane per frame (10 without a cursor; an occluded pane adds only 2).
A two-pane frame goes ~6 -> ~28 reads. At tens of ns per `DispatchTime.now()`, that is
~0.3 us against a 20,400 us region — **~0.0015%**, well below existing run-to-run variance.
**None is per-cell or per-row.**

REJECTED: per-cell timing (~6,400 reads/frame, ~0.6% — but the fatal objection is not clock
cost, it is that two `DispatchTime.now()` calls in the innermost loop act as **optimization
barriers**, inhibit inlining of `WolfColor.cgColor`/`GlyphStyleResolver.resolve`, and change
the very quantity under measurement). Also rejected: per-row timing (same barrier, less
benefit); accumulate-into-a-local (still needs the per-cell reads and the barrier).

### GATE — with `T(b) = count(b) x mean(b)` (PRODUCTS, not means) and `D = T(draw.pane)`

- **Teardown** `= T(pane.cells) - T(pane.vec) - T(pane.glyphs)`
- **FFI marshalling** `M = T(pane.read) + T(pane.vec) + Teardown + T(pane.cursor)`
- **Per-cell Swift work** `C = T(pane.glyphs)`
- **Statusbar** `S = T(pane.status)`

Decision rule:

- `M / D >= 0.50` → primary fix is **Rust/FFI** (borrowed view or flat packed buffer instead
  of per-cell String clones). First Rust change in this effort; scope separately.
- `C / D >= 0.50` → primary fix is **Swift**, but run the discriminating probe FIRST to
  choose between a glyph cache + run coalescing vs eliminating the per-cell bridge.
- Neither >= 0.50 → fix both, cheaper first, re-measuring between so each is attributable.
- `S / D > 0.10` → the statusbar was mis-assumed cheap; re-plan before proceeding.

**VALIDITY CHECKS — must pass BEFORE the gate is applied. If any fails the split is not
trustworthy and must be explained first:**

1. `count(pane.read) == count(pane.cells) == count(pane.vec) == count(pane.glyphs) == count(draw.pane)`.
   A `pane.read` count EXCEEDING `draw.pane` means a preview pane painted its grid (web view
   not yet loaded/opaque) and the sub-buckets are contaminated by a second pane.
2. `count(pane.status) == count(draw.pane) + count(draw.prev)` — sub-buckets record for both
   editor and preview panes, and only the statusbar runs on an occluded pane.
3. `count(pane.cursor) <= count(draw.pane)` — `drawCursor` runs only when `cursor_visible()`.
4. `T(pane.read) + T(pane.cells) + T(pane.cursor) + T(pane.status) <= T(draw.pane) + T(draw.prev)`,
   residual (background fill, clip/save/restore, `gridRect`/`geometry`) under ~1 ms/frame.
   A large residual invalidates the split.
5. `T(pane.vec) + T(pane.glyphs) <= T(pane.cells)`, and Teardown >= 0.

Capture as `test-5`, same protocol: `~/Git/jplein/Notepad/commonplace-book.md`, typing near
the top, preview open, steady state. Also record `draw`, `draw.pane`, `draw.prev`,
`prev.reload`, `prev.render`, `ctrl.wake`, `keys`/`wakes`/`wakes/key` unchanged for
comparability with `test-1`..`test-4`.

### Tests (`FrameStatsTests.swift`)

MODIFY: `testBucketRawValuesAndLabels` gains six rows (`.paneFrameRead, 8, "pane.read"` …
`.paneStatusbar, 13, "pane.status"`); the derived `allCases.count == cases.count` and
`allCases.map(\.rawValue) == Array(0..<count)` assertions then re-pin density for 14 buckets
automatically. Update the file-header `CONTRACT UNDER TEST` block (lines 18-28).

ADD: `testBucketLabelsAreUniqueAndFitTheReportColumn` — every label unique across `allCases`
(a copy-paste duplicate in the `switch` would silently produce two identical report rows,
and would also fool `testFormattedBucketRowsAreInRawValueOrder`'s substring scan) and every
label <= 13 chars so the left-14 field always leaves a separating space.
`testEveryBucketRoundTripsThroughRecordAndSummarize` — one distinct sample per bucket across
all 14, assert `summarize` returns 14 rows with expected values; catches an off-by-one if
accumulator sizing ever stops deriving from `allCases`.

UNCHANGED and should pass without edits: `testEveryFormattedLineCarriesTheGrepPrefix`
(derived from `allCases.count`), `testFormattedBucketRowsAreInRawValueOrder`,
`testFormatProducesTheExactPinnedTable`, `testFormatEmitsHeaderAndFooterWithNoBuckets`
(fixed fixtures with explicit bucket lists).

Known gaps (window-server-bound, per `Tests/README.md:109`): an occluded pane recording
`pane.status` only; `pane.cursor` absent on a pane not holding the cursor; a 0x0 frame giving
near-zero `pane.glyphs` with non-zero `pane.read`. **The count identities during the manual
capture are the actual acceptance check for this pass.**

### No BUILD.bazel changes, no new files, no Rust, no dependencies

`wolf_key_map` lists `FrameStats.swift` explicitly (`BUILD.bazel:52`); `wolf_macos_lib`
globs `Sources/WolfMacOS/*.swift` (`:82`). Nothing added or removed. `wolf-ffi/src/lib.rs`
is READ-ONLY this pass — timing `read_frame_for_grid` from Swift captures clone A in full.

### Docs

`crates/wolf-macos/README.md:121` ("the eight buckets" -> fourteen, list the six, state the
nesting/disjointness contract, the `pane.status`-counts-occluded-panes-too asymmetry, the
`pane.glyphs`-is-not-CoreText-alone caveat, and that teardown is a derived difference);
`Tests/README.md:107` (bucket-discriminant list + the two new tests);
`Sources/README.md:32` (FrameStats bucket set); `build.md` only if it enumerates buckets.
`prettier --write` all.

### Explicitly NOT changing

No render behavior — not one pixel. No `paintCells`/`paintGrid` refactor: **the architect
looked specifically for whether the FFI/CoreText split forced one and it does not** — every
boundary already falls on an existing statement boundary. The tempting restructure (scoping
`cells` in a nested block to capture teardown directly) is **REJECTED** because ARC release
points are not guaranteed at a lexical brace, and a restructure that looks behavior-preserving
but shifts a deallocation is exactly what corrupts a measurement pass. No Rust/FFI changes.
No changes to `FrameStats` storage/percentile/format/reset/env parsing (so `test-1`..`test-4`
stay diff-comparable). No `FrameMetrics.swift` change. Not in this pass: `prev.render`
threading, wake amplification, dirty rects, `INPUT_COALESCE_DELAY`, `isOpaque`.

## Status

- [x] architect plan
- [ ] PHASE A: sub-bucket instrumentation
- [ ] PHASE A: measure + evaluate gate
- [ ] PHASE B: architect specifies fix from evidence
- [ ] PHASE B: tests
- [ ] PHASE B: implementation
- [ ] review
- [ ] docs
- [ ] re-measure and record before/after
