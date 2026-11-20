// Draws Nerd Font icons through GlyphPainter offscreen and checks which face painted them.

import AppKit
import CoreText
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// Pixel coverage of the Symbols Nerd Font Mono cascade `CellMetrics` puts behind every grid
/// face, drawn by the real `GlyphPainter` through CoreText.
///
/// A test draws one cell into an 8-bit gray bitmap and compares it with the same cell drawn by
/// a reference painter over the same geometry: one whose faces are the symbols face itself
/// ("symbols direct"), or one whose faces are the plain configured faces, which CoreText's
/// system cascade alone backs ("system alone"). A message that starts `premise:` means the
/// comparison could not tell the outcomes apart, or the staging is wrong, never that the
/// cascade is.
@MainActor
final class SymbolsFallbackTests: XCTestCase {
  /// nf-fa-folder, which neither iA Writer Mono S nor JetBrains Mono maps.
  private static let folder: UInt16 = 0xF07B
  /// The Powerline branch, which iA Writer Mono S does not map.
  private static let branch: UInt16 = 0xE0A0
  /// Powerline symbols a monospace family may ship itself, in order of preference.
  private static let powerline: [UInt16] = [0xE0A0, 0xE0B0, 0xE0A1, 0xE0B2]
  /// Hangul 한, which neither the configured faces nor the symbols face maps.
  private static let hangul: UInt16 = 0xD55C
  /// Letters every configured face maps.
  private static let letters: [UInt16] = [0x61, 0x67, 0x4D]

  /// The family `wolf_ffi_load_config()` defaults to.
  private static let defaultFamily = "iA Writer Mono S"
  private static let jetBrains = "JetBrains Mono"
  /// A family no font provides, standing in for a build with no symbols font.
  private static let missingSymbols = "Wolf Missing Symbols"
  /// The stepped size the View-menu path measures at, off the 14 pt default.
  private static let steppedSize: CGFloat = 28

  /// Device pixels per point in the gray bitmaps, so a 14 pt icon covers enough pixels.
  private static let scale: CGFloat = 2
  /// A gray pixel darker than this is ink.
  private static let inkLevel: UInt8 = 192
  /// Two gray pixels further apart than this differ.
  private static let pixelTolerance = 64
  /// "Match": at most this fraction of the reference's ink pixels, and at least
  /// `matchFloor`, differ.
  private static let matchFraction = 0.01
  private static let matchFloor = 2
  /// "Differ": at least this fraction of the reference's ink pixels differ.
  private static let differFraction = 0.20
  /// The fewest ink pixels a reference drawing needs to be worth comparing against.
  private static let minimumInk = 20

  /// The colour the editor-grid test highlights its icon in, and how near a pixel must be.
  private static let green = NSColor(srgbRed: 0, green: 1, blue: 0, alpha: 1)
  private static let greenTolerance: CGFloat = 0.4
  /// The `:lua` line the editor-grid test types: the folder icon alone on buffer line 2,
  /// highlighted in `green`, with the cursor on line 3 so its block covers no part of it.
  /// It holds no `|`, which would end the `:lua` command.
  private static let iconLine =
    "lua local b=vim.api.nvim_get_current_buf() local c=vim.fn.nr2char(0xF07B)"
    + " vim.api.nvim_set_hl(0,'WolfTestIcon',{fg='#00FF00'})"
    + " vim.api.nvim_buf_set_lines(b,0,-1,false,{'',c,''})"
    + " vim.fn.matchadd('WolfTestIcon',c) vim.api.nvim_win_set_cursor(0,{3,0})"
    + " vim.bo[b].modified=false"

  // MARK: - Tests

  func testRegularFaceDrawsNerdGlyphsFromSymbolsFont() {
    guard let config = stage() else { return }
    let metrics = CellMetrics.measured(config: config)
    for code in [Self.folder, Self.branch] {
      assertDrawsFromSymbols(code, metrics: metrics, family: Self.defaultFamily, face: .regular)
    }
  }

