// The titlebar background's precedence: a set WolfTitlebar, then Normal, then the system colour.

import XCTest

@testable import WolfKeyMap

/// `TitlebarBackground(titlebar:normal:)` over hand-built bridge colours. An unset colour
/// carries a non-zero `rgb` distinct from every set one, so code that reads `rgb` without
/// checking `set` returns a colour no row expects.
final class TitlebarBackgroundTests: XCTestCase {
  /// `WolfTitlebar`'s background when a colorscheme sets it.
  private let titlebarRgb: UInt32 = 0x10_1010
  /// `Normal`'s background, the grid fill.
  private let normalRgb: UInt32 = 0x1B_1B1B
  /// The `rgb` an unset `Normal` carries: neither colour above.
  private let staleRgb: UInt32 = 0xAB_CDEF

  func testASetWolfTitlebarBeatsNormal() {
    let titlebar = FfiColor(rgb: titlebarRgb, set: true)
    XCTAssertEqual(
      TitlebarBackground(titlebar: titlebar, normal: FfiColor(rgb: normalRgb, set: true)),
      .colour(titlebarRgb),
      "a set WolfTitlebar #101010 must win over a set Normal #1b1b1b, since a colorscheme that"
        + " defines it is asking for it. .colour(0x1b1b1b) means Normal is checked first;"
        + " .system means a set WolfTitlebar is ignored")
    XCTAssertEqual(
      TitlebarBackground(titlebar: titlebar, normal: FfiColor(rgb: staleRgb, set: false)),
      .colour(titlebarRgb),
      "a set WolfTitlebar #101010 must colour the titlebar while Normal is unknown, too."
        + " .system means WolfTitlebar is only consulted once Normal is known;"
        + " .colour(0xabcdef) means an unset Normal's stale rgb is read")
  }

  func testNormalIsUsedWhenWolfTitlebarIsUnset() {
    XCTAssertEqual(
      TitlebarBackground(
        titlebar: FfiColor(rgb: titlebarRgb, set: false),
        normal: FfiColor(rgb: normalRgb, set: true)),
      .colour(normalRgb),
      "with WolfTitlebar unset the titlebar must take Normal's background #1b1b1b, the grid's."
        + " .system means the WolfTitlebar-only rule, which leaves the titlebar the system colour"
        + " under a colorscheme such as Lighthouse; .colour(0x101010) means an unset WolfTitlebar's"
        + " stale rgb is read without checking set")
  }

  func testTheSystemColourWhenNeitherIsKnown() {
    XCTAssertEqual(
      TitlebarBackground(
        titlebar: FfiColor(rgb: titlebarRgb, set: false),
        normal: FfiColor(rgb: staleRgb, set: false)),
      .system,
      "with neither WolfTitlebar nor Normal known, before the first frame, the titlebar must"
        + " keep the system colour. .colour(0xabcdef) means an unset Normal's rgb is read;"
        + " .colour(0x101010) means an unset WolfTitlebar's rgb is read")
  }
}
