// Pure precedence rule for the window titlebar's background colour.

import Foundation

/// The colour the window titlebar's background takes: a set `WolfTitlebar`
/// background, else the `Normal` background the grid fills with, else the
/// system colour.
///
/// `WolfTitlebar` comes first because a colorscheme that defines it is asking
/// for it. `Normal` is next so a colorscheme that does not, such as Lighthouse,
/// still gets a titlebar that matches the grid below it. The system colour is
/// left only while neither is known, before the first frame.
public enum TitlebarBackground: Equatable {
  /// A packed `0x00RRGGBB` colour, drawn in sRGB as the grid is.
  case colour(UInt32)
  /// The window's own system background colour.
  case system

  /// Picks the titlebar background from `WolfTitlebar`'s background
  /// (`titlebar`) and `Normal`'s (`normal`). The `rgb` of an unset colour is
  /// never read.
  public init(titlebar: FfiColor, normal: FfiColor) {
    if titlebar.set {
      self = .colour(titlebar.rgb)
    } else if normal.set {
      self = .colour(normal.rgb)
    } else {
      self = .system
    }
  }
}
