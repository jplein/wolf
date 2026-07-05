# wolf-macos/Tests

The Swift tests for the macOS front-end. The Bazel target is `//crates/wolf-macos:WolfKeyMapTests`; it runs regardless of `WOLF_NVIM_BIN` (it needs no Neovim) and covers the pure seams — the key seam (`KeyMapTests.swift`), the picker seam (`PickerSeamTests.swift`), the command-palette seam (`PaletteSeamTests.swift`), the pane-layout seam (`PaneTreeTests.swift`), the per-pane geometry seam (`PaneGeometryTests.swift`), the scroll-accumulator seam (`ScrollAccumulatorTests.swift`), the glyph-style seam (`GlyphStyleTests.swift`), the state-restore seam (`StateSeamTests.swift`), the command-decode seam (`CommandSeamTests.swift`), the font-size seam (`FontSizeSeamTests.swift`), the per-pane statusbar layout seam (`StatusbarLayoutTests.swift`), and the multi-window seams (`ShortcutSeamTests.swift`, `WindowIndexSeamTests.swift`).

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

## `WolfKeyMapTests/PaletteSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `PaletteSeam` namespace (`@testable import WolfKeyMap`), the command-palette analog of `PickerSeamTests`. Two contracts, table-driven:

- `isPaletteTrigger(charactersIgnoringModifiers:command:)` — the **Command+P** contract (lowercase and uppercase `"p"` with command held → `true`, command not held → `false`, the wrong letter `"o"` (the picker chord) → `false`, and an empty string → `false`). The seam is **dormant / unwired** in the live app (⌘P is a menu key-equivalent that preempts `keyDown`), but the test pins the predicate regardless.
- `PaletteSeam.PaletteRowKind(ffi:)` — the FFI row-kind discriminant decode (`0` → `.staticCommand`, `1` → `.switchWindow`, `2` → `.openRecent`; out-of-range `3` and `255` → `nil`), keeping the Swift port in lockstep with wolf-ffi's `mod palette_row_kind`.

## `WolfKeyMapTests/PaneTreeTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.PaneTree` seam (`@testable import WolfKeyMap`) — a 1:1 port of wolf-gnome's `layout.rs` test suite (same `P1…P4` ids, `W = 100` / `H = 40` bounds, half/third/quarter constants and `approx` / `assertRect` / `rectsOverlap` / `rectFor` helpers). It exercises construction (`new_is_single_leaf`); split + pane rects (vertical / horizontal / nested); the **n-ary flatten** (a same-orientation vsplit flattens a two-pane row into three equal columns, a perpendicular split nests); remove / collapse / merge / re-equalize; `neighbor` resolution (basic, nested, flat row of three, unknown-pane → `nil`); and DFS determinism of `leaves()` / `paneRects()`. A final test pins the **discriminant contract** with `wolf-ffi`: `Orientation.init?(ffi:)` maps `0/1` to `horizontal`/`vertical` and `FocusDir.init?(ffi:)` maps `0/1/2/3` to `left`/`right`/`up`/`down`, so the Swift port stays in lockstep with the Rust `intent_kind` / `orientation` / `focus_dir` modules and the `tests/panes.rs` integration test.

## `WolfKeyMapTests/PaneGeometryTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.PaneGeometry` seam (`@testable import WolfKeyMap`, reusing `PaneTreeTests`' `eps` / `approx` style with a non-square `8 × 16` cell so column and row math are distinguishable). Cases: with `maxCols == nil` `gridRect == paneRect` and both paint offsets are `0`; `cols` / `rows` derive from the cell size (`800 × 480` at `8 × 16` → `100 × 30`, a sub-cell pane clamps to `1 × 1`); `cellAt` maps an interior point (`3.5 × 2.5` cells → `col 3, row 2`); `cellAt` clamps a left/top-margin point to `(0, 0)`; clamps a far-corner point to `(cols-1, rows-1)` (the off-by-one edge guard); and subtracts a non-zero pane origin (an origin-relative point maps correctly, the pane origin maps to `(0, 0)`). It also covers the **max-column cap + centering**: a pane WIDER than the cap caps `cols` to the cap (e.g. `100`) and centers the `gridRect` (equal side margins, `paintOffsetX` = the centering inset); a pane NARROWER than the cap is unaffected (`gridRect == paneRect`, offset `0`); an EXACT-boundary pane produces no margin; and `cellAt` on a centered grid clamps points in the left/right side margins to the nearest edge column while mapping an interior point through the paint offset.

## `WolfKeyMapTests/ScrollAccumulatorTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.ScrollAccumulator` seam (`@testable import WolfKeyMap`), the signed pixel→line-step accumulator with a carried remainder. Cases: a whole `±cellHeight` of `deltaY` yields exactly `±1` step; two sub-cell (`0.6h`) nudges yield `0` then `1` (crossing at `1.2h`); the remainder carries across calls (`1.5h` → `1` step + a `0.5h` remainder, then a further `0.5h` completes another step); a sign change reverses (`+1.5h` → `+1` step + `+0.5h` remainder, then `-2h` → remainder `-1.5h` → `-1` step); and a zero (non-positive) `cellHeight` short-circuits to `0` (no divide-by-zero / infinite loop).

