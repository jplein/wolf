// Owns one top-level window's stack: core binding, window, panes, sidebar, pickers.

import AppKit
import Foundation
import WolfFFI
import WolfKeyMap

/// One saved tab as the launch restores it, read out of the window state at launch. Each tab is
/// its own captive Neovim.
struct RestoredTabState {
  /// The tab as saved, which seeds the view's cache until it can gather its own.
  let saved: RestoreSeam.SavedTab
  /// The restored tab to rebuild, `nil` for one with no root or no folder.
  let rebuild: WindowStateCodec.Rebuild?
}

/// Where a controller's window opens before `build()` first orders it front.
enum WindowPlacement: Equatable {
  /// The built default size, where AppKit places it.
  case system
  /// A validated stored frame.
  case frame(CGRect)
  /// The active window's normal size, cascaded from its top-left (`specs/application-state.md`
  /// §7). A point rather than the window, so the controller retains no other window.
  case cascaded(topLeft: CGPoint, size: CGSize)
}

extension NSWindow {
  /// Places this new window by `specs/application-state.md` §7: gives it `size`, then cascades it
  /// down and right of `topLeft`, the active window's top-left. The size is set first, because
  /// `cascadeTopLeft(from:)` constrains the frame to the screen at the size it has when called.
  /// Only this window moves.
  func cascade(fromTopLeft topLeft: CGPoint, size: CGSize) {
    setFrame(NSRect(origin: frame.origin, size: size), display: false)
    // The first call puts this window's top-left on the active window's; the second offsets it.
    let cascaded = cascadeTopLeft(from: topLeft)
    _ = cascadeTopLeft(from: cascaded)
  }
}

/// Owns and drives a single top-level window (one tab) and its own captive Neovim.
/// Created by the `AppDelegate` coordinator (once per tab); `build()` builds and
/// shows the window. Wakes reach this controller through its `TabCore`, which
/// `WindowRegistry` resolves from the core's `wakeToken`.
final class WolfWindowController: NSObject, NSWindowDelegate, HistoryNavigating, TabCoreView {
  /// Initial grid width in columns.
  private static let initialCols = 80
  /// Initial grid height in rows.
  private static let initialRows = 24

  /// This tab's monotonic identity (assigned by the coordinator; used as the
  /// Window-menu item tag, the `raiseWindow`/`otherWindows` key and the stacking
  /// order's tab id).
  let tabId: Int

  /// This tab's captive Neovim. Held STRONG and shut down in `teardown()`; the core
  /// holds this view WEAK, so there is no cycle.
  let tabCore: TabCore

  /// The saved facets to restore (cwd already applied to the core's launch; the
  /// sidebar visibility is applied in `start()`), or `nil` for a fresh
  /// (Cmd-N / new-tab) window.
  private let restoredTab: RestoredTabState?

  /// Where `build()` puts this window before it first orders it front.
  private let placement: WindowPlacement

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

  /// The tab core's handle (`nil` before `build()` / after `teardown()`), kept
  /// optional so the `handle?.…` call sites go quiet once the tab is torn down.
  private(set) var handle: WolfCoreHandle?

  private var window: MainWindow?
  private var paneContainer: PaneContainerView?
  /// Accessed by `WakeBridge` / `TreeWakeBridge` (via `WindowRegistry`) to
  /// refresh the sidebar on render/tree wakes.
  private(set) var tree: FileTreeView?
  /// Accessed by `SearchWakeBridge` (via `WindowRegistry`) to refresh the sidebar
  /// search panel on content-search wakes.
  private(set) var search: SearchView?
  /// The sidebar body container (tree/search switch).
  private var sidebarContainer: SidebarContainerView?
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
  /// The panel hosts owning each picker's borderless child window (strong; the
  /// panel owns its picker view as `contentView`). Nulled in `teardown()` after
  /// each picker's `close()` dismisses its panel.
  private var pickerHost: PickerPanelHost?
  private var paletteHost: PickerPanelHost?
  private var bufferPickerHost: PickerPanelHost?
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
  /// terminate-time backstop). The coordinator's commit walk skips a torn-down view.
  private(set) var isTornDown = false

