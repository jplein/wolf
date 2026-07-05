//
//  CommandPaletteView.swift
//  The AppKit command palette: a floating, top-center overlay panel with a search
//  field and a results list, fed by the Rust command-palette bridge.
//
//  The command-palette sibling of `PickerView.swift` (the proven file picker is
//  left untouched). Structurally parallel — a search bar + `NSTableView` in an
//  `NSScrollView`, navigated via `PickerSeam.nextSelection` / `restoredIndex`,
//  rendered with `PickerRow` / `PickerRowCellView` — but backed by an opaque
//  `FfiCommandPalette` session instead of an `FfiFileSearch`, and activating a row
//  by KIND (dispatch a static command / raise a window / open-or-raise a recent
//  folder) rather than opening a file. Candidates are fixed at open (recent dirs +
//  the added windows), so there is NO wake bridge: `ranked()` re-filters
//  synchronously per keystroke. Exercised by manual smoke only.
//

import AppKit
import WolfFFI
import WolfKeyMap

/// One row of the command palette, carrying the label plus the kind-specific
/// payload the controller needs to activate it. Module-internal (not file-private)
/// because `WolfWindowController.activatePaletteRow(_:)` — in another file of the
/// same module — receives it through the `onActivate` closure.
struct CommandPaletteRow {
    /// The label shown in the list.
    let label: String
    /// Which of the three palette actions this row performs.
    let kind: PaletteSeam.PaletteRowKind
    /// For a `.staticCommand` row: the FFI `command_kind` to dispatch; 0 otherwise.
    let commandKind: UInt8
    /// For a `.switchWindow` row: the raise-target window index; 0 otherwise.
    let windowIndex: UInt16
    /// For an `.openRecent` row: the recent-folder path; `""` otherwise.
    let path: String
    /// The opaque MRU key recorded on activation.
    let mruKey: String
}

