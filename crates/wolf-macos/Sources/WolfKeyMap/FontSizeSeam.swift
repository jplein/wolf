//
//  FontSizeSeam.swift
//  Pure, AppKit-/CoreText-free font-size stepping + clamping seam.
//
//  The per-window View-menu font-size commands (Reset / Increase / Decrease,
//  plan §3) step and clamp a point size. This seam owns ONLY the pure arithmetic
//  (step ±1, clamp into `[minPointSize, maxPointSize]`); the on-screen relayout
//  (re-measuring `CellMetrics`, reflowing the grid, repainting) is AppKit glue in
//  `WolfWindowController` and is manual-smoke only. Reset is NOT in the seam — the
//  controller resets to its stored launch default.
//
//  The API is `Double`-typed (the controller bridges to/from `CGFloat` at its call
//  sites) so it can be asserted headlessly with a tight accuracy. Imports ONLY
//  Foundation, so it lives in the Foundation-only `WolfKeyMap` module alongside
//  `CellMetricsMath.swift` / `ShortcutSeam.swift`.
//

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
