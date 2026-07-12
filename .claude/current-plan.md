# Current Plan: Three small UI changes to Wolf Markdown editor

## Status: COMPLETE

## Docs result (technical-writer + orchestrator prettier pass)
Updated: runtime/nvim/README.md, crates/wolf-core/README.md + src/README.md,
crates/wolf-ffi/README.md + src/README.md + tests/README.md, crates/wolf-macos/README.md +
Sources/README.md + Tests/README.md, architecture.md. New PreviewClickFocusHandler.swift documented.
`prettier --write` run on all changed Markdown — all already conformant (unchanged).

## Review result (reviewer): APPROVED
All three code changes conform to the plan, correct + complete, no retain cycle, symbol names
match tests. `palette_rank` failure confirmed unrelated/pre-existing. Only outstanding: README
doc-sync (this DOCS pass). Stale README refs flagged:
- runtime/nvim/README.md:146,151 (`Preview: <basename>` → cwd-relative)
- crates/wolf-core/src/README.md (new WolfCore::display_path; render.rs PreviewInfo doc)
- crates/wolf-ffi/README.md:105 + crates/wolf-ffi/src/README.md:29 (source_display_path + regen note)
- crates/wolf-ffi/tests/README.md (new assertions)
- crates/wolf-macos/README.md:121 + Sources/README.md:14,38,41,45 (click-to-focus, secondary line, new maps/methods/file, PreviewSeam)
- crates/wolf-macos/Tests/README.md (PreviewSeamTests additions)

## Implementation results (coder)
- All three changes implemented per plan. Regen target confirmed as `//crates/wolf-ffi:gen_bindings`
  (rust_binary in BUILD.bazel); only `generated/wolf-ffi.swift` + `wolf-ffi.h` changed.
- Builds PASS: `//crates/wolf-ffi:wolf_ffi`, `//crates/wolf-core:wolf_core`, `//crates/wolf-macos:wolf_macos_lib`.
- Tests PASS: `//crates/wolf-ffi:preview` (5 tests incl. new subdir test + extended gate test);
  `//crates/wolf-macos:WolfKeyMapTests` (both new seam tests + existing suite).
- PRE-EXISTING unrelated failure: `//crates/wolf-core:palette_rank` (18 vs 17 static command rows) —
  fails identically on clean baseline (git stash verified). OUT OF SCOPE; not touched. Flag to user.
- cargo/rustc absent in env; all Rust ran under bazelisk.
- READMEs intentionally NOT edited (technical-writer pass pending).

## Tests written (test-writer)
- `crates/wolf-ffi/tests/preview.rs`: (a) new block `--- 6b ---` in
  `preview_gates_opens_lists_and_live_updates` asserting `alpha_row.source_display_path()`
  non-empty + `ends_with("alpha.md")`; (b) NEW `#[test] fn preview_picker_shows_cwd_relative_path_for_subdir_source`
  seeding `docs/alpha.md`, asserting picker row `starts_with("Preview: docs/alpha.md")` and
  `source_display_path().ends_with("docs/alpha.md")`.
- `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewSeamTests.swift`: `testClickFocusHandlerName`
  (`== "wolfPaneFocus"`) and `testClickFocusJSCaptureMousedownPostsToHandler` (contains `mousedown`,
  `true`, `window.webkit.messageHandlers.wolfPaneFocus`).
- REQUIRED symbol names the coder MUST match: Rust `FfiPreview::source_display_path(&self) -> String`;
  Swift `PreviewSeam.clickFocusHandlerName` (== "wolfPaneFocus") and `PreviewSeam.clickFocusJS(handlerName:)`
  with a default arg. The JS listener MUST use capture phase with a literal `true` and post to
  `window.webkit.messageHandlers.wolfPaneFocus`.

## Problem Statement

Three small UI changes spanning the nvim runtime (Lua), Rust core/FFI, and the
macOS front-end (Swift), each verified with real-Neovim integration tests where
practical. READMEs are the source of truth and must stay in sync.

### CHANGE 1 — Buffer picker shows the full cwd-relative path for preview buffers
Today the preview scratch buffer is named `Preview: <basename>` in
`runtime/nvim/init.lua` (`_G.wolf_ensure_preview_buf`, ~line 652/661 via
`vim.fn.fnamemodify(source_path, ":t")`). Change it to name the buffer with the
cwd-relative path (`fnamemodify ":."`), e.g. `Preview: crates/wolf-macos/README.md`.
Keep the E95 basename-collision fallback. Update init.lua comments and
`runtime/nvim/README.md`, plus `crates/wolf-core` README references to
`Preview: <basename>`. Test: `crates/wolf-ffi/tests/preview.rs` step 7 asserts the
picker row `starts_with("Preview: alpha.md")` — confirm the test's source file
lives directly in cwd so `":."` still yields `alpha.md`.

