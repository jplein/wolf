# wolf-macos/Sources/WolfMacOS

The AppKit application: windows, views, menus, and the coordinator that owns projects and views. It holds only what genuinely needs a window; the decisions it acts on live in the pure [`WolfKeyMap/`](../WolfKeyMap/README.md) seam.

Nothing in this directory has automated coverage — there is no headless window server. See [wolf-macos-decisions.md](../../../../notes/wolf-macos-decisions.md) for that posture and for the project, threading, and persistence models these files implement.

## Files

#### `AppDelegate.swift`

Purpose: The application coordinator: once-only launch setup, the live `WolfWindowController` (one per tab) and `Project` collections, window/tab tree restore and persistence, and the dynamic menus (`NSMenuDelegate` / `NSMenuItemValidation`). The one-Neovim-per-project model is described in [`wolf-macos-decisions.md`](../../../../notes/wolf-macos-decisions.md).

**Public API**

- `makeNewWindow(originatingCwd:)` — always opens a fresh project in the empty state; the argument is ignored, and exists only to keep the shared `CommandDispatcher` call site uniform.
- `makeNewTab(from:)` — reuses the key controller's project, adding a tabpage rather than starting a core.
- `scheduleStateSnapshot()` / `snapshotNow()` — coalesced versus immediate whole-tree commit to `state.json`. Every geometry, sidebar, cwd and tab-group change routes through the coalesced form.
- `otherWindows(excluding:)`, `controllerForCwd(_:)`, `raiseWindow(tabId:)` — the command palette's candidate sources and raise target.

**Decisions**

- Launch order is fonts, then config, then metrics: measurement needs the config's family, size and line height, and the bundled families must reach CoreText first.
- `appearanceObservation` must stay stored — an `NSKeyValueObservation` invalidates itself once deallocated, so a discarded token silently stops auto-mode appearance tracking.
- Both app focus edges are pushed to nvim: `FocusGained` fires only on a real unfocused-to-focused transition, so without the resign-side `false` the `WolfAutoRead` `:checktime` reload fires once and never again.

**Gotchas**

- Owning-controller lookups use `effectiveKeyWindow`, not `NSApp.keyWindow`: while a picker panel is key, no controller owns the key window, so it resolves to that panel's parent.
- The snapshot records each tab's `persistedCwd` (tabpage-local), not `currentCwd`, which follows the current tabpage and so is wrong for unselected tabs.

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

- The front-end-only items (History, Project, Search) target the coordinator directly instead of carrying a tag, so `CommandKind.toggleSidebar`, `.sidebarShowProject` and `.searchInProject` never reach `CommandDispatcher` from a menu item. They still reach it from the nvim-drained path and from the command palette, whose static rows dispatch through it.

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

Purpose: The monospace cell geometry (advance width, cell height, baseline) plus the four cached CoreText faces used to paint the grid, measured from the resolved user config. `baseline` is measured from the cell's top edge, since the grid view is flipped.

**Public API**

- `measured(config:)` / `measured(config:pointSize:)` — the default size, and an explicit size for the per-window font-size steps.
- `defaultPointSize(config:)` — `WOLF_FONT_PT` when set to a positive number, else `config.font_size`. Also what a window's font-size reset targets, so both read one resolution.
- `font(for:)` — the cached face for a `GlyphFace`.

**Decisions**

- `cellWidth` comes from the regular face's advance only, so every face paints into the same fixed-cell slot.
- A derived bold or italic variant is accepted only when `CTFontGetSymbolicTraits` confirms the traits actually applied. A family with no such face yields a copy without them, so the check is what distinguishes "resolved" from "silently unchanged"; the fallback is the regular face, so the glyph paints rather than dropping.
- An unresolvable configured family falls back to Menlo, compared case-insensitively so that e.g. "jetbrains mono" matches "JetBrains Mono".

#### `CmdlineView.swift`

Purpose: The native command-line overlay (`ext_cmdline`). It paints one row — `prompt + firstc + text` with a caret at `pos` — pinned to the bottom of the editor area, shown only while a command line is active rather than reserving a permanent band. The `prompt` is painted inline for `confirm()` and `input()`-style prompts and is empty for a plain `:` or `/` line.

**Public API**

