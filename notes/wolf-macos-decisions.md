# wolf-macos decisions

Last verified at commit `0a440f06a1406f753b8939df335107bed4cfd578`.

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

### Why the bundle version is raised by hand

Raise `CFBundleShortVersionString` and `CFBundleVersion` in `Info.plist` whenever it gains or changes a declaration Launch Services reads: document types, imported or exported types, URL schemes. Launch Services prefers the highest version among the copies that share `org.wolf.WolfMacOS`, and stale copies stay registered: Bazel's temporary bundles, the app-launch smoke's copies and workers' build outputs. With the versions tied, Finder can take an old copy, without the new declarations, for Wolf.

The version is a literal kept by hand. Deriving it from git or stamping it at build time would change an input of the bundle on every commit and defeat Bazel's caching of it. `bundle_layout_test` pins the exact values, so a bump edits the test in the same change.

The version alone does not refresh the installed copy's record, which Launch Services can keep across a reinstall; `:InstallWolf` force-registers it ([why](crates/wolf-macos/install_wolf.sh.md#launch-services-registration)).

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

Each window's normal frame, its tabs with the selected one, whether it is minimized, and each tab's working directory, sidebar visibility, pane-split layout and [extras](#the-per-tab-extras) persist as the window state of [application-state.md](../specs/application-state.md), and restore at launch before the windows are shown. The coordinator walks its live windows and tabs and submits the whole tree through [`WindowStateCodec`](../crates/wolf-macos/Sources/WolfKeyMap/Persistence/README.md#windowstatecodecswift), and the store writes each open directory's project state in the same transaction. Core does not mirror the working directory into a store, and there is no per-window slot.

The stacking order is the coordinator's own [`StackingOrder`](../crates/wolf-macos/Sources/WolfKeyMap/Persistence/README.md#stackingorderswift), never asked of AppKit. A tab moves to the front when its window becomes main, the edge tab focus follows, so a picker panel taking key moves nothing. A minimize marks the window's whole tab group, which AppKit minimizes as one, and a restore from the Dock unmarks it. Each commit writes the windows in that order, minimized ones behind, each with its tab recency for the project-state rule.

Every Alive core wake requests a commit, alongside the geometry, sidebar, cwd, tab-group, focus and minimize triggers and the sidebar panel's own changes. Opening, splitting or closing a pane fires nothing else, and a force quit or crash skips the terminate commit, so without it those files are lost. The request is throttled rather than debounced, so continuous wakes cannot starve the commit, and committing an identical tree is a no-op, so the frequent commits cause no disk churn.

**Each view contributes only a tab it can read whole, and otherwise its cache.** Leaves are read by window handle from `previews` and `win_buffers`. A view whose cwd or any leaf is not yet reported refuses, and contributes its last tab, which starts as the restored one. The launch commit runs before any pane binds, so a partial tab there would replace the saved files with nothing.

Two restored views never gather and go on contributing their seed. One whose rebuild failed shows a fallback pane rather than its saved split. One restored to the empty state because its saved folder was missing keeps that folder until the user opens another, so remounting a volume brings the session back.

**Closing the last window is quitting, so it commits the tree with that window in it.** The app terminates after its last window closes, and a commit after the close would find no live view. The commit writes the tree as it stood at the first close of that runloop turn, because a tabbed window's close button closes each tab's window in turn, and by the last close the earlier tabs are torn down. So the closing window keeps all its tabs, and a close-all keeps every window.

A tab closed alone ends its close burst on the next turn, so it drops, and a later close of the last tab keeps only that tab. That AppKit closes a tab group's windows within one turn is a premise no automated test measures. Any other close commits without the closing view, and a close during termination commits nothing over the terminate commit. A walk with no live view or no window commits nothing at all, because it would erase the session.

Saved tabs that no view took at launch, because their core failed to start, are never retried in that session. They are carried into every commit, under their window's saved frame, active tab and minimized flag, so the next launch tries them again.

**The saved frame is the normal frame.** `StatePersistence` records a window's frame on each geometry change only while it is neither zoomed nor full screen, so a quit while zoomed or in full screen reopens at the earlier size. A tab group writes its selected tab's normal frame, else any member's, else the selected window's frame.

**At quit, the window state is on disk before any Neovim exits.** `applicationWillTerminate` runs `SessionSeam.terminate`: every tab captures its cursors and scroll, then the tree is committed and `wolf_ffi_app_state_flush` blocks on the writer, and only then does each teardown shut its core down. `WolfCoreHandle::shutdown` does not flush the store, so nothing else orders the two, and a capture after a teardown has no Neovim to read.

Only directories the user actually opened persist. The passive launch working directory is deliberately not written and not recorded in the recent-directories list, which is what keeps Open Recent clean. A frame restores only when it has a position and still intersects a live screen, and otherwise keeps a new window's placement. A tab's working directory restores only when it is still a directory; an empty or deleted one restores to the empty state, with no rebuild, rather than falling back to the process working directory.

Restore starts one Neovim per saved tab, in window then tab order, at the tab's saved directory, or at home when that is empty or gone. Nothing groups tabs by directory, so several tabs on one directory each come back with their own Neovim. Each folder tab rebuilds `FfiAppTab.restored()`, in which a missing file is an empty pane and a zoom on it is dropped, while its cache keeps the tab as saved.

The windows come back frontmost first: assembly orders them back to front, each with its selected tab in front of its group, then minimizes the minimized ones and makes the first other one key. Only then does the launch seed the stacking order from them, because each restored `build()` makes its window key in creation order, and those focus changes would reorder it.

## The per-tab extras

A tab also saves its focused pane, its sidebar's mode, query and toggles, its drawer, each edit pane's cursor and top line, and each preview's scroll ([the model](../specs/application-state.md#2-the-state-model)). The drawer is no layout leaf, so a focus on it is the tab's `focused` reference and its zoom is `SavedDrawer.zoomed`, never a leaf mark.

**Cursors and scroll are captured, never read at write time.** `PaneContainerView.paneViews` keeps each leaf's last capture, and the gather writes it. Captures run at [§4's moments](../specs/application-state.md#cursor-and-scroll-capture) only: `setActivePane` captures the pane it leaves while that pane is still bound, `refreshTabFocus` the tab the main window leaves, `closeCommit` every live tab of the closing group before the burst reads, and the quit every tab. App deactivation changes no main tab, so it captures nothing.

No capture schedules a commit itself. The focus change rides a render wake, which schedules one; a tab switch reorders the stacking, which does too; a close and the quit commit explicitly.

A capture overwrites a leaf only with what it read, never with nothing. An edit pane is read through `window_cursor`, and only while the core is `alive`, so a stalled Neovim costs no RPC timeout. A preview is read only once its page has loaded and no saved scroll waits to apply, or the capture would save an unscrolled page over the restore. A terminal pane saves nothing, and the launch restore seeds `paneViews` from the saved leaves, so a write before the first capture keeps them.

`window_cursor` asks Neovim rather than reading the snapshot's viewport mirror, which carries no column and lags Neovim by a flush that a quit right after a keystroke would lose.

**A close that drops its tab writes at once.** The closing view still counts then, so its directory's project state takes the capture. The drop's own commit, once the view is gone, leaves that row, since [absence never deletes a project state](../specs/application-state.md#which-tab-becomes-a-directorys-project-state).

**A saved scroll waits for the page.** `rebuildLayout` records each restored preview's fraction, which `previewDidFinishLoad` applies at the first finished load, ahead of a reload restore, which it drops. A queued fragment drops it instead, since a fragment jump is explicit navigation. The live source cursor is recorded as followed, so the follow coalescer does not scroll over the restore.

**A restored open drawer opens after the rebuild, and a zoom waits for Neovim.** The rebuild leaves Neovim on the zoomed leaf, else the focused one ([why](wolf-core-decisions.md#the-persisted-zoom-mark)). With an open drawer, the restore then calls `toggle_terminal()` and, for a focused pane, `focus_win`, which core's ordered input channel queues behind the open. The open's `DrawerOpen` intent unzooms, so the zoom and focus wait in `pendingDrawerRestore` until the drawer binds: a drawer zoom applies there, the open having left Neovim on the drawer, and a pane zoom once Neovim's current window is that pane.

That ordering holds because `wolf_toggle_terminal`'s split reports the drawer current before it emits `drawer_open`. Once the intent is drained, the active window is the drawer or the queued focus target, never an earlier pane, so no stale report unzooms the restore.

**The sidebar schedules its own writes.** A mode switch, a toggle click and Return wake no core, so `SidebarContainerView.onChange` schedules the snapshot. A toggle click runs no search. The saved query is the field's text, and a restore runs it only for a `search` sidebar with a non-empty query, while a `tree` sidebar keeps its query and toggles unrun. Expanded tree folders are not stored.

## Opening a folder

[§6](../specs/application-state.md#6-opening-a-folder) is the rule. File > Open Folder…, the empty state's button, File > Open Recent and the palette's Open Recent row all reach `AppDelegate.openFolder(_:from:)` with the key window's view, `nil` when no Wolf window is key. It canonicalizes the path and asks [`OpenFolderSeam`](../crates/wolf-macos/Sources/WolfKeyMap/Persistence/README.md#openfolderseamswift) for the step over every live tab's directory, grouped as the snapshot groups them.

A focus raises the view §4's rule picks and reads no project state. `raise()` deminiaturizes a minimized window first, since ordering it front alone leaves it in the Dock.

**A fill replaces the empty tab; it never chdirs it.** Core's cwd moves only when Neovim reports `DirChanged`, so a rebuild racing a chdir would start the drawer's shell and a terminal leaf whose saved directory is gone in the old directory. A snapshot between the chdir and the rebuild would also write a one-pane tab over the folder's project state. So a new tab on the folder, its Neovim started there, joins the empty tab's group with `addTabbedWindow(_:ordered: .above)` and becomes key, and only then does the empty window close.

The new tab opens first because closing the last window quits; when its core fails to start, the empty tab stays and nothing else happens. The close waits a runloop turn, since the dialog calls in from that window's own sheet completion. The stored frame, validated as at launch, applies only when the empty tab was its window's only tab, so a tab beside others keeps its group's frame.

Otherwise a new window opens holding the folder's tab, at the stored frame when it intersects a screen, else as [New windows](#new-windows) places it. A tab Open Folder opens is a folder tab whatever `StateSeam.launchDecision` would say of the folder, because opening a folder is an explicit act, as a typed `:cd /` is. Only a fill and a new window read the project state, which marks it used.

**A fill of New Tab's tab takes the sidebar its origin showed.** `makeNewTab(from:)` reads `key.sidebarVisible` at once, since the origin may close or change before a folder opens, and `fill(_:from:)` hands it to the folder's tab for `OpenFolderSeam.sidebarVisible`. It lives in memory only, so no other empty tab carries one. A typed `:cd` ignores it: `didOpenFolder()` reveals the sidebar.

Every Open Folder, a focus included, records the folder through `record_recent_dir` on the core of the tab that lands on it. That tab's Neovim starts in the folder, and core never records a launch directory.

The palette lists every recent directory. The invoking tab's own folder focuses that tab, which is harmless, and a Switch to Window row raises a window as it stands, while an Open Recent row for a folder open elsewhere focuses §4's tab, which can be a background tab.

## Opening files from outside Wolf

[The `wolf` command and opening files](../specs/cli-and-file-open.md) is the rule. Core resolves each request and chooses its pane; the coordinator routes it through [`FileOpenSeam`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#fileopenseamswift) and acts on the choice.

**The listener binds before the restore, and requests wait for its end.** `applicationDidFinishLaunching` binds the open channel before the launch restore, so a `wolf` that launched the app connects as soon as it can. `handleOpenRequests()` keeps each request in `openQueue` until the restore, or a fresh run's empty window, is in place, so a cold launch restores the session first ([§2](../specs/cli-and-file-open.md#when-wolf-is-not-running)). Quitting stops the listener first, so no `wolf` hands a request to an app that is quitting.

**The socket and Finder share one queue.** `application(_:open:)` resolves each URL as a request with no `WOLF_TAB_ID` ([§7](../specs/cli-and-file-open.md#7-finder-and-gnome-files)) and appends it to `openQueue`, so both sources take one path in arrival order. AppKit can deliver the URLs of a launch from Finder before `applicationDidFinishLaunching`, and the queue holds them through the restore.

**A file's project opens through Open Folder in full.** A file with no live named tab goes to `openFolder(_:from:)` on its project directory `P`, then into the tab that lands. When `P` is open, that is [Opening a folder](#opening-a-folder)'s focus by §4's rule, which is exactly [§5.1](../specs/cli-and-file-open.md#51-choosing-the-tab) step 3, and otherwise a fill or a new window. Like any Open Folder, it records `P` in the recent directories.

**A folder tab counts as on its launch directory until Neovim reports one.** `persistedCwd` falls back to `TabCore.launchCwd` through `OpenFolderSeam.liveDirectory`. Without it, two files of one new project opened together, as Finder does for a multi-selection, would open the project twice, because the first file's new tab has not reported its cwd when the second routes.

**A file sent to an empty tab opens there.** When `WOLF_TAB_ID` names an empty tab, whose drawer can run a shell, §5.1 rule 1 still holds. `WolfWindowController.openFile` hides the empty state so the file shows, and the tab stays empty, with no folder.

**A file waits for the view to be ready.** `PaneContainerView.openFile` queues the path until a first pane is bound and no layout rebuild, drawer restore or terminal restore is pending, since each would replace the pane the file lands in. An open placed before the rebuild chooses over the startup window, so it misses a restored pane already on the file. Its `:edit` is queued input, which the blocking rebuild runs ahead of, so it lands in whichever pane the rebuild focuses. A crash-restart reset keeps the queue.

**Rule 4's split binds its own pane.** `open_split_left` answers the new window and emits no `Split` intent ([why](wolf-core-decisions.md#opening-files-from-outside-wolf)), so the view binds it and `PaneTree.splitRootLeft` makes its leaf the root's first child. With the drawer open, Neovim's own layout then differs from the view's, as it does for every grid the view sizes, which is harmless because the view owns the geometry.

**The command ships at `Contents/Resources/bin/wolf`, never `Contents/MacOS/wolf`.** A Mac's file system is case-insensitive by default, so `MacOS/wolf` collides with the app's executable `MacOS/Wolf`. `Resources/` already carries executables, `nvim/bin/nvim` and `rg/rg`, under the ad-hoc signature.

**Install and Uninstall touch only Wolf's link.** [`CommandInstallSeam`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#commandinstallseamswift) changes `/usr/local/bin/wolf` only when nothing is there or it is a symbolic link whose destination ends in `/Contents/Resources/bin/wolf`, some `Wolf.app`'s binary. Anything else is refused with an alert and left alone, because `/usr/local/bin` is Homebrew's on Intel Macs. A permission failure, or a missing `/usr/local/bin`, re-runs the step as an AppleScript `do shell script … with administrator privileges`, and Uninstall asks for the same rights, since `/usr/local/bin` is usually root-owned. Each step reads the link again afterwards, so an alert never reports a change that did not happen.

## New windows

[§7](../specs/application-state.md#7-new-windows-and-tabs) is the rule. New Window, and an Open Folder new window with no stored frame that intersects a screen, take the active window's normal size and cascade from it, through `AppDelegate.newWindowPlacement(stored:)`. With no window open, a new window keeps the default size and AppKit's placement. New Tab's empty tab is tabbed into the key window's group instead.

**The active window is the stacking order's first, never the key window**, as §2 defines it. [`StateSeam.newWindowFrame`](../crates/wolf-macos/Sources/WolfKeyMap/Persistence/README.md#stateseamswift) takes its size through `normalFrame`, so a zoomed or full-screen window lends its earlier size, and starts the cascade from the top-left of its live frame, so the new window cascades from where that window is on screen.

**The cascade moves only the new window.** `NSWindow.cascade(fromTopLeft:size:)` calls the new window's own `cascadeTopLeft(from:)` twice, the first putting its top-left on the active window's and the second offsetting it. The active window travels as a point, so it is never moved or constrained, and the controller retains no other window.

**A placed window opens standalone.** Under the default "Prefer tabs: In Full Screen" setting, AppKit makes a new window a tab of a full-screen window, which opens it full screen. So `build()` disallows automatic tabbing around the first order-front of any window it places, and restores `.automatic` after, so only that first order-front is affected.

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

The control's selected segment is derived in one place, [`SidebarToolbarSeam`](../crates/wolf-macos/Sources/WolfKeyMap/Sidebar/README.md#sidebartoolbarseamswift), from the sidebar's visibility and mode. `MainWindow` applies it whenever the split item's `isCollapsed` changes and at the end of `showSidebar(panel:)` and `restoreSidebar(_:)`. No path that changes the sidebar sets it: not the commands and their shortcuts, the View menu, a divider drag, the launch restore, nor the control's own click. Set from each path, the control goes stale after any path that forgets, a later one included; observing the state misses none.

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

## A preview's content width

A preview's `WKWebView` spans its pane below the statusbar strip, and the page caps its own content. A view framed to the capped grid rect puts the page's scrollbar beside the content and leaves the margins and padding gutters to the container, dead to the wheel. The page instead carries an inline style on its root element, [`PreviewWidthSeam`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#previewwidthseamswift)'s: the pane padding, plus a `max-width` of the column cap centred by auto side margins unless the source is full width, so the content lands where an editor's capped grid paints.

The style goes on `<html>`, never `<body>`, because `base.css` owns the body's box: its zero padding is what the first-block margin rules and the baseline grid rest on. It reaches the page two ways. Every full load carries it in the document's `<html>` tag, so a load never paints uncapped, and a script sets it on each change and once more after each finished load, which lands a change made while that load was in flight. Core's render is untouched, so the head stays byte-identical and the body swap above stays sound.

On a side or the bottom that borders a divider, the view stops `dividerHitTolerance` short of the pane's edge. The container strokes its dividers beneath its subviews and takes the divider drag within that tolerance, so a view reaching the edge would hide the line and swallow the press that drags it, and two side-by-side previews could not be resized. The side inset applies to both sides together, and the page's side padding shrinks by as much, so the auto-margin centring still lands on the editor's capped grid rect.

The statusbar strip stays chrome rather than a margin: the view starts below it, so a wheel or press there never reaches the page. Two consequences are accepted. Content wider than the cap, such as a wide table, spills into the right margin instead of scrolling inside the cap, and scrolled content passes under the top padding gutter up to the strip.

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

Close Pane closes the drawer although the drawer is outside `tree`, and refuses the tree's last leaf even beside the drawer, because [`wolf_do_close`](../runtime/nvim/README.md#wolf_setup_layoutchan) refuses the tab's last pane, the drawer not counting, and the menu item must be disabled where the close does nothing. `closeActivePane()` sends `focus_win` for the active pane's window before `close_pane()`, both on core's ordered input channel, so the close acts on the pane `canClosePane` read even while a focus move is in flight. `WolfWindowController.closePane()` goes through it, so a dispatched Close Pane is refused wherever the menu item is disabled.

A file the picker, the sidebar or the Open dialog opens goes through `openPickedFile`, which routes by the active pane. A preview pane previews it, falling back to an edit when `preview_file` answers `0`. With the drawer active it goes to `openFile`, which places it by [§5.2](../specs/cli-and-file-open.md#52-choosing-the-pane), because `open` edits in Neovim's current window, the drawer's ([The drawer](../specs/terminal-panes.md#the-drawer)).

`splitActivePane(_:)` sends `focus_win` before its split for the same reason, so Split Horizontal and Split Vertical split the pane this view shows as active.

After a crash-restart, `TabCore.respawnNeovim` captures the tab's terminal-pane sessions and drawer state before `handle.restart()`, and hands the view its [`terminalRestore`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#livenessseamswift) steps. The view runs them once, from a render wake after `ensureInitialPane` has bound its first pane and no layout rebuild waits ([why](crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift.md#terminal-restore)). `resetAfterRestart()` clears a request made before it.

The drawer's top edge is no `PaneTree` divider, so the container hit-tests it at `DrawerLayout.editorHeight` itself, with the tree dividers' tolerance and cursor. A drag sets the share the drawer takes, which a `nil` from `draggedWeight` leaves as it is.

That share is the container's, held outside `tree`, so it never reaches `paneLayout`, and a new view starts at `DrawerLayout.defaultWeight`. The window state saves it as the tab's drawer, through `drawerShare`, and a launch restore applies it through `DrawerLayout.restoredWeight`, which takes `defaultWeight` for a share that is not finite and strictly between 0 and 1, since a non-finite weight collapses both heights to zero. Closing the drawer and a Neovim restart keep it: `resetAfterRestart()` leaves it alone, because the share belongs to the window, not to Neovim. A drawer drag ends with `onLayoutChanged`, as a divider drag does, so the new share is written.

## Zoom Pane

[The intents contract](../specs/architecture.md#output-intents-core--front-end) owns what Zoom Pane does. These are the macOS choices behind it. Every rule is [`PaneZoom`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#panezoomswift)'s, so `PaneContainerView` holds one value, `zoomedPane`, and the panes the toggle considers are the tree's leaves plus the drawer, so the drawer can be zoomed.

Zoom is front-end geometry, and Neovim is never told. `:only` would close the hidden windows, and `<C-w>_`, `<C-w>|` or `:resize` would size Neovim's frames, which are not the pane geometry: the front-end sizes each grid with `ResizeGrid`. The view never touches `tree`, its weights or `drawerWeight`, so an unzoom restores them exactly, and `paneLayout`, `leafContents()` and `capturedTerminals()` read the unzoomed tree that the window state records. `toggleZoom()` fires `onLayoutChanged`, because it is the one zoom change made outside a render wake, and every wake already schedules a snapshot. A new view starts unzoomed.

A hidden pane keeps its last grid and session size. `paneRects()` returns the zoomed pane alone, and the grid and session sizing read it, so nothing is sent for a hidden pane; that rests on Neovim re-dividing only the grids the front-end has not sized, so a sized hidden grid holds with no request. Unzooming then needs no reflow: no hidden buffer rewraps and no hidden shell is resized twice. After a window resize while zoomed, the hidden panes are sized once, on unzoom, to the restored layout.

Hidden terminal views and preview web views are kept with `isHidden` set, never removed. A removed web view reloads and loses its scroll state on unzoom, and one left shown paints over the zoomed pane, since a subview draws above the container's own paint. A hidden preview keeps its maps and its reloads, as a shown one does.

The zoomed pane closing unzooms whatever closed it, Toggle Terminal on a zoomed drawer included, since the check runs right after `detectCloses()`. A `Split` intent the view applies unzooms first, whatever made it, a terminal split included, and so does a `DrawerOpen`, since a pane joining the layout must show. A hidden pane closing keeps the zoom, its siblings renormalizing as they do unzoomed, and so do a window resize and a tab switch.

A focus move unzooms through one event, `.focusMove(target:)`. `applyFocus` starts from `PaneZoom.focusOrigin`, the zoomed pane while zoomed, and resolves through `DrawerFocusSeam.focusTarget` in the unzoomed layout, so `<C-w>h/j/k/l`, the router's chords from a terminal and both drawer bridges reach the pane they would unzoomed. A move with no target there keeps the zoom. The tree dividers and the drawer's top edge stay inert, with neither hit tests nor cursor rects, since they are hidden; the other-pane commands get `0` from a zoomed tree leaf, so they take their split path and unzoom, and `nil` from a zoomed drawer, as unzoomed.

The view trusts Neovim's current window. `updateActivePane()` applies `.focusMove` to the pane it resolves to, so leaving the zoomed pane by a path the view does not intercept, such as `<C-w>w`, `<C-w>p` or `:wincmd`, unzooms and the view follows. There is deliberately no guard for focus still in flight: only a zoom made within one Neovim round trip of a click reads a stale window, which unzooms harmlessly and which no human input reaches. Zooming sends `focus_win` for the zoomed pane's window, as `closeActivePane()` does, so Neovim's current window is the pane the view shows.

Toggle Terminal applies `.toggleTerminal(drawer:)` and then always calls `toggle_terminal()`, whose helper opens, focuses or closes the drawer by Neovim's current window, which `updateActivePane()` follows; the view adds no check of its own. It unzooms first unless the drawer is the zoomed pane, whose zoom lasts until `.panesChanged` sees the drawer close: unzooming on the toggle would show the hidden panes for a round trip before the close.

The save path marks the zoomed leaf through `zoomedPaneId`, `SessionSeam.gatherTab` and `PaneLayoutCodec.encode`, and `WindowStateCodec` writes the mark as the tab's `zoomed` pane, by the leaf's depth-first index. The drawer is no layout leaf, so its zoom persists as `SavedDrawer.zoomed`. A zoom on a pane `restored()` empties is dropped before the rebuild.

With no open drawer to restore, `rebuildLayout` re-zooms at once, applying `.restore` to the leaf at `zoomedLeafIndex` after a successful zip and making it active; with one, the zoom waits for the drawer ([why](#the-per-tab-extras)). The handle-count fallback returns first, so it restores unzoomed, and `.restore` refuses a lone pane, which has nothing to hide. The restore is safe under the trust above only because the rebuild leaves Neovim's current window on that leaf ([why](wolf-core-decisions.md#the-persisted-zoom-mark)).

A crash-restart unzooms, because it rebuilds no saved leaf: `resetAfterRestart()` drops the tree and the zoom, each live split session comes back as a `Split` intent, and the drawer through `toggle_terminal()`'s `DrawerOpen`, both of which unzoom. A launch rebuild still pending at the restart is the exception: `pendingLayoutRebuild` survives the reset, so that rebuild re-zooms its leaf as a launch does.

## Nerd Font icons

Every grid face, in the Neovim grid and the terminal panes alike, falls back to the bundled Symbols Nerd Font Mono for a glyph it lacks, so the Private Use Area icons that prompts, `ls` replacements and Neovim plugins print render under any configured family, as in Ghostty. The cascade list is built in [`CellMetrics`](../crates/wolf-macos/Sources/WolfMacOS/README.md#cellmetricsswift).

The symbols face sits behind the configured face, not ahead of it, so a family that carries its own icons keeps them: a patched Nerd Font the user configured, or JetBrains Mono's Powerline glyphs. It sits ahead of CoreText's system list because the system fallback does not reach the registered face for these code points, which `SymbolsFallbackTests` checks as a premise. The system list follows it, so emoji and other scripts still fall back to the system's faces.

The Mono face is the one cascaded because it is drawn unscaled, and every Mono glyph advances one em, where the proportional face's advances vary and run wider ([measurements](../bazel/nerd-fonts-symbols/README.md#why-the-mono-face)). One em is still wider than a text cell, which is why both painters fill a row's backgrounds before its glyphs ([`GlyphPainter`](../crates/wolf-macos/Sources/WolfMacOS/Terminal/README.md#glyphpainterswift)).

## Present limitations

- Only windows opened via the overridden split commands (`<C-w>s`/`v`, `:split`/`:vsplit`, Split Horizontal and Split Vertical) become panes. Help, quickfix, preview, and `:diffsplit` windows do not, and floating or external windows are ignored.
- Content is capped at a hard-coded 90 columns and centered in a wider pane, with an editor's margins painted in the grid's own background and a preview's in the page's. Making the cap configurable is a possible enhancement.
- Scrollbar thumbs are non-interactive; thumb-drag is deferred.
- Markdown preview ships one theme. Cursor-follow anchors to the containing block's top inside a fenced code block, an HTML block or a table, and inside a paragraph hard-wrapped across several short source lines; an image's alt text collapses to a single anchor just after the image. The marker rules are in [the preview position-marker contract](wolf-core-decisions.md).
- A symbols-face icon draws unscaled, about one em wide, over part of the cell after it, rather than fitted to its own cell as Ghostty fits it.
- In a terminal pane, the cursor and blinking text draw steady, and double, dotted and dashed underlines draw as the single straight line.
- Appearance flows one way only: colorscheme to app chrome works, OS theme to Neovim theming does not.
- The empty state's "no folder open" view (`EmptyStateView`) draws in the window background colour, where `wolf-gnome`'s takes its tab's `Normal` background ([Theming](../specs/architecture.md#theming)), so the colorscheme does not reach it.
- `gtk-titlebar-style` is a configured key this front-end ignores, as the GNOME front-end ignores `macos-option-as-alt`: one config file is shared across both, and the key's name says which platform it is for.
- A Neovim crash restart reopens the tab's open files into one pane, so its splits collapse, and the next commit records that. Restoring the pane topology across a restart would change the restart model.
- Only what a pane shows persists: the window state carries buffer paths only as panes, so a file the user switched away from in its pane is not reopened at launch.
- The toolbar's search segment seeds the search toggles from Neovim each time it shows the search panel, replacing restored or user-set toggles, while the sidebar commands do not seed. Seeding once would save a never-seeded panel's all-off toggles as if chosen, which needs a design of its own.
- A fill discards an unsaved buffer typed into the empty tab, as closing that tab would.
- A file opened into an empty tab is not restored at launch: the tab has no folder, so it persists as an empty tab.
- A capture runs only at a focus move, a tab switch, a window close and quit, so a crash loses the cursor and scroll movement since the last one, and a crash restart drops every capture with its panes.
- Per-pane navigation history is not persisted across launches, and is not cursor-position history — Neovim's own jumplist (`<C-o>` / `<C-i>`) is untouched. Navigating back to a file deleted from disk opens an empty `[New]` buffer, exactly as `:e missing.md` would.

## Frame-time instrumentation

The crate carries an off-by-default frame-time measurement surface for finding where main-thread time goes on the keystroke-to-pixel path. Recording a sample never changes what is painted.

`WOLF_FRAME_STATS=1` enables it, `WOLF_FRAME_STATS_INTERVAL` sets the report period, and `WOLF_FRAME_STATS_FILE` also appends each report to a file. With the flag unset the cost at each call site is one `Bool` test. See [Building Wolf](../build.md) for the terminal invocation — a double-clicked `.app` inherits `launchd`'s stderr and shows nothing.

One caveat when reading a report: `wakes/key` is a counter ratio, so it is an upper bound rather than real attribution. The per-bucket caveats, including what `prev.html` does and does not time, are in [`Sources/WolfKeyMap/README.md`](../crates/wolf-macos/Sources/WolfKeyMap/README.md).
