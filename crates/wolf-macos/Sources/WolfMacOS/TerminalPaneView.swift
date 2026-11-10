// Paints one terminal session's frame over a terminal pane's content area.

import AppKit
import CoreGraphics
import CoreText
import WolfFFI
import WolfKeyMap

/// A subview of `PaneContainerView` over a terminal pane's content area, painting its
/// session's frame and invalidating only the rows a new frame damaged. Events pass
/// through to the container, which routes them to the session.
final class TerminalPaneView: NSView {
  /// The session this view paints.
  let session: UInt64
  private var metrics: CellMetrics
  /// The latest frame `update` stored, which `draw` paints.
  private var terminalFrame: FfiTerminalFrame?
  /// The paint key of the last `update`; a change repaints every row.
  private var paintKey: TerminalDamage.PaintKey?
  /// The frame version painted last, `nil` when nothing is.
  private var paintedVersion: UInt64?
  /// Where the cursor of the stored frame paints.
  private var cursorMark: TerminalDamage.CursorMark?

  /// An input method's uncommitted text, painted at the cursor in place of the cursor.
  var markedText = "" {
    didSet {
      guard markedText != oldValue else { return }
      if let row = cursorMark?.row ?? terminalFrame.map({ Int($0.cursor_row()) }) {
        setNeedsDisplay(rowRect(row))
      }
    }
  }

  init(session: UInt64, metrics: CellMetrics) {
    self.session = session
    self.metrics = metrics
    super.init(frame: .zero)
    // The SDK default is false, and draw(_:) fills its dirty rect, which can then reach
    // past the bounds and cover the container's Neovim panes.
    clipsToBounds = true
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("TerminalPaneView is not constructed from a nib")
  }

  override var isFlipped: Bool { true }
  override var isOpaque: Bool { true }

  /// Every event reaches the container, which routes it to the session.
  override func hitTest(_ point: NSPoint) -> NSView? {
    nil
  }

  /// Takes new cell metrics and repaints everything.
  func applyMetrics(_ newMetrics: CellMetrics) {
    metrics = newMetrics
    paintKey = nil
    needsDisplay = true
  }

  /// The cursor cell in this view's coordinates, `nil` with no visible cursor, for the
  /// input method's candidate window.
  var cursorRect: CGRect? {
    guard let frame = terminalFrame, frame.has_cursor() else { return nil }
    return cellRect(col: Int(frame.cursor_col()), row: Int(frame.cursor_row()), span: 1)
  }

  /// Stores `frame` and invalidates the rows it damaged since the version painted
  /// last, plus the rows the cursor left and entered. Nothing is invalidated when
  /// nothing changed.
  func update(_ frame: FfiTerminalFrame, scale: CGFloat) {
    let rows = Int(frame.rows())
    let key = TerminalDamage.PaintKey(
      cols: Int(frame.cols()), rows: rows, defaultBg: frame.default_bg().rgb,
      cellWidth: Double(metrics.cellWidth), cellHeight: Double(metrics.cellHeight),
      scale: Double(scale))
    let painted = TerminalDamage.effectivePaintedVersion(
      previous: paintKey, current: key, painted: paintedVersion)
    let mark = Self.cursorMark(of: frame)
    if painted == nil {
      needsDisplay = true
    } else {
      let versions = (0..<rows).map { frame.row_version(UInt16($0)) }
      let damaged = TerminalDamage.rowsToRepaint(rowVersions: versions, paintedVersion: painted)
      let cursor = TerminalDamage.cursorRows(previous: cursorMark, current: mark)
      for row in Set(damaged + cursor) {
        setNeedsDisplay(rowRect(row))
      }
    }
    terminalFrame = frame
    paintKey = key
    paintedVersion = frame.version()
    cursorMark = mark
  }

