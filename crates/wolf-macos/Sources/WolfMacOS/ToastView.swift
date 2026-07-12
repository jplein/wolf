//
//  ToastView.swift
//  The message toast (`ext_messages`): a dedicated overlay `NSView` pinned
//  BOTTOM-LEFT of the editor area, painting the current messages with their
//  styled chunks (colors RESOLVED in Rust against the highlight table — message
//  color is load-bearing). Height is capped at ~10 lines; a longer message is
//  scrollable, and scrolling WHILE HOVERING the toast is the single gesture that
//  does NOT dismiss it (OD8) — realized by the container forwarding that wheel to
//  `scrollBy(deltaY:)` here instead of to Neovim.
//
//  A projection of the core's `Snapshot.toast` read across the FFI as an
//  `FfiToast`. This AppKit glue is compile-verified + manual-smoke only.
//

import AppKit
import CoreText
import WolfFFI
import WolfKeyMap

/// One painted toast chunk: its text plus the foreground color already resolved
/// in Rust against the highlight table.
private struct ToastRenderChunk {
    let text: String
    let fg: CGColor
}

/// The message-toast overlay. Paints its lines top-to-bottom within a frame the
/// container caps at `maxVisibleLines`; extra lines scroll via `scrollBy`. Hidden
/// (via `isHidden`) whenever there is no message to show.
final class ToastView: NSView {
    /// Maximum number of message lines shown before the toast becomes scrollable.
    static let maxVisibleLines = 10
    /// Horizontal inset from the view's left edge, in points.
    private static let horizontalPadding: CGFloat = CGFloat(EditorInset.padding)
    /// Vertical padding above the first / below the last line, in points.
    private static let verticalPadding: CGFloat = 4

    private var metrics: CellMetrics

    // -- Latest projected state (set by `update`) -----------------------------
    private var lines: [[ToastRenderChunk]] = []
    private var bgColor: CGColor
    /// Scroll offset from the top of the content, in points (clamped in `draw`).
    private var scrollOffset: CGFloat = 0

    /// Builds the overlay against the measured `metrics`. Starts hidden.
    init(metrics: CellMetrics) {
        self.metrics = metrics
        self.bgColor = WolfColor.fallbackBg
        super.init(frame: .zero)
        wantsLayer = true
        isHidden = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("ToastView is not constructed from a nib")
    }

    override var isFlipped: Bool { true }

    /// The toast paints its own content; the container decides whether a wheel
    /// gesture reaches it (the scroll-over-toast exemption), so this view is not a
    /// responder-chain scroll target.
    override var acceptsFirstResponder: Bool { false }

    /// Re-measures against `newMetrics` (a per-window font-size change).
    func applyMetrics(_ newMetrics: CellMetrics) {
        metrics = newMetrics
        needsDisplay = true
    }

    /// The per-line height, in points.
    var lineHeight: CGFloat { ceil(metrics.cellHeight) }

    /// The total number of lines currently held (uncapped).
    var lineCount: Int { lines.count }

    /// The preferred overlay height for the current content, capped at
    /// `maxVisibleLines` rows plus vertical padding.
    var preferredHeight: CGFloat {
        let shown = min(lines.count, Self.maxVisibleLines)
        guard shown > 0 else { return 0 }
        return CGFloat(shown) * lineHeight + 2 * Self.verticalPadding
    }

    /// The preferred overlay width: the widest line's text plus padding.
    var preferredWidth: CGFloat {
        let font = overlayFont(metrics: metrics)
        let attrs: [NSAttributedString.Key: Any] = [.font: font]
        var maxWidth: CGFloat = 0
        for line in lines {
            let text = line.map(\.text).joined()
            maxWidth = max(maxWidth, (text as NSString).size(withAttributes: attrs).width)
        }
        return maxWidth + 2 * Self.horizontalPadding
    }

    /// Projects the latest `FfiToast` into the view: stores each line's chunks
    /// with their Rust-resolved foreground colors, toggles visibility on
    /// `visible()`, resets the scroll offset when the content changes, and
    /// requests a repaint. Returns whether the toast is visible.
    @discardableResult
    func update(_ toast: FfiToast, defaultBg: CGColor) -> Bool {
        let visible = toast.visible()
        bgColor = defaultBg

        var newLines: [[ToastRenderChunk]] = []
        let ffiLines = toast.lines()
        for i in 0..<ffiLines.len() {
            let chunks = ffiLines[i].chunks()
            var renderChunks: [ToastRenderChunk] = []
            for j in 0..<chunks.len() {
                let chunk = chunks[j]
                renderChunks.append(
                    ToastRenderChunk(
                        text: chunk.text().toString(),
                        fg: WolfColor.cgColor(chunk.fg(), fallback: WolfColor.fallbackFg)))
            }
            newLines.append(renderChunks)
        }

        // Reset the scroll offset only when the content actually changed (a repeat
        // wake with the same message keeps the user's scroll position).
        if newLines.count != lines.count {
            scrollOffset = 0
        }
        lines = newLines
        isHidden = !visible
        needsDisplay = true
        return visible
    }

    /// Scrolls the toast content by `deltaY` points (positive scrolls the content
    /// up, revealing later lines), clamped to the scrollable range. Called by the
    /// container when a wheel gesture lands over the toast, so that gesture never
    /// reaches Neovim (and hence never dismisses the toast — OD8).
    func scrollBy(deltaY: CGFloat) {
        scrollOffset = clampedOffset(scrollOffset - deltaY)
        needsDisplay = true
    }

    /// The maximum scroll offset for the current content and frame height.
    private var maxScrollOffset: CGFloat {
        let contentHeight = CGFloat(lines.count) * lineHeight + 2 * Self.verticalPadding
        return max(0, contentHeight - bounds.height)
    }

    /// Clamps `offset` to `[0, maxScrollOffset]`.
    private func clampedOffset(_ offset: CGFloat) -> CGFloat {
        min(max(0, offset), maxScrollOffset)
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.setShouldAntialias(true)
        ctx.setShouldSmoothFonts(false)

        // Background band (subtle border so it reads as a distinct surface).
        ctx.setFillColor(bgColor)
        ctx.fill(bounds)
        ctx.setStrokeColor(NSColor.separatorColor.cgColor)
        ctx.setLineWidth(1)
        ctx.stroke(bounds.insetBy(dx: 0.5, dy: 0.5))

        let font = overlayFont(metrics: metrics)
        let offset = clampedOffset(scrollOffset)
        let lh = lineHeight

        ctx.saveGState()
        ctx.clip(to: bounds)
        for (row, line) in lines.enumerated() {
            let y = Self.verticalPadding + CGFloat(row) * lh - offset
            // Skip lines fully outside the visible band.
            if y + lh < 0 || y > bounds.height { continue }
            var x = Self.horizontalPadding
            for chunk in line {
                let attrs: [NSAttributedString.Key: Any] = [
                    .font: font,
                    .foregroundColor: NSColor(cgColor: chunk.fg) ?? .textColor,
                ]
                let ns = chunk.text as NSString
                ns.draw(at: CGPoint(x: x, y: y), withAttributes: attrs)
                x += ns.size(withAttributes: attrs).width
            }
        }
        ctx.restoreGState()
    }
}