  func testStyledFacesDrawNerdGlyphsFromSymbolsFont() {
    guard let config = stage() else { return }
    let metrics = CellMetrics.measured(config: config, pointSize: Self.steppedSize)
    let traits: [(GlyphFace, CTFontSymbolicTraits)] = [
      (.bold, .traitBold), (.italic, .traitItalic), (.boldItalic, [.traitBold, .traitItalic]),
    ]
    for (face, wanted) in traits {
      let actual = CTFontGetSymbolicTraits(metrics.font(for: face))
      XCTAssertTrue(
        actual.contains(wanted),
        "premise: the \(face) face of \(Self.defaultFamily) carries traits \(actual.rawValue),"
          + " not \(wanted.rawValue): the variant is the regular face, so this test would"
          + " exercise the regular face twice")
      for code in [Self.folder, Self.branch] {
        assertDrawsFromSymbols(code, metrics: metrics, family: Self.defaultFamily, face: face)
      }
    }
  }

  func testJetBrainsMonoDrawsMissingNerdGlyphFromSymbolsFont() {
    guard let config = stage() else { return }
    let metrics = CellMetrics.measured(
      family: Self.jetBrains, pointSize: Self.steppedSize,
      lineHeightPercent: config.line_height())
    for face in [GlyphFace.regular, .bold] {
      assertDrawsFromSymbols(Self.folder, metrics: metrics, family: Self.jetBrains, face: face)
    }
  }

  func testConfiguredFaceKeepsItsOwnNerdGlyph() {
    guard let config = stage() else { return }
    let metrics = CellMetrics.measured(
      family: Self.jetBrains, pointSize: Self.steppedSize,
      lineHeightPercent: config.line_height())
    let plain = plainFaces(Self.jetBrains, size: Self.steppedSize)
    let symbols = symbolsFont(size: Self.steppedSize)
    guard
      let code = Self.powerline.first(where: { maps(plain.regular, $0) && maps(symbols, $0) })
    else {
      XCTFail(
        "premise: \(Self.jetBrains) and the symbols face share none of "
          + Self.powerline.map { hex($0) }.joined(separator: ", ")
          + ", so no glyph shows which face wins where both map it")
      return
    }
    let style = glyphStyle(.regular)
    let own = draw(code, metrics: faceMetrics(metrics, plain), style: style)
    let direct = draw(code, metrics: symbolsDirect(metrics), style: style)
    let apart = compare(own, to: direct)
    XCTAssertFalse(
      apart.matches,
      "premise: \(Self.jetBrains)'s own \(hex(code)) draws like the symbols face's"
        + " (\(apart.counts)), so which face painted it cannot be told apart")
    let drawn = compare(draw(code, metrics: metrics, style: style), to: own)
    XCTAssertTrue(
      drawn.matches,
      "\(hex(code)), which \(Self.jetBrains) maps, does not draw like \(Self.jetBrains)'s own"
        + " glyph (\(drawn.counts)): the symbols face sits ahead of the configured face, or"
        + " the cascaded face is not the configured face")
  }

  func testAsciiLetterDrawsFromConfiguredFace() {
    guard let config = stage() else { return }
    for family in [Self.defaultFamily, Self.jetBrains] {
      let metrics = CellMetrics.measured(
        family: family, pointSize: Self.steppedSize, lineHeightPercent: config.line_height())
      let plain = faceMetrics(metrics, plainFaces(family, size: Self.steppedSize))
      for face in [GlyphFace.regular, .bold] {
        let style = glyphStyle(face)
        for code in Self.letters {
          let own = draw(code, metrics: plain, style: style)
          let direct = draw(code, metrics: symbolsDirect(metrics), style: style)
          let apart = compare(direct, to: own)
          XCTAssertFalse(
            apart.matches,
            "premise: \(family) \(face) '\(text(code))' draws like the symbols face with its"
              + " own fallback (\(apart.counts)), so a face swapped for it would pass")
          let drawn = compare(draw(code, metrics: metrics, style: style), to: own)
          XCTAssertTrue(
            drawn.matches,
            "\(family) \(face) '\(text(code))' does not draw like the plain configured face"
              + " (\(drawn.counts)): the grid face is not the configured face, or the cascade"
              + " moved its glyphs")
        }
      }
    }
  }