/// A floating command palette overlaid (top-center) on the editor surface.
///
/// Holds a live `FfiCommandPalette` session while open. Keyboard focus stays in
/// the search field so typing keeps filtering; Up/Down move the list selection,
/// Enter activates the selection, Esc closes. All state is plain main-thread
/// properties (every method runs on the AppKit main thread).
final class CommandPaletteView: NSView, NSTableViewDataSource, NSTableViewDelegate,
    NSTextFieldDelegate
{
    /// Maximum ranked rows materialized into the list.
    private static let maxRows = 200
    /// Panel width in points (parity with the picker).
    private static let panelWidth: CGFloat = 600
    /// Initial panel height used only for the view's starting frame; the live
    /// height is content-driven.
    private static let panelHeight: CGFloat = 400
    /// Maximum height of the results list before it scrolls.
    private static let maxListHeight: CGFloat = 360
    /// Top margin from the window's top edge.
    private static let topMargin: CGFloat = 24
    /// Inner padding around the search field + list.
    private static let padding: CGFloat = 8
    /// Extra vertical padding added to the measured font/icon height to form each
    /// list row's height.
    private static let rowVerticalPadding: CGFloat = 10
    /// Point size of the (larger) search field font.
    private static let searchFontSize: CGFloat = 20
    /// The (larger) search field font.
    private static let searchFont = NSFont.systemFont(ofSize: searchFontSize)
    /// Vertical breathing room added to the search font's line height.
    private static let searchFieldVerticalPadding: CGFloat = 14
    /// The search bar's explicit height: the font's line height plus breathing room.
    private static var searchFieldHeight: CGFloat {
        NSLayoutManager().defaultLineHeight(for: searchFont).rounded(.up)
            + searchFieldVerticalPadding
    }
    /// Horizontal inset of the magnifier (and the text field's trailing edge).
    private static let searchBarHInset: CGFloat = 8
    /// Point size of the magnifier symbol.
    private static let magnifierSize: CGFloat = 18
    /// Corner radius of the floating panel.
    private static let cornerRadius: CGFloat = 8
    /// Corner radius of the rounded search bar.
    private static let searchBarCornerRadius: CGFloat = 8
    /// Reuse identifier for the list's cells.
    private static let cellIdentifier = NSUserInterfaceItemIdentifier("CommandPaletteCell")

    private let handle: WolfCoreHandle
    private weak var paneContainer: PaneContainerView?

    /// The rounded search bar: a container holding the magnifier + text field.
    private let searchContainer = NSView()
    private let magnifierView = NSImageView()
    /// The borderless text field the user types into (the "search field").
    private let searchField = NSTextField()
    private let tableView = NSTableView()
    private let scrollView = NSScrollView()

    /// The live command-palette session while open; `nil` when closed.
    private var session: FfiCommandPalette?
    /// The list rows.
    private var rows: [CommandPaletteRow] = []
    /// The query used by the previous `refresh()`, deciding reset-vs-preserve.
    private var lastQuery: String?
    /// Whether the palette is currently open (visible + focused).
    private var isOpen = false

    /// Invoked with the chosen row on activation (before `close()`); the controller
    /// performs the row's kind-specific action. The chosen row reaches the
    /// controller this way (the same pattern as `PaneContainerView.onPickerTrigger`
    /// / `EmptyStateView.onOpenFolder`).
    var onActivate: ((CommandPaletteRow) -> Void)?

    /// Leading inset that aligns a row's icon with the search magnifier.
    private var iconLeadingInset: CGFloat { Self.searchBarHInset }
    /// The shared text column where BOTH a row's title and the search field's text
    /// begin, so they align.
    private var labelLeadingInset: CGFloat {
        iconLeadingInset + PickerRowCellView.iconSize + PickerRowCellView.iconToTextGap
    }

    /// The scroll view's height (the results area).
    private var listHeightConstraint: NSLayoutConstraint!
    /// Pins the scroll view below the search field; active only when rows are shown.
    private var scrollTopConstraint: NSLayoutConstraint!
    /// Pins the panel's bottom below the scroll view; active only when rows are shown.
    private var listBottomConstraint: NSLayoutConstraint!
    /// Pins the panel's bottom directly below the search field; active when EMPTY.
    private var emptyBottomConstraint: NSLayoutConstraint!

    /// Builds the palette bound to a live core `handle` and the editor
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
        fatalError("CommandPaletteView is not constructed from a nib")
    }

    /// Pins the panel top-center within its superview at a fixed WIDTH; the height
    /// is content-driven (see `updateResultsLayout`).
    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        guard let superview else { return }
        NSLayoutConstraint.activate([
            centerXAnchor.constraint(equalTo: superview.centerXAnchor),
            topAnchor.constraint(equalTo: superview.topAnchor, constant: Self.topMargin),
            widthAnchor.constraint(equalToConstant: Self.panelWidth),
        ])
    }

    /// Opens the palette: creates a fresh session seeded with the core's recent
    /// dirs + command MRU, feeds each OTHER window, shows + focuses the field, and
    /// does an initial `refresh()` so the empty-query MRU-first seed appears
    /// immediately. Idempotent.
    ///
    /// `otherWindows` is the coordinator's window list EXCLUDING this window (the
    /// current window cannot switch to itself).
    func open(otherWindows: [(index: Int, cwd: String)]) {
        if isOpen { return }
        let session = handle.command_palette()
        for window in otherWindows {
            session.add_window(UInt16(truncatingIfNeeded: window.index), window.cwd)
        }
        self.session = session
        isOpen = true
        isHidden = false
        searchField.stringValue = ""
        window?.makeFirstResponder(searchField)
        // Force the first refresh to select the top row.
        lastQuery = nil
        refresh()
    }

    /// Closes the palette: drops the session, hides, and returns focus to the
    /// editor. Idempotent.
    func close() {
        if !isOpen { return }
        isOpen = false
        session = nil
        isHidden = true
        if let paneContainer {
            window?.makeFirstResponder(paneContainer)
        }
    }

    /// Rebuilds the results list for the current query from the session's ranker.
    /// No-op when closed.
    ///
    /// An empty query renders ALL rows MRU-first (the seed shown on open); a
    /// non-empty query ranks/filters them. Each ranked `FfiCommandRow` is mapped
    /// into a `CommandPaletteRow`. The selection index is preserved across a rebuild
    /// with an unchanged query, else reset to the top.
    func refresh() {
        guard isOpen, let session else { return }
        let query = searchField.stringValue

        let resetToTop = (lastQuery != query)
        lastQuery = query
        let prevSelected = tableView.selectedRow >= 0 ? tableView.selectedRow : nil

        var newRows: [CommandPaletteRow] = []
        for row in session.ranked(query, UInt(Self.maxRows)) {
            // An unknown row-kind discriminant is skipped (defensive; the bridge
            // only ever emits 0..=2).
            guard let kind = PaletteSeam.PaletteRowKind(ffi: row.row_kind()) else { continue }
            newRows.append(
                CommandPaletteRow(
                    label: row.display().toString(),
                    kind: kind,
                    commandKind: row.command_kind(),
                    windowIndex: row.window_index(),
                    path: row.path().toString(),
                    mruKey: row.mru_key().toString()))
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

    /// Moves the list selection by `delta` rows (clamped, no wrap), leaving focus on
    /// the search field. No-op when there are no rows.
    func moveSelection(delta: Int) {
        let current = tableView.selectedRow
        if let next = PickerSeam.nextSelection(
            current: current, delta: delta, nRows: rows.count)
        {
            tableView.selectRowIndexes(IndexSet(integer: next), byExtendingSelection: false)
            tableView.scrollRowToVisible(next)
        }
    }

    /// Records the selected row's MRU key, hands it to `onActivate` for its
    /// kind-specific action, then closes. With ZERO rows it is a true no-op.
    func activateSelection() {
        if rows.isEmpty { return }
        let idx = tableView.selectedRow
        let row = (idx >= 0 && idx < rows.count) ? rows[idx] : rows[0]
        session?.record(row.mruKey)
        onActivate?(row)
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
            // Command rows carry no icon; only the label is shown.
            cell.configure(with: PickerRow(icon: nil, title: rows[row].label, payload: rows[row].mruKey))
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

    /// Builds + lays out the search bar (magnifier + borderless text field) and the
    /// results list inside the panel.
    private func buildSubviews() {
        buildSearchBar()

        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("command"))
        column.resizingMask = .autoresizingMask
        tableView.addTableColumn(column)
        tableView.headerView = nil
        tableView.allowsMultipleSelection = false
        tableView.style = .plain
        tableView.intercellSpacing = NSSize(width: 0, height: tableView.intercellSpacing.height)
        tableView.dataSource = self
        tableView.delegate = self
        tableView.target = self
        tableView.doubleAction = #selector(handleDoubleClick)
        tableView.usesAutomaticRowHeights = false
        tableView.rowHeight = PickerRowCellView.rowHeight(
            iconSize: PickerRowCellView.iconSize,
            titleFont: PickerRowCellView.titleFont,
            verticalPadding: Self.rowVerticalPadding)

        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.drawsBackground = false

        addSubview(searchContainer)
        addSubview(scrollView)

        let pad = Self.padding
        scrollTopConstraint = scrollView.topAnchor.constraint(
            equalTo: searchContainer.bottomAnchor, constant: pad)
        listHeightConstraint = scrollView.heightAnchor.constraint(equalToConstant: 0)
        listBottomConstraint = bottomAnchor.constraint(
            equalTo: scrollView.bottomAnchor, constant: pad)
        emptyBottomConstraint = bottomAnchor.constraint(
            equalTo: searchContainer.bottomAnchor, constant: pad)

        NSLayoutConstraint.activate([
            searchContainer.topAnchor.constraint(equalTo: topAnchor, constant: pad),
            searchContainer.leadingAnchor.constraint(equalTo: leadingAnchor, constant: pad),
            searchContainer.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -pad),
            searchContainer.heightAnchor.constraint(equalToConstant: Self.searchFieldHeight),
            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: pad),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -pad),
        ])
        updateResultsLayout()
    }

    /// Builds the rounded search bar: a container whose leading holds the magnifier
    /// and whose remaining width is a borderless, focus-ring-less `NSTextField`.
    private func buildSearchBar() {
        searchContainer.translatesAutoresizingMaskIntoConstraints = false
        searchContainer.wantsLayer = true
        searchContainer.layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
        searchContainer.layer?.cornerRadius = Self.searchBarCornerRadius
        searchContainer.layer?.borderWidth = 0

        magnifierView.translatesAutoresizingMaskIntoConstraints = false
        magnifierView.image = NSImage(
            systemSymbolName: "magnifyingglass", accessibilityDescription: "Search")
        magnifierView.symbolConfiguration = NSImage.SymbolConfiguration(
            pointSize: Self.magnifierSize, weight: .regular)
        magnifierView.contentTintColor = .secondaryLabelColor
        magnifierView.imageScaling = .scaleProportionallyUpOrDown

        searchField.translatesAutoresizingMaskIntoConstraints = false
        searchField.delegate = self
        searchField.font = Self.searchFont
        searchField.placeholderString = "Run a command…"
        searchField.isBordered = false
        searchField.isBezeled = false
        searchField.drawsBackground = false
        searchField.focusRingType = .none
        searchField.lineBreakMode = .byTruncatingTail
        searchField.usesSingleLineMode = true
        searchField.cell?.isScrollable = true

        searchContainer.addSubview(magnifierView)
        searchContainer.addSubview(searchField)

        NSLayoutConstraint.activate([
            magnifierView.leadingAnchor.constraint(
                equalTo: searchContainer.leadingAnchor, constant: Self.searchBarHInset),
            magnifierView.centerYAnchor.constraint(equalTo: searchContainer.centerYAnchor),
            magnifierView.widthAnchor.constraint(equalToConstant: Self.magnifierSize),
            magnifierView.heightAnchor.constraint(equalToConstant: Self.magnifierSize),
            searchField.leadingAnchor.constraint(
                equalTo: searchContainer.leadingAnchor, constant: labelLeadingInset),
            searchField.trailingAnchor.constraint(
                equalTo: searchContainer.trailingAnchor, constant: -Self.searchBarHInset),
            searchField.centerYAnchor.constraint(equalTo: searchContainer.centerYAnchor),
        ])
    }

    /// Sizes the panel to its content: just the search field when empty, else the
    /// field plus the scroll view sized to the content height (capped).
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

    /// Creates a reusable, file-agnostic row cell configured with the computed
    /// alignment insets.
    private func makeCell() -> PickerRowCellView {
        let cell = PickerRowCellView(
            iconLeadingInset: iconLeadingInset, labelLeadingInset: labelLeadingInset)
        cell.identifier = Self.cellIdentifier
        return cell
    }

    /// Double-clicking a row activates it.
    @objc private func handleDoubleClick() {
        activateSelection()
    }
}
