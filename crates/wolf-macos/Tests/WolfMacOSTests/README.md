# wolf-macos/Tests/WolfMacOSTests

XCTest coverage of what [`WolfMacOS`](../../Sources/WolfMacOS/README.md) views draw. A test builds a production view in a window that is never shown, renders it offscreen and samples its pixels:

```sh
bazel test //crates/wolf-macos:WolfMacOSTests
```

**Adding a file means editing `srcs`.** As for [`WolfKeyMapTests`](../WolfKeyMapTests/README.md), the target's `srcs` is a hand-maintained list, not a glob, so a file missing from it never compiles and never runs. Helpers such as `OffscreenRenderer.swift` are listed too.

## What renders under `bazel test`

The macOS worker has no GUI session and no screen, so XCUITest, which needs a UI-test runner app and a display session, is unavailable. AppKit still works offscreen: a window that is never ordered front lays out its views, and `cacheDisplay(in:to:)` renders their glyphs with no window server. Event delivery, key-window behaviour, menus and real focus are not reachable this way.

**Do not drop the target's `CFProcessPath` env.** rules_swift places the test binary at `WolfMacOSTests.xctest/Contents/MacOS/`, where AppKit treats it as a bundled app and aborts, with no message, when it cannot check in with LaunchServices, which Seatbelt denies. A flat process path needs no check-in.

The target stages Neovim, `//bazel/nvim:nvim_tree` and `//runtime/nvim:init.lua` wired as `WOLF_TEST_NVIM_DIR` and `WOLF_TEST_INIT_LUA`, and carries `tags = ["no-sandbox"]`, because a test that starts a core spawns a child Neovim, which the macOS build sandbox does not allow. [`LiveCore.swift`](#livecoreswift) fences the state and config directories before any core or config read.

Captures are taken in Aqua at the host's backing scale, and the accent resolves through the window's appearance, so a test compares against `controlAccentColor` resolved at run time, never a literal. Only the worker's path, at backing scale 1 with a blue accent, gates; a desktop Mac with Retina, dark mode or another accent is untested.

## Files

#### `OffscreenRenderer.swift`

Purpose: The reusable mechanism for view tests: host a view, render it, sample what it drew. It holds no assertions.

**Public API**

- `OffscreenRenderer(hosting:size:)`, its `window`, and `render() -> RenderedView`.
- `RenderedView.color(at:in:)`, `colors(in:of:)` and `ink(in:of:minAlpha:)`. Each takes a point or rect in the coordinates of any view under the root and reads un-premultiplied sRGB. `ink` is the mean colour and count of the pixels at least `minAlpha` opaque, which over a clear background is the colour of the text drawn there.
- `colorDistance(_:_:)`, the sRGB distance ignoring alpha; pass resolved colours.
- `NSTableView.setRowsEmphasized(_:)`.

**Decisions**

- `render()` runs `displayIfNeeded()` before every capture. Without it the capture shows stale layer contents and a change made since the last capture never reaches the bitmap, even with `needsDisplay` set.
- The window is pinned to Aqua, so captures do not follow the host's dark mode.
- The window is never key, so a table never emphasizes its selection by itself; `setRowsEmphasized(_:)` sets `isEmphasized` on the row views, as first responder would.
- `NSApplication.shared` is touched before the window is built, since it sets up AppKit's drawing.
- The pixel scale is read from the bitmap, never assumed, because a Retina host captures at 2.
- A pixel is `colorAt`'s components re-tagged in the bitmap's own sRGB `colorSpace`, then converted to sRGB. `colorAt` tags them by the bitmap's `colorSpaceName`, which is calibrated (Generic) RGB for this capture, and converting that shifts saturated and dark colours by up to about 0.12.

#### `LiveCore.swift`

Purpose: Starts a real core over the staged Neovim for the view tests that need one. It asserts nothing, though it fails the calling test when the core cannot start.

**Public API**

- `LiveCore.start()` — a core at 80 × 24 in a fresh working directory, attached, with `metrics` measured from the loaded config; `nil` with the test failed otherwise.
- `openTerminal(_:)` — a session in Neovim's current window, as the bridge's `open_terminal` runs a command.
- `waitFor(timeout:_:)` — spins the main run loop, so wakes reach the main queue while the test waits.
- `shutdown()` — for the end of every test.

**Decisions**

- The environment is prepared once per process, before any core or config read: `WOLF_NVIM_BIN` from the staged tree, and fresh `WOLF_STATE_DIR` and `WOLF_CONFIG_DIR`, so no test touches the user's state or preferences. Once, because `setenv` races a concurrent `getenv`.
- A staged tree that does not resolve fails the test, naming the variable, rather than skipping: this target always stages Neovim.

#### `TerminalPaneRenderTests.swift`

Purpose: Renders a `PaneContainerView` whose current window shows a real `/bin/sh` session, over [`LiveCore`](#livecoreswift), and checks what its `TerminalPaneView` drew.

**Covers**

- A crimson background and bold blue text the program prints, in the cells it printed them.
- The session sized from the pane's content area, matching the placeholder grid and off the starting 80 × 24.
- The cursor cell in the cursor colour, or the default foreground when none is set.
- A drag selection painting swapped colours.
- A pane whose shell exited losing its terminal view while the pane stays.

**Gotchas**

- Background checks sample cell interiors, inset 1 point so no pixel straddles a neighbour. Glyph checks count blue pixels within 0.4, since antialiasing blends a glyph's edges.
- The selected cells have an unset foreground, so swapped they fill with the default foreground, not blue.
- The cursor test sets a steady block, which fills the whole cell.

#### `FileTreeViewTests.swift`

Purpose: Checks that a sidebar row draws for its present state only, over the production `TreeCellView.make()`, `FileTreeView.configureOutline(_:)` and `FileTreeView.restyleRows(in:isCurrent:)` and four test files. No Neovim: the core reaches these cells only as the `isCurrent` bit.

**Covers**

- A click emphasizes the rows, selects, captures, then de-emphasizes; the render wake marks the file current, selects it and restyles. After three files in turn, the two rows current at earlier clicks draw ink that matches a never-touched row, far from the accent, and each such cell is not current, has a `.normal` background style, and draws `labelColor`.
- The positive control: at a click the still-current row draws in the accent and the untouched row does not, which proves the sampler sees the accent.

**Gotchas**

- Pixel checks sample unselected rows only: a selected row's fill covers its text.
- The property checked is the label's drawn `attributedStringValue` foreground, not `textColor`, which reads `labelColor` even while the row draws in the accent.
