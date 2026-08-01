//
//  SearchView.swift
//  The AppKit sidebar SEARCH panel: a search field + three toggle buttons above an
//  `NSOutlineView` (in an `NSScrollView`) showing ripgrep-backed content-search
//  results as a two-level tree — matched files (expanded by default) with their
//  matching lines beneath, the match portion bold and the surrounding context
//  dimmed, each line wrapped to at most two rows. Search runs ONLY on Return.
//  Clicking a line opens the file at the match's line/column.
//
//  The pure projection + display math lives in wolf-core (`search_tree`: `project`,
//  `file_label`, `context_window`) and is reached over FFI (`FfiSearch.files`,
//  `FfiSearchFile`, `FfiSearchMatch`); the byte→UTF-16 span mapping + char budget
//  live in the `WolfKeyMap` seams. This file is the AppKit shell that drives them,
//  exercised by manual smoke only (NSOutlineView needs a window server).
//

import AppKit
import WolfFFI
import WolfKeyMap

/// A matched file row in the search-results outline (stable identity by
/// `absPath`, which `NSOutlineView` keys expansion/selection on).
final class SearchFileItem {
  let absPath: String
  var label: String
  var lines: [SearchLineItem]

  init(absPath: String, label: String, lines: [SearchLineItem]) {
    self.absPath = absPath
    self.label = label
    self.lines = lines
  }
}

/// A matching-line row beneath a `SearchFileItem`.
final class SearchLineItem {
  /// The absolute path of the file this line belongs to (for `open_at`).
  let absPath: String
  /// 1-based line number (cursor row).
  let lineNumber: UInt64
  /// 0-based byte column of the first match (cursor column).
  let column: UInt64
  /// The visible (budget-windowed) line text.
  let displayText: String
  /// Whether context was hidden on each side (ellipsis prefix/suffix).
  let truncatedLeft: Bool
  let truncatedRight: Bool
  /// Bold byte ranges into `displayText` (parallel start/end).
  let boldByteRanges: [(Int, Int)]

  init(
    absPath: String, lineNumber: UInt64, column: UInt64, displayText: String,
    truncatedLeft: Bool, truncatedRight: Bool, boldByteRanges: [(Int, Int)]
  ) {
    self.absPath = absPath
    self.lineNumber = lineNumber
    self.column = column
    self.displayText = displayText
    self.truncatedLeft = truncatedLeft
    self.truncatedRight = truncatedRight
    self.boldByteRanges = boldByteRanges
  }
}

/// The sidebar search panel: a search field + toggles feeding an `NSOutlineView`
/// of streamed content-search results.
///
/// Holds a transient `FfiSearch` (started via `start_search`, dropped/replaced on
/// each new run to cancel the prior rg subprocess). The registered search wake
/// calls `scheduleRefresh()` on each streamed batch (COALESCED to at most one
/// `refresh()` per frame). All state is plain main-thread properties.
final class SearchView: NSView, NSOutlineViewDataSource, NSOutlineViewDelegate {
  /// Reuse identifier for the outline's cells.
  private static let cellIdentifier = NSUserInterfaceItemIdentifier("SearchCell")
  /// The ellipsis glyph prepended/appended to a truncated line.
  private static let ellipsis = "\u{2026}"

  private let handle: WolfCoreHandle
  private weak var paneContainer: PaneContainerView?

  private let font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
  private let boldFont = NSFont.boldSystemFont(ofSize: NSFont.systemFontSize)

  private let searchField = NSTextField()
  private let caseButton = NSButton()
  private let regexButton = NSButton()
  private let allFilesButton = NSButton()
  private let outlineView = NSOutlineView()
  private let scrollView = NSScrollView()

  /// The current (transient) search session; `nil` before the first run / once
  /// closed.
  private var search: FfiSearch?
  /// The projected file rows, in display order.
  private var files: [SearchFileItem] = []
  /// Absolute paths of file rows the user has explicitly collapsed, so a rebuild
  /// preserves their collapsed state (files are expanded by default).
  private var collapsedFiles: Set<String> = []

