# Current plan: Markdown-preview scroll sync (cursor-follow)

## Status

PHASE 2 — architect plan APPROVED and recorded below. Next: test-writer.

## Problem statement (from the user)

When a Markdown document is open in an editor pane AND in a preview pane, typing in the
editor makes the preview jump back to the top. `PaneContainerView.reloadDirtyPreviews()`
does a full `web.loadHTMLString(...)` on every `changedtick` bump, resetting scroll to 0.
The preview HTML already carries invisible position markers
(`<span class="wolf-src" id="wl-{L}-{C}" data-line data-col></span>`, `L` = 1-based hard
line, `C` = 0-based CHARACTER column in multiples of 100) emitted by
`crates/wolf-core/src/markdown.rs`. Use those markers to keep the region around the
editing cursor scrolled into view.

Required behavior:

1. **No jump-to-top on edit.** Across a reload the edited region stays visually
   stationary: capture the cursor anchor's viewport-relative Y before the reload
   (plus raw `scrollY` as fallback), restore it after `didFinish`, clamped.
2. **Cursor-follow.** When the cursor moves and its anchor is off-screen or near a
   viewport edge, scroll it into view. Do NOT scroll when the anchor is comfortably
   inside the viewport (the viewport-edge threshold rule) — otherwise the preview
   twitches on every keystroke.
3. **Anchor resolution.** Prefer `#wl-{line}-{col rounded down to a multiple of 100}`,
   then `#wl-{line}-0`, then the nearest PRECEDING line marker, then raw `scrollY`.

Out of scope: the browser preview (`browser_preview.rs`) polling reload.

## Orchestrator decisions (these OVERRIDE the plan below where they conflict)

- **D1 — Take Risk 5's mitigation, do not just comment it.** The tuning constants
  (`48`, `0.15`, `0.35`, `0.33`, `100`, `200`) must exist in exactly ONE place: the
  `PreviewScrollSeam` Swift constants, INTERPOLATED into the generated JS string. Do not
  duplicate the literals in the JS and cross-reference them in comments — a silent
  divergence would leave the Swift tests green while the shipped behavior is wrong.
  Mind the 100-column swift-format limit when interpolating. The seam tests must assert
  that the generated JS actually CONTAINS the constant values (so a future constant
  change provably reaches the JS).
- **D2 — The architect's four deviations (§0) are ACCEPTED**: no `seq`-invariance
  integration test (structurally enforced, untestable through keystrokes); no Lua-side or
  Rust-side debounce (Swift-side coalescer only); no preview-existence gate in Lua; no
  work in `crates/wolf-gnome` beyond the one-sentence README note.
- **D3 — The §5.5 CRITICAL INVARIANT is non-negotiable.** `PreviewSeam.PreviewLoad` stays
  exactly `(sourceBufnr, sourceTick)`. Cursor values must never reach either
  `PreviewLoad` construction site, so a cursor-only change is structurally incapable of
  arming the HTML-reload coalescer.

---

# Implementation plan: Markdown-preview scroll sync (cursor-follow)

## 0. Verification of the plan file's scope notes

I checked every claim. Results:

| Plan-file claim | Verdict |
|---|---|
| "no `CursorMoved` autocmd exists anywhere today" | **CORRECT.** Only `CursorHold`/`CursorHoldI` exist, in the unrelated `WolfAutoRead` group (`/Users/jplein/Git/jplein/wolf/runtime/nvim/init.lua:93`). `charcol` is used nowhere in the repo. |
| "`reloadDirtyPreviews()` does a full `loadHTMLString` on every `changedtick` bump" | **CORRECT** — `/Users/jplein/Git/jplein/wolf/crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift:1301`. |
| "markers are `<span class="wolf-src" id="wl-{L}-{C}" data-line data-col></span>`, `L` 1-based line, `C` 0-based CHAR column, `C ∈ {0, 100, 200, …}`" | **CORRECT** — `marker_span` at `/Users/jplein/Git/jplein/wolf/crates/wolf-core/src/markdown.rs:415`, `desired_markers` at `:432`, `MARKER_COLUMN_STRIDE = 100` at `:99`. Every hard line gets a `(L, 0)`; per-100 markers only for lines strictly longer than 100 chars. |
| "same shape as the existing `wolf_markdown_dirty` autocmds" | **PARTLY WRONG.** The existing dirty autocmd (`init.lua:2096-2108`) guards ONLY on `vim.bo[ev.buf].filetype == "markdown"` — there is **no** preview-scratch guard. Preview scratch buffers happen to be excluded because they are `nofile` scratch buffers that never run filetype detection, so their `filetype` stays `""`. The new cursor autocmd must add an **explicit** `b:wolf_preview_source` guard (as the plan file requires) — that is a deliberate asymmetry with the sibling, and it must be documented as such. |
| "FFI: follow the existing `FfiPreview` / `previews()` shape" | **CORRECT and it is the right choice.** See §3 for the append-only analysis: the repo's append-only contract governs **`u8` discriminant modules** (`command_kind`, `layout_intent`, `liveness`) — see `/Users/jplein/Git/jplein/wolf/crates/wolf-ffi/tests/commands.rs:116` — **not** opaque-type accessors. `FfiPreview` is opaque; ADDING accessors is regen-only and ABI-safe (`/Users/jplein/Git/jplein/wolf/crates/wolf-ffi/README.md:252` documents exactly this precedent for `source_display_path()`). |
| "check `PreviewSeam.winsNeedingReload`'s gate so a cursor bump is not folded into the reload trigger" | **CORRECT, and the fix is structural**: `PreviewLoad` is `(sourceBufnr, sourceTick)` only (`PreviewSeam.swift:58-65`) and is constructed at exactly two sites (`PaneContainerView.swift:1213` and `:1275`). If the coder simply does not touch `PreviewLoad`, cursor changes cannot arm the reload coalescer. |
| "Rust integration test in `crates/wolf-core/tests/`" | **FEASIBLE.** `WolfCore::open_preview(&str)` is public (`/Users/jplein/Git/jplein/wolf/crates/wolf-core/src/api.rs:659`) and `Snapshot` fields are `pub`. Harness: `crates/wolf-core/tests/current_file.rs` + `tests/common/mod.rs`. |
| "Update `architecture.md` / READMEs" | **CORRECT** — exact anchors listed in §7. |

**Deviations I am making from the plan file, with reasons:**

1. **No seq-invariance integration test.** A pure cursor motion in nvim also emits a `cursor_goto` + `flush` in the redraw batch, which legitimately bumps `Snapshot.seq`. Asserting "cursor notification does not bump seq" through keystrokes is untestable and would be flaky-or-wrong. The no-`seq`-bump property is enforced structurally (the handler never touches `snap.seq`) and asserted by code review only — same posture as every other out-of-band notification in `redraw.rs`.
2. **No debounce on the Lua side or the Rust side; debounce on the Swift side only.** Justified in §1.4.
3. **No preview-existence gate in Lua.** Justified in §1.3.
4. **The GTK front-end needs nothing.** Confirmed: `crates/wolf-gnome` has zero occurrences of "preview" outside two README sentences, one of which is about nvim's own `:preview` window (`/Users/jplein/Git/jplein/wolf/crates/wolf-gnome/README.md:153`). There is no `WebKitGTK` preview pane. The new `Snapshot` field is additive with a `Default` impl, so `wolf-gnome` keeps compiling untouched. **State this in the plan; do no work there.**

---

## 1. Summary

Build the second half of the already-designed Markdown-preview scroll-sync: a Neovim→core cursor-position signal (`wolf_markdown_cursor`, carrying 1-based line + **0-based CHARACTER** column via `charcol()`), a new `Snapshot::markdown_cursors` map keyed by SOURCE buffer number (mirroring `markdown_ticks`, no `seq` bump), two additive `FfiPreview` accessors joining that cursor per preview window, and a new pure Swift seam (`PreviewScrollSeam`) plus AppKit glue that (a) keeps the edited region visually stationary across a full `loadHTMLString` reload by capturing a continuously-cached anchor offset **before** the reload and restoring it after `didFinish`, and (b) scrolls the cursor's `wl-{L}-{C}` anchor into view only when it crosses a viewport-edge threshold. A cursor-only change must never arm the HTML-reload coalescer.

---

## 2. Layer 1 — Lua (`runtime/nvim/init.lua`)

### 2.1 New autocmd inside `_G.wolf_setup_preview(chan)`

Add **after** the existing `TextChanged`/`TextChangedI`/`BufWritePost` autocmd (currently `init.lua:2096-2108`) and **before** the final `emit_previews()` call at `:2111`.

**Local closure `emit_cursor(buf)`** (defined inside `wolf_setup_preview` so it closes over `chan`):

- Bail if `buf` is nil or `not vim.api.nvim_buf_is_valid(buf)`.
- Bail if `buf ~= vim.api.nvim_get_current_buf()`. **Load-bearing:** `vim.fn.line(".")` / `vim.fn.charcol(".")` read the CURRENT window's cursor, so reporting for a non-current buffer would emit the wrong position. `CursorMoved`/`CursorMovedI` always fire for the current buffer, so this is purely defensive — but it is what makes the un-winid'd `charcol('.')` unambiguous.
- Bail if `vim.bo[buf].filetype ~= "markdown"` (the same gate as `wolf_markdown_dirty`).
- Bail if `vim.b[buf].wolf_preview_source ~= nil` (the preview-scratch guard). Document that this is an ASYMMETRY with the sibling dirty autocmd, which relies on the scratch's empty filetype; here we are explicit so a future `:filetype detect` on a `Preview: foo.md`-named scratch can never leak a bogus cursor.
- Compute `local line = vim.fn.line(".")` and `local char_col = vim.fn.charcol(".") - 1`.
  - **`charcol()` semantics, verified:** `charcol({expr})` is `col()`'s character-index twin — it returns the **1-based CHARACTER index** of the cursor column. Subtracting 1 yields the 0-based character column the marker scheme wants (OD4), with **no** byte→char conversion anywhere downstream. In insert mode with the cursor past end-of-line, `charcol('.')` returns `char_len + 1`, so `char_col == char_len` (one past the last character) — the JS rounds down to a multiple of 100 and falls back, so this is benign.
  - Defensive clamp: `if char_col < 0 then char_col = 0 end`.
- `vim.rpcnotify(chan, "wolf_markdown_cursor", buf, line, char_col)` — **three FLAT scalars**, mirroring `wolf_markdown_dirty`'s flat `(bufnr, tick)` shape rather than a table. Flat scalars sidestep the empty-Lua-table map-vs-array msgpack ambiguity that `wolf_setup_theme_tracking` already documents (`init.lua:526-530`).

**The autocmd:**

```
vim.api.nvim_create_autocmd({ "CursorMoved", "CursorMovedI" }, {
  group = group,           -- the same WolfPreview augroup
  callback = function(ev) emit_cursor(ev.buf) end,
})
```

