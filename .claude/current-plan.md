# Plan: Align statusbar left edge with preview content

## Status: COMPLETE (pending visual confirmation)

- `WolfKeyMapTests` passes cold (`--nocache_test_results`): 171 tests, 0 failures, including exactly 13 `StatusbarTextSpanTests`. All `//crates/wolf-macos/...` targets build and pass; `swift_format` clean. Rust side unregressed (`markdown_render`, `wolf_core_lib_test` both pass).
- Reviewer: **Approved**, editor branch verified algebraically identical. Two notes raised, both closed.

### Corrections to this plan's own text

**§2's claim "during a live resize both are stale together" is WRONG**, and the doc comment written from it overclaimed. The reviewer verified: the statusbar span is recomputed from live geometry on every `draw(_:)`, so it is _never_ stale — only the webview _frame_ lags. `setFrameSize` (~633-637) and `applyMetrics` (~464-471) both repaint without calling `syncPreviewWebViews`. `applyMetrics` is the sharper case: a font-size change moves `cellWidth` (hence `cap` and `paintOffsetX`) _and_ `statusbarHeight` (hence grid rect y/h) at once, shifting the statusbar immediately while the webview stays put; it converges via `syncGridSizes` → `handle.resize` → wake.

The guarantee that DOES hold, and is what the shipped doc comment now says: the pane-kind map and the webview set are written together in one `syncPreviewWebViews` pass, so a pane is never drawn with `.preview` alignment when it has no webview. The frame lag is pre-existing and orthogonal.

### Other deltas from the plan

1. **13 tests, not 12.** Added `testEditorSpanEqualsTheLegacyExpressionAcrossTheCapThreshold` — a sweep asserting the `.editor` span equals the longhand legacy expression at 10 widths × 2 `gridLeft` values, straddling the cap in both directions. Case 2 alone pinned only a single point, and "editor panes stay bit-identical" is the top risk.
2. **The wide test cases were unrealistic.** They paired `gridWidth = 1400` with a hard-coded `paintOffsetX = 250`, but the real value is 340 — the `expectedPaintOffsetX` helper existed to model that and was only used by two cases. Now derived. Cases 4 and 5 also gained `offset > 0` guards, since the central assertion in case 5 (`editor.left - preview.left == paintOffsetX`) passes vacuously at zero.
3. **`w` used in both branches**, not the plan's raw `gridWidth` for `.editor`. Reviewer confirmed value-identical for _every_ input, not just negatives.
4. **Two pre-existing undocumented seams closed.** `Sources/README.md` said "Twenty-four files" — true before this change, but `TreeChildrenMemo.swift` and `TreeRefreshGate.swift` had no bullets at all, so the corrected count of 25 disagreed with a 23-entry list. Both now documented; sweep confirms 25 files ↔ 25 bullets.

### Confirmed as planned

- **Right edge fixed for free** — `.preview` spans the whole grid rect, so `textRight == gRect.maxX == web.frame.maxX`. No extra work.
- **`pumAnchorX` is NOT the same defect** — view-wide cmdline overlay, no pane/grid rect in scope, already self-consistent with `CmdlineView`. Left alone, with a doc sentence so nobody "fixes" it by analogy.
- **GNOME front-end** has zero statusbar code (grepped `statusbar|status_bar|statusline`). **Rust core** unchanged — strings only, no geometry crosses the FFI.

### Outstanding — rests on visual review

That the preview statusbar's left edge lands on the rendered content's left edge **on screen** spans `paintStatusbar`, `syncPreviewWebViews`, and `base.css`'s flush padding. No unit test pins it. **Only observable on a pane wider than 90 cells** — in a typical vertical split each pane is under the cap, `paintOffsetX == 0`, and the two kinds coincide, so a reviewer testing in a split will see no difference at all.

### Noted, not acted on

- SourceKit reports "No such module" for `XCTest`/`WolfFFI` in the editor. Confirmed artifacts: the worktree has no `compile_commands.json`, `buildServer.json`, `.bsp/`, or `.xcodeproj`, so SourceKit falls back to standalone parsing. Both files are Bazel-compiled sources (verified by `bazel query`) and both targets build clean.
- The two new files are **untracked** and need `git add` before commit.
- `WolfMacOS/`'s 33-bullet README list was not audited against its directory; only `WolfKeyMap/` was swept.

