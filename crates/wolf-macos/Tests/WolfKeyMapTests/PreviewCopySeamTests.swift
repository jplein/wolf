//
//  PreviewCopySeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.PreviewCopySeam`.
//
//  These exercise the pure selection-push / copy-source seam the coder adds in
//  `Sources/WolfKeyMap/PreviewCopySeam.swift`. Like `PreviewSeamTests` /
//  `PreviewScrollSeamTests`, they import ONLY Foundation-level test machinery (no
//  AppKit, no WKWebView, no live Neovim) and stay in the headless `WolfKeyMap`
//  module. `PaneContainerView` / `PreviewSelectionHandler`, the `NSPasteboard`
//  write and `validateMenuItem` are AppKit glue (compile + manual-smoke only); a
//  real WebKit selection and a live ⌘C cannot be automated at all. EVERY rule —
//  the handler name, the emitted JS, the message decoding and the "which source
//  does Copy use" decision — lives here and is unit-tested.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor)
//
//    public enum PreviewCopySeam {
//        public struct Selection: Equatable {
//            public let text: String
//            public init(text: String)
//            // true when `text` is non-empty after trimming
//            // .whitespacesAndNewlines
//            public var isCopyable: Bool { get }
//        }
//        public enum SelectionMessage: Equatable {
//            case selection(Selection)  // a well-formed push carrying copyable text
//            case cleared               // a well-formed push with no (or
//                                       // whitespace-only) selection
//        }
//        public enum CopySource: Equatable {
//            case previewSelection(String)  // the payload is the RAW selected text
//            case neovimVisual
//            case none
//        }
//        public static let selectionHandlerName: String  // "wolfPreviewSelection"
//        public static func selectionScriptJS(
//            handlerName: String = selectionHandlerName) -> String
//        // nil means MALFORMED — ignore the message, leave the cache alone.
//        public static func decodeSelectionMessage(_ body: Any?) -> SelectionMessage?
//        public static func copySource(
//            previewSelection: Selection?, isVisualMode: Bool) -> CopySource
//        // Defined as `copySource(...) != .none`, so the two cannot disagree.
//        public static func isCopyEnabled(
//            previewSelection: Selection?, isVisualMode: Bool) -> Bool
//    }
//
//  ## THE DECISION MATRIX (`copySource`, ORDERED rules)
//
//   1. a non-nil `previewSelection` whose `isCopyable` is true -> `.previewSelection`,
//      EVEN IN VISUAL MODE. The caller supplies a non-nil selection only when the
//      ACTIVE pane is that preview, so a stale global nvim mode must never override
//      what the user is looking at.
//   2. otherwise `isVisualMode` -> `.neovimVisual`. A cached-but-NON-copyable entry
//      must NOT mask visual mode — that is a named case below, because "the preview
//      pushed something" and "the preview pushed something copyable" are different
//      questions and conflating them silently disables ⌘C in the editor pane.
//   3. otherwise `.none`.
//
//  `.previewSelection` carries the RAW, UNTRIMMED text. Trimming decides WHETHER to
//  copy; it must never change WHAT is copied — pasting a code block with its leading
//  indentation stripped would be a silent corruption.
//
//  Whitespace-only counts as empty: a stray click-drag across a margin selects
//  whitespace, and clobbering `NSPasteboard` with `" "` is worse than doing nothing.
//  `CharacterSet.whitespaces` includes U+00A0, so a non-breaking-space-only selection
//  is "empty" too — asserted below so a hand-rolled `text != ""` check cannot pass.
//
//  ## THE DECODING TABLE (`decodeSelectionMessage`)
//
//  The payload shape is exactly one key, ALWAYS present: `{ t: <string> }`. There is
//  deliberately no separate "cleared" boolean, so the Swift decoder has exactly one
//  thing to validate. Only a `[String: Any]` carrying a `String` under `"t"` decodes;
//  anything else (non-dictionary, missing key, wrong value type, nil) is `nil` =
//  MALFORMED, which the handler treats as "leave the cache alone". An empty or
//  whitespace-only `"t"` is `.cleared` — a POSITIVE clear, distinct from malformed,
//  because a collapsed selection must actively DROP the cached entry rather than
//  leave a stale one enabling Copy. Extra keys are ignored, so the page may grow the
//  payload without a Swift-side change.
//
//  ## THE JS CONTRACT (`selectionScriptJS`)
//
//  Assertions are string containment, exactly like the scroll seam's, and are tied to
//  the contract rather than to formatting: the interpolated handler name, the three
//  listeners (`selectionchange`, `mouseup`, `keyup`), the rAF throttle, the
//  `isCollapsed` O(1) early-out (a collapsed selection must never pay for
//  `toString()`), and the whole-script `try` / `catch` wrapper that keeps a missing
//  message handler from throwing into the page.
//

