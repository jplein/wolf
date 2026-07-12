# Plan: Edit command (inverse of Preview) — task 3

## Problem statement

Implement task 3 in `markdown-tasks.md`:

> Edit command - like the inverse of preview. Only has an effect in a Preview pane. If the active pane is a preview pane and the other pane is an editor pane, edit the file in the other pane; otherwise, open it in the same pane as the preview. If the preview pane is the only pane, open the editor in a new pane on the left side.

## Status

- [x] Architect plan
- [x] Tests written (command.rs, render.rs, wolf-ffi/tests/edit.rs + BUILD target, commands.rs, EditSeamTests.swift + BUILD)
- [x] Implementation + tests pass (wolf-core lib, wolf-ffi:edit 6/6, :commands, :preview, :bridge_smoke, WolfKeyMapTests 155/0, macOS lib builds; FFI bindings regenerated)
- [x] Review — APPROVED, no blockers
- [x] Docs — decisions doc created; architecture.md + all crate READMEs + markdown-tasks.md (task 3 DONE) updated; prettier verified clean

## COMPLETE

---

# Architect plan

## 1. Summary

Add a new user command, **Edit** (`id = "edit"`, `:WolfEdit`, `require('wolf').edit()`), the inverse of **Preview**. Effective only when the active pane is a **preview pane** (buffer carrying `b:wolf_preview_source`). Navigates to that preview's SOURCE file using one of three placement modes chosen by a pure front-end seam, mirroring Preview:

- **`split_left`** — preview pane is the only pane: open the source editor in a NEW pane on the LEFT (preview stays right).
- **`reuse_editor_pane`** — exactly two panes, active is preview, OTHER pane is editor (non-preview): edit source in that other pane and focus it.
- **`active`** — every other case: replace the preview in the active pane with the source editor.

Edit is appended as discriminant **19** everywhere (append-only; Preview stays 18).

## 2. Design decisions (also → decisions doc)

**D1 — Menu placement + naming.** View menu (with Preview), NOT the Copy/Paste Edit NSMenu. DISPLAY label **"Edit Source"** for menu item + palette row. Registry `CommandSpec.title = "Edit Source"`, `id = "edit"`, `ex_command = "WolfEdit"`, `menu = MenuGroup::View`. No key equivalent. Enabled only when active pane is a preview pane (gated in `validateMenuItem` via `isPreviewActive`). Palette row NOT gated (mirrors Preview); Lua no-ops (returns 0) off a preview pane.

**D2 — `is_preview_active` semantics.** Mirror `is_markdown_active` (reads `Snapshot.current_filetype`). Extend `wolf_current_file` notification with THIRD boolean arg `is_preview`; add `Snapshot.current_is_preview: bool` fed by it; `Snapshot::is_preview_active()` / `WolfCore::is_preview_active()` read it. Cheap synchronous read (no blocking RPC on menu validation). Fallback (only if extending proves troublesome): blocking `is_preview_active()` RPC reading `vim.b[…].wolf_preview_source ~= nil`. PREFER the snapshot-field mirror.

**D3 — `wolf_edit_source(mode)` Lua semantics.** Gate: read `b:wolf_preview_source` / `b:wolf_preview_source_bufnr` off the CURRENT buffer; return `0` if not a preview. Binding: prefer `nvim_win_set_buf(win, source_bufnr)` when bufnr valid (window-scoped, no disk re-read, no MRU churn — like `wolf_switch_buffer`); else focus the target window + `:edit fnameescape(source_path)`. No MRU recording. After bind, fire `wolf_emit_previews()` + `wolf_emit_current_file()` (idempotent). Return `source_bufnr`.

- `split_left`: `orig = nvim_get_current_win()`, `wolf_do_split("v")`, `edit_into(orig)`, `nvim_set_current_win(orig)`. (PaneTree inserts new pane AFTER target → new window is RIGHT, orig is LEFT → editor left, preview right.)
- `reuse_editor_pane`: scan `nvim_tabpage_list_wins(0)` for first non-current window whose buffer has `b:wolf_preview_source == nil`; `target = that or cur`; `edit_into(target)`, `nvim_set_current_win(target)`.
- `active`: `edit_into(0)` (current), replacing the preview.

**D4 — Front-end wake.** `editSource()` calls `paneContainer.onRenderWake()` after, mirroring `openPreview()`.

**D5 — No new layout intent for split_left.** Reuses `wolf_do_split("v")` → existing `wolf_layout` SPLIT intent → existing `applySplit`. LEFT placement owned by binding source into the ORIGINAL (`from`) window. No Swift intent-kind change.

## 3. Files to CREATE

