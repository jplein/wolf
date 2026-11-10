// Pins OffscreenRenderer to known sRGB colours drawn by a plain view, so a sampling misread shows.

import AppKit
import XCTest

/// A plain view that fills its bounds with equal-width vertical stripes, left to right, each
/// with `NSRect.fill()` in its colour.
private final class StripeView: NSView {
  let stripes: [NSColor]

  init(frame: NSRect, stripes: [NSColor]) {
    self.stripes = stripes
    super.init(frame: frame)
  }

  required init?(coder: NSCoder) {
    preconditionFailure("StripeView is built in code only")
  }

  /// The rect of stripe `index`, in this view's coordinates.
  func stripeRect(_ index: Int) -> NSRect {
    let width = bounds.width / CGFloat(stripes.count)
    return NSRect(
      x: bounds.minX + CGFloat(index) * width, y: bounds.minY, width: width,
      height: bounds.height)
  }

  override func draw(_ dirtyRect: NSRect) {
    for (index, color) in stripes.enumerated() {
      color.setFill()
      stripeRect(index).fill()
    }
  }
}

@MainActor
final class OffscreenRendererTests: XCTestCase {

  /// Every component is a whole number of 255ths, so 8-bit storage holds each exactly and any
  /// distance over the tolerance is the sampler's, not the bitmap's.
  private static let colors: [(name: String, color: NSColor)] = [
    ("crimson", NSColor(srgbRed: 220 / 255, green: 20 / 255, blue: 60 / 255, alpha: 1)),
    ("pure blue", NSColor(srgbRed: 0, green: 0, blue: 1, alpha: 1)),
    ("dark grey", NSColor(srgbRed: 0.2, green: 0.2, blue: 0.2, alpha: 1)),
  ]

  /// The largest sRGB distance a sampled pixel may be from the colour drawn there.
  private static let tolerance: CGFloat = 0.01

  /// A plain view's crimson, pure blue and dark grey stripes read back as those sRGB colours
  /// at every pixel sampled inside each stripe.
  func testSampledPixelsMatchTheSRGBColoursDrawn() {
    let size = NSSize(width: 60, height: 20)
    let view = StripeView(
      frame: NSRect(origin: .zero, size: size), stripes: Self.colors.map(\.color))
    let renderer = OffscreenRenderer(hosting: view, size: size)
    let capture = renderer.render()

    for (index, expected) in Self.colors.enumerated() {
      // An inset of one point keeps the antialiased stripe edges out of the sample.
      let rect = view.stripeRect(index).insetBy(dx: 1, dy: 1)
      let pixels = capture.colors(in: rect, of: view)
      XCTAssertFalse(
        pixels.isEmpty,
        "the \(expected.name) stripe at \(rect) sampled no pixels: the rect fell outside the"
          + " capture, so the colour check below would pass vacuously")

      let measured = pixels.map { (distance: colorDistance($0, expected.color), pixel: $0) }
      guard let worst = measured.max(by: { $0.distance < $1.distance }) else { continue }
      XCTAssertLessThanOrEqual(
        worst.distance, Self.tolerance,
        "the \(expected.name) stripe's worst pixel is sRGB \(Self.describe(worst.pixel)),"
          + " \(worst.distance) from the drawn \(Self.describe(expected.color)): a distance"
          + " near 0.1 means the samples were converted from colorAt's calibrated tag instead"
          + " of the bitmap's own sRGB colorSpace, and one near 1 means the stripe was read at"
          + " the wrong place")
    }
  }

  /// `color`'s sRGB red, green and blue, to three places.
  private static func describe(_ color: NSColor) -> String {
    guard let srgb = color.usingColorSpace(.sRGB) else { return "\(color) (no sRGB form)" }
    let components = [srgb.redComponent, srgb.greenComponent, srgb.blueComponent]
    return "(" + components.map { String(format: "%.3f", $0) }.joined(separator: ", ") + ")"
  }
}