  func testCascadeLeavesCellMetricsUnchanged() {
    guard let config = stage() else { return }
    for family in [Self.defaultFamily, Self.jetBrains] {
      let cascaded = CellMetrics.measured(
        family: family, pointSize: Self.steppedSize, lineHeightPercent: config.line_height())
      let plain = CellMetrics.measured(
        family: family, pointSize: Self.steppedSize, lineHeightPercent: config.line_height(),
        symbolsFamily: Self.missingSymbols)
      let style = glyphStyle(.regular)
      let apart = compare(
        draw(Self.folder, metrics: cascaded, style: style),
        to: draw(Self.folder, metrics: plain, style: style))
      XCTAssertFalse(
        apart.matches,
        "premise: \(family) draws \(hex(Self.folder)) alike with and without the symbols"
          + " cascade (\(apart.counts)), so the metrics below compare two uncascaded measures")
      XCTAssertEqual(
        cascaded.cellWidth, plain.cellWidth,
        "\(family): the symbols cascade moved the cell width, so it is measured from a face"
          + " other than the configured one")
      XCTAssertEqual(
        cascaded.cellHeight, plain.cellHeight,
        "\(family): the symbols cascade moved the cell height, so ascent, descent or leading"
          + " come from a face other than the configured one")
      XCTAssertEqual(
        cascaded.baseline, plain.baseline,
        "\(family): the symbols cascade moved the baseline, so every row's text shifts")
    }
  }

  func testUncoveredCharacterFallsThroughToSystemCascade() {
    guard let config = stage() else { return }
    let metrics = CellMetrics.measured(
      family: Self.defaultFamily, pointSize: Self.steppedSize,
      lineHeightPercent: config.line_height())
    let plain = plainFaces(Self.defaultFamily, size: Self.steppedSize)
    XCTAssertFalse(
      maps(plain.regular, Self.hangul),
      "premise: \(Self.defaultFamily) maps \(hex(Self.hangul)), so the system cascade is"
        + " never consulted for it")
    XCTAssertFalse(
      maps(symbolsFont(size: Self.steppedSize), Self.hangul),
      "premise: the symbols face maps \(hex(Self.hangul)), so it, not the system, paints it")
    let fallback = runFamilies(Self.hangul, font: plain.regular)
    XCTAssertFalse(
      fallback.isEmpty || fallback.contains("LastResort"),
      "premise: the plain face's own cascade draws \(hex(Self.hangul)) from \(fallback), not"
        + " a real Hangul face, so a cascade cut short at LastResort draws the same")
    let style = glyphStyle(.regular)
    let drawn = compare(
      draw(Self.hangul, metrics: metrics, style: style),
      to: draw(Self.hangul, metrics: faceMetrics(metrics, plain), style: style))
    XCTAssertTrue(
      drawn.matches,
      "\(hex(Self.hangul)) does not draw as the plain face's system cascade draws it"
        + " (\(drawn.counts)): the cascade after the symbols face is not the system list")
  }

