// The preview web view's placement in its pane, and the root style that caps its page's content.

import Foundation
import XCTest

@testable import WolfKeyMap

/// `PreviewWidthSeam`: where a preview's web view sits in its pane, and the inline style on the
/// page's root element that caps and centres the content inside it.
///
/// The content box is laid out from the style by `contentBox`, a model of CSS block layout for
/// the root element of a `border-box` page: a used width of the viewport's, capped by any
/// `max-width`, centred by two `auto` side margins and then inset by the padding. Every expected
/// rect is the editor's own, from `EditorInset` and `PaneGeometry`, so the tests hold the page's
/// content on the grid rect an editor pane of that size paints.
final class PreviewWidthSeamTests: XCTestCase {
  /// The tolerance for comparing lengths, which the style carries as printed decimals.
  private let eps = 1e-6
  /// A cell width that is no whole number of points, so a rounded cap does not pass.
  private let cellW = 8.4
  /// A cell height; only the rows below depend on it.
  private let cellH = 17.0
  /// The column cap the tests pass, as `PaneContainerView` does.
  private let maxCols = 90
  /// The per-pane padding.
  private let padding = EditorInset.padding
  /// The statusbar strip's height.
  private let strip = 30.0
  /// The divider band.
  private let band = 4.0

  // MARK: - Helpers

  /// The style's declarations, by name, failing the test on a malformed one.
  private func declarations(
    _ style: String, file: StaticString = #filePath, line: UInt = #line
  ) -> [String: String] {
    var out: [String: String] = [:]
    for part in style.split(separator: ";", omittingEmptySubsequences: false) {
      let pair = part.split(separator: ":", maxSplits: 1).map(String.init)
      guard pair.count == 2, !pair[0].isEmpty, !pair[1].isEmpty else {
        XCTFail(
          "\(style.debugDescription) holds \(String(part).debugDescription), which is no"
            + " name:value declaration: the format is name:value joined by ;, with no spaces and"
            + " no trailing ;", file: file, line: line)
        continue
      }
      XCTAssertNil(
        out[pair[0]], "\(style.debugDescription) declares \(pair[0]) twice", file: file,
        line: line)
      out[pair[0]] = pair[1]
    }
    return out
  }

  /// A `px` length, failing the test when `value` is none.
  private func px(
    _ value: String?, _ name: String, file: StaticString = #filePath, line: UInt = #line
  ) -> Double {
    guard let value, value.hasSuffix("px"), let number = Double(value.dropLast(2)) else {
      XCTFail(
        "\(name) is \(value.debugDescription), not a decimal followed by px", file: file,
        line: line)
      return .nan
    }
    return number
  }

  /// The rect the page's content box lays out at, in `frame`'s coordinates, under `style`.
  private func contentBox(
    _ style: String, in frame: Rect, file: StaticString = #filePath, line: UInt = #line
  ) -> Rect {
    let decls = declarations(style, file: file, line: line)
    XCTAssertEqual(
      decls["box-sizing"], "border-box",
      "\(style.debugDescription): the root's box must be border-box, or a max-width caps the"
        + " content alone and the padding widens it past the grid", file: file, line: line)
    let top = px(decls["padding-top"] ?? decls["padding"], "padding-top", file: file, line: line)
    let right = px(
      decls["padding-right"] ?? decls["padding"], "padding-right", file: file, line: line)
    let bottom = px(
      decls["padding-bottom"] ?? decls["padding"], "padding-bottom", file: file, line: line)
    let left = px(
      decls["padding-left"] ?? decls["padding"], "padding-left", file: file, line: line)
    var width = frame.w
    if let fixed = decls["width"] { width = px(fixed, "width", file: file, line: line) }
    if let cap = decls["max-width"] {
      width = min(width, px(cap, "max-width", file: file, line: line))
    }
    let marginLeft: Double
    if decls["margin-left"] == "auto" && decls["margin-right"] == "auto" {
      marginLeft = max(0, (frame.w - width) / 2)
    } else if let value = decls["margin-left"], value != "auto" {
      marginLeft = px(value, "margin-left", file: file, line: line)
    } else {
      marginLeft = 0
    }
    return Rect(
      x: frame.x + marginLeft + left, y: frame.y + top, w: width - left - right,
      h: frame.h - top - bottom)
  }

  /// The editor's grid rect in `pane`, in the pane's coordinates' parent: `EditorInset`'s
  /// pane-local rect, capped at `maxCols` when given.
  private func editorGrid(pane: Rect, maxCols: Int?, cellWidth: Double? = nil) -> Rect {
    let inset = EditorInset(
      paneWidth: pane.w, paneHeight: pane.h, statusbarHeight: strip, padding: padding)
    let local = inset.gridRect
    return PaneGeometry(
      paneRect: Rect(x: pane.x + local.x, y: pane.y + local.y, w: local.w, h: local.h),
      cellWidth: cellWidth ?? cellW, cellHeight: cellH, maxCols: maxCols
    ).gridRect
  }

