// The application menu's Install and Uninstall wolf Command items, built by AppMenu.install.

import AppKit
import XCTest

@testable import WolfMacOS

/// `AppMenu.install(target:)` over a bare `AppDelegate`, as `PreviewKeyEquivalentTests` builds
/// it: the application menu carries "Install wolf Command" and "Uninstall wolf Command" of
/// `specs/cli-and-file-open.md` §1, in a group after About, each sent to the delegate.
///
/// What the actions do, the link and the administrator prompt, is `CommandInstallSeam`'s and a
/// person check's; no item is fired here.
@MainActor
final class InstallCommandMenuTests: XCTestCase {

  func testTheApplicationMenuOffersInstallAndUninstall() {
    // `NSApp` is nil until `NSApplication.shared` is first touched.
    _ = NSApplication.shared
    let delegate = AppDelegate()
    _ = AppMenu.install(target: delegate)
    defer { NSApp.mainMenu = nil }
    guard let appMenu = NSApp.mainMenu?.items.first?.submenu else {
      XCTFail("premise: AppMenu.install must set a main menu whose first item has a submenu")
      return
    }
    let titles = appMenu.items.map(\.title)
    let expected: [(String, Selector)] = [
      ("Install wolf Command", #selector(AppDelegate.installCommandMenuItem(_:))),
      ("Uninstall wolf Command", #selector(AppDelegate.uninstallCommandMenuItem(_:))),
    ]
    for (title, action) in expected {
      guard let item = appMenu.items.first(where: { $0.title == title }) else {
        XCTFail("the application menu must offer \(title); it holds \(titles)")
        continue
      }
      XCTAssertTrue(
        item.target === delegate,
        "\(title) must target the app delegate; \(String(describing: item.target)) means the"
          + " action reaches some other object or the responder chain")
      XCTAssertEqual(
        item.action, action,
        "\(title) must send \(action); another selector runs another command")
    }

    guard let about = titles.firstIndex(of: "About Wolf"),
      let install = titles.firstIndex(of: "Install wolf Command"),
      let uninstall = titles.firstIndex(of: "Uninstall wolf Command")
    else { return }
    XCTAssertTrue(
      about < install && uninstall == install + 1,
      "the two items must follow About, Install then Uninstall, side by side: \(titles)")
    XCTAssertTrue(
      appMenu.items[install - 1].isSeparatorItem
        && appMenu.items.indices.contains(uninstall + 1)
        && appMenu.items[uninstall + 1].isSeparatorItem,
      "the two items must form a group of their own, between separators: \(titles)")
  }
}
