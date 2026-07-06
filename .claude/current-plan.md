# Plan: Move editor padding to a per-pane text-grid inset (wolf-macos)

## Problem statement

Currently the 20pt padding is a uniform OUTER margin around the whole editor
content region (the `EditorInset` seam offsets every pane by `(padding, padding)`
and shrinks the content area). Change it so the padding is a PER-PANE inset around
each pane's text grid instead.

### Desired layout (per pane)

```
+--------------------------------
| <h padding> filename            <- statusbar strip: TOP FLUSH, text h-padded
| <h padding> path/to/filename    <- statusbar secondary line
| <h padding><vertical padding>   <- vertical gap after statusbar
| <h padding> text text text      <- grid text, inset on all 4 sides
```

- Statusbar strip: flush with the TOP of its pane (no top margin), full pane
  width, `statusbarHeight` tall — as it was before the outer-margin change.
- Statusbar TEXT: horizontal padding (left-aligned with the grid text left edge).
- Below the statusbar: a vertical padding gap, then the grid.
- Grid (text area): inset by `padding` on ALL FOUR sides within the pane
  (left/right = padding; top = statusbarHeight + padding; bottom = padding).
- Cmdline strip text: horizontally indented by `padding` to align with grid text.
- Padding stays 20pt (`EditorInset.padding`); one value for h and v.

### Constraints / notes

- `MainWindow` needs NO dimensional change: the initial content already grows by
  `2*padding` in each dimension; that space now feeds the per-pane inset instead
  of the outer margin (the grid ends up the same size). Only comments change.
- `PaneContainerView`/`MainWindow` are AppKit glue (compile + manual-smoke only);
  headless test coverage lands on the reshaped `EditorInset` seam.

## Status

- [x] Architect plan
- [x] Tests written — `EditorInsetTests.swift` rewritten for the per-pane API
- [x] Implementation + tests passing — WolfKeyMapTests PASS; wolf_macos_lib + Wolf build
- [x] Review — APPROVED, no correctness issues (alignment, single-inset, hit-test consistency all verified)
- [x] Documentation — Sources/Tests/README.md all updated; `prettier --write` clean (no changes needed)

## COMPLETE ✅

Padding moved from an outer margin to a per-pane text-grid inset. `EditorInset` seam
reshaped (per-pane `gridRect` + `textLeftInset`); `PaneContainerView` reverted to
full-bounds consumers with the inset applied only in `gridRect(ofPane:)`; statusbar
top-flush with horizontally-padded text aligned to the grid; cmdline text indented;
`MainWindow` dims unchanged (comments only). WolfKeyMapTests PASS; `wolf_macos_lib` +
`Wolf` build; reviewer APPROVED; docs synced + prettier-clean. REMAINING (not
automatable): manual GUI smoke — launch the app and confirm each pane's statusbar is
flush at the top with indented text, the grid has padding on all four sides with a
vertical gap after the statusbar, the cmdline text aligns with the grid text, and the
initial window still opens at the exact cols×rows.

## Architect plan

### 1. Reshape `EditorInset.swift` (per-pane grid inset)

Replace the whole type. New public API (Foundation-only, `Double`, reuses `Rect`,
doc comment every member):
- Keep `public static let padding: Double = 20`.
- `public init(paneWidth: Double, paneHeight: Double, statusbarHeight: Double, padding: Double)`.
- Echoed stored inputs: `paneWidth`, `paneHeight`, `statusbarHeight`, `padding`.
- `public let gridRect: Rect` — PANE-LOCAL (origin at pane top-left):
  `x = padding`, `y = statusbarHeight + padding`,
  `w = max(0, paneWidth - 2*padding)`, `h = max(0, paneHeight - statusbarHeight - 2*padding)`.
  Only `w`/`h` clamp; `x`/`y` are fixed offsets (not clamped).
- `public let textLeftInset: Double` (`== padding`) — statusbar/cmdline text h-padding.
- Rewrite the header + struct doc comments. REMOVE all old members
  (containerWidth/Height, stripHeight, originX/Y, contentWidth/Height,
  paneAreaWidth/Height, cmdlineStripRect, backgroundRect).

### 2. `PaneContainerView.swift` — method by method

- REMOVE the container-level `editorInset()` helper.
- REMOVE the `statusbarTextTrailingInset` constant (role replaced by grid-right alignment).
- ADD back `private var paneAreaHeight: CGFloat { max(0, bounds.height - stripHeight) }`.
- `paneRects()` → full bounds, (0,0)-origin, NO offset:
  `tree.paneRects(width: bounds.width, height: paneAreaHeight)` mapped straight to CGRect.
  Update doc comment (drop "offset by inset"; inset now applied in `gridRect(ofPane:)`).
- `gridRect(ofPane rect:)` → inset via the reshaped seam:
  ```
  let inset = EditorInset(paneWidth: Double(rect.width), paneHeight: Double(rect.height),
                          statusbarHeight: Double(statusbarHeight), padding: EditorInset.padding)
  return CGRect(x: rect.minX + inset.gridRect.x, y: rect.minY + inset.gridRect.y,
                width: inset.gridRect.w, height: inset.gridRect.h)
  ```
