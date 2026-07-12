//
//  AppDelegate.swift
//  The application coordinator: once-only launch setup (wake bridges, fonts,
//  config, cell metrics), a collection of per-window controllers, and the Cmd-N
//  new-window entry point.
//

import AppKit
import Foundation
import WolfFFI
import WolfKeyMap

/// The application delegate, a thin COORDINATOR over a collection of per-TAB
/// `WolfWindowController`s (each its own OS window; several may share one native
/// tab group). It performs the once-only launch setup, restores the saved
/// window/tab tree group-by-group, opens new windows (⇧⌘N) and tabs (⌘T / the
/// native "+"), owns the coalesced whole-tree state snapshot, and refreshes the
/// tab-bar titles.
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuItemValidation, NSMenuDelegate {
    /// The once-measured shared cell metrics (nil until launch).
    private var metrics: CellMetrics?
    /// The once-loaded shared user config (nil until launch).
    private var config: FfiConfig?

    /// The live per-VIEW controllers (each bound to one nvim tabpage of a shared
    /// project). One OS window per controller; several controllers may belong to
    /// one native tab group AND/OR one project.
    private var controllers: [WolfWindowController] = []

    /// The live PROJECTS (each a captive Neovim shared by N views). Strong-held so
    /// a core outlives a view teardown while a sibling view still binds it; a
    /// project removes itself (`removeProject`) when its last view closes.
    private var projects: [Project] = []

    /// The next monotonic tab id to mint (replaces the old per-window slot index;
    /// used only as an opaque Window-menu / raise key, never a `state.json` slot).
    private var nextTabId = 0

    /// The coalesced whole-tree snapshot work item (~150ms debounce), so a KVO /
    /// observer storm collapses into a single tree walk + commit.
    private var snapshotWorkItem: DispatchWorkItem?

    /// The whole-tree snapshot coalesce delay.
    private static let snapshotCoalesceDelay: TimeInterval = 0.15

    /// The Window submenu (returned by `AppMenu.install`), rebuilt with the
    /// dynamic per-window list on window open/close/cwd-change. `nil` until launch.
    private var windowMenu: NSMenu?

    /// The File > Open Recent submenu (returned by `AppMenu.install`), rebuilt
    /// lazily in `menuNeedsUpdate` from the recent-dirs MRU. `nil` until launch.
    private var openRecentMenu: NSMenu?

    /// The recent-dirs list backing the current Open Recent submenu, captured in
    /// `menuNeedsUpdate` so `openRecentMenuItem(_:)` can resolve `tag` -> path.
    private var recentDirs: [String] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Register the three process-global wake callbacks ONCE; each routes by
        // the firing core's token through `WindowRegistry`.
        WakeBridge.install()
        PickerWakeBridge.install()
        TreeWakeBridge.install()

        // Build the native menu bar (net-new). Every Wolf item funnels through
        // `handleCommandMenuItem(_:)` → `CommandDispatcher`. Keep the Window and
        // Open Recent submenus so their dynamic lists can be rebuilt; the Open
        // Recent submenu rebuilds lazily via `menuNeedsUpdate`.
        let menus = AppMenu.install(target: self)
        windowMenu = menus.window
        openRecentMenu = menus.openRecent
        openRecentMenu?.delegate = self

        // Register the bundled font TTFs with CoreText, then resolve the user
        // config BEFORE measuring so `CellMetrics` can resolve the configured
        // family/size/line-height. Both are measured ONCE and shared by every
        // window.
        FontRegistration.registerBundledFonts()
        let config = wolf_ffi_load_config()
        let metrics = CellMetrics.measured(config: config)
        self.config = config
        self.metrics = metrics

        // Read the saved state BEFORE any core starts: one WINDOW GROUP per saved
        // window, each with its ordered tabs. A fresh run restores nothing → one
        // default empty-state window with a single tab (its own fresh project).
        let restoredGroups = Self.restoredGroups(from: wolf_ffi_load_state())

        if restoredGroups.isEmpty {
            // Fresh first run: always the empty state (no folder), nvim at home.
            guard let controller = startEmptyStateWindow() else {
                presentStartupFailureAndQuit()
                return
            }
            controllers.append(controller)
            rebuildWindowMenu()
            scheduleStateSnapshot()
            return
        }

        // GROUP-BY-CWD restore (plan §4.2 + OD5): build the pure project plan from
        // the saved tree, materialize its projects + views (Phase A), then assemble
        // the OS windows/tab-groups from the original saved geometry (Phase B).
        let plan = RestoreSeam.plan(
            windows: Self.savedWindows(from: restoredGroups),
            home: homeDir(),
            isDirectory: isDirectoryCheck())
        let byGroup = materializeProjects(plan)
        assembleWindows(byGroup, groups: restoredGroups)

        // Every restored core failed (edge case): quit rather than run windowless.
        if controllers.isEmpty {
            presentStartupFailureAndQuit()
            return
        }
        rebuildWindowMenu()
        scheduleStateSnapshot()
    }

    /// PHASE A — materializes every planned project + its views WITHOUT anchoring
    /// or tab-grouping them yet. Returns a map from the ORIGINAL saved group index
    /// to the group's `(positionInGroup, controller)` members, so Phase B can drop
    /// each view back into the right OS window/tab-group. A project whose core
    /// fails to start (or whose every tab fails) is dropped; a failed tab is
    /// skipped.
    private func materializeProjects(
        _ plan: RestoreSeam.RestorePlan
    ) -> [Int: [(pos: Int, controller: WolfWindowController)]] {
        var byGroup: [Int: [(pos: Int, controller: WolfWindowController)]] = [:]

        for projectPlan in plan.projects {
            guard let project = Project.start(launchCwd: projectPlan.launchCwd, coordinator: self)
            else { continue }

            var built = 0
            var boundFirst = false
            for tab in projectPlan.tabs {
                // The first tab binds the project's implicit tabpage 1; the rest
                // create fresh tabpages rooted at the project's (uniform) cwd.
                let tabpage: Int64
                if !boundFirst {
                    tabpage = project.bindFirstTabpage()
                    boundFirst = true
                } else {
                    tabpage = project.addTabpage(cwd: projectPlan.launchCwd)
                }
                guard tabpage != 0 else { continue }

                let (_, hasFolder) = Self.dir(launchDecision(restoredCwd: tab.cwd))
                let restoredTab = RestoredTabState(
                    cwd: tab.cwd, sidebarVisible: tab.sidebarVisible)
                let controller = makeController(
                    tabId: mintTabId(), project: project, tabpageHandle: tabpage,
                    restoredTab: restoredTab, anchorFrame: nil, hasFolder: hasFolder)
                guard controller.build() else { continue }
                controllers.append(controller)
                byGroup[tab.groupIndex, default: []].append((tab.positionInGroup, controller))
                built += 1
            }

            if built == 0 {
                // No view survived for this project: shut its idle core down.
                project.shutdown()
            } else {
                projects.append(project)
            }
        }
        return byGroup
    }

    /// PHASE B — assembles the OS windows/tab-groups: for each saved window group
    /// in order, anchors the first surviving view at the group's validated frame
    /// and `addTabbedWindow`s the rest into its tab group (parity with the old
    /// single-phase group assembly). A group with no surviving views is dropped.
    private func assembleWindows(
        _ byGroup: [Int: [(pos: Int, controller: WolfWindowController)]],
        groups: [RestoredWindowGroup]
    ) {
        for groupIndex in byGroup.keys.sorted() {
            guard let group = groups[safe: groupIndex] else { continue }
            let members = byGroup[groupIndex]!.sorted { $0.pos < $1.pos }
            guard let anchor = members.first?.controller, let anchorWindow = anchor.nsWindow
            else { continue }

            if let anchorFrame = StateSeam.restoredWindowFrame(
                x: group.x, y: group.y, width: group.width, height: group.height,
                screenVisibleFrames: NSScreen.screens.map(\.visibleFrame))
            {
                anchorWindow.setFrame(anchorFrame, display: false)
            }

            for member in members.dropFirst() {
                if let newWindow = member.controller.nsWindow {
                    anchorWindow.addTabbedWindow(newWindow, ordered: .above)
                }
            }
        }
    }

    /// Starts a fresh empty-state PROJECT (nvim at home) with a single view bound
    /// to its implicit tabpage 1, and makes that view current. Returns the view, or
    /// `nil` when the core fails to start. The caller shows/appends it.
    private func startEmptyStateWindow() -> WolfWindowController? {
        guard let project = Project.start(launchCwd: homeDir(), coordinator: self) else {
            return nil
        }
        let tabpage = project.bindFirstTabpage()
        let controller = makeController(
            tabId: mintTabId(), project: project, tabpageHandle: tabpage, restoredTab: nil,
            anchorFrame: nil, hasFolder: false)
        guard controller.build() else {
            project.shutdown()
            return nil
        }
        projects.append(project)
        project.setCurrent(controller)
        return controller
    }

    /// The user's home directory path (the empty-state launch cwd).
    private func homeDir() -> String {
        FileManager.default.homeDirectoryForCurrentUser.path
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

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationWillTerminate(_ notification: Notification) {
        // Walk the whole live window/tab tree and commit it SYNCHRONOUSLY (any
        // coalesced snapshot may not have fired yet), then block briefly on the
        // state writer so `state.json` is on disk before teardown.
        snapshotNow()
        _ = wolf_ffi_state_flush()

        // Tear down the remaining views (backstop for any that did not go through
        // windowWillClose); each view's teardown detaches from its project, which
        // shuts the core down when its last view goes. Then shut down any project
        // that still lingers (belt-and-suspenders — e.g. a project with no views).
        for controller in controllers {
            controller.teardown()
        }
        // `shutdown()` calls `removeProject`, mutating `projects`; iterate a copy.
        let remainingProjects = projects
        for project in remainingProjects {
            project.shutdown()
        }
        projects.removeAll()
    }

    /// The Cmd-N (⇧⌘N) entry point: opens a new STANDALONE window. Cmd-N ALWAYS
    /// opens the empty state (no folder), regardless of the originating window's
    /// cwd — nvim launches at home, the sidebar is hidden, and the centered "Open
    /// folder…" overlay is shown. `originatingCwd` is ignored (kept as a label for
    /// the shared dispatcher call site).
    func makeNewWindow(originatingCwd _: String) {
        // Metrics/config are set at launch before any window opens; bail if not.
        guard metrics != nil, config != nil else { return }
        // A new window is a NEW project (a fresh nvim) with one empty-state view.
        guard let controller = startEmptyStateWindow() else { return }
        controllers.append(controller)
        rebuildWindowMenu()
        scheduleStateSnapshot()
    }

    /// The ⌘T / `:WolfNewTab` entry point: opens a new TAB in `key`'s window
    /// group, inheriting `key`'s open folder (a new tab ≈ a new window in the same
    /// directory). The new window is `addTabbedWindow`-ed into `key`'s group and
    /// selected. A no-op before launch or if the core fails to start.
    func makeNewTab(from key: WolfWindowController) {
        guard metrics != nil, config != nil else { return }
        // A new tab is a new VIEW of the KEY window's SAME project (a new tabpage in
        // that shared nvim), inheriting the key window's folder.
        let inheritedCwd = key.hasFolder ? key.currentCwd : ""
        let tabpage = key.project.addTabpage(cwd: inheritedCwd)
        guard tabpage != 0 else { return }
        let (_, hasFolder) = Self.dir(launchDecision(restoredCwd: inheritedCwd))
        let controller = makeController(
            tabId: mintTabId(), project: key.project, tabpageHandle: tabpage, restoredTab: nil,
            anchorFrame: nil, hasFolder: hasFolder)
        guard controller.build() else { return }
        if let keyWindow = key.nsWindow, let newWindow = controller.nsWindow {
            keyWindow.addTabbedWindow(newWindow, ordered: .above)
            newWindow.makeKeyAndOrderFront(nil)
        }
        controllers.append(controller)
        key.project.setCurrent(controller)
        rebuildWindowMenu()
        scheduleStateSnapshot()
    }

    /// The native tab bar's "+" button forwards here (NSApp → delegate). Opens a
    /// new tab in the key window's group, falling back to the first controller.
    @objc func newWindowForTab(_ sender: Any?) {
        let key = controllers.first { $0.owns(NSApp.keyWindow) } ?? controllers.first
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
        let key = NSApp.keyWindow
        let target = controllers.first { $0.owns(key) }
        let cwd = target?.currentCwd ?? ""
        CommandDispatcher.dispatch(
            kind, window: key, coordinator: self, originatingCwd: cwd, controller: target)
    }

    /// Drops `controller` from the collection (called by the controller at
    /// teardown / window close), then rebuilds the dynamic Window menu.
    func removeController(_ controller: WolfWindowController) {
        controllers.removeAll { $0 === controller }
        rebuildWindowMenu()
    }

    /// Drops `project` from the collection (called by the project when its last
    /// view closes and it shuts its core down).
    func removeProject(_ project: Project) {
        projects.removeAll { $0 === project }
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

    /// The OTHER open tabs (all live controllers EXCEPT `excluding`), as
    /// `(tabId, currentCwd)` pairs — the command palette's "Switch to Window"
    /// candidate source. The current tab is excluded (it cannot switch to itself).
    func otherWindows(excluding: WolfWindowController) -> [(index: Int, cwd: String)] {
        controllers
            .filter { $0 !== excluding }
            .map { (index: $0.tabId, cwd: $0.persistedCwd) }
    }

    /// The first live controller whose PERSISTED cwd equals `path`, or `nil` when
    /// no open window is rooted there — used by the palette's "Open Recent Folder"
    /// action to raise an existing window instead of re-chdiring. Uses the per-view
    /// `persistedCwd` so a non-current view still matches.
    func controllerForCwd(_ path: String) -> WolfWindowController? {
        controllers.first { $0.persistedCwd == path }
    }

    /// Raises the tab whose `tabId` equals `tabId` (the palette's "Switch to
    /// Window" action). A no-op when no such tab is open.
    func raiseWindow(tabId: Int) {
        controllers.first { $0.tabId == tabId }?.raise()
    }

    /// Schedules a coalesced whole-tree state snapshot (~150ms debounce): a rapid
    /// burst of geometry / sidebar / tab-group changes collapses into a single
    /// tree walk + `state.json` write. Every trigger (geometry, sidebar toggle,
    /// cwd change, tab open/close, tear-off/merge) routes here.
    func scheduleStateSnapshot() {
        snapshotWorkItem?.cancel()
        let item = DispatchWorkItem { [weak self] in self?.snapshotNow() }
        snapshotWorkItem = item
        DispatchQueue.main.asyncAfter(
            deadline: .now() + Self.snapshotCoalesceDelay, execute: item)
    }

    /// Walks the WHOLE live window/tab tree and commits it to `state.json` via the
    /// write-side snapshot builder. Windows are grouped by their native tab group
    /// (a solo window is its own group); tab order comes from `tabGroup.windows`;
    /// each group's geometry is its selected tab's frame (fallback: the first
    /// tab's). Per tab it records the live cwd + sidebar visibility.
    func snapshotNow() {
        wolf_ffi_state_snapshot_begin()
        var seenGroups = Set<ObjectIdentifier>()
        for controller in controllers {
            guard let window = controller.nsWindow else { continue }

            // The ordered windows of this controller's group (a solo window when
            // it is not in a multi-window tab group).
            let groupWindows: [NSWindow]
            if let tabGroup = window.tabGroup {
                let id = ObjectIdentifier(tabGroup)
                if seenGroups.contains(id) { continue }
                seenGroups.insert(id)
                groupWindows = tabGroup.windows
            } else {
                groupWindows = [window]
            }

            let frame = window.tabGroup?.selectedWindow?.frame ?? groupWindows.first?.frame
                ?? window.frame
            wolf_ffi_state_snapshot_add_window(
                frame.origin.x, frame.origin.y, frame.size.width, frame.size.height)

            for tabWindow in groupWindows {
                guard let tab = controllers.first(where: { $0.owns(tabWindow) }) else { continue }
                // Per-view PERSISTED cwd (OD6): the tabpage's authoritative tab-local
                // `:tcd` cwd, correct even for a non-current view (unlike the live
                // `currentCwd`, which follows the current tabpage).
                wolf_ffi_state_snapshot_add_tab(tab.persistedCwd, tab.sidebarVisible)
            }
        }
        wolf_ffi_state_snapshot_commit()
    }

    /// Refreshes the tab-bar titles for `window`'s whole tab group from each tab's
    /// active buffer basename, disambiguating collisions via `TabTitleSeam`. Each
    /// tab IS its own NSWindow, so setting `window.title` per tab drives both the
    /// tab label and that tab's titlebar. A solo window is a one-element group.
    func refreshTabTitles(forGroupOf window: NSWindow) {
        let groupWindows = window.tabGroup?.windows ?? [window]
        let paths = groupWindows.map { tabWindow -> String in
            controllers.first { $0.owns(tabWindow) }?.activeBufferPath ?? ""
        }
        let labels = TabTitleSeam.labels(activeBufferPaths: paths)
        for (tabWindow, label) in zip(groupWindows, labels) {
            tabWindow.title = label
        }
    }

    /// `NSMenuDelegate`: rebuilds the Open Recent submenu just before it opens,
    /// reading the recent-dirs MRU from the key window's controller (falling back
    /// to the first controller when no Wolf window is key). Ignores any other menu.
    func menuNeedsUpdate(_ menu: NSMenu) {
        guard menu === openRecentMenu else { return }
        let source = controllers.first { $0.owns(NSApp.keyWindow) } ?? controllers.first
        recentDirs = source?.recentDirs() ?? []
        AppMenu.rebuildOpenRecentMenu(menu, recentDirs: recentDirs, target: self)
    }

    /// The dynamic Open Recent item action: changes the key window's Neovim
    /// working directory to the recent dir at the item's `tag` (an index into the
    /// `recentDirs` snapshot captured in `menuNeedsUpdate`). Out-of-range tags are
    /// ignored.
    @objc func openRecentMenuItem(_ sender: NSMenuItem) {
        guard sender.tag >= 0, sender.tag < recentDirs.count else { return }
        let path = recentDirs[sender.tag]
        let target = controllers.first { $0.owns(NSApp.keyWindow) } ?? controllers.first
        target?.changeDirectory(path)
    }

    /// Notified by a controller when its live cwd changed (its Window-menu title
    /// is the full cwd path), rebuilding the dynamic Window menu.
    func windowCwdDidChange(_ controller: WolfWindowController) {
        rebuildWindowMenu()
    }

    /// Menu validation: disables the Close (`.closePane`) command when the key
    /// window is on its LAST split (`:close` would error, E444), disables the
    /// Copy (`.copy`) command unless the key window is in visual mode (there is
    /// no selection to copy otherwise), and disables the Preview (`.preview`)
    /// command unless the key window's active buffer is Markdown. Every other item
    /// stays enabled.
    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        guard item.action == #selector(handleCommandMenuItem(_:)) else { return true }
        let kind = CommandKind(ffi: UInt8(truncatingIfNeeded: item.tag))
        let keyController = controllers.first { $0.owns(NSApp.keyWindow) }
        switch kind {
        case .closePane:
            return (keyController?.paneCount ?? 0) > 1
        case .copy:
            return keyController?.isVisualMode ?? false
        case .preview:
            return keyController?.isMarkdownActive ?? false
        default:
            return true
        }
    }

    /// Builds a VIEW controller bound to `project`'s shared core + `tabpageHandle`,
    /// wired to this coordinator and the shared metrics/config.
    private func makeController(
        tabId: Int, project: Project, tabpageHandle: Int64, restoredTab: RestoredTabState?,
        anchorFrame: CGRect?, hasFolder: Bool
    ) -> WolfWindowController {
        // `metrics` / `config` are set in `applicationDidFinishLaunching` before
        // any controller is built.
        let controller = WolfWindowController(
            tabId: tabId,
            project: project,
            tabpageHandle: tabpageHandle,
            restoredTab: restoredTab,
            anchorFrame: anchorFrame,
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
    /// with nvim launched at home. NEVER the process working directory (the old
    /// `launchCwd` fallback that polluted the recent-dirs MRU).
    private func launchDecision(restoredCwd: String) -> StateSeam.LaunchDecision {
        StateSeam.launchDecision(
            restored: restoredCwd, home: homeDir(), isDirectory: isDirectoryCheck())
    }

    /// Projects the restored window GROUPS into `RestoreSeam.SavedWindow`s (the
    /// grouping seam's input), preserving window + tab order.
    private static func savedWindows(from groups: [RestoredWindowGroup]) -> [RestoreSeam.SavedWindow]
    {
        groups.map { group in
            RestoreSeam.SavedWindow(
                tabs: group.tabs.map {
                    RestoreSeam.SavedTab(cwd: $0.cwd, sidebarVisible: $0.sidebarVisible)
                })
        }
    }

    /// Extracts EVERY saved window group (geometry + ordered tabs) from the
    /// bridge's restore read into plain values (element refs must not outlive the
    /// vec). A group with no tabs is dropped (nothing to restore).
    private static func restoredGroups(from state: FfiState) -> [RestoredWindowGroup] {
        let windows = state.windows()
        var result: [RestoredWindowGroup] = []
        for i in 0..<windows.len() {
            guard let w = windows.get(index: UInt(i)) else { continue }
            let tabsVec = w.tabs()
            var tabs: [RestoredTabState] = []
            for j in 0..<tabsVec.len() {
                guard let t = tabsVec.get(index: UInt(j)) else { continue }
                tabs.append(
                    RestoredTabState(cwd: t.cwd().toString(), sidebarVisible: t.sidebar_visible()))
            }
            if tabs.isEmpty { continue }
            result.append(
                RestoredWindowGroup(
                    x: w.x(), y: w.y(), width: w.width(), height: w.height(), tabs: tabs))
        }
        return result
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
