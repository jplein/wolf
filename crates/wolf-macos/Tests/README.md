# wolf-macos/Tests

The Swift tests for the macOS front-end. The Bazel target is `//crates/wolf-macos:WolfKeyMapTests`; it runs regardless of `WOLF_NVIM_BIN` (it needs no Neovim) and covers the pure seams — the key seam (`KeyMapTests.swift`), the picker seam (`PickerSeamTests.swift`), the pane-layout seam (`PaneTreeTests.swift`), the per-pane geometry seam (`PaneGeometryTests.swift`), the scroll-accumulator seam (`ScrollAccumulatorTests.swift`), the glyph-style seam (`GlyphStyleTests.swift`), and the state-restore seam (`StateSeamTests.swift`).

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

- `isPickerTrigger(charactersIgnoringModifiers:command:)` — the **Command+O** contract (lowercase and uppercase `"o"` → `true`, command not held → `false`, the wrong letter → `false`, and an empty string → `false`);
- `nextSelection(current:delta:nRows:)` — the list-navigation clamp across its cases (including the empty-list `nil`);
- `restoredIndex(resetToTop:prev:newLen:)` — selection restore across streaming result batches.

## `WolfKeyMapTests/PaneTreeTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.PaneTree` seam (`@testable import WolfKeyMap`) — a 1:1 port of wolf-gnome's `layout.rs` test suite (same `P1…P4` ids, `W = 100` / `H = 40` bounds, half/third/quarter constants and `approx` / `assertRect` / `rectsOverlap` / `rectFor` helpers). It exercises construction (`new_is_single_leaf`); split + pane rects (vertical / horizontal / nested); the **n-ary flatten** (a same-orientation vsplit flattens a two-pane row into three equal columns, a perpendicular split nests); remove / collapse / merge / re-equalize; `neighbor` resolution (basic, nested, flat row of three, unknown-pane → `nil`); and DFS determinism of `leaves()` / `paneRects()`. A final test pins the **discriminant contract** with `wolf-ffi`: `Orientation.init?(ffi:)` maps `0/1` to `horizontal`/`vertical` and `FocusDir.init?(ffi:)` maps `0/1/2/3` to `left`/`right`/`up`/`down`, so the Swift port stays in lockstep with the Rust `intent_kind` / `orientation` / `focus_dir` modules and the `tests/panes.rs` integration test.

## `WolfKeyMapTests/PaneGeometryTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.PaneGeometry` seam (`@testable import WolfKeyMap`, reusing `PaneTreeTests`' `eps` / `approx` style with a non-square `8 × 16` cell so column and row math are distinguishable). Cases: with `maxCols == nil` `gridRect == paneRect` and both paint offsets are `0`; `cols` / `rows` derive from the cell size (`800 × 480` at `8 × 16` → `100 × 30`, a sub-cell pane clamps to `1 × 1`); `cellAt` maps an interior point (`3.5 × 2.5` cells → `col 3, row 2`); `cellAt` clamps a left/top-margin point to `(0, 0)`; clamps a far-corner point to `(cols-1, rows-1)` (the off-by-one edge guard); and subtracts a non-zero pane origin (an origin-relative point maps correctly, the pane origin maps to `(0, 0)`). It also covers the **max-column cap + centering**: a pane WIDER than the cap caps `cols` to the cap (e.g. `100`) and centers the `gridRect` (equal side margins, `paintOffsetX` = the centering inset); a pane NARROWER than the cap is unaffected (`gridRect == paneRect`, offset `0`); an EXACT-boundary pane produces no margin; and `cellAt` on a centered grid clamps points in the left/right side margins to the nearest edge column while mapping an interior point through the paint offset.

## `WolfKeyMapTests/ScrollAccumulatorTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.ScrollAccumulator` seam (`@testable import WolfKeyMap`), the signed pixel→line-step accumulator with a carried remainder. Cases: a whole `±cellHeight` of `deltaY` yields exactly `±1` step; two sub-cell (`0.6h`) nudges yield `0` then `1` (crossing at `1.2h`); the remainder carries across calls (`1.5h` → `1` step + a `0.5h` remainder, then a further `0.5h` completes another step); a sign change reverses (`+1.5h` → `+1` step + `+0.5h` remainder, then `-2h` → remainder `-1.5h` → `-1` step); and a zero (non-positive) `cellHeight` short-circuits to `0` (no divide-by-zero / infinite loop).

## `WolfKeyMapTests/StateSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `StateSeam` (`WolfKeyMap`) — the state-restore decision seams, table-driven:

- `testToolbarModeDiscriminantTable` — the `ToolbarMode` ↔ discriminant contract in BOTH directions (`0/1/2` ↔ `iconAndText`/`iconOnly`/`textOnly`; unknown discriminants `3` and `99` → `nil`), keeping the Swift port in lockstep with wolf-ffi's `mod toolbar_mode` and the `//crates/wolf-ffi:state` discriminant guard.
- `testRestoredWindowFrameTable` — `restoredWindowFrame(x:y:width:height:screenVisibleFrames:)` accepts a frame intersecting a screen visible frame and rejects (`nil`) the non-positive width/height "unset" sentinel and a fully-offscreen frame (e.g. a disconnected display).
- `testLaunchCwdTable` — `launchCwd(restored:fallback:isDirectory:)` picks the restored cwd only when it is non-empty and the injected `isDirectory` check passes (tests pass a set lookup, keeping the seam pure), falling back on an empty or no-longer-a-directory path.

The AppKit save/restore wiring (`StatePersistence.swift`, the `AppDelegate` restore path) stays compile-verified + manual-smoke, per the plan; the bridge surface it drives is covered by the real-nvim `//crates/wolf-ffi:state` test.