- `update(_:defaultFg:defaultBg:) -> Bool` — projects an `FfiCmdline` in and returns whether the command line is active, so the container can lay the overlay out or hide it.
- `applyMetrics(_:)` — re-measure after a per-window font-size change.
- `preferredHeight` — one text row.
- `overlayFont(metrics:)` — a free function, not a member: an `NSFont` matching the grid's `CellMetrics`, shared with the completion-popup and toast overlays.

**Decisions**

- `pos` is a byte offset, so the caret x is measured from `CmdlineSeam.caretPrefix` rather than from a Swift character or UTF-16 index.

#### `Color.swift`

Purpose: Converts the bridge's packed `FfiColor` (`0x00RRGGBB` plus a `set` flag) into a `CGColor`, applying the white and black fallbacks when unset.

**Decisions**

- Colors are built in an explicit sRGB color space. The convenience `CGColor(red:green:blue:alpha:)` initializer builds `kCGColorSpaceGenericRGB` instead, whose different gamma renders the same packed value visibly lighter (`#1b1b1b` displays as roughly `#242424`), so the grid background would not match the sRGB titlebar.

#### `CommandDispatcher.swift`

Purpose: Maps a decoded `CommandKind` to its native AppKit action. Both the menu path (`AppDelegate.handleCommandMenuItem`) and the nvim-drained path (`WolfWindowController`) route here, so a command has one implementation regardless of how it was invoked.

**Public API**

- `dispatch(_:window:coordinator:originatingCwd:controller:)` — `window` and `controller` are the command's target: the key window for the menu path, the originating controller for the nvim-drained path, so a command issued from window A acts on window A. `originatingCwd` is consumed by `.newWindow` alone; `nil` for either target means the command quietly does nothing.

**Decisions**

- The switch carries no `default` case, so adding a `CommandKind` variant fails to compile until it is handled here.

#### `CommandPaletteView.swift`

Purpose: The floating command palette — a search field plus a results list backed by an `FfiCommandPalette` session, hosted in a picker panel. `CommandPaletteRow` carries the label plus the payload for one of three actions: dispatch a static command, raise a window, or open-or-raise a recent folder.

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
- `preferredWidth` / `preferredHeight` / `rowHeight` / `visibleRowCount` — sizing inputs for the container's layout pass.
- `applyMetrics(_:)` — re-measure after a per-window font-size change.

**Decisions**

- A list longer than `maxVisibleRows` scrolls to keep the selection visible instead of growing or gaining a scrollbar.
- Each row shows the item's `word` with `kind` as a trailing hint; `menu` and `info` are ignored.

**Gotchas**

- The view does not place itself. `PaneContainerView` positions it directly above `CmdlineView`, anchored at the cmdline byte column.

#### `Cursor.swift`

Purpose: Pure cursor-rectangle geometry. `CursorShape` mirrors `wolf_core::CursorShape` as packed into `FfiFrame.cursor_shape()`.

**Public API**

- `rect(shape:cellX:cellY:cellW:cellH:cellPercentage:)` — `.block` fills the cell and ignores `cellPercentage`; `.vertical` is a left bar of full height; `.horizontal` is a bottom bar of full width. A `cellPercentage` of 0 means Neovim specified none, in which case `defaultCellFraction` applies; bar thickness is floored at `minBarPx` and capped at the cell.

**Decisions**

- Only geometry lives here. The reverse-video fill and glyph colors are resolved in Rust (`FfiFrame.cursor_fill()` / `cursor_glyph()`); do not recompute them in Swift.
- The `minBarPx` floor exists so a small `cellPercentage` still renders a visible bar instead of a hairline.

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

Purpose: The "no folder open" overlay — a full-bleed subview of `PaneContainerView` holding one centered "Open folder…" button. `WolfWindowController` toggles it with `isHidden` and hides it once a folder opens.

**Public API**

- `onOpenFolder: (() -> Void)?` — invoked by the button; the controller wires it to the same Open Folder path as the menu command.

**Decisions**

- The view never becomes first responder, so keystrokes typed at launch reach the editor grid instead of the button.
- The layer background is re-resolved inside `performAsCurrentDrawingAppearance` on every effective-appearance change. Layer `CGColor`s do not adapt to a dark/light switch, and a bare `NSColor.windowBackgroundColor.cgColor` resolves against whatever appearance is globally current, which is often the stale one mid-change. The centered `NSButton` needs no such handling; AppKit re-resolves its dynamic colors itself.

