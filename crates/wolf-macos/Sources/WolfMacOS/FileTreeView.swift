// The AppKit file-tree sidebar, fed by the Rust file-tree bridge.

import AppKit
import WolfFFI
import WolfKeyMap

/// One row of the sidebar outline: a label whose color is re-resolved through
/// `SidebarRowStyle` whenever the current-buffer flag or AppKit's background
/// style changes.
///
/// The current-buffer mark is applied HERE, to the label's own color, rather than
/// being left to the row's selection tint — see `SidebarRowStyle` for why a
/// selection tint disappears at the first app switch.
final class TreeCellView: NSTableCellView {
  /// Whether this row's file is the current buffer.
  var isCurrent = false {
    didSet { applyLabelColor() }
  }

  override var backgroundStyle: NSView.BackgroundStyle {
    didSet { applyLabelColor() }
  }

  private func applyLabelColor() {
    switch SidebarRowStyle.labelColor(
      isCurrent: isCurrent, isEmphasized: backgroundStyle == .emphasized)
    {
    case .normal:
      textField?.textColor = .labelColor
    case .current:
      textField?.textColor = .controlAccentColor
    case .selected:
      textField?.textColor = .alternateSelectedControlTextColor
    }
  }
}

/// A stable, identity-bearing node in the sidebar's outline.
///
/// `NSOutlineView` keys expansion + selection on object identity, so the view
/// caches one `TreeItem` per `relPath` (`itemsByRelPath`) and hands the SAME
/// instance back across reloads. Fields other than `relPath` are refreshed in
/// place when the projection restreams.
final class TreeItem {
  /// Whether this node is a directory (vs. a leaf file).
  var isDir: Bool
  /// The immediate component name shown in the row.
  var name: String
  /// The full relative, `/`-separated path from the cwd root (stable identity).
  let relPath: String
  /// The absolute filesystem path for a file; empty for a directory.
  var absPath: String

  init(isDir: Bool, name: String, relPath: String, absPath: String) {
    self.isDir = isDir
    self.name = name
    self.relPath = relPath
    self.absPath = absPath
  }
}

/// The file-tree sidebar view: an `NSOutlineView` driven by the Rust file-tree
/// projection.
///
/// Holds a long-lived `FfiFileSearch` (started via `start_tree_search`) for its
/// whole lifetime; the registered tree wake calls `scheduleRefresh()` on each
/// traversal batch (COALESCED to at most one `refresh()` per frame), and the
/// editor render wake calls `onRenderWake()` to reveal + accent the current
/// buffer.
/// All state is plain main-thread properties.
final class FileTreeView: NSView, NSOutlineViewDataSource, NSOutlineViewDelegate {
  /// Reuse identifier for the outline's text cells.
  private static let cellIdentifier = NSUserInterfaceItemIdentifier("TreeCell")

  private let handle: WolfCoreHandle
  private weak var paneContainer: PaneContainerView?

  /// The proportional system UI font for rows (the sidebar is chrome, not the
  /// editor grid). Every row shares it: the current buffer is marked by the
  /// label's COLOR, not by a heavier face.
  private let font = NSFont.systemFont(ofSize: NSFont.systemFontSize)

  private let outlineView = NSOutlineView()
  private let scrollView = NSScrollView()

  /// The long-lived file-tree session; `nil` once closed.
  private var search: FfiFileSearch?
  /// One `TreeItem` per `relPath`, giving NSOutlineView stable item identity
  /// across reloads.
  private var itemsByRelPath: [String: TreeItem] = [:]
  /// Per-directory memo of projected children, so AppKit's numberOfChildren /
  /// child:index callback storm calls `projectChildren` (and the FFI
  /// `children_of`) at most ONCE per directory per reload. Built after
  /// `buildSubviews`; invalidated on reload and reroot. `itemsByRelPath` still
  /// provides stable identity — the memo caches the SAME `TreeItem` instances.
  private var childrenMemo: TreeChildrenMemo<TreeItem>!
  /// The visibility + version gate: defers all refresh work while the sidebar is
  /// collapsed (one catch-up on reappear) and reloads the outline only when the
  /// snapshot version has advanced.
  private let refreshGate = TreeRefreshGate(isVisible: true)

  /// Coalesces streaming tree-traversal wakes into at most one `refresh()` per
  /// frame (~30fps) so a large load's many batch wakes don't each re-do the O(N)
  /// reload/rebuild. Keystroke/open, `onRenderWake()`, and `setSidebarVisible()`
  /// still call `refresh()` directly (immediate); only the background traversal
  /// wake path goes through here.
  private lazy var refreshCoalescer = PickerRefreshCoalescer(
    schedule: { work in
      DispatchQueue.main.asyncAfter(
        deadline: .now() + PickerRefreshCoalescer.refreshInterval, execute: work)
    },
    perform: { [weak self] in self?.refresh() })

