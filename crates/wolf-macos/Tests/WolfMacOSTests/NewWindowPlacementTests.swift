// Checks that a new window cascaded from the active window takes its size and moves off its corner.

import AppKit
import XCTest

@testable import WolfMacOS

/// A window, never shown, that stands in for AppKit's `cascadeTopLeft(from:)` and records each
/// call with the window's size at the time.
///
/// It stands in because the worker has no screen: AppKit's own call then clamps the window to
/// the missing screen, squeezing it to 1 pt wide with its top-left pinned at (0, 0), and answers
/// (29, −29) whatever point it gets. The stand-in does what AppKit documents and needs no
/// screen: it puts the top-left at the point and answers a point `offsetRight` right and
/// `offsetDown` down. The two amounts differ, so a swapped axis shows.
@MainActor
private final class RecordingWindow: NSWindow {
  /// How far right of the point the stand-in's answer lies.
  static let offsetRight: CGFloat = 21
  /// How far below the point the stand-in's answer lies.
  static let offsetDown: CGFloat = 23

  /// Each `cascadeTopLeft(from:)` call's point and the window's frame size when it was made.
  private(set) var calls: [(point: CGPoint, size: CGSize)] = []

  /// The points of `calls`, in order.
  var points: [CGPoint] { calls.map(\.point) }

  /// The frame's top-left, in screen coordinates.
  var topLeft: CGPoint { CGPoint(x: frame.minX, y: frame.maxY) }

  /// A titled, resizable window over a 900 × 600 content rect at the origin. Neither its size
  /// nor its top-left is one the tests expect, so a cascade that changes nothing cannot pass.
  static func make() -> RecordingWindow {
    // AppKit's drawing machinery is set up by the shared application.
    _ = NSApplication.shared
    return RecordingWindow(
      contentRect: NSRect(x: 0, y: 0, width: 900, height: 600),
      styleMask: [.titled, .resizable], backing: .buffered, defer: true)
  }

  override func cascadeTopLeft(from topLeftPoint: NSPoint) -> NSPoint {
    calls.append((point: topLeftPoint, size: frame.size))
    setFrameTopLeftPoint(topLeftPoint)
    return NSPoint(x: topLeftPoint.x + Self.offsetRight, y: topLeftPoint.y - Self.offsetDown)
  }
}

/// `NSWindow.cascade(fromTopLeft:size:)`, the §7 placement of a new window
/// (`specs/application-state.md`): the new window takes the active window's normal size and its
/// top-left lands one cascade step down and right of the active window's.
///
/// The active window is stood in for by its top-left, `activeTopLeft`, and its normal size,
/// `activeSize`, which are all the cascade reads. Each test first checks the premise that the
/// built window has neither that size nor that top-left.
@MainActor
final class NewWindowPlacementTests: XCTestCase {
  /// The active window's top-left, in screen coordinates.
  private static let activeTopLeft = CGPoint(x: 120, y: 800)
  /// The active window's normal size, which the new window takes.
  private static let activeSize = CGSize(width: 1120, height: 700)
  /// Where one stand-in cascade step puts a window from `activeTopLeft`.
  private static let cascadedTopLeft = CGPoint(
    x: activeTopLeft.x + RecordingWindow.offsetRight,
    y: activeTopLeft.y - RecordingWindow.offsetDown)
  /// The largest difference, in points, a coordinate may have from the one expected.
  private static let tolerance: CGFloat = 0.5

  /// A recording window, after failing the test as a premise failure when the built window
  /// already has `activeSize` in either dimension or its top-left is `activeTopLeft` or
  /// `cascadedTopLeft`; `nil` then.
  private func builtWindow(file: StaticString = #filePath, line: UInt = #line)
    -> RecordingWindow?
  {
    let window = RecordingWindow.make()
    let built = window.frame
    let differs =
      abs(built.width - Self.activeSize.width) > Self.tolerance
      && abs(built.height - Self.activeSize.height) > Self.tolerance
      && window.topLeft != Self.activeTopLeft && window.topLeft != Self.cascadedTopLeft
      && window.calls.isEmpty
    XCTAssertTrue(
      differs,
      "premise: the built window's frame \(built) must differ from \(Self.activeSize) in both"
        + " dimensions, have a top-left other than \(Self.activeTopLeft) and"
        + " \(Self.cascadedTopLeft), and have no cascade recorded (\(window.calls.count) are),"
        + " or a cascade that changes nothing passes",
      file: file, line: line)
    return differs ? window : nil
  }

  /// W1a and W1b.
  func testCascadeSizesTheWindow() {
    guard let window = builtWindow() else { return }

    window.cascade(fromTopLeft: Self.activeTopLeft, size: Self.activeSize)
    guard let first = window.calls.first else {
      XCTFail(
        "the cascade must call cascadeTopLeft(from:) (§7), but it never did; the window is"
          + " placed some other way, at \(window.frame)")
      return
    }
    XCTAssertEqual(
      first.size.width, Self.activeSize.width, accuracy: Self.tolerance,
      "cascadeTopLeft(from:) must first run at the new size \(Self.activeSize), since it"
        + " constrains the frame to the screen at the size the frame has then, but it ran at"
        + " \(first.size): the size is set after the cascade, or never")
    XCTAssertEqual(
      first.size.height, Self.activeSize.height, accuracy: Self.tolerance,
      "cascadeTopLeft(from:) must first run at the new size \(Self.activeSize), since it"
        + " constrains the frame to the screen at the size the frame has then, but it ran at"
        + " \(first.size): the size is set after the cascade, or never")
    let size = window.frame.size
    XCTAssertEqual(
      size.width, Self.activeSize.width, accuracy: Self.tolerance,
      "the new window must be as wide as the active window's normal frame (§7), but its frame"
        + " is \(window.frame); the built width means the cascade never sets the size")
    XCTAssertEqual(
      size.height, Self.activeSize.height, accuracy: Self.tolerance,
      "the new window must be as tall as the active window's normal frame (§7), but its frame"
        + " is \(window.frame); the built height means the cascade never sets the size")
  }

  /// W3a and W3b.
  func testCascadeOffsetsTheNewWindowFromTheActiveWindowsTopLeft() {
    guard let window = builtWindow() else { return }

    window.cascade(fromTopLeft: Self.activeTopLeft, size: Self.activeSize)
    XCTAssertEqual(
      window.points, [Self.activeTopLeft, Self.cascadedTopLeft],
      "the cascade must call cascadeTopLeft(from:) on the active window's top-left and then on"
        + " the point it answers (§7). No calls means the window is placed without a cascade;"
        + " one means the second step is skipped; a first point other than"
        + " \(Self.activeTopLeft) means it starts from elsewhere, such as the bottom-left")
    let topLeft = window.topLeft
    let message =
      "the new window's top-left must be one cascade step, \(Self.cascadedTopLeft), from the"
      + " active window's \(Self.activeTopLeft) (§7), but it is \(topLeft). At"
      + " \(Self.activeTopLeft) the new window covers the active window exactly; anywhere else,"
      + " it is not placed by the point cascadeTopLeft(from:) answers"
    XCTAssertEqual(topLeft.x, Self.cascadedTopLeft.x, accuracy: Self.tolerance, message)
    XCTAssertEqual(topLeft.y, Self.cascadedTopLeft.y, accuracy: Self.tolerance, message)
  }
}
