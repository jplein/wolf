// Pure rules behind Copy from a Markdown preview pane.
// See notes/crates/wolf-macos/Sources/WolfKeyMap/PreviewCopySeam.swift.md#synchronous-copy

import Foundation

/// Pure rules for copying a selection out of a Markdown preview page: the selection
/// push channel, its payload decoding, and the Copy-source decision.
public enum PreviewCopySeam {

  // MARK: - Types

  /// The text a preview page reported as selected, cached per preview window on the
  /// AppKit side and consulted synchronously by menu validation and by Copy itself.
  public struct Selection: Equatable {
    /// The RAW, UNTRIMMED selected text, exactly as the page reported it.
    public let text: String

    public init(text: String) {
      self.text = text
    }

    /// Whether this selection is worth copying: `true` when ``text`` is non-empty
    /// after trimming `.whitespacesAndNewlines`.
    ///
    /// Whitespace-only counts as EMPTY because a stray click-drag across a margin
    /// selects whitespace, and clobbering `NSPasteboard` with `" "` is worse than
    /// doing nothing. `CharacterSet.whitespaces` includes U+00A0, of which a rendered
    /// Markdown document is full — so a hand-rolled `text != ""` check is NOT an
    /// equivalent simplification.
    public var isCopyable: Bool {
      !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
  }

  /// A decoded selection push from the page.
  ///
  /// `.cleared` is a POSITIVE clear and is deliberately distinct from the `nil` a
  /// malformed body decodes to: a collapsed selection must actively DROP the cached
  /// entry, while a message this seam did not send must leave the cache alone.
  public enum SelectionMessage: Equatable {
    /// A well-formed push carrying copyable text.
    case selection(Selection)
    /// A well-formed push with no — or whitespace-only — selection.
    case cleared
  }

  /// Where Edit > Copy takes its bytes from for one window.
  public enum CopySource: Equatable {
    /// The active pane's preview page. The payload is the RAW selected text.
    case previewSelection(String)
    /// The captive Neovim's visual-mode selection, yanked over the blocking RPC.
    case neovimVisual
    /// Nothing to copy — the item greys out and the action is a no-op.
    case none
  }

  // MARK: - The push channel

  /// The `WKScriptMessageHandler` name the injected selection script posts to.
  ///
  /// MUST differ from `PreviewSeam.clickFocusHandlerName` AND
  /// `PreviewScrollSeam.stateHandlerName` — all three ride the SAME
  /// `WKUserContentController`, where a duplicate name silently REPLACES the other
  /// handler and kills that channel with no error.
  public static let selectionHandlerName = "wolfPreviewSelection"

  /// The document-start user script injected into every preview web view, which
  /// PUSHES the page's current selection text out to ``selectionHandlerName``.
  ///
  /// Wrapped entirely in `try { … } catch (e) {}`, mirroring
  /// `PreviewScrollSeam.stateScriptJS`, so a torn-down message handler can never
  /// throw into the page's own scripts.
  ///
  /// The payload is exactly one always-present key, `{ t: <string> }`. The empty
  /// string IS the "cleared" signal; there is deliberately no separate boolean, so
  /// ``decodeSelectionMessage(_:)`` has exactly one thing to validate.
  ///
  /// A collapsed (or absent) selection takes an O(1) early-out to the empty string —
  /// `toString()` is O(selection length) and the common case must never pay for it.
  ///
  /// The three listeners, and why each is needed:
  ///
  ///  1. `selectionchange`, rAF-THROTTLED with the same `pending` flag idiom as the
  ///     scroll script's `scroll` listener. The primary trigger and the one that
  ///     needs throttling: it fires per pointer move while a selection is dragged out.
  ///  2. `mouseup` in the CAPTURE phase, unthrottled — the "settled" push. A
  ///     `mousedown` COLLAPSES the selection at the click point, so this is what
  ///     guarantees the cache is correct by the time the button is released, well
  ///     before a menu can open or a ⌘C can be typed. Capture phase for the same
  ///     reason `PreviewSeam.clickFocusJS` uses it: an inner element that stops
  ///     propagation must not suppress the push.
  ///  3. `keyup` in the capture phase, unthrottled. Wolf's menu has no Select All
  ///     item, so ⌘A reaches the web view, and a programmatic Select All cannot be
  ///     assumed to fire `selectionchange`.
  ///
  /// The key-DOWN event is deliberately NOT listened for: the selection is not yet
  /// updated at that point, so a push there would cache the PREVIOUS selection.
  ///
  /// One `post()` runs at document start, so a full `loadHTMLString` re-seeds the
  /// cache to cleared with no Swift involvement.
  ///
  /// - Parameter handlerName: The message-handler name to post to; defaults to
  ///   ``selectionHandlerName``.
  public static func selectionScriptJS(handlerName: String = selectionHandlerName) -> String {
    """
    (function () {
      try {
        var W = window;
        var text = function () {
          var s = W.getSelection ? W.getSelection() : null;
          if (!s || s.rangeCount === 0 || s.isCollapsed) { return ''; }
          return s.toString();
        };
        var post = function () {
          var mh = W.webkit && W.webkit.messageHandlers;
          if (!mh) { return; }
          var h = window.webkit.messageHandlers.\(handlerName);
          if (h) { h.postMessage({ t: text() }); }
        };
        var pending = false;
        document.addEventListener('selectionchange', function () {
          if (pending) { return; }
          pending = true;
          W.requestAnimationFrame(function () {
            pending = false;
            post();
          });
        });
        document.addEventListener('mouseup', post, true);
        document.addEventListener('keyup', post, true);
        post();
      } catch (e) {}
    })();
    """
  }

