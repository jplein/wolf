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

  /// The background `markEditor()` gives the marked rows. No default colour core gives
  /// Neovim or a session is near it.
  static let mark = NSColor(srgbRed: 220.0 / 255, green: 20.0 / 255, blue: 60.0 / 255, alpha: 1)

  /// The `:lua` line `markEditor()` types: 80 lines in the current buffer, each with a
  /// `line_hl_group` extmark in `mark`, so every window showing the buffer paints `mark`
  /// across its whole grid width. Not `Normal`, whose colours core pushes into every session
  /// too. It holds no `|`, which would end the `:lua` command.
  private static let markLine =
    "lua local b=vim.api.nvim_get_current_buf()"
    + " local ns=vim.api.nvim_create_namespace('wolf_test_mark')"
    + " vim.api.nvim_set_hl(0,'WolfTestMark',{bg='#DC143C'})"
    + " local l={} for i=1,80 do l[i]='' end vim.api.nvim_buf_set_lines(b,0,-1,false,l)"
    + " for i=0,79 do vim.api.nvim_buf_set_extmark(b,ns,i,0,{line_hl_group='WolfTestMark'}) end"
    + " vim.bo[b].modified=false"

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
    terminalViews(in: container)
  }

  /// The terminal views `host` hosts.
  func terminalViews(in host: PaneContainerView) -> [TerminalPaneView] {
    host.subviews.compactMap { $0 as? TerminalPaneView }
  }

  /// The terminal view showing `session`, if one is hosted.
  func view(showing session: UInt64) -> TerminalPaneView? {
    view(showing: session, in: container)
  }

  /// The terminal view of `host` showing `session`, if one is hosted.
  func view(showing session: UInt64, in host: PaneContainerView) -> TerminalPaneView? {
    terminalViews(in: host).first { $0.session == session }
  }

  /// A second container over `tabpage`, hosted in its own offscreen window at `windowSize`.
  /// Wake it only while `tabpage` is current, as the app wakes only its current view.
  func hostContainer(tabpage: Int64) -> (container: PaneContainerView, renderer: OffscreenRenderer)
  {
    let other = PaneContainerView(
      handle: handle, metrics: live.metrics, tabpageHandle: tabpage, optionAsAlt: false)
    return (other, OffscreenRenderer(hosting: other, size: Self.windowSize))
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
    wake(container, until: condition)
  }

  /// Runs render wakes of `host` until `condition` holds or the timeout passes, and returns
  /// whether it held.
  @discardableResult
  func wake(_ host: PaneContainerView, until condition: () -> Bool) -> Bool {
    live.waitFor(timeout: Self.timeout) {
      host.onRenderWake()
      return condition()
    }
  }

  /// How many pixels of `rect`, in `view`'s coordinates, `capture` drew within 0.05 of
  /// `mark`.
  func markedPixels(in rect: NSRect, of view: NSView, _ capture: RenderedView) -> Int {
    capture.colors(in: rect, of: view).filter { colorDistance($0, Self.mark) <= 0.05 }.count
  }

  /// Four cells' area: a `markedPixels` count at or above it means marked rows paint there.
  var markThreshold: Int {
    Int(4 * live.metrics.cellWidth * live.metrics.cellHeight)
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

  /// Marks Neovim's current buffer with `markLine`: every window showing it then paints rows
  /// in `mark`.
  func markEditor() {
    typeEx(Self.markLine)
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
