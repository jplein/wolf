# Plan: Redesign the file picker UI (keep it general/reusable)

## Feature request

Redesign the macOS file picker (`crates/wolf-macos/Sources/WolfMacOS/PickerView.swift`).
Reusable for other selectors (e.g. a buffer selector), so keep the row/cell rendering GENERAL.

1. Larger font in the text entry (search) field.
2. No focus outline (focus ring) for the text entry field.
3. Picker starts with NO result entries — just the text field.
4. Once the user types, entries appear.
5. Larger entry rows: left = medium Finder icon (aligned with the magnifier); base filename
   (aligned with the typed text); below it, in a smaller font, the path to the file relative
   to cwd (see decision on subtitle below).

## Decision: subtitle = relative PARENT directory

The subtitle is the file's relative parent directory (display path minus its last component),
not the full relative path — the standard fuzzy-finder convention, avoids duplicating the
filename already shown as the title. One-line reversible (use `entry.display()` directly).

## Files

### CREATE `crates/wolf-macos/Sources/WolfMacOS/PickerRow.swift`
File-agnostic value type:
```
struct PickerRow {
    let icon: NSImage?    // medium icon; Finder icon for files
    let title: String     // primary line (base filename)
    let subtitle: String  // secondary line (relative parent dir); may be ""
    let payload: String   // opaque activation token (abs path for files)
}
```
`payload` is opaque; only `activateSelection()` consumes it. A buffer selector would set
title = buffer name, subtitle = detail, payload = a buffer id/token.

### CREATE `crates/wolf-macos/Sources/WolfMacOS/PickerRowCellView.swift`
`final class PickerRowCellView: NSTableCellView`, renders any `PickerRow`:
- `NSImageView` for the icon; two `NSTextField`s (title, subtitle) in a vertical `NSStackView`
  (`alignment = .leading`, small spacing), vertically centered.
- `configure(with row: PickerRow)` sets image/title/subtitle.
- Static layout constants + fonts: `iconSize`, `titleFontSize`, `subtitleFontSize`, `iconToTextGap`.
- Init takes alignment insets (`iconLeadingInset`, `labelLeadingInset`) computed by PickerView
  from the search-field cell rects; used as leading-constraint constants.
- `static func rowHeight(iconSize:titleFont:subtitleFont:verticalPadding:) -> CGFloat`
  = `max(iconSize, titleLineHeight + subtitleLineHeight + interlineGap) + verticalPadding`
  via `NSLayoutManager().defaultLineHeight(for:)`.
- Constraints: imageView leading == cell.leading + iconLeadingInset, centerY, width/height == iconSize;
  stackView leading == cell.leading + labelLeadingInset, centerY, trailing <= cell.trailing - inset.
  Labels: `.byTruncatingMiddle`, `drawsBackground = false`.
- `labelLeadingInset = max(searchTextLeadingInset, iconLeadingInset + iconSize + iconToTextGap)`
  so text never overlaps a wide medium icon while aligning to the typed-text column.

### MODIFY `crates/wolf-macos/Sources/WolfKeyMap/PickerSeam.swift`
Add three pure, Foundation-only static funcs (documented **macOS-only presentation seams, no
Rust/wolf-gnome parity**, unlike the existing three trigger/nav/restore ports):
```
static func shouldShowResults(query: String) -> Bool
    // !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
static func baseName(fromRelativePath relativePath: String) -> String
    // (relativePath as NSString).lastPathComponent
static func parentRelativePath(fromRelativePath relativePath: String) -> String
    // (relativePath as NSString).deletingLastPathComponent
```

### MODIFY `crates/wolf-macos/Sources/WolfMacOS/PickerView.swift`
- State: replace `rowDisplays`/`rowPaths` with `rows: [PickerRow] = []`. Update `numberOfRows`,
  `moveSelection`, `activateSelection`, `refresh` to use `rows`.