- `statusbarRect(ofPane:)` UNCHANGED (top-flush, full width).
- `paintStatusbar` → align text with grid text left/right (use local `gRect` to avoid
  collision with the `grid: UInt64` param):
  ```
  let gRect = gridRect(ofPane: paneRect)
  let geo = geometry(for: gRect)
  let textLeft = gRect.minX + CGFloat(geo.paintOffsetX)
  let textWidth = max(0, gRect.maxX - textLeft)
  ```
  Strip bg still fills full strip width; primary/secondary `y` origins unchanged.
- `paintStripRow` (cmdline) → x-translate by `CGFloat(EditorInset.padding)` instead of
  `stripRect.minX` (stripRect is back at x=0), so cmdline text aligns with grid text.
- `paintCmdlineStrip` → `stripRect = CGRect(x: 0, y: paneAreaHeight, width: bounds.width, height: stripHeight)`.
- `drawDividers` → `areaW = bounds.width`, `areaH = paneAreaHeight`.
- `applyFocus` → `tree.neighbor(origin, dir, width: Double(bounds.width), height: Double(paneAreaHeight))`.
- `syncGridSizes` grid 1 → `gridCells(width: bounds.width, height: bounds.height)`.
  Per-pane loop UNCHANGED (routes through `gridRect(ofPane:)`).
- `draw(_:)` UNCHANGED (fills `bounds` with live grid default_bg / fallbackBg — now
  paints the per-pane padding regions). Update only the inline comment.
- Mouse/scroll + `drawScrollbars` — NO change beyond reverted `paneRects` (they route
  through `geometry(for: gridRect(ofPane: rect))`; padding points clamp to nearest edge cell).

### 3. `MainWindow.swift` — comments only (NO dimensional change)

`editorWidth = cols*cellWidth + 2*padding`, `editorHeight = rows*cellHeight + cmdlineStripHeight + 2*padding`
stay as-is. Verified identity: new grid height `= paneHeight - statusbarHeight - 2*padding
= rows*cellHeight - statusbarHeight` == today's; width → `cols*cellWidth`. Update the
comments (lines ~45-47, ~50-53) to say the `2*padding` now reserves the PER-PANE inset.

### 4. Padding constant

Still `EditorInset.padding`, consumed by `gridRect(ofPane:)`, `paintStripRow`, `MainWindow`.

### 5. BUILD.bazel

No source-list change — both files already listed; reshape/rewrite in place.

### 6. Tests — rewrite `EditorInsetTests.swift`

Keep `eps`(1e-9)/`approx`/`assertRectEqual` + `@testable import WolfKeyMap`. Rewrite
the contract comment. Cases:
- Exact: `EditorInset(paneWidth: 840, paneHeight: 600, statusbarHeight: 40, padding: 20)`
  → `gridRect == Rect(x: 20, y: 60, w: 800, h: 520)`, `textLeftInset == 20`.
- `XCTAssertEqual(EditorInset.padding, 20, accuracy: eps)`.
- Structural invariants (2nd pane 1000×700, statusbar 37, padding 20):
  `gridRect.x == padding`; `gridRect.y == statusbarHeight + padding`;
  `gridRect.w == paneWidth - 2*padding`; `gridRect.h == paneHeight - statusbarHeight - 2*padding`;
  `gridRect.w + 2*padding == paneWidth`; `gridRect.y + gridRect.h + padding == paneHeight`;
  `textLeftInset == padding == gridRect.x`.
- Narrow pane (30×600, sb 40) → `gridRect.w == 0`.
- Short pane (840×60, sb 40) → `gridRect.h == 0`.
- Statusbar taller than pane (840×50, sb 80) → `gridRect.h == 0` (`gridRect.y == 100` ok).

### 7. Docs (rewrite, then `prettier --write`)

- `Sources/README.md`: line 25 EditorInset bullet → per-pane API; line 41 PaneContainerView
  bullet → statusbar flush top + h-padded text, grid inset all 4 sides, cmdline text
  indented, full-bounds consumers, `draw` paints per-pane padding; line 37 MainWindow
  bullet → 2*padding reserves per-pane inset. "Nineteen ... files" count UNCHANGED.
- `Tests/README.md`: lines 81-83 EditorInsetTests section → new cases; adjust the intro
  "uniform-20pt-margin" wording.
- `README.md`: line 61 EditorInset bullet; line 73 MainWindow bullet; line 76
  PaneContainerView bullet — same reshape.

### 8. Implementation order (build green only after seam + consumers agree)

1. Reshape `EditorInset.swift`. 2. Update `PaneContainerView.swift` (all edits). 3.
`MainWindow.swift` comments. 4. Rewrite `EditorInsetTests.swift`. 5. Docs + prettier.
6. Run Bazel targets.

### 9. Bazel targets

`bazelisk test //crates/wolf-macos:WolfKeyMapTests`;
`bazelisk build //crates/wolf-macos:wolf_macos_lib` (+ optionally `:Wolf`).

### Load-bearing details
- `stripHeight == cmdlineStripHeight == ceil(cellHeight)` in both files — the identity relies on it.
- `paintStatusbar` must base `textLeft` on the INSET `gRect.minX` (+ paintOffsetX), not `paneRect.minX`.
- `paintStripRow` must use `EditorInset.padding` (stripRect.minX is now 0).