  /// The tab this view last contributed to a commit, seeded from the restored tab, so
  /// a commit while this view cannot be read still writes its saved files. See
  /// `persistedTab()`.
  private var persistedCache: RestoreSeam.SavedTab?

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
    tabId: Int,
    tabCore: TabCore,
    restoredTab: RestoredTabState?,
    placement: WindowPlacement,
    hasFolder: Bool,
    metrics: CellMetrics,
    config: FfiConfig
  ) {
    self.tabId = tabId
    self.tabCore = tabCore
    self.restoredTab = restoredTab
    self.placement = placement
    self.hasFolder = hasFolder
    self.metrics = metrics
    self.config = config
    // The passed-in shared metrics were already measured at this size, so seed
    // the default/current from the same resolution WITHOUT re-measuring.
    let seed = CellMetrics.defaultPointSize(config: config)
    self.defaultPointSize = seed
    self.currentPointSize = seed
    self.persistedCache = restoredTab?.saved
    super.init()
  }

  /// Builds this tab's stack on its core and shows the window. The coordinator
  /// starts the core (`AppDelegate.startTabCore`), never this method.
  func build() {
    let handle = tabCore.handle
    self.handle = handle

    let paneContainer = PaneContainerView(
      handle: handle, metrics: metrics, optionAsAlt: config.macos_option_as_alt())
    self.paneContainer = paneContainer

    // A divider drag (or a restore rebuild) persists the whole-tree snapshot.
    paneContainer.onLayoutChanged = { [weak self] in
      self?.coordinator?.scheduleStateSnapshot()
    }
    // Rebuild any saved layout, including a single leaf, which reopens its file or preview in the
    // tab's sole window.
    if let rebuild = restoredTab?.rebuild {
      paneContainer.requestLayoutRebuild(
        rebuild.layout, tab: rebuild.tab, focused: rebuild.focused, drawer: rebuild.drawer)
    }

    // The sidebar owns its own file-search session. `start()` waits for the end of this method,
    // after the `.initial` KVO seeds the refresh gate, so a collapsed sidebar never loads hidden.
    let tree = FileTreeView(handle: handle, paneContainer: paneContainer)
    self.tree = tree

    // The sidebar search panel, its own rg-backed session, behind the toolbar's search segment.
    let search = SearchView(handle: handle, paneContainer: paneContainer)
    self.search = search
    let sidebarContainer = SidebarContainerView(tree: tree, search: search)
    self.sidebarContainer = sidebarContainer

    let window = MainWindow(
      sidebar: sidebarContainer,
      paneContainer: paneContainer,
      metrics: metrics,
      cols: Self.initialCols,
      rows: Self.initialRows
    )
    // Back / Forward target this controller explicitly: it is not an NSWindowController, so it is
    // not in the responder chain and a `target: nil` action could never reach it.
    window.historyNav = self

    // Added first, so this full-bleed opaque overlay sits below the picker and palette overlays;
    // otherwise a palette opened in an empty window would render behind it.
    let emptyState = EmptyStateView()
    paneContainer.addSubview(emptyState)
    emptyState.onOpenFolder = { [weak self] in self?.presentOpenFolderDialog() }
    emptyState.isHidden = hasFolder
    self.emptyState = emptyState

    // The "Restart Neovim" button (on the crash/stall overlay) restarts this tab's Neovim (kill +
    // respawn + reopen the Tier-1 files).
    paneContainer.onRestart = { [weak self] in self?.tabCore.restartNeovim() }

    // Hosted in a child panel in front of this window, so it renders above the preview
    // `WKWebView`. The host is held strongly; resign-key dismisses through a weak capture.
    let picker = PickerView(handle: handle, paneContainer: paneContainer)
    let pickerHost = PickerPanelHost(
      parentWindow: window, paneContainer: paneContainer, contentView: picker,
      onResignKey: { [weak picker] in picker?.close() })
    picker.host = pickerHost
    self.pickerHost = pickerHost
    paneContainer.onPickerTrigger = { [weak picker] in picker?.open() }
    self.picker = picker

    // The command palette, in its own child panel; activation flows to `activatePaletteRow`.
    let palette = CommandPaletteView(handle: handle, paneContainer: paneContainer)
    let paletteHost = PickerPanelHost(
      parentWindow: window, paneContainer: paneContainer, contentView: palette,
      onResignKey: { [weak palette] in palette?.close() })
    palette.host = paletteHost
    self.paletteHost = paletteHost
    palette.onActivate = { [weak self] row in self?.activatePaletteRow(row) }
    self.palette = palette

    // The buffer picker, in its own child panel; activation switches the active pane's buffer.
    let bufferPicker = BufferPickerView(handle: handle, paneContainer: paneContainer)
    let bufferPickerHost = PickerPanelHost(
      parentWindow: window, paneContainer: paneContainer, contentView: bufferPicker,
      onResignKey: { [weak bufferPicker] in bufferPicker?.close() })
    bufferPicker.host = bufferPickerHost
    self.bufferPickerHost = bufferPickerHost
    bufferPicker.onActivate = { [weak self] row in self?.activateBufferRow(row) }
    self.bufferPicker = bufferPicker

    switch placement {
    case .system: break
    case .frame(let frame): window.setFrame(frame, display: false)
    case .cascaded(let topLeft, let size): window.cascade(fromTopLeft: topLeft, size: size)
    }

    // An empty-state window always hides the sidebar, ignoring the stored `sidebar_visible`; a
    // restored folder tab honors it, and a fresh folder keeps it visible.
    if !hasFolder {
      window.sidebarItem.isCollapsed = true
    } else if let restoredTab {
      window.sidebarItem.isCollapsed = !restoredTab.saved.sidebarVisible
      window.restoreSidebar(restoredTab.saved.sidebar)
    }
    // A mode change, a toggle click and Return wake no core, so each schedules a write itself.
    sidebarContainer.onChange = { [weak self] in self?.coordinator?.scheduleStateSnapshot() }

    window.delegate = self
    // A placed window opens standalone: automatic tabbing off for its first order-front keeps it
    // out of a full-screen window's tab group, which would open it full screen.
    let standalone = placement != .system
    if standalone { window.tabbingMode = .disallowed }
    window.makeKeyAndOrderFront(nil)
    if standalone { window.tabbingMode = .automatic }
    // Setting the content VC can move first responder; ensure the editor pane
    // container keeps it so launch keystrokes reach Neovim.
    window.makeFirstResponder(paneContainer)
    NSApp.activate(ignoringOtherApps: true)
    self.window = window

    // Each change schedules a coalesced whole-tree snapshot; the coordinator also schedules one
    // right after this window is added, so the initial layout persists.
    let persistence = StatePersistence(
      window: window,
      sidebarItem: window.sidebarItem,
      onChange: { [weak self] in self?.coordinator?.scheduleStateSnapshot() }
    )
    persistence.install()
    statePersistence = persistence

    // Relay the sidebar's collapse state into the refresh gate; `.initial` seeds it immediately.
    sidebarVisibilityObservation = window.sidebarItem.observe(
      \.isCollapsed, options: [.initial, .new]
    ) { [weak self] item, _ in
      self?.sidebarContainer?.setSidebarVisible(!item.isCollapsed)
    }

    // Start the tree last, now that the `.initial` KVO has seeded the gate. An empty-state window
    // never scans `home`; the tree reroots when a folder opens (see `didOpenFolder`).
    if hasFolder {
      tree.start()
    }

    // From here the core routes its wakes to this view.
    tabCore.view = self
  }

  /// Drives this window's render wake: apply layout intents + redraw every pane
  /// and the cmdline strip, then reveal/bold the current buffer in the sidebar.
  /// Paints only: the tab core calls `drainAndDispatchCommands()` after it, in
  /// every liveness state.
  func onRenderWake() {
    let frameStart = FrameMetrics.begin()
    defer { FrameMetrics.end(.controllerWake, frameStart) }

    paneContainer?.onRenderWake()
    // Two lock-free snapshot reads, no RPC. Must run after the pane container reconciles
    // `activePane`, so `historyWin` addresses the pane the user now sees.
    window?.refreshHistoryToolbar(canBack: historyCanBack, canForward: historyCanForward)
    // An empty-state window never scans or reroots; the first wake after `didOpenFolder()` reroots
    // the tree at the opened folder.
    if hasFolder {
      tree?.onRenderWake()
    }

    let cwd = handle?.cwd().toString() ?? ""

    // Detect a typed `:cd` out of the empty state against the first observed cwd (nvim reports
    // `home`). A `:cd` to that same dir does not transition.
    if !hasFolder, !cwd.isEmpty {
      if let baseline = initialCwd {
        if cwd != baseline {
          didOpenFolder()
        }
      } else {
        initialCwd = cwd
      }
    }

    // Re-derive tab titles every wake: a `:cd` changes a tab's cwd with no separate signal here.
    if let window {
      coordinator?.refreshTabTitles(forGroupOf: window)
    }

    // Rebuild the Window menu, whose titles are cwd paths, and persist only when this cwd changed.
    if lastKnownCwd != cwd {
      lastKnownCwd = cwd
      coordinator?.windowCwdDidChange(self)
      coordinator?.scheduleStateSnapshot()
    }
  }

  /// Shows this view's crash/stall overlay for `mode` (asked by the tab core when
  /// it reads a non-`Alive` liveness).
  func showNvimOverlay(_ mode: NvimStatusMode) {
    paneContainer?.showNvimOverlay(mode)
  }

  /// Hides this view's crash/stall overlay (liveness returned to `Alive`).
  func hideNvimOverlay() {
    paneContainer?.hideNvimOverlay()
  }

  /// Flashes this view's transient "Neovim was restarted" banner (asked by the tab
  /// core at the end of a successful restart; auto-dismisses on its own timer).
  func flashRestartNotice() {
    paneContainer?.flashRestartNotice()
  }

  /// Resets the pane container to one fresh pane after a crash-restart, which the
  /// next render wake binds to the new nvim's window.
  func resetAfterRestart() {
    paneContainer?.resetAfterRestart()
  }

  /// Refreshes this tab's picker on a file-search wake (session-local, so a wake
  /// while no picker is open is harmless).
  func onPickerWake() {
    picker?.scheduleRefresh()
  }

  /// Refreshes this tab's file-tree sidebar on a tree wake. `scheduleRefresh()` is
  /// version-gated and coalesced, so a wake with no new data does no real work.
  func onTreeWake() {
    tree?.scheduleRefresh()
  }

  /// Refreshes this tab's sidebar search panel on a content-search wake, coalesced
  /// and version-gated like `onTreeWake()`.
  func onSearchWake() {
    search?.scheduleRefresh()
  }

  /// This view's terminal panes' sessions and whether its drawer is open, captured
  /// BEFORE a restart drops the window bindings; nothing when no pane container exists.
  func capturedTerminals() -> (splits: [UInt64], drawerOpen: Bool) {
    paneContainer?.capturedTerminals() ?? ([], false)
  }

  /// Hands a crash-restart's terminal restore to the pane container, which runs it once
  /// this view's first pane binds.
  func requestTerminalRestore(_ steps: [LivenessSeam.TerminalRestoreStep]) {
    paneContainer?.requestTerminalRestore(steps)
  }

  /// Transitions this window from the empty state into an open project on a typed `:cd`:
  /// reveals the sidebar, hides the "Open folder…" overlay, and lets the file tree start
  /// (reroot at the new folder on the next render wake). Open Folder never lands here: it
  /// replaces the empty tab instead. Idempotent.
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
  ///
  /// Separate from `onRenderWake()` so `TabCore.onWake` drains in every liveness state;
  /// see `crates/wolf-macos/Sources/WolfKeyMap/README.md#livenessseamswift`.
  func drainAndDispatchCommands() {
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

  /// Opens the chosen row's buffer in the active pane, routed by the pane's
  /// mode: a PREVIEW pane previews the buffer (`preview_buffer`, falling back to
  /// a plain `switch_buffer` when the buffer is not previewable — the pane
  /// converts to edit mode); an EDIT pane switches to it (`switch_buffer`,
  /// window-scoped; the active window is resolved core-side).
  private func activateBufferRow(_ row: BufferPickerRow) {
    guard let handle else { return }
    if let paneContainer, paneContainer.activePaneIsPreview {
      let activeWin = paneContainer.activeWin
      if handle.preview_buffer(activeWin, row.bufnr) != 0 {
        paneContainer.onRenderWake()
        return
      }
    }
    handle.switch_buffer(row.bufnr)
  }

  /// Performs a chosen palette row's action, dispatched by its kind:
  /// - `.staticCommand`: decode the FFI `command_kind` and funnel through
  ///   `CommandDispatcher` (the SAME funnel every other command uses).
  /// - `.switchWindow`: raise the window at `windowIndex` (a no-op if it closed).
  /// - `.openRecent`: Open Folder on that folder from this view
  ///   (`AppDelegate.openFolder(_:from:)`), as File > Open Recent does.
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
      coordinator?.openFolder(row.path, from: self)
    }
  }

  /// Presents a native `NSOpenPanel` (files only) as a sheet on this window and,
  /// on confirmation, opens the chosen file in this window's Neovim through
  /// `openChosenFile`. Then returns first responder to the editor pane.
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
      self?.openChosenFile(url.path)
      self?.refocusEditor()
    }
  }

  /// Opens `path`, chosen in the open dialog, through
  /// `PaneContainerView.openPickedFile`, or `handle.open` when there is no container.
  private func openChosenFile(_ path: String) {
    guard let handle else { return }
    if let paneContainer {
      paneContainer.openPickedFile(path)
    } else {
      handle.open(path)
    }
  }

  /// Presents a native `NSOpenPanel` (directories only) as a sheet on this window
  /// and, on confirmation, hands the chosen folder to Open Folder
  /// (`AppDelegate.openFolder(_:from:)`) from this view.
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
      guard response == .OK, let url = panel.url, let self else { return }
      self.coordinator?.openFolder(url.path, from: self)
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

  /// Closes this window's active pane, a split or the drawer, through
  /// `PaneContainerView.closeActivePane()`, which refuses what `canClosePane` refuses.
  func closePane() {
    paneContainer?.closeActivePane()
  }

  /// Splits this window's active pane through `PaneContainerView.splitActivePane(_:)`.
  func splitPane(_ orientation: Orientation) {
    paneContainer?.splitActivePane(orientation)
  }

  /// Opens a new empty buffer in this window's current Neovim window (`:enew`).
  func newBuffer() {
    handle?.new_buffer()
  }

  /// Toggles this window's terminal drawer (`:WolfTerminal`) through
  /// `PaneContainerView.toggleTerminal()`, which unzooms first.
  func toggleTerminal() {
    paneContainer?.toggleTerminal()
  }

  /// Zoom Pane: toggles this window's active pane between zoomed and not, through
  /// `PaneContainerView.toggleZoom()`.
  func zoomPane() {
    paneContainer?.toggleZoom()
  }

  /// Toggles the active buffer's content width between the ~90-char default and
  /// full pane width (`:WolfFullWidth`). Core flips `b:wolf_full_width` on the
  /// effective buffer (the SOURCE buffer for a preview scratch) and re-emits
  /// the current-file / per-window full-width / previews notifications; a repaint is
  /// requested so the editor + preview panes resize promptly on the next wake.
  func toggleFullWidth() {
    handle?.toggle_full_width()
    paneContainer?.onRenderWake()
  }

  // MARK: - Edit commands

  /// Copies the current selection to the system clipboard, from whichever source the
  /// pure `PreviewCopySeam.copySource` picks — the active terminal pane's session, the
  /// active pane's preview page, or the captive Neovim's visual-mode selection. This is a
  /// mechanical switch
  /// over that decision, and `canCopy` (which `AppDelegate.validateMenuItem` reads)
  /// is the same decision, so the menu can never claim enabled while Copy yields
  /// nothing.
  ///
  /// The preview branch is fully SYNCHRONOUS: the text was already pushed out of the
  /// page and cached, so nothing is evaluated or awaited here. It deliberately does
  /// NOT call `copy_selection()` — that is a blocking RPC that yanks INSIDE Neovim,
  /// and it would touch registers for a selection that is not Neovim's.
  func copy() {
    switch PreviewCopySeam.copySource(
      previewSelection: activePreviewSelection, isVisualMode: isVisualMode,
      terminalSelection: paneContainer?.activeTerminalSelection)
    {
    case .previewSelection(let text), .terminalSelection(let text):
      writeToPasteboard(text)
    case .neovimVisual:
      // Yanks out of the captive Neovim (blocking briefly), and writes only when the
      // returned text is non-empty.
      guard let text = handle?.copy_selection().toString(), !text.isEmpty else { return }
      writeToPasteboard(text)
    case .none:
      return
    }
  }

  /// Replaces `NSPasteboard.general`'s contents with `text`. Shared by both Copy
  /// sources so the two can never write the pasteboard differently.
  private func writeToPasteboard(_ text: String) {
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    pasteboard.setString(text, forType: .string)
  }

  /// Pastes the system clipboard's string into the active terminal pane's session, or
  /// else into the active document via `nvim_paste`. A no-op when the clipboard holds
  /// no string.
  func paste() {
    guard let text = NSPasteboard.general.string(forType: .string) else { return }
    if let session = paneContainer?.activeTerminal {
      handle?.terminal_paste(session, text)
    } else {
      handle?.paste(text)
    }
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

  /// The active sidebar panel (file tree vs. content search), or `nil` before the
  /// window is built. Drives the View-menu "Project" / "Search" checkmarks via
  /// `SidebarMenuSeam`.
  var sidebarPanel: SidebarPanel? {
    window?.sidebarPanel
  }

  /// Reveals the sidebar (if collapsed), switches it to `panel` and focuses the
  /// search field for `.search`, through `MainWindow.showSidebar(panel:)`, which
  /// also refreshes the toolbar's sidebar control.
  func showSidebar(panel: SidebarPanel) {
    window?.showSidebar(panel: panel)
  }

  /// Toggles the sidebar (collapse/expand) via the window's animated
  /// `NSSplitViewController.toggleSidebar(_:)` path, which flips the sidebar
  /// item's `isCollapsed` so `StatePersistence`'s KVO still relays visibility.
  func toggleSidebar() {
    window?.toggleSidebar()
  }

  /// The cwd this view surfaces, in tab titles, the Window menu and the palette, and the
  /// directory Open Folder and a file's project count it as open on: its own Neovim's live
  /// cwd, or its launch directory before Neovim reports one. An empty-state view answers `""`.
  var persistedCwd: String {
    OpenFolderSeam.liveDirectory(
      hasFolder: hasFolder, reportedCwd: currentCwd, launchCwd: tabCore.launchCwd)
  }

  /// Opens `path`, a file opened from outside Wolf, in this tab's pane by
  /// `specs/cli-and-file-open.md` §5.2, and raises the tab with the pane container focused.
  /// An empty tab hides its empty state so the file shows, and stays a tab with no folder.
  /// `wait`, a drained item holding a `wolf --wait`, goes to this tab's core with the file.
  func openFile(_ path: String, wait: FfiOpenItem? = nil) {
    if !hasFolder { emptyState?.isHidden = true }
    paneContainer?.openFile(path, wait: wait)
    raise()
    if let paneContainer {
      window?.makeFirstResponder(paneContainer)
    }
  }

  /// This tab's top-level NSWindow (for tab grouping + the snapshot walk), or
  /// `nil` before the window is built / after teardown.
  var nsWindow: NSWindow? {
    window
  }

  /// The window's last recorded normal frame, un-zoomed and not full screen, or `nil` when none
  /// was recorded (`StatePersistence.normalFrame`).
  var normalFrame: CGRect? {
    statePersistence?.normalFrame
  }

  /// Whether `candidate` is this controller's own top-level window (used by the
  /// coordinator to find the key window's controller).
  func owns(_ candidate: NSWindow?) -> Bool {
    window === candidate
  }

  /// Whether Close Pane may close this window's active pane
  /// (`PaneContainerView.canClosePane`). Read by the coordinator's menu validation.
  var canClosePane: Bool { paneContainer?.canClosePane ?? false }

  /// The tab this view contributes to a state commit (`AppDelegate.snapshotNow`):
  /// its live state when `SessionSeam` can read all of it, otherwise the tab it last
  /// contributed, which starts as the restored tab. `nil` only for a fresh view that
  /// has never been readable. Each call stores its answer as the next fallback.
  func persistedTab() -> RestoreSeam.SavedTab? {
    let tab = SessionSeam.contribution(
      mayGather: SessionSeam.mayGather(
        hasFolder: hasFolder, restoredCwd: restoredTab?.saved.cwd,
        restoreFailed: paneContainer?.restoreFailed ?? false),
      gather: liveTab(), cache: persistedCache)
    persistedCache = tab
    return tab
  }

  /// Reads this view's live state into the tab it persists, each pane's view being its
  /// last capture.
  private func liveTab() -> SessionSeam.TabGather {
    guard let paneContainer, let handle else { return .refuse }
    return SessionSeam.gatherTab(
      hasFolder: hasFolder, cwd: handle.cwd().toString(),
      sidebarVisible: sidebarVisible, layout: paneContainer.paneLayout,
      content: paneContainer.leafContents(), zoomed: paneContainer.zoomedPaneId,
      views: paneContainer.paneViews, active: paneContainer.activePaneId,
      drawer: SessionSeam.LiveDrawer(
        pane: paneContainer.drawerPane, share: paneContainer.drawerShare),
      sidebar: window?.savedSidebar ?? persistedCache?.sidebar ?? RestoreSeam.SavedSidebar())
  }

  /// Captures every pane's cursor, top line or scroll (§4), as a tab switch, a window close
  /// and the quit do. A no-op once torn down.
  func captureViewports() {
    guard !isTornDown else { return }
    paneContainer?.captureViewports()
  }

  /// Whether this window's active document is in Neovim visual mode
  /// (`v`/`V`/`<C-v>`). One of the two inputs to `canCopy`; it gates only the
  /// NEOVIM source of Edit > Copy.
  var isVisualMode: Bool { handle?.is_visual_mode() ?? false }

  /// The selection this window's ACTIVE pane's preview page last pushed, or `nil`
  /// when the active pane is not a preview or reported no selection. The other input
  /// to `canCopy`, and one dictionary read (see
  /// `PaneContainerView.activePaneSelection` for why it keys on the per-view active
  /// pane).
  var activePreviewSelection: PreviewCopySeam.Selection? { paneContainer?.activePaneSelection }

  /// Whether Edit > Copy has anything to copy, from any source. Read by the
  /// coordinator's menu validation; `copy()` switches over the same pure decision
  /// with the same inputs, so enablement and action cannot drift apart.
  var canCopy: Bool {
    PreviewCopySeam.isCopyEnabled(
      previewSelection: activePreviewSelection, isVisualMode: isVisualMode,
      terminalSelection: paneContainer?.activeTerminalSelection)
  }

  /// Whether this window's active buffer is a Markdown file. Read by the
  /// coordinator's menu validation to enable the View > Preview command only on
  /// a Markdown buffer.
  var isMarkdownActive: Bool { handle?.is_markdown_active() ?? false }

  /// Whether this window's active pane is a PREVIEW pane. Read by the
  /// coordinator's menu validation to enable the View > Edit command only
  /// from a preview pane.
  var isPreviewActive: Bool { handle?.is_preview_active() ?? false }

  /// Whether this window's active buffer is in FULL-WIDTH mode. Read by the
  /// coordinator's menu validation to show the View > Use full width for content
  /// checkmark (the item is always enabled).
  var isFullWidthActive: Bool { handle?.is_full_width_active() ?? false }

  /// The nvim WINDOW handle addressed by the History menu and the Back / Forward
  /// toolbar buttons: this VIEW's active pane, falling back to core's global
  /// `active_win()` before the container has bound one. `0` when neither resolves.
  ///
  /// The container's own `activeWin` is PREFERRED because `activePane` is the pane
  /// the user is looking at. The terminal drawer's pane IS in `panes`, but core never
  /// gives it history, so `historyCanBack` correctly reports `false` when it is
  /// focused.
  var historyWin: Int64 {
    let win = paneContainer?.activeWin ?? 0
    return win != 0 ? win : (handle?.active_win() ?? 0)
  }

  /// Whether the active pane has an older entry to go Back to. A cheap snapshot
  /// read (no RPC), safe to call on every render wake.
  var historyCanBack: Bool {
    let win = historyWin
    guard win != 0 else { return false }
    return handle?.history_can_back(win) ?? false
  }

  /// Whether the active pane has a newer entry to go Forward to. See
  /// ``historyCanBack``.
  var historyCanForward: Bool {
    let win = historyWin
    guard win != 0 else { return false }
    return handle?.history_can_forward(win) ?? false
  }

  /// Moves the active pane one entry BACK in its navigation history, then returns
  /// focus to the editor (the click/menu activation moved it to the chrome).
  func historyBack() {
    let win = historyWin
    guard win != 0 else { return }
    _ = handle?.history_back(win)
    refocusEditor()
  }

  /// Moves the active pane one entry FORWARD. See ``historyBack()``.
  func historyForward() {
    let win = historyWin
    guard win != 0 else { return }
    _ = handle?.history_forward(win)
    refocusEditor()
  }

  /// Moves the active pane to the STACK index `index` (a `HistorySeam.Row.index`
  /// carried on the clicked menu item's `tag`). A negative index — or the
  /// checkmarked row's own index — is a no-op.
  ///
  /// The `UInt32` conversion is non-trapping (`exactly:`): every row index
  /// originates from a `u32` on the Rust side, so an out-of-range `Int` is
  /// unreachable today, but an `NSMenuItem.tag` is a plain `Int` that any future
  /// caller could set — a trap here would crash the app rather than no-op.
  func historyJump(index: Int) {
    let win = historyWin
    guard win != 0, index >= 0, let index32 = UInt32(exactly: index) else { return }
    _ = handle?.history_jump(win, index32)
    refocusEditor()
  }

  /// The active pane's History-menu rows, NEWEST-FIRST with at most one checkmark.
  ///
  /// Wolf holds NO copy of the history: this pulls core's finished, disambiguated
  /// rows through the pure `HistorySeam.rows` zip on every `menuNeedsUpdate`.
  func historyRows() -> [HistorySeam.Row] {
    let win = historyWin
    guard win != 0, let handle else { return [] }
    let entries = handle.history_entries(win)
    return HistorySeam.rows(
      displays: entries.map { $0.display().toString() },
      indices: entries.map { Int($0.index()) },
      currentFlags: entries.map { $0.is_current() })
  }

  /// Previews the active source buffer IN PLACE: the active pane becomes the
  /// preview pane (core's `open_preview()`, no placement decision).
  ///
  /// The front-end learns the source↔preview mapping on the next wake (via
  /// `previews()`), so no synchronous window handle is needed here; a repaint is
  /// requested so the web view surfaces promptly.
  func openPreview() {
    guard let handle, let paneContainer else { return }
    _ = handle.open_preview()
    paneContainer.onRenderWake()
  }

  /// Navigates from the active PREVIEW pane back to its source file IN PLACE:
  /// the active pane becomes an edit pane (core's `edit_source()`, no placement
  /// decision — the inverse of `openPreview`).
  ///
  /// The front-end learns the updated snapshot on the next wake (via
  /// `previews()` / the current-file notification), so no synchronous window
  /// handle is needed here; a repaint is requested so the editor surfaces
  /// promptly.
  func editSource() {
    guard let handle, let paneContainer else { return }
    _ = handle.edit_source()
    paneContainer.onRenderWake()
  }

  /// Previews the active pane's Markdown file in ANOTHER pane
  /// (`:WolfPreviewInOtherPane`): the container resolves the target — prefer the
  /// pane to the RIGHT, else any other pane, else a split (`0`) — and core
  /// performs the bind (or the atomic split+bind) via `preview_in_other`. Focus
  /// stays in the source-editing pane. A no-op when the resolution is `nil` (the
  /// drawer is focused) or the handle is gone.
  func previewInOtherPane() {
    guard let handle, let paneContainer,
      let target = paneContainer.otherPaneTarget(prefer: .right)
    else { return }
    _ = handle.preview_in_other(target)
    paneContainer.onRenderWake()
  }

  /// Edits the active preview pane's SOURCE file in ANOTHER pane
  /// (`:WolfEditInOtherPane`): the container resolves the target — prefer the
  /// pane to the LEFT, else any other pane, else a split (`0`) — and core
  /// performs the bind (or the atomic split+bind) via `edit_in_other`, focusing
  /// the pane now editing the source. A no-op when the resolution is `nil` (the
  /// drawer is focused) or the handle is gone.
  func editInOtherPane() {
    guard let handle, let paneContainer,
      let target = paneContainer.otherPaneTarget(prefer: .left)
    else { return }
    _ = handle.edit_in_other(target)
    paneContainer.onRenderWake()
  }

  /// Renders the active Markdown buffer and opens it in the system default web
  /// browser (`:WolfPreviewInBrowser`).
  ///
  /// Core lazily starts a loopback-only HTTP server pinned at the active markdown
  /// buffer and returns its `http://127.0.0.1:<port>/` URL (an empty string when
  /// the active buffer is not markdown / on error). On a non-empty, parseable URL
  /// the front-end hands it to `NSWorkspace.shared.open`, which opens the user's
  /// default browser. A no-op when the handle is gone or the URL is empty/invalid.
  func openInBrowser() {
    guard let handle else { return }
    let url = handle.open_in_browser().toString()
    guard !url.isEmpty, let u = URL(string: url) else { return }
    NSWorkspace.shared.open(u)
  }

  /// Raises and activates this window, selecting its tab in its group (the Window menu, Switch
  /// to Window and Open Folder's focus). A minimized window is deminiaturized first, since
  /// ordering it front alone leaves it in the Dock.
  func raise() {
    if window?.isMiniaturized == true { window?.deminiaturize(nil) }
    window?.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
  }

  // MARK: - NSWindowDelegate

  func windowWillClose(_ notification: Notification) {
    teardown()
  }

  /// This view became key: the active terminal pane's focus report goes out, on every key change.
  func windowDidBecomeKey(_ notification: Notification) {
    paneContainer?.syncTerminalFocus()
  }

  /// This view's window became main, so its tab may have gained focus: the coordinator
  /// recomputes which tab is focused and reports the edges to the tabs' Neovims.
  func windowDidBecomeMain(_ notification: Notification) {
    coordinator?.refreshTabFocus()
  }

  /// This view's window was minimized, which minimizes its whole tab group.
  func windowDidMiniaturize(_ notification: Notification) {
    coordinator?.windowMinimizedChanged(self, minimized: true)
  }

  /// This view's window was restored from the Dock.
  func windowDidDeminiaturize(_ notification: Notification) {
    coordinator?.windowMinimizedChanged(self, minimized: false)
  }

  /// This view stopped being key, which app deactivation also causes: the active
  /// terminal pane's session hears that it lost focus.
  func windowDidResignKey(_ notification: Notification) {
    paneContainer?.syncTerminalFocus()
  }

  /// Tears this VIEW down. It decides its close commit (`AppDelegate.closeCommit(for:)`)
  /// while it still counts as live, then drops the state writer, the sidebar's and
  /// picker's file-search sessions and the palette session. It shuts this tab's
  /// core down, and asks the coordinator to drop this controller. Idempotent
  /// (windowWillClose + terminate-time backstop).
  func teardown() {
    guard !isTornDown else { return }
    // Decided while this view still counts as live, so the burst's read and the last-view
    // count include it.
    let commit = coordinator?.closeCommit(for: self) ?? .skip
    isTornDown = true

    // The coalesced commit walks only live views, so it persists the tree without this one.
    if commit == .dropSubtree {
      coordinator?.scheduleStateSnapshot()
    }

    statePersistence = nil

    sidebarVisibilityObservation?.invalidate()
    sidebarVisibilityObservation = nil

    tree?.close()
    tree = nil
    search?.close()
    search = nil
    sidebarContainer = nil
    picker?.close()
    picker = nil
    palette?.close()
    palette = nil
    bufferPicker?.close()
    bufferPicker = nil
    // Each `close()` above already dismissed its panel (removing the child
    // window); drop the hosts so no dangling panel survives.
    pickerHost = nil
    paletteHost = nil
    bufferPickerHost = nil

    tabCore.shutdown()
    handle = nil

    // Remove on the next runloop turn: the coordinator holds the last strong ref, and removing now
    // frees the `NSWindow` mid-close, a use-after-free. The strong `self` capture is intentional.
    DispatchQueue.main.async {
      self.coordinator?.removeController(self)
    }
  }
}