**Gotchas**

- It constrains itself to its superview in `viewDidMoveToSuperview`, so it must be added as a subview of the view it is meant to fill.

#### `FileTreeView.swift`

Purpose: The file-tree sidebar: an `NSOutlineView` driven by a long-lived `FfiFileSearch`, rooted at a synthetic cwd node. A click opens a file or toggles a directory.

**Public API**

- `start()` / `close()` — open and drop the tree-search session, cancelling its background traversal.
- `refresh()` is immediate; `scheduleRefresh()`, which `TreeWakeBridge` uses, coalesces traversal batches to about one reload per frame.
- `onRenderWake()` — re-roots on a cwd change, then accents and reveals what the active pane shows.
- `setSidebarVisible(_:)` — relays the split item's `isCollapsed` KVO; refresh defers while collapsed.

**Decisions**

- `NSOutlineView` keys expansion and selection on object identity, so one `TreeItem` per `relPath` is cached and handed back across reloads. Fresh instances would collapse the tree.
- The outline draws a clear background and adds no `NSVisualEffectView`: the enclosing `.sidebar` split item already supplies the vibrancy.
- `reveal(abs:)` expands ancestors top-down before locating the row, because a row exists only once its parent is expanded; a not-yet-streamed file leaves the reveal pending.
- `reroot(to:)` cancels the pending coalesced refresh first — a stale fire would run against the discarded tree.
- `TreeCellView` marks what the active pane shows — a preview's source, else the current buffer — with a [`SidebarRowStyle`](../WolfKeyMap/README.md#sidebarrowstyleswift) label color, not the row's selection tint, which AppKit drops at the first app switch.
- Activating a file routes on the active pane's mode, as the pickers do: a preview pane gets `preview_file`, falling back to `open` on `0`, which drops the pane to edit mode.

#### `FontRegistration.swift`

Purpose: Registers the TTFs bundled at `Contents/Resources/fonts/` with CoreText so both vendored families resolve — "iA Writer Mono S" (the default grid font) and "JetBrains Mono". `AppDelegate` calls `registerBundledFonts()` at launch, before the config is loaded and metrics are measured.

**Decisions**

- Registration is process-scoped, not a system-wide font install.
- Failures log and continue. A missing `fonts/` directory (the developer and `swift run` paths, which have no bundle) and an already-registered face are both expected, and neither may be fatal.

#### `FrameMetrics.swift`

Purpose: Process-global glue around the pure `WolfKeyMap.FrameStats` seam: the once-only `enabled` evaluation, the clock reads, the report `Timer`, and the writes to stderr and `WOLF_FRAME_STATS_FILE`. Development instrumentation, off unless `WOLF_FRAME_STATS` is set, main thread only.

**Public API**

- `begin() -> UInt64` / `end(_:_:)` — bracket an instrumented region, ideally through `defer` so early returns are still measured. `begin()` returns 0 when disabled, and the elapsed subtraction saturates.
- `start()` / `emit()` — `AppDelegate` arms the reporter at launch and calls `emit()` at termination to flush the final partial interval.

**Decisions**

- `enabled` is a `static let`, evaluated once via `swift_once`; `start()` touches it at launch so that one `ProcessInfo` read cannot land inside a measured frame. Disabled, each call site costs a token load and a branch.
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

Purpose: The single editor window. Its content view controller is an `EditorSplitViewController` under a transparent full-size-content titlebar with a unified `NSToolbar`; the class is its own toolbar delegate.

**Public API**

- `protocol HistoryNavigating` — the Back / Forward actions the toolbar needs from `WolfWindowController`. Explicit delegation rather than a `target: nil` responder-chain hop, because that controller is not an `NSWindowController` and so is not in the chain.
- `init(sidebar:paneContainer:metrics:cols:rows:)` — sizes the content so the editor pane keeps exactly `cols` × `rows` cells below the chrome.
- `applyTitlebarBackground(rgb:set:)` — idempotent; the first themed call captures the pristine `backgroundColor`, so `set: false` restores the system default exactly.

**Decisions**

