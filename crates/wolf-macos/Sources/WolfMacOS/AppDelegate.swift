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
    private var picker: PickerView?
    private var tree: FileTreeView?

    func applicationDidFinishLaunching(_ notification: Notification) {
        WakeBridge.install()
        PickerWakeBridge.install()
        TreeWakeBridge.install()

        guard let handle = startCore() else {
            presentStartupFailureAndQuit()
            return
        }
        self.handle = handle

        let metrics = CellMetrics.measured()
        let gridView = GridView(handle: handle, metrics: metrics)
        WakeBridge.registeredView = gridView

        // Build the file-tree sidebar (left pane). It owns its OWN long-lived
        // file-search session via `start_tree_search`; the render wake reveals +
        // bolds the current buffer, the tree wake rebuilds its rows.
        let tree = FileTreeView(handle: handle, gridView: gridView)
        WakeBridge.registeredTree = tree
        TreeWakeBridge.registeredTree = tree
        tree.start()
        self.tree = tree

        let window = MainWindow(
            treeView: tree,
            gridView: gridView,
            metrics: metrics,
            cols: Self.initialCols,
            rows: Self.initialRows
        )

        // Build the file picker as a hidden top-center overlay over the editor
        // (the picker is a subview of the grid view). Shift+Ctrl+P opens it.
        let picker = PickerView(handle: handle, gridView: gridView)
        gridView.addSubview(picker)
        PickerWakeBridge.registeredPicker = picker
        gridView.onPickerTrigger = { [weak picker] in picker?.open() }
        self.picker = picker

        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        self.window = window
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationWillTerminate(_ notification: Notification) {
        // Drop the sidebar's and picker's file-search sessions (cancelling any
        // in-flight traversals) before the core is torn down.
        tree?.close()
        tree = nil
        picker?.close()
        picker = nil
        handle?.shutdown()
        handle = nil
    }

    /// Starts the Rust core against the bundled `init.lua` and a working dir.
    private func startCore() -> WolfCoreHandle? {
        // Point wolf-core at the bundled Neovim unless a developer has already
        // exported `WOLF_NVIM_BIN` (overwrite=0 so the dev override wins). This
        // is what makes the packaged `.app` double-clickable with no shell env.
        if let bin = nvimBinPath() {
            setenv("WOLF_NVIM_BIN", bin, 0)
        }
        let initLua = initLuaPath()
        let cwd = FileManager.default.currentDirectoryPath
        return wolf_ffi_start(
            initLua,
            cwd,
            UInt16(Self.initialCols),
            UInt16(Self.initialRows)
        )
    }

    /// Resolves the bundled Neovim binary at `Contents/Resources/nvim/bin/nvim`.
    /// Returns `nil` (rather than a bogus path) when it is absent or not
    /// executable, so a developer's `WOLF_NVIM_BIN` export still takes effect.
    private func nvimBinPath() -> String? {
        guard let resourcePath = Bundle.main.resourcePath else { return nil }
        let candidate = resourcePath + "/nvim/bin/nvim"
        guard FileManager.default.isExecutableFile(atPath: candidate) else {
            return nil
        }
        return candidate
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
            "Starting the embedded Neovim failed. The app bundles its own Neovim; "
            + "if you set WOLF_NVIM_BIN, ensure it points at a valid Neovim binary."
        alert.alertStyle = .critical
        alert.runModal()
        NSApp.terminate(nil)
    }
}
