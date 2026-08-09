# "Preview in Another Pane" / "Edit in Another Pane" commands

## Status

Phase: complete (reviewed, documented, all tests green, lint clean; Linux/Cargo path not exercisable from this host — run once on Linux before/after landing)

## Problem statement

The per-pane-mode rework landed: `open_preview()` / `edit_source()` are no-arg in-place; pane mode derives from the `previews` snapshot; `preview_buffer(win, bufnr)` / `preview_file(win, path)` bind previews into arbitrary windows. Add a complementary command pair targeting **another** pane.

### Required behavior

1. **Preview in Another Pane** — previews the active pane's file in another pane.
2. **Edit in Another Pane** — edits the active pane's previewed file in another pane.
3. **One pane**: split — Preview adds a new pane to the **right** (preview there); Edit adds a new pane to the **left** (edit there).
4. **More than one pane**: use an existing other pane. Prefer the pane to the **right** for preview, the pane to the **left** for edit; otherwise any other existing pane.

### Correction to the original statement

Discriminants 20/21 were stale: the registry ends at `SearchInProject` = 25. The new commands take **26 (`PreviewInOtherPane`) and 27 (`EditInOtherPane`)** (append-only across `wolf_core::Command`, `wolf-ffi` `command_kind`, Swift `CommandKind`).

## Architect plan (approved)

### Design resolutions

- **D1 Target choice is front-end, pure seam.** New `OtherPaneSeam` (WolfKeyMap, Foundation only): `enum Side { left, right }`, `enum Target { split, existing(PaneId) }`, `static func target(active: PaneId, prefer: Side, tree: PaneTree, width: Double, height: Double) -> Target?`. Rules in order: (1) active not a leaf (unknown/drawer) → nil; (2) single leaf → `.split`; (3) `tree.neighbor(active, dir, …)` resolves → `.existing`; (4) deterministic fallback: DFS-adjacent leaf with wrap (`.right` → next in `leaves()` order wrapping to first; `.left` → previous wrapping to last). Purely geometric — no buffer-content preference (the common round-trip works out naturally; a fallback target that is itself a preview gets repointed, ordinary bind semantics).
- **D2 Core/Lua: atomic split+bind helpers** (front-end split rejected — no FFI split call, async round trip has no window handle). Verified mechanics: `wolf_do_split("v")` makes the new window current and notifies Split `{from, win}`; `applySplit` inserts the new pane **after** `from` (always lands right). So: preview-split binds the scratch into the **new** (right) window, source stays in original (left); edit-split keeps the scratch in the **new** (right) window and binds the **source** into the **original** (left) window. **Focus rule: focus follows the edit-mode view of the document** — Preview keeps focus in the source editor; Edit moves focus to the pane now editing the source.
  - `_G.wolf_preview_in_other(target_win)` → source bufnr or 0. Gate first (markdown, before any split — palette misfires must not split); cursor pre-emit; ensure scratch; validate target (non-nil/non-zero, valid, not current window, not drawer via `b:wolf_terminal`/`w:wolf_terminal`) else split path; reuse: `nvim_win_set_buf(target_win, preview_buf)` (focus untouched); split: `from = current`, `wolf_do_split("v")`, bind scratch into new window, `nvim_set_current_win(from)`; guarded `wolf_emit_previews()` + `wolf_emit_current_file()`; no MRU.
  - `_G.wolf_edit_in_other(target_win)` → bufnr bound into target or 0. Gate: `b:wolf_preview_source`; same target validation; `bind_source_into(win)` mirrors `edit_into` (prefer `nvim_win_set_buf`, fallback focus+`:edit`); reuse: bind then focus target; split: `wolf_do_split("v")` (new right window keeps scratch), bind source into `from`, focus `from`; same re-emits. Returns `nvim_win_get_buf` of the bound window (so an `:edit` fallback reports non-zero — deliberate divergence from `wolf_edit_source`).
