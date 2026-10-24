// Pure Markdown-preview scroll-sync rules and the JavaScript they generate.
// See notes/crates/wolf-macos/Sources/WolfKeyMap/PreviewScrollSeam.swift.md#no-json-escaper

import Foundation

/// Pure Markdown-preview cursor-anchor resolution, scroll-decision arithmetic, and
/// scroll-JS synthesis.
public enum PreviewScrollSeam {

  // MARK: - Types

  /// A cursor position in a SOURCE Markdown buffer, as reported by core's
  /// `wolf_markdown_cursor` signal and handed across the bridge by
  /// `FfiPreview.source_cursor_line()` / `source_cursor_col()`.
  public struct CursorPos: Equatable, Hashable {
    /// 1-based hard buffer line, matching the `wl-{L}-{C}` marker scheme's `L`.
    /// `0` is the "never reported" sentinel.
    public let line: UInt32
    /// 0-based CHARACTER column (NOT a byte column), matching the marker scheme's
    /// `C` before it is rounded down to a multiple of ``markerColumnStride``.
    public let col: UInt32

    public init(line: UInt32, col: UInt32) {
      self.line = line
      self.col = col
    }

    /// Whether a cursor has ever been reported for this buffer. A line of `0` means
    /// no `wolf_markdown_cursor` has arrived, and the front-end must NOT scroll.
    public var isKnown: Bool { line > 0 }
  }

  /// The preview page's live scroll state, pushed OUT of the page over the
  /// ``stateHandlerName`` message handler (never pulled — a pull would be async and
  /// could not beat a `loadHTMLString`). The AppKit side caches the latest value per
  /// preview window so capturing it before a reload is a synchronous dictionary read.
  public struct ScrollState: Equatable {
    /// The viewport-relative Y of the cursor anchor's box, or `nil` when the anchor
    /// could not be resolved in the current document.
    public let anchorTop: Double?
    /// The raw `window.scrollY`, used as the fallback when no anchor resolves.
    public let scrollY: Double

    public init(anchorTop: Double?, scrollY: Double) {
      self.anchorTop = anchorTop
      self.scrollY = scrollY
    }
  }

  /// Everything needed to put the edited region back where it visually was after a
  /// full `loadHTMLString` replaced the document. Captured synchronously immediately
  /// BEFORE the reload and consumed in the web view's `didFinish`.
  public struct RestoreRequest: Equatable {
    /// The cursor the reload was captured at.
    public let cursor: CursorPos
    /// Where that cursor's anchor sat in the viewport before the reload, or `nil`
    /// when no anchor was resolved (then ``fallbackScrollY`` is used instead).
    public let targetY: Double?
    /// The raw pre-reload `window.scrollY`.
    public let fallbackScrollY: Double

    public init(cursor: CursorPos, targetY: Double?, fallbackScrollY: Double) {
      self.cursor = cursor
      self.targetY = targetY
      self.fallbackScrollY = fallbackScrollY
    }
  }

  /// What the pre-reload capture step must do for one preview window.
  ///
  /// This decision deliberately lives in the pure seam rather than inline in
  /// `PaneContainerView`: BOTH shipping-blocker defects the review found were
  /// ordering bugs in that (untestable, AppKit-bound) capture block, so the rules
  /// moved here where they are pinned headlessly and the caller keeps only a
  /// mechanical `switch`.
  public enum CaptureDecision: Equatable {
    /// The preview window navigated to ANOTHER SOURCE FILE. There is nothing to
    /// restore, and the caller must ALSO clear both the cached scroll state and any
    /// pending restore for the window — that is what makes a fragment-less
    /// cross-file link open the new document at its top.
    case drop
    /// A restore is already in flight and the cache carries no position, so the
    /// in-flight capture is the better one. Leave the pending restore untouched.
    case keepPending
    /// Capture this restore for the window (overwriting any pending one).
    case capture(RestoreRequest)
  }

  /// A rendered preview document carved into "the part that must not be touched"
  /// and "the part that is swapped in place".
  ///
  /// Produced by ``splitDocument(_:)`` from `render_html`'s FIXED wrapper. The head
  /// is deliberately NOT copied — only its length is kept, because the only question
  /// ever asked of it is "did it change?" (``updateStrategy(html:currentLoad:liveDocument:loadInFlight:hasPendingFragment:)``
  /// rule 6). Copying it would mean carrying the ~965 KiB of inlined base64
  /// `@font-face` CSS around on every keystroke, which is the exact cost the in-place
  /// path exists to remove.
  public struct DocumentSplit: Equatable {
    /// The UTF-8 BYTE count of everything up to and INCLUDING the `<body …>` open
    /// tag. A byte count, not a `Character` count, matching the Rust side's notion of
    /// length (the wrapper is `format!`ed from a Rust `String`).
    public let headLength: Int
    /// The body element's INNER html — what `__wolfUpdate` `DOMParser`-parses and
    /// `replaceChildren`s into the live document. The open/close `<body>` tags are
    /// delimiters, not content: including the open tag would nest a second body
    /// inside the live one.
    public let body: String

    public init(headLength: Int, body: String) {
      self.headLength = headLength
      self.body = body
    }
  }

  /// What is currently COMMITTED in a preview web view.
  ///
  /// Deliberately separate from `PreviewSeam.PreviewLoad`, which is what was last
  /// DISPATCHED and must stay exactly `(sourceBufnr, sourceTick)` so a cursor-only
  /// change can never arm a reload. This type carries no tick on purpose: what
  /// is committed is a DOCUMENT, and how many edits behind it is says nothing about
  /// whether its body can be swapped. What it carries instead is `headLength`, the
  /// only thing a body-only swap can get WRONG.
  public struct LiveDocument: Equatable {
    /// The SOURCE buffer whose render is committed in the web view.
    public let sourceBufnr: Int64
    /// The ``DocumentSplit/headLength`` of the committed document.
    public let headLength: Int

