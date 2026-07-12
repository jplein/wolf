# Plan: Follow-up bug — reused preview pane keeps rendering the OLD source

## Status: COMPLETE ✅

### Technical-writer (agent a505ccf6f8e88a9ce) + prettier
Updated the 3 macOS READMEs (dropped "currently UNUSED (OD9)"; documented `PreviewLoad`/
`winsNeedingReload` wired into the live gate + the bufnr-or-tick predicate + the reused-pane fix).
Orchestrator ran `prettier --write` (all unchanged/conformant) and grep-confirmed zero stale
`previewsNeedingReload`/`lastPreviewTicks` references remain anywhere in the repo.

### Reviewer verdict (agent aa6ac9c2f5b2dde96): PASS — approved
Traced the user's exact A→B repro end-to-end and confirmed the fix resolves it (bufnr differs →
`winsNeedingReload` returns the win → reload renders B). No regressions (same-source edit, two-panes-
same-source, redundant-render guard all preserved), no correctness bugs, no deviations. Docs pending
(technical-writer's job): the three macOS READMEs must drop the "currently UNUSED (OD9)" language and
reflect the `PreviewLoad`/`winsNeedingReload` rename + the bufnr-or-tick gate.

### Coder result (agent a926cbc36a13724c8)
Implemented `PreviewSeam.PreviewLoad` + `winsNeedingReload` (removed unused `previewsNeedingReload`),
rewired both `PaneContainerView` gate sites + teardown. `bazel test //crates/wolf-macos:WolfKeyMapTests`
PASSED (8 new `winsNeedingReload` cases + preserved `placementMode` cases). `bazel build
//crates/wolf-macos/...` succeeded. Grep confirms zero remaining `lastPreviewTicks`/`previewsNeedingReload`
refs. No deviations. No test files or docs touched.

## Orchestrator rulings (BINDING)
- Approach **(b)** ratified: add pure `PreviewSeam.PreviewLoad` (Equatable `{sourceBufnr: Int64,
  sourceTick: UInt64}`) + `winsNeedingReload(current: [Int64: PreviewLoad], loaded: [Int64: PreviewLoad])
  -> [Int64]`; wire BOTH `PaneContainerView` gate sites to it; REMOVE the unused `previewsNeedingReload`.
- Open-question defaults all accepted: nest `PreviewLoad` in `PreviewSeam`; return `[Int64]`; exclude
  source_name from `PreviewLoad`.
- Reload predicate: reload a win iff `loaded[win] != current[win]` → fires on first-bind,
  bufnr-changed (THE bug, even at equal tick), or tick-advanced; NO reload when all three match.
- Per-win state written only in `reloadDirtyPreviews` after a successful load; removed only in
  `removePreviewWebView` (which already clears the old map — no latent leak, confirmed).
- Front-end-only fix: no new wolf-ffi integration test expected; core/Lua reuse already covered.

## Problem statement (user-reported repro)

1. Open Wolf in a project; open `A.md`; Preview → left pane `A.md`, right pane preview of `A.md`.
2. In the left pane, `:e B.md`.
3. Preview again → the right pane STILL shows the preview of `A.md` (should show `B.md`).

This is a regression EXPOSED (not caused) by the new `"reuse_preview_pane"` placement: it is the first
time a single preview WINDOW can switch which source it shows. The placement/Lua layer is correct
(core's `previews()` now reports the reused window as previewing `B.md`, and `previewNamesByGrid` even
flips the statusbar to `Preview: B.md`). The bug is in the FRONT-END reload gate.

## Root cause (confirmed by reading the code)

`crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift` gates the preview `WKWebView` reload on
the **preview WINDOW handle** + the source `changedtick` ONLY:

- `lastPreviewTicks: [Int64: UInt64]` — win → last-loaded source tick (line 158).
- `syncPreviewWebViews()` requests a reload only `if lastPreviewTicks[win] != preview.source_tick()` (line 652).
- `reloadDirtyPreviews()` (the coalesced pass) guards `lastPreviewTicks[win] != tick` and updates
  `lastPreviewTicks[win] = tick`, then `render_preview(source_bufnr)` (lines 670-679).

When the reuse path rebinds source B's preview buffer into the window that previously showed source A,
the window handle is UNCHANGED, so `lastPreviewTicks[win]` still holds A's tick. Two freshly-opened
files reliably share the same `changedtick`, so `A_tick == B_tick` → the gate is FALSE → NO reload →
the web view keeps rendering A's HTML even though the pane now previews B. (Even when the ticks differ
it is only accidentally correct.) The gate must also react to the SOURCE BUFFER IDENTITY bound to a
window changing, not just the tick advancing.

## Relevant existing seam

`crates/wolf-macos/Sources/WolfKeyMap/PreviewSeam.swift` already carries a pure, unit-tested-capable but
**currently UNUSED** helper `previewsNeedingReload(current: [Int64: UInt64], lastTicks: [Int64: UInt64])`
keyed by SOURCE-BUFNR. That key is wrong for the live gate (two panes previewing the SAME source must
each reload — the reason the live code keys by WIN). The fix must key by WIN while tracking BOTH the
bound source-bufnr AND the tick per window, reloading when EITHER changed. The architect should decide
whether to (a) fix the live gate minimally with a parallel win→source-bufnr map, or (b) refactor this
into a pure, headlessly-testable seam (win → (source_bufnr, tick)) and wire the live gate to it —
preferred for testability and to retire the stale unused helper.

## Key files

- `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift` — `lastPreviewTicks` (158),
  `syncPreviewWebViews()` (625-662), `reloadDirtyPreviews()` (670-680).
