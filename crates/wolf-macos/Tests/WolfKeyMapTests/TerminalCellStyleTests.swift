// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

final class TerminalCellStyleTests: XCTestCase {

  // Distinct colours, so a swap or a default taken in error names itself in the failure.
  private let red: UInt32 = 0x00FF_0000
  private let green: UInt32 = 0x0000_FF00
  private let defaultFgRGB: UInt32 = 0x00DD_DDDD
  private let defaultBgRGB: UInt32 = 0x0011_1111
  private let cursorRGB: UInt32 = 0x00FF_8800

  private var defaultFg: FfiColor { FfiColor(rgb: defaultFgRGB, set: true) }
  private var defaultBg: FfiColor { FfiColor(rgb: defaultBgRGB, set: true) }
  private var unset: FfiColor { FfiColor(rgb: 0x00AB_CDEF, set: false) }

  /// `resolve` over a red-on-green narrow cell, with the given flags, underline, width and
  /// selection.
  private func paint(
    flags: UInt8 = 0, underline: UInt8 = TerminalCellStyle.Underline.none,
    width: UInt8 = TerminalCellStyle.Width.narrow, selected: Bool = false,
    fg: FfiColor? = nil, bg: FfiColor? = nil
  ) -> TerminalCellPaint {
    TerminalCellStyle.resolve(
      fg: fg ?? FfiColor(rgb: red, set: true), bg: bg ?? FfiColor(rgb: green, set: true),
      flags: flags, underline: underline, width: width, selected: selected,
      defaultFg: defaultFg, defaultBg: defaultBg)
  }

  // MARK: - the constants mirror wolf-ffi's discriminants

  func testFlagBitsMirrorTheBridge() {
    let flags = TerminalCellStyle.Flag.self
    XCTAssertEqual(
      [
        flags.bold, flags.faint, flags.italic, flags.strikethrough, flags.inverse,
        flags.invisible, flags.blink,
      ],
      [1 << 0, 1 << 1, 1 << 2, 1 << 3, 1 << 4, 1 << 5, 1 << 6],
      "the flag bits must equal `mod terminal_flag` in crates/wolf-ffi/src/lib.rs bit for bit;"
        + " a mismatch paints the wrong attribute for every cell")
  }

  func testUnderlineAndWidthValuesMirrorTheBridge() {
    let u = TerminalCellStyle.Underline.self
    XCTAssertEqual(
      [u.none, u.single, u.double, u.curly, u.dotted, u.dashed], [0, 1, 2, 3, 4, 5],
      "the underline values must equal `mod terminal_underline`")
    let w = TerminalCellStyle.Width.self
    XCTAssertEqual(
      [w.narrow, w.wide, w.spacerTail, w.spacerHead], [0, 1, 2, 3],
      "the width values must equal `mod terminal_cell_width`")
    XCTAssertEqual(
      [
        TerminalCursorShape.block.rawValue, TerminalCursorShape.bar.rawValue,
        TerminalCursorShape.underline.rawValue, TerminalCursorShape.hollow.rawValue,
      ], [0, 1, 2, 3], "the cursor shapes must equal `mod terminal_cursor_style`")
  }

  // MARK: - colours

  func testSetColoursPassThrough() {
    let p = paint()
    XCTAssertEqual(
      p.foreground, red,
      "a set fg must be the foreground; \(String(p.foreground, radix: 16)) means it was swapped"
        + " or replaced by the default")
    XCTAssertEqual(p.background, green, "a set bg must be the background")
  }

  func testUnsetColoursTakeTheDefaults() {
    let p = paint(fg: unset, bg: unset)
    XCTAssertEqual(
      p.foreground, defaultFgRGB,
      "an unset fg must take the default fg, never the unset colour's stale rgb (0xABCDEF)")
    XCTAssertEqual(
      p.background, defaultBgRGB,
      "an unset bg must take the default bg, never the unset colour's stale rgb")
  }

  func testInverseSwaps() {
    let p = paint(flags: TerminalCellStyle.Flag.inverse)
    XCTAssertEqual(
      [p.foreground, p.background], [green, red],
      "an inverse cell must draw its bg as the foreground and its fg as the background")
  }

  func testInverseSwapsResolvedDefaults() {
    let p = paint(flags: TerminalCellStyle.Flag.inverse, fg: unset, bg: unset)
    XCTAssertEqual(
      [p.foreground, p.background], [defaultBgRGB, defaultFgRGB],
      "an inverse cell with default colours must swap the defaults, which is how a program"
        + " draws a reverse-video bar")
  }

  func testSelectedSwaps() {
    let p = paint(selected: true)
    XCTAssertEqual(
      [p.foreground, p.background], [green, red],
      "a selected cell must draw swapped; unswapped means the selection is invisible (the"
        + " `selected` term is dropped)")
  }

  func testSelectedInverseDrawsPlain() {
    let p = paint(flags: TerminalCellStyle.Flag.inverse, selected: true)
    XCTAssertEqual(
      [p.foreground, p.background], [red, green],
      "a selected inverse cell must draw plain (inverse != selected): swapped means the rule"
        + " is `inverse || selected`, so selecting reverse video shows no selection")
  }

  // MARK: - glyph visibility

  func testInvisibleAndSpacersDrawNoGlyph() {
    let cases: [(String, UInt8, UInt8)] = [
      ("invisible", TerminalCellStyle.Flag.invisible, TerminalCellStyle.Width.narrow),
      ("spacer tail", 0, TerminalCellStyle.Width.spacerTail),
      ("spacer head", 0, TerminalCellStyle.Width.spacerHead),
    ]
    for (name, flags, width) in cases {
      XCTAssertFalse(
        paint(flags: flags, width: width).drawsGlyph,
        "a \(name) cell must draw no glyph: drawing one paints hidden text, or a stray glyph"
          + " over the right half of a wide character")
    }
  }

