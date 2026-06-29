//
//  AppDelegate.swift
//  Wires up the core, window, and grid view; tears down on terminate.
//

import AppKit
import Foundation
import WolfFFI
import WolfKeyMap

/// The application delegate: resolves `init_lua` + cwd, starts the Rust core,
/// builds the window and `GridView`, registers the view with `WakeBridge`, and
/// shuts the core down cleanly on terminate.
final class AppDelegate: NSObject, NSApplicationDelegate {
    /// Initial grid width in columns.
    private static let initialCols = 80
    /// Initial grid height in rows.
    private static let initialRows = 24

    private var window: MainWindow?
    private var handle: WolfCoreHandle?

    func applicationDidFinishLaunching(_ notification: Notification) {
        WakeBridge.install()

        guard let handle = startCore() else {
            presentStartupFailureAndQuit()
            return
        }
        self.handle = handle

        let metrics = CellMetrics.measured()
        let gridView = GridView(handle: handle, metrics: metrics)
        WakeBridge.registeredView = gridView

        let window = MainWindow(
            gridView: gridView,
            metrics: metrics,
            cols: Self.initialCols,
            rows: Self.initialRows
        )
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        self.window = window
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationWillTerminate(_ notification: Notification) {
        handle?.shutdown()
        handle = nil
    }

    /// Starts the Rust core against the bundled `init.lua` and a working dir.
    private func startCore() -> WolfCoreHandle? {
        let initLua = initLuaPath()
        let cwd = FileManager.default.currentDirectoryPath
        return wolf_ffi_start(
            initLua,
            cwd,
            UInt16(Self.initialCols),
            UInt16(Self.initialRows)
        )
    }

    /// Resolves the bundled `runtime/nvim/init.lua` path. Inside a packaged
    /// `.app` it lives in `Contents/Resources`; a `WOLF_INIT_LUA` override is
    /// honored first (for development).
    private func initLuaPath() -> String {
        if let override = ProcessInfo.processInfo.environment["WOLF_INIT_LUA"],
            !override.isEmpty
        {
            return override
        }
        if let resource = Bundle.main.path(forResource: "init", ofType: "lua") {
            return resource
        }
        return "runtime/nvim/init.lua"
    }

    /// Shows a fatal alert when the core fails to start, then terminates.
    private func presentStartupFailureAndQuit() {
        let alert = NSAlert()
        alert.messageText = "Wolf could not start its editor engine."
        alert.informativeText =
            "Starting the embedded Neovim failed. Ensure WOLF_NVIM_BIN points at a "
            + "valid Neovim binary."
        alert.alertStyle = .critical
        alert.runModal()
        NSApp.terminate(nil)
    }
}
