// The File menu's Close Pane item, built by AppMenu.install, and its validation.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// `AppMenu.install(target:)` over a bare `AppDelegate`, as `CloseTabMenuTests` builds it: the
/// File menu carries Close Pane on ⌘W, it dispatches `close_pane` through
/// `handleCommandMenuItem(_:)`, and `validateMenuItem(_:)` enables it.
///
/// No live core runs here, and none is needed. Close Pane's validation reads no pane state, so
/// the answer a bare delegate gives is the one a one-pane tab gets: on the tree's last leaf
/// only Neovim knows whether to close the tab or refuse. AppKit makes no window key offscreen,
/// so a delegate over a live window would find no key controller either. The one-pane close
/// itself is `TerminalClosePaneTests`'.
@MainActor
final class ClosePaneMenuTests: XCTestCase {

  func testTheFileMenuOffersClosePaneOnCommandWAndEnablesIt() {
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
    guard let item = fileMenu.items.first(where: { $0.title == "Close Pane" }) else {
      XCTFail(
        "the File menu must offer Close Pane; it holds \(titles). Close alone means the item"
          + " still has the registry's old title")
      return
    }

    XCTAssertEqual(
      CommandKind(ffi: UInt8(truncatingIfNeeded: item.tag)), .closePane,
      "Close Pane's tag \(item.tag) must decode to .closePane: .closeTab or .closeWindow means"
        + " ⌘W runs another close, nil that the tag is no command_kind")
    XCTAssertEqual(
      item.keyEquivalent, "w",
      "Close Pane must be on ⌘W; \(item.keyEquivalent) means the chord moved to another item")
    XCTAssertEqual(
      item.keyEquivalentModifierMask, [.command],
      "Close Pane must be on ⌘W alone; \(item.keyEquivalentModifierMask) takes ⇧⌘W, Close"
        + " Window's chord, or another")
    XCTAssertTrue(
      item.target === delegate,
      "Close Pane must target the app delegate; \(String(describing: item.target)) means the"
        + " action reaches some other object or the responder chain")
    XCTAssertEqual(
      item.action, #selector(AppDelegate.handleCommandMenuItem(_:)),
      "Close Pane must send handleCommandMenuItem(_:); another selector runs another command")

    XCTAssertTrue(
      delegate.validateMenuItem(item),
      "validateMenuItem must enable Close Pane with no pane state to read, as on a one-pane"
        + " tab; false means it still gates the item on the pane count, so ⌘W on the tab's last"
        + " pane never reaches core")
  }
}