  /// The visibility + version gate (deferred while the sidebar is collapsed;
  /// reloads only when the snapshot version advances).
  private let refreshGate = TreeRefreshGate(isVisible: true)
  /// The snapshot version the rows were last rebuilt at, and the char budget used,
  /// so a resize (budget change) also forces a rebuild.
  private var lastBudget: Int = 0

  /// Coalesces streaming search wakes into at most one `refresh()` per frame.
  private lazy var refreshCoalescer = PickerRefreshCoalescer(
    schedule: { work in
      DispatchQueue.main.asyncAfter(
        deadline: .now() + PickerRefreshCoalescer.refreshInterval, execute: work)
    },
    perform: { [weak self] in self?.refresh() })

  /// Builds the search panel bound to a live core `handle` and the editor
  /// `paneContainer` (re-focused after opening a result).
  init(handle: WolfCoreHandle, paneContainer: PaneContainerView) {
    self.handle = handle
    self.paneContainer = paneContainer
    super.init(frame: .zero)
    buildSubviews()
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("SearchView is not constructed from a nib")
  }

  /// Seeds the toggle buttons' initial states (case-sensitive from nvim's
  /// `:ignorecase`, one-shot). Safe to call once when the panel is first shown.
  func seedToggles() {
    caseButton.state =
      SearchToggleSeam.initialCaseSensitive(ignorecase: handle.ignorecase()) ? .on : .off
    regexButton.state = SearchToggleSeam.defaultUseRegex ? .on : .off
    allFilesButton.state = SearchToggleSeam.defaultAllFiles ? .on : .off
  }

  /// Make the search text field the window's first responder (called when the
  /// search panel is activated via menu / ⇧⌘F / the magnifier button).
  func focusSearchField() { window?.makeFirstResponder(searchField) }

  /// Relays the sidebar's visibility (from the enclosing split item's
  /// `isCollapsed` KVO). Becoming visible with a refresh deferred runs one catch-up.
  func setSidebarVisible(_ visible: Bool) {
    if refreshGate.setVisible(visible) {
      refresh()
    }
  }

  /// Requests a COALESCED refresh (called by `SearchWakeBridge` on each streamed
  /// batch). Idempotent and cheap (each `refresh()` is version-gated).
  func scheduleRefresh() {
    refreshCoalescer.request()
  }

  /// Rebuilds the result rows from the latest streamed snapshot when the version
  /// (or the char budget) has advanced, preserving each file's collapse state by
  /// `absPath`. No-op once closed / before the first run.
  func refresh() {
    guard let search else { return }
    guard refreshGate.requestRefresh() else { return }
    let version = search.version()
    let budget = currentBudget()
    let versionChanged = refreshGate.shouldReload(version: version)
    guard versionChanged || budget != lastBudget else { return }
    lastBudget = budget
    rebuildRows(from: search, budget: budget)
    outlineView.reloadData()
    autoExpandFiles()
  }

  /// Drops the search session (cancelling the rg subprocess) and any pending
  /// coalesced refresh.
  func close() {
    refreshCoalescer.cancelPending()
    search = nil
  }

  // MARK: - Actions

  /// Runs a fresh search from the field + toggle states (Return only). Replacing
  /// the session drops the prior `FfiSearch`, cancelling its rg subprocess.
  @objc private func runSearch() {
    let query = searchField.stringValue
    guard !query.isEmpty else {
      search = nil
      files = []
      outlineView.reloadData()
      return
    }
    search = handle.start_search(
      query,
      regexButton.state == .on,
      caseButton.state == .on,
      allFilesButton.state == .on
    )
    files = []
    collapsedFiles = []
    lastBudget = 0
    refreshGate.resetReloadTracking()
    outlineView.reloadData()
    refresh()
  }

  /// Routes a single click: a file row toggles expansion (recording the collapse
  /// preference); a line row opens the file at its line/column.
  @objc private func handleClick() {
    let row = outlineView.clickedRow
    guard row >= 0 else { return }
    let item = outlineView.item(atRow: row)
    if let file = item as? SearchFileItem {
      if outlineView.isItemExpanded(file) {
        outlineView.collapseItem(file)
        collapsedFiles.insert(file.absPath)
      } else {
        outlineView.expandItem(file)
        collapsedFiles.remove(file.absPath)
      }
    } else if let line = item as? SearchLineItem {
      handle.open_at(line.absPath, line.lineNumber, line.column)
      if let paneContainer {
        window?.makeFirstResponder(paneContainer)
      }
    }
  }