  func testMissingSymbolsFamilyLeavesFacesUncascaded() {
    guard let config = stage() else { return }
    let lineHeight = config.line_height()
    let cascaded = CellMetrics.measured(
      family: Self.defaultFamily, pointSize: Self.steppedSize, lineHeightPercent: lineHeight)
    XCTAssertNotNil(
      CTFontCopyAttribute(cascaded.font, kCTFontCascadeListAttribute),
      "premise: the face measured with the symbols font present has no cascade list"
        + " attribute, so the absence of one below shows nothing")
    let missing = CellMetrics.measured(
      family: Self.defaultFamily, pointSize: Self.steppedSize, lineHeightPercent: lineHeight,
      symbolsFamily: Self.missingSymbols)
    let plain = plainFaces(Self.defaultFamily, size: Self.steppedSize)
    for face in [GlyphFace.regular, .bold, .italic, .boldItalic] {
      let got = CTFontCopyAttribute(missing.font(for: face), kCTFontCascadeListAttribute)
      let want = CTFontCopyAttribute(plain.font(for: face), kCTFontCascadeListAttribute)
      XCTAssertTrue(
        sameAttribute(got, want),
        "with \(Self.missingSymbols) not resolving, the \(face) face carries cascade list"
          + " \(String(describing: got)), not the plain face's \(String(describing: want)):"
          + " a cascade was built from whatever font CoreText substituted for the family")
    }
    let style = glyphStyle(.regular)
    let drawn = compare(
      draw(Self.folder, metrics: missing, style: style),
      to: draw(Self.folder, metrics: systemAlone(missing, family: Self.defaultFamily), style: style)
    )
    XCTAssertTrue(
      drawn.matches,
      "with no symbols font, \(hex(Self.folder)) does not draw as the system cascade alone"
        + " draws it (\(drawn.counts)): the faces were changed anyway")
  }

  func testEditorGridKeepsTheIconsOverflow() {
    guard stage() != nil, let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let icon = text(Self.folder)
    fixture.typeEx(Self.iconLine)
    var background: CGColor?
    let shown = fixture.wake {
      let frame = handle.read_frame_for_grid(handle.grid_for_win(handle.active_win()))
      let cols = Int(frame.cols())
      let cells = frame.cells()
      // Row 1, column 0.
      guard frame.rows() > 2, cols > 1, cols < cells.len() else { return false }
      let cell = cells[cols]
      guard cell.text().toString() == icon, cell.fg().set, cell.fg().rgb == 0x00_FF00 else {
        return false
      }
      background = WolfColor.cgColor(cell.bg(), fallback: WolfColor.fallbackBg)
      return true
    }
    guard shown, let background else {
      XCTFail(
        "premise: the active grid never showed \(hex(Self.folder)) in #00FF00 at row 1,"
          + " column 0: the typed :lua line did not run, or the highlight did not apply")
      return
    }

    let capture = fixture.render()
    let container = fixture.container
    let scale = CGFloat(capture.bitmap.pixelsWide) / container.bounds.width
    // The top third holds the statusbar strip and the grid's first rows, and nothing else green.
    let top = NSRect(
      x: 0, y: 0, width: container.bounds.width, height: container.bounds.height / 3)
    let grid = capture.colors(in: top, of: container).filter {
      colorDistance($0, Self.green) <= Self.greenTolerance
    }.count

    let reference = greenRegions(
      metrics: fixture.live.metrics, background: background, scale: scale)
    let total = reference.kept + reference.right
    XCTAssertGreaterThan(
      reference.right, total / 10,
      "premise: \(hex(Self.folder)) drawn alone puts \(reference.right) of its \(total) green"
        + " pixels right of its cell, so overpainting the next cell cannot be seen")
    let threshold = reference.kept + reference.right / 2
    XCTAssertGreaterThanOrEqual(
      grid, threshold,
      "the editor grid shows \(grid) green pixels of \(hex(Self.folder)); drawn alone it has"
        + " \(reference.kept) that no later fill covers and \(reference.right) right of its"
        + " cell. Near \(reference.kept): the next cell's background is painted over the icon,"
        + " so the row is not filled before its glyphs. Near 0: the icon is not drawn at all")
  }

  // MARK: - Staging and premises

  /// Registers the staged fonts and checks the families resolve; the config, or `nil` with the
  /// test failed.
  private func stage(file: StaticString = #filePath, line: UInt = #line) -> FfiConfig? {
    guard LiveCore.stageFonts(file: file, line: line) else { return nil }
    let config = wolf_ffi_load_config()
    let family = config.font_family().toString()
    guard family == Self.defaultFamily else {
      XCTFail(
        "premise: the default font_family is \(family), not \(Self.defaultFamily), which these"
          + " tests take as the configured face that lacks the icons", file: file, line: line)
      return nil
    }
    for name in [Self.defaultFamily, Self.jetBrains, FontRegistration.symbolsFamily] {
      let actual = CTFontCopyFamilyName(CTFontCreateWithName(name as CFString, 14, nil)) as String
      guard actual.caseInsensitiveCompare(name) == .orderedSame else {
        XCTFail(
          "premise: \(name) resolves to \(actual): its staged tree was not registered with"
            + " CoreText before the test measured", file: file, line: line)
        return nil
      }
    }
    return config
  }

