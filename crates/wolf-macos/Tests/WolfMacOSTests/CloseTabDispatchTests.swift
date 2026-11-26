// Checks that a drained close_tab closes the tab's own window without asking it first.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A window, never shown, that records `close()` and `performClose(_:)` in the order they are
/// called.
///
/// `close()` goes on to AppKit's. `performClose(_:)` does not, since AppKit's would ask the
/// delegate and then call `close()` itself; so a dispatch that asks first records
/// `performClose` alone and is told apart from one that closes.
@MainActor
private final class RecordingWindow: NSWindow {
  /// A recorded call.
  enum Call: Equatable {
    case close
    case performClose
  }

  /// Every recorded call, in order.
  private(set) var calls: [Call] = []

  /// A titled, closable window that ARC alone releases.
  static func make() -> RecordingWindow {
    // AppKit's drawing machinery is set up by the shared application.
    _ = NSApplication.shared
    let window = RecordingWindow(
      contentRect: NSRect(x: 0, y: 0, width: 400, height: 300),
      styleMask: [.titled, .closable], backing: .buffered, defer: true)
    // The default adds an unbalanced release on close, which would free the window twice.
    window.isReleasedWhenClosed = false
    return window
  }

  override func close() {
    calls.append(.close)
    super.close()
  }

  override func performClose(_ sender: Any?) {
    calls.append(.performClose)
  }
}

/// `CommandDispatcher.dispatch(.closeTab, …)`, the arm a drained `close_tab` reaches: the quit
/// that sent it has already refused unsaved changes, so the arm closes the tab's own window
/// without asking it again. It needs no core and no screen.
@MainActor
final class CloseTabDispatchTests: XCTestCase {
  /// M1a and M1b.
  func testCloseTabClosesItsWindowWithoutAsking() {
    let window = RecordingWindow.make()
    XCTAssertEqual(window.calls, [], "premise: a built window must have recorded no call")

    CommandDispatcher.dispatch(
      .closeTab, window: window, coordinator: nil, originatingCwd: "", controller: nil)

    XCTAssertEqual(
      window.calls, [.close],
      ".closeTab must close its window once, without asking: [performClose] means it asks"
        + " windowShouldClose again, after the quit's own check; [] means the arm does nothing;"
        + " more than one call means it closes twice")
  }
}
