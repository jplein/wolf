// Pure colour and weight rule for the per-pane statusbar text.

import Foundation

/// One of AppKit's system label colours, which the view maps to `NSColor`.
public enum StatusbarLabelTier: Equatable {
  /// `labelColor`.
  case primary
  /// `secondaryLabelColor`.
  case secondary
  /// `tertiaryLabelColor`.
  case tertiary
}

/// The colour of one statusbar text line.
public enum StatusbarTextColor: Equatable {
  /// The titlebar foreground, packed `0x00RRGGBB`, at `alpha`.
  case themed(rgb: UInt32, alpha: Double)
  /// A system label colour.
  case label(StatusbarLabelTier)
}

/// The colours and weight of a pane's statusbar text: the primary (filename) and
/// secondary (path) lines, and whether the filename is bold.
///
/// The text colour and weight are the whole active/inactive treatment: an active
/// pane's filename is bold at full strength, an inactive pane's is regular weight
/// and dimmed.
///
/// The band fill is not decided here. It reads the live window's `backgroundColor`,
/// so it stays in `PaneContainerView`, titlebar-matched.
///
/// wolf-gnome's `statusbar_text` takes the editor's colours and has no label tier.
/// That is a deliberate divergence, which the user accepted.
public struct StatusbarText: Equatable {
  /// The secondary line's alpha relative to the primary's when themed.
  private static let secondaryThemedAlpha: Double = 0.7
  /// The factor an inactive pane applies to both themed alphas. Unthemed, an
  /// inactive pane steps each line down one label tier instead.
  private static let inactiveAlpha: Double = 0.5

  /// The filename line's colour.
  public let primary: StatusbarTextColor
  /// The path line's colour.
  public let secondary: StatusbarTextColor
  /// Whether the filename line is bold; true exactly for the active pane.
  public let primaryBold: Bool

  /// Derives the text treatment from the theme's titlebar foreground and whether
  /// the pane is active. The branch keys on `titlebarFg.set`, never on its rgb, so
  /// a set black foreground is still themed.
  public init(titlebarFg: FfiColor, active: Bool) {
    if titlebarFg.set {
      let a = active ? 1 : Self.inactiveAlpha
      primary = .themed(rgb: titlebarFg.rgb, alpha: a)
      secondary = .themed(rgb: titlebarFg.rgb, alpha: Self.secondaryThemedAlpha * a)
    } else {
      primary = .label(active ? .primary : .secondary)
      secondary = .label(active ? .secondary : .tertiary)
    }
    primaryBold = active
  }
}
