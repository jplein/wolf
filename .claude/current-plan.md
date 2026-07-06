# Plan: 20pt padding on all four sides of the editor text grid (wolf-macos)

## Problem statement

Add 20pt of padding on all four sides of the editor text grid in the macOS
front-end (`wolf-macos`), matching the macOS-standard leading inset of the native
titlebar text after the sidebar.

The editor content region (panes + per-pane statusbar strips + bottom cmdline
strip) should be inset by a uniform 20pt margin from the content view's edges on
all four sides. The margin is painted in the editor background.

### Constraints

- Preserve the exact initial `cols × rows` grid: `MainWindow`'s initial
  content-size math must grow by `2 * padding` in both width and height, so the
  OCR-checked initial grid (e.g. 80 cols) is undisturbed.
- Padding value chosen by the user: **20pt**.

### Touchpoints (from initial investigation)

- `PaneContainerView.swift` — pane layout, `paneAreaHeight`, `paneRects`, cmdline
  strip position, mouse/scroll hit-testing, `syncGridSizes`, bounds background
  fill.
- `MainWindow.swift` — initial content-size math.

### Preference

Factor the inset arithmetic into a pure, headlessly-testable `WolfKeyMap` seam
(Foundation-only, `Double`-based) so it gets `XCTest` coverage like the other
seams (`PaneGeometry`, `StatusbarLayout`, etc.).

## Status

- [x] Architect plan
- [x] Tests written — `Tests/WolfKeyMapTests/EditorInsetTests.swift` (+ BUILD.bazel test src)
- [x] Implementation + tests passing — WolfKeyMapTests PASS; wolf_key_map, wolf_macos_lib, Wolf all build
- [x] Review — APPROVED; dead `paneAreaHeight` property removed per reviewer note (rebuilt, passes)
- [x] Documentation — Sources/Tests/README.md all updated; `prettier --write` clean (no changes needed)

## COMPLETE ✅

All steps done. Headless `//crates/wolf-macos:WolfKeyMapTests` PASS; `wolf_key_map`,
`wolf_macos_lib`, and the `Wolf` app bundle all build. Reviewer APPROVED. Docs synced +
prettier-clean. REMAINING (not automatable): manual GUI smoke on a window server — launch
the app and confirm the 20pt margin appears on all four sides of the editor grid, the
cmdline strip + statusbar text align at `x = padding`, mouse/scroll hit-testing lands on
the right cells, and the initial window still opens at the exact `cols × rows` grid.

### Coder's optional test suggestions (deferred unless reviewer flags a gap)
- `padding == 0` no-op case (originX/Y == 0, contentWidth == containerWidth).
- `stripHeight == contentHeight` boundary → paneAreaHeight == 0.
Rationale for deferral: plan §6 already covers the clamps + representative geometry.

### Note on SourceKit diagnostics (false positives)
`Cannot find type 'Rect'`, `No such module 'WolfFFI'/'WolfKeyMap'`, `No such module 'XCTest'`
are SourceKit-without-Bazel artifacts; the authoritative `bazelisk build`/`test` all pass.

## Architect plan

### 1. New pure seam — `Sources/WolfKeyMap/EditorInset.swift`

`import Foundation` only, `Double`-based, reuses `public struct Rect` from
`PaneTree.swift`. Header comment in the `PaneGeometry.swift` / `StatusbarLayout.swift`
style; doc-comment every public member; no AppKit/CoreGraphics, no force-unwraps.

```
public struct EditorInset {
    public static let padding: Double = 20   // single source of truth (points)

    // Echoed inputs
    public let containerWidth: Double
    public let containerHeight: Double
    public let padding: Double
    public let stripHeight: Double

    // Derived outputs
    public let originX: Double          // = padding
    public let originY: Double          // = padding
    public let contentWidth: Double     // = max(0, containerWidth  - 2*padding)
    public let contentHeight: Double    // = max(0, containerHeight - 2*padding)
    public let paneAreaWidth: Double    // = contentWidth
    public let paneAreaHeight: Double   // = max(0, contentHeight - stripHeight)
    public let cmdlineStripRect: Rect   // VIEW coords, offset by padding
    public let backgroundRect: Rect     // = Rect(0, 0, containerWidth, containerHeight)

    public init(containerWidth: Double, containerHeight: Double,
                padding: Double, stripHeight: Double)
}
```

