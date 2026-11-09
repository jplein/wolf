// Draws one cell's glyph and decorations with CoreText, for the grid and terminal painters.

import AppKit
import CoreGraphics
import CoreText
import WolfFFI
import WolfKeyMap

/// Paints one cell's glyph and its underline and strikethrough at the cell geometry of
/// `metrics`, into a flipped context.
struct GlyphPainter {
  let metrics: CellMetrics

  /// A plain regular glyph style (no face variant, no decorations) used for the
  /// glyph redrawn under a block cursor — avoids a bold face overflowing the
  /// reverse-video cell.
  static let plainStyle = GlyphStyle(
    face: .regular, underline: nil, underlineColor: FfiColor(rgb: 0, set: false),
    strikethrough: false, strikethroughColor: FfiColor(rgb: 0, set: false))

  /// Underline stroke offset below the baseline, in points; it also places the
  /// undercurl wave's centre line.
  private static let underlineOffset: CGFloat = 2
  /// Undercurl wave amplitude (centre to crest), in points.
  private static let undercurlAmplitude: CGFloat = 1
  /// Undercurl wave period (crest to crest), in points.
  private static let undercurlPeriod: CGFloat = 4
  /// Polyline segments per undercurl period; 8 keeps the sampled sine smooth at
  /// the 1-point stroke.
  private static let undercurlSamplesPerPeriod: CGFloat = 8
  /// Underline / strikethrough stroke thickness, in points.
  private static let decorationThickness: CGFloat = 1
  /// Strikethrough height above the baseline as a fraction of the cell height
  /// (roughly the x-height midline).
  private static let strikethroughRatio: CGFloat = 0.3

  /// Draws one cell's glyph in `fg` using CoreText at the cell baseline, in the
  /// face `style.face` selects, and strokes any underline (straight or undercurl
  /// wave) / strikethrough decorations MANUALLY (in `style`'s resolved colors) —
  /// `CTLineDraw` paints decorations only in the foreground color, ignores AppKit
  /// decoration-color attributes and has no wavy underline style, so the special
  /// (underline) color and the wave must be stroked by hand.
  func drawGlyph(
    _ ctx: CGContext, text: String, x: CGFloat, y: CGFloat, fg: CGColor,
    style: GlyphStyle
  ) {
    let attrs: [NSAttributedString.Key: Any] = [
      .font: metrics.font(for: style.face),
      .foregroundColor: fg,
    ]
    let line = CTLineCreateWithAttributedString(
      NSAttributedString(string: text, attributes: attrs))
    ctx.saveGState()
    // The view is flipped; flip the text matrix so glyphs are upright.
    ctx.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
    ctx.textPosition = CGPoint(x: x, y: y + metrics.baseline)
    CTLineDraw(line, ctx)
    ctx.restoreGState()

    drawDecorations(ctx, x: x, y: y, style: style)
  }

  /// Strokes the underline / strikethrough for one cell in the resolved
  /// decoration colors, drawn separately from the glyph because CoreText
  /// ignores decoration-color attributes and has no wavy underline style.
  ///
  /// A straight underline and the strikethrough are horizontal lines. An
  /// undercurl is a sine wave (`undercurlAmplitude`, `undercurlPeriod`) centred
  /// on the underline position, with two rules:
  ///   - Its phase is a function of the grid-local `x`, never of the cell's
  ///     start. Cells are drawn one at a time, and this is what joins
  ///     consecutive undercurled cells into one continuous wave even when
  ///     `cellWidth` is not a multiple of the period.
  ///   - When the trough would fall below the cell's bottom edge (as at a
  ///     100 % `line_height`), the centre line is lifted to keep it inside: the
  ///     row below paints its background after this row and would overpaint it.
  private func drawDecorations(
    _ ctx: CGContext, x: CGFloat, y: CGFloat, style: GlyphStyle
  ) {
    guard style.underline != nil || style.strikethrough else { return }
    let width = metrics.cellWidth
    ctx.saveGState()
    ctx.setLineWidth(Self.decorationThickness)
    if let underline = style.underline {
      let uy = y + metrics.baseline + Self.underlineOffset
      ctx.setStrokeColor(
        WolfColor.cgColor(style.underlineColor, fallback: WolfColor.fallbackFg))
      switch underline {
      case .straight:
        ctx.move(to: CGPoint(x: x, y: uy))
        ctx.addLine(to: CGPoint(x: x + width, y: uy))
      case .wave:
        addUndercurlPath(ctx, x: x, width: width, centre: uy, cellBottom: y + metrics.cellHeight)
      }
      ctx.strokePath()
    }
    if style.strikethrough {
      let sy = y + metrics.cellHeight * Self.strikethroughRatio
      ctx.setStrokeColor(
        WolfColor.cgColor(style.strikethroughColor, fallback: WolfColor.fallbackFg))
      ctx.move(to: CGPoint(x: x, y: sy))
      ctx.addLine(to: CGPoint(x: x + width, y: sy))
      ctx.strokePath()
    }
    ctx.restoreGState()
  }

  /// Adds the undercurl wave across `[x, x + width]` to the current path, under the
  /// phase and lift rules `drawDecorations` documents.
  private func addUndercurlPath(
    _ ctx: CGContext, x: CGFloat, width: CGFloat, centre: CGFloat, cellBottom: CGFloat
  ) {
    let amplitude = Self.undercurlAmplitude
    let lowest = cellBottom - amplitude - Self.decorationThickness / 2
    let cy = min(centre, lowest)
    let step = Self.undercurlPeriod / Self.undercurlSamplesPerPeriod
    let segments = max(1, Int((width / step).rounded(.up)))
    func point(_ px: CGFloat) -> CGPoint {
      CGPoint(x: px, y: cy + amplitude * sin(2 * .pi * px / Self.undercurlPeriod))
    }
    ctx.move(to: point(x))
    for i in 1...segments {
      ctx.addLine(to: point(min(x + CGFloat(i) * step, x + width)))
    }
  }
}
