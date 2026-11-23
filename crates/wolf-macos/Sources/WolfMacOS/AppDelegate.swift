// The application coordinator: launch setup, the window controllers, and Cmd-N.

import AppKit
import Foundation
import WolfFFI
import WolfKeyMap

/// The application delegate, a thin COORDINATOR over a collection of per-TAB
/// `WolfWindowController`s (each its own OS window; several may share one native
/// tab group), each running its own captive Neovim. It performs the once-only
/// launch setup, restores the saved window/tab tree, opens new windows (⇧⌘N) and
/// tabs (⌘T / the native "+"), reports which tab is focused to the tabs' Neovims,
/// owns the coalesced whole-tree state snapshot, and refreshes the tab-bar titles.
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuItemValidation, NSMenuDelegate {
  /// The once-measured shared cell metrics (nil until launch).
  private var metrics: CellMetrics?
  /// The once-loaded shared user config (nil until launch).
  private var config: FfiConfig?

  /// The resolved appearance as a boolean (`true` == dark), computed by
  /// `applyResolvedAppearance()` from the config's `appearance` mode. In `auto`
  /// mode this tracks the live OS appearance; in `dark`/`light` it is fixed.
  /// Every tab's nvim is driven to this via `setBackground`.
  private var resolvedDark: Bool = true

  /// The `NSApp.effectiveAppearance` KVO token, installed ONLY in `auto` mode so
  /// an OS dark/light toggle re-resolves + re-pushes live. Must be stored — an
  /// `NSKeyValueObservation` invalidates itself when deallocated.
  private var appearanceObservation: NSKeyValueObservation?

  /// The live per-TAB controllers, each owning its tab's `TabCore`. One OS window
  /// per controller; several controllers may belong to one native tab group.
  private var controllers: [WolfWindowController] = []

  /// The tab whose Neovim was last told it has focus (`refreshTabFocus()`), or `nil`.
  private var focusedTabId: Int?

  /// The main window's tab at the last `refreshTabFocus()`, whose panes are captured when
  /// another tab becomes main.
  private var lastMainTabId: Int?

  /// The next monotonic tab id to mint: the Window-menu / raise key and the stacking
  /// order's tab id.
  private var nextTabId = 0

  /// The window stacking order and tab recency (`specs/application-state.md` §3), fed by
  /// main-window changes and minimize events and seeded once the launch restore has
  /// assembled its windows.
  private var stacking = StackingOrder()

  /// Throttles whole-tree snapshots: the first request arms one walk + commit
  /// `snapshotCoalesceDelay` later, and requests until then join it. A throttle, not a
  /// debounce, so a steady stream of wakes never starves the commit.
  private lazy var snapshotCoalescer = PickerRefreshCoalescer(
    schedule: { fire in
      DispatchQueue.main.asyncAfter(deadline: .now() + Self.snapshotCoalesceDelay, execute: fire)
    },
    perform: { [weak self] in self?.snapshotNow() })

  /// The whole-tree snapshot coalesce delay.
  private static let snapshotCoalesceDelay: TimeInterval = 0.15

  /// Set as `applicationWillTerminate` starts, so a view torn down from then on
  /// commits nothing over the terminate commit (`closeCommit(for:)`).
  private var isTerminating = false

  /// The saved windows, or the saved tabs of a window, that no live view took at
  /// launch (a failed core start). Never retried, but carried into
  /// every commit so the next launch can try them again.
  private var orphanWindows: [WindowStateCodec.Window] = []

  /// The Window submenu (returned by `AppMenu.install`), rebuilt with the
  /// dynamic per-window list on window open/close/cwd-change. `nil` until launch.
  private var windowMenu: NSMenu?

  /// The File > Open Recent submenu (returned by `AppMenu.install`), rebuilt
  /// lazily in `menuNeedsUpdate` from the recent-dirs MRU. `nil` until launch.
  private var openRecentMenu: NSMenu?

  /// The recent-dirs list backing the current Open Recent submenu, captured in
  /// `menuNeedsUpdate` so `openRecentMenuItem(_:)` can resolve `tag` -> path.
  private var recentDirs: [String] = []

  /// The History submenu (returned by `AppMenu.install`), rebuilt lazily in
  /// `menuNeedsUpdate` from the key window's ACTIVE pane. Wolf holds no copy of the
  /// history — core is the single source of truth. `nil` until launch.
  private var historyMenu: NSMenu?

  /// The key window as far as owning-controller lookups are concerned. While a
  /// picker panel is key, `NSApp.keyWindow` is the borderless child panel (which no
  /// controller owns); the panel is a child window of its `MainWindow`, so resolve
  /// to that parent. Otherwise it is the key window itself.
  private var effectiveKeyWindow: NSWindow? {
    let key = NSApp.keyWindow
    return key?.parent ?? key
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    // Arm frame-time instrumentation first, so the once-only `FrameMetrics.enabled` read happens
    // here rather than mid-frame.
    FrameMetrics.start()

    // Register the three process-global wake callbacks ONCE; each routes by
    // the firing core's token through `WindowRegistry`.
    WakeBridge.install()
    PickerWakeBridge.install()
    TreeWakeBridge.install()
    SearchWakeBridge.install()

    // Keep all three dynamic submenus: Window rebuilds from lifecycle hooks, while Open Recent and
    // History rebuild lazily in `menuNeedsUpdate`, which dispatches on menu identity.
    let menus = AppMenu.install(target: self)
    windowMenu = menus.window
    openRecentMenu = menus.openRecent
    openRecentMenu?.delegate = self
    // The History submenu rebuilds the same lazy way, from the key window's
    // active pane, so it always reflects the pane the user is looking at.
    historyMenu = menus.history
    historyMenu?.delegate = self

    // Register fonts and load the config before measuring, so `CellMetrics` resolves the configured
    // family, size and line height. Measured once and shared by every window.
    FontRegistration.registerBundledFonts()
    let config = wolf_ffi_load_config()
    let metrics = CellMetrics.measured(config: config)
    self.config = config
    self.metrics = metrics

    // Resolve the appearance before any core starts, so every core below gets the value; `auto`
    // follows and observes the live OS appearance.
    applyResolvedAppearance()

    // Read the window state before any core starts. A fresh run restores nothing and opens one
    // empty-state window.
    let loaded = WindowStateCodec.load(wolf_ffi_app_state_window_state())

    if loaded.isEmpty {
      // Fresh first run: always the empty state (no folder), nvim at home.
      guard let controller = startEmptyStateWindow() else {
        presentStartupFailureAndQuit()
        return
      }
      controllers.append(controller)
      refreshTabFocus()
      rebuildWindowMenu()
      scheduleStateSnapshot()
      return
    }

    // Plan one tab, with its own Neovim, per saved tab, materialize them, then assemble the OS
    // windows and tab groups in stacking order, which seeds the live stacking order.
    let plan = RestoreSeam.plan(
      windows: loaded.map { RestoreSeam.SavedWindow(tabs: $0.tabs.map(\.saved)) },
      home: homeDir(),
      isDirectory: isDirectoryCheck())
    let byGroup = materializeTabs(plan, windows: loaded)
    stacking = StackingOrder(seed: assembleWindows(byGroup, windows: loaded))
    orphanWindows = Self.unmaterializedWindows(byGroup, windows: loaded)

    // Every restored core failed (edge case): quit rather than run windowless.
    if controllers.isEmpty {
      presentStartupFailureAndQuit()
      return
    }
    refreshTabFocus()
    rebuildWindowMenu()
    scheduleStateSnapshot()
  }

  /// PHASE A — starts a core and builds a controller for every planned tab
  /// WITHOUT anchoring or tab-grouping them yet. Returns a map from the ORIGINAL
  /// saved window index to the window's `(positionInGroup, controller)` members, so
  /// Phase B can drop each view back into the right OS window/tab-group. A tab
  /// whose core fails to start is skipped and stays an orphan. Only a tab planned
  /// with a folder rebuilds its saved layout; one whose directory is missing shows
  /// the empty state and keeps its saved tab in its cache.
  private func materializeTabs(
    _ plan: [RestoreSeam.PlannedTab], windows: [WindowStateCodec.LoadedWindow]
  ) -> [Int: [(pos: Int, controller: WolfWindowController)]] {
    var byGroup: [Int: [(pos: Int, controller: WolfWindowController)]] = [:]
    for tab in plan {
      guard
        let loadedTab = windows[safe: tab.groupIndex]?.tabs[safe: tab.positionInGroup],
        let tabCore = startTabCore(launchCwd: tab.launchCwd)
      else { continue }
      let restoredTab = RestoredTabState(
        saved: loadedTab.saved, rebuild: tab.hasFolder ? loadedTab.rebuild : nil)
      let controller = makeController(
        tabId: mintTabId(), tabCore: tabCore, restoredTab: restoredTab, placement: .system,
        hasFolder: tab.hasFolder)
      controller.build()
      controllers.append(controller)
      byGroup[tab.groupIndex, default: []].append((tab.positionInGroup, controller))
    }
    return byGroup
  }

  /// The saved tabs no view took in `byGroup`, each saved window's in saved order
  /// under that window's frame, active tab and minimized flag (`SessionSeam.orphanedSlots`).
  private static func unmaterializedWindows(
    _ byGroup: [Int: [(pos: Int, controller: WolfWindowController)]],
    windows: [WindowStateCodec.LoadedWindow]
  ) -> [WindowStateCodec.Window] {
    var materialized = Set<SessionSeam.TabSlot>()
    for (groupIndex, members) in byGroup {
      for member in members {
        materialized.insert(
          SessionSeam.TabSlot(groupIndex: groupIndex, positionInGroup: member.pos))
      }
    }
    let orphans = SessionSeam.orphanedSlots(
      savedTabCounts: windows.map { $0.tabs.count }, materialized: materialized)
    return orphans.map { orphan in
      let window = windows[orphan.groupIndex]
      return WindowStateCodec.Window(
        frame: window.frame ?? .zero,
        activeTab: orphan.positions.firstIndex(of: window.activeTab) ?? 0,
        minimized: window.minimized, tabs: orphan.positions.map { window.tabs[$0].saved },
        recency: [])
    }
  }

  /// One assembled window: its members in tab order, the selected one's index, and
  /// whether it restores minimized.
  private struct AssembledWindow {
    let members: [WolfWindowController]
    let selected: Int
    let minimized: Bool
  }

  /// PHASE B — assembles the OS windows/tab-groups: for each saved window in
  /// stacking order, anchors the first surviving view at the saved frame when one
  /// intersects a screen, `addTabbedWindow`s the rest and selects the saved active
  /// tab. It then orders the windows so the first is frontmost and key, minimizes
  /// the minimized ones, and returns the stacking-order seed. A window with no
  /// surviving view is dropped.
  private func assembleWindows(
    _ byGroup: [Int: [(pos: Int, controller: WolfWindowController)]],
    windows: [WindowStateCodec.LoadedWindow]
  ) -> [StackingOrder.SeedWindow] {
    var assembled: [AssembledWindow] = []
    for groupIndex in byGroup.keys.sorted() {
      guard let saved = windows[safe: groupIndex] else { continue }
      let members = byGroup[groupIndex]!.sorted { $0.pos < $1.pos }
      guard let anchor = members.first?.controller, let anchorWindow = anchor.nsWindow
      else { continue }

      if let frame = saved.frame,
        let anchorFrame = StateSeam.restoredWindowFrame(
          x: frame.origin.x, y: frame.origin.y, width: frame.size.width,
          height: frame.size.height, screenVisibleFrames: NSScreen.screens.map(\.visibleFrame))
      {
        anchorWindow.setFrame(anchorFrame, display: false)
      }

      for member in members.dropFirst() {
        if let newWindow = member.controller.nsWindow {
          anchorWindow.addTabbedWindow(newWindow, ordered: .above)
        }
      }
      let positions = members.map(\.pos)
      let selected = RestoreSeam.selectedPosition(active: saved.activeTab, materialized: positions)
        .flatMap { positions.firstIndex(of: $0) }
      assembled.append(
        AssembledWindow(
          members: members.map(\.controller), selected: selected ?? 0,
          minimized: saved.minimized))
    }

    // Back to front, each window's selected tab in front of its group, so the first ends
    // frontmost; then the minimized windows leave, and the first other one takes key.
    for window in assembled.reversed() {
      guard let selected = window.members[window.selected].nsWindow else { continue }
      selected.tabGroup?.selectedWindow = selected
      selected.orderFront(nil)
    }
    for window in assembled where window.minimized {
      window.members[window.selected].nsWindow?.miniaturize(nil)
    }
    if let front = assembled.first(where: { !$0.minimized }) {
      front.members[front.selected].nsWindow?.makeKeyAndOrderFront(nil)
    }
    return assembled.map {
      StackingOrder.SeedWindow(
        tabs: $0.members.map(\.tabId), activeTab: $0.selected, minimized: $0.minimized)
    }
  }

  /// Starts a tab's captive Neovim at `launchCwd` and pushes the resolved appearance
  /// to it, so its first paint uses the right background (the config value is
  /// restart-only). `nil` when the core fails to start.
  private func startTabCore(launchCwd: String) -> TabCore? {
    guard let tabCore = TabCore.start(launchCwd: launchCwd, coordinator: self) else {
      return nil
    }
    tabCore.setBackground(dark: resolvedDark)
    return tabCore
  }

  /// Starts a fresh empty-state tab (nvim at home) in a window of its own at `placement`.
  /// Returns the view, or `nil` when the core fails to start. The caller appends it.
  private func startEmptyStateWindow(placement: WindowPlacement = .system)
    -> WolfWindowController?
  {
    guard let tabCore = startTabCore(launchCwd: homeDir()) else { return nil }
    let controller = makeController(
      tabId: mintTabId(), tabCore: tabCore, restoredTab: nil, placement: placement,
      hasFolder: false)
    controller.build()
    return controller
  }

  /// The user's home directory path (the empty-state launch cwd).
  private func homeDir() -> String {
    FileManager.default.homeDirectoryForCurrentUser.path
  }

  // MARK: - Appearance (OS dark/light resolution + observation)

  /// The `NSApp.appearance` that `mode` forces on the whole app: Dark Aqua or
  /// Aqua for a forced mode, `nil` (follow the system) for `auto`. Every window,
  /// and so its title, inherits it.
  static func appAppearance(for mode: AppearanceMode) -> NSAppearance? {
    mode.forcedDark.flatMap { NSAppearance(named: $0 ? .darkAqua : .aqua) }
  }

  /// Resolves the configured appearance to `resolvedDark` and applies it to the
  /// app chrome, then pushes it to every live tab. It forces
  /// `NSApp.appearance` (the whole app, not just the grid) from the mode, and
  /// installs the OS-toggle observer only in `auto`, which follows the system.
  /// An unknown discriminant falls back to `auto`.
  private func applyResolvedAppearance() {
    let raw = config?.appearance() ?? AppearanceMode.auto.rawValue
    let mode = AppearanceMode(rawValue: raw) ?? .auto
    NSApp.appearance = Self.appAppearance(for: mode)
    // Read after the assignment: in `auto` this is the system's; a forced mode ignores it.
    let systemDark = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
    resolvedDark = mode.resolvesDark(systemDark: systemDark)
    if mode == .auto {
      installAppearanceObserverIfNeeded()
    } else {
      removeAppearanceObserver()
    }
    pushAppearanceToAllTabs()
  }

  /// Installs the `NSApp.effectiveAppearance` KVO observer (auto mode only), if
  /// not already installed. The token is STORED (`appearanceObservation`) — an
  /// `NSKeyValueObservation` invalidates itself once deallocated.
  private func installAppearanceObserverIfNeeded() {
    guard appearanceObservation == nil else { return }
    appearanceObservation = NSApp.observe(\.effectiveAppearance, options: [.new]) {
      [weak self] _, _ in
      self?.systemAppearanceChanged()
    }
  }

  /// Tears down the appearance observer (fixed dark/light modes, or app teardown).
  private func removeAppearanceObserver() {
    appearanceObservation?.invalidate()
    appearanceObservation = nil
  }

  /// The `auto`-mode observer callback: re-resolves the live OS appearance and,
  /// when it actually changed, re-pushes it to every tab. No debounce:
  /// the KVO fires once per real OS toggle and re-applying an unchanged background
  /// is a harmless no-op.
  private func systemAppearanceChanged() {
    let nowDark = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
    guard nowDark != resolvedDark else { return }
    resolvedDark = nowDark
    pushAppearanceToAllTabs()
  }

  /// Fans the resolved appearance out to every live tab's nvim (`:set
  /// background=…`). A no-op before any tab exists (e.g. the launch call).
  private func pushAppearanceToAllTabs() {
    for controller in controllers where !controller.isTornDown {
      controller.tabCore.setBackground(dark: resolvedDark)
    }
  }

  /// Recomputes which tab is focused and reports the change to the tabs' Neovims
  /// (`TabFocusSeam`): a tab is focused while the app is active and its window is
  /// main, so a gain fires `FocusGained` and the `WolfAutoRead` `:checktime` reloads
  /// a file another tab saved. Main rather than key, so a picker panel taking key
  /// reports nothing.
  ///
  /// The main window's tab also moves to the front of the stacking order, which
  /// schedules a snapshot when it changes the order. When the main window's tab changes,
  /// the tab switched away from captures its panes' cursors and scroll first (§4); the
  /// app resigning active leaves the main tab as it is, so it captures nothing.
  func refreshTabFocus() {
    let mainTab = controllers.first { !$0.isTornDown && $0.owns(NSApp.mainWindow) }?.tabId
    if let mainTab {
      if let previous = lastMainTabId, previous != mainTab {
        controllers.first { $0.tabId == previous }?.captureViewports()
      }
      lastMainTabId = mainTab
      let before = stacking
      stacking.focus(mainTab)
      if stacking != before { scheduleStateSnapshot() }
    }
    let next = TabFocusSeam.focusedTab(appActive: NSApp.isActive, mainTab: mainTab)
    for edge in TabFocusSeam.edges(from: focusedTabId, to: next) {
      let tab = controllers.first { $0.tabId == edge.tab && !$0.isTornDown }
      tab?.tabCore.setUiFocus(gained: edge.gained)
    }
    focusedTabId = next
  }

  /// A FileManager-backed `isDirectory` check for `RestoreSeam`/`StateSeam`.
  private func isDirectoryCheck() -> (String) -> Bool {
    { path in
      var isDir: ObjCBool = false
      return FileManager.default.fileExists(atPath: path, isDirectory: &isDir)
        && isDir.boolValue
    }
  }

  /// Mints the next monotonic tab id.
  private func mintTabId() -> Int {
    let id = nextTabId
    nextTabId += 1
    return id
  }

  /// The app regained OS focus (e.g. Dock reactivation, Cmd-Tab back): its main
  /// window's tab gains focus, so an edit made in another app reloads there.
  func applicationDidBecomeActive(_ notification: Notification) {
    refreshTabFocus()
  }

  /// The app lost OS focus: the focused tab loses it. `didResign` rather than
  /// `willResign`, so `NSApp.isActive` already reads `false`.
  func applicationDidResignActive(_ notification: Notification) {
    refreshTabFocus()
  }

  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    true
  }

  func applicationWillTerminate(_ notification: Notification) {
    isTerminating = true

    // Flush the final partial frame-stats interval. Best-effort: this does not
    // run on a force-quit. No-op unless WOLF_FRAME_STATS is set.
    FrameMetrics.emit()

    // Stop observing the OS appearance before teardown.
    removeAppearanceObserver()

    // Capture every tab, then commit synchronously, as a coalesced snapshot may not have fired,
    // and flush before any teardown shuts a Neovim down (§4). Teardown is also the backstop.
    SessionSeam.terminate(
      controllers, capture: { $0.captureViewports() }, commit: { snapshotNow() },
      flush: { _ = wolf_ffi_app_state_flush() }, teardown: { $0.teardown() })
  }

  /// The Cmd-N (⇧⌘N) entry point: opens a new STANDALONE window. Cmd-N ALWAYS
  /// opens the empty state (no folder), regardless of the originating window's
  /// cwd — nvim launches at home, the sidebar is hidden, and the centered "Open
  /// folder…" overlay is shown. `originatingCwd` is ignored (kept as a label for
  /// the shared dispatcher call site). The window takes the active window's normal
  /// size and cascades from it (`newWindowPlacement`), and inherits nothing else.
  func makeNewWindow(originatingCwd _: String) {
    // Metrics/config are set at launch before any window opens; bail if not.
    guard metrics != nil, config != nil else { return }
    // A new window is a new tab with its own fresh nvim, in the empty state.
    let placement = newWindowPlacement(stored: nil)
    guard let controller = startEmptyStateWindow(placement: placement) else { return }
    controllers.append(controller)
    // Its window became main before it joined `controllers`, so that refresh missed it.
    refreshTabFocus()
    rebuildWindowMenu()
    scheduleStateSnapshot()
  }

  /// The ⌘T / `:WolfNewTab` entry point: opens a new TAB in `key`'s window
  /// group, inheriting `key`'s open folder (a new tab ≈ a new window in the same
  /// directory) in a Neovim of its own, launched there, or at home when `key` has
  /// no folder. The new window is `addTabbedWindow`-ed into `key`'s group and
  /// selected. A no-op before launch or if the core fails to start.
  func makeNewTab(from key: WolfWindowController) {
    guard metrics != nil, config != nil else { return }
    let inheritedCwd = key.hasFolder ? key.currentCwd : ""
    let (launchCwd, hasFolder) = Self.dir(launchDecision(restoredCwd: inheritedCwd))
    guard let tabCore = startTabCore(launchCwd: launchCwd) else { return }
    let controller = makeController(
      tabId: mintTabId(), tabCore: tabCore, restoredTab: nil, placement: .system,
      hasFolder: hasFolder)
    controller.build()
    if let keyWindow = key.nsWindow, let newWindow = controller.nsWindow {
      keyWindow.addTabbedWindow(newWindow, ordered: .above)
      newWindow.makeKeyAndOrderFront(nil)
    }
    controllers.append(controller)
    // Its window became main before it joined `controllers`, so that refresh missed it.
    refreshTabFocus()
    rebuildWindowMenu()
    scheduleStateSnapshot()
  }

  /// The native tab bar's "+" button forwards here (NSApp → delegate). Opens a
  /// new tab in the key window's group, falling back to the first controller.
  @objc func newWindowForTab(_ sender: Any?) {
    let key = controllers.first { $0.owns(effectiveKeyWindow) } ?? controllers.first
    guard let key else { return }
    makeNewTab(from: key)
  }

  /// The native menu-item command entry point: decodes the item's `tag` (an FFI
  /// `command_kind` discriminant, set by `AppMenu`) into a `CommandKind` and
  /// funnels through `CommandDispatcher`, targeting the current key window — the
  /// SAME dispatcher the nvim-drained command path uses. An unknown tag is
  /// ignored.
  @objc func handleCommandMenuItem(_ sender: NSMenuItem) {
    guard let kind = CommandKind(ffi: UInt8(truncatingIfNeeded: sender.tag)) else { return }
    let key = effectiveKeyWindow
    let target = controllers.first { $0.owns(key) }
    let cwd = target?.currentCwd ?? ""
    CommandDispatcher.dispatch(
      kind, window: key, coordinator: self, originatingCwd: cwd, controller: target)
  }

  /// Drops `controller` from the collection (called by the controller at
  /// teardown / window close), then rebuilds the dynamic Window menu.
  func removeController(_ controller: WolfWindowController) {
    controllers.removeAll { $0 === controller }
    stacking.remove(controller.tabId)
    rebuildWindowMenu()
  }

  /// Notified by a controller when its window was minimized or restored, which acts on
  /// its whole tab group: the group's tabs are marked in the stacking order, and a
  /// snapshot is scheduled.
  func windowMinimizedChanged(_ controller: WolfWindowController, minimized: Bool) {
    guard let window = controller.nsWindow else { return }
    let tabs = (window.tabGroup?.windows ?? [window]).compactMap { tabWindow in
      controllers.first { !$0.isTornDown && $0.owns(tabWindow) }?.tabId
    }
    if minimized {
      stacking.minimize(tabs)
    } else {
      stacking.deminimize(tabs)
    }
    scheduleStateSnapshot()
  }

  /// Rebuilds the dynamic Window-menu per-window list from the live controllers.
  /// A no-op before the menu is installed at launch.
  private func rebuildWindowMenu() {
    guard let windowMenu else { return }
    AppMenu.rebuildWindowMenu(windowMenu, controllers: controllers, target: self)
  }

  /// The dynamic Window-menu item action: raises the tab whose `tabId` equals
  /// the item's `tag`.
  @objc func raiseWindowMenuItem(_ sender: NSMenuItem) {
    controllers.first { $0.tabId == sender.tag }?.raise()
  }

  /// View > Project: reveals the key window's sidebar and switches it
  /// to the file tree (a no-op checkmark is applied in `validateMenuItem`).
  @objc func showProjectTreeMenuItem(_ sender: NSMenuItem) {
    controllers.first { $0.owns(effectiveKeyWindow) }?.showSidebar(panel: .tree)
  }

  /// View > Search: reveals the key window's sidebar and switches it to the
  /// content-search panel.
  @objc func showSearchMenuItem(_ sender: NSMenuItem) {
    controllers.first { $0.owns(effectiveKeyWindow) }?.showSidebar(panel: .search)
  }

  /// The About panel's preamble, Linux's About dialog sentence verbatim.
  private static let aboutPreamble =
    "A Markdown editor that embeds Neovim as its editing engine."

  /// Wolf > About Wolf: the standard About panel whose credits are the preamble, the
  /// bundled fonts' licence notices (`wolf_ffi_font_license_notices()`) and then the
  /// linked libraries' (`wolf_ffi_library_license_notices()`).
  /// `.foregroundColor` must stay `labelColor`: an attributed credits string with no
  /// colour renders black on the dark panel in dark mode.
  @objc func showAboutPanel(_ sender: NSMenuItem) {
    let credits = NSAttributedString(
      string: Self.aboutPreamble + "\n\n" + wolf_ffi_font_license_notices().toString() + "\n"
        + wolf_ffi_library_license_notices().toString(),
      attributes: [
        .font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize),
        .foregroundColor: NSColor.labelColor,
      ])
    NSApp.orderFrontStandardAboutPanel(options: [.credits: credits])
  }

  /// The OTHER open tabs (all live controllers EXCEPT `excluding`), as
  /// `(tabId, currentCwd)` pairs — the command palette's "Switch to Window"
  /// candidate source. The current tab is excluded (it cannot switch to itself).
  func otherWindows(excluding: WolfWindowController) -> [(index: Int, cwd: String)] {
    controllers
      .filter { $0 !== excluding }
      .map { (index: $0.tabId, cwd: $0.persistedCwd) }
  }

  /// Open Folder, for every entry point: applies `specs/application-state.md` §6 to `path`,
  /// from `current`, the key window's view, `nil` when no Wolf window is key.
  ///
  /// `OpenFolderSeam.step` decides over every live tab's directory. A focus raises the tab §4's
  /// rule picks; a fill replaces the empty current tab with one from the folder's project state;
  /// otherwise a new window opens from it. The landed tab's core records the folder in the
  /// recent directories, since its Neovim starts there and core records no launch directory.
  func openFolder(_ path: String, from current: WolfWindowController?) {
    guard metrics != nil, config != nil else { return }
    let directory = OpenFolderSeam.canonicalDirectory(path)
    let windows = liveGroups().map { group in
      OpenFolderSeam.LiveWindow(
        tabs: group.members.map {
          OpenFolderSeam.LiveTab(id: $0.tabId, directory: $0.persistedCwd)
        },
        selected: group.selected?.tabId)
    }
    let invoking = current.flatMap { view in
      view.isTornDown ? nil : OpenFolderSeam.LiveTab(id: view.tabId, directory: view.persistedCwd)
    }
    let landed: WolfWindowController?
    switch OpenFolderSeam.step(
      directory: directory, current: invoking, windows: windows, stacking: stacking)
    {
    case .focus(let tab):
      landed = controllers.first { $0.tabId == tab && !$0.isTornDown }
      landed?.raise()
    case .fill:
      landed = current.flatMap { fill($0, from: directory) }
    case .newWindow:
      landed = openWindow(at: directory)
    }
    guard let landed else { return }
    refreshTabFocus()
    rebuildWindowMenu()
    scheduleStateSnapshot()
    landed.tabCore.handle.record_recent_dir(directory)
  }

  /// §6 step 2: opens a tab on `directory` from its project state at `empty`'s position in its
  /// window, selects it, then closes `empty`. The stored frame applies only when `empty` was its
  /// window's only tab. `nil`, with `empty` kept, when the core fails to start.
  ///
  /// A replacement rather than a chdir of the empty tab's Neovim: core's cwd moves only when
  /// Neovim reports it, so a rebuild racing the chdir would start its shells in the old
  /// directory, and a snapshot between the two would write the empty tab over the project state.
  private func fill(_ empty: WolfWindowController, from directory: String)
    -> WolfWindowController?
  {
    guard let emptyWindow = empty.nsWindow, let tabCore = startTabCore(launchCwd: directory)
    else { return nil }
    let windowTabs = emptyWindow.tabGroup?.windows.count ?? 1
    let project = WindowStateCodec.loadProject(wolf_ffi_app_state_project_state(directory))
    let controller = makeController(
      tabId: mintTabId(), tabCore: tabCore, restoredTab: Self.restoredTab(project, at: directory),
      placement: .system, hasFolder: true)
    controller.build()
    if let newWindow = controller.nsWindow {
      emptyWindow.addTabbedWindow(newWindow, ordered: .above)
      if let frame = OpenFolderSeam.fillFrame(
        stored: project?.frame, windowTabs: windowTabs,
        screenVisibleFrames: NSScreen.screens.map(\.visibleFrame))
      {
        newWindow.setFrame(frame, display: true)
      }
      newWindow.makeKeyAndOrderFront(nil)
    }
    controllers.append(controller)
    // Deferred: the dialog calls in from the empty window's own sheet completion.
    DispatchQueue.main.async { emptyWindow.close() }
    return controller
  }

  /// §6 step 3: a new window holding one tab on `directory`, from its project state when it has
  /// one, else a fresh tab. It opens at the stored frame when that intersects a screen, else as
  /// `newWindowPlacement` places a new window. `nil` when the core fails to start.
  private func openWindow(at directory: String) -> WolfWindowController? {
    guard let tabCore = startTabCore(launchCwd: directory) else { return nil }
    let project = WindowStateCodec.loadProject(wolf_ffi_app_state_project_state(directory))
    let placement = newWindowPlacement(stored: project?.frame)
    let controller = makeController(
      tabId: mintTabId(), tabCore: tabCore, restoredTab: Self.restoredTab(project, at: directory),
      placement: placement, hasFolder: true)
    controller.build()
    controllers.append(controller)
    return controller
  }

  /// The restored tab a project state opens as, on `directory`; `nil` with no project state.
  private static func restoredTab(
    _ project: WindowStateCodec.LoadedProject?, at directory: String
  ) -> RestoredTabState? {
    guard let project else { return nil }
    let saved = project.tab.saved
    return RestoredTabState(
      saved: RestoreSeam.SavedTab(
        cwd: directory, sidebarVisible: saved.sidebarVisible, layout: saved.layout,
        focused: saved.focused, sidebar: saved.sidebar, drawer: saved.drawer),
      rebuild: project.tab.rebuild)
  }

  /// Raises the tab whose `tabId` equals `tabId` (the palette's "Switch to
  /// Window" action). A no-op when no such tab is open.
  func raiseWindow(tabId: Int) {
    controllers.first { $0.tabId == tabId }?.raise()
  }

  /// Schedules a throttled whole-tree state snapshot (`snapshotCoalescer`): a burst
  /// of triggers collapses into a single tree walk + commit, and the store skips a
  /// tree identical to the last. Every Alive core wake routes here, as do geometry,
  /// sidebar, cwd, tab open/close and tear-off/merge changes.
  func scheduleStateSnapshot() {
    snapshotCoalescer.request()
  }

  /// Groups the view closes of one runloop turn (`SessionSeam.CloseBurst`); a keep
  /// commits the tree read at the burst's first close.
  private lazy var closeBurst = SessionSeam.CloseBurst<[WindowStateCodec.Window]?>(
    scheduleEnd: { DispatchQueue.main.async(execute: $0) },
    read: { [weak self] in self?.gatherTree() },
    commit: { [weak self] windows in
      if let windows { self?.commitTree(windows) }
    })

  /// What closing `view` commits, asked by its `teardown` while it still counts as
  /// live. Closing the last live view quits the app, so it commits synchronously,
  /// before returning, the tree read at the burst's first close: a tabbed window
  /// closed by its close button closes each tab in turn, and so keeps every tab.
  ///
  /// Outside a quit it first captures every live tab of the closing view's group, so the
  /// burst's read carries the cursors (§4). A drop then writes at once, while the closing
  /// view still counts, so its directory's project state keeps the capture.
  func closeCommit(for view: WolfWindowController) -> SessionSeam.CloseCommit {
    if !isTerminating, let window = view.nsWindow {
      let group = window.tabGroup?.windows ?? [window]
      for controller in controllers where !controller.isTornDown {
        if group.contains(where: { controller.owns($0) }) { controller.captureViewports() }
      }
    }
    let others = controllers.filter { $0 !== view && !$0.isTornDown }.count
    let decision = closeBurst.close(terminating: isTerminating, otherLiveViews: others)
    if decision == .dropSubtree { snapshotNow() }
    return decision
  }

  /// Walks the live window/tab tree and submits it to the window state, when
  /// `gatherTree()` answers a tree.
  func snapshotNow() {
    if let windows = gatherTree() { commitTree(windows) }
  }

  /// Walks the live window/tab tree into the windows a commit writes. Only views that
  /// are not torn down count. Windows are grouped by their native tab group (a solo
  /// window is its own group); tab order comes from `tabGroup.windows`. Each group's
  /// frame is its selected tab's normal frame, else any member's, else the selected
  /// window's frame. Every tab contributes its `persistedTab()`, and
  /// `SessionSeam.windowState` orders the windows by the stacking order and merges
  /// `orphanWindows` in. `SessionSeam.committedWindows` decides whether anything is
  /// committed at all: `nil` commits nothing.
  private func gatherTree() -> [WindowStateCodec.Window]? {
    let gathered = liveGroups().map { group in
      SessionSeam.LiveWindow(
        frame: group.normalFrame ?? group.selectedWindow.frame,
        tabs: group.members.map { SessionSeam.LiveTab(id: $0.tabId, tab: $0.persistedTab()) },
        selected: group.selected?.tabId)
    }
    return SessionSeam.committedWindows(
      liveViews: controllers.filter { !$0.isTornDown }.count,
      windows: SessionSeam.windowState(live: gathered, stacking: stacking, orphans: orphanWindows))
  }

  /// One live OS window: its views in tab order, its selected view and its selected window.
  private struct LiveGroup {
    let members: [WolfWindowController]
    let selected: WolfWindowController?
    let selectedWindow: NSWindow

    /// The selected view's recorded normal frame, else any member's.
    var normalFrame: CGRect? {
      selected?.normalFrame ?? members.lazy.compactMap(\.normalFrame).first
    }
  }

  /// Where a new window opens (`specs/application-state.md` §7): at `stored` when it intersects
  /// a screen, else at the active window's normal size cascaded from it, else as AppKit places
  /// it. The active window is the stacking order's first, never the key one. Called before the
  /// new controller builds, since `build()` makes the new window main.
  private func newWindowPlacement(stored: CGRect?) -> WindowPlacement {
    let groups = liveGroups()
    let active = stacking.order(groups.map { $0.members.map(\.tabId) }).first.map { groups[$0] }
    let window = active.map { group in
      StateSeam.ActiveWindow(
        normalFrame: group.normalFrame, frame: group.selectedWindow.frame,
        zoomed: group.selectedWindow.isZoomed,
        fullScreen: group.selectedWindow.styleMask.contains(.fullScreen))
    }
    switch StateSeam.newWindowFrame(
      stored: stored, active: window, screenVisibleFrames: NSScreen.screens.map(\.visibleFrame))
    {
    case .stored(let frame):
      return .frame(frame)
    case .cascaded(let topLeft, let size):
      return .cascaded(topLeft: topLeft, size: size)
    case .system:
      return .system
    }
  }

  /// The live views that are not torn down, grouped by their native tab group (a solo window is
  /// its own group), each group's members in `tabGroup.windows` order.
  private func liveGroups() -> [LiveGroup] {
    let live = controllers.filter { !$0.isTornDown }
    var groups: [LiveGroup] = []
    var seenGroups = Set<ObjectIdentifier>()
    for controller in live {
      guard let window = controller.nsWindow else { continue }
      let groupWindows: [NSWindow]
      if let tabGroup = window.tabGroup {
        guard seenGroups.insert(ObjectIdentifier(tabGroup)).inserted else { continue }
        groupWindows = tabGroup.windows
      } else {
        groupWindows = [window]
      }
      let members = groupWindows.compactMap { tabWindow in live.first { $0.owns(tabWindow) } }
      let selectedWindow = window.tabGroup?.selectedWindow ?? window
      groups.append(
        LiveGroup(
          members: members, selected: members.first { $0.owns(selectedWindow) },
          selectedWindow: selectedWindow))
    }
    return groups
  }

  /// Submits `windows` to the window state (`WindowStateCodec.submit`).
  private func commitTree(_ windows: [WindowStateCodec.Window]) {
    WindowStateCodec.submit(windows)
  }

  /// Refreshes the tab-bar titles for `window`'s whole tab group from each tab's
  /// working directory, abbreviating the home prefix to `~` via `CwdTitleSeam`.
  /// Each tab IS its own NSWindow, so setting `window.title` per tab drives both
  /// the tab label and that tab's titlebar; the tab's `representedURL` is set to
  /// the cwd directory so the proxy icon + command-click parent-hierarchy popup
  /// work. An empty-state tab (no folder) gets the "Wolf" fallback title and no
  /// proxy. A solo window is a one-element group.
  func refreshTabTitles(forGroupOf window: NSWindow) {
    let home = homeDir()
    let groupWindows = window.tabGroup?.windows ?? [window]
    for tabWindow in groupWindows {
      let cwd = controllers.first { $0.owns(tabWindow) }?.persistedCwd ?? ""
      let title = CwdTitleSeam.displayTitle(cwd: cwd, home: home)
      tabWindow.title = title.isEmpty ? "Wolf" : title
      tabWindow.representedURL = cwd.isEmpty ? nil : URL(fileURLWithPath: cwd, isDirectory: true)
    }
  }

  /// `NSMenuDelegate`: rebuilds a dynamic submenu just before it opens, reading
  /// from the key window's controller (falling back to the first controller when no
  /// Wolf window is key). Dispatches on menu IDENTITY; any other menu is ignored.
  ///
  /// * Open Recent -> the recent-dirs MRU.
  /// * History -> the ACTIVE pane's navigation rows (newest-first, ≤10 distinct,
  ///   one checkmark), pulled fresh from core on every open.
  func menuNeedsUpdate(_ menu: NSMenu) {
    let source = controllers.first { $0.owns(effectiveKeyWindow) } ?? controllers.first
    if menu === openRecentMenu {
      recentDirs = source?.recentDirs() ?? []
      AppMenu.rebuildOpenRecentMenu(menu, recentDirs: recentDirs, target: self)
      return
    }
    if menu === historyMenu {
      AppMenu.rebuildHistoryMenu(menu, rows: source?.historyRows() ?? [], target: self)
    }
  }

  /// History > Back: moves the key window's ACTIVE pane one entry back.
  @objc func historyBackMenuItem(_ sender: NSMenuItem) {
    let target = controllers.first { $0.owns(effectiveKeyWindow) } ?? controllers.first
    target?.historyBack()
  }

  /// History > Forward: moves the key window's ACTIVE pane one entry forward.
  @objc func historyForwardMenuItem(_ sender: NSMenuItem) {
    let target = controllers.first { $0.owns(effectiveKeyWindow) } ?? controllers.first
    target?.historyForward()
  }

  /// A dynamic History row: jumps the key window's ACTIVE pane to the STACK index
  /// carried in the item's `tag`.
  ///
  /// That `tag` is a stack index, NOT a `command_kind` discriminant — this distinct
  /// selector is what keeps it unambiguous. Clicking the CHECKMARKED row is a true
  /// no-op: its tag is the cursor's own index, which core rejects.
  @objc func historyJumpMenuItem(_ sender: NSMenuItem) {
    let target = controllers.first { $0.owns(effectiveKeyWindow) } ?? controllers.first
    target?.historyJump(index: sender.tag)
  }

  /// The dynamic Open Recent item action: Open Folder (`openFolder(_:from:)`) on the recent dir
  /// at the item's `tag` (an index into the `recentDirs` snapshot captured in
  /// `menuNeedsUpdate`), from the key window's view. Out-of-range tags are ignored.
  @objc func openRecentMenuItem(_ sender: NSMenuItem) {
    guard sender.tag >= 0, sender.tag < recentDirs.count else { return }
    openFolder(recentDirs[sender.tag], from: controllers.first { $0.owns(effectiveKeyWindow) })
  }

  /// Notified by a controller when its live cwd changed (its Window-menu title
  /// is the full cwd path), rebuilding the dynamic Window menu.
  func windowCwdDidChange(_ controller: WolfWindowController) {
    rebuildWindowMenu()
  }

  /// Menu validation. Two groups of front-end-only items (they carry no
  /// `command_kind` tag) are evaluated FIRST, before the
  /// `handleCommandMenuItem` guard:
  ///
  /// 1. The three **History** branches — Back / Forward are enabled from the
  ///    active pane's stack (`historyCanBack` / `historyCanForward`), and a
  ///    dynamic jump row is always enabled (the checkmarked one is a core-side
  ///    no-op).
  /// 2. The **sidebar** branch — Project / Search set their checkmark from the
  ///    pure `SidebarMenuSeam` (checked only when the sidebar is visible AND
  ///    showing that panel) and ALWAYS return `true`, since choosing either
  ///    while the sidebar is hidden reveals it.
  ///
  /// For tagged Wolf commands it then enables the Close (`.closePane`) command
  /// only when the key window's `canClosePane` holds (its active pane is the
  /// drawer, or a split that is not the tree's last leaf), disables the Copy
  /// (`.copy`) command unless the key window has something to copy from
  /// EITHER source — its active pane is a preview holding a non-empty
  /// selection, or the window is in visual mode — and disables the Preview
  /// (`.preview`) command unless the key window's active buffer is Markdown.
  /// Every other item stays enabled.
  func validateMenuItem(_ item: NSMenuItem) -> Bool {
    // History items carry no `command_kind` tag, so validate them before the guard. Keep the
    // `?? controllers.first` fallback: without it Back/Forward grey out when no Wolf window is key.
    switch item.action {
    case #selector(historyBackMenuItem(_:)):
      let target = controllers.first { $0.owns(effectiveKeyWindow) } ?? controllers.first
      return target?.historyCanBack ?? false
    case #selector(historyForwardMenuItem(_:)):
      let target = controllers.first { $0.owns(effectiveKeyWindow) } ?? controllers.first
      return target?.historyCanForward ?? false
    case #selector(historyJumpMenuItem(_:)):
      return true
    default:
      break
    }
    if item.action == #selector(showProjectTreeMenuItem(_:))
      || item.action == #selector(showSearchMenuItem(_:))
    {
      let c = controllers.first { $0.owns(effectiveKeyWindow) }
      let panel: SidebarPanel =
        item.action == #selector(showProjectTreeMenuItem(_:)) ? .tree : .search
      item.state =
        SidebarMenuSeam.isChecked(
          sidebarVisible: c?.sidebarVisible ?? false, current: c?.sidebarPanel, item: panel)
        ? .on : .off
      // Always enabled — choosing either while the sidebar is hidden reveals it.
      return true
    }
    guard item.action == #selector(handleCommandMenuItem(_:)) else { return true }
    let kind = CommandKind(ffi: UInt8(truncatingIfNeeded: item.tag))
    let keyController = controllers.first { $0.owns(effectiveKeyWindow) }
    switch kind {
    case .closePane:
      return keyController?.canClosePane ?? false
    case .copy:
      // Both Copy sources, via the decision `copy()` also uses; `isVisualMode` alone would grey the
      // item out over a live preview selection.
      return keyController?.canCopy ?? false
    case .preview:
      return keyController?.isMarkdownActive ?? false
    case .previewInBrowser:
      return keyController?.isMarkdownActive ?? false
    case .edit:
      return keyController?.isPreviewActive ?? false
    case .previewInOtherPane:
      return keyController?.isMarkdownActive ?? false
    case .editInOtherPane:
      return keyController?.isPreviewActive ?? false
    case .fullWidth:
      // ALWAYS enabled; only the checkmark tracks the active buffer's
      // full-width state.
      item.state = (keyController?.isFullWidthActive ?? false) ? .on : .off
      return true
    default:
      return true
    }
  }

  /// Builds a tab's controller owning `tabCore`, wired to this coordinator and the
  /// shared metrics/config.
  private func makeController(
    tabId: Int, tabCore: TabCore, restoredTab: RestoredTabState?, placement: WindowPlacement,
    hasFolder: Bool
  ) -> WolfWindowController {
    // `metrics` / `config` are set in `applicationDidFinishLaunching` before
    // any controller is built.
    let controller = WolfWindowController(
      tabId: tabId,
      tabCore: tabCore,
      restoredTab: restoredTab,
      placement: placement,
      hasFolder: hasFolder,
      metrics: metrics!,
      config: config!
    )
    controller.coordinator = self
    return controller
  }

  /// Maps a `LaunchDecision` to the controller's `(launchCwd, hasFolder)`: a
  /// `.folder` opens that directory (`hasFolder = true`); an `.emptyState`
  /// launches nvim at home with no folder (`hasFolder = false`).
  private static func dir(_ decision: StateSeam.LaunchDecision) -> (cwd: String, hasFolder: Bool) {
    switch decision {
    case .folder(let cwd): return (cwd, true)
    case .emptyState(let nvimCwd): return (nvimCwd, false)
    }
  }

  /// Decides how a window launches from its restored cwd: reopen the folder when
  /// `restoredCwd` still names a directory, else the "no folder open" empty state
  /// with nvim launched at home. NEVER the process working directory, which would
  /// pollute the recent-dirs MRU.
  private func launchDecision(restoredCwd: String) -> StateSeam.LaunchDecision {
    StateSeam.launchDecision(
      restored: restoredCwd, home: homeDir(), isDirectory: isDirectoryCheck())
  }

  /// Shows a fatal alert when the core fails to start, then terminates.
  private func presentStartupFailureAndQuit() {
    let alert = NSAlert()
    alert.messageText = "Wolf could not start its editor engine."
    alert.informativeText =
      "Starting the embedded Neovim failed. The app bundles its own Neovim; "
      + "if you set WOLF_NVIM_BIN, ensure it points at a valid Neovim binary."
    alert.alertStyle = .critical
    alert.runModal()
    NSApp.terminate(nil)
  }
}

extension Array {
  /// Bounds-checked subscript: the element at `index`, or `nil` when out of
  /// range (used by Phase-B assembly to look a saved group up by index).
  subscript(safe index: Int) -> Element? {
    indices.contains(index) ? self[index] : nil
  }
}
