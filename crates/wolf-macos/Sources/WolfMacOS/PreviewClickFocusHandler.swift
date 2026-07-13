//
//  PreviewClickFocusHandler.swift
//  The `WKScriptMessageHandler` glue that focuses a clicked preview pane.
//
//  A thin AppKit/WebKit adapter (compile + manual-smoke only): the preview
//  `WKWebView` swallows AppKit mouse events, so the container's `mouseDown` never
//  fires over a preview pane. The document-start capture-phase `mousedown` user
//  script (`PreviewSeam.clickFocusJS`) posts to this handler, which dispatches back
//  into the `PaneContainerView` to focus the pane. ALL of the placement/reload
//  arithmetic lives in the headless `WolfKeyMap.PreviewSeam`; this file only wires
//  WebKit's message callback to the container.
//
//  The web view's `WKUserContentController` retains this handler STRONGLY; this
//  handler holds the container WEAKLY, so there is no retain cycle. The callback
//  fires on the main thread (WebKit's script-message callbacks are main-thread).
//

import WebKit

/// Routes a preview web view's capture-phase `mousedown` (posted by
/// `PreviewSeam.clickFocusJS`) back into its `PaneContainerView` to focus the pane.
final class PreviewClickFocusHandler: NSObject, WKScriptMessageHandler {
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

  /// A click landed in the preview web view: focus the pane bound to `win`.
  func userContentController(
    _ userContentController: WKUserContentController,
    didReceive message: WKScriptMessage
  ) {
    container?.focusPreviewPane(win: win)
  }
}
