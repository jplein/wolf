// Renders an AppKit view in a never-shown window and samples the pixels it drew, for view tests.

import AppKit

/// An `NSWindow` whose `isKeyWindow` is whatever a test sets in `reportsKey`.
///
/// AppKit never makes it key and sends no key notifications, so only code that reads
/// `isKeyWindow` sees the change, on its next read.
final class OffscreenWindow: NSWindow {
  /// What `isKeyWindow` returns; false until a test sets it.
  var reportsKey = false

  override var isKeyWindow: Bool { reportsKey }
}

/// Hosts a view as the content of an `OffscreenWindow` that is never ordered front
/// and reports key only when a test sets `reportsKey`, and captures what it draws
/// through `cacheDisplay(in:to:)`, which needs no window server or display session.
@MainActor
final class OffscreenRenderer {
  /// The hosting window. AppKit never makes it key, so nothing in it takes emphasis
  /// by itself: see `NSTableView.setRowsEmphasized(_:)`. Setting `reportsKey` changes
  /// `isKeyWindow` alone.
  let window: OffscreenWindow

  /// Hosts `view` as the content of a `size`-point window drawn in the Aqua
  /// (light) appearance, so captures do not depend on the host's dark mode.
  init(hosting view: NSView, size: NSSize) {
    // AppKit's drawing machinery is set up by the shared application.
    _ = NSApplication.shared
    window = OffscreenWindow(
      contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled],
      backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.appearance = NSAppearance(named: .aqua)
    window.contentView = view
  }

  /// Lays out and displays the window, then captures its content view.
  ///
  /// The display pass is required: without `displayIfNeeded()` the capture
  /// shows stale layer contents, and a property change made since the last
  /// capture never reaches the bitmap, even with `needsDisplay` set.
  func render() -> RenderedView {
    window.layoutIfNeeded()
    window.displayIfNeeded()
    guard let root = window.contentView,
      let bitmap = root.bitmapImageRepForCachingDisplay(in: root.bounds)
    else {
      preconditionFailure("OffscreenRenderer: the window has no content view to capture")
    }
    root.cacheDisplay(in: root.bounds, to: bitmap)
    return RenderedView(root: root, bitmap: bitmap)
  }
}

/// One capture of a window's content view. Every query takes a rect or point in
/// the coordinates of any view under the root, and reads sRGB colours with
/// alpha un-premultiplied.
@MainActor
struct RenderedView {
  /// The captured content view, whose bounds the bitmap covers.
  let root: NSView
  /// The captured pixels, at the backing scale of the window's screen (or 1 with
  /// none), with row 0 at the top.
  let bitmap: NSBitmapImageRep

  /// The colour of the pixel under `point`, given in `view`'s coordinates.
  func color(at point: NSPoint, in view: NSView) -> NSColor {
    let pixels = pixelRange(of: NSRect(origin: root.convert(point, from: view), size: .zero))
    let x = min(pixels.x.lowerBound, bitmap.pixelsWide - 1)
    let y = min(pixels.y.lowerBound, bitmap.pixelsHigh - 1)
    return srgb(x: max(x, 0), y: max(y, 0))
  }

  /// The colours of every pixel `rect`, given in `view`'s coordinates, covers,
  /// clipped to the capture.
  func colors(in rect: NSRect, of view: NSView) -> [NSColor] {
    let pixels = pixelRange(of: root.convert(rect, from: view))
    var result: [NSColor] = []
    for y in pixels.y {
      for x in pixels.x {
        result.append(srgb(x: x, y: y))
      }
    }
    return result
  }

  /// The mean colour of the pixels in `rect` that are at least `minAlpha`
  /// opaque, and how many there are; nil when there are none. Over a clear
  /// background this is the colour of whatever was drawn there, such as text.
  func ink(in rect: NSRect, of view: NSView, minAlpha: CGFloat = 0.25) -> (
    color: NSColor, pixels: Int
  )? {
    var red: CGFloat = 0
    var green: CGFloat = 0
    var blue: CGFloat = 0
    var count = 0
    for color in colors(in: rect, of: view) where color.alphaComponent >= minAlpha {
      red += color.redComponent
      green += color.greenComponent
      blue += color.blueComponent
      count += 1
    }
    guard count > 0 else { return nil }
    let n = CGFloat(count)
    return (NSColor(srgbRed: red / n, green: green / n, blue: blue / n, alpha: 1), count)
  }