import XCTest

@testable import WolfKeyMap

final class PreviewCopySeamTests: XCTestCase {

  /// A selection with copyable content AND surrounding whitespace, so a test that
  /// claims the raw text is carried through cannot be satisfied by a trimmed copy.
  private let paddedSelection = PreviewCopySeam.Selection(text: "  hello world  \n")

  /// Every `Selection?` shape the matrix has to answer for, copyable and not.
  private let matrixSelections: [PreviewCopySeam.Selection?] = [
    nil,
    PreviewCopySeam.Selection(text: ""),
    PreviewCopySeam.Selection(text: " "),
    PreviewCopySeam.Selection(text: "\n\t"),
    PreviewCopySeam.Selection(text: "\u{00A0}"),
    PreviewCopySeam.Selection(text: "a"),
    PreviewCopySeam.Selection(text: "  padded  "),
    PreviewCopySeam.Selection(text: "line one\nline two"),
  ]

  /// The `Selection` inside a `.selection` message, or `nil` (with a failure).
  private func decoded(
    _ message: PreviewCopySeam.SelectionMessage?,
    file: StaticString = #filePath,
    line: UInt = #line
  ) -> PreviewCopySeam.Selection? {
    guard case .selection(let selection)? = message else {
      XCTFail(
        "expected a .selection message, got \(String(describing: message))",
        file: file, line: line)
      return nil
    }
    return selection
  }

  // MARK: - the message-handler name is the documented selection channel

  func testSelectionHandlerNameIsTheDocumentedName() {
    XCTAssertEqual(
      PreviewCopySeam.selectionHandlerName, "wolfPreviewSelection",
      "the selection message handler must be named `wolfPreviewSelection`")
  }

  // MARK: - all THREE preview channels ride one WKUserContentController

  func testSelectionHandlerNameDiffersFromTheOtherPreviewChannels() {
    XCTAssertNotEqual(
      PreviewCopySeam.selectionHandlerName, PreviewSeam.clickFocusHandlerName,
      "click-to-focus and selection are registered on the SAME "
        + "WKUserContentController, where a duplicate name silently REPLACES the "
        + "other handler — clicking a preview would stop focusing its pane")
    XCTAssertNotEqual(
      PreviewCopySeam.selectionHandlerName, PreviewScrollSeam.stateHandlerName,
      "scroll state and selection are registered on the SAME "
        + "WKUserContentController, so their names must differ or one channel goes "
        + "silently dead")
  }

  // MARK: - the injected script posts to the selection message handler

  func testSelectionScriptJSPostsToTheHandler() {
    let js = PreviewCopySeam.selectionScriptJS()
    XCTAssertTrue(
      js.contains("window.webkit.messageHandlers.\(PreviewCopySeam.selectionHandlerName)"),
      "the script must post to `window.webkit.messageHandlers."
        + "\(PreviewCopySeam.selectionHandlerName)`: \(js)")
    XCTAssertTrue(
      js.contains("getSelection"),
      "the pushed text comes from `window.getSelection()`: \(js)")
  }

  // MARK: - the JS payload KEY and the decoder's key are one contract