**Expose the closure** as `_G.wolf_emit_markdown_cursor = emit_cursor` (mirroring `_G.wolf_emit_previews` at `init.lua:2086`), and add a **one-shot initial emit** `emit_cursor(vim.api.nvim_get_current_buf())` right beside the existing `emit_previews()` at `:2111`, so the startup buffer's cursor is known without waiting for a move.

### 2.2 Re-emit when a preview opens

In `_G.wolf_open_preview(mode)` (`init.lua:1813`): immediately after the markdown gate and `local source_buf = vim.api.nvim_get_current_buf()` (i.e. **before** any placement branch, while the SOURCE buffer + its window are still current), call

```
if _G.wolf_emit_markdown_cursor then _G.wolf_emit_markdown_cursor(source_buf) end
```

**Load-bearing:** for `"split_right"` / `"active"`, the placement branch does `nvim_win_set_buf(0, preview_buf)`, making the preview scratch the CURRENT buffer. Emitting after that would hit the preview-scratch guard and bail, leaving the freshly-opened preview with no anchor until the user's next cursor move. Emitting first means a newly-opened preview immediately scrolls to where the user is.

Do **not** add an emit to `wolf_preview_file` (`init.lua:1982`): its source buffer is a background `bufadd`/`bufload` with no window and no meaningful cursor. The next cursor move covers it.

### 2.3 Rejected: gating the notification on "a preview exists for this buffer"

A gate like `_G.wolf_preview_bufs[buf] ~= nil` would cut volume when no preview is open, but it breaks the open-a-preview-after-editing case: the first cursor notification would only arrive on the NEXT cursor move, so a just-opened preview would have no anchor. It also makes the notification's meaning conditional on unrelated state. **Do not add it.** The §2.2 re-emit is the correct way to solve the freshness problem.

### 2.4 Throttle/volume analysis — decision: no Lua-side and no Rust-side debounce

- `CursorMovedI` fires on every insert-mode keystroke. That is exactly the cadence `TextChangedI` (and therefore `wolf_markdown_dirty`) already runs at today. No new order of magnitude.
- `CursorMoved` additionally fires on normal-mode motions (`j`/`k`/`h`/`l`), which do **not** fire `TextChanged`. But every such motion already produces a redraw batch ending in `flush`, hence a wake. So the new notification adds **one msgpack notify + one brief `RwLock` write per motion** and **zero additional wakes**.
- The expensive work is `WKWebView.evaluateJavaScript`, which is what must be rate-limited — and that is Swift-side.
- A Lua-side `vim.schedule` coalescer (as `wolf_setup_buffer_tracking` uses, `init.lua:420-436`) would buy almost nothing: `CursorMoved` already fires at most once per event-loop iteration. It would only add code and a one-tick latency.

**Decision: no debounce in Lua, no debounce in Rust; a `PickerRefreshCoalescer` (~30fps trailing) in Swift.** Same architecture as `previewReloadCoalescer`.

---

## 3. Layer 2 — Core (`crates/wolf-core`)

### 3.1 `src/render.rs`

Add, next to `PreviewInfo` (currently at `:459-482`):

```
/// The cursor position in a SOURCE Markdown buffer, as reported by the
/// `wolf_markdown_cursor` notification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkdownCursor {
    /// 1-based hard buffer line (`line('.')`), matching the `wl-{L}-{C}` marker
    /// scheme's `L`. `0` means "never reported".
    pub line: u32,
    /// 0-based CHARACTER column (`charcol('.') - 1`), matching the marker
    /// scheme's `C` (OD4 — a character offset, NOT a byte offset).
    pub col: u32,
}
```

Named `MarkdownCursor`, **not** `Cursor` — `render.rs` already owns a grid `Cursor` type and the two must not be confused.

Add to `Snapshot` (immediately after `markdown_ticks`, `:809`):

```
/// Per-source-buffer cursor position, keyed by the SOURCE Markdown buffer
/// number. Upserted out-of-band via `wolf_markdown_cursor`; mirrors
/// `markdown_ticks`. Does NOT bump `seq`. Defaults to empty.
pub markdown_cursors: HashMap<i64, MarkdownCursor>,
```

