//
//  MainWindow.swift
//  The single editor window, sized from cell metrics × initial grid size.
//

import AppKit

/// The main "Wolf" window. Its content view is a `GridView` sized to the initial
/// columns/rows times the measured cell metrics.
final class MainWindow: NSWindow {
    /// Builds the window around `gridView`, sizing it to `cols` × `rows` cells.
    convenience init(gridView: GridView, metrics: CellMetrics, cols: Int, rows: Int) {
        let contentRect = NSRect(
            x: 0,
            y: 0,
            width: CGFloat(cols) * metrics.cellWidth,
            height: CGFloat(rows) * metrics.cellHeight
        )
        self.init(
            contentRect: contentRect,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        title = "Wolf"
        gridView.frame = contentRect
        contentView = gridView
        makeFirstResponder(gridView)
    }
}
