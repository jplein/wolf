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
  /// The tab's saved pane-split layout, `nil` ONLY for an empty-state (unnamed,
  /// non-preview) lone pane; a lone pane showing a file or preview persists a
  /// single leaf.
  let layout: SavedPaneNode?
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
final class WolfWindowController: NSObject, NSWindowDelegate, HistoryNavigating {
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
  /// Accessed by `SearchWakeBridge` (via `WindowRegistry`) to refresh the sidebar
  /// search panel on content-search wakes.
  private(set) var search: SearchView?
  /// The sidebar body container (header toggle + tree/search switch).
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
      handle: handle, metrics: metrics, tabpageHandle: tabpageHandle,
      optionAsAlt: config.macos_option_as_alt())
    self.paneContainer = paneContainer

    // A divider drag (or a restore rebuild) persists the whole-tree snapshot.
    paneContainer.onLayoutChanged = { [weak self] in
      self?.coordinator?.scheduleStateSnapshot()
    }
    // Rebuild any saved layout on launch — including a single leaf, which just
    // re-opens its file (or preview) in the tab's sole window via the shared
    // rebuild path. (`leafCount` is always >= 1, so no guard is needed.)
    if let saved = restoredTab?.layout {
      paneContainer.requestLayoutRebuild(saved)
    }

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

    // The sidebar search panel (its own transient rg-backed session), shown when
    // the header's magnifying-glass toggle is active. Wrapped alongside the tree
    // in a `SidebarContainerView` whose header switches the sidebar body.
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
    // The toolbar's Back / Forward buttons delegate here explicitly: this
    // controller is NOT an NSWindowController, so it is not in the responder
    // chain and a `target: nil` action could never reach it.
    window.historyNav = self

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

    // The "Restart Neovim" button (on the crash/stall overlay) drives the
    // project-wide restart (kill + respawn + reopen the Tier-1 files per view).
    paneContainer.onRestart = { [weak self] in self?.project.restartNeovim() }

    // The top-center picker, hosted in its OWN borderless non-activating child
    // panel composited IN FRONT of this window (so it renders above the preview
    // `WKWebView`) — not a pane-container subview. The host is strong-held here;
    // the view is the panel's `contentView`, dismissed on resign-key via a WEAK
    // capture.
    let picker = PickerView(handle: handle, paneContainer: paneContainer)
    let pickerHost = PickerPanelHost(
      parentWindow: window, paneContainer: paneContainer, contentView: picker,
      onResignKey: { [weak picker] in picker?.close() })
    picker.host = pickerHost
    self.pickerHost = pickerHost
    paneContainer.onPickerTrigger = { [weak picker] in picker?.open() }
    self.picker = picker

    // The command-palette overlay, hosted in its own child panel (parallel to
    // the picker). Activation flows back through `onActivate` to
    // `activatePaletteRow`.
    let palette = CommandPaletteView(handle: handle, paneContainer: paneContainer)
    let paletteHost = PickerPanelHost(
      parentWindow: window, paneContainer: paneContainer, contentView: palette,
      onResignKey: { [weak palette] in palette?.close() })
    palette.host = paletteHost
    self.paletteHost = paletteHost
    palette.onActivate = { [weak self] row in self?.activatePaletteRow(row) }
    self.palette = palette

    // The buffer-picker overlay, hosted in its own child panel (parallel to the
    // palette). Activation flows back through `onActivate` to switch the active
    // pane's buffer.
    let bufferPicker = BufferPickerView(handle: handle, paneContainer: paneContainer)
    let bufferPickerHost = PickerPanelHost(
      parentWindow: window, paneContainer: paneContainer, contentView: bufferPicker,
      onResignKey: { [weak bufferPicker] in bufferPicker?.close() })
    bufferPicker.host = bufferPickerHost
    self.bufferPickerHost = bufferPickerHost
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
      self?.sidebarContainer?.setSidebarVisible(!item.isCollapsed)
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
    let frameStart = FrameMetrics.begin()
    defer { FrameMetrics.end(.controllerWake, frameStart) }

    paneContainer?.onRenderWake()
    // Re-enable/disable the Back + Forward toolbar buttons for THIS frame's active
    // pane (two cheap lock-free snapshot reads, no RPC — the same cost class as the
    // `handle?.cwd()` read below). Must run AFTER the pane container reconciles
    // `activePane`, so `historyWin` addresses the pane the user now sees.
    window?.refreshHistoryToolbar(canBack: historyCanBack, canForward: historyCanForward)
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
    // tab's working directory (abbreviating the home prefix to `~`) via the
    // coordinator. Set unconditionally each wake — a `:cd` changes a tab's cwd
    // with no separate signal here, so re-derive the titles every wake.
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

  /// Shows this view's crash/stall overlay for `mode` (fanned by the project when
  /// it reads a non-`Alive` liveness).
  func showNvimOverlay(_ mode: NvimStatusMode) {
    paneContainer?.showNvimOverlay(mode)
  }

  /// Hides this view's crash/stall overlay (liveness returned to `Alive`).
  func hideNvimOverlay() {
    paneContainer?.hideNvimOverlay()
  }

  /// Flashes this view's transient "Neovim was restarted" banner (fanned by the
  /// project at the end of a successful restart; auto-dismisses on its own timer).
  func flashRestartNotice() {
    paneContainer?.flashRestartNotice()
  }

  /// Rebinds this view to a FRESH tabpage after a crash-restart: updates the
  /// stored `tabpageHandle` and re-gates the pane container's refocus rebind so
  /// the next render wake repaints against the new nvim's tabpage.
  func rebind(toTabpage handle: Int64) {
    tabpageHandle = handle
    paneContainer?.rebind(tabpageHandle: handle)
  }

  /// This view's bound nvim tabpage's Tier-1 open-file set (captured BEFORE a
  /// restart wipes the mirror), or `nil` when the core is gone.
  func capturedOpenFiles() -> FfiOpenFiles? {
    handle?.open_files_for_tabpage(tabpageHandle)
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

  /// Toggles this window's per-project terminal drawer (`:WolfTerminal`).
  func toggleTerminal() {
    handle?.toggle_terminal()
  }

  /// Toggles the active buffer's content width between the ~90-char default and
  /// full pane width (`:WolfFullWidth`). Core flips `b:wolf_full_width` on the
  /// effective buffer (the SOURCE buffer for a preview scratch — OQ-1) and re-emits
  /// the current-file / per-window full-width / previews notifications; a repaint is
  /// requested so the editor + preview panes resize promptly on the next wake.
  func toggleFullWidth() {
    handle?.toggle_full_width()
    paneContainer?.onRenderWake()
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

  /// The active sidebar panel (file tree vs. content search), or `nil` when there
  /// is no sidebar container yet. Drives the View-menu "Project" /
  /// "Search" checkmarks via `SidebarMenuSeam`.
  var sidebarPanel: SidebarPanel? {
    guard let container = sidebarContainer else { return nil }
    switch container.mode {
    case .tree: return .tree
    case .search: return .search
    }
  }

  /// Reveals the sidebar (if collapsed) and switches it to `panel`. The
  /// `isCollapsed = false` write relays visibility to the container via KVO; the
  /// `setMode` call swaps the active child (and re-relays visibility so the
  /// newly-shown panel catches up any deferred refresh).
  func showSidebar(panel: SidebarPanel) {
    window?.sidebarItem.isCollapsed = false
    sidebarContainer?.setMode(panel == .tree ? .tree : .search)
    if panel == .search { sidebarContainer?.search.focusSearchField() }
  }

  /// Toggles the sidebar (collapse/expand) via the window's animated
  /// `NSSplitViewController.toggleSidebar(_:)` path, which flips the sidebar
  /// item's `isCollapsed` so `StatePersistence`'s KVO still relays visibility.
  func toggleSidebar() {
    window?.toggleSidebar()
  }

  /// This view's PERSISTED working directory for the state snapshot + any
  /// surfaced cwd (Window menu / palette). A folder view reads its tabpage's
  /// authoritative tab-local `:tcd` cwd via `cwd_for_tabpage` — now a cheap
  /// out-of-band snapshot read (no RPC; core is fed every tabpage's cwd via the
  /// `wolf_tab_cwds` notification), correct even when this view is NOT current,
  /// unlike the live `handle.cwd()` which follows the current tabpage; an
  /// empty-state view persists `""` (the `hasFolder` gate preserves the
  /// "empty-state persists ''" contract).
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

  /// This tab's persistable pane-split layout (topology + weights + per-leaf buffer
  /// path), `nil` ONLY for an empty-state (unnamed, non-preview) lone pane; a lone
  /// pane showing a file or preview persists a single leaf. Read by the
  /// coordinator's whole-tree snapshot walk (`AppDelegate.snapshotNow`).
  func layoutSnapshot() -> SavedPaneNode? { paneContainer?.layoutSnapshot() }

  /// Whether this window's active document is in Neovim visual mode
  /// (`v`/`V`/`<C-v>`). Read by the coordinator's menu validation to enable
  /// the Edit > Copy command only when there is a selection to copy.
  var isVisualMode: Bool { handle?.is_visual_mode() ?? false }

  /// Whether this window's active buffer is a Markdown file. Read by the
  /// coordinator's menu validation to enable the View > Preview command only on
  /// a Markdown buffer.
  var isMarkdownActive: Bool { handle?.is_markdown_active() ?? false }

  /// Whether this window's active pane is a PREVIEW pane. Read by the
  /// coordinator's menu validation to enable the View > Edit Source command only
  /// from a preview pane.
  var isPreviewActive: Bool { handle?.is_preview_active() ?? false }

  /// Whether this window's active buffer is in FULL-WIDTH mode. Read by the
  /// coordinator's menu validation to show the View > Use full width for content
  /// checkmark (the item is always enabled — OQ-2).
  var isFullWidthActive: Bool { handle?.is_full_width_active() ?? false }

  /// The nvim WINDOW handle addressed by the History menu and the Back / Forward
  /// toolbar buttons: this VIEW's active pane, falling back to core's global
  /// `active_win()` before the container has bound one. `0` when neither resolves.
  ///
  /// The container's own `activeWin` is PREFERRED because the core handle is shared
  /// across a project's tabpages while `activePane` is per-view — it is the pane the
  /// user is looking at. The terminal drawer's pane IS in `panes`, but core never
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

  /// Opens (or reuses) the Markdown preview pane for the active source buffer.
  ///
  /// The placement mode is chosen from this window's live pane/preview state via
  /// the pure `PreviewSeam.placementMode` (three modes: single pane → new pane to
  /// the right; two panes whose OTHER pane already previews → reuse THAT preview
  /// pane rather than clobbering the active editor; every other multi-pane case →
  /// replace the active pane), then handed to core's `open_preview(mode)`. The
  /// front-end learns the new/reused pane + the source↔preview mapping on the next
  /// wake (via `previews()`), so no synchronous window handle is needed here; a
  /// repaint is requested so the web view surfaces promptly.
  func openPreview() {
    guard let handle, let paneContainer else { return }
    let inputs = paneContainer.previewPlacementInputs()
    let mode = PreviewSeam.placementMode(
      paneCount: inputs.paneCount,
      activePaneIsPreview: inputs.activePaneIsPreview,
      otherPreviewPaneCount: inputs.otherPreviewPaneCount)
    _ = handle.open_preview(mode)
    paneContainer.onRenderWake()
  }

  /// Navigates from the active PREVIEW pane back to its source file (the inverse
  /// of `openPreview`).
  ///
  /// The placement mode is chosen from this window's live pane/preview state via
  /// the pure `EditSeam.placementMode` (three modes: single pane → new editor pane
  /// to the LEFT, preview stays right; two panes whose OTHER pane is an editor →
  /// bind the source into THAT editor pane and focus it; every other case → replace
  /// the active preview pane), then handed to core's `edit_source(mode)`. The
  /// front-end learns the new pane + updated snapshot on the next wake (via
  /// `previews()` / the current-file notification), so no synchronous window handle
  /// is needed here; a repaint is requested so the editor surfaces promptly.
  func editSource() {
    guard let handle, let paneContainer else { return }
    let inputs = paneContainer.editPlacementInputs()
    let mode = EditSeam.placementMode(
      paneCount: inputs.paneCount,
      activePaneIsPreview: inputs.activePaneIsPreview,
      otherEditorPaneCount: inputs.otherEditorPaneCount)
    _ = handle.edit_source(mode)
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
