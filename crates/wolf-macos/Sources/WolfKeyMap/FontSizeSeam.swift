// Pure font-size stepping and clamping for the View-menu font-size commands.

import Foundation

/// The pure font-size stepping + clamping arithmetic backing the View menu. A
/// namespace (`enum` with only static members).
public enum FontSizeSeam {
  /// The smallest allowed point size (clamp floor).
  public static let minPointSize: Double = 6
  /// The largest allowed point size (clamp ceiling).
  public static let maxPointSize: Double = 72
  /// The per-step point-size delta (Increase / Decrease move by this much).
  public static let step: Double = 1

  /// Clamps `size` into `[minPointSize, maxPointSize]`.
  public static func clamped(_ size: Double) -> Double {
    min(maxPointSize, max(minPointSize, size))
  }

  /// The next size up: `clamped(current + step)`.
  public static func increased(_ current: Double) -> Double {
    clamped(current + step)
  }

  /// The next size down: `clamped(current - step)`.
  public static func decreased(_ current: Double) -> Double {
    clamped(current - step)
  }
}