  func testNarrowAndWideDrawTheirGlyph() {
    for (name, width) in [
      ("narrow", TerminalCellStyle.Width.narrow), ("wide", TerminalCellStyle.Width.wide),
    ] {
      XCTAssertTrue(
        paint(width: width).drawsGlyph, "a visible \(name) cell must draw its glyph")
    }
  }

  // MARK: - faint

  func testFaintDrawsAtFaintAlpha() {
    XCTAssertEqual(
      paint(flags: TerminalCellStyle.Flag.faint).foregroundAlpha, TerminalCellStyle.faintAlpha,
      "a faint cell must draw its foreground at faintAlpha")
    XCTAssertLessThan(
      TerminalCellStyle.faintAlpha, 1, "faint text must draw lighter than normal text")
  }

  func testNotFaintDrawsOpaque() {
    XCTAssertEqual(
      paint().foregroundAlpha, 1, "a cell that is not faint must draw its foreground opaque")
  }

  // MARK: - face

  func testEachFace() {
    let f = TerminalCellStyle.Flag.self
    let cases: [(UInt8, GlyphFace)] = [
      (0, .regular), (f.bold, .bold), (f.italic, .italic), (f.bold | f.italic, .boldItalic),
    ]
    for (flags, face) in cases {
      XCTAssertEqual(
        paint(flags: flags).style.face, face, "flags \(flags) must draw the \(face) face")
    }
  }

  // MARK: - decorations

  func testCurlyUnderlineIsAWave() {
    XCTAssertEqual(
      paint(underline: TerminalCellStyle.Underline.curly).style.underline, .wave,
      "a curly underline must draw as the wave: .straight means it is drawn as single")
  }

  func testOtherUnderlinesAreStraight() {
    let u = TerminalCellStyle.Underline.self
    for (name, value) in [
      ("single", u.single), ("double", u.double), ("dotted", u.dotted), ("dashed", u.dashed),
    ] {
      XCTAssertEqual(
        paint(underline: value).style.underline, .straight,
        "a \(name) underline must draw as the single straight line")
    }
  }

  func testNoUnderlineIsNil() {
    XCTAssertNil(paint().style.underline, "a cell with no underline must stroke none")
  }

  func testDecorationsUseTheResolvedForeground() {
    let p = paint(
      flags: TerminalCellStyle.Flag.inverse | TerminalCellStyle.Flag.strikethrough,
      underline: TerminalCellStyle.Underline.single)
    XCTAssertTrue(p.style.strikethrough, "the strikethrough bit must stroke a strikethrough")
    XCTAssertEqual(
      p.style.underlineColor, FfiColor(rgb: green, set: true),
      "the underline must be stroked in the resolved (here swapped) foreground, set")
    XCTAssertEqual(
      p.style.strikethroughColor, FfiColor(rgb: green, set: true),
      "the strikethrough must be stroked in the resolved foreground, set")
  }

  func testNoStrikethroughWithoutTheBit() {
    XCTAssertFalse(paint().style.strikethrough, "no strikethrough bit, no strikethrough")
  }

  func testBlinkIsIgnored() {
    XCTAssertEqual(
      paint(flags: TerminalCellStyle.Flag.blink), paint(),
      "blink must not change the paint: the cursor and text draw steady")
  }

  // MARK: - cursor

  func testCursorFillIsTheCursorColourWhenSet() {
    let c = TerminalCellStyle.cursorColors(
      cursorColor: FfiColor(rgb: cursorRGB, set: true), defaultFg: defaultFg,
      defaultBg: defaultBg)
    XCTAssertEqual(
      c.fill, cursorRGB,
      "a set cursor colour must fill the cursor; the default fg means it is ignored")
    XCTAssertEqual(c.glyph, defaultBgRGB, "the glyph under a block cursor draws in the default bg")
  }

  func testCursorFillIsTheDefaultForegroundWhenUnset() {
    let c = TerminalCellStyle.cursorColors(
      cursorColor: unset, defaultFg: defaultFg, defaultBg: defaultBg)
    XCTAssertEqual(
      c.fill, defaultFgRGB,
      "an unset cursor colour must fall back to the default fg, never the unset colour's rgb")
    XCTAssertEqual(c.glyph, defaultBgRGB, "the glyph under a block cursor draws in the default bg")
  }

  func testCursorSpansTwoCellsOverAWideCell() {
    XCTAssertEqual(
      TerminalCellStyle.cursorSpan(width: TerminalCellStyle.Width.wide), 2,
      "a cursor over a wide character must cover both of its cells")
    XCTAssertEqual(
      TerminalCellStyle.cursorSpan(width: TerminalCellStyle.Width.narrow), 1,
      "a cursor over a narrow character covers one cell")
  }

  func testCursorShapeIsHollowUnlessFocused() {
    let cases: [(style: UInt8, focused: Bool, shape: TerminalCursorShape)] = [
      (0, true, .block), (1, true, .bar), (2, true, .underline), (3, true, .hollow),
      (9, true, .block),
      (0, false, .hollow), (1, false, .hollow), (2, false, .hollow), (3, false, .hollow),
      (9, false, .hollow),
    ]
    for c in cases {
      XCTAssertEqual(
        TerminalCellStyle.cursorShape(style: c.style, focused: c.focused), c.shape,
        "style \(c.style) with focused \(c.focused) must paint \(c.shape): a focused pane paints"
          + " the frame's style, an unknown one as a block, and any other pane a hollow block."
          + " The frame's style unfocused means focus is ignored; hollow focused means the"
          + " style is")
    }
  }
}