- **D3 Registry/FFI/Swift.** Specs: `{ id: "preview_in_other_pane", ex: "WolfPreviewInOtherPane", title: "Preview in Another Pane", MenuGroup::View }` (26) and `{ id: "edit_in_other_pane", ex: "WolfEditInOtherPane", title: "Edit in Another Pane", View }` (27). No key equivalents. Menu gates: PreviewInOtherPane iff `isMarkdownActive`; EditInOtherPane iff `isPreviewActive` (never both true).
- **D4** Blocking-call flavor; first command whose dispatch carries a front-end-derived argument (target win or 0) into core — one sentence for `command-lifecycle.md` §6.
- **D5** Palette: both listed ungated; Lua gate-first-return-0 means a mis-timed palette activation is a no-op with no stray split.

### Files to create

1. `crates/wolf-macos/Sources/WolfKeyMap/OtherPaneSeam.swift`
2. `crates/wolf-macos/Tests/WolfKeyMapTests/OtherPaneSeamTests.swift`
3. `crates/wolf-ffi/tests/other_pane.rs` (modeled on preview.rs/edit.rs, common/mod.rs, loud SKIP)

### Files to modify

- `crates/wolf-core/src/command.rs` — two variants + spec rows; tests: `EXPECTED_COMMAND_COUNT` 26 → 28, new index constants 26/27, from_id round-trips, spec-field tests, fixed-index guards; `search_in_project_is_the_last_row` becomes a fixed-index guard, `edit_in_other_pane` is the new last-row guard.
- `crates/wolf-core/src/nvim.rs` — `preview_in_other(target_win)` / `edit_in_other(target_win)` exec-lua futures (mirror `preview_buffer`).
- `crates/wolf-core/src/api.rs` — two `#[must_use]` blocking wrappers, 0 on error.
- `crates/wolf-core/tests/palette_rank.rs` — `STATIC_COMMAND_COUNT` 25 → 27.
- `crates/wolf-core/tests/commands.rs` — check for count/enumeration assertions.
- `runtime/nvim/init.lua` — the two helpers per D2, after `wolf_preview_file`.
- `crates/wolf-ffi/src/lib.rs` — `command_kind` 26/27, `FfiCommand::from_command` arms, bridge methods `preview_in_other` / `edit_in_other` (neutral 0 when shut down); regenerate `generated/` via `bazel run //crates/wolf-ffi:gen_bindings`.
- `crates/wolf-ffi/tests/commands.rs` — contiguous guard array to 27; drain steps for the two new ex-commands.
- `crates/wolf-ffi/BUILD.bazel` — new `rust_test(name = "other_pane", …)` mirroring `preview` verbatim (no-sandbox, nvim_tree + init.lua data, WOLF_TEST_* env).
- Swift: `CommandSeam.swift` cases + init arms 26/27 (update doc table); `PaneContainerView.swift` — `otherPaneTarget(prefer:)` using the same width/height pair `applyFocus` uses (drawer-reduced height), maps PaneId → win, nil when unbound; `WolfWindowController.swift` — `previewInOtherPane()` / `editInOtherPane()` (resolve target, call handle, `onRenderWake()`, no-op on nil); `CommandDispatcher.swift` two arms; `AppMenu.swift` tags 26/27 + two View items after Edit (empty key equivalents); `AppDelegate.swift` validateMenuItem arms; `crates/wolf-macos/BUILD.bazel` — OtherPaneSeamTests.swift into WolfKeyMapTests srcs (mandatory); `CommandSeamTests.swift` — decode 26/27, move out-of-range probe 26 → 28.

### Test plan

Swift `OtherPaneSeamTests.swift` (headless, build PaneTrees):
1. Single leaf → `.split` both sides.
2. Two side-by-side, active left, prefer right → existing(right).
3. Two side-by-side, active left, prefer left → fallback wraps → existing(right pane).
4. Three in a row [A][B][C], active B: right → C; left → A.
5. Active C prefer right → wraps → A; active A prefer left → wraps → C.
6. Vertical stack, prefer right → no lateral neighbor → fallback → other pane.
7. 2×2 grid: right from top-left → top-right (cross-axis-overlap rule, not diagonal).
8. Active not a leaf (fresh PaneId — drawer case) → nil.

