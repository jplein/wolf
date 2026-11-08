// Drives the sidebar outline's production cells through clicks and checks what each row draws.

import AppKit
import XCTest

@testable import WolfMacOS

@MainActor
final class FileTreeViewTests: XCTestCase {

  /// A row that was current at a click, and is no longer current, draws its
  /// label like a row that was never selected or current, not in the accent.
  func testPreviouslyCurrentRowsDrawLikeAnUntouchedRow() {
    let sidebar = SidebarFixture()
    sidebar.showCurrent(0)
    _ = sidebar.click(1)
    sidebar.showCurrent(1)
    _ = sidebar.click(2)
    sidebar.showCurrent(2)
    let capture = sidebar.renderer.render()
    let accent = sidebar.accent()

    // Row 2 is current and selected, and its selection fill covers its text: rows 0, 1 and 3 only.
    let untouched = sidebar.ink(ofRow: 3, in: capture)
    for row in [0, 1] {
      if let untouched, let ink = sidebar.ink(ofRow: row, in: capture) {
        let fromUntouched = colorDistance(ink, untouched)
        let fromAccent = colorDistance(ink, accent)
        XCTAssertLessThan(
          fromUntouched, 0.1,
          "row \(row), current before a click and no longer, draws \(ink), \(fromUntouched) from"
            + " the untouched row 3's \(untouched): its label keeps a colour from an earlier state")
        XCTAssertGreaterThan(
          fromAccent, 0.5,
          "row \(row), current before a click and no longer, draws \(ink), \(fromAccent) from"
            + " the accent \(accent): it still draws as the current file (the reported bug)")
      }

      guard let cell = sidebar.cell(atRow: row), let label = cell.textField else {
        XCTFail("row \(row) has no TreeCellView with a label: the outline built no production cell")
        continue
      }
      XCTAssertFalse(
        cell.isCurrent,
        "row \(row) is still marked current: restyleRows(in:isCurrent:) did not move the mark")
      XCTAssertEqual(
        cell.backgroundStyle, .normal,
        "row \(row) is unselected but its background style is not .normal: an emphasis or"
          + " selection style outlived the click, which would pick the selected label colour")
      // textColor reads labelColor even while the row draws in the accent; the drawn string's
      // foreground is what goes stale, so it is the property checked.
      let string = label.attributedStringValue
      let drawn =
        string.length > 0
        ? string.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor : nil
      XCTAssertEqual(
        drawn, NSColor.labelColor,
        "row \(row)'s drawn string has foreground \(String(describing: drawn)), not labelColor"
          + " (textColor reads \(String(describing: label.textColor))): AppKit kept the colour"
          + " from the emphasized click, so the label string was not rebuilt after the change")
    }
  }

  /// The current file's row draws in the accent while another row is selected:
  /// the positive control, which proves the sampler sees the accent here.
  func testCurrentRowIsAccentedUntilTheWakeMovesIt() {
    let sidebar = SidebarFixture()
    sidebar.showCurrent(0)
    // The capture taken at the click, before the render wake moves the mark to row 1.
    let capture = sidebar.click(1)
    let accent = sidebar.accent()

    guard let current = sidebar.ink(ofRow: 0, in: capture),
      let untouched = sidebar.ink(ofRow: 3, in: capture)
    else { return }
    let fromAccent = colorDistance(current, accent)
    XCTAssertLessThan(
      fromAccent, 0.25,
      "the current row 0 draws \(current), \(fromAccent) from the accent \(accent): either the"
        + " current file is not accented, or the capture does not see the colour drawn")
    let untouchedFromAccent = colorDistance(untouched, accent)
    XCTAssertGreaterThan(
      untouchedFromAccent, 0.5,
      "the untouched row 3 draws \(untouched), \(untouchedFromAccent) from the accent: every row"
        + " is accented, or the sampler reads the accent wherever it looks")
  }
}