    public init(sourceBufnr: Int64, headLength: Int) {
      self.sourceBufnr = sourceBufnr
      self.headLength = headLength
    }
  }

  /// How a preview window's next render must reach the page.
  public enum UpdateStrategy: Equatable {
    /// `loadHTMLString`, with `didFinish` driving the existing capture/restore
    /// machinery. The path for first bind, a source switch, a pending cross-file
    /// fragment, a load already in flight, HTML that cannot be split, and recovery
    /// after an in-place failure.
    case fullLoad
    /// `callAsyncJavaScript(__wolfUpdate)` with this body — NO navigation, so no
    /// document commit, no ~1 MB CSS re-parse, and no frame at scrollY 0.
    case inPlace(body: String)
  }

  /// What one preview navigation's terminal callback (`didFinish` or `didFail…`) may
  /// do to that window's live and pending document descriptors.
  ///
  /// Produced by ``loadCommitOutcome(remaining:pending:succeeded:)``. The three cases
  /// are deliberately distinct VALUES rather than one optional, because a caller's
  /// mechanical `switch` is only as safe as the type's ability to tell them apart —
  /// and `.discard` versus `.commit(nil)` is exactly the pair whose conflation wiped a
  /// good live document.
  public enum LoadCommitOutcome: Equatable {
    /// More navigations are still outstanding for this window, so THIS callback is
    /// not the one that decides: leave the live AND the pending descriptor alone.
    /// The case carries no payload on purpose — a superseded load's callback is
    /// handed nothing it could commit, and is told nothing that would let it clear
    /// the pending slot.
    case stillOutstanding
    /// This was the last outstanding navigation and it SUCCEEDED: the pending
    /// descriptor becomes the live one and the pending slot is cleared. The payload
    /// is optional because an unsplittable document stages no descriptor, and
    /// `.commit(nil)` must positively CLEAR the live slot — an unsplittable document
    /// may never be treated as live.
    case commit(LiveDocument?)
    /// This was the last outstanding navigation and it FAILED: leave the live
    /// descriptor UNCHANGED and clear only the pending one.
    case discard
  }

  /// Whether a resolved anchor needs the viewport moved, and to where.
  public enum FollowDecision: Equatable {
    /// The anchor is inside the comfort band — do NOT scroll. This is the
    /// anti-twitch case: without it the preview would jerk on every keystroke.
    case stay
    /// The anchor crossed an edge threshold — scroll so it lands at this
    /// viewport-relative Y.
    case scroll(toViewportY: Double)
  }

  // MARK: - Tuning constants (the SINGLE source of truth; interpolated into the JS)

  /// The column spacing of the emitted position markers. Mirrors
  /// `wolf_core::markdown::MARKER_COLUMN_STRIDE`; a cross-language coupling, so a
  /// change on the Rust side must be mirrored here (and is pinned by a test).
  public static let markerColumnStride: UInt32 = 100

  /// The floor on the viewport-edge comfort band, in CSS pixels. Two rows of
  /// `base.css`'s 24px baseline grid — and the same 48px `scroll-margin-top`
  /// headings already use — so the band is grid-aligned and always at least two
  /// lines of clearance.
  public static let minEdgeThreshold: Double = 48

  /// The comfort band as a fraction of viewport height: 15% keeps a generous band
  /// on a tall pane without eating it.
  public static let edgeThresholdFraction: Double = 0.15

  /// The cap on the comfort band as a fraction of viewport height. Without it, a
  /// 100px-tall pane would take the 48px floor on BOTH edges, leaving a 4px band and
  /// therefore near-continuous re-scrolling.
  public static let maxEdgeThresholdFraction: Double = 0.35

  /// Where the anchor lands when a scroll IS required, as a fraction of viewport
  /// height. Biased ABOVE center on purpose: typing moves the cursor DOWN, so
  /// leaving two thirds of the viewport below means the next several lines of typing
  /// stay inside the band and do not re-trigger a scroll.
  public static let restPositionFraction: Double = 0.33

  /// The bound on the JS "nearest PRECEDING line marker" downward scan. Every hard
  /// line gets a `wl-{L}-0` marker, so the scan normally terminates in ONE O(1) id
  /// lookup; the bound only caps the pathological case where the document shrank far
  /// past the reported line.
  public static let maxAnchorScanLines: UInt32 = 200

  /// The `WKScriptMessageHandler` name the injected scroll-state script posts to.
  /// MUST differ from `PreviewSeam.clickFocusHandlerName` — both are registered on
  /// the SAME `WKUserContentController`.
  public static let stateHandlerName = "wolfPreviewScroll"

  // MARK: - Pure anchor arithmetic

  /// `col` rounded DOWN to a multiple of ``markerColumnStride`` — the `C` of the
  /// `wl-{L}-{C}` marker that actually exists in the document. Integer division, so
  /// `0…99 -> 0`, `100…199 -> 100`, and so on.
  public static func anchorColumn(_ col: UInt32) -> UInt32 {
    col / markerColumnStride * markerColumnStride
  }

  /// The marker element ids to try, in preference order, for a cursor at
  /// `(line, col)`:
  ///
  ///  1. `wl-{line}-{anchorColumn(col)}` — the nearest marker at or before the
  ///     cursor's column on its own line,
  ///  2. `wl-{line}-0` — the line's own start marker (every hard line has one),
  ///
  /// deduped: when `anchorColumn(col) == 0` the two are the SAME id, so exactly one
  /// element is returned. An unknown cursor (`line == 0`) yields no candidates at
  /// all — there is nothing to scroll to.
  ///
  /// This mirrors the resolution order the injected JS uses, so the ordering
  /// contract is headlessly testable in Swift. (The JS additionally falls back to a
  /// bounded backward scan and then a document-order sweep, neither of which can be
  /// expressed as a static id list.)
  public static func anchorIds(line: UInt32, col: UInt32) -> [String] {
    guard line > 0 else { return [] }
    let exact = anchorColumn(col)
    guard exact != 0 else { return ["wl-\(line)-0"] }
    return ["wl-\(line)-\(exact)", "wl-\(line)-0"]
  }