### CHANGE 2 — Clicking anywhere in a preview pane focuses that pane
The preview pane hosts a `WKWebView`
(`crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`,
`ensurePreviewWebView` ~line 725) that swallows clicks, so the container's
`mouseDown` (~line 427, which does `handle.focus_win` + `setActivePane`) never
fires there. Add click-to-focus via a document-start `WKUserScript` (capture-phase
`mousedown` → `WKScriptMessageHandler`, e.g. `wolfPaneFocus`); the handler resolves
the pane for that window (`paneForWin`) and calls `handle.focus_win(win)` +
`setActivePane`. Avoid the `WKUserContentController` retain cycle (weak proxy).
Factor pure logic into a testable seam under `Sources/WolfKeyMap` with a test under
`Tests/WolfKeyMapTests` where practical. Update `crates/wolf-macos/README.md`.

### CHANGE 3 — Preview panes show the file path on the secondary statusbar line
In `PaneContainerView.paintStatusbar` (~line 1062-1074) a preview pane sets
`filename="Preview: <name>"` and `displayPath=""`. Make it also show the
cwd/home-relative display path below, mirroring editor panes. Reuse core's
`crate::pane_status::pane_status` logic for the preview source's absolute path
(`WolfCore::pane_status`, api.rs ~line 681, has `live_cwd()` + `canonical_home()`):
add a display-path for the preview source — a new field on `FfiPreview`
(`crates/wolf-ffi/src/lib.rs`, ~line 2347 / `from_preview` ~2393; `previews()`
~1840 has the core handle) exposed as `source_display_path()`, then stash it in a
per-grid Swift map parallel to `previewNamesByGrid` and paint it as the secondary
line. Keep the primary line `Preview: <basename>`. Update `crates/wolf-ffi` and
`wolf-macos` READMEs and extend a `preview.rs` assertion for the new accessor.

## Constraints
- READMEs are source of truth; every touched source dir's README must reflect the change.
- Prefer real-Neovim e2e/integration tests over mocks.
- macOS crate builds with Bazel (bazelisk); Rust core/ffi tests run under Cargo.
- After editing Markdown, run `prettier --write`; no hard line breaks within a paragraph.

## Plan (from architect)

### CHANGE 1 — Lua (self-contained, do first)
- `runtime/nvim/init.lua` `_G.wolf_ensure_preview_buf` (~646-671): line 652 replace
  `local basename = vim.fn.fnamemodify(source_path, ":t")` with cwd-relative
  `local rel = vim.fn.fnamemodify(source_path, ":.")`; line 661 `local wanted = "Preview: " .. rel`.
  Keep E95 fallback (line 663) unchanged. Update the surrounding `Preview: <basename>`
  comments (635, 638-639, 644-645, 653-660) to cwd-relative wording.
- Edge: empty source path → `""` → `Preview: ` (matches today). File at cwd root → bare basename.

### CHANGE 3 — core → FFI → bindings → Swift
- CORE `crates/wolf-core/src/api.rs` (near `pane_status` ~694): add
  `pub fn display_path(&self, abs_path: &str) -> String` = `crate::pane_status::pane_status(abs_path, self.live_cwd().as_deref(), canonical_home()).display_path`.
  `#[must_use]`. Empty abs_path → empty string.
- FFI `crates/wolf-ffi/src/lib.rs`:
  - `FfiPreview` struct (~2348): add field `source_display_path: String`.
  - `from_preview` (~2393): add param `source_display_path: String`; store it. New sig
    `from_preview(win, info, source_tick, source_display_path)`.
  - accessor `pub fn source_display_path(&self) -> String` (`#[must_use]`, near source_path ~2381).
  - `previews()` (~1840): compute `core.display_path(&info.source_path.to_string_lossy())` and pass in.
  - swift-bridge extern block (~500-514): add `fn source_display_path(&self) -> String;`.
- REGEN BINDINGS: `bazel run //crates/wolf-ffi:gen_bindings` → commit only
  `generated/wolf-ffi.swift` + `wolf-ffi.h` (wake header unchanged). Do NOT hand-edit.
  (Coder: verify the exact regen target name in wolf-ffi BUILD.bazel before running.)
