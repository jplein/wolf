//
//  MainWindow.swift
//  The single editor window. Its content is an `EditorSplitViewController`
//  (an `NSSplitViewController`) with a `.sidebar` source-list split item for the
//  file-tree sidebar and a content split item for the editor grid, under a
//  transparent full-size-content-view titlebar. The sidebar's vibrancy extends
//  full-height under the titlebar; the editor pane is pinned below the titlebar
//  (and the unified toolbar) so it keeps its exact `cols` × `rows` cell size.
//  The window has a `.unified` `NSToolbar` whose only item is the system
//  toggle-sidebar button (auto-wired to `NSSplitViewController.toggleSidebar`).
//

import AppKit
import WolfKeyMap

/// The main "Wolf" window. Its `contentViewController` is an
/// `EditorSplitViewController`; the window uses a transparent
/// `.fullSizeContentView` titlebar.
final class MainWindow: NSWindow {
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
    // Each pane's text grid is inset by a uniform 20pt padding on all four sides
    // (`EditorInset.padding`) — with the statusbar strip flush at the pane top and
    // the grid inset below it — so the initial content must grow by 2*padding in
    // each dimension to keep the grid at exactly cols × rows cells (the 80-col OCR
    // width is preserved) inside that per-pane inset.
    let padding = CGFloat(EditorInset.padding)
    let editorWidth = CGFloat(cols) * metrics.cellWidth + 2 * padding
    // The grid stays exactly cols × rows cells; the one-row cmdline strip is extra
    // height beneath the pane area, and the 2*padding reserves the per-pane inset
    // around the grid (rather than an outer margin around the whole editor).
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

    // ARC owns this window (the `WolfWindowController` holds the strong
    // reference). AppKit's default `isReleasedWhenClosed == true` would add an
    // unbalanced release on close, over-releasing the window — the deferred
    // close-animation dealloc then crashes. Let ARC be the sole owner.
    isReleasedWhenClosed = false

    title = "Wolf"
    titleVisibility = .visible
    titlebarAppearsTransparent = true

    // A shared tabbing identifier lets every Wolf window merge into the same
    // native Tahoe tab group (Finder/Safari-style): the "+" button, ⌘T, and
    // drag-to-merge all target this identity. `tabbingMode` stays the default
    // (`.automatic`) so the OS shows the tab bar once a window has ≥ 2 tabs.
    tabbingIdentifier = "com.wolf.editor"

    contentViewController = splitVC

    // Install the unified toolbar (the system toggle-sidebar button pinned to
    // the sidebar section at the top-left, then a sidebar-tracking separator
    // aligned to the sidebar divider) BEFORE measuring the chrome height, so the
    // measured chrome includes the toolbar band and `setContentSize` keeps the
    // grid at exactly cols × rows cells.
    let toolbar = NSToolbar(identifier: Self.toolbarIdentifier)
    toolbar.delegate = self
    toolbar.displayMode = .iconOnly
    self.toolbar = toolbar
    toolbarStyle = .unified

    // Size the content so the editor pane is exactly `editorWidth` ×
    // `editorHeight` BELOW the titlebar chrome. `contentLayoutRect` is
    // the area not obscured by the (now-attached) titlebar + toolbar, so the
    // chrome height is the difference from the full content height.
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
extension MainWindow: NSToolbarDelegate {
  func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
    [.toggleSidebar, .sidebarTrackingSeparator]
  }

  func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
    [.toggleSidebar, .sidebarTrackingSeparator]
  }

  func toolbar(
    _ toolbar: NSToolbar,
    itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier,
    willBeInsertedIntoToolbar flag: Bool
  ) -> NSToolbarItem? {
    // The tracking separator is bound to the sidebar/content divider (index 0)
    // so it — and everything after it, including the toggle button — stays
    // pinned to the sidebar's right edge.
    if itemIdentifier == .sidebarTrackingSeparator {
      return NSTrackingSeparatorToolbarItem(
        identifier: itemIdentifier,
        splitView: splitViewController.splitView,
        dividerIndex: 0)
    }
    // Return nil for the system `.toggleSidebar` identifier so AppKit supplies
    // the built-in item (auto-wired to `toggleSidebar(_:)`).
    return nil
  }
}