- `crates/wolf-macos/Sources/WolfKeyMap/EditSeam.swift` — pure Foundation seam, `EditSeam.placementMode(paneCount:activePaneIsPreview:otherEditorPaneCount:) -> String` (`split_left` when paneCount<=1; `reuse_editor_pane` when paneCount==2 && activePaneIsPreview && otherEditorPaneCount==1; else `active`). Default `activePaneIsPreview = true`.
- `preview-edit-command-decisions.md` (repo root) — capture D1–D5, append-only=19 rule, modes+Lua bindings, is_preview_active mechanism (+RPC fallback noted), no-MRU/bind-by-bufnr, "Edit Source" naming. prettier --write.
- Test files (test-writer): `crates/wolf-ffi/tests/edit.rs`, `crates/wolf-macos/Tests/WolfKeyMapTests/EditSeamTests.swift`.

## 4. Files to MODIFY

### wolf-core

- **`command.rs`**: append `Edit` variant after `Preview`; append `COMMAND_SPECS` row `{ Edit, "edit", "WolfEdit", "Edit Source", MenuGroup::View }`; update enum doc discriminant list (add `Edit(19)`). Tests: `EXPECTED_COMMAND_COUNT` 19→20; add `EDIT_INDEX=19`; rewrite `preview_is_the_last_row` → `preview_is_at_index_18` (drop "final row" clause); add `edit_id_round_trips`, `edit_spec_fields`, `edit_is_the_last_row`; update count-test message + PREVIEW_INDEX doc.
- **`render.rs`**: add `pub current_is_preview: bool` to `Snapshot` (derives Default→false); add `pub fn is_preview_active(&self) -> bool { self.current_is_preview }` near `is_markdown_active` (~619); add unit test (false default, true when set).
- **`redraw.rs`** `apply_current_file` (~328): read arg 2 `args.get(2).and_then(Value::as_bool).unwrap_or(false)` → `snap.current_is_preview`; update doc.
- **`nvim.rs`**: add `pub async fn edit_source(&self, mode: &str) -> Result<i64, NvimError>` mirroring `open_preview` (~336): `nvim_exec_lua "return wolf_edit_source(...)"`, `Value::Array([mode])`, decode `as_i64().unwrap_or_default()`.
- **`api.rs`**: add `pub fn edit_source(&self, mode: &str) -> i64` (mirror `open_preview` ~423, 0 on error); add `pub fn is_preview_active(&self) -> bool { self.read_snapshot().is_preview_active() }` (mirror `is_markdown_active` ~462).

### runtime

- **`init.lua`**: in `wolf_setup_current_file_tracking` emit (~191) send 3rd bool arg (preview branch `("", "", true)`, normal `(path, filetype, false)`); update comment. Add `function _G.wolf_edit_source(mode)` near `wolf_open_preview` (~674) per D3, with an `edit_into(win)` local helper (prefer bufnr, else set-current-win+`:edit`). Doc block mirroring `wolf_open_preview`.

### wolf-ffi

- **`src/lib.rs`**: `mod command_kind` (~179) add `pub const EDIT: u8 = 19;`; bridge decls add `fn is_preview_active(&self) -> bool;` + `fn edit_source(&self, mode: String) -> i64;` (~473); `impl WolfCoreHandle` add both methods (~1787); `FfiCommand::from_command` match (~2080) add `Command::Edit => command_kind::EDIT,`; update module doc discriminant contract.
- **Regenerate committed bindings**: `bazel run //crates/wolf-ffi:gen_bindings` (writes `generated/wolf-ffi.swift`/`.h` in-tree); commit them. Do NOT hand-edit `generated/`.

### wolf-macos (Swift)

- **`CommandSeam.swift`**: `enum CommandKind` add `case edit` after `.preview`; `init?(ffi:)` add `case 19: self = .edit`; update docs.
- **`PaneContainerView.swift`**: add `struct EditPlacementInputs { paneCount; activePaneIsPreview; otherEditorPaneCount }` (~40); add `func editPlacementInputs()` (~375) like `previewPlacementInputs` but `otherEditorPaneCount = panes.values.filter { !previewWins.contains($0.win) && $0.win != activeWin }.count`.
- **`WolfWindowController.swift`**: add `var isPreviewActive: Bool { handle?.is_preview_active() ?? false }` (~667); add `func editSource()` (~679): `editPlacementInputs()` → `EditSeam.placementMode(...)` → `handle.edit_source(mode)` → `paneContainer.onRenderWake()`.
- **`CommandDispatcher.swift`**: add `case .edit: controller?.editSource()` after `case .preview` (exhaustive switch).
- **`AppMenu.swift`**: `Tag` enum add `static let edit = 19`; add View-menu item after Preview `commandItem(title: "Edit Source", tag: Tag.edit, keyEquivalent: "", target: target)`.
- **`AppDelegate.swift`** `validateMenuItem` (~491): add `case .edit: return keyController?.isPreviewActive ?? false`; update doc.

## 5. Test plan

