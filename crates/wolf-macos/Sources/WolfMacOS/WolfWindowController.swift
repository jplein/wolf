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
/// reads `state.json`) and the controller (which restores frame/sidebar).
struct RestoredWindowState {
    let x: Double
    let y: Double
    let width: Double
    let height: Double
    let cwd: String
    let sidebarVisible: Bool
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

    /// The saved facets to restore (frame/sidebar), or `nil` for a fresh
    /// (Cmd-N) window.
    private let restored: RestoredWindowState?

    /// The current user's home directory (the SAME value `StateSeam.launchDecision`
    /// uses), passed to `WindowTitleSeam.title` so the window title abbreviates a
    /// cwd at/under home to "~"/"~/…".
    private let homeDir: String

    /// Whether this window has a real folder open. `false` for the empty state (a
    /// fresh first run, a Cmd-N window, or a restored window whose saved cwd was
    /// empty/deleted): the sidebar stays hidden, the file tree is not started, and
    /// the centered "Open folder…" overlay is shown until a folder opens. Flipped
    /// to `true` (once) by `didOpenFolder()`.
    private var hasFolder: Bool

    /// This window's cell metrics — seeded from the shared, once-measured launch
    /// metrics and RE-MEASURED per-window on a font-size change (View menu). A
    /// `var` so `setPointSize` can swap it.
    private var metrics: CellMetrics

    /// The shared, once-loaded user config (measured/loaded by the coordinator).
    private let config: FfiConfig

    /// The launch/default font size (the Reset target), resolved from the SAME
    /// `WOLF_FONT_PT`/`config.font_size` path the shared launch metrics used, so a
    /// Reset returns to exactly the size the window opened at.
    private let defaultPointSize: CGFloat

    /// This window's CURRENT font size (ephemeral, per-window). Starts at
    /// `defaultPointSize`; Increase/Decrease step it, Reset returns it to the
    /// default.
    private var currentPointSize: CGFloat

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
    /// The command-palette overlay (⌘P / the View-menu item). No wake bridge — its
    /// candidates are fixed at open and re-filtered synchronously per keystroke.
    private(set) var palette: CommandPaletteView?
    /// The buffer-picker overlay (⌘E / the View-menu item). No wake bridge — its
    /// candidates are snapshotted at open and re-filtered synchronously per
    /// keystroke.
    private(set) var bufferPicker: BufferPickerView?
    /// The centered "Open folder…" overlay, present only while `hasFolder == false`;
    /// hidden by `didOpenFolder()` on the empty-state → open-project transition.
    private var emptyState: EmptyStateView?
    private var statePersistence: StatePersistence?
    /// KVO on the sidebar split item's `isCollapsed`, relaying visibility into the
    /// file tree's refresh gate so it defers/catches-up its outline work. A second
    /// observer alongside `StatePersistence`'s (which persists disk state); this one
    /// keeps the tree's gating out of that focused concern. Invalidated in teardown.
    private var sidebarVisibilityObservation: NSKeyValueObservation?

    /// Guards `teardown()` against running twice (windowWillClose + the
    /// terminate-time backstop).
    private var isTornDown = false

    /// The last live cwd observed on a render wake, so the dynamic Window menu is
    /// rebuilt (via the coordinator) ONLY when this window's cwd actually changes
    /// (its menu title is the full cwd path). `nil` until the first wake reads one.
    private var lastKnownCwd: String?

    /// The FIRST non-empty raw `handle.cwd()` observed on a render wake — the
    /// baseline launch cwd (nvim reports `home` for an empty-state window). Used
    /// ONLY while `hasFolder == false` to detect an nvim-initiated `:cd` away from
    /// the launch dir, which flips the window into a folder (see `onRenderWake`).
    /// `nil` until the first non-empty cwd is observed.
    private var initialCwd: String?

