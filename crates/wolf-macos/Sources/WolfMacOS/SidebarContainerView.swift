// The sidebar body: a header with two toggles swapping the file tree and search.

import AppKit

/// Hosts the file tree + search panel with a header toggle to switch between them.
final class SidebarContainerView: NSView {
  /// Which body is currently shown.
  enum Mode {
    case tree
    case search
  }

  /// Height of the header toggle strip.
  private static let headerHeight: CGFloat = 30

  let tree: FileTreeView
  let search: SearchView

  private let folderButton = NSButton()
  private let searchButton = NSButton()

  /// The active body; switching swaps which child is hidden and updates the
  /// header button selection.
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

  // MARK: - Header actions

  @objc private func showTree() {
    setMode(.tree)
  }

  @objc private func showSearch() {
    setMode(.search)
    // Seed the toggles the first time the panel is shown, and give the field focus.
    search.seedToggles()
    search.focusSearchField()
  }

  // MARK: - Private

  /// Applies `mode`: hides the inactive body, shows the active one, updates the
  /// header selection, and relays visibility.
  private func updateMode() {
    tree.isHidden = mode != .tree
    search.isHidden = mode != .search
    folderButton.state = mode == .tree ? .on : .off
    searchButton.state = mode == .search ? .on : .off
    setSidebarVisible(isSidebarVisible)
  }

  private func buildSubviews() {
    configureHeaderButton(
      folderButton, symbol: "folder", tooltip: "Files", action: #selector(showTree))
    configureHeaderButton(
      searchButton, symbol: "magnifyingglass", tooltip: "Search", action: #selector(showSearch))

    let header = NSStackView(views: [folderButton, searchButton])
    header.translatesAutoresizingMaskIntoConstraints = false
    header.orientation = .horizontal
    header.spacing = 6
    header.edgeInsets = NSEdgeInsets(top: 0, left: 8, bottom: 0, right: 8)
    addSubview(header)

    tree.translatesAutoresizingMaskIntoConstraints = false
    search.translatesAutoresizingMaskIntoConstraints = false
    addSubview(tree)
    addSubview(search)

    NSLayoutConstraint.activate([
      header.topAnchor.constraint(equalTo: safeAreaLayoutGuide.topAnchor),
      header.leadingAnchor.constraint(equalTo: leadingAnchor),
      header.trailingAnchor.constraint(equalTo: trailingAnchor),
      header.heightAnchor.constraint(equalToConstant: Self.headerHeight),

      tree.topAnchor.constraint(equalTo: header.bottomAnchor),
      tree.leadingAnchor.constraint(equalTo: leadingAnchor),
      tree.trailingAnchor.constraint(equalTo: trailingAnchor),
      tree.bottomAnchor.constraint(equalTo: bottomAnchor),

      search.topAnchor.constraint(equalTo: header.bottomAnchor),
      search.leadingAnchor.constraint(equalTo: leadingAnchor),
      search.trailingAnchor.constraint(equalTo: trailingAnchor),
      search.bottomAnchor.constraint(equalTo: bottomAnchor),
    ])
  }

  /// Configures a header toggle as a borderless SF-Symbol push-on/push-off button.
  private func configureHeaderButton(
    _ button: NSButton, symbol: String, tooltip: String, action: Selector
  ) {
    button.translatesAutoresizingMaskIntoConstraints = false
    button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: tooltip)
    button.toolTip = tooltip
    button.isBordered = false
    button.bezelStyle = .regularSquare
    button.setButtonType(.pushOnPushOff)
    button.imagePosition = .imageOnly
    button.target = self
    button.action = action
  }
}
