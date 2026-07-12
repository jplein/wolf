//
//  CompletionPopupView.swift
//  The native completion popup (`ext_popupmenu`): a dedicated overlay `NSView`
//  that lists the completion items vertically, highlighting the selected one,
//  rendered directly ABOVE the command line and anchored at the cmdline byte
//  column (OD9). A projection of the core's `Snapshot.pum` read across the FFI as
//  an `FfiPum`.
//
//  Per OD2 each row shows `word` primarily with `kind` as a small trailing hint;
//  `menu` / `info` are unused this pass. This AppKit glue is compile-verified +
//  manual-smoke only.
//

import AppKit
import CoreText
import WolfFFI
import WolfKeyMap

/// The completion popup overlay. Paints up to `maxVisibleRows` items in a
/// vertical list; the selected row gets a highlighted background. Hidden (via
/// `isHidden`) whenever the popup is not visible.
final class CompletionPopupView: NSView {
    /// Maximum rows shown at once; a longer list is scrolled to keep the selected
    /// row visible (v1 caps the visible window rather than adding a scrollbar).
    private static let maxVisibleRows = 10
    /// Horizontal padding inside each row, in points.
    private static let rowPadding: CGFloat = 6
    /// Extra vertical padding per row beyond the cell height, in points.
    private static let rowExtraHeight: CGFloat = 2

    private var metrics: CellMetrics

    // -- Latest projected state (set by `update`) -----------------------------
    private var words: [String] = []
    private var kinds: [String] = []
    private var selected: Int = -1

    /// Builds the overlay against the measured `metrics`. Starts hidden.
    init(metrics: CellMetrics) {
        self.metrics = metrics
        super.init(frame: .zero)
        wantsLayer = true
        isHidden = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("CompletionPopupView is not constructed from a nib")
    }

    override var isFlipped: Bool { true }

    /// Re-measures against `newMetrics` (a per-window font-size change).
    func applyMetrics(_ newMetrics: CellMetrics) {
        metrics = newMetrics
        needsDisplay = true
    }

    /// The per-row height (cell height plus a little padding), in points.
    var rowHeight: CGFloat { ceil(metrics.cellHeight) + Self.rowExtraHeight }

    /// The number of rows actually shown (capped at `maxVisibleRows`).
    var visibleRowCount: Int { min(words.count, Self.maxVisibleRows) }

    /// The preferred overlay height for the current item count.
    var preferredHeight: CGFloat { CGFloat(visibleRowCount) * rowHeight }

    /// The preferred overlay width: the widest row's text plus padding.
    var preferredWidth: CGFloat {
        let font = overlayFont(metrics: metrics)
        let attrs: [NSAttributedString.Key: Any] = [.font: font]
        var maxWidth: CGFloat = 0
        for (i, word) in words.enumerated() {
            let hint = kinds.indices.contains(i) && !kinds[i].isEmpty ? "  \(kinds[i])" : ""
            let w = ((word + hint) as NSString).size(withAttributes: attrs).width
            maxWidth = max(maxWidth, w)
        }
        return maxWidth + 2 * Self.rowPadding
    }

    /// Projects the latest `FfiPum` into the view: stores the item words/kinds and
    /// the selected index, toggles visibility on `visible()`, and requests a
    /// repaint. Returns whether the popup is visible.
    @discardableResult
    func update(_ pum: FfiPum) -> Bool {
        let visible = pum.visible()
        let items = pum.items()
        words = (0..<items.len()).map { items[$0].word().toString() }
        kinds = (0..<items.len()).map { items[$0].kind().toString() }
        selected = Int(pum.selected())
        isHidden = !visible
        needsDisplay = true
        return visible
    }

    /// The scroll offset (first visible row index) chosen so the selected row is
    /// within the visible window.
    private var firstVisibleRow: Int {
        guard words.count > Self.maxVisibleRows, selected >= 0 else { return 0 }
        // Keep the selected row within [first, first + maxVisibleRows).
        let last = Self.maxVisibleRows - 1
        if selected <= last { return 0 }
        return min(selected - last, words.count - Self.maxVisibleRows)
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.setShouldAntialias(true)
        ctx.setShouldSmoothFonts(false)

        // Panel background.
        ctx.setFillColor(NSColor.windowBackgroundColor.cgColor)
        ctx.fill(bounds)
        ctx.setStrokeColor(NSColor.separatorColor.cgColor)
        ctx.setLineWidth(1)
        ctx.stroke(bounds.insetBy(dx: 0.5, dy: 0.5))

        let font = overlayFont(metrics: metrics)
        let rh = rowHeight
        let first = firstVisibleRow

        for visualRow in 0..<visibleRowCount {
            let itemIndex = first + visualRow
            guard itemIndex < words.count else { break }
            let rowRect = CGRect(x: 0, y: CGFloat(visualRow) * rh, width: bounds.width, height: rh)

            let textColor: NSColor
            if itemIndex == selected {
                ctx.setFillColor(NSColor.selectedContentBackgroundColor.cgColor)
                ctx.fill(rowRect)
                textColor = .selectedMenuItemTextColor
            } else {
                textColor = .labelColor
            }

            let hint = kinds.indices.contains(itemIndex) && !kinds[itemIndex].isEmpty
                ? "  \(kinds[itemIndex])" : ""
            let label = words[itemIndex] + hint
            let attrs: [NSAttributedString.Key: Any] = [
                .font: font,
                .foregroundColor: textColor,
            ]
            (label as NSString).draw(
                at: CGPoint(x: Self.rowPadding, y: rowRect.minY + Self.rowExtraHeight / 2),
                withAttributes: attrs)
        }
    }
}
