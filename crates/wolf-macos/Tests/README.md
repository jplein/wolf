# wolf-macos/Tests

The Swift tests for the macOS front-end. The Bazel target is `//crates/wolf-macos:WolfKeyMapTests`; it runs regardless of `WOLF_NVIM_BIN` (it needs no Neovim) and covers the pure seams — the key seam (`KeyMapTests.swift`), the picker seam (`PickerSeamTests.swift`), the command-palette seam (`PaletteSeamTests.swift`), the pane-layout seam (`PaneTreeTests.swift`), the per-pane geometry seam (`PaneGeometryTests.swift`), the scroll-accumulator seam (`ScrollAccumulatorTests.swift`), the glyph-style seam (`GlyphStyleTests.swift`), the state-restore seam (`StateSeamTests.swift`), the command-decode seam (`CommandSeamTests.swift`), the Markdown-preview placement seam (`PreviewSeamTests.swift`), the font-size seam (`FontSizeSeamTests.swift`), the per-pane statusbar layout seam (`StatusbarLayoutTests.swift`), the new-window trigger seam (`ShortcutSeamTests.swift`), the tab-title seam (`TabTitleSeamTests.swift`), the editor-inset seam (`EditorInsetTests.swift`), and the group-by-cwd restore seam (`RestoreSeamTests.swift`).

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

The AppKit save/restore wiring (`StatePersistence.swift`, the `WolfWindowController` restore path) stays compile-verified + manual-smoke, per the plan; the bridge surface it drives is covered by the headless `//crates/wolf-ffi:state` test.

## `WolfKeyMapTests/StatusbarLayoutTests.swift`

Pure, headless `XCTest` coverage of the AppKit-/CoreText-free `WolfKeyMap.StatusbarLayout` seam (`@testable import WolfKeyMap`) — the per-pane statusbar's vertical-layout math. It pins the line factor at `1.25` (`ceil(fontSize * 1.25)`): one EXACT assertion checks the representative inputs (primary 13, secondary 10, padding 3, gap 1) collapse to `primaryLineHeight == 17`, `secondaryLineHeight == 13`, `primaryLineTop == 3`, `secondaryLineTop == 21`, `height == 37`; a second cross-checks the full arithmetic against the pinned formula over several inputs; and the structural invariants (heights strictly positive, `primaryLineTop == verticalPadding`, `secondaryLineTop > primaryLineTop`, `primaryLineTop + primaryLineHeight + lineGap == secondaryLineTop`, `height >= secondaryLineTop + secondaryLineHeight`) plus the zero-padding and small-font (1pt → `ceil(1.25) == 2`) edges hold. The on-screen bg fill / NSString drawing / truncation in `PaneContainerView.paintStatusbar` stays compile-verified + manual-smoke.

## `WolfKeyMapTests/ShortcutSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `ShortcutSeam.isNewWindowTrigger(charactersIgnoringModifiers:command:)` seam (`@testable import WolfKeyMap`), the sibling of `PickerSeamTests` for the **Command+N** new-window chord. Table-driven: `Command+"n"` → `true`, `Command+"N"` (Command+Shift+N — `charactersIgnoringModifiers` carries the uppercase glyph, matched case-insensitively) → `true`, plain `"n"` with command not held → `false`, `Command+"o"` (the wrong letter, guarding against colliding with the picker chord) → `false`, and an empty string → `false`. (The seam is now **dormant / unwired** in the live app — New Window moved to the menu's ⇧⌘N — but the test is kept so the pure seam stays green.)

## `WolfKeyMapTests/CommandSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `CommandKind(ffi:)` decode seam (`@testable import WolfKeyMap`), the sibling of `PaneTreeTests`' discriminant-contract test. It pins the `command_kind` contract with `wolf-ffi`: `init?(ffi:)` maps `0..18` to the nineteen cases (`newWindow` … `closePane == 8`, `newBuffer == 9`, `resetFontSize == 10`, `increaseFontSize == 11`, `decreaseFontSize == 12`, `commandPalette == 13`, `copy == 14`, `paste == 15`, `bufferPicker == 16`, `newTab == 17`, `preview == 18`) and returns `nil` for out-of-range bytes (the boundary is now `19`, plus `255`) — `CommandKind(ffi: 18) == .preview`, `CommandKind(ffi: 19) == nil` — so the Swift port stays in lockstep with the Rust `mod command_kind` and the `//crates/wolf-ffi:commands` discriminant guard.

## `WolfKeyMapTests/PreviewSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `PreviewSeam` (`@testable import WolfKeyMap`), the Markdown-preview placement seam. It pins `placementMode(paneCount:)`: `1` → `"split_right"` (single pane → a new pane to the right), `2` → `"active"` (multi pane → replace the active pane's buffer), and the degenerate `0` → `"split_right"`. It may also exercise the provided-but-currently-unused `previewsNeedingReload(current:lastTicks:)` helper (OD9). The live web-view hosting (`PaneContainerView`'s `WKWebView` overlay) and `WolfWindowController.openPreview` stay compile-verified + manual-smoke; the `is_markdown_active` / `open_preview` / `render_preview` / `previews` bridge surface is covered by the real-nvim `//crates/wolf-ffi:preview` test.