- **wolf-core unit** (`command.rs`/`render.rs`): registry count=20; Edit id round-trips; Edit spec fields; Edit last row; Preview at 18 (not last); `is_preview_active` false default / true when set.
- **wolf-ffi real-nvim `tests/edit.rs`** (model `tests/preview.rs`, reuse `tests/common/mod.rs`, loud SKIP w/o nvim): is*preview_active gate (false on md buffer; true after open_preview+focus preview); `split_left` from single preview pane → new pane, source in a non-preview window, one preview remains, returned bufnr==source; `reuse_editor_pane` → source bound into OTHER editor window + focused, no new pane; `active` → active preview window now shows source; reuse fallback (no other editor → binds active, no new pane); negative (not a preview → returns 0, nothing changes); shutdown neutrality; prefer basename matching + `wait_for*\*`polling helpers. Add`//crates/wolf-ffi:edit`rust_test target to BUILD.bazel (copy`:preview` stanza).
- **wolf-ffi `tests/commands.rs`**: add `const CMD_KIND_EDIT: u8 = 19;`, extend contiguity guard to `0..=19`, update header comment, add `:WolfEdit` drain assertion after `:WolfPreview`.
- **Swift `EditSeamTests.swift`** (model `PreviewSeamTests` placement part): single/zero pane→`split_left`; 2 panes active-preview+1 editor→`reuse_editor_pane`; 2 panes active-preview+no editor→`active`; 2 panes active-not-preview→`active`; 3 panes→`active`.

## 6. Implementation order (avoid compile breaks)

1. `command.rs` variant+row+tests. 2. `render.rs`. 3. `redraw.rs`. 4. `nvim.rs`. 5. `api.rs`. 6. `wolf-ffi/src/lib.rs` (all arms — exhaustive matches break until done) → `bazel run //crates/wolf-ffi:gen_bindings` → commit generated/. 7. `init.lua`. 8. Swift `CommandSeam` `.edit` + new `EditSeam.swift`. 9. Swift `WolfMacOS` glue (PaneContainerView, WolfWindowController, CommandDispatcher exhaustive, AppMenu, AppDelegate). 10. Tests. 11. Docs.

## 7. Dependencies

None.

## 8. Documentation updates

- **`architecture.md`**: `command_kind` list (~43) add `EDIT=19`; mention `is_preview_active()`/`edit_source(mode)`; macOS menu list (~65) add "Edit Source (View menu, no key equiv, preview-pane only)"; palette prose (~71) "18 OTHER"→"19 OTHER".
- **`crates/wolf-core/src/README.md`**: add `current_is_preview` to Snapshot fields; note `is_preview_active()`; Command registry incl Edit(19).
- **`crates/wolf-ffi/README.md`** + **`src/README.md`**: add `edit_source`/`is_preview_active`/`command_kind::EDIT=19`.
- **`crates/wolf-macos/README.md`**: mention `EditSeam` + View > Edit Source item.
- **`markdown-tasks.md`**: mark task 3 DONE + link `preview-edit-command-decisions.md`. prettier --write all touched Markdown.

## 9. Build / regen / test commands

- `bazel run //crates/wolf-ffi:gen_bindings` (then commit generated/).
- `cargo test -p wolf-core` (or Bazel rust_test).
- `bazel test //crates/wolf-ffi:edit //crates/wolf-ffi:commands //crates/wolf-ffi:preview //crates/wolf-ffi:bridge_smoke`.
- Swift seam: `bazel test //crates/wolf-macos:WolfKeyMapTests` (confirm target name; add EditSeamTests.swift to srcs).
- Build the macOS app/library to compile-verify AppKit glue.

## 10. Lint

- Use named consts (`EDIT_INDEX`, `EXPECTED_COMMAND_COUNT`) — no inline 19/20. No new `#[allow]`. `#[must_use]` on the handle methods where their Preview mirrors are.

## 11. Coder MUST-VERIFY

- init.lua split-left: PaneTree inserts new leaf AFTER target → binding source into `orig` (`from`) window = editor LEFT, preview RIGHT. Don't rely on `:leftabove`/`splitright` (front-end owns geometry; `splitright` unset). Manual-smoke the reconcile.
- FFI codegen target `//crates/wolf-ffi:gen_bindings` (bazel run, in-tree write). Verify regenerated Swift/.h has `edit_source`/`is_preview_active` and `command_kind` byte-stable except additive.
- `is_markdown_active` reads `Snapshot.current_filetype` via `wolf_current_file`; mirror adds `current_is_preview` via same notification's 3rd arg. Verify `rmpv::Value::as_bool()` decoder; missing arg → false.
- Exhaustive matches: `FfiCommand::from_command` (Rust) + `CommandDispatcher.dispatch` (Swift) break until arms added; `CommandKind.init?(ffi:)` non-exhaustive (`default: nil`).
- No hardcoded palette count test in `palette.rs` (candidates derived dynamically) — only architecture.md prose changes.