  /// Neovim's live working directory the tree is currently rooted at (empty
  /// until nvim first reports it). Compared on each render wake to re-root on
  /// `:cd`/`DirChanged`.
  private var cwdPath: String = ""
  /// The synthetic root node: the cwd ITSELF, shown as a single top-level node
  /// (its basename), expanded on init. `nil` until the cwd is known.
  private var rootItem: TreeItem?
  /// Whether the synthetic root has been auto-expanded once after streaming in
  /// (VS Code / Snacks model: root expanded, its subdirectories collapsed).
  private var didInitialExpand = false

  /// The absolute path of the current buffer (accented), or `nil` for none.
  private var currentFileAbs: String?
  /// The last `current_file()` acted on, so `onRenderWake` only reveals on an
  /// actual change.
  private var lastCurrent: String = ""
  /// A reveal target whose file had not yet streamed in (so `rel_path_for`
  /// returned ""); retried on each refresh.
  private var pendingReveal: String?

  /// Builds the sidebar bound to a live core `handle` and the editor
  /// `paneContainer` (re-focused after a single-click open).
  init(handle: WolfCoreHandle, paneContainer: PaneContainerView) {
    self.handle = handle
    self.paneContainer = paneContainer
    super.init(frame: .zero)
    buildSubviews()
    // Weak self: FileTreeView owns the memo, which owns this closure; a strong
    // capture would form a retain cycle.
    childrenMemo = TreeChildrenMemo { [weak self] dir in
      self?.projectChildren(dir: dir) ?? []
    }
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("FileTreeView is not constructed from a nib")
  }

  /// Starts the long-lived file-tree search, roots the tree at nvim's live cwd
  /// (shown as a single synthetic root node), and does an initial refresh so the
  /// root / top level appears immediately.
  func start() {
    search = handle.start_tree_search()
    cwdPath = handle.cwd().toString()
    rootItem = cwdPath.isEmpty ? nil : makeRoot(cwdPath)
    refresh()
  }

  /// Re-roots the tree at a new cwd (`:cd`/`DirChanged`): rebuilds the synthetic
  /// root, restarts the tree search (dropping the old handle cancels its
  /// traversal and re-registers the same tree wake), clears the cached items and
  /// the expand flag, and re-arms a reveal of the current buffer into the NEW
  /// tree. Cancels any pending coalesced refresh FIRST: reroot rebuilds the
  /// outline wholesale, so a stale pending fire against the OLD tree must not
  /// run (this method does its own final `refresh()`).
  func reroot(to cwd: String) {
    refreshCoalescer.cancelPending()
    cwdPath = cwd
    rootItem = makeRoot(cwd)
    search = handle.start_tree_search()
    itemsByRelPath = [:]
    childrenMemo.invalidate()
    refreshGate.resetReloadTracking()
    didInitialExpand = false
    pendingReveal = currentFileAbs
    refresh()
  }

  /// Reloads the outline from the latest streamed snapshot (preserving expansion
  /// + selection by item identity), then retries any pending reveal. No-op once
  /// closed.
  ///
  /// Gated on two axes: the visibility gate skips all work (deferring a catch-up)
  /// while the sidebar is collapsed, and the version gate reloads the outline only
  /// when the snapshot version has advanced since the last reload — but a pending
  /// reveal is always retried (outline structure unchanged; only selection/scroll
  /// move, children served from the still-valid memo).
  func refresh() {
    guard search != nil else { return }
    guard refreshGate.requestRefresh() else { return }
    let version = search?.version() ?? 0
    if refreshGate.shouldReload(version: version) {
      childrenMemo.invalidate()
      outlineView.reloadData()
      if !didInitialExpand, let rootItem, !children(of: rootItem).isEmpty {
        outlineView.expandItem(rootItem)
        didInitialExpand = true
      }
    }
    if let abs = pendingReveal {
      reveal(abs: abs)
    }
  }

  /// Requests a COALESCED refresh: a burst of streaming tree-traversal wakes
  /// collapses to at most one trailing `refresh()` per frame. Called by
  /// `TreeWakeBridge` on each traversal batch (the O(N) reload path); the
  /// immediate paths (`start()`, `onRenderWake()`, `setSidebarVisible()`,
  /// `reroot(to:)`) still call `refresh()` directly.
  func scheduleRefresh() {
    refreshCoalescer.request()
  }

  /// Relays the sidebar's visibility (from the enclosing split item's
  /// `isCollapsed` KVO). Becoming visible with a refresh deferred while hidden
  /// runs exactly one catch-up refresh.
  func setSidebarVisible(_ visible: Bool) {
    if refreshGate.setVisible(visible) {
      refresh()
    }
  }