  /// Checks that `code`, which the configured face lacks, draws from the symbols face through
  /// `metrics`, and not as the system cascade alone draws it.
  private func assertDrawsFromSymbols(
    _ code: UInt16, metrics: CellMetrics, family: String, face: GlyphFace,
    file: StaticString = #filePath, line: UInt = #line
  ) {
    let size = CTFontGetSize(metrics.font)
    let what = "\(family) \(face) at \(size) pt, \(hex(code))"
    let plain = plainFaces(family, size: size)
    XCTAssertFalse(
      maps(plain.font(for: face), code),
      "premise: \(what) is mapped by the configured face itself, so no cascade is consulted",
      file: file, line: line)
    XCTAssertTrue(
      maps(symbolsFont(size: size), code),
      "premise: \(what) is not mapped by \(FontRegistration.symbolsFamily): the staged tree"
        + " is not the vendored face", file: file, line: line)
    let style = glyphStyle(face)
    let direct = draw(code, metrics: symbolsDirect(metrics), style: style)
    let system = draw(code, metrics: systemAlone(metrics, family: family), style: style)
    let ink = direct.filter { $0 < Self.inkLevel }.count
    guard ink >= Self.minimumInk else {
      XCTFail(
        "premise: \(what) drawn from the symbols face directly has \(ink) ink pixels, too few"
          + " to compare against", file: file, line: line)
      return
    }
    let apart = compare(system, to: direct)
    XCTAssertTrue(
      apart.differs,
      "premise: \(what) drawn by the system cascade alone already looks like the symbols"
        + " face's (\(apart.counts)): CoreText's own fallback reaches the registered face, so"
        + " dropping the cascade cannot be seen", file: file, line: line)

    let drawn = draw(code, metrics: metrics, style: style)
    let fromSymbols = compare(drawn, to: direct)
    XCTAssertTrue(
      fromSymbols.matches,
      "\(what) does not draw like the symbols face (\(fromSymbols.counts)): this face has no"
        + " symbols cascade, the cascade does not reach CTLineDraw, or it moves the glyph",
      file: file, line: line)
    let fromSystem = compare(drawn, to: system)
    XCTAssertTrue(
      fromSystem.differs,
      "\(what) draws like the system cascade alone (\(fromSystem.counts)): the symbols face"
        + " is missing from this face's cascade, or sits behind the system list",
      file: file, line: line)
  }

  /// The families of the fonts CoreText draws `code` from, in a line set in `font`.
  private func runFamilies(_ code: UInt16, font: CTFont) -> [String] {
    let line = CTLineCreateWithAttributedString(
      NSAttributedString(string: text(code), attributes: [.font: font]))
    let runs = CTLineGetGlyphRuns(line) as? [CTRun] ?? []
    return runs.compactMap { run -> String? in
      let attributes = CTRunGetAttributes(run) as NSDictionary
      guard let value = attributes[kCTFontAttributeName] else { return nil }
      return CTFontCopyFamilyName(value as! CTFont) as String
    }
  }

  /// Whether `font` itself, without its cascade, has a glyph for `code`.
  private func maps(_ font: CTFont, _ code: UInt16) -> Bool {
    var character = code
    var glyph = CGGlyph(0)
    return CTFontGetGlyphsForCharacters(font, &character, &glyph, 1)
  }

  private func sameAttribute(_ a: CFTypeRef?, _ b: CFTypeRef?) -> Bool {
    switch (a, b) {
    case (nil, nil): return true
    case (let a?, let b?): return CFEqual(a, b)
    default: return false
    }
  }

  // MARK: - Faces

