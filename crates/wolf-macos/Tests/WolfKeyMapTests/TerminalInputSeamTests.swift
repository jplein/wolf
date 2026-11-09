// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

// MARK: - macOS virtual key codes (kVK_*) used below.
private enum VK {
  static let ansiS: UInt16 = 0x01  // kVK_ANSI_S — a plain printable letter
  static let returnKey: UInt16 = 0x24  // kVK_Return — a NAMED key
  static let leftArrow: UInt16 = 0x7B  // kVK_LeftArrow — a NAMED key
}

final class TerminalInputSeamTests: XCTestCase {

  private typealias Route = TerminalInputSeam.TextRoute
  private typealias Report = TerminalInputSeam.FocusReport

  /// `routesThroughInputContext` with every modifier off unless given.
  private func routes(
    keyCode: UInt16 = VK.ansiS, ctrl: Bool = false, meta: Bool = false, alt: Bool = false,
    optionAsAlt: Bool = false, hasMarkedText: Bool = false
  ) -> Bool {
    TerminalInputSeam.routesThroughInputContext(
      keyCode: keyCode, ctrl: ctrl, meta: meta, alt: alt, optionAsAlt: optionAsAlt,
      hasMarkedText: hasMarkedText)
  }

  // MARK: - routesThroughInputContext

  func testAPlainKeyGoesThroughTheInputContext() {
    XCTAssertTrue(
      routes(),
      "a plain printable key must go through interpretKeyEvents, so input methods"
        + " and dead keys see it")
  }

  func testNamedKeysCtrlAndMetaBypassTheInputContext() {
    XCTAssertFalse(
      routes(keyCode: VK.returnKey), "Return is a named key and goes straight to terminal_key")
    XCTAssertFalse(
      routes(keyCode: VK.leftArrow), "an arrow is a named key and goes straight to terminal_key")
    XCTAssertFalse(
      routes(ctrl: true), "a Ctrl chord must reach terminal_key as a chord, so <C-w> routes")
    XCTAssertFalse(routes(meta: true), "a Cmd chord is an accelerator, never input-method text")
  }

  func testOptionFollowsTheOptionAsAltSetting() {
    XCTAssertFalse(
      routes(alt: true, optionAsAlt: true),
      "with option-as-alt, Option+key is an Alt chord and must bypass the input method")
    XCTAssertTrue(
      routes(alt: true, optionAsAlt: false),
      "with option-as-option, Option+key composes (Option-u then u gives ü), so it must go"
        + " through the input method")
  }

  func testMarkedTextKeepsEveryKeyInTheInputContext() {
    XCTAssertTrue(
      routes(ctrl: true, hasMarkedText: true),
      "while an input method has marked text, a Ctrl key belongs to it: bypassing it commits"
        + " a half-typed composition to the shell")
    XCTAssertTrue(
      routes(keyCode: VK.returnKey, hasMarkedText: true),
      "while marked text is pending, Return confirms the composition in the input method")
  }

  // MARK: - committedText

  func testOnePrintableASCIIScalarIsAKey() {
    XCTAssertEqual(
      TerminalInputSeam.committedText("s"), Route.key(0x73),
      "a commit of `s` must be a key, so the <C-w> router sees it as the chord's second key")
    XCTAssertEqual(
      TerminalInputSeam.committedText(" "), Route.key(0x20), "a space is a printable key")
  }

  func testOtherTextIsText() {
    XCTAssertEqual(
      TerminalInputSeam.committedText("é"), Route.text("é"),
      "a composed `é` must go through terminal_text: .key means any single scalar is taken as a"
        + " key, which the spec reserves for input-method text")
    XCTAssertEqual(
      TerminalInputSeam.committedText("ab"), Route.text("ab"),
      "a commit of two characters is text")
  }

  func testControlScalarsAreRemovedFromText() {
    XCTAssertEqual(
      TerminalInputSeam.committedText("a\u{1}b\u{7F}"), Route.text("ab"),
      "C0 and DEL scalars must be removed from committed text, so an input method cannot type"
        + " a control character")
  }

  func testEmptyOrControlOnlyTextIsNothing() {
    XCTAssertEqual(TerminalInputSeam.committedText(""), Route.none, "an empty commit sends nothing")
    XCTAssertEqual(
      TerminalInputSeam.committedText("\u{1}"), Route.none,
      "a control-only commit sends nothing: .key(1) would type Ctrl-A into the shell")
  }

  // MARK: - focusReports

  func testAnUnchangedSessionReportsNothing() {
    XCTAssertEqual(
      TerminalInputSeam.focusReports(previous: 5, current: 5), [],
      "focus staying on session 5 must report nothing, or every wake sends it a focus report")
    XCTAssertEqual(
      TerminalInputSeam.focusReports(previous: nil, current: nil), [],
      "focus staying off every session reports nothing")
  }

  func testFocusArrivingReportsAGain() {
    XCTAssertEqual(
      TerminalInputSeam.focusReports(previous: nil, current: 5),
      [Report(session: 5, gained: true)], "focus entering session 5 reports its gain")
  }

  func testFocusMovingReportsTheLossThenTheGain() {
    XCTAssertEqual(
      TerminalInputSeam.focusReports(previous: 3, current: 5),
      [Report(session: 3, gained: false), Report(session: 5, gained: true)],
      "focus moving from 3 to 5 must report 3's loss, then 5's gain: a missing loss leaves"
        + " session 3's program believing it is focused, and skips its file-tree rescan")
  }

  func testFocusLeavingReportsALoss() {
    XCTAssertEqual(
      TerminalInputSeam.focusReports(previous: 5, current: nil),
      [Report(session: 5, gained: false)], "focus leaving session 5 reports its loss")
  }
}
