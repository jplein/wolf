// The `WKScriptMessageHandler` glue that focuses a clicked preview pane.

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