## Problem statement

On a Markdown preview pane wider than 90 cells, the statusbar's left edge does not line up with the preview content's left edge — it sits where editor text _would_ be.

`paintStatusbar` (`crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift` ~1219) computes the text origin as `gRect.minX + geo.paintOffsetX` — correct for editor panes, whose grid is painted at that same offset (`paintGrid` ~1198-1201). But `syncPreviewWebViews` (~836-838) frames the preview `WKWebView` to the **uncentered** `gridRect`, and the preview CSS is flush (`body.wolf-markdown { margin: 0; padding: 0 }`, a recent intentional change).

`paintOffsetX` is the centering inset applied when a pane exceeds `maxGridCols = 90` cells. So on a wide preview pane the statusbar text is indented relative to its own content by an amount that grows with pane width.

## Fix direction (fixed, do not revisit)

Align the **statusbar** to the **preview content**: for preview panes, do not add `paintOffsetX`, so the text lands at `gRect.minX`, matching the webview frame. **Do NOT center the webview instead** — that caps preview line length and is a different change than requested. **Editor panes must be bit-identical.**

---

# Architect's plan

## 1. The extracted seam

**New file: `crates/wolf-macos/Sources/WolfKeyMap/StatusbarTextSpan.swift`**

```swift
import Foundation

public enum PaneContentKind {
  case editor
  case preview
}

public struct StatusbarTextSpan {
  public let left: Double
  public let width: Double

  public init(gridLeft: Double, gridWidth: Double, paintOffsetX: Double, kind: PaneContentKind)
}
```

`Double`-based, Foundation-only (no `CGFloat`/CoreGraphics), matching `EditorInset` / `PaneGeometry` / `StatusbarLayout`. Scalars rather than a `Rect` — the caller holds a `CGRect` and needs `CGFloat` back, so two scalars keep the conversion to two casts each way.

**Placement rejections:**

