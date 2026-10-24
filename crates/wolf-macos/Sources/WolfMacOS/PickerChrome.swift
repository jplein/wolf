// The shared backdrop chrome behind every picker panel.

import AppKit

/// Builds the two `NSVisualEffectView` backdrops a picker panel is made of.
///
/// A picker view owns a full-bleed `panelBackdrop()` behind all its content and a
/// `searchBarBackdrop()` behind the search bar; both are plain subviews the
/// caller constrains.
enum PickerChrome {
  /// The panel background: the material AppKit itself uses for a window's
  /// background.
  ///
  /// It must be a material and not `NSColor.windowBackgroundColor`, because when
  /// the desktop tints windows ("Allow wallpaper tinting in windows", System
  /// Settings › Appearance) the tint lives in the material, NOT in the colour.
  /// The results list is an `NSTableView`, which paints its own tinted material,
  /// so a picker whose panel is a flat `windowBackgroundColor` shows the list in
  /// one shade and the padding and search bar around it in another. Filling with
  /// the same `NSColor` at draw time does not close the gap — a measured fill is
  /// the untinted shade whether it is frozen into a `CGColor` or drawn live.
  static func panelBackdrop(cornerRadius: CGFloat) -> NSVisualEffectView {
    backdrop(material: .windowBackground, cornerRadius: cornerRadius)
  }

  /// The search bar background: the material behind editable content, which is
  /// distinct from the panel's in a light appearance and matches it in a dark
  /// one, mirroring `controlBackgroundColor` against `windowBackgroundColor`.
  static func searchBarBackdrop(cornerRadius: CGFloat) -> NSVisualEffectView {
    backdrop(material: .contentBackground, cornerRadius: cornerRadius)
  }

  private static func backdrop(
    material: NSVisualEffectView.Material, cornerRadius: CGFloat
  ) -> NSVisualEffectView {
    let view = NSVisualEffectView()
    view.translatesAutoresizingMaskIntoConstraints = false
    view.material = material
    view.blendingMode = .behindWindow
    // `.followsWindowActiveState` would drop the material and tint whenever the picker's panel is
    // not key, which it is not while the editor behind it holds focus.
    view.state = .active
    view.maskImage = roundedMask(radius: cornerRadius)
    return view
  }

  /// A resizable rounded-rectangle mask for an `NSVisualEffectView`.
  ///
  /// Corner radius has to be a `maskImage` rather than a layer `cornerRadius`: a
  /// behind-window material is composited by the window server outside the view's
  /// own layer, which leaves a layer mask with nothing to clip.
  private static func roundedMask(radius: CGFloat) -> NSImage {
    let edge = 2 * radius + 1
    let mask = NSImage(
      size: NSSize(width: edge, height: edge),
      flipped: false,
      drawingHandler: { rect in
        NSColor.black.setFill()
        NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).fill()
        return true
      })
    mask.capInsets = NSEdgeInsets(
      top: radius, left: radius, bottom: radius, right: radius)
    mask.resizingMode = .stretch
    return mask
  }
}