  /// The comfort band, in CSS pixels, at each edge of a `viewportHeight`-tall pane:
  /// `min(max(minEdgeThreshold, vh * edgeThresholdFraction), vh * maxEdgeThresholdFraction)`,
  /// never negative. The floor guarantees at least two text lines of clearance on a
  /// tall pane; the cap guarantees a SHORT pane still has a usable band.
  public static func edgeThreshold(viewportHeight: Double) -> Double {
    let raw = min(
      max(minEdgeThreshold, viewportHeight * edgeThresholdFraction),
      viewportHeight * maxEdgeThresholdFraction)
    return max(0, raw)
  }

  /// Whether an anchor sitting at `anchorTop` (viewport-relative Y) in a
  /// `viewportHeight`-tall pane needs the viewport moved.
  ///
  /// `.stay` when `edgeThreshold <= anchorTop <= viewportHeight - edgeThreshold` —
  /// the boundaries are INCLUSIVE, so an anchor resting exactly ON a threshold is
  /// considered comfortable and does not scroll. Otherwise `.scroll` to
  /// `viewportHeight * restPositionFraction`.
  ///
  /// This mirrors the rule the injected JS applies, so the rule itself is headlessly
  /// testable.
  public static func followDecision(anchorTop: Double, viewportHeight: Double)
    -> FollowDecision
  {
    let t = edgeThreshold(viewportHeight: viewportHeight)
    if anchorTop >= t && anchorTop <= viewportHeight - t {
      return .stay
    }
    return .scroll(toViewportY: viewportHeight * restPositionFraction)
  }

  /// The preview WINDOW handles whose source cursor differs from the one a follow
  /// was last evaluated for: those in `current` (win → live ``CursorPos``) whose
  /// entry is absent from — or unequal to — `applied`.
  ///
  /// The exact structural mirror of `PreviewSeam.winsNeedingReload`, and
  /// deliberately INDEPENDENT of it: `PreviewSeam.PreviewLoad` carries only
  /// `(sourceBufnr, sourceTick)`, so a cursor-only change can arm THIS gate while
  /// being structurally incapable of arming the expensive HTML-reload gate. Windows
  /// in `applied` but absent from `current` (closed previews) are never returned.
  /// Order is unspecified.
  public static func winsNeedingScroll(
    current: [Int64: CursorPos],
    applied: [Int64: CursorPos]
  ) -> [Int64] {
    current
      .filter { win, cursor in applied[win] != cursor }
      .map(\.key)
  }

  // MARK: - The pre-reload capture decision

  /// What the caller must do with `pendingPreviewRestores[win]` immediately BEFORE
  /// it replaces a preview window's document with `loadHTMLString`.
  ///
  /// The three rules are ORDERED and the ordering is load-bearing:
  ///
  ///  1. **`.drop`** when the window's bound SOURCE BUFFER changed
  ///     (`previousLoad!.sourceBufnr != currentLoad.sourceBufnr`). A source switch is
  ///     a NAVIGATION, not an edit, so the previous file's offset must not be carried
  ///     into the new one: reading a long `guide.md` scrolled to y≈3000 and clicking a
  ///     plain `[appendix](appendix.md)` (NO `#fragment`, so nothing is stashed in
  ///     `pendingPreviewFragments`) would otherwise open `appendix.md` 3000px into its
  ///     middle — worse than the pre-feature behavior, where `loadHTMLString` reset
  ///     scroll to 0. The TICK is irrelevant here: two freshly opened files can
  ///     legitimately share a `changedtick`, so only the bufnr may decide. A `nil`
  ///     `previousLoad` is a FIRST BIND, not a switch away from anything.
  ///  2. **`.keepPending`** when a restore is already in flight (`pendingRestore !=
  ///     nil`) and `cachedState` carries NO position — either it is `nil`, or it is
  ///     the document-start SEED (`anchorTop == nil && scrollY == 0`). The new
  ///     document's `.atDocumentStart` `post()` resolves nothing (`__wolfLine` is
  ///     still 0) and pushes exactly that seed, and reload N+1 can be armed before
  ///     load N's `didFinish` arrives (see the note on straddling reloads in
  ///     `PaneContainerView.reloadDirtyPreviews`), so capturing the seed would replace
  ///     a GOOD pending restore with a target-less one and the page would sit at the
  ///     top — the original jump-to-top bug, returned. `cachedState == nil` is folded
  ///     into the same rule DEFENSIVELY: it carries no position either, so the same
  ///     reasoning applies. Do not "simplify" it back to the literal seed shape.
  ///  3. **`.capture`** otherwise — including a REAL scroll position arriving while a
  ///     restore is in flight, which must still win. Without that boundary a preview
  ///     whose `didFinish` was slow would freeze on its first captured offset and
  ///     ignore every later scroll.
  ///
  /// - Parameters:
  ///   - previousLoad: `lastPreviewLoads[win]` read BEFORE it is reassigned to
  ///     `currentLoad` — an ordering hazard, and the whole of rule 1.
  ///   - currentLoad: The `(sourceBufnr, sourceTick)` about to be loaded.
  ///   - cursor: The source buffer's live cursor, carried VERBATIM into the request
  ///     (the JS does its own rounding to a marker stride).
  ///   - cachedState: `previewScrollStates[win]`, the page's last pushed state.
  ///   - pendingRestore: `pendingPreviewRestores[win]`, an earlier capture not yet
  ///     consumed by `previewDidFinishLoad`.
  public static func captureDecision(
    previousLoad: PreviewSeam.PreviewLoad?,
    currentLoad: PreviewSeam.PreviewLoad,
    cursor: CursorPos,
    cachedState: ScrollState?,
    pendingRestore: RestoreRequest?
  ) -> CaptureDecision {
    if let previousLoad, previousLoad.sourceBufnr != currentLoad.sourceBufnr {
      return .drop
    }
    let carriesNoPosition = cachedState.map { $0.anchorTop == nil && $0.scrollY == 0 } ?? true
    if pendingRestore != nil && carriesNoPosition {
      return .keepPending
    }
    return .capture(
      RestoreRequest(
        cursor: cursor,
        targetY: cachedState?.anchorTop,
        fallbackScrollY: cachedState?.scrollY ?? 0))
  }

