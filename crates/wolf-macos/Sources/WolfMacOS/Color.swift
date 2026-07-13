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
  /// The sRGB color space that packed colors are unpacked into. Neovim / terminal
  /// hex colors (and the `WolfTitlebar` window `backgroundColor`, set via
  /// `NSColor(srgbRed:...)`) are sRGB, so the grid MUST use sRGB too. The
  /// convenience `CGColor(red:green:blue:alpha:)` initializer instead builds
  /// `kCGColorSpaceGenericRGB` colors, whose different gamma renders the SAME
  /// packed value visibly lighter (e.g. `#1b1b1b` displays as ~`#242424`) — so
  /// the editor background would not match the sRGB titlebar. Building in sRGB
  /// keeps the two paths pixel-identical.
  private static let srgb = CGColorSpace(name: CGColorSpace.sRGB)!

  /// Fallback foreground (white) when neither a highlight nor the defaults set
  /// one. Mirrors wolf-gnome's `FALLBACK_FG`.
  static let fallbackFg = cgColor(packed: 0x00FF_FFFF)
  /// Fallback background (black). Mirrors wolf-gnome's `FALLBACK_BG`.
  static let fallbackBg = cgColor(packed: 0x0000_0000)

  /// Divisor turning an 8-bit channel (0–255) into the 0.0–1.0 `CGColor` range.
  private static let channelMax: CGFloat = 255

  /// Resolves `color` to a `CGColor`, using `fallback` when `color.set` is false.
  static func cgColor(_ color: FfiColor, fallback: CGColor) -> CGColor {
    guard color.set else { return fallback }
    return cgColor(packed: color.rgb)
  }

  /// Unpacks a `0x00RRGGBB` value into an opaque sRGB `CGColor` (see `srgb`).
  static func cgColor(packed: UInt32) -> CGColor {
    let r = CGFloat((packed >> 16) & 0xFF) / channelMax
    let g = CGFloat((packed >> 8) & 0xFF) / channelMax
    let b = CGFloat(packed & 0xFF) / channelMax
    return CGColor(colorSpace: srgb, components: [r, g, b, 1])
      ?? CGColor(red: r, green: g, blue: b, alpha: 1)
  }
}
