# wolf-macos/Tests/WolfMacOSTests

XCTest coverage of what [`WolfMacOS`](../../Sources/WolfMacOS/README.md) views draw. A test builds a production view in a window that is never shown, renders it offscreen and samples its pixels:

```sh
bazel test //crates/wolf-macos:WolfMacOSTests
```

**Adding a file means editing `srcs`.** As for [`WolfKeyMapTests`](../WolfKeyMapTests/README.md), the target's `srcs` is a hand-maintained list, not a glob, so a file missing from it never compiles and never runs. Helpers such as `OffscreenRenderer.swift` are listed too.

## What renders under `bazel test`

The macOS worker has no GUI session and no screen, so XCUITest, which needs a UI-test runner app and a display session, is unavailable. AppKit still works offscreen: a window that is never ordered front lays out its views, and `cacheDisplay(in:to:)` renders their glyphs with no window server. Event delivery, key-window behaviour, menus and real focus are not reachable this way.

**Do not drop the target's `CFProcessPath` env.** rules_swift places the test binary at `WolfMacOSTests.xctest/Contents/MacOS/`, where AppKit treats it as a bundled app and aborts, with no message, when it cannot check in with LaunchServices, which Seatbelt denies. A flat process path needs no check-in.

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

#### `FileTreeViewTests.swift`

Purpose: Checks that a sidebar row draws for its present state only, over the production `TreeCellView.make()`, `FileTreeView.configureOutline(_:)` and `FileTreeView.restyleRows(in:isCurrent:)` and four test files. No Neovim: the core reaches these cells only as the `isCurrent` bit.

**Covers**

- A click emphasizes the rows, selects, captures, then de-emphasizes; the render wake marks the file current, selects it and restyles. After three files in turn, the two rows current at earlier clicks draw ink that matches a never-touched row, far from the accent, and each such cell is not current, has a `.normal` background style, and draws `labelColor`.
- The positive control: at a click the still-current row draws in the accent and the untouched row does not, which proves the sampler sees the accent.

**Gotchas**

- Pixel checks sample unselected rows only: a selected row's fill covers its text.
- The property checked is the label's drawn `attributedStringValue` foreground, not `textColor`, which reads `labelColor` even while the row draws in the accent.
