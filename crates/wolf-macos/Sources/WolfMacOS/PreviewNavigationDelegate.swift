//
//  PreviewNavigationDelegate.swift
//  The `WKNavigationDelegate` glue that routes a clicked preview link.
//
//  A thin AppKit/WebKit adapter (compile + manual-smoke only): it turns a
//  `.linkActivated` navigation into a pure `PreviewLinkSeam.Action` and dispatches
//  that back into the `PaneContainerView`. ALL classification lives in the headless
//  `WolfKeyMap.PreviewLinkSeam`; this file only wires WebKit's policy callback to it
//  and calls `NSWorkspace.open` / the container's preview methods. It never lets a
//  clicked link navigate the web view itself (every clicked link is `.cancel`led and
//  handled out-of-band), so the pane only ever shows core-rendered HTML.
//
//  The container retains this delegate (a web view holds `navigationDelegate`
//  weakly); this delegate holds the container weakly, so there is no retain cycle.
//  All callbacks fire on the main thread (WebKit's navigation callbacks are
//  main-thread).
//

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
}