  // MARK: - NSOutlineViewDataSource

  func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
    if item == nil { return files.count }
    if let file = item as? SearchFileItem { return file.lines.count }
    return 0
  }

  func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
    if item == nil { return files[index] }
    // A file's children are its line rows; a line row has none.
    return (item as? SearchFileItem)?.lines[index] ?? files[index]
  }

  func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
    (item as? SearchFileItem) != nil
  }

  // MARK: - NSOutlineViewDelegate

  func outlineView(
    _ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any
  ) -> NSView? {
    let cell =
      outlineView.makeView(withIdentifier: Self.cellIdentifier, owner: self)
      as? NSTableCellView ?? makeCell()
    if let file = item as? SearchFileItem {
      cell.textField?.maximumNumberOfLines = 1
      cell.textField?.lineBreakMode = .byTruncatingMiddle
      cell.textField?.attributedStringValue = NSAttributedString(
        string: file.label, attributes: [.font: font, .foregroundColor: NSColor.labelColor])
    } else if let line = item as? SearchLineItem {
      cell.textField?.maximumNumberOfLines = 2
      cell.textField?.lineBreakMode = .byWordWrapping
      cell.textField?.attributedStringValue = attributedLine(line)
    }
    return cell
  }

  func outlineView(_ outlineView: NSOutlineView, heightOfRowByItem item: Any) -> CGFloat {
    // Line rows wrap to up to two rows; file rows are single-line.
    (item as? SearchLineItem) != nil ? ceil(font.pointSize * 2 * 1.35) : ceil(font.pointSize * 1.6)
  }

  // MARK: - Private

  /// Builds the styled attributed string for a line row: an ellipsis prefix/suffix
  /// per the truncation flags, the match portions bold, and the context dimmed.
  private func attributedLine(_ line: SearchLineItem) -> NSAttributedString {
    let text = line.displayText
    let result = NSMutableAttributedString()
    if line.truncatedLeft {
      result.append(
        NSAttributedString(
          string: Self.ellipsis,
          attributes: [.font: font, .foregroundColor: NSColor.tertiaryLabelColor]))
    }
    for span in SearchHighlightSeam.spans(text: text, boldByteRanges: line.boldByteRanges) {
      let start = text.utf16.index(text.utf16.startIndex, offsetBy: span.utf16Start)
      let end = text.utf16.index(start, offsetBy: span.utf16Length)
      let piece = String(text.utf16[start..<end]) ?? ""
      let attrs: [NSAttributedString.Key: Any] =
        span.bold
        ? [.font: boldFont, .foregroundColor: NSColor.labelColor]
        : [.font: font, .foregroundColor: NSColor.secondaryLabelColor]
      result.append(NSAttributedString(string: piece, attributes: attrs))
    }
    if line.truncatedRight {
      result.append(
        NSAttributedString(
          string: Self.ellipsis,
          attributes: [.font: font, .foregroundColor: NSColor.tertiaryLabelColor]))
    }
    return result
  }

  /// Rebuilds the `files` row model from the search snapshot, windowing each match
  /// line to `budget` characters.
  private func rebuildRows(from search: FfiSearch, budget: Int) {
    var rows: [SearchFileItem] = []
    for file in search.files(UInt32(budget)) {
      let absPath = file.abs_path().toString()
      var lines: [SearchLineItem] = []
      for match in file.matches() {
        var starts: [UInt32] = []
        for s in match.range_starts() { starts.append(s) }
        var ends: [UInt32] = []
        for e in match.range_ends() { ends.append(e) }
        let ranges: [(Int, Int)] = zip(starts, ends).map { (Int($0), Int($1)) }
        lines.append(
          SearchLineItem(
            absPath: absPath,
            lineNumber: match.line_number(),
            column: UInt64(match.column()),
            displayText: match.display_text().toString(),
            truncatedLeft: match.truncated_left(),
            truncatedRight: match.truncated_right(),
            boldByteRanges: ranges))
      }
      rows.append(
        SearchFileItem(absPath: absPath, label: file.label().toString(), lines: lines))
    }
    files = rows
  }

  /// Expands every file row except those the user explicitly collapsed (files are
  /// expanded by default).
  private func autoExpandFiles() {
    for file in files where !collapsedFiles.contains(file.absPath) {
      outlineView.expandItem(file)
    }
  }

  /// The current char budget from the outline's width and the row font advance.
  private func currentBudget() -> Int {
    let width = Double(outlineView.bounds.width)
    let advance = Double(("0" as NSString).size(withAttributes: [.font: font]).width)
    return SearchBudgetSeam.charBudget(sidebarWidth: width, charAdvance: advance)
  }

  /// Builds + lays out the header strip (field + toggles) above the outline.
  private func buildSubviews() {
    searchField.translatesAutoresizingMaskIntoConstraints = false
    searchField.placeholderString = "Search"
    searchField.target = self
    searchField.action = #selector(runSearch)
    // The default NSTextField fires its action on Return; we deliberately do NOT
    // wire `controlTextDidChange`, so it never searches as the user types.
    addSubview(searchField)

    configureToggle(caseButton, title: "Aa")
    configureToggle(regexButton, title: ".*")
    configureToggle(allFilesButton, title: "All")

    let toggles = NSStackView(views: [caseButton, regexButton, allFilesButton])
    toggles.translatesAutoresizingMaskIntoConstraints = false
    toggles.orientation = .horizontal
    toggles.spacing = 8
    addSubview(toggles)

    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("match"))
    column.resizingMask = .autoresizingMask
    outlineView.addTableColumn(column)
    outlineView.outlineTableColumn = column
    outlineView.headerView = nil
    outlineView.allowsMultipleSelection = false
    outlineView.dataSource = self
    outlineView.delegate = self
    outlineView.target = self
    outlineView.action = #selector(handleClick)
    outlineView.style = .sourceList
    outlineView.backgroundColor = .clear

    scrollView.translatesAutoresizingMaskIntoConstraints = false
    scrollView.drawsBackground = false
    scrollView.documentView = outlineView
    scrollView.hasVerticalScroller = true
    scrollView.autohidesScrollers = true
    addSubview(scrollView)

    NSLayoutConstraint.activate([
      searchField.topAnchor.constraint(equalTo: safeAreaLayoutGuide.topAnchor, constant: 8),
      searchField.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
      searchField.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),

      toggles.topAnchor.constraint(equalTo: searchField.bottomAnchor, constant: 6),
      toggles.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),

      scrollView.topAnchor.constraint(equalTo: toggles.bottomAnchor, constant: 6),
      scrollView.leadingAnchor.constraint(equalTo: leadingAnchor),
      scrollView.trailingAnchor.constraint(equalTo: trailingAnchor),
      scrollView.bottomAnchor.constraint(equalTo: bottomAnchor),
    ])
  }

  /// Configures a push-on/push-off toggle button with `title`.
  private func configureToggle(_ button: NSButton, title: String) {
    button.translatesAutoresizingMaskIntoConstraints = false
    button.title = title
    button.bezelStyle = .rounded
    button.setButtonType(.pushOnPushOff)
    button.font = font
  }

  /// Creates a reusable outline cell with a wrapping label.
  private func makeCell() -> NSTableCellView {
    let cell = NSTableCellView()
    cell.identifier = Self.cellIdentifier

    let label = NSTextField(labelWithString: "")
    label.translatesAutoresizingMaskIntoConstraints = false
    label.font = font
    label.drawsBackground = false
    label.cell?.wraps = true
    label.cell?.isScrollable = false
    cell.addSubview(label)
    cell.textField = label

    NSLayoutConstraint.activate([
      label.leadingAnchor.constraint(equalTo: cell.leadingAnchor),
      label.trailingAnchor.constraint(equalTo: cell.trailingAnchor),
      label.topAnchor.constraint(equalTo: cell.topAnchor),
      label.bottomAnchor.constraint(equalTo: cell.bottomAnchor),
    ])
    return cell
  }
}
