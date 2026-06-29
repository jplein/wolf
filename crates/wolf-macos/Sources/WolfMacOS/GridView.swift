//
//  GridView.swift
//  The AppKit view that paints wolf-core's character grid and forwards keys.
//

import AppKit
import CoreGraphics
import CoreText
import WolfFFI
import WolfKeyMap

/// The editor surface: an `NSView` that reads a frame from the Rust core on each
/// repaint, fills each cell's background, draws its glyph via CoreText, and
/// overlays a mode-aware cursor. Keystrokes are translated by the pure
/// `WolfKeyMap` seam and forwarded to the core; resizes are mapped from points to
/// a (cols, rows) grid size. The macOS analog of wolf-gnome's `DrawingArea` glue.
final class GridView: NSView {
    private let handle: WolfCoreHandle
    private let metrics: CellMetrics

    /// Creates the view bound to a live core `handle` and measured `metrics`.
    init(handle: WolfCoreHandle, metrics: CellMetrics) {
        self.handle = handle
        self.metrics = metrics
        super.init(frame: .zero)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("GridView is not constructed from a nib")
    }

    /// Top-left origin so grid row/col map directly to (x, y) in points.
    override var isFlipped: Bool { true }

    /// The view takes key input directly (there is one editor surface).
    override var acceptsFirstResponder: Bool { true }

    /// The grid size, in cells, that the current view bounds afford.
    var gridSize: (cols: UInt16, rows: UInt16) {
        let cols = max(1, Int(bounds.width / metrics.cellWidth))
        let rows = max(1, Int(bounds.height / metrics.cellHeight))
        return (UInt16(truncatingIfNeeded: cols), UInt16(truncatingIfNeeded: rows))
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        let frame = handle.read_frame()

        let cols = Int(frame.cols())
        let rows = Int(frame.rows())
        let cells = frame.cells()
        let cellW = metrics.cellWidth
        let cellH = metrics.cellHeight

        for row in 0..<rows {
            for col in 0..<cols {
                let index = row * cols + col
                guard index < cells.len() else { continue }
                let cell = cells[index]
                let x = CGFloat(col) * cellW
                let y = CGFloat(row) * cellH

                let bg = WolfColor.cgColor(cell.bg(), fallback: WolfColor.fallbackBg)
                ctx.setFillColor(bg)
                ctx.fill(CGRect(x: x, y: y, width: cellW, height: cellH))

                let text = cell.text().toString()
                if !text.isEmpty {
                    let fg = WolfColor.cgColor(cell.fg(), fallback: WolfColor.fallbackFg)
                    drawGlyph(ctx, text: text, x: x, y: y, fg: fg)
                }
            }
        }

        if frame.cursor_visible() {
            drawCursor(ctx, frame: frame, cells: cells, cols: cols)
        }
    }

    override func keyDown(with event: NSEvent) {
        let flags = event.modifierFlags
        guard
            let ev = WolfKeyMap.translate(
                keyCode: event.keyCode,
                characters: event.characters ?? "",
                charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "",
                ctrl: flags.contains(.control),
                shift: flags.contains(.shift),
                alt: flags.contains(.option),
                meta: flags.contains(.command)
            )
        else { return }
        handle.send_key(ev)
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        requestResize()
    }

    override func viewDidEndLiveResize() {
        super.viewDidEndLiveResize()
        requestResize()
    }

    /// Maps the current bounds to a grid size and requests it from the core.
    private func requestResize() {
        let size = gridSize
        handle.resize(size.cols, size.rows)
    }

    /// Draws one cell's glyph in `fg` using CoreText at the cell baseline.
    private func drawGlyph(_ ctx: CGContext, text: String, x: CGFloat, y: CGFloat, fg: CGColor) {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: metrics.font,
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
    }

    /// Overlays the mode-aware cursor (reverse-video colors resolved in Rust).
    private func drawCursor(
        _ ctx: CGContext, frame: FfiFrame, cells: RustVec<FfiCell>, cols: Int
    ) {
        let row = Int(frame.cursor_row())
        let col = Int(frame.cursor_col())
        let shape = CursorShape(rawValue: frame.cursor_shape()) ?? .block
        let rect = Cursor.rect(
            shape: shape,
            cellX: CGFloat(col) * metrics.cellWidth,
            cellY: CGFloat(row) * metrics.cellHeight,
            cellW: metrics.cellWidth,
            cellH: metrics.cellHeight,
            cellPercentage: frame.cursor_cell_percentage()
        )

        let fill = WolfColor.cgColor(frame.cursor_fill(), fallback: WolfColor.fallbackFg)
        ctx.setFillColor(fill)
        ctx.fill(rect)

        // Redraw the glyph under the cursor in the readable (glyph) color, for a
        // block cursor only (bar cursors leave the glyph visible).
        let index = row * cols + col
        if shape == .block, index >= 0, index < cells.len() {
            let text = cells[index].text().toString()
            if !text.isEmpty {
                let glyph = WolfColor.cgColor(frame.cursor_glyph(), fallback: WolfColor.fallbackBg)
                drawGlyph(
                    ctx, text: text,
                    x: CGFloat(col) * metrics.cellWidth,
                    y: CGFloat(row) * metrics.cellHeight,
                    fg: glyph)
            }
        }
    }
}
