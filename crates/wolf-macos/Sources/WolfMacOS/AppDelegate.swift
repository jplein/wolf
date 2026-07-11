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

    /// The live per-TAB controllers (each its own captive Neovim). One OS window
    /// per controller; several controllers may belong to one native tab group.
    private var controllers: [WolfWindowController] = []

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
        // default empty-state window with a single tab.
        let restoredGroups = Self.restoredGroups(from: wolf_ffi_load_state())

        if restoredGroups.isEmpty {
            // Fresh first run: always the empty state (no folder), nvim at home.
            let (cwd, hasFolder) = Self.dir(launchDecision(restoredCwd: ""))
            let controller = makeController(
                tabId: mintTabId(), launchCwd: cwd, restoredTab: nil, anchorFrame: nil,
                hasFolder: hasFolder)
            if !controller.start() {
                presentStartupFailureAndQuit()
                return
            }
            controllers.append(controller)
            rebuildWindowMenu()
            scheduleStateSnapshot()
            return
        }

        for group in restoredGroups {
            restoreGroup(group)
        }

        // Every restored core failed (edge case): quit rather than run windowless.
        if controllers.isEmpty {
            presentStartupFailureAndQuit()
            return
        }
        rebuildWindowMenu()
        scheduleStateSnapshot()
    }

    /// Restores one saved window GROUP: starts each tab's controller in order,
    /// anchoring the FIRST successfully-started tab's window at the group's saved
    /// frame and `addTabbedWindow`-ing the rest into that window's tab group. A
    /// tab whose core fails to start is skipped; if the whole group fails, it is
    /// silently dropped (the caller quits only when EVERY group failed).
    private func restoreGroup(_ group: RestoredWindowGroup) {
        let anchorFrame = StateSeam.restoredWindowFrame(
            x: group.x, y: group.y, width: group.width, height: group.height,
            screenVisibleFrames: NSScreen.screens.map(\.visibleFrame))

        var anchor: WolfWindowController?
        for tab in group.tabs {
            let (cwd, hasFolder) = Self.dir(launchDecision(restoredCwd: tab.cwd))
            let isAnchor = anchor == nil
            let controller = makeController(
                tabId: mintTabId(), launchCwd: cwd, restoredTab: tab,
                anchorFrame: isAnchor ? anchorFrame : nil, hasFolder: hasFolder)
            guard controller.start() else { continue }
            if let anchor, let anchorWindow = anchor.nsWindow, let newWindow = controller.nsWindow {
                anchorWindow.addTabbedWindow(newWindow, ordered: .above)
            } else {
                anchor = controller
            }
            controllers.append(controller)
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

        // Tear down the remaining controllers (backstop for any that did not go
        // through windowWillClose).
        for controller in controllers {
            controller.teardown()
        }
    }

    /// The Cmd-N (⇧⌘N) entry point: opens a new STANDALONE window. Cmd-N ALWAYS
    /// opens the empty state (no folder), regardless of the originating window's
    /// cwd — nvim launches at home, the sidebar is hidden, and the centered "Open
    /// folder…" overlay is shown. `originatingCwd` is ignored (kept as a label for
    /// the shared dispatcher call site).
    func makeNewWindow(originatingCwd _: String) {
        // Metrics/config are set at launch before any window opens; bail if not.
        guard metrics != nil, config != nil else { return }
        let (cwd, hasFolder) = Self.dir(launchDecision(restoredCwd: ""))
        let controller = makeController(
            tabId: mintTabId(), launchCwd: cwd, restoredTab: nil, anchorFrame: nil,
            hasFolder: hasFolder)
        if controller.start() {
            controllers.append(controller)
            rebuildWindowMenu()
            scheduleStateSnapshot()
        }
    }

    /// The ⌘T / `:WolfNewTab` entry point: opens a new TAB in `key`'s window
    /// group, inheriting `key`'s open folder (a new tab ≈ a new window in the same
    /// directory). The new window is `addTabbedWindow`-ed into `key`'s group and
    /// selected. A no-op before launch or if the core fails to start.
    func makeNewTab(from key: WolfWindowController) {
        guard metrics != nil, config != nil else { return }
        // Inherit the key window's folder (empty state stays empty state).
        let inheritedCwd = key.hasFolder ? key.currentCwd : ""
        let (cwd, hasFolder) = Self.dir(launchDecision(restoredCwd: inheritedCwd))
        let controller = makeController(
            tabId: mintTabId(), launchCwd: cwd, restoredTab: nil, anchorFrame: nil,
            hasFolder: hasFolder)
        guard controller.start() else { return }
        if let keyWindow = key.nsWindow, let newWindow = controller.nsWindow {
            keyWindow.addTabbedWindow(newWindow, ordered: .above)
            newWindow.makeKeyAndOrderFront(nil)
        }
        controllers.append(controller)
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
            .map { (index: $0.tabId, cwd: $0.currentCwd) }
    }

    /// The first live controller whose `currentCwd` equals `path`, or `nil` when no
    /// open window is rooted there — used by the palette's "Open Recent Folder"
    /// action to raise an existing window instead of re-chdiring.
    func controllerForCwd(_ path: String) -> WolfWindowController? {
        controllers.first { $0.currentCwd == path }
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
                wolf_ffi_state_snapshot_add_tab(tab.currentCwd, tab.sidebarVisible)
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
    /// window is on its LAST split (`:close` would error, E444), and disables the
    /// Copy (`.copy`) command unless the key window is in visual mode (there is
    /// no selection to copy otherwise). Every other item stays enabled.
    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        guard item.action == #selector(handleCommandMenuItem(_:)) else { return true }
        let kind = CommandKind(ffi: UInt8(truncatingIfNeeded: item.tag))
        let keyController = controllers.first { $0.owns(NSApp.keyWindow) }
        switch kind {
        case .closePane:
            return (keyController?.paneCount ?? 0) > 1
        case .copy:
            return keyController?.isVisualMode ?? false
        default:
            return true
        }
    }

    /// Builds a tab controller wired to this coordinator and the shared
    /// metrics/config.
    private func makeController(
        tabId: Int, launchCwd: String, restoredTab: RestoredTabState?, anchorFrame: CGRect?,
        hasFolder: Bool
    ) -> WolfWindowController {
        // `metrics` / `config` are set in `applicationDidFinishLaunching` before
        // any controller is built.
        let controller = WolfWindowController(
            tabId: tabId,
            launchCwd: launchCwd,
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
            restored: restoredCwd,
            home: FileManager.default.homeDirectoryForCurrentUser.path,
            isDirectory: { path in
                var isDir: ObjCBool = false
                return FileManager.default.fileExists(atPath: path, isDirectory: &isDir)
                    && isDir.boolValue
            }
        )
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
