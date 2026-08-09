//
//  PreviewSeam.swift
//  Pure, AppKit-free Markdown-preview reload arithmetic.
//
//  The sibling of `CommandSeam` / `FontSizeSeam`: a pure, display-free decision
//  exercised with scalar inputs only — NO AppKit, NO WKWebView, NO live Neovim. It
//  decides which preview WINDOWS need their web view reloaded by comparing each
//  window's last-loaded `(sourceBufnr, sourceTick)` against the live one, and it
//  owns the click-to-focus script a preview pane's web view injects.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap` module
//  alongside `CommandSeam.swift` / `PaneTree.swift`.
//

import Foundation

/// Pure Markdown-preview reload arithmetic.
public enum PreviewSeam {
  /// The `(sourceBufnr, sourceTick)` a preview WINDOW's web view was last loaded
  /// at. A struct (not a tuple) so `Optional<PreviewLoad>` inequality compiles,
  /// letting `winsNeedingReload` compare a window's live load against its last one.
  public struct PreviewLoad: Equatable {
    public let sourceBufnr: Int64
    public let sourceTick: UInt64
    public init(sourceBufnr: Int64, sourceTick: UInt64) {
      self.sourceBufnr = sourceBufnr
      self.sourceTick = sourceTick
    }
  }

  /// The preview WINDOW handles whose bound source buffer OR tick differs from what
  /// was last loaded: those in `current` (win → live `PreviewLoad`) whose entry is
  /// absent from — or unequal to — `loaded` (win → last-loaded `PreviewLoad`). A
  /// window is returned on:
  ///
  ///  - first bind (no `loaded` entry for the win),
  ///  - a changed `sourceBufnr` — a REUSED preview pane switching sources, even at
  ///    an equal `sourceTick` (the bug this seam fixes: two fresh files share a
  ///    `changedtick`, so a tick-only gate would suppress the reload),
  ///  - an advanced `sourceTick` (a live edit of the same source).
  ///
  /// A window whose live `PreviewLoad` equals its last one is NOT returned (no
  /// redundant blocking `render_preview`). Windows in `loaded` but absent from
  /// `current` (dropped previews) are never returned. Order is unspecified.
  ///
  /// A pure helper so the front-end's debounced reload gate is headlessly testable;
  /// the AppKit `PaneContainerView` keeps the per-window `loaded` map and calls
  /// `render_preview` for each returned window's source buffer.
  public static func winsNeedingReload(
    current: [Int64: PreviewLoad],
    loaded: [Int64: PreviewLoad]
  ) -> [Int64] {
    current
      .filter { win, load in loaded[win] != load }
      .map(\.key)
  }

  /// The `WKScriptMessageHandler` name a preview pane's click-to-focus user
  /// script posts to. The AppKit side registers a `PreviewClickFocusHandler`
  /// under this name on the web view's `WKUserContentController`; the injected
  /// script (see `clickFocusJS`) posts to
  /// `window.webkit.messageHandlers.<name>`.
  public static let clickFocusHandlerName = "wolfPaneFocus"

  /// The document-start JavaScript that makes a click ANYWHERE in a preview
  /// pane focus that pane. Because the hosting `WKWebView` swallows AppKit
  /// mouse events, the container's `mouseDown` never fires over the preview, so
  /// this self-invoking listener registers a CAPTURE-phase `mousedown` handler
  /// (the literal `true` third argument to `addEventListener`) that posts to
  /// `window.webkit.messageHandlers.<handlerName>` — routed to the pane's
  /// `PreviewClickFocusHandler`, which focuses the pane. Capture phase ensures
  /// the notification fires even when inner elements stop propagation.
  ///
  /// - Parameter handlerName: The message-handler name to post to; defaults to
  ///   `clickFocusHandlerName`.
  public static func clickFocusJS(handlerName: String = clickFocusHandlerName) -> String {
    """
    (function () {
      document.addEventListener('mousedown', function () {
        window.webkit.messageHandlers.\(handlerName).postMessage(1);
      }, true);
    })();
    """
  }
}
