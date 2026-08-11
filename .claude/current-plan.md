# Plan: Copy from a Markdown preview pane

## Status

Complete, pending the manual smoke list in §4. Every lint green (`//tools/lint:check`, 5/5), `//crates/wolf-macos:WolfKeyMapTests` 428 cases with 30 in `PreviewCopySeamTests`, `//crates/... //bazel/...` 96/96, `//crates/wolf-macos:Wolf` builds, docs written and under budget.

Earlier detail, kept for the record:

Implemented and green: `//crates/wolf-macos:WolfKeyMapTests` runs 28 `PreviewCopySeamTests` cases, all passing; `//crates/wolf-macos:Wolf` builds; `//crates/... //bazel/...` 96/96 pass; swift-format clean.

Second test pass done — two accepted gaps from the coder's review, both added and passing with no seam change:

1. `testSelectionScriptJSPostsUnderTheKeyTheDecoderAccepts` pins the payload key `"t"` across the JS/Swift boundary (`selectionScriptJS` emits it, `decodeSelectionMessage` requires it; nothing tied them, so a JS-side rename would leave the suite green and the feature dead).
2. `testSelectionScriptJSFiresOnePushAtDocumentStart` pins the document-start `post()` that re-seeds the cache to cleared on a full load.

`PreviewCopySeamTests` is 30 cases; the `WolfKeyMapTests` binary is 428, 0 failures, run with `--cache_test_results=no`.

Declined nothing. The coder's third item is the documentation sweep, queued for the technical writer.

Reviewer: **approved with minor notes**. All eight plan-conformance checks pass, with file:line evidence — teardown removes all three handlers by name and prunes both dictionaries; the cache clear sits above the `updateStrategy` switch so both replacement branches pass through it; `activePaneSelection` keys on `activeWin` with no fallback; the `.neovimVisual` branch is byte-for-byte today's behavior; both hand-maintained `srcs` lists are edited.

Orchestrator decisions on the review findings:

- **F6 (accepted, test-writer)** — the capture-phase assertion `js.contains("true")` is vacuous (`pending = true` satisfies it), so dropping the capture argument would leave the suite green while breaking §7 defense 2. Pinned to `'mouseup', post, true)` / `'keyup', post, true)`. Plus a `toString()` pin: without it, a `text` function returning `''` unconditionally satisfies every other JS assertion.
- **F3 (accepted, coder — fix the code)** — a late message after teardown wrote a permanent entry: the prune iterates `previewWebViews.keys`, which `removePreviewWebView` has already nil'd, so the inherited "the next prune drops it" comment is false. Harmless for two `Double`s of scroll state, not for a document's text. `recordPreviewSelection` now ignores a `win` absent from `previewWebViews`.
- **F4 (accepted, coder — comment)** — the synchronous clear is authoritative against a late *clear*, not a late *set*: a `mouseup` / `keyup` / pending rAF push can re-populate the entry in the frame or two before WebKit commits the replacement. Benign (`post()` reads live state at call time), but the comment claimed more than it delivers.
- **F2 (out of scope, reported not fixed)** — while a preview selection is cached, the main menu's ⌘C wins over a focused Wolf text field (sidebar Search, file-tree rename), so the preview text is copied instead of the field's. A live Neovim visual-mode selection already does this by the identical mechanism, so this change widens a pre-existing defect rather than introducing one; fixing it means changing who wins ⌘C for the editor path too, which is beyond this bug. Added to the smoke list; report to the user.
- **F5 (accepted, plan)** — the two-preview-pane smoke step could not pass as written; corrected above.
- **F1 and the doc list** — technical writer.

Also noted by the reviewer, deliberately NOT fixed here: `PreviewSeamTests.testClickFocusJSCaptureMousedownPostsToHandler` has the same vacuous `contains("true")` hole, and `recordPreviewScrollState` carries the same false "next prune drops it" sentence. Both are pre-existing and outside this change.

Then: technical writer.

Deviations from the plan: none reported by the coder. One incidental fix — the `previewScrollHandlers` doc comment said "Both handlers" and was made wrong by this change, so it names all three.

