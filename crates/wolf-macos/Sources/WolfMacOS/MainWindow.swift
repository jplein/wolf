// The editor window: sidebar split, full-size-content titlebar and unified toolbar.

import AppKit
import WolfKeyMap

/// The per-pane navigation-history actions the toolbar's Back / Forward buttons
/// need from the window controller.
///
/// Explicit delegation rather than a `target: nil` responder-chain hop, because
/// `WolfWindowController` is NOT an `NSWindowController` and is therefore not in
/// the responder chain.
protocol HistoryNavigating: AnyObject {
  /// Moves the active pane one entry BACK in its navigation history.
  func historyBack()
  /// Moves the active pane one entry FORWARD in its navigation history.
  func historyForward()
  /// Whether the active pane has an older entry to go Back to.
  var historyCanBack: Bool { get }
  /// Whether the active pane has a newer entry to go Forward to.
  var historyCanForward: Bool { get }
}

/// The main "Wolf" window. Its `contentViewController` is an
/// `EditorSplitViewController`; the window uses a transparent
/// `.fullSizeContentView` titlebar.
///
/// The window's `appearance` stays `nil`, so it and its title inherit the app
/// appearance `AppDelegate` forces from Wolf's resolved appearance; see
/// specs/architecture.md#theming.
final class MainWindow: NSWindow {
  /// The Back toolbar item's identifier, derived from the pure seam so the
  /// ordering and the identifier strings have exactly one source of truth.
  static let backItemIdentifier = NSToolbarItem.Identifier(HistorySeam.backIdentifier)

  /// The Forward toolbar item's identifier. See ``backItemIdentifier``.
  static let forwardItemIdentifier = NSToolbarItem.Identifier(HistorySeam.forwardIdentifier)

  /// The sidebar panel control's toolbar item identifier, from ``SidebarToolbarSeam``.
  static let sidebarPanelsItemIdentifier = NSToolbarItem.Identifier(SidebarToolbarSeam.identifier)

  /// The window controller that owns this window's panes, for the Back / Forward
  /// toolbar buttons. Weak — the controller owns the window, not the reverse.
  /// Assigned in `WolfWindowController.build()` right after construction.
  weak var historyNav: HistoryNavigating?
  /// Fixed initial width of the file-tree sidebar pane in points.
  static let treePaneWidth: CGFloat = 240

  /// The window's `NSToolbar` identifier (a unified toolbar holding Back, Forward
  /// and the sidebar panel control).
  private static let toolbarIdentifier = NSToolbar.Identifier("com.wolf.editor.toolbar")

  /// The window's pristine `backgroundColor`, captured on the FIRST themed
  /// titlebar application so `TitlebarBackground.system`, when neither
  /// `WolfTitlebar` nor `Normal` is known, can restore the exact system default.
  /// `nil` until a theme color has ever been applied.
  private var defaultBackgroundColor: NSColor?

  /// The content split-view controller, retained so the sidebar split item is
  /// reachable for state restore/persistence.
  private let splitViewController: EditorSplitViewController

  /// The sidebar body, whose mode the toolbar control shows and switches.
  private let sidebar: SidebarContainerView

  /// KVO on the sidebar item's `isCollapsed`, refreshing the toolbar control on every
  /// collapse or expand. Stored, because a discarded token stops observing.
  private var sidebarCollapseObservation: NSKeyValueObservation?

  /// The sidebar `NSSplitViewItem`, exposed so state restore can set — and
  /// `StatePersistence` can observe — `isCollapsed`.
  var sidebarItem: NSSplitViewItem {
    splitViewController.sidebarItem
  }

  /// Toggles the sidebar via the split VC's animated
  /// `NSSplitViewController.toggleSidebar(_:)`, which flips the sidebar item's
  /// `isCollapsed` (so `StatePersistence`'s KVO on it still fires).
  func toggleSidebar() {
    splitViewController.toggleSidebar(nil)
  }

  /// The sidebar's active panel, whether or not the sidebar is visible.
  var sidebarPanel: SidebarPanel {
    switch sidebar.mode {
    case .tree: return .tree
    case .search: return .search
    }
  }