  /// The one thing neither side can check alone. `decodeSelectionMessage` requires the
  /// key `"t"` and the emitted script posts under it, but nothing in either function
  /// references the other: renaming the JS key to `text` would leave every other case
  /// in this file green while the channel silently carried nothing at runtime — Copy
  /// would simply never enable in a preview pane.
  ///
  /// The key literal is written ONCE here and asserted from BOTH sides, so the test
  /// states the contract rather than duplicating a third copy of it.
  func testSelectionScriptJSPostsUnderTheKeyTheDecoderAccepts() {
    let payloadKey = "t"

    XCTAssertTrue(
      PreviewCopySeam.selectionScriptJS().contains("postMessage({ \(payloadKey):"),
      "the emitted script must post its payload under the `\(payloadKey)` key — the "
        + "SAME key decodeSelectionMessage accepts. If the two ever drift, every push "
        + "decodes to nil (malformed), the cache is never written, and Copy stays "
        + "greyed out with a live selection on screen: "
        + "\(PreviewCopySeam.selectionScriptJS())")

    XCTAssertEqual(
      decoded(PreviewCopySeam.decodeSelectionMessage([payloadKey: "paired"]))?.text,
      "paired",
      "and the decoder must accept that same key: a body built with the key the JS "
        + "emits has to decode to a selection, or the channel is dead in the other "
        + "direction")
  }

  // MARK: - listener 1: selectionchange, the primary trigger

  func testSelectionScriptJSRegistersSelectionChange() {
    XCTAssertTrue(
      PreviewCopySeam.selectionScriptJS().contains("selectionchange"),
      "`selectionchange` is the primary trigger: it is what fires while the user "
        + "drags a selection out, and without it the cache is only ever as fresh as "
        + "the last mouseup")
  }

  // MARK: - listeners 2 and 3: the unthrottled settled pushes

  func testSelectionScriptJSRegistersMouseupAndKeyup() {
    let js = PreviewCopySeam.selectionScriptJS()
    XCTAssertTrue(
      js.contains("mouseup"),
      "the settled push. A mousedown COLLAPSES the selection at the click point, so "
        + "the unthrottled mouseup push is what guarantees the cache is correct by "
        + "the time the button is released — before any menu can open: \(js)")
    XCTAssertTrue(
      js.contains("keyup"),
      "⌘A reaches the web view (Wolf's menu has no Select All item), and a "
        + "programmatic Select All cannot be assumed to fire `selectionchange`: \(js)")
    XCTAssertFalse(
      js.contains("keydown"),
      "`keydown` is deliberately absent: the selection is not yet updated at "
        + "keydown, so a push there would cache the PREVIOUS selection: \(js)")
    XCTAssertTrue(
      js.contains("'mouseup', post, true)"),
      "the mouseup listener must register in the CAPTURE phase (the `true` third "
        + "argument), for the same reason `clickFocusJS` does — an inner element "
        + "calling stopPropagation() must not suppress the settled push, which is "
        + "the whole click-collapse defense: \(js)")
    XCTAssertTrue(
      js.contains("'keyup', post, true)"),
      "the keyup listener must register in the CAPTURE phase for the same reason: a "
        + "bubble-phase registration is silently swallowed by any element that stops "
        + "propagation, and ⌘A would then never re-seed the cache: \(js)")
  }

  // MARK: - the document-start push that re-seeds the cache with no Swift involvement

  /// The script must not merely REGISTER the pushes — it must fire one immediately, so
  /// a full `loadHTMLString` reports the new document's (empty) selection on its own.
  /// Without it, a document replacement leaves the previously cached selection enabling
  /// Copy until the user next touches the page.
  ///
  /// Pinned by POSITION rather than by counting: the listener registrations pass `post`
  /// as a REFERENCE (`', post, true)'`), so the distinguishing property of the
  /// document-start push is that it is an actual CALL standing after the last
  /// `addEventListener`. That also excludes the call inside the rAF throttle, which is
  /// nested in the first listener and therefore precedes the later registrations.
  func testSelectionScriptJSFiresOnePushAtDocumentStart() {
    let js = PreviewCopySeam.selectionScriptJS()
    guard let lastRegistration = js.range(of: "addEventListener", options: .backwards) else {
      XCTFail("the script must register its listeners: \(js)")
      return
    }
    guard let lastCall = js.range(of: "post();", options: .backwards) else {
      XCTFail("the script must CALL post(), not only hand it to addEventListener: \(js)")
      return
    }
    XCTAssertTrue(
      lastCall.lowerBound > lastRegistration.upperBound,
      "a bare `post()` must run at document start, AFTER the listeners are wired, so a "
        + "full loadHTMLString re-seeds the Swift-side cache to cleared with no Swift "
        + "involvement. Every call site before the last addEventListener is either a "
        + "listener reference or the throttled call nested inside one, neither of "
        + "which fires on a fresh document: \(js)")
  }