  /// Decodes one raw `WKScriptMessage.body` pushed by ``selectionScriptJS(handlerName:)``.
  ///
  /// Only a `[String: Any]` carrying a `String` under `"t"` decodes. Anything else —
  /// a non-dictionary, a missing key, a wrong value type, `nil` — returns `nil`,
  /// meaning MALFORMED: the caller must ignore the message and leave the cache alone,
  /// never mistake it for a clear. Extra keys are ignored, so the page can grow the
  /// payload without a lockstep Swift change.
  ///
  /// An empty or whitespace-only `"t"` decodes to ``SelectionMessage/cleared``; any
  /// other text decodes to ``SelectionMessage/selection(_:)`` carrying the text
  /// VERBATIM. Trimming decides WHETHER to copy and must never change WHAT is copied,
  /// or a selected code block would paste with its indentation silently stripped.
  public static func decodeSelectionMessage(_ body: Any?) -> SelectionMessage? {
    guard let dict = body as? [String: Any], let text = dict["t"] as? String else {
      return nil
    }
    let selection = Selection(text: text)
    return selection.isCopyable ? .selection(selection) : .cleared
  }

  // MARK: - The Copy decision

  /// Where Edit > Copy must take its bytes from, given the ACTIVE pane's cached
  /// preview selection and the key window's Neovim mode.
  ///
  /// The rules are ORDERED:
  ///
  ///  1. A non-`nil` `previewSelection` whose ``Selection/isCopyable`` is true wins,
  ///     EVEN IN VISUAL MODE. The caller supplies a non-`nil` selection only when the
  ///     ACTIVE pane is that preview, so a stale global nvim mode must not override
  ///     what the user is looking at — and yanking from Neovim here would touch
  ///     registers for a selection that is not Neovim's.
  ///  2. Otherwise `isVisualMode` gives ``CopySource/neovimVisual``, today's editor
  ///     behavior reached by exactly this branch. Rule 1 keys on `isCopyable`, NOT on
  ///     the entry merely being non-`nil`: a presence-only check would let a stale
  ///     empty cache silently disable ⌘C in an editor pane that really is in visual
  ///     mode.
  ///  3. Otherwise ``CopySource/none``.
  ///
  /// The `.previewSelection` payload is the RAW, UNTRIMMED text (see
  /// ``Selection/isCopyable``).
  public static func copySource(
    previewSelection: Selection?, isVisualMode: Bool
  ) -> CopySource {
    if let previewSelection, previewSelection.isCopyable {
      return .previewSelection(previewSelection.text)
    }
    if isVisualMode { return .neovimVisual }
    return .none
  }

  /// Whether Edit > Copy has anything to copy from either source.
  ///
  /// DEFINED as `copySource(…) != .none` rather than re-deriving the rules, so menu
  /// validation and the Copy action are structurally the same decision: if these two
  /// could disagree, the menu would claim enabled while Copy yielded nothing.
  public static func isCopyEnabled(
    previewSelection: Selection?, isVisualMode: Bool
  ) -> Bool {
    copySource(previewSelection: previewSelection, isVisualMode: isVisualMode) != .none
  }
}
