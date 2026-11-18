// The sidebar body: the file tree or the search panel, one shown at a time.

import AppKit

/// Hosts the file tree and the search panel, showing the one `mode` names from the
/// top of the safe area down. The toolbar's sidebar control in `MainWindow` switches it.
final class SidebarContainerView: NSView {
  /// Which body is currently shown.
  enum Mode {
    case tree
    case search
  }

  let tree: FileTreeView
  let search: SearchView

  /// The active body; switching swaps which child is hidden.
  private(set) var mode: Mode = .tree

  /// The sidebar's last-relayed visibility (from the split item's `isCollapsed`
  /// KVO), forwarded only to the ACTIVE child so the hidden one does no work.
  private var isSidebarVisible = true

  /// Builds the container over a pre-built `tree` and `search`.
  init(tree: FileTreeView, search: SearchView) {
    self.tree = tree
    self.search = search
    super.init(frame: .zero)
    buildSubviews()
    updateMode()
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("SidebarContainerView is not constructed from a nib")
  }

  /// Switches the shown body, hiding the other and relaying visibility so the
  /// newly-active child catches up any deferred work.
  func setMode(_ newMode: Mode) {
    guard newMode != mode else { return }
    mode = newMode
    updateMode()
  }

  /// Relays the sidebar's collapse state (from `isCollapsed` KVO) to BOTH children
  /// so each keeps an accurate visibility for its refresh gate; the inactive child
  /// is additionally `isHidden`, so it defers regardless.
  func setSidebarVisible(_ visible: Bool) {
    isSidebarVisible = visible
    tree.setSidebarVisible(visible && mode == .tree)
    search.setSidebarVisible(visible && mode == .search)
  }

  // MARK: - Private

  /// Applies `mode`: hides the inactive body, shows the active one, and relays
  /// visibility.
  private func updateMode() {
    tree.isHidden = mode != .tree
    search.isHidden = mode != .search
    setSidebarVisible(isSidebarVisible)
  }

  private func buildSubviews() {
    tree.translatesAutoresizingMaskIntoConstraints = false
    search.translatesAutoresizingMaskIntoConstraints = false
    addSubview(tree)
    addSubview(search)

    // The safe area's top clears the transparent full-size-content titlebar.
    NSLayoutConstraint.activate([
      tree.topAnchor.constraint(equalTo: safeAreaLayoutGuide.topAnchor),
      tree.leadingAnchor.constraint(equalTo: leadingAnchor),
      tree.trailingAnchor.constraint(equalTo: trailingAnchor),
      tree.bottomAnchor.constraint(equalTo: bottomAnchor),

      search.topAnchor.constraint(equalTo: safeAreaLayoutGuide.topAnchor),
      search.leadingAnchor.constraint(equalTo: leadingAnchor),
      search.trailingAnchor.constraint(equalTo: trailingAnchor),
      search.bottomAnchor.constraint(equalTo: bottomAnchor),
    ])
  }
}