  /// The cursor mark of `frame`, `nil` for an absent or invisible cursor.
  private static func cursorMark(of frame: FfiTerminalFrame) -> TerminalDamage.CursorMark? {
    guard frame.has_cursor(), frame.cursor_visible() else { return nil }
    let row = frame.cursor_row()
    let col = Int(frame.cursor_col())
    let cells = frame.row_cells(row)
    let width = col < cells.len() ? cells[col].width() : TerminalCellStyle.Width.narrow
    return TerminalDamage.CursorMark(
      row: Int(row), col: col, span: TerminalCellStyle.cursorSpan(width: width),
      style: frame.cursor_style())
  }

  /// Row `row`'s strip: the full view width by one cell.
  private func rowRect(_ row: Int) -> CGRect {
    CGRect(
      x: 0, y: CGFloat(row) * metrics.cellHeight, width: bounds.width,
      height: metrics.cellHeight)
  }

  /// The rect of `span` cells from `col` in row `row`.
  private func cellRect(col: Int, row: Int, span: Int) -> CGRect {
    CGRect(
      x: CGFloat(col) * metrics.cellWidth, y: CGFloat(row) * metrics.cellHeight,
      width: CGFloat(span) * metrics.cellWidth, height: metrics.cellHeight)
  }

  override func draw(_ dirtyRect: NSRect) {
    guard let ctx = NSGraphicsContext.current?.cgContext else { return }
    // As the container: no font smoothing, which thickens light text on dark backgrounds.
    ctx.setShouldAntialias(true)
    ctx.setShouldSmoothFonts(false)
    guard let frame = terminalFrame else {
      ctx.setFillColor(WolfColor.fallbackBg)
      ctx.fill(dirtyRect)
      return
    }
    let defaultFg = frame.default_fg()
    let defaultBg = frame.default_bg()
    ctx.setFillColor(WolfColor.cgColor(defaultBg, fallback: WolfColor.fallbackBg))
    ctx.fill(dirtyRect)

    let rows = Int(frame.rows())
    let cellH = metrics.cellHeight
    let first = max(0, Int((dirtyRect.minY / cellH).rounded(.down)))
    let last = min(rows - 1, Int((dirtyRect.maxY / cellH).rounded(.up)))
    if first <= last {
      for row in first...last {
        ctx.saveGState()
        ctx.clip(to: rowRect(row))
        paintRow(row, of: frame, defaultFg: defaultFg, defaultBg: defaultBg, ctx: ctx)
        ctx.restoreGState()
      }
    }
    drawCursor(of: frame, defaultFg: defaultFg, defaultBg: defaultBg, ctx: ctx)
  }

  /// Paints row `row`: every cell's background first, so a spacer's background cannot
  /// overpaint the right half of a wide glyph, then the glyphs.
  private func paintRow(
    _ row: Int, of frame: FfiTerminalFrame, defaultFg: FfiColor, defaultBg: FfiColor,
    ctx: CGContext
  ) {
    let cells = frame.row_cells(UInt16(row))
    let selection: ClosedRange<Int>? =
      frame.row_has_selection(UInt16(row))
      ? Int(frame.row_selection_start(UInt16(row)))...Int(frame.row_selection_end(UInt16(row)))
      : nil
    let y = CGFloat(row) * metrics.cellHeight
    var glyphs: [(text: String, x: CGFloat, paint: TerminalCellPaint)] = []
    for (col, cell) in cells.enumerated() {
      let paint = TerminalCellStyle.resolve(
        fg: cell.fg(), bg: cell.bg(), flags: cell.flags(), underline: cell.underline(),
        width: cell.width(), selected: selection?.contains(col) ?? false,
        defaultFg: defaultFg, defaultBg: defaultBg)
      let x = CGFloat(col) * metrics.cellWidth
      ctx.setFillColor(WolfColor.cgColor(packed: paint.background))
      ctx.fill(CGRect(x: x, y: y, width: metrics.cellWidth, height: metrics.cellHeight))
      if paint.drawsGlyph {
        let text = cell.text().toString()
        if !text.isEmpty { glyphs.append((text, x, paint)) }
      }
    }
    let painter = GlyphPainter(metrics: metrics)
    for glyph in glyphs {
      let fg =
        WolfColor.cgColor(packed: glyph.paint.foreground)
        .copy(alpha: CGFloat(glyph.paint.foregroundAlpha)) ?? WolfColor.fallbackFg
      painter.drawGlyph(ctx, text: glyph.text, x: glyph.x, y: y, fg: fg, style: glyph.paint.style)
    }
  }