  /// The four faces `CellMetrics` builds for `family` before any cascade: the named face, and
  /// its trait copies, each falling back to it when CoreText resolves no such face.
  private struct Faces {
    let regular: CTFont
    let bold: CTFont
    let italic: CTFont
    let boldItalic: CTFont

    func font(for face: GlyphFace) -> CTFont {
      switch face {
      case .regular: return regular
      case .bold: return bold
      case .italic: return italic
      case .boldItalic: return boldItalic
      }
    }
  }

  private func plainFaces(_ family: String, size: CGFloat) -> Faces {
    let base = CTFontCreateWithName(family as CFString, size, nil)
    func variant(_ traits: CTFontSymbolicTraits) -> CTFont {
      guard let copy = CTFontCreateCopyWithSymbolicTraits(base, size, nil, traits, traits),
        CTFontGetSymbolicTraits(copy).intersection(traits) == traits
      else { return base }
      return copy
    }
    return Faces(
      regular: base, bold: variant(.traitBold), italic: variant(.traitItalic),
      boldItalic: variant([.traitBold, .traitItalic]))
  }

  private func symbolsFont(size: CGFloat) -> CTFont {
    CTFontCreateWithName(FontRegistration.symbolsFamily as CFString, size, nil)
  }

  /// A painter's metrics with `metrics`' geometry and `faces`.
  private func faceMetrics(_ metrics: CellMetrics, _ faces: Faces) -> CellMetrics {
    CellMetrics(
      cellWidth: metrics.cellWidth, cellHeight: metrics.cellHeight, baseline: metrics.baseline,
      font: faces.regular, boldFont: faces.bold, italicFont: faces.italic,
      boldItalicFont: faces.boldItalic)
  }

  /// `metrics`' geometry with the symbols face in every slot.
  private func symbolsDirect(_ metrics: CellMetrics) -> CellMetrics {
    let symbols = symbolsFont(size: CTFontGetSize(metrics.font))
    return faceMetrics(
      metrics, Faces(regular: symbols, bold: symbols, italic: symbols, boldItalic: symbols))
  }

  /// `metrics`' geometry with the plain faces of `family`, backed by the system cascade alone.
  private func systemAlone(_ metrics: CellMetrics, family: String) -> CellMetrics {
    faceMetrics(metrics, plainFaces(family, size: CTFontGetSize(metrics.font)))
  }

  // MARK: - Drawing

  private func glyphStyle(_ face: GlyphFace) -> GlyphStyle {
    GlyphStyle(
      face: face, underline: nil, underlineColor: FfiColor(rgb: 0, set: false),
      strikethrough: false, strikethroughColor: FfiColor(rgb: 0, set: false))
  }

  private func text(_ code: UInt16) -> String {
    guard let scalar = Unicode.Scalar(code) else {
      preconditionFailure("\(hex(code)) is not a scalar value")
    }
    return String(Character(scalar))
  }

  private func hex(_ code: UInt16) -> String {
    String(format: "U+%04X", code)
  }