- **`StatusbarLayout`** — its init takes _font sizes_; this takes _rect scalars + pane kind_. No shared input. It is documented in three places (file header, `Sources/README.md:23`, `Tests/README.md:53`) as specifically the **vertical** math; widening it invalidates all three descriptions of an otherwise stable seam.
- **`EditorInset`** — deliberately **pane-local** (its `gridRect` origin is the pane's top-left) and knows nothing about the 90-column cap or centering. Feeding `paintOffsetX` in would make it depend on a concept it correctly does not model. `textLeftInset` (`== padding`) is the _pane-local_ left inset and stays exactly as-is.
- **`PaneGeometry`** — models the grid, not the statusbar; has no notion of content kind and should not acquire one.

One-type-per-file with its own `Sources/README.md` bullet is the established convention (`PickerPanelSeam`, `EditorInset`, `StatusbarLayout`).

### Arithmetic

Let `w = max(0, gridWidth)` and `offset = min(max(0, paintOffsetX), w)` — the clamp handles a negative `paintOffsetX` (impossible today, defensive) and an offset larger than the grid, so `width` never goes negative and `left` never passes the grid's right edge.

- `.editor`: `left = gridLeft + offset`, `width = max(0, gridWidth - offset)`
- `.preview`: `left = gridLeft`, `width = w`

The editor branch reproduces today's expression **exactly** (`gRect.maxX - (gRect.minX + offset) == gridWidth - offset`), so editor panes are bit-identical. Only the clamp is new, and it is a no-op for every real input.

### Call-site rewrite (paintStatusbar, replacing 1236-1242, merging with 1249-1260)

1. Hoist a **single** preview lookup above the geometry block: `let previewName = previewNamesByGrid[grid]`.
2. Fetch `gRect` and `geo` as today, then construct `StatusbarTextSpan(gridLeft: Double(gRect.minX), gridWidth: Double(gRect.width), paintOffsetX: geo.paintOffsetX, kind: previewName != nil ? .preview : .editor)`; derive `textLeft = CGFloat(span.left)`, `textWidth = CGFloat(span.width)`.
3. Change the filename branch at 1251 from `if let name = previewNamesByGrid[grid]` to `if let name = previewName` — one lookup, one source of truth for "is this a preview".

`primaryRect` / `secondaryRect` (1288-1296) unchanged; they already consume `textLeft`/`textWidth`.

## 2. Pane kind: reuse `previewNamesByGrid`

**Recommendation: reuse the existing lookup.** Do not thread a new parameter or store a kind on `PaneBinding`.

- `paintStatusbar` already reads that map two lines below the geometry it must affect, and `drawScrollbars` (~1390) already uses exactly this predicate for exactly this purpose. A third mechanism for the same fact is a divergence risk.
- It is the only live record of preview-ness in scope at paint time. Deriving kind from `PaneBinding` would require re-querying `handle.previews()` during `draw(_:)` — an FFI call in the paint path, which nothing else does.

### Ordering hazard — assessed, NOT a problem

`syncPreviewWebViews()` runs at `onRenderWake` (~728, and ~715 on the refocus-rebind path) and is the **same function** that both rebuilds `previewNamesByGrid` (~811, 827) and sets `web.frame = gRect` (~838). `needsDisplay = true` follows at ~730; AppKit's `draw(_:)` runs later in the run loop, never synchronously inside `onRenderWake`. So on any frame after a wake, the map and the webview frames were written **atomically in the same pass**.

The other `needsDisplay = true` sites (471, 513, 637, 644, 936, 1463, 1474, 1484, 1611) repaint without rebuilding the map — but they also do not move the webview (`setFrameSize` ~633 calls only `syncGridSizes` + `refreshOverlays`). During a live resize both are stale together and converge on the wake `syncGridSizes` provokes. Pre-existing lag; this change neither creates nor worsens it.

**Failure mode if it were wrong** (recorded so a future reader recognizes it): a single-frame flash where a just-opened preview's statusbar is still indented while the HTML is flush. It does not occur. **The coder must not move the `previewNamesByGrid` rebuild out of `syncPreviewWebViews`, and must not reorder `syncPreviewWebViews()` after `needsDisplay = true`** — that atomicity is load-bearing and belongs in the doc comment.

## 3. Geometry, independently derived

`pad = EditorInset.padding = 20`, `cap = 90 * cellWidth`. `gridRect(ofPane:)` → `gRect.minX = P.minX + 20`, `gRect.width = max(0, P.width - 40)`. `geometry(for: gRect)` passes `gRect` as `paneRect`, so `paintOffsetX = max(0, (gRect.width - min(gRect.width, cap)) / 2)`.

| case | gRect.width | paintOffsetX | editor textLeft | preview textLeft | textWidth |
| --- | --- | --- | --- | --- | --- |
| narrow (< 90 cells) | `< cap` | 0 | `P.minX + 20` | `P.minX + 20` | `gRect.width` |
| wide (> 90 cells) | `> cap` | `(gRect.width - cap)/2` | `P.minX + 20 + offset` | `P.minX + 20` | editor `gRect.width - offset`; preview `gRect.width` |

**Preview matches the webview exactly:** `web.frame = gridRect(ofPane:)` so `web.frame.minX == gRect.minX == P.minX + 20`, and the CSS is flush. Preview `textLeft == web.frame.minX`. Confirmed.

**Right edge — fixed for free.** For previews `textWidth = gRect.width` gives `textRight == gRect.maxX == web.frame.maxX`. `.preview` simply spans the whole grid rect and **both** edges land on the webview frame. No extra work needed.

For **editors**, `textRight` remains `gRect.maxX` — i.e. `paintOffsetX` past the centered grid's right column. This is a **pre-existing** asymmetry, invisible in practice (it only widens the tail-truncation budget of a long path on a wide pane). **Out of scope — do not "fix" it**; changing it would alter editor truncation behavior, contradicting "editor panes completely unaffected". The `.editor` branch deliberately preserves it.

## 4. The strip itself does not change

`statusbarRect(ofPane:)` (~384) spans the full pane width and is unchanged; the background fill (`ctx.fill(strip)` ~1234) must keep covering the whole pane width for both kinds. Only the **text** is inset. No edit to `statusbarRect`, no edit to the fill.

## 5. Test plan

**Approach confirmed.** Swift unit tests over a pure `WolfKeyMap` seam is right here; CLAUDE.md's "start a real Neovim process" preference does not bite because no Neovim state participates — pane kind arrives as an enum, the rest is rect arithmetic. Mirrors `StatusbarLayoutTests` / `PaneGeometryTests` / `EditorInsetTests`. The extraction exists _precisely so_ there is something testable.

**File:** `crates/wolf-macos/Tests/WolfKeyMapTests/StatusbarTextSpanTests.swift` **Target:** the Bazel `swift_test` named `WolfKeyMapTests` in `crates/wolf-macos/BUILD.bazel` (~193). `srcs` lists are **explicit, not globbed**, for both the library (~45) and the test (~195) — both need an entry. There is no SwiftPM `Package.swift`; Bazel only.

Follow sibling style: file header stating the contract, `eps: Double = 1e-9`, helper constructors.

Cases:

1. **Editor, narrow** (`paintOffsetX == 0`) — `left == gridLeft`, `width == gridWidth`.
2. **Editor, wide** (`gridWidth = 1400`, `paintOffsetX = 250`) — `left == gridLeft + 250`, `width == 1150`; assert `left + width == gridLeft + gridWidth` (pins that the editor right edge is preserved, not silently changed).
3. **Preview, narrow** — identical to case 1. Pins that the fix is a no-op below the cap.
4. **Preview, wide** — `left == gridLeft` (ignores `paintOffsetX`), `width == gridWidth`; assert `left + width == gridLeft + gridWidth`.
5. **Kind-difference invariant** — same wide inputs: `editor.left - preview.left == paintOffsetX`; same narrow inputs: both `left` and `width` equal. **This is the assertion that most directly encodes the bug.**
6. **Non-zero `gridLeft`** — `20` and `512` (a right-hand split), both kinds: `left` is absolute, never pane-local.
7. **Zero-width pane** — `gridWidth == 0`, `paintOffsetX == 0`, both kinds: `width == 0`, `left == gridLeft`, no negative output.
8. **Pane narrower than the padding** — `gridWidth == 0` (what `EditorInset` clamps a sub-`2*padding` pane to) with `gridLeft` beyond the pane's right edge: `width == 0`, no negative. Comment that `EditorInset` produces the clamped 0.
9. **`paintOffsetX` larger than the grid** — `gridWidth = 100`, `paintOffsetX = 400`: editor `width == 0` and `left == gridLeft + 100` (never past the right edge); preview unaffected.
10. **Negative `paintOffsetX`** (defensive) — clamps to 0; editor equals the narrow case.
11. **Exact boundary** — `gridWidth == 90 * cellWidth` exactly → `paintOffsetX == 0`, both kinds identical. Pins continuity across the cap threshold.
12. **Universal invariants helper** (mirroring `assertInvariants` in `StatusbarLayoutTests`) over every case: `width >= 0`, `left >= gridLeft`, `left + width <= gridLeft + max(0, gridWidth)`.

### What this CANNOT test

That the statusbar origin and the _live_ `WKWebView` frame coincide on screen is an integration property spanning `paintStatusbar`, `syncPreviewWebViews`, and the CSS's zero left padding. No unit test can pin it. Mitigations, in order of value:

- **Pin the shared expression, not the two consumers.** Both derive from the _same_ `gridRect(ofPane:)` call. State this in **both** doc comments: the preview statusbar's left edge is `gridRect(ofPane:).minX` **because** that is the webview's frame origin, and the two must change together.
- A Rust-side assertion that `base.css` keeps `margin: 0; padding: 0` would pin the CSS half. Check whether `crates/wolf-core` already has such a test; if not, **do not add one** — separate concern, low value.
- The remainder rests on visual review: open a preview in a window wider than 90 columns, confirm the statusbar filename's left edge sits on the first heading's left edge.

**Do not attempt an AppKit/XCTest UI test** — no such harness exists here and adding one is disproportionate.

## 6. `pumAnchorX` — leave it, it is NOT the same defect

`pumAnchorX` (~788) returns `EditorInset.padding + <rendered prefix width>`, no `paintOffsetX`, no pane rect.

1. **Different coordinate space and anchor.** The popup is positioned in the container view's own `bounds` (~769-771) against the **cmdline**, a bottom-spanning overlay across the whole view — not inside any pane. There is no pane rect, grid rect, or `paintOffsetX` in scope; bare `EditorInset.padding` is correct for a view-wide overlay.
2. **Already self-consistent.** The popup aligns to the cmdline text, and `CmdlineView` uses the same bare padding. Adding a centering offset would _break_ that agreement, not fix one.

**Leave it and note it.** Add one sentence to its doc comment recording _why_ bare padding is right there, so the next reader doesn't "fix" it by analogy.

## 7. File-by-file

### Create

1. `Sources/WolfKeyMap/StatusbarTextSpan.swift` — per §1. House-style header: what it is, why it is a headless seam, that `.preview` deliberately ignores `paintOffsetX` because the webview is framed to the uncentered grid rect, and that only `width` clamps.
2. `Tests/WolfKeyMapTests/StatusbarTextSpanTests.swift` — per §5.

### Modify

3. **`crates/wolf-macos/BUILD.bazel`** — add the source to `wolf_key_map` `srcs` (~45) and the test to `WolfKeyMapTests` `srcs` (~195), both alphabetically after the `StatusbarLayout*` entries. Verify the comment block at ~25-33 globs `Tests/WolfKeyMapTests/*.swift` (no change expected there) and that no other explicit list exists.
4. **`PaneContainerView.swift`** — the call-site rewrite (§1); `paintStatusbar` doc comment (~1216-1218) currently says text is "left-aligned with the grid's text left edge (respecting `PaneGeometry.paintOffsetX`)" → rewrite to state both kinds; `syncPreviewWebViews` doc comment (~799-809) → add that `web.frame = gRect` (uncentered) is what `.preview` mirrors and that rebuilding the map in the same pass keeps them consistent within a frame; `pumAnchorX` doc comment (~783-787) → the §6 sentence. **No change** to `statusbarRect`, `gridRect(ofPane:)`, `geometry(for:)`, `paintGrid`, `drawScrollbars`.
5. **`crates/wolf-macos/Sources/README.md`** — new `StatusbarTextSpan.swift` bullet after `StatusbarLayout.swift` (~23) in the dense house style (type, `PaneContentKind`, init signature, both branches' arithmetic, the clamp, why preview ignores `paintOffsetX`); update the `StatusbarLayout.swift` bullet to say **vertical** and cross-reference the horizontal sibling; update the `EditorInset.swift` bullet (~29) to clarify `textLeftInset` is the _pane-local_ left padding and that the statusbar's absolute origin additionally depends on pane kind and the centering offset via `StatusbarTextSpan`; update the `PaneContainerView` bullet (~48) where it describes `paintStatusbar`. **Update the module-count phrase at ~6** ("Twenty-four deliberately AppKit-free files" → twenty-five) — **verify the count against the actual directory first**; if already stale, fix it to the true count rather than incrementing.
6. **`crates/wolf-macos/Tests/README.md`** — add a `StatusbarTextSpanTests` entry alongside `StatusbarLayoutTests` (~53) and `EditorInsetTests` (~95, 101), matching that file's structure. Update any test-count figure.
7. **`architecture.md`** — **read ~30-90 first.** Line 85 describes preview as "a SECOND kind of pane content: a web view." If that section enumerates consequences of the two-kinds split, add statusbar horizontal alignment (alongside the noted scrollbar suppression). If it is a one-line mention with no consequence list, **leave it alone — do not pad it.** Line 34 unaffected; no Rust change.

### Confirmed NOT changed

- **`crates/wolf-gnome`** — grepped case-insensitively for `statusbar|status_bar|statusline`: **zero matches**. Confirmed out of scope.
- **`crates/wolf-core/src/pane_status.rs`** — strings only, no geometry crosses the FFI. Confirmed unchanged.
- **`crates/wolf-core/src/markdown/base.css`** — the flush padding is what this fix aligns _to_. Unchanged. **Never prettier it.**

**No new dependencies.** `StatusbarTextSpan.swift` imports only `Foundation`; the test imports `XCTest` + `@testable import WolfKeyMap`, like its siblings.

## 8. Implementation order

1. Create `StatusbarTextSpan.swift` (compiles standalone against Foundation).
2. Add to `wolf_key_map` `srcs`; build the library alone to confirm it compiles before anything consumes it.
3. Create the test file, add to `WolfKeyMapTests` `srcs`, run the target — the seam is now pinned independently of the view.
4. Rewrite the `paintStatusbar` call site. Build the app target.
5. Doc comments in `PaneContainerView.swift`.
6. READMEs + `architecture.md` if warranted, then `prettier --write` on **only** the Markdown files.

Steps 1-3 cannot break the app; step 4 is the only behavioral change and lands after the seam is proven.

## 9. Lint

No ignore directives expected. Watch: swift-format line length (~100 cols, 2-space indent) — the `StatusbarTextSpan(...)` construction will wrap, follow surrounding style. Buildifier — keep both `srcs` lists sorted. Doc-comment coverage — every `public` member in `WolfKeyMap` carries `///`, so `PaneContentKind`, each case, the struct, `left`, `width`, and `init` all need one. Prettier on Markdown only.

## 10. Risks

- **Editor panes regressing** — lowest probability, highest impact. Mitigated structurally (the `.editor` branch is algebraically identical) and by test case 2 asserting `left + width == gridLeft + gridWidth`. **The coder must not "simplify" the editor branch to also drop `paintOffsetX`.**
- **Exactly 90 cells.** At `gridWidth == 90*cw`, `inset = max(0, 0/2) = 0` → both kinds identical. `paintOffsetX` is **continuous** across the threshold, so no jump or flicker as a window is dragged across it — the gap widens continuously from zero. Pinned by case 11.
- **A pane changing kind.** Editor→preview: the map insert and the webview frame happen in the same pass, then a redraw; the first frame showing the webview is the first frame using `.preview`. Preview→closed: the map is fully cleared (`removeAll` ~811) and rebuilt at the top of every sync, and `removePreviewWebView` runs in the same pass (~852-854), so stale `.preview` alignment cannot outlive the webview. **The `removeAll`-then-rebuild pattern is what makes this safe — do not convert it to an incremental update.**
- **Multi-pane splits.** `gridLeft` is absolute; the seam only adds to it. Case 6 covers a non-zero `gridLeft`. Note: in a vertical split each pane is typically **narrower than 90 cells**, so `paintOffsetX == 0` and the kinds coincide — **the bug and the fix are both only observable on wide panes**; a reviewer testing in a split may see no difference. Say so in the manual-smoke note.
- **Cmdline / pum.** Untouched; `pumAnchorX` shares no state with `paintStatusbar`. Risk is a reviewer applying the new branch by analogy — pre-empted by the §6 doc comment.
- **Scrollbars.** `drawScrollbars` keeps its own lookup and its own `gridRect` call; the hoisted `previewName` local is scoped to `paintStatusbar`. No change.
- **Preview webview sync.** Untouched in code. The only new coupling is documentary: `web.frame = gRect` is now load-bearing for the statusbar too. If someone later centers the preview webview (the rejected alternative), they must flip the statusbar back to `.editor` semantics — which is exactly what the cross-referencing doc comments exist to catch.
- **Live-resize lag.** During a live resize, `setFrameSize` repaints without re-framing the webview, so the two can visibly disagree mid-drag until the wake lands. **Pre-existing** and orthogonal; do not fix here. If it becomes distracting once alignment is noticeable, the follow-up is calling `syncPreviewWebViews()` from `setFrameSize` — a separate change with its own performance question.

---

## Workflow log

- [x] Problem statement written
- [x] Architect plan
- [x] Tests written (13)
- [x] Implementation + tests passing (cold run, 171 total)
- [x] Review (approved; two notes closed)
- [x] Documentation
- [ ] Visual confirmation — needs a preview pane wider than 90 cells
