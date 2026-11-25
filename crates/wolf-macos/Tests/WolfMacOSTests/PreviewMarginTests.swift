// A preview's web view spans its pane, margins included, and its page carries the column cap.

import AppKit
import WebKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A preview's `WKWebView` over a real core: it spans its pane below the statusbar strip, so a
/// press or a wheel event over the pane's gutters and side margins reaches the page, and the
/// column cap travels as the page's root style rather than as the view's frame.
///
/// Premise: AppKit sends a scroll-wheel or mouse event to the view `hitTest(_:)` returns for its
/// location, so a point that hit-tests to the web view, or a view inside it, is a point whose
/// wheel scrolls the page. The page itself never loads where the gate runs (see
/// `PreviewViewFixture`), so nothing here reads what it draws or how far it scrolls.
///
/// | ID | Test | Kind |
/// | --- | --- | --- |
/// | M6 | `testALoneCappedPreviewSpansItsPane` | frame |
/// | M7 | `testTheGuttersAndMarginsOfALonePreviewHitTheWebView` | hit test |
/// | M8 | `testALoneCappedPreviewCarriesTheCappedRootStyle` | root style |
/// | M9 | `testAFullWidthPreviewSpansItsPaneUncapped` | frame and root style |
/// | M10 | `testASplitPreviewLeavesTheDividerBandToTheContainer` | hit test |
@MainActor
final class PreviewMarginTests: XCTestCase {

  // MARK: - Helpers

  /// The view a press at `point`, in `container`'s coordinates, is dispatched to: the hit test
  /// AppKit runs from the window's content view.
  private func hit(_ point: CGPoint, in container: PaneContainerView) -> NSView? {
    guard let parent = container.superview else {
      XCTFail("premise: the container must sit in a window's view tree")
      return nil
    }
    let inWindow = container.convert(point, to: nil)
    return container.hitTest(parent.convert(inWindow, from: nil))
  }

  /// The class of the view a hit test returned, or "nothing".
  private func describe(_ view: NSView?) -> String {
    view.map { String(describing: type(of: $0)) } ?? "nothing"
  }

  /// Whether `view` is `web` or a view inside it.
  private func isWeb(_ view: NSView?, _ web: WKWebView) -> Bool {
    guard let view else { return false }
    return view === web || view.isDescendant(of: web)
  }

  /// `r` as the seams' `Rect`.
  private func rect(_ r: CGRect) -> Rect {
    Rect(x: Double(r.minX), y: Double(r.minY), w: Double(r.width), h: Double(r.height))
  }

  /// The root style a lone preview of `fixture` carries: the pane flush on every side, so the
  /// placement has no insets.
  private func loneStyle(_ fixture: PreviewViewFixture, fullWidth: Bool) -> String {
    PreviewWidthSeam.rootStyle(
      maxCols: fullWidth ? nil : PaneContainerView.maxGridCols,
      cellWidth: Double(fixture.base.live.metrics.cellWidth), padding: EditorInset.padding,
      placement: PreviewWidthSeam.Placement(
        frame: rect(fixture.web.frame), sideInset: 0, bottomInset: 0))
  }

  /// A lone preview, with the premise that its pane is wider than the capped content and that
  /// the preview's source starts capped; `nil`, with the test failed, otherwise.
  private func lonePreview() -> PreviewViewFixture? {
    guard let fixture = PreviewViewFixture(split: false) else { return nil }
    let width = Double(fixture.container.bounds.width)
    let capped =
      Double(PaneContainerView.maxGridCols) * Double(fixture.base.live.metrics.cellWidth) + 2
      * EditorInset.padding
    guard capped < width else {
      XCTFail(
        "premise: the capped content and its padding, \(capped) pt, must be narrower than the"
          + " \(width)-pt pane, or the pane has no side margin to test")
      fixture.shutdown()
      return nil
    }
    guard !sourceFullWidth(fixture) else {
      XCTFail("premise: the previewed source must start capped, not full width")
      fixture.shutdown()
      return nil
    }
    return fixture
  }

  /// Whether the preview's source buffer is full width, from core's `previews()`.
  private func sourceFullWidth(_ fixture: PreviewViewFixture) -> Bool {
    var full = false
    for preview in fixture.handle.previews() where preview.win() == fixture.previewWin {
      full = preview.source_full_width()
    }
    return full
  }

  /// Asserts `web` spans the container's width and starts below the strip.
  private func assertSpans(
    _ web: WKWebView, _ container: PaneContainerView, _ what: String,
    file: StaticString = #filePath, line: UInt = #line
  ) {
    let frame = web.frame
    let bounds = container.bounds
    XCTAssertEqual(
      frame.minX, bounds.minX, accuracy: 0.001,
      "\(what): the web view \(frame) must start at the pane's left edge \(bounds.minX). A start"
        + " at the padding or past it means the view is framed to the grid rect, so the margins"
        + " are dead to the wheel", file: file, line: line)
    XCTAssertEqual(
      frame.maxX, bounds.maxX, accuracy: 0.001,
      "\(what): the web view \(frame) must reach the pane's right edge \(bounds.maxX), where its"
        + " scrollbar then sits", file: file, line: line)
    XCTAssertGreaterThan(
      frame.minY, bounds.minY,
      "\(what): the web view \(frame) must start below the statusbar strip, which stays chrome",
      file: file, line: line)
  }

  // MARK: - M6: a lone capped preview spans its pane

