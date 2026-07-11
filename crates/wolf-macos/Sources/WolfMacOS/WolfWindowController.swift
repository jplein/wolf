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

/// The scalar facets of one saved TAB, copied OUT of the bridge's
/// `RustVec<FfiTabState>` immediately (element refs must not outlive the vec)
/// into a plain value that survives launch. Each tab is its own captive Neovim.
struct RestoredTabState {
    let cwd: String
    let sidebarVisible: Bool
}

/// One saved WINDOW GROUP: its geometry (points; AppKit bottom-left) plus its
/// ordered tabs. Copied OUT of the bridge's `RustVec<FfiWindowState>` at launch.
/// The coordinator restores each group by starting the FIRST tab's controller,
/// anchoring the window frame, then `addTabbedWindow`-ing the rest.
struct RestoredWindowGroup {
    let x: Double
    let y: Double
    let width: Double
    let height: Double
    let tabs: [RestoredTabState]
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

    /// This tab's monotonic identity (assigned by the coordinator; used as the
    /// Window-menu item tag and the `raiseWindow`/`otherWindows` key). Distinct
    /// from any `state.json` slot — persistence is now whole-tree, not per-index.
    let tabId: Int

    /// The PROJECT this view belongs to: the shared captive Neovim (one core, one
    /// buffer list) that this view's tabpage lives in. Held STRONG — the core must
    /// outlive this view's teardown while a sibling view still binds it; the
    /// project holds its views WEAK, so there is no cycle.
    let project: Project

    /// This view's bound nvim TABPAGE handle (the first view binds the project's
    /// implicit tabpage 1; a joining/restored view creates a fresh one).
    private(set) var tabpageHandle: Int64

    /// The saved facets to restore (cwd already applied to the project launch; the
    /// sidebar visibility is applied in `start()`), or `nil` for a fresh
    /// (Cmd-N / new-tab) window.
    private let restoredTab: RestoredTabState?

    /// The validated window frame to anchor this window at (only the FIRST tab of
    /// a restored group carries one); `nil` for a joining tab or a fresh window,
    /// which keeps AppKit's cascade placement.
    private let anchorFrame: CGRect?

    /// Whether this window has a real folder open. `false` for the empty state (a
    /// fresh first run, a Cmd-N window, or a restored window whose saved cwd was
    /// empty/deleted): the sidebar stays hidden, the file tree is not started, and
    /// the centered "Open folder…" overlay is shown until a folder opens. Flipped
    /// to `true` (once) by `didOpenFolder()`. Exposed so a new tab inherits it.
    private(set) var hasFolder: Bool

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

    /// A reference to the PROJECT's shared core handle (`nil` before `build()` /
    /// after `teardown()`). NOT owned — the project owns the handle's lifetime; a
    /// view only holds a reference and drops it at teardown (it NEVER shuts the
    /// core down itself). Kept optional so the existing `handle?.…` call sites are
    /// unchanged.
    private(set) var handle: WolfCoreHandle?

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

    /// This view's cached active-buffer path, refreshed from `handle.current_file()`
    /// at the TOP of every render wake (OD6). A NON-current view keeps its last
    /// cached value (a non-current tabpage's buffer cannot change without input),
    /// so tab titles stay frozen-accurate; a restored-but-never-focused view holds
    /// `""` until its first focus (a generic-label fallback — v1 limitation).
    private var cachedActiveBufferPath = ""

