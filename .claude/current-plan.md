# Current Plan — Per-buffer "Full width for content" toggle

## Status: COMPLETE ✅

Implemented, all tests green, reviewer APPROVED WITH NITS (nit fixed), docs updated + prettier-clean.
NOT committed (per standing instruction). Generated FFI bindings are staged.

## Problem statement

Add a per-buffer toggle that switches a buffer's content area between the ~90-character
default width and the full pane width, for BOTH the editor and the Markdown preview.

### Requirements

- Default for both editing and preview: content area ~90 characters wide. Editing is
  already 90 columns — leave it at 90.
- New per-buffer command **"Use full width for content"** toggles that buffer between the
  ~90-char default and full-pane width.
- **Preview in browser** always uses the ~90-char default and has NO toggle mechanism.
- There is NO global "make full width the default" mechanism — per-buffer toggle only.
- The statusbar must stay LEFT-ALIGNED with the content area in BOTH width settings.

### Confirmed design decisions (from the user)

1. **Centered everywhere.** In the ~90-char mode the content block is CENTERED in the pane
   for BOTH editor AND preview, and the statusbar indents to match the centered content's
   left edge. The editor is already capped+centered; the PREVIEW (currently flush-left /
   full-width) MUST change to capped+centered to match. In full-width mode content fills the
   pane and the statusbar sits at the pane's left edge (no indent).
2. **Toggle with menu checkmark.** Each invocation flips the active buffer between ~90-char
   and full-width. The View-menu item shows a checkmark when the active buffer is full-width.
   It is also a command-palette row.

### Source of truth

A Neovim buffer-local variable `b:wolf_full_width` (truthy = full width; absent/false =
~90-char default).

## Reconnaissance (verified — see the orchestrator's handoff for file/line detail)

- Editor 90-col cap + centering lives in `PaneGeometry` (`maxCols`), reached through the
  single choke point `PaneContainerView.geometry(for:)` (constant `maxGridCols = 90`).
  `maxCols: nil` disables the cap. ~7 call sites funnel through `geometry(for:)`.
- In-app preview WKWebView is framed to the UNCENTERED `gridRect(ofPane:)` (full inset pane,
  NO 90-cap), content flush-left via core CSS `body.wolf-markdown { margin:0; padding:0 }`.
- Browser preview serves the SAME core HTML/CSS (`markdown.rs::render_html`), which has NO
  content max-width, so it is full-width today.
- Statusbar horizontal span: `StatusbarTextSpan` (`.editor` => indent by `paintOffsetX`;
  `.preview` => flush-left).
- `PaneBinding` is per-pane only `{win, boundGrid}`; no per-buffer state cached in Swift.
- Existing nvim→Swift state channels: `wolf_current_file` (active-buffer scalar, already
  carries an `is_preview` bool as 3rd arg), `wolf_buffers` (per-window map), `wolf_preview`
  (per-preview-window map). Command lifecycle is documented in `command-lifecycle.md`.
- Command discriminants are APPEND-ONLY; current last is `PreviewInBrowser = 21`, new
  command appends as 22.

## Target design (high level — architect to detail)