  /// Reveals the sidebar (if collapsed), switches it to `panel`, and focuses the
  /// search field for `.search`. The `isCollapsed = false` write relays visibility
  /// to the container via KVO; `setMode` swaps the active child.
  ///
  /// The toolbar control is refreshed last, because the `isCollapsed` KVO refresh
  /// runs before the mode switch and sees the old mode.
  func showSidebar(panel: SidebarPanel) {
    sidebarItem.isCollapsed = false
    sidebar.setMode(panel == .tree ? .tree : .search)
    if panel == .search { sidebar.search.focusSearchField() }
    refreshSidebarToolbar()
  }

  /// Builds the window around `sidebar` (the tree/search container) and
  /// `paneContainer` (editor),
  /// sizing the editor pane to `cols` × `rows` cells with the cmdline strip as
  /// extra height below it.
  convenience init(
    sidebar: SidebarContainerView, paneContainer: PaneContainerView, metrics: CellMetrics,
    cols: Int, rows: Int
  ) {
    let splitVC = EditorSplitViewController(sidebar: sidebar, paneContainer: paneContainer)
    // Each pane's grid is inset by `EditorInset.padding` on all sides, so the initial content grows
    // by 2*padding per dimension to keep the grid at exactly cols × rows cells.
    let padding = CGFloat(EditorInset.padding)
    let editorWidth = CGFloat(cols) * metrics.cellWidth + 2 * padding
    // The one-row cmdline strip is extra height beneath the pane area.
    let cmdlineStripHeight = ceil(metrics.cellHeight)
    let editorHeight = CGFloat(rows) * metrics.cellHeight + cmdlineStripHeight + 2 * padding
    let divider = splitVC.splitView.dividerThickness

    // An initial content rect; the exact size (with chrome) is set below once
    // the content VC is attached and the titlebar height is known.
    let contentRect = NSRect(
      x: 0, y: 0,
      width: Self.treePaneWidth + divider + editorWidth,
      height: editorHeight
    )

    self.init(
      splitVC: splitVC,
      sidebar: sidebar,
      contentRect: contentRect,
      editorWidth: editorWidth,
      editorHeight: editorHeight,
      divider: divider,
      paneContainer: paneContainer
    )
  }

  /// Designated initializer: wires the split VC and transparent titlebar, then
  /// sizes the content so the editor pane keeps its exact cell size below the
  /// titlebar chrome.
  private init(
    splitVC: EditorSplitViewController,
    sidebar: SidebarContainerView,
    contentRect: NSRect,
    editorWidth: CGFloat,
    editorHeight: CGFloat,
    divider: CGFloat,
    paneContainer: PaneContainerView
  ) {
    splitViewController = splitVC
    self.sidebar = sidebar

    super.init(
      contentRect: contentRect,
      styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )

    // ARC owns this window; the default `isReleasedWhenClosed` adds an unbalanced release on close,
    // and the deferred close-animation dealloc then crashes.
    isReleasedWhenClosed = false

    title = "Wolf"
    titleVisibility = .visible
    titlebarAppearsTransparent = true

    // A shared tabbing identifier merges every Wolf window into one native tab group;
    // `tabbingMode` stays `.automatic`, so the tab bar shows once a window has 2 tabs.
    tabbingIdentifier = "com.wolf.editor"

    contentViewController = splitVC

    // Install the toolbar before measuring the chrome height, so `setContentSize` accounts for the
    // toolbar band and the grid stays exactly cols × rows cells.
    let toolbar = NSToolbar(identifier: Self.toolbarIdentifier)
    toolbar.delegate = self
    toolbar.displayMode = .iconOnly
    self.toolbar = toolbar
    toolbarStyle = .unified

    // Size the content so the editor is `editorWidth` × `editorHeight` below the chrome;
    // `contentLayoutRect` excludes the titlebar and toolbar.
    let fullHeight = contentView?.bounds.height ?? editorHeight
    let chrome = fullHeight - contentLayoutRect.height
    setContentSize(
      NSSize(
        width: Self.treePaneWidth + divider + editorWidth,
        height: editorHeight + chrome
      )
    )

    // Pin the divider so the sidebar pane is exactly `treePaneWidth` wide.
    splitVC.pinDivider()

    initialFirstResponder = paneContainer

    sidebarCollapseObservation = sidebarItem.observe(\.isCollapsed, options: [.new]) {
      [weak self] _, _ in
      self?.refreshSidebarToolbar()
    }
  }

