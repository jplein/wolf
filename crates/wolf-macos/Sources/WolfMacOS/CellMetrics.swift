//
//  CellMetrics.swift
//  Monospaced cell geometry measured from a CoreText font + the user config.
//

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
    /// The base CoreText font, used directly for all glyphs. Per-cell text
    /// attributes (bold/italic/underline/undercurl/strikethrough) are resolved and
    /// packed by wolf-ffi but are NOT yet consumed in painting: this increment
    /// paints per-cell fg/bg plus a mode-aware cursor only. Glyph attribute
    /// rendering (font variants, underlines) is deferred to a future increment.
    let font: CTFont

    /// Fallback monospaced family when the configured family cannot be resolved.
    private static let fallbackFamily = "Menlo"

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
        let size = pointSize(config: config)
        let font = makeFont(family: config.font_family().toString(), size: size)

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
            font: font
        )
    }

    /// Reads `WOLF_FONT_PT` (a positive number) or returns the config's font size.
    private static func pointSize(config: FfiConfig) -> CGFloat {
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