- `isReleasedWhenClosed = false`: the window controller holds the strong reference, and AppKit's default adds an unbalanced release that over-releases the window — the deferred close-animation dealloc then crashes.
- The toolbar is installed before the chrome is measured, so `contentLayoutRect` accounts for the toolbar band; measuring first silently costs the grid rows.
- The navigation items set `autovalidates = false`, which is what makes an `isEnabled` set by `refreshHistoryToolbar(canBack:canForward:)` stick instead of being overwritten by AppKit's own validation pass.
- Item order lives in the pure `HistorySeam.itemIdentifiers` so it is testable without AppKit. The system `.toggleSidebar` must precede the tracking separator, or the visible title claims the content-leading slot and pushes the toggle to the trailing edge.
- The delegate returns `nil` for `.toggleSidebar` so AppKit supplies its built-in item, auto-wired to `NSSplitViewController.toggleSidebar(_:)`.

#### `NvimRestartedBannerView.swift`

Purpose: A small transient banner reading "Neovim was restarted", pinned top-center of the `PaneContainerView` over a working editor. `flash()` reveals it and schedules its own auto-dismiss.

**Decisions**

- `hitTest(_:)` returns `nil` for every point. The banner has no interactive controls, so a click landing in its rect during the seconds it is visible belongs to the editor pane beneath it.
- `flash()` carries a monotonic generation token so a newer flash supersedes an older pending hide, coalescing rapid restarts into one banner rather than a premature or double hide.
- Layer chrome colors are re-resolved against the view's current `effectiveAppearance` on `viewDidChangeEffectiveAppearance()`, because layer `CGColor`s do not follow a runtime dark/light switch.

**Gotchas**

- Independent of liveness, unlike `NvimStatusOverlayView`: it is flashed from the restart success path rather than from a wake, so the Alive branch that calls `hideNvimOverlay()` never clears it. A later `showNvimOverlay` re-adds the full-bleed overlay above this banner on purpose.

#### `NvimStatusOverlayView.swift`

Purpose: The crash/stall overlay — a full-bleed opaque cover over a broken editor with a status label and a "Restart Neovim" button, added as a `PaneContainerView` subview and driven by the project's liveness reads.

**Public API**

- `enum NvimStatusMode` — `dead` / `notResponding`, mirroring the two non-`Alive` `wolf_core::Liveness` states.
- `show(mode:)` / `hide()` — toggle visibility and set the message for the mode.
- `onRestart` — the button action; the controller wires it to `project.restartNeovim()`.

**Decisions**

- The restart button is present in both modes: a fallback while a `dead` core auto-restarts, and the primary action for `notResponding`, which waits on the user.
- The opaque layer background is re-resolved against the view's current `effectiveAppearance` on `viewDidChangeEffectiveAppearance()`, because layer `CGColor`s do not follow a runtime dark/light switch.
- Deliberately never first responder, so keystrokes still reach the editor grid.

#### `PaneContainerView.swift`

Purpose: The editor area — one flat, flipped `NSView` that paints every split pane, owns the pure `PaneTree`, hosts the preview web views, and is the editor's first responder. [wolf-macos-decisions.md](../../../../notes/wolf-macos-decisions.md) covers the flat-view choice and how a preview reaches its web view.

**Public API**

- `markRefocusRebind()` / `rebind(tabpageHandle:)` — arm a grid rebind after a tabpage switch or restart.
- `layoutSnapshot()` / `requestLayoutRebuild(_:)` — the snapshot is `nil` for a lone empty pane.
- `activeWin` — the per-view active window, keying `activePaneSelection` and `activePreviewSourcePath`. Never the tabpage-shared `WolfCoreHandle.active_win()`, which would let a background tab's selection enable Copy here.
- `activePreviewSourcePath` — the Markdown source the active pane previews, or `nil` in edit mode; the sidebar marks it.
- `otherPaneTarget(prefer:)` — maps `OtherPaneSeam.target` to the window argument for `preview_in_other` / `edit_in_other`: `0` means split, `nil` means no-op.

**Decisions**

