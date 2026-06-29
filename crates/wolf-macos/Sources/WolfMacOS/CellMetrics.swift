//
//  CellMetrics.swift
//  Monospaced cell geometry measured from a CoreText font.
//

import CoreGraphics
import CoreText
import Foundation

/// Per-cell geometry plus the base font used to paint the grid. A monospace cell
/// is `cellWidth` wide and `cellHeight` tall; `baseline` is the text baseline
/// measured from the cell's TOP edge (the view is flipped). The macOS analog of
/// wolf-gnome's `render::CellMetrics`.
struct CellMetrics {
    /// Cell advance width, in points.
    let cellWidth: CGFloat
    /// Cell height (ascent + descent + leading), in points.
    let cellHeight: CGFloat
    /// Baseline offset from the cell top, in points.
    let baseline: CGFloat
    /// The base CoreText font, used directly for all glyphs. Per-cell text
    /// attributes (bold/italic/underline/undercurl/strikethrough) are resolved and
    /// packed by wolf-ffi but are NOT yet consumed in painting: this increment
    /// paints per-cell fg/bg plus a mode-aware cursor only. Glyph attribute
    /// rendering (font variants, underlines) is deferred to a future increment.
    let font: CTFont

    /// The default font point size when `WOLF_FONT_PT` is unset.
    private static let defaultPointSize: CGFloat = 13
    /// Preferred monospaced family, with a Menlo fallback (parity with gnome).
    private static let preferredFamily = "SF Mono"
    private static let fallbackFamily = "Menlo"

    /// Measures monospace cell metrics for the preferred family at the configured
    /// point size (`WOLF_FONT_PT` overrides the default), falling back to Menlo if
    /// the preferred family is unavailable.
    static func measured() -> CellMetrics {
        let size = pointSizeFromEnv()
        let font = makeFont(size: size)

        let ascent = CTFontGetAscent(font)
        let descent = CTFontGetDescent(font)
        let leading = CTFontGetLeading(font)

        // Advance width of a representative monospace glyph ('M').
        let advance = monospaceAdvance(font)

        return CellMetrics(
            cellWidth: advance,
            cellHeight: ceil(ascent + descent + leading),
            baseline: ceil(ascent),
            font: font
        )
    }

    /// Reads `WOLF_FONT_PT` (a positive number) or returns the default size.
    private static func pointSizeFromEnv() -> CGFloat {
        if let raw = ProcessInfo.processInfo.environment["WOLF_FONT_PT"],
            let value = Double(raw), value > 0
        {
            return CGFloat(value)
        }
        return defaultPointSize
    }

    /// Creates the base font, preferring SF Mono and falling back to Menlo.
    private static func makeFont(size: CGFloat) -> CTFont {
        let preferred = CTFontCreateWithName(preferredFamily as CFString, size, nil)
        let actual = CTFontCopyFamilyName(preferred) as String
        if actual == preferredFamily {
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
