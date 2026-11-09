// A hosted PaneContainerView over a live core, with key and wake helpers for terminal tests.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A `PaneContainerView` over a `LiveCore`, hosted offscreen, that the drawer and
/// terminal-pane flow tests drive with keys and render wakes.
@MainActor
final class TerminalViewFixture {
  /// The hosting window's size in points: tall enough that the drawer strip, a third of
  /// it, lies wholly in the lower half.
  static let windowSize = NSSize(width: 960, height: 640)
  /// How long a wake, a publish, a split or a focus move gets to take effect.
  static let timeout: TimeInterval = 15

  /// `FfiKeyEvent.kind` of a printable character.
  private static let kindChar: UInt8 = 0
  /// `FfiKeyEvent.kind` of the Return key.
  private static let kindEnter: UInt8 = 1

  let live: LiveCore
  private(set) var container: PaneContainerView
  private(set) var renderer: OffscreenRenderer

  /// Starts a core and hosts a container over its current tabpage, then wakes it until its
  /// first pane is bound; `nil`, with the test failed, when any step does not happen.
  init?(file: StaticString = #filePath, line: UInt = #line) {
    guard let live = LiveCore.start(file: file, line: line) else { return nil }
    self.live = live
    let handle = live.handle
    container = PaneContainerView(
      handle: handle, metrics: live.metrics, tabpageHandle: handle.current_tabpage(),
      optionAsAlt: false)
    renderer = OffscreenRenderer(hosting: container, size: Self.windowSize)
    let container = self.container
    guard
      live.waitFor(
        timeout: Self.timeout,
        {
          container.onRenderWake()
          return container.paneCount == 1 && handle.active_win() != 0
        })
    else {
      XCTFail("the container never bound its first pane", file: file, line: line)
      live.shutdown()
      return nil
    }
  }

  var handle: WolfCoreHandle { live.handle }

  /// The terminal views the container hosts.
  var terminalViews: [TerminalPaneView] {
    container.subviews.compactMap { $0 as? TerminalPaneView }
  }

  /// The terminal view showing `session`, if one is hosted.
  func view(showing session: UInt64) -> TerminalPaneView? {
    terminalViews.first { $0.session == session }
  }

  /// Whether `view` lies in the lower half of the container, where the drawer strip is:
  /// flipped coordinates, so a larger y is lower.
  func isInDrawerStrip(_ view: NSView) -> Bool {
    view.frame.minY >= container.bounds.height / 2
  }

  /// The window whose buffer is `session`'s placeholder, or 0.
  func window(showing session: UInt64) -> Int64 {
    handle.windows().map { $0.win() }.first { handle.win_buffer($0).terminal() == session } ?? 0
  }

  /// Every row of `session`'s frame, joined with newlines.
  func text(of session: UInt64) -> String {
    let frame = handle.terminal_frame(session)
    return (0..<frame.rows()).map { row in
      frame.row_cells(row).map { cell -> String in
        let text = cell.text().toString()
        return text.isEmpty ? " " : text
      }.joined()
    }.joined(separator: "\n")
  }

  /// Runs render wakes until `condition` holds or the timeout passes, and returns whether
  /// it held.
  @discardableResult
  func wake(until condition: () -> Bool) -> Bool {
    let container = self.container
    return live.waitFor(timeout: Self.timeout) {
      container.onRenderWake()
      return condition()
    }
  }

  /// A key event: a printable character, or Return for `"\r"`.
  private func key(_ char: Character, ctrl: Bool = false) -> FfiKeyEvent {
    if char == "\r" {
      return FfiKeyEvent(
        kind: Self.kindEnter, ch: 0, fnum: 0, ctrl: false, shift: false, alt: false, meta: false)
    }
    return FfiKeyEvent(
      kind: Self.kindChar, ch: char.unicodeScalars.first?.value ?? 0, fnum: 0, ctrl: ctrl,
      shift: false, alt: false, meta: false)
  }

  /// Types `line` into `session` as text, then presses Return through `terminal_key`.
  func runLine(_ line: String, in session: UInt64) {
    handle.terminal_text(session, line)
    handle.terminal_key(session, key("\r"))
  }

  /// Sends `<C-w>` then `next` to `session`, through core's router.
  func terminalChord(_ next: Character, to session: UInt64) {
    handle.terminal_key(session, key("w", ctrl: true))
    handle.terminal_key(session, key(next))
  }

  /// Sends `<C-w>` then `next` to Neovim, as an editor pane's keys go.
  func neovimChord(_ next: Character) {
    handle.send_key(key("w", ctrl: true))
    handle.send_key(key(next))
  }

  /// Types `:command` and Return into Neovim.
  func typeEx(_ command: String) {
    for char in ":" + command { handle.send_key(key(char)) }
    handle.send_key(key("\r"))
  }

  /// One more wake, then a capture.
  func render() -> RenderedView {
    container.onRenderWake()
    return renderer.render()
  }

  func shutdown() {
    live.shutdown()
  }
}
