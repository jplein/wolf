// Monospaced cell geometry measured from a CoreText font and the user config.

import CoreGraphics
import CoreText
import Foundation
import WolfKeyMap

/// Per-cell geometry plus the base font used to paint the grid. A monospace cell
/// is `cellWidth` wide and `cellHeight` tall; `baseline` is the text baseline
/// measured from the cell's TOP edge (the view is flipped). The macOS analog of
/// wolf-gnome's `render::CellMetrics`.
struct CellMetrics {
  /// Cell advance width, in points.
  let cellWidth: CGFloat
  /// Cell height (line-height-driven; see `CellMetricsMath.cellHeight`), in points.
  let cellHeight: CGFloat
  /// Baseline offset from the cell top, in points.
  let baseline: CGFloat
  /// The base (regular) CoreText font. Bold/italic/boldItalic variants are
  /// derived from it and cached alongside; the renderer selects a face per cell
  /// via `font(for:)`. Per-cell text attributes (bold/italic/underline/
  /// undercurl/strikethrough) resolved and packed by wolf-ffi ARE now consumed in
  /// painting: `GlyphStyleResolver` maps a cell's flags to a `GlyphFace` (drawn
  /// with the matching cached font) plus underline/strikethrough strokes.
  let font: CTFont
  /// The bold face (falls back to `font` when CoreText resolves no distinct face).
  let boldFont: CTFont
  /// The italic face (falls back to `font` when CoreText resolves no distinct face).
  let italicFont: CTFont
  /// The bold+italic face (falls back to `font` when no distinct face resolves).
  let boldItalicFont: CTFont

  /// Fallback monospaced family when the configured family cannot be resolved.
  private static let fallbackFamily = "Menlo"

  /// Returns the cached CoreText font for `face`. All faces share the regular
  /// face's advance width (the grid is fixed-cell), so they paint into the same
  /// slot; a variant that CoreText could not resolve falls back to the regular
  /// face.
  func font(for face: GlyphFace) -> CTFont {
    switch face {
    case .regular: return font
    case .bold: return boldFont
    case .italic: return italicFont
    case .boldItalic: return boldItalicFont
    }
  }

  /// Measures monospace cell metrics for the configured font family at the
  /// configured point size, with line-height-driven cell height + recentered
  /// baseline.
  ///
  /// The point size is the `WOLF_FONT_PT` override when set (a positive number),
  /// else `config.font_size`. The family is `config.font_family`, falling back to
  /// Menlo when CoreText cannot resolve it (compared case-insensitively). Cell
  /// height and baseline come from the pure `CellMetricsMath` seam using
  /// `config.line_height` (a percentage of the point size).
  static func measured(config: FfiConfig) -> CellMetrics {
    measured(config: config, pointSize: defaultPointSize(config: config))
  }

  /// Measures monospace cell metrics for the configured font family at an
  /// EXPLICIT `pointSize` (the per-window View-menu font-size relayout path),
  /// with line-height-driven cell height + recentered baseline.
  ///
  /// Identical to [`measured(config:)`](measured(config:)) except the point size
  /// is passed in rather than resolved from `WOLF_FONT_PT` / `config.font_size`,
  /// so `WolfWindowController` can re-measure at a stepped size while keeping the
  /// configured family + line-height.
  static func measured(config: FfiConfig, pointSize: CGFloat) -> CellMetrics {
    let size = pointSize
    let font = makeFont(family: config.font_family().toString(), size: size)
    let boldFont = makeVariant(base: font, size: size, addingTraits: .traitBold)
    let italicFont = makeVariant(base: font, size: size, addingTraits: .traitItalic)
    let boldItalicFont = makeVariant(
      base: font, size: size, addingTraits: [.traitBold, .traitItalic])

    let ascent = CTFontGetAscent(font)
    let descent = CTFontGetDescent(font)
    let leading = CTFontGetLeading(font)

    // Advance width of a representative monospace glyph ('M').
    let advance = monospaceAdvance(font)

    let cellHeight = CellMetricsMath.cellHeight(
      fontSize: Double(size),
      lineHeightPercent: config.line_height(),
      ascent: Double(ascent),
      descent: Double(descent),
      leading: Double(leading)
    )
    let baseline = CellMetricsMath.baseline(
      cellHeight: cellHeight,
      ascent: Double(ascent),
      descent: Double(descent)
    )

    return CellMetrics(
      cellWidth: advance,
      cellHeight: CGFloat(cellHeight),
      baseline: CGFloat(baseline),
      font: font,
      boldFont: boldFont,
      italicFont: italicFont,
      boldItalicFont: boldItalicFont
    )
  }

  /// Derives a variant of `base` with `addingTraits` (bold and/or italic)
  /// applied via CoreText symbolic traits. Returns the trait-matched font ONLY
  /// when CoreText resolves a DISTINCT face carrying the requested traits; if the
  /// copy fails or the resolved face does not actually carry the traits (no such
  /// face in the family), it falls back to `base` so the glyph still paints (as
  /// regular) rather than dropping.
  private static func makeVariant(
    base: CTFont, size: CGFloat, addingTraits: CTFontSymbolicTraits
  ) -> CTFont {
    guard
      let variant = CTFontCreateCopyWithSymbolicTraits(
        base, size, nil, addingTraits, addingTraits)
    else {
      return base
    }
    // Confirm CoreText actually applied the requested traits (a family with no
    // such face yields a copy WITHOUT them → treat as no distinct face).
    let resolved = CTFontGetSymbolicTraits(variant)
    guard resolved.intersection(addingTraits) == addingTraits else {
      return base
    }
    return variant
  }

  /// Reads `WOLF_FONT_PT` (a positive number) or returns the config's font size.
  ///
  /// The per-window font-size default (also the Reset target), so
  /// `WolfWindowController` seeds its `defaultPointSize` from the SAME resolution
  /// the shared launch metrics used.
  static func defaultPointSize(config: FfiConfig) -> CGFloat {
    if let raw = ProcessInfo.processInfo.environment["WOLF_FONT_PT"],
      let value = Double(raw), value > 0
    {
      return CGFloat(value)
    }
    return CGFloat(config.font_size())
  }

  /// Creates the base font for `family`, falling back to Menlo when CoreText
  /// resolves a DIFFERENT family (compared case-insensitively so e.g.
  /// "jetbrains mono" still matches "JetBrains Mono").
  private static func makeFont(family: String, size: CGFloat) -> CTFont {
    let preferred = CTFontCreateWithName(family as CFString, size, nil)
    let actual = CTFontCopyFamilyName(preferred) as String
    if actual.caseInsensitiveCompare(family) == .orderedSame {
      return preferred
    }
    return CTFontCreateWithName(fallbackFamily as CFString, size, nil)
  }

  /// Returns the advance width of the glyph for 'M' (exact for a monospace font).
  private static func monospaceAdvance(_ font: CTFont) -> CGFloat {
    var ch: UniChar = UnicodeScalar("M").value.toUInt16
    var glyph = CGGlyph(0)
    guard CTFontGetGlyphsForCharacters(font, &ch, &glyph, 1) else {
      return ceil(CTFontGetSize(font))
    }
    var advance = CGSize.zero
    _ = CTFontGetAdvancesForGlyphs(font, .horizontal, &glyph, &advance, 1)
    return ceil(advance.width)
  }
}

extension UInt32 {
  /// Narrows a BMP scalar value to `UniChar`, for CoreText glyph lookups.
  fileprivate var toUInt16: UInt16 { UInt16(truncatingIfNeeded: self) }
}