- The refocus rebind waits for `handle.active_tabpage()` to match this view's tabpage and skips close detection that pass: the vacated tabpage's grids would prune live panes.
- Web-view frame, occlusion test and statusbar span come from one `geometry(_:fullWidth:)` call, written together before `needsDisplay`, so no frame paints preview alignment without a web view.
- A grid is skipped only under an opaque, loaded preview web view; the flag survives reloads because WebKit keeps showing the outgoing document.
- Scroll direction is the sign of `scrollingDeltaY`, which already reflects natural scrolling; `isDirectionInvertedFromDevice` double-corrects.
- The terminal drawer pane lives in the pane map but outside `tree`, so leaves, neighbours and splits exclude it.

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

- The panel is added with `addChildWindow(_:ordered:.above)` so the window server composites it in front of the parent's content and moves it with the parent.
- `windowDidResignKey` is the single dismissal funnel for click-outside, Cmd-Tab, Mission Control, and another window activating. Adding separate paths for these duplicates it.
- `dismissPanel()` deliberately omits `makeKeyAndOrderFront` and `NSApp.activate`: a user who Cmd-Tabbed away should not be yanked back. It only hands first responder to the pane container for when the parent is next key.
- `refitPanel()` measures height by pinning a temporary width constraint and reading `fittingSize`, because AppKit has no equivalent of UIKit's `systemLayoutSizeFitting(_:withHorizontalFittingPriority:verticalFittingPriority:)`.

**Gotchas**

- The positioning arithmetic is pure and lives in `PickerPanelSeam`; this class only maps its top-left content rect into a bottom-left screen frame.

#### `PickerRow.swift`

Purpose: A file-agnostic value type describing one row of a picker list: an optional icon, a single-line title, and an opaque activation `payload`.

**Decisions**

- Deliberately general, so the same panel and cell back other selectors. Only the picker's `activateSelection()` interprets `payload` — for files it is the absolute path; a buffer selector would put a buffer token there.
- For files the title is the whole working-directory-relative path rather than the basename, because Neovim users expect to see paths.

#### `PickerRowCellView.swift`

Purpose: A reusable `NSTableCellView` rendering a `PickerRow` as a leading icon plus one title label, general enough to back any picker or selector.

**Public API**

- `init(iconLeadingInset:labelLeadingInset:)` — the caller derives both insets from the search-bar layout so the icon lines up with the magnifier and the text with the typed text.
- `configure(with:)` — applies a row's icon and title.
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
- The panel background and the search bar are `PickerChrome` backdrops; the border alone stays a `CGColor`, which ignores appearance changes, so `viewDidChangeEffectiveAppearance()` re-resolves it inside `performAsCurrentDrawingAppearance`.
- Traversal wakes route through `PickerRefreshCoalescer`; keystrokes and `open()` refresh immediately. A large tree emits hundreds of batch wakes and each direct refresh re-ranks the whole list.
- `refresh()` keeps the selection index when the query is unchanged, so a streaming batch does not yank the user's selection back to the top.
- `close()` clears `isOpen` before dismissing, because `orderOut` re-enters through `windowDidResignKey → close()`.

**Gotchas**

- While open, the panel is key and the search field holds first responder, so `PaneContainerView.keyDown` never fires and keys do not reach Neovim.

#### `PickerWakeBridge.swift`

Purpose: Installs the process-global file-search wake callback so wolf-core's traversal batches reach the right project's picker.

**Public API**

- `install()` — called once at launch.

**Decisions**

- The callback closure captures nothing, so it converts to a `@convention(c)` function pointer. Routing is by the per-project token looked up in `WindowRegistry`, not by a captured reference, which is what lets a single global callback serve every project.
- The wake requests a coalesced `scheduleRefresh()` rather than `refresh()`, so a large traversal's burst of batch wakes collapses to one re-rank per frame.

**Gotchas**

- Core fires this on a tokio worker thread; the thunk hops to the main thread before touching AppKit. A wake arriving after the picker has closed is a harmless no-op.

#### `PreviewClickFocusHandler.swift`

Purpose: The `WKScriptMessageHandler` that turns a preview web view's capture-phase `mousedown` (posted by `PreviewSeam.clickFocusJS`) into `PaneContainerView.focusPreviewPane(win:)`.

**Public API**

- `init(win:container:)` — `win` is the Neovim window handle identifying the pane in the container's per-window maps.

**Decisions**