  /// Asserts `got` is `want` within `eps`.
  private func assertRect(
    _ got: Rect, _ want: Rect, _ message: String, file: StaticString = #filePath,
    line: UInt = #line
  ) {
    let close =
      abs(got.x - want.x) < eps && abs(got.y - want.y) < eps && abs(got.w - want.w) < eps
      && abs(got.h - want.h) < eps
    XCTAssertTrue(
      close, "\(message): the content box is \(got), the editor's grid \(want)",
      file: file, line: line)
  }

  /// A pane, its area, and what the case is.
  private struct Case {
    let name: String
    let pane: Rect
    let area: Rect
  }

  /// A lone pane and the right-hand pane of a vertical split, each wider than the cap, and the
  /// same two narrower than it.
  private var cases: [Case] {
    [
      Case(
        name: "a lone pane wider than the cap", pane: Rect(x: 0, y: 0, w: 1200, h: 800),
        area: Rect(x: 0, y: 0, w: 1200, h: 800)),
      Case(
        name: "the right pane of a vertical split, wider than the cap",
        pane: Rect(x: 1000, y: 0, w: 1000, h: 800), area: Rect(x: 0, y: 0, w: 2000, h: 800)),
      Case(
        name: "a lone pane narrower than the cap", pane: Rect(x: 0, y: 0, w: 600, h: 800),
        area: Rect(x: 0, y: 0, w: 600, h: 800)),
      Case(
        name: "the right pane of a vertical split, narrower than the cap",
        pane: Rect(x: 600, y: 0, w: 600, h: 800), area: Rect(x: 0, y: 0, w: 1200, h: 800)),
    ]
  }

  // MARK: - M1: the capped content lands on the editor's capped grid rect

  /// M1.
  func testCappedContentBoxIsTheEditorsCappedGridRect() {
    XCTAssertGreaterThan(
      1200 - 2 * padding, Double(maxCols) * cellW,
      "premise: the wide panes must be wider than the cap, or no case is capped")
    XCTAssertLessThan(
      600 - 2 * padding, Double(maxCols) * cellW,
      "premise: the narrow panes must be narrower than the cap")
    for c in cases {
      let placement = PreviewWidthSeam.placement(
        pane: c.pane, area: c.area, statusbarHeight: strip, dividerBand: band)
      let style = PreviewWidthSeam.rootStyle(
        maxCols: maxCols, cellWidth: cellW, padding: padding, placement: placement)
      assertRect(
        contentBox(style, in: placement.frame), editorGrid(pane: c.pane, maxCols: maxCols),
        "\(c.name), style \(style.debugDescription) in the frame \(placement.frame): a wider"
          + " box means the max-width leaves out the padding, a box past the frame's width"
          + " means a fixed width, and one shifted by the divider band means the side padding"
          + " ignores it")
    }
  }

  // MARK: - M2: the full-width style caps nothing

  /// M2.
  func testFullWidthStylePadsAndCapsNothing() {
    for c in cases {
      let placement = PreviewWidthSeam.placement(
        pane: c.pane, area: c.area, statusbarHeight: strip, dividerBand: band)
      let style = PreviewWidthSeam.rootStyle(
        maxCols: nil, cellWidth: cellW, padding: padding, placement: placement)
      let decls = declarations(style)
      XCTAssertNil(
        decls["max-width"],
        "\(c.name): a full-width style must carry no max-width, or a full-width buffer stays"
          + " capped: \(style.debugDescription)")
      assertRect(
        contentBox(style, in: placement.frame), editorGrid(pane: c.pane, maxCols: nil),
        "\(c.name), full width, style \(style.debugDescription) in \(placement.frame): the"
          + " content must fill the pane less its padding")
    }
  }

  /// M11, with no cell width: the cap is skipped, as `PaneGeometry` skips it.
  func testZeroCellWidthCapsNothing() {
    let pane = Rect(x: 0, y: 0, w: 1200, h: 800)
    let placement = PreviewWidthSeam.placement(
      pane: pane, area: pane, statusbarHeight: strip, dividerBand: band)
    let style = PreviewWidthSeam.rootStyle(
      maxCols: maxCols, cellWidth: 0, padding: padding, placement: placement)
    XCTAssertNil(
      declarations(style)["max-width"],
      "a zero cell width must cap nothing, as PaneGeometry caps nothing then; a max-width of the"
        + " padding alone would squeeze the page to nothing: \(style.debugDescription)")
    assertRect(
      contentBox(style, in: placement.frame),
      editorGrid(pane: pane, maxCols: maxCols, cellWidth: 0),
      "a zero cell width, style \(style.debugDescription)")
  }