    init(
        windowIndex: Int,
        launchCwd: String,
        restored: RestoredWindowState?,
        hasFolder: Bool,
        metrics: CellMetrics,
        config: FfiConfig
    ) {
        self.windowIndex = windowIndex
        self.launchCwd = launchCwd
        self.restored = restored
        self.homeDir = FileManager.default.homeDirectoryForCurrentUser.path
        self.hasFolder = hasFolder
        self.metrics = metrics
        self.config = config
        // The passed-in shared metrics were already measured at this size, so seed
        // the default/current from the same resolution WITHOUT re-measuring.
        let seed = CellMetrics.defaultPointSize(config: config)
        self.defaultPointSize = seed
        self.currentPointSize = seed
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
        // wake rebuilds its rows. It is only STARTED for a window that has a
        // folder — an empty-state window must not scan `home`; the tree starts
        // (reroots) when a folder later opens (see `didOpenFolder` / `onRenderWake`).
        // The `start()` (whose first `refresh()` may `reloadData`) is deferred to
        // the END of this method, AFTER the sidebar's real initial `isCollapsed` has
        // seeded the refresh gate (via the `.initial` KVO below) — so a restored,
        // COLLAPSED sidebar defers its first outline work instead of reloading while
        // hidden.
        let tree = FileTreeView(handle: handle, paneContainer: paneContainer)
        self.tree = tree

        let window = MainWindow(
            treeView: tree,
            paneContainer: paneContainer,
            metrics: metrics,
            cols: Self.initialCols,
            rows: Self.initialRows
        )

        // The centered "Open folder…" overlay (a subview of the pane container),
        // visible only in the empty state (`hasFolder == false`). Added FIRST so it
        // sits at the BOTTOM of the overlay stack: it is a full-bleed opaque overlay,
        // so the transient picker/palette overlays (added below) must sit ABOVE it —
        // otherwise opening the command palette in an empty window would render it
        // behind this overlay, with no visible effect.
        let emptyState = EmptyStateView()
        paneContainer.addSubview(emptyState)
        emptyState.onOpenFolder = { [weak self] in self?.presentOpenFolderDialog() }
        emptyState.isHidden = hasFolder
        self.emptyState = emptyState

        // The hidden top-center picker overlay (a subview of the pane container).
        let picker = PickerView(handle: handle, paneContainer: paneContainer)
        paneContainer.addSubview(picker)
        paneContainer.onPickerTrigger = { [weak picker] in picker?.open() }
        self.picker = picker

        // The hidden top-center command-palette overlay (a subview of the pane
        // container, parallel to the picker). Activation flows back through
        // `onActivate` to `activatePaletteRow`.
        let palette = CommandPaletteView(handle: handle, paneContainer: paneContainer)
        paneContainer.addSubview(palette)
        palette.onActivate = { [weak self] row in self?.activatePaletteRow(row) }
        self.palette = palette

        // The hidden top-center buffer-picker overlay (a subview of the pane
        // container, parallel to the palette). Activation flows back through
        // `onActivate` to switch the active pane's buffer.
        let bufferPicker = BufferPickerView(handle: handle, paneContainer: paneContainer)
        paneContainer.addSubview(bufferPicker)
        bufferPicker.onActivate = { [weak self] row in self?.activateBufferRow(row) }
        self.bufferPicker = bufferPicker

        // Restore the saved frame only for a restored window; a fresh
        // (Cmd-N) window keeps AppKit's cascade placement. The sidebar is set
        // below, gated on `hasFolder`.
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
        }

        // Sidebar visibility: an empty-state window (no folder) ALWAYS hides the
        // sidebar (ignoring any stored `sidebar_visible`); a restored FOLDER window
        // honors its stored `sidebar_visible`; a fresh folder keeps the default
        // (visible). `didOpenFolder()` reveals it on the empty → open transition.
        if !hasFolder {
            window.sidebarItem.isCollapsed = true
        } else if let restored {
            window.sidebarItem.isCollapsed = !restored.sidebarVisible
        }

        window.delegate = self
        window.makeKeyAndOrderFront(nil)
        // Setting the content VC can move first responder; ensure the editor pane
        // container keeps it so launch keystrokes reach Neovim.
        window.makeFirstResponder(paneContainer)
        NSApp.activate(ignoringOtherApps: true)
        self.window = window

        // Observe geometry/sidebar changes, then push one full snapshot so
        // this window persists its initial layout immediately.
        let persistence = StatePersistence(
            handle: handle,
            window: window,
            sidebarItem: window.sidebarItem
        )
        persistence.install()
        persistence.pushSnapshot()
        statePersistence = persistence

        // Relay the sidebar's collapse state into the file tree's refresh gate so
        // outline work is deferred while hidden and caught up on reappear. `.initial`
        // seeds the tree with the current visibility immediately.
        sidebarVisibilityObservation = window.sidebarItem.observe(
            \.isCollapsed, options: [.initial, .new]
        ) { [weak self] item, _ in
            self?.tree?.setSidebarVisible(!item.isCollapsed)
        }

