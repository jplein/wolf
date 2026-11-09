// Opens, paints, hides and reopens the terminal drawer over a real core and its shell session.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A packed `0x00RRGGBB` colour as an sRGB `NSColor`.
private func srgb(_ packed: UInt32) -> NSColor {
  NSColor(
    srgbRed: CGFloat((packed >> 16) & 0xFF) / 255, green: CGFloat((packed >> 8) & 0xFF) / 255,
    blue: CGFloat(packed & 0xFF) / 255, alpha: 1)
}

@MainActor
final class TerminalDrawerTests: XCTestCase {

  /// The background the drawer's shell prints, far from any default terminal colour.
  private let crimson = srgb(0x00DC_143C)
  /// Eight crimson-background spaces, then a newline. Typed, the line holds no crimson.
  private let crimsonLine = "printf '\\033[48;2;220;20;60m        \\033[0m\\n'"

  /// `toggle_terminal()` shows the drawer as a terminal view in the bottom strip on the
  /// drawer window's session, paints what the shell prints, hides with the session kept,
  /// and shows the same session again on the next toggle.
  func testTheDrawerOpensOnASessionPaintsHidesAndReopens() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle

    handle.toggle_terminal()
    var session: UInt64 = 0
    let opened = fixture.wake {
      session = Array(handle.live_terminals()).first ?? 0
      guard session != 0, let view = fixture.view(showing: session) else { return false }
      return fixture.isInDrawerStrip(view) && fixture.window(showing: session) != 0
    }
    XCTAssertTrue(
      opened,
      "after toggle_terminal() a terminal view must show the drawer's session in the bottom"
        + " strip; live sessions \(Array(handle.live_terminals())), terminal views at"
        + " \(fixture.terminalViews.map { $0.frame }). No session means toggle_terminal starts"
        + " none; a session with no view means applyDrawerOpen bound no drawer pane; a view in"
        + " the upper half means the drawer was put into the pane tree")
    guard opened else { return }
    XCTAssertEqual(
      handle.win_buffer(fixture.window(showing: session)).terminal(), session,
      "the drawer view's session must be the one the drawer window's buffer carries")
    XCTAssertEqual(
      fixture.container.paneCount, 1,
      "the drawer lives outside the pane tree, so the tree must still hold one leaf")

    guard let before = fixture.view(showing: session) else { return }
    let precondition = fixture.render()
    XCTAssertEqual(
      precondition.colors(in: before.bounds, of: before).filter {
        colorDistance($0, crimson) <= 0.05
      }.count, 0,
      "premise: nothing in the drawer is crimson before the line runs")

    fixture.runLine(crimsonLine, in: session)
    let painted = fixture.wake {
      guard let view = fixture.view(showing: session) else { return false }
      let capture = fixture.renderer.render()
      let crimsonPixels = capture.colors(in: view.bounds, of: view).filter {
        colorDistance($0, crimson) <= 0.05
      }.count
      let cellArea = Double(fixture.live.metrics.cellWidth * fixture.live.metrics.cellHeight)
      return Double(crimsonPixels) >= 4 * cellArea
    }
    XCTAssertTrue(
      painted,
      "the drawer view must paint the crimson background its shell printed under eight cells"
        + " (at least four cells' area); the frame reads:\n\(fixture.text(of: session))")

    handle.toggle_terminal()
    let hidden = fixture.wake { fixture.view(showing: session) == nil }
    XCTAssertTrue(
      hidden, "a second toggle must hide the drawer: its terminal view must go")
    XCTAssertTrue(
      Array(handle.live_terminals()).contains(session),
      "hiding the drawer must keep its session live; gone means its placeholder was wiped")

    handle.toggle_terminal()
    let reopened = fixture.wake {
      guard let view = fixture.view(showing: session) else { return false }
      return fixture.isInDrawerStrip(view)
    }
    XCTAssertTrue(
      reopened,
      "a third toggle must show the SAME session \(session) in the drawer strip again; live"
        + " sessions \(Array(handle.live_terminals())). A stale drawer pane id refuses the"
        + " reopen; another session means the drawer started a new shell")
  }
}
