// Opens the terminal drawer over a showing message toast and checks the toast view hides.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

@MainActor
final class TerminalToastTests: XCTestCase {

  /// The message the toast shows. Typed as two quoted halves, so only the `:echo` running puts
  /// it on the toast, never the echo of the typed command line.
  private let marker = "zzdrawertoastyankee"
  private let echoCommand = "echo 'zzdrawertoast' . 'yankee'"

  /// The container's message-toast view.
  private func toastView(_ fixture: TerminalViewFixture) -> ToastView? {
    fixture.container.subviews.compactMap { $0 as? ToastView }.first
  }

  /// Core's toast lines, each its chunks' text, joined with newlines.
  private func toastText(_ fixture: TerminalViewFixture) -> String {
    let lines = fixture.handle.toast().lines()
    return (0..<lines.len()).map { index in
      let chunks = lines[index].chunks()
      return (0..<chunks.len()).map { chunks[$0].text().toString() }.joined()
    }.joined(separator: "\n")
  }

  /// With a toast showing, `toggle_terminal()`, which the palette's Terminal row runs, opens
  /// the drawer and the container hides its toast view on the following wakes.
  func testOpeningTheDrawerHidesAShowingToast() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    guard let toast = toastView(fixture) else {
      XCTFail("premise: the container must host a ToastView among its subviews")
      return
    }

    fixture.typeEx(echoCommand)
    let shown = fixture.wake { !toast.isHidden && self.toastText(fixture).contains(self.marker) }
    XCTAssertTrue(
      shown,
      "premise: after `:\(echoCommand)` the toast view must show \(marker); hidden"
        + " \(toast.isHidden), core's toast reads \(toastText(fixture).debugDescription)")
    guard shown else { return }

    handle.toggle_terminal()
    var session: UInt64 = 0
    let opened = fixture.wake {
      session = Array(handle.live_terminals()).first ?? 0
      guard session != 0, let view = fixture.view(showing: session) else { return false }
      return fixture.isInDrawerStrip(view)
    }
    XCTAssertTrue(
      opened,
      "premise: toggle_terminal() must show the drawer's session in the bottom strip; live"
        + " sessions \(Array(handle.live_terminals()))")
    guard opened else { return }

    let hidden = fixture.wake { toast.isHidden }
    XCTAssertTrue(
      hidden,
      "with the drawer open the toast view must be hidden, but it still shows; core's toast"
        + " visible \(handle.toast().visible()), reading \(toastText(fixture).debugDescription)."
        + " Visible in core means opening the drawer never dismissed it; hidden in core with"
        + " the view shown means the container stopped refreshing the toast on a wake")
  }
}
