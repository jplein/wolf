//
//  PickerPanelSeam.swift
//  Pure, AppKit-/CoreGraphics-free arithmetic for POSITIONING a picker panel
//  top-center over its owning window's content (plan §2a).
//
//  This is a headless seam in the `WolfKeyMap` module — Foundation-only, no live
//  window/panel, no `CGFloat`. Given the parent window's content size, the panel's
//  desired size, and a fixed top margin, it computes the panel's frame in the
//  parent's CONTENT space (TOP-LEFT origin): centered horizontally, pinned a fixed
//  margin below the content top, and clamped so a panel wider/taller than the
//  content never overflows. The AppKit glue (`PickerPanelHost`) maps this top-left
//  content rect into a bottom-left SCREEN frame for the child `NSPanel`; only this
//  arithmetic is unit-tested (see `PickerPanelSeamTests.swift`).
//
//  It reuses the existing `Rect` value type from `PaneTree.swift` and, like
//  `EditorInset` / `PaneGeometry`, is `Double`-based so it stays headlessly
//  testable.
//

import Foundation

/// Positions a picker panel top-center over its owning window's content.
///
/// Mirrors the `EditorInset` convention: only the SIZES (`w` / `h`) CLAMP; the
/// origins are derived (`x` centers the panel horizontally, `y` is a FIXED offset
/// from the content top and is NOT clamped). `x` floors at 0 (never negative when
/// the panel is wider than the content); `h` caps at `parentContentHeight -
/// topMargin` (never taller than the content below the margin, flooring at 0). The
/// returned `Rect` is in parent CONTENT space, TOP-LEFT origin.
public enum PickerPanelSeam {
  /// Computes the panel's frame in parent content space (top-left origin):
  /// `w = min(panelWidth, max(0, parentContentWidth))`,
  /// `x = max(0, (parentContentWidth - w) / 2)` — centered, never negative,
  /// `y = topMargin` — a fixed offset from the content top, NOT clamped,
  /// `h = min(panelHeight, max(0, parentContentHeight - topMargin))` — capped at
  /// the content below the margin. Only `w` / `h` clamp; `x` / `w` center
  /// horizontally; `h` caps at `parentContentHeight - topMargin`.
  public static func frame(
    parentContentWidth: Double,
    parentContentHeight: Double,
    panelWidth: Double,
    panelHeight: Double,
    topMargin: Double
  ) -> Rect {
    let w = min(panelWidth, max(0, parentContentWidth))
    let x = max(0, (parentContentWidth - w) / 2)
    let y = topMargin
    let available = max(0, parentContentHeight - topMargin)
    let h = min(panelHeight, available)
    return Rect(x: x, y: y, w: w, h: h)
  }
}
