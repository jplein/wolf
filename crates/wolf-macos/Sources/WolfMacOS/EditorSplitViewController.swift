//
//  EditorSplitViewController.swift
//  The window's content controller: a standard macOS source-list split of the
//  file-tree sidebar (a `.sidebar` split item → OS vibrancy + full-height) and the
//  editor grid, hosted in lightweight `HostViewController`s.
//

import AppKit

/// The `NSSplitViewController` that backs the main window's content: a leading
/// `.sidebar` split item wrapping the `FileTreeView` (giving it the standard
/// translucent, full-height source-list material that extends under the
/// transparent titlebar) and a trailing content split item wrapping the
/// `PaneContainerView` editor (the split-panes area + bottom cmdline strip).
///
/// The sidebar item keeps the higher holding priority that
/// `NSSplitViewItem(sidebarWithViewController:)` assigns by default, so window
/// resizing is absorbed by the EDITOR pane (the sidebar stays put). The editor
/// pane is pinned below the safe area in its host, so the transparent
/// titlebar never obscures the grid.
///
/// The window/VC/appearance glue is not headlessly testable (no window server);
/// it is compile-verified + manual-smoke only.
final class EditorSplitViewController: NSSplitViewController {
  /// Minimum width of the sidebar pane, in points.
  static let minThickness: CGFloat = 180
  /// Maximum width of the sidebar pane, in points.
  static let maxThickness: CGFloat = 420

  /// Builds the split of `sidebar` (the tree/search container) and `paneContainer`
  /// (editor content).
  init(sidebar: SidebarContainerView, paneContainer: PaneContainerView) {
    super.init(nibName: nil, bundle: nil)

    let sidebarHost = HostViewController(hostedView: sidebar, pinTopToSafeArea: false)
    let contentHost = HostViewController(hostedView: paneContainer, pinTopToSafeArea: true)

    // `sidebarWithViewController:` gives the `.sidebar` behavior: vibrancy,
    // full-height under the titlebar, collapsibility, and the higher holding
    // priority that makes the editor pane absorb resizing. DO NOT lower the
    // holding priority here — that would invert the resize behavior.
    let sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebarHost)
    sidebarItem.canCollapse = true
    sidebarItem.minimumThickness = Self.minThickness
    sidebarItem.maximumThickness = Self.maxThickness

    let contentItem = NSSplitViewItem(viewController: contentHost)

    splitViewItems = [sidebarItem, contentItem]
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("EditorSplitViewController is not constructed from a nib")
  }

  /// The sidebar `NSSplitViewItem` (the leading split item), exposed so state
  /// restore can set — and state persistence can observe — `isCollapsed`.
  var sidebarItem: NSSplitViewItem {
    splitViewItems[0]
  }

  /// Pins the divider so the sidebar pane is exactly `MainWindow.treePaneWidth`
  /// wide. Called by `MainWindow` after the content size is set.
  func pinDivider() {
    splitView.setPosition(MainWindow.treePaneWidth, ofDividerAt: 0)
  }
}

/// A minimal `NSViewController` that hosts a single pre-built `NSView` inside the
/// split. Two hosting modes:
///
/// - SIDEBAR (`pinTopToSafeArea == false`): the hosted view becomes the
///   controller's `view` directly, so the `.sidebar` material fills the whole
///   pane (the `FileTreeView`'s `NSScrollView` auto-insets its content under the
///   titlebar).
/// - CONTENT (`pinTopToSafeArea == true`): the hosted view is added to a plain
///   container and pinned leading/trailing/bottom to the container edges, with its
///   TOP pinned to the container's `safeAreaLayoutGuide.topAnchor`, so the editor
///   grid is not obscured by the transparent titlebar.
final class HostViewController: NSViewController {
  private let hostedView: NSView
  private let pinTopToSafeArea: Bool

  /// Builds a host for `hostedView`; `pinTopToSafeArea` selects the CONTENT
  /// hosting mode (vs. the SIDEBAR mode that adopts the view directly).
  init(hostedView: NSView, pinTopToSafeArea: Bool) {
    self.hostedView = hostedView
    self.pinTopToSafeArea = pinTopToSafeArea
    super.init(nibName: nil, bundle: nil)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("HostViewController is not constructed from a nib")
  }

  override func loadView() {
    guard pinTopToSafeArea else {
      // SIDEBAR mode: adopt the hosted view directly.
      view = hostedView
      return
    }

    // CONTENT mode: pin the hosted view below the safe area.
    let container = NSView()
    hostedView.translatesAutoresizingMaskIntoConstraints = false
    container.addSubview(hostedView)
    NSLayoutConstraint.activate([
      hostedView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
      hostedView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
      hostedView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
      hostedView.topAnchor.constraint(equalTo: container.safeAreaLayoutGuide.topAnchor),
    ])
    view = container
  }
}
