//
//  PickerView.swift
//  The AppKit file picker: a floating, top-center overlay panel with a search
//  field and a results list, fed by the Rust file-search bridge.
//
//  The macOS analog of wolf-gnome's `picker.rs` widget wiring (an `Overlay` over
//  the editor, a `SearchEntry` + `ListBox` fed by `start_file_search` /
//  `rank_matches`). The pure decision logic (trigger chord, list navigation,
//  selection restore, empty-query gating, filename/parent derivation) lives in
//  the headless `WolfKeyMap.PickerSeam` seam; this file is the AppKit shell that
//  drives it and is exercised by manual smoke only.
//
//  Row rendering is file-agnostic (`PickerRow` + `PickerRowCellView`) so the same
//  panel can back other selectors; only `refresh()` (which maps `FfiFileEntry`s
//  into `PickerRow`s) and `activateSelection()` (which opens a row's path) are
//  file-specific.
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
    /// Initial panel height used only for the view's starting frame; the live
    /// height is content-driven (just the search field when empty, growing with
    /// the results up to `maxListHeight`).
    private static let panelHeight: CGFloat = 400
    /// Maximum height of the results list before it scrolls. The panel is only as
    /// tall as its content: the search field alone when empty, then the field plus
    /// the results (capped here) once rows appear.
    private static let maxListHeight: CGFloat = 360
    /// Top margin from the window's top edge (parity with gnome's margin).
    private static let topMargin: CGFloat = 24
    /// Inner padding around the search field + list.
    private static let padding: CGFloat = 8
    /// Extra vertical padding added to the measured font/icon height to form each
    /// list row's height, so the selection highlight has comfortable breathing
    /// room around the (vertically centered) content.
    private static let rowVerticalPadding: CGFloat = 10
    /// Point size of the (larger) search field font.
    private static let searchFontSize: CGFloat = 20
    /// Corner radius of the floating panel.
    private static let cornerRadius: CGFloat = 8
    /// Fallback leading inset for a row icon when the search-field cell rect is
    /// unavailable (the magnifier's typical left offset).
    private static let fallbackIconLeadingInset: CGFloat = 8
    /// Fallback leading inset for a row's typed-text column when the search-field
    /// cell rect is unavailable.
    private static let fallbackSearchTextLeadingInset: CGFloat = 32
    /// Reuse identifier for the list's cells.
    private static let cellIdentifier = NSUserInterfaceItemIdentifier("PickerCell")

    private let handle: WolfCoreHandle
    private weak var paneContainer: PaneContainerView?

    private let searchField = NSSearchField()
    private let tableView = NSTableView()
    private let scrollView = NSScrollView()

    /// The live file-search session while open; `nil` when closed.
    private var search: FfiFileSearch?
    /// The list rows (icon/title/subtitle/payload), file-agnostic.
    private var rows: [PickerRow] = []
    /// The query used by the previous `refresh()`, deciding reset-vs-preserve.
    private var lastQuery: String?
    /// Whether the picker is currently open (visible + focused).
    private var isOpen = false

    /// Leading inset that aligns a row's icon with the search magnifier, computed
    /// once from the search-field cell rects.
    private var iconLeadingInset = PickerView.fallbackIconLeadingInset
    /// Leading inset that aligns a row's title with the typed text (clamped so it
    /// never overlaps the icon), computed once from the search-field cell rects.
    private var labelLeadingInset = PickerView.fallbackSearchTextLeadingInset

    /// The scroll view's height (the results area); driven to the content height
    /// (capped at `maxListHeight`) when rows are shown.
    private var listHeightConstraint: NSLayoutConstraint!
    /// Pins the scroll view below the search field; active only when rows are shown.
    private var scrollTopConstraint: NSLayoutConstraint!
    /// Pins the panel's bottom below the scroll view; active only when rows are shown.
    private var listBottomConstraint: NSLayoutConstraint!
    /// Pins the panel's bottom directly below the search field; active when EMPTY,
    /// so the panel is just a text field with no reserved results space.
    private var emptyBottomConstraint: NSLayoutConstraint!

    /// Builds the picker bound to a live core `handle` and the editor
    /// `paneContainer` (re-focused on close). Starts hidden.
    init(handle: WolfCoreHandle, paneContainer: PaneContainerView) {
        self.handle = handle
        self.paneContainer = paneContainer
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

    /// Pins the panel top-center within its superview (the editor content view) at
    /// a fixed WIDTH; the height is content-driven (see `updateResultsLayout`).
    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        guard let superview else { return }
        NSLayoutConstraint.activate([
            centerXAnchor.constraint(equalTo: superview.centerXAnchor),
            topAnchor.constraint(equalTo: superview.topAnchor, constant: Self.topMargin),
            widthAnchor.constraint(equalToConstant: Self.panelWidth),
        ])
    }

    /// Opens the picker: starts a file search, shows + focuses the field. The
    /// picker opens with NO result rows (empty query), just the search field.
    /// Idempotent.
    func open() {
        if isOpen { return }
        search = handle.start_file_search()
        isOpen = true
        isHidden = false
        searchField.stringValue = ""
        window?.makeFirstResponder(searchField)
        // Empty query -> no rows until the user types.
        rows = []
        lastQuery = nil
        tableView.reloadData()
        updateResultsLayout()
    }

    /// Closes the picker: drops the file search (cancelling traversal), hides, and
    /// returns focus to the editor. Idempotent.
    func close() {
        if !isOpen { return }
        isOpen = false
        // Dropping the FileSearch cancels the background traversal.
        search = nil
        isHidden = true
        if let paneContainer {
            window?.makeFirstResponder(paneContainer)
        }
    }

    /// Rebuilds the results list for the current query from the latest search
    /// snapshot. No-op when closed.
    ///
    /// An empty or whitespace-only query shows NO rows (per
    /// `PickerSeam.shouldShowResults`). Otherwise each ranked `FfiFileEntry` is
    /// mapped into a file-agnostic `PickerRow` (title = base filename, subtitle =
    /// relative parent dir, payload = absolute path, icon = Finder icon). If the
    /// query is unchanged since the previous refresh (a streaming traversal wake,
    /// not a keystroke) the selection index is PRESERVED across the rebuild
    /// (clamped); when the query changed (typing) or it is the first refresh after
    /// `open`, the selection resets to the top.
    func refresh() {
        guard isOpen, let search else { return }
        let query = searchField.stringValue

        guard PickerSeam.shouldShowResults(query: query) else {
            rows = []
            lastQuery = query
            tableView.reloadData()
            updateResultsLayout()
            return
        }

        let resetToTop = (lastQuery != query)
        lastQuery = query
        let prevSelected = tableView.selectedRow >= 0 ? tableView.selectedRow : nil

        var newRows: [PickerRow] = []
        for entry in search.ranked(query, UInt(Self.maxRows)) {
            let display = entry.display().toString()
            let abs = entry.path().toString()
            newRows.append(
                PickerRow(
                    icon: fileIcon(forPath: abs),
                    title: PickerSeam.baseName(fromRelativePath: display),
                    subtitle: PickerSeam.parentRelativePath(fromRelativePath: display),
                    payload: abs))
        }
        rows = newRows
        tableView.reloadData()
        updateResultsLayout()

        if let idx = PickerSeam.restoredIndex(
            resetToTop: resetToTop, prev: prevSelected, newLen: rows.count)
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
            current: current, delta: delta, nRows: rows.count)
        {
            tableView.selectRowIndexes(IndexSet(integer: next), byExtendingSelection: false)
            tableView.scrollRowToVisible(next)
        }
    }

    /// Opens the selected row's payload (or the first row if none is selected) via
    /// `WolfCoreHandle.open`, then closes. With ZERO rows it is a true no-op.
    func activateSelection() {
        if rows.isEmpty { return }
        let idx = tableView.selectedRow
        let payload = (idx >= 0 && idx < rows.count) ? rows[idx].payload : rows[0].payload
        handle.open(payload)
        close()
    }

    // MARK: - NSTableViewDataSource / NSTableViewDelegate

    func numberOfRows(in tableView: NSTableView) -> Int {
        rows.count
    }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        let cell =
            tableView.makeView(withIdentifier: Self.cellIdentifier, owner: self)
            as? PickerRowCellView ?? makeCell()
        if row < rows.count {
            cell.configure(with: rows[row])
        }
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

    /// Builds + lays out the search field and results list inside the panel,
    /// computing the row alignment insets from the search-field cell rects.
    private func buildSubviews() {
        searchField.translatesAutoresizingMaskIntoConstraints = false
        searchField.delegate = self
        searchField.font = NSFont.systemFont(ofSize: Self.searchFontSize)
        searchField.placeholderString = "Open file…"
        // No focus outline on the search field.
        searchField.focusRingType = .none
        (searchField.cell as? NSSearchFieldCell)?.focusRingType = .none

        computeAlignmentInsets()

        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("file"))
        column.resizingMask = .autoresizingMask
        tableView.addTableColumn(column)
        tableView.headerView = nil
        tableView.allowsMultipleSelection = false
        tableView.dataSource = self
        tableView.delegate = self
        tableView.target = self
        tableView.doubleAction = #selector(handleDoubleClick)
        // Fixed row height sized for the icon + stacked title/subtitle.
        tableView.usesAutomaticRowHeights = false
        tableView.rowHeight = PickerRowCellView.rowHeight(
            iconSize: PickerRowCellView.iconSize,
            titleFont: PickerRowCellView.titleFont,
            subtitleFont: PickerRowCellView.subtitleFont,
            verticalPadding: Self.rowVerticalPadding)

        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.drawsBackground = false

        addSubview(searchField)
        addSubview(scrollView)

        let pad = Self.padding
        // The results area (`scrollView`) is only present when there are rows: its
        // top/height/bottom constraints are toggled against `emptyBottomConstraint`
        // by `updateResultsLayout`, so an empty picker is just the search field.
        scrollTopConstraint = scrollView.topAnchor.constraint(
            equalTo: searchField.bottomAnchor, constant: pad)
        listHeightConstraint = scrollView.heightAnchor.constraint(equalToConstant: 0)
        listBottomConstraint = bottomAnchor.constraint(
            equalTo: scrollView.bottomAnchor, constant: pad)
        emptyBottomConstraint = bottomAnchor.constraint(
            equalTo: searchField.bottomAnchor, constant: pad)

        NSLayoutConstraint.activate([
            searchField.topAnchor.constraint(equalTo: topAnchor, constant: pad),
            searchField.leadingAnchor.constraint(equalTo: leadingAnchor, constant: pad),
            searchField.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -pad),
            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: pad),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -pad),
        ])
        updateResultsLayout()
    }

    /// Sizes the panel to its content: when there are no rows the panel is just the
    /// search field (the scroll view is hidden and its constraints are inactive, so
    /// no results space is reserved); when rows are present the scroll view is shown
    /// and sized to the content height, capped at `maxListHeight` (beyond which it
    /// scrolls).
    private func updateResultsLayout() {
        if rows.isEmpty {
            scrollView.isHidden = true
            scrollTopConstraint.isActive = false
            listHeightConstraint.isActive = false
            listBottomConstraint.isActive = false
            emptyBottomConstraint.isActive = true
        } else {
            emptyBottomConstraint.isActive = false
            let contentHeight = CGFloat(rows.count) * tableView.rowHeight
            listHeightConstraint.constant = min(contentHeight, Self.maxListHeight)
            scrollView.isHidden = false
            scrollTopConstraint.isActive = true
            listHeightConstraint.isActive = true
            listBottomConstraint.isActive = true
        }
    }

    /// Computes `iconLeadingInset` / `labelLeadingInset` once from the search
    /// field's magnifier + text rects (falling back to constants if the cell cast
    /// fails), so each row's icon aligns with the magnifier and its title aligns
    /// with the typed text without overlapping a wide icon.
    private func computeAlignmentInsets() {
        let fieldWidth = Self.panelWidth - 2 * Self.padding
        let fieldHeight = searchField.intrinsicContentSize.height
        let bounds = NSRect(x: 0, y: 0, width: fieldWidth, height: fieldHeight)
        searchField.frame = bounds

        let searchTextLeadingInset: CGFloat
        if let cell = searchField.cell as? NSSearchFieldCell {
            iconLeadingInset = cell.searchButtonRect(forBounds: bounds).minX
            searchTextLeadingInset = cell.searchTextRect(forBounds: bounds).minX
        } else {
            iconLeadingInset = Self.fallbackIconLeadingInset
            searchTextLeadingInset = Self.fallbackSearchTextLeadingInset
        }
        labelLeadingInset = max(
            searchTextLeadingInset,
            iconLeadingInset + PickerRowCellView.iconSize + PickerRowCellView.iconToTextGap)
    }

    /// Creates a reusable, file-agnostic row cell configured with the computed
    /// alignment insets.
    private func makeCell() -> PickerRowCellView {
        let cell = PickerRowCellView(
            iconLeadingInset: iconLeadingInset, labelLeadingInset: labelLeadingInset)
        cell.identifier = Self.cellIdentifier
        return cell
    }

    /// The medium Finder icon for the file at `path`, sized for a row.
    private func fileIcon(forPath path: String) -> NSImage {
        let icon = NSWorkspace.shared.icon(forFile: path)
        icon.size = NSSize(width: PickerRowCellView.iconSize, height: PickerRowCellView.iconSize)
        return icon
    }

    /// Double-clicking a row opens it.
    @objc private func handleDoubleClick() {
        activateSelection()
    }
}