Semantics in `init`:
- `originX = padding`, `originY = padding`.
- `contentWidth = max(0, containerWidth - 2*padding)`, `contentHeight = max(0, containerHeight - 2*padding)` (clamp non-negative).
- `paneAreaWidth = contentWidth`.
- `paneAreaHeight = max(0, contentHeight - stripHeight)` (clamp non-negative).
- `cmdlineStripRect = Rect(x: originX, y: originY + paneAreaHeight, w: contentWidth, h: stripHeight)` — VIEW coords.
- `backgroundRect = Rect(x: 0, y: 0, w: containerWidth, h: containerHeight)`.

### 2. `PaneContainerView.swift` changes

- New helper in the Geometry section:
  ```
  private func editorInset() -> EditorInset {
      EditorInset(containerWidth: Double(bounds.width),
                  containerHeight: Double(bounds.height),
                  padding: EditorInset.padding,
                  stripHeight: Double(stripHeight))
  }
  ```
- `paneAreaHeight` → `CGFloat(editorInset().paneAreaHeight)`.
- `paneRects()` — build inset once; OFFSET each pane rect by `(originX, originY)` and
  size the layout from `paneAreaWidth`/`paneAreaHeight`:
  ```
  let inset = editorInset()
  return tree.paneRects(width: inset.paneAreaWidth, height: inset.paneAreaHeight)
      .map { ($0.0, CGRect(x: $0.1.x + inset.originX, y: $0.1.y + inset.originY,
                           width: $0.1.w, height: $0.1.h)) }
  ```
  All downstream consumers (`geometry(for:)`, `gridRect(ofPane:)`, `statusbarRect(ofPane:)`,
  mouse handlers, `paneRect(forGrid:)`, `drawScrollbars`) already operate on these offset
  rects — do NOT double-offset there.
- `applyFocus(dir:from:)` — pass `inset.paneAreaWidth`/`inset.paneAreaHeight` to
  `tree.neighbor(...)` (dims must match `paneRects()`).
- `syncGridSizes()` — keep `bounds > 0` guard; derive grid 1 from the inset CONTENT area:
  ```
  let inset = editorInset()
  let global = gridCells(width: CGFloat(inset.contentWidth), height: CGFloat(inset.contentHeight))
  ```
  Per-pane loop unchanged.
- `paintCmdlineStrip(_:)` — use `editorInset().cmdlineStripRect` for `stripRect`.
- `paintStripRow(...)` — **critical**: change x-translate from `0` to `stripRect.minX`
  so the cmdline row + cursor start at `x = padding` and align with the offset pane grids.
- `drawDividers(_:ctx:)` — compare against content-region edges, not bounds:
  `areaW = CGFloat(inset.originX + inset.paneAreaWidth)`,
  `areaH = CGFloat(inset.originY + inset.paneAreaHeight)`.
- `draw(_:)` — keep filling `bounds` with `WolfColor.fallbackBg` (paints the margins in
  editor bg). No change required.

### 3. `MainWindow.swift` changes

In the convenience initializer (lines 43-47): fold padding into `editorWidth`/`editorHeight`.
- `let padding = CGFloat(EditorInset.padding)`.
- `editorWidth = CGFloat(cols) * metrics.cellWidth + 2 * padding`.
- `cmdlineStripHeight = ceil(metrics.cellHeight)` (unchanged).
- `editorHeight = CGFloat(rows) * metrics.cellHeight + cmdlineStripHeight + 2 * padding`.
Designated initializer needs NO structural change (values flow through). Update the
explanatory comments to note the padding is included so the pane area stays exactly
`cols × rows` inside the 20pt inset.

### 4. Padding constant source of truth

