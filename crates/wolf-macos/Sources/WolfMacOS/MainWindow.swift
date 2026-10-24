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
final class MainWindow: NSWindow {
  /// The Back toolbar item's identifier, derived from the pure seam so the
  /// ordering and the identifier strings have exactly one source of truth.
  static let backItemIdentifier = NSToolbarItem.Identifier(HistorySeam.backIdentifier)

  /// The Forward toolbar item's identifier. See ``backItemIdentifier``.
  static let forwardItemIdentifier = NSToolbarItem.Identifier(HistorySeam.forwardIdentifier)

  /// The window controller that owns this window's panes, for the Back / Forward
  /// toolbar buttons. Weak — the controller owns the window, not the reverse.
  /// Assigned in `WolfWindowController.build()` right after construction.
  weak var historyNav: HistoryNavigating?
  /// Fixed initial width of the file-tree sidebar pane in points.
  static let treePaneWidth: CGFloat = 240

  /// The window's `NSToolbar` identifier (a unified toolbar whose only item is
  /// the system toggle-sidebar button).
  private static let toolbarIdentifier = NSToolbar.Identifier("com.wolf.editor.toolbar")

  /// The window's pristine `backgroundColor`, captured on the FIRST themed
  /// titlebar application so an unset `WolfTitlebar` can restore the exact
  /// system default. `nil` until a theme color has ever been applied.
  private var defaultBackgroundColor: NSColor?

  /// The content split-view controller, retained so the sidebar split item is
  /// reachable for state restore/persistence.
  private let splitViewController: EditorSplitViewController

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
    contentRect: NSRect,
    editorWidth: CGFloat,
    editorHeight: CGFloat,
    divider: CGFloat,
    paneContainer: PaneContainerView
  ) {
    splitViewController = splitVC

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
  }

  /// Applies (or reverts) the `WolfTitlebar` theme background.
  ///
  /// The window uses `.fullSizeContentView` + `titlebarAppearsTransparent`,
  /// so `backgroundColor` shows through the titlebar over the editor pane
  /// (the sidebar keeps its own vibrancy material) — setting it is the
  /// minimal, reversible lever for a themed titlebar. On the first themed
  /// application the pristine `backgroundColor` is captured; `set == false`
  /// (group absent or cleared) restores that captured default. Idempotent:
  /// re-applying the same state is harmless.
  func applyTitlebarBackground(rgb: UInt32, set: Bool) {
    if set {
      if defaultBackgroundColor == nil {
        defaultBackgroundColor = backgroundColor
      }
      backgroundColor = NSColor(
        srgbRed: CGFloat((rgb >> 16) & 0xFF) / 255,
        green: CGFloat((rgb >> 8) & 0xFF) / 255,
        blue: CGFloat(rgb & 0xFF) / 255,
        alpha: 1
      )
    } else if let defaultBackgroundColor {
      backgroundColor = defaultBackgroundColor
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
}

/// The window's toolbar delegate. Two items in the Apple-standard order: the
/// SYSTEM `.toggleSidebar` button FIRST (so it lands in the toolbar's SIDEBAR
/// section — the top-left, just right of the traffic lights, at the LEFT of the
/// toolbar), then a `.sidebarTrackingSeparator` that stays aligned with the
/// sidebar/content split divider (the window title / cwd renders in the content
/// section to its right). Placing the toggle AFTER the separator instead pushes
/// it to the trailing edge, because the visible title claims the content-leading
/// slot. AppKit auto-wires `.toggleSidebar` to
/// `NSSplitViewController.toggleSidebar(_:)` up the responder chain (the same
/// mechanism as the View-menu Toggle Sidebar item), so returning `nil` for it
/// lets AppKit supply the built-in item; the separator is constructed here bound
/// to the split view's first divider.
/// The Back / Forward navigation buttons follow the separator, giving the LOCKED
/// toolbar order `traffic lights → toggle sidebar → sidebar separator → back →
/// forward → title`. That ordering lives in the pure `HistorySeam.itemIdentifiers`
/// so it is unit-testable without AppKit.
extension MainWindow: NSToolbarDelegate {
  /// The locked ordering, mapped back from the pure seam's raw identifiers.
  private static var orderedItemIdentifiers: [NSToolbarItem.Identifier] {
    HistorySeam.itemIdentifiers(
      toggleSidebar: NSToolbarItem.Identifier.toggleSidebar.rawValue,
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
    // Bound to the sidebar divider (index 0), so it and every item after it, including the toggle
    // button, stay pinned to the sidebar's right edge.
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
    // Return nil for the system `.toggleSidebar` identifier so AppKit supplies
    // the built-in item (auto-wired to `toggleSidebar(_:)`).
    return nil
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