  // MARK: - The in-place update decision

  /// Carves a whole rendered preview document into its head LENGTH and its body's
  /// INNER html, or `nil` when it does not have the expected shape.
  ///
  /// The shape is `render_html`'s fixed wrapper (`crates/wolf-core/src/markdown.rs`):
  /// `<!doctype html><html><head>…</head><body class="wolf-markdown">{body}</body></html>`.
  ///
  /// The scan is FIRST-open / LAST-close, and that asymmetry is deliberate. Raw HTML
  /// passes straight through pulldown-cmark (there is no disable flag in
  /// `markdown_options()`), so a source file can put a LITERAL `</body>` into the
  /// rendered body — but the wrapper's own closing tag is always the LAST one.
  /// Symmetrically, any `<body` appearing in content is always AFTER the wrapper's
  /// open tag, so the FIRST one is the wrapper's.
  ///
  /// Returns `nil` when there is no `<body`, no `>` closing that tag, no `</body>`,
  /// or when the closing tag PRECEDES the open one (an inverted range). An empty
  /// `html` — what `WolfCore::render_preview` returns on an RPC failure — hits the
  /// first of those. Every `nil` degrades that tick to ``UpdateStrategy/fullLoad``,
  /// i.e. exactly today's behavior; a wrapper change can therefore cost performance
  /// but never correctness.
  ///
  /// This is a SCAN, not a parse: the body is returned byte-for-byte, never
  /// sanitized, re-encoded or normalized. The `.wolf-src` markers are what the page's
  /// `resolve` looks up by id, and the inline `style="color:#…"` highlight spans are
  /// the only thing a dark/light flip actually changes.
  public static func splitDocument(_ html: String) -> DocumentSplit? {
    guard let open = html.range(of: "<body") else { return nil }
    guard let openEnd = html.range(of: ">", range: open.upperBound..<html.endIndex) else {
      return nil
    }
    guard let close = html.range(of: "</body>", options: .backwards) else { return nil }
    guard close.lowerBound >= openEnd.upperBound else { return nil }
    return DocumentSplit(
      headLength: html[html.startIndex..<openEnd.upperBound].utf8.count,
      body: String(html[openEnd.upperBound..<close.lowerBound]))
  }

  /// How one preview window's freshly rendered `html` must reach its page.
  ///
  /// The rules are ORDERED and each is load-bearing:
  ///
  ///  1. **`.fullLoad` when `hasPendingFragment`.** A cross-file `other.md#anchor`
  ///     link stashes its fragment in `pendingPreviewFragments`, and that fragment is
  ///     delivered from `previewDidFinishLoad` — which an in-place update NEVER
  ///     fires. Swapping the body here would strand the anchor forever.
  ///  2. **`.fullLoad` when `loadInFlight`.** Mutating a document that an outstanding
  ///     `loadHTMLString` is about to replace is pointless, and it would race that
  ///     load's `didFinish` restore. What the caller passes here is
  ///     `(previewLoadsInFlight[win] ?? 0) > 0` — a per-window COUNT, never a set
  ///     membership, because TWO navigations can straddle one page load (the
  ///     coalescer serializes to one reload per ~33ms and does NOT wait for
  ///     `didFinish`). The count is what this rule actually guarantees: it stays
  ///     non-zero from the FIRST outstanding navigation until the LAST one's callback,
  ///     so no in-place update can ever run while any navigation is pending. A
  ///     Set-based flag guarantees only "the most recent callback has not landed", and
  ///     the earlier callback clears it out from under a still-navigating load.
  ///     Because the slot is empty again exactly when the count reaches zero — every
  ///     capture is consumed by a `didFinish`, and
  ///     ``loadCommitOutcome(remaining:pending:succeeded:)``'s `.discard` is where the
  ///     caller drops a restore no navigation will ever consume — this rule ALSO
  ///     keeps `pendingPreviewRestores[win] == nil` on every in-place path.
  ///  3. **`.fullLoad` when `liveDocument == nil`.** Nothing is committed, so
  ///     `window.__wolfUpdate` may not exist. Covers first bind AND recovery after a
  ///     failed in-place update, which clears `previewLiveDocuments[win]` precisely to
  ///     force this rule.
  ///  4. **`.fullLoad` when the SOURCE BUFFER changed.** That is a NAVIGATION, and
  ///     the new file must open at its top — the source-switch reset, mirrored here at
  ///     the strategy layer. The tick is irrelevant: two freshly opened files can share a
  ///     `changedtick`, so only the bufnr may decide.
  ///  5. **`.fullLoad` when ``splitDocument(_:)`` returns `nil`.**
  ///  6. **`.fullLoad` when the head LENGTH changed.** A body-only swap leaves the
  ///     committed `<head>` in place, so a changed head would mean stale CSS. Today
  ///     unreachable — the in-app preview pins `PreviewTheme::Default` +
  ///     `WidthMode::Full`, so the head is byte-identical on every render, INCLUDING
  ///     across a dark/light flip (that flip is a `prefers-color-scheme` media query
  ///     inside the constant CSS; only the body's inline highlight colors change) —
  ///     but encoding it as a RULE makes the premise self-enforcing rather than
  ///     comment-only. TRADE-OFF: this is a LENGTH comparison, not a content hash. A
  ///     future theme whose head happened to be the same length would slip through;
  ///     hashing ~1 MB per keystroke was judged the worse cost, and the failure mode
  ///     (stale CSS until the next full load) is mild.
  ///  7. **`.inPlace`** otherwise — the typing path, and the whole reason this
  ///     function exists.
  ///
  /// **Relationship to ``captureDecision(previousLoad:currentLoad:cursor:cachedState:pendingRestore:)``.**
  /// That function is UNCHANGED by the in-place path and is still called exactly
  /// where it is called today: immediately before `loadHTMLString`, i.e. on the
  /// `.fullLoad` branch only. The two answer different questions at different times —
  /// `captureDecision` keys on `lastPreviewLoads[win]` ("what did we last DISPATCH"),
  /// this function keys on `previewLiveDocuments[win]` ("what is COMMITTED"). Rule 4
  /// here and `captureDecision`'s rule 1 are near-duplicates BY DESIGN and always
  /// agree on a real source switch; both are required, because one runs before the
  /// navigation exists and the other before the restore is consumed. Neither may be
  /// removed on the strength of the other.
  ///
  /// - Parameters:
  ///   - html: The whole document `render_preview` just returned.
  ///   - currentLoad: The `(sourceBufnr, sourceTick)` this tick is rendering.
  ///   - liveDocument: `previewLiveDocuments[win]` — what is committed NOW.
  ///   - loadInFlight: `(previewLoadsInFlight[win] ?? 0) > 0` — the per-window count
  ///     of outstanding navigations, NOT a set membership (see rule 2).
  ///   - hasPendingFragment: `pendingPreviewFragments[win] != nil`.
  public static func updateStrategy(
    html: String,
    currentLoad: PreviewSeam.PreviewLoad,
    liveDocument: LiveDocument?,
    loadInFlight: Bool,
    hasPendingFragment: Bool
  ) -> UpdateStrategy {
    if hasPendingFragment { return .fullLoad }
    if loadInFlight { return .fullLoad }
    guard let liveDocument else { return .fullLoad }
    if liveDocument.sourceBufnr != currentLoad.sourceBufnr { return .fullLoad }
    guard let split = splitDocument(html) else { return .fullLoad }
    if liveDocument.headLength != split.headLength { return .fullLoad }
    return .inPlace(body: split.body)
  }