        // Start the file tree LAST — only for a window that has a folder — now that
        // the refresh gate has been seeded with the sidebar's real initial
        // visibility (the `.initial` KVO above fired synchronously on install). A
        // VISIBLE sidebar does its first refresh + initial root-expand normally; a
        // restored COLLAPSED sidebar defers, then runs exactly one catch-up refresh
        // when it is first shown. An empty-state window never scans `home`; the tree
        // starts (reroots) when a folder later opens (see `didOpenFolder`).
        if hasFolder {
            tree.start()
        }

        return true
    }

    /// Drives this window's render wake: apply layout intents + redraw every pane
    /// and the cmdline strip, then reveal/bold the current buffer in the sidebar.
    func onRenderWake() {
        paneContainer?.onRenderWake()
        // The tree is driven only for a window that has a folder — an empty-state
        // window never scans/reroots (TreeWakeBridge already no-ops while its
        // search is nil). `didOpenFolder()` flips `hasFolder`, so the FIRST wake
        // after a folder opens reroots the tree at the opened folder.
        if hasFolder {
            tree?.onRenderWake()
        }

        let cwd = handle?.cwd().toString() ?? ""

        // Detect an nvim-initiated `:cd` out of the empty state: baseline the first
        // non-empty observed cwd (nvim reports `home` for an empty-state window),
        // then transition when a later non-empty cwd differs from that baseline.
        // The Open Folder button / Open Recent transition optimistically instead
        // (see `didOpenFolder`); this covers a typed `:cd`. A typed `:cd` to the
        // SAME launch dir won't transition (no cwd change) — the button is the
        // intended path and always transitions.
        if !hasFolder, !cwd.isEmpty {
            if let baseline = initialCwd {
                if cwd != baseline {
                    didOpenFolder()
                }
            } else {
                initialCwd = cwd
            }
        }

        // Set the OS window title from BOTH the live cwd and the current
        // `hasFolder` (which the transition block above may have just flipped).
        // Set unconditionally each wake — an idempotent assignment correct across
        // initial show, the empty→folder flip (cwd string may not change), and a
        // typed `:cd`; NOT gated by the `lastKnownCwd` guard below.
        updateWindowTitle(cwd: cwd)

        // Rebuild the dynamic Window menu (its titles are the windows' full cwd
        // paths) only when THIS window's live cwd actually changed.
        if lastKnownCwd != cwd {
            lastKnownCwd = cwd
            coordinator?.windowCwdDidChange(self)
        }
        drainAndDispatchCommands()
    }

    /// Sets the OS window title from the live `cwd`, this window's `hasFolder`, and
    /// the user's `homeDir` via the pure `WindowTitleSeam` (home-abbreviated to
    /// "~"/"~/…"; "Wolf" for an empty-state window). Thin AppKit glue.
    private func updateWindowTitle(cwd: String) {
        window?.title = WindowTitleSeam.title(cwd: cwd, hasFolder: hasFolder, homeDir: homeDir)
    }

    /// Transitions this window from the empty state into an open project: reveals
    /// the sidebar, hides the "Open folder…" overlay, and lets the file tree start
    /// (reroot at the opened folder on the next render wake). Idempotent — the
    /// second caller (both transition paths may fire) is a no-op.
    private func didOpenFolder() {
        guard !hasFolder else { return }
        hasFolder = true
        emptyState?.isHidden = true
        window?.sidebarItem.isCollapsed = false
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
                kind, window: window, coordinator: coordinator, originatingCwd: currentCwd,
                controller: self)
        }
    }

    // MARK: - File commands

    /// Opens the file-picker overlay (the `Go to File…` command / Cmd-O), mirroring
    /// the pane container's own picker trigger.
    func openFilePicker() {
        picker?.open()
    }

    /// Opens the command-palette overlay (the `Command Palette…` command / ⌘P),
    /// seeded with the OTHER open windows (from the coordinator, excluding this
    /// window) plus this window's recent dirs + command MRU (assembled core-side).
    func openCommandPalette() {
        let others = coordinator?.otherWindows(excluding: self) ?? []
        palette?.open(otherWindows: others)
    }

    /// Opens the buffer-picker overlay (the `Buffer Picker…` command / ⌘E),
    /// snapshotting this window's listed buffers (assembled core-side).
    func openBufferPicker() {
        bufferPicker?.open()
    }

    /// Switches the active pane to the chosen row's buffer via
    /// `WolfCoreHandle.switch_buffer` (window-scoped; the active window is resolved
    /// core-side).
    private func activateBufferRow(_ row: BufferPickerRow) {
        handle?.switch_buffer(row.bufnr)
    }

    /// Performs a chosen palette row's action, dispatched by its kind:
    /// - `.staticCommand`: decode the FFI `command_kind` and funnel through
    ///   `CommandDispatcher` (the SAME funnel every other command uses).
    /// - `.switchWindow`: raise the window at `windowIndex` (a no-op if it closed).
    /// - `.openRecent`: raise the window already rooted at that folder if one
    ///   exists, else chdir this window there (matching File > Open Recent).
    private func activatePaletteRow(_ row: CommandPaletteRow) {
        switch row.kind {
        case .staticCommand:
            guard let kind = CommandKind(ffi: row.commandKind) else { return }
            CommandDispatcher.dispatch(
                kind, window: window, coordinator: coordinator, originatingCwd: currentCwd,
                controller: self)
        case .switchWindow:
            coordinator?.raiseWindow(windowIndex: Int(row.windowIndex))
        case .openRecent:
            if let existing = coordinator?.controllerForCwd(row.path) {
                existing.raise()
            } else {
                changeDirectory(row.path)
            }
        }
    }

    /// Presents a native `NSOpenPanel` (files only) as a sheet on this window and,
    /// on confirmation, opens the chosen file in this window's Neovim (`handle.open`)
    /// then returns first responder to the editor pane.
    func presentOpenFileDialog() {
        guard let window else { return }
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        let cwd = currentCwd
        if !cwd.isEmpty {
            panel.directoryURL = URL(fileURLWithPath: cwd)
        }
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            self?.handle?.open(url.path)
            self?.refocusEditor()
        }
    }

    /// Presents a native `NSOpenPanel` (directories only) as a sheet on this window
    /// and, on confirmation, changes this window's Neovim working directory to the
    /// chosen folder (`handle.chdir`) then returns first responder to the editor pane.
    func presentOpenFolderDialog() {
        guard let window else { return }
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        let cwd = currentCwd
        if !cwd.isEmpty {
            panel.directoryURL = URL(fileURLWithPath: cwd)
        }
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            self?.handle?.chdir(url.path)
            // Optimistically transition out of the empty state (the button is the
            // intended path and always transitions, even to the launch dir).
            self?.didOpenFolder()
            self?.refocusEditor()
        }
    }

    /// The GLOBAL recent-directories MRU (newest-first), read from this window's
    /// core handle to build the File > Open Recent submenu. Returns `[]` when the
    /// handle is gone (before `start()` / after `teardown()`).
    func recentDirs() -> [String] {
        guard let handle else { return [] }
        var result: [String] = []
        for path in handle.recent_dirs() {
            result.append(path.as_str().toString())
        }
        return result
    }

    /// The File > Open Recent action: changes this window's Neovim working
    /// directory to `path` (`handle.chdir`) then returns first responder to the
    /// editor pane. Same effect as Open Folder, without a panel.
    func changeDirectory(_ path: String) {
        handle?.chdir(path)
        // Open Recent also transitions out of the empty state optimistically.
        didOpenFolder()
        refocusEditor()
    }

    /// Closes this window's current split, i.e. the pane (`:close`). On the last
    /// split `:close` errors harmlessly (E444); the menu item is additionally
    /// disabled there via `AppDelegate.validateMenuItem`.
    func closePane() {
        handle?.close_pane()
    }

    /// Opens a new empty buffer in this window's current Neovim window (`:enew`).
    func newBuffer() {
        handle?.new_buffer()
    }

    /// Closes the active pane's active tab (the `Close Tab` command / ⌘W).
    ///
    /// Resolves the active window → its grid → its tab strip, finds the active
    /// tab, and closes it via `handle.close_tab(win, bufnr)`. A no-op when the
    /// core is gone, there is no active window, or the active pane has no active
    /// tab. On the last-tab-of-the-last-pane the core-side helper is a complete
    /// no-op (Decision #5); the menu item is additionally disabled there via
    /// `AppDelegate.validateMenuItem`.
    func closeTab() {
        guard let handle else { return }
        let win = handle.active_win()
        guard win != 0 else { return }
        let grid = handle.grid_for_win(win)
        guard grid != 0 else { return }
        let tabs = handle.pane_tabs_for_grid(grid)
        for i in 0..<tabs.len() {
            guard let tab = tabs.get(index: UInt(i)) else { continue }
            if tab.active() {
                handle.close_tab(win, tab.bufnr())
                return
            }
        }
    }

    // MARK: - Edit commands

    /// Copies the current visual-mode selection to the system clipboard. Yanks
    /// the selection out of the captive Neovim (blocking briefly) and, only when
    /// the returned text is non-empty, writes it to `NSPasteboard.general`. A
    /// no-op outside visual mode (the Edit > Copy item is additionally disabled
    /// there via `AppDelegate.validateMenuItem`).
    func copy() {
        guard let text = handle?.copy_selection().toString(), !text.isEmpty else { return }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }

    /// Pastes the system clipboard's string into the active document via
    /// `nvim_paste`. A no-op when the clipboard holds no string.
    func paste() {
        guard let text = NSPasteboard.general.string(forType: .string) else { return }
        handle?.paste(text)
    }

    // MARK: - View (font size) commands

    /// Resets this window's font size to its launch default (the View-menu Reset
    /// command / Cmd-0).
    func resetFontSize() {
        setPointSize(defaultPointSize)
    }

    /// Increases this window's font size by one point, clamped at the max (the
    /// View-menu Increase command / Cmd-=).
    func increaseFontSize() {
        setPointSize(CGFloat(FontSizeSeam.increased(Double(currentPointSize))))
    }

    /// Decreases this window's font size by one point, clamped at the min (the
    /// View-menu Decrease command / Cmd--).
    func decreaseFontSize() {
        setPointSize(CGFloat(FontSizeSeam.decreased(Double(currentPointSize))))
    }

    /// Re-measures this window's `CellMetrics` at `size` and reflows the grid,
    /// keeping the OS window frame unchanged (the grid's cols/rows reflow to fit).
    /// A no-op when `size` equals the current size (e.g. already at a clamp bound).
    private func setPointSize(_ size: CGFloat) {
        guard size != currentPointSize else { return }
        currentPointSize = size
        metrics = CellMetrics.measured(config: config, pointSize: size)
        paneContainer?.applyMetrics(metrics)
    }

    /// Returns first responder to the editor pane container so keystrokes reach
    /// Neovim after a sheet or overlay dismisses.
    private func refocusEditor() {
        if let paneContainer {
            window?.makeFirstResponder(paneContainer)
        }
    }

    /// Re-pushes this window's layout snapshot (the terminate-time correctness
    /// backstop for the best-effort sidebar KVO).
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

    /// This window's live pane count (`PaneTree` leaf count); 1 for an unsplit
    /// window. Read by the coordinator's menu validation to disable the Close
    /// command on the last split.
    var paneCount: Int { paneContainer?.paneCount ?? 1 }

    /// The number of tabs in this window's ACTIVE pane (its active Neovim
    /// window's tab strip); 0 when the core is gone or no active window/grid is
    /// known. Read by the coordinator's menu validation to disable the Close Tab
    /// command at the last-pane-last-tab (where it would be a no-op).
    var activeTabCount: Int {
        guard let handle else { return 0 }
        let win = handle.active_win()
        guard win != 0 else { return 0 }
        let grid = handle.grid_for_win(win)
        guard grid != 0 else { return 0 }
        return Int(handle.pane_tabs_for_grid(grid).len())
    }

    /// Whether this window's active document is in Neovim visual mode
    /// (`v`/`V`/`<C-v>`). Read by the coordinator's menu validation to enable
    /// the Edit > Copy command only when there is a selection to copy.
    var isVisualMode: Bool { handle?.is_visual_mode() ?? false }

    /// Raises and activates this window (the Window-menu dynamic list item's
    /// action, routed by the coordinator from the item's `windowIndex` tag).
    func raise() {
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    // MARK: - NSWindowDelegate

    func windowWillClose(_ notification: Notification) {
        teardown()
    }

    /// Tears this window down: push the final snapshot, drain the state writer,
    /// drop the sidebar's + picker's file-search sessions and the palette session,
    /// shut the core down, unregister from the wake registry, and ask the
    /// coordinator to drop this controller. Idempotent (windowWillClose +
    /// terminate-time backstop).
    func teardown() {
        guard !isTornDown else { return }
        isTornDown = true

        statePersistence?.pushSnapshot()
        _ = handle?.state_flush()
        statePersistence = nil

        sidebarVisibilityObservation?.invalidate()
        sidebarVisibilityObservation = nil

        tree?.close()
        tree = nil
        picker?.close()
        picker = nil
        palette?.close()
        palette = nil
        bufferPicker?.close()
        bufferPicker = nil
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