  /// Reacts to an editor render wake: if the file the active pane shows changed,
  /// records it, re-accents, and reveals it.
  func onRenderWake() {
    // Re-root FIRST so a simultaneous cwd + current-file change reveals into
    // the NEW tree.
    let cwd = handle.cwd().toString()
    if !cwd.isEmpty && cwd != cwdPath {
      reroot(to: cwd)
    }
    // What the active pane SHOWS, which for a preview pane is the Markdown source
    // it renders rather than its unnamed scratch buffer.
    let current = SidebarRowStyle.markedFile(
      previewSource: paneContainer?.activePreviewSourcePath,
      currentFile: handle.current_file().toString())
    if current == lastCurrent { return }
    lastCurrent = current
    currentFileAbs = current.isEmpty ? nil : current
    pendingReveal = current.isEmpty ? nil : current
    refresh()
    restyleVisibleRows()
  }

  /// Reveals `abs` in the outline: expands its ancestor directories (top-down,
  /// materializing their cached `TreeItem`s) BEFORE locating the row, then
  /// selects + scrolls to it. Leaves `pendingReveal` set when the file has not
  /// streamed in yet (so the next refresh retries); clears it on success.
  func reveal(abs: String) {
    guard let search else { return }
    let rel = search.rel_path_for(abs).toString()
    if rel.isEmpty {
      // Not resolvable yet; leave pendingReveal set for the next refresh.
      pendingReveal = abs
      return
    }

    // Expand the synthetic cwd root first so its children materialize.
    if let rootItem {
      outlineView.expandItem(rootItem)
    }

    // Expand ancestors top-down so each child materializes before the next.
    for ancestor in wolf_ffi_ancestors_to_reveal(rel) {
      if let item = itemsByRelPath[ancestor.as_str().toString()] {
        outlineView.expandItem(item)
      }
    }

    if let item = itemsByRelPath[rel] {
      let row = outlineView.row(forItem: item)
      if row >= 0 {
        outlineView.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
        outlineView.scrollRowToVisible(row)
        pendingReveal = nil
      }
    }
  }

  /// Single-click activation: a directory toggles its expansion; a file opens in
  /// the active pane, routed by that pane's mode — a PREVIEW pane previews the
  /// file (`WolfCoreHandle.preview_file`, falling back to `open` when the file is
  /// not previewable, which converts the pane to edit mode), an EDIT pane edits it
  /// (`WolfCoreHandle.open`). Then focus returns to the editor.
  func activate(item: TreeItem) {
    if item.isDir {
      if outlineView.isItemExpanded(item) {
        outlineView.collapseItem(item)
      } else {
        outlineView.expandItem(item)
      }
    } else {
      if let paneContainer, paneContainer.activePaneIsPreview,
        handle.preview_file(paneContainer.activeWin, item.absPath) != 0
      {
        paneContainer.onRenderWake()
      } else {
        handle.open(item.absPath)
      }
      if let paneContainer {
        window?.makeFirstResponder(paneContainer)
      }
    }
  }

  /// Drops the file-tree search, cancelling the background traversal, and
  /// cancels any pending coalesced refresh (a late armed fire then finds the
  /// search `nil` and no-ops).
  func close() {
    refreshCoalescer.cancelPending()
    search = nil
  }

  // MARK: - NSOutlineViewDataSource