  /// What a preview navigation's terminal callback may commit, given how many
  /// navigations for that window are STILL outstanding after it decremented the count.
  ///
  /// TWO NAVIGATIONS CAN STRADDLE ONE PAGE LOAD. The reload coalescer serializes to one
  /// reload per ~33ms and does NOT wait for `didFinish`, so full load N+1 is routinely
  /// issued while N is still navigating, OVERWRITING the pending descriptor. WebKit then
  /// reports the superseded load N as a FAILURE as often as not, so which callback
  /// flavor arrives is not under our control and must not change the answer.
  ///
  /// The rules:
  ///
  ///  * `remaining > 0` → ``LoadCommitOutcome/stillOutstanding``, whatever `succeeded`
  ///    says. A superseded load's callback decides nothing: it may not commit, and it
  ///    may not destructively consume the pending descriptor that the load STILL
  ///    navigating needs. (Consuming it is exactly what a `Set<Int64>` flag would do,
  ///    after which the next coalescer tick would see an all-clear in-flight flag,
  ///    choose `.inPlace`, and swap a body into a document that is about to be
  ///    replaced — a visible flicker, i.e. the exact symptom the in-place path exists
  ///    to remove.)
  ///  * `remaining == 0 && succeeded` → ``LoadCommitOutcome/commit(_:)`` carrying
  ///    `pending` VERBATIM, including `nil`: a successful load of an unsplittable
  ///    document must positively clear the live slot, or
  ///    ``updateStrategy(html:currentLoad:liveDocument:loadInFlight:hasPendingFragment:)``
  ///    rule 6 would compare against a `headLength` describing nothing on screen.
  ///  * `remaining == 0 && !succeeded` → ``LoadCommitOutcome/discard``, which is
  ///    deliberately NOT `.commit(nil)`. A failed navigation never commits, so WebKit
  ///    keeps displaying the PREVIOUS document and the previous live descriptor stays
  ///    accurate; erasing it would force one pointless extra full load and, worse,
  ///    would let rule 3 fire against a page that is perfectly swappable.
  ///
  /// The caller's whole bookkeeping is: decrement first, hand the REMAINING count
  /// here, then apply the answer mechanically. Nothing else belongs in the callback.
  ///
  /// - Parameters:
  ///   - remaining: `previewLoadsInFlight[win]` AFTER this callback decremented it.
  ///   - pending: `pendingPreviewDocuments[win]` — what the load was carrying.
  ///   - succeeded: `didFinish` → `true`, either `didFail…` → `false`.
  public static func loadCommitOutcome(
    remaining: Int,
    pending: LiveDocument?,
    succeeded: Bool
  ) -> LoadCommitOutcome {
    if remaining > 0 { return .stillOutstanding }
    return succeeded ? .commit(pending) : .discard
  }

  // MARK: - JS synthesis

  /// A `Double` rendered for interpolation into JS: fixed two decimals, no locale
  /// argument (so non-localized — the separator is always `.`).
  private static func js(_ v: Double) -> String {
    String(format: "%.2f", v)
  }