- **Command** `FullWidth` (id `full_width`, `:WolfFullWidth`, title "Use full width for
  content", View menu), editor-affecting toggle flipping `b:wolf_full_width`.
- **Menu checkmark**: 4th bool arg on `wolf_current_file` → `Snapshot.current_is_full_width`
  → FFI `is_full_width_active()` → `AppDelegate.validateMenuItem`. Palette candidate.
- **Editor width**: per-window full-width map nvim→core→FFI, threaded into `geometry(for:)`
  so a pane passes `maxCols: nil` (full) or `90` (default). Stays centered when capped.
- **In-app preview**: frame the WKWebView to the capped+centered rect (reuse the editor's
  PaneGeometry cap) by the SOURCE buffer's flag (added to `wolf_preview`/`FfiPreview`);
  full-width → full inset pane. Preview statusbar switches to `.editor`-style span. In-app
  core render stays full-bleed (width_mode Full) so the FRAME does the capping.
- **Browser preview**: `render_html` gains a width mode (Capped|Full). Browser ALWAYS
  Capped (CSS `max-width` + `margin: 0 auto`, ~90 chars, centered), ignoring the flag.

## Orchestrator decisions on the architect's open questions

- **OQ-1 (preview-pane toggle target + checkmark): SOURCE BUFFER.** When the active pane is a
  preview scratch, `wolf_toggle_full_width` flips the SOURCE buffer's `b:wolf_full_width` (so
  the visible preview resizes and the document's editor + preview views stay consistent), and
  `wolf_current_file`'s 4th arg for a preview scratch reports that SOURCE buffer's flag (so the
  checkmark matches). This is the "per-buffer = per-document" semantics the user asked for.
- **OQ-2 (menu gating): ALWAYS ENABLED.** The "Use full width for content" item is always
  enabled; only its checkmark state varies from `is_full_width_active()`.
- **OQ-3 (browser CSS width): `max-width: 45rem` (≈ 90 proportional Inter chars).** Honors the
  user's explicit "~90 characters". Document the reasoning in `markdown.rs`.

## Plan (from architect — authoritative)

### 2. Command registry — `crates/wolf-core/src/command.rs`
- Append `FullWidth` LAST in `enum Command` (after `PreviewInBrowser`); doc: "Toggle the active
  buffer between the ~90-char default content width and full pane width (`b:wolf_full_width`)."
  Update the enum doc list to add `FullWidth`(22).
- Append `COMMAND_SPECS` row LAST: `command: Command::FullWidth`, `id: "full_width"`,
  `ex_command: "WolfFullWidth"`, `title: "Use full width for content"`, `menu: MenuGroup::View`.
- Tests (mirror `preview_in_browser_*`, demote the last-row guard): `EXPECTED_COMMAND_COUNT`
  22→23; add `FULL_WIDTH_INDEX = 22`; rewrite `preview_in_browser_is_the_last_row` →
  `_is_at_index_21` (drop len()-1); add `full_width_id_round_trips`, `full_width_spec_fields`,
  `full_width_is_the_last_row`.

### 3. Editor-affecting toggle round-trip (flavor b)
Chain: `WolfCoreHandle::toggle_full_width` → `WolfCore::toggle_full_width` →
`InputEvent::ToggleFullWidth` → `input.rs` apply arm → `wolf_toggle_full_width()` Lua helper.
- `input.rs`: add `InputEvent::ToggleFullWidth` (no fields) near `ToggleTerminal`; apply arm calls
  async `toggle_full_width(nvim)` → `nvim_exec_lua("wolf_toggle_full_width()", [])`.
- `api.rs`: `pub fn toggle_full_width(&self)` → `send_input(InputEvent::ToggleFullWidth)`.
- `runtime/nvim/init.lua`: `function _G.wolf_toggle_full_width()` — resolve effective buffer
  (if current buffer has `wolf_preview_source_bufnr`, target the SOURCE buffer — OQ-1); flip
  `vim.b[buf].wolf_full_width`; re-emit `wolf_emit_current_file()`, `wolf_emit_full_width()`,
  `wolf_emit_previews()`. Define alongside `wolf_toggle_terminal`.

### 4. FFI wiring — `crates/wolf-ffi/src/lib.rs` (+ regen)
- `command_kind::FULL_WIDTH = 22`; `FfiCommand::from_command` arm `Command::FullWidth =>
  command_kind::FULL_WIDTH`.
- Handle methods: `toggle_full_width(&self)`, `is_full_width_active(&self)->bool`,
  `full_width_for_win(&self, win:i64)->bool`. `FfiPreview` gains `source_full_width()->bool`.
- Regen: `bazel run //crates/wolf-ffi:gen_bindings`, commit `crates/wolf-ffi/generated/` BEFORE
  Swift build.
- `crates/wolf-ffi/tests/commands.rs`: `CMD_KIND_FULL_WIDTH = 22`, extend contract array to
  `0..=22`, add a `:WolfFullWidth` drain step.

### 5. Menu-checkmark scalar (active buffer) — `wolf_current_file` 4th arg
- `render.rs`: `Snapshot.current_is_full_width: bool` + `is_full_width_active()`; unit test.
- `redraw.rs` `apply_current_file`: read 4th bool arg → `current_is_full_width` (no `seq` bump).
- `api.rs`: `is_full_width_active()`. FFI accessor (§4).
- `init.lua` `wolf_setup_current_file_tracking`: add 4th arg to BOTH branches (preview-scratch
  reports the SOURCE buffer's flag — OQ-1; normal reports `vim.b[buf].wolf_full_width == true`).

### 6. Per-window full-width map (editor panes) — NEW `wolf_full_width` notification
RECOMMENDED: a dedicated sibling notification (NOT extending `wolf_buffers`, which skips preview
& unnamed windows).
- `render.rs`: `Snapshot.full_width: HashMap<i64,bool>` + `full_width_for_win(win)`.
- `redraw.rs`: `FULL_WIDTH_EVENT = "wolf_full_width"`; route to `apply_full_width` (mirror
  `apply_buffers`; wholesale-replace; no `seq` bump; build map before write lock).
- FFI: `full_width_for_win(win)` handle method.
- `init.lua`: `wolf_setup_full_width(chan)` mirroring `wolf_setup_buffer_tracking` — emit
  `{win, vim.b[buf].wolf_full_width==true}` for ALL windows on
  BufEnter/BufWinEnter/WinEnter/WinNew/WinClosed; expose `_G.wolf_emit_full_width`.
- `nvim.rs`: call `wolf_setup_full_width(chan)` in the attach sequence next to
  `wolf_setup_buffer_tracking`.

### 7. Preview source-buffer flag — `wolf_preview` 4th tuple element
- `init.lua` `emit_previews`: tuple `{win, source_bufnr, src, vim.b[source_bufnr].wolf_full_width
  == true}`.
- `render.rs` `PreviewInfo.source_full_width: bool`; `redraw.rs` `apply_previews` reads 4th elem.
- FFI `FfiPreview.full_width` + `source_full_width()` + `from_preview` extension.

### 8. macOS front-end
- `CommandSeam.swift`: `case fullWidth`; `init?(ffi:)` `case 22`.
- `CommandDispatcher.swift`: `case .fullWidth: controller?.toggleFullWidth()`.
- `WolfWindowController.swift`: `var isFullWidthActive` (→ `is_full_width_active()`);
  `func toggleFullWidth()` → `handle?.toggle_full_width(); paneContainer?.onRenderWake()`.
- `AppMenu.swift`: `Tag.fullWidth = 22`; View-menu item "Use full width for content", no key
  equiv, after Preview in Browser.
- `AppDelegate.swift` `validateMenuItem`: `case .fullWidth:` set checkmark from
  `isFullWidthActive`, `return true` (ALWAYS ENABLED — OQ-2).
- `PaneContainerView.swift`: change `geometry(for:)` → `geometry(for:fullWidth:)`
  (`maxCols: fullWidth ? nil : Self.maxGridCols`); add `previewFullWidthByGrid` cache (populate/
  clear in `syncPreviewWebViews`); add `fullWidth(forWin:)`/`fullWidth(forGrid:)` resolvers;
  update ALL ~7 call sites (mouseDown/Dragged/Up, scrollWheel, syncGridSizes, syncPreviewWebViews,
  paintGrid via draw() loop, paintStatusbar) to pass the resolved per-pane flag; frame the preview
  webview to `geometry(…, fullWidth: source_full_width()).gridRect` (capped+centered when default,
  full inset when full-width); align the occlusion test (line ~1361) to the same capped rect.
- `StatusbarTextSpan.swift`: REDEFINE `.preview` to mirror `.editor` (`left = gridLeft + offset;
  width = w - offset`) since the preview is now centered like the editor; update header/doc
  comments (previously asserted preview is flush/uncentered).

### 9. Browser + core CSS width mode — `crates/wolf-core/src/markdown.rs`
- Add `pub enum WidthMode { Capped, Full }` (`Default = Full`). `render_html(source, theme,
  width_mode)`: Full → `""` (full-bleed, current); Capped → append after theme CSS
  `body.wolf-markdown{max-width:45rem;margin-left:auto;margin-right:auto;padding-left:24px;
  padding-right:24px}` (only HORIZONTAL props, preserving the vertical baseline grid). Document
  the 45rem ≈ 90-Inter-char reasoning (OQ-3).
- Callers: `nvim.rs::render_source_html` gains a `width_mode` param.
  `NvimProcess::render_preview` (IN-APP) passes `WidthMode::Full` (webview frame caps it);
  `browser_preview.rs::render_pinned_live` (BROWSER) passes `WidthMode::Capped`; the static
  `.md` disk path passes `WidthMode::Capped`.

### 10. Palette — `crates/wolf-core/tests/palette_rank.rs`
- `STATIC_COMMAND_COUNT` 21 → 22.

### 11. Tests (test-writer)
Rust: `command.rs` (§2); `wolf-ffi/tests/commands.rs` (§4); `palette_rank.rs` (§10);
`markdown.rs` `render_html_full_is_flush` + `render_html_capped_centers`; `render.rs` snapshot
accessor tests (`is_full_width_active_*`, `full_width_for_win`); NEW integration test
`crates/wolf-ffi/tests/full_width.rs` (real nvim: `:WolfFullWidth` drains the command kind; direct
`handle.toggle_full_width()` flips `is_full_width_active()` + `full_width_for_win(active_win)`;
`previews()` reports `source_full_width()`) with a `//crates/wolf-ffi:full_width` BUILD target
copied from `commands`.
Swift (WolfKeyMap headless): `CommandSeamTests` (round-trip `.fullWidth`/22 + nil boundary);
`StatusbarTextSpanTests` (preview now centered/indented; keep a full-width offset==0 case);
confirm a `PaneGeometry` `maxCols: nil` test exists (add if missing).

### 12. Docs (first-class)
`command-lifecycle.md` (enum list, command_kind list, dispatcher, add FullWidth as a flavor-b
worked example, checklist counts); `architecture.md` §10 (twenty-three commands 0..22; the 3 new
state channels; preview/typography section: capped+centered in-app + editor + always-90 centered
browser + `WidthMode`); crate READMEs (`wolf-core/src`, `wolf-ffi`, `wolf-macos/Sources`,
`wolf-macos/Tests`, `runtime/nvim`); NEW `full-width-decisions.md` capturing the decisions.
`prettier --write` all edited Markdown; no hard line breaks in paragraphs.

### 13. Implementation order (avoid compile breaks)
command.rs → render.rs → input.rs/api.rs → redraw.rs → markdown.rs → nvim.rs → browser_preview.rs
→ init.lua (+ nvim.rs attach wiring) → wolf-ffi/src/lib.rs → **regen bindings + commit generated/**
→ wolf-ffi tests + palette_rank → Swift sources → Swift tests → docs + prettier.

### 16. Risks / constraints
- Regen bindings + commit `generated/` BEFORE the Swift build (hard ordering).
- Append-only discriminant: `FullWidth == 22` everywhere; never reorder.
- `StatusbarTextSpan.preview`, `syncPreviewWebViews` webview frame, and the occlusion test all
  derive from the SAME `gridRect(ofPane:)` + `geometry(…, fullWidth:)` — they MUST change together.

## Progress log

- Orchestrator: problem statement written; architect plan received and adopted; OQ-1/2/3 resolved
  (source-buffer toggle, always-enabled menu, 45rem). Invoking test-writer next.
- Test-writer DONE. Files: command.rs (+FULL_WIDTH_INDEX, round-trip/spec/last-row, count 23),
  render.rs (is_full_width_active + full_width_for_win tests), markdown.rs (full_is_flush +
  capped_centers; migrated 12 existing render_html calls to 3-arg WidthMode::Full), palette_rank.rs
  (STATIC_COMMAND_COUNT 22), wolf-ffi/tests/commands.rs (CMD_KIND_FULL_WIDTH=22, 0..=22,
  :WolfFullWidth drain), NEW wolf-ffi/tests/full_width.rs (2 e2e tests incl. preview source flag) +
  BUILD.bazel target, CommandSeamTests (.fullWidth==22, boundary 23=nil), StatusbarTextSpanTests
  (.preview now centered/indented like .editor). PaneGeometryTests already cover nil+capped.
  Tests reference not-yet-implemented symbols (expected TDD). Invoking coder next.
- Coder DONE (everything compiles; bindings regenerated + staged). Results: wolf-ffi:all 31/31
  incl. real-nvim `full_width` e2e; wolf-macos WolfKeyMapTests PASS; palette_rank PASS; clippy/
  rustfmt/swift_format PASS; dep-guard clean. wolf_core_lib_test 144/1 (only render_html_full_is_flush
  fails — TEST BUG); markdown_render integration target fails to BUILD (test-writer missed migrating
  its ~9 2-arg render_html calls — TEST BUG). §12 docs NOT done (coder scope was §2–§10).
- Orchestrator decision: both are test-file bugs → route to test-writer (coder may not edit tests).
  Fix A: render_html_full_is_flush assert `!contains("max-width:45rem")` (base.css has a real
  `img{max-width:100%}`). Fix B: append `, WidthMode::Full` (import `wolf_core::WidthMode`) to the
  ~9 `render_html(...)` calls in `crates/wolf-core/tests/markdown_render.rs`. Then re-run coder.
- Test-writer DONE (Fix A: assertion → `!contains("max-width:45rem")`; Fix B: migrated ALL 20
  render_html calls in markdown_render.rs + WidthMode import). Re-running coder on wolf-core targets.
- Coder re-run DONE — ALL GREEN. wolf-core:all 22/22 (lib 145/0, markdown_render 28/0,
  palette_rank 9/0), rustfmt_core clean. Combined with prior run: wolf-ffi:all 31/31 (real-nvim
  e2e incl.), wolf-macos WolfKeyMapTests PASS, clippy/swift_format/dep-guard clean, bindings
  regenerated+staged. Implementation COMPLETE + tests GREEN. Invoking reviewer next; §12 docs still TODO.
- Reviewer DONE — VERDICT: APPROVED WITH NITS. All 6 areas verified (discriminant contract, 3 state
  channels end-to-end, toggle round-trip incl. OQ-1 source-buffer redirect, geometry threading with a
  single per-pane fullWidth resolver `fullWidth(forGrid:)`, statusbar alignment, browser-vs-in-app
  WidthMode). No Blockers, no Should-fix. ONE nit: stale inline comment at
  `crates/wolf-ffi/src/lib.rs:813` (`FfiCommand::kind()` upper bound should read `FULL_WIDTH`, not
  `PREVIEW_IN_BROWSER`). Fold into the docs pass. Code doc-comments otherwise clean (no stale
  flush/uncentered preview assertions remain). Invoking technical-writer for §12 docs + the nit.
- Technical-writer DONE — 11 Markdown files updated (new full-width-decisions.md;
  command-lifecycle.md; architecture.md; wolf-core/{README,src/README}; wolf-ffi/{README,src/README};
  wolf-macos/{README,Sources/README,Tests/README}; runtime/nvim/README) + fixed the lib.rs:813
  comment nit. Verified docs against actual code — NO plan-vs-code discrepancies. Orchestrator ran
  `prettier --write` on all 11 files → all "unchanged" (already format-clean). FEATURE COMPLETE.