  // MARK: - the throttle and the collapsed-selection early-out

  func testSelectionScriptJSThrottlesAndEarlyOutsOnACollapsedSelection() {
    let js = PreviewCopySeam.selectionScriptJS()
    XCTAssertTrue(
      js.contains("requestAnimationFrame"),
      "`selectionchange` fires per pointer move during a drag-select and "
        + "`toString()` is O(selection length), so that listener must be "
        + "rAF-throttled with the same `pending` flag idiom as the scroll script: \(js)")
    XCTAssertTrue(
      js.contains("isCollapsed"),
      "a collapsed selection must take an O(1) early-out to the empty string — "
        + "`toString()` is never called for the common case: \(js)")
    XCTAssertTrue(
      js.contains("toString()"),
      "and the NON-collapsed branch must actually read the selection out. Without "
        + "this call every push reports the empty string, so each one decodes to "
        + "`.cleared`, the cache is never populated, and Copy stays greyed out with a "
        + "live selection on screen — while every other JS assertion here still "
        + "passes: \(js)")
  }

  // MARK: - a missing message handler can never throw into the page

  func testSelectionScriptJSIsWrappedInTryCatch() {
    let js = PreviewCopySeam.selectionScriptJS()
    XCTAssertTrue(
      js.contains("try"),
      "the whole self-invoking function is wrapped in try/catch, mirroring "
        + "`PreviewScrollSeam.stateScriptJS`: \(js)")
    XCTAssertTrue(
      js.contains("catch"),
      "a torn-down message handler must be swallowed, not thrown into the page's "
        + "own scripts: \(js)")
  }

  // MARK: - the handler name is INTERPOLATED, not hardcoded in the JS string

  func testSelectionScriptJSInterpolatesACustomHandlerName() {
    let js = PreviewCopySeam.selectionScriptJS(handlerName: "customChannel")
    XCTAssertTrue(
      js.contains("customChannel"),
      "the passed handler name must reach the generated source: \(js)")
    XCTAssertFalse(
      js.contains(PreviewCopySeam.selectionHandlerName),
      "the default name must NOT survive a custom argument — a hardcoded name in "
        + "the JS string would post to a channel nobody registered: \(js)")
  }

  // MARK: - decode: a well-formed push carrying copyable text

  func testDecodeCopyableTextYieldsASelection() {
    XCTAssertEqual(
      decoded(PreviewCopySeam.decodeSelectionMessage(["t": "hello"]))?.text, "hello",
      "a String under the `t` key is the whole payload contract")
  }

  // MARK: - decode: the text is preserved VERBATIM, never trimmed

  func testDecodePreservesSurroundingWhitespaceVerbatim() {
    XCTAssertEqual(
      decoded(PreviewCopySeam.decodeSelectionMessage(["t": "  hi  "]))?.text, "  hi  ",
      "trimming decides WHETHER to copy, never WHAT is copied: the decoded text "
        + "must keep its leading and trailing whitespace, or a selected code block "
        + "would paste with its indentation silently stripped")
  }

  // MARK: - decode: an empty payload is a POSITIVE clear

  func testDecodeEmptyTextIsAPositiveClear() {
    XCTAssertEqual(
      PreviewCopySeam.decodeSelectionMessage(["t": ""]),
      PreviewCopySeam.SelectionMessage.cleared,
      "the empty string is the documented `cleared` signal — a collapsed selection "
        + "must actively DROP the cached entry, so this is NOT the malformed nil")
  }

  // MARK: - decode: whitespace-only is empty too

