// The File menu's Close Tab item, built by AppMenu.install.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// `AppMenu.install(target:)` over a bare `AppDelegate`, as `InstallCommandMenuTests` builds it:
/// the File menu carries Close Tab between Close and Close Window, with no shortcut, and it
/// dispatches `close_tab` through `handleCommandMenuItem(_:)`. No item is fired here; what the
/// dispatch does is `CloseTabDispatchTests`'.
@MainActor
final class CloseTabMenuTests: XCTestCase {

  func testTheFileMenuOffersCloseTabWithoutAShortcut() {
    // `NSApp` is nil until `NSApplication.shared` is first touched.
    _ = NSApplication.shared
    let delegate = AppDelegate()
    _ = AppMenu.install(target: delegate)
    defer { NSApp.mainMenu = nil }
    guard
      let fileMenu = NSApp.mainMenu?.items.first(where: { $0.submenu?.title == "File" })?.submenu
    else {
      XCTFail("premise: AppMenu.install must set a main menu with a File submenu")
      return
    }
    let titles = fileMenu.items.map(\.title)
    guard let closeTab = titles.firstIndex(of: "Close Tab") else {
      XCTFail("the File menu must offer Close Tab; it holds \(titles)")
      return
    }
    XCTAssertTrue(
      closeTab > 0 && titles[closeTab - 1] == "Close"
        && titles.indices.contains(closeTab + 1) && titles[closeTab + 1] == "Close Window",
      "Close Tab must sit directly between Close and Close Window; the File menu holds \(titles)")

    let item = fileMenu.items[closeTab]
    XCTAssertTrue(
      item.target === delegate,
      "Close Tab must target the app delegate; \(String(describing: item.target)) means the"
        + " action reaches some other object or the responder chain")
    XCTAssertEqual(
      item.action, #selector(AppDelegate.handleCommandMenuItem(_:)),
      "Close Tab must send handleCommandMenuItem(_:); another selector runs another command")
    XCTAssertEqual(
      CommandKind(ffi: UInt8(truncatingIfNeeded: item.tag)), .closeTab,
      "Close Tab's tag \(item.tag) must decode to .closeTab: .closeWindow means it closes the"
        + " whole window, nil that the tag is no command_kind")
    XCTAssertEqual(
      item.keyEquivalent, "",
      "Close Tab must have no shortcut; \(item.keyEquivalent) with \(item.keyEquivalentModifierMask)"
        + " takes a chord the task leaves free")
  }
}
