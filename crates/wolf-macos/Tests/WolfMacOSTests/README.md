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
- A pixel is `colorAt`'s components re-tagged in the bitmap's own sRGB `colorSpace`, then converted to sRGB. `colorAt` tags them by the bitmap's `colorSpaceName`, which is calibrated (Generic) RGB for this capture, and converting that shifts saturated and dark colours by up to about 0.2.

#### `LiveCore.swift`

Purpose: Starts a real core over the staged Neovim for the view tests that need one. It asserts nothing, though it fails the calling test when the core cannot start.

**Public API**

- `LiveCore.start()` — a core at 80 × 24 in a fresh working directory, attached, with `metrics` measured from the loaded config; `nil` with the test failed otherwise.
- `openTerminal(_:)` — a session in Neovim's current window, as the bridge's `open_terminal` runs a command.
- `waitFor(timeout:_:)` — spins the main run loop, so wakes reach the main queue while the test waits.
- `shutdown()` — for the end of every test.

**Decisions**

- The environment is prepared once per process, before any core or config read: `WOLF_NVIM_BIN` from the staged tree, fresh `WOLF_STATE_DIR` and `WOLF_CONFIG_DIR`, so no test touches the user's state or preferences, and `SHELL=/bin/sh`, so the drawer and a typed `:terminal` run a known shell. Once, because `setenv` races a concurrent `getenv`.
- A staged tree that does not resolve fails the test, naming the variable, rather than skipping: this target always stages Neovim.

#### `TerminalViewFixture.swift`

Purpose: A `PaneContainerView` over a [`LiveCore`](#livecoreswift), hosted offscreen at a fixed `windowSize` with its first pane bound, for the drawer and terminal-pane flow tests. It asserts nothing.

**Public API**

- `wake(until:)` — runs render wakes until the condition holds or `timeout` passes; `wake(_:until:)` wakes another container.
- `view(showing:)`, `window(showing:)`, `text(of:)` — a session's terminal view, placeholder window and frame text. `terminalViews(in:)` and `view(showing:in:)` read another container.
- `isInDrawerStrip(_:)` — whether a view lies in the container's lower half. The drawer's rect is private, and at `windowSize` its strip, a third of the height, lies wholly there.
- `runLine`, `terminalChord`, `neovimChord`, `typeEx` — input to a session or to Neovim.
- `markEditor()` — fills Neovim's current buffer with rows whose `line_hl_group` paints `mark` across the window's grid. `markedPixels(in:of:_:)` counts those pixels in a rect, and `markThreshold` is four cells' area.
- `hostContainer(tabpage:)` — a second container over another tabpage, in its own offscreen window. Wake it only while its tabpage is current, as the app wakes only its current view.

**Decisions** — the mark is a highlight group of its own, not `Normal`, because core pushes Neovim's default colours into every session, so a `Normal` mark would also match a terminal view.

#### `OffscreenRendererTests.swift`

**Covers** — [`OffscreenRenderer`](#offscreenrendererswift) reading a plain view's sRGB crimson, pure blue and dark grey stripes back within 0.01 at every pixel sampled inside each stripe. No core and no Neovim.

**Gotchas** — compare in sRGB, through `colorDistance` or after `usingColorSpace(.sRGB)`. A raw component read of `colorAt`'s calibrated-tagged sample shows the stored bytes, so it would hide the misread this test exists to catch.

#### `TerminalDrawerTests.swift`

**Covers** — `toggle_terminal()` showing a terminal view in the drawer strip on the drawer window's session, painting a truecolour background the shell prints, hiding with the session kept, and showing the same session again on the next toggle.

#### `TerminalNeighbourPaneTests.swift`

**Covers** — the Neovim panes staying painted while a terminal view is open and after it hides or closes, in the drawer, in a split beside an editor pane, and in a split in a new tab, with the first tab checked again after switching back to it.

**Gotchas**

- Each test marks the editor with `markEditor()` and fails as a premise if the container never paints the mark. It then samples only where the editor alone paints: beside the terminal view while it is open, and inside the view's last frame once it has closed.
- A close leg waits for Neovim's side of the close, not for the view to go, so a view `syncTerminalViews` wrongly keeps still reaches the pixel check.
- Every check polls until `TerminalViewFixture.timeout`, so each red leg costs that long.

#### `TerminalPaneFlowTests.swift`

**Covers**

- `<C-w>h` in a terminal pane focusing the editor beside it, `<C-w>l` there focusing the pane again, and `<C-w>v` in the pane splitting it onto a new, focused session.
- `capturedTerminals()` naming the tree's terminal leaf, not the drawer's session, and the drawer open. After a Neovim kill and restart, `requestTerminalRestore` putting that session back as a terminal leaf beside the editor and reopening the drawer on its own session, both with their output.

**Gotchas** — the test calls `capturedTerminals()` and `requestTerminalRestore` itself, so `Project.respawnNeovim`'s loop around them and `leafContents()`'s terminal arm reach no test here; they are manual-smoke, as [`LivenessSeamTests.swift`](../WolfKeyMapTests/README.md#livenessseamtestsswift) and [`SessionSeamTests.swift`](../WolfKeyMapTests/README.md#sessionseamtestsswift) record.

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
