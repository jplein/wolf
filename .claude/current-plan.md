# Current Plan: Markdown preview link handling

## Feature request

Make link-handling changes in the Markdown preview pane:

1. **File-on-disk link** → preview the linked file, replacing the current preview view with the preview of that file.
2. **Anchor link** → scroll to the anchor. Covers same-file (`[x](#anchor)`) and cross-file (`[x](other.md#anchor)`).
3. **URL link** (http/https/mailto/etc.) → open in the default browser, NOT the preview webview.

## Orchestrator rulings on the architect's open questions (BINDING)

- **O1 (same-file anchor):** Intercept + `evaluateJavaScript(scrollIntoView)` + `.cancel` (the JS path), NOT native `.allow`. Consistent with cross-file scroll and respects `scroll-margin-top`.
- **O2 (slug fidelity):** ACCEPT Unicode-preserving GitHub-style slugs; `"section"` empty-after-strip fallback; NON-collapsing hyphen rule; the rare literal-`foo-1` dedup edge is documented and accepted.
- **O3 (non-markdown / missing file link):** `preview_file` returns `0` for both. Front-end behavior on `0`: if `FileManager.default.fileExists(atPath: path)` is TRUE (exists but not previewable) → `NSWorkspace.shared.open(URL(fileURLWithPath: path))` (hand it to the OS default app); if FALSE (missing) → no-op. Do NOT open it in the editor. This keeps the preview context view-only and is simple.
- **O4 (local image subresources):** OUT OF SCOPE (unchanged from today's `nil`-baseURL behavior). Accept.
- **O5 (`ENABLE_HEADING_ATTRIBUTES`):** ENABLE it (honor explicit `{#custom-id}`; auto-slug only when id is None). Accept.
- **O6 (two-parse cost):** ACCEPT the extra pre-pass parse per render (renders are debounced). No single-pass refactor now.

## Recon (current preview architecture)

- **Render (Rust, pure):** `crates/wolf-core/src/markdown.rs` `render_html(source, theme) -> String` via `pulldown_cmark`. Headings currently emit NO `id`. Injects invisible `<span class="wolf-src" id="wl-{L}-{C}">` markers before block starts via `MarkerInjector` over `Parser::into_offset_iter()`. Options enabled: TABLES, FOOTNOTES, STRIKETHROUGH, TASKLISTS.
- **nvim Lua:** `runtime/nvim/init.lua` `wolf_open_preview(mode)` (~640) + `wolf_setup_preview(chan)` (~730). Scratch buffer carries `b:wolf_preview_source` (abs path) + `b:wolf_preview_source_bufnr`. `_G.wolf_preview_bufs` maps source bufnr -> preview bufnr. `wolf_preview` emits `[preview_win, source_bufnr, abs_path]` triples; `wolf_markdown_dirty` emits `[source_bufnr, changedtick]`. `_G.wolf_emit_previews` / `_G.wolf_emit_current_file` are the re-emit hooks.
- **Rust RPC/API:** `nvim.rs::render_preview` (~287) / `open_preview` (~336); `api.rs::WolfCore::render_preview` (~397) / `open_preview` (~423) / `is_markdown_active`.
- **FFI:** `crates/wolf-ffi/src/lib.rs` extern block (~471-505) exposes `open_preview`, `render_preview`, `previews() -> Vec<FfiPreview>`. `FfiPreview`: `win()`, `source_bufnr()`, `source_name()`, `source_path()` (ABS — already present), `source_tick()`. Generated Swift at `crates/wolf-ffi/generated/wolf-ffi.swift`, regen via `bazel run //crates/wolf-ffi:gen_bindings`.
- **macOS:** `PaneContainerView.swift` — `previewWebViews[win]` hosts one `WKWebView` each. `reloadDirtyPreviews()` (~683) → `handle.render_preview(bufnr)` + `web.loadHTMLString(html, baseURL: nil)`. `ensurePreviewWebView(for:)` (~702) builds the webview with bare config, NO nav delegate. `PreviewSeam.swift` (Foundation-only) holds pure placement/reload arithmetic; tests in `Tests/WolfKeyMapTests/PreviewSeamTests.swift`. `lastPreviewLoads: [Int64: PreviewSeam.PreviewLoad]` keyed by `(sourceBufnr, sourceTick)` gates reloads.
- **GTK (`wolf-gnome`):** no preview webview — macOS-only. No GTK work.

---

# AUTHORITATIVE PLAN (from architect; rulings above applied)

## Files to CREATE

### A. `crates/wolf-macos/Sources/WolfKeyMap/PreviewLinkSeam.swift` (pure, Foundation-only)
- `public enum PreviewLinkSeam`
  - `public enum Action: Equatable { case openInBrowser(URL); case scrollToFragment(String); case previewFile(path: String, fragment: String?) }`
  - `public static func classify(url: URL, sourcePath: String) -> Action`:
    - scheme non-`file`, non-nil (`http`/`https`/`mailto`/`tel`/`ftp`/…) → `.openInBrowser(url)`.
    - scheme == `file`: `frag = url.fragment` (Foundation returns percent-decoded; empty→nil). Compare `URL(fileURLWithPath: sourcePath).standardizedFileURL.path` vs `url.standardizedFileURL.path`. Same+frag → `.scrollToFragment(frag)`; same+noFrag → `.previewFile(path: url.path, fragment: nil)`; different → `.previewFile(path: url.path, fragment: frag)`.
    - nil scheme (defensive) → `.previewFile(path: url.path, fragment: url.fragment)` if it has a path, else `.openInBrowser(url)`.
  - `public static func scrollIntoViewJS(fragmentId: String) -> String`: self-invoking JS `getElementById(<json>)` + `scrollIntoView(true)`, with `fragmentId` JSON-escaped (hand-written escaper covering `\`, `"`, `\n`, `\r`, `\t`, `<`, `/`). Pure/unit-testable.

### B. `crates/wolf-macos/Sources/WolfMacOS/PreviewNavigationDelegate.swift` (AppKit glue; compile + manual-smoke only)
- `final class PreviewNavigationDelegate: NSObject, WKNavigationDelegate`
  - `let win: Int64`; `weak var container: PaneContainerView?`; `init(win:container:)`.
  - `webView(_:decidePolicyFor:decisionHandler:)`: `guard navigationType == .linkActivated, let url = request.url else { .allow }` (lets the initial `loadHTMLString` + JS scrolls through). `src = container?.previewSourcePath(forWin: win) ?? ""`. `switch PreviewLinkSeam.classify(url:sourcePath:)`: `.openInBrowser(u)` → `NSWorkspace.shared.open(u)` + `.cancel`; `.scrollToFragment(f)` → `.cancel` + `container?.scrollPreview(win:toFragment:)`; `.previewFile(p,f)` → `.cancel` + `container?.previewFile(win:path:fragment:)`.
  - `webView(_:didFinish:)` → `container?.previewDidFinishLoad(win: win)`.
  - Imports: `AppKit`, `WebKit`, `WolfKeyMap`. Delegate retained by container (webview `navigationDelegate` is weak); container held weakly (no cycle). All callbacks main-thread.

### C. `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewLinkSeamTests.swift` (headless) — see Test Plan.

## Files to MODIFY

### 1. `crates/wolf-core/src/markdown.rs` — heading slug ids
- In `render_html`: add `options.insert(Options::ENABLE_HEADING_ATTRIBUTES);`.
- `fn slugify(text: &str, seen: &mut HashMap<String, u32>) -> String` — algorithm in §SLUG below.
- `fn heading_slugs(source: &str, options: Options) -> Vec<Option<String>>` — fresh `Parser::new_ext`; one entry per heading in doc order. Explicit `id: Some` → push `None` but register the explicit id in `seen` (increment). `id: None` → accumulate `Event::Text`+`Event::Code` between `Start(Heading)` and `End(TagEnd::Heading(_))`, push `Some(slugify(&acc, &mut seen))`.
- `struct HeadingIdAssigner<'a, I>` (mirrors `MarkerInjector` style) wrapping the offset iter; holds `slugs` + `next_heading`. In `next()`: on `Event::Start(Tag::Heading { id: None, level, classes, attrs })`, take `slugs[next_heading]` and if `Some(slug)` rebuild as `Start(Heading { level, id: Some(slug.into()), classes, attrs })`; increment `next_heading` on EVERY heading start (aligned with `heading_slugs`); pass through `(event, range)` otherwise.
- Pipeline: `let with_ids = HeadingIdAssigner::new(offset_iter, heading_slugs(source, options)); let injector = MarkerInjector::new(with_ids, markers);` — marker behavior unchanged (still `(Event, Range)`).
- Doc-comment the slug algorithm + the two-parse note precisely (source of truth).

### 2. `crates/wolf-core/src/markdown/base.css` — anchor scroll margin
- In the `h1..h6` rule add `scroll-margin-top: 2rem;` (matches body top padding). Update the file header comment if it enumerates owned rules.

### 3. `runtime/nvim/init.lua` — extract helper + `wolf_preview_file`
- Extract scratch create/reuse (current lines ~647-669) into `function _G.wolf_ensure_preview_buf(source_buf, source_path) -> preview_buf` (the `_G.wolf_preview_bufs` reuse lookup, `nvim_create_buf(true,true)`, `"Preview: <basename>"` naming w/ E95 fallback, `b:wolf_preview_source`/`b:wolf_preview_source_bufnr` vars, `modifiable=false`, map insert). `wolf_open_preview` calls it.
- New `function _G.wolf_preview_file(win, path)`:
  - `if path==nil or path=="" then return 0 end`; `if vim.fn.filereadable(path)==0 then return 0 end`.
  - `local source_buf = vim.fn.bufadd(path); vim.fn.bufload(source_buf)`.
  - markdown gate: `if vim.bo[source_buf].filetype ~= "markdown" then local ft = vim.filetype.match({buf=source_buf, filename=path}); if ft=="markdown" then vim.bo[source_buf].filetype="markdown" else return 0 end end`.
  - `local source_path = vim.api.nvim_buf_get_name(source_buf)`; `local preview_buf = _G.wolf_ensure_preview_buf(source_buf, source_path)`.
  - bind: `if win and win~=0 and vim.api.nvim_win_is_valid(win) then vim.api.nvim_win_set_buf(win, preview_buf) else vim.api.nvim_win_set_buf(0, preview_buf) end`.
  - re-emit: `if _G.wolf_emit_previews then _G.wolf_emit_previews() end`; `if _G.wolf_emit_current_file then _G.wolf_emit_current_file() end`.
  - `return source_buf`.
  - Contract: returns linked file's SOURCE bufnr (`>0`) on success; `0` on empty/unreadable/non-markdown.

### 4. `crates/wolf-core/src/nvim.rs` — `preview_file` RPC (after `open_preview` ~351)
- `pub async fn preview_file(&self, win: i64, path: &str) -> Result<i64, NvimError>`: `nvim_exec_lua` `"return wolf_preview_file(...)"` with args `[win, path]`; RPC err → `NvimError::Rpc`; decode `value.as_i64().unwrap_or_default()`. Doc the reuse/gate/`0`-contract.

### 5. `crates/wolf-core/src/api.rs` — `WolfCore::preview_file` (after `open_preview` ~431)
- `#[must_use] pub fn preview_file(&self, win: i64, path: &str) -> i64`: `block_on(self.nvim.preview_file(win, path))`; `Ok(b)=>b`, `Err(e)=>{ tracing::warn!(...); 0 }`. Doc contract.

### 6. `crates/wolf-ffi/src/lib.rs` — bridge method
- extern block after `render_preview` (~484): `fn preview_file(&self, win: i64, path: String) -> i64;` + doc comment.
- impl after `render_preview` (~1809): `#[must_use] pub fn preview_file(&self, win: i64, path: String) -> i64 { self.core.borrow().as_ref().map_or(0, |c| c.preview_file(win, &path)) }`.

### 7. `crates/wolf-ffi/generated/wolf-ffi.swift` — REGENERATE only (`bazel run //crates/wolf-ffi:gen_bindings`). Do NOT hand-edit.

### 8. `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`
- New stored props (near `previewWebViews`/`lastPreviewLoads`): `previewNavDelegates: [Int64: PreviewNavigationDelegate]`, `previewSourcePaths: [Int64: String]`, `pendingPreviewFragments: [Int64: String]`.
- `ensurePreviewWebView(for:)`: after creating the webview, `let d = PreviewNavigationDelegate(win: win, container: self); web.navigationDelegate = d; previewNavDelegates[win] = d`.
- `reloadDirtyPreviews()`: in the `previews()` loop capture `previewSourcePaths[preview.win()] = preview.source_path().toString()`. Reload site: `let path = previewSourcePaths[win] ?? ""; let base = path.isEmpty ? nil : URL(fileURLWithPath: path); web.loadHTMLString(html, baseURL: base)`. (Do NOT add source_path to `PreviewLoad`; gate stays `(sourceBufnr, sourceTick)`.)
- `removePreviewWebView(_:)`: also nil out `previewNavDelegates[win]`, `previewSourcePaths[win]`, `pendingPreviewFragments[win]`.
- New methods (main-thread, called by delegate):
  - `func previewSourcePath(forWin win: Int64) -> String` → `previewSourcePaths[win] ?? ""` (fallback: scan `handle.previews()` for the win if cache empty).
  - `func scrollPreview(win: Int64, toFragment: String)` → `previewWebViews[win]?.evaluateJavaScript(PreviewLinkSeam.scrollIntoViewJS(fragmentId:), completionHandler: nil)`.
  - `func previewFile(win: Int64, path: String, fragment: String?)`: `let src = handle.preview_file(win, path)`. If `src == 0`: **O3** — if `FileManager.default.fileExists(atPath: path)` → `NSWorkspace.shared.open(URL(fileURLWithPath: path))`; else no-op; then `return`. Else: if `let f = fragment { pendingPreviewFragments[win] = f }`; `previewReloadCoalescer.request()`.
  - `func previewDidFinishLoad(win: Int64)`: `guard let f = pendingPreviewFragments.removeValue(forKey: win) else { return }`; `previewWebViews[win]?.evaluateJavaScript(PreviewLinkSeam.scrollIntoViewJS(fragmentId: f), completionHandler: nil)`.

## §SLUG — heading slug algorithm (EXACT)
Accumulated heading text = in-order concat of every `Event::Text` + `Event::Code` between `Start(Heading)` and `End`. Then:
1. `str::to_lowercase()` (full Unicode).
2. char-by-char (NO hyphen collapsing): `is_alphanumeric()` → push c; `'-'` → push `'-'`; `is_whitespace()` → push `'-'`; else drop. (`"C++ & Rust"` → `"c--rust"`; `"foo  bar"` → `"foo--bar"`.)
3. empty-after-strip → base `"section"`.
4. dedup vs shared `HashMap<String,u32>` (doc order): unseen → base, count=1; `n>=1` → `"{base}-{n}"`, count=n+1. Explicit `{#custom}` ids used verbatim AND registered (increment) so later auto-slugs suffix around them. Rare literal-`foo-1` collision edge documented + accepted.
- Code spans included. Unicode retained+lowercased (`"Café Ω"` → `"café-ω"`).

## preview_file end-to-end
Lua `_G.wolf_preview_file(win, path)` → nvim.rs `async preview_file(win:i64, path:&str)->Result<i64,NvimError>` → api.rs `preview_file(win:i64, path:&str)->i64` → lib.rs `preview_file(win:i64, path:String)->i64` → Swift `preview_file(win:Int64, path:String)->Int64`. Returns linked file SOURCE bufnr (`>0`) / `0`. `path` is ABSOLUTE (from WebKit's resolved `file://` URL, front-end passes `url.path`); Lua does NO relative resolution.

## baseURL
One site only: `reloadDirtyPreviews()`. `baseURL: nil` → `file://` URL from the win's source path (empty/unknown → keep `nil`). Grep-confirmed sole `loadHTMLString` call. Policy callback fires before any real file read, so classify+cancel needs no sandbox file access.

---

# TEST PLAN

### Rust core unit (`markdown.rs` `#[cfg(test)]`)
- `slugify_basic` `"Hello World"`→`"hello-world"`; `slugify_drops_punctuation` `"Foo, Bar!"`→`"foo-bar"`, `"C++ & Rust"`→`"c--rust"`; `slugify_keeps_existing_hyphens` `"multi-word title"`→`"multi-word-title"`; `slugify_code_span` `## \`foo\``→`foo`; `slugify_unicode` `"Café Ω"`→`"café-ω"`; `slugify_empty_after_strip` `"!!!"`→`"section"`; `slugify_dedup` two `"Intro"`→`["intro","intro-1"]`, third→`"intro-2"`.
- `render_html_heading_has_id`: `render_html("# Alpha Heading\n", Default)` contains `id="alpha-heading"` on `<h1`.
- `render_html_explicit_heading_id`: `"# Title {#custom}\n"` → `id="custom"`; a following `"# Custom"` → `id="custom-1"`.
- `render_html_marker_still_present`: `id="wl-1-0"` still present alongside heading id.
- Keep existing marker tests unchanged.
- Run: `cargo test -p wolf-core markdown`.

### wolf-ffi integration (real nvim) — `crates/wolf-ffi/tests/preview.rs`
- New `preview_file_repoints_pane_to_linked_file()`: reuse existing helpers (`nvim_bin_or_skip`, `wait_for_attach`, `send_ex_command`, `wait_for_previews`, loud-SKIP guard). Seed `alpha.md` (link `[to beta](beta.md#beta-heading)` + a heading) + `beta.md` (`# Beta Heading`). `:e alpha.md`; `open_preview("split_right")`; capture preview `win`. Call `handle.preview_file(win, <abs beta.md>)` → assert `> 0`. `wait_for_previews` until the row for `win` has `source_name()=="beta.md"` and window count unchanged. `render_preview(beta_bufnr)` contains `"Beta Heading"` + `id="beta-heading"`. Negative: `preview_file(win, <abs .txt>)` → `0`, `previews()` unchanged. Post-`shutdown()` → `0`.
- Extend the existing preview test to assert `render_preview` output contains `id="alpha-heading"` (slug shipped through nvim).
- `preview.rs` already a bazel target — no BUILD change. Run: `WOLF_NVIM_BIN=<nvim> cargo test -p wolf-ffi --test preview` or `bazel test //crates/wolf-ffi:preview`.

### Pure Swift seam — `PreviewLinkSeamTests.swift` (`@testable import WolfKeyMap`)
- `testExternalHttpOpensBrowser`, `testMailtoOpensBrowser` → `.openInBrowser`.
- `testSameFileFragmentScrolls` (`file:///a/x.md#sec`, src `/a/x.md`) → `.scrollToFragment("sec")`.
- `testCrossFileWithFragmentPreviews` (`file:///a/y.md#sec`) → `.previewFile("/a/y.md","sec")`.
- `testCrossFileNoFragmentPreviews` → `.previewFile("/a/y.md",nil)`.
- `testSameFileNoFragment` → `.previewFile("/a/x.md",nil)`.
- `testPathNormalization` (src `/a/./x.md`) → same-file.
- `testScrollIntoViewJSEscapes` (`"a\"b"`) → escaped `\"`, contains `scrollIntoView`.
- Run: `bazel test //crates/wolf-macos:WolfKeyMapTests` (confirm target name in BUILD.bazel).

### Cannot be integration-tested (compile + manual-smoke only)
Nav-delegate routing, `NSWorkspace.open`, `loadHTMLString` baseURL, native/JS scroll, `evaluateJavaScript`. Keep glue thin; all branching in `PreviewLinkSeam` (unit-tested). Manual smoke: (1) `.md` link → pane shows linked file; (2) `#anchor` → scrolls clear of top; (3) `other.md#anchor` → switch+scroll; (4) `https://` → browser, webview unchanged; (5) live edits still reload; (6) missing/non-md link → O3 behavior, no crash.

---

# READMEs to update
- `crates/wolf-core/src/README.md` — `markdown.rs` (slug ids, `ENABLE_HEADING_ATTRIBUTES`, `scroll-margin-top`, algorithm+dedup+explicit-id), `nvim.rs` (`preview_file`), `api.rs` (`WolfCore::preview_file`).
- `crates/wolf-core/README.md` (crate root) — `render_preview`/`preview_file` surface + markdown section mention link handling.
- `crates/wolf-ffi/src/README.md` — new `preview_file(win, path) -> i64` bridge method + contract.
- `crates/wolf-ffi/tests/README.md` — new `preview_file` integration case.
- `runtime/nvim/README.md` — `_G.wolf_ensure_preview_buf` extraction + `_G.wolf_preview_file(win, path)`.
- `crates/wolf-macos/Sources/README.md` — `PreviewLinkSeam`, `PreviewNavigationDelegate`, baseURL change, new `PaneContainerView` methods.
- `crates/wolf-macos/Tests/README.md` — new `PreviewLinkSeamTests`.
- Sanity-check `crates/wolf-macos/README.md` for a preview overview; update if it mentions preview behavior.
- Do NOT touch `.worktrees/`. Markdown: no hard line breaks in paragraphs; `prettier --write` each edited `.md`.

# Implementation order (avoid compile breaks)
1. `markdown.rs` + `base.css` (pure Rust) → `cargo test -p wolf-core`.
2. `init.lua` (extract helper + `wolf_preview_file`).
3. `nvim.rs` → `api.rs` → `lib.rs` (together) → `cargo build -p wolf-ffi`.
4. `bazel run //crates/wolf-ffi:gen_bindings`.
5. `PreviewLinkSeam.swift` + `PreviewLinkSeamTests.swift` → build/test WolfKeyMap.
6. `PreviewNavigationDelegate.swift` + `PaneContainerView.swift` edits.
7. wolf-ffi integration test additions in `preview.rs`.
8. READMEs.

# Build/test commands
- `cargo test -p wolf-core` (slug+render).
- `WOLF_NVIM_BIN=<nvim> cargo test -p wolf-ffi --test preview` OR `bazel test //crates/wolf-ffi:preview`.
- `bazel run //crates/wolf-ffi:gen_bindings` (regen).
- `bazel test //crates/wolf-macos:WolfKeyMapTests`; `bazel build //crates/wolf-macos/...`.
- (All require flake devShell so `WOLF_NVIM_BIN` is set.)

# Lint
- Rust: `#[must_use]` on `preview_file` (api.rs/lib.rs), matching `open_preview`. No blanket `allow`.
- Swift: `weak var container` (no cycle); prefix `_ =` if `NSWorkspace.open`'s Bool result is flagged; call `decisionHandler` on every path.

---

## Status
- [x] Architect plan (agent ad690b7b7bd56d20e) + orchestrator rulings O1-O6
- [x] Tests written (agent ab89e2529ba12ebeb)
- [x] Implementation complete (agent a930c49a77fa7c562) — all NEW/extended tests PASS with real nvim
- [x] Flaky pre-existing test hardened (agent abe19a4fd1e5760a2) + preview target green 10/10 runs (agent a88ad655edce3f82e)
- [x] Review — PASS/approved (agent a086754c2b6563651). No correctness blockers. Verified §SLUG exactness, next_heading alignment, marker preservation, preview_file chain, PreviewLinkSeam branches + JS escaper, delegate decisionHandler-once + weak container, cross-file scroll race handled (pending fragment survives until real didFinish). Only outstanding: READMEs (expected next step).
- [ ] Docs

## Coder result (agent a930c49a77fa7c562)
Implemented all 7 steps. Files: markdown.rs (slug pipeline + ENABLE_HEADING_ATTRIBUTES), base.css (scroll-margin-top), init.lua (`wolf_ensure_preview_buf` + `wolf_preview_file`), nvim.rs/api.rs/lib.rs (`preview_file`), regenerated wolf-ffi.swift, PreviewLinkSeam.swift + PreviewNavigationDelegate.swift (new), PaneContainerView.swift edits, BUILD.bazel (added PreviewLinkSeam.swift to non-test `wolf_key_map` srcs; WolfMacOS globs).
PASS: wolf-core lib test (all slugify/render_html), markdown_render, wolf-ffi build + gen_bindings, WolfKeyMapTests (7 PreviewLinkSeam cases), macos build, wolf-ffi `preview_file_repoints_pane_to_linked_file` + extended `preview_gates_...` (id="alpha-heading").
Deviation (accepted): `HeadingIdAssigner<I>` not `<'a, I>` — rustc E0392 since the struct stores no `Event<'a>` (unlike MarkerInjector). Behavior identical.
FLAKY (pre-existing, NOT ours): `preview_reuses_the_other_preview_pane` fails ~75% on HEAD too (verified by reverting init.lua). Test reads `is_markdown_active()` immediately after a reuse rebind, racing the `wolf_emit_current_file` re-assert.

## Orchestrator ruling: harden the flaky test
DECISION: have the test-writer add a bounded poll/wait for the post-rebind `is_markdown_active()` state in `preview_reuses_the_other_preview_pane` (mirroring the existing `wait_for_previews` helper). This is a test-robustness fix (not weakening the assertion), keeps the `preview` bazel target green so our feature's signal is clean. In-scope because it's in the same file we modified.

## Test artifacts written (test-writer)
- `crates/wolf-core/src/markdown.rs` — extended `#[cfg(test)] mod tests`: `slugify_*` (basic/punctuation/hyphens/code-span/unicode/empty/dedup), `render_html_heading_has_id`, `render_html_explicit_heading_id`, `render_html_marker_still_present`. Added `use std::collections::HashMap;` + `fresh_seen()` helper in the test module.
- `crates/wolf-ffi/tests/preview.rs` — new `preview_file_repoints_pane_to_linked_file()` (seeds alpha.md/beta.md/notes.txt; repoint→beta, negative .txt→0, post-shutdown→0); extended existing preview test to assert `id="alpha-heading"`.
- `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewLinkSeamTests.swift` (new) — classify/scrollIntoViewJS cases.
- `crates/wolf-macos/BUILD.bazel` — added the new Swift test file to the explicitly-listed `WolfKeyMapTests` `srcs` (NOT globbed).
- Note: negative FFI case asserts the invariant behaviorally (previews() rows are opaque, no PartialEq).

## Docs (technical-writer a3207c58a7412e176) — FEATURE COMPLETE ✅
8 READMEs updated (wolf-core src + root, wolf-ffi src + tests, runtime/nvim, wolf-macos Sources + Tests + root). `prettier --write` run on all — every file already conformant.
Noted PRE-EXISTING doc drift (NOT this feature, left for a separate pass): `crates/wolf-macos/README.md`'s `openPreview()` bullet still describes the 2-input `placementMode` instead of the shipped 3-input `previewPlacementInputs()` incl. `"reuse_preview_pane"` — stale from the earlier reuse-preview-pane change.

## Progress log
- (init) Orchestrator recon complete.
- Architect plan complete; rulings O1-O6 applied. Invoking test-writer.
- Tests written. Invoking coder to implement per plan + run tests.
- Coder implemented all 7 steps; all NEW/extended tests green with real nvim; flagged pre-existing flaky test.
- Test-writer hardened flaky test (bounded poll); coder confirmed preview target green 10/10.
- Reviewer PASS/approved. Technical-writer updated 8 READMEs; prettier clean. DONE.