## `WolfKeyMapTests/FontSizeSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `FontSizeSeam` step+clamp seam (`@testable import WolfKeyMap`) backing the View font-size commands. Table-driven (asserting with an accuracy epsilon, since the API is `Double -> Double` with `minPointSize` / `maxPointSize` / `step` as `Double`): `increased`/`decreased` step by ±1, in-range values pass through, and the clamps hold at the `6` / `72` bounds (`71 → 72`, `72 → 72`, `7 → 6`, `6 → 6`).

## `WolfKeyMapTests/TabTitleSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `TabTitleSeam.labels(activeBufferPaths:)` seam (`@testable import WolfKeyMap`), which computes the per-tab titles for the tabs in one window. Cases: all-distinct basenames pass through (`["/a/b/alpha.md", "/c/d/beta.txt", "/e/gamma.rs"]` → `["alpha.md", "beta.txt", "gamma.rs"]`, order + count preserved); a two-way `README.md` collision extends each to its distinct immediate parent (`"README.md — wolf"` / `"README.md — docs"`, EM DASH U+2014); a three-way collision whose immediate parents ALSO collide grows to TWO trailing segments (`"x.md — a/shared"` etc., all distinct); `""` and `"[No Name]"` → `"untitled"` (excluded from disambiguation) while a distinct path keeps its basename, and a bare distinct filename (`"file.md"`, no slash) keeps its basename; and 1:1 order preservation on a mixed colliding/non-colliding input.

## `WolfKeyMapTests/RestoreSeamTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.RestoreSeam.plan(windows:home:isDirectory:)` **group-by-cwd** restore planner (`@testable import WolfKeyMap`) — the pure half of the two-phase project restore (plan §4.2 + OD5). Grouping is driven by scalar `SavedWindow` / `SavedTab` inputs and an injected `isDirectory` set lookup (keeping the seam pure — no FileManager, no NSWindow, no nvim), asserted ORDER-INDEPENDENTLY via a `projectIndex(...)` helper and an order-independent `Set<PlannedTab>`. Cases: (a) two same-cwd tabs in ONE window collapse into ONE project; (b) same-dir tabs in DIFFERENT saved windows MERGE into one project; (c) two different-dir tabs in one window SPLIT into two projects; (d) **OD5** — N empty-state (`""`) tabs become N DISTINCT single-tab projects (each `launchCwd == home`, never merged), with (d′) confirming an empty-state tab and a since-deleted / non-directory tab each get their OWN project (never merging into a real-dir project) while the deleted-dir `PlannedTab` still retains its ORIGINAL saved cwd string; and (e) every tab retains its `(groupIndex, positionInGroup, cwd, sidebarVisible)` placement metadata (no tab dropped or duplicated). This is the (a)–(e) contract the plan pins for Phase A materialization.

The AppKit multi-window/project glue (`Project`, `WolfWindowController` as a VIEW, `WindowRegistry` mapping token→`Project`, the token-routed `*WakeBridge` thunks that fan out to the project's current view, the `AppDelegate` coordinator's `projects` + `controllers` collections + `makeNewWindow` / `makeNewTab` / the two-phase restore + snapshot walk + `NSWindowTabGroup` grouping, `PaneContainerView`'s refocus rebind, `MainWindow.tabbingIdentifier`) stays compile-verified + manual-smoke (no headless window server); the Rust bridge surface it drives (`wolf_ffi_start`'s 4-arg form, `wake_token`, the token-carrying wake ABI, and the six tabpage methods) is covered by the real-nvim `//crates/wolf-ffi:multi_window` + `//crates/wolf-ffi:tabpages` tests, and the whole-tree snapshot builder round-trip by the headless `//crates/wolf-ffi:state` test. (The former `WindowIndexSeamTests` / `WindowTitleSeamTests` are REMOVED — those seams no longer exist; the OS tab/window title now derives from the active buffer via `TabTitleSeam`.)

## `WolfKeyMapTests/EditorInsetTests.swift`

Pure, headless `XCTest` coverage of the AppKit-free `WolfKeyMap.EditorInset` seam (`@testable import WolfKeyMap`, reusing `PaneGeometryTests`' `eps` / `approx` + `assertRectEqual` style) — the PER-PANE text-grid-inset arithmetic (the padding is now a per-pane inset around each pane's text grid rather than a uniform outer margin around the whole editor). One EXACT case pins the representative numbers (pane `840 × 600`, statusbar `40`, padding `20` → `gridRect == Rect(20, 60, 800, 520)`, `textLeftInset == 20`); a separate assertion pins `EditorInset.padding == 20`; a second pane (`1000 × 700`, statusbar `37`, padding `20`) checks the STRUCTURAL invariants (`gridRect.x == padding`, `gridRect.y == statusbarHeight + padding`, `gridRect.w == paneWidth − 2*padding`, `gridRect.h == paneHeight − statusbarHeight − 2*padding`, `gridRect.w + 2*padding == paneWidth`, `gridRect.y + gridRect.h + padding == paneHeight`, `textLeftInset == padding == gridRect.x`); and three clamp edges pin the non-negative floor on the grid width/height only (the `x` / `y` origins are fixed, unclamped offsets) — a narrow pane (`30 × 600`, statusbar `40` → `gridRect.w == 0`), a short pane (`840 × 60`, statusbar `40` → `gridRect.h == 0`), and a statusbar taller than the pane (`840 × 50`, statusbar `80` → `gridRect.h == 0` while `gridRect.y == 100` stays unclamped). `PaneContainerView` / `MainWindow` consume the seam but are window-server-bound AppKit glue, so they stay compile + manual-smoke only.
