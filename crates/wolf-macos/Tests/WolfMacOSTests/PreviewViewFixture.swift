// A Markdown preview as the active pane, beside its source or alone, over a live core.

import AppKit
import WebKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A `TerminalViewFixture` whose container shows a Markdown preview as the active pane, hosted
/// in a real `WKWebView`: by default in a two-pane split, its source in the other pane, or with
/// `split: false` as the lone pane, which the preview replaces in place.
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

  /// Splits the editor, unless `split` is `false`, and previews `probe.md` in the current pane,
  /// which becomes the active pane, over `initLua` when given.
  init?(
    initLua: String? = nil, split: Bool = true, file: StaticString = #filePath, line: UInt = #line
  ) {
    guard let base = TerminalViewFixture(initLua: initLua, file: file, line: line) else {
      return nil
    }
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
    // The starting buffer is already Markdown and open_preview() overtakes queued keys, so the
    // wait is on the probe's path: else the preview shows the start buffer and :edit replaces it.
    var current = ""
    guard
      base.wake(until: {
        current = handle.current_file().toString()
        return current == probePath && handle.is_markdown_active()
      })
    else {
      XCTFail(
        "premise: :edit \(probePath) must make the probe the current Markdown buffer, but"
          + " current_file() reads \"\(current)\", Markdown active"
          + " \(handle.is_markdown_active())", file: file, line: line)
      base.shutdown()
      return nil
    }
    if split {
      base.neovimChord("v")
      guard base.wake(until: { container.paneCount == 2 }) else {
        XCTFail(
          "premise: <C-w>v must split the editor into two panes: \(container.paneCount)",
          file: file, line: line)
        base.shutdown()
        return nil
      }
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
    let otherCount = split ? 1 : 0
    guard previewCount == 1, previewWin == container.activeWin, others.count == otherCount else {
      XCTFail(
        "premise: one preview, the active pane, beside \(otherCount) source window(s):"
          + " \(previewCount) preview(s), preview window \(previewWin), active window"
          + " \(container.activeWin), other windows \(others)", file: file, line: line)
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