- It exists because a `WKWebView` swallows AppKit mouse events, so the container's `mouseDown` never fires over a preview pane. The injected script message is the only signal that a preview pane was clicked.
- `container` is weak: the web view's `WKUserContentController` retains this handler strongly, so a strong back-reference would be a cycle.

#### `PreviewNavigationDelegate.swift`

Purpose: The `WKNavigationDelegate` that routes a preview web view's clicked links through `PreviewLinkSeam` back into the owning `PaneContainerView`.

**Public API**

- `init(win:container:)` — `win` is the Neovim window handle identifying the pane in the container's per-window maps.

**Decisions**

- Every `.linkActivated` navigation is cancelled and handled out of band (open in the browser, scroll to a fragment, or preview another file), so the pane only ever shows core-rendered HTML. Non-link navigations — the initial `loadHTMLString` and programmatic `evaluateJavaScript` scrolls — must keep passing through with `.allow`, so the guard cannot be widened to cancel everything.
- Link classification lives in `WolfKeyMap.PreviewLinkSeam`; this file only dispatches the resulting action.
- `container` is weak because the container retains this delegate (`WKWebView.navigationDelegate` is itself weak).
- The two `didFail…` callbacks are routine bookkeeping, not an error path: they decrement that window's outstanding-navigation count — see [wolf-macos-decisions.md](../../../../notes/wolf-macos-decisions.md) for why it is a count. A cancelled policy decision, meaning every clicked link, also arrives here, so the container ignores a callback for which it counted no navigation.

#### `PreviewScrollStateHandler.swift`

Purpose: The `WKScriptMessageHandler` that forwards a preview page's pushed scroll state, posted by `PreviewScrollSeam.stateScriptJS`, into its `PaneContainerView`'s per-window cache.

**Public API**

- `init(win:container:)` — `win` is the Neovim window handle identifying the pane in the container's per-window maps.

**Decisions**

- The push direction, and why it cannot be a pull, is in [wolf-macos-decisions.md](../../../../notes/wolf-macos-decisions.md).
- A body missing `s` degrades to `0` rather than dropping the message, so a partially decodable push still refreshes the cache.
- `container` is weak: the web view's `WKUserContentController` retains this handler strongly.

#### `PreviewSelectionHandler.swift`

Purpose: The `WKScriptMessageHandler` that forwards a preview page's pushed selected text, posted by `PreviewCopySeam.selectionScriptJS`, into its `PaneContainerView`'s per-window cache, where Edit > Copy reads it synchronously.

**Public API**

- `init(win:container:)` — `win` is the Neovim window handle identifying the pane in the container's per-window maps.

**Decisions**

- The push direction, and why it cannot be a pull, is in [wolf-macos-decisions.md](../../../../notes/wolf-macos-decisions.md).
- An empty payload is a positive clear and drops the cached entry, while a body `PreviewCopySeam.decodeSelectionMessage` rejects leaves the cache untouched: a malformed message says nothing about the selection, so treating it as a clear would discard a good entry.
- `container` is weak: the web view's `WKUserContentController` retains this handler strongly.

#### `Project.swift`

Purpose: One captive Neovim instance shared by N views, each bound to one nvim tabpage. Owns the single `WolfCoreHandle` and the per-project wake token. Main-thread-only.

**Public API**

- `start(launchCwd:coordinator:)` — the sole caller of `wolf_ffi_start`; `nil` when the core fails to start. Points core at the bundled `nvim` and `rg` with `overwrite=0`, so a developer's `WOLF_NVIM_BIN` / `WOLF_RG_BIN` export wins.
- `attach(_:)`, `detach(_:)`, `setCurrent(_:)`, `bindFirstTabpage()`, `addTabpage(cwd:)`, `closeTabpage(for:)`; the wakes `onWake()`, `onPickerWake()`, `onTreeWake()`, `onSearchWake()`; plus `restartNeovim()`, `setBackground(dark:)`, `setUiFocus(gained:)`, `shutdown()`.

**Decisions**