  /// The document-start user script injected into every preview web view.
  ///
  /// Wrapped entirely in `try { … } catch (e) {}` so a missing message handler can
  /// never throw into the page. It installs on `window`:
  ///
  ///  * `__wolfFollow(line, col)` — record the cursor, resolve its anchor, and apply
  ///    the edge-threshold rule (scrolling only when the anchor left the comfort
  ///    band); an UNRESOLVED anchor never scrolls. Then push state out.
  ///  * `__wolfRestore(line, col, targetY, fallbackY, hasTarget)` — reproduce the
  ///    pre-reload viewport offset exactly, then compose `__wolfFollow` on top. That
  ///    composition is deliberate: the restore satisfies "the edited region stays
  ///    stationary", and the follow is then a no-op unless the user typed past an
  ///    edge, which satisfies "cursor-follow" — one entry point, no ordering hazard.
  ///  * `__wolfUpdate(html, line, col)` — the IN-PLACE body swap that replaces a
  ///    navigation on the typing path. See the ordering contract below.
  ///  * a rAF-throttled, PASSIVE `scroll` listener that keeps the Swift-side cache
  ///    fresh when the user wheel-scrolls independently of the cursor.
  ///
  /// ## `__wolfUpdate`'s ordering contract
  ///
  /// The whole flicker fix IS this ordering, and every step is load-bearing:
  ///
  ///  1. **The anchor is resolved and measured BEFORE the swap**, using the INCOMING
  ///     `(line, col)` rather than the stale `__wolfLine`/`__wolfCol`, so the freshly
  ///     moved cursor is what stays visually stationary. Every hard line emits a
  ///     `wl-{L}-0` marker, so a line the edit just created resolves through the
  ///     existing bounded backward scan to the preceding line. Reading
  ///     `getBoundingClientRect()` here also forces the pending layout, so `beforeTop`
  ///     is exact.
  ///  2. **The parsed children are moved into a fragment with a `while` loop, then
  ///     swapped with ONE `replaceChildren(frag)`.** Deliberately NOT
  ///     `replaceChildren.apply(body, Array.from(…))`: a long README has thousands of
  ///     top-level children, and `Function.prototype.apply` with a very large argument
  ///     list can throw `RangeError` — on exactly the documents that flicker worst.
  ///     `appendChild` into the fragment ADOPTS each node into the live document
  ///     automatically, so no explicit `document.adoptNode` is needed.
  ///  3. **After the swap the anchor is resolved AGAIN and re-anchored to
  ///     `beforeTop`.** `getBoundingClientRect()` on the new element forces the
  ///     post-swap layout (which also clamps `scrollTop` if the document shrank), and
  ///     `scrollAnchorTo(afterEl, beforeTop)` sets `scrollY = anchorDocY_new -
  ///     beforeTop` — pinning the anchor to the SAME viewport offset it had, no matter
  ///     how much content above it grew or shrank. That IS the height correction; no
  ///     separate one is needed. When no anchor resolves on either side, `moveTo`
  ///     restores the raw pre-swap `scrollY`.
  ///  4. **`__wolfFollow` LAST**, exactly mirroring `__wolfRestore`'s restore-then-
  ///     follow composition: the re-anchor satisfies "the edited region stays
  ///     stationary" and the follow is a no-op unless the cursor left the comfort
  ///     band. It also `post()`s, refreshing the Swift-side cache.
  ///
  /// The swap AND the re-anchor happen in ONE JavaScript task, so rendering cannot
  /// interleave and no frame at scrollY 0 can ever be painted. That is a structural
  /// guarantee, not a race won on average.
  ///
  /// ONE DELIBERATE ASYMMETRY BETWEEN THE TWO PATHS: `<script>` elements in the body
  /// do NOT execute on the in-place path. A `DOMParser`-created script element is
  /// flagged "already started" by the HTML spec and does not run when it is adopted
  /// into another document, whereas `loadHTMLString` runs it. The wrapper emits no
  /// scripts of its own, but pulldown-cmark passes raw HTML through, so a source file
  /// containing a literal `<script>` can reach the body — and it will run on a full
  /// load and not on a body swap. Nothing in the rendered document relies on script
  /// execution and the in-place behavior is the SAFER of the two, so this is recorded
  /// rather than fixed: it is a known asymmetry, not a bug to be diagnosed later.
  ///
  /// `__wolfUpdate` deliberately does NOT touch `__wolfUserScrolled`. `__wolfRestore`
  /// clears it only to re-open the gate on ``restoreJS(_:)``'s rAF /
  /// `document.fonts.ready` re-applies; there are NO late re-applies on this path (the
  /// fonts were never unloaded and layout is forced synchronously), so clearing it
  /// would buy nothing and would silently discard a genuine user scroll.
  ///
  /// It is defined AFTER `moveTo`/`__wolfRestore` and BEFORE the scroll listener, and
  /// it reaches the viewport ONLY through `scrollAnchorTo` / `moveTo` /
  /// `__wolfFollow` — it adds no second `scrollTo`, and none of `scrollBy`,
  /// `scrollIntoView` or `scrollTop =`. Keep it that way (see the funnel invariant
  /// below); a direct scroll here would fire an unarmed `scroll` event that the
  /// listener classifies as the USER's.
  ///
  /// Anchor resolution order: the rounded exact-column marker, then the line's
  /// `-0` marker, then a bounded backward scan for the nearest PRECEDING line's
  /// marker, then one document-order sweep of `.wolf-src[data-col="0"]` walked
  /// backwards. All but the last are O(1) id lookups; because every hard line emits
  /// a `-0` marker, the scan normally terminates in a single step.
  ///
  /// That tier-4 sweep's `[data-col="0"]` filter is LOAD-BEARING and must never be
  /// widened to all `.wolf-src`. wolf-core places the per-100-column anchors
  /// INLINE, at their true position inside a block's rendered content, so `col > 0`
  /// anchors are not in globally ascending document order (a later hard line's
  /// `-0` anchor is still flushed at the block top, ahead of an earlier-offset
  /// `col > 0` anchor placed inside that block). Only the `data-col="0"`
  /// SUBSEQUENCE is guaranteed ascending, which is exactly what a backwards sweep
  /// assumes.
  ///
  /// Scrolling is always instantaneous `window.scrollTo(0, y)`. A smooth animation
  /// would still be running when the next keystroke's follow fires, which would make
  /// the restore inexact.
  ///
  /// EVERY programmatic scroll funnels through the single `moveTo` helper, which arms
  /// `window.__wolfProgrammatic` so the scroll listener does not mistake the script's
  /// own scroll for the user's — without that, `__wolfRestore`'s own `scrollTo` would
  /// set `__wolfUserScrolled` and `restoreJS`'s rAF / `document.fonts.ready`
  /// re-applies would ALWAYS be skipped, silently killing the font-settling
  /// mitigation they exist for. The flag is armed ONLY when the requested offset
  /// actually differs from `window.scrollY`: scrolling to where the page already sits
  /// fires no `scroll` event, so an unconditionally armed flag would never be
  /// consumed and would swallow the NEXT genuine user scroll (reachable on any
  /// restore whose computed target is 0). The listener consumes (clears) the flag, so
  /// each scroll is judged on its own merits.
  ///
  /// `moveTo` clamps its argument to `[0, scrollHeight - innerHeight]` BEFORE that
  /// comparison, and the comparison judges the CLAMPED target. WebKit clamps an
  /// out-of-range `scrollTo` itself, so an over-target offset — e.g. the raw
  /// `fallbackY` captured from a longer previous document — can silently become a
  /// no-op that fires no `scroll` event; comparing the raw value would arm the flag
  /// for a scroll that never happens and swallow the next genuine user scroll.
  /// Clamping in `moveTo` covers every programmatic scroll, which is why
  /// `scrollAnchorTo` does not clamp separately.
  ///
  /// The ONE programmatic scroll in this crate that deliberately does NOT go through
  /// `moveTo` is ``PreviewLinkSeam/scrollIntoViewJS(fragmentId:)``'s
  /// `e.scrollIntoView(true)`: it runs in the page but never touches
  /// `__wolfProgrammatic`, so the resulting `scroll` event IS recorded as a user
  /// scroll. That is the DESIRABLE outcome, not an oversight — a fragment jump is an
  /// explicit user navigation, and setting `__wolfUserScrolled` is exactly what stops
  /// a late `restoreJS` rAF / `document.fonts.ready` re-apply from yanking the page
  /// back off the anchor the user just jumped to. (`previewDidFinishLoad` also drops
  /// any pending restore when it takes the fragment branch, so the two mechanisms
  /// agree.) Do not "fix" this by routing `scrollIntoView` through `moveTo`.
  ///
  /// The `.wolf-src` markers are `visibility: hidden` with `font-size: 0`, NOT
  /// `display: none` — that is the load-bearing reason they still generate a box and
  /// `getBoundingClientRect()` returns a real position.
  ///
  /// - Parameter handlerName: The message-handler name to post state to; defaults to
  ///   ``stateHandlerName``.
  public static func stateScriptJS(handlerName: String = stateHandlerName) -> String {
    """
    (function () {
      try {
        var W = window;
        var STRIDE = \(markerColumnStride);
        var SCAN = \(maxAnchorScanLines);
        var MIN_EDGE = \(js(minEdgeThreshold));
        var EDGE_FRAC = \(js(edgeThresholdFraction));
        var EDGE_MAX = \(js(maxEdgeThresholdFraction));
        var REST_FRAC = \(js(restPositionFraction));
        W.__wolfLine = 0;
        W.__wolfCol = 0;
        W.__wolfUserScrolled = false;
        W.__wolfProgrammatic = false;
        var byId = function (id) { return document.getElementById(id); };
        var resolve = function (line, col) {
          if (line < 1) { return null; }
          var c = Math.floor(col / STRIDE) * STRIDE;
          var e = null;
          if (c > 0) {
            e = byId('wl-' + line + '-' + c);
            if (e) { return e; }
          }
          e = byId('wl-' + line + '-0');
          if (e) { return e; }
          for (var l = line - 1, n = 0; l >= 1 && n < SCAN; l--, n++) {
            e = byId('wl-' + l + '-0');
            if (e) { return e; }
          }
          var all = document.querySelectorAll('.wolf-src[data-col="0"]');
          for (var i = all.length - 1; i >= 0; i--) {
            if (+all[i].getAttribute('data-line') <= line) { return all[i]; }
          }
          return null;
        };
        var threshold = function (vh) {
          return Math.min(Math.max(MIN_EDGE, vh * EDGE_FRAC), vh * EDGE_MAX);
        };
        var moveTo = function (y) {
          var doc = document.documentElement;
          var max = Math.max(0, doc.scrollHeight - W.innerHeight);
          y = Math.min(Math.max(0, y), max);
          if (y !== W.scrollY) { W.__wolfProgrammatic = true; }
          W.scrollTo(0, y);
        };
        var scrollAnchorTo = function (el, targetY) {
          moveTo(el.getBoundingClientRect().top + W.scrollY - targetY);
        };
        var post = function () {
          var el = resolve(W.__wolfLine, W.__wolfCol);
          var a = el ? el.getBoundingClientRect().top : null;
          var wk = W.webkit && W.webkit.messageHandlers;
          var h = wk && wk.\(handlerName);
          if (h) { h.postMessage({ a: a, s: W.scrollY }); }
        };
        W.__wolfFollow = function (line, col) {
          W.__wolfLine = line;
          W.__wolfCol = col;
          var el = resolve(line, col);
          if (el) {
            var vh = W.innerHeight;
            var th = threshold(vh);
            var top = el.getBoundingClientRect().top;
            if (top < th || top > vh - th) {
              scrollAnchorTo(el, vh * REST_FRAC);
            }
          }
          post();
        };
        W.__wolfRestore = function (line, col, targetY, fallbackY, hasTarget) {
          W.__wolfUserScrolled = false;
          W.__wolfLine = line;
          W.__wolfCol = col;
          var el = resolve(line, col);
          if (el) {
            if (hasTarget) { scrollAnchorTo(el, targetY); }
          } else {
            moveTo(fallbackY);
          }
          W.__wolfFollow(line, col);
        };
        W.__wolfUpdate = function (html, line, col) {
          if (!document.body) { return 0; }
          var beforeEl = resolve(line, col);
          var beforeTop = beforeEl ? beforeEl.getBoundingClientRect().top : null;
          var beforeY = W.scrollY;
          var parsed = new DOMParser().parseFromString(html, 'text/html');
          if (!parsed || !parsed.body) { return 0; }
          var frag = document.createDocumentFragment();
          while (parsed.body.firstChild) {
            frag.appendChild(parsed.body.firstChild);
          }
          document.body.replaceChildren(frag);
          W.__wolfLine = line;
          W.__wolfCol = col;
          var afterEl = resolve(line, col);
          if (afterEl && beforeTop !== null) {
            scrollAnchorTo(afterEl, beforeTop);
          } else {
            moveTo(beforeY);
          }
          W.__wolfFollow(line, col);
          return 1;
        };
        var pending = false;
        W.addEventListener('scroll', function () {
          if (pending) { return; }
          pending = true;
          W.requestAnimationFrame(function () {
            pending = false;
            if (W.__wolfProgrammatic) {
              W.__wolfProgrammatic = false;
            } else {
              W.__wolfUserScrolled = true;
            }
            post();
          });
        }, { passive: true });
        post();
      } catch (e) {}
    })();
    """
  }

