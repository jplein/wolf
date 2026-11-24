# wolf-macos/Sources/WolfMacOS

The AppKit application: windows, views, menus, and the coordinator that owns the tabs, each with its own captive Neovim. It holds only what genuinely needs a window; the decisions it acts on live in the pure [`WolfKeyMap/`](../WolfKeyMap/README.md) seam.

What a view here draws is testable offscreen through [`WolfMacOSTests`](../../Tests/WolfMacOSTests/README.md); event handling, focus and menus have no automated coverage. See [wolf-macos-decisions.md](../../../../notes/wolf-macos-decisions.md) for that posture and for the per-tab Neovim, threading, and persistence models these files implement.

## Subdirectories

Each compiles into this module and has its own README with the entries for its files.

- [`Terminal/`](Terminal/README.md) — the terminal pane view and the glyph painter.
- [`Preview/`](Preview/README.md) — a Markdown preview web view's delegate and script-message handlers.

## Files

#### `AppDelegate.swift`

Purpose: The application coordinator: launch, the live `WolfWindowController`s, their restore and persistence, tab focus and dynamic menus.

**Public API**

- `makeNewWindow(originatingCwd:)` — an empty-state tab in a window [sized and cascaded](../../../../notes/wolf-macos-decisions.md#new-windows) from the active one.
- `makeNewTab(from:)` — New Window's tab in `key`'s group, carrying its sidebar visibility ([why](../../../../notes/wolf-macos-decisions.md#opening-a-folder)).
- `refreshTabFocus()` — sends each [`TabFocusSeam`](../WolfKeyMap/README.md#tabfocusseamswift) edge to its core and focuses the main window's tab in the `StackingOrder`, capturing the tab it leaves ([capture](../../../../notes/wolf-macos-decisions.md#the-per-tab-extras)).
- `windowMinimizedChanged(_:minimized:)` — marks or unmarks the controller's tab group.
- `scheduleStateSnapshot()` / `snapshotNow()` — throttled and immediate whole-tree commits ([write model](../../../../notes/wolf-macos-decisions.md#state-is-a-tree-and-the-front-end-owns-every-write)).
- `closeCommit(for:)` — a closing view's commit through [`SessionSeam.CloseBurst`](../WolfKeyMap/Persistence/README.md#sessionseamswift), after capturing its group.
- `otherWindows(excluding:)`, `raiseWindow(tabId:)` — Switch to Window's rows and raise.
- `openFolder(_:from:)` — [Open Folder](../../../../notes/wolf-macos-decisions.md#opening-a-folder), answering the landed view.
- `handleOpenRequests()` — handles the queued socket and Finder requests once launched ([why](../../../../notes/wolf-macos-decisions.md#opening-files-from-outside-wolf)).
- `appAppearance(for:)` — a mode's forced `NSApp.appearance`; `nil` under `auto`.

**Decisions**

- Launch order is fonts, config, metrics: CoreText needs the bundled families, and measuring reads the config.
- `appearanceObservation` must stay stored: a deallocated observation invalidates itself, ending auto-mode tracking.
- Tab focus and the stacking order follow main ([why](../../../../notes/wolf-macos-decisions.md#one-neovim-per-tab)). Its loss rides `didResignActive`, where `NSApp.isActive` reads `false`.
- A directory request raises its landed view: a fill does not activate the app.
- The About panel's credits are in `labelColor`: uncoloured, they render black in dark mode.

**Gotchas**

- Owning-controller lookups use `effectiveKeyWindow`, which resolves a key picker panel to its parent: no controller owns the panel.

#### `AppMenu.swift`

Purpose: Builds and installs `NSApp.mainMenu` (App, File, Edit, View, History, Window). Each Wolf command item carries the FFI `command_kind` discriminant in its `NSMenuItem.tag` and fires `AppDelegate.handleCommandMenuItem(_:)`, which funnels it through `CommandDispatcher`.

**Public API**

- `install(target:) -> InstalledMenus` — returns the three submenus with dynamic contents (Window, Open Recent, History) so the coordinator can rebuild them.
- `rebuildWindowMenu(_:controllers:target:)`, `rebuildOpenRecentMenu(_:recentDirs:target:)`, `rebuildHistoryMenu(_:rows:target:)` — full rebuilds; each re-seeds its disabled placeholder when its list is empty.

**Decisions**

- The menu description is hardcoded here rather than read from the core registry over FFI; only the `command_kind` discriminant crosses the bridge. Item titles and their single U+2026 ellipses mirror `wolf_core::COMMAND_SPECS`, so a title change belongs in both places.
- Toggle Sidebar and Enter Full Screen are standard AppKit items with a `nil` target, routed up the responder chain to the split view controller and the key window. AppKit owns their validation and no `wolf_core::Command` variant backs them.
- A History row's `tag` is a stack index, not a `command_kind`. The distinct `historyJumpMenuItem(_:)` selector is what keeps the two tag namespaces unambiguous.

**Gotchas**

- About Wolf targets the coordinator's `showAboutPanel(_:)` rather than going nil-targeted to `NSApplication`, whose stock action shows no credits.
- The front-end-only items (History, Project, Search, Install and Uninstall `wolf` Command) target the coordinator directly instead of carrying a tag, so `CommandKind.toggleSidebar`, `.sidebarShowProject` and `.searchInProject` never reach `CommandDispatcher` from a menu item. They still reach it from the nvim-drained path and from the command palette, whose static rows dispatch through it.

#### `BufferPickerView.swift`

Purpose: The floating buffer picker — a search field plus a results list backed by an `FfiBufferPicker` session, hosted in a picker panel. `BufferPickerRow` carries the chosen buffer's label, handle and modified flag out through `onActivate`, which the controller uses to switch the active pane.

**Public API**

- `open()` / `close()` — idempotent. `open()` snapshots a fresh session from the core's listed buffers.
- `refresh()`, `moveSelection(delta:)`, `activateSelection()` — activation calls `onActivate` and then closes; with zero rows it does nothing.
- `host` — set by `WolfWindowController` after construction. Weak, because the host owns this view as its panel's `contentView`.

**Decisions**

- The initial empty-query open preselects row 2, so a bare Return switches to the most-recently-used other buffer (telescope-style, like `Ctrl-^`). Later refreshes follow the shared `PickerSeam.restoredIndex` rule.
- Activation records no MRU; ordering comes from Neovim's buffer list.
- Candidates are fixed at open, so there is no wake bridge — `ranked()` re-filters synchronously per keystroke.
- `close()` clears `isOpen` before dismissing the panel, because `orderOut` re-enters through `windowDidResignKey`; the guard makes that re-entry a no-op.
- The panel background and the search bar are `PickerChrome` backdrops; the border alone stays a `CGColor`, which ignores appearance changes, so `viewDidChangeEffectiveAppearance()` re-resolves it inside `performAsCurrentDrawingAppearance`.

#### `CellMetrics.swift`

Purpose: The monospace cell geometry (advance width, cell height, baseline) plus the four cached CoreText faces used to paint the grid, measured from the resolved user config. `baseline` is measured from the cell's top edge, since the grid view is flipped. Each face falls back to [`FontRegistration.symbolsFamily`](#fontregistrationswift) for a glyph it lacks ([why](../../../../notes/wolf-macos-decisions.md#nerd-font-icons)).

**Public API**

- `measured(config:)` / `measured(config:pointSize:)` — the default size, and an explicit size for the per-window font-size steps. Both forward to `measured(family:pointSize:lineHeightPercent:symbolsFamily:)`, where a test names a missing `symbolsFamily`.
- `defaultPointSize(config:)` — `WOLF_FONT_PT` when set to a positive number, else `config.font_size`. Also what a window's font-size reset targets, so both read one resolution.
- `font(for:)` — the cached face for a `GlyphFace`.

**Decisions**

- The cell width, height and baseline come from the regular face before its cascade, so every face paints into the same fixed-cell slot and a fallback glyph never moves a row.
- The cascade is the symbols face, then the face's `CTFontCopyDefaultCascadeListForLanguages`, appended because a custom list may replace the system fallback rather than precede it.
- Each finished face is cascaded separately: nothing documents whether `CTFontCreateCopyWithSymbolicTraits` keeps the attribute.
- A symbols family that does not resolve leaves the faces uncascaded, logged.
- A derived bold or italic variant is accepted only when `CTFontGetSymbolicTraits` confirms its traits: a family with no such face yields an unchanged copy. The fallback is the regular face, so the glyph still paints.
- An unresolvable configured family falls back to Menlo, compared case-insensitively.

#### `CmdlineView.swift`

Purpose: The native command-line overlay (`ext_cmdline`). It paints one row — `prompt + firstc + text` with a caret at `pos` — pinned to the bottom of the editor area, shown only while a command line is active rather than reserving a permanent band. The `prompt` is painted inline for `confirm()` and `input()`-style prompts and is empty for a plain `:` or `/` line.

**Public API**

- `update(_:defaultFg:defaultBg:) -> Bool` — projects an `FfiCmdline` in and returns whether the command line is active, so the container can lay the overlay out or hide it.
- `overlayFont(metrics:)` — a free function, not a member: an `NSFont` matching the grid's `CellMetrics`, shared with the completion-popup and toast overlays.

**Decisions**

- `pos` is a byte offset, so the caret x is measured from `CmdlineSeam.caretPrefix` rather than from a Swift character or UTF-16 index.

#### `Color.swift`

Purpose: Converts the bridge's packed `FfiColor` (`0x00RRGGBB` plus a `set` flag) into a `CGColor`, applying the white and black fallbacks when unset.

**Decisions**

- Colors are built in an explicit sRGB color space. The convenience `CGColor(red:green:blue:alpha:)` initializer builds `kCGColorSpaceGenericRGB` instead, whose different gamma renders the same packed value visibly lighter, so the grid background would not match the sRGB titlebar.

#### `CommandDispatcher.swift`

Purpose: Maps a decoded `CommandKind` to its native AppKit action. Both the menu path (`AppDelegate.handleCommandMenuItem`) and the nvim-drained path (`WolfWindowController`) route here, so a command has one implementation regardless of how it was invoked.

**Public API**

- `dispatch(_:window:coordinator:originatingCwd:controller:)` — `window` and `controller` are the command's target: the key window for the menu path, the originating controller for the nvim-drained path, so a command issued from window A acts on window A. `originatingCwd` is consumed by `.newWindow` alone; `nil` for either target means the command quietly does nothing.

**Decisions**

- The switch carries no `default` case, so adding a `CommandKind` variant fails to compile until it is handled here.

#### `CommandPaletteView.swift`

Purpose: The floating command palette — a search field plus a results list backed by an `FfiCommandPalette` session, hosted in a picker panel. `CommandPaletteRow` carries the label plus the payload for one of three actions: dispatch a static command, raise a window, or Open Folder on a recent folder.

**Public API**

- `open(otherWindows:)` — idempotent. `otherWindows` must exclude this window, which cannot switch to itself.
- `close()`, `refresh()`, `moveSelection(delta:)`, `activateSelection()` — activation records the row's MRU key, hands the row to `onActivate`, then closes.
- `host` — set by `WolfWindowController` after construction. Weak, because the host owns this view.

**Decisions**

- Candidates are fixed at open (recent dirs plus the added windows), so there is no wake bridge — `ranked()` re-filters synchronously per keystroke.
- A row whose `row_kind` discriminant does not decode is skipped rather than trapped.

**Gotchas**

- Panel hosting, close re-entrancy and appearance-driven chrome re-resolution behave as in `BufferPickerView.swift`.

#### `CompletionPopupView.swift`

Purpose: The `ext_popupmenu` completion overlay — an `NSView` that lists the completion items vertically and highlights the selected one. A projection of the core's `Snapshot.pum`, read across the FFI as an `FfiPum`.

**Public API**

- `update(_:) -> Bool` — projects the popup state and returns whether it is visible; the container lays the overlay out only when it is.
- `leadingTextInset` — the inset from the popup's left edge to where row text is drawn. `PaneContainerView` subtracts it from the cmdline byte anchor so the item text column lines up with the completed word in the command line rather than with the popup border.

**Decisions**

- A list longer than `maxVisibleRows` scrolls to keep the selection visible instead of growing or gaining a scrollbar.
- Each row shows the item's `word` with `kind` as a trailing hint; `menu` and `info` are ignored.

**Gotchas**

- The view does not place itself. `PaneContainerView` positions it directly above `CmdlineView`, anchored at the cmdline byte column.

#### `Cursor.swift`

Purpose: Pure cursor-rectangle geometry. `CursorShape` mirrors `wolf_core::CursorShape` as packed into `FfiFrame.cursor_shape()`.

**Public API**

- `rect(shape:cellX:cellY:cellW:cellH:cellPercentage:)` — a `cellPercentage` of 0 means Neovim specified none, and `defaultCellFraction` applies.

**Decisions**

- Only geometry lives here. The reverse-video fill and glyph colors are resolved in Rust (`FfiFrame.cursor_fill()` / `cursor_glyph()`); do not recompute them in Swift.

#### `EditorSplitViewController.swift`

Purpose: The window's `contentViewController` — an `NSSplitViewController` with a leading `.sidebar` item wrapping the `SidebarContainerView` and a trailing content item wrapping the `PaneContainerView`. `HostViewController`, in the same file, hosts each pre-built view inside the split.

**Public API**

- `sidebarItem` — the leading split item, exposed so state restore can set, and state persistence can observe, `isCollapsed`.
- `pinDivider()` — pins the sidebar to `MainWindow.treePaneWidth`; `MainWindow` calls it after setting the content size.
- `HostViewController(hostedView:pinTopToSafeArea:)` — `false` adopts the hosted view as the controller's own `view`; `true` wraps it in a container.

**Decisions**

- The sidebar item is built with `NSSplitViewItem(sidebarWithViewController:)`, which supplies vibrancy, full height under the transparent titlebar, collapsibility, and a holding priority above the content item's. Leaving that priority alone is what makes window resizing absorb into the editor pane; lowering it inverts the behavior.
- Content-mode hosting pins the hosted view's top to the container's `safeAreaLayoutGuide.topAnchor`, so the transparent titlebar never covers the editor grid. Sidebar-mode hosting deliberately skips this — its `NSScrollView` insets its own content under the titlebar, and a container would break the `.sidebar` material filling the pane.

#### `EmptyStateView.swift`

Purpose: The "no folder open" overlay — a full-bleed subview of `PaneContainerView` holding one centered "Open folder…" button. `WolfWindowController` toggles it with `isHidden` and hides it on a typed `:cd`; Open Folder replaces the whole tab instead.

**Public API**

- `onOpenFolder: (() -> Void)?` — invoked by the button; the controller wires it to the same Open Folder dialog as the menu command.

**Decisions**

- The view never becomes first responder, so keystrokes typed at launch reach the editor grid instead of the button.
- The layer background is re-resolved inside `performAsCurrentDrawingAppearance` on every effective-appearance change. Layer `CGColor`s do not adapt to a dark/light switch, and a bare `NSColor.windowBackgroundColor.cgColor` resolves against whatever appearance is globally current, which is often the stale one mid-change. The centered `NSButton` needs no such handling; AppKit re-resolves its dynamic colors itself.

**Gotchas**

- It constrains itself to its superview in `viewDidMoveToSuperview`, so it must be added as a subview of the view it is meant to fill.

#### `FileTreeView.swift`

Purpose: The file-tree sidebar: an `NSOutlineView` driven by a long-lived `FfiFileSearch`, rooted at a synthetic cwd node.

**Public API**

- `start()` / `close()` — open and drop the tree-search session.
- `refresh()` is immediate; `scheduleRefresh()` coalesces traversal batches to about one reload per frame.
- `onRenderWake()` — re-roots on a cwd change, then accents and reveals what the active pane shows.
- `setSidebarVisible(_:)` — relays the split item's `isCollapsed` KVO; refresh defers while collapsed.

**Decisions**

- `NSOutlineView` keys expansion and selection on object identity, so one cached `TreeItem` per `relPath` is reused across reloads; fresh instances collapse the tree.
- The outline draws clear, with no `NSVisualEffectView`: the `.sidebar` split item supplies the vibrancy.
- `reveal(abs:)` expands ancestors top-down first, since a row exists only under an expanded parent; an unstreamed file leaves the reveal pending.
- `reroot(to:)` first cancels the pending coalesced refresh, whose stale fire would hit the discarded tree.
- `onRenderWake()`, `reroot(to:)`'s only caller, re-roots when `handle.cwd()` differs from its root: Neovim's cwd is the one authority. wolf-gnome's settle-point re-root ([`shell.rs`](../../../wolf-gnome/src/README.md#shellrs)) is an accepted divergence.
- `TreeCellView` marks what the active pane shows with a [`SidebarRowStyle`](../WolfKeyMap/Sidebar/README.md#sidebarrowstyleswift) label color, never the selection tint.
- `applyLabelColor()` re-sets the label's string, an apparent no-op: after an emphasized click AppKit ignores `textColor`, leaving the formerly current row accented. `WolfMacOSTests` fails without it.
- A file click opens through [`PaneContainerView.openPickedFile`](#panecontainerviewswift), as the picker, the search results and the Open dialog do.

#### `FontRegistration.swift`

Purpose: Registers the TTFs bundled at `Contents/Resources/fonts/` with CoreText so the vendored families resolve — "iA Writer Mono S" (the default grid font), "JetBrains Mono", and "Symbols Nerd Font Mono", the Private Use Area icon face, which is never a configured family and which [`CellMetrics`](#cellmetricsswift) cascades behind every grid face. `AppDelegate` calls `registerBundledFonts()` at launch, before the config is loaded and metrics are measured.

**Public API**

- `symbolsFamily` — the icon face's family name.
- `registerFonts(in:)` — registers the TTFs directly under any directory, as `registerBundledFonts()` does the bundle's. The test binary registers its staged font trees through it.

**Decisions**

- Registration is process-scoped, not a system-wide font install.
- Failures log and continue. A missing `fonts/` directory (the developer and `swift run` paths, which have no bundle) and an already-registered face are both expected, and neither may be fatal.

#### `FrameMetrics.swift`

Purpose: Process-global glue around the pure `WolfKeyMap.FrameStats` seam: the once-only `enabled` evaluation, the clock reads, the report `Timer`, and the writes to stderr and `WOLF_FRAME_STATS_FILE`. Development instrumentation, off unless `WOLF_FRAME_STATS` is set, main thread only.

**Public API**

- `begin() -> UInt64` / `end(_:_:)` — bracket an instrumented region, ideally through `defer` so early returns are still measured. `begin()` returns 0 when disabled, and the elapsed subtraction saturates.
- `start()` / `emit()` — `AppDelegate` arms the reporter at launch and calls `emit()` at termination to flush the final partial interval.

**Decisions**

- `enabled` is a `static let`, evaluated once via `swift_once`; `start()` touches it at launch so that one `ProcessInfo` read cannot land inside a measured frame.
- `emit()` checks `recordedAnything` before summarizing, because `summarize()` allocates and sorts — an idle app must not pay that once per interval to discover it has nothing to say.
- Writes use the throwing `write(contentsOf:)` and discard the error. The non-throwing `write(_:)` raises an uncatchable `NSException` on `EPIPE`, which would kill the app as soon as its output were piped into a short-lived reader such as `head`.

**Gotchas**

- Measure an optimized release build, from a terminal: debug numbers are meaningless, and a double-clicked bundle inherits launchd's stderr. Recipe in `build.md`.

#### `KeyComposer.swift`

Purpose: The macOS dead-key and Option-character composition glue for option-as-option mode — a wrapper over Carbon's `UCKeyTranslate` that threads a persisted dead-key state across keystrokes, so Option-u then u composes to "ü" while a plain key with nothing pending composes to itself.

**Public API**

- `compose(keyCode:shift:option:capsLock:) -> String` — returns "" while a dead key is still pending, which the caller treats as "consume the event, send nothing". `capsLock` maps to Carbon's `alphaLock` bit so letters uppercase with Caps Lock on.
- `reset()` — clears pending state. `PaneContainerView` calls it on focus loss so a later keystroke cannot commit a stale accent.

**Decisions**

- `keyTranslateOptions` is 0 rather than the no-dead-keys bit: that bit would stop dead keys being processed at all, which is the entire feature.
- Every failure path (no layout data, a null `CFDataGetBytePtr` from empty `CFData`, a non-`noErr` status) returns "" rather than trapping.

**Gotchas**

- Whether a key routes here, and how a composed string becomes `FfiKeyEvent`s, is decided by the headlessly tested `WolfKeyMap.KeyComposition`. This file reads the live TSM keyboard layout and cannot be unit-tested; its contract is covered end to end by the `wolf-ffi` `unicode_key` real-nvim test.

#### `MainWindow.swift`

Purpose: The single editor window: an `EditorSplitViewController` under a transparent full-size-content titlebar and a unified `NSToolbar`, whose delegate it is.

**Public API**

- `protocol HistoryNavigating` — Back and Forward, delegated to `WolfWindowController` ([why](#wolfwindowcontrollerswift)).
- `init(sidebar:paneContainer:metrics:cols:rows:)` — sizes the content so the editor pane keeps exactly `cols` × `rows` cells below the chrome.
- `showSidebar(panel:)`, `toggleSidebar()` and `sidebarPanel`, which `WolfWindowController` forwards to; `sidebarPanelsItemIdentifier` names the sidebar control's toolbar item.
- `savedSidebar`, `restoreSidebar(_:)` — the window state's sidebar mode, query and toggles.
- `applyTitlebarBackground(_:)` — idempotent; `.system` restores the pristine `backgroundColor` the first `.colour` captured.

**Decisions**

- `isReleasedWhenClosed = false`: the controller holds the window, and the default's unbalanced release crashes the deferred close-animation dealloc.
- The toolbar is installed before the chrome is measured, or `contentLayoutRect` misses the toolbar band and the grid silently loses rows.
- The navigation items set `autovalidates = false`, so AppKit's validation does not overwrite the `isEnabled` that `refreshHistoryToolbar(canBack:canForward:)` sets.
- Item order lives in the pure `HistorySeam.itemIdentifiers` ([why](../../../../notes/wolf-macos-decisions.md#the-toolbar)). The sidebar control's item is navigational, placing it beside Back and Forward, not after the title.
- The sidebar control is an `NSSegmentedControl`, not an `NSToolbarItemGroup`, whose `.selectOne` mode cannot show nothing selected.
- No caller sets the control's selection: `SidebarToolbarSeam` derives it on the `isCollapsed` KVO and at the end of `showSidebar(panel:)` and `restoreSidebar(_:)`, after the mode switch the KVO fires too early to see.
- `appearance` stays `nil`, so the window and its title inherit the app appearance `AppDelegate` forces ([why](../../../../notes/wolf-macos-decisions.md#the-titlebar)).

#### `NvimRestartedBannerView.swift`

Purpose: A small transient banner reading "Neovim was restarted", pinned top-center of the `PaneContainerView` over a working editor. `flash()` reveals it and schedules its own auto-dismiss.

**Decisions**

- `hitTest(_:)` returns `nil` for every point. The banner has no interactive controls, so a click landing in its rect during the seconds it is visible belongs to the editor pane beneath it.
- `flash()` carries a monotonic generation token so a newer flash supersedes an older pending hide, coalescing rapid restarts into one banner rather than a premature or double hide.
- Layer chrome colors are re-resolved on each appearance change, as in [`EmptyStateView.swift`](#emptystateviewswift).

**Gotchas**

- Independent of liveness, unlike `NvimStatusOverlayView`: it is flashed from the restart success path rather than from a wake, so the Alive branch that calls `hideNvimOverlay()` never clears it. A later `showNvimOverlay` re-adds the full-bleed overlay above this banner on purpose.

#### `NvimStatusOverlayView.swift`

Purpose: The crash/stall overlay — a full-bleed opaque cover over a broken editor with a status label and a "Restart Neovim" button, added as a `PaneContainerView` subview and driven by its tab core's liveness reads.

**Public API**

- `enum NvimStatusMode` — `dead(retrying:)` / `notResponding`, mirroring the two non-`Alive` `wolf_core::Liveness` states. `retrying` picks between "restarting…" and "automatic restart failed", so the message never promises a restart the spent automatic budget will not make.
- `show(mode:)` / `hide()` — toggle visibility and set the message for the mode.
- `onRestart` — the button action; the controller wires it to `tabCore.restartNeovim()`.

**Decisions**

- The restart button is present in every mode: a fallback while a `dead` core auto-restarts, and otherwise the primary action, which waits on the user.
- The opaque layer background is re-resolved on each appearance change, as in [`EmptyStateView.swift`](#emptystateviewswift).
- Deliberately never first responder, so keystrokes still reach the editor grid.

#### `OpenWakeBridge.swift`

Purpose: Registers the open channel's wake at launch, so each request the listener takes reaches `AppDelegate.handleOpenRequests()` on the main thread.

**Public API**

- `install()` — call once per process, at launch.

**Decisions**

- The thunk resolves the app delegate, not a `WindowRegistry` token: the listener is process-wide, so its token is always `0`.

#### `PaneContainerView.swift`

Purpose: The editor area: one flipped first-responder `NSView` painting every pane, owning `PaneTree`, hosting preview and terminal views.

**Public API**

- `resetAfterRestart()` — drops every pane and hosted view after a crash-restart, whose Neovim reuses window handles.
- `paneLayout`, `leafContents()`, `restoreFailed`, `paneViews`, `activePaneId`, `drawerPane`, `drawerShare` — `SessionSeam`'s inputs, terminals included; `captureViewports()` fills `paneViews` ([capture](../../../../notes/wolf-macos-decisions.md#the-per-tab-extras)).
- `capturedTerminals()`, `requestTerminalRestore(_:)` — a restart's capture and restore.
- `activeTerminal`, `activeTerminalSelection` — `nil` off a terminal pane.
- `activeWin` — the active pane's window, `0` before one binds.
- `activePreviewSourcePath` — `nil` in edit mode.
- `otherPaneTarget(prefer:)` — `0` splits, `nil` no-ops.
- `canClosePane`, `closeActivePane()`, `splitActivePane(_:)` — Close Pane's gate and action, and the splits' ([why](../../../../notes/wolf-macos-decisions.md#terminal-panes)).
- `toggleZoom()`, `toggleTerminal()`, `isZoomed`, `zoomedPaneId`, `visiblePaneFrames` — Zoom Pane, Toggle Terminal unzooming first, the saved zoom, frames by window ([why](../../../../notes/wolf-macos-decisions.md#zoom-pane)).
- `requestLayoutRebuild(_:tab:focused:drawer:)` — the launch restore at the first pane's bind ([order](../../../../notes/wolf-macos-decisions.md#the-per-tab-extras)).
- `openFile(_:wait:at:)` — places a file by [§5.2](../../../../specs/cli-and-file-open.md#52-choosing-the-pane), waiting while a restore is pending ([why](../../../../notes/wolf-macos-decisions.md#opening-files-from-outside-wolf)).
- `openPickedFile(_:at:)` — a picked file's route ([why](../../../../notes/wolf-macos-decisions.md#terminal-panes)).

**Decisions**

- One `geometry(_:fullWidth:)` call sets web-view frame, occlusion and statusbar span: no frame paints preview alignment without its web view.
- Grids are skipped only under an opaque, loaded preview web view; the flag survives reloads: WebKit shows the outgoing document.
- Scroll follows `scrollingDeltaY`'s natural-scroll-aware sign; `isDirectionInvertedFromDevice` double-corrects.
- The drawer is a pane outside `tree`, excluded from leaves, neighbours, splits ([why](../../../../notes/wolf-macos-decisions.md#terminal-panes)).
- Only a terminal pane gets `NSTextInputClient` ([why](../../../../notes/wolf-macos-decisions.md#terminal-panes)), its input going to its session; `doCommand(by:)` [passes the rest on](../../../../notes/wolf-macos-decisions.md#key-equivalents-over-a-preview).

#### `PickerChrome.swift`

Purpose: Builds the two `NSVisualEffectView` backdrops a picker panel is made of — the full-bleed panel background and the rounded search-bar background.

**Public API**

- `PickerChrome.panelBackdrop(cornerRadius:)` / `searchBarBackdrop(cornerRadius:)` — configured, masked subviews the caller pins with constraints.

**Decisions**

- The backdrops are materials rather than `windowBackgroundColor` and `controlBackgroundColor` fills, because when the desktop tints windows the tint lives in the material, not in the colour. The results list is an `NSTableView`, which paints its own tinted material, so a flat fill leaves the list one shade and the padding and search bar around it another. Drawing the colour live instead of freezing it into a `CGColor` does not help: measured, both are the untinted shade.
- `state` is `.active` rather than the default, which would drop the material — and the tint with it — whenever the panel is not key, which it is not while the editor behind it holds focus.
- Corners come from `maskImage`, not a layer `cornerRadius`: a behind-window material is composited outside the view's own layer, leaving a layer mask with nothing to clip.

#### `PickerPanel.swift`

Purpose: The reusable borderless, non-activating child window that hosts one transient picker overlay in front of its owning main window. Constructed and driven by `PickerPanelHost`.

**Decisions**

- Key eligibility is the only reason this subclass exists: a borderless panel cannot become key by default, and the hosted search field's field editor needs a key window to accept typing. `canBecomeMain` stays `false` — the panel is transient chrome.
- Hosting a picker in a child window rather than as a `PaneContainerView` subview is what puts it above the Markdown-preview `WKWebView`, which otherwise draws over sibling views regardless of subview order. The window server always composites a child window in front of its parent's content.

#### `PickerPanelHost.swift`

Purpose: Drives one picker's `PickerPanel` — presents it as a child window of the owning main window, sizes and positions it top-center, and funnels every dismissal into the picker's `close()`.

**Public API**

- `protocol PickerPanelHosting` — `presentPanel()` / `dismissPanel()` / `refitPanel()`, the seam a picker view calls through its weak `host`. Ownership runs the other way: controller to host to panel to picker view, all strong.
- `init(parentWindow:paneContainer:contentView:onResignKey:)` — `onResignKey` must capture the picker weakly, since the host already reaches it strongly through the panel.

**Decisions**

- `windowDidResignKey` is the single dismissal funnel for click-outside, Cmd-Tab, Mission Control, and another window activating. Adding separate paths for these duplicates it.
- `dismissPanel()` deliberately omits `makeKeyAndOrderFront` and `NSApp.activate`: a user who Cmd-Tabbed away should not be yanked back. It only hands first responder to the pane container for when the parent is next key.
- `refitPanel()` measures height by pinning a temporary width constraint and reading `fittingSize`, because AppKit has no equivalent of UIKit's `systemLayoutSizeFitting(_:withHorizontalFittingPriority:verticalFittingPriority:)`.

#### `PickerRow.swift`

Purpose: A file-agnostic value type describing one row of a picker list: an optional icon, a single-line title, and an opaque activation `payload`.

**Decisions**

- Deliberately general, so the same panel and cell back other selectors. Only the picker's `activateSelection()` interprets `payload` — for files it is the absolute path; a buffer selector would put a buffer token there.
- For files the title is the whole working-directory-relative path rather than the basename, because Neovim users expect to see paths.

#### `PickerRowCellView.swift`

Purpose: A reusable `NSTableCellView` rendering a `PickerRow` as a leading icon plus one title label, general enough to back any picker or selector.

**Public API**

- `init(iconLeadingInset:labelLeadingInset:)` — the caller derives both insets from the search-bar layout so the icon lines up with the magnifier and the text with the typed text.
- `rowHeight(iconSize:titleFont:verticalPadding:)` — the fixed list row height: the taller of icon and title line, plus padding that leaves room for descenders.

**Decisions**

- The title's first baseline is pinned slightly above the icon view's bottom edge rather than to it, because Finder icon images carry a transparent bottom margin below the drawn glyph; anchoring to the raw bottom leaves the text visibly low.

#### `PickerView.swift`

Purpose: The floating file picker: a hand-built search bar over an `NSTableView`, fed by a live `FfiFileSearch` session and presented in its own child panel.

**Public API**

- `open()` / `close()` — idempotent; `close()` drops the search session, cancelling the background traversal.
- `weak var host: PickerPanelHosting?` — set by `WolfWindowController`. Presentation, dismissal, and refit all go through it, so this view is never added to a superview.
- `refresh()` (immediate), `scheduleRefresh()` (coalesced), `moveSelection(delta:)`, and `activateSelection()` — which opens the selected row (or the first when none is selected) and closes, and is a no-op with no rows.

**Decisions**

- The search bar is hand-built (rounded container, magnifier, borderless `NSTextField`) rather than an `NSSearchField`, whose cell caps its text rect at the standard control height and clips the large font used here.
- Traversal wakes route through `PickerRefreshCoalescer`; keystrokes and `open()` refresh immediately. A large tree emits hundreds of batch wakes and each direct refresh re-ranks the whole list.
- `refresh()` keeps the selection index when the query is unchanged, so a streaming batch does not yank the user's selection back to the top. **Gotchas**

- Close re-entrancy and the chrome's appearance handling behave as in [`BufferPickerView.swift`](#bufferpickerviewswift).
- While open, the panel is key and the search field holds first responder, so `PaneContainerView.keyDown` never fires and keys do not reach Neovim.

#### `PickerWakeBridge.swift`

Purpose: Installs the process-global file-search wake callback so wolf-core's traversal batches reach the picker of the tab whose core fired. Same thunk shape as [`TreeWakeBridge`](#treewakebridgeswift).

**Public API**

- `install()` — called once at launch.

**Decisions**

- The wake requests a coalesced `scheduleRefresh()` rather than `refresh()`, so a large traversal's burst of batch wakes collapses to one re-rank per frame. A wake after the picker has closed is a no-op.

#### `SearchView.swift`

Purpose: The sidebar content-search panel — a query field and three toggles above an `NSOutlineView` of ripgrep-backed results, grouped as matched files with their matching lines beneath. Line windowing, the byte-to-UTF-16 span mapping, and the char budget live in wolf-core's `search_tree` and the `WolfKeyMap` seams, not here.

**Public API**

- `init(handle:paneContainer:)`, `seedToggles()`, `focusSearchField()`, `setSidebarVisible(_:)`, `scheduleRefresh()`, `refresh()`, `close()`.
- `query`, `caseSensitive`, `useRegex`, `allFiles`, `searchedQuery`, `restore(…)` — the panel the window state saves and restores. `onChange` reports Return and toggle clicks, which wake no core.
- `SearchFileItem` / `SearchLineItem` — the outline's row models. `absPath` is the identity `NSOutlineView` keys expansion and selection on.

**Decisions**

- Searching is wired to the field's action (Return), and `controlTextDidChange` and the toggles deliberately run nothing, because each run spawns an rg subprocess. Assigning `search` replaces the session, which cancels the previous subprocess.
- `focusSearchField()` defers `makeFirstResponder` by one runloop tick. Invoked from the Command Palette, the palette's `close()` runs afterwards and ends in `makeFirstResponder(paneContainer)`, clobbering a synchronous focus.
- `refresh()` rebuilds only when the snapshot version or the char budget has advanced, so repeated batches and idle wakes cost nothing, while a sidebar resize re-windows the lines.
- Collapsed files are tracked by path so a rebuild preserves them; every other file row expands.
- Activating a line row opens through [`PaneContainerView.openPickedFile`](#panecontainerviewswift) at the match's line and column. A preview has no cursor, so they reach nvim only when the file is edited.

#### `SearchWakeBridge.swift`

Purpose: Installs the process-global content-search wake so each streamed ripgrep batch reaches the `SearchView` of the tab whose core fired. Same thunk shape as [`TreeWakeBridge`](#treewakebridgeswift).

**Public API**

- `install()` — called once at launch.

**Decisions**

- The wake asks for a coalesced refresh, which an idle panel no-ops on its own version gate.

#### `SidebarContainerView.swift`

Purpose: The sidebar body — `FileTreeView` or `SearchView`, one shown at a time, chosen by the toolbar's sidebar control in [`MainWindow.swift`](#mainwindowswift). Adopted directly as the sidebar's hosted view.

**Public API**

- `init(tree:search:)`, plus `tree` and `search` so the controller can reach either child.
- `mode`, `setMode(_:)`, and `setSidebarVisible(_:)`, which is where the split item's `isCollapsed` KVO is relayed.
- `onChange` — fired by a mode change and relayed from `search.onChange`, so the controller can schedule a state write.

**Decisions**

- Both children stay in the view hierarchy and the inactive one is hidden, so switching modes preserves each panel's state.
- `setSidebarVisible` passes `visible && mode == …` down, so the inactive child's refresh gate stays closed and it does no work while off-screen.
- Both children are pinned to the top of the safe area, not of the view, which keeps them clear of the transparent full-size-content titlebar.

**Gotchas**

- `setMode(_:)` only swaps which child is shown. Seeding the search toggles and focusing its field is the caller's job: `MainWindow.showSidebar(panel:)` focuses, and the toolbar control's action also seeds.

#### `StatePersistence.swift`

Purpose: Watches one window's geometry, sidebar collapse state, and tab-group membership/selection, calling an injected `onChange` on every change. The owning `WolfWindowController` wires that closure to the coordinator's coalesced whole-tree snapshot, so the window state tracks the live UI.

**Public API**

- `init(window:sidebarItem:onChange:)` — every UI reference is weak; the controller owns those lifetimes.
- `install()` — arms the observers. They are torn down in `deinit`, so the object must be kept alive for as long as the window.
- `normalFrame` — the window's last frame while neither zoomed nor full screen, recorded at install and before each geometry `onChange`, which the window state saves as the window's frame. Exiting full screen is a geometry change, so the frame it returns to is recorded.

**Decisions**

- Trigger-only glue, the normal frame aside: no file IO, no per-field state writes. Persistence is a whole-tree snapshot the coordinator walks, so writing anything here would duplicate that walk rather than extend it.
- The `didResize` observer bails while `inLiveResize`. A live drag emits it once per frame, and `didEndLiveResize` already catches the final size; the `didResize` registration exists only for discrete resizes such as zoom and a programmatic `setFrame`.

#### `TabCore.swift`

Purpose: One tab's captive Neovim, wake token and crash restart, serving its view through `TabCoreView`. Main-thread-only.

**Public API**

- `start(launchCwd:coordinator:)` — the sole caller of `wolf_ffi_start`, `nil` on failure, registering the core in `WindowRegistry`. Defaults `WOLF_NVIM_BIN`, `WOLF_RG_BIN` and `WOLF_TERMINFO_DIR` to the bundle's (`overwrite=0`), so an export wins.
- `wolfTabId` — the tab's [`WOLF_TAB_ID`](../../../../specs/cli-and-file-open.md#3-tab-ids), set in `start` before any session and kept by a crash restart.
- `view` — the tab it serves, weak; a live test stands a recording `TabCoreView` in.
- `onWake()` and its siblings — the wake bridges' targets.
- `setUiFocus(gained:)` — a gain fires `FocusGained`, whose `:checktime` reloads a file another tab saved.
- `restartNeovim()` — the "Restart Neovim" button's entry, resetting the attempt count.
- `shutdown()` — the teardown's, unregistering the token.

**Decisions**

- `onWake` acts on [`LivenessSeam`](../WolfKeyMap/README.md#livenessseamswift)'s decision, committing on `persist`, and drains commands last, since a drained close can shut the core down.
- An automatic restart waits `autoRestartDelay`: `handle.restart()` blocks the main thread, so only a pending timer lets the overlay paint first.
- Attempts count when scheduled and reset on `Alive`, a successful respawn or the button, so a press against a failing respawn can cost two respawns ([why](../../../../notes/crates/wolf-gnome/src/shell.rs.md#one-press-two-respawns)).
- The respawn captures files, cwd and terminals before `handle.restart()` replaces the mirror, then runs [`restoreSteps(for:launchCwd:)`](../WolfKeyMap/README.md#livenessseamswift). On `false` it applies only a fresh decision's overlay: scheduling there would bypass the cap.

**Gotchas**

- The respawn reopens files as queued input, which a blocking call such as `open_terminal` overtakes; so terminals restore through `queue_bind_terminal`.

#### `TabGroupObserver.swift`

Purpose: Best-effort KVO on one window's native tab group — its `windows` and `selectedWindow` — so a tear-off, drag-merge, reorder, or tab switch fires `onChange`. Owned by `StatePersistence`; the native tab bar mutates the group with no call site of ours, so nothing else would notice.

**Public API**

- `install()` — attach and arm.
- `invalidate()` — drop every observation; the owner calls this from its `deinit`.

**Decisions**

- The observations are re-attached on `didBecomeKey`, because a window's `tabGroup` can change identity: a torn-off window lands in a fresh group, and KVO registered against the group it left never fires again. That notification also re-emits `onChange`, since the membership change itself has no other dependable signal.

**Gotchas**

- Deliberately best-effort. The consumer is expected to coalesce, both to absorb the resulting KVO burst and to backstop the transitions KVO misses.

#### `ToastView.swift`

Purpose: The `ext_messages` message toast — an overlay view that `PaneContainerView` pins bottom-left of the editor area and paints the current message's styled chunks into. Content taller than `maxVisibleLines` scrolls in place.

**Public API**

- `update(_:defaultBg:) -> Bool` — projects the latest `FfiToast` and returns whether the toast is visible, having already applied that to `isHidden`.
- `scrollBy(deltaY:)` — positive `deltaY` reveals later lines; clamped to the scrollable range.
- `preferredHeight` / `preferredWidth` / `lineCount` / `lineHeight` — sizing inputs for the container. `preferredHeight` is capped at `maxVisibleLines`; `lineCount` is not.
- `applyMetrics(_:)` — re-measure after a per-window font-size change.

**Decisions**

- Chunk foreground colors arrive already resolved in Rust against the highlight table. Message color is load-bearing, so do not re-resolve or override it here.
- `acceptsFirstResponder` is false: the container decides whether a wheel gesture reaches `scrollBy`. Scrolling while hovering the toast is the single gesture that must not reach Neovim or a terminal session, because any input dismisses the toast core-side — making this view a responder-chain scroll target would defeat that.
- The scroll offset resets only when the line count changes, so a repeat wake carrying the same message keeps the user's position.

#### `TreeWakeBridge.swift`

Purpose: Registers the file-tree traversal wake with the Rust core at launch and routes each wake to the sidebar of the tab whose core fired it.

Shared `*WakeBridge` shape: core fires the registered `extern "C"` callback on a tokio worker thread carrying its core's token; the thunk hops to the main thread, resolves the token through `WindowRegistry`, and calls one method on the resulting `TabCore`.

**Public API**

- `install()` — call once per process, at launch.

**Decisions**

- The thunk closure captures nothing but the global `WindowRegistry`. That is what lets it convert to a `@convention(c)` function pointer; capturing any per-window state breaks the conversion outright.
- The tree is asked for a coalesced refresh, so a large directory's batch wakes collapse to at most one reload per frame.

**Gotchas**

- A wake arriving after a tree has closed is harmless — the refresh is a no-op when closed.

#### `WakeBridge.swift`

Purpose: Registers the editor render wake with the Rust core at launch, driving a repaint of the right window. Same thunk shape as `TreeWakeBridge`.

**Public API**

- `install()` — call once per process, at launch.

**Decisions**

- Hands the wake to `TabCore.onWake()` immediately rather than coalesced: this wake is the frame.

#### `WindowRegistry.swift`

Purpose: The process-global map from a core's wake token to the `TabCore` that owns it. The per-core wake bridges' thunks carry only a plain `u64` across the thread hop, and resolve it here.

**Public API**

- `register(_:token:)` / `unregister(token:)` / `tabCore(for:)` — all main-thread only.

**Decisions**

- Unsynchronized on purpose. Every access is already on the main thread: registration at `TabCore.start`, lookup inside the dispatched-to-main wake, removal at `TabCore.shutdown`. Adding a lock would imply it is safe to call from the wake's own tokio thread, which it is not.

**Gotchas**

- `tabCore(for:)` returns `nil` for a wake that arrives after its tab closed; every caller treats that as a no-op rather than an error.

#### `WolfApplication.swift`

Purpose: The app's entry point, called by [`WolfMain`](../WolfMain/README.md): `WolfApplication.run()` builds the shared `NSApplication`, installs `AppDelegate`, sets the regular activation policy, and runs until the app terminates.

**Decisions**

- The delegate is a local kept alive by `withExtendedLifetime` across `run()`. `NSApplication.delegate` is weak, so without it an optimized build may release the delegate as soon as it is assigned.

#### `WolfWindowController.swift`

Purpose: One tab's whole UI stack, owning its [`TabCore`](#tabcoreswift) as that core's `TabCoreView`. Each tab is its own OS window, maybe tab-grouped.

**Public API**

- `build()` — shows the stack at its `WindowPlacement`, `.cascaded` being [§7's](../../../../notes/wolf-macos-decisions.md#new-windows) through `NSWindow.cascade(fromTopLeft:size:)`, and becomes its core's view.
- `hasFolder` — false in the empty state, whose sidebar never scans `home`.
- `persistedCwd` — the live cwd, else the launch directory ([why](../../../../notes/wolf-macos-decisions.md#opening-files-from-outside-wolf)); `""` in the empty state.
- `openFile(_:wait:)` — places a file from outside Wolf and raises the tab ([empty tab](../../../../notes/wolf-macos-decisions.md#opening-files-from-outside-wolf)).
- `persistedTab()` — this view's [`SessionSeam`](../WolfKeyMap/Persistence/README.md#sessionseamswift) contribution, seeded from the restored tab.
- `normalFrame` — [`StatePersistence`](#statepersistenceswift)'s, `nil` until recorded.
- `canCopy` — Copy's gate; a terminal pane copies and pastes through its session.
- `drainAndDispatchCommands()` — apart from `onRenderWake()`, so the core drains in every state ([why](../WolfKeyMap/README.md#livenessseamswift)).
- `captureViewports()` — a no-op once torn down.

**Decisions**

- `tabCore` is held strong and holds this view weakly.
- `teardown()` commits its close before setting `isTornDown`, which the burst's read skips, or a last close commits nothing. It is idempotent and defers `removeController` a turn, holding `self`: the coordinator holds the last reference in `windowWillClose`, and dropping it there frees the `NSWindow` mid-close.
- `window.historyNav = self` is explicit: as no `NSWindowController`, it gets no `target: nil` toolbar action.
- The file tree starts last in `build()`, after the collapse state seeds its refresh gate, so a collapsed sidebar defers outline work.
- Both key edges sync terminal focus; tab focus follows `windowDidBecomeMain` ([why](../../../../notes/wolf-macos-decisions.md#one-neovim-per-tab)).