`EditorInset.padding` (`static let = 20`), consumed by both `PaneContainerView` and
`MainWindow`.

### 5. BUILD.bazel (both explicit, alphabetical)

- `wolf_key_map.srcs`: insert `"Sources/WolfKeyMap/EditorInset.swift",` between
  `CommandSeam.swift` and `FontSizeSeam.swift`.
- `WolfKeyMapTests.srcs`: insert `"Tests/WolfKeyMapTests/EditorInsetTests.swift",`
  between `CommandSeamTests.swift` and `FontSizeSeamTests.swift`.

### 6. Test plan — `Tests/WolfKeyMapTests/EditorInsetTests.swift`

`@testable import WolfKeyMap`, `import XCTest`. Mirror `PaneGeometryTests`/`StatusbarLayoutTests`
(header comment, `eps`/`approx` + `assertRectEqual`). Cases:
- Exact: container `840 × 640`, padding `20`, strip `16` → `originX/Y == 20`,
  `contentWidth == 800`, `contentHeight == 600`, `paneAreaWidth == 800`,
  `paneAreaHeight == 584`, `cmdlineStripRect == Rect(20, 604, 800, 16)`,
  `backgroundRect == Rect(0, 0, 840, 640)`.
- `XCTAssertEqual(EditorInset.padding, 20)`.
- Structural invariants (second case, e.g. `1000 × 700`, strip `18`):
  `contentWidth + 2*padding == containerWidth`; `contentHeight + 2*padding == containerHeight`;
  `paneAreaHeight + stripHeight == contentHeight`; `cmdlineStripRect.y == originY + paneAreaHeight`;
  `cmdlineStripRect.w == contentWidth`; `cmdlineStripRect.h == stripHeight`; origins == padding.
- Edge: container `30 × 30`, padding `20` → content `0 × 0`, paneArea `0 × 0`, strip.w `0` (no negatives).
- Edge: container exactly `40 × 40` → content `0 × 0`.
- Edge: strip taller than content — container `48 × 44` (content h `4`), strip `16` → `paneAreaHeight == 0`.

`PaneContainerView`/`MainWindow` are window-server-bound AppKit glue — compile + manual
smoke only; all headless coverage lands on `EditorInset`.

### 7. Implementation order (never breaks the build)

1. Create `EditorInset.swift` + add to `wolf_key_map.srcs`; build `//crates/wolf-macos:wolf_key_map`.
2. Create `EditorInsetTests.swift` + add to `WolfKeyMapTests.srcs`; run `//crates/wolf-macos:WolfKeyMapTests`.
3. Modify `PaneContainerView.swift` (helper + edits); build `//crates/wolf-macos:wolf_macos_lib`.
4. Modify `MainWindow.swift`; build `//crates/wolf-macos:wolf_macos_lib`.
5. Update docs + `prettier --write`.

### 8. Docs to update

- `Sources/README.md` — line 5 "Eighteen" → "Nineteen"; add `EditorInset.swift` bullet;
  amend `PaneContainerView.swift` + `MainWindow.swift` bullets (20pt margin, consumes
  `EditorInset`, MainWindow grows initial content by `2*padding`).
- `Tests/README.md` — line 3 intro list add the editor-inset seam; add
  `## WolfKeyMapTests/EditorInsetTests.swift` section.
- `README.md` — add `EditorInset` seam paragraph; add to the `wolf_key_map` compile
  enumeration (line ~61); update `MainWindow.swift` (line ~71) + `PaneContainerView.swift`
  (line ~74) bullets.

### 9. Test targets

- `bazelisk test //crates/wolf-macos:WolfKeyMapTests`
- `bazelisk build //crates/wolf-macos:wolf_macos_lib` (compile-verify glue).

### Key correctness notes

- `paintStripRow` x-translate must become `stripRect.minX` (else cmdline clipped/misaligned).
- Do NOT double-offset in per-pane geometry helpers/hit-testers — `paneRects()` already offsets.
- `drawDividers` compares against `originX + paneAreaWidth` / `originY + paneAreaHeight`.
