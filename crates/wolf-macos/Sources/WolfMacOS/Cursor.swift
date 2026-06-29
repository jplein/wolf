//
//  Cursor.swift
//  Cursor-shape geometry, ported from wolf-gnome's `cursor.rs`.
//

import CoreGraphics
import Foundation

/// Cursor-shape discriminants packed into `FfiFrame.cursor_shape()`, mirroring
/// `wolf_core::CursorShape`.
enum CursorShape: UInt8 {
    case block = 0
    case horizontal = 1
    case vertical = 2
}

/// Pure cursor-rectangle geometry (the macOS analog of wolf-gnome's
/// `cursor::cursor_rect`). The reverse-video fill/glyph colors are computed in
/// Rust (`FfiFrame.cursor_fill()` / `FfiFrame.cursor_glyph()`), so only the
/// rectangle lives here.
enum Cursor {
    /// Minimum bar thickness in points, so a tiny `cellPercentage` still renders a
    /// visible bar rather than a hairline.
    static let minBarPx: CGFloat = 2
    /// Bar thickness as a fraction of the cell when `cellPercentage` is 0/absent.
    static let defaultCellFraction: CGFloat = 0.25
    /// Divisor turning a `cellPercentage` PERCENT (1–100) into a 0.0–1.0 fraction.
    static let pctDenom: CGFloat = 100

    /// Computes the rectangle to fill for `shape` within the cell box
    /// `(cellX, cellY, cellW, cellH)`. `cellPercentage` is 0 when unspecified.
    ///
    /// - `.block` fills the whole cell, ignoring `cellPercentage`.
    /// - `.vertical` is a left bar of full height; width is `cellPercentage` of the
    ///   cell (default 25%), floored to `minBarPx`, capped at `cellW`.
    /// - `.horizontal` is a bottom bar of full width; height follows the same rule
    ///   against `cellH`, anchored to the cell's bottom edge.
    static func rect(
        shape: CursorShape,
        cellX: CGFloat,
        cellY: CGFloat,
        cellW: CGFloat,
        cellH: CGFloat,
        cellPercentage: UInt32
    ) -> CGRect {
        switch shape {
        case .block:
            return CGRect(x: cellX, y: cellY, width: cellW, height: cellH)
        case .vertical:
            let w = min(max(cellW * fraction(cellPercentage), minBarPx), cellW)
            return CGRect(x: cellX, y: cellY, width: w, height: cellH)
        case .horizontal:
            let h = min(max(cellH * fraction(cellPercentage), minBarPx), cellH)
            return CGRect(x: cellX, y: cellY + cellH - h, width: cellW, height: h)
        }
    }

    /// Turns a `cellPercentage` PERCENT (or 0 for "unspecified") into a clamped
    /// 0.0–1.0 fraction, defaulting to `defaultCellFraction`.
    private static func fraction(_ cellPercentage: UInt32) -> CGFloat {
        guard cellPercentage > 0 else { return defaultCellFraction }
        return min(max(CGFloat(cellPercentage) / pctDenom, 0), 1)
    }
}