  func testDecodeWhitespaceOnlyTextIsACleared() {
    XCTAssertEqual(
      PreviewCopySeam.decodeSelectionMessage(["t": "   \n\t "]),
      PreviewCopySeam.SelectionMessage.cleared,
      "a click-drag across a margin selects whitespace; caching it would enable "
        + "Copy and then clobber the pasteboard with blanks")
  }

  // MARK: - decode: a wrong value type is MALFORMED

  func testDecodeWrongValueTypeIsMalformed() {
    XCTAssertNil(
      PreviewCopySeam.decodeSelectionMessage(["t": 42]),
      "only a String decodes. A non-String `t` is malformed, and malformed means "
        + "LEAVE THE CACHE ALONE — it must not be mistaken for a clear")
  }

  // MARK: - decode: a missing key is MALFORMED

  func testDecodeMissingKeyIsMalformed() {
    XCTAssertNil(
      PreviewCopySeam.decodeSelectionMessage([String: Any]()),
      "the `t` key is ALWAYS present in a well-formed push, so its absence is a "
        + "message this seam did not send, not an empty selection")
  }

  // MARK: - decode: a non-dictionary body is MALFORMED

  func testDecodeNonDictionaryBodyIsMalformed() {
    XCTAssertNil(
      PreviewCopySeam.decodeSelectionMessage("hello"),
      "`WKScriptMessage.body` is untyped `Any`; a bare String is not the agreed "
        + "payload shape")
  }

  // MARK: - decode: a nil body is MALFORMED

  func testDecodeNilBodyIsMalformed() {
    XCTAssertNil(
      PreviewCopySeam.decodeSelectionMessage(nil),
      "a nil body carries no information at all, so it must not clear a good "
        + "cached selection")
  }

  // MARK: - decode: extra keys are ignored

  func testDecodeIgnoresExtraKeys() {
    XCTAssertEqual(
      decoded(PreviewCopySeam.decodeSelectionMessage(["t": "x", "junk": 1]))?.text, "x",
      "the decoder validates only `t`, so the page can grow the payload without a "
        + "lockstep Swift change")
  }

  // MARK: - decode: multi-line and non-ASCII text round-trips unchanged

  func testDecodeRoundTripsMultilineAndNonASCIIText() {
    let text = "первая строка\nsecond — line\n\t三行目 🐺"
    XCTAssertEqual(
      decoded(PreviewCopySeam.decodeSelectionMessage(["t": text]))?.text, text,
      "the pushed text is carried to NSPasteboard byte for byte: no normalization, "
        + "no line-ending rewriting, no escaping left over from the JS side")
  }

  // MARK: - matrix: nothing to copy from either source

  func testCopySourceIsNoneWithNoSelectionAndNoVisualMode() {
    XCTAssertEqual(
      PreviewCopySeam.copySource(previewSelection: nil, isVisualMode: false), .none,
      "an editor pane in normal mode with no preview selection has nothing to "
        + "copy — Edit > Copy greys out, exactly as it does today")
  }

  // MARK: - matrix: visual mode alone selects the Neovim yank

  func testCopySourceIsNeovimVisualWithNoPreviewSelection() {
    XCTAssertEqual(
      PreviewCopySeam.copySource(previewSelection: nil, isVisualMode: true), .neovimVisual,
      "the pre-existing editor behavior must be reached by exactly this branch: "
        + "visual mode with no preview selection still yanks from Neovim")
  }

  // MARK: - matrix: a copyable preview selection carries the RAW text

  func testCopySourceCarriesTheRawPreviewSelectionText() {
    XCTAssertEqual(
      PreviewCopySeam.copySource(previewSelection: paddedSelection, isVisualMode: false),
      .previewSelection("  hello world  \n"),
      "the payload is the RAW, UNTRIMMED text: trimming only decided that there was "
        + "something worth copying")
  }

  // MARK: - matrix: the preview OUTRANKS visual mode