  func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
    children(of: item).count
  }

  func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
    children(of: item)[index]
  }

  func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
    (item as? TreeItem)?.isDir ?? false
  }

  // MARK: - NSOutlineViewDelegate

  func outlineView(
    _ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any
  ) -> NSView? {
    let cell =
      outlineView.makeView(withIdentifier: Self.cellIdentifier, owner: self)
      as? TreeCellView ?? makeCell()
    guard let treeItem = item as? TreeItem else { return cell }
    cell.textField?.stringValue = treeItem.name
    cell.isCurrent = isCurrent(treeItem)
    return cell
  }

  // MARK: - Private

  /// Whether `item` is the file the editor currently has open (directories never
  /// are).
  private func isCurrent(_ item: TreeItem) -> Bool {
    !item.isDir && item.absPath == currentFileAbs
  }

  /// Moves the current-buffer accent over the rows AppKit already has on screen.
  /// The version gate skips the reload whenever the snapshot has not advanced —
  /// which is the common case for a buffer switch — and a skipped reload never
  /// re-runs `viewFor`, so without this the accent would stay on the old row.
  private func restyleVisibleRows() {
    outlineView.enumerateAvailableRowViews { rowView, row in
      guard let item = outlineView.item(atRow: row) as? TreeItem,
        let cell = rowView.view(atColumn: 0) as? TreeCellView
      else { return }
      cell.isCurrent = isCurrent(item)
    }
  }

  /// Builds the synthetic root `TreeItem` for `cwd`: a directory node whose name
  /// is the cwd's basename, with the sentinel rel-path `""` (so its children are
  /// `children_of("")`). Kept OUT of `itemsByRelPath`.
  private func makeRoot(_ cwd: String) -> TreeItem {
    TreeItem(
      isDir: true,
      name: URL(fileURLWithPath: cwd).lastPathComponent,
      relPath: "",
      absPath: cwd
    )
  }

  /// Projects the immediate children of `item` from the latest streamed
  /// snapshot. The top level (`nil`) is the single synthetic cwd root; the root
  /// (rel-path `""`) and deeper directories project `children_of(relPath)`,
  /// reusing cached `TreeItem`s by `relPath` so the outline keeps stable
  /// identity. The synthetic root is never cached in `itemsByRelPath`.
  private func children(of item: Any?) -> [TreeItem] {
    if item == nil {
      // The synthetic root is not memoized (it is not a real dir projection).
      return rootItem.map { [$0] } ?? []
    }
    guard let treeItem = item as? TreeItem else { return [] }
    return childrenMemo.children(ofDir: treeItem.relPath)
  }

  /// Projects the immediate children of directory `dir` from the latest streamed
  /// snapshot, reusing cached `TreeItem`s by `relPath` so the outline keeps stable
  /// identity. Invoked (at most once per `dir` per reload) by `childrenMemo`.
  private func projectChildren(dir: String) -> [TreeItem] {
    guard let search else { return [] }
    var result: [TreeItem] = []
    for node in search.children_of(dir) {
      let rel = node.rel_path().toString()
      let cached: TreeItem
      if let existing = itemsByRelPath[rel] {
        existing.isDir = node.is_dir()
        existing.name = node.name().toString()
        existing.absPath = node.abs_path().toString()
        cached = existing
      } else {
        cached = TreeItem(
          isDir: node.is_dir(),
          name: node.name().toString(),
          relPath: rel,
          absPath: node.abs_path().toString()
        )
        itemsByRelPath[rel] = cached
      }
      result.append(cached)
    }
    return result
  }

  /// Builds + lays out the outline view inside a scroll view filling the sidebar.
  private func buildSubviews() {
    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("file"))
    column.resizingMask = .autoresizingMask
    outlineView.addTableColumn(column)
    outlineView.outlineTableColumn = column
    outlineView.headerView = nil
    outlineView.allowsMultipleSelection = false
    outlineView.dataSource = self
    outlineView.delegate = self
    outlineView.target = self
    outlineView.action = #selector(handleClick)
    // The `.sidebar` split item supplies vibrancy, so draw clear: an own `NSVisualEffectView` would
    // double it. The cwd root is a normal row, not an `isGroupItem` group row.
    outlineView.style = .sourceList
    outlineView.backgroundColor = .clear

    scrollView.translatesAutoresizingMaskIntoConstraints = false
    scrollView.drawsBackground = false
    scrollView.documentView = outlineView
    scrollView.hasVerticalScroller = true
    scrollView.autohidesScrollers = true
    addSubview(scrollView)

    NSLayoutConstraint.activate([
      scrollView.topAnchor.constraint(equalTo: topAnchor),
      scrollView.leadingAnchor.constraint(equalTo: leadingAnchor),
      scrollView.trailingAnchor.constraint(equalTo: trailingAnchor),
      scrollView.bottomAnchor.constraint(equalTo: bottomAnchor),
    ])
  }

  /// Creates a reusable outline cell: a `TreeCellView` with a single-line,
  /// vertically centered label.
  private func makeCell() -> TreeCellView {
    let cell = TreeCellView()
    cell.identifier = Self.cellIdentifier

    let label = NSTextField(labelWithString: "")
    label.translatesAutoresizingMaskIntoConstraints = false
    label.font = font
    label.lineBreakMode = .byTruncatingMiddle
    label.drawsBackground = false
    cell.addSubview(label)
    cell.textField = label

    NSLayoutConstraint.activate([
      label.leadingAnchor.constraint(equalTo: cell.leadingAnchor),
      label.trailingAnchor.constraint(equalTo: cell.trailingAnchor),
      label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
    ])
    return cell
  }

  /// Routes a single click on a row to `activate(item:)`.
  @objc private func handleClick() {
    let row = outlineView.clickedRow
    guard row >= 0, let item = outlineView.item(atRow: row) as? TreeItem else { return }
    activate(item: item)
  }
}