  /// Applies (or reverts) the titlebar background that `TitlebarBackground`
  /// picked: `WolfTitlebar`'s, else `Normal`'s, else the system colour.
  ///
  /// The window uses `.fullSizeContentView` + `titlebarAppearsTransparent`,
  /// so `backgroundColor` shows through the titlebar over the editor pane
  /// (the sidebar keeps its own vibrancy material) — setting it is the
  /// minimal, reversible lever for a themed titlebar. A `.colour` is built in
  /// sRGB, as the grid is, so the titlebar and the grid are pixel-identical.
  /// On the first `.colour` the pristine `backgroundColor` is captured;
  /// `.system` restores that captured default. Idempotent: re-applying the
  /// same value is harmless.
  func applyTitlebarBackground(_ background: TitlebarBackground) {
    switch background {
    case .colour(let rgb):
      if defaultBackgroundColor == nil {
        defaultBackgroundColor = backgroundColor
      }
      backgroundColor = NSColor(
        srgbRed: CGFloat((rgb >> 16) & 0xFF) / 255,
        green: CGFloat((rgb >> 8) & 0xFF) / 255,
        blue: CGFloat(rgb & 0xFF) / 255,
        alpha: 1
      )
    case .system:
      if let defaultBackgroundColor {
        backgroundColor = defaultBackgroundColor
      }
    }
  }

  /// Enables/disables the Back and Forward toolbar buttons for the ACTIVE pane.
  ///
  /// Called from `WolfWindowController.onRenderWake()` (right after the pane
  /// container reconciles its active pane for this frame) with two cheap,
  /// lock-free core snapshot reads. The items are built with
  /// `autovalidates = false`, which is what makes an explicitly-set `isEnabled`
  /// stick instead of being overwritten by AppKit's own validation pass.
  func refreshHistoryToolbar(canBack: Bool, canForward: Bool) {
    guard let items = toolbar?.items else { return }
    for item in items {
      switch item.itemIdentifier {
      case Self.backItemIdentifier: item.isEnabled = canBack
      case Self.forwardItemIdentifier: item.isEnabled = canForward
      default: break
      }
    }
  }

  /// Toolbar Back button action; forwards to the window controller.
  @objc func historyBackToolbarAction(_ sender: Any?) {
    historyNav?.historyBack()
  }

  /// Toolbar Forward button action; forwards to the window controller.
  @objc func historyForwardToolbarAction(_ sender: Any?) {
    historyNav?.historyForward()
  }

  /// Toolbar sidebar control action. A `.selectOne` click has already selected the
  /// clicked segment; ``SidebarToolbarSeam/click(_:sidebarVisible:current:)`` decides
  /// whether it hides the sidebar or shows that panel. Showing search from here seeds
  /// the search toggles first, which the sidebar commands do not do.
  @objc func sidebarPanelsToolbarAction(_ sender: NSSegmentedControl) {
    guard let panel = SidebarToolbarSeam.panel(atSegment: sender.selectedSegment) else { return }
    switch SidebarToolbarSeam.click(
      panel, sidebarVisible: !sidebarItem.isCollapsed, current: sidebarPanel)
    {
    case .hide:
      toggleSidebar()
    case .show(.tree):
      showSidebar(panel: .tree)
    case .show(.search):
      sidebar.search.seedToggles()
      showSidebar(panel: .search)
    }
  }

  /// The sidebar control's selected segment, derived from the sidebar's visibility
  /// and mode; nothing selected while the sidebar is hidden.
  private var sidebarSelectedSegment: Int {
    SidebarToolbarSeam.selectedSegment(
      sidebarVisible: !sidebarItem.isCollapsed, current: sidebarPanel) ?? -1
  }

  /// Sets the toolbar sidebar control's selection from the sidebar's state. A no-op
  /// until the toolbar has built the item.
  private func refreshSidebarToolbar() {
    let item = toolbar?.items.first { $0.itemIdentifier == Self.sidebarPanelsItemIdentifier }
    guard let control = item?.view as? NSSegmentedControl else { return }
    control.selectedSegment = sidebarSelectedSegment
  }
}

