//
//  FileTreeView.swift
//  The AppKit file-tree sidebar: an `NSOutlineView` (in an `NSScrollView`) fed by
//  the Rust file-tree bridge, listing the cwd's files filtered by `.gitignore`
//  (dotfiles included). Single click opens a file (and returns focus to the
//  editor) or toggles a directory; the current buffer is bolded and revealed.
//
//  The macOS analog of wolf-gnome's `tree_view.rs`. The pure hierarchical
//  projection lives in `wolf-core`'s `tree` module and is reached over FFI
//  (`FfiFileSearch.children_of` / `rel_path_for`, the free
//  `wolf_ffi_ancestors_to_reveal`); this file is the AppKit shell that drives it,
//  exercised by manual smoke only (NSOutlineView needs a window server).
//

import AppKit
import WolfFFI
import WolfKeyMap

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
/// whole lifetime; the registered tree wake calls `refresh()` on each traversal
/// batch, and the editor render wake calls `onRenderWake()` to reveal + bold the
/// current buffer. All state is plain main-thread properties.
final class FileTreeView: NSView, NSOutlineViewDataSource, NSOutlineViewDelegate {
    /// Reuse identifier for the outline's text cells.
    private static let cellIdentifier = NSUserInterfaceItemIdentifier("TreeCell")

    private let handle: WolfCoreHandle
    private weak var gridView: GridView?

    /// The proportional system UI font for rows (the sidebar is chrome, not the
    /// editor grid), with a bold variant for the current buffer's row.
    private let font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
    private let boldFont = NSFont.boldSystemFont(ofSize: NSFont.systemFontSize)

    private let outlineView = NSOutlineView()
    private let scrollView = NSScrollView()

    /// The long-lived file-tree session; `nil` once closed.
    private var search: FfiFileSearch?
    /// One `TreeItem` per `relPath`, giving NSOutlineView stable item identity
    /// across reloads.
    private var itemsByRelPath: [String: TreeItem] = [:]

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

    /// The absolute path of the current buffer (bolded), or `nil` for none.
    private var currentFileAbs: String?
    /// The last `current_file()` acted on, so `onRenderWake` only reveals on an
    /// actual change.
    private var lastCurrent: String = ""
    /// A reveal target whose file had not yet streamed in (so `rel_path_for`
    /// returned ""); retried on each refresh.
    private var pendingReveal: String?

    /// Builds the sidebar bound to a live core `handle` and the editor `gridView`
    /// (re-focused after a single-click open).
    init(handle: WolfCoreHandle, gridView: GridView) {
        self.handle = handle
        self.gridView = gridView
        super.init(frame: .zero)
        buildSubviews()
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
    /// tree.
    func reroot(to cwd: String) {
        cwdPath = cwd
        rootItem = makeRoot(cwd)
        search = handle.start_tree_search()
        itemsByRelPath = [:]
        didInitialExpand = false
        pendingReveal = currentFileAbs
        refresh()
    }

    /// Reloads the outline from the latest streamed snapshot (preserving expansion
    /// + selection by item identity), then retries any pending reveal. No-op once
    /// closed.
    func refresh() {
        guard search != nil else { return }
        outlineView.reloadData()
        if !didInitialExpand, let rootItem, !children(of: rootItem).isEmpty {
            outlineView.expandItem(rootItem)
            didInitialExpand = true
        }
        if let abs = pendingReveal {
            reveal(abs: abs)
        }
    }

    /// Reacts to an editor render wake: if `current_file()` changed, records it,
    /// re-bolds, and reveals the new current buffer.
    func onRenderWake() {
        // Re-root FIRST so a simultaneous cwd + current-file change reveals into
        // the NEW tree.
        let cwd = handle.cwd().toString()
        if !cwd.isEmpty && cwd != cwdPath {
            reroot(to: cwd)
        }
        let current = handle.current_file().toString()
        if current == lastCurrent { return }
        lastCurrent = current
        currentFileAbs = current.isEmpty ? nil : current
        pendingReveal = current.isEmpty ? nil : current
        refresh()
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

    /// Single-click activation: a file opens (then focus returns to the editor); a
    /// directory toggles its expansion.
    func activate(item: TreeItem) {
        if item.isDir {
            if outlineView.isItemExpanded(item) {
                outlineView.collapseItem(item)
            } else {
                outlineView.expandItem(item)
            }
        } else {
            handle.open(item.absPath)
            if let gridView {
                window?.makeFirstResponder(gridView)
            }
        }
    }

    /// Drops the file-tree search, cancelling the background traversal.
    func close() {
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
            as? NSTableCellView ?? makeCell()
        guard let treeItem = item as? TreeItem else { return cell }
        cell.textField?.stringValue = treeItem.name
        let isCurrent = !treeItem.isDir && treeItem.absPath == currentFileAbs
        cell.textField?.font = isCurrent ? boldFont : font
        return cell
    }

    // MARK: - Private

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
        guard let search else { return [] }
        if item == nil {
            return rootItem.map { [$0] } ?? []
        }
        guard let treeItem = item as? TreeItem else { return [] }
        let dir = treeItem.relPath
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
        // The standard macOS source-list look: `.sourceList` drives the sidebar
        // row metrics and selection styling. The enclosing `.sidebar` split item
        // supplies the vibrancy material, so the outline + scroll view draw a
        // CLEAR background (no own `NSVisualEffectView` — that would double up the
        // vibrancy). The synthetic cwd root stays a normal selectable/expandable
        // row (no `isGroupItem` / group-row styling).
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

    /// Creates a reusable outline cell: an `NSTableCellView` with a single-line,
    /// vertically centered label.
    private func makeCell() -> NSTableCellView {
        let cell = NSTableCellView()
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
