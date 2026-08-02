# Current plan: fix preview flicker while typing

## Status

PHASE 2 — architect plan APPROVED and recorded below. Next: test-writer.

## Prior context

This is a FOLLOW-UP defect on the Markdown-preview scroll-sync feature that just landed on this branch (`feat/preview-scroll-handling`, working tree uncommitted, reviewer-approved). The full plan record for that feature is archived at `/private/tmp/claude-501/-Users-jplein-Git-jplein-wolf/093644f5-19c7-4081-93bc-5ef4f26d7da0/scratchpad/scroll-sync-plan-archive.md` — read it for the invariants (D1/D3, DEV-1..DEV-4, H1/H2/L1/C1, F-1/F-2, `captureDecision`'s three rules, the N1 known limitation) before changing any of that machinery.

## Problem statement (user report, verbatim)

> The original bug is fixed, but there's new bad behavior: when typing, there is flickering, as the preview rapidly goes between the document as it is when scrolled to the very top and as it is scrolled to the position it should be at.

## Constraints (must not regress — all of this was just built and reviewed)

- Typing must not jump the preview to the top; the edited region must stay visually stationary.
- Cursor-follow must still fire only at the viewport-edge threshold (no twitch on every keystroke).
- A cursor-only change must still never trigger a full reload — the `PreviewSeam.PreviewLoad == (sourceBufnr, sourceTick)` invariant (D3).
- Cross-file `#fragment` link jumps must still win over any restore.
- **H1 must survive**: a fragment-less cross-file link still opens the new document at its top. **H2 must survive**: a document-start seed still must not clobber an in-flight restore.
- Keep the pure/testable seam split: JS-generating and decision logic in `crates/wolf-macos/Sources/WolfKeyMap/PreviewScrollSeam.swift` (Foundation-only, unit-tested); AppKit glue in `PaneContainerView.swift`.
- **D1**: tuning constants live in exactly ONE place (the Swift constants) and are interpolated into the JS — no duplicated literals.
- Out of scope: the browser preview (`browser_preview.rs`).

## Test requirements (CLAUDE.md)

Real Neovim over mocks. **Every new test file MUST be explicitly wired into its build target or it silently never runs** — `crates/wolf-macos:WolfKeyMapTests`' `srcs` is a hand-maintained list, NOT a glob. Confirm new tests actually RUN in `bazelisk test` output, not merely that the build passes. Push as much decision logic as possible into the pure seam, as was done for `captureDecision`.

---

# Plan: eliminate preview flicker while typing (in-place DOM update)

## 0. The DECISION — **Option A (in-place DOM update), as a hybrid**

**Chosen: Option A**, with the existing `loadHTMLString` + `didFinish` + `restoreJS` machinery retained *unchanged* for the five paths that genuinely need a navigation. **Option B is rejected.** Reasons, on evidence:

### Evidence gathered

**E1 — the document carries ~965 KiB of base64 in its `<head>`.** `crates/wolf-core/src/markdown.rs:132-149` inlines two `@font-face` rules built with `concat!(include_str!(…b64))`. `crates/wolf-core/src/markdown/README.md:71-72` records the raw woff2 sizes: `inter-variable.woff2` = **352,240 B**, `latinmodern-math.woff2` = **387,976 B** → 740,216 B → base64 ≈ **986 KB**. Every `loadHTMLString` therefore ships ~1 MB across the WKWebView IPC boundary and re-parses it as CSS — **at the reload coalescer's ~33 ms cadence, i.e. ~30 MB/s while the user types.** Option B does not touch a byte of this.

**E2 — the `<head>` is INVARIANT for the in-app preview.** `crates/wolf-core/src/nvim.rs:397-405` hardcodes `WidthMode::Full`, and `render_source_html` (`nvim.rs:1379-1425`) hardcodes `PreviewTheme::Default`. `markdown.rs:422-427` assembles the head from compile-time consts only. So for the in-app preview the entire `<head>` — fonts, base.css, math.css, theme.css — is byte-identical on every render, **including across a dark/light flip** (dark/light comes from `prefers-color-scheme` inside the theme CSS, which re-evaluates live; only the code-block *inline* `style="color:#…"` spans in the **body** change). This is the load-bearing invariant that makes a body-only swap correct, and it makes Option A strictly better than the plan file assumed.

**E3 — `.atDocumentEnd` is not guaranteed to precede first paint.** WebKit injects `atDocumentEnd` user scripts at `finishedParsing()` — after the DOM is parsed, before subresources. But WebKit renders **progressively**: for a long document the parser yields and the compositor can commit intermediate frames before parsing completes. The documents that flicker worst (long READMEs with highlighted code and MathML) are exactly the ones where the parser yields. So B's win is timing-dependent, unverifiable in this repo's test posture (`PaneContainerView` is compile+manual-smoke only), and would regress silently on longer documents.

**E4 — B cannot fix the whole report even when the scroll is right.** The user says the preview "rapidly goes between the document as it is when scrolled to the very top and as it is scrolled to the position it should be at." That is a *re-render*, not only a scroll offset. A navigation rebuilds the whole render tree, re-resolves style for every node, re-lays-out the MathML, and re-commits — 30x per second. (There is no *white* teardown frame: `PaneContainerView.swift:318-322` documents that WebKit keeps displaying the previous document until the new one commits, which is why `previewLoadedWins` is deliberately not cleared per reload. So B's "blank frame" risk is *absent* — but the "commits at scrollY 0, then jumps" risk is exactly the reported symptom.)

**E5 — B has an ugly mechanical cost.** `WKUserContentController` has no "remove one user script"; only `removeAllUserScripts()`. Re-registering a per-load `.atDocumentEnd` script means tearing down and re-adding `PreviewSeam.clickFocusJS` and `PreviewScrollSeam.stateScriptJS` on **every keystroke**, with a correctness window in between. That is a worse seam than the one we have.

**E6 — `DOMParser` fidelity is fine.**
- *MathML*: `parseFromString(html, "text/html")` runs the same HTML parser as a document load, including the foreign-content/`<math>` integration-point rules. `MathRenderer` (`markdown.rs:674-680`) emits inline `<math>` markup as `Event::InlineHtml`, so it is ordinary HTML-parser input. OK
- *Highlight spans*: `markdown.rs:82-85` emits `<span style="color:#RRGGBB;…">` — plain inline-styled elements, no CSSOM dependency, no external resources. OK
- *Markers*: `<span class="wolf-src" id="wl-{L}-{C}" data-line data-col>` — plain. `base.css` makes them `visibility:hidden; font-size:0`, **not** `display:none`, so they still generate boxes and `getBoundingClientRect()` works — unchanged by the swap since the CSS is untouched. OK
- *Scripts*: the wrapper emits none. pulldown-cmark passes **raw HTML through** (there is no disable flag in `markdown_options()`, `markdown.rs:286-299`), so a source file containing a literal `<script>` could reach the body — but that is **already** true of the `loadHTMLString` path, so behavior is unchanged either way. Nothing in the rendered HTML *relies* on script execution.

**E7 — `replaceChildren` cost < navigation cost, strictly.** The swap does: parse the body HTML (same parser, but ~1 MB less input than the full document), adopt nodes, one style-resolution + layout + paint. The navigation does all of that **plus** ~1 MB IPC, a ~1 MB CSS re-parse, two `data:` font URL resolutions, a document teardown, a provisional-to-committed navigation dance, and a `didFinish` round-trip. Option A is a strict subset.

**E8 — the decisive structural property.** In Option A the swap **and** the re-anchor happen in **one JavaScript task**. Rendering cannot interleave. So no frame at scrollY 0 can ever be painted. This is a *guarantee*, not a race won on average — which is precisely what B cannot offer.

**E9 — `callAsyncJavaScript` is available and is the right tool.** `WKWebView.callAsyncJavaScript(_:arguments:in:contentWorld:completionHandler:)` is macOS 11.0+; `crates/wolf-macos/BUILD.bazel:160` sets `minimum_os_version = "26.0"`. Versus `evaluateJavaScript` it gives us (a) real **arguments**, so a ~200 KB body string never gets escaped into or re-lexed as JS source, (b) a proper `Result<Any, Error>` we can use to drive the fallback, (c) `await`/Promise semantics. Both are async with main-thread completion; neither blocks.

### Trade-off, stated plainly

| | Option A | Option B |
|---|---|---|
| Eliminates the flash | **Guaranteed** (single task, no intermediate paint) | Probabilistic; fails on long documents |
| Removes the ~1 MB/keystroke CSS+font churn | Yes | No |
| Removes the per-keystroke document commit | Yes | No |
| New machinery | ~1 pure seam function + 1 JS entry point + 3 caller state vars | Per-load `WKUserScript` + `removeAllUserScripts` dance |
| Risk | `DOMParser`/`replaceChildren` fidelity (checked, E6), body-extraction fragility (guarded) | Silent timing regression that no test can catch |
| Existing 52 seam tests | **all 52 stay valid, unchanged** | all 52 stay valid, but `restoreJS`'s late-re-apply tests become dead weight |

### The hybrid boundary

`loadHTMLString` + `didFinish` + `pendingPreviewRestores` + `restoreJS` is **kept intact and still fires** for: (1) first bind, (2) a source-buffer switch, (3) a pending cross-file `#fragment`, (4) a load already in flight, (5) HTML whose `<body>` wrapper cannot be located, and (6) recovery after an in-place failure. All of `captureDecision`'s three rules still apply on every one of those. The dark/light appearance flip **moves onto the in-place path** (correct per E2, and an improvement — no flash there either).

---

## 1. Summary

Stop navigating the preview `WKWebView` on the typing path. Add a pure `PreviewScrollSeam.updateStrategy(…)` that decides, per preview window, between the existing full `loadHTMLString` and a new **in-place** update; add a pure `PreviewScrollSeam.splitDocument(_:)` that extracts the `<body>` inner HTML and the head's length from `render_html`'s fixed wrapper; add a `__wolfUpdate(html, line, col)` entry point to the already-injected `stateScriptJS` that reads the cursor anchor's viewport offset, `DOMParser`-parses the new body, `replaceChildren`s it in, re-anchors to the recorded offset, and composes `__wolfFollow` on top — all in one task, so no frame at scrollY 0 is ever painted. `PaneContainerView.reloadDirtyPreviews` gains a mechanical switch over the strategy and ships the body via `callAsyncJavaScript` with the HTML as a real argument. The capture/restore machinery is untouched and remains live for the five paths that still navigate.

---

## 2. Files to modify

### 2.1 `crates/wolf-macos/Sources/WolfKeyMap/PreviewScrollSeam.swift`

**Everything existing stays.** Additions only.

**New type — `DocumentSplit`**
```
public struct DocumentSplit: Equatable {
  public let headLength: Int   // UTF-8 count of everything up to and INCLUDING the `<body …>` tag
  public let body: String      // the body element's inner HTML
  public init(headLength: Int, body: String)
}
```

**New pure function — `splitDocument(_ html: String) -> DocumentSplit?`**

Contract (document it against `markdown.rs:422-427`'s fixed wrapper `<!doctype html><html><head>…</head><body class="wolf-markdown">{body}</body></html>`):
- Find the **first** `"<body"`; from there find the **first** `">"`. Everything up to and including that `>` is the head; its UTF-8 count is `headLength`.
- Find the **last** `"</body>"` at or after that point. The substring between is `body`.
- Return `nil` when either marker is missing, or when the closing tag precedes the opening one, or on an empty `html` (which is what `WolfCore::render_preview` returns on RPC failure, `api.rs:540-549`).
- **Why first-open / last-close:** raw HTML passthrough is enabled (E6), so a source file could contain a literal `</body>`; the wrapper's is always last. `<body` inside content always comes *after* the wrapper's. Note this reasoning in the doc comment.
- Do the scan on `html` directly with `range(of:)` / `range(of:options:.backwards)`; do **not** regex, and do **not** copy the head.

**New type — `LiveDocument`**
```
public struct LiveDocument: Equatable {
  public let sourceBufnr: Int64
  public let headLength: Int
  public init(sourceBufnr: Int64, headLength: Int)
}
```
"What is currently committed in a preview web view." Deliberately separate from `PreviewSeam.PreviewLoad` (which is what was last *dispatched*, and must stay exactly `(sourceBufnr, sourceTick)` per D3).

**New type — `UpdateStrategy`**
```
public enum UpdateStrategy: Equatable {
  case fullLoad                  // loadHTMLString; didFinish drives capture/restore
  case inPlace(body: String)     // callAsyncJavaScript(__wolfUpdate); NO navigation
}
```

**New pure function — the strategy decision**
```
public static func updateStrategy(
  html: String,
  currentLoad: PreviewSeam.PreviewLoad,
  liveDocument: LiveDocument?,     // previewLiveDocuments[win]
  loadInFlight: Bool,              // previewLoadsInFlight.contains(win)
  hasPendingFragment: Bool         // pendingPreviewFragments[win] != nil
) -> UpdateStrategy
```
Rules, **ordered**, each with a doc-comment justification:
1. `.fullLoad` when `hasPendingFragment` — the fragment is delivered from `previewDidFinishLoad`, which an in-place update never fires. Without this the `#anchor` would be stranded forever.
2. `.fullLoad` when `loadInFlight` — mutating the outgoing document is pointless (the in-flight navigation will replace it) and would race that load's `didFinish` restore. **This rule is also what guarantees `pendingPreviewRestores[win] == nil` on every in-place path**, since a pending restore exists only between `loadHTMLString` and `didFinish`.
3. `.fullLoad` when `liveDocument == nil` — nothing is committed, so `window.__wolfUpdate` may not exist. Covers first bind and post-failure recovery.
4. `.fullLoad` when `liveDocument!.sourceBufnr != currentLoad.sourceBufnr` — a NAVIGATION. The new file must open at its top (this is H1's guarantee, mirrored here at the strategy layer).
5. `.fullLoad` when `splitDocument(html) == nil` — an empty/unexpected document.
6. `.fullLoad` when `liveDocument!.headLength != split.headLength` — the head changed, so a body-only swap would leave stale CSS. Today unreachable (E2), but it makes the invariant **self-enforcing** rather than comment-only, which is the class of latent bug that produced H1/H2.
7. Otherwise `.inPlace(body: split.body)`.

**Relationship to `captureDecision` — resolved (design question 1).** `captureDecision` is **unchanged**, keeps all three rules, and is called **only on the `.fullLoad` path**, exactly where it is called today. It gets **no new case**. The two functions answer different questions at different times: `captureDecision` keys on `lastPreviewLoads[win]` ("what did we last *dispatch*"), `updateStrategy` keys on `previewLiveDocuments[win]` ("what is *committed*"). Rule 4 and `captureDecision`'s rule 1 are near-duplicates by design and always agree on a real source switch; both are required because one runs before the navigation exists and the other before the restore is consumed. Document this pairing in both doc comments.

**New JS entry point inside `stateScriptJS`** — added between `__wolfRestore` and the scroll listener, so it is defined at document start with the rest:
```
W.__wolfUpdate = function (html, line, col) {
  if (!document.body) { return 0; }
  var beforeEl = resolve(line, col);
  var beforeTop = beforeEl ? beforeEl.getBoundingClientRect().top : null;
  var beforeY = W.scrollY;
  var doc = new DOMParser().parseFromString(html, 'text/html');
  if (!doc || !doc.body) { return 0; }
  var frag = document.createDocumentFragment();
  while (doc.body.firstChild) { frag.appendChild(doc.body.firstChild); }
  document.body.replaceChildren(frag);
  W.__wolfLine = line;
  W.__wolfCol = col;
  var afterEl = resolve(line, col);
  if (afterEl && beforeTop !== null) { scrollAnchorTo(afterEl, beforeTop); }
  else { moveTo(beforeY); }
  W.__wolfFollow(line, col);
  return 1;
};
```
Exact ordering requirements (design question 2), to be stated in the doc comment:
- **Anchor is read BEFORE the swap**, using the *incoming* `(line, col)` — not the stale `__wolfLine/__wolfCol` — so the freshly moved cursor is what stays stationary. Every hard line emits a `wl-{L}-0` marker, so a line created by the edit resolves via the existing bounded backward scan to the preceding line. Reading `getBoundingClientRect()` here forces the pending layout, so `beforeTop` is exact.
- **Move the fragment, then one `replaceChildren(frag)`.** Do **not** use `replaceChildren.apply(body, Array.from(doc.body.childNodes))`: a large document can have thousands of top-level children, and `Function.prototype.apply` with a very large argument list can throw `RangeError`. `appendChild` into the fragment adopts each node into the live document automatically ("replace all" also adopts), so no explicit `document.adoptNode` is needed.
- **After the swap, `resolve` again and re-anchor.** `getBoundingClientRect()` on the new element forces the post-swap layout, which also clamps `scrollTop` if the document shrank. `scrollAnchorTo` then `moveTo` clamps to `[0, scrollHeight - innerHeight]` and arms `__wolfProgrammatic` only on an actual change (F-1/L1 semantics, unchanged).
- **Does the scroll need correcting when content above the cursor changes height?** Yes, and that is precisely what steps 1/4 do: `scrollAnchorTo(afterEl, beforeTop)` sets `scrollY = anchorDocY_new - beforeTop`, so the anchor lands at the *same viewport offset* it had, regardless of how much the content above it grew or shrank. No separate correction is needed.
- **`__wolfFollow` last**, exactly mirroring `__wolfRestore`'s restore-then-follow composition: the re-anchor satisfies "the edited region stays stationary", the follow is a no-op unless the cursor left the comfort band. It also `post()`s, refreshing the Swift cache.
- **Do NOT touch `__wolfUserScrolled`.** `__wolfRestore` clears it only to gate `restoreJS`'s rAF/`fonts.ready` re-applies. There are **no late re-applies on this path** — the fonts were never unloaded and layout is forced synchronously — so clearing it would silently discard a genuine user scroll. Say this in the comment.
- **No new `scrollTo(`.** The N2 test (`testStateScriptJSFunnelsEveryScrollThroughMoveTo`) asserts exactly one `scrollTo(` in the whole script and bans the substrings `.scroll(`, `scrollBy(`, `scrollIntoView`, `scrollTop=`. `__wolfUpdate` reaches the viewport only through `scrollAnchorTo`/`moveTo`/`__wolfFollow`, so it adds none of them. **The coder must keep it that way**; this is a hard constraint, not a style note.
- Watch the ~95-column limit inside the `"""` literal (swift-format `lineLength: 100`).

**New pure function — the `callAsyncJavaScript` body**
```
public static let inPlaceUpdateArgumentNames = (html: "h", line: "l", col: "c")   // or three lets
public static func inPlaceUpdateJS() -> String
```
Returns the async-function body, e.g. `"if (!window.__wolfUpdate) { return 0; } return window.__wolfUpdate(h, l, c);"`, whose free identifiers are exactly the `arguments` dictionary keys. Keeping the argument names in the seam (as constants) is what lets a test prove the body and the caller's dictionary cannot drift. Returning `0` when the entry point is absent is the "the page is not ours / wrong content world" sentinel.

### 2.2 `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`

**New stored state** (beside the existing preview dictionaries, ~`:253-341`), each with the house-style doc comment:
- `private var previewLoadsInFlight: Set<Int64> = []` — a `loadHTMLString` was issued and `didFinish` has not arrived. Feeds `updateStrategy`'s rule 2.
- `private var pendingPreviewDocuments: [Int64: PreviewScrollSeam.LiveDocument] = [:]` — what the in-flight load *will* commit; absent when the HTML could not be split.
- `private var previewLiveDocuments: [Int64: PreviewScrollSeam.LiveDocument] = [:]` — what is committed **now**. The sole input to `updateStrategy` rules 3/4/6. Written only in `previewDidFinishLoad`.

**`reloadDirtyPreviews()` (`:1340`)** — inside the `for win in needing` loop, after `let html = handle.render_preview(load.sourceBufnr).toString()` (`:1371`) and after `previousLoad` is read (`:1366`, the ordering hazard comment stays):

Add a switch on `PreviewScrollSeam.updateStrategy(html: html, currentLoad: load, liveDocument: previewLiveDocuments[win], loadInFlight: previewLoadsInFlight.contains(win), hasPendingFragment: pendingPreviewFragments[win] != nil)`:

- **`.inPlace(let body)`**
  - `let cursor = currentCursors[win] ?? CursorPos(line: 0, col: 0)`
  - `lastPreviewCursors[win] = cursor` — the in-place JS ends with `__wolfFollow`, so the follow *has* been applied; recording it here keeps `followDirtyPreviewCursors` from immediately re-issuing a redundant `followJS`. Exactly mirrors `previewDidFinishLoad`'s restore branch (`:1657`).
  - `web.callAsyncJavaScript(PreviewScrollSeam.inPlaceUpdateJS(), arguments: [<html key>: body, <line key>: Int(cursor.line), <col key>: Int(cursor.col)], in: nil, in: .page) { [weak self] result in … }`
    - **`contentWorld` MUST be `.page`.** User scripts added via `addUserScript` without a world land in `.page`; a `.defaultClient` world would see a *different* `window`, every call would return the `0` sentinel, and the feature would silently degrade to permanent full reloads. Comment this at the call site — it is the highest-value comment in the change.
  - Completion handler: on `.failure`, or on `.success` whose value bridges to `0`/`false`/nil, call a new `private func previewInPlaceUpdateFailed(win:)`.
  - Do **not** call `captureDecision`, do **not** touch `pendingPreviewRestores`, `previewLoadsInFlight`, `pendingPreviewDocuments`, `previewLiveDocuments`, or `previewScrollStates`.
  - Keep a `FrameMetrics` sample around the call (a new bucket, or reuse `.previewLoadHTML`; prefer a new `.previewInPlace` case if `FrameStats.swift` allows it cheaply — otherwise reuse and note it).

- **`.fullLoad`** — the *existing* body, verbatim, plus three lines before `web.loadHTMLString`:
  - the existing `switch PreviewScrollSeam.captureDecision(…)` block (`:1393-1419`) **unchanged, all three cases**;
  - `previewLoadsInFlight.insert(win)`;
  - `pendingPreviewDocuments[win] = PreviewScrollSeam.splitDocument(html).map { PreviewScrollSeam.LiveDocument(sourceBufnr: load.sourceBufnr, headLength: $0.headLength) }` (assigning `nil` clears — an unsplittable document must never become a live document).
  - Then `web.loadHTMLString(html, baseURL: base)` as today.
  - In the `.drop` case, additionally `previewLiveDocuments[win] = nil` alongside the existing three clears — everything that branch touches is stale by definition (the F-2 reasoning).

**New `private func previewInPlaceUpdateFailed(win: Int64)`** — the non-looping fallback (design question 5):
```
previewLiveDocuments[win] = nil     // forces updateStrategy rule 3 -> .fullLoad
lastPreviewLoads[win] = nil         // forces winsNeedingReload to return this win again
previewReloadCoalescer.request()
```
**Termination argument (must be in the doc comment):** clearing `lastPreviewLoads[win]` is what re-arms `winsNeedingReload`; clearing `previewLiveDocuments[win]` is what forces that re-armed pass onto the full path. The full path never calls this function, so at most **one** extra full reload per failure. A *persistently* failing in-place update degrades to one full load per tick — i.e. exactly today's behavior — and recovers automatically the moment a load commits. Do **not** add a "stop trying" latch; automatic recovery is worth the one wasted async call.

**`previewDidFinishLoad(win:)` (`:1642`)** — the **three-branch precedence is unchanged** (design question 3). Insert two statements immediately after the existing `previewLoadedWins.insert(win)` and **before** the fragment branch, so every early-returning branch still updates them:
```
previewLoadsInFlight.remove(win)
previewLiveDocuments[win] = pendingPreviewDocuments.removeValue(forKey: win)
```
Update the doc comment to say this function is now reached only on the five navigating paths, and that `pendingPreviewRestores` / `restoreJS` remain load-bearing for exactly those.

**`removePreviewWebView(_:)` (`:1532`)** — add `previewLoadsInFlight.remove(win)`, `pendingPreviewDocuments[win] = nil`, `previewLiveDocuments[win] = nil` to the existing clear list.

**`viewDidChangeEffectiveAppearance()` (`:1326`)** — code unchanged (`lastPreviewLoads.removeAll()` + request). **Doc comment must change**: it no longer merely "re-renders in place rather than snapping to the top" via capture/restore — because it clears `lastPreviewLoads` but **not** `previewLiveDocuments`, the flip now takes the genuine in-place path and never navigates at all. Record why that is correct: the `<head>` is byte-identical across a flip (E2) and only the body's inline highlight colors change.

**`syncPreviewWebViews()` (`:1232`)** — no change. The CRITICAL INVARIANT comments at `:1260-1265` and `:1348-1349` stay.

**`ensurePreviewWebView(for:)` (`:1428`)** — no change (the same `stateScriptJS` now also defines `__wolfUpdate`).

**`followDirtyPreviewCursors()` (`:1509`)** — no change. Its `previewLoadedWins.contains(win)` gate is still correct.

### 2.3 `crates/wolf-macos/Sources/WolfMacOS/PreviewNavigationDelegate.swift`

Add the two failure callbacks, each forwarding to a new `container?.previewDidFailLoad(win: win)`:
- `webView(_:didFail:withError:)`
- `webView(_:didFailProvisionalNavigation:withError:)`

And in `PaneContainerView`, `func previewDidFailLoad(win: Int64)` performs `previewLoadsInFlight.remove(win); pendingPreviewDocuments[win] = nil`. Without this, a failed load would leave `loadInFlight` stuck true and pin that window on the full path forever — a safe degradation, but a silent one. Cheap to close.

### 2.4 Rust / Lua / FFI — **no code changes** (design question 7)

Explicitly: `wolf-core`, `wolf-ffi`, `runtime/nvim/init.lua` and the generated bindings are **untouched**. `render_preview` keeps returning a whole document; the Swift seam splits it. No regen, `wolf_ffi_wake.h` untouched, no ABI surface change.

Two Rust-side **comment** updates are required (the C2 precedent: stale in-source comments that contradict the docs are bugs):
- `crates/wolf-core/src/markdown.rs:67-73` — step 4 of the "who consumes the markers" chain still describes only "captures the anchor's viewport offset before a reload and restores it afterward". Add the in-place path, and add a sentence recording that **the `<head>` is invariant for the in-app preview**, which is what makes a body-only swap sound — so a future themes/width feature knows to revisit.
- `crates/wolf-core/src/nvim.rs:397-405` — `render_preview`'s doc comment: same invariant note (fixed `PreviewTheme::Default` + `WidthMode::Full` implies a constant `<style>` block; the macOS front-end swaps only the body).

---

## 3. Design questions — explicit resolutions

1. **New seam API / composition with `captureDecision`.** `splitDocument(_:) -> DocumentSplit?`, `updateStrategy(html:currentLoad:liveDocument:loadInFlight:hasPendingFragment:) -> UpdateStrategy`, plus `LiveDocument`, `DocumentSplit`, `UpdateStrategy`, `inPlaceUpdateJS()` and the argument-name constants. `captureDecision` is **unchanged**, gets **no new case**, and applies on the `.fullLoad` path **only** — which is exactly where it applies today. All three of its rules still hold there.

2. **The swap JS and its ordering.** In `stateScriptJS` (so it exists at document start), plus a three-line `callAsyncJavaScript` body from `inPlaceUpdateJS()`. Order after `replaceChildren`: set `__wolfLine`/`__wolfCol`, then `resolve(line, col)`, then `scrollAnchorTo(el, beforeTop)` (or `moveTo(beforeY)` when no anchor resolved either side), then `__wolfFollow(line, col)`. The scroll **does** need correcting when content above the cursor changes height, and `scrollAnchorTo(el, beforeTop)` is exactly that correction — it pins the anchor to its previous viewport offset by construction. `__wolfUserScrolled` is deliberately not cleared. The scroll cache and `window` state survive; only `__wolfLine`/`__wolfCol` are re-stamped.

3. **`pendingPreviewRestores` / `restoreJS` / the three-branch precedence.** All retained, unchanged, and still exercised — by first bind, source switch, pending fragment, straddling loads, unsplittable HTML and failure recovery. They are **never** consulted on the in-place path, and `updateStrategy` rule 2 (`loadInFlight`) makes it structurally impossible for a pending restore to coexist with an in-place update on the same window. The three branches in `previewDidFinishLoad` keep their exact precedence; only two bookkeeping lines are inserted ahead of them.

4. **State maintenance with no navigation.**
   - `previewLoadedWins` — untouched (it only ever grows per window and is cleared on removal; the occlusion test keeps working because the document is never blank).
   - `lastPreviewLoads[win]` — set eagerly to `load` before the strategy switch, exactly as today (`:1367`). It means "this tick has been handled", regardless of how.
   - `lastPreviewCursors[win]` — set synchronously at in-place issue time, because `__wolfUpdate` ends with `__wolfFollow`. Prevents a redundant follow from `previewScrollCoalescer`.
   - `previewScrollStates[win]` — untouched by Swift; refreshed by the page's `post()` at the end of `__wolfFollow`. Momentarily stale between the swap and that async post (<= 1 frame), which only a *subsequent* full-load `captureDecision` could read — the same bounded staleness the design already accepts.
   - `previewLiveDocuments[win]` — unchanged by an in-place update (same bufnr, same head length by rule 6).

5. **Failure modes.**

   | Failure | Handling |
   |---|---|
   | `callAsyncJavaScript` throws (JS exception, invalid frame, web-process crash) | `.failure` triggers `previewInPlaceUpdateFailed(win:)` then one full reload, no loop |
   | `__wolfUpdate` undefined (wrong content world, script never injected) | body returns `0`, same fallback; recovers on the next commit |
   | Page has not finished its first load | `previewLiveDocuments[win] == nil`, rule 3, `.fullLoad`. Never attempted. |
   | Swap races a genuine navigation | rule 2 (`loadInFlight`) forbids it. `previewDidFailLoad` keeps the flag from sticking. |
   | Head changed (future theme/width feature) | rule 6, `.fullLoad` |
   | `render_preview` returned `""` (RPC failure) | `splitDocument` returns `nil`, rule 5, `.fullLoad` (today's behavior) |
   | Document shrank past the cursor's line | `resolve`'s existing chain: exact-col, line-0, bounded backward scan, document sweep, `moveTo(beforeY)` |
   | Late completion for an already-removed window | writes dead dictionary entries; the next `syncPreviewWebViews` prune drops them. Benign, same as `recordPreviewScrollState`. |

6. **`#fragment`, `.drop`, appearance flip, H1/H2.**
   - Same-file `#fragment` (`scrollPreview(win:toFragment:)`) never reloads — untouched.
   - Cross-file `other.md#anchor` — `pendingPreviewFragments[win]` is set, rule 1 forces `.fullLoad`, `didFinish` fires, the fragment branch still wins and still drops the pending restore. OK
   - Fragment-less cross-file link — the bufnr changes, so rule 4 forces `.fullLoad` *and* `captureDecision` rule 1 returns `.drop`. **H1 survives, doubly.** OK
   - **H2 survives** unchanged: `captureDecision` rule 2 (including the `cachedState == nil` half from the clarification) is untouched, and it now runs far less often because the straddling-reload window largely disappears. Do **not** weaken it.
   - Appearance flip — now takes the in-place path and is *better* (no flash). Correct because of E2. Doc comment updated.

7. **Rust/Lua/FFI.** No code changes at all; two doc-comment corrections only. Confirmed above.

---

## 4. Test plan

### 4.1 Existing `PreviewScrollSeamTests.swift` — **all 52 methods stay valid; 0 change semantically**

Checked each. **51 of 52 require no edit.** Specifically:
- The 6 tuning/anchor/threshold groups (`testTuningConstants…` through `testFollowDecisionJustAboveTheTopThresholdScrolls`, 17 methods) — untouched surface.
- The 7 `winsNeedingScroll` methods + `testCursorOnlyChangeDoesNotArmReload` — untouched surface; the D3 invariant is if anything strengthened.
- **All 12 `captureDecision` methods stay valid verbatim**, because `captureDecision` is unchanged and still runs on every full load. This is deliberate: the fix must not silently retire H1/H2's regression guards.
- `testStateHandlerNameIsDistinct`, `testFollowJS*` (2), `testRestoreJS*` (3), `testInterpolatedDoublesAreNotLocalized` — untouched surface. `restoreJS`'s late-re-apply test stays meaningful because the full path still exists.
- `testStateScriptJSShape`, `testStateScriptJSNeverUsesSmoothScrolling`, `testStateScriptJSCarriesTheSwiftTuningConstants`, `testStateScriptJSArmsTheProgrammaticFlagBeforeScrolling`, `testStateScriptJSDoesNotTreatAProgrammaticScrollAsAUserScroll`, `testStateScriptJSArmsTheProgrammaticFlagOnlyOnAnActualOffsetChange`, `testStateScriptJSClampsAProgrammaticScrollBeforeArming` — all still pass **provided** `__wolfUpdate` adds no second `scrollTo` and no second `__wolfProgrammatic = true`, which the design guarantees. **These are the constraints the coder must respect, not tests to relax.**

**`testStateScriptJSFunnelsEveryScrollThroughMoveTo`**: its bypass sweep asserts the script contains none of `.scroll(`, `scrollBy(`, `scrollIntoView`, `scrollTop=`. `__wolfUpdate` introduces none of those. Its "the sole `scrollTo(` sits inside `moveTo`" check works by asserting no `function` token appears between `moveTo=function` and the `scrollTo(`. Since `__wolfUpdate` is defined **after** `moveTo` in the script, that check is unaffected. **Verdict: stays valid unchanged.** The required change is **additive**: extend its doc comment to record that `__wolfUpdate` is a new caller that must reach the viewport only through `scrollAnchorTo`/`moveTo`/`__wolfFollow`, so a future reader does not "optimize" a direct scroll into it. If the coder finds the ordering assertion does break in practice, the fix is to define `__wolfUpdate` after `moveTo` — never to loosen the assertion.

So: **0 methods deleted, 0 assertions weakened, 1 comment extended.**

### 4.2 New tests — same file, `PreviewScrollSeamTests.swift`

Add to the file header's CONTRACT UNDER TEST block: `DocumentSplit`, `LiveDocument`, `UpdateStrategy`, `splitDocument`, `updateStrategy`, `inPlaceUpdateJS`.

**`splitDocument` (7 methods)**
- `testSplitDocumentExtractsTheBodyOfTheRealWrapper` — feed the exact `<!doctype html><html><head><meta charset="utf-8"><style>CSS</style></head><body class="wolf-markdown">BODY</body></html>` shape from `markdown.rs:422-427`; assert `body == "BODY"` and `headLength` equals the UTF-8 count of the prefix through `<body class="wolf-markdown">`.
- `testSplitDocumentReturnsNilForEmptyHTML` — `""` (what a failed `render_preview` yields).
- `testSplitDocumentReturnsNilWithoutABodyTag`.
- `testSplitDocumentReturnsNilWithoutAClosingBodyTag`.
- `testSplitDocumentUsesTheLastClosingBodyTag` — body content containing a literal `</body>` (reachable via raw-HTML passthrough); assert the inner content keeps its embedded `</body>`.
- `testSplitDocumentPreservesMathMLAndHighlightSpans` — a body containing `<math>…</math>` and `<span style="color:#ff0000">` survives byte-for-byte (the seam must not sanitize).
- `testSplitDocumentHeadLengthChangesWithTheHeadContent` — two documents with different `<style>` lengths yield different `headLength` (this is what powers rule 6).

**`updateStrategy` truth table (11 methods)** — one per rule plus the boundaries:
- `…FullLoadWhenAFragmentIsPending`
- `…FullLoadWhenALoadIsInFlight` (and that this holds even for an otherwise perfect same-buffer edit — the anti-race rule)
- `…FullLoadOnFirstBind` (`liveDocument == nil`)
- `…FullLoadWhenTheSourceBufferSwitched` (H1's strategy-layer mirror; assert it fires even at an equal tick)
- `…FullLoadWhenTheHTMLCannotBeSplit`
- `…FullLoadWhenTheHeadLengthChanged`
- `…InPlaceForAnOrdinaryEdit` (same bufnr, bumped tick, nothing in flight) — assert the carried `body`
- `…InPlaceIgnoresTheTick` (bufnr is what matters; two ticks apart still in-place)
- `…RuleOrderFragmentBeatsAValidInPlace`
- `…RuleOrderSourceSwitchBeatsAMatchingHeadLength`
- `…InPlaceCarriesTheExactBodyNotTheWholeDocument` — assert the returned body does **not** contain `<style` or `@font-face` (the E1 payload-reduction property, pinned).

**Composition (2 methods)** — the pairing that must never drift:
- `testASourceSwitchIsBothAFullLoadAndADrop` — the same `(previousLoad/liveDocument, currentLoad)` inputs give `updateStrategy == .fullLoad` **and** `captureDecision == .drop`. Pins that the two gates agree.
- `testAnOrdinaryEditIsInPlaceAndNeverConsultsCaptureDecision` — documents (in a comment, asserted structurally by the `.inPlace` result) that `captureDecision` is a full-load-only concern.

**`inPlaceUpdateJS` (3 methods)**
- `testInPlaceUpdateJSGuardsOnTheEntryPointExisting` — contains `__wolfUpdate` and a `!`/`&&` guard, and returns a falsy sentinel when absent.
- `testInPlaceUpdateJSUsesExactlyTheDeclaredArgumentNames` — every free identifier in the body is one of the three exported argument-name constants. This is the anti-drift pin between the JS body and the `arguments:` dictionary.
- `testInPlaceUpdateJSInterpolatesNoDocument` — the returned string contains no `<` at all, proving the HTML travels as an argument and never as interpolated source (the whole point of `callAsyncJavaScript`).

**`stateScriptJS` gains 4 methods**
- `testStateScriptJSDefinesTheInPlaceUpdateEntryPoint` — contains `__wolfUpdate`, `DOMParser`, `replaceChildren`, `createDocumentFragment`.
- `testStateScriptJSInPlaceUpdateDoesNotUseApplyForTheSwap` — assert the script does **not** contain `replaceChildren.apply` (the `RangeError` hazard) and does contain the fragment loop.
- `testStateScriptJSInPlaceUpdateReadsTheAnchorBeforeTheSwap` — within `__wolfUpdate`'s body, the first `getBoundingClientRect` precedes `replaceChildren`, and a second `resolve(` follows it. Pins the load-bearing ordering.
- `testStateScriptJSInPlaceUpdateDoesNotClearTheUserScrollFlag` — `__wolfUserScrolled = false` appears exactly once in the whole script (inside `__wolfRestore`), so `__wolfUpdate` provably does not clear it.

**Edge cases explicitly covered:** empty HTML; a body containing a literal `</body>`; a body with MathML and inline-styled spans; a head-length change; an equal-tick source switch; a fragment pending together with an otherwise valid in-place candidate; a load in flight together with an otherwise valid in-place candidate.

**Not automatable** (this crate's stated posture — `PaneContainerView` is compile + manual-smoke): the actual absence of flicker, the `callAsyncJavaScript` content world, the failure fallback loop, and `previewDidFailLoad`. Those get manual checklist steps and are minimized by pushing every decision into the seam.

### 4.3 BUILD wiring

- `crates/wolf-macos/BUILD.bazel`, `swift_library(name = "wolf_key_map")` `srcs` — **no change**. `Sources/WolfKeyMap/PreviewScrollSeam.swift` is already listed at line **77**; no new source file is created.
- `crates/wolf-macos/BUILD.bazel`, `swift_test(name = "WolfKeyMapTests")` `srcs` — **no change**. `Tests/WolfKeyMapTests/PreviewScrollSeamTests.swift` is already listed at line **275**. All new tests go into that existing, already-wired file. **XCTest auto-discovers `test…` methods inside a listed file, so the new methods will run.**
- `Sources/WolfMacOS/*.swift` is a glob (line **101**), so the `PreviewNavigationDelegate.swift` edit needs no BUILD change.
- **Verification is still mandatory:** run `bazelisk test //crates/wolf-macos:WolfKeyMapTests` and confirm the total count rises from its current baseline by the number of added methods **and** that the new method names appear by name in the output. A green build is not evidence.

---

## 5. Documentation updates

| File | Change |
|---|---|
| `architecture.md` | The Markdown-preview scroll-sync section: describe the two-path model — in-place `__wolfUpdate` for the typing/appearance path, `loadHTMLString` + capture/restore for first bind, source switch, pending fragment, in-flight load, unsplittable HTML and failure recovery. Record the head-is-invariant premise and the ~965 KiB base64 payload that motivates it. |
| `crates/wolf-macos/README.md` | Update the scroll-sync description with the in-place path. Extend the manual eyeball checklist: **new step — "type continuously for several seconds: the preview must update without any flicker, and must never flash to the top"**; amend the existing dark/light step to "re-renders in place with no navigation at all"; add a step for a fragment-less cross-file link (opens the new file at its top) and one for a cross-file `#anchor` link (still wins). |
| `crates/wolf-macos/Sources/README.md` | Extend the `PreviewScrollSeam.swift` bullet with `splitDocument`, `updateStrategy`, `LiveDocument`, `DocumentSplit`, `UpdateStrategy`, `inPlaceUpdateJS`, `__wolfUpdate`; extend the `PreviewNavigationDelegate.swift` bullet with the two failure callbacks. |
| `crates/wolf-macos/Tests/README.md` | Extend the `PreviewScrollSeamTests` entry with the new groups. |
| `crates/wolf-core/src/markdown.rs` (comments) | Module docs step 4: add the in-place path and the head-invariant note. |
| `crates/wolf-core/src/nvim.rs` (comment) | `render_preview` doc comment: record that the fixed theme + `WidthMode::Full` make the `<style>` block constant, which the macOS front-end relies on to swap only the body. |
| `crates/wolf-core/src/markdown/README.md` | Add one sentence to the fonts section: the ~965 KiB inlined base64 is why the front-end swaps only the body rather than re-loading the document per keystroke. |

Reformat every edited `.md` with `prettier --write`; no hard line breaks inside a paragraph.

---

## 6. Implementation order

1. **`PreviewScrollSeam.swift` — types + `splitDocument` + `updateStrategy`.** Pure Foundation; compiles alone; `captureDecision` untouched.
2. **`PreviewScrollSeam.swift` — `__wolfUpdate` inside `stateScriptJS`, plus `inPlaceUpdateJS()` and the argument-name constants.** Still no caller. **Run `bazelisk test //crates/wolf-macos:WolfKeyMapTests` now** — all 52 existing methods must still be green *before* any caller exists; that isolates any `stateScriptJS` structural-assertion breakage to this step.
3. **New tests in `PreviewScrollSeamTests.swift`** (+ the one comment extension). Run and confirm the new method names appear by name in the output.
4. **`PaneContainerView.swift` — the three new state vars + `previewDidFinishLoad` bookkeeping + `removePreviewWebView` clears + `previewDidFailLoad`.** Compiles against step 1; no behavior change yet (nothing reads `previewLiveDocuments`).
5. **`PreviewNavigationDelegate.swift` — the two failure callbacks.** Depends on step 4's `previewDidFailLoad`.
6. **`PaneContainerView.reloadDirtyPreviews` — the strategy switch, the `callAsyncJavaScript` call, `previewInPlaceUpdateFailed`.** This is the behavior change; everything it depends on already exists.
7. **`viewDidChangeEffectiveAppearance` doc comment** + the `.drop` case's extra `previewLiveDocuments[win] = nil`.
8. **Build + sweep**: `bazelisk build //crates/wolf-macos:Wolf`, then `bazelisk test //crates/wolf-core/... //crates/wolf-ffi/... //crates/wolf-macos:WolfKeyMapTests`. Rust/FFI are untouched but must stay green.
9. **Manual smoke** against the extended checklist — especially "type continuously, no flicker" and the `.page` content-world sanity check (if the world were wrong, *every* update would silently fall back to a full load and the flicker would remain, with a green suite; this is the single most important manual check).
10. **Rust comment corrections** (`markdown.rs`, `nvim.rs`).
11. **Docs**, then `prettier --write`.
12. **Lint**: `bazel run //tools/lint:fix`, then `bazel test //tools/lint/...`. Per DEV-3, revert any unrelated files the fixer reformats.

---

## 7. Dependencies

**None.** No new Rust crates, no new Swift packages. `WKWebView.callAsyncJavaScript(_:arguments:in:contentWorld:completionHandler:)` is macOS 11.0+ and the bundle's `minimum_os_version` is `26.0` (`BUILD.bazel:160`). `DOMParser`, `ParentNode.replaceChildren` and `createDocumentFragment` are all long-standing WebKit APIs.

---

## 8. Lint considerations

- **swift-format `lineLength: 100`**: every physical line inside `stateScriptJS`'s `"""` literal must stay under ~95 columns. `__wolfUpdate` is the longest addition — break the fragment loop and the re-anchor branch across short lines, matching the existing style.
- The `callAsyncJavaScript` call has a long argument list; format it as one argument per line. The trailing closure plus two `in:` labels is awkward — prefer the explicit `completionHandler:` label over a trailing closure so the two `in:` parameters read unambiguously.
- `arguments: [String: Any]` will trigger no lint, but annotate the dictionary literal's type if inference is slow.
- No `// swift-format-ignore` should be needed anywhere.
- No new Rust code, so no clippy considerations; the two Rust changes are comments only.
- Markdown: `prettier --write` on every edited `.md`; preserve the single-giant-line paragraph style.

---

## 9. Risks

1. **The `.page` content world.** Getting it wrong makes the whole feature a silent no-op that falls back to today's behavior — green tests, unchanged bug. Mitigated by a load-bearing comment at the call site and by making it manual-check item #1.
2. **`headLength` as the head-change detector.** It is a length comparison, not a content hash — a future theme change that happened to produce an identical head length would slip through. Chosen over hashing because hashing ~1 MB per keystroke is real cost. The probability is negligible and the failure mode (stale CSS until the next full load) is mild. Record the trade-off in the doc comment.
3. **`splitDocument` couples Swift to `render_html`'s wrapper.** Mitigated by the first-open/last-close scan (which needs no exact literal), by rule 5's `nil` fallback to a full load, and by the test that feeds the wrapper verbatim. A wrapper change degrades to today's behavior, never to breakage.
4. **`replaceChildren` cost on a very large document.** Strictly less than a navigation (E7), but not free. If a pathological document ever proves too slow, the next step is a keyed/diffed swap — do **not** attempt that now.
5. **The ~1 MB `render_preview` string still crosses the Rust-to-Swift FFI on every keystroke** (`handle.render_preview(...).toString()`, `PaneContainerView.swift:1371`). Option A removes it from the *WebKit* IPC and CSS parse, not from the bridge copy. **Recommended follow-up, out of scope here:** an additive `render_preview_body` FFI (or a `WidthMode`-agnostic body-only render) so the fonts never leave `wolf-core`. Surface this to the user rather than doing it unilaterally.
6. **N1 (from the archived plan) is unchanged and slightly *less* reachable**: `.drop` nils `previewScrollStates[win]` while the old document can still `post()`. The in-place path reduces the number of straddling full loads, so N1's window shrinks. Not fixed here.
7. **A persistently failing in-place update degrades to today's behavior**, silently. Acceptable (it is a strict non-regression), but worth a note in the fallback's doc comment so a future debugger knows to look there.

---

## 10. What the architect found WRONG in the plan file's brief

1. **The diagnosis is correct but incomplete — and the incompleteness matters.** The brief attributes the flicker entirely to restore timing. Verified correct as far as it goes (`reloadDirtyPreviews` to `loadHTMLString` at `PaneContainerView.swift:1421`; restore only in `previewDidFinishLoad` at `:1654`). But it omits the dominant cost: **~965 KiB of base64 `@font-face` CSS is re-shipped and re-parsed on every keystroke** (E1). That omission is what makes Option B look viable; with E1 on the table, B is clearly the wrong choice. This is the single most important correction.

2. **"a full navigation … discards already-decoded webfonts" is imprecise, and the implied "blank frame" is wrong.** `PaneContainerView.swift:318-322` documents — from measurement — that WebKit keeps displaying the *previous* document until the next one commits. There is **no blank/white teardown frame**. The visible artifact is the new document committing at scrollY 0, not a blank flash. This means B's failure is purely a scroll-timing race, and Option A's win is "no commit at all" rather than "no blank frame".

3. **"the theme CSS and the inlined base64 `@font-face` only change on an appearance flip, which already takes a separate path" is WRONG in both halves.** The head does **not** change on an appearance flip: `render_preview` is pinned to `PreviewTheme::Default` + `WidthMode::Full` (`nvim.rs:397-405`, `:1419-1424`), and dark/light is a `prefers-color-scheme` media query *inside* that constant CSS. What changes on a flip is the **body's** inline `style="color:#…"` highlight spans, which nvim's re-sourced colorscheme produces. Consequence: the head **never** needs syncing for the in-app preview, and the appearance flip **should take the in-place path**. This makes the fix simpler *and* better than the brief expected.

4. **The "keep the full path for first bind / source switch / appearance flip" list is incomplete.** It omits two cases that are load-bearing for correctness, not performance: a **pending cross-file `#fragment`** (delivered from `previewDidFinishLoad`, which an in-place update never fires — the anchor would be stranded), and a **load already in flight** (an in-place mutation of the outgoing document would be discarded and would race that load's restore). Both are rules 1 and 2 of `updateStrategy`.

5. **"which may let some capture/restore machinery relax ON THIS PATH" — no relaxation is possible or desirable.** `captureDecision` keeps all three rules and is called on every full load exactly as today; all 12 of its regression tests stay valid verbatim. The correct statement is stronger and cleaner: the in-place path **never consults** the capture/restore machinery at all, and `updateStrategy` rule 2 makes the two mutually exclusive per window by construction.

6. **Minor:** the Option B sketch understates the mechanics — `WKUserContentController` offers only `removeAllUserScripts()`, so B would have to tear down and re-add `clickFocusJS` and `stateScriptJS` on every keystroke too (E5). Worth recording so nobody revisits B without knowing that.

---

## 11. Orchestrator decisions (these OVERRIDE the plan above where they conflict)

- **OD-1 — Option A is APPROVED as specified**, including the corrections in §10. In particular the appearance flip moves onto the in-place path, and the full-load list is the five-case list from §0, not the three-case list in the original brief.
- **OD-2 — `captureDecision` is FROZEN.** No new case, no rule change, no relaxation. All 12 of its existing tests must stay valid verbatim. It is the H1/H2 regression guard and it still runs on every full load.
- **OD-3 — the N2 single-funnel constraint is HARD.** `__wolfUpdate` must reach the viewport only through `scrollAnchorTo` / `moveTo` / `__wolfFollow`. If `testStateScriptJSFunnelsEveryScrollThroughMoveTo` breaks, the fix is to move `__wolfUpdate`'s definition after `moveTo` — never to loosen the assertion.
- **OD-4 — the `.page` content world is the single highest-risk line in the change.** It must carry the load-bearing comment at the call site, and it is manual-check item #1. A wrong world yields a green test suite and an unchanged bug.
- **OD-5 — risk 5 (the ~1 MB `render_preview` string still crossing the FFI per keystroke) is OUT OF SCOPE** and will be surfaced to the user as a recommended follow-up, not implemented here.

---

## 12. Test-writer contract (BINDING on the coder)

Tests landed in the existing, already-wired `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewScrollSeamTests.swift`: **52 → 79 methods** (27 new). BUILD wiring confirmed against the real file — `PreviewScrollSeam.swift` at `BUILD.bazel:77` (`wolf_key_map` srcs), `PreviewScrollSeamTests.swift` at `:275` (`WolfKeyMapTests` srcs). No BUILD change needed. **Zero of the 52 existing methods were modified, weakened or deleted; all 12 `captureDecision` methods are byte-identical** (OD-2 honored). The only non-additive edit is a doc-comment extension on `testStateScriptJSFunnelsEveryScrollThroughMoveTo` recording OD-3.

### The exact API the coder must implement

```swift
public struct DocumentSplit: Equatable {
  public let headLength: Int        // UTF-8 BYTE count through the `<body …>` tag
  public let body: String
  public init(headLength: Int, body: String)
}

public struct LiveDocument: Equatable {
  public let sourceBufnr: Int64
  public let headLength: Int
  public init(sourceBufnr: Int64, headLength: Int)
}

public enum UpdateStrategy: Equatable {
  case fullLoad
  case inPlace(body: String)        // the label `body:` is pinned by equality asserts
}

public static func splitDocument(_ html: String) -> DocumentSplit?   // unlabeled first arg

public static func updateStrategy(
  html: String,
  currentLoad: PreviewSeam.PreviewLoad,
  liveDocument: LiveDocument?,
  loadInFlight: Bool,
  hasPendingFragment: Bool
) -> UpdateStrategy

public static let inPlaceUpdateArgumentNames: (html: String, line: String, col: String)
public static func inPlaceUpdateJS() -> String                       // no arguments
```

**`inPlaceUpdateArgumentNames` MUST be the tuple form**, not "three lets" — §2.1 offered both; the tuple is what makes the anti-drift test a real pin, and it is what the tests use (`.html` / `.line` / `.col`).

### Additional pins

- `stateScriptJS` must contain `__wolfUpdate`, `DOMParser`, `parseFromString`, `text/html`, `createDocumentFragment`, `replaceChildren`, and a `while(` loop over `firstChild` with `appendChild(`.
- The whole script must **not** contain `replaceChildren.apply`, `.apply(`, or `Array.from(`.
- `__wolfUpdate` must be defined **after** `moveTo`/`__wolfRestore` and **before** the scroll listener — both the region helper and the N2 funnel test depend on this ordering (this is also OD-3's prescribed remedy).
- Inside `__wolfUpdate`, in this index order: `resolve(` → `getBoundingClientRect` → `replaceChildren` → `resolve(` → `scrollAnchorTo(`/`moveTo(` → `__wolfFollow(`.
- `__wolfUpdate` must contain **no** `__wolfUserScrolled` token at all.
- `inPlaceUpdateJS()`'s string: contains `__wolfUpdate`, a `!`/`&&` guard, a `return`; no `throw`, no `<`, no `wolf-markdown`; a `0` sentinel; and its free identifiers are exactly the three declared argument names.

### Plan error the test-writer corrected

**§4.2's claim that `__wolfUserScrolled = false` "appears exactly once in the whole script (inside `__wolfRestore`)" is FACTUALLY WRONG** — it appears **twice**: the document-start initialization (`PreviewScrollSeam.swift:401`) and `__wolfRestore`'s clear (`:459`). Writing that test as specified would fail against correct code. The test instead asserts (a) `__wolfUpdate`'s region contains no `__wolfUserScrolled` at all — stronger and count-independent — and (b) a whole-script count pinned at **2** with both legitimate sites named, so a third assignment site is still caught.

### Other interpretations pinned (coder must match)

- The free-identifier check uses a real scanner (strips string literals, property lookups after `.`, JS reserved words, page globals, and locally-bound names) and asserts **set equality** with the three declared names — so both an extra free name (ReferenceError → silent permanent full-load fallback) and a missing one (payload shipped and ignored) fail.
- The guard syntax is not over-pinned: `!` OR `&&` is accepted, and the sentinel matches any of four whitespace-squeezed forms.
- Rule-order tests each carry a control assertion (same inputs, deciding flag cleared → `.inPlace`), so a `.fullLoad` result cannot pass for an incidental reason.
- `headLength` is a UTF-8 **byte** count, pinned by a multibyte (em-dash) head test — not a `Character` count.
- `…InPlaceIgnoresTheTick` varies only `currentLoad.sourceTick` (1/2/99), because `LiveDocument` deliberately carries no tick.

---

## 13. Implementation landed — deviations and round-2 tests

Implementation complete and green. `//crates/wolf-macos:WolfKeyMapTests`: **79 methods, 0 failures**, all 27 new ones observed by name. Checkpoint after the seam step (before any caller existed) confirmed the 52 pre-existing methods still green, so OD-3's structural assertions never needed the remedy. Full sweep: `bazelisk build //crates/wolf-macos:Wolf` success, `//crates/wolf-core/... //crates/wolf-ffi/... :WolfKeyMapTests` 83/83, all four lint check targets pass uncached. The whole `crates/wolf-core/` diff is 37 inserted comment lines — no Rust behavior change, as §2.4 specified.

### Accepted deviations

- **DEV-A — `FrameStats` bucket reused rather than added.** §2.2 offered "prefer a new `.previewInPlace` case if `FrameStats.swift` allows it cheaply". It does not: `FrameStatsTests.swift` pins the bucket table exhaustively (an explicit 14-entry `(case, rawValue, label)` list plus `allCases.count` and ascending-rawValue assertions), so adding a case would require editing a test file — which the coder is not permitted to do. `.previewLoadHTML` is reused, documented at the call site. ACCEPTED.
- **DEV-B — `callAsyncJavaScript` success-payload handling.** The payload is `Any`, so the coder tests `((value as? NSNumber)?.intValue ?? 0) == 0`, which folds `NSNull`/undefined into the failure branch. That is what §3.5 wants. ACCEPTED. Full call form used: `callAsyncJavaScript(_:arguments:in: nil, in: .page, completionHandler:)` with the explicit label rather than a trailing closure, per §8.
- **DEV-C — 32 unrelated files reformatted by `//tools/lint:fix` were reverted** (the DEV-3 precedent from the predecessor feature). The fixer's only change to this change's code — wrapping the opening brace of `webView(_:didFail:withError:)` onto its own line for the 100-column limit — was kept.

**Confirmed:** the test-writer's §12 correction was right — `__wolfUserScrolled = false` does appear **twice**, and §4.2's "exactly once" claim would have failed against correct code.

### Round-2 tests (ORCHESTRATOR DECISION: all four APPROVED)

All are cheap pure-seam tests in the already-wired `PreviewScrollSeamTests.swift`.

- **T-A — `inPlaceUpdateJS` returns a function BODY, not a whole function.** Assert no leading `function` / `=>` token. **Highest value of the four:** today's free-identifier scanner would pass a body accidentally wrapped in `function(h,l,c){…}`, which `callAsyncJavaScript` rejects at runtime — and per OD-4's failure mode that means every in-place update fails, the fallback pins every window on the full path, and the flicker returns **with a fully green suite**. This is the same silent-degradation class as the content world.
- **T-B — `splitDocument` where `<body` appears only inside body content** (e.g. an unescaped `<body>` reaching the body via raw-HTML passthrough). The last-close half of the scan has a dedicated test; the first-open half is asserted only implicitly.
- **T-C — `splitDocument` on a `<body>` with no attributes** (rather than `<body class="wolf-markdown">`). Pins that the `>`-scan does not depend on an attribute being present — relevant to the browser-preview wrapper and to any future render change.
- **T-D — `updateStrategy` with `loadInFlight` AND `hasPendingFragment` both true.** Pins that rules 1 and 2 are independent rather than one masking the other.

### Noted, not actioned

- `//crates/wolf-core:redraw_grid` (`scroll_shifts_grid_content`, `scroll_emits_viewport`) failed once and passed on three subsequent runs. The only wolf-core change in this pass is comments, so this is the same pre-existing real-Neovim scroll-timing flake already recorded against the predecessor feature. Out of scope; worth owning separately.

---

## 14. Review findings — REVISION REQUIRED

The reviewer confirmed items 1-5, 7, 8 and 9 of the verification brief with no defects: the single-task guarantee (E8) genuinely holds (`__wolfUpdate` has no `await`/promise/rAF/`setTimeout`; `getBoundingClientRect()` forces layout, not a compositor commit, and `callAsyncJavaScript`'s synthesized async function has no `await` either, so **no frame at scrollY 0 can be painted**); the ordering contract and scroll math are exactly right (`rect.top + W.scrollY` and `beforeTop` are read after the same forced layout, so a WebKit scroll-clamp on a shrunken document cannot desync them); OD-3's single funnel holds; OD-2's `captureDecision` is byte-identical; the seven rules are correct, correctly ordered and each reachable — **note rule 4 is reachable only because the strategy switch runs BEFORE the capture switch, so reordering those two would silently kill it**; `splitDocument` is sound (the real head provably contains no `<body`, and base64 cannot contain `<`); the `.page` content world is correct and all three injection sites agree; DEV-B cannot misread a success, so there is no spurious per-keystroke reload; and the E2 head-invariant premise is verified against the real render path. Also confirmed: `clickFocusJS` attaches to `document` and the scroll listener to `window`, so neither is lost by `replaceChildren`.

### M1 — BLOCKER. `previewLoadsInFlight` is a Set, not a count, and `pendingPreviewDocuments` is destructively consumed by whichever callback lands first.

`PaneContainerView.swift:311` (decl), `:1525` (insert), `:1775-1776` (didFinish), `:1813-1814` (didFail). Both structures assume at most ONE outstanding navigation per window — but the predecessor feature already established (and shipped H2 to fix) that **two loads can straddle one page load**: the coalescer serializes to one reload per 33 ms and does NOT wait for `didFinish`.

Failure: (1) full load N issued for a long document, `previewLoadsInFlight = {win}`, `pendingPreviewDocuments[win] = D`. (2) Coalescer tick: rule 2 → full load N+1; pending overwritten, set still `{win}`. (3) `didFinish(N)` — or `didFail(N)`, since WebKit interrupts a superseded load — does `previewLoadsInFlight.remove(win)`. **N+1 is still in flight but the flag is now clear.** (4) The same callback `removeValue`s **N+1's** descriptor into `previewLiveDocuments` and empties the slot. (5) Next tick: rule 2 false, rule 3 false → **`.inPlace`**, swapping into the document N+1 is about to replace. (6) N+1 commits, discarding the swap and reverting to older content, then restores — **a visible flicker, the exact symptom this change exists to remove**. (7) `didFinish(N+1)` then sets `previewLiveDocuments[win] = nil`, forcing an extra pointless full load.

This directly falsifies the "structurally impossible" claim at `PreviewScrollSeam.swift:422-424` and plan §3.3. Reachable in the first-bind / post-`.drop` window on a long document while typing; bounded and self-healing, but invisible to the suite.

### M2 — MEDIUM. `previewInPlaceUpdateFailed` disables `captureDecision` rule 1 (H1's guard).

`PaneContainerView.swift:1834` sets `lastPreviewLoads[win] = nil`; `reloadDirtyPreviews` reads `previousLoad` from it at `:1395`, and rule 1 requires a non-nil `previousLoad`. In the OD-4 failure mode (wrong content world / persistently failing `__wolfUpdate`) this is **not transient**: every keystroke fails, nils the entry, and every subsequent full load runs with `previousLoad == nil`. A fragment-less cross-file link landing in that state takes `.capture` with the previous file's cached offsets — **H1 regressed**, on top of the flicker persisting. It also makes the termination doc comment at `:1825-1828` inaccurate: the degraded mode is pre-change behavior PLUS a disabled H1 guard.

### L1 — LOW. `previewDidFailLoad` does not clear `pendingPreviewRestores[win]`.

`PaneContainerView.swift:1812-1815`. After a failed navigation the flag clears but the pending restore survives forever (only `didFinish` consumes it) — a second counterexample to the rule-2 claim. Normally overwritten by the next `.capture`, but if that capture returns `.keepPending` (post-`.drop` empty cache) the stale offset is applied.

### L2 — LOW, informational. `<script>` elements do not execute on the in-place path.

`DOMParser`-created script elements are flagged "already started" and do not run when adopted, whereas `loadHTMLString` runs them. E6's "behavior is unchanged either way" is strictly wrong. Nothing depends on it and the new behavior is safer, but the asymmetry needs a sentence in `stateScriptJS`'s doc comment so nobody later diagnoses it as a bug.

## 15. Fix design — ORCHESTRATOR DECISION

Per the reviewer's recommendation, M1's bookkeeping moves into the pure seam so it is headlessly testable — the same move that made H1/H2 testable.

**F-M1.** `previewLoadsInFlight` becomes `[Int64: Int]` (a per-window COUNT). `:1525` increments; both `previewDidFinishLoad` and `previewDidFailLoad` decrement, then route through a new pure seam function:

```swift
public enum LoadCommitOutcome: Equatable {
  case stillOutstanding          // more loads pending: leave live AND pending alone
  case commit(LiveDocument?)     // last load, succeeded: live = pending, clear pending
  case discard                   // last load, FAILED: leave live unchanged, clear pending
}

public static func loadCommitOutcome(
  remaining: Int,                // the count AFTER decrementing
  pending: LiveDocument?,
  succeeded: Bool
) -> LoadCommitOutcome
```

`remaining > 0` → `.stillOutstanding`; `remaining == 0 && succeeded` → `.commit(pending)`; `remaining == 0 && !succeeded` → `.discard`. **`.discard` must NOT clear `previewLiveDocuments`** — a failed navigation never commits, so WebKit keeps displaying the previous document and the previous live descriptor stays accurate. `updateStrategy`'s `loadInFlight` argument becomes `(previewLoadsInFlight[win] ?? 0) > 0`. Correct the rule-2 doc comment at `PreviewScrollSeam.swift:422-424` and plan §3.3's claim so they describe what the count actually guarantees.

**F-M2.** `previewInPlaceUpdateFailed` must NOT destroy `lastPreviewLoads[win]`'s bufnr. **Preferred: a `previewForcedReloads: Set<Int64>` unioned into `needing` in `reloadDirtyPreviews` and cleared as it is consumed** — it re-arms the reload without fabricating data. (The alternative of writing back `PreviewLoad(sourceBufnr: prior.sourceBufnr, sourceTick: 0)` also works but fakes a tick; prefer the explicit set.) Then correct the termination doc comment.

**F-L1.** Add `pendingPreviewRestores[win] = nil` to `previewDidFailLoad`.

**F-L2.** Add the `<script>`-does-not-execute sentence to `stateScriptJS`'s doc comment.

**F-DOC.** The technical-writer has since completed plan §5 (7 files: `architecture.md`, `crates/wolf-macos/README.md`, `Sources/README.md`, `Tests/README.md`, `crates/wolf-core/README.md`, `crates/wolf-core/src/markdown/README.md`, `build.md`), so the reviewer's D1 queue is CLOSED — it reviewed a snapshot taken before that pass. **But `prettier --write` was never run** (the technical-writer has no Bash). It must be run on those 7 files, with any collateral reverted.

### Round-3 tests (APPROVED)

- **T-E** — `loadCommitOutcome` truth table: `remaining > 0` → `.stillOutstanding` (regardless of `succeeded`, and the pending descriptor must survive); `remaining == 0 && succeeded` → `.commit(pending)`, including `.commit(nil)` when the HTML was unsplittable; `remaining == 0 && !succeeded` → `.discard`; and a straddling-load regression test walking M1's exact 7-step sequence through the function to prove the swap can no longer race a live navigation.
- **T-F** — a pure-seam pin that `.discard` and `.stillOutstanding` are distinguishable from `.commit(nil)`, since conflating them is precisely how M1's step 7 wiped a good live document.

### Noted, not actioned

`feat-list.md` has no row for the preview scroll-sync feature at all — a gap left by the PREDECESSOR feature, not by this fix. The technical-writer declined to add one because the table is column-aligned and it could not run prettier. Surfacing to the user rather than doing it unilaterally.

---

## 16. Round-3 verification: APPROVED — ship it

The reviewer re-verified all four fixes. F-M1, F-M2, F-L1 and F-L2 are correctly implemented, and **M1's 7-step failure sequence is now structurally impossible**: the count is non-zero from the first outstanding navigation to the last one's callback, so at step 4 rule 2 still forces `.fullLoad` and no in-place swap can be issued into a document a live navigation is about to replace. Counting is exact — the single `loadHTMLString` call site sits five branch-free lines below the only increment, and all three WebKit terminal callbacks funnel through the one decrement. `.discard` provably never touches the live slot. Nothing previously verified was disturbed (`__wolfUpdate`'s body is byte-for-byte the approved form; the only `stateScriptJS` edit is F-L2's doc paragraph).

Both coder deviations confirmed sound. **Deviation 2 in particular caught a defect the plan missed**: `decidePolicyFor` `.cancel`s every clicked link, so WebKit delivers `didFailProvisionalNavigation` for navigations that were never counted — without the zero-count guard the count would go negative and wedge rule 2 on permanently, disabling the in-place path and restoring the flicker.

### Residual follow-ups (NOT blockers — surfaced to the user, not fixed unilaterally)

- **R1 (medium-low) — the count carries no navigation identity.** The zero-count guard rejects an uncounted callback only when the count is *already* zero; an uncounted callback arriving while a real navigation is outstanding still decrements someone else's count. Reachable by clicking a preview link while a reload is in flight (common on a long document, whose load far outlasts the ~33 ms coalescer window): `decidePolicyFor` cancels the click, `didFailProvisionalNavigation` decrements 1 → 0 → `.discard` clears the pending restore and descriptor, and the real load's `didFinish` then commits nothing. Mild variant costs one jump plus a stale descriptor. Worse variant — when the click was `.previewFile`, so a new load is issued right after the spurious decrement — lets the OLD load's `didFinish` commit the NEW load's descriptor while its navigation is still live, so rule 2 reads all-clear and the next tick can pick `.inPlace` into a doomed document. Bounded, self-healing after one discarded swap, but it is the M1 family through a narrower door.
  **Remedy:** `loadHTMLString` returns `WKNavigation?` and both `didFail…` callbacks receive the `WKNavigation`. Key the bookkeeping on identity — `previewNavigations: [Int64: Set<ObjectIdentifier>]`, insert at issue, remove at callback — and derive `remaining` from `.count`. The seam needs NO change; `loadCommitOutcome` still takes `remaining`. That makes an uncounted callback exactly ignorable at any count.
- **R2 (low) — a lost terminal callback now wedges a window permanently** instead of self-healing as the old Set did. If a web-content process dies mid-navigation and WebKit delivers no `didFail…` (there is no `webViewWebContentProcessDidTerminate` in `PreviewNavigationDelegate.swift`), the count keeps a permanent +N offset, rule 2 stays true forever, `.commit` never fires again, and that pane reverts to permanent full loads — i.e. the flicker returns for it until the preview is closed. **Remedy:** forward `webViewWebContentProcessDidTerminate` to a container method clearing `previewLoadsInFlight[win]`, `pendingPreviewDocuments[win]` and `previewLiveDocuments[win]`.
- **R3 (note only)** — the zero-count guard lives in the AppKit layer, the same compile-and-manual-smoke stratum M1 hid in. Correct as written, but it could be seam-side and headlessly tested.

### Final state

`PreviewScrollSeamTests.swift`: **89 methods** (52 pre-existing + 27 + 4 round-2 + 6 round-3), 0 failures; whole `WolfKeyMapTests` binary 399 tests. Full sweep 87/87 with all lint check targets clean. `prettier --write` run on all 7 technical-writer files — **zero reflow, all reported unchanged**. Docs verified in sync by the reviewer.

`feat-list.md` still has no row for the preview scroll-sync feature — a gap left by the PREDECESSOR feature, not this fix. Surfaced to the user.