- `crates/wolf-macos/Sources/WolfKeyMap/PreviewSeam.swift` — `previewsNeedingReload` (unused).
- `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewSeamTests.swift` — pure tests.
- READMEs: `crates/wolf-macos/README.md`, `crates/wolf-macos/Sources/README.md`,
  `crates/wolf-macos/Tests/README.md`.

## Constraints

- Do not regress: two panes previewing the SAME source must each still reload on a source edit; a
  single preview window switching sources (A→B) must reload; an unchanged (win, source, tick) must NOT
  reload (avoid redundant blocking `render_preview` round-trips).
- Keep any pure seam Foundation-only and headlessly testable; keep existing seam tests passing.
- The FfiPreview accessors available: `win() -> i64`, `source_bufnr() -> i64`, `source_name() -> String`,
  `source_tick() -> u64`.
- Update every affected README.md. Run `prettier --write` on edited Markdown.

## Architect Plan (authoritative — see agent a98625ee7a2c4cf00 for full detail)

### Files to modify
1. `crates/wolf-macos/Sources/WolfKeyMap/PreviewSeam.swift`:
   - Add nested `public struct PreviewLoad: Equatable { public let sourceBufnr: Int64; public let sourceTick: UInt64; public init(...) }` (struct not tuple, so `Optional<PreviewLoad>` inequality compiles).
   - Add `public static func winsNeedingReload(current: [Int64: PreviewLoad], loaded: [Int64: PreviewLoad]) -> [Int64]` = `current.filter { win, load in loaded[win] != load }.map(\.key)`.
   - REMOVE `previewsNeedingReload(current:lastTicks:)` (~lines 55-68); update file header doc.
2. `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`:
   - `:158` field `lastPreviewTicks: [Int64: UInt64]` → `lastPreviewLoads: [Int64: PreviewSeam.PreviewLoad]`; update its doc + the `previewReloadCoalescer` doc (~164-170).
   - `syncPreviewWebViews()` (625-662): build a local `var current: [Int64: PreviewSeam.PreviewLoad]` inside the `for preview in handle.previews()` loop (copy scalars out per borrowed-ref caution: `current[win] = PreviewLoad(sourceBufnr: preview.source_bufnr(), sourceTick: preview.source_tick())`). Remove the inline `if lastPreviewTicks[win] != preview.source_tick()` request at `:652`. After the loop, `if !PreviewSeam.winsNeedingReload(current: current, loaded: lastPreviewLoads).isEmpty { previewReloadCoalescer.request() }`.
   - `reloadDirtyPreviews()` (670-680): build `current` from one `previews()` pass; `let needing = winsNeedingReload(current:, loaded: lastPreviewLoads)`; for each `win` in `needing`, `guard let web = previewWebViews[win], let load = current[win] else { continue }` (skip if no web view yet — do NOT update the map), set `lastPreviewLoads[win] = load`, `render_preview(load.sourceBufnr)` + `loadHTMLString`. Update method doc.
   - `removePreviewWebView(_:)` (~718-724): `lastPreviewTicks[win] = nil` → `lastPreviewLoads[win] = nil`; update doc wording. (This cleanup already exists — no latent leak.)
3. `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewSeamTests.swift`: keep all `placementMode` tests; add
   `winsNeedingReload` cases (assert as `Set<Int64>`, order unspecified) + update the CONTRACT doc block.
4. READMEs: `crates/wolf-macos/README.md` (line 47 seam roll-up; line 83 `PaneContainerView` doc — map
   rename + bufnr-or-tick gate), `crates/wolf-macos/Sources/README.md` (line 25 `PreviewSeam` bullet;
   line 45 `PaneContainerView` bullet), `crates/wolf-macos/Tests/README.md` (line 67 `PreviewSeamTests`).
   `prettier --write` each. NOTE: `PreviewLoad` fields (sourceBufnr/sourceTick) are the reload key; the
   "provided but currently UNUSED (OD9)" language for the old helper must be removed everywhere.

### Test plan (pure headless `winsNeedingReload` cases — enumerate all)
1. First bind: `loaded=[:]`, `current=[10:(1,5)]` → `{10}`.
2. Bufnr changed, SAME tick (THE bug): `loaded=[10:(1,5)]`, `current=[10:(2,5)]` → `{10}`.
3. Tick advanced, same bufnr: `loaded=[10:(1,5)]`, `current=[10:(1,6)]` → `{10}`.
4. Identical no-op: `loaded=[10:(1,5)]`, `current=[10:(1,5)]` → `{}`.
5. Multiple wins independent: `loaded=[10:(1,5),11:(2,6)]`, `current=[10:(1,5),11:(2,7)]` → `{11}`.
6. Two wins same source both reload: `loaded=[10:(1,5),11:(1,5)]`, `current=[10:(1,6),11:(1,6)]` → `{10,11}`.
7. Dropped win not returned: `loaded=[10:(1,5),99:(2,3)]`, `current=[10:(1,5)]` → `{}`.
8. Empty current: `current=[:]`, `loaded=[10:(1,5)]` → `{}`.
Manual-smoke: the A→B reuse repro (right pane must render B); same-source edit still lives; two panes
same source both update; teardown/re-open clean. No wolf-ffi integration change (front-end only).

### Build/test commands
`bazel test //crates/wolf-macos:WolfKeyMapTests`; `bazel build //crates/wolf-macos/...`.

## Progress log

- (init) Follow-up bug diagnosed by orchestrator; architect invoked.
- Architect plan complete (agent a98625ee7a2c4cf00). Approach (b) ratified; open-question defaults
  accepted. Invoking test-writer.
