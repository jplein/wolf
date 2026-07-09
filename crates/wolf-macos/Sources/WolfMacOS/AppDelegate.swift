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

/// The application delegate, now a thin COORDINATOR over a collection of
/// `WolfWindowController`s (one per OS window). It performs the once-only launch
/// setup, restores one window per saved `state.json` entry, opens new windows on
/// Cmd-N, and prunes ghost window slots at terminate.
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuItemValidation, NSMenuDelegate {
    /// The once-measured shared cell metrics (nil until launch).
    private var metrics: CellMetrics?
    /// The once-loaded shared user config (nil until launch).
    private var config: FfiConfig?

    /// The live per-window controllers, one per open OS window.
    private var controllers: [WolfWindowController] = []

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

        // Read the saved state BEFORE any core starts: one controller per saved
        // window (its slot == its list position), else one default at index 0.
        let restoredWindows = Self.restoredWindows(from: wolf_ffi_load_state())

        if restoredWindows.isEmpty {
            // Fresh first run: always the empty state (no folder), nvim at home.
            let (cwd, hasFolder) = Self.dir(launchDecision(restoredCwd: ""))
            let controller = makeController(
                windowIndex: 0, launchCwd: cwd, restored: nil, hasFolder: hasFolder)
            if !controller.start() {
                presentStartupFailureAndQuit()
                return
            }
            controllers.append(controller)
            rebuildWindowMenu()
            return
        }

        for (index, restored) in restoredWindows.enumerated() {
            let (cwd, hasFolder) = Self.dir(launchDecision(restoredCwd: restored.cwd))
            let controller = makeController(
                windowIndex: index, launchCwd: cwd, restored: restored, hasFolder: hasFolder)
            let ok = controller.start()
            // A fatal startup alert only for the FIRST window; a later window's
            // core failing simply opens fewer windows.
            if !ok {
                if index == 0 {
                    presentStartupFailureAndQuit()
                    return
                }
                continue
            }
            controllers.append(controller)
        }

        // Every restored core failed after the first (edge case): quit rather
        // than run windowless.
        if controllers.isEmpty {
            presentStartupFailureAndQuit()
            return
        }
        rebuildWindowMenu()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationWillTerminate(_ notification: Notification) {
        // Re-push each live window's snapshot at its slot, then prune any ghost
        // slots left by closed windows above the highest live index (the
        // documented v1 limitation: a hole left by a closed MIDDLE window
        // survives until it is reused). Truncating to the highest live index + 1
        // never drops a live slot (unlike a naive `controllers.count`, which
        // would truncate a live higher slot when a hole exists).
        for controller in controllers {
            controller.pushSnapshot()
        }
        if let highest = controllers.map(\.windowIndex).max() {
            controllers.first?.handle?.state_set_window_count(UInt16(highest + 1))
        }

        // Tear down the remaining controllers (backstop for any that did not go
        // through windowWillClose). Iterate a copy: teardown removes from
        // `controllers`.
        for controller in controllers {
            controller.teardown()
        }
    }

    /// The Cmd-N entry point: opens a new window in the first free `state.json`
    /// slot. Cmd-N ALWAYS opens the empty state (no folder), regardless of the
    /// originating window's cwd — nvim launches at home, the sidebar is hidden, and
    /// the centered "Open folder…" overlay is shown. `originatingCwd` is ignored
    /// (kept as a label for the shared dispatcher call site).
    func makeNewWindow(originatingCwd _: String) {
        // Metrics/config are set at launch before any window opens; bail if not.
        guard metrics != nil, config != nil else { return }
        let index = WindowIndexSeam.nextFreeIndex(occupied: controllers.map(\.windowIndex))
        let (cwd, hasFolder) = Self.dir(launchDecision(restoredCwd: ""))
        let controller = makeController(
            windowIndex: index, launchCwd: cwd, restored: nil, hasFolder: hasFolder)
        if controller.start() {
            controllers.append(controller)
            rebuildWindowMenu()
        }
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

    /// The dynamic Window-menu item action: raises the window whose `windowIndex`
    /// equals the item's `tag`.
    @objc func raiseWindowMenuItem(_ sender: NSMenuItem) {
        controllers.first { $0.windowIndex == sender.tag }?.raise()
    }

    /// The OTHER open windows (all live controllers EXCEPT `excluding`), as
    /// `(windowIndex, currentCwd)` pairs — the command palette's "Switch to Window"
    /// candidate source. The current window is excluded (it cannot switch to
    /// itself).
    func otherWindows(excluding: WolfWindowController) -> [(index: Int, cwd: String)] {
        controllers
            .filter { $0 !== excluding }
            .map { (index: $0.windowIndex, cwd: $0.currentCwd) }
    }

    /// The first live controller whose `currentCwd` equals `path`, or `nil` when no
    /// open window is rooted there — used by the palette's "Open Recent Folder"
    /// action to raise an existing window instead of re-chdiring.
    func controllerForCwd(_ path: String) -> WolfWindowController? {
        controllers.first { $0.currentCwd == path }
    }

    /// Raises the window whose `windowIndex` equals `windowIndex` (the palette's
    /// "Switch to Window" action). A no-op when no such window is open.
    func raiseWindow(windowIndex: Int) {
        controllers.first { $0.windowIndex == windowIndex }?.raise()
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
        case .closeTab:
            // Disable only at the last-pane-last-tab, where the core-side helper
            // is a complete no-op (Decision #5). Otherwise Close Tab is enabled.
            return !((keyController?.paneCount ?? 0) <= 1 && (keyController?.activeTabCount ?? 0) <= 1)
        case .copy:
            return keyController?.isVisualMode ?? false
        default:
            return true
        }
    }

    /// Builds a controller wired to this coordinator and the shared metrics/config.
    private func makeController(
        windowIndex: Int, launchCwd: String, restored: RestoredWindowState?, hasFolder: Bool
    ) -> WolfWindowController {
        // `metrics` / `config` are set in `applicationDidFinishLaunching` before
        // any controller is built.
        let controller = WolfWindowController(
            windowIndex: windowIndex,
            launchCwd: launchCwd,
            restored: restored,
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

    /// Extracts EVERY saved window from the bridge's restore read into plain
    /// `RestoredWindowState` values (element refs must not outlive the vec).
    private static func restoredWindows(from state: FfiState) -> [RestoredWindowState] {
        let windows = state.windows()
        var result: [RestoredWindowState] = []
        for i in 0..<windows.len() {
            guard let w = windows.get(index: UInt(i)) else { continue }
            result.append(
                RestoredWindowState(
                    x: w.x(), y: w.y(),
                    width: w.width(), height: w.height(),
                    cwd: w.cwd().toString(),
                    sidebarVisible: w.sidebar_visible()
                ))
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