- Constants: add `searchFontSize` (~20). Row fonts live on `PickerRowCellView`.
- `buildSubviews()`:
  - `searchField.font = NSFont.systemFont(ofSize: searchFontSize)`.
  - Focus ring: `searchField.focusRingType = .none` and
    `(searchField.cell as? NSSearchFieldCell)?.focusRingType = .none`.
  - Register `PickerRowCellView` for reuse via `cellIdentifier`.
  - `tableView.rowHeight = PickerRowCellView.rowHeight(...)` (fixed).
  - Compute alignment insets once from `NSSearchFieldCell.searchButtonRect(forBounds:)` and
    `searchTextRect(forBounds:)` (after setting the field's final width): `iconLeadingInset =
    buttonRect.minX`, `searchTextLeadingInset = textRect.minX`; fallback constants
    (iconLeadingInset ~8, searchTextLeadingInset ~32) if the cell cast fails.
- `open()`: remove the forced MRU seed. Start the search + focus the field, but set `rows = []`,
  `tableView.reloadData()`, `lastQuery = nil`; do NOT produce rows for the empty query.
- `refresh()`: gate at top after reading `query`:
  `guard PickerSeam.shouldShowResults(query: query) else { rows = []; lastQuery = query;
   tableView.reloadData(); return }`. Then map each ranked `FfiFileEntry` -> `PickerRow` at the
  boundary: `display = entry.display().toString()`, `abs = entry.path().toString()`,
  `icon = fileIcon(forPath: abs)`, title = `baseName(fromRelativePath: display)`,
  subtitle = `parentRelativePath(fromRelativePath: display)`, payload = abs.
  Keep the existing `restoredIndex`/selection logic on `rows.count`.
- `tableView(_:viewFor:row:)`: dequeue/create `PickerRowCellView`, call `configure(with: rows[row])`.
- `activateSelection()`: `handle.open(rows[idx].payload)`; keep the zero-row no-op.
- Remove `makeCell()`.
- Add private `fileIcon(forPath: String) -> NSImage`: `NSWorkspace.shared.icon(forFile:)` with
  `.size = NSSize(width: iconSize, height: iconSize)`.
- NOTE: `display()`/`path()` return `RustString` — call `.toString()` before the seam funcs.

### MODIFY `crates/wolf-macos/Tests/WolfKeyMapTests/PickerSeamTests.swift`
Table-driven (existing style), the only automated tests here:
- `testShouldShowResultsTable`: "" -> false; "   " -> false; "\n\t" -> false; "a"/"foo" -> true;
  " foo " -> true.
- `testBaseNameTable`: "" -> ""; "README.md" -> "README.md"; "src/foo/bar.swift" -> "bar.swift";
  "a/b/c" -> "c"; "dir/" -> "dir" (NSString.lastPathComponent robustness).
- `testParentRelativePathTable`: "" -> ""; "README.md" -> ""; "a/b.txt" -> "a";
  "src/foo/bar.swift" -> "src/foo"; "a/b/c" -> "a/b".

## Concrete constants
searchFontSize=20, titleFontSize=15, subtitleFontSize=11 (secondary label color),
iconSize=28, iconToTextGap=8, cellHorizontalInset=8, interline gap ~2, rowVerticalPadding=10.

## AppKit-only (manual-smoke, not unit-tested)
PickerRow, PickerRowCellView, icon fetch, search-field font/focus-ring, the NSSearchFieldCell
rect-based inset computation, and open/refresh/viewFor wiring — all need AppKit/live field editor.
Only the empty-query gate + filename/parent derivations are pure seams and unit-tested.

## Build
No BUILD.bazel change — `wolf_macos_lib` globs `Sources/WolfMacOS/*.swift`; seam/tests added to
listed files. Targets: `//crates/wolf-macos:wolf_macos_lib`, `//crates/wolf-macos:WolfKeyMapTests`.
(The Go/golangci lint notes in the generic brief do NOT apply — this is Swift + Bazel.)

## Implementation order
1. PickerSeam funcs. 2. PickerSeamTests. 3. PickerRow.swift. 4. PickerRowCellView.swift.
5. PickerView.swift rewire. 6. READMEs + `prettier --write`.

## Docs to update (source of truth)
`crates/wolf-macos/Sources/README.md` (PickerSeam bullet + 3 new funcs flagged macOS-only;
PickerView bullet; add PickerRow.swift + PickerRowCellView.swift bullets) and
`crates/wolf-macos/README.md` (PickerSeam list; PickerView bullet; picker eyeball paragraph).
Reformat Markdown with `prettier --write` after.

## Manual-smoke eyeball checklist
1. Open: only the search field, no rows; larger font; no focus ring.
2. Type a char -> rows appear; delete to empty -> rows disappear.
3. Rows taller: medium Finder icon left, base filename primary, relative parent dir smaller/dimmer below.
4. Row icon left edge aligns with the magnifier; filename left edge aligns with typed text; subtitle
   left-aligned under filename.
5. A cwd-root file shows empty subtitle, row height unchanged.
6. Up/Down move selection; Enter/double-click open + close; Esc closes; zero rows -> Enter no-op.
7. Streaming wakes fill rows while a non-empty query is present; selection preserved/clamped.

## Status: COMPLETE

- test-writer: added 3 table-driven tests to PickerSeamTests.swift.
- coder: implemented all files; build OK, tests PASSED; docs updated + prettier-clean.
- reviewer: APPROVED — all 6 verification areas PASS; flagged one comment-wording nuance.
- coder (follow-up): reworded the PickerRowCellView.configure comment; rebuild OK.
- technical-writer: verified both READMEs accurate + complete + prettier-clean; no changes needed.

- test-writer: added 3 table-driven tests to PickerSeamTests.swift.
- coder: implemented all files; `bazel build //crates/wolf-macos:wolf_macos_lib` OK,
  `bazel test //crates/wolf-macos:WolfKeyMapTests` PASSED. Docs updated + prettier-clean.
- Deviation accepted: reuse via dequeue-or-create (`makeView ?? makeCell`) instead of
  nib-based `register(_:forIdentifier:)` (which needs a nib; the cell needs injected insets).
  Functionally equivalent.
