// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#glyphstyletestsswift

import XCTest

@testable import WolfKeyMap

final class GlyphStyleTests: XCTestCase {

  // Style-flag bits, mirroring `mod flag` in `crates/wolf-ffi/src/lib.rs` (the
  // single source of truth — keep in lock-step).
  private let bold: UInt8 = 1 << 0
  private let italic: UInt8 = 1 << 1
  private let underline: UInt8 = 1 << 2
  private let undercurl: UInt8 = 1 << 3
  private let strikethrough: UInt8 = 1 << 4

  // Distinct, recognizable colors so a fg-vs-special mix-up is unambiguous. The
  // generated `FfiColor` is the transparent `{ rgb: UInt32, set: Bool }` struct.
  private let fg = FfiColor(rgb: 0x00FF_FFFF, set: true)
  private let special = FfiColor(rgb: 0x0000_FF00, set: true)
  private let unsetSpecial = FfiColor(rgb: 0, set: false)

  /// Component-wise color equality (does not assume `FfiColor: Equatable`).
  private func assertColorEqual(
    _ lhs: FfiColor, _ rhs: FfiColor, _ message: String,
    file: StaticString = #file, line: UInt = #line
  ) {
    XCTAssertEqual(lhs.rgb, rhs.rgb, message, file: file, line: line)
    XCTAssertEqual(lhs.set, rhs.set, message, file: file, line: line)
  }

  // MARK: - face selection

  func testFlagsZeroIsRegularWithNoDecorations() {
    let style = GlyphStyleResolver.resolve(flags: 0, fg: fg, special: unsetSpecial)
    XCTAssertEqual(style.face, .regular, "no flags -> the regular face")
    XCTAssertNil(style.underline, "no flags -> no underline of either kind")
    XCTAssertFalse(style.strikethrough, "no flags -> no strikethrough")
  }

  func testBoldBitSelectsBoldFace() {
    let style = GlyphStyleResolver.resolve(flags: bold, fg: fg, special: unsetSpecial)
    XCTAssertEqual(style.face, .bold, "the bold bit alone -> the bold face")
  }

  func testItalicBitSelectsItalicFace() {
    let style = GlyphStyleResolver.resolve(flags: italic, fg: fg, special: unsetSpecial)
    XCTAssertEqual(style.face, .italic, "the italic bit alone -> the italic face")
  }

  func testBoldAndItalicSelectBoldItalicFace() {
    let style = GlyphStyleResolver.resolve(
      flags: bold | italic, fg: fg, special: unsetSpecial)
    XCTAssertEqual(style.face, .boldItalic, "bold + italic -> the boldItalic face")
  }

  // MARK: - underline kind: straight vs. wave

  func testUnderlineBitAloneResolvesToStraightUnderline() {
    let style = GlyphStyleResolver.resolve(flags: underline, fg: fg, special: unsetSpecial)
    XCTAssertEqual(
      style.underline, .straight,
      "the underline bit alone must draw a straight line and no wave: nil means the bit was"
        + " dropped, .wave means a plain underline is painted as an undercurl")
  }

  func testUndercurlBitAloneResolvesToWave() {
    let style = GlyphStyleResolver.resolve(flags: undercurl, fg: fg, special: unsetSpecial)
    XCTAssertEqual(
      style.underline, .wave,
      "the undercurl bit alone must draw a wave and no straight underline: .straight means"
        + " undercurl is folded into a plain underline, nil means the bit was dropped")
  }

  func testUnderlineAndUndercurlTogetherResolveToWave() {
    for sp in [special, unsetSpecial] {
      let style = GlyphStyleResolver.resolve(
        flags: underline | undercurl, fg: fg, special: sp)
      XCTAssertEqual(
        style.underline, .wave,
        "with both bits set (special.set = \(sp.set)) undercurl must win, as in wolf-gnome's"
          + " `glyph_decoration` precedence: .straight means the underline bit is tested first"
          + " or undercurl is folded into it, and the two front-ends disagree")
    }
  }

  // MARK: - strikethrough

  func testStrikethroughBitEnablesStrikethroughInForegroundColor() {
    let style = GlyphStyleResolver.resolve(
      flags: strikethrough, fg: fg, special: unsetSpecial)
    XCTAssertTrue(style.strikethrough, "the strikethrough bit enables strikethrough")
    assertColorEqual(
      style.strikethroughColor, fg, "strikethrough is drawn in the foreground color")
  }

  // MARK: - underline color: special when set, else foreground

  func testUnderlineColorIsSpecialWhenSpecialSet() {
    let style = GlyphStyleResolver.resolve(flags: underline, fg: fg, special: special)
    assertColorEqual(
      style.underlineColor, special,
      "a SET special color drives the underline color")
  }

  func testUnderlineColorFallsBackToForegroundWhenSpecialUnset() {
    let style = GlyphStyleResolver.resolve(flags: underline, fg: fg, special: unsetSpecial)
    assertColorEqual(
      style.underlineColor, fg,
      "with the special color unset the underline color falls back to fg")
  }

  func testUndercurlColorIsSpecialWhenSpecialSet() {
    let style = GlyphStyleResolver.resolve(flags: undercurl, fg: fg, special: special)
    assertColorEqual(
      style.underlineColor, special,
      "a SET special color must drive the undercurl's wave color too; fg here means the wave"
        + " ignores `guisp` (spell and diagnostic colors are lost)")
  }

  func testUndercurlColorFallsBackToForegroundWhenSpecialUnset() {
    let style = GlyphStyleResolver.resolve(flags: undercurl, fg: fg, special: unsetSpecial)
    assertColorEqual(
      style.underlineColor, fg,
      "with the special color unset the wave must fall back to fg; any other value means the"
        + " unset special (rgb 0, set false) leaked through as a black wave")
  }
}
