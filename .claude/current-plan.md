# Current Plan: Markdown Preview (web-view content in a pane)

## Status: COMPLETE ✅

## Finalization
- Technical-writer synced all docs to the ACTUAL code (agent ac65585e26cfb492b): `architecture.md`
  (Decision 10 + `command_kind` 18, "eighteen"→"nineteen", second-pane-content-kind + core-side
  render note, "Output: preview" contract, macOS preview panes, deferred scroll-sync/multi-theme),
  `wolf-core` README + `src`/`tests` READMEs, `wolf-ffi` README + `src`/`tests` READMEs, `wolf-macos`
  README + `Sources`/`Tests` READMEs. `runtime/nvim/README.md` was MISSED in the first pass (the
  init.lua preview helpers) and updated in a second technical-writer pass (verified against
  init.lua: `wolf_open_preview`/`wolf_setup_preview`, the scratch-buffer scheme + E95 disambiguation,
  the `FileType`/filetype-arg addition, the preview-buffer skip guards). Orchestrator ran
  `prettier --write` on all 11 touched Markdown files → all already conformant (`proseWrap: never`).
- **OD13 — palette gating: LEAVE PREVIEW IN THE PALETTE (user decision).** The tech-writer surfaced
  that `rank_palette` does NOT gate the Preview row on Markdown (only the native menu gates it, via
  `validateMenuItem`/`is_markdown_active`). This diverges from the architect plan (which called for
  threading `is_markdown` into the palette seed) BUT matches the app's existing precedent — `Copy` is
  visual-mode-gated in the menu yet always shown in the palette. Asked the user; user chose to KEEP
  Preview always listed in ⌘P (off-Markdown it's a harmless no-op; the menu already honors "only
  available when viewing a Markdown file"). No code change; the coder's implementation is RATIFIED as
  intended and the docs already describe it correctly.

## Status history: CODE COMPLETE & GREEN — technical-writer done

Nit fixes landed: OD10 (nvim.rs rustdoc), OD11 (init.lua basename-collision pcall+disambiguate,
`Preview: alpha.md` assertion still holds, `//crates/wolf-ffi:preview` PASS), OD12
(`PickerRefreshCoalescer`-debounced preview reloads in PaneContainerView, `Wolf` app builds).
`//crates/wolf-core:markdown_render` PASS (untouched). All prior regression targets remained green.

Reviewer agent id: a6b68a385b67d7638. Verdict APPROVED-WITH-NITS — NO blockers, full plan/OD
conformance verified (marker literal + OD4 char-column math + OD8 `< range.end` collapse; denylist
clean w/ sound glib-vs-glibc reasoning; append-only 18 intact core↔ffi↔swift; scratch-buffer skip
guards; split-lands-right confirmed independent of `splitright`; web-view lifecycle no leaks).

## Orchestrator rulings on the reviewer's nits (BINDING)

- **OD10 — FIX the stale `nvim.rs` rustdoc (nit #1).** `crates/wolf-core/src/nvim.rs:~603–611`
  `setup_current_file_tracking` doc still claims only `BufEnter`/`BufWinEnter`/`BufFilePost` and no
  filetype arg; the change added `FileType` + `vim.bo.filetype`. Coder updates the inline rustdoc to
  match `init.lua` (README-level sync stays the technical-writer's job).
- **OD11 — FIX the basename buffer-name collision (nit #2).** `runtime/nvim/init.lua:~644`
  `nvim_buf_set_name(preview_buf, "Preview: "..basename)` collides for two sources sharing a basename
  (E95 → `open_preview` silently returns 0 for the second). Guard the set-name and, ONLY on
  collision, fall back to a disambiguated name (append the source bufnr or parent-dir hint — coder's
  choice; precedent = `TabTitleSeam` collision disambiguation). The NON-colliding common case MUST
  still name the buffer exactly `Preview: <basename>` so `//crates/wolf-ffi:preview`'s
  `Preview: alpha.md` assertion holds. Re-run `//crates/wolf-ffi:preview`.
- **OD12 — HONOR OD6's explicit coalescer (nit #3).** Wire preview reloads through
  `PickerRefreshCoalescer` in `PaneContainerView` so a fast-typing burst yields one blocking
  `render_preview` main-thread round-trip, not one per wake (the per-window tick gate stays as the
  redundant-reload guard; the coalescer bounds the RATE). Compile-verify (`bazel build
  //crates/wolf-macos:Wolf`); this layer stays manual-smoke. Keep the provided-but-unused
  `previewsNeedingReload` seam as-is (OD9) — the technical-writer will note it as unused.

## Status: GREEN + OD7/OD8 TESTS LANDED — reviewer done

OD7/OD8 test pass complete: test-writer added `per_hundred_column_marker_counts_characters_not_bytes`
+ `container_collapses_interior_line_markers_before_its_opening_tag` to `markdown_render.rs`; coder
ran `//crates/wolf-core:markdown_render` → 10/10 PASS, no impl change needed. Coder recs #1/#3 now
satisfied by tests; #2 (`previewsNeedingReload`) declined per OD9.

Coder agent id: a0700297669ce010a. All targeted + regression Bazel targets PASS; denylist CLEAN
(pulldown-cmark 0.12.2 + pulldown-cmark-escape only, both GUI-free); bindings regenerated,
`wolf_ffi_wake.h` byte-identical. `bazel build //crates/wolf-macos:Wolf` builds.

## Orchestrator rulings on the coder's recommendations (BINDING)

- **OD7 — ACCEPT the multibyte OD4 render test.** OD4 (0-based CHARACTER columns) was an explicit
  ruling; only an in-module test currently covers the byte-offset math. Add a PUBLIC-API
  (`wolf_core::render_html`) case: a `>=100`-CHARACTER line built from 2-byte chars still yields
  `id="wl-{L}-100"` (and the byte offset is computed from char count, not byte count). Test-writer
  adds it to `crates/wolf-core/tests/markdown_render.rs`.
- **OD8 — ACCEPT the container-collapse marker test (pins the coder's judgment call #1).** The plan
  text (`offset <= range.start`) and its own multi-line-paragraph test were inconsistent; the coder
  implemented `offset < range.end` (a block's interior line markers all flush before that block's
  opening tag), satisfying every existing test. This is the correct v1 collapse granularity and is
  now load-bearing → pin it: add a nested-list (or blockquote) case asserting every interior line's
  `wl-{L}-0` id EXISTS and all sit before the container's opening tag (accepted v1 consequence:
  container interior markers collapse to the top of the container). Test-writer adds it to
  `markdown_render.rs`. The coder's `< range.end` rule is RATIFIED as the intended behavior — do not
  change the implementation.
- **OD9 — DECLINE the `previewsNeedingReload` test.** That pure helper is NOT on the actual reload
  path (`PaneContainerView` gates reload on the preview WINDOW handle so two panes previewing the
  same source each reload). Testing an off-path helper adds coverage for effectively dead code.
  Leave the helper as a documented optional seam (harmless); no test. If it stays unused after the
  reviewer pass, the technical-writer should note it as a provided-but-unused seam (or the reviewer
  may recommend deletion).

## Status: PLAN APPROVED — orchestrator rulings recorded

Architect agent id: a3f4746a1f6a75a90 (SendMessage to continue with context intact).

## Problem statement

Add the ability to preview a Markdown file as rendered HTML inside a Wolf pane. Until now a
Wolf view/pane only ever paints a Neovim window grid; this introduces a **second kind of pane
content: a web view**. A new "Preview" command renders the active Markdown buffer to HTML and
shows it in a web view; the preview **live-updates** as the user edits the Markdown.

### Confirmed design decisions (from the user)

1. **Platform scope: macOS ONLY.** Implement in the realized AppKit shell using `WKWebView`.
   GNOME (WebKitGTK) is a deferred parity pass — out of scope for this change. This matches how
   mouse/scroll, the per-pane statusbar, and state persistence all landed macOS-first.

2. **Views/panes now show a SECOND kind of content: a web view.** The pane model must generalize
   from "always a Neovim window grid" to "either a grid OR a web preview". The architect decides
   how this is represented (e.g. per-pane content-kind on the front-end pane model) while keeping
   `wolf-core` GUI-free.

3. **New command: "Preview".** A user-triggered command, **only available/enabled when the active
   buffer is a Markdown file** (menu item + palette row gated on Markdown; the architect specifies
   the "is this a Markdown buffer" signal — e.g. filetype/extension from Neovim). Follows the
   existing one-canonical-id → two-faces Command model (`COMMAND_SPECS`, append-only discriminant).

4. **Pane placement:**
   - If the window has a **single pane**, open the preview in a **new pane to the RIGHT** (a
     vertical split, preview on the right).
   - If the window has **more than one pane**, open the preview in the **active pane** (replacing
     that pane's content with the web view).

5. **Live-updating.** The preview re-renders as the user edits the source Markdown buffer (on
   buffer change and/or write — the architect specifies the exact trigger and the Neovim→shell
   signal that carries updated buffer content / a change notification).

6. **Rendering: in the RUST CORE.** `wolf-core` converts the buffer's Markdown text → HTML using a
   pure-Rust Markdown crate (e.g. `pulldown-cmark`), staying GUI-free (no GUI toolkit enters core's
   dependency tree — verify against the denylist). Themes are CSS the core assembles/wraps around
   the rendered HTML body. The front-end web view just loads the HTML string core produces.

7. **HTML themes — plan ahead, ship ONE.** Design the theming so multiple HTML/CSS themes can be
   added later, but implement exactly ONE theme now. The architect specifies where themes live
   (core-side CSS) and the (single-value-for-now) selection mechanism.

8. **Buffer list shows previews as `Preview: <filename>`.** A preview entry appears in the buffer
   list / buffer picker labeled `Preview: <filename>` (the source Markdown file's name). The
   architect specifies how a preview is represented so it surfaces in the buffer picker with that
   label WITHOUT being a real editable Neovim buffer (or, if it is backed by a scratch/nofile
   buffer, how that is named and kept non-editable).

### Scroll-sync (cursor-follow) — DESIGN NOW, BUILD LATER

The user wants a mechanism so the preview keeps the content the user is editing in view. The
approach:

- The emitted HTML carries **invisible position markers** (anchors / hidden elements) whose names
  encode a position in the ORIGINAL Markdown source: (a) the **hard line** (buffer line, NOT a
  screen/wrapped line), and (b) the **column**. There is a marker for **every hard line** in the
  file and an additional marker **every 100 columns**.
- Later, a cursor-position signal from Neovim will let the web view scroll so the marker for the
  user's current (line, column) stays visible, and it will **only scroll when that target nears /
  crosses a viewport-edge threshold** (not on every keystroke).

**Scope for THIS change (confirmed by the user — "design now, build later"):**

- The architect MUST specify the full anchor/marker scheme now, and the core's HTML emission MUST
  include these per-line and per-100-column markers **from day one**, so the emitted HTML is
  forward-compatible.
- The **JS cursor-follow scrolling behavior + the Neovim→web-view cursor-position signal ship as a
  SEPARATE follow-up task** — NOT implemented in this change. This change delivers a live-updating
  preview whose HTML already contains the markers, but does not yet auto-scroll to the cursor.

## Key facts about the current implementation (from READMEs — architect to verify in code)

- **wolf-core** is GUI-free (enforced denylist). Owns the captive `nvim --embed`, the multigrid
  `Snapshot`, the `Command` enum + `COMMAND_SPECS` (18 rows today, append-only discriminant shared
  with `wolf-ffi` `command_kind` and Swift `CommandSeam`/`CommandKind`), `InputEvent`, the
  `init.lua` setup helpers, buffer tracking (`wolf_buffers` → `Snapshot.buffers`), current-file
  tracking (`Snapshot.current_file`), layout intents, etc.
- **Pane content today is ALWAYS a Neovim window grid.** macOS `PaneContainerView` is a single flat
  `NSView` that paints every pane's window grid via `read_frame_for_grid`, driven by a pure
  `PaneTree` layout. Splits come from nvim `wolf_layout` intents (`:vsplit` etc.). The web-view
  pane is a NEW content kind the front-end must host (a `WKWebView` subview positioned over the
  pane rect) alongside the painted grids.
- **Buffer picker** = `wolf_core::buffer_picker` (`list_buffers()` → `getbufinfo({'buflisted':1})`
  → `BufInfo`; pure `rank_buffers`), bridged as `FfiBufferPicker`. Buffer-list label `Preview:
  <filename>` must be reconciled with this snapshot-of-nvim-buffers model.
- **Command flow:** one canonical snake_case id in `COMMAND_SPECS` → `:Wolf<Pascal>` Ex-command +
  `require('wolf').<snake>()` Lua fn → `wolf_command` rpcnotify → `Command` on a drainable queue →
  FFI `command_kind` u8 → Swift `CommandDispatcher`. Menu-item enable/disable via
  `NSMenuItemValidation` (e.g. Copy gated on visual mode via `is_visual_mode()`) is the precedent
  for gating Preview on "is Markdown".
- **FFI** = swift-bridge, confined to `wolf-ffi`; generated Swift/C bindings committed under
  `crates/wolf-ffi/generated/`; regenerate via the Bazel codegen target after any bridge change.
  New FFI should be ADDITIVE (new methods / one appended discriminant), never renumbering.
- **macOS builds wolf-core/wolf-ffi via Bazel**; wolf-gnome has no macOS Bazel target and is out of
  scope here.

## Constraints

- Documentation is the source of truth; keep every touched dir's README + `architecture.md` in
  sync (technical-writer at the end).
- Markdown docs: no hard line breaks within a paragraph; run `prettier --write` after editing.
- **wolf-core one hard rule:** no GUI toolkit in its dependency tree — re-verify the denylist after
  adding the markdown crate (`pulldown-cmark` or the architect's choice must be GUI-free).
- Command / `command_kind` discriminant contract is APPEND-ONLY (Preview appends at the end).
- Regenerate the committed swift-bridge bindings after any bridge change.
- Prefer e2e/integration tests that spawn a REAL pinned nvim (per CLAUDE.md); pure HTML/markdown
  rendering + theming + the marker scheme are headlessly unit-testable in Rust and SHOULD be.
- Scroll-sync JS + cursor signal are OUT OF SCOPE (design-only) for this change.

## Implementation areas (to be detailed by architect)

1. Core: Markdown→HTML rendering module (pulldown-cmark or equiv), CSS theme wrapping (one theme,
   pluggable), the per-line/per-100-col marker emission, an "is the active buffer Markdown" signal,
   a live buffer-content/change signal to drive re-render, and the Preview `Command`
   (`COMMAND_SPECS` append).
2. FFI: bridge the rendered-HTML accessor + the "is Markdown" gate + any preview-open signal;
   append the `command_kind` discriminant; regenerate bindings.
3. Swift front-end: generalize the pane content model to host a `WKWebView`; the Preview command
   handler with the single-pane→split-right vs multi-pane→active-pane placement; live-update wiring;
   Preview menu item + palette gating on "is Markdown"; buffer-list `Preview: <filename>` entry.
4. Tests: Rust unit tests for render/theme/markers/is-markdown; real-nvim integration where a buffer
   drives HTML; FFI round-trip; pure Swift seams (placement decision, buffer-list label) headless;
   AppKit WKWebView wiring compile + manual smoke.
5. Docs: sync every touched README + architecture.md; note scroll-sync as a designed-but-deferred
   follow-up.

## Progress log

- (init) Requirements clarified with the user (platform=macOS only; live-updating; render in Rust
  core; scroll-sync design-now-build-later with per-line + per-100-col markers). Problem statement
  written. Invoking architect.
- Architect plan complete (appended below). Orchestrator rulings OD1–OD6 recorded (all six of the
  architect's open questions ruled on, none require re-consulting the user). Invoking test-writer.
- Test-writer complete (agent id a3d8854beb5c0c471). Created: `crates/wolf-core/tests/markdown_render.rs`
  (8 pure render/marker cases through the public API), `crates/wolf-ffi/tests/preview.rs` (real-nvim
  gate/open/list/live-update), `.../WolfKeyMapTests/PreviewSeamTests.swift`. Edited test modules:
  `command.rs` (count→19, PREVIEW_INDEX=18, round-trip/spec/last-row), `render.rs` (added a
  `#[cfg(test)]` module — `is_markdown_active`), `wolf-ffi/tests/commands.rs` (`:WolfPreview`→18,
  `0..=18` guard), `CommandSeamTests.swift` (ffi 18→.preview, 19→nil). Reconciliation notes passed to
  coder (below). XCTest SourceKit warnings are the known editor-only artifact (Bazel resolves XCTest).
  Invoking coder.

### Test-writer reconciliation notes the CODER must honor
- `FfiPreview` must be re-exported from the wolf-ffi crate root (like `FfiBufferRow`); `preview.rs`
  imports `wolf_ffi::FfiPreview`.
- Marker span is the plan's literal `<span class="wolf-src" id="wl-{L}-{C}" data-line data-col></span>`
  (double-quoted `id`, a real `</span>` close — NOT self-closing); the "immediately-before-`<h1>`" test
  depends on this exact form.
- `open_preview("active")` on the reuse path is asserted to return the SAME scratch bufnr as the
  first `open_preview("split_right")` (create-or-reuse one buffer per source). Keep that contract.
- Bazel targets `//crates/wolf-core:markdown_render` and `//crates/wolf-ffi:preview` do NOT exist yet
  — the CODER adds both `rust_test` targets (test-writer intentionally left BUILD files alone).
- `CommandKind` is a plain (non-RawRepresentable) Swift enum; the contract is asserted via
  `CommandKind(ffi: 18) == .preview` (no `.rawValue`). Keep it plain unless there's reason to change.

---

# Orchestrator rulings on the architect's open questions (BINDING — all agents follow these)

- **OD1 — ADOPT Design S (scratch-buffer-backed preview pane).** The preview pane IS a real nvim
  `nofile` / `nomodifiable` / `buflisted` scratch buffer named `Preview: <basename>` carrying
  `b:wolf_preview_source`, shown in a real pane; the front-end overlays a `WKWebView`. Rationale:
  requirement 8 (buffer-picker `Preview: <filename>` row) falls out of the existing pure-Rust
  `list_buffers`/`rank_buffers` with zero front-end injection; both placement cases reuse existing
  seams (the `:vsplit` `wolf_layout` intent path + window-scoped `nvim_win_set_buf`);
  `PaneTree`/reconciliation/resize/close/dim stay untouched; core stays the single source of truth.
  Design F (pure front-end pseudo-pane) is REJECTED — it would force synthesizing the buffer-picker
  row front-end-side and teach `PaneTree`/`paneRects`/`syncGridSizes`/intent-application about
  non-grid leaves, a deeper and riskier reconciliation change. The skip-guards Design S requires
  (current-file / buffer / statusbar trackers ignoring `b:wolf_preview_source` buffers) are the
  accepted cost and MUST be implemented as the architect specified.

- **OD2 — NO key equivalent for the Preview menu item in this cut.** The user did not request a
  shortcut; `⌘P` is Command Palette. Ship View > Preview with no key equivalent; a shortcut can be
  assigned later. (Surfaced to the user as a courtesy in the final report.)

- **OD3 — RATIFY the literal multi-pane semantics.** With >1 pane, Preview replaces the ACTIVE
  pane's buffer with the preview scratch buffer (via `nvim_win_set_buf`), exactly as requirement 4
  states. If the active pane was showing the source markdown, that pane switches to the preview; the
  source is still editable wherever else it is shown and live-update still works. This is the
  intended UX per the locked requirement. (Surfaced in the final report so the user can revisit.)

- **OD4 — Marker columns are 0-based CHARACTER offsets** (not byte offsets). Correct for multibyte
  lines. `desired_markers` computes each `(L, C)` byte offset as line-start-byte + byte-length of
  the first `C` characters of that line. The DEFERRED cursor signal will convert nvim's byte-column
  to a character-column before looking up `#wl-{L}-{C}` — document this in the forward-compat note.

- **OD5 — The single shipped preview theme is a FIXED CSS theme, NOT wired to the live nvim
  colorscheme** (`WolfTitlebar`/`WolfCursor`). Wiring the preview to the app theme is an explicit
  designed-for-later extension. Ship one `PreviewTheme::Default` (dark, to sit alongside the bundled
  lighthouse look), structured (enum + `theme_css` + `include_str!` CSS files) so more themes and a
  config-driven selector drop in later.

- **OD6 — Live-on-change re-render (CONFIRMED by the user's "live-updating" choice).** Drive the
  dirty signal from `TextChanged` / `TextChangedI` (plus `BufWritePost`) on markdown source buffers,
  debounced through `PickerRefreshCoalescer` so a keystroke burst yields one reload. Do NOT restrict
  to `BufWritePost`-only.

None of OD1–OD6 require re-consulting the user; OD2 and OD3 are surfaced in the final report as
courtesies. The scroll-sync JS + Neovim→web-view cursor signal remain OUT OF SCOPE (design-only) per
the locked requirement; the emitted HTML MUST still carry the markers from day one.

---

# Architect Plan (authoritative — all agents implement against this + the OD1–OD6 rulings)

> Source: architect agent a3f4746a1f6a75a90. Where this plan lists an "open question," the OD ruling
> above is BINDING and overrides any hedging in the plan text.

## Summary

Add a live-updating rendered-Markdown preview shown inside a Wolf pane. `wolf-core` gains a pure,
GUI-free Markdown→HTML renderer (`pulldown-cmark`) that wraps the rendered body in a themed HTML
document and embeds forward-compatible per-line / per-100-column invisible position markers. A new
append-only `Preview` command (discriminant 18), gated on the active buffer being Markdown, opens
the preview: single-pane windows split a new pane to the right and show it there; multi-pane windows
show it in the active pane. The preview pane is backed by a real Neovim scratch buffer named
`Preview: <filename>` (so it surfaces in the buffer picker for free and reuses the existing split +
`nvim_win_set_buf` seams), and the macOS front-end overlays a `WKWebView` on any pane whose window
shows a preview buffer, reloading it from core-rendered HTML whenever the source buffer changes. The
JS cursor-follow scrolling and the Neovim→web-view cursor signal are designed here but NOT built
(deferred follow-up); the emitted HTML already carries the markers.

## Design decisions (with rationale)

**Markdown crate — `pulldown-cmark` (GUI-free).** Pure Rust; transitive deps pull in no GUI toolkit
so the denylist stays byte-identical. Decisive feature: `Parser::into_offset_iter()` yields
`(Event, Range<usize>)` with source byte offsets — the basis of the marker scheme. Re-verify after
adding via `cargo tree -e no-dev -p wolf-core` and the Bazel `deps()` grep; DOCUMENT it exactly like
the existing `rusqlite`/`ignore`/`serde` notes.

**"Is the active buffer Markdown" — nvim `filetype == "markdown"`,** reusing the existing
`wolf_current_file` path (extend that autocmd to also fire on `FileType` and send `vim.bo.filetype`
as a 2nd arg). Core folds it into `Snapshot.current_filetype: String`;
`Snapshot::is_markdown_active()` mirrors `is_visual_mode()`. The gate lives in the front-end:
`AppDelegate.validateMenuItem` disables Preview unless the key window reports
`handle.is_markdown_active()` (the Copy-gated-on-visual-mode precedent). For the palette row, thread
an `is_markdown` bool into the palette seed and drop the Preview `Static` row in `rank_palette` when
false (additive param).

**Web-view host (macOS) — `WKWebView` SUBVIEWS over the pane rect.** A `WKWebView` is a real view,
not paintable into the flat `PaneContainerView`, so preview web views are subviews in a
`[Int64 win: WKWebView]` map. On each `onRenderWake` after `paneRects`: for each pane whose bound
window is a preview win, ensure a `WKWebView`, frame it to the pane's GRID rect (pane rect minus the
statusbar strip and `EditorInset` padding, reusing `EditorInset`/`StatusbarLayout`), and
`loadHTMLString(html, baseURL: nil)` when the source's dirty tick advanced. Coexistence: the blank
scratch grid paints underneath (occluded, cheap); a dim overlay subview when the pane is not active;
the top statusbar strip still paints but shows `Preview: <basename>`; suppress the synthetic
scrollbar for preview panes; focus stays nvim-window-driven (clicking inside the web view not
re-routing Wolf focus is a v1 limitation).

**Preview pane representation — nvim scratch buffer + web overlay (OD1 = Design S).** The preview
pane is an ordinary pane bound to a real nvim window/grid showing a dedicated scratch buffer
(`nvim_create_buf(listed=true, scratch=true)`, `buftype=nofile`, `bufhidden=hide`,
`modifiable=false`, `swapfile=false`), named `Preview: <basename>`, carrying
`b:wolf_preview_source = <abs source path>`. A per-source→preview-buffer Lua map makes re-invoking
reuse the buffer. Consequences: (a) it is an ordinary pane so `PaneTree`/`paneRects`/`syncGridSizes`/
dim/close/resize/focus work unchanged; (b) it is `buflisted` so `list_buffers()` → `rank_buffers`
includes it as `Preview: <basename>` with NO front-end injection, and activating that picker row
`switch_buffer`s a pane to it (a consistent round-trip); (c) it has a real grid so reconciliation is
untouched. The path-based trackers (`wolf_current_file`, `wolf_buffers`) MUST skip
`b:wolf_preview_source` buffers so `Snapshot.current_file`/`buffers`/statusbar never see the scratch
name as a real path. "Which window previews what source" is carried by the new `wolf_preview`
notification.

**Pane placement (single vs multi).** Front-end owns pane count
(`PaneContainerView.paneCount`/`WolfWindowController.paneCount`). Decision is front-end (pure
`PreviewSeam.placementMode(paneCount:)` → `"split_right"` when `<= 1` else `"active"`); mechanism is
core's `wolf_open_preview(mode)` Lua helper: resolve active source + abs path; no-op (return 0)
unless `&filetype == "markdown"`; create-or-reuse the scratch buffer; for `"split_right"` call the
existing `wolf_do_split('v')` (emits a `wolf_layout` Vertical Split intent; shell creates+binds the
new pane next wake) then `nvim_win_set_buf(0, previewbuf)`; for `"active"` just
`nvim_win_set_buf(0, previewbuf)`. Reuses the existing `:vsplit` intent path and the window-scoped
switch; the front-end learns the new pane + mapping on the next wake (no synchronous window handle
needed).

**Live-update trigger — notify-only, front-end pulls HTML on wake (OD6 live-on-change).** Two new
out-of-band notifications plus a blocking render pull:
- `wolf_preview` (like `wolf_buffers`): a `WinEnter`/`BufWinEnter`/`WinNew`/`WinClosed`/`BufWinLeave`
  autocmd scans windows for `b:wolf_preview_source` and emits
  `[[preview_win, source_bufnr, source_abs_path], …]`. Core folds into
  `Snapshot.previews: HashMap<i64, PreviewInfo{ source_bufnr, source_path }>`.
- `wolf_markdown_dirty`: `TextChanged`/`TextChangedI`/`BufWritePost` on markdown source buffers emits
  `[source_bufnr, changedtick]`. Core folds into `Snapshot.markdown_ticks: HashMap<i64, u64>`.
- Blocking pull: on wake, for each preview pane whose source tick advanced, the front-end calls
  `handle.render_preview(source_bufnr)`; core `block_on`s `nvim_buf_get_lines(...)`, joins `\n`,
  renders HTML (mirrors blocking `list_buffers`). Debounce with `PickerRefreshCoalescer`.

**Rendering + themes (OD5 fixed single theme).** `wolf-core` owns rendering. `PreviewTheme` enum
(one variant `Default`); `theme_css(theme) -> &'static str` via `include_str!` of
`crates/wolf-core/src/markdown/themes/default.css`; `BASE_CSS = include_str!("markdown/base.css")`
holds structural + marker rules; the renderer wraps
`<!doctype html><html><head><meta charset="utf-8"><style>{BASE_CSS}{theme_css}</style></head><body class="wolf-markdown">{body}</body></html>`.
Selection is a hardcoded `PreviewTheme::Default` at the call site this cut (a future
`wolf-config.yaml` key is the extension point — note, don't build).

## The marker/anchor scheme (build the HTML markers now; JS + cursor signal deferred)

Element: empty inline `<span class="wolf-src" id="wl-{L}-{C}" data-line="{L}" data-col="{C}"></span>`.
`L` = 1-based hard buffer line (matches nvim `line('.')`). `C` = 0-based CHARACTER column (OD4).
`id` format `wl-{L}-{C}` for O(1) `getElementById`. CSS `.wolf-src` = zero-size inline marker.

Marker set: for EVERY hard line `L` in `1..=line_count`, a marker `(L, 0)`; ADDITIONALLY for any
line whose character length `>= MARKER_COLUMN_STRIDE (100)`, markers `(L, 100)`, `(L, 200)`, … for
each multiple of 100 strictly less than the line's character length. `MARKER_COLUMN_STRIDE = 100`
is a named const (no magic number).

DOM placement (mapping survives block rendering): implement a marker-injecting adapter over
`Parser::into_offset_iter()`, feeding the augmented event stream to `html::push_html`. Precompute the
sorted desired-marker byte offsets. Walk `(event, range)`; IMMEDIATELY BEFORE yielding a block-level
`Start(tag)` whose `range.start >=` the next pending marker's offset, first yield an
`Event::Html(Owned(<span…>))` for every pending marker with offset `<= range.start`, in source
order. Block-level injection points: `Paragraph`, `Heading`, `BlockQuote`, `CodeBlock`, `List`,
`Item`, `Table`, `TableHead`, `TableRow`, `HtmlBlock`, `FootnoteDefinition`. Inline tags are NOT
injection points (markers never split inline content or land inside `<pre>`/`<code>`). Flush
remaining markers as trailing spans after the stream ends.

Consequence (documented): lines inside one block (wrapped paragraph, fenced code, table) collapse to
that block's start y — every line's `wl-{L}-0` marker still EXISTS (unique id) but resolves to the
block top. Correct scroll granularity for v1. The per-100-col markers are mostly redundant in v1 but
are REQUIRED by the spec and become meaningful when inline-level anchoring is added later.

Forward-compat contract (documented, NOT built): the follow-up adds (1) a Neovim→web-view
cursor-position `(line, col)` signal (new notification + FFI accessor) — converting nvim's
byte-column to a character-column — and (2) JS that on cursor move finds `#wl-{line}-{col}` (falling
back to `#wl-{line}-0`) and scrolls it into view only when it nears/crosses a viewport-edge
threshold. None of that is in scope now.

## Files to CREATE

- `crates/wolf-core/src/markdown.rs` (or `markdown/` module dir) — pure, GUI-free, IO-free (like
  `rank.rs`), with `#[cfg(test)] mod tests`. `pub enum PreviewTheme { Default }`;
  `pub fn theme_css(PreviewTheme) -> &'static str`; `const BASE_CSS`; `const MARKER_COLUMN_STRIDE:
  usize = 100`; `pub fn render_html(source: &str, theme: PreviewTheme) -> String`; internal
  `desired_markers(source) -> Vec<(usize byte, u32 line, u32 col)>`, a `MarkerInjector` iterator
  adapter, `marker_span(line, col) -> String`.
- `crates/wolf-core/src/markdown/base.css` + `crates/wolf-core/src/markdown/themes/default.css`
  (`include_str!`'d; NOT Markdown docs — no prettier).
- `crates/wolf-ffi/tests/preview.rs` — real-nvim integration (`:preview` Bazel target), modeled on
  `tests/buffer_picker.rs` / `tests/pane_status.rs`, reusing `tests/common/mod.rs`.
- `crates/wolf-macos/Sources/WolfKeyMap/PreviewSeam.swift` — `public enum PreviewSeam`, Foundation
  only. `static func placementMode(paneCount: Int) -> String`. Optional pure
  `previewsNeedingReload(current:lastTicks:) -> [Int64]`.
- `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewSeamTests.swift`.

## Files to MODIFY

### Core
- `crates/wolf-core/Cargo.toml` — add `pulldown-cmark` (pinned). Repin Bazel `crate_universe`
  (`CARGO_BAZEL_REPIN=1 bazel build …`), commit `Cargo.lock` + Bazel locks.
- `crates/wolf-core/src/lib.rs` — `pub mod markdown;` + re-export `render_html`, `PreviewTheme`,
  `theme_css`; update module-list docs.
- `crates/wolf-core/src/command.rs` — append `Command::Preview` (discriminant 18); append
  `COMMAND_SPECS` row `{ Preview, id "preview", ex_command "WolfPreview", title "Preview",
  MenuGroup::View }`; update doc variant list; bump `EXPECTED_COMMAND_COUNT` to 19; add
  `PREVIEW_INDEX = 18`; add round-trip/spec/last-row tests; move the "last row" assertion from
  `NewTab` to `Preview`.
- `crates/wolf-core/src/render.rs` — add `current_filetype: String` (+ `is_markdown_active()`),
  `previews: HashMap<i64, PreviewInfo>` (+ `pub struct PreviewInfo{ source_bufnr, source_path }`),
  `markdown_ticks: HashMap<i64, u64>`; init in `Snapshot::new`; NONE bump `seq`; update module docs.
- `crates/wolf-core/src/redraw.rs` — extend `apply_current_file` to read arg1 filetype;
  `PREVIEW_EVENT = "wolf_preview"` → `apply_previews` (build map before write guard, wholesale
  replace, no `seq`, wake after unlock — like `apply_buffers`); `MARKDOWN_DIRTY_EVENT =
  "wolf_markdown_dirty"` → `apply_markdown_dirty` (decode `[bufnr, tick]`, upsert, wake); route both.
- `crates/wolf-core/src/nvim.rs` — extend current-file tracking doc/Lua for `FileType` + filetype
  arg; `render_preview(source_bufnr) -> Result<String>` (get_lines → join → `markdown::render_html`);
  `open_preview(mode: &str) -> Result<i64>` (runs `_G.wolf_open_preview`); `setup_preview()` (installs
  `wolf_preview` + `wolf_markdown_dirty`, channel-id pattern; called from `start` after
  `setup_view_tracking`).
- `crates/wolf-core/src/api.rs` — `render_preview(source_bufnr) -> String` (blocking, empty on
  error); `open_preview(mode: &str) -> i64` (blocking, 0 on error/not-markdown);
  `is_markdown_active() -> bool`; add `proc.setup_preview().await?;` to `start`.
- `crates/wolf-core/README.md` + `src/README.md` — markdown module + `pulldown-cmark` (denylist
  re-verified), new `WolfCore` methods, snapshot fields/accessor, new `nvim.rs` RPCs/setup, new
  redraw notifications.

### FFI
- `crates/wolf-ffi/src/lib.rs` — `mod command_kind`: `pub const PREVIEW: u8 = 18;`;
  `FfiCommand::from_command` arm `Command::Preview => PREVIEW`; new opaque `FfiPreview`
  (`from_preview(win, &PreviewInfo, source_tick)`; accessors `win()`, `source_bufnr()`,
  `source_name()` basename, `source_path()`, `source_tick()`); `WolfCoreHandle` methods with the
  `borrow().as_ref()` core-gone fallback: `is_markdown_active() -> bool`,
  `render_preview(source_bufnr: i64) -> String`, `open_preview(mode: String) -> i64`,
  `previews() -> Vec<FfiPreview>` (joins each source bufnr's `markdown_ticks` tick); update bridge
  type list + re-exports.
- Regenerate bindings: `bazel run //crates/wolf-ffi:gen_bindings`; commit changed
  `generated/wolf-ffi.swift` / `wolf-ffi.h` (`wolf_ffi_wake.h` stays byte-identical — preview rides
  the editor render wake).
- `crates/wolf-ffi/tests/commands.rs` — type `:WolfPreview<CR>`, poll `drain_commands()` for
  `kind() == 18`; update the local contiguous `0..=18` asserts.
- `crates/wolf-ffi/BUILD.bazel` — add `:preview` `rust_test` (mirror `:buffer_picker`).
- `crates/wolf-ffi/README.md` + `src/README.md` — four new handle methods, `FfiPreview`,
  `command_kind` extended to 18, `:preview` target.

### Swift front-end
- `Sources/WolfKeyMap/CommandSeam.swift` — `case preview = 18`; extend `init?(ffi:)`; update
  `CommandSeamTests`.
- `Sources/WolfMacOS/AppMenu.swift` — `Tag.preview = 18`; add View > Preview `commandItem` (title
  "Preview", NO key equivalent per OD2); wire to `handleCommandMenuItem`.
- `Sources/WolfMacOS/AppDelegate.swift` — `validateMenuItem`: enable `.preview` only when key
  window's controller reports `handle.is_markdown_active()` (Copy/visual-mode precedent).
- `Sources/WolfMacOS/CommandDispatcher.swift` — `.preview → controller?.openPreview()`.
- `Sources/WolfMacOS/WolfWindowController.swift` — `func openPreview()`
  (`PreviewSeam.placementMode(paneCount:)` → `handle?.open_preview(mode)`); `isMarkdownActive`
  passthrough for `validateMenuItem`.
- `Sources/WolfMacOS/PaneContainerView.swift` — `import WebKit`; `previewWebViews: [Int64: WKWebView]`,
  `previewDimOverlays: [Int64: NSView]`, `lastPreviewTicks: [Int64: UInt64]`; in `onRenderWake`
  (after `paneRects`): read `handle.previews()`, ensure/position a `WKWebView` at the pane grid rect,
  `loadHTMLString(handle.render_preview(source_bufnr))` when the tick changed (debounced), remove web
  views for no-longer-preview/closed panes, apply/clear dim overlay per active state, suppress
  scrollbar + show `Preview: <name>` statusbar for preview panes.
- `crates/wolf-macos/BUILD.bazel` — link `WebKit` (sdk_frameworks/linkopts).
- `crates/wolf-macos/README.md` + `Sources/README.md` + `Tests/README.md` — `PreviewSeam`,
  `CommandKind.preview`, AppMenu item + gate, dispatcher arm, `openPreview`, `PaneContainerView`
  web-view hosting, new tests.

### Neovim runtime
- `runtime/nvim/init.lua` — extend `wolf_setup_current_file_tracking` (add `FileType`, send
  `vim.bo.filetype`, skip `b:wolf_preview_source` buffers); extend `wolf_setup_buffer_tracking`
  (skip preview-source windows); add `wolf_open_preview(mode)` (resolve source + abs path; no-op
  unless markdown; create-or-reuse scratch buffer w/ props + name + `b:wolf_preview_source` +
  source→buf upvalue map; `"split_right"` → `wolf_do_split('v')` then `nvim_win_set_buf(0, buf)`;
  `"active"` → `nvim_win_set_buf(0, buf)`; return bufnr); add `wolf_setup_preview(chan)`
  (`WolfPreview` augroup: window-scan `wolf_preview` emit + initial emit; markdown-guarded
  `TextChanged`/`TextChangedI`/`BufWritePost` → `wolf_markdown_dirty`; channel id as vararg).
- `runtime/nvim/README.md` — `wolf_setup_preview`, `wolf_open_preview`, the scratch-buffer scheme +
  `b:wolf_preview_source`, the `FileType` addition, the preview-buffer skip guards.

### Top-level docs
- `architecture.md` — Decision 10 + `command_kind` table: append Preview (18); "eighteen" →
  "nineteen"; a new "Output: preview" contract note (`wolf_preview` / `wolf_markdown_dirty` /
  filetype facets; `render_preview` pull; scratch-buffer representation); a note on the second pane
  content kind (web view) + core-side rendering; the macOS phase-status paragraph gains preview
  panes; "Deferred / open" gains the scroll-sync JS + cursor signal (designed, not built) and the
  multi-theme selector.

## Test plan

**Pure Rust unit (`markdown.rs` `#[cfg(test)]`):** `render_html("# Title\n\nHello")` contains `<h1`,
`<p>`, and the doctype/`<style>`/`body class="wolf-markdown"` wrapper; every hard line `1..=N` yields
`id="wl-{L}-0"`; a heading on line 3 places `wl-3-0` immediately BEFORE its `<h…>` (substring order);
a `>=100`-char line yields `wl-{L}-100` (and a `<100` line does not); NO `wolf-src` span inside a
fenced code block's `<pre><code>`; markers in source order; a multi-line paragraph collapses its line
markers before the single `<p>` (all ids present); `theme_css(Default)` non-empty; enum+fn shape
compiles. `render.rs` tests: `is_markdown_active()` true iff `current_filetype == "markdown"`.
Denylist re-verification is a documented manual command, not a test.

**Real-nvim FFI (`tests/preview.rs`, `:preview`):** seed temp cwd with `alpha.md` + `notes.txt`;
`:e alpha.md` → poll `is_markdown_active() == true`; `:e notes.txt` → `false`. Back on `alpha.md`,
`open_preview("split_right")` → poll `windows()` to 2 editor grids; `previews()` yields one entry
`source_name() == "alpha.md"` with alpha's bufnr; `render_preview(bufnr)` contains the heading +
`id="wl-1-0"`; type into source → poll `previews()[…].source_tick()` advancing;
`buffer_picker().ranked("", n)` includes a row whose `display()` begins `Preview: alpha.md`;
`open_preview("active")` with two panes replaces the active window's buffer; `shutdown` → neutral.

**Swift headless seams (`WolfKeyMapTests`):** `CommandSeamTests` `.preview == 18`, `CommandKind(ffi:
18) == .preview`, `CommandKind(ffi: 19) == nil`; `PreviewSeamTests` `placementMode(1)=="split_right"`,
`(2)=="active"`, `(0)=="split_right"`; optional `previewsNeedingReload` gating test.

**FFI command contract:** `tests/commands.rs` `:WolfPreview<CR>` → `kind()==18`, contiguous `0..=18`;
`command.rs` count 19, `PREVIEW_INDEX = 18` is last row.

**AppKit WKWebView wiring:** compile-verified + manual-smoke only (no window server), per repo
convention for `PaneContainerView`/AppKit glue. Manual smoke checklist in the PR: single-pane
Preview splits right + shows rendered HTML; multi-pane Preview replaces the active pane; editing the
source live-updates; buffer picker shows `Preview: <file>`; menu item disabled on a non-markdown
buffer.

## Implementation order (avoids compile breaks)
1. Core `markdown` module + `base.css`/`default.css` + unit tests (self-contained, no nvim).
2. `command.rs`: append `Command::Preview` + spec + count/index tests.
3. `render.rs`: add fields + `is_markdown_active` + `Snapshot::new` init.
4. `redraw.rs`: extend `apply_current_file`; add `apply_previews` / `apply_markdown_dirty` + routing.
5. `nvim.rs`: `render_preview` / `open_preview` / `setup_preview`; extend current-file/buffer drivers.
6. `api.rs`: three methods + wire `setup_preview` into `start`.
7. `runtime/nvim/init.lua`: `wolf_setup_preview` / `wolf_open_preview` / scratch helpers / filetype +
   skip guards.
8. FFI `lib.rs`: `command_kind::PREVIEW`, `FfiCommand` arm, `FfiPreview`, four handle methods.
   Regenerate + commit bindings. Repin Cargo/Bazel locks.
9. FFI tests: `tests/preview.rs`, extend `tests/commands.rs`, `:preview` BUILD target.
10. Swift seams: `CommandSeam` (18), `PreviewSeam` + tests.
11. Swift AppKit: `AppMenu` item + `validateMenuItem` gate, `CommandDispatcher` arm,
    `WolfWindowController.openPreview`, `PaneContainerView` web-view hosting, `BUILD.bazel` WebKit.
12. Docs: all READMEs + `architecture.md`, `prettier --write` touched Markdown.
