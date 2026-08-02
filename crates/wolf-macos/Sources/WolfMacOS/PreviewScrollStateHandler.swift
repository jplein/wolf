//
//  PreviewScrollStateHandler.swift
//  The `WKScriptMessageHandler` glue that caches a preview page's scroll state.
//
//  A thin AppKit/WebKit adapter (compile + manual-smoke only), the exact structural
//  sibling of `PreviewClickFocusHandler`. The document-start scroll script
//  (`PreviewScrollSeam.stateScriptJS`) PUSHES the page's live scroll state out to
//  this handler — on every cursor follow, on every rAF-throttled user scroll, and
//  once at document start — and this handler forwards it into the
//  `PaneContainerView`'s per-window cache.
//
//  The push direction is load-bearing. A full `loadHTMLString` DESTROYS `window`, so
//  nothing can be stashed in the page across a reload, and `evaluateJavaScript` is
//  asynchronous so the state cannot be PULLED synchronously in the instant before
//  the reload. Keeping a continuously refreshed copy on the Swift side makes the
//  pre-reload capture a plain dictionary read with no race and no blocking.
//
//  ALL the scroll arithmetic and every JS string live in the headless
//  `WolfKeyMap.PreviewScrollSeam`; this file only decodes WebKit's message body.
//
//  The web view's `WKUserContentController` retains this handler STRONGLY; this
//  handler holds the container WEAKLY, so there is no retain cycle. The callback
//  fires on the main thread (WebKit's script-message callbacks are main-thread).
//

import WebKit
import WolfKeyMap

/// Routes a preview web view's pushed scroll state (posted by
/// `PreviewScrollSeam.stateScriptJS` under `PreviewScrollSeam.stateHandlerName`)
/// into its `PaneContainerView`'s per-window cache.
final class PreviewScrollStateHandler: NSObject, WKScriptMessageHandler {
  /// The preview WINDOW handle this handler serves (identifies the pane in the
  /// container's per-window maps).
  let win: Int64
  /// The owning container, held WEAKLY (its content controller retains this
  /// handler — no cycle).
  weak var container: PaneContainerView?

  init(win: Int64, container: PaneContainerView) {
    self.win = win
    self.container = container
    super.init()
  }

  /// The page pushed its scroll state: `{ a: <cursor anchor's viewport-relative Y,
  /// or null when unresolved>, s: <window.scrollY> }`. A body that is not the
  /// expected dictionary is ignored; a missing `s` degrades to `0` rather than
  /// dropping the message, so a partially decodable push still refreshes the cache.
  func userContentController(
    _ userContentController: WKUserContentController,
    didReceive message: WKScriptMessage
  ) {
    guard let body = message.body as? [String: Any] else { return }
    let state = PreviewScrollSeam.ScrollState(
      anchorTop: body["a"] as? Double,
      scrollY: body["s"] as? Double ?? 0)
    container?.recordPreviewScrollState(win: win, state: state)
  }
}
