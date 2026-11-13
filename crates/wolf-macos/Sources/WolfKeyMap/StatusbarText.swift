// Pure colour and weight rule for the per-pane statusbar text.

import Foundation

/// The colours and weight of a pane's statusbar text: the primary (filename) and
/// secondary (path) lines, and whether the filename is bold.
///
/// The text colour and weight are the whole active/inactive treatment: an active
/// pane's filename is bold at full strength, an inactive pane's is regular weight
/// and dimmed.
///
/// The colours are the editor's default foreground and background (`Normal`), never
/// the titlebar's. Each line is the foreground blended toward the background, which
/// is the opaque strip fill the caller paints, so the result is the pixel alpha
/// compositing would give and is drawn opaque.
///
/// This mirrors wolf-gnome's `statusbar_text`
/// (`crates/wolf-gnome/src/README.md#statusbar_layoutrs`) constant for constant, and
/// in its blend and rounding, so the two front-ends paint one theme's strip text
/// alike. Change both together.
public struct StatusbarText: Equatable {
  /// The secondary line's alpha relative to the primary's. wolf-gnome's
  /// `STATUSBAR_SECONDARY_ALPHA`.
  private static let secondaryAlpha: Double = 0.7
  /// The alpha both lines are scaled by in an inactive pane. wolf-gnome's
  /// `STATUSBAR_INACTIVE_ALPHA`.
  private static let inactiveAlpha: Double = 0.5
  /// The largest value a colour channel takes.
  private static let channelMax: Double = 255

  /// The filename line's colour, packed `0x00RRGGBB`, opaque and already blended.
  public let primary: UInt32
  /// The path line's colour, packed `0x00RRGGBB`, opaque and already blended.
  public let secondary: UInt32
  /// Whether the filename line is bold; true exactly for the active pane.
  public let primaryBold: Bool

  /// Derives the text treatment from the editor's default foreground and background,
  /// packed `0x00RRGGBB` after the caller's fallbacks, and whether the pane is active.
  public init(fg: UInt32, bg: UInt32, active: Bool) {
    let primaryAlpha = active ? 1 : Self.inactiveAlpha
    primary = Self.blend(fg, bg, primaryAlpha)
    secondary = Self.blend(fg, bg, Self.secondaryAlpha * primaryAlpha)
    primaryBold = active
  }

  /// Blends `fg` toward `bg` by `alpha`, channel by channel.
  private static func blend(_ fg: UInt32, _ bg: UInt32, _ alpha: Double) -> UInt32 {
    var packed: UInt32 = 0
    for shift: UInt32 in [16, 8, 0] {
      let f = Double((fg >> shift) & 0xFF)
      let b = Double((bg >> shift) & 0xFF)
      // `rounded()` is half away from zero, as Rust's `f64::round` in wolf-gnome's blend.
      let channel = min(max((b + (f - b) * alpha).rounded(), 0), channelMax)
      packed |= UInt32(channel) << shift
    }
    return packed
  }
}
