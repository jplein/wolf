//
//  WolfWindowController.swift
//  Owns ONE top-level window's stack: its embedded Neovim core, the window, the
//  pane container, the file-tree sidebar, the picker, and its state persistence.
//  One instance per OS window (plan §4); the `AppDelegate` coordinator holds a
//  collection of them.
//

import AppKit
import Foundation
import WolfFFI
import WolfKeyMap

/// The scalar facets of one saved window, copied OUT of the bridge's
/// `RustVec<FfiWindowState>` immediately (element refs must not outlive the vec)
/// into a plain value that survives launch. Shared by the coordinator (which
/// reads `state.json`) and the controller (which restores frame/sidebar/toolbar).
struct RestoredWindowState {
    let x: Double
    let y: Double
    let width: Double
    let height: Double
    let cwd: String
    let sidebarVisible: Bool
    let toolbarMode: StateSeam.ToolbarMode?
}

/// Owns and drives a single top-level window and its embedded Neovim. Created by
/// the `AppDelegate` coordinator (once per window); `start()` builds and shows the
/// window. Wake routing reaches this controller through `WindowRegistry` keyed on
/// the core's `wakeToken`.
final class WolfWindowController: NSObject, NSWindowDelegate {
    /// Initial grid width in columns.
    private static let initialCols = 80
    /// Initial grid height in rows.
    private static let initialRows = 24

    /// This window's fixed slot in `state.json`'s `windows` list (assigned by the
    /// coordinator; passed into `wolf_ffi_start` and every `state_*` setter).
    let windowIndex: Int

    /// The embedded Neovim's launch working directory (already resolved by the
    /// coordinator).
    private let launchCwd: String

    /// The saved facets to restore (frame/sidebar/toolbar), or `nil` for a fresh
    /// (Cmd-N) window.
    private let restored: RestoredWindowState?

    /// The shared, once-measured cell metrics (measured by the coordinator).
    private let metrics: CellMetrics

    /// The shared, once-loaded user config (measured/loaded by the coordinator).
    private let config: FfiConfig

    /// The coordinator, asked to open new windows and to drop this controller at
    /// teardown. Weak so the collection owns the lifetime.
    weak var coordinator: AppDelegate?

    /// The live core handle (`nil` before `start()` / after `teardown()`).
    private(set) var handle: WolfCoreHandle?
    /// The core's per-window wake-routing token (0 until `start()`).
    private(set) var wakeToken: UInt64 = 0

    private var window: MainWindow?
    private var paneContainer: PaneContainerView?
    /// Accessed by `WakeBridge` / `TreeWakeBridge` (via `WindowRegistry`) to
    /// refresh the sidebar on render/tree wakes.
    private(set) var tree: FileTreeView?
    /// Accessed by `PickerWakeBridge` (via `WindowRegistry`) to refresh the picker
    /// on file-search wakes.
    private(set) var picker: PickerView?
    private var statePersistence: StatePersistence?

    /// Guards `teardown()` against running twice (windowWillClose + the
    /// terminate-time backstop).
    private var isTornDown = false

    init(
        windowIndex: Int,
        launchCwd: String,
        restored: RestoredWindowState?,
        metrics: CellMetrics,
        config: FfiConfig
    ) {
        self.windowIndex = windowIndex
        self.launchCwd = launchCwd
        self.restored = restored
        self.metrics = metrics
        self.config = config
        super.init()
    }

    /// Builds this window's stack and shows it. Returns `false` (having built
    /// nothing) when the core fails to start, so the coordinator can decide
    /// whether to present a fatal alert (only for the FIRST window).
    func start() -> Bool {
        guard let handle = startCore() else {
            return false
        }
        self.handle = handle

        let wakeToken = handle.wake_token()
        self.wakeToken = wakeToken
        WindowRegistry.shared.register(self, token: wakeToken)

        let paneContainer = PaneContainerView(handle: handle, metrics: metrics)
        self.paneContainer = paneContainer

        // The file-tree sidebar (left pane) owns its OWN long-lived file-search
        // session; the render wake reveals/bolds the current buffer, the tree
        // wake rebuilds its rows.
        let tree = FileTreeView(handle: handle, paneContainer: paneContainer)
        tree.start()
        self.tree = tree

        let window = MainWindow(
            treeView: tree,
            paneContainer: paneContainer,
            metrics: metrics,
            cols: Self.initialCols,
            rows: Self.initialRows
        )

        // The hidden top-center picker overlay (a subview of the pane container).
        let picker = PickerView(handle: handle, paneContainer: paneContainer)
        paneContainer.addSubview(picker)
        paneContainer.onPickerTrigger = { [weak picker] in picker?.open() }
        paneContainer.onNewWindow = { [weak self] in self?.requestNewWindow() }
        self.picker = picker

        // Restore the saved frame/sidebar/toolbar only for a restored window;
        // a fresh (Cmd-N) window keeps AppKit's cascade placement.
        if let restored {
            if let frame = StateSeam.restoredWindowFrame(
                x: restored.x, y: restored.y,
                width: restored.width, height: restored.height,
                screenVisibleFrames: NSScreen.screens.map(\.visibleFrame))
            {
                window.setFrame(frame, display: false)
            } else {
                window.center()
            }
            window.sidebarItem.isCollapsed = !restored.sidebarVisible
            if let mode = restored.toolbarMode {
                window.toolbar?.displayMode = Self.toolbarDisplayMode(mode)
            }
        }

        window.delegate = self
        window.makeKeyAndOrderFront(nil)
        // Setting the content VC can move first responder; ensure the editor pane
        // container keeps it so launch keystrokes reach Neovim.
        window.makeFirstResponder(paneContainer)
        NSApp.activate(ignoringOtherApps: true)
        self.window = window

        // Observe geometry/sidebar/toolbar changes, then push one full snapshot so
        // this window persists its initial layout immediately.
        let persistence = StatePersistence(
            handle: handle,
            window: window,
            sidebarItem: window.sidebarItem,
            toolbar: window.toolbar
        )
        persistence.install()
        persistence.pushSnapshot()
        statePersistence = persistence

        return true
    }

