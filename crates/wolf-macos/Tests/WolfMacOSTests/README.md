# wolf-macos/Tests/WolfMacOSTests

XCTest coverage of what [`WolfMacOS`](../../Sources/WolfMacOS/README.md) views draw. A test builds a production view in a window that is never shown, renders it offscreen and samples its pixels.

A few read something else. [`PreviewKeyEquivalentTests.swift`](#previewkeyequivalenttestsswift) instead calls AppKit's responder and menu dispatch directly, [`SidebarToolbarTests.swift`](#sidebartoolbartestsswift) sends a toolbar control its action and reads its selection, [`TerminalDrawerResizeTests.swift`](#terminaldrawerresizetestsswift) calls the pane container's mouse handlers with synthesized events, [`SymbolsFallbackTests.swift`](#symbolsfallbacktestsswift) draws single cells through `GlyphPainter` into a bitmap, with no view, [`NewWindowPlacementTests.swift`](#newwindowplacementtestsswift) reads a window's frame and its cascade calls, with no core, and [`InstallCommandMenuTests.swift`](#installcommandmenutestsswift) reads the application menu's items:

```sh
bazel test //crates/wolf-macos:WolfMacOSTests
```

[`PreviewMarginTests.swift`](#previewmargintestsswift) samples no pixels either: it reads a preview web view's frame, its recorded root style and AppKit's hit test. [`CloseTabDispatchTests.swift`](#closetabdispatchtestsswift) reads the close calls a dispatch makes on a window, with no core.

**Adding a file means editing `srcs`.** As for [`WolfKeyMapTests`](../WolfKeyMapTests/README.md), the target's `srcs` is a hand-maintained list, not a glob, so a file missing from it never compiles and never runs. Helpers such as `OffscreenRenderer.swift` are listed too.

## What renders under `bazel test`

The macOS worker has no GUI session and no screen, so XCUITest, which needs a UI-test runner app and a display session, is unavailable. AppKit still works offscreen: a window that is never ordered front lays out its views, and `cacheDisplay(in:to:)` renders their glyphs with no window server. A test can set the window's first responder and call a responder's or the main menu's `performKeyEquivalent(with:)` itself, but AppKit routes no event there, real focus is not reachable, and key-window behaviour only as `isKeyWindow`, which [`OffscreenWindow.reportsKey`](#offscreenrendererswift) sets.

**No page loads in a `WKWebView`.** One can be hosted and its responder methods called, but inside the worker's Seatbelt profile WebKit cannot issue the sandbox extensions its WebContent process needs, so no page loads and no script runs. A test must not wait on a page; [`PreviewViewFixture.swift`](#previewviewfixtureswift) never does.

**A live test's own fixture files go under `TEST_TMPDIR`**, as `LiveCore.swift` and `LighthouseColours.swift` place theirs, falling back to `NSTemporaryDirectory()`. `FileManager.default.temporaryDirectory` resolves to the per-user `/var/folders/…/T` on the worker, and its Seatbelt profile denies the test a write there.

**Do not drop the target's `CFProcessPath` env.** rules_swift places the test binary at `WolfMacOSTests.xctest/Contents/MacOS/`, where AppKit treats it as a bundled app and aborts, with no message, when it cannot check in with LaunchServices, which Seatbelt denies. A flat process path needs no check-in.

The target stages Neovim, `//bazel/nvim:nvim_tree` and `//runtime/nvim:init.lua` wired as `WOLF_TEST_NVIM_DIR` and `WOLF_TEST_INIT_LUA`, the vendored Lighthouse, `//bazel/lighthouse:lighthouse_tree` as `WOLF_TEST_LIGHTHOUSE_DIR`, and the three font trees the app bundles: `//bazel/ia-writer-mono:ia_writer_mono_tree`, `//bazel/jetbrains-mono:jetbrains_mono_tree` and `//bazel/nerd-fonts-symbols:nerd_fonts_symbols_tree` as `WOLF_TEST_IA_WRITER_MONO_DIR`, `WOLF_TEST_JETBRAINS_MONO_DIR` and `WOLF_TEST_NERD_FONTS_SYMBOLS_DIR`. It carries `tags = ["no-sandbox"]`, because a test that starts a core spawns a child Neovim, which the macOS build sandbox does not allow. [`LiveCore.swift`](#livecoreswift) fences the state and config directories, and registers the fonts, before any core or config read.

The target also stages the `wolf` command, `//crates/wolf-core:wolf`, as `WOLF_TEST_WOLF_BIN`, which [`OpenRequestFlowTests.swift`](#openrequestflowtestsswift) runs.

Captures are taken in Aqua at the host's backing scale, and the accent resolves through the window's appearance, so a test compares against `controlAccentColor` resolved at run time, never a literal. Only the worker's path, at backing scale 1 with a blue accent, gates; a desktop Mac with Retina, dark mode or another accent is untested.

## Files

#### `OffscreenRenderer.swift`

Purpose: Hosts a view for view tests, renders it and samples what it drew, asserting nothing.

**Public API**

- `OffscreenRenderer(hosting:size:)`, its `window`, an `OffscreenWindow` whose `reportsKey` sets `isKeyWindow`, and `render() -> RenderedView`.
- `static capture(_:) -> RenderedView` — captures one view in any window, such as a production `MainWindow`.
- `RenderedView.color(at:in:)`, `colors(in:of:)` and `ink(in:of:minAlpha:)`. Each takes a point or rect in the coordinates of any view under the root and reads un-premultiplied sRGB. `ink` is the mean colour and count of the pixels at least `minAlpha` opaque: over a clear background, the text's colour.
- `colorDistance(_:_:)`, the sRGB distance ignoring alpha; pass resolved colours.
- `NSTableView.setRowsEmphasized(_:)`.

**Decisions**

- Every capture runs `displayIfNeeded()` first, or a change made since the last capture never reaches the bitmap, even with `needsDisplay` set.
- The window is pinned to Aqua, so captures do not follow the host's dark mode.
- AppKit never makes the window key, so a table never emphasizes its selection by itself; `setRowsEmphasized(_:)` sets the row views' `isEmphasized`, as first responder would. `reportsKey` sends no key notification.
- `NSApplication.shared` is touched before the window is built, since it sets up AppKit's drawing.
- The pixel scale is read from the bitmap, never assumed: a Retina host captures at 2.
- A pixel is `colorAt`'s components re-tagged in the bitmap's own sRGB `colorSpace`, then converted to sRGB. `colorAt` tags them by the bitmap's `colorSpaceName`, calibrated (Generic) RGB here, and converting that shifts saturated and dark colours by up to about 0.2.

#### `LiveCore.swift`

Purpose: Starts a real core over the staged Neovim for the view tests that need one. It asserts nothing.

**Public API**

- `LiveCore.start(initLua:)` — a core at 80 × 24 in a fresh working directory, attached, with `metrics` measured from the loaded config; `nil` with the test failed otherwise. `initLua` replaces the staged `init.lua`, such as a copy beside a vendored tree it self-locates.
- `stagedInitLua()` — the staged `init.lua`, to copy; `nil` with the test failed when it does not resolve.
- `stageFonts()` — registers the fonts for a coreless test.
- `runfile(_:)` — the existing path a runfiles-relative variable names; `nil` when unset or missing.
- `openTerminal(_:)` — a session in Neovim's current window, as the bridge's `open_terminal` runs a command.
- `waitFor(timeout:_:)` — spins the main run loop, so wakes reach the main queue while the test waits. `spin(timeout:until:)` and `scratchDirectory()` serve a test whose cores `TabCore.start` makes.
- `shutdown()` — for the end of every test.

**Decisions**

- The environment is prepared once per process, before any core or config read: `WOLF_NVIM_BIN` from the staged tree, `WOLF_INIT_LUA` from the staged `init.lua`, fresh `WOLF_STATE_DIR` and `WOLF_CONFIG_DIR`, so no test touches the user's state or preferences, `SHELL=/bin/sh`, so the drawer and a typed `:terminal` run a known shell, and the staged fonts registered. Once, because `setenv` races a concurrent `getenv`.
- A staged tree that does not resolve fails the test, naming the variable, rather than skipping: this target always stages Neovim and the fonts.

#### `TerminalViewFixture.swift`

Purpose: A `PaneContainerView` over a [`LiveCore`](#livecoreswift), hosted offscreen at a fixed `windowSize` with its first pane bound, for the drawer and terminal-pane flow tests and [`PreviewViewFixture`](#previewviewfixtureswift). It asserts nothing.

**Public API**

- `TerminalViewFixture(initLua:)` — passes `initLua` to `LiveCore.start(initLua:)`.
- `wake(until:)` — runs render wakes until the condition holds or `timeout` passes; `wake(_:until:)` wakes another container.
- `view(showing:)`, `window(showing:)`, `text(of:)` — a session's terminal view, placeholder window and frame text. `terminalViews(in:)` and `view(showing:in:)` read another container.
- `isInDrawerStrip(_:)` — whether a view lies in the container's lower half. The drawer's rect is private, and at `windowSize` its strip, a third of the height, lies wholly there.
- `runLine`, `terminalChord`, `neovimChord`, `typeEx` — input to a session or to Neovim.
- `markEditor()` — fills Neovim's current buffer with rows whose `line_hl_group` paints `mark` across the window's grid. `markedPixels(in:of:_:)` counts those pixels in a rect, and `markThreshold` is four cells' area.

**Decisions** — the mark is a highlight group of its own, not `Normal`, because core pushes Neovim's default colours into every session, so a `Normal` mark would also match a terminal view.

#### `PreviewViewFixture.swift`

Purpose: A [`TerminalViewFixture`](#terminalviewfixtureswift) whose container shows a short Markdown file's preview as the active pane, in a real `WKWebView`: beside its source in a two-pane split, or alone with `split: false`, replacing the lone pane. It fails the calling test on a `premise:` and asserts nothing else.

**Public API** — `PreviewViewFixture(initLua:split:)`, `nil` once a premise has failed, with `initLua` passed to the base fixture; `web`, `previewWin`, `probePath`; `container`, `handle` and `window` from the base fixture; `shutdown()`.

**Gotchas**

- The page never loads, for the reason [What renders under `bazel test`](#what-renders-under-bazel-test) gives, so a test may use the web view's responder chain but nothing the page would draw or answer.
- After `:edit` it waits for `current_file()` to be the probe's path, not for a Markdown buffer. The starting buffer is already Markdown, and Neovim answers the `open_preview()` request ahead of typed keys still queued, so a wait on Markdown alone previews the starting buffer, which the late `:edit` then replaces.

#### `LighthouseColours.swift`

Purpose: The vendored Lighthouse's expected colours, for the tests that render under it. It asserts nothing, though staging fails the calling test.

**Public API**

- `LighthouseColours(tree:)` — reads the pinned `colors/lighthouse.lua`. `colour(_:_:)` is one palette key's colour and `ansi(_:)` the 16 ANSI colours, packed `0x00RRGGBB`.
- `normal(_:)` — `Normal`'s foreground and background for a `LighthouseBackground`.
- `stageLighthouse()` — `init.lua` copied beside `lighthouse/colors/lighthouse.lua` in a fresh directory, since `init.lua` self-locates the colorscheme, and the colours; `nil` with the test failed when `WOLF_TEST_LIGHTHOUSE_DIR` or `init.lua` does not resolve.
- `srgbColour`, `hex`, `meanColour`, `describeColour` — conversions for assertions and the printed tables.

**Decisions**

- The reader parses the file as `crates/wolf-ffi/tests/common/mod.rs` does, so a pin bump needs no constant.
- `normal(_:)` throws, naming the line, unless the file sets `Normal` to the palette's `foreground` on its `background`, so a pin that rewires `Normal` fails as a broken premise rather than as a wrong colour.

#### `OffscreenRendererTests.swift`

**Covers** — [`OffscreenRenderer`](#offscreenrendererswift) reading a plain view's sRGB crimson, pure blue and dark grey stripes back within 0.01 at every pixel sampled inside each stripe. No core and no Neovim.

**Gotchas** — compare in sRGB, through `colorDistance` or after `usingColorSpace(.sRGB)`. A raw component read of `colorAt`'s calibrated-tagged sample shows the stored bytes, so it would hide the misread this test exists to catch.

#### `TabCoreTests.swift`

**Covers** — `TabCore` over the staged Neovim and `init.lua`, each core started through `TabCore.start` as the app starts a tab. Two tabs on one directory run two Neovim processes under distinct wake tokens, each resolving to itself in `WindowRegistry`. Shutting one down unregisters it and leaves the other editing. SIGKILLing one tab's Neovim restarts that tab alone: its view records the crash overlay, a reset and the restart notice, and its file reopens, while the other tab keeps its process, its unsaved edit and a view that recorded nothing.

**Gotchas** — no wake callback is installed, so each test drives `onWake()` itself while it spins the main run loop, which runs the deferred automatic restart. A recording `TabCoreView` stands in for `WolfWindowController`, which cannot be built offscreen, so the real view's reset and terminal restore reach no test here.

#### `TabIdTests.swift`

**Covers** — `TabCore.wolfTabId` over real cores started through `TabCore.start`: two tabs get distinct, lowercase version-4 IDs, and a session each starts prints its own tab's `WOLF_TAB_ID`. After a SIGKILL and the automatic restart, the ID is unchanged and a new session prints it.

**Gotchas** — the restart reopens the tab's files as queued input, which a blocking `open_terminal` would overtake, its session then replaced. So the restart test opens a file of its own, queued behind the restore, and starts its session only once that file shows. No wake callback is installed, so the restart test drives `onWake()` itself, as [`TabCoreTests.swift`](#tabcoretestsswift) does.

#### `TerminalClosePaneTests.swift`

**Covers** — Close Pane through `PaneContainerView.canClosePane` and `closeActivePane()`: a focused terminal split closing and leaving the tree with its session hung up; the focused drawer closing with its session kept, which the next toggle shows again; the gate refusing a lone terminal pane and the last editor pane beside the drawer; and the close acting on the view's active pane while Neovim's current window lags it.

**Gotchas**

- `validateMenuItem` and `WolfWindowController`'s forwards reach no test here: AppKit routes no event offscreen, and [`PreviewKeyEquivalentTests.swift`](#previewkeyequivalenttestsswift) retargets the menu's items.
- The lag test sends `focus_win` with no wake before the close, which recreates a focus move in flight.

#### `SplitCommandTests.swift`

**Covers** — Split Horizontal and Split Vertical through `PaneContainerView.splitActivePane(_:)` over a live core: from an editor pane, a terminal pane and a preview pane, the new leaf sits beside its origin under a split of the expected orientation and is of the origin's kind, a window on the same buffer, a terminal on a second live session, or a preview of the same source. From the focused drawer neither adds a pane or a session, and the split acts on the view's active pane while Neovim's current window lags it.

**Gotchas**

- The palette rows, `CommandDispatcher`'s split arms and `WolfWindowController.splitPane(_:)` reach no test here: AppKit routes no event offscreen. The [app-launch smoke](../../../../build.md#the-app-launch-smoke) drives `:WolfSplitVertical` through the last two.
- The front-end drops a `Split` from the drawer anyway, so the drawer test's witness is the live session count, after a `focus_win` queued behind both splits has landed.
- The lag test sends `focus_win` with no wake before the split, as [`TerminalClosePaneTests.swift`](#terminalclosepanetestsswift) does.

#### `TerminalCursorFocusTests.swift`

**Covers** — a terminal pane's cursor painting the frame's block while its pane is the key window's active pane, and a hollow block while `<C-w>h` focuses the editor beside it, while the window is not key, and in the drawer once `<C-w>k` leaves it; the block again once focus returns; and a focus change alone, with no wake, marking the view for display.

**Gotchas**

- Focus needs a key window, which each test sets through `OffscreenWindow.reportsKey`; `syncTerminalFocus` sees it on the next render wake, or when a test calls it directly.
- A hollow cursor is told from a block by its interior, which keeps the default background, and from no cursor by its top band in the cursor colour. Every checked cursor sits in row 0, so that 1-point band covers whole pixels.
- `render()` redraws the whole view whatever was invalidated, so a pixel check cannot show the invalidation. `testAFocusChangeAloneMarksTheViewForDisplay` checks it instead, through the backing layer's `needsDisplay()`, with no wake. It reads the layer's flag because a layer-backed view's own `needsDisplay` stays true in a window never ordered front.

#### `PreviewKeyEquivalentTests.swift`

**Covers** — with a preview's web view first responder, as a click leaves it: `noop:` sent to the web view reaching a sink hung at the end of its responder chain, and ⌘W and ⌘P firing Close and Command Palette… from the production `AppMenu`. Both are red when `PaneContainerView.doCommand(by:)` swallows the command ([why](../../../../notes/wolf-macos-decisions.md#key-equivalents-over-a-preview)).

**Gotchas**

- WebKit's web-process step is modelled, since no page loads: the page leaves the chord unhandled and its binding is `noop:`. If the sink receives `noop:`, the test resends the event to `NSApp.mainMenu`, as WebKit's resend through `NSApp.sendEvent` would.
- Every item with a key equivalent is retargeted to a recorder that enables everything, so `validateMenuItem` and `AppDelegate`'s actions reach no test. A `control:` leg first fires each chord with the container first responder, so a later miss is the chain's.
- Each delivery prints a `KEYROUTE` line: responder, chord, whether the window claimed it, whether the chain passed `noop:`, and the item fired. Whether the window claims it is printed, not asserted, because without a web process the value is WebKit's.

#### `PreviewMarginTests.swift`

**Covers** — over [`PreviewViewFixture.swift`](#previewviewfixtureswift): a lone capped preview's web view spans the container's width below the strip, and its padding gutters and side margin hit-test to it; `previewRootStyle(forWin:)` holds the capped `PreviewWidthSeam.rootStyle`, and after `toggle_full_width()` the frame still spans and the style caps nothing. In the default split, a point 2 pt inside the preview's pane from the divider hit-tests to the container, which drags the divider, and the outer gutter to the web view.

**Gotchas**

- A hit test stands in for the wheel: AppKit sends a scroll-wheel or mouse event to the view `hitTest(_:)` returns, so a point that reaches the web view is one whose wheel scrolls the page. No page loads, so nothing reads how far it scrolls or where WebKit lays out the content.
- The lone-pane tests take as a premise that the capped content and its padding are narrower than the container, or there is no margin to test.

#### `InstallCommandMenuTests.swift`

**Covers** — `AppMenu.install(target:)` over a bare `AppDelegate`: the application menu carries Install and Uninstall `wolf` Command after About, side by side between separators, each targeting the delegate with its action. No item is fired; what the actions do is [`CommandInstallSeam`](../WolfKeyMapTests/README.md#commandinstallseamtestsswift)'s and a person check's.

#### `OtherPaneFlowTests.swift`

**Covers** — Preview and Edit in Another Pane reusing the other pane, over a live core through `PaneContainerView.otherPaneTarget(prefer:)` and the bridge. With an editor beside a preview, previewing a second file binds into the existing preview pane and leaves the editor focused; with the preview repointed at a second file, Edit binds its source into the existing editor pane and focuses it. Both keep two panes, and the first press, from a lone pane, splits.

**Gotchas**

- Each test repeats `WolfWindowController`'s composition, target then bridge call then wake, over a [`TerminalViewFixture`](#terminalviewfixtureswift), because AppKit routes no event to an offscreen window; the controller itself reaches no test here. No page loads, and nothing waits on one.
- A wait for an opened file checks the file the window shows: the startup buffer is the markdown-named `Untitled.md`, so a Markdown buffer being active proves nothing. `otherPaneTarget` resolves from the container's active pane, which follows Neovim only on a wake, so each read first waits for `container.activeWin` to match.

#### `TerminalDrawerLastPaneTests.swift`

**Covers** — the tab's only editor pane beside the open drawer, over a live core: `:bd` keeping one pane, active and Neovim's current window, on the file shown before, which takes focus again after the drawer has had it; and `PaneContainerView.openPickedFile` with the drawer active opening a file in the editor pane, active, with the drawer kept, and at a given line.

#### `TerminalDrawerResizeTests.swift`

**Covers** — a drag on the drawer's top edge over a live core: the drawer view moving and growing before any wake, then its session's rows, the shell's `stty size` and the editor grid above following. The dragged share survives closing and reopening the drawer, and a second fixture, with its own core as a new tab has, opens its drawer at the default third.

**Gotchas**

- The tests call `PaneContainerView`'s `mouseDown`, `mouseDragged` and `mouseUp` with synthesized events, so AppKit's own hit-testing and the cursor rects reach no assertion.
- The press point comes from `DrawerLayout.defaultWeight` at the fixture's `windowSize`, since the container's share is private, so each test drags once, from the opening share.
- After a drag, compare `frame.minY` rather than call `isInDrawerStrip`: the dragged view may start above the midline.

#### `ZoomPaneTests.swift`

**Covers** — Zoom Pane through `PaneContainerView.toggleZoom()` over a live core and `/bin/sh`, from two editor panes and a terminal pane with the drawer open. A zoomed pane fills the area and its grid or session grows, while the others keep their windows, sessions and sizes, and unzoom restores every frame and weight after drags and a window resize while zoomed. A hidden preview keeps its web view, hidden, until unzoom.

A split, a close and the shell exiting each unzoom, and so does Toggle Terminal, focusing an open drawer with every frame and session size kept, opening and focusing a closed one, or closing a zoomed drawer. `<C-w>l` unzooms onto the neighbour with every frame and weight restored, `:wincmd w` unzooms, the view following Neovim, and `<C-w>h` from the leftmost pane changes nothing. A zoom survives a save and restore through `gatherTab`, `WindowStateCodec`, `FfiAppTab.restored()` and `rebuild_tab`.

**Gotchas**

- The palette, the `CommandDispatcher` arms and `WolfWindowController` reach no test, so `toggleZoom()` and `toggleTerminal()` are called directly; the [app-launch smoke](../../../../build.md#the-app-launch-smoke) covers the last two.
- A hidden pane's sizes are compared after a settle: a wrong resize lands a round trip later.
- A move that must change nothing and a restored zoom are sampled at every wake: Neovim reporting another pane a round trip later unzooms the view.
- The save and restore test writes `LiveCore`'s `WOLF_STATE_DIR`, as [`SessionRestoreTests.swift`](#sessionrestoretestsswift) does; each submit replaces the whole window state.
- Drags while zoomed land in the zoomed pane and reach Neovim as clicks.

#### `SessionRestoreTests.swift`

**Covers** — the launch restore of a saved tab over a live core: a vertical split of two files, the second zoomed, submitted through `WindowStateCodec` and flushed, then its second file deleted, loaded back and rebuilt through `requestLayoutRebuild` with the load's `rebuild`. The pane on the kept file shows it in place, the pane on the deleted file shows the startup buffer `Untitled.md` rather than the missing file, and the view stays unzoomed at every wake of a settle.

**Gotchas**

- That an empty-path pane keeps the buffer it was split from is the rebuild helper's premise, which the test measures rather than assumes.
- It writes `LiveCore`'s `WOLF_STATE_DIR`, as [`ZoomPaneTests.swift`](#zoompanetestsswift) does.

#### `ViewportCaptureTests.swift`

**Covers** — the cursor and top-line capture of [§4](../../../../specs/application-state.md#cursor-and-scroll-capture) in a `PaneContainerView` over a live core: moving the focus off a pane captures its cursor and top line into `paneViews`, and nothing captures them before; `captureViewports()`, which the quit runs, captures every edit pane at once.

**Gotchas**

- The expected top line is read through `window_cursor`, never a constant, and a premise requires it to be neither 1 nor the cursor line, so a capture that saves no top line, or the cursor line as one, fails.
- A preview's scroll capture is manual-smoke, since no page loads here ([why](#what-renders-under-bazel-test)).

#### `ExtrasRestoreTests.swift`

**Covers** — the launch restore of a tab's extras, submitted through `WindowStateCodec`, flushed, loaded and rebuilt through `requestLayoutRebuild(_:tab:focused:drawer:)` over a live core and `/bin/sh`: a focused pane restores focused, and a focused drawer too; a saved cursor beyond a shortened file is clamped to its last line; an open drawer restores open at its saved share; and a zoomed drawer, or a zoomed pane beside an open drawer, restores zoomed.

**Gotchas**

- A focus or a zoom is sampled at every wake of a settle: Neovim reporting another pane a round trip after the rebuild would move the active pane, or unzoom it, after a first check had passed.
- The share is read as the drawer pane's top against the container's midline, since the drawer's own rect is private.
- It writes `LiveCore`'s `WOLF_STATE_DIR`, as [`SessionRestoreTests.swift`](#sessionrestoretestsswift) does. The tab-switch and window-close captures, a preview's saved scroll and the sidebar's change writes are manual-smoke, since AppKit routes no event offscreen and no page loads.

#### `OpenFolderRestoreTests.swift`

**Covers** — Open Folder's read of a project state over a live core: a directory's tab, a vertical split of two files, submitted and flushed, then a submit without it, loads back through `WindowStateCodec.loadProject(wolf_ffi_app_state_project_state(_:))` with its window's frame, and its `rebuild` through `requestLayoutRebuild` shows both files in order. A directory with no project state loads `nil`.

**Gotchas** — the second submit is what makes the read the stored project state rather than the window state. It writes `LiveCore`'s `WOLF_STATE_DIR`, as [`SessionRestoreTests.swift`](#sessionrestoretestsswift) does. The AppKit steps of Open Folder, its focus, fill and new window, are manual-smoke.

#### `OpenRequestFlowTests.swift`

**Covers** — [opening from outside Wolf](../../../../specs/cli-and-file-open.md) end to end. The real `wolf` sends each request over the socket; the test drains and routes it through `FileOpenSeam.route` with a live [`TerminalViewFixture`](#terminalviewfixtureswift) as tab 7 beside a frontmost decoy, and hands tab 7's files to `PaneContainerView.openFile(_:wait:)`, as `WolfWindowController.openFile` does.

A directory is Open Folder from tab 7, a file with no tab ID goes to its open project, and a stale ID falls back to resolution. In tab 7: a pane already on the file is focused; §5.2's four rules, a terminal always keeping its session; a zoomed tab unzooms; a missing file opens as an unwritten new buffer. Before the first bind, an open waits for a pending rebuild, and two opens both land.

A `wolf --wait` popped into `Request(owning:)` exits `0` at `:q` in its pane.

**Gotchas**

- The state dir is relative to a canonical base under `TEST_TMPDIR`, since no absolute socket path under Bazel's `TMPDIR` fits 103 bytes.
- The rule-3 preview is the left pane, since rule 3 takes only a leftmost one; `PreviewViewFixture` previews on the right, so the test makes that pane the terminal and previews again on the left.
- The rebuild case restores `vertical[f.md, b.md]` with pane 1 focused, so pane 0 already shows the file. Were rule 1 to pick the focused pane, the rebuild would run ahead of an early open's queued `:edit`, both landing there, and an early open would pass.
- `AppDelegate`'s queue and the raise are person checks.

#### `NewWindowPlacementTests.swift`

**Covers** — `NSWindow.cascade(fromTopLeft:size:)`, the [§7](../../../../specs/application-state.md#7-new-windows-and-tabs) placement: the window has the new size before its first `cascadeTopLeft(from:)` call and after the cascade, and that call starts at the active window's top-left and the second at the point the first answers, where the window's top-left ends.

**Decisions**

- A recording `NSWindow` subclass stands in for `cascadeTopLeft(from:)`, with unequal offsets so a swapped axis shows. The worker has no screen, and AppKit's own call then squeezes the window to 1 pt wide and pins its top-left at (0, 0). AppKit's own offset and its clamping to a real screen are a person check.

#### `CloseTabDispatchTests.swift`

**Covers** — `CommandDispatcher.dispatch(.closeTab, …)`, the arm a drained `close_tab` reaches, closing its window once with `close()` and never calling `performClose(_:)`, so no `windowShouldClose` asks again after the quit's own check.

**Decisions**

- A recording `NSWindow` subclass stands in for both calls. Its `performClose(_:)` records and goes no further, because AppKit's would call `close()` itself, and a dispatch that asks first would then look like one that closes.

#### `TerminalDrawerTests.swift`

**Covers** — `toggle_terminal()` showing a terminal view in the drawer strip on the drawer window's session, painting a truecolour background the shell prints, hiding with the session kept, and showing the same session again on the next toggle. Through `PaneContainerView.toggleTerminal()`, with the drawer open and an editor pane active, a toggle making the drawer the active pane, its session taking the keys, at its frame, and a second closing it with the session kept.

#### `TerminalToastTests.swift`

**Covers** — `toggle_terminal()`, which the palette's Terminal row runs, hiding the container's `ToastView` once the drawer shows its session, after a typed `:echo` showed it. A failure message tells a toast core never dismissed from a view the container never refreshed.

**Gotchas** — the echoed marker is typed as two quoted halves, so only the `:echo` running puts it on the toast, never the typed command line.

#### `TerminalNeighbourPaneTests.swift`

**Covers** — the Neovim panes staying painted while a terminal view is open and after it hides or closes, in the drawer and in a split beside an editor pane.

**Gotchas**

- Each test marks the editor with `markEditor()` and fails as a premise if the container never paints the mark. It then samples only where the editor alone paints: beside the terminal view while it is open, and inside the view's last frame once it has closed.
- A close leg waits for Neovim's side of the close, not for the view to go, so a view `syncTerminalViews` wrongly keeps still reaches the pixel check.
- Every check polls until `TerminalViewFixture.timeout`, so each red leg costs that long.

#### `TerminalPaneFlowTests.swift`

**Covers**

- `<C-w>h` in a terminal pane focusing the editor beside it, `<C-w>l` there focusing the pane again, and `<C-w>v` in the pane splitting it onto a new, focused session.
- `capturedTerminals()` naming the tree's terminal leaf, not the drawer's session, and the drawer open. After a Neovim kill and restart, `requestTerminalRestore` putting that session back as a terminal leaf beside the editor and reopening the drawer on its own session, both with their output.

**Gotchas** — the test calls `capturedTerminals()` and `requestTerminalRestore` itself. `TabCore.respawnNeovim` calls them only on a recording view, in [`TabCoreTests`](#tabcoretestsswift), so its hand-off to a real container is manual-smoke, and so is `leafContents()`'s terminal arm, as [`SessionSeamTests.swift`](../WolfKeyMapTests/README.md#sessionseamtestsswift) records.

#### `TerminalPaneRenderTests.swift`

Purpose: Renders a `PaneContainerView` whose current window shows a real `/bin/sh` session, over [`LiveCore`](#livecoreswift), and checks what its `TerminalPaneView` drew.

**Covers**

- A crimson background and bold blue text the program prints, in the cells it printed them.
- The session sized from the pane's content area, matching the placeholder grid and off the starting 80 × 24.
- The cursor cell in the cursor colour, or the default foreground when none is set.
- A drag selection painting swapped colours.
- A pane whose shell exited losing its terminal view while the pane stays.

**Gotchas**

- Its `TerminalPaneFixture` is internal rather than private because other test files reuse it, over their own `init.lua`.
- Background checks sample cell interiors, inset 1 point so no pixel straddles a neighbour. Glyph checks count blue pixels within 0.4, since antialiasing blends a glyph's edges.
- The selected cells have an unset foreground, so swapped they fill with the default foreground, not blue.
- The fixture's window reports key, so its pane is the focused pane and paints the frame's cursor style. The cursor test sets a steady block, which fills the whole cell.

#### `TerminalLighthouseColourTests.swift`

Purpose: Checks that a terminal pane's 16 ANSI colours are the vendored Lighthouse's, in the session's cells and in the pixels its `TerminalPaneView` paints, for the dark background and, after `set_background(false)`, the light one.

**Covers**

- Every foreground and background cell of the 16 colours matching Lighthouse's `g:terminal_color_0` to `_15`, and at least 0.9 of each background cell's interior painting that colour in sRGB, within `OffscreenRendererTests`' 0.01.
- An open pane repainting in the light colours when the background switches.

**Gotchas**

- The expected colours and the staged `init.lua` come from [`LighthouseColours.swift`](#lighthousecoloursswift).
- Every run prints a table of expected, cell and pixel colours.

#### `StatusbarStripColourTests.swift`

Purpose: Checks that a pane's statusbar strip paints Lighthouse's `Normal` background, the colour of the grid below it, in dark and, after `set_background(false)`, in light. It renders over [`LighthouseColours.swift`](#lighthousecoloursswift)'s staged `init.lua`.

**Covers**

- An editor pane's strip and a blank block of its grid, both `Normal`'s background, with means that agree.
- A terminal pane's strip and its view's blank cells, the same way, and the strip's pixel farthest from the background nearer `Normal`'s foreground than its background.
- A preview's strip while its web view occludes its grid, against a blank block of the source pane.

**Gotchas**

- The strip band is the top 2 points of a pane, inset 2 from its sides, above the text's vertical padding, so it holds no glyph. A preview's band keeps a further `EditorInset.padding` inside its web view's sides, which reach the pane's edges and, beside the divider, its band.
- `previewDidFinishLoad(win:)` stands in for WebKit's `didFinish`, which never comes here, so `PaneOcclusion` skips the preview's grid and its frame is never read.
- The window is no `MainWindow` and Lighthouse leaves `WolfTitlebar` unset, so a strip that follows the titlebar paints the black fallback.
- Every stage prints a table row of the expected colour, `read_frame()`'s default background, and each region's mean and share.

#### `TitlebarColourTests.swift`

Purpose: Checks the titlebar of a real `MainWindow`, never shown, hosting a `PaneContainerView` over [`LighthouseColours.swift`](#lighthousecoloursswift)'s staged `init.lua`. The window's `backgroundColor` is what shows through the transparent titlebar.

**Covers**

- The window's background is Lighthouse's `Normal` background in sRGB, and matches a blank block of the grid, in dark and, after `set_background(false)`, in light.
- Three wakes with an unchanged theme leave a colour set on the window alone.
- A `WolfTitlebar` defined at runtime colours the window, and `hi clear` restores `Normal`'s.
- Under each forced mode's `AppDelegate.appAppearance(for:)`, the window's effective appearance is the one `resolvesDark` gives, whatever the host's.

**Gotchas**

- The window is deliberately not pinned to Aqua, unlike `OffscreenRenderer`'s, so the appearance test sees what it inherits from `NSApp`.
- The appearance test sets `NSApp.appearance` and restores it on exit. The window is never key, so the active and inactive title colours are manual-smoke.
- Each background stage prints a table row of the computed `TitlebarBackground`, the window colour and the grid's mean and share. The appearance test prints the host appearance and each mode's resolved `windowFrameTextColor`.

#### `SidebarToolbarTests.swift`

Purpose: Checks the toolbar's sidebar control in a production `MainWindow`, never shown, over a [`LiveCore`](#livecoreswift). The core is real because a search click seeds the search toggles from Neovim's `ignorecase`.

**Covers**

- The toolbar running tracking separator, Back, Forward, then the control, with no system `.toggleSidebar`; the item navigational, and each segment carrying its seam tooltip.
- `showSidebar(panel:)` and `toggleSidebar()`, the paths of the `toggleSidebar`, `sidebarShowProject` and `searchInProject` commands, leaving only the visible panel's segment selected, and none while hidden.
- A direct `isCollapsed` write, as a restore or a divider drag makes, clearing and restoring the selection.
- Clicks switching, hiding and showing as `SidebarToolbarSeam.click` decides.
- Both panels' tops at the sidebar's safe-area top.
- `restoreSidebar(_:)`: a search sidebar shows its query and toggles, runs the search and selects the search segment; a tree sidebar keeps its query and toggles and runs nothing.

**Gotchas**

- `WolfWindowController` cannot be built offscreen: its `build()` needs a started `TabCore`, and orders its window front. The tests enter at the `MainWindow` members it forwards to, so the forwarders and the `CommandDispatcher` arms but `.closeTab`'s are compile-verified only.
- A click is `selectedSegment` set, then `sendAction`, as a `.selectOne` click does.
- Selection is read with `isSelected(forSegment:)`, because `selectedSegment` keeps its old value after a segment is deselected.
- Search seeding and field focus touch private `SearchView` state, so they are manual-smoke.

#### `FileTreeViewTests.swift`

Purpose: Checks that a sidebar row draws for its present state only, over the production `TreeCellView.make()`, `FileTreeView.configureOutline(_:)` and `FileTreeView.restyleRows(in:isCurrent:)` and four test files. No Neovim: the core reaches these cells only as the `isCurrent` bit.

**Covers**

- A click emphasizes the rows, selects, captures, then de-emphasizes; the render wake marks the file current, selects it and restyles. After three files in turn, the two rows current at earlier clicks draw ink that matches a never-touched row, far from the accent, and each such cell is not current, has a `.normal` background style, and draws `labelColor`.
- The positive control: at a click the still-current row draws in the accent and the untouched row does not, which proves the sampler sees the accent.

**Gotchas**

- Pixel checks sample unselected rows only: a selected row's fill covers its text.
- The property checked is the label's drawn `attributedStringValue` foreground, not `textColor`, which reads `labelColor` even while the row draws in the accent.

#### `SymbolsFallbackTests.swift`

Purpose: Checks the symbols cascade [`CellMetrics`](../../Sources/WolfMacOS/README.md#cellmetricsswift) gives every grid face. A test draws one cell through the real `GlyphPainter` into a gray bitmap three cells square, and compares it with the cell a reference painter draws over the same geometry: one whose faces are the symbols face itself, or the plain configured faces with only the system cascade behind them.

**Covers**

- U+F07B and U+E0A0 drawing from the symbols face, unlike the system cascade alone: iA Writer Mono S regular at the launch size and its three styled faces at a stepped size, and U+F07B in JetBrains Mono regular and bold.
- A Powerline glyph JetBrains Mono maps itself, and `a`, `g` and `M`, drawing from the configured face.
- Cell width, height and baseline unchanged by the cascade.
- Hangul 한, which neither the configured nor the symbols face maps, drawing as the system cascade draws it.
- A symbols family that does not resolve leaving every face uncascaded.
- In a live editor grid, an icon's overflow past its cell surviving the next cell's fill.

**Gotchas**

- A message starting `premise:` means the comparison cannot tell the outcomes apart or the staging is wrong, never that the cascade is.
- The gray bitmaps draw at scale 2 whatever the host, so a 14 pt icon covers enough pixels to compare.
