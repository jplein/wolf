// The `WKNavigationDelegate` glue that routes a clicked preview link.

import AppKit
import WebKit
import WolfKeyMap

/// Routes a preview web view's clicked links through `PreviewLinkSeam` back into
/// its `PaneContainerView`.
final class PreviewNavigationDelegate: NSObject, WKNavigationDelegate {
  /// The preview WINDOW handle this delegate serves (identifies the pane in the
  /// container's per-window maps).
  let win: Int64
  /// The owning container, held WEAKLY (it retains this delegate — no cycle).
  weak var container: PaneContainerView?

  init(win: Int64, container: PaneContainerView) {
    self.win = win
    self.container = container
    super.init()
  }

  /// WebKit's navigation-policy gate. Only USER-CLICKED links (`.linkActivated`)
  /// are intercepted; the initial `loadHTMLString` and any programmatic
  /// `evaluateJavaScript` scrolls pass through with `.allow`. A clicked link is
  /// classified and `.cancel`led — the web view never navigates itself.
  func webView(
    _ webView: WKWebView,
    decidePolicyFor navigationAction: WKNavigationAction,
    decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
  ) {
    guard navigationAction.navigationType == .linkActivated,
      let url = navigationAction.request.url
    else {
      decisionHandler(.allow)
      return
    }

    let sourcePath = container?.previewSourcePath(forWin: win) ?? ""
    switch PreviewLinkSeam.classify(url: url, sourcePath: sourcePath) {
    case .openInBrowser(let browserURL):
      NSWorkspace.shared.open(browserURL)
      decisionHandler(.cancel)
    case .scrollToFragment(let fragment):
      decisionHandler(.cancel)
      container?.scrollPreview(win: win, toFragment: fragment)
    case .previewFile(let path, let fragment):
      decisionHandler(.cancel)
      container?.previewFile(win: win, path: path, fragment: fragment)
    }
  }

  /// A preview load finished: let the container run any pending post-load scroll
  /// (a cross-file `other.md#anchor` navigation).
  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    container?.previewDidFinishLoad(win: win)
  }

  /// A COMMITTED preview load failed. Purely bookkeeping: the container decrements
  /// that window's outstanding-navigation COUNT so `PreviewScrollSeam.updateStrategy`
  /// rule 2 does not pin this window on the full-load path forever (a safe
  /// degradation, but a silent one). It is a count rather than a flag because two
  /// navigations can straddle one page load, and WebKit reports the SUPERSEDED one as
  /// a failure — so this callback is a routine part of the typing path, not an
  /// error-only path.
  func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: any Error)
  {
    container?.previewDidFailLoad(win: win)
  }

  /// A preview load failed BEFORE committing (the far likelier half — the
  /// provisional navigation never became the displayed document). Same bookkeeping;
  /// the web view keeps showing the previous document either way, which is why the
  /// container leaves its live-document descriptor alone on a failure. WebKit also
  /// reports a `.cancel`led policy decision here, i.e. every clicked link, so the
  /// container ignores a callback for which it counted no navigation.
  func webView(
    _ webView: WKWebView,
    didFailProvisionalNavigation navigation: WKNavigation!,
    withError error: any Error
  ) {
    container?.previewDidFailLoad(win: win)
  }
}