  /// Draws the cursor, or the marked text in its place. A block redraws the glyph it
  /// covers in the cursor's glyph colour; a hollow cursor strokes the block's outline.
  private func drawCursor(
    of frame: FfiTerminalFrame, defaultFg: FfiColor, defaultBg: FfiColor, ctx: CGContext
  ) {
    guard let mark = cursorMark else { return }
    if !markedText.isEmpty {
      drawMarkedText(at: mark, defaultFg: defaultFg, defaultBg: defaultBg, ctx: ctx)
      return
    }
    let colors = TerminalCellStyle.cursorColors(
      cursorColor: frame.cursor_color(), defaultFg: defaultFg, defaultBg: defaultBg)
    let cell = cellRect(col: mark.col, row: mark.row, span: mark.span)
    let shape = TerminalCursorShape(rawValue: mark.style) ?? .block
    ctx.saveGState()
    ctx.setFillColor(WolfColor.cgColor(packed: colors.fill))
    switch shape {
    case .hollow:
      ctx.setStrokeColor(WolfColor.cgColor(packed: colors.fill))
      ctx.setLineWidth(1)
      ctx.stroke(cell.insetBy(dx: 0.5, dy: 0.5))
    case .bar, .underline:
      ctx.fill(
        Cursor.rect(
          shape: shape == .bar ? .vertical : .horizontal, cellX: cell.minX, cellY: cell.minY,
          cellW: cell.width, cellH: cell.height, cellPercentage: 0))
    case .block:
      ctx.fill(cell)
      let cells = frame.row_cells(UInt16(mark.row))
      if mark.col < cells.len() {
        let text = cells[mark.col].text().toString()
        if !text.isEmpty {
          GlyphPainter(metrics: metrics).drawGlyph(
            ctx, text: text, x: cell.minX, y: cell.minY,
            fg: WolfColor.cgColor(packed: colors.glyph), style: GlyphPainter.plainStyle)
        }
      }
    }
    ctx.restoreGState()
  }

  /// Draws the input method's marked text at the cursor: one line in the default
  /// foreground over the default background, underlined.
  private func drawMarkedText(
    at mark: TerminalDamage.CursorMark, defaultFg: FfiColor, defaultBg: FfiColor,
    ctx: CGContext
  ) {
    let attrs: [NSAttributedString.Key: Any] = [
      .font: metrics.font,
      .foregroundColor: WolfColor.cgColor(defaultFg, fallback: WolfColor.fallbackFg),
    ]
    let line = CTLineCreateWithAttributedString(
      NSAttributedString(string: markedText, attributes: attrs))
    let width = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
    let origin = cellRect(col: mark.col, row: mark.row, span: 1).origin
    ctx.saveGState()
    ctx.setFillColor(WolfColor.cgColor(defaultBg, fallback: WolfColor.fallbackBg))
    ctx.fill(CGRect(x: origin.x, y: origin.y, width: width, height: metrics.cellHeight))
    ctx.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
    ctx.textPosition = CGPoint(x: origin.x, y: origin.y + metrics.baseline)
    CTLineDraw(line, ctx)
    let underlineY = origin.y + metrics.cellHeight - 1
    ctx.setStrokeColor(WolfColor.cgColor(defaultFg, fallback: WolfColor.fallbackFg))
    ctx.setLineWidth(1)
    ctx.move(to: CGPoint(x: origin.x, y: underlineY))
    ctx.addLine(to: CGPoint(x: origin.x + width, y: underlineY))
    ctx.strokePath()
    ctx.restoreGState()
  }
}
