//
//  GlyphStyleTests.swift
//  Pure, headless unit tests for the AppKit-/CoreText-free `WolfKeyMap.GlyphStyle`
//  resolution seam.
//
//  These exercise `GlyphStyleResolver.resolve(flags:fg:special:)` — the pure seam
//  the coder extracts into `Sources/WolfKeyMap/GlyphStyle.swift` (plan §B1) that
//  maps a cell's packed style-flag bits (from `FfiCell.flags()`) plus its fg /
//  special colors into a `GlyphStyle` value the renderer paints from. Like the
//  sibling `CellMetricsMathTests`, they import ONLY Foundation-level test machinery
//  (no AppKit / CoreText, no live font) and stay in the headless `WolfKeyMap`
//  module; on-screen font-variant selection + decoration stroking are manual-smoke
//  only.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor, plan §B1)
//
//    public enum GlyphFace: Equatable { case regular, bold, italic, boldItalic }
//    public struct GlyphStyle: Equatable {
//        face; underline: Bool; underlineColor: FfiColor;
//        strikethrough: Bool; strikethroughColor: FfiColor
//    }
//    public enum GlyphStyleResolver {
//        public static func resolve(flags: UInt8, fg: FfiColor, special: FfiColor)
//            -> GlyphStyle
//    }
//
//  Resolution rules asserted below:
//   - face = boldItalic if BOTH bold+italic bits, else bold, else italic, else regular.
//   - underline = underline-bit OR undercurl-bit (undercurl → straight underline).
//   - underlineColor = special if `special.set` else fg.
//   - strikethrough = strikethrough-bit; strikethroughColor = fg.
//

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
    XCTAssertFalse(style.underline, "no flags -> no underline")
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

  // MARK: - underline (straight + undercurl)

  func testUnderlineBitEnablesUnderline() {
    let style = GlyphStyleResolver.resolve(flags: underline, fg: fg, special: unsetSpecial)
    XCTAssertTrue(style.underline, "the underline bit enables underline")
  }

  func testUndercurlBitAloneEnablesStraightUnderline() {
    let style = GlyphStyleResolver.resolve(flags: undercurl, fg: fg, special: unsetSpecial)
    XCTAssertTrue(
      style.underline, "undercurl maps to a straight underline this pass")
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
}
