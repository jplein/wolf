//
//  Color.swift
//  Packed-u32 + `set` flag → `CGColor`, with fallbacks mirroring wolf-gnome.
//

import CoreGraphics
import Foundation
import WolfKeyMap

/// Converts the bridge's packed `FfiColor` (a `0x00RRGGBB` value plus a `set`
/// flag) into a `CGColor`, applying the white/black fallbacks when unset — the
/// macOS analog of wolf-gnome's `render::FALLBACK_FG` / `FALLBACK_BG`.
enum WolfColor {
    /// Fallback foreground (white) when neither a highlight nor the defaults set
    /// one. Mirrors wolf-gnome's `FALLBACK_FG`.
    static let fallbackFg = CGColor(red: 1, green: 1, blue: 1, alpha: 1)
    /// Fallback background (black). Mirrors wolf-gnome's `FALLBACK_BG`.
    static let fallbackBg = CGColor(red: 0, green: 0, blue: 0, alpha: 1)

    /// Divisor turning an 8-bit channel (0–255) into the 0.0–1.0 `CGColor` range.
    private static let channelMax: CGFloat = 255

    /// Resolves `color` to a `CGColor`, using `fallback` when `color.set` is false.
    static func cgColor(_ color: FfiColor, fallback: CGColor) -> CGColor {
        guard color.set else { return fallback }
        return cgColor(packed: color.rgb)
    }

    /// Unpacks a `0x00RRGGBB` value into an opaque `CGColor`.
    static func cgColor(packed: UInt32) -> CGColor {
        let r = CGFloat((packed >> 16) & 0xFF) / channelMax
        let g = CGFloat((packed >> 8) & 0xFF) / channelMax
        let b = CGFloat(packed & 0xFF) / channelMax
        return CGColor(red: r, green: g, blue: b, alpha: 1)
    }
}