  /// M6.
  func testALoneCappedPreviewSpansItsPane() {
    guard let fixture = lonePreview() else { return }
    defer { fixture.shutdown() }
    fixture.container.onRenderWake()
    assertSpans(fixture.web, fixture.container, "a lone capped preview")
  }

  // MARK: - M7: the gutters and the side margin hit the web view

  /// M7.
  func testTheGuttersAndMarginsOfALonePreviewHitTheWebView() {
    guard let fixture = lonePreview() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    container.onRenderWake()
    let bounds = container.bounds
    let padding = CGFloat(EditorInset.padding)
    let inner = Rect(
      x: Double(bounds.minX + padding), y: 0, w: Double(bounds.width - 2 * padding), h: 1)
    let capped = PaneGeometry(
      paneRect: inner, cellWidth: Double(fixture.base.live.metrics.cellWidth),
      cellHeight: Double(fixture.base.live.metrics.cellHeight),
      maxCols: PaneContainerView.maxGridCols
    ).gridRect
    XCTAssertGreaterThan(
      capped.x, inner.x, "premise: the capped content must leave a side margin: \(capped)")
    let y = bounds.midY
    let points: [(String, CGPoint)] = [
      ("the left padding gutter", CGPoint(x: bounds.minX + padding / 2, y: y)),
      ("the left side margin", CGPoint(x: (inner.x + capped.x) / 2, y: y)),
      ("the right padding gutter", CGPoint(x: bounds.maxX - padding / 2, y: y)),
    ]
    for (name, point) in points {
      let view = hit(point, in: container)
      XCTAssertTrue(
        isWeb(view, fixture.web),
        "\(name), at \(point), must hit-test to the web view \(fixture.web.frame), so its wheel"
          + " and press reach the page, but it hit \(describe(view))."
          + " Only the side margin reaching it means the view is framed to the padded pane; none"
          + " means it is framed to the capped grid rect")
    }
  }

  // MARK: - M8: the page carries the cap

  /// M8.
  func testALoneCappedPreviewCarriesTheCappedRootStyle() {
    guard let fixture = lonePreview() else { return }
    defer { fixture.shutdown() }
    fixture.container.onRenderWake()
    let expected = loneStyle(fixture, fullWidth: false)
    XCTAssertTrue(
      expected.contains("max-width"), "premise: the capped style carries a max-width: \(expected)")
    XCTAssertEqual(
      fixture.container.previewRootStyle(forWin: fixture.previewWin), expected,
      "the lone capped preview must record the capped root style for its page. nil means no"
        + " style was ever sent, and the full-width style means the cap was dropped, so the"
        + " content spans the whole pane")
  }

  // MARK: - M9: a full-width preview spans its pane, uncapped

  /// M9.
  func testAFullWidthPreviewSpansItsPaneUncapped() {
    guard let fixture = lonePreview() else { return }
    defer { fixture.shutdown() }
    fixture.handle.toggle_full_width()
    guard fixture.base.wake(until: { sourceFullWidth(fixture) }) else {
      XCTFail("premise: toggle_full_width() must make the preview's source full width")
      return
    }
    // The wake that saw the flag may have synced before it landed.
    fixture.container.onRenderWake()
    assertSpans(fixture.web, fixture.container, "a full-width preview")
    let expected = loneStyle(fixture, fullWidth: true)
    XCTAssertFalse(
      expected.contains("max-width"), "premise: the full-width style caps nothing: \(expected)")
    XCTAssertEqual(
      fixture.container.previewRootStyle(forWin: fixture.previewWin), expected,
      "after the toggle the page must carry the full-width root style. The capped one means the"
        + " style ignores the source's flag, and the toggle no longer reaches the page")
  }

  // MARK: - M10: the divider band stays the container's

  /// M10.
  func testASplitPreviewLeavesTheDividerBandToTheContainer() {
    guard let fixture = PreviewViewFixture() else { return }
    defer { fixture.shutdown() }
    let container = fixture.container
    container.onRenderWake()
    guard let pane = container.visiblePaneFrames[fixture.previewWin] else {
      XCTFail("premise: the preview's pane must be visible: \(container.visiblePaneFrames)")
      return
    }
    let bounds = container.bounds
    let rightIsInterior = pane.maxX < bounds.maxX - 0.5
    let leftIsInterior = pane.minX > bounds.minX + 0.5
    guard rightIsInterior != leftIsInterior else {
      XCTFail(
        "premise: the preview's pane \(pane) must border the divider on one side of \(bounds)")
      return
    }
    let padding = CGFloat(EditorInset.padding)
    let y = pane.midY
    let besideDivider = CGPoint(x: leftIsInterior ? pane.minX + 2 : pane.maxX - 2, y: y)
    let outerGutter = CGPoint(
      x: leftIsInterior ? pane.maxX - padding / 2 : pane.minX + padding / 2, y: y)
    let atDivider = hit(besideDivider, in: container)
    XCTAssertTrue(
      atDivider === container,
      "\(besideDivider), 2 pt inside the preview's pane \(pane) from the divider, must hit the"
        + " container, which drags the divider, but it hit \(describe(atDivider)). The web view"
        + " \(fixture.web.frame) there means it covers the divider's band and swallows the drag")
    let atGutter = hit(outerGutter, in: container)
    XCTAssertTrue(
      isWeb(atGutter, fixture.web),
      "the preview's outer padding gutter, at \(outerGutter), must hit the web view"
        + " \(fixture.web.frame), but it hit \(describe(atGutter)): the view is framed short of"
        + " its pane's outer edge")
  }
}
