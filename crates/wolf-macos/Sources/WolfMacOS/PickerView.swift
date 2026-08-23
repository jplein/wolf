//
//  PickerView.swift
//  The AppKit file picker: a floating, top-center overlay panel with a search
//  field and a results list, fed by the Rust file-search bridge.
//
//  The macOS analog of wolf-gnome's `picker.rs` widget wiring (an `Overlay` over
//  the editor, a `SearchEntry` + `ListBox` fed by `start_file_search` /
//  `rank_matches`). The pure decision logic (trigger chord, list navigation,
//  selection restore) lives in
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
  NSTextFieldDelegate
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
  /// The (larger) search field font.
  private static let searchFont = NSFont.systemFont(ofSize: searchFontSize)
  /// Vertical breathing room added to the search font's line height to form the
  /// search bar's height, so ascenders/descenders (e.g. the stem of "p") are
  /// never clipped. We render the search bar as a plain container (magnifier +
  /// borderless text field) rather than an `NSSearchField`, because the search
  /// cell's text rect is capped to the standard control height and clips a large
  /// font regardless of the field's height.
  private static let searchFieldVerticalPadding: CGFloat = 14
  /// The search bar's explicit height: the font's line height plus breathing room.
  private static var searchFieldHeight: CGFloat {
    NSLayoutManager().defaultLineHeight(for: searchFont).rounded(.up)
      + searchFieldVerticalPadding
  }
  /// Horizontal inset of the magnifier (and the text field's trailing edge)
  /// within the search bar.
  private static let searchBarHInset: CGFloat = 8
  /// Point size of the magnifier symbol.
  private static let magnifierSize: CGFloat = 18
  /// Corner radius of the floating panel.
  private static let cornerRadius: CGFloat = 8
  /// Corner radius of the rounded search bar.
  private static let searchBarCornerRadius: CGFloat = 8
  /// Reuse identifier for the list's cells.
  private static let cellIdentifier = NSUserInterfaceItemIdentifier("PickerCell")

  private let handle: WolfCoreHandle
  private weak var paneContainer: PaneContainerView?

  /// The panel host that presents/dismisses/refits this picker's floating child
  /// window (set by `WolfWindowController` after construction). Weak — the host is
  /// strong-held by the controller and owns this view (its panel's `contentView`).
  weak var host: PickerPanelHosting?

  /// The rounded search bar: a container holding the magnifier + text field. We
  /// build it ourselves (rather than an `NSSearchField`) so a large font is never
  /// clipped and the magnifier/text alignment is under our control.
  private let searchContainer = PickerChrome.searchBarBackdrop(
    cornerRadius: PickerView.searchBarCornerRadius)
  private let magnifierView = NSImageView()
  /// The borderless text field the user types into (the "search field").
  private let searchField = NSTextField()
  private let tableView = NSTableView()
  private let scrollView = NSScrollView()
  /// The panel's full-bleed background, behind every other subview.
  private let backdrop = PickerChrome.panelBackdrop(
    cornerRadius: PickerView.cornerRadius)

  /// The live file-search session while open; `nil` when closed.
  private var search: FfiFileSearch?
  /// The list rows (icon/title/payload), file-agnostic.
  private var rows: [PickerRow] = []
  /// The query used by the previous `refresh()`, deciding reset-vs-preserve.
  private var lastQuery: String?
  /// Whether the picker is currently open (visible + focused).
  private var isOpen = false

  /// Coalesces streaming traversal wakes into at most one `refresh()` per frame
  /// (~30fps) so a large load's many batch wakes don't each re-rank the whole
  /// accumulated list. Keystrokes and `open()` still call `refresh()` directly
  /// (immediate); only the background wake path goes through here.
  private lazy var refreshCoalescer = PickerRefreshCoalescer(
    schedule: { work in
      DispatchQueue.main.asyncAfter(
        deadline: .now() + PickerRefreshCoalescer.refreshInterval, execute: work)
    },
    perform: { [weak self] in self?.refresh() })

  /// Leading inset that aligns a row's icon with the search magnifier. The search
  /// bar and the row cells share the same leading edge (both inset by `padding`
  /// from the panel), so a row icon at the magnifier's container-inset lines up
  /// with the magnifier.
  private var iconLeadingInset: CGFloat { Self.searchBarHInset }
  /// The shared text column: the leading inset (from the search bar / a row cell's
  /// leading edge) where BOTH a row's title and the search field's text begin, so
  /// they align. It sits past the row icon with a gap, so the title has padding to
  /// the right of the icon; the search field's text is anchored to the same column.
  private var labelLeadingInset: CGFloat {
    iconLeadingInset + PickerRowCellView.iconSize + PickerRowCellView.iconToTextGap
  }

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
    fatalError("PickerView is not constructed from a nib")
  }

  /// Opens the picker: starts a file search, shows + focuses the field, and does
  /// an initial `refresh()` so the empty-query MRU-first result seed appears
  /// immediately (mirroring wolf-gnome's `PickerInner::open`). Idempotent.
  func open() {
    if isOpen { return }
    search = handle.start_file_search()
    isOpen = true
    searchField.stringValue = ""
    // Present the panel FIRST so `self.window` is the now-key panel, then focus
    // the search field within it.
    host?.presentPanel()
    window?.makeFirstResponder(searchField)
    // Force the first refresh to select the top row, regardless of any stale
    // selection/query left over from a previous session.
    lastQuery = nil
    refresh()
  }

  /// Closes the picker: drops the file search (cancelling traversal), orders its
  /// panel out, and returns focus to the editor. Idempotent.
  func close() {
    if !isOpen { return }
    // Tear down state BEFORE dismissing the panel: `orderOut` inside
    // `dismissPanel` can re-enter `windowDidResignKey → close()`; the `isOpen`
    // guard above makes that re-entry a no-op.
    isOpen = false
    refreshCoalescer.cancelPending()
    // Dropping the FileSearch cancels the background traversal.
    search = nil
    host?.dismissPanel()
  }

  /// Rebuilds the results list for the current query from the latest search
  /// snapshot. No-op when closed.
  ///
  /// An empty query renders ALL entries MRU-first (the seed shown on open); a
  /// non-empty query ranks/filters them. Each ranked `FfiFileEntry` is mapped
  /// into a file-agnostic `PickerRow` (title = the full working-directory-
  /// relative path, payload = absolute path, icon = Finder icon). If the
  /// query is unchanged since the previous refresh (a streaming traversal wake,
  /// not a keystroke) the selection index is PRESERVED across the rebuild
  /// (clamped); when the query changed (typing) or it is the first refresh after
  /// `open`, the selection resets to the top.
  func refresh() {
    guard isOpen, let search else { return }
    let query = searchField.stringValue

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
          // The full path relative to the working-directory root.
          title: display,
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

  /// Requests a COALESCED refresh for a streaming traversal wake (see
  /// `refreshCoalescer`): a burst of batch wakes during a large load collapses to
  /// at most one `refresh()` per frame instead of one full re-rank per batch.
  func scheduleRefresh() {
    refreshCoalescer.request()
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

  /// Opens the selected row's payload (or the first row if none is selected),
  /// routed by the active pane's mode: a PREVIEW pane previews the file
  /// (`WolfCoreHandle.preview_file`, falling back to `open` when the file is not
  /// previewable — the pane converts to edit mode); an EDIT pane edits it
  /// (`WolfCoreHandle.open`). Then closes. With ZERO rows it is a true no-op.
  func activateSelection() {
    if rows.isEmpty { return }
    let idx = tableView.selectedRow
    let payload = (idx >= 0 && idx < rows.count) ? rows[idx].payload : rows[0].payload
    if let paneContainer, paneContainer.activePaneIsPreview,
      handle.preview_file(paneContainer.activeWin, payload) != 0
    {
      paneContainer.onRenderWake()
    } else {
      handle.open(payload)
    }
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

  /// Builds + lays out the search bar (magnifier + borderless text field) and the
  /// results list inside the panel. The row alignment insets are derived from the
  /// search-bar layout constants (see `iconLeadingInset` / `labelLeadingInset`).
  private func buildSubviews() {
    buildSearchBar()

    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("file"))
    column.resizingMask = .autoresizingMask
    tableView.addTableColumn(column)
    tableView.headerView = nil
    tableView.allowsMultipleSelection = false
    // Full-width rows flush with the leading edge (the modern inset style would
    // indent rows and draw a rounded, inset selection), and no horizontal
    // intercell inset, so a row's icon lines up with the search magnifier.
    tableView.style = .plain
    tableView.intercellSpacing = NSSize(width: 0, height: tableView.intercellSpacing.height)
    tableView.dataSource = self
    tableView.delegate = self
    tableView.target = self
    tableView.doubleAction = #selector(handleDoubleClick)
    // Fixed row height sized for the icon + single title line.
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
    // The results area (`scrollView`) is only present when there are rows: its
    // top/height/bottom constraints are toggled against `emptyBottomConstraint`
    // by `updateResultsLayout`, so an empty picker is just the search bar.
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
  /// and whose remaining width is a borderless, focus-ring-less `NSTextField`. We
  /// avoid `NSSearchField` because its text cell clips a large font; here the text
  /// field sizes to its font and is vertically centered, so nothing is clipped.
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
    searchField.placeholderString = "Open file…"
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
      // Anchor the search text to the shared text column so it aligns with a
      // row's title (which sits past the wider row icon), not just past the
      // magnifier.
      searchField.leadingAnchor.constraint(
        equalTo: searchContainer.leadingAnchor, constant: labelLeadingInset),
      searchField.trailingAnchor.constraint(
        equalTo: searchContainer.trailingAnchor, constant: -Self.searchBarHInset),
      searchField.centerYAnchor.constraint(equalTo: searchContainer.centerYAnchor),
    ])
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
