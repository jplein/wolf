//
//  PickerView.swift
//  The AppKit file picker: a floating, top-center overlay panel with a search
//  field and a results list, fed by the Rust file-search bridge.
//
//  The macOS analog of wolf-gnome's `picker.rs` widget wiring (an `Overlay` over
//  the editor, a `SearchEntry` + `ListBox` fed by `start_file_search` /
//  `rank_matches`). The pure decision logic (trigger chord, list navigation,
//  selection restore) lives in the headless `WolfKeyMap.PickerSeam` seam; this
//  file is the AppKit shell that drives it and is exercised by manual smoke only.
//

import AppKit
import WolfFFI
import WolfKeyMap

/// A floating file picker overlaid (top-center) on the editor surface.
///
/// Holds a live `FfiFileSearch` session while open (dropping it on close cancels
/// the background traversal). Keyboard focus stays in the `NSSearchField` so
/// typing keeps filtering; Up/Down move the list selection, Enter opens the
/// selection, Esc closes. All state is plain main-thread properties (every method
/// runs on the AppKit main thread).
final class PickerView: NSView, NSTableViewDataSource, NSTableViewDelegate,
    NSSearchFieldDelegate
{
    /// Maximum ranked rows materialized into the list (mirrors wolf-gnome's
    /// `PICKER_MAX_ROWS`).
    private static let maxRows = 200
    /// Panel width in points (parity with gnome's `PICKER_BOX_WIDTH`).
    private static let panelWidth: CGFloat = 600
    /// Panel height in points (parity with gnome's `PICKER_BOX_HEIGHT`).
    private static let panelHeight: CGFloat = 400
    /// Top margin from the window's top edge (parity with gnome's margin).
    private static let topMargin: CGFloat = 24
    /// Inner padding around the search field + list.
    private static let padding: CGFloat = 8
    /// Extra vertical padding added to the measured font cell height to form each
    /// list row's height, so the selection highlight has comfortable breathing room
    /// around the (vertically centered) text.
    private static let rowVerticalPadding: CGFloat = 6
    /// Horizontal inset of a row's label within its cell.
    private static let cellHorizontalInset: CGFloat = 4
    /// Corner radius of the floating panel.
    private static let cornerRadius: CGFloat = 8
    /// Reuse identifier for the list's text cells.
    private static let cellIdentifier = NSUserInterfaceItemIdentifier("PickerCell")

    private let handle: WolfCoreHandle
    private weak var gridView: GridView?
    /// The proportional system UI font used for the search field + result rows
    /// (the picker is chrome, not the editor grid, so it uses the native UI font
    /// rather than the editor's monospace cell font).
    private let font = NSFont.systemFont(ofSize: NSFont.systemFontSize)

    private let searchField = NSSearchField()
    private let tableView = NSTableView()
    private let scrollView = NSScrollView()

    /// The live file-search session while open; `nil` when closed.
    private var search: FfiFileSearch?
    /// Display strings parallel to the list rows.
    private var rowDisplays: [String] = []
    /// Absolute paths parallel to the list rows (row index -> path to open).
    private var rowPaths: [String] = []
    /// The query used by the previous `refresh()`, deciding reset-vs-preserve.
    private var lastQuery: String?
    /// Whether the picker is currently open (visible + focused).
    private var isOpen = false

    /// Builds the picker bound to a live core `handle` and the editor `gridView`
    /// (re-focused on close). The picker uses the proportional system UI font (see
    /// `font`), not the editor's monospace cell font. Starts hidden.
    init(handle: WolfCoreHandle, gridView: GridView) {
        self.handle = handle
        self.gridView = gridView
        super.init(
            frame: NSRect(x: 0, y: 0, width: Self.panelWidth, height: Self.panelHeight))
        wantsLayer = true
        layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
        layer?.cornerRadius = Self.cornerRadius
        layer?.borderWidth = 1
        layer?.borderColor = NSColor.separatorColor.cgColor
        translatesAutoresizingMaskIntoConstraints = false
        isHidden = true
        buildSubviews()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("PickerView is not constructed from a nib")
    }

    /// Pins the panel top-center within its superview (the editor content view)
    /// once added, at a fixed size.
    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        guard let superview else { return }
        NSLayoutConstraint.activate([
            centerXAnchor.constraint(equalTo: superview.centerXAnchor),
            topAnchor.constraint(equalTo: superview.topAnchor, constant: Self.topMargin),
            widthAnchor.constraint(equalToConstant: Self.panelWidth),
            heightAnchor.constraint(equalToConstant: Self.panelHeight),
        ])
    }

    /// Opens the picker: starts a file search, shows + focuses the field, and does
    /// an initial refresh so the MRU seed appears immediately. Idempotent.
    func open() {
        if isOpen { return }
        search = handle.start_file_search()
        isOpen = true
        isHidden = false
        searchField.stringValue = ""
        window?.makeFirstResponder(searchField)
        // Force the first refresh to select the top row, regardless of any stale
        // selection/query from a previous session.
        lastQuery = nil
        refresh()
    }

    /// Closes the picker: drops the file search (cancelling traversal), hides, and
    /// returns focus to the editor. Idempotent.
    func close() {
        if !isOpen { return }
        isOpen = false
        // Dropping the FileSearch cancels the background traversal.
        search = nil
        isHidden = true
        if let gridView {
            window?.makeFirstResponder(gridView)
        }
    }

    /// Rebuilds the results list for the current query from the latest search
    /// snapshot. No-op when closed.
    ///
    /// If the query is unchanged since the previous refresh (a streaming traversal
    /// wake, not a keystroke), the user's selection index is PRESERVED across the
    /// rebuild (clamped to the new row count); when the query changed (typing) or
    /// it is the first refresh after `open`, the selection resets to the top.
    func refresh() {
        guard isOpen, let search else { return }
        let query = searchField.stringValue
        let resetToTop = (lastQuery != query)
        lastQuery = query
        let prevSelected = tableView.selectedRow >= 0 ? tableView.selectedRow : nil

        var displays: [String] = []
        var paths: [String] = []
        for entry in search.ranked(query, UInt(Self.maxRows)) {
            displays.append(entry.display().toString())
            paths.append(entry.path().toString())
        }
        rowDisplays = displays
        rowPaths = paths
        tableView.reloadData()

        if let idx = PickerSeam.restoredIndex(
            resetToTop: resetToTop, prev: prevSelected, newLen: rowDisplays.count)
        {
            tableView.selectRowIndexes(IndexSet(integer: idx), byExtendingSelection: false)
            tableView.scrollRowToVisible(idx)
        }
    }

    /// Moves the list selection by `delta` rows (clamped, no wrap), leaving focus
    /// on the search field so typing continues. No-op when there are no rows.
    func moveSelection(delta: Int) {
        let current = tableView.selectedRow
        if let next = PickerSeam.nextSelection(
            current: current, delta: delta, nRows: rowDisplays.count)
        {
            tableView.selectRowIndexes(IndexSet(integer: next), byExtendingSelection: false)
            tableView.scrollRowToVisible(next)
        }
    }

    /// Opens the selected file (or the first row if none is selected) via
    /// `WolfCoreHandle.open`, then closes. With ZERO rows it is a true no-op.
    func activateSelection() {
        if rowPaths.isEmpty { return }
        let idx = tableView.selectedRow
        let path = (idx >= 0 && idx < rowPaths.count) ? rowPaths[idx] : rowPaths[0]
        handle.open(path)
        close()
    }

    // MARK: - NSTableViewDataSource / NSTableViewDelegate

    func numberOfRows(in tableView: NSTableView) -> Int {
        rowDisplays.count
    }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        let cell =
            tableView.makeView(withIdentifier: Self.cellIdentifier, owner: self)
            as? NSTableCellView ?? makeCell()
        cell.textField?.stringValue = row < rowDisplays.count ? rowDisplays[row] : ""
        return cell
    }

    // MARK: - NSControlTextEditingDelegate

    func control(
        _ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector
    ) -> Bool {
        switch commandSelector {
        case #selector(NSResponder.moveUp(_:)):
            moveSelection(delta: -1)
            return true
        case #selector(NSResponder.moveDown(_:)):
            moveSelection(delta: 1)
            return true
        case #selector(NSResponder.insertNewline(_:)):
            activateSelection()
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            close()
            return true
        default:
            return false
        }
    }

    func controlTextDidChange(_ obj: Notification) {
        refresh()
    }

    // MARK: - Private

    /// Builds + lays out the search field and results list inside the panel.
    private func buildSubviews() {
        searchField.translatesAutoresizingMaskIntoConstraints = false
        searchField.delegate = self
        searchField.font = font
        searchField.placeholderString = "Open file…"

        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("file"))
        column.resizingMask = .autoresizingMask
        tableView.addTableColumn(column)
        tableView.headerView = nil
        tableView.allowsMultipleSelection = false
        tableView.dataSource = self
        tableView.delegate = self
        tableView.target = self
        tableView.doubleAction = #selector(handleDoubleClick)
        // Fixed, font-derived row height so the selection highlight matches the
        // (vertically centered) text rather than floating above it.
        tableView.usesAutomaticRowHeights = false
        tableView.rowHeight =
            NSLayoutManager().defaultLineHeight(for: font).rounded(.up) + Self.rowVerticalPadding

        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.drawsBackground = false

        addSubview(searchField)
        addSubview(scrollView)

        let pad = Self.padding
        NSLayoutConstraint.activate([
            searchField.topAnchor.constraint(equalTo: topAnchor, constant: pad),
            searchField.leadingAnchor.constraint(equalTo: leadingAnchor, constant: pad),
            searchField.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -pad),
            scrollView.topAnchor.constraint(equalTo: searchField.bottomAnchor, constant: pad),
            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: pad),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -pad),
            scrollView.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -pad),
        ])
    }

    /// Creates a reusable list cell: an `NSTableCellView` whose single-line label is
    /// vertically centered within the row, so the selection highlight (which fills
    /// the row) is centered around the text.
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
            label.leadingAnchor.constraint(
                equalTo: cell.leadingAnchor, constant: Self.cellHorizontalInset),
            label.trailingAnchor.constraint(
                equalTo: cell.trailingAnchor, constant: -Self.cellHorizontalInset),
            label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    /// Double-clicking a row opens it.
    @objc private func handleDoubleClick() {
        activateSelection()
    }
}