- Views are held weakly (the coordinator owns their lifetime) and each view holds its project strongly, so the core outlives one view's teardown while a sibling still binds it. `detach` of the last view is the only core-shutdown path.
- Render and picker wakes go only to `currentView`; tree and content-search wakes fan out to every view, because each view runs its own traversal or search session and the token carries no view discriminator. That is safe because each view's refresh is version-gated.
- `onWake` reads liveness before rendering, so a dead core auto-restarts and a stalled one waits for the user's button instead of painting a stale frame.
- `restartNeovim` captures each view's open files and cwd before `handle.restart()`, which wipes the mirror, and reopens the active file last so it lands focused. A `false` return means no restart happened, so nothing is rebound.
- `closeTabpage` skips the last view: closing nvim's last tabpage fails with E784, and that view's `detach` shuts the core down anyway.

#### `SearchView.swift`

Purpose: The sidebar content-search panel — a query field and three toggles above an `NSOutlineView` of ripgrep-backed results, grouped as matched files with their matching lines beneath. Line windowing, the byte-to-UTF-16 span mapping, and the char budget live in wolf-core's `search_tree` and the `WolfKeyMap` seams, not here.

**Public API**

- `init(handle:paneContainer:)`, `seedToggles()`, `focusSearchField()`, `setSidebarVisible(_:)`, `scheduleRefresh()`, `refresh()`, `close()`.
- `SearchFileItem` / `SearchLineItem` — the outline's row models. `absPath` is the identity `NSOutlineView` keys expansion and selection on.

**Decisions**

- Searching is wired to the field's action (Return) and `controlTextDidChange` is deliberately left unwired, because each run spawns an rg subprocess. Assigning `search` replaces the session, which cancels the previous subprocess.
- `focusSearchField()` defers `makeFirstResponder` by one runloop tick. Invoked from the Command Palette, the palette's `close()` runs afterwards and ends in `makeFirstResponder(paneContainer)`, clobbering a synchronous focus. Other entry points have no competing refocus, so the one deferral serves every path.
- `refresh()` rebuilds only when the snapshot version or the char budget has advanced, so repeated batches and idle wakes cost nothing. The budget is part of the gate so a sidebar resize re-windows the lines.
- Collapsed files are tracked by path so a rebuild preserves them; every other file row expands.
- Activating a line row routes on the active pane's mode, as the pickers do: a preview pane gets `preview_file`, falling back to `open_at` on `0`. A preview has no cursor, so the match's line and column reach nvim only on the edit path.

#### `SearchWakeBridge.swift`

Purpose: Installs the process-global content-search wake so each streamed ripgrep batch reaches every attached view's `SearchView`.

**Public API**

- `install()` — called once at launch.

**Decisions**

- The callback closure captures nothing, so it converts to a `@convention(c)` function pointer; the per-project token is resolved through `WindowRegistry` instead.
- The wake fans out to all of a project's views rather than just the current one, because each view owns an independent `start_search` session and the token carries no view discriminator. Idle panels no-op on their own version gate.

**Gotchas**

- Fired from a tokio worker thread; the thunk hops to the main thread before touching AppKit.

#### `SidebarContainerView.swift`

Purpose: The sidebar body — a header strip whose folder and magnifier toggles swap the content area between `FileTreeView` and `SearchView`. Adopted directly as the sidebar's hosted view.

**Public API**

- `init(tree:search:)`, plus `tree` and `search` so the controller can reach either child.
- `mode`, `setMode(_:)`, and `setSidebarVisible(_:)`, which is where the split item's `isCollapsed` KVO is relayed.

**Decisions**

- Both children stay in the view hierarchy and the inactive one is hidden, so switching modes preserves each panel's state.
- `setSidebarVisible` passes `visible && mode == …` down, so the inactive child's refresh gate stays closed and it does no work while off-screen.

**Gotchas**

- `setMode(_:)` only swaps which child is shown. Seeding the search toggles and focusing its field is the caller's job; the header magnifier button does both.

#### `StatePersistence.swift`

Purpose: Watches one window's geometry, sidebar collapse state, and tab-group membership/selection, calling an injected `onChange` on every change. The owning `WolfWindowController` wires that closure to the coordinator's coalesced whole-tree snapshot, so `state.json` tracks the live UI.

**Public API**

- `init(window:sidebarItem:onChange:)` — every UI reference is weak; the controller owns those lifetimes.
- `install()` — arms the observers. They are torn down in `deinit`, so the object must be kept alive for as long as the window.

**Decisions**

