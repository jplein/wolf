//
//  PreviewScrollSeam.swift
//  Pure, AppKit-free Markdown-preview scroll-sync seam.
//
//  The sibling of `PreviewSeam` / `PreviewLinkSeam`: a pure, display-free decision
//  exercised with scalar inputs only — NO AppKit, NO WKWebView, NO live Neovim. It
//  owns ALL the anchor arithmetic (which `wl-{L}-{C}` marker id a cursor resolves
//  to), the viewport-edge "should we scroll at all" rule, the per-window
//  needs-a-scroll gate, and every JavaScript string the preview web view is ever
//  asked to evaluate for scroll purposes. The AppKit glue
//  (`PaneContainerView` + `PreviewScrollStateHandler`) stays thin: it caches state,
//  captures it before a reload, and evaluates the strings this seam synthesizes.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap` module
//  alongside `PreviewSeam.swift` / `PreviewLinkSeam.swift`.
//
//  ## No JSON escaper here, deliberately
//
//  Unlike `PreviewLinkSeam` — which interpolates a user-controlled fragment string
//  and therefore needs `jsonEscape` — EVERY value this seam interpolates into JS is
//  a validated NUMBER (a `UInt32` line/column or a `Double` coordinate). Anchor ids
//  are assembled INSIDE the JS from those numbers, never in Swift. So no escaper is
//  needed and none should be added.
//
//  `Double`s are rendered with `String(format: "%.2f", v)` — no locale argument, so
//  non-localized, so the decimal separator is always `.` and can never become a `,`
//  that would silently split one JS argument into two. Integers use `String(v)`.
//
//  ## The tuning constants live HERE and ONLY here
//
//  The six tuning numbers below are INTERPOLATED into the generated JS rather than
//  duplicated as literals in the script text. A literal in both places could drift
//  silently: the Swift tests would stay green while the shipped scrolling behaved
//  differently. `PreviewScrollSeamTests` asserts the generated JS actually contains
//  each constant's rendered form, so a constant change provably reaches the page.
//

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
  ///  * a rAF-throttled, PASSIVE `scroll` listener that keeps the Swift-side cache
  ///    fresh when the user wheel-scrolls independently of the cursor.
  ///
  /// Anchor resolution order: the rounded exact-column marker, then the line's
  /// `-0` marker, then a bounded backward scan for the nearest PRECEDING line's
  /// marker, then one document-order sweep of `.wolf-src[data-col="0"]` walked
  /// backwards. All but the last are O(1) id lookups; because every hard line emits
  /// a `-0` marker, the scan normally terminates in a single step.
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
  /// `scrollAnchorTo` no longer clamps separately.
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
}