  // MARK: - M3: the style rides the <html> tag, and the head is untouched

  /// A document shaped like `render_html`'s wrapper.
  private let document =
    "<!doctype html><html><head><meta charset=\"utf-8\"><!--\nnotices\n-->"
    + "<style>body{margin:0}</style></head><body class=\"wolf-markdown\"><h1 id=\"a\">Probe</h1>"
    + "<p>One paragraph.</p></body></html>"

  /// M3.
  func testStyledDocumentPutsTheStyleOnTheRootTagAlone() throws {
    let pane = Rect(x: 0, y: 0, w: 1200, h: 800)
    let placement = PreviewWidthSeam.placement(
      pane: pane, area: pane, statusbarHeight: strip, dividerBand: band)
    let style = PreviewWidthSeam.rootStyle(
      maxCols: maxCols, cellWidth: cellW, padding: padding, placement: placement)
    XCTAssertFalse(style.isEmpty, "premise: the capped style is not empty")
    let styled = PreviewWidthSeam.styledDocument(document, rootStyle: style)
    let expected = document.replacingOccurrences(
      of: "<html>", with: "<html style=\"\(style)\">")
    XCTAssertEqual(
      styled, expected,
      "the style must be inserted directly after <html and nowhere else. The raw document back"
        + " means it was never inserted, and a style on <body> means base.css's body box is"
        + " overridden")
    let headStart = try XCTUnwrap(document.range(of: "<head>"))
    let styledHeadStart = try XCTUnwrap(styled.range(of: "<head>"))
    XCTAssertEqual(
      String(styled[styledHeadStart.lowerBound...]), String(document[headStart.lowerBound...]),
      "everything from <head> on must be byte-identical to the raw document, or the body swap"
        + " and the head comparison read a different document")
    XCTAssertEqual(
      PreviewScrollSeam.splitDocument(styled)?.body,
      PreviewScrollSeam.splitDocument(document)?.body,
      "the styled document's body must split out as the raw one's")
  }

  /// M12, the two cases that leave the document alone.
  func testStyledDocumentLeavesTheDocumentWhenItCannotStyleIt() {
    XCTAssertEqual(
      PreviewWidthSeam.styledDocument(document, rootStyle: ""), document,
      "an empty style must leave the document as it is, not add an empty attribute")
    let headless = "<head><style>x</style></head><html><body>b</body></html>"
    XCTAssertEqual(
      PreviewWidthSeam.styledDocument(headless, rootStyle: "padding:1px"), headless,
      "with no <html before <head> the document must be left as it is")
  }

  /// M13, an attribute value cannot end early.
  func testStyledDocumentEscapesTheAttribute() throws {
    let style = "a\"b&amp;c"
    let styled = PreviewWidthSeam.styledDocument(document, rootStyle: style)
    let open = try XCTUnwrap(
      styled.range(of: "<html style=\""), "the style must open on the <html tag: \(styled)")
    let rest = styled[open.upperBound...]
    let close = try XCTUnwrap(rest.firstIndex(of: "\""))
    let value = String(rest[..<close])
    let decoded = value.replacingOccurrences(of: "&quot;", with: "\"")
      .replacingOccurrences(of: "&#34;", with: "\"").replacingOccurrences(of: "&amp;", with: "&")
      .replacingOccurrences(of: "&#38;", with: "&")
    XCTAssertEqual(
      decoded, style,
      "the attribute value \(value.debugDescription) must escape \" and & and decode to the"
        + " style; a raw \" ends the attribute early, and a raw & lets an entity in the style"
        + " decode to another value, here &amp; to &")
    XCTAssertTrue(
      String(rest[close...]).hasPrefix("\"><head>"),
      "the attribute must close right before the tag ends: \(rest.prefix(80))")
  }

  // MARK: - M4: the live script sets the root's style

