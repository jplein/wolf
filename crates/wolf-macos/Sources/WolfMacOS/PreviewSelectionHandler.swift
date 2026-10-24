// The `WKScriptMessageHandler` glue that caches a preview page's selected text.

import WebKit
import WolfKeyMap

/// Routes a preview web view's pushed selection (posted by
/// `PreviewCopySeam.selectionScriptJS` under `PreviewCopySeam.selectionHandlerName`)
/// into its `PaneContainerView`'s per-window cache.
final class PreviewSelectionHandler: NSObject, WKScriptMessageHandler {
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

  /// The page pushed its current selection: `{ t: <the selected text, empty when the
  /// selection is collapsed or whitespace-only> }`.
  ///
  /// An empty payload is a POSITIVE clear and drops the cached entry, because a
  /// collapsed selection must not leave Copy enabled over text the user no longer has
  /// selected. A MALFORMED body — anything `decodeSelectionMessage` returns `nil` for
  /// — leaves the cache untouched instead: it carries no information about the
  /// selection at all, so treating it as a clear would discard a good entry.
  func userContentController(
    _ userContentController: WKUserContentController,
    didReceive message: WKScriptMessage
  ) {
    switch PreviewCopySeam.decodeSelectionMessage(message.body) {
    case .selection(let selection):
      container?.recordPreviewSelection(win: win, selection: selection)
    case .cleared:
      container?.recordPreviewSelection(win: win, selection: nil)
    case nil:
      return
    }
  }
}
