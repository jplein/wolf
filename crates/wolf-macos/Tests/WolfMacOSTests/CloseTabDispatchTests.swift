// Checks that Close Tab asks its window first and that Close Window closes without performClose.

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

/// `CommandDispatcher.dispatch` for `.closeTab` and `.closeWindow`. Close Tab goes through
/// `performClose(_:)`, so `windowShouldClose(_:)` asks the tab's Neovim, or lets a drained
/// `close_tab` through. Close Window hands the window to `AppDelegate.closeWindow(_:)`, which
/// asks the tabs' Neovims itself and then closes the tab group with `close()`, which no
/// `windowShouldClose` refuses. It needs no core and no screen.
@MainActor
final class CloseTabDispatchTests: XCTestCase {
  /// D1a and D1b.
  func testCloseTabAsksItsWindowFirst() {
    let window = RecordingWindow.make()
    XCTAssertEqual(window.calls, [], "premise: a built window must have recorded no call")

    CommandDispatcher.dispatch(
      .closeTab, window: window, coordinator: nil, originatingCwd: "", controller: nil)

    XCTAssertEqual(
      window.calls, [.performClose],
      ".closeTab must ask its window once through performClose: [close] means it closes"
        + " without windowShouldClose, so the menu and palette skip the unsaved check; [] means"
        + " the arm does nothing; more than one call means it asks twice")
  }

  /// CW18. A bare `AppDelegate` owns no live controller, so no tab is asked and every tab
  /// counts as clean: Close Window must still close the window. Premise: a window never shown
  /// has a `tabGroup` that lists it, or none, so its group is the window alone either way.
  func testCloseWindowClosesWithoutAsking() {
    let window = RecordingWindow.make()
    let coordinator = AppDelegate()
    XCTAssertEqual(window.calls, [], "premise: a built window must have recorded no call")

    CommandDispatcher.dispatch(
      .closeWindow, window: window, coordinator: coordinator, originatingCwd: "",
      controller: nil)

    XCTAssertEqual(
      window.calls, [.close],
      ".closeWindow must close its window once, without performClose: [performClose] means"
        + " windowShouldClose would ask again after Close Window's own check; [] means the arm"
        + " or AppDelegate.closeWindow(_:) closed nothing for a window no live controller owns,"
        + " so an unasked tab counts as unsaved; two calls mean the group was closed twice")
  }
}