  func testCopyablePreviewSelectionWinsOverVisualMode() {
    XCTAssertEqual(
      PreviewCopySeam.copySource(previewSelection: paddedSelection, isVisualMode: true),
      .previewSelection("  hello world  \n"),
      "the caller passes a non-nil selection ONLY when the active pane is that "
        + "preview, so a stale global nvim mode must not override what the user is "
        + "looking at — and copying from Neovim here would also touch registers for "
        + "a selection that is not Neovim's")
  }

  // MARK: - matrix: an EMPTY cached entry is not a selection

  func testEmptyCachedSelectionIsNotCopied() {
    XCTAssertEqual(
      PreviewCopySeam.copySource(
        previewSelection: PreviewCopySeam.Selection(text: ""), isVisualMode: false),
      .none,
      "a cached entry whose text is empty must behave exactly like no entry at all")
  }

  // MARK: - matrix: an empty cached entry must NOT mask visual mode

  func testEmptyCachedSelectionDoesNotMaskVisualMode() {
    XCTAssertEqual(
      PreviewCopySeam.copySource(
        previewSelection: PreviewCopySeam.Selection(text: ""), isVisualMode: true),
      .neovimVisual,
      "rule 1 keys on `isCopyable`, NOT on the entry merely being non-nil. A "
        + "presence-only check would let a stale empty cache disable ⌘C in an editor "
        + "pane that really is in visual mode")
  }

  // MARK: - matrix: whitespace-only behaves identically to empty, both ways

  func testWhitespaceOnlyCachedSelectionBehavesLikeNoSelection() {
    let blank = PreviewCopySeam.Selection(text: " \n ")
    XCTAssertEqual(
      PreviewCopySeam.copySource(previewSelection: blank, isVisualMode: true),
      .neovimVisual,
      "a whitespace-only cache is not copyable, so it must not mask visual mode "
        + "either — the same rule as the empty entry, not a separate one")
    XCTAssertEqual(
      PreviewCopySeam.copySource(previewSelection: blank, isVisualMode: false), .none,
      "and with nothing else to copy it is simply `.none`")
  }

  // MARK: - Selection.isCopyable: whitespace-only is empty, including U+00A0

  func testIsCopyableTreatsWhitespaceOnlyTextAsEmpty() {
    let notCopyable = ["", " ", "\n", "\t", "\u{00A0}", "  \n\t  "]
    for text in notCopyable {
      XCTAssertFalse(
        PreviewCopySeam.Selection(text: text).isCopyable,
        "`\(text.debugDescription)` trims to nothing, so copying it would clobber "
          + "the pasteboard with blanks — worse than doing nothing")
    }

    let copyable = ["a", " a ", "\na\n", "🐺"]
    for text in copyable {
      XCTAssertTrue(
        PreviewCopySeam.Selection(text: text).isCopyable,
        "`\(text.debugDescription)` has real content once trimmed and must be "
          + "copyable — padding never disqualifies a selection")
    }
  }

  // MARK: - U+00A0 specifically: an emptiness check on the raw text would be wrong

  func testIsCopyableTreatsANonBreakingSpaceAsEmpty() {
    XCTAssertFalse(
      PreviewCopySeam.Selection(text: "\u{00A0}").isCopyable,
      "`CharacterSet.whitespaces` includes U+00A0, and a rendered Markdown document "
        + "is full of them — a hand-rolled `text != \"\"` check would call this "
        + "copyable and enable Copy on a margin drag")
  }

  // MARK: - THE anti-drift pin: enabled iff there is a source

  func testIsCopyEnabledAgreesWithCopySourceAcrossTheWholeMatrix() {
    for selection in matrixSelections {
      for isVisualMode in [false, true] {
        let source = PreviewCopySeam.copySource(
          previewSelection: selection, isVisualMode: isVisualMode)
        XCTAssertEqual(
          PreviewCopySeam.isCopyEnabled(
            previewSelection: selection, isVisualMode: isVisualMode),
          source != .none,
          "menu validation and the Copy action must be the SAME decision: if these "
            + "two can disagree, the menu claims enabled while Copy yields nothing "
            + "(or greys out with a live selection). selection="
            + "\(String(describing: selection)), isVisualMode=\(isVisualMode)")
      }
    }
  }
}
