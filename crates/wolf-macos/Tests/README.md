# wolf-macos/Tests

The Swift tests for the macOS front-end. The Bazel target is `//crates/wolf-macos:WolfKeyMapTests`; it runs regardless of `WOLF_NVIM_BIN` (it needs no Neovim) and covers the three pure seams — the key seam (`KeyMapTests.swift`), the picker seam (`PickerSeamTests.swift`), and the pane-layout seam (`PaneTreeTests.swift`).

## `WolfKeyMapTests/KeyMapTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.translate(...)` seam — the macOS analog of wolf-gnome's headless `gdk_key` tests. It exercises the function with scalar inputs only (no `NSEvent`, no window, no event loop) — exactly the data an `NSView.keyDown(_:)` pulls off an `NSEvent` — and asserts the **Shift contract**, which must match `wolf-core/src/input.rs`:

- a plain printable char maps to `Char` with all flags false;
- a shifted printable (`Shift+a` → `A`, `Shift+1` → `!`) delivers the already-shifted glyph in `ch` with `shift = false` (Swift never sets `shift` on a `Char`);
- named keys (Return, Escape, Tab, forward-delete, the arrows, Home/End/PageUp/ PageDown, F1/F5) are recognized from `keyCode` and **keep** the real `shift` (`Shift+Tab`, `Shift+Left` stay `shift = true`);
- ctrl/alt/meta combos take the glyph from `charactersIgnoringModifiers` (lowercased) — `Ctrl+w` → `w` (not U+0017), `Alt+x` → `x`, `Cmd+s` → `s`, `Ctrl+Shift+w` → `w` with `shift = false`;
- bare modifier keys and unmodified control characters map to `nil`.

The test pins the `Kind` discriminants and the `kVK_*` virtual key codes (notably Backspace = `kVK_Delete` 0x33 and forward-delete = `kVK_ForwardDelete` 0x75) so they stay in lockstep with the Rust `FfiKeyEvent` `kind` scheme and the Swift `WolfKeyMap`.

## `WolfKeyMapTests/PickerSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `PickerSeam` helpers (`@testable import WolfKeyMap`), table-driven to mirror wolf-gnome's picker.rs tables:

- `isPickerTrigger(charactersIgnoringModifiers:ctrl:shift:)` — the **Shift+Control+P** contract (including Ctrl+Shift+Alt+P → `true`, the wrong letter → `false`, and a missing modifier → `false`);
- `nextSelection(current:delta:nRows:)` — the list-navigation clamp across its cases (including the empty-list `nil`);
- `restoredIndex(resetToTop:prev:newLen:)` — selection restore across streaming result batches.

## `WolfKeyMapTests/PaneTreeTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.PaneTree` seam (`@testable import WolfKeyMap`) — a 1:1 port of wolf-gnome's `layout.rs` test suite (same `P1…P4` ids, `W = 100` / `H = 40` bounds, half/third/quarter constants and `approx` / `assertRect` / `rectsOverlap` / `rectFor` helpers). It exercises construction (`new_is_single_leaf`); split + pane rects (vertical / horizontal / nested); the **n-ary flatten** (a same-orientation vsplit flattens a two-pane row into three equal columns, a perpendicular split nests); remove / collapse / merge / re-equalize; `neighbor` resolution (basic, nested, flat row of three, unknown-pane → `nil`); and DFS determinism of `leaves()` / `paneRects()`. A final test pins the **discriminant contract** with `wolf-ffi`: `Orientation.init?(ffi:)` maps `0/1` to `horizontal`/`vertical` and `FocusDir.init?(ffi:)` maps `0/1/2/3` to `left`/`right`/`up`/`down`, so the Swift port stays in lockstep with the Rust `intent_kind` / `orientation` / `focus_dir` modules and the `tests/panes.rs` integration test.