- Trigger-only glue: no file IO, no per-field state writes. Persistence is a whole-tree snapshot the coordinator walks, so writing anything here would duplicate that walk rather than extend it.
- The `didResize` observer bails while `inLiveResize`. A live drag emits it once per frame, and `didEndLiveResize` already catches the final size; the `didResize` registration exists only for discrete resizes such as zoom and a programmatic `setFrame`.

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
- `acceptsFirstResponder` is false: the container decides whether a wheel gesture reaches `scrollBy`. Scrolling while hovering the toast is the single gesture that must not reach Neovim, because any input dismisses the toast core-side — making this view a responder-chain scroll target would defeat that.
- The scroll offset resets only when the line count changes, so a repeat wake carrying the same message keeps the user's position.

#### `TreeWakeBridge.swift`

Purpose: Registers the file-tree traversal wake with the Rust core at launch and routes each wake to the owning project's sidebars.

Shared `*WakeBridge` shape: core fires the registered `extern "C"` callback on a tokio worker thread carrying its per-project token; the thunk hops to the main thread, resolves the token through `WindowRegistry`, and calls one method on the resulting `Project`.

**Public API**

- `install()` — call once per process, at launch.

**Decisions**

- The thunk closure captures nothing but the global `WindowRegistry`. That is what lets it convert to a `@convention(c)` function pointer; capturing any per-window state breaks the conversion outright.
- The wake fans out to every attached view's tree, not just the current one, because the token is per-project and carries no view discriminator while each view runs its own independent `start_tree_search()` traversal.
- Each view is asked for a coalesced refresh, so a large directory's batch wakes collapse to at most one reload per frame.

**Gotchas**

- A wake arriving after a tree has closed is harmless — the refresh is a no-op when closed.

#### `WakeBridge.swift`

Purpose: Registers the editor render wake with the Rust core at launch, driving a repaint of the right window. Same thunk shape as `TreeWakeBridge`.

**Public API**

- `install()` — call once per process, at launch.

**Decisions**

- Routes to the project's current view only, and immediately rather than coalesced: this wake is the frame. Non-focused views of the same project hold their last painted frame until refocused.

#### `WindowRegistry.swift`

Purpose: The process-global map from a core's per-project wake token to the `Project` that owns it. The `*WakeBridge` thunks carry only a plain `u64` across the thread hop, and resolve it here.

**Public API**

- `register(_:token:)` / `unregister(token:)` / `project(for:)` — all main-thread only.

**Decisions**

- Unsynchronized on purpose. Every access is already on the main thread: registration at project start, lookup inside the dispatched-to-main wake, removal at last-view shutdown. Adding a lock would imply it is safe to call from the wake's own tokio thread, which it is not.

**Gotchas**

- `project(for:)` returns `nil` for a wake that arrives after its project's last view closed; every caller treats that as a no-op rather than an error.

#### `WolfWindowController.swift`

Purpose: One view — a single tab's whole UI stack, bound to a `(project, tabpageHandle)` pair. A tab is its own OS window; several may share a native tab group.

**Public API**

- `build() -> Bool` — returns `false`, having built nothing, when the tabpage bind is invalid, so the coordinator drops the would-be view.
- `hasFolder` — false in the empty state, gating the sidebar and file tree, since an empty-state window must never scan `home`.
- `persistedCwd` — the tabpage-local cwd, correct even when this view is not current, unlike `currentCwd`, which follows whichever tabpage is current.
- `canCopy` — whether Edit > Copy has anything to copy; `copy()` takes the same decision.

**Decisions**

- `project` is held strong so the shared core outlives this view's teardown while a sibling still binds it; the project holds views weakly, so there is no cycle. A view never shuts the core down itself.
- `teardown()` is idempotent and defers `removeController` one runloop turn, capturing `self` strongly on purpose: during `windowWillClose` the coordinator's collection is the last strong reference, so dropping it synchronously frees the `NSWindow` mid-close.
- `window.historyNav = self` is wired explicitly: this is not an `NSWindowController`, so a `target: nil` toolbar action could never reach it through the responder chain.
- The file tree starts at the end of `build()`, after the sidebar's collapse state has seeded the tree's refresh gate, so a restored collapsed sidebar defers its first outline work instead of reloading while hidden.

#### `main.swift`

Purpose: Process entry point — builds the shared `NSApplication`, installs `AppDelegate`, sets the regular activation policy, and runs.