  /// The JS that follows the cursor to `(line, col)`.
  ///
  /// The RAW 0-based CHARACTER column is passed through — the injected script does
  /// the rounding to a marker stride, so the two never disagree. Short-circuits on
  /// `window.__wolfFollow` so evaluating this against a web view whose document-start
  /// script has not run yet is a silent no-op rather than a thrown exception.
  public static func followJS(line: UInt32, col: UInt32) -> String {
    "window.__wolfFollow&&window.__wolfFollow(\(line),\(col));"
  }

  /// The JS evaluated in `didFinish` after a full `loadHTMLString`, putting the
  /// edited region back where it visually was.
  ///
  /// Applies the restore immediately, then re-applies it on the next animation frame
  /// and again once `document.fonts` settles: `didFinish` can fire BEFORE the inlined
  /// base64 `@font-face` faces have settled layout, which would otherwise leave the
  /// restore a few dozen pixels off. Both late re-applies are guarded by
  /// `window.__wolfUserScrolled` (cleared at the start of every `__wolfRestore`, set
  /// by the page's scroll listener on a genuine USER scroll) so they can never yank a
  /// user who scrolled in the interim.
  ///
  /// The call always passes the same five arguments; `hasTarget` (`1`/`0`) is what
  /// distinguishes "restore to this offset" from "no anchor was resolved, fall back
  /// to the raw scrollY", so an absent `targetY` still emits a full call with a `0.00`
  /// placeholder.
  public static func restoreJS(_ request: RestoreRequest) -> String {
    let line = String(request.cursor.line)
    let col = String(request.cursor.col)
    let target = js(request.targetY ?? 0)
    let fallback = js(request.fallbackScrollY)
    let hasTarget = request.targetY == nil ? "0" : "1"
    let call =
      "window.__wolfRestore&&window.__wolfRestore("
      + "\(line),\(col),\(target),\(fallback),\(hasTarget));"
    return "(function(){var f=function(){\(call)};f();"
      + "window.requestAnimationFrame(function(){"
      + "if(!window.__wolfUserScrolled){f();}});"
      + "if(document.fonts&&document.fonts.ready){"
      + "document.fonts.ready.then(function(){"
      + "if(!window.__wolfUserScrolled){f();}});}})();"
  }