    /// Drives this window's render wake: apply layout intents + redraw every pane
    /// and the cmdline strip, then reveal/bold the current buffer in the sidebar.
    func onRenderWake() {
        paneContainer?.onRenderWake()
        tree?.onRenderWake()
        drainAndDispatchCommands()
    }

    /// Drains this window's pending user-triggered commands (`:Wolf<Pascal>` /
    /// `require('wolf')` calls) and funnels each through `CommandDispatcher`,
    /// targeting THIS controller's own window/coordinator (so a command triggered
    /// in window A acts on window A) — the SAME dispatcher the native menu path
    /// uses. An unknown command kind is skipped.
    private func drainAndDispatchCommands() {
        guard let handle else { return }
        for cmd in handle.drain_commands() {
            guard let kind = CommandKind(ffi: cmd.kind()) else { continue }
            CommandDispatcher.dispatch(
                kind, window: window, coordinator: coordinator, originatingCwd: currentCwd)
        }
    }

    /// Re-pushes this window's layout snapshot (the terminate-time correctness
    /// backstop for the best-effort toolbar KVO).
    func pushSnapshot() {
        statePersistence?.pushSnapshot()
    }

    /// This window's LIVE working directory (empty when the core is not running);
    /// inherited by a new window opened from this one (Cmd-N / drained
    /// `:WolfNewWindow` / the New Window menu item when this is the key window).
    var currentCwd: String {
        handle?.cwd().toString() ?? ""
    }

    /// Whether `candidate` is this controller's own top-level window (used by the
    /// coordinator to find the key window's controller).
    func owns(_ candidate: NSWindow?) -> Bool {
        window === candidate
    }

    /// Asks the coordinator to open a new window rooted at this window's LIVE
    /// working directory (the coordinator applies the home-dir fallback when it is
    /// empty).
    private func requestNewWindow() {
        coordinator?.makeNewWindow(originatingCwd: currentCwd)
    }

    // MARK: - NSWindowDelegate

    func windowWillClose(_ notification: Notification) {
        teardown()
    }

    /// Tears this window down: push the final snapshot, drain the state writer,
    /// drop the sidebar's + picker's file-search sessions, shut the core down,
    /// unregister from the wake registry, and ask the coordinator to drop this
    /// controller. Idempotent (windowWillClose + terminate-time backstop).
    func teardown() {
        guard !isTornDown else { return }
        isTornDown = true

        statePersistence?.pushSnapshot()
        _ = handle?.state_flush()
        statePersistence = nil

        tree?.close()
        tree = nil
        picker?.close()
        picker = nil
        handle?.shutdown()
        handle = nil

        WindowRegistry.shared.unregister(token: wakeToken)

        // Drop this controller from the coordinator's collection on the NEXT
        // main-runloop turn, not synchronously. During `windowWillClose` the
        // coordinator's array is the last strong reference to `self`, and `self`
        // owns `window`; removing it here would deallocate the `NSWindow` while
        // AppKit is still mid-close (its deferred close animation captures the
        // window), causing a use-after-free. Deferring keeps the window alive
        // until AppKit's close transaction has finished. Capturing `self`
        // strongly is intentional — it is what keeps the window alive until then.
        DispatchQueue.main.async {
            self.coordinator?.removeController(self)
        }
    }

    // MARK: - Core start helpers

    /// Starts the Rust core against the bundled `init.lua` at this window's
    /// resolved `launchCwd` and fixed `windowIndex`.
    private func startCore() -> WolfCoreHandle? {
        // Point wolf-core at the bundled Neovim unless a developer has already
        // exported `WOLF_NVIM_BIN` (overwrite=0 so the dev override wins).
        if let bin = nvimBinPath() {
            setenv("WOLF_NVIM_BIN", bin, 0)
        }
        return wolf_ffi_start(
            initLuaPath(),
            launchCwd,
            UInt16(Self.initialCols),
            UInt16(Self.initialRows),
            UInt16(windowIndex)
        )
    }

    /// Maps the shared `StateSeam.ToolbarMode` onto AppKit's display mode.
    private static func toolbarDisplayMode(_ mode: StateSeam.ToolbarMode) -> NSToolbar.DisplayMode {
        switch mode {
        case .iconAndText: return .iconAndLabel
        case .iconOnly: return .iconOnly
        case .textOnly: return .labelOnly
        }
    }

    /// Resolves the bundled Neovim binary at `Contents/Resources/nvim/bin/nvim`,
    /// or `nil` when absent/not executable (so a `WOLF_NVIM_BIN` export wins).
    private func nvimBinPath() -> String? {
        guard let resourcePath = Bundle.main.resourcePath else { return nil }
        let candidate = resourcePath + "/nvim/bin/nvim"
        guard FileManager.default.isExecutableFile(atPath: candidate) else {
            return nil
        }
        return candidate
    }

    /// Resolves the bundled `runtime/nvim/init.lua`; a `WOLF_INIT_LUA` override is
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
}