## Problem statement

When text is selected in a Markdown preview pane, ⌘C does nothing and Edit > Copy is greyed out.

Copy must work for a selection in a preview pane — both the ⌘C key equivalent and the Edit > Copy menu item — copying the preview web view's selected text to `NSPasteboard`. The existing editor-pane behavior (Neovim visual-mode yank) must be unchanged.

## Root cause (verified by the architect)

- `Sources/WolfMacOS/AppMenu.swift:234-247` `editMenuItem()` builds Copy via `commandItem(title: "Copy", tag: Tag.copy, keyEquivalent: "c", target: target)` — an explicitly targeted item, so the main menu's `performKeyEquivalent` matches ⌘C and validation decides everything. The target is `AppDelegate`, not `nil`, so the responder chain is never consulted: the fix must live inside the menu-item path, and making the web view first responder cannot help.
- `Sources/WolfMacOS/AppDelegate.swift:748-749` `validateMenuItem` returns `keyController?.isVisualMode ?? false` for `.copy` — why the item greys out and ⌘C is inert.
- `Sources/WolfMacOS/WolfWindowController.swift:664-669` `copy()` yanks via `handle.copy_selection()`.

The existence of the bug proves the menu wins the key-equivalent race against the `WKWebView`; if WebKit ever won it, it would copy the same bytes, so the fix is robust either way.

**No core change is needed.** `wolf_core::COMMAND_SPECS` (`crates/wolf-core/src/command.rs:257-263`) carries only `command / id / ex_command / title / menu` — no availability predicate — and `palette.rs` does no enablement filtering. Availability is entirely a front-end concern. The only core-side touch is a stale one-line doc comment (§9.7).

## Design constraints

- Menu validation is synchronous, so "does the preview have a selection" cannot be an async `evaluateJavaScript` round trip. Use the existing push-based pattern: a document-start user script plus a `WKScriptMessageHandler` pushing state into a Swift-side cache on `PaneContainerView` — parallel to click-to-focus (`PreviewSeam.clickFocusJS` / `PreviewClickFocusHandler`) and scroll state (`PreviewScrollSeam` / `PreviewScrollStateHandler`).
- Any new script-message handler on a preview web view's `WKUserContentController` must be removed in `PaneContainerView.removePreviewWebView`, or the controller leaks the handler and keeps the container alive.
- All pure logic lives in a headless `WolfKeyMap` seam with unit tests.
- New Swift test files must be added explicitly to the `srcs` of `//crates/wolf-macos:WolfKeyMapTests`, or they never run.
- Real `WKWebView` selection and a live ⌘C cannot be automated: manual smoke steps.

---

## 1. Summary

Add a third push channel to the Markdown preview web view — a document-start user script that pushes the page's current selection text out to Swift over a `WKScriptMessageHandler`, cached per preview window on `PaneContainerView`. All rules (handler name, JS source, message decoding, and the "is Copy enabled / which source does Copy use" decision) live in a new headless `WolfKeyMap.PreviewCopySeam`. `AppDelegate.validateMenuItem` and `WolfWindowController.copy()` both consult that one seam decision with the same two inputs, so the menu can never claim enabled while Copy yields nothing, and the Neovim visual-mode yank path is reached by exactly the same branch as today.

## 2. Files to create

### `crates/wolf-macos/Sources/WolfKeyMap/PreviewCopySeam.swift`

A new seam file rather than an addition to `PreviewSeam`. Reason to state in its header: `PreviewSeam` owns reload arithmetic plus the click-to-focus channel and `PreviewScrollSeam` owns the scroll channel; a third channel is a third file, which is also what the one-entry-per-file README rule expects. Imports **Foundation only**.

