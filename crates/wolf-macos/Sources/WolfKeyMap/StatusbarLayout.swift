//
//  StatusbarLayout.swift
//  Pure, AppKit-/CoreText-free vertical-layout math for the per-pane statusbar
//  (plan §6.1).
//
//  This is a headless seam in the `WolfKeyMap` module — Foundation-only, no live
//  font, no NSString drawing. It computes the strip height and the top/height of
//  the two text lines (primary filename, secondary path) from four inputs, so the
//  AppKit painter (`PaneContainerView.paintStatusbar`) can lay out the strip
//  deterministically. The on-screen painting itself is manual-smoke only; only
//  this arithmetic is unit-tested.
//

import Foundation

/// The vertical layout of a per-pane statusbar strip: its total `height` and the
/// top offset + height of each of the two text lines.
///
/// Each line's height is `ceil(fontSize * lineFactor)` with a fixed `lineFactor`
/// of `1.25`. The primary line sits at `verticalPadding`; the secondary line sits
/// one `lineGap` below the primary line's bottom; the strip's total height leaves
/// a matching `verticalPadding` beneath the secondary line.
public struct StatusbarLayout {
  /// The line-height multiplier applied to each font size (`ceil(fontSize *
  /// lineFactor)`), pinned by `StatusbarLayoutTests`.
  private static let lineFactor: Double = 1.25

  /// The total strip height.
  public let height: Double
  /// The top offset of the primary (filename) line.
  public let primaryLineTop: Double
  /// The height of the primary (filename) line.
  public let primaryLineHeight: Double
  /// The top offset of the secondary (path) line.
  public let secondaryLineTop: Double
  /// The height of the secondary (path) line.
  public let secondaryLineHeight: Double

  /// Computes the layout from the primary/secondary font sizes, the vertical
  /// padding above the first and below the last line, and the gap between the
  /// two lines.
  public init(
    primaryFontSize: Double, secondaryFontSize: Double, verticalPadding: Double, lineGap: Double
  ) {
    let primary = (primaryFontSize * StatusbarLayout.lineFactor).rounded(.up)
    let secondary = (secondaryFontSize * StatusbarLayout.lineFactor).rounded(.up)

    self.primaryLineHeight = primary
    self.secondaryLineHeight = secondary
    self.primaryLineTop = verticalPadding
    self.secondaryLineTop = verticalPadding + primary + lineGap
    self.height = self.secondaryLineTop + secondary + verticalPadding
  }
}
