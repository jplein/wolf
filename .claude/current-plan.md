# Pane-mode preview/edit rework

## Status

Phase: complete (reviewed, documented, all tests green, lint clean)

### Approved deviation

`wolf_preview_file` marks the source buflisted only after the markdown gate passes (the plan's literal wording was "set buflisted, then delegate"). Rationale: a rejected non-markdown file must not gain a permanent picker row. Pinned by a test; record in the decisions doc.

## Problem statement

Wolf's Preview command currently opens a preview in a new pane if there is only one pane, and uses a heuristic to find an existing preview pane when there is more than one. The Edit ("Edit Source") command uses a three-mode placement decision (`split_left` / `reuse_editor_pane` / `active`, see `preview-edit-command-decisions.md`). Replace both with a per-pane mode model: each pane is in **preview** mode or **edit** mode.

### Required behavior

1. **Rename** the "Edit Source" command's display title to "Edit" (registry `id` stays `edit`).
2. **Preview command**: if the active pane is editing a file, preview that file **in the active pane** (in place — no new pane, no pane-hunting heuristic).
3. **Edit command**: if the active pane is previewing a file, edit that file **in the active pane** (in place).
4. **Pane mode drives navigation**:
   - Buffer picker: active pane in edit mode → chosen buffer is edited; preview mode → chosen buffer is previewed.
   - File picker / open dialog: same rule (edit mode edits, preview mode previews the chosen file).
5. **Buffer picker listing**: one entry per file — the union of files previewed and edited, with no preview/edit distinction ("Preview: " rows disappear).

### Project rules

- Docs are source of truth and must be updated; prefer real-Neovim integration tests over mocks.
- Every new test file must be wired into its Bazel target explicitly (`rust_test` in the crate's BUILD.bazel with nvim runfiles + `tags = ["no-sandbox"]` for real-nvim tests; Swift test files into the hand-maintained `//crates/wolf-macos:WolfKeyMapTests` `srcs` list).

## Architect plan (approved)

### Design resolutions

- **Q1 Pane mode is derived, no new state.** A pane bound to nvim window `win` is in preview mode iff `win` appears in `Snapshot.previews` (front-end: `handle.previews()`; Lua: the window's buffer carries `b:wolf_preview_source`). Everything else is edit mode. `Snapshot.current_is_preview` / `is_preview_active()` untouched (still the Edit menu gate).
- **Q2 In-place semantics.** `_G.wolf_open_preview()` and `_G.wolf_edit_source()` lose their `mode` parameter and keep only the in-place ("active") arm: `nvim_win_set_buf(0, preview_buf)` / `edit_into(0)`, with existing gates, cursor pre-emit, and re-emits. Delete `split_left` / `reuse_editor_pane` / `split_right` / `reuse_preview_pane` branches and other-window scans.
- **Q3 Seams.** `EditSeam.swift` deleted entirely. `PreviewSeam.placementMode` deleted (keep `PreviewLoad`, `winsNeedingReload`, click-focus members). `PaneContainerView`'s `PreviewPlacementInputs` / `EditPlacementInputs` + accessors deleted; replaced by `var activePaneIsPreview: Bool` via new pure seam `PaneModeSeam`.
- **Q4 Picker union.** `wolf_ensure_preview_buf` creates the scratch **unlisted** (`nvim_create_buf(false, true)`, keeps `Preview: <rel>` name); `wolf_preview_file` sets `vim.bo[source_buf].buflisted = true` after `bufload`. Listed set = deduplicated union; no core `rank_buffers` change. New Lua helper `_G.wolf_preview_buffer(win, bufnr)`: validates bufnr, markdown gate (`filetype == "markdown"` or `vim.filetype.match`), ensures scratch, binds into `win` (0 → current), re-emits, returns source bufnr or 0. `wolf_preview_file` becomes the disk-path front half delegating to it.
- **Q5 Routing.** `PickerView.activateSelection()` and `WolfWindowController.presentOpenFileDialog()`: if active pane is preview mode → `handle.preview_file(activeWin, path)`, falling back to `handle.open(path)` on `0` (non-markdown → pane converts to edit mode). Buffer picker (`activateBufferRow`): preview mode → `handle.preview_buffer(activeWin, bufnr)`, fallback `switch_buffer(bufnr)` on `0`. Sidebar file tree and search results deliberately out of scope (record in decisions doc).
- **Q6 FFI/Swift surface.** `wolf-ffi`: `open_preview()` / `edit_source()` lose `mode`; add `preview_buffer(win, bufnr) -> i64`. No discriminant changes (EDIT stays 19). Regenerate `generated/wolf-ffi.swift`. Swift: menu title "Edit Source" → "Edit"; `validateMenuItem` gates kept (Preview on `isMarkdownActive`, Edit on `isPreviewActive`). `wolf_current_file` / Snapshot: no changes.
- **Q7 Edge cases.** Non-markdown Preview: gated/no-op (unchanged). Edit off-preview: gated/returns 0 (unchanged). Non-markdown or unnamed pick in preview mode: helper returns 0 → edit fallback, pane becomes edit pane. Two panes previewing same file: same scratch bound in both, one picker row. Source stays buflisted after its preview pane closes. Preview binds record no MRU (deliberate); edit-mode `open` keeps recording.

### Files to modify

1. `runtime/nvim/init.lua` — scratch unlisted; `wolf_open_preview()` / `wolf_edit_source()` drop mode, in-place only; new `_G.wolf_preview_buffer(win, bufnr)`; `wolf_preview_file` delegates + `buflisted = true`.
2. `crates/wolf-core/src/command.rs` — Edit row `title: "Edit"`; update `edit_spec_fields` test.
3. `crates/wolf-core/src/nvim.rs` — `open_preview()` / `edit_source()` no-arg; new `preview_buffer(&self, win: i64, bufnr: i64)` mirroring `preview_file` (impl Future + Send + 'static shape, varargs never interpolated).
4. `crates/wolf-core/src/api.rs` — no-arg `open_preview` / `edit_source`; new `#[must_use] pub fn preview_buffer(&self, win: i64, bufnr: i64) -> i64` (0 on error/timeout, `block_on_rpc`).
5. `crates/wolf-ffi/src/lib.rs` — bridge updates + `preview_buffer`; regenerate `crates/wolf-ffi/generated/wolf-ffi.swift`.
6. `crates/wolf-macos`: delete `EditSeam.swift`; trim `PreviewSeam.swift`; new `Sources/WolfKeyMap/PaneModeSeam.swift` (`enum PaneMode { case edit, preview }`, `static func mode(activeWin: Int64, previewWins: Set<Int64>) -> PaneMode`, `activeWin == 0` → `.edit`); `PaneContainerView` (delete placement inputs, add `activePaneIsPreview`); `WolfWindowController` (`openPreview`/`editSource` no-mode, `activateBufferRow`, `presentOpenFileDialog`); `PickerView.activateSelection`; `AppMenu` title; `BUILD.bazel` WolfKeyMapTests srcs (remove EditSeamTests.swift, add PaneModeSeamTests.swift).

### Test plan

Swift (`//crates/wolf-macos:WolfKeyMapTests`, hand-maintained srcs):
- Delete `EditSeamTests.swift` (and BUILD entry).
- `PreviewSeamTests.swift`: delete `placementMode` tests, keep `winsNeedingReload` / click-focus.
- New `PaneModeSeamTests.swift`: active win in previewWins → .preview; absent → .edit; activeWin == 0 → .edit; empty set → .edit; other pane previewing → .edit.

Rust real-nvim (`crates/wolf-ffi/tests`, existing Bazel stanzas keep their srcs):
- `preview.rs` rewrite: in-place open_preview (window count 1, previews maps active win); scratch reuse across edit/preview cycles; no "Preview: " picker rows, one row per source (incl. `docs/alpha.md` relative case); two panes previewing same source (both in previews(), one picker row); `preview_buffer` (markdown → binds+returns bufnr; non-markdown → 0; unsaved Untitled.md works); `preview_file` leaves source buflisted; post-shutdown neutral 0 with no-arg signatures.
- `edit.rs` rewrite: in-place edit_source (same window, previews drops win, is_preview_active falls, is_markdown_active rises); off-preview returns 0; delete split_left/reuse_editor_pane scenarios.
- `buffer_picker.rs`: union case (edit A + preview A → one row); preview-only file appears as own row.
- `commands.rs` + `crates/wolf-core/tests/palette_rank.rs`: "Edit Source" → "Edit" title assertions.
- `crates/wolf-core/tests/{markdown_cursor.rs,pane_buffers.rs,nav_history_live.rs}`: rework `open_preview("split_right")` call sites to split first (typed `:vsplit`) then in-place `open_preview()`; `nav_history_live.rs` edit_source → no-arg.
- `crates/wolf-ffi/tests/{full_width.rs,code_block_highlight.rs}`: update call sites.

Swift routing glue (activateBufferRow / activateSelection / presentOpenFileDialog) is manual-smoke per existing picker precedent; decision logic covered by PaneModeSeamTests + Rust tests.

### Documentation updates

- `architecture.md` preview section: in-place commands, derived pane mode, `preview_file`/`preview_buffer`.
- `preview-edit-command-decisions.md`: rewrite in present tense for the pane-mode model (delete three-mode Decisions 3–5 content).
- `runtime/nvim/README.md`: helper entries updated, `wolf_preview_buffer` added, scratch-unlisted + source-buflisted decision.
- `crates/wolf-ffi/src/README.md`, `crates/wolf-macos/Sources/WolfKeyMap/README.md` (delete EditSeam entry, add PaneModeSeam), test READMEs (`crates/wolf-macos/Tests/WolfKeyMapTests/README.md`, `crates/wolf-ffi/tests/README.md`), `feat-list.md`, `crates/wolf-core/src/README.md` (only if mode strings named).
- `prettier --write` on all edited Markdown; `./tools/lint/markdown_check.sh`. No changelog words ("now", "no longer", "previously").

### Implementation order

1. `runtime/nvim/init.lua`
2. `wolf-core` (command.rs, nvim.rs, api.rs)
3. `wolf-core/tests` updates
4. `wolf-ffi` lib + regenerate swift + `wolf-ffi/tests` updates
5. `wolf-macos` sources + tests + BUILD.bazel
6. Docs, then `bazelisk test` across wolf-ffi, wolf-core, and WolfKeyMapTests targets.

## Progress log

- Orchestration started; problem statement written.
- Architect plan received and approved (agent afa55c755aed496f8).
- Test-writer pass 1: Swift PaneModeSeamTests added, EditSeamTests removed, Rust suites rewritten for in-place model, BUILD srcs updated.
- Coder pass 1: full implementation landed (init.lua, wolf-core, wolf-ffi + regenerated bindings, wolf-macos). All suites green: 36/36 wolf-ffi, 46/46 wolf-core, WolfKeyMapTests 390 tests, app builds, lint clean.
- Test-writer pass 2: two pin tests added in preview.rs (success-gated buflisted; bufload of :badd-ed buffer).
- Coder pass 2: `//crates/wolf-ffi:preview` 9/9 pass, no source changes needed; 36/36 overall.
- Reviewer: approved with minor notes (stale BUILD comments, test-header comment, doc divergences).
- Coder pass 3: BUILD comments rewritten, PaneModeSeamTests header trimmed, EditSeam deletions confirmed staged; targets re-run green.
- Technical-writer: nine Markdown files updated (architecture.md, runtime/nvim/README.md, ffi/macos READMEs, test READMEs, feat-list.md, doc-refactor-misc-recommendations.md, preview-edit-command-decisions.md rewritten for the pane-mode model).
- Coder pass 4: prettier — all nine unchanged; markdown_check — no findings. Orchestration complete.