/// The sidebar's outline as `FileTreeView` builds it, with production cells and
/// configuration, over four top-level files; only the data is the test's.
@MainActor
private final class SidebarFixture {
  /// Kept here because the outline's `dataSource` and `delegate` are weak.
  let source = SidebarSource()
  let outline = NSOutlineView()
  let renderer: OffscreenRenderer

  init() {
    let size = NSSize(width: 240, height: 200)
    FileTreeView.configureOutline(outline)
    outline.dataSource = source
    outline.delegate = source
    let scrollView = NSScrollView(frame: NSRect(origin: .zero, size: size))
    scrollView.drawsBackground = false
    scrollView.documentView = outline
    renderer = OffscreenRenderer(hosting: scrollView, size: size)
    outline.reloadData()
    // The first display creates the row views that selection and emphasis act on.
    _ = renderer.render()
  }

  /// What the editor's render wake does when `row`'s file becomes current: mark
  /// it, reveal it by selecting it, and restyle the rows AppKit already has.
  func showCurrent(_ row: Int) {
    source.current = source.items[row]
    outline.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
    FileTreeView.restyleRows(in: outline) { [source] item in
      item.absPath == source.current?.absPath
    }
  }

  /// A click on `row`: mouse-down makes the outline first responder, so its
  /// selection is emphasized, and the click's display is captured; then
  /// `activate(item:)` hands first responder to the pane. Returns the capture.
  func click(_ row: Int) -> RenderedView {
    outline.setRowsEmphasized(true)
    outline.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
    let capture = renderer.render()
    outline.setRowsEmphasized(false)
    return capture
  }

  /// The accent colour as the window's appearance resolves it, in sRGB.
  func accent() -> NSColor {
    var resolved: NSColor?
    renderer.window.effectiveAppearance.performAsCurrentDrawingAppearance {
      resolved = NSColor.controlAccentColor.usingColorSpace(.sRGB)
    }
    guard let resolved else {
      preconditionFailure("controlAccentColor has no sRGB form in the window's appearance")
    }
    return resolved
  }

  /// The ink of `row` in `capture`, failing the test when the row drew fewer
  /// than 50 pixels, which no label of these names does.
  func ink(
    ofRow row: Int, in capture: RenderedView, file: StaticString = #filePath, line: UInt = #line
  ) -> NSColor? {
    let ink = capture.ink(in: outline.rect(ofRow: row), of: outline)
    let pixels = ink?.pixels ?? 0
    guard let ink, pixels >= 50 else {
      XCTFail(
        "row \(row) drew \(pixels) pixels at alpha 0.25 or more, fewer than 50: its label drew"
          + " no text, or the capture missed the display pass",
        file: file, line: line)
      return nil
    }
    return ink.color
  }

  /// The production cell AppKit holds for `row`.
  func cell(atRow row: Int) -> TreeCellView? {
    outline.view(atColumn: 0, row: row, makeIfNecessary: false) as? TreeCellView
  }
}

/// The outline's data: four top-level files, and which of them is current.
/// `viewFor` mirrors `FileTreeView`'s.
@MainActor
private final class SidebarSource: NSObject, NSOutlineViewDataSource, NSOutlineViewDelegate {
  let items = ["alpha.md", "bravo.md", "charlie.md", "delta.md"].map {
    TreeItem(isDir: false, name: $0, relPath: $0, absPath: "/w/\($0)")
  }
  var current: TreeItem?

  func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
    item == nil ? items.count : 0
  }

  func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
    items[index]
  }

  func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
    false
  }

  func outlineView(
    _ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any
  ) -> NSView? {
    let cell =
      outlineView.makeView(withIdentifier: TreeCellView.reuseIdentifier, owner: self)
      as? TreeCellView ?? TreeCellView.make()
    guard let treeItem = item as? TreeItem else { return cell }
    cell.textField?.stringValue = treeItem.name
    cell.isCurrent = treeItem.absPath == current?.absPath
    return cell
  }
}
