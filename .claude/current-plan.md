# Plan: macOS file picker opens already expanded

## Problem statement

The macOS file picker (Shift+Ctrl+P) currently opens as a bare search field and
shows result rows only after the user types. We want it to open already
expanded — showing result rows immediately on open — matching the GNOME picker,
which shows the MRU-first result seed as soon as it opens.

## Key findings from initial investigation

- macOS picker: `crates/wolf-macos/Sources/WolfMacOS/PickerView.swift`.
  `open()` sets `rows = []` and does NOT call `refresh()`; `refresh()` gates on
  `PickerSeam.shouldShowResults(query:)` (empty/whitespace query => no rows).
- Pure gate: `crates/wolf-macos/Sources/WolfKeyMap/PickerSeam.swift`
  (`shouldShowResults`), unit-tested in
  `crates/wolf-macos/Tests/WolfKeyMapTests/PickerSeamTests.swift`.
- `wolf_core::rank_matches("", entries)` already returns ALL entries MRU-first
  (`crates/wolf-core/src/rank.rs`).
- GNOME picker (`crates/wolf-gnome/src/picker.rs`) already does the desired
  behavior: `open()` calls `refresh()` with an empty query, showing the MRU seed
  immediately. macOS should match GNOME.

## Proposed change (to be refined by architect)

- In `PickerView.open()`, call `refresh()` after setup so the empty-query
  MRU-first results appear immediately.
- Remove the `shouldShowResults` gate in `refresh()` so an empty query renders
  rows.
- Remove (or repurpose) the now-unused `shouldShowResults` seam + its tests.
- Update docs (source of truth per CLAUDE.md): `crates/wolf-macos/README.md`,
  `crates/wolf-macos/Sources/README.md` (both describe "shows NO result rows
  until the user types" and the picker eyeball checklist).

## Testing constraints

- App communicates with a real Neovim process — prefer real-nvim/integration
  tests over mocks.
- FFI file-search surface covered by `//crates/wolf-ffi:file_search`.
- macOS builds with Bazel via bazelisk.

## Status: COMPLETE — implemented, tests green, reviewed, docs verified

## Architect plan

### Summary

macOS mirrors GNOME: `PickerView.open()` calls `refresh()` so the empty-query
MRU-first result seed shows on open, and the `shouldShowResults` empty-query gate
is deleted (function + tests). Docs corrected. FFI empty-query path is already
covered by `//crates/wolf-ffi:file_search`; no new automated test needed. AppKit
`PickerView` is manual-smoke only (no headless window server).

### Files to modify

1. **`crates/wolf-macos/Sources/WolfMacOS/PickerView.swift`**
   - `open()`: keep `isOpen` guard, `start_file_search()`, `isOpen = true`,
     `isHidden = false`, `searchField.stringValue = ""`,
     `makeFirstResponder(searchField)`. Then set `lastQuery = nil` and call
     `refresh()`. Remove the manual `rows = []` / `tableView.reloadData()` /
     `updateResultsLayout()` lines and the "no rows until the user types" comment.
     Update the `open()` doc comment (now opens showing MRU-first seed, mirrors GNOME).
   - `refresh()`: delete the entire
     `guard PickerSeam.shouldShowResults(query: query) else { … }` block so an
     empty query flows to `ranked("", maxRows)`. Update `refresh()` doc comment.
   - File header comment: remove "empty-query gating" from the seam list.
   - No change to `updateResultsLayout()` (keys off `rows.isEmpty`, not the query).
2. **`crates/wolf-macos/Sources/WolfKeyMap/PickerSeam.swift`**
   - Delete the "macOS-only presentation seam" section + `shouldShowResults(query:)`.
     Keep `isPickerTrigger`, `nextSelection`, `restoredIndex`.
3. **`crates/wolf-macos/Tests/WolfKeyMapTests/PickerSeamTests.swift`** (TEST-WRITER)
   - Delete `// MARK: - shouldShowResults` section: `ShouldShowResultsCase` struct
     + `testShouldShowResultsTable()`. Keep the other three tables.
4. **`crates/wolf-macos/README.md`** — remove the `shouldShowResults` seam bullet;
   rewrite the `PickerView.swift` entry (opens showing MRU-first seed via `refresh()`);
   fix the known-limitations paragraph (drop `shouldShowResults` / "empty-query gate"
   wording) and the picker eyeball checklist (opens already populated, typing filters).
5. **`crates/wolf-macos/Sources/README.md`** — remove the `shouldShowResults` sentence
   from the `PickerSeam.swift` bullet; rewrite the `PickerView.swift` bullet.
   Reformat both `.md` files with `prettier --write`; no hard line breaks in a paragraph.
6. **`architecture.md`** — no change required (verified: does not assert "no rows
   until typing"; line 166 already notes the MRU seed is present synchronously).

### Edge case (layout on open)

`updateResultsLayout()` keys off `rows.isEmpty`. On open, `refresh()` runs
`ranked("")`: with MRU history the seed is synchronous so rows appear at once and
the panel expands. A fresh project with no MRU + no traversal batch yet opens
collapsed to the field, then self-corrects: each file-search wake re-runs
`refresh()` → `ranked("")` → `updateResultsLayout()`, expanding as entries stream
in. `open()` sets `lastQuery = nil` so the first refresh is `resetToTop = true`
(selects row 0); streaming wakes keep the empty query so selection is preserved/clamped.

### Implementation order

1. `PickerView.swift` (removes the only caller of the seam).
2. `PickerSeamTests.swift` (removes the test reference).
3. `PickerSeam.swift` (delete the now-unreferenced function).
4. `README.md` + `Sources/README.md` docs, then `prettier --write`.

### Test plan

- FFI empty-query `ranked("")` MRU-first: ALREADY covered by
  `crates/wolf-ffi/tests/file_search.rs` (`//crates/wolf-ffi:file_search`).
- Core empty-query MRU-first: ALREADY covered by `rank.rs`
  `empty_query_returns_all_mru_first`.
- Swift seams: subtractive only — remove `testShouldShowResultsTable`; the other
  three tables still pass (`//crates/wolf-macos:WolfKeyMapTests`).
- Manual smoke: eyeball checklist (updated in README) — picker opens expanded with
  MRU-first rows; typing filters; Up/Down/Enter/Esc as before.

### Verification commands (macOS Bazel)

```sh
bazel test //crates/wolf-macos:WolfKeyMapTests
bazel test //crates/wolf-ffi:file_search
bazel build //crates/wolf-macos:Wolf
bazel test //crates/wolf-core:wolf_core_lib_test   # optional, reconfirms rank test
```

