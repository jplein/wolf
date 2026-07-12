//
//  CmdlineView.swift
//  The native command line (`ext_cmdline`): a dedicated overlay `NSView` that
//  paints `firstc + text` with a caret at `pos`, pinned to the bottom of the
//  editor area and shown only while a command line is active (OD9 — overlay
//  when active rather than reserving a permanent band).
//
//  A projection of the core's `Snapshot.cmdline` read across the FFI as an
//  `FfiCmdline` (OD1 — the command-line content is flattened to a single `text()`
//  String; only the message toast keeps styled chunks). This AppKit glue is
//  compile-verified + manual-smoke only.
//

import AppKit
import CoreText
import WolfFFI
import WolfKeyMap

/// The native command-line overlay. Paints one row: the `firstc` prefix
/// (`:` / `/` / `?`) immediately followed by the command text, with a thin caret
/// at the byte position `pos`. Hidden (via `isHidden`) whenever the command line
/// is not active.
final class CmdlineView: NSView {
    /// Horizontal inset from the view's left edge, matching the editor grid's
    /// per-pane padding so the command line aligns with the grid text.
    private static let horizontalPadding: CGFloat = CGFloat(EditorInset.padding)
    /// Caret width, in points.
    private static let caretWidth: CGFloat = 2

    private var metrics: CellMetrics

    // -- Latest projected state (set by `update`) -----------------------------
    private var firstc: String = ""
    private var text: String = ""
    private var pos: Int = 0
    private var fgColor: CGColor
    private var bgColor: CGColor

    /// Builds the overlay against the measured `metrics`. Starts hidden (no
    /// command line active yet).
    init(metrics: CellMetrics) {
        self.metrics = metrics
        self.fgColor = WolfColor.fallbackFg
        self.bgColor = WolfColor.fallbackBg
        super.init(frame: .zero)
        wantsLayer = true
        isHidden = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("CmdlineView is not constructed from a nib")
    }

    /// Top-left origin so text lays out upright in a flipped context.
    override var isFlipped: Bool { true }

    /// Re-measures against `newMetrics` (a per-window font-size change).
    func applyMetrics(_ newMetrics: CellMetrics) {
        metrics = newMetrics
        needsDisplay = true
    }

    /// The preferred height (one text row) for the overlay's frame.
    var preferredHeight: CGFloat { ceil(metrics.cellHeight) }

    /// Projects the latest `FfiCmdline` into the view: stores the firstc / text /
    /// caret position and the default fg/bg colors, toggles visibility on
    /// `active()`, and requests a repaint. Returns whether the command line is
    /// active (so the container can lay the overlay out or hide it).
    @discardableResult
    func update(_ cmdline: FfiCmdline, defaultFg: CGColor, defaultBg: CGColor) -> Bool {
        let active = cmdline.active()
        firstc = cmdline.firstc().toString()
        text = cmdline.text().toString()
        pos = Int(cmdline.pos())
        fgColor = defaultFg
        bgColor = defaultBg
        isHidden = !active
        needsDisplay = true
        return active
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.setShouldAntialias(true)
        ctx.setShouldSmoothFonts(false)

        // Background band.
        ctx.setFillColor(bgColor)
        ctx.fill(bounds)

        let font = overlayFont(metrics: metrics)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: NSColor(cgColor: fgColor) ?? .textColor,
        ]

        let line = firstc + text
        let baselineY = bounds.minY
        let textOrigin = CGPoint(x: Self.horizontalPadding, y: baselineY)
        (line as NSString).draw(at: textOrigin, withAttributes: attrs)

        // Caret: measure the width of everything up to the byte offset `pos`
        // (plus the firstc prefix) and stroke a thin bar there.
        let prefix = firstc + Self.bytePrefix(of: text, byteCount: pos)
        let caretX = Self.horizontalPadding + (prefix as NSString).size(withAttributes: attrs).width
        ctx.setFillColor(fgColor)
        ctx.fill(CGRect(x: caretX, y: bounds.minY, width: Self.caretWidth, height: ceil(metrics.cellHeight)))
    }

    /// The UTF-8 prefix of `s` of at most `byteCount` bytes, decoded back to a
    /// `String` (lossily replacing a split multi-byte boundary). Used to place the
    /// caret at the reported byte offset.
    static func bytePrefix(of s: String, byteCount: Int) -> String {
        if byteCount <= 0 { return "" }
        let bytes = Array(s.utf8)
        if byteCount >= bytes.count { return s }
        return String(decoding: bytes.prefix(byteCount), as: UTF8.self)
    }
}

/// Builds an `NSFont` matching the grid's `CellMetrics` (family + size), for the
/// cmdline / completion-popup / toast overlays' `NSString`-based text drawing.
/// Falls back to the monospaced system font when the family cannot be resolved.
func overlayFont(metrics: CellMetrics) -> NSFont {
    let size = CTFontGetSize(metrics.font)
    let family = CTFontCopyFamilyName(metrics.font) as String
    return NSFont(name: family, size: size)
        ?? .monospacedSystemFont(ofSize: size, weight: .regular)
}
