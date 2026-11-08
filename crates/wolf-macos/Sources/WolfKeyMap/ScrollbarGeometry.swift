// Pure projection of a window's viewport onto a pane's synthetic scrollbar.

import Foundation

/// The track and thumb rectangles of a pane's synthetic scrollbar, projected from
/// its window's viewport.
///
/// - `inset + thickness` fits inside `EditorInset.padding`, so a track measured
///   from the pane's right edge lies wholly in the right padding gutter and covers
///   no glyph at any pane width. Measured from the grid rect, it would cover the
///   last text column of any pane narrower than the cap.
/// - The bar never enters grid sizing. A bar that took grid width would drop a
///   column whenever a buffer overflowed, so scrolling would reflow the text.
/// - The thumb height is clamped to the track, so a pane shorter than `minThumb`
///   cannot draw a thumb over the pane below.
/// - The viewport fields are widened to `Double` before any subtraction, because a
///   `UInt64` subtraction on a malformed viewport traps.
public struct ScrollbarGeometry: Equatable {
  /// The track and thumb width.
  public static let thickness: Double = 8
  /// The gap between the bar and the pane's right edge, and between the track and
  /// the grid's top and bottom.
  public static let inset: Double = 2
  /// The thumb's minimum height, so a tiny viewport fraction stays visible; the
  /// track height caps it.
  public static let minThumb: Double = 24

  /// The full scroll range, spanning the grid rect's height less `inset` at each end.
  public let track: Rect
  /// The visible part of the buffer, always inside `track`.
  public let thumb: Rect

  /// Projects `topline` / `botline` / `linecount` (0-based, `botline` one past the
  /// last visible line) onto the bar of the pane at `paneRect`, whose inset grid
  /// rect is `gridRect`. Both rects share one coordinate space, and so does the
  /// result.
  ///
  /// `nil` when there is nothing to draw: a non-finite rect, nothing visible, a
  /// buffer that fits, a pane too narrow for the bar, or a grid too short for a track.
  public init?(
    paneRect: Rect, gridRect: Rect, topline: UInt64, botline: UInt64, linecount: UInt64
  ) {
    let fields = [
      paneRect.x, paneRect.y, paneRect.w, paneRect.h,
      gridRect.x, gridRect.y, gridRect.w, gridRect.h,
    ]
    guard fields.allSatisfy({ $0.isFinite }) else { return nil }

    let total = Double(linecount)
    let top = Double(topline)
    let visible = max(0, Double(botline) - top)
    guard total > 0, visible > 0, visible < total else { return nil }

    let x = paneRect.x + paneRect.w - Self.inset - Self.thickness
    guard x >= paneRect.x else { return nil }

    let track = Rect(
      x: x, y: gridRect.y + Self.inset, w: Self.thickness, h: gridRect.h - 2 * Self.inset)
    guard track.h > 0 else { return nil }

    // 0-based: the thumb's top sits at `topline / total` of the track, 0 at the buffer's top.
    let thumbH = min(max(Self.minThumb, track.h * min(1, visible / total)), track.h)
    let rawY = track.y + (top / total) * track.h
    let thumbY = min(max(rawY, track.y), max(track.y, track.y + track.h - thumbH))

    self.track = track
    self.thumb = Rect(x: track.x, y: thumbY, w: Self.thickness, h: thumbH)
  }
}