  /// The bitmap columns and rows `rect`, in the root's coordinates, covers. The
  /// scale is read from the bitmap, since a Retina host captures at 2.
  private func pixelRange(of rect: NSRect) -> (x: Range<Int>, y: Range<Int>) {
    let bounds = root.bounds
    let scaleX = CGFloat(bitmap.pixelsWide) / bounds.width
    let scaleY = CGFloat(bitmap.pixelsHigh) / bounds.height
    // Bitmap rows count down from the top; an unflipped root's y counts up.
    let top = root.isFlipped ? rect.minY - bounds.minY : bounds.maxY - rect.maxY
    let x0 = clamp(Int(((rect.minX - bounds.minX) * scaleX).rounded(.down)), bitmap.pixelsWide)
    let x1 = clamp(Int(((rect.maxX - bounds.minX) * scaleX).rounded(.up)), bitmap.pixelsWide)
    let y0 = clamp(Int((top * scaleY).rounded(.down)), bitmap.pixelsHigh)
    let y1 = clamp(Int(((top + rect.height) * scaleY).rounded(.up)), bitmap.pixelsHigh)
    return (x0..<max(x0, x1), y0..<max(y0, y1))
  }

  private func clamp(_ value: Int, _ limit: Int) -> Int {
    min(max(value, 0), limit)
  }

  /// The pixel at bitmap column `x` and row `y`, as an sRGB colour.
  ///
  /// `colorAt(x:y:)` tags the samples by the bitmap's `colorSpaceName`, which
  /// for this capture is calibrated (Generic) RGB, not by its own sRGB
  /// `colorSpace`; converting that tag to sRGB shifts saturated and dark
  /// colours by up to about 0.12. So the components `colorAt` decodes are
  /// re-tagged in `bitmap.colorSpace` before the conversion.
  private func srgb(x: Int, y: Int) -> NSColor {
    guard let sample = bitmap.colorAt(x: x, y: y) else {
      preconditionFailure("RenderedView: pixel (\(x), \(y)) has no sRGB colour")
    }
    let space = bitmap.colorSpace
    let count = sample.numberOfComponents
    guard count == space.numberOfColorComponents + 1 else {
      preconditionFailure(
        "RenderedView: pixel (\(x), \(y)) has \(count) components, but the bitmap's "
          + "colour space has \(space.numberOfColorComponents) plus alpha")
    }
    var components = [CGFloat](repeating: 0, count: count)
    sample.getComponents(&components)
    let retagged = NSColor(colorSpace: space, components: components, count: count)
    guard let color = retagged.usingColorSpace(.sRGB) else {
      preconditionFailure("RenderedView: pixel (\(x), \(y)) has no sRGB colour")
    }
    return color
  }
}

/// The Euclidean distance between two colours over their sRGB red, green and
/// blue, ignoring alpha: 0 for identical colours, about 1.73 for black and
/// white. Pass resolved colours; a dynamic one is resolved against whatever
/// drawing appearance is current.
func colorDistance(_ a: NSColor, _ b: NSColor) -> CGFloat {
  guard let a = a.usingColorSpace(.sRGB), let b = b.usingColorSpace(.sRGB) else {
    preconditionFailure("colorDistance: \(a) or \(b) has no sRGB form")
  }
  let red = a.redComponent - b.redComponent
  let green = a.greenComponent - b.greenComponent
  let blue = a.blueComponent - b.blueComponent
  return (red * red + green * green + blue * blue).squareRoot()
}

extension NSTableView {
  /// Sets `isEmphasized` on every row view the table has, which is what the
  /// table does itself when it takes or resigns first responder in a key window.
  /// A never-shown window is never key, so a test drives emphasis here.
  func setRowsEmphasized(_ emphasized: Bool) {
    enumerateAvailableRowViews { rowView, _ in
      rowView.isEmphasized = emphasized
    }
  }
}