`Snapshot` derives `Default`, so no constructor change is needed. Re-export `MarkdownCursor` from the crate root wherever `PreviewInfo` is re-exported (check `src/lib.rs`'s `pub use render::{…}` list).

### 3.2 `src/redraw.rs`

**Const**, immediately after `MARKDOWN_DIRTY_EVENT` (`:166`):

```
/// Name of the out-of-band notification carrying a `[source_bufnr, line,
/// char_col]` triple for a Markdown source buffer whose cursor moved, emitted by
/// the `CursorMoved`/`CursorMovedI` autocmds in `runtime/nvim/init.lua`. `line`
/// is 1-based; `char_col` is a 0-based CHARACTER column (OD4). Core UPSERTS it
/// into `Snapshot::markdown_cursors`. Routed separately from `redraw`.
const MARKDOWN_CURSOR_EVENT: &str = "wolf_markdown_cursor";
```

**Dispatch arm** in `handle_notify`, immediately after the `MARKDOWN_DIRTY_EVENT` arm (`:335-342`), with the identical wake-after-unlock comment:

```
MARKDOWN_CURSOR_EVENT => {
    self.apply_markdown_cursor(args).await;
    (self.wake)();
}
```

**Handler**, immediately after `apply_markdown_dirty` (`:580-590`), modeled on it exactly:

```
async fn apply_markdown_cursor(&self, args: Vec<Value>) {
    // bufnr via `ext_handle` (plain int or defensively Ext-wrapped)
    // line  via `as_u64`, saturating into u32
    // col   via `as_u64`, saturating into u32
    // a payload missing any of the three is dropped
    // then: snap.markdown_cursors.insert(bufnr, MarkdownCursor { line, col });
}
```

Use the existing private helpers `ext_handle` (`:1020`) and `as_u64` (`:1008`). Convert with `u32::try_from(v).unwrap_or(u32::MAX)` or a `min(u64::from(u32::MAX)) as u32` — do **not** use a bare `as u32` cast (clippy `cast_possible_truncation`). No `seq` bump; no filesystem work; no `.await` under the guard.

### 3.3 `src/nvim.rs`

**No code change** to `setup_preview()` (`:649`) — the new autocmd is installed by the same `wolf_setup_preview(chan)` call. **Update its doc comment** (`:636-648`) to list the new `CursorMoved`/`CursorMovedI` autocmd and the `wolf_markdown_cursor` payload.

### 3.4 `src/api.rs`

No change. (`WolfCore::read_snapshot()` already exposes the whole `Snapshot`.)

---

## 4. Layer 3 — FFI (`crates/wolf-ffi/src/lib.rs`)

### 4.1 ABI / append-only analysis

The append-only contract in this repo governs the **`u8` discriminant modules** (`command_kind`, `layout_intent`, `liveness`) — the invariant is that a new command takes the next higher byte and existing bytes never move (`crates/wolf-ffi/tests/commands.rs:116`). `FfiPreview` is an **opaque** type: Swift only ever calls named accessor functions through generated externs, so **adding** accessors is ABI-safe and regen-only. The precedent is documented verbatim at `/Users/jplein/Git/jplein/wolf/crates/wolf-ffi/README.md:252` ("The later ADDITIVE `FfiPreview.source_display_path()` accessor … is likewise regen-only … no new wake"). The only real constraint: **do not rename or change the signature of any existing accessor**, and **do not reorder** the `extern "Rust"` block's existing entries.

**Decision: two additive scalar accessors on `FfiPreview`.** This is the cleanest surface because `previews()` is *already* the per-window join point that both Swift call sites use (`PaneContainerView.swift:1198` and `:1273`), so the cursor arrives with zero new FFI calls and zero new opaque types.

### 4.2 Changes

**Bridge declaration** — append to the `extern "Rust" { type FfiPreview; … }` block (after `source_full_width` at `:711`):

```
// The SOURCE buffer's cursor LINE (1-based, matching the `wl-{L}-{C}` marker
// scheme). `0` until the first `wolf_markdown_cursor` — the front-end treats
// 0 as "cursor unknown" and does not scroll.
fn source_cursor_line(&self) -> u32;
// The SOURCE buffer's cursor COLUMN — a 0-based CHARACTER column (OD4), NOT a
// byte column. `0` until the first `wolf_markdown_cursor`.
fn source_cursor_col(&self) -> u32;
```

**Struct** — add `source_cursor_line: u32` and `source_cursor_col: u32` to `pub struct FfiPreview` (`:3396-3404`).

**Accessors** — two `#[must_use] pub fn` returning the fields, after `source_full_width()` (`:3455`).

**Constructor** — `FfiPreview::from_preview` (`:3464`) gains a `cursor: MarkdownCursor` parameter (a `Copy` type, so pass by value) and fills the two fields from it.

**Join** — in `WolfCoreHandle::previews()` (`:2742-2760`), alongside the existing `source_tick` lookup:

```
let cursor = snap.markdown_cursors.get(&info.source_bufnr).copied().unwrap_or_default();
```

and pass it into `from_preview`. Update the doc comment on `previews()` (`:2737-2741`).

**Regenerate + commit** `crates/wolf-ffi/generated/` via `bazel run //crates/wolf-ffi:gen_bindings`. Only `wolf-ffi.swift` / `wolf-ffi.h` change; `wolf_ffi_wake.h` stays **byte-identical** (this rides the existing editor render wake — no new wake channel).

---

## 5. Layer 4 — Swift

### 5.1 New pure seam: `crates/wolf-macos/Sources/WolfKeyMap/PreviewScrollSeam.swift`

`public enum PreviewScrollSeam`, Foundation-only (no AppKit, no WebKit), sibling of `PreviewSeam` / `PreviewLinkSeam`.

**Note on escaping:** unlike `PreviewLinkSeam` (which interpolates a user-controlled fragment string and therefore needs `jsonEscape`), **every value this seam interpolates into JS is a validated NUMBER**. Anchor ids are assembled *inside* the JS from numeric line/col. So **no JSON escaper is needed and none should be added** — say so in the file header. Format `Double`s with `String(format: "%.2f", v)` (no locale argument ⇒ non-localized ⇒ no comma decimal separator can corrupt the JS); format integers with `String(UInt32)`.

**Types:**

```
public struct CursorPos: Equatable, Hashable {
  public let line: UInt32   // 1-based; 0 == unknown
  public let col: UInt32    // 0-based CHARACTER column
  public init(line: UInt32, col: UInt32)
  public var isKnown: Bool  // line > 0
}

public struct ScrollState: Equatable {
  public let anchorTop: Double?  // viewport-relative Y of the cursor anchor; nil = unresolved
  public let scrollY: Double     // raw window.scrollY fallback
  public init(anchorTop: Double?, scrollY: Double)
}

public struct RestoreRequest: Equatable {
  public let cursor: CursorPos
  public let targetY: Double?    // where the anchor sat before the reload; nil = no target
  public let fallbackScrollY: Double
  public init(cursor: CursorPos, targetY: Double?, fallbackScrollY: Double)
}

public enum FollowDecision: Equatable {
  case stay
  case scroll(toViewportY: Double)
}
```

**Tuning constants (public so tests pin them):**

| Constant | Value | Justification |
|---|---|---|
| `markerColumnStride: UInt32` | `100` | Mirrors `wolf_core::markdown::MARKER_COLUMN_STRIDE`. Document the cross-language coupling. |
| `minEdgeThreshold: Double` | `48` | Two rows of `base.css`'s 24px baseline grid, and the same 48px `scroll-margin-top` headings already use — so the comfort band is grid-aligned and always ≥ 2 lines of clearance. |
| `edgeThresholdFraction: Double` | `0.15` | 15% of viewport height keeps a comfortable band on a tall pane without eating it. |
| `maxEdgeThresholdFraction: Double` | `0.35` | Caps the threshold on a SHORT pane. Without it, a 100px-tall pane would get a 48px threshold on both edges, leaving a 4px "comfort band" ⇒ near-continuous re-scrolling. |
| `restPositionFraction: Double` | `0.33` | When a scroll IS required, the anchor lands one third down. Biased ABOVE center on purpose: typing moves the cursor DOWN, so leaving 2/3 of the viewport below means the next several lines of typing stay inside the band and do not re-trigger a scroll. This is the anti-twitch property. |
| `maxAnchorScanLines: UInt32` | `200` | Bounds the "nearest preceding line marker" downward scan. |
| `stateHandlerName: String` | `"wolfPreviewScroll"` | Must differ from `PreviewSeam.clickFocusHandlerName` (`"wolfPaneFocus"`) — both are registered on the SAME `WKUserContentController`. |

**Pure functions:**

- `static func anchorColumn(_ col: UInt32) -> UInt32` — `col / markerColumnStride * markerColumnStride` (integer division ⇒ round down to a multiple of 100).
- `static func anchorIds(line: UInt32, col: UInt32) -> [String]` — `[]` when `line == 0`; else `["wl-\(line)-\(anchorColumn(col))"]` plus `"wl-\(line)-0"` **only when `anchorColumn(col) != 0`** (deduped). Mirrors the JS resolution order so the ordering contract is unit-testable in Swift.
- `static func edgeThreshold(viewportHeight: Double) -> Double` — `min(max(minEdgeThreshold, viewportHeight * edgeThresholdFraction), viewportHeight * maxEdgeThresholdFraction)`, floored at `0`.
- `static func followDecision(anchorTop: Double, viewportHeight: Double) -> FollowDecision` — let `t = edgeThreshold(viewportHeight:)`; return `.stay` when `anchorTop >= t && anchorTop <= viewportHeight - t` (**inclusive** at both boundaries — pin this convention in a test), else `.scroll(toViewportY: viewportHeight * restPositionFraction)`. This mirrors the JS so the rule itself is headlessly testable.
- `static func winsNeedingScroll(current: [Int64: CursorPos], applied: [Int64: CursorPos]) -> [Int64]` — the exact structural mirror of `PreviewSeam.winsNeedingReload`: `current.filter { win, c in applied[win] != c }.map(\.key)`. A win in `applied` but absent from `current` is never returned. Order unspecified.
- `static func stateScriptJS(handlerName: String = stateHandlerName) -> String` — the document-start user script (§5.2).
- `static func followJS(line: UInt32, col: UInt32) -> String` — `"window.__wolfFollow&&window.__wolfFollow(<line>,<col>);"`.
- `static func restoreJS(_ request: RestoreRequest) -> String` — §5.3.

### 5.2 The injected document-start script (`stateScriptJS`)

A self-invoking IIFE, wrapped entirely in `try { … } catch (e) {}` so a missing handler can never throw into the page. It defines, on `window`:

1. `__wolfLine = 0`, `__wolfCol = 0`, `__wolfUserScrolled = false`.

2. **`resolve(line, col)`** — the anchor-resolution chain, in exactly the order the plan requires:
   - `if (line < 1) return null;`
   - `var c = Math.floor(col / 100) * 100;`
   - if `c > 0`: `getElementById('wl-' + line + '-' + c)` → return if found.
   - `getElementById('wl-' + line + '-0')` → return if found.
   - **Nearest PRECEDING line marker — bounded downward scan.** `for (var l = line - 1, n = 0; l >= 1 && n < 200; l--, n++) { e = getElementById('wl-' + l + '-0'); if (e) return e; }`.
     **Why this is efficient:** `desired_markers` emits a `wl-{L}-0` for **every** hard line (`markdown.rs:438`), so the only way the exact line marker is missing is that the document SHRANK past that line. The scan therefore normally terminates in **one** step (line-1 exists), and each step is an O(1) hash lookup. A linear-in-document-length scan is not possible within the 200 bound.
   - **Deep fallback (only if the bounded scan exhausts).** One `document.querySelectorAll('.wolf-src[data-col="0"]')` pass, walked **backwards**; the NodeList is in document order (⇒ ascending line), so the first node walking backwards whose `+el.getAttribute('data-line') <= line` is the nearest preceding marker. This is a single O(N) DOM query taken only on deep failure.
   - else `null`.

3. **`threshold(vh)`** — `Math.min(Math.max(48, vh * 0.15), vh * 0.35)`. Keep the literals in sync with `PreviewScrollSeam`'s constants and say so in a comment on both sides.

4. **`scrollAnchorTo(el, targetY)`** — `var docY = el.getBoundingClientRect().top + window.scrollY; var max = Math.max(0, document.documentElement.scrollHeight - window.innerHeight); window.scrollTo(0, Math.min(Math.max(0, docY - targetY), max));`
   **Use `window.scrollTo(0, y)` — never `behavior: 'smooth'`.** A smooth animation would still be running when the next keystroke's follow fires, and would make the restore inexact. Add a test asserting the string does NOT contain `behavior`.
   `.wolf-src` is `visibility: hidden` with `font-size: 0; line-height: 0` (`base.css:187-196`) — it is NOT `display: none`, so it still generates a box and `getBoundingClientRect()` returns a real position. Note this in a comment; it is the load-bearing reason the markers are usable as anchors.

5. **`post()`** — resolves the current `(__wolfLine, __wolfCol)` and posts `{ a: <anchor rect top or null>, s: window.scrollY }` to `window.webkit.messageHandlers.<handlerName>`.

6. **`__wolfFollow(line, col)`** — records `__wolfLine`/`__wolfCol`, resolves the anchor; if resolved, applies the edge-threshold rule (`t < th || t > vh - th` ⇒ `scrollAnchorTo(el, vh * 0.33)`); if unresolved, **does nothing** (never jump on an unresolvable anchor). Then `post()`.

7. **`__wolfRestore(line, col, targetY, fallbackScrollY, hasTarget)`** — sets `__wolfUserScrolled = false`, records line/col, resolves the anchor. If `hasTarget && el` ⇒ `scrollAnchorTo(el, targetY)`. Else if `!el` ⇒ `window.scrollTo(0, fallbackScrollY)`. Then calls `__wolfFollow(line, col)`.
   **Restore-then-follow is deliberate composition:** the restore reproduces the pre-reload viewport offset exactly (requirement 1); the follow is then a no-op if that offset was inside the comfort band, and pulls the anchor into the band if the user had typed past an edge (requirement 2). One function, both requirements, no ordering hazard.

8. **Scroll listener** — `window.addEventListener('scroll', …, { passive: true })`, rAF-throttled (a `pending` flag + `requestAnimationFrame`), setting `__wolfUserScrolled = true` and calling `post()`. This is what keeps the Swift-side cache fresh when the user wheel-scrolls independently of the cursor.

9. A final `post()` so the cache is seeded at document start.

### 5.3 `restoreJS` — the late-layout re-apply

```
(function(){
  var f = function(){ window.__wolfRestore && window.__wolfRestore(<line>,<col>,<targetY>,<fallback>,<hasTarget 0|1>); };
  f();
  window.requestAnimationFrame(function(){ if(!window.__wolfUserScrolled) f(); });
  if (document.fonts && document.fonts.ready) {
    document.fonts.ready.then(function(){ if(!window.__wolfUserScrolled) f(); });
  }
})();
```

The two late re-applies exist because `didFinish` can fire before the inlined base64 `@font-face` faces (`INTER_FONT_FACE` / `MATH_FONT_FACE`, `markdown.rs:113` / `:126`) have settled layout. The `__wolfUserScrolled` guard (cleared at the start of each `__wolfRestore`, set by the scroll listener) is what stops a late re-apply from yanking a user who scrolled in the interim.

### 5.4 New AppKit glue: `crates/wolf-macos/Sources/WolfMacOS/PreviewScrollStateHandler.swift`

An exact structural mirror of `PreviewClickFocusHandler.swift`: `final class PreviewScrollStateHandler: NSObject, WKScriptMessageHandler`, holding `let win: Int64` and `weak var container: PaneContainerView?`. Its `userContentController(_:didReceive:)` decodes `message.body as? [String: Any]` into `PreviewScrollSeam.ScrollState(anchorTop: body["a"] as? Double, scrollY: body["s"] as? Double ?? 0)` and calls `container?.recordPreviewScrollState(win:state:)`. Same retain topology (the content controller retains the handler strongly; the handler holds the container weakly).

### 5.5 `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`

**New stored state** (next to the existing preview dictionaries at `:253-310`):

- `private var lastPreviewCursors: [Int64: PreviewScrollSeam.CursorPos] = [:]` — the last cursor a `followJS` was evaluated for, per window. The `winsNeedingScroll` gate.
- `private var previewScrollStates: [Int64: PreviewScrollSeam.ScrollState] = [:]` — continuously refreshed by the page (scroll listener + every `post()`); this is the **race-safe capture cache**.
- `private var previewScrollHandlers: [Int64: PreviewScrollStateHandler] = [:]` — retained like `previewFocusHandlers` (the content controller holds it strongly; we need it for explicit removal).
- `private var pendingPreviewRestores: [Int64: PreviewScrollSeam.RestoreRequest] = [:]` — captured immediately before `loadHTMLString`, consumed in `previewDidFinishLoad`.
- `private lazy var previewScrollCoalescer = PickerRefreshCoalescer(schedule: …asyncAfter(.now() + PickerRefreshCoalescer.refreshInterval)…, perform: { [weak self] in self?.followDirtyPreviewCursors() })` — a second coalescer beside `previewReloadCoalescer` (`:319-324`), same ~30fps window.

**`syncPreviewWebViews()` (`:1187`)** — inside the existing `for preview in handle.previews()` loop, alongside the existing `current[win] = PreviewSeam.PreviewLoad(...)` at `:1213`, build a parallel

```
currentCursors[win] = PreviewScrollSeam.CursorPos(
  line: preview.source_cursor_line(), col: preview.source_cursor_col())
```

Then, after the existing reload-arming block at `:1233-1235` (**leave that block byte-identical**), add:

```
if !PreviewScrollSeam.winsNeedingScroll(current: currentCursors, applied: lastPreviewCursors).isEmpty {
  previewScrollCoalescer.request()
}
```

> **CRITICAL INVARIANT — the design-question-6 gate.** `PreviewSeam.PreviewLoad` must remain exactly `(sourceBufnr, sourceTick)`. Do **not** add cursor fields to it, and do **not** let `source_cursor_line()` / `source_cursor_col()` reach either `PreviewLoad` construction site (`:1213` and `:1275`). Because `winsNeedingReload` compares only `PreviewLoad`s, a cursor-only change is structurally incapable of arming `previewReloadCoalescer`. Add a comment at both construction sites saying so.

**`reloadDirtyPreviews()` (`:1268`)** — inside the existing top loop that reads `handle.previews()` (`:1273-1280`), also capture `currentCursors[win]`. Then, in the `for win in needing` loop, **immediately before** `web.loadHTMLString(html, baseURL: base)` (`:1301`):

```
pendingPreviewRestores[win] = PreviewScrollSeam.RestoreRequest(
  cursor: currentCursors[win] ?? PreviewScrollSeam.CursorPos(line: 0, col: 0),
  targetY: previewScrollStates[win]?.anchorTop,
  fallbackScrollY: previewScrollStates[win]?.scrollY ?? 0)
```

This is the **entire** capture step, and it is fully synchronous — see §5.7.

**`ensurePreviewWebView(for:)` (`:1308`)** — before creating the `WKWebView`, additionally:
- `let scrollProxy = PreviewScrollStateHandler(win: win, container: self)`; `previewScrollHandlers[win] = scrollProxy`; `config.userContentController.add(scrollProxy, name: PreviewScrollSeam.stateHandlerName)`.
- `config.userContentController.addUserScript(WKUserScript(source: PreviewScrollSeam.stateScriptJS(), injectionTime: .atDocumentStart, forMainFrameOnly: true))`.
Exactly parallel to the existing click-focus wiring at `:1320-1328`.

**`removePreviewWebView(_:)` (`:1356`)** — also `removeScriptMessageHandler(forName: PreviewScrollSeam.stateHandlerName)` on the still-live content controller, and clear `previewScrollHandlers[win]`, `previewScrollStates[win]`, `lastPreviewCursors[win]`, `pendingPreviewRestores[win]`.

**New `func recordPreviewScrollState(win: Int64, state: PreviewScrollSeam.ScrollState)`** — `previewScrollStates[win] = state`. Nothing else; no repaint, no wake.

**New `private func followDirtyPreviewCursors()`** — the `previewScrollCoalescer`'s `perform`, mirroring `reloadDirtyPreviews`' shape:
- Re-read `handle.previews()` into a fresh `current: [Int64: CursorPos]`.
- `for win in PreviewScrollSeam.winsNeedingScroll(current: current, applied: lastPreviewCursors)`:
  - `guard let web = previewWebViews[win], let c = current[win], c.isKnown, previewLoadedWins.contains(win) else { continue }` — **skip WITHOUT updating `lastPreviewCursors`**, so the next sync re-requests once the web view exists and has loaded. Same documented skip semantics as `reloadDirtyPreviews` (`:1283-1285`).
  - `lastPreviewCursors[win] = c`
  - `web.evaluateJavaScript(PreviewScrollSeam.followJS(line: c.line, col: c.col), completionHandler: nil)`

**`previewDidFinishLoad(win:)` (`:1446`)** — rewrite the body to this exact order:
1. `previewLoadedWins.insert(win)` (unchanged, must stay first).
2. If `pendingPreviewFragments.removeValue(forKey: win)` is non-nil: **also `pendingPreviewRestores[win] = nil`**, evaluate `PreviewLinkSeam.scrollIntoViewJS(...)`, `return`. A cross-file `other.md#anchor` navigation is an intentional jump — the restore must be dropped, not fought.
3. Else if `pendingPreviewRestores.removeValue(forKey: win)` is non-nil: evaluate `PreviewScrollSeam.restoreJS(request)`, and set `lastPreviewCursors[win] = request.cursor` (the restore already applied the follow, so the coalescer must not immediately re-apply it).
4. Else (a first-ever load with no pending restore): if a known cursor exists for `win` in a fresh `handle.previews()` read, evaluate `followJS` and record it in `lastPreviewCursors`. This is what makes the very first paint land at the cursor.

**`viewDidChangeEffectiveAppearance()` (`:1254`)** — unchanged. It clears `lastPreviewLoads` to force a reload; the reload path now captures/restores automatically, so a dark↔light flip no longer jumps to the top either (a free bonus worth noting in the doc comment).

### 5.6 Files to add to `wolf_key_map`'s `srcs`

`crates/wolf-macos/BUILD.bazel`, the `swift_library(name = "wolf_key_map")` `srcs` list (`:50-90`) — insert `"Sources/WolfKeyMap/PreviewScrollSeam.swift"` **between** `PreviewLinkSeam.swift` (`:76`) and `PreviewSeam.swift` (`:77`) to keep it alphabetical. `Sources/WolfMacOS/*.swift` is a **glob** (`:100`), so `PreviewScrollStateHandler.swift` needs no BUILD edit — but `swift_format_srcs` (`:33-41`) is also a glob, so lint picks both up automatically.

### 5.7 Design question 5 — the capture race, concretely

**The problem.** `loadHTMLString` is effectively synchronous from the caller's view, but `evaluateJavaScript` is asynchronous with a completion handler. There is no way to synchronously ask the page "where is the cursor anchor right now?" immediately before replacing the document. And a full reload **destroys `window`**, so nothing can be stashed in the page itself.

**The design.** Keep the state **in Swift**, continuously refreshed by the page pushing it out over a `WKScriptMessageHandler` — never pulled. `previewScrollStates[win]` is written by:
- every `__wolfFollow` (i.e. every coalesced cursor move — which, in insert mode, is every keystroke), and
- every rAF-throttled `scroll` event (i.e. every user wheel/trackpad scroll), and
- the document-start seed `post()`.

At reload time the capture is a pure dictionary read: **zero async work, zero blocking, no race with `loadHTMLString`.**

**Failure modes, named:**

| Failure mode | Window | Mitigation / verdict |
|---|---|---|
| Cache is stale because the user wheel-scrolled in the last frame before the reload | ≤ 1 rAF (~16ms) | Restores a few px off. Accepted. |
| Cache is stale because an edit did NOT move the cursor (`r` replace-char) | until the next cursor move | The cached anchor is still on the SAME line, so the restore is still correct to within the line. Accepted. |
| `window` destroyed by the reload | always | Exactly why the cache is Swift-side. Not a failure. |
| Anchor missing in the NEW document (the line was deleted) | rare | `resolve()`'s fallback chain: exact-col → line-0 → nearest preceding → raw `scrollY`. |
| `didFinish` fires before web fonts settle layout | ~1 frame + font decode | The rAF + `document.fonts.ready` re-applies in `restoreJS`, guarded by `__wolfUserScrolled`. |
| Two reloads land back-to-back within one debounce window | never | The reload coalescer already serializes to one trailing reload per ~33ms; `pendingPreviewRestores[win]` is overwritten, so only the latest capture survives. Correct. |
| A `followJS` is evaluated against the OLD document just before the reload replaces it | ≤ 33ms | Harmless — the restore path re-applies the follow after `didFinish`. |
| The message handler fires after the web view was removed | any | `PreviewScrollStateHandler` holds the container weakly and keys by `win`; `removePreviewWebView` removes the handler by name and clears the dicts. A late message writes a dict entry for a dead win, which the next `syncPreviewWebViews` prune removes. Benign. |

**Rejected alternative:** persisting the anchor in the JS `window` and reading it back after reload — impossible, the window object does not survive a full load. **Rejected alternative:** `evaluateJavaScript` with a completion handler that then triggers the reload — this serializes a blocking `render_preview` RPC behind an async JS round-trip on every keystroke burst, and inverts the existing coalescer's structure for no benefit.

---

## 6. Test plan

### 6.1 Rust real-nvim integration test

**New file:** `/Users/jplein/Git/jplein/wolf/crates/wolf-core/tests/markdown_cursor.rs`

Harness: `mod common;` + `use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT, start_core_with_temp_md, start_core_sized, temp_md, DEFAULT_COLS, DEFAULT_ROWS};` — modeled directly on `tests/current_file.rs` (poll `read_snapshot()` with a deadline, `waiter.drain()` each iteration, `POLL_INTERVAL = 25ms`, loud panic on timeout with the last observed value). Same loud-SKIP-when-no-nvim posture as its siblings.

Add a shared helper `wait_for_cursor(core, waiter, predicate) -> (i64, MarkdownCursor)` that polls `read_snapshot().markdown_cursors` until the predicate holds.

**Tests:**

1. **`cursor_reports_one_based_line_and_zero_based_char_col`** — open a temp `.md` with a few plain-paragraph lines. Poll until `markdown_cursors` is non-empty; assert exactly **one** entry (only one markdown buffer exists). Send `j` then `l` keystrokes; assert `line` advances by 1 and `col` becomes 1. Pins the 1-based line / 0-based column contract from the ground truth.
2. **`multibyte_line_reports_character_column_not_byte_column`** — **the load-bearing OD4 test.** Content: a line of three `é` (each 2 bytes UTF-8) followed by `X`, e.g. `"# T\n\néééX\n"`. Navigate to that line, `0` to column 0, then three `l` (normal-mode `l` moves by CHARACTER). Assert `col == 3` **and explicitly assert `col != 6`** with a message naming the byte column, so a regression to `col('.')` fails loudly with a diagnostic rather than an off-by-N.
3. **`cursor_updates_without_an_edit`** — record `markdown_ticks` for the buffer, then send a pure normal-mode motion (`j`). Assert `markdown_cursors` changed while `markdown_ticks` did NOT. Proves the cursor signal is independent of the dirty signal.
4. **`non_markdown_buffer_reports_no_cursor`** — from a started core, type a raw `:enew<CR>` (as `current_file.rs:118-120` does), move the cursor, and assert **no new key** appears in `markdown_cursors` (snapshot the key set before/after). Covers the filetype guard.
5. **`preview_scratch_buffer_reports_no_cursor`** — open a temp `.md`, call `core.open_preview("split_right")` (public at `api.rs:659`), wait for `snapshot.previews` to become non-empty, then `:wincmd w` into the preview pane (with a `CYCLE_SETTLE`-style sleep) and send motions. Assert `markdown_cursors` gains **no** key equal to the preview scratch bufnr (`open_preview`'s return value) — only the source bufnr is ever a key. Covers the `b:wolf_preview_source` guard.
6. **`cursor_and_tick_share_the_source_bufnr_key`** — type a character to fire `wolf_markdown_dirty`, then assert `markdown_ticks` and `markdown_cursors` have the **same** key set for that buffer. Pins the "both keyed by SOURCE bufnr" contract the FFI join depends on.

**Explicitly NOT tested:** `seq` invariance (see §0 deviation 1).

**Bazel wiring — `crates/wolf-core/BUILD.bazel`.** Add a `rust_test` that is a byte-for-byte copy of the `current_file` stanza (`:1087-1108`) with the names swapped:

```
rust_test(
    name = "markdown_cursor",
    srcs = ["tests/common/mod.rs", "tests/markdown_cursor.rs"],
    crate_root = "tests/markdown_cursor.rs",
    tags = ["no-sandbox"],          # a real nvim child cannot spawn under the macOS sandbox
    size = "small",
    edition = "2021",
    data = ["//bazel/nvim:nvim_tree", "//runtime/nvim:init.lua"],
    env = {
        "WOLF_NVIM_BIN": "$(rootpath //bazel/nvim:nvim_tree)/nvim/bin/nvim",
        "WOLF_TEST_INIT_LUA": "$(rootpath //runtime/nvim:init.lua)",
        "WOLF_TEST_NVIM_DIR": "$(rootpath //bazel/nvim:nvim_tree)",
    },
    deps = [":wolf_core"] + all_crate_deps(normal = True, normal_dev = True),
)
```

Place it beside the other real-nvim preview tests, with a comment block matching the house style. **Verify with `bazelisk test //crates/wolf-core:markdown_cursor` that it actually RUNS** (confirm it appears in the test output, not just that the build passed).

### 6.2 Swift seam unit tests

**New file:** `/Users/jplein/Git/jplein/wolf/crates/wolf-macos/Tests/WolfKeyMapTests/PreviewScrollSeamTests.swift`, `import XCTest` + `@testable import WolfKeyMap` (match the sibling `PreviewSeamTests.swift`'s import style).

Behaviors to cover:

- **`anchorColumn` rounding:** 0→0, 1→0, 99→0, 100→100, 101→100, 199→100, 200→200, 250→200.
- **`anchorIds` ordering + dedup:** `(line: 7, col: 0)` → exactly `["wl-7-0"]` (one element, not two); `(7, 250)` → `["wl-7-200", "wl-7-0"]` in that order; `(7, 100)` → `["wl-7-100", "wl-7-0"]`; `(line: 0, col: 5)` → `[]` (unknown cursor yields no candidates).
- **`edgeThreshold`:** `1000` → `150` (fraction wins, under the cap); `200` → `48` (the 48px floor wins over `30`, and is under the `70` cap); `100` → `35` (the `0.35` cap wins over the 48px floor — the short-pane case); `0` → `0`; and the invariant `0 <= edgeThreshold(v) <= v * 0.35` for a spread of heights.
- **`followDecision`:** anchor comfortably mid-viewport → `.stay`; anchor above the top (e.g. `-50`) → `.scroll(toViewportY: vh * 0.33)`; anchor below the bottom → `.scroll(...)`; anchor exactly AT `threshold` → `.stay`; anchor exactly at `vh - threshold` → `.stay` (**pins the inclusive boundary convention**).
- **`winsNeedingScroll`:** first bind (win absent from `applied`) → returned; equal cursor → not returned; changed `line` → returned; changed `col` (same line) → returned; a win in `applied` but absent from `current` → never returned; empty `current` → empty result.
- **`cursorOnlyChangeDoesNotArmReload`** (the design-question-6 regression guard): build two identical `PreviewSeam.PreviewLoad`s for the same win, assert `PreviewSeam.winsNeedingReload(current:loaded:)` is **empty**, while `PreviewScrollSeam.winsNeedingScroll` for a *changed* cursor on that same win is **non-empty**. Documents in a test that the two gates are independent.
- **`stateHandlerName` distinctness:** `PreviewScrollSeam.stateHandlerName == "wolfPreviewScroll"` and `!= PreviewSeam.clickFocusHandlerName` (they share one `WKUserContentController`).
- **`followJS` shape:** contains `__wolfFollow`, contains the literal `(7,200)` for `(7, 200)`, contains no `,` decimal separator artifact.
- **`restoreJS` shape:** with `targetY: nil` still emits a call and carries the `hasTarget` flag `0` plus the fallback scrollY; with `targetY: 123.5` contains `123.50`; contains `requestAnimationFrame`; contains `__wolfUserScrolled` (the late-re-apply guard).
- **`stateScriptJS` shape:** contains the handler name, `getElementById`, `'wl-'`, `requestAnimationFrame`, `passive`, `scrollTo`, `data-col`, and **does NOT contain `behavior`** (no smooth scrolling).
- **Locale safety:** format a `Double` through the seam and assert the output contains `.` and not `,` (guards against a `String(format:)` locale regression).

**Bazel wiring — `crates/wolf-macos/BUILD.bazel`, `swift_test(name = "WolfKeyMapTests")` `srcs` (`:246-287`).** Insert `"Tests/WolfKeyMapTests/PreviewScrollSeamTests.swift"` **between** `PreviewLinkSeamTests.swift` (`:273`) and `PreviewSeamTests.swift` (`:274`). This list is hand-maintained and is **NOT a glob** — an unwired file compiles nowhere and runs never. `swift_format_srcs` (`:38`) IS a glob and will pick the file up for lint, which can make it *look* wired while being unrun; do not be fooled. **Verify with `bazelisk test //crates/wolf-macos:WolfKeyMapTests` that the new test methods appear in the output.**

### 6.3 Not automatable

The actual on-screen scroll behavior (no jump-to-top on edit; no twitch when typing mid-viewport; cursor-follow at the edges) is a **manual smoke test** — there is no headless window server, and `PaneContainerView` / `PreviewScrollStateHandler` / `PreviewNavigationDelegate` are compile-verified + manual-smoke only, per this crate's stated posture. Add an eyeball checklist to `crates/wolf-macos/README.md`: (1) open a `.md` + its preview side by side, scroll the preview to the middle, type in the editor — the preview must not move; (2) hold `j` to walk the cursor to the bottom of the preview's viewport — the preview scrolls once the anchor crosses the edge band and then stays put for several more lines; (3) `G` to the end of a long document — the preview jumps and lands the end one third down; (4) wheel-scroll the preview, then type — the preview restores to where you scrolled it, not to the cursor, unless the cursor is off the edge; (5) click a cross-file link with a `#anchor` — the anchor jump still wins over the restore; (6) toggle OS dark/light — the preview re-renders in place without jumping to the top.

---

## 7. Documentation (docs are the source of truth here — all of these are REQUIRED)

| File | Change |
|---|---|
| `/Users/jplein/Git/jplein/wolf/architecture.md` | Line **261** — the "Markdown-preview scroll-sync (cursor-follow) is DESIGNED but NOT built" bullet under "## Deferred / open". Rewrite: scroll-sync IS built; describe the `wolf_markdown_cursor` signal, `Snapshot::markdown_cursors`, the `FfiPreview` cursor accessors, `PreviewScrollSeam`, the anchor-resolution chain, the capture/restore flow and the edge-threshold rule with its concrete constants. Keep the accepted v1 marker-collapse-granularity paragraph — that limitation is unchanged and now MORE user-visible (a cursor anywhere inside a wrapped paragraph anchors to the paragraph top). |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-core/README.md` | Line **158** — "The marker/anchor scheme (built now; JS + cursor signal DEFERRED)" → retitle and rewrite the forward-compat sentence to describe the now-built signal. Line **164** — the `Snapshot` field inventory: add `markdown_cursors: HashMap<i64, MarkdownCursor>` and the `MarkdownCursor { line, col }` type beside `PreviewInfo`. |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-core/src/README.md` | Line **93** area — add a `markdown_cursors` bullet after the `markdown_ticks` one. Line **98** — add `markdown_cursors` to the "does NOT bump `seq`" list. Line **171** / **191** — the `redraw.rs` notification/handler inventory: add `MARKDOWN_CURSOR_EVENT` / `apply_markdown_cursor`. Line **465** area — the `render.rs` type inventory: add `MarkdownCursor`. |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-core/src/markdown.rs` | Module docs **lines 10 and 50-58**: retitle "## The marker/anchor scheme (built now; JS + cursor signal deferred)" and replace the "## Forward-compat contract (designed, NOT built here)" section with a "## The cursor signal + scroll JS (now built elsewhere)" section pointing at `runtime/nvim/init.lua`'s `wolf_markdown_cursor`, `crate::redraw`, and `WolfKeyMap.PreviewScrollSeam`. **Correct the stale claim** at `:53-54` that the follow-up "must convert nvim's BYTE column to a CHARACTER column" — it does not; `charcol()` yields the character column directly. |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-core/src/markdown/base.css` | Header comment **lines 7-9** ("the deferred scroll-sync JS will getElementById…") and **:184-186** ("purely an anchor target for a future cursor-follow scroll") — drop "deferred"/"future"; name `PreviewScrollSeam` as the live consumer and note that `visibility: hidden` (not `display: none`) is what keeps `getBoundingClientRect()` usable. |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-core/src/markdown/README.md` | Line **37** — same de-deferring edit. |
| `/Users/jplein/Git/jplein/wolf/runtime/nvim/README.md` | Section "**`_G.wolf_setup_preview(chan)`** installs the `WolfPreview` augroup" (**:249-254**) — add a third bullet for the `CursorMoved`/`CursorMovedI` autocmd: the two guards (markdown filetype AND `b:wolf_preview_source == nil`, and note the explicit-preview-guard asymmetry with the dirty autocmd), the `charcol('.') - 1` 0-based-CHARACTER-column contract, the flat 3-scalar payload, the `_G.wolf_emit_markdown_cursor` export, the initial emit, and the "no seq bump; wakes the front-end" sentence at `:254`. Also update the `_G.wolf_open_preview` section (**:229-233**) to mention the pre-placement cursor re-emit and WHY it must precede the bind. |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-ffi/README.md` | The "Generated bindings" paragraph at **:252** — append a sentence in the established style: the two ADDITIVE `FfiPreview` accessors are regen-only, touching just `wolf-ffi.swift` / `wolf-ffi.h`, riding the existing editor render wake, so `wolf_ffi_wake.h` stays **byte-identical**. |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-ffi/src/README.md` | Add the two accessors to the `FfiPreview` entry. |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-macos/README.md` | Line **161** — replace "**Scroll-sync (cursor-follow) is DESIGNED but not built** — the emitted HTML already carries the position markers, but the Neovim→web-view cursor signal + the scrolling JS are a deferred follow-up" with the built description + the eyeball checklist from §6.3, and the note that the AppKit glue is compile+manual-smoke while `PreviewScrollSeam` is headlessly tested. |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-macos/Sources/README.md` | Add a `PreviewScrollSeam.swift` bullet (between the `PreviewSeam.swift` bullet at **:35** and the `PreviewLinkSeam.swift` bullet at **:37**) in the same exhaustive house style, and a `PreviewScrollStateHandler.swift` bullet beside the `PreviewClickFocusHandler.swift` one. |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-macos/Tests/README.md` | Add a `PreviewScrollSeamTests` entry beside `PreviewSeamTests` / `PreviewLinkSeamTests` (they appear at **:3**, **:106**, **:114**, **:154**, **:160** — check each list). |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-core/tests/README.md` | Add a `markdown_cursor.rs` entry to the test-file inventory (the `current_file.rs` entries at **:63**, **:72**, **:117** show the shape and which lists exist). |
| `/Users/jplein/Git/jplein/wolf/crates/wolf-gnome/README.md` | **State explicitly** (a one-sentence note in its known-limitations section) that the GTK shell has no Markdown preview pane at all, so preview scroll-sync is macOS-only. This is the "docs are the source of truth" obligation for design question 7. |

Reformat every edited Markdown file with `prettier --write`; no hard line breaks inside a paragraph.

---

## 8. Implementation order (avoids compilation breakage at every step)

1. **`crates/wolf-core/src/render.rs`** — add `MarkdownCursor` + `Snapshot::markdown_cursors` (+ crate-root re-export). Compiles alone; `wolf-gnome` unaffected (additive `Default` field).
2. **`crates/wolf-core/src/redraw.rs`** — `MARKDOWN_CURSOR_EVENT`, the dispatch arm, `apply_markdown_cursor`. Compiles against step 1.
3. **`runtime/nvim/init.lua`** — the `emit_cursor` closure, the `CursorMoved`/`CursorMovedI` autocmd, `_G.wolf_emit_markdown_cursor`, the initial emit, and the `wolf_open_preview` pre-placement re-emit. No Rust dependency; the core handler from step 2 already accepts it.
4. **`crates/wolf-core/src/nvim.rs`** — doc-comment-only update on `setup_preview()`.
5. **Rust test + Bazel wiring** — `crates/wolf-core/tests/markdown_cursor.rs` + the `rust_test` stanza. **Run it now** (`bazelisk test //crates/wolf-core:markdown_cursor`) — this proves the Lua→core half end-to-end before any FFI or Swift work, and isolates any `charcol` surprise to a two-layer surface.
6. **`crates/wolf-ffi/src/lib.rs`** — the two accessors, struct fields, `from_preview` signature, `previews()` join, bridge declarations. Then `bazel run //crates/wolf-ffi:gen_bindings` and commit `generated/`.
7. **`crates/wolf-macos/Sources/WolfKeyMap/PreviewScrollSeam.swift`** + add it to `wolf_key_map`'s `srcs`. Pure Foundation; compiles with zero dependency on step 6.
8. **Swift seam tests + Bazel wiring** — `PreviewScrollSeamTests.swift` + the `WolfKeyMapTests` `srcs` entry. **Run it now** (`bazelisk test //crates/wolf-macos:WolfKeyMapTests`) and confirm the new methods appear in the output.
9. **`crates/wolf-macos/Sources/WolfMacOS/PreviewScrollStateHandler.swift`** — needs steps 7 + the `PaneContainerView` method from step 10, so land it together with 10 (or stub `recordPreviewScrollState` first).
10. **`crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`** — the new state, the two coalescer arms, the capture in `reloadDirtyPreviews`, `ensure`/`remove` wiring, `recordPreviewScrollState`, `followDirtyPreviewCursors`, the rewritten `previewDidFinishLoad`. Depends on steps 6–9.
11. **Full build + test sweep** — `bazelisk build //crates/wolf-macos:Wolf` and `bazelisk test //crates/wolf-core/... //crates/wolf-ffi/... //crates/wolf-macos:WolfKeyMapTests`.
12. **Docs** (§7), then `prettier --write` on each edited `.md`.
13. **Lint** — `bazel run //tools/lint:fix` then `bazel test //tools/lint/...`.

---

## 9. Dependencies

**None.** No new Rust crates, no new Swift packages, no new Lua modules. `charcol()` is a built-in Vimscript function available in the pinned Neovim 0.12.3; `WKScriptMessageHandler` / `WKUserScript` are already used by the click-to-focus path; `PickerRefreshCoalescer` already exists.

---

## 10. Lint considerations

**Rust (clippy, advisory via `//tools/lint:clippy`):**
- Every new `pub fn` / `pub` accessor needs `#[must_use]` — the house style is uniform on this.
- `u64 → u32` in `apply_markdown_cursor` must NOT be a bare `as` cast (`clippy::cast_possible_truncation`). Use `u32::try_from(v).unwrap_or(u32::MAX)`.
- `apply_markdown_cursor` mirrors `apply_markdown_dirty` exactly: no filesystem work, no `.await` while the guard is held, no `MutexGuard` as a `for`/`match` scrutinee — so `clippy::await_holding_lock` and `clippy::significant_drop_in_scrutinee` do not fire. **No `#[allow]` should be needed anywhere in this change.**
- `MarkdownCursor` should derive `Clone, Copy, Debug, Default, PartialEq, Eq` (matching `PreviewInfo`'s derive set minus `Clone`'s necessity) and carry `///` docs on both fields — `missing_docs` is enforced on this crate's public API by convention.

**Swift (`swift-format lint --strict`, default `lineLength: 100`):**
- The JS in `stateScriptJS` is the risk. Inside a `"""` multiline literal, **every physical line must stay under ~95 columns**. Keep the JS broken across many short lines (the existing `PreviewSeam.clickFocusJS` is the model). Do **not** try to emit the whole script as one long concatenated single-line string.
- `restoreJS` / `followJS` are short enough for single-line construction, but prefer building them from a few `+`-joined fragments the way `PreviewLinkSeam.scrollIntoViewJS` does.
- `PreviewScrollSeamTests.swift` is picked up by the `swift_format_srcs` glob automatically — no BUILD edit for lint, but **the `WolfKeyMapTests` `srcs` edit is still mandatory for execution**.
- No `// swift-format-ignore` directives should be needed.

**Markdown:** `prettier --write` after every `.md` edit; no hard line breaks inside a paragraph (the existing READMEs are single-giant-line paragraphs — preserve that).

---

## 11. Risks

1. **`charcol()` edge semantics in insert mode past EOL** — returns `char_len + 1`, so the reported col can exceed the last character index. Benign (the JS rounds down and falls back), but the Rust test should include an insert-mode assertion so this is pinned rather than assumed.
2. **Marker collapse granularity (pre-existing, now more visible).** Every line inside a wrapped paragraph / fenced block / list container resolves to that container's TOP (`markdown.rs:38-48`). Cursor-follow inside a long paragraph will therefore feel "sticky". This is the accepted v1 granularity — do not attempt to fix it here, but **do** call it out in `architecture.md` as the primary known limitation of the now-built feature.
3. **Two script-message handlers on one content controller.** `wolfPaneFocus` and `wolfPreviewScroll` must both be registered AND both removed in `removePreviewWebView`; missing the second removal leaks the proxy (the controller retains it strongly).
4. **Order-of-operations in `previewDidFinishLoad`.** `previewLoadedWins.insert(win)` must stay FIRST (the occlusion test depends on it), and the fragment branch must DROP the pending restore rather than fall through — otherwise a cross-file `#anchor` link jump gets immediately undone.
5. **Constant drift between Swift and JS.** `48` / `0.15` / `0.35` / `0.33` / `100` / `200` exist in both `PreviewScrollSeam`'s constants and the JS string. Cross-reference them in comments on both sides; the Swift constants are what the tests pin, and the JS is what actually runs — a silent divergence would make the tests green while the behavior is wrong. Consider interpolating the Swift constants into the JS string instead of duplicating literals; that removes the risk entirely at the cost of slightly longer `stateScriptJS` lines (watch the 100-column lint limit).
6. **`cargo test` on Linux/Nix will auto-discover `markdown_cursor.rs`** while Bazel will not — so an unwired Bazel target is a macOS-only silent gap. The wiring in §6.1 is mandatory, and must be confirmed by observing the target in `bazelisk test` output.

---

## 12. Test-writer contract notes (BINDING on the coder)

The test-writer has landed the tests. These are the judgment calls it made; the
implementation MUST match them, or fix the test with an explicit justification routed
back through the orchestrator.

Files created: `crates/wolf-core/tests/markdown_cursor.rs`,
`crates/wolf-macos/Tests/WolfKeyMapTests/PreviewScrollSeamTests.swift`.
Files modified (wiring only): `crates/wolf-core/BUILD.bazel` (new `rust_test` target
`markdown_cursor`), `crates/wolf-macos/BUILD.bazel` (added
`Sources/WolfKeyMap/PreviewScrollSeam.swift` to `wolf_key_map`'s `srcs` AND
`Tests/WolfKeyMapTests/PreviewScrollSeamTests.swift` to `WolfKeyMapTests`' `srcs`).
`PreviewScrollSeam.swift` itself does NOT exist yet — its `srcs` slot is already wired
and waiting for the coder.

1. **§6.1/1's "assert exactly ONE entry in `markdown_cursors`" was WRONG and is not
   implemented.** The startup buffer is a markdown `Untitled.md` (markdown filetype, no
   `b:wolf_preview_source`) and `setup_preview()` runs during `WolfCore::start`, so the
   one-shot initial emit records it; `InputEvent::Open` is `:edit <path>`, which leaves
   it listed. Two entries is CORRECT. The test resolves the opened file's bufnr from
   `Snapshot::win_buffers` by canonicalized path and asserts on that entry.
2. Likewise §6.1/6 asserts tick keys are a **subset** of cursor keys, not equal — an
   unedited markdown buffer has a cursor but no tick.
3. **§6.1/5's `:wincmd w` was wrong**: `wolf_open_preview("split_right")` already leaves
   the preview pane current. The test waits on `is_preview_active()`, drives motions in
   the preview pane, then `:wincmd w` back to prove the pipeline is still live. It also
   `:setlocal modifiable` + `:setlocal filetype=markdown` on the scratch so the test
   exercises the EXPLICIT `b:wolf_preview_source` guard rather than passing incidentally
   on the scratch's empty filetype. **This makes the explicit guard load-bearing — the
   coder must implement it, not rely on the empty filetype.**
4. **`Double` formatting in generated JS is pinned to `String(format: "%.2f", v)`.**
   Tests assert `48.00`, `0.15`, `0.35`, `0.33`, `123.50`, `55.00`, `1234.50`.
   `String(v)` (→ `"48.0"`) FAILS.
5. **Argument lists carry no spaces after commas.** `followJS` must emit `(7,200)`;
   `restoreJS` must end its call `,0)` / `,1)`.
6. **`restoreJS` passes the RAW column**, not `anchorColumn(col)` — the JS does the
   rounding. A cursor at column 250 puts `250` in the JS.
7. **`restoreJS` with `targetY: nil` still emits the full fixed 5-argument call** (the
   target placeholder rendered as `0.00`) with the `hasTarget` flag `0`.
8. `CursorPos.isKnown` is tested and must exist. `followJS` must keep the `&&`
   short-circuit guard on `window.__wolfFollow`.

---

## 13. Implementation landed — coder deviations (ACCEPTED) and round-2 test work

Implementation is complete and green: `//crates/wolf-core:markdown_cursor` ran 7 tests
(all pass, stable over `--runs_per_test=5`); `//crates/wolf-macos:WolfKeyMapTests` grew
317 → 345 tests with the 28 `PreviewScrollSeamTests` methods observed executing; the full
sweep (`wolf-core/...`, `wolf-ffi/...`, `WolfKeyMapTests`, clippy/rustfmt/swift_format)
is 87/87. `wolf_ffi_wake.h` verified byte-identical; the FFI regen is purely additive.

### Accepted deviations

- **DEV-1 — the cursor autocmd also fires on `BufEnter` / `BufWinEnter`**, not just
  `CursorMoved` / `CursorMovedI`. ACCEPTED: on `:edit foo.md` the `CursorMoved` fires
  BEFORE filetype detection (`ft == ""`, so the markdown guard bails) and no further
  `CursorMoved` fires until the user moves — so a markdown buffer that is opened and
  merely looked at would report NO cursor, leaving a bound preview at
  `source_cursor_line() == 0` forever. This is a real feature gap, not a test artifact.
- **DEV-2 — a `window.__wolfProgrammatic` suppression flag was added to the injected JS.**
  ACCEPTED: as specified in §5.2/8 + §5.3 the guard is self-defeating — `__wolfRestore`'s
  own programmatic `scrollTo` fires a `scroll` event whose rAF callback sets
  `__wolfUserScrolled`, so the rAF and `document.fonts.ready` re-applies would ALWAYS be
  skipped, killing the exact font-settling mitigation they exist for. The flag is set
  before every programmatic `scrollTo` and consumed by the next throttled scroll callback
  without setting `__wolfUserScrolled`. Genuine user scrolls still set the flag.
- **DEV-3 — 32 unrelated files reformatted by `bazel run //tools/lint:fix` were reverted**
  (18 wolf-core test files, 8 wolf-ffi test files, `examples/grid_dump.rs`,
  `AppearanceSeam.swift`, 5 existing `WolfKeyMapTests` files). ACCEPTED: the fixer rewrites
  a broader set than the check targets verify; all check targets pass with them reverted.

### Round-2 test additions (ORCHESTRATOR DECISION: all three APPROVED)

Each pins a property that is currently unpinned, and two of them pin the accepted
deviations above so they cannot be silently re-broken.

- **T-1 — `crates/wolf-ffi/tests/preview.rs`** (new): the FFI join is the one layer of
  this feature covered only by compilation. `preview_carries_the_source_buffers_cursor`
  (open a `.md`, open a preview, move the cursor, assert `previews()[0]`'s
  `source_cursor_line()` / `source_cursor_col()` match `Snapshot::markdown_cursors` for
  that `source_bufnr`) and `preview_cursor_defaults_to_zero_when_unreported` (pins the
  `line == 0` "unknown" sentinel produced by `unwrap_or_default()`).
- **T-2 — `crates/wolf-core/tests/markdown_cursor.rs`**: add
  `cursor_reported_on_buffer_entry_without_any_motion` — open a `.md` and assert a cursor
  appears with ZERO keystrokes sent. Directly pins DEV-1.
- **T-3 — `PreviewScrollSeamTests.swift`**: assert `stateScriptJS` contains the
  `__wolfProgrammatic` machinery. Pins DEV-2.

### Noted, not actioned

- `//tools/lint:fix` reflowed one chained expression in `markdown_cursor.rs`'s
  `cursor_keys`. Cosmetic, test still passes, left as the repo's own fixer produced it.
- **`//crates/wolf-core:redraw_grid` is PRE-EXISTING flaky**, not a regression:
  `scroll_up_shifts_grid_content_down` measured 4/20 failures with `init.lua` stashed to
  HEAD vs 2/20 with this change. Out of scope here; worth someone owning separately.

---

## 14. Round-2 results — DEV-4 (a real bug the new tests caught)

All five round-2 tests executed and are green after one implementation fix.
`//crates/wolf-ffi:preview` 7 → 9 tests, `//crates/wolf-core:markdown_cursor` 7 → 8,
`//crates/wolf-macos:WolfKeyMapTests` 345 → 347. Full sweep 87/87 including all lint
check targets. Stability re-run `--runs_per_test=3` on the Lua-affected targets: 12/12.

**DEV-4 — `emit_cursor` now also guards on `vim.fn.win_gettype() ~= ""`.**
`preview_cursor_defaults_to_zero_when_unreported` FAILED on first run, exposing a real
defect in DEV-1: `vim.fn.bufload()` fires `BufEnter` + `BufWinEnter` inside nvim's
synthetic **autocmd window**, with the target buffer genuinely current — so the
`buf ~= nvim_get_current_buf()`, filetype and scratch guards ALL pass, and DEV-1's entry
events fabricated a `(1, 0)` cursor for a buffer whose cursor is genuinely unknown, read
from a synthetic window belonging to no user pane. Every link-navigated preview would
have reported a bogus top-of-document anchor instead of the `isKnown == false` sentinel
that tells the front-end to leave a freshly bound preview alone — and it would have
fought the `pendingPreviewFragments` `#anchor` jump. Only a NORMAL window
(`win_gettype() == ""`) carries a real cursor; the new guard also rejects floating/popup,
command-line, quickfix and loclist windows. `nvim.rs`'s `setup_preview()` doc comment
records the guard and the measured `bufload` rationale.

**This is a load-bearing constraint on DEV-1 that a future refactor could easily drop.**

### Round-3 test addition (ORCHESTRATOR DECISION: APPROVED)

- **T-4 — `crates/wolf-core/tests/markdown_cursor.rs`**: add
  `background_bufload_of_a_markdown_file_reports_no_cursor`. DEV-4 is currently pinned
  only indirectly, by an FFI-layer test going through `preview_file`. Pin it at the
  Lua↔core seam where the guard actually lives: `:badd other.md` + `bufload()` on a
  SECOND markdown file, assert no key for that bufnr ever appears in
  `Snapshot::markdown_cursors` while the focused buffer's entry is unchanged. The test
  comment must record the measured fact that `bufload` fires `BufEnter`/`BufWinEnter` in
  the autocmd window — that is the non-obvious part.

### Still outstanding

- **Plan §7 documentation has NOT landed** — deliberately deferred to the technical-writer
  phase. `architecture.md:261` still reads "DESIGNED but NOT built".

---

## 15. Review findings — REVISION REQUIRED

The reviewer confirmed full plan conformance (D1, D3/§5.5, anchor resolution, §5.3
composition, DEV-4 guard placement and all five `emit_cursor` guards, §11 risks 3 and 4,
the end-to-end `charcol()` character-column contract, and all Bazel wiring), and found
three behavioral defects in the Swift capture/restore layer. Two block shipping.

### H1 (must fix) — a cross-file link with NO `#fragment` restores the OLD document's scroll

`PaneContainerView.swift:1571-1585` + `:1377-1380`. `previewFile(win:path:fragment:)`
takes `fragment: String?`. With `nil` — a plain `[see](other.md)` link, very common —
nothing is stashed in `pendingPreviewFragments`, so `previewDidFinishLoad` skips the drop
branch and takes the RESTORE branch, while `previewScrollStates[win]` still holds the
PREVIOUS file's state and `reloadDirtyPreviews` captures it unconditionally even though
`PreviewLoad.sourceBufnr` just changed.

Failure: reading a long `guide.md` scrolled to y≈3000, click `[appendix](appendix.md)`.
`wolf_preview_file` background-`bufload`s it so DEV-4 correctly leaves its cursor unknown
(`line == 0`); `resolve(0, 0)` returns null, the `else` branch runs `moveTo(fallbackY)` =
**3000**, and `appendix.md` opens 3000px into its middle. **This is a REGRESSION** — before
this change `loadHTMLString` reset scroll to 0 and a new file opened at the top.

### H2 (must fix) — the document-start `post()` clobbers an in-flight restore, and the ORIGINAL BUG RETURNS

`PreviewScrollSeam.swift:369` → `PreviewScrollStateHandler.swift:54-58` →
`PaneContainerView.swift:1459-1461` (unconditional overwrite) → consumed at `:1377-1380`.

At document start `__wolfLine == 0`, so the seeding `post()` resolves null and pushes
`ScrollState(anchorTop: nil, scrollY: 0)`, overwriting the good cached value. Sequence:
(1) preview at scrollY 1500, cache `(320, 1500)`; (2) keystroke → reload N captures
`(targetY: 320, fallback: 1500)`, calls `loadHTMLString`; (3) the new document's
`.atDocumentStart` script runs → cache becomes `(nil, 0)`; (4) `didFinish` for load N has
NOT arrived yet — a long README with highlighted code and the inlined base64 `@font-face`
faces easily exceeds `PickerRefreshCoalescer.refreshInterval = 0.033` — so a second
keystroke fires reload N+1, whose capture reads `(nil, 0)` and OVERWRITES the good
`pendingPreviewRestores[win]`; (5) `restoreJS` runs with `hasTarget = 0`, `el` resolves so
the `if (hasTarget)` is false and the `else` is skipped — **no scroll at all, page stays
at the top.**

**Plan §5.7's failure table is WRONG** where it claims two reloads can straddle one page
load "never": the coalescer serializes reloads to one per 33ms but does NOT wait for
`didFinish`. Correct that table as part of the fix.

### L1 (fold in) — stale `__wolfProgrammatic` swallows one genuine user scroll

`PreviewScrollSeam.swift:312-315`. `moveTo` arms the flag unconditionally, but it is only
consumed inside the `scroll` listener's rAF callback — and `window.scrollTo(0, y)` to the
CURRENT offset fires no `scroll` event, so the flag stays `true`. Reachable whenever a
restore's computed target is 0 (anchor near the top, or `moveTo(fallbackY)` with
`fallbackY == 0`). The next genuine user scroll's first throttled callback then consumes
the stale flag without setting `__wolfUserScrolled`; if `document.fonts.ready` resolves in
that one-frame window the late re-apply yanks the user back. Bounded to ~1 frame.
Fix: arm the flag only when `y !== W.scrollY`.

### C1 (fold in) — stale doc comment

`redraw.rs:168-175` still says the payload is "emitted by the `CursorMoved`/`CursorMovedI`
autocmds" — DEV-1 added `BufEnter`/`BufWinEnter` and DEV-4 added the `win_gettype()` guard,
neither mentioned. `nvim.rs:636-676` is correct and complete. Since §14 calls DEV-4 "a
load-bearing constraint a future refactor could easily drop", this is the comment most
likely to mislead that refactor. (`markdown_cursor.rs:5` and `BUILD.bazel:436-438` have
the same omission but explain the entry events correctly further down — test/BUILD files,
leave to the test-writer/technical-writer.)

## 16. Fix design — ORCHESTRATOR DECISION

Both H1 and H2 live in `PaneContainerView`'s capture ordering, which plan §6.3 exempts
from testing — so fixing them in place would ship two untestable regressions-in-waiting.
**Per the reviewer's recommendation, the capture decision moves into the pure seam.**

Add to `PreviewScrollSeam`:

```swift
public enum CaptureDecision: Equatable {
  case drop                      // source switched — no restore; caller clears the cached state
  case keepPending               // a seed-only cache must not clobber an in-flight restore
  case capture(RestoreRequest)
}

public static func captureDecision(
  previousLoad: PreviewSeam.PreviewLoad?,   // lastPreviewLoads[win] BEFORE it is reassigned
  currentLoad: PreviewSeam.PreviewLoad,
  cursor: CursorPos,
  cachedState: ScrollState?,
  pendingRestore: RestoreRequest?
) -> CaptureDecision
```

Rules, in order:

1. **`.drop`** when `previousLoad != nil && previousLoad!.sourceBufnr != currentLoad.sourceBufnr`
   — a source switch is a NAVIGATION, not an edit (fixes H1). The caller must also clear
   `previewScrollStates[win]` and `pendingPreviewRestores[win]`, so `previewDidFinishLoad`
   falls through to its first-load branch and the new document opens at its top (or at its
   own cursor, if that buffer's cursor is genuinely known).
2. **`.keepPending`** when `pendingRestore != nil` and `cachedState` is the document-start
   seed (`anchorTop == nil && scrollY == 0`) — the in-flight restore's capture is the good
   one; do not overwrite it (fixes H2).
3. **`.capture(...)`** otherwise, building the `RestoreRequest` exactly as today.

`previousLoad` must be read BEFORE `lastPreviewLoads[win]` is reassigned at
`PaneContainerView.swift:1358` — note the ordering hazard explicitly.

This makes both fixes headlessly testable. `PaneContainerView` keeps only the mechanical
`switch`.

Also fold in **L1** (arm `__wolfProgrammatic` only when `y !== W.scrollY`) and **C1**
(the `redraw.rs` doc comment), and correct §5.7's failure table.

### Round-4 tests (APPROVED)

- **T-5** — `PreviewScrollSeamTests.swift`: `captureDecision` truth table. Source switch →
  `.drop` (incl. that it wins over a pending restore); first bind (`previousLoad == nil`)
  → NOT `.drop`; seed cache + pending restore → `.keepPending`; seed cache + NO pending
  restore → `.capture`; non-seed cache + pending restore → `.capture` (a real scroll
  position must still overwrite); same bufnr with a bumped tick → `.capture`; `nil`
  cached state → `.capture` with `targetY == nil` and `fallbackScrollY == 0`; the built
  `RestoreRequest` carries the passed cursor.
- **T-6** — `PreviewScrollSeamTests.swift`: assert `stateScriptJS` arms
  `__wolfProgrammatic` only on an actual offset change (pin the `!==` / `!=` comparison
  against `scrollY` inside `moveTo`).

### §16 clarification (ORCHESTRATOR DECISION, binding on the coder)

The test-writer flagged one gap: `cachedState == nil` together with a non-nil
`pendingRestore`. Rule 2 as written keys literally on the seed shape
(`anchorTop == nil && scrollY == 0`), which a `nil` state is not — so the literal reading
returns `.capture` of an empty request and clobbers a good in-flight restore, i.e. the H2
shape again. The test-writer judged the combination unreachable and left it untested.

**Decision: make rule 2 defensive rather than relying on unreachability.** A `nil`
`cachedState` carries NO information about scroll position, exactly like the seed, so it
must not overwrite an in-flight restore either. Rule 2 becomes:

> `.keepPending` when `pendingRestore != nil` AND `cachedState` carries no position —
> i.e. `cachedState == nil`, OR `cachedState!.anchorTop == nil && cachedState!.scrollY == 0`.

This does not conflict with any written test (`testCaptureDecisionWithNoCachedStateCapturesAnEmptyRequest`
passes `pendingRestore: nil`, which still takes rule 3). Document the reasoning at the
rule, so a later reader does not "simplify" it back to the literal seed check.

The test-writer's other four interpretations are ACCEPTED as specified: rule 1 keys on
`sourceBufnr` ONLY (equal ticks still drop); the `.capture` payload is
`targetY: cachedState?.anchorTop`, `fallbackScrollY: cachedState?.scrollY ?? 0` (`targetY`
stays `nil`, never `0` — a `0` target would pin the anchor to the very top of the
viewport); any of `!==`/`!=` in either operand order and either `W.scrollY`/`window.scrollY`
spelling is acceptable in `moveTo`; and `.drop`'s caller-side obligation (clear BOTH
`previewScrollStates[win]` and `pendingPreviewRestores[win]`) is untestable here but
mandatory.

---

## 17. Fixes landed — round 5 (final polish)

H1, H2, L1 and C1 are all fixed. `//crates/wolf-macos:WolfKeyMapTests` 347 → **359 tests,
0 failures** (`PreviewScrollSeamTests` now 49 methods); all 12 round-4 methods observed
executing by name; full sweep 87/87 with all lint check targets re-run uncached.

**The §16 rule-2 clarification turned out to be load-bearing, not merely defensive.**
The coder confirmed `cachedState == nil` alongside a non-nil `pendingRestore` IS reachable
in production *precisely because of* the new `.drop` branch, which nils
`previewScrollStates[win]`: a `.drop` immediately followed by another reload for the same
window (before any page `post()`) would, under the literal seed check, return `.capture` of
an empty request and reintroduce H2. The test file's comment claiming the combination is
unreachable is now false and must be corrected.

Flake note: a first sweep run had `//crates/wolf-core:commands`, `:current_file` and
`:messages` killed by SIGUSR1 under parallel load with empty failure output; all three pass
in isolation and the re-run sweep was 87/87. The Rust change in that crate is
doc-comment-only, so this is load-shedding, not a regression.

### Round-5 items (ORCHESTRATOR DECISION: all three APPROVED)

- **F-1 — clamp inside `moveTo` before arming `__wolfProgrammatic`.** L1's fix
  (`y !== W.scrollY`) is bounded, not total: a `moveTo(fallbackY)` whose target exceeds the
  NEW (shorter) document's max scroll gets clamped by WebKit, fires no `scroll` event, and
  leaves the flag armed for the same ~1-frame window. `scrollAnchorTo` already clamps to
  `max`; only the raw-`fallbackY` path is exposed. Clamp inside `moveTo` so every
  programmatic scroll is clamped before the offset comparison. This is the same bug class
  we just fixed — close it rather than leave a second instance.
- **F-2 — `.drop` must also clear `lastPreviewCursors[win]`.** It currently leaves the OLD
  file's cursor there; if the new file's cursor is numerically identical,
  `winsNeedingScroll` sees no change and no follow fires. The coder judged the outcome
  "correct-enough" (the document stays at its top), but a one-line clear removes a whole
  class of stale-state reasoning from a branch whose entire purpose is discarding stale
  state. Take it.
- **F-3 — tests for both**, plus correcting the stale unreachability comment.

### Round-5 tests (APPROVED)

- **T-7** — `testCaptureDecisionNilCacheWithAPendingRestoreKeepsIt`: `cachedState: nil`,
  `pendingRestore: inFlightRestore`, same bufnr → `.keepPending`. This is the
  §16-clarification branch and is currently the only rule-2 sub-case with no test. **Also
  correct the now-false "unreachable" comment** at
  `testCaptureDecisionWithNoCachedStateCapturesAnEmptyRequest` — the coder proved the
  combination reachable via `.drop`'s cache-clear.
- **T-8** — `testStateScriptJSClampsAProgrammaticScrollBeforeArming`: assert `moveTo`'s
  offset guard sits AFTER a `Math.min` / `scrollHeight` clamp, so the comparison is against
  the clamped target rather than the raw one.

---

## 18. Review round 5: APPROVED. Final cleanup items.

The reviewer verified H1, H2, L1, C1, F-1 and F-2 are all correctly and completely fixed —
rule ordering, the `previousLoad`-read ordering, the rule-2/rule-3 boundary, the `moveTo`
clamp-then-compare, the single-funnel property and the `redraw.rs` comment all check out,
with no regression introduced by the fixes. Notably confirmed: `.drop` cannot fire on an
ordinary edit (rule 1 compares `sourceBufnr` alone, an edit changes only `sourceTick`), and
the dark/light-flip free bonus still holds (`viewDidChangeEffectiveAppearance`'s
`lastPreviewLoads.removeAll()` makes `previousLoad == nil`, so rule 1 cannot fire).
**Verdict: ship it.**

### Mandatory cleanup (CLAUDE.md: docs are the source of truth)

- **C2 — two SOURCE files still carry stale comments** that the technical-writer correctly
  refused to touch (they are code, not `.md`). `crates/wolf-core/src/markdown.rs:10` and
  `:50-58` — the heading "(built now; JS + cursor signal deferred)" and the
  "Forward-compat contract (designed, NOT built here)" section, which contains a claim that
  is now **actively wrong**: that the follow-up "must convert nvim's BYTE column to a
  CHARACTER column". It does not — `charcol()` returns the character column directly. Also
  `crates/wolf-core/src/markdown/base.css:7-9` and `:184-186` ("the deferred scroll-sync JS
  will…", "purely an anchor target for a future cursor-follow scroll"). The neighbouring
  READMEs have already been corrected, so these in-source comments now CONTRADICT the
  documentation.
- **C3 — `prettier --write` was never run** on the 12 edited `.md` files; the
  technical-writer had no Bash tool. Edits were written to be prettier-stable
  (`proseWrap: "never"`, single-line paragraphs) but that must be confirmed.

### Optional items taken (cheap, close real gaps)

- **N2 — pin the single-funnel invariant.** Tests assert exactly one
  `__wolfProgrammatic = true` inside `moveTo`, but nothing asserts exactly one `scrollTo(`
  in `stateScriptJS`. A future edit adding a direct `W.scrollTo(...)` that bypasses
  `moveTo` would pass every current test while making the script classify its own scroll as
  a USER scroll — killing `restoreJS`'s late re-applies (the DEV-2 bug, third instance).
- **N3 — document that `PreviewLinkSeam.scrollIntoViewJS`'s `e.scrollIntoView(true)`
  legitimately bypasses `moveTo`**, so a fragment jump IS recorded as a user scroll. That is
  the DESIRABLE outcome (a late restore re-apply must not fight a deliberate anchor jump),
  but it is undocumented and someone could "fix" it later.

### Optional item NOT taken — N1, recorded as a known limitation

`.drop` nils `previewScrollStates[win]`, but the OLD document stays live until the new one
commits, so its rAF scroll listener or a follow can `post()` into the same per-window slot
after the clear. If a SECOND reload for that window is armed before `didFinish` (by then
`previousLoad.sourceBufnr == currentLoad.sourceBufnr`, so rule 1 no longer applies), that
repopulated old-document state is captured and a scaled-down H1 reappears. Requires a
scroll/follow post in the old document inside the load window AND another reload for the new
file before `didFinish` — after a link click, both are unlikely. The reviewer explicitly
said not to hold the ship for it. **Fix if it ever surfaces: carry the owning `sourceBufnr`
on `ScrollState` and have `captureDecision` discard a state whose bufnr does not match
`currentLoad`.** Surfaced to the user rather than fixed unilaterally.