## `WolfKeyMapTests/StateSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `StateSeam` (`WolfKeyMap`) — the state-restore decision seams, table-driven:

- `testRestoredWindowFrameTable` — `restoredWindowFrame(x:y:width:height:screenVisibleFrames:)` accepts a frame intersecting a screen visible frame and rejects (`nil`) the non-positive width/height "unset" sentinel and a fully-offscreen frame (e.g. a disconnected display).
- `testLaunchDecisionTable` — `launchDecision(restored:home:isDirectory:)` returns `.folder(restored)` only when the restored cwd is non-empty and the injected `isDirectory` check passes (tests pass a set lookup, keeping the seam pure), and `.emptyState(home)` on an empty, deleted, or plain-file path — asserting it NEVER returns the process working directory.

The AppKit save/restore wiring (`StatePersistence.swift`, the `WolfWindowController` restore path) stays compile-verified + manual-smoke, per the plan; the bridge surface it drives is covered by the real-nvim `//crates/wolf-ffi:state` test.

## `WolfKeyMapTests/StatusbarLayoutTests.swift`

Pure, headless `XCTest` coverage of the AppKit-/CoreText-free `WolfKeyMap.StatusbarLayout` seam (`@testable import WolfKeyMap`) — the per-pane statusbar's vertical-layout math. It pins the line factor at `1.25` (`ceil(fontSize * 1.25)`): one EXACT assertion checks the representative inputs (primary 13, secondary 10, padding 3, gap 1) collapse to `primaryLineHeight == 17`, `secondaryLineHeight == 13`, `primaryLineTop == 3`, `secondaryLineTop == 21`, `height == 37`; a second cross-checks the full arithmetic against the pinned formula over several inputs; and the structural invariants (heights strictly positive, `primaryLineTop == verticalPadding`, `secondaryLineTop > primaryLineTop`, `primaryLineTop + primaryLineHeight + lineGap == secondaryLineTop`, `height >= secondaryLineTop + secondaryLineHeight`) plus the zero-padding and small-font (1pt → `ceil(1.25) == 2`) edges hold. The on-screen bg fill / NSString drawing / truncation in `PaneContainerView.paintStatusbar` stays compile-verified + manual-smoke.

## `WolfKeyMapTests/ShortcutSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `ShortcutSeam.isNewWindowTrigger(charactersIgnoringModifiers:command:)` seam (`@testable import WolfKeyMap`), the sibling of `PickerSeamTests` for the **Command+N** new-window chord. Table-driven: `Command+"n"` → `true`, `Command+"N"` (Command+Shift+N — `charactersIgnoringModifiers` carries the uppercase glyph, matched case-insensitively) → `true`, plain `"n"` with command not held → `false`, `Command+"o"` (the wrong letter, guarding against colliding with the picker chord) → `false`, and an empty string → `false`. (The seam is now **dormant / unwired** in the live app — New Window moved to the menu's ⇧⌘N — but the test is kept so the pure seam stays green.)

## `WolfKeyMapTests/CommandSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `CommandKind(ffi:)` decode seam (`@testable import WolfKeyMap`), the sibling of `PaneTreeTests`' discriminant-contract test. It pins the `command_kind` contract with `wolf-ffi`: `init?(ffi:)` maps `0..13` to the fourteen cases (`newWindow` … `closePane == 8`, `newBuffer == 9`, `resetFontSize == 10`, `increaseFontSize == 11`, `decreaseFontSize == 12`, `commandPalette == 13`) and returns `nil` for out-of-range bytes (the boundary is now `14`, plus `255`), so the Swift port stays in lockstep with the Rust `mod command_kind` and the `//crates/wolf-ffi:commands` discriminant guard.

## `WolfKeyMapTests/FontSizeSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `FontSizeSeam` step+clamp seam (`@testable import WolfKeyMap`) backing the View font-size commands. Table-driven (asserting with an accuracy epsilon, since the API is `Double -> Double` with `minPointSize` / `maxPointSize` / `step` as `Double`): `increased`/`decreased` step by ±1, in-range values pass through, and the clamps hold at the `6` / `72` bounds (`71 → 72`, `72 → 72`, `7 → 6`, `6 → 6`).

## `WolfKeyMapTests/WindowIndexSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WindowIndexSeam.nextFreeIndex(occupied:)` seam (`@testable import WolfKeyMap`), which picks a new window's `state.json` slot: the smallest non-negative integer NOT in `occupied` (order and duplicates irrelevant). Table-driven: `[]` → `0`, a dense `[0,1,2]` → `3` (append), `[1]` → `0`, hole reuse `[0,2]` → `1` and `[0,1,3]` → `2`, and an unordered `[2,0,1]` → `3`.

The AppKit multi-window glue (`WolfWindowController`, `WindowRegistry`, the token-routed `*WakeBridge` thunks, the `AppDelegate` coordinator collection + `makeNewWindow`) stays compile-verified + manual-smoke (no headless window server); the Rust bridge surface it drives (`wolf_ffi_start`'s trailing `window_index`, `wake_token`, the token-carrying wake ABI, the shared-store per-index writes, `state_set_window_count`) is covered by the real-nvim `//crates/wolf-ffi:multi_window` test.
