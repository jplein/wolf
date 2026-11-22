# wolf-macos decisions

Last verified at commit `500d64fda2e3d0313b1f527edeb8954d468c40d5`.

The durable constraints behind this crate. Read this before changing anything here; most of it exists because a reasonable-looking simplification breaks something. Per-file detail lives in the directory READMEs under [`Sources/`](../crates/wolf-macos/Sources/README.md).

## Platform and build

- Tahoe (macOS 26) and newer only (`LSMinimumSystemVersion` / `minimum_os_version` = `26.0`).
- AppKit, not SwiftUI. Implementation language Swift, Xcode 26+.
- The Bazel build is authoritative: `rules_swift` and `rules_apple` build the `.app`. `rules_xcodeproj` generates an uncommitted `.xcodeproj` for debugging and LLDB only.
- Rust and Swift meet through swift-bridge, confined to [`wolf-ffi`](../crates/wolf-ffi/README.md). The generated bindings are committed.

### Why `:WolfRelease` is a custom rule

`release.bzl` defines the `opt_bundle` rule that `:WolfRelease` instantiates. Compilation mode is a Bazel configuration rather than an attribute, so no `alias` or `macos_application` wiring can set it; `opt_bundle` applies a Starlark outgoing-edge transition to its single `bundle` dep, setting `compilation_mode` to `opt` so the whole subgraph rebuilds optimized without `-c opt` on the command line.

It forwards only `DefaultInfo`, deliberately not the Apple providers, because nothing consumes them here and forwarding Apple providers from a non-Apple rule invites `rules_apple` to treat the target as an embeddable bundle.

The rule is executable: it symlinks the transitioned `macos_application`'s own runner into its predeclared output and forwards the dep's runfiles, which is how that runner locates `Wolf.zip`. The runner's app path is a bare working-directory-relative runfiles path, so dropping the runfiles merge — or passing `--nobuild_runfile_links` or `--enable_runfiles=false` — breaks it.

For frame-stats measurement, exec `"$APP/Contents/MacOS/Wolf"` directly rather than using `bazel run`, which unzips a temporary copy to a path that changes every launch and interleaves the app's stderr with Bazel's client output.

### Why `:InstallWolf` takes the archive through a filegroup

The installer is handed its input as `$(rootpath :wolf_release_archive)`, a filegroup wrapping `:WolfRelease`, and the indirection is not redundant. Location expansion on an executable target yields that target's executable, so `$(rootpath :WolfRelease)` resolves to the runner symlink rather than to `Wolf.zip`. A non-executable filegroup has no executable to prefer, so expansion falls back to `files`. Collapsing it is not a build error: the script receives the runner and fails at extraction time.

It carries `manual` for the same reason `:WolfRelease` does. It depends on the transitioned bundle, so without the tag every wildcard build would rebuild the whole Rust, Swift and treesitter graph a second time at `-c opt`.

## The pure seam

Swift here is split in two, and the split is the single most important structural fact about the crate.

[`Sources/WolfKeyMap/`](../crates/wolf-macos/Sources/WolfKeyMap/README.md) is Foundation plus the generated FFI glue, with no AppKit. Every decision that can be expressed as a function of values lives there, so it can be unit-tested with no window server. [`Sources/WolfMacOS/`](../crates/wolf-macos/Sources/WolfMacOS/README.md) is the AppKit application, and holds only what genuinely needs a window. The process entry point sits apart, in the one-file [`Sources/WolfMain/`](../crates/wolf-macos/Sources/WolfMain/README.md).

