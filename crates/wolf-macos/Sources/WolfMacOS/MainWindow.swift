//
//  MainWindow.swift
//  The single editor window: a horizontal split of the file-tree sidebar (left)
//  and the editor grid (right), sized from cell metrics × initial grid size.
//

import AppKit

/// The main "Wolf" window. Its content view is a horizontal `NSSplitView` with the
/// `FileTreeView` sidebar on the left and the `GridView` editor on the right. The
/// window's content width is widened by the sidebar pane width + the divider so the
/// editor keeps its full `cols` × `rows` cell size (and the OCR-checked grid is
/// undisturbed).
final class MainWindow: NSWindow {
    /// Fixed width of the file-tree sidebar pane in points.
    static let treePaneWidth: CGFloat = 240

    /// Builds the window around `treeView` (left) and `gridView` (right), sizing the
    /// editor pane to `cols` × `rows` cells.
    convenience init(
        treeView: FileTreeView, gridView: GridView, metrics: CellMetrics, cols: Int, rows: Int
    ) {
        let splitView = NSSplitView()
        splitView.isVertical = true
        splitView.dividerStyle = .thin
        let divider = splitView.dividerThickness

        let editorWidth = CGFloat(cols) * metrics.cellWidth
        let height = CGFloat(rows) * metrics.cellHeight
        let totalWidth = Self.treePaneWidth + divider + editorWidth
        let contentRect = NSRect(x: 0, y: 0, width: totalWidth, height: height)

        self.init(
            contentRect: contentRect,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        title = "Wolf"

        splitView.frame = contentRect
        treeView.frame = NSRect(x: 0, y: 0, width: Self.treePaneWidth, height: height)
        gridView.frame = NSRect(x: 0, y: 0, width: editorWidth, height: height)
        splitView.addArrangedSubview(treeView)
        splitView.addArrangedSubview(gridView)
        // Pin the divider so the editor pane is exactly `editorWidth` at startup.
        splitView.setPosition(Self.treePaneWidth, ofDividerAt: 0)

        contentView = splitView
        makeFirstResponder(gridView)
    }
}