/// The window's toolbar delegate, in the LOCKED order `traffic lights → sidebar
/// separator → back → forward → sidebar control → title`, which lives in the pure
/// `HistorySeam.itemIdentifiers` so it is unit-testable without AppKit.
///
/// The `.sidebarTrackingSeparator` comes first and is bound to the split view's
/// first divider, so the navigational items after it ride the sidebar's trailing
/// edge, with the window title / cwd in the content section to their right.
extension MainWindow: NSToolbarDelegate {
  /// The locked ordering, mapped back from the pure seam's raw identifiers.
  private static var orderedItemIdentifiers: [NSToolbarItem.Identifier] {
    HistorySeam.itemIdentifiers(
      trackingSeparator: NSToolbarItem.Identifier.sidebarTrackingSeparator.rawValue
    ).map(NSToolbarItem.Identifier.init(_:))
  }

  func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
    Self.orderedItemIdentifiers
  }

  func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
    Self.orderedItemIdentifiers
  }

  func toolbar(
    _ toolbar: NSToolbar,
    itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier,
    willBeInsertedIntoToolbar flag: Bool
  ) -> NSToolbarItem? {
    // Bound to the sidebar divider (index 0), so it and every item after it stay pinned to the
    // sidebar's right edge.
    if itemIdentifier == .sidebarTrackingSeparator {
      return NSTrackingSeparatorToolbarItem(
        identifier: itemIdentifier,
        splitView: splitViewController.splitView,
        dividerIndex: 0)
    }
    if itemIdentifier == Self.backItemIdentifier {
      return navigationItem(
        identifier: itemIdentifier,
        label: "Back",
        symbolName: HistorySeam.backSymbolName,
        action: #selector(historyBackToolbarAction(_:)))
    }
    if itemIdentifier == Self.forwardItemIdentifier {
      return navigationItem(
        identifier: itemIdentifier,
        label: "Forward",
        symbolName: HistorySeam.forwardSymbolName,
        action: #selector(historyForwardToolbarAction(_:)))
    }
    if itemIdentifier == Self.sidebarPanelsItemIdentifier {
      return sidebarPanelsItem(identifier: itemIdentifier)
    }
    return nil
  }

  /// Builds the sidebar panel control: one SF-Symbol segment per panel, already showing
  /// the derived selection.
  ///
  /// An `NSSegmentedControl` item rather than an `NSToolbarItemGroup`, because a
  /// `.selectOne` group cannot show nothing selected, which a hidden sidebar needs.
  private func sidebarPanelsItem(identifier: NSToolbarItem.Identifier) -> NSToolbarItem {
    let panels = SidebarToolbarSeam.panels
    let control = NSSegmentedControl(
      images: panels.map {
        NSImage(
          systemSymbolName: SidebarToolbarSeam.symbolName(for: $0),
          accessibilityDescription: SidebarToolbarSeam.label(for: $0)) ?? NSImage()
      },
      trackingMode: .selectOne,
      target: self,
      action: #selector(sidebarPanelsToolbarAction(_:)))
    for (segment, panel) in panels.enumerated() {
      control.setToolTip(SidebarToolbarSeam.toolTip(for: panel), forSegment: segment)
    }
    control.selectedSegment = sidebarSelectedSegment

    let item = NSToolbarItem(itemIdentifier: identifier)
    item.view = control
    item.label = "Sidebar"
    item.paletteLabel = "Sidebar"
    // Navigational, so it joins Back and Forward ahead of the title.
    item.isNavigational = true
    return item
  }

  /// Builds one SF-Symbol navigation toolbar item targeting this window.
  ///
  /// `autovalidates = false` is LOAD-BEARING: it hands enablement entirely to
  /// `refreshHistoryToolbar(canBack:canForward:)`, which the render wake drives
  /// from core's per-pane history.
  private func navigationItem(
    identifier: NSToolbarItem.Identifier,
    label: String,
    symbolName: String,
    action: Selector
  ) -> NSToolbarItem {
    let item = NSToolbarItem(itemIdentifier: identifier)
    item.image = NSImage(systemSymbolName: symbolName, accessibilityDescription: label)
    item.label = label
    item.paletteLabel = label
    item.toolTip = label
    item.isNavigational = true
    item.autovalidates = false
    item.isEnabled = false
    item.target = self
    item.action = action
    return item
  }
}