```swift
public enum PreviewCopySeam {

  public struct Selection: Equatable {
    public let text: String
    public init(text: String)
    /// `true` when `text` is non-empty after trimming `.whitespacesAndNewlines`.
    public var isCopyable: Bool { get }
  }

  public enum SelectionMessage: Equatable {
    case selection(Selection)   // a well-formed push carrying copyable text
    case cleared                // a well-formed push with no (or whitespace-only) selection
  }

  public enum CopySource: Equatable {
    case previewSelection(String)   // the payload is the RAW selected text
    case neovimVisual
    case none
  }

  /// Must differ from `PreviewSeam.clickFocusHandlerName` and
  /// `PreviewScrollSeam.stateHandlerName` — all three ride one
  /// `WKUserContentController`.
  public static let selectionHandlerName = "wolfPreviewSelection"

  public static func selectionScriptJS(handlerName: String = selectionHandlerName) -> String

  /// `nil` means malformed — ignore the message and leave the cache alone.
  public static func decodeSelectionMessage(_ body: Any?) -> SelectionMessage?

  public static func copySource(
    previewSelection: Selection?, isVisualMode: Bool) -> CopySource

  /// Defined as `copySource(...) != .none`, so the two can never disagree.
  public static func isCopyEnabled(
    previewSelection: Selection?, isVisualMode: Bool) -> Bool
}
```

Rules to encode and document on the declarations:

- `copySource` precedence, ordered: a non-`nil` `previewSelection` whose `isCopyable` is true wins over `isVisualMode`; otherwise `isVisualMode` gives `.neovimVisual`; otherwise `.none`. Preview wins because the caller passes a non-`nil` selection only when the **active pane is that preview**, so a stale global nvim mode must not override what the user is looking at. A cached-but-non-copyable entry must **not** mask visual mode — a named test case.
- `.previewSelection` carries the **raw, untrimmed** text. Trimming decides *whether* to copy; it must never change *what* is copied.
- Whitespace-only counts as empty: a stray click-drag across a margin selects whitespace, and clobbering `NSPasteboard` with `" "` is worse than doing nothing. Note that `CharacterSet.whitespaces` includes U+00A0, so a non-breaking-space-only selection is also "empty".
- `decodeSelectionMessage` accepts only `[String: Any]` with a `String` under key `"t"`. Anything else (non-dictionary, missing key, wrong value type, `nil`) returns `nil`. Extra keys are ignored. An empty or whitespace-only `"t"` returns `.cleared` — a positive clear, distinct from the malformed `nil`, because a collapsed selection must actively drop the cached entry.

**The JS contract** produced by `selectionScriptJS(handlerName:)`:

