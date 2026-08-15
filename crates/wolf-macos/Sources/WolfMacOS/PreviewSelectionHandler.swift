//
//  PreviewSelectionHandler.swift
//  The `WKScriptMessageHandler` glue that caches a preview page's selected text.
//
//  A thin AppKit/WebKit adapter (compile + manual-smoke only), the exact structural
//  sibling of `PreviewClickFocusHandler` / `PreviewScrollStateHandler`. The
//  document-start selection script (`PreviewCopySeam.selectionScriptJS`) PUSHES the
//  page's current selection out to this handler — on every selection change, every
//  mouse-up and key-up, and once at document start — and this handler forwards it
//  into the `PaneContainerView`'s per-window cache.
//
//  The push direction is load-bearing. Edit > Copy's enablement is decided in
//  `AppDelegate.validateMenuItem`, which is SYNCHRONOUS and therefore cannot await an
//  `evaluateJavaScript` round trip; a continuously refreshed Swift-side copy turns
//  that decision into a plain dictionary read. The text rides along in the push so
//  the enable decision and the copied bytes come from one snapshot.
//
//  ALL the rules — the handler name, the injected JS, the payload shape and the
//  copy-source decision — live in the headless `WolfKeyMap.PreviewCopySeam`; this file
//  only routes the decoded result.
//
//  The web view's `WKUserContentController` retains this handler STRONGLY; this
//  handler holds the container WEAKLY, so there is no retain cycle. The callback fires
//  on the main thread (WebKit's script-message callbacks are main-thread).
//

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