  /// The names of the three values ``inPlaceUpdateJS()``'s body resolves against its
  /// enclosing scope — i.e. exactly the keys the caller must build its
  /// `callAsyncJavaScript(_:arguments:in:in:completionHandler:)` dictionary from.
  ///
  /// They live HERE, as one exported constant, for the same reason the tuning numbers
  /// do: the JS body and the caller's dictionary must be provably unable to
  /// drift. An EXTRA free name in the body is a `ReferenceError` on every keystroke —
  /// which degrades the feature to permanent full loads, silently, with a green
  /// suite — and a missing one is a payload shipped and then ignored. Short names
  /// because they are re-lexed as JS source on every call.
  public static let inPlaceUpdateArgumentNames: (html: String, line: String, col: String) = (
    html: "h", line: "l", col: "c"
  )

  /// The `callAsyncJavaScript` FUNCTION BODY that drives one in-place update.
  ///
  /// `callAsyncJavaScript` — not `evaluateJavaScript` — because it takes REAL
  /// arguments: the ~200 KB body string is handed over as a VALUE and is never
  /// escaped into, or re-lexed as, JS source. Hence this string contains no `<` at
  /// all, and hard-codes no part of `render_html`'s wrapper. It also yields a proper
  /// `Result<Any, Error>`, which is what drives the caller's fallback.
  ///
  /// The body short-circuits on `window.__wolfUpdate` being absent — a page that is
  /// not ours, or a WRONG CONTENT WORLD — and returns a FALSY `0` sentinel rather
  /// than throwing. The sentinel is deliberate: a throw would surface as `.failure`
  /// and trigger the same fallback, but it would also log a JS exception on every
  /// call. Returning `undefined` would be indistinguishable from a successful swap
  /// that simply returned nothing.
  ///
  /// Its free identifiers are exactly ``inPlaceUpdateArgumentNames``.
  public static func inPlaceUpdateJS() -> String {
    let names = inPlaceUpdateArgumentNames
    return "if (!window.__wolfUpdate) { return 0; }\n"
      + "return window.__wolfUpdate(\(names.html), \(names.line), \(names.col));"
  }
}
