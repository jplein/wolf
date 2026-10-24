// The AppKit buffer picker overlay, fed by the Rust buffer-picker bridge.

import AppKit
import WolfFFI
import WolfKeyMap

/// One row of the buffer picker, carrying the label plus the buffer handle to
/// switch to. Module-internal so `WolfWindowController` (in another file of the
/// same module) can receive it through the `onActivate` closure.
struct BufferPickerRow {
  /// The label shown in the list (cwd-relative path, or `[No Name]`).
  let label: String
  /// The Neovim buffer handle to switch the active pane to on activation.
  let bufnr: Int64
  /// Whether the buffer has unsaved modifications (shown in the row chrome).
  let modified: Bool
}

/// A floating buffer picker overlaid (top-center) on the editor surface.
///
/// Holds a live `FfiBufferPicker` session while open. Keyboard focus stays in the
/// search field so typing keeps filtering; Up/Down move the list selection, Enter
/// activates the selection, Esc closes. All state is plain main-thread properties
/// (every method runs on the AppKit main thread).
final class BufferPickerView: NSView, NSTableViewDataSource, NSTableViewDelegate,
  NSTextFieldDelegate
{
  /// Maximum ranked rows materialized into the list.
  private static let maxRows = 200
  /// Panel width in points (parity with the palette).
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
  private static let cellIdentifier = NSUserInterfaceItemIdentifier("BufferPickerCell")
  /// The marker suffixed to a modified buffer's label.
  private static let modifiedMarker = "  ●"

  private let handle: WolfCoreHandle
  private weak var paneContainer: PaneContainerView?

  /// The panel host that presents/dismisses/refits this picker's floating child
  /// window (set by `WolfWindowController` after construction). Weak — the host is
  /// strong-held by the controller and owns this view (its panel's `contentView`).
  weak var host: PickerPanelHosting?

  /// The rounded search bar: a container holding the magnifier + text field.
  private let searchContainer = PickerChrome.searchBarBackdrop(
    cornerRadius: BufferPickerView.searchBarCornerRadius)
  private let magnifierView = NSImageView()
  /// The borderless text field the user types into (the "search field").
  private let searchField = NSTextField()
  private let tableView = NSTableView()
  private let scrollView = NSScrollView()
  /// The panel's full-bleed background, behind every other subview.
  private let backdrop = PickerChrome.panelBackdrop(
    cornerRadius: BufferPickerView.cornerRadius)

  /// The live buffer-picker session while open; `nil` when closed.
  private var session: FfiBufferPicker?
  /// The list rows.
  private var rows: [BufferPickerRow] = []
  /// The query used by the previous `refresh()`, deciding reset-vs-preserve.
  private var lastQuery: String?
  /// Whether the picker is currently open (visible + focused).
  private var isOpen = false
  /// Whether the next `refresh()` is the initial empty-query open (which
  /// preselects ROW 2 so a bare <CR> switches to the most-recently-used OTHER
  /// buffer). Cleared after that first refresh.
  private var isInitialOpen = false

  /// Invoked with the chosen row on activation (before `close()`); the controller
  /// performs the buffer switch. The same `onActivate` pattern the command palette
  /// uses.
  var onActivate: ((BufferPickerRow) -> Void)?

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

  /// Builds the picker bound to a live core `handle` and the editor
  /// `paneContainer` (re-focused on close). Starts hidden.
  init(handle: WolfCoreHandle, paneContainer: PaneContainerView) {
    self.handle = handle
    self.paneContainer = paneContainer
    super.init(
      frame: NSRect(x: 0, y: 0, width: Self.panelWidth, height: Self.panelHeight))
    wantsLayer = true
    layer?.cornerRadius = Self.cornerRadius
    layer?.borderWidth = 1
    translatesAutoresizingMaskIntoConstraints = false
    buildSubviews()
    applyChromeColors()
  }

  /// Re-applies the layer chrome colors when the effective appearance changes at
  /// runtime (the OS switches dark↔light in `auto` mode, or a forced appearance
  /// propagates from `NSApp.appearance`). Layer CGColors do NOT auto-adapt, so
  /// they are re-resolved here; the search field + its container are marked for
  /// redraw so the typed text / placeholder re-resolve their dynamic colors too.
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    applyChromeColors()
    searchField.needsDisplay = true
    searchContainer.needsDisplay = true
  }

  /// Assigns the panel's border colour from the dynamic system NSColor, resolved
  /// against this view's CURRENT effective appearance. A bare
  /// `NSColor.separatorColor.cgColor` resolves against whatever appearance is
  /// globally current (often stale during an appearance change), so the
  /// conversion runs inside `performAsCurrentDrawingAppearance`. Neither
  /// background needs the same handling: a `PickerChrome` material re-resolves
  /// itself.
  private func applyChromeColors() {
    effectiveAppearance.performAsCurrentDrawingAppearance {
      layer?.borderColor = NSColor.separatorColor.cgColor
    }
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("BufferPickerView is not constructed from a nib")
  }

  /// Opens the picker: snapshots a fresh session from the core's listed buffers,
  /// shows + focuses the field, and does an initial `refresh()` so the empty-query
  /// MRU-first seed appears immediately with ROW 2 preselected. Idempotent.
  func open() {
    if isOpen { return }
    session = handle.buffer_picker()
    isOpen = true
    searchField.stringValue = ""
    // Present the panel FIRST so `self.window` is the now-key panel, then focus
    // the search field within it.
    host?.presentPanel()
    window?.makeFirstResponder(searchField)
    // Force the first refresh to preselect row 2.
    lastQuery = nil
    isInitialOpen = true
    refresh()
  }

  /// Closes the picker: drops the session, orders its panel out, and returns focus
  /// to the editor. Idempotent.
  func close() {
    if !isOpen { return }
    // Tear down before dismissing: `orderOut` can re-enter `close()` via `windowDidResignKey`,
    // and the `isOpen` guard above makes that re-entry a no-op.
    isOpen = false
    session = nil
    host?.dismissPanel()
  }

  /// Rebuilds the results list for the current query from the session's ranker.
  /// No-op when closed.
  ///
  /// An empty query renders ALL buffers MRU-first (current buffer at row 0); a
  /// non-empty query ranks/filters them. On the initial empty-query open ROW 2 is
  /// preselected (`min(1, count-1)`); otherwise the selection index is preserved
  /// across a rebuild with an unchanged query, else reset to the top.
  func refresh() {
    guard isOpen, let session else { return }
    let query = searchField.stringValue

    let resetToTop = (lastQuery != query)
    lastQuery = query
    let prevSelected = tableView.selectedRow >= 0 ? tableView.selectedRow : nil

    var newRows: [BufferPickerRow] = []
    for row in session.ranked(query, UInt(Self.maxRows)) {
      newRows.append(
        BufferPickerRow(
          label: row.display().toString(),
          bufnr: row.bufnr(),
          modified: row.modified()))
    }
    rows = newRows
    tableView.reloadData()
    updateResultsLayout()

    let initialOpen = isInitialOpen
    isInitialOpen = false
    if initialOpen {
      // Preselect row 2 (index 1) so a bare <CR> switches to the
      // most-recently-used OTHER buffer; clamp for a 0/1-row list.
      if !rows.isEmpty {
        let idx = min(1, rows.count - 1)
        tableView.selectRowIndexes(IndexSet(integer: idx), byExtendingSelection: false)
        tableView.scrollRowToVisible(idx)
      }
    } else if let idx = PickerSeam.restoredIndex(
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

  /// Hands the selected row to `onActivate` (the controller switches the active
  /// pane to that buffer), then closes. Records NO MRU. With ZERO rows it is a
  /// true no-op.
  func activateSelection() {
    if rows.isEmpty { return }
    let idx = tableView.selectedRow
    let row = (idx >= 0 && idx < rows.count) ? rows[idx] : rows[0]
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
      let title =
        rows[row].modified ? rows[row].label + Self.modifiedMarker : rows[row].label
      // Buffer rows carry no icon; only the label is shown.
      cell.configure(
        with: PickerRow(icon: nil, title: title, payload: String(rows[row].bufnr)))
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

    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("buffer"))
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

    addSubview(backdrop)
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
      backdrop.topAnchor.constraint(equalTo: topAnchor),
      backdrop.leadingAnchor.constraint(equalTo: leadingAnchor),
      backdrop.trailingAnchor.constraint(equalTo: trailingAnchor),
      backdrop.bottomAnchor.constraint(equalTo: bottomAnchor),
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
    searchField.placeholderString = "Switch to buffer…"
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
    // Grow/shrink the hosting panel with the results list (capped by
    // `maxListHeight`).
    host?.refitPanel()
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
