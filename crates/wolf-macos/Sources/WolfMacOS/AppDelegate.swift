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
final class AppDelegate: NSObject, NSApplicationDelegate {
    /// The once-measured shared cell metrics (nil until launch).
    private var metrics: CellMetrics?
    /// The once-loaded shared user config (nil until launch).
    private var config: FfiConfig?

    /// The live per-window controllers, one per open OS window.
    private var controllers: [WolfWindowController] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Register the three process-global wake callbacks ONCE; each routes by
        // the firing core's token through `WindowRegistry`.
        WakeBridge.install()
        PickerWakeBridge.install()
        TreeWakeBridge.install()

        // Build the native menu bar (net-new). Every Wolf item funnels through
        // `handleCommandMenuItem(_:)` → `CommandDispatcher`.
        AppMenu.install(target: self)

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
            let cwd = launchCwd(restoredCwd: "")
            let controller = makeController(windowIndex: 0, launchCwd: cwd, restored: nil)
            if !controller.start() {
                presentStartupFailureAndQuit()
                return
            }
            controllers.append(controller)
            return
        }

        for (index, restored) in restoredWindows.enumerated() {
            let cwd = launchCwd(restoredCwd: restored.cwd)
            let controller = makeController(
                windowIndex: index, launchCwd: cwd, restored: restored)
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
        }
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
    /// slot, rooted at `originatingCwd` (falling back to the home directory when
    /// it is empty), with AppKit cascade placement.
    func makeNewWindow(originatingCwd: String) {
        // Metrics/config are set at launch before any window opens; bail if not.
        guard metrics != nil, config != nil else { return }
        let index = WindowIndexSeam.nextFreeIndex(occupied: controllers.map(\.windowIndex))
        let cwd = originatingCwd.isEmpty
            ? FileManager.default.homeDirectoryForCurrentUser.path
            : originatingCwd
        let controller = makeController(windowIndex: index, launchCwd: cwd, restored: nil)
        if controller.start() {
            controllers.append(controller)
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
    /// teardown / window close).
    func removeController(_ controller: WolfWindowController) {
        controllers.removeAll { $0 === controller }
    }

    /// Builds a controller wired to this coordinator and the shared metrics/config.
    private func makeController(
        windowIndex: Int, launchCwd: String, restored: RestoredWindowState?
    ) -> WolfWindowController {
        // `metrics` / `config` are set in `applicationDidFinishLaunching` before
        // any controller is built.
        let controller = WolfWindowController(
            windowIndex: windowIndex,
            launchCwd: launchCwd,
            restored: restored,
            metrics: metrics!,
            config: config!
        )
        controller.coordinator = self
        return controller
    }

    /// Picks the embedded Neovim's launch working directory: the restored cwd
    /// when it still names a directory, else the process's current directory.
    private func launchCwd(restoredCwd: String) -> String {
        StateSeam.launchCwd(
            restored: restoredCwd,
            fallback: FileManager.default.currentDirectoryPath,
            isDirectory: { path in
                var isDir: ObjCBool = false
                return FileManager.default.fileExists(atPath: path, isDirectory: &isDir)
                    && isDir.boolValue
            }
        )
    }

    /// Extracts EVERY saved window from the bridge's restore read into plain
    /// `RestoredWindowState` values (element refs must not outlive the vec). An
    /// unknown toolbar discriminant maps to `nil` (the toolbar keeps its default).
    private static func restoredWindows(from state: FfiState) -> [RestoredWindowState] {
        let windows = state.windows()
        var result: [RestoredWindowState] = []
        for i in 0..<windows.len() {
            guard let w = windows.get(index: UInt(i)) else { continue }
            let toolbarMode: StateSeam.ToolbarMode? =
                w.toolbar_mode_set() ? StateSeam.ToolbarMode(rawValue: w.toolbar_mode()) : nil
            result.append(
                RestoredWindowState(
                    x: w.x(), y: w.y(),
                    width: w.width(), height: w.height(),
                    cwd: w.cwd().toString(),
                    sidebarVisible: w.sidebar_visible(),
                    toolbarMode: toolbarMode
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