    init(
        tabId: Int,
        project: Project,
        tabpageHandle: Int64,
        restoredTab: RestoredTabState?,
        anchorFrame: CGRect?,
        hasFolder: Bool,
        metrics: CellMetrics,
        config: FfiConfig
    ) {
        self.tabId = tabId
        self.project = project
        self.tabpageHandle = tabpageHandle
        self.restoredTab = restoredTab
        self.anchorFrame = anchorFrame
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

    /// Builds this view's stack (bound to its project's shared core + its own
    /// tabpage) and shows the window. Returns `false` (having built nothing) when
    /// the tabpage bind is invalid, so the coordinator can drop the would-be view.
    /// The core itself is started once per PROJECT (`Project.start`), never here.
    func build() -> Bool {
        guard tabpageHandle != 0 else {
            return false
        }
        let handle = project.handle
        self.handle = handle

        let paneContainer = PaneContainerView(
            handle: handle, metrics: metrics, tabpageHandle: tabpageHandle)
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

        // Anchor the saved window frame only for the FIRST tab of a restored
        // group (the coordinator validated it via `StateSeam.restoredWindowFrame`
        // and passes it here); a joining tab or a fresh window keeps AppKit's
        // cascade placement.
        if let anchorFrame {
            window.setFrame(anchorFrame, display: false)
        }

        // Sidebar visibility: an empty-state window (no folder) ALWAYS hides the
        // sidebar (ignoring any stored `sidebar_visible`); a restored FOLDER tab
        // honors its stored `sidebar_visible`; a fresh folder keeps the default
        // (visible). `didOpenFolder()` reveals it on the empty → open transition.
        if !hasFolder {
            window.sidebarItem.isCollapsed = true
        } else if let restoredTab {
            window.sidebarItem.isCollapsed = !restoredTab.sidebarVisible
        }

        window.delegate = self
        window.makeKeyAndOrderFront(nil)
        // Setting the content VC can move first responder; ensure the editor pane
        // container keeps it so launch keystrokes reach Neovim.
        window.makeFirstResponder(paneContainer)
        NSApp.activate(ignoringOtherApps: true)
        self.window = window

        // Observe geometry / sidebar / tab-group changes; each schedules a
        // coalesced whole-tree snapshot on the coordinator (which walks every
        // live window/tab). The coordinator also schedules one right after this
        // window is added, so the initial layout persists.
        let persistence = StatePersistence(
            window: window,
            sidebarItem: window.sidebarItem,
            onChange: { [weak self] in self?.coordinator?.scheduleStateSnapshot() }
        )
        persistence.install()
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

        // Join the project's (weak) view list. The coordinator drives `setCurrent`
        // (and `windowDidBecomeKey` does too) so the project routes wakes here once
        // this view is the current one.
        project.attach(self)

        return true
    }

    /// Drives this window's render wake: apply layout intents + redraw every pane
    /// and the cmdline strip, then reveal/bold the current buffer in the sidebar.
    func onRenderWake() {
        // Cache this view's active-buffer path FIRST (OD6): tab titles read the
        // cached value per wake, so a non-current view keeps its last-known title.
        cachedActiveBufferPath = handle?.current_file().toString() ?? cachedActiveBufferPath

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

        // Refresh the tab titles for this window's whole tab group from each
        // tab's active buffer basename (with minimal disambiguation on a
        // collision) via the coordinator. Set unconditionally each wake — the
        // active buffer / current_file may change with no cwd change.
        if let window {
            coordinator?.refreshTabTitles(forGroupOf: window)
        }

        // Rebuild the dynamic Window menu (its titles are the windows' full cwd
        // paths) only when THIS window's live cwd actually changed, and persist
        // the tree (a `:cd` changes a tab's saved cwd).
        if lastKnownCwd != cwd {
            lastKnownCwd = cwd
            coordinator?.windowCwdDidChange(self)
            coordinator?.scheduleStateSnapshot()
        }
        drainAndDispatchCommands()
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
        // The empty→folder transition changes this tab's persisted cwd + sidebar.
        coordinator?.scheduleStateSnapshot()
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
            coordinator?.raiseWindow(tabId: Int(row.windowIndex))
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

    /// This window's LIVE working directory (empty when the core is not running);
    /// inherited by a new window/tab opened from this one (Cmd-N / ⌘T / drained
    /// `:WolfNewWindow` / `:WolfNewTab`).
    var currentCwd: String {
        handle?.cwd().toString() ?? ""
    }

    /// Whether this tab's file-tree sidebar is currently visible (its persisted
    /// `sidebar_visible`). `false` before the window is built / after teardown.
    var sidebarVisible: Bool {
        guard let window else { return false }
        return !window.sidebarItem.isCollapsed
    }

    /// The absolute path of this tab's active buffer, CACHED from the last render
    /// wake (OD6). A non-current view returns its last-known value (its buffer
    /// cannot change without input), so the coordinator's `TabTitleSeam` titles
    /// stay frozen-accurate for background tabs. `""` for an unnamed buffer or a
    /// restored-but-never-focused view.
    var activeBufferPath: String {
        cachedActiveBufferPath
    }

    /// This view's PERSISTED working directory for the state snapshot + any
    /// surfaced cwd (Window menu / palette). A folder view reads its tabpage's
    /// authoritative tab-local `:tcd` cwd via the blocking `cwd_for_tabpage`
    /// (correct even when this view is NOT current, unlike the live `handle.cwd()`
    /// which follows the current tabpage); an empty-state view persists `""` (the
    /// `hasFolder` gate preserves the "empty-state persists ''" contract).
    var persistedCwd: String {
        guard hasFolder, let handle else { return "" }
        return handle.cwd_for_tabpage(tabpageHandle).toString()
    }

    /// This tab's top-level NSWindow (for tab grouping + the snapshot walk), or
    /// `nil` before the window is built / after teardown.
    var nsWindow: NSWindow? {
        window
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

    /// This view became key: make it its project's current view so nvim renders
    /// THIS view's tabpage, and arm the pane container's one-shot refocus rebind
    /// (nvim churns grid ids across the switch; the container re-derives its
    /// grid bindings on the next wake reflecting this tabpage — OD2). Guarded so a
    /// redundant key notification for the already-current view is a no-op.
    func windowDidBecomeKey(_ notification: Notification) {
        guard project.currentView !== self else { return }
        project.setCurrent(self)
        paneContainer?.markRefocusRebind()
    }

    /// Tears this VIEW down: push the final snapshot, drain the state writer, drop
    /// the sidebar's + picker's file-search sessions and the palette session, close
    /// this view's tabpage on the shared core (non-last close), detach from the
    /// project (which shuts the core down only when THIS was the last view), and ask
    /// the coordinator to drop this controller. Idempotent (windowWillClose +
    /// terminate-time backstop). This view NEVER calls `handle.shutdown()` itself —
    /// the project owns the core's lifetime.
    func teardown() {
        guard !isTornDown else { return }
        isTornDown = true

        // This tab is going away; persist the remaining tree. Scheduling BEFORE
        // the deferred `removeController` is fine — the coalesced snapshot fires
        // after this controller is dropped, so the walk sees the surviving tabs.
        coordinator?.scheduleStateSnapshot()

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

        // Close this view's tabpage (a no-op when it is the LAST view — `detach`
        // then shuts the whole core down), then detach. Order matters: `closeTabpage`
        // must see this view still attached to know whether it is the last one.
        project.closeTabpage(for: self)
        project.detach(self)
        handle = nil

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
}