- One self-invoking function wrapped entirely in `try { … } catch (e) {}`, mirroring `PreviewScrollSeam.stateScriptJS`, so a missing message handler can never throw into the page.
- A single internal `post` closure: read `window.getSelection()`; if absent, `rangeCount === 0`, or `isCollapsed`, the text is the empty string (an **O(1) early-out** — `toString()` is never called for a collapsed selection); otherwise `selection.toString()`. Post `{ t: <string> }` to `window.webkit.messageHandlers.<handlerName>`, guarded by the same existence check the scroll script uses.
- Payload shape is exactly one key, always present. Empty string is the "cleared" signal; deliberately no separate boolean, so the Swift decoder has one thing to validate.
- Listeners:
  1. `document.addEventListener('selectionchange', …)` — **rAF-throttled** with the same `pending` flag idiom as the scroll script's `scroll` listener. Primary trigger and the one needing throttling: during a drag-select it fires per pointer move and `toString()` is O(selection length).
  2. `mouseup` in the **capture phase**, unthrottled. The "settled" push, and what makes §7's ordering safe: it runs after WebKit collapses the selection at a click point, capture-phase for the same reason `clickFocusJS` is.
  3. `keyup` in the capture phase, unthrottled. Covers ⌘A (Wolf's menu has no Select All item, so ⌘A reaches the web view) without depending on `selectionchange` firing for a programmatic Select All.
  4. One `post()` at document start, so a full `loadHTMLString` resets the cache with no Swift involvement.
- `keydown` is deliberately absent: the selection is not yet updated at keydown, so a push there would be stale.

### `crates/wolf-macos/Sources/WolfMacOS/PreviewSelectionHandler.swift`

The thin `WKScriptMessageHandler` adapter, structurally identical to `PreviewScrollStateHandler.swift` — copy its header shape (why the push direction, why `container` is weak, main-thread callback).

- `final class PreviewSelectionHandler: NSObject, WKScriptMessageHandler`
- `let win: Int64`, `weak var container: PaneContainerView?`, `init(win:container:)`
- `userContentController(_:didReceive:)` calls `PreviewCopySeam.decodeSelectionMessage(message.body)` and switches: `.selection(let sel)` → `container?.recordPreviewSelection(win: win, selection: sel)`; `.cleared` → `container?.recordPreviewSelection(win: win, selection: nil)`; `nil` → return without touching the cache.
- No logic beyond that switch. Imports `WebKit` and `WolfKeyMap`.
- No BUILD edit needed: `wolf_macos_lib` globs `Sources/WolfMacOS/*.swift`.

### `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewCopySeamTests.swift`

See §4. Header comment in the house style of `PreviewSeamTests.swift`: a "CONTRACT UNDER TEST" block restating the seam's signatures and the decision matrix.

## 3. Files to modify

### `crates/wolf-macos/BUILD.bazel` — two srcs lists

Both hand-maintained; missing either is a silent gap.

1. `swift_library(name = "wolf_key_map")` `srcs`: add `"Sources/WolfKeyMap/PreviewCopySeam.swift"` immediately before line 91's `"Sources/WolfKeyMap/PreviewLinkSeam.swift"`, keeping alphabetical order. **Without this the seam does not compile into the module at all.**
2. `swift_test(name = "WolfKeyMapTests")` `srcs`: add `"Tests/WolfKeyMapTests/PreviewCopySeamTests.swift"` immediately before line 290's `"Tests/WolfKeyMapTests/PreviewLinkSeamTests.swift"`.

No change to `wolf_macos_lib` (globbed) or to `swift_format_srcs` / `markdown_srcs` (globbed).

### `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`

**Add two stored properties** in the per-window map block (after `previewScrollStates`, around line 271):

- `private var previewSelectionHandlers: [Int64: PreviewSelectionHandler] = [:]` — the retained proxy, documented with the same reason as `previewScrollHandlers`.
- `private var previewSelections: [Int64: PreviewCopySeam.Selection] = [:]` — the live copyable selection each preview page pushed. Document that its only reader is the synchronous menu-validation / copy path, and that it may hold the whole document's text after ⌘A (bounded, dropped on document replacement and on pane close).

**`ensurePreviewWebView(for:)` (line 1523)** — after the scroll-sync block (line 1558), add a third parallel block: construct `PreviewSelectionHandler(win: win, container: self)`, store it in `previewSelectionHandlers[win]`, `config.userContentController.add(proxy, name: PreviewCopySeam.selectionHandlerName)`, and `addUserScript` a `WKUserScript(source: PreviewCopySeam.selectionScriptJS(), injectionTime: .atDocumentStart, forMainFrameOnly: true)`. Update the existing comment that says "Both handlers must be removed in `removePreviewWebView`" to say all three.

**`removePreviewWebView(_:)` (line 1627)** — add `removeScriptMessageHandler(forName: PreviewCopySeam.selectionHandlerName)` alongside the two existing removals, then `previewSelectionHandlers[win] = nil` and `previewSelections[win] = nil`.

**`syncPreviewWebViews()` prune path (line 1297)** — no edit needed; it already routes dead wins through `removePreviewWebView`. Verify only.

**`reloadDirtyPreviews()` (line 1340)** — add one synchronous clear, `previewSelections[win] = nil`, immediately before the `switch PreviewScrollSeam.updateStrategy(…)` at line 1394, so it covers **both** the `.inPlace` and `.fullLoad` branches. Comment why: both paths destroy the DOM selection, and the Swift-side clear is authoritative because it is synchronous — it does not depend on the page getting a `selectionchange` out before the user opens the Edit menu. The `captureDecision` `.drop` branch needs no separate clear: a source switch always takes `.fullLoad` and passes through the same line.

**New internal accessor** next to `activeWin` / `activePaneIsPreview` (around line 652-669):

```swift
/// The copyable selection the ACTIVE pane's preview page last pushed, or `nil`
/// when the active pane is not a preview / has no selection.
var activePaneSelection: PreviewCopySeam.Selection? { get }
```

Implementation: `let win = activeWin; guard win != 0 else { return nil }; return previewSelections[win]`. Document that identity is `activeWin` — the **per-view** active pane — and never `handle.active_win()`, which is shared across a project's tabpages and would let a background tab's preview selection enable Copy in this window. Unlike `WolfWindowController.historyWin`, there is deliberately **no** `active_win()` fallback, for that exact reason.

**New method** next to `recordPreviewScrollState(win:state:)` (line 1593):

```swift
/// Caches (or clears) the selection a preview page pushed out.
func recordPreviewSelection(win: Int64, selection: PreviewCopySeam.Selection?)
```

Pure cache write, no repaint, no wake — same doc shape as `recordPreviewScrollState`, including the note that a late message for a removed window writes a dead entry the next prune drops.

### `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift`

**Two new read-only properties**, placed with `isVisualMode` (line 797):

```swift
/// The active pane's pushed preview selection, or `nil`.
var activePreviewSelection: PreviewCopySeam.Selection? { paneContainer?.activePaneSelection }

/// Whether Edit > Copy has anything to copy, from either source.
var canCopy: Bool  // PreviewCopySeam.isCopyEnabled(previewSelection:isVisualMode:)
```

`canCopy` resolves both inputs in one place so validation and the action cannot drift. Both are cheap: one dictionary read plus the existing snapshot read behind `is_visual_mode()`.

**Rewrite `copy()` (line 664)** as a mechanical switch over `PreviewCopySeam.copySource(previewSelection: activePreviewSelection, isVisualMode: isVisualMode)`:

- `.previewSelection(let text)` → write `text` to `NSPasteboard.general`. It must **not** call `handle.copy_selection()`: that is a blocking RPC that yanks inside Neovim and would touch registers for a selection that is not Neovim's.
- `.neovimVisual` → today's body verbatim: `handle?.copy_selection().toString()`, guarded by `!text.isEmpty`, then write.
- `.none` → return.

Factor the two-line pasteboard write into a `private func writeToPasteboard(_ text: String)` so both branches share it. Rewrite the doc comment: Copy has two sources, chosen by the pure seam; the preview branch is fully synchronous because the text was already pushed.

### `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift`

- `validateMenuItem` `.copy` case (line 748-749): `return keyController?.canCopy ?? false`. The `keyController` resolution is unchanged (no `?? controllers.first` fallback for tagged commands).
- Update the function's doc comment (lines 701-706), which says Copy is disabled "unless the key window is in visual mode": it is enabled when the active pane is a preview with a non-empty selection **or** the key window is in visual mode.

### `crates/wolf-macos/Sources/WolfMacOS/AppMenu.swift`

Comment-only: the `editMenuItem` doc (line 231-233) says "Copy is enabled only in visual mode". Restate: Copy is enabled when either source has something to copy (gated by `AppDelegate.validateMenuItem` → `WolfWindowController.canCopy`); Paste is always enabled. No structural change — Copy stays a tagged Wolf command item.

### `crates/wolf-macos/Sources/WolfMacOS/CommandDispatcher.swift`

**No change.** `.copy` → `controller?.copy()` is already source-agnostic. Consequence for the record: the command-palette row and a drained `:WolfCopy` both reach the same `copy()`, so they copy a preview selection when the originating window's active pane is a preview — consistent, and a no-op when neither source has content, exactly as today.

## 4. Test plan

### New headless file: `Tests/WolfKeyMapTests/PreviewCopySeamTests.swift`

Scalar and string inputs only — no AppKit, no `WKWebView`, no Neovim.

**Handler-name cases**

1. `selectionHandlerName == "wolfPreviewSelection"`.
2. It differs from **both** `PreviewSeam.clickFocusHandlerName` and `PreviewScrollSeam.stateHandlerName`. Load-bearing: all three ride one `WKUserContentController`, where a duplicate name silently replaces a handler.

**JS-synthesis cases** (string containment, in the style of `PreviewScrollSeamTests`)

3. The generated source posts to `window.webkit.messageHandlers.wolfPreviewSelection`.
4. It registers `selectionchange`.
5. It registers `mouseup` and `keyup`.
6. It contains `requestAnimationFrame` (the throttle) and `isCollapsed` (the O(1) early-out).
7. It is wrapped in `try` / `catch`.
8. A custom `handlerName:` argument is interpolated instead of the default.

**Decoding cases**

9. `["t": "hello"]` → `.selection` with `text == "hello"`.
10. `["t": "  hi  "]` → `.selection` with the text preserved **verbatim**, leading/trailing whitespace included.
11. `["t": ""]` → `.cleared`.
12. `["t": "   \n\t "]` → `.cleared` (whitespace-only).
13. `["t": 42]` → `nil` (wrong value type).
14. `[:]` → `nil` (missing key).
15. `"hello"` (not a dictionary) → `nil`.
16. `nil` body → `nil`.
17. `["t": "x", "junk": 1]` → `.selection` (extra keys ignored).
18. Multi-line and non-ASCII text round-trips unchanged.

**Decision-matrix cases**

19. `(nil, isVisualMode: false)` → `.none`.
20. `(nil, true)` → `.neovimVisual`.
21. `(copyable, false)` → `.previewSelection` carrying the raw text.
22. `(copyable, true)` → `.previewSelection` — preview wins over visual mode.
23. `(Selection(text: ""), false)` → `.none`.
24. `(Selection(text: ""), true)` → `.neovimVisual` — a non-copyable cached entry must not mask visual mode.
25. `(Selection(text: " \n "), true)` → `.neovimVisual`; `(same, false)` → `.none`.
26. `Selection.isCopyable` directly: `""`, `" "`, `"\n"`, `"\t"`, U+00A0-only are all false; `"a"` and `" a "` are true.
27. **Equivalence sweep**: loop the whole matrix asserting `isCopyEnabled(...) == (copySource(...) != .none)`.

### BUILD wiring and proof it runs

Add the file to `swift_test(name = "WolfKeyMapTests")` `srcs` as in §3, then run `bazelisk test //crates/wolf-macos:WolfKeyMapTests --test_output=all` and confirm the `PreviewCopySeamTests` case names appear. A file absent from `srcs` never compiles into the binary and never runs, with no error.

### Manual smoke steps (report at the end; not automatable)

- Drag-select text in a preview pane → Edit > Copy becomes enabled; ⌘C then pastes exactly the selected text into another app.
- Choosing Edit > Copy with the mouse copies the same bytes as ⌘C.
- Click once in the preview to collapse the selection → Copy greys out again (exercises §7).
- With a preview selection live, type in the source file (body-swap path) → Copy greys out.
- Editor pane unchanged: visual-mode ⌘C still yanks from Neovim; Copy is greyed in normal mode.
- ⌘A in the preview enables Copy and copies the whole document.
- Two preview panes, selection in one: focusing the other greys Copy out. Focusing back does NOT re-enable it, and that is correct: a preview pane regains focus only by a click inside its web view, and that same `mousedown` collapses the selection (§7 defense 2). Verify the re-enable half with ⌘A after refocusing instead.
- With a paragraph selected in a preview pane, click into the sidebar Search field, select the query text, press ⌘C and paste elsewhere: confirm which text lands. Copy is now enabled while a preview selection is cached, so the main menu's ⌘C wins over the focused field. Check whether a live Neovim visual-mode selection already does the same — it should, which makes this a pre-existing defect this change widens rather than a regression it introduces.
- Close a preview pane, then open a new one and repeat — no crash, no stale enablement (proves handler removal and cache prune).
- Copy in a background tab's preview does not enable Copy in the key window.

## 5. Implementation order

1. `Sources/WolfKeyMap/PreviewCopySeam.swift` + its `wolf_key_map` `srcs` entry.
2. `Tests/WolfKeyMapTests/PreviewCopySeamTests.swift` + its `WolfKeyMapTests` `srcs` entry. Run the suite and confirm the new cases execute before touching any AppKit file.
3. `PaneContainerView.swift`: the two dictionaries, `recordPreviewSelection(win:selection:)`, `activePaneSelection`, the `ensurePreviewWebView` registration, the `removePreviewWebView` teardown, the `reloadDirtyPreviews` clear.
4. `PreviewSelectionHandler.swift`.
5. `WolfWindowController.swift`: `activePreviewSelection`, `canCopy`, the `copy()` rewrite, `writeToPasteboard`.
6. `AppDelegate.swift`: the `.copy` validation case and its doc comment.
7. `AppMenu.swift`: the `editMenuItem` comment.
8. Build the app (`bazelisk build //crates/wolf-macos:Wolf`), re-run `//crates/wolf-macos:WolfKeyMapTests`, then the manual smoke list.
9. Documentation (§9), then `prettier --write` on every touched Markdown file and `./tools/lint/markdown_check.sh` on them.

## 6. Dependencies

None. No new external packages, no new Bazel external repos, no FFI surface change, no Rust change.

## 7. Interaction with the click-to-focus script

A `mousedown` in the preview posts to `wolfPaneFocus` (focusing the pane) and, in WebKit, also collapses the selection at the click point. The hazard is a stale `previewSelections[win]` left true after a click that cleared the selection. Three independent defenses, in the order they fire:

1. The collapse fires `selectionchange`, whose rAF-throttled push runs on the next frame and posts `{ t: "" }` → `.cleared` → the entry is removed.
2. The unthrottled capture-phase `mouseup` push runs after the collapse in the same gesture, so the cache is correct by the time the button is released — well before any menu can open or a ⌘C can be typed.
3. The two handlers are independent and neither reads the other's state, so no ordering between `wolfPaneFocus` and `wolfPreviewSelection` can produce a wrong answer. Both are posted from the page on the same IPC path and delivered on the main thread in post order; `focusPreviewPane(win:)` deliberately does not touch `previewSelections`.

The remaining theoretical failure — `selectionchange` never firing *and* no `mouseup` arriving — leaves at worst a stale enabled Copy that copies the text the user had genuinely selected a moment earlier. That is the benign direction.

## 8. Payload decision: text is carried in the push

**Chosen: carry the text.** `copy()` is then fully synchronous, exactly like the Neovim branch beside it.

- **For:** menu validation is synchronous and cannot await `evaluateJavaScript`, so a cache is required for the enable decision regardless. Deriving both the enable decision and the copied bytes from the *same* cached snapshot makes "enabled but copies nothing" structurally impossible. It also avoids an asynchronous `NSPasteboard` write, which would let a ⌘C immediately followed by ⌘V read the previous clipboard, and avoids copying an empty string when the document is replaced between ⌘C and the JS callback.
- **Against:** `selection.toString()` is O(selection length) and runs on every throttled push. Mitigated by the rAF throttle, by the O(1) collapsed early-out (the common case never calls `toString()`), and by selection changes being user-gesture-driven — typing in the editor pane produces no selection pushes.
- **Staleness:** the two document-replacement paths are covered by the single synchronous `previewSelections[win] = nil` before the `updateStrategy` switch in `reloadDirtyPreviews`, authoritative precisely because it does not depend on the page delivering a `selectionchange` in time. The page's document-start `post()` then re-seeds the cache to cleared on the full-load path.
- **Memory:** worst case one document's text per preview window, dropped on document replacement and on `removePreviewWebView`.

## 9. Documentation updates required

Per CLAUDE.md ownership. Edit existing entries in place; do not append.

1. **`crates/wolf-macos/Sources/WolfKeyMap/README.md`** — new `#### PreviewCopySeam.swift` entry before the `PreviewLinkSeam.swift` entry (line 457). Purpose: the pure rules behind Copy from a preview pane — the selection push channel and the "which source does Copy use" decision. Public API: the four symbols plus `selectionHandlerName`. Decisions: the text rides in the payload so `copy()` is synchronous and the enable decision and the copied bytes come from one snapshot; whitespace-only counts as empty (with the U+00A0 note); a copyable preview selection outranks Neovim visual mode because the caller only supplies one for the active pane. Gotchas: the handler name shares a `WKUserContentController` with `PreviewSeam.clickFocusHandlerName` and `PreviewScrollSeam.stateHandlerName`, so all three must differ; teardown is in DECISIONS.md (link, do not restate). Budget ~200 words.
2. **`crates/wolf-macos/Sources/WolfMacOS/README.md`**
   - New `#### PreviewSelectionHandler.swift` entry after the `PreviewScrollStateHandler.swift` entry (line 454): purpose, `init(win:container:)`, the weak-container reason, the malformed-body-is-ignored-versus-empty-body-clears distinction, and a link to DECISIONS.md.
   - `#### PaneContainerView.swift` (line 315): one Public API bullet for `activePaneSelection`, stating it keys on `activeWin` (per-view) with no `active_win()` fallback so a background tab's selection cannot enable Copy here. Adjust the closing "four-file model" bullet if the DECISIONS section's file count changes.
   - `#### WolfWindowController.swift` (line 632): one Public API bullet for `canCopy`.
   - `AppMenu.swift` / `AppDelegate.swift` / `CommandDispatcher.swift` entries: no change.
3. **`crates/wolf-macos/DECISIONS.md`**, "How a preview reaches its web view" (line 66). Correct the sentence at line 74 — "Two script-message handlers ride on one preview web view's content controller… and both must be removed at teardown" — to name **three** channels (click-to-focus, scroll state, selection), keeping the distinct-names and teardown requirements. Add at most three sentences: Copy reads the same pushed cache for the same reason scroll state does — menu validation is synchronous and `evaluateJavaScript` is not — and the cache is cleared synchronously on both document-replacement paths rather than trusting the page to report the collapse. Link, do not restate the seam's rules.
4. **`crates/wolf-macos/Tests/WolfKeyMapTests/README.md`** — new `#### PreviewCopySeamTests.swift` entry before `PreviewLinkSeamTests.swift` (line 250): the decode table, the `copySource` / `isCopyEnabled` matrix including the equivalence sweep, the JS-containment assertions. Gotchas: the handler-name-distinctness case exists because all three names share one content controller; live selection and a real ⌘C are manual-smoke.
5. **`crates/wolf-ffi/src/README.md`** line 74 — "`is_visual_mode(&self) -> bool` — Gates the Edit > Copy menu item." Inaccurate: it gates only the Neovim branch of Copy. One-line correction. Line 72's `copy_selection` entry stays accurate.
6. **`architecture.md`** line 101 — "Copy yanks the visual-mode selection over a blocking RPC so the shell can write it out". Amend: Copy has two sources, and a selection in a Markdown preview is copied from the web view without any RPC. One clause; it is a genuine cross-crate contract statement.
7. **`crates/wolf-core/src/command.rs`** line 69 — the `Command::Copy` doc comment "Copy the visual-mode selection to the system clipboard." Reword to "Copy the current selection to the system clipboard" so core does not assert a front-end rule it no longer owns. No `COMMAND_SPECS`, palette, or FFI change accompanies it.

## 10. Lint considerations

- No linter ignore directives expected.
- swift-format: both new Swift files are covered by the `swift_format_srcs` glob. Run `bazel run //tools/lint:swift_format.fix`, verify with `bazel test //tools/lint:swift_format`. The generated JS is a multi-line string literal like `PreviewScrollSeam.stateScriptJS` — keep the indentation shape swift-format already accepts there.
- Markdown: after editing docs run `prettier --write` (`.prettierrc` sets `proseWrap: never`), then `./tools/lint/markdown_check.sh <files>` or `bazel test //tools/lint:markdown`. New entries must be present tense with no "now", "no longer", or "previously"; the DECISIONS.md edit is a rewrite of the existing sentence, not a sentence added beside it.