  /// Draws `code` in black on white through `GlyphPainter(metrics:)` into a gray bitmap three
  /// cells square, at the middle cell, so overflow in any direction stays on the canvas.
  private func draw(_ code: UInt16, metrics: CellMetrics, style: GlyphStyle) -> [UInt8] {
    let width = Int((3 * metrics.cellWidth * Self.scale).rounded(.up))
    let height = Int((3 * metrics.cellHeight * Self.scale).rounded(.up))
    guard
      let ctx = CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)
    else {
      preconditionFailure("could not create a \(width) x \(height) gray bitmap")
    }
    ctx.setFillColor(gray: 1, alpha: 1)
    ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
    // GlyphPainter draws into a flipped context, as the views hand it.
    ctx.translateBy(x: 0, y: CGFloat(height))
    ctx.scaleBy(x: Self.scale, y: -Self.scale)
    GlyphPainter(metrics: metrics).drawGlyph(
      ctx, text: text(code), x: metrics.cellWidth, y: metrics.cellHeight,
      fg: CGColor(gray: 0, alpha: 1), style: style)
    return pixels(of: ctx, bytesPerPixel: 1)
  }

  /// `ctx`'s pixels, row by row from the top, without row padding.
  private func pixels(of ctx: CGContext, bytesPerPixel: Int) -> [UInt8] {
    guard let data = ctx.data else { preconditionFailure("the bitmap has no backing data") }
    let rowBytes = ctx.width * bytesPerPixel
    let base = data.bindMemory(to: UInt8.self, capacity: ctx.bytesPerRow * ctx.height)
    var result: [UInt8] = []
    result.reserveCapacity(rowBytes * ctx.height)
    for row in 0..<ctx.height {
      result.append(
        contentsOf: UnsafeBufferPointer(start: base + row * ctx.bytesPerRow, count: rowBytes))
    }
    return result
  }

  /// How a drawing differs from a reference of the same size.
  private struct Comparison {
    let referenceInk: Int
    let differing: Int
    let matches: Bool
    let differs: Bool

    var counts: String { "\(differing) of the reference's \(referenceInk) ink pixels differ" }
  }

  private func compare(_ drawing: [UInt8], to reference: [UInt8]) -> Comparison {
    precondition(drawing.count == reference.count, "compared drawings differ in size")
    let ink = reference.filter { $0 < Self.inkLevel }.count
    var differing = 0
    for (a, b) in zip(drawing, reference) where abs(Int(a) - Int(b)) > Self.pixelTolerance {
      differing += 1
    }
    let allowed = max(Self.matchFloor, Int(Double(ink) * Self.matchFraction))
    return Comparison(
      referenceInk: ink, differing: differing, matches: differing <= allowed,
      differs: Double(differing) >= Double(ink) * Self.differFraction)
  }

  /// The folder icon drawn alone in `green` on `background`, at the middle cell of an sRGB
  /// bitmap at `scale`, and its green pixels at or right of the cell's left edge and above its
  /// bottom: `right` within the cell's rows and right of the cell, `kept` the rest.
  ///
  /// In the grid, the row below covers what lies below the cell under either paint order, and
  /// the next cell's background covers `right` only when it is filled after this glyph.
  private func greenRegions(
    metrics: CellMetrics, background: CGColor, scale: CGFloat
  ) -> (kept: Int, right: Int) {
    let width = Int((3 * metrics.cellWidth * scale).rounded(.up))
    let height = Int((3 * metrics.cellHeight * scale).rounded(.up))
    guard let space = CGColorSpace(name: CGColorSpace.sRGB),
      let ctx = CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
        space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else {
      preconditionFailure("could not create a \(width) x \(height) sRGB bitmap")
    }
    ctx.setFillColor(background)
    ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
    ctx.translateBy(x: 0, y: CGFloat(height))
    ctx.scaleBy(x: scale, y: -scale)
    GlyphPainter(metrics: metrics).drawGlyph(
      ctx, text: text(Self.folder), x: metrics.cellWidth, y: metrics.cellHeight,
      fg: Self.green.cgColor, style: GlyphPainter.plainStyle)
    let rgba = pixels(of: ctx, bytesPerPixel: 4)
    var kept = 0
    var right = 0
    for row in 0..<height {
      let y = (CGFloat(row) + 0.5) / scale - metrics.cellHeight
      guard y < metrics.cellHeight else { continue }
      for col in 0..<width {
        let x = (CGFloat(col) + 0.5) / scale - metrics.cellWidth
        guard x >= 0 else { continue }
        let i = (row * width + col) * 4
        let colour = NSColor(
          srgbRed: CGFloat(rgba[i]) / 255, green: CGFloat(rgba[i + 1]) / 255,
          blue: CGFloat(rgba[i + 2]) / 255, alpha: 1)
        guard colorDistance(colour, Self.green) <= Self.greenTolerance else { continue }
        if y >= 0, x >= metrics.cellWidth {
          right += 1
        } else {
          kept += 1
        }
      }
    }
    return (kept, right)
  }
}
