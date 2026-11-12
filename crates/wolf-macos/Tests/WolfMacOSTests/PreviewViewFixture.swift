// A Markdown preview as the active pane of a split, over a live core, for key tests.

import AppKit
import WebKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A `TerminalViewFixture` whose container shows a Markdown preview as the active pane of a
/// two-pane split, its source in the other pane, hosted in a real `WKWebView`.
///
/// The page is never waited for, and no test may depend on it: where the gate runs, the
/// worker's Seatbelt profile keeps WebKit from issuing the sandbox extensions its WebContent
/// process needs, so no page loads and no script runs. The web view is still a real one, with
/// its real responder chain. Every step that does not happen fails the calling test with a
/// `premise:` message and makes `init` return `nil`.
@MainActor
final class PreviewViewFixture {
  /// The fixture the container, window and core come from.
  let base: TerminalViewFixture
  /// The preview's web view, the only `WKWebView` the container hosts.
  let web: WKWebView
  /// The preview's window handle.
  let previewWin: Int64
  /// The absolute path of `probe.md`, the previewed source.
  let probePath: String

  var container: PaneContainerView { base.container }
  var handle: WolfCoreHandle { base.handle }
  var window: OffscreenWindow { base.renderer.window }

  /// Splits the editor and previews `probe.md` in the new pane, which becomes the active pane.
  init?(file: StaticString = #filePath, line: UInt = #line) {
    guard let base = TerminalViewFixture(file: file, line: line) else { return nil }
    let handle = base.handle
    let container = base.container
    var cwd = ""
    guard
      base.live.waitFor(
        timeout: TerminalViewFixture.timeout,
        {
          cwd = handle.cwd().toString()
          return !cwd.isEmpty
        })
    else {
      XCTFail("premise: Neovim never reported its working directory", file: file, line: line)
      base.shutdown()
      return nil
    }
    let probePath = (cwd as NSString).appendingPathComponent("probe.md")
    let probe = "# Probe\n\nA short document to preview.\n"
    do {
      try probe.write(toFile: probePath, atomically: true, encoding: .utf8)
    } catch {
      XCTFail("premise: could not write \(probePath): \(error)", file: file, line: line)
      base.shutdown()
      return nil
    }

    base.typeEx("edit \(probePath)")
    guard base.wake(until: { handle.is_markdown_active() }) else {
      XCTFail(
        "premise: :edit \(probePath) never made a Markdown buffer active", file: file, line: line)
      base.shutdown()
      return nil
    }
    base.neovimChord("v")
    guard base.wake(until: { container.paneCount == 2 }) else {
      XCTFail(
        "premise: <C-w>v must split the editor into two panes: \(container.paneCount)",
        file: file, line: line)
      base.shutdown()
      return nil
    }
    let scratch = handle.open_preview()
    var webViews: [WKWebView] = []
    let previewed =
      scratch > 0
      && base.wake(until: {
        webViews = container.subviews.compactMap { $0 as? WKWebView }
        return webViews.count == 1 && container.activePaneIsPreview
      })
    guard previewed, let web = webViews.first else {
      XCTFail(
        "premise: open_preview() (scratch \(scratch)) must leave one hosted WKWebView and the"
          + " preview as the active pane: \(webViews.count) web view(s), preview active"
          + " \(container.activePaneIsPreview)", file: file, line: line)
      base.shutdown()
      return nil
    }

    // `previews()` yields borrowed refs valid only in the loop body, so copy the scalars out.
    var previewWin: Int64 = 0
    var previewCount = 0
    for preview in handle.previews() {
      previewWin = preview.win()
      previewCount += 1
    }
    let others = handle.windows().map { $0.win() }.filter { $0 != previewWin }
    guard previewCount == 1, previewWin == container.activeWin, others.count == 1 else {
      XCTFail(
        "premise: one preview, the active pane, beside one source window: \(previewCount)"
          + " preview(s), preview window \(previewWin), active window \(container.activeWin),"
          + " other windows \(others)", file: file, line: line)
      base.shutdown()
      return nil
    }

    self.base = base
    self.web = web
    self.previewWin = previewWin
    self.probePath = probePath
  }

  func shutdown() {
    base.shutdown()
  }
}