  /// The string literal `js` passes `setAttribute("style", …)` on the root element, decoded.
  private func rootStyleArgument(_ js: String, file: StaticString = #filePath, line: UInt = #line)
    -> String?
  {
    let call = "document.documentElement.setAttribute("
    guard let start = js.range(of: call) else {
      XCTFail("\(js.debugDescription) must call \(call)", file: file, line: line)
      return nil
    }
    var rest = js[start.upperBound...].drop(while: { $0 == " " })
    guard rest.hasPrefix("\"style\"") else {
      XCTFail("\(js.debugDescription) must set the \"style\" attribute", file: file, line: line)
      return nil
    }
    rest = rest.dropFirst("\"style\"".count).drop(while: { $0 == " " })
    guard rest.first == "," else {
      XCTFail("\(js.debugDescription): no second argument", file: file, line: line)
      return nil
    }
    rest = rest.dropFirst().drop(while: { $0 == " " })
    guard rest.first == "\"" else {
      XCTFail("\(js.debugDescription): the value is no string literal", file: file, line: line)
      return nil
    }
    var end = rest.index(after: rest.startIndex)
    var escaped = false
    while end < rest.endIndex {
      let char = rest[end]
      if escaped {
        escaped = false
      } else if char == "\\" {
        escaped = true
      } else if char == "\"" {
        break
      }
      end = rest.index(after: end)
    }
    guard end < rest.endIndex else {
      XCTFail("\(js.debugDescription): the literal never closes", file: file, line: line)
      return nil
    }
    let literal = String(rest[rest.startIndex...end])
    guard
      let decoded = try? JSONSerialization.jsonObject(
        with: Data(literal.utf8), options: .fragmentsAllowed) as? String
    else {
      XCTFail("\(literal.debugDescription) is no JSON string literal", file: file, line: line)
      return nil
    }
    return decoded
  }

  /// M4.
  func testApplyRootStyleJSSetsTheRootsStyleToExactlyTheStyle() {
    let pane = Rect(x: 0, y: 0, w: 1200, h: 800)
    let placement = PreviewWidthSeam.placement(
      pane: pane, area: pane, statusbarHeight: strip, dividerBand: band)
    let real = PreviewWidthSeam.rootStyle(
      maxCols: maxCols, cellWidth: cellW, padding: padding, placement: placement)
    for style in [real, "a\"b\\c</script>\n"] {
      let js = PreviewWidthSeam.applyRootStyleJS(style)
      XCTAssertFalse(
        js.contains("document.body"),
        "the script must style the root element, never the body, whose box base.css owns: \(js)")
      XCTAssertEqual(
        rootStyleArgument(js), style,
        "the literal \(js.debugDescription) must decode to exactly the style; anything else is"
          + " raw interpolation or a different value")
    }
  }

  // MARK: - M5: placement

  /// M5.
  func testPlacementInsetsOnlyTheEdgesThatBorderADivider() {
    let area = Rect(x: 0, y: 0, w: 1200, h: 800)
    XCTAssertEqual(
      PreviewWidthSeam.placement(pane: area, area: area, statusbarHeight: strip, dividerBand: band),
      PreviewWidthSeam.Placement(
        frame: Rect(x: 0, y: strip, w: 1200, h: 800 - strip), sideInset: 0, bottomInset: 0),
      "a lone pane borders no divider, so its web view is flush on every side below the strip")
    XCTAssertEqual(
      PreviewWidthSeam.placement(
        pane: Rect(x: 0, y: 0, w: 600, h: 800), area: area, statusbarHeight: strip,
        dividerBand: band),
      PreviewWidthSeam.Placement(
        frame: Rect(x: band, y: strip, w: 600 - 2 * band, h: 800 - strip), sideInset: band,
        bottomInset: 0),
      "the left pane of a vertical split borders the divider on its right, and the side inset"
        + " applies to both sides together, so the page's centring is the editor's")
    XCTAssertEqual(
      PreviewWidthSeam.placement(
        pane: Rect(x: 600, y: 0, w: 600, h: 800), area: area, statusbarHeight: strip,
        dividerBand: band),
      PreviewWidthSeam.Placement(
        frame: Rect(x: 600 + band, y: strip, w: 600 - 2 * band, h: 800 - strip),
        sideInset: band, bottomInset: 0),
      "the right pane of a vertical split is inset on both sides as well")
    XCTAssertEqual(
      PreviewWidthSeam.placement(
        pane: Rect(x: 0, y: 0, w: 1200, h: 400), area: area, statusbarHeight: strip,
        dividerBand: band),
      PreviewWidthSeam.Placement(
        frame: Rect(x: 0, y: strip, w: 1200, h: 400 - strip - band), sideInset: 0,
        bottomInset: band),
      "the upper pane of a horizontal split borders the divider at its bottom alone")
    XCTAssertEqual(
      PreviewWidthSeam.placement(
        pane: Rect(x: 0, y: 400, w: 1200, h: 400), area: area, statusbarHeight: strip,
        dividerBand: band),
      PreviewWidthSeam.Placement(
        frame: Rect(x: 0, y: 400 + strip, w: 1200, h: 400 - strip), sideInset: 0,
        bottomInset: 0),
      "the lower pane's divider is under its strip, so the top is never inset")
  }
}
