// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

final class StatusbarTextTests: XCTestCase {

  /// Tolerance for the alpha comparisons, which are products of two constants.
  private let eps: Double = 1e-9

  /// A distinctive themed colour, so a rule that substitutes any other rgb is caught.
  private let themedRgb: UInt32 = 0x0012_3456

  // MARK: - Helpers

  /// Asserts `got` is `.themed` with exactly `rgb` and `alpha` (within `eps`).
  private func assertThemed(
    _ got: StatusbarTextColor, rgb: UInt32, alpha: Double, _ desc: String,
    file: StaticString = #filePath, line: UInt = #line
  ) {
    guard case .themed(let gotRgb, let gotAlpha) = got else {
      XCTFail(
        "\(desc): want .themed(rgb: \(rgb), alpha: \(alpha)), got \(got). A .label here means "
          + "the rule ignored the set titlebar foreground and fell back to AppKit's tiers.",
        file: file, line: line)
      return
    }
    XCTAssertEqual(
      gotRgb, rgb,
      "\(desc): the themed rgb must be the titlebar fg passed in, unchanged; another value "
        + "means the rule rewrote or dropped the theme colour",
      file: file, line: line)
    XCTAssertEqual(
      gotAlpha, alpha, accuracy: eps,
      "\(desc): wrong alpha. 1 / 0.7 are the active primary / secondary, 0.5 / 0.35 the "
        + "inactive ones; any other value means the active or secondary scaling changed",
      file: file, line: line)
  }

  // MARK: - The 2x2 cross product: titlebar fg set/unset x active/inactive

  func testThemedActiveIsFullStrengthAndBold() {
    let text = StatusbarText(titlebarFg: FfiColor(rgb: themedRgb, set: true), active: true)
    assertThemed(text.primary, rgb: themedRgb, alpha: 1.0, "themed active primary")
    assertThemed(text.secondary, rgb: themedRgb, alpha: 0.7, "themed active secondary")
    XCTAssertTrue(
      text.primaryBold,
      "the active pane's filename must be bold; regular weight here means the active pane lost "
        + "its only weight cue")
  }

  func testThemedInactiveIsDimmedAndRegular() {
    let text = StatusbarText(titlebarFg: FfiColor(rgb: themedRgb, set: true), active: false)
    assertThemed(text.primary, rgb: themedRgb, alpha: 0.5, "themed inactive primary")
    assertThemed(text.secondary, rgb: themedRgb, alpha: 0.35, "themed inactive secondary")
    XCTAssertFalse(
      text.primaryBold,
      "an inactive pane's filename must be regular weight; bold here makes every pane look "
        + "active")
  }

  func testUnsetActiveUsesPrimaryAndSecondaryLabels() {
    let text = StatusbarText(titlebarFg: FfiColor(rgb: 0, set: false), active: true)
    XCTAssertEqual(
      text.primary, .label(.primary),
      "unset titlebar fg, active: the filename must be labelColor; another tier means the "
        + "active pane is dimmed, a .themed value means the rule ignored `set`")
    XCTAssertEqual(
      text.secondary, .label(.secondary),
      "unset titlebar fg, active: the path must be secondaryLabelColor")
    XCTAssertTrue(text.primaryBold, "the active pane's filename must be bold without a theme too")
  }

  func testUnsetInactiveStepsDownOneLabelTier() {
    let text = StatusbarText(titlebarFg: FfiColor(rgb: 0, set: false), active: false)
    XCTAssertEqual(
      text.primary, .label(.secondary),
      "unset titlebar fg, inactive: the filename must step down to secondaryLabelColor; "
        + ".primary means the inactive pane is not dimmed at all")
    XCTAssertEqual(
      text.secondary, .label(.tertiary),
      "unset titlebar fg, inactive: the path must step down to tertiaryLabelColor; .secondary "
        + "means the path line is not dimmed with its pane")
    XCTAssertFalse(text.primaryBold, "an inactive pane's filename must be regular weight")
  }

  // MARK: - Discriminators: the branch keys on `set`, never on the rgb value

  func testSetBlackIsStillThemed() {
    for active in [true, false] {
      let text = StatusbarText(titlebarFg: FfiColor(rgb: 0, set: true), active: active)
      let a = active ? 1.0 : 0.5
      assertThemed(
        text.primary, rgb: 0, alpha: a,
        "set black titlebar fg (active \(active)) primary; a .label means the rule keys on a "
          + "non-zero rgb instead of `set`, so a black theme falls back to AppKit's colours")
      assertThemed(
        text.secondary, rgb: 0, alpha: 0.7 * a,
        "set black titlebar fg (active \(active)) secondary")
    }
  }

  func testUnsetWithNonZeroRgbIsStillLabel() {
    for active in [true, false] {
      let text = StatusbarText(titlebarFg: FfiColor(rgb: 0x00AB_CDEF, set: false), active: active)
      XCTAssertEqual(
        text.primary, .label(active ? .primary : .secondary),
        "unset titlebar fg with a stale non-zero rgb (active \(active)) must still use the "
          + "label tier; a .themed value means the rule keys on the rgb instead of `set`")
      XCTAssertEqual(
        text.secondary, .label(active ? .secondary : .tertiary),
        "unset titlebar fg with a stale non-zero rgb (active \(active)): secondary tier")
    }
  }
}