Rust `other_pane.rs` (real nvim, siblings' poll idiom):
1. Split-preview: `preview_in_other(0)` → source bufnr; 2 windows; Split intent vertical, from = original; `previews()` single entry win == intent's new win; `active_win()` == from; markdown_active true / preview_active false.
2. Split-edit: in-place `open_preview()` then `edit_in_other(0)` → non-zero; 2 windows; previews entry win == new win (scratch right); focus from; `current_file()` == md path; preview_active false.
3. Reuse-preview: typed `:vsplit`, focus editor, `preview_in_other(other_win)` → previews gains entry win == other_win; still 2 windows; no new Split intent; focus unchanged.
4. Reuse-edit: preview in one window + second window; `edit_in_other(other_win)` → non-zero; `active_win() == other_win`; `current_file()` == source; original preview window still in previews(); window count unchanged.
5. Gates: non-markdown → `preview_in_other(0) == 0`, no split, no intent (pins gate-before-split); off-preview → `edit_in_other(0) == 0`.
6. Invalid target (the active win itself) → falls back to split.
7. Post-shutdown neutrals: both 0.

Existing tests touched: `command.rs` inline (counts/guards), `palette_rank.rs`, `wolf-ffi/tests/commands.rs`, `CommandSeamTests.swift` (probe 26 → 28).

### Documentation updates (technical-writer, after implementation)

`crates/wolf-ffi/src/README.md` (command_kind rows 26/27, preview-section entries); `runtime/nvim/README.md` (two helper entries: gate-before-split, bind-placement contract with applySplit, target validation, focus rule, no MRU); `crates/wolf-core/src/README.md` (in place, if surface listed); `crates/wolf-macos/Sources/WolfKeyMap/README.md` (OtherPaneSeam entry); `crates/wolf-macos/Sources/WolfMacOS/README.md` (AppMenu/WolfWindowController/PaneContainerView entries in place); test READMEs; `architecture.md` (1–2 sentences: target chosen front-end from PaneTree geometry, split+bind atomic in Lua via ordinary Split intent); `command-lifecycle.md` §6 (front-end-computed argument); `preview-edit-command-decisions.md` (new decision section: 26/27, geometric rules + DFS fallback, bind-placement mechanics, focus-follows-editor, gate-before-split, no MRU); `feat-list.md`.

### Implementation order

1. command.rs → 2. init.lua → 3. nvim.rs/api.rs → 4. wolf-core test counts → 5. wolf-ffi lib + regen → 6. wolf-ffi tests + BUILD → 7+8. Swift seams and glue in one change (CommandKind cases won't compile without dispatcher arms) → 9. Docs + prettier + markdown_check.

## Progress log

- Orchestration started; problem statement written.
- Architect plan received and approved (agent a4535421f7e347365); discriminant correction 20/21 → 26/27 noted.
- Test-writer pass 1: OtherPaneSeamTests (8 cases), other_pane.rs (5 fns), CommandSeamTests 26/27 + probe→28, guard/count updates, BUILD wiring both crates.
- Coder pass 1: full implementation landed; 83/83 Rust targets, other_pane 5/5 executed fresh, WolfKeyMapTests green incl. all 8 seam cases, app builds, lint clean.
- Test-writer pass 2: drawer-rejection and stale-bufnr-fallback tests added (7 total in other_pane.rs).
- Coder pass 2: other_pane 7/7 fresh; flagged unused_assignments warning in a test helper.
- Test-writer pass 3: warning fixed (dead pre-loop init removed). Coder verified 7/7 warning-free.
- Reviewer: approved; only doc divergences + shell checks outstanding.
- Coder: gen_bindings byte-identity confirmed (hash check; regeneration changes zero bytes).
- Technical-writer: ten Markdown files updated (helper entries, command_kind rows, OtherPaneSeam entries, architecture/command-lifecycle sentences, Decision 7 in preview-edit-command-decisions.md, feat-list row); two lint findings fixed on a second pass (entry budget, long line).
- Coder: final prettier unchanged, markdown_check no findings. Orchestration complete.