- SWIFT `PaneContainerView.swift`:
  - new map `previewDisplayPathsByGrid: [UInt64: String]` beside `previewNamesByGrid` (~179).
  - `syncPreviewWebViews` (~642): `.removeAll` it at top; inside loop set
    `previewDisplayPathsByGrid[grid] = preview.source_display_path().toString()`.
  - `paintStatusbar` preview branch (~1065-1074): `displayPath = previewDisplayPathsByGrid[grid] ?? ""`
    (keep `filename = "Preview: \(name)"`). Existing `if !displayPath.isEmpty` guard handles empty.
- Edge: uncanonicalized `source_path` vs canonical `live_cwd` may fall back to absolute/`~` on
  `/var`↔`/private/var`. Tests use `ends_with`, not exact equality.

### CHANGE 2 — Swift click-to-focus (WKWebView swallows clicks)
- SEAM `crates/wolf-macos/Sources/WolfKeyMap/PreviewSeam.swift` (Foundation-only):
  add `public static let clickFocusHandlerName = "wolfPaneFocus"` and
  `public static func clickFocusJS(handlerName:) -> String` — self-invoking capture-phase
  `mousedown` listener posting to `window.webkit.messageHandlers.<name>.postMessage(1)`.
- NEW FILE `crates/wolf-macos/Sources/WolfMacOS/PreviewClickFocusHandler.swift`:
  `final class PreviewClickFocusHandler: NSObject, WKScriptMessageHandler` with `let win: Int64`,
  `weak var container: PaneContainerView?`; `userContentController(_:didReceive:)` →
  `container?.focusPreviewPane(win: win)`. Weak proxy breaks the WKUserContentController cycle.
- `PaneContainerView.swift`:
  - new retaining map `previewFocusHandlers: [Int64: PreviewClickFocusHandler]` beside `previewNavDelegates` (~165).
  - `ensurePreviewWebView` (~725): build `WKWebViewConfiguration`, create proxy, store in map,
    `config.userContentController.add(proxy, name: PreviewSeam.clickFocusHandlerName)`,
    `addUserScript(WKUserScript(source: PreviewSeam.clickFocusJS(), injectionTime: .atDocumentStart, forMainFrameOnly: true))`,
    then `WKWebView(frame:.zero, configuration: config)`. Keep nav delegate + addSubview.
  - new `func focusPreviewPane(win: Int64)` (~781): `guard let id = paneForWin(win) else { return }`,
    `handle.focus_win(win)`, `setActivePane(id)`, `needsDisplay = true`. No mouse event sent.
  - `removePreviewWebView` (~765): BEFORE dropping web view,
    `previewWebViews[win]?.configuration.userContentController.removeScriptMessageHandler(forName: PreviewSeam.clickFocusHandlerName)`,
    then `previewFocusHandlers[win] = nil`.

## Test plan
- `crates/wolf-ffi/tests/preview.rs`: step 7 (`Preview: alpha.md`) UNAFFECTED (fixture at cwd root).
  ADD assertion on `alpha_row.source_display_path()`: non-empty AND `ends_with("alpha.md")`.
  OPTIONAL new `#[test]` seeding `docs/alpha.md` in a SUBDIR to actually witness `:.` — asserts picker
  row `starts_with("Preview: docs/alpha.md")` and `source_display_path().ends_with("docs/alpha.md")`.
  (Only this subdir test distinguishes `:.` from `:t`; recommended.)
- `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewSeamTests.swift`: assert
  `clickFocusHandlerName == "wolfPaneFocus"` and `clickFocusJS()` contains `mousedown`, capture `true`,
  and `window.webkit.messageHandlers.wolfPaneFocus`.
- Click→focus dispatch itself is AppKit/WebKit glue — compile + manual smoke only, no headless server.

## Docs checklist (prettier --write each Markdown after)
1. `runtime/nvim/README.md` (~146,151) — cwd-relative naming.
2. `crates/wolf-core/src/README.md` — new `WolfCore::display_path`; `render.rs` PreviewInfo doc.
3. `crates/wolf-ffi/README.md` (~55,105,188,217) — `source_display_path()`, regen note, test note.
4. `crates/wolf-ffi/src/README.md` (~29) — accessor + computed in `previews()`.
5. `crates/wolf-ffi/tests/README.md` (~51) — new assertion(s).
6. `crates/wolf-macos/README.md` (~121) — click-to-focus + secondary statusbar line.
7. `crates/wolf-macos/Sources/README.md` (~25,46) — new maps/methods/file, PreviewSeam additions.
8. `crates/wolf-macos/Tests/README.md` (~67) — PreviewSeamTests additions.

## Implementation order
1. Lua (CHANGE 1) → 2. core display_path → 3. FFI struct/accessor/extern/call site →
4. regen bindings → 5. PreviewSeam JS/const → 6. Swift handler+PaneContainerView edits →
7. tests → 8. docs.