The consequence is the crate's testing posture. There is no window server under `bazel test`, but a view in a never-shown window still renders through `cacheDisplay`, so [`Tests/WolfMacOSTests/`](../crates/wolf-macos/Tests/WolfMacOSTests/README.md) checks what a `WolfMacOS` view draws. It also calls a responder-chain step and the main menu's key-equivalent matching directly, over a real `WKWebView` whose page never loads ([why](../crates/wolf-macos/Tests/WolfMacOSTests/README.md#what-renders-under-bazel-test)), and `PaneContainerView`'s mouse handlers with synthesized events. **Key-window routing, event delivery such as `keyDown:`, menu validation and dispatch, WebKit's web-process step and real clicks are compile-verified and manual-smoke only**, and XCUITest is unavailable.

Coverage comes from the pure seams via [`Tests/WolfKeyMapTests/`](../crates/wolf-macos/Tests/WolfKeyMapTests/README.md), the bridge surface via the real-nvim tests in `//crates/wolf-ffi`, and the core logic via `//crates/wolf-core`. The drained Ex-command route into `CommandDispatcher`, and a zoom across quit and relaunch, run against the real bundle only in the on-demand [app-launch smoke](../build.md#the-app-launch-smoke). When you add behavior here, put the decision in `WolfKeyMap` and test it there; leaving it in a view class means only what it draws can be tested.

## Key translation lives in Rust

Swift sends a structured `FfiKeyEvent`; `wolf-core` owns nvim notation. Do not add notation-building to Swift.

## Threading

`wolf-core` runs tokio on its own threads and fires a wake on `flush` and on each terminal session's frame publish. The wake is the registered `extern "C"` callback (see [`wolf-ffi`](../crates/wolf-ffi/README.md)); `WakeBridge` hops it onto the AppKit main thread and hands it to the token's `TabCore`, whose view runs `onRenderWake` and repaints.

`WolfCoreHandle` is not `Send` and is used only on the main thread. Frame-stats state is main-thread-only too, enforced by a debug-only `dispatchPrecondition`.

## One Neovim per tab

[The model and its cost](../specs/application-state.md#8-one-neovim-per-tab) are the spec's. On macOS every tab owns a `TabCore`: one captive Neovim, its wake token and its crash restart, serving that tab's view alone. It starts when the tab is created or restored, background tabs included, and shuts down in the tab's teardown. Per-tab Neovims avoid the edge cases of one shared by several views: `:cd` moving every view of a directory, one crash or stall taking them all down, and a background view freezing on its last frame.

Wakes route through the core's `wakeToken`, looked up in `WindowRegistry`, because one process-global callback per channel serves every core. That indirection is why two tabs never clobber each other's redraws. `TabCore` reaches its tab through the `TabCoreView` protocol, so a live test can stand a recording view in for `WolfWindowController`, which cannot be built offscreen.

`Chdir` keeps `:tcd`, which moves a one-tabpage Neovim's tab alone ([Input: events](../specs/architecture.md#input-events-front-end--core)). Core keeps it because it also serves a front-end whose tabs are tabpages of one Neovim, where `:cd` would move every tab.

A tab is focused while the app is active and the tab's window is main, by [`TabFocusSeam`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#tabfocusseamswift). Each change reports a loss to the previous tab's Neovim and a gain to the next one's, and a gain fires `FocusGained`, whose `:checktime` reloads a file another tab saved. Main rather than key, because a picker panel takes key and cannot become main: key-based focus would report a loss and a gain around every picker use, and each gain also rescans the file tree.

## State is a tree, and the front-end owns every write

Window frames, ordered tabs, each tab's working directory, sidebar visibility, and full pane-split layout persist to `state.json` and restore at launch before the windows are shown. The coordinator walks its live windows and tabs and commits the whole tree through the `wolf-ffi` snapshot builder. Core does not mirror the working directory into a store, and there is no per-window slot.

Every Alive core wake requests a commit, alongside the geometry, sidebar, cwd and tab-group triggers. Opening, splitting or closing a pane fires nothing else, and a force quit or crash skips the terminate commit, so without it those files are lost. The request is throttled rather than debounced, so continuous wakes cannot starve the commit, and committing an identical tree is a no-op, so the frequent commits cause no disk churn.

**Each view contributes only a tab it can read whole, and otherwise its cache.** Leaves are read by window handle from `previews` and `win_buffers`. A view whose cwd or any leaf is not yet reported refuses, and contributes its last tab, which starts as the restored one. The launch commit runs before any pane binds, so a partial tab there would replace the saved files with nothing.

Two restored views never gather and go on contributing their seed. One whose rebuild failed shows a fallback pane rather than its saved split. One restored to the empty state because its saved folder was missing keeps that folder until the user opens another, so remounting a volume brings the session back.

**Closing the last window is quitting, so it commits the tree with that window in it.** The app terminates after its last window closes, and a commit after the close would find no live view. The commit writes the tree as it stood at the first close of that runloop turn, because a tabbed window's close button closes each tab's window in turn, and by the last close the earlier tabs are torn down. So the closing window keeps all its tabs, and a close-all keeps every window.

A tab closed alone ends its close burst on the next turn, so it drops, and a later close of the last tab keeps only that tab. That AppKit closes a tab group's windows within one turn is a premise no automated test measures. Any other close commits without the closing view, and a close during termination commits nothing over the terminate commit. A walk with no live view or no window commits nothing at all, because it would erase the session.

Saved tabs that no view took at launch, because their core failed to start, are never retried in that session. They are carried into every commit so the next launch tries them again.

Only directories the user actually opened persist. The passive launch working directory is deliberately not written and not recorded in the recent-directories list, which is what keeps Open Recent clean. A frame restores only when it still intersects a live screen, and a tab's working directory only when it is still a directory; an empty or deleted one restores to the empty state rather than falling back to the process working directory.

Restore starts one Neovim per saved tab, in window then tab order, at the tab's saved directory, or at home when that is empty or gone. Nothing groups tabs by directory, so several tabs on one directory each come back with their own Neovim.

## One flat pane view, not nested split views

`PaneContainerView` paints every pane itself, computing each pane's rectangle from the pure `PaneTree` port. It is deliberately not a tree of nested `NSSplitView`s. Divider drag is done by hit-testing the pure `dividers()` list inside the flat view, and the terminal drawer's top edge beside it.

Weight policy: a same-orientation split re-equalizes the row or column to equal shares, while removing a pane re-normalizes the survivors proportionally. That is what makes a user-resized layout survive an adjacent pane closing, a tab switch, and quit and relaunch.

Every pane's body paints identically whether active or inactive. The only active/inactive treatment is statusbar text weight and dimming, because that distinction is awkward to express inside Neovim itself.

The statusbar strip takes the editor's default colours, `Normal`, as wolf-gnome's does, and never the titlebar colour. A colorscheme's `WolfTitlebar` colours the titlebar and never the grid, and the strip must match the grid it sits on. `PaneContainerView.draw` reads them from grid 1's frame once per draw, not from each pane's frame, because an occluded preview and a terminal pane never read their own.

## The titlebar

The window's `backgroundColor` shows through its transparent titlebar. It takes a set `WolfTitlebar` background, else `Normal`'s, read from grid 1 once per wake as the statusbar strip reads it and built in sRGB as the grid is, so the titlebar matches the grid below it. Only before the first frame, when neither is known, does it keep the system colour. The rule is [`TitlebarBackground`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#titlebarbackgroundswift).

The title text is AppKit's, drawn in the title colours of the window's effective appearance, active or inactive. It follows Wolf's resolved appearance and not the OS's, because an OS in light mode under `appearance: dark` would otherwise draw the light-mode title, nearly invisible on the dark titlebar. It takes no colour from the colorscheme, by the user's decision.

So no window or view sets its own `appearance`: every window inherits the `NSApp.appearance` that `AppDelegate` forces from Wolf's resolved appearance, which in `auto` is the OS's. Setting one per window, whether from `Normal`'s luminance as Ghostty does or by pinning Aqua as the tests' `OffscreenRenderer` does, would let the title and Neovim's `background` disagree.

## The toolbar

After the traffic lights the toolbar runs the sidebar tracking separator, Back, Forward, then the sidebar control, with the window title after them; the order is [`HistorySeam.itemIdentifiers`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#historyseamswift). The separator comes first and is bound to the split's divider, so the items after it ride the sidebar's trailing edge.

The control's selected segment is derived in one place, [`SidebarToolbarSeam`](../crates/wolf-macos/Sources/WolfKeyMap/Sidebar/README.md#sidebartoolbarseamswift), from the sidebar's visibility and mode. `MainWindow` applies it whenever the split item's `isCollapsed` changes and at the end of `showSidebar(panel:)`. No path that changes the sidebar sets it: not the commands and their shortcuts, the View menu, a divider drag, the launch restore, nor the control's own click. Set from each path, the control goes stale after any path that forgets, a later one included; observing the state misses none.

Each tab is its own `MainWindow`, with its own toolbar and observer, so a tab switch needs no refresh of its own.

## How a preview reaches its web view

Four files jointly honor one model, so do not change any of them without reading this section: `WolfKeyMap/PreviewScrollSeam` owns the rules, `PaneContainerView` the bookkeeping, `PreviewScrollStateHandler` the state channel, and `PreviewNavigationDelegate` the navigation count.

A rendered document reaches the page one of two ways, chosen per preview window by `PreviewScrollSeam.updateStrategy`. The typing path swaps only the body in place, which is what stops the inlined base64 font CSS in the head from being re-shipped across the WebKit boundary and re-parsed on every keystroke. Every other case navigates with `loadHTMLString`. The swap is sound only because the in-app render pins one theme and one width, so the head is byte-identical every time; the strategy compares head lengths so that premise is self-enforcing rather than a comment.

The in-place body travels as a real `callAsyncJavaScript` argument, never interpolated into JavaScript source, and it must run in the content world `addUserScript` installed the page script into. A wrong world returns the falsy sentinel on every call, pinning every window on the full-load path silently and with a green suite. That is the one failure here no test catches, so it is what the manual smoke check is for.

Scroll state is pushed out of the page, never pulled. A full load destroys `window`, so nothing can be stashed in the page across a reload, and `evaluateJavaScript` is asynchronous, so state cannot be read in the instant before one. A continuously refreshed Swift-side cache makes the pre-reload capture a plain dictionary read with no race.

Three script-message handlers ride on one preview web view's content controller — click-to-focus, scroll state, and selection — under three names that must differ, since a duplicate name silently replaces the handler already registered and kills that channel with no error. Every one of them must be removed at teardown, or the controller keeps the container alive.

Edit > Copy reads the pushed selection cache for the same reason the pre-reload capture reads the scroll cache: menu validation is synchronous and `evaluateJavaScript` is not. That cache is cleared synchronously on both document-replacement paths rather than trusting the page to report the collapse; a push landing in the frame or two before WebKit commits the replacement can still re-populate it, which is benign because such a push reads the live selection. The rules behind the channel and the copy-source decision are in [`PreviewCopySeam`](../crates/wolf-macos/Sources/WolfKeyMap/README.md).

Navigations are counted, not flagged. Two can straddle one page load, because the reload coalescer does not wait for `didFinish`, and WebKit reports the superseded one as a failure — so a failure callback is a routine part of the typing path, not an error path. A count lets a superseded callback decline to commit anything; a flag would let one clear the gate out from under a still-navigating load, which flickers the pane.

Recovery is one step and self-limiting. A failed in-place update clears that window's live-document descriptor and re-arms the reload gate, so the next pass takes the full path: at most one extra load, no loop, and normal service resumes as soon as one commits.

## Key equivalents over a preview

A click in a preview leaves its `WKWebView` the window's first responder, and while it is, WebKit claims every key equivalent and sends it to the page. When the page leaves the key unhandled, WebKit sends the binding's command up the web view's responder chain with its own sink, `WKResponderChainSink`, at the end. An unbound ⌘ or ⌃ chord's command is `noop:`. Only when the sink receives the command does WebKit resend the event through `NSApp.sendEvent`, where the main menu sees a ⌘ chord and `PaneContainerView.keyDown` passes an unbound ⌃ chord to Neovim.

So `PaneContainerView`, the web view's next responder, and any responder placed between the web view and the window must pass every command it does not handle on to `super`. One that returns without passing it on drops every shortcut over a clicked preview, Close and Command Palette… among them, while a preview made active with `<C-w>l` keeps them, because the container stays first responder.

Arrow keys and space scroll the page through WebCore's own key handlers and never reach the chain, so the preview keeps them whatever the chain does. ⌘C is not among them: Edit > Copy is a menu item, and over a clicked preview ⌘C reaches it only through the same resend, so Copy depends on this rule as Close and Command Palette… do.

## Terminal panes

[Terminal panes](../specs/terminal-panes.md) owns the contract. These are the macOS choices behind it, in `PaneContainerView` and the seams it calls.

A terminal pane's content is painted by its own [`TerminalPaneView`](../crates/wolf-macos/Sources/WolfMacOS/Terminal/README.md#terminalpaneviewswift) subview, for the reason that entry gives, while the container still paints its statusbar and scrollbar and never reads its placeholder grid.

Any drawing subview of `PaneContainerView` must clip to its bounds or fill only its bounds. Under the SDK's default `clipsToBounds == false` a dirty-rect fill reaches past the view and covers the panes the container painted, so an unclipped terminal pane blanks every Neovim pane beside it; [`TerminalPaneView`](../crates/wolf-macos/Sources/WolfMacOS/Terminal/README.md#terminalpaneviewswift) sets the clip.

Input methods reach a terminal pane through `NSTextInputClient`, and only a terminal pane: Neovim panes keep `KeyComposer` unchanged. A key goes through `interpretKeyEvents` unless it is a Ctrl, Cmd or option-as-alt chord or a named key, and a commit of one printable ASCII character is sent as a key, so core's `<C-w>` router still sees it. The rules are [`TerminalInputSeam`](../crates/wolf-macos/Sources/WolfKeyMap/Terminal/README.md#terminalinputseamswift)'s.

Among a view's panes showing a session, [`TerminalGeometry.sizer`](../crates/wolf-macos/Sources/WolfKeyMap/Terminal/README.md#terminalgeometryswift) picks the active pane, then the last sizer, which makes the spec's most recently focused pane the sizer.

The size takes `fullWidth(forWin:)` rather than `fullWidth(forGrid:)`, because the pane's grid may still be pending. A size is resent when it or the frame's size differs from what was sent, but an exited session applies no resize, so its frame size is never compared.

A session holds focus while its pane is the active pane of the key window, and `syncTerminalFocus` reports the loss before the gain. It runs on every active-pane change, every wake and both key-window edges, and app deactivation resigns the key window. The loss matters beyond mode 1004: core rescans the project on it, so a restart reports it too.

The same call sets each `TerminalPaneView`'s `focused`, which picks the cursor's shape under [the render contract](../specs/terminal-panes.md#the-render-contract). The painter decides rather than core, because focus belongs to a pane and a frame to a session, and the container knows its active pane at once, where core would learn it a wake late and publish a frame although no cell changed.

Close Pane closes the drawer although the drawer is outside `tree`, and refuses the tree's last leaf even beside the drawer: Neovim counts the drawer's window, so `:close` there would leave the drawer as the tabpage's only window, which the pane tree cannot hold. `closeActivePane()` sends `focus_win` for the active pane's window before `close_pane()`, both on core's ordered input channel, so `:close` closes the pane `canClosePane` read even while a focus move is in flight. `WolfWindowController.closePane()` goes through it, so a dispatched Close Pane is refused wherever the menu item is disabled.

`splitActivePane(_:)` sends `focus_win` before its split for the same reason, so Split Horizontal and Split Vertical split the pane this view shows as active.

After a crash-restart, `TabCore.respawnNeovim` captures the tab's terminal-pane sessions and drawer state before `handle.restart()`, and hands the view its [`terminalRestore`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#livenessseamswift) steps. The view runs them once, from a render wake after `ensureInitialPane` has bound its first pane and no layout rebuild waits ([why](crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift.md#terminal-restore)). `resetAfterRestart()` clears a request made before it.

The drawer's top edge is no `PaneTree` divider, so the container hit-tests it at `DrawerLayout.editorHeight` itself, with the tree dividers' tolerance and cursor. A drag sets the share the drawer takes, which a `nil` from `draggedWeight` leaves as it is.

That share is the container's, held outside `tree`, so it never reaches `paneLayout` or `state.json`, as [Session restore](../specs/terminal-panes.md#session-restore) requires, and a new view starts at `DrawerLayout.defaultWeight`. Closing the drawer and a Neovim restart keep it: `resetAfterRestart()` leaves it alone, because the share belongs to the window, not to Neovim. A drawer drag ends without `onLayoutChanged`, since nothing it changes is persisted.

## Zoom Pane

[The intents contract](../specs/architecture.md#output-intents-core--front-end) owns what Zoom Pane does. These are the macOS choices behind it. Every rule is [`PaneZoom`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#panezoomswift)'s, so `PaneContainerView` holds one value, `zoomedPane`, and the panes the toggle considers are the tree's leaves plus the drawer, so the drawer can be zoomed.

Zoom is front-end geometry, and Neovim is never told. `:only` would close the hidden windows, and `<C-w>_`, `<C-w>|` or `:resize` would size Neovim's frames, which are not the pane geometry: the front-end sizes each grid with `ResizeGrid`. The view never touches `tree`, its weights or `drawerWeight`, so an unzoom restores them exactly, and `paneLayout`, `leafContents()` and `capturedTerminals()` read the unzoomed tree that `state.json` records. `toggleZoom()` fires `onLayoutChanged`, because it is the one zoom change made outside a render wake, and every wake already schedules a snapshot. A new view starts unzoomed.

A hidden pane keeps its last grid and session size. `paneRects()` returns the zoomed pane alone, and the grid and session sizing read it, so nothing is sent for a hidden pane; that rests on Neovim re-dividing only the grids the front-end has not sized, so a sized hidden grid holds with no request. Unzooming then needs no reflow: no hidden buffer rewraps and no hidden shell is resized twice. After a window resize while zoomed, the hidden panes are sized once, on unzoom, to the restored layout.

Hidden terminal views and preview web views are kept with `isHidden` set, never removed. A removed web view reloads and loses its scroll state on unzoom, and one left shown paints over the zoomed pane, since a subview draws above the container's own paint. A hidden preview keeps its maps and its reloads, as a shown one does.

The zoomed pane closing unzooms whatever closed it, Toggle Terminal on a zoomed drawer included, since the check runs right after `detectCloses()`. A `Split` intent the view applies unzooms first, whatever made it, a terminal split included, and so does a `DrawerOpen`, since a pane joining the layout must show. A hidden pane closing keeps the zoom, its siblings renormalizing as they do unzoomed, and so do a window resize and a tab switch.

A focus move unzooms through one event, `.focusMove(target:)`. `applyFocus` starts from `PaneZoom.focusOrigin`, the zoomed pane while zoomed, and resolves through `DrawerFocusSeam.focusTarget` in the unzoomed layout, so `<C-w>h/j/k/l`, the router's chords from a terminal and both drawer bridges reach the pane they would unzoomed. A move with no target there keeps the zoom. The tree dividers and the drawer's top edge stay inert, with neither hit tests nor cursor rects, since they are hidden; the other-pane commands get `0` from a zoomed tree leaf, so they take their split path and unzoom, and `nil` from a zoomed drawer, as unzoomed.

The view trusts Neovim's current window. `updateActivePane()` applies `.focusMove` to the pane it resolves to, so leaving the zoomed pane by a path the view does not intercept, such as `<C-w>w`, `<C-w>p` or `:wincmd`, unzooms and the view follows. There is deliberately no guard for focus still in flight: only a zoom made within one Neovim round trip of a click reads a stale window, which unzooms harmlessly and which no human input reaches. Zooming sends `focus_win` for the zoomed pane's window, as `closeActivePane()` does, so Neovim's current window is the pane the view shows.

Toggle Terminal applies `.toggleTerminal(drawer:)` and then always toggles the drawer. It unzooms first unless the drawer is the zoomed pane, whose zoom lasts until `.panesChanged` sees the drawer close: unzooming on the toggle would show the hidden panes for a round trip before the close.

The save path marks the zoomed leaf through `zoomedPaneId`, `SessionSeam.gatherTab` and `PaneLayoutCodec.encode`. The drawer is no layout leaf, so a zoomed drawer persists unzoomed. The restore re-zooms in `rebuildLayout(from:node:)`, applying `.restore` to the leaf at `zoomedLeafIndex` after a successful zip and making it active; the handle-count fallback returns first, so it restores unzoomed, and `.restore` refuses a lone pane, which has nothing to hide. The restore is safe under the trust above only because the rebuild leaves Neovim's current window on that leaf ([why](wolf-core-decisions.md#the-persisted-zoom-mark)).

A crash-restart unzooms, because it rebuilds no saved leaf: `resetAfterRestart()` drops the tree and the zoom, each live split session comes back as a `Split` intent, and the drawer through `toggle_terminal()`'s `DrawerOpen`, both of which unzoom. A launch rebuild still pending at the restart is the exception: `pendingLayoutRebuild` survives the reset, so that rebuild re-zooms its leaf as a launch does.

## Nerd Font icons

Every grid face, in the Neovim grid and the terminal panes alike, falls back to the bundled Symbols Nerd Font Mono for a glyph it lacks, so the Private Use Area icons that prompts, `ls` replacements and Neovim plugins print render under any configured family, as in Ghostty. The cascade list is built in [`CellMetrics`](../crates/wolf-macos/Sources/WolfMacOS/README.md#cellmetricsswift).

The symbols face sits behind the configured face, not ahead of it, so a family that carries its own icons keeps them: a patched Nerd Font the user configured, or JetBrains Mono's Powerline glyphs. It sits ahead of CoreText's system list because the system fallback does not reach the registered face for these code points, which `SymbolsFallbackTests` checks as a premise. The system list follows it, so emoji and other scripts still fall back to the system's faces.

The Mono face is the one cascaded because it is drawn unscaled, and every Mono glyph advances one em, where the proportional face's advances vary and run wider ([measurements](../bazel/nerd-fonts-symbols/README.md#why-the-mono-face)). One em is still wider than a text cell, which is why both painters fill a row's backgrounds before its glyphs ([`GlyphPainter`](../crates/wolf-macos/Sources/WolfMacOS/Terminal/README.md#glyphpainterswift)).

## Present limitations

- Only windows opened via the overridden split commands (`<C-w>s`/`v`, `:split`/`:vsplit`, Split Horizontal and Split Vertical) become panes. Help, quickfix, preview, and `:diffsplit` windows do not, and floating or external windows are ignored.
- The grid is capped at a hard-coded 90 columns and centered in a wider pane, with the margins painted in the grid's own background. Making the cap configurable is a possible enhancement.
- Scrollbar thumbs are non-interactive; thumb-drag is deferred.
- Markdown preview ships one theme. Cursor-follow anchors to the containing block's top inside a fenced code block, an HTML block or a table, and inside a paragraph hard-wrapped across several short source lines; an image's alt text collapses to a single anchor just after the image. The marker rules are in [the preview position-marker contract](wolf-core-decisions.md).
- A symbols-face icon draws unscaled, about one em wide, over part of the cell after it, rather than fitted to its own cell as Ghostty fits it.
- In a terminal pane, the cursor and blinking text draw steady, and double, dotted and dashed underlines draw as the single straight line.
- Appearance flows one way only: colorscheme to app chrome works, OS theme to Neovim theming does not.
- A Neovim crash restart reopens the tab's open files into one pane, so its splits collapse, and the next commit records that. Restoring the pane topology across a restart would change the restart model.
- Only what a pane shows persists: `state.json` carries buffer paths only as layout leaves, so a file the user switched away from in its pane is not reopened at launch.
- Per-pane navigation history is not persisted across launches, and is not cursor-position history — Neovim's own jumplist (`<C-o>` / `<C-i>`) is untouched. Navigating back to a file deleted from disk opens an empty `[New]` buffer, exactly as `:e missing.md` would.

## Frame-time instrumentation

The crate carries an off-by-default frame-time measurement surface for finding where main-thread time goes on the keystroke-to-pixel path. Recording a sample never changes what is painted.

`WOLF_FRAME_STATS=1` enables it, `WOLF_FRAME_STATS_INTERVAL` sets the report period, and `WOLF_FRAME_STATS_FILE` also appends each report to a file. With the flag unset the cost at each call site is one `Bool` test. See [Building Wolf](../build.md) for the terminal invocation — a double-clicked `.app` inherits `launchd`'s stderr and shows nothing.

One caveat when reading a report: `wakes/key` is a counter ratio, so it is an upper bound rather than real attribution. The per-bucket caveats, including what `prev.html` does and does not time, are in [`Sources/WolfKeyMap/README.md`](../crates/wolf-macos/Sources/WolfKeyMap/README.md).
