# Current Plan: Copy & Paste commands + Edit menu

## Problem statement

Add two new commands, `copy` and `paste`, forming a new **Edit** menu (the only two entries on it).

- **Copy** is disabled unless the active document in the active window is in **visual mode**. It copies everything in the visual-mode selection onto the system clipboard.
- **Paste** pastes from the system clipboard into the active document.

## Status

- [x] Architect plan
- [x] Tests written — command.rs units; NEW tests/copy_paste.rs (core + ffi); commands.rs extended; BUILD.bazel copy_paste target; CommandSeamTests.swift. Tokens: core copy=WOLFCOPY paste=WOLFPASTE; ffi copy=WOLF paste=Z. Copy uses inclusive visual-y via unnamed register (must be synchronous).
- [x] Implementation + tests passing — all core/ffi/macos targets green; copy_paste stable across 3 runs; palette_rank fixed (STATIC_COMMAND_COUNT 13→15); Wolf app builds. Coder also added a copy_paste rust_test target to wolf-core/BUILD.bazel.
- [x] Review — APPROVED, no blockers/nits. All 8 checks conform (append-only contract, copy/paste correctness, visual-mode enablement, Edit menu, dispatcher, GUI-free core, generated bindings).
- [x] Documentation — architecture.md + all crate READMEs (core/ffi/macos + src/tests) synced; prettier --write clean (all unchanged). Also brought crate-level wolf-macos/README.md in sync.

## COMPLETE ✅

Copy & Paste commands + Edit menu delivered. Command::Copy=14 / Paste=15 appended (APPEND-ONLY intact); MenuGroup::Edit; WolfCore::copy_visual_selection/paste; NvimProcess::yank_visual_selection (mode-guarded synchronous yank of unnamed register); Snapshot::is_visual_mode; InputEvent::Paste→nvim_paste; FFI copy_selection/paste/is_visual_mode + regenerated bindings; macOS Edit menu (Copy ⌘C / Paste ⌘V between File and View) with Copy gated on visual mode via validateMenuItem, NSPasteboard bridging. All tests green (core/ffi/macos), Wolf app builds, docs synced. REMAINING (not automatable): manual GUI smoke — launch app, confirm Edit menu shows Copy/Paste, Copy greys out outside visual mode, and a visual-select→Copy→Paste round-trip works via the system clipboard.

---

# Architect plan

## 1. Summary

Add two registry commands, `copy` and `paste`, forming a new **Edit** menu (its only two entries) on macOS. **Copy** is enabled only when the active window is in Neovim visual mode (`v`/`V`/`<C-v>`); it yanks the visual selection out of the captive Neovim over RPC and the front-end writes the returned text to `NSPasteboard`. **Paste** reads a string from `NSPasteboard` and inserts it into the active document via Neovim's `nvim_paste` (correct in normal/insert/visual/cmdline modes). The commands append to the canonical registry as discriminants **14 (`copy`)** and **15 (`paste`)** — strictly APPEND-ONLY. Copy needs a synchronous return value (selection text), so it is a blocking core method, not a fire-and-forget input event; visual-mode enablement rides a new snapshot flag exposed over FFI.

## 2. APPEND-ONLY discriminant contract (call-out)

`Command` enum variant order IS the FFI `command_kind` u8. Currently `0..=13`. Append `Copy = 14`, `Paste = 15` at the END of the enum AND `COMMAND_SPECS`. Never reorder.

Locations to keep in sync (all appended):
1. `crates/wolf-core/src/command.rs` — `Command` enum + `COMMAND_SPECS` + doc comment + test counts.
2. `crates/wolf-ffi/src/lib.rs` — `mod command_kind` consts + `FfiCommand::from_command` match (exhaustive).
3. `crates/wolf-macos/Sources/WolfKeyMap/CommandSeam.swift` — `CommandKind` cases + `init?(ffi:)`.
4. `crates/wolf-macos/Sources/WolfMacOS/AppMenu.swift` — `Tag` consts + Edit menu wiring.
5. `crates/wolf-macos/Sources/WolfMacOS/CommandDispatcher.swift` — `dispatch` switch (exhaustive).
6. Tests: `crates/wolf-ffi/tests/commands.rs` guard (`0..=15`), `CommandSeamTests.swift`, `command.rs` units.
7. `crates/wolf-ffi/generated/wolf-ffi.swift` / `wolf-ffi.h` — regenerated (new handle methods).

## 3. wolf-core changes

### 3.1 `crates/wolf-core/src/command.rs`
- `Command` enum: append `Copy` then `Paste` after `CommandPalette` (line 63). Doc comment (26–32) extend to `CommandPalette(13), Copy(14), Paste(15)`.
- `MenuGroup` enum (71–81): add variant `Edit` (order cosmetic — not a discriminant).
- `COMMAND_SPECS` (ends line 210): append rows: `Copy` {id `"copy"`, ex `"WolfCopy"`, title `"Copy"`, menu `Edit`}; `Paste` {id `"paste"`, ex `"WolfPaste"`, title `"Paste"`, menu `Edit`}.
- Tests: `EXPECTED_COMMAND_COUNT` 14→16; add `copy_id_round_trips`/`paste_id_round_trips`, spec-field pins, append guards `COMMAND_SPECS[14].command == Command::Copy`, `[15] == Command::Paste`.

### 3.2 `crates/wolf-core/src/input.rs`
- `InputEvent` enum (after `NewBuffer`, line 201): add `Paste { text: String }`. Derives unchanged (String is Clone/Debug/PartialEq/Eq).
- `apply` match (294–339): arm `InputEvent::Paste { text } => nvim_paste(nvim, text).await`; new private helper `nvim_paste` → `nvim.call("nvim_paste", vec![Value::from(text.as_str()), Value::from(true), Value::from(-1i64)])` (crlf=true, phase=-1 single-shot).
- `apply_batch` no change (non-Key events flush then apply individually).

### 3.3 `crates/wolf-core/src/nvim.rs`
- Add async method on `NvimProcess` (sibling of `current_dir`, 190–207): `pub async fn yank_visual_selection(&self) -> Result<Option<String>, NvimError>`.
- Impl: one `nvim_exec_lua` with a STATIC (injection-safe) Lua snippet: read `nvim_get_mode().mode`; return `vim.NIL` unless mode ∈ {`v`, `V`, `"\22"` (Ctrl-V)}; else `nvim_feedkeys('y','nx',false)` (`x` = synchronous, drains typeahead so yank completes) then return `vim.fn.getreg('"')` (unnamed register — matches vim visual-y semantics). Map `as_str()`→`Some`, Nil/absent→`None`. Errors → `NvimError::Rpc`. Mode-guard prevents a `y` operator-hang if `:WolfCopy` fires outside visual mode.

### 3.4 `crates/wolf-core/src/api.rs`
- On `WolfCore` (sibling of `resolve_search_root`): `#[must_use] pub fn copy_visual_selection(&self) -> Option<String>` → `self.runtime.block_on(self.nvim.yank_visual_selection())`, mapping Err/Ok(None)/empty→None. `pub fn paste(&self, text: String)` → `self.send_input(InputEvent::Paste { text })`.

### 3.5 `crates/wolf-core/src/render.rs`
- On `impl Snapshot` (near `active_grid_id`, line 530): `#[must_use] pub fn is_visual_mode(&self) -> bool` — resolves `self.mode_infos.get(self.cursor.mode_idx)`, returns true when `mode.name == "visual"` (nvim uses one `"visual"` cursor-mode-info for v/V/<C-v>; excludes `"visual_select"`). Cite: `ModeInfo.name` set in `redraw.rs` mode_info_set (~710); `cursor.mode_idx` updated in mode_change (~739).

### 3.6 Re-exports
- `WolfCore`, `Snapshot`, `InputEvent` already re-exported from `lib.rs`. No new public type. Confirm `Snapshot::is_visual_mode` reachable via `SnapshotReadGuard` deref (it is).

## 4. wolf-ffi changes

### 4.1 `crates/wolf-ffi/src/lib.rs`
- `mod command_kind` (140–169): append `pub const COPY: u8 = 14;`, `pub const PASTE: u8 = 15;`.
- `FfiCommand::from_command` (1662–1680): arms `Command::Copy => command_kind::COPY`, `Command::Paste => command_kind::PASTE`. (Palette `FfiCommandRow::from_row` reuses this.)
- `FfiCommand::kind()` doc (~610, ~1653): range text → `NEW_WINDOW .. PASTE`.
- Bridge `extern "Rust"` block for `WolfCoreHandle` (325–432), near close_pane/new_buffer:
  - `fn copy_selection(&self) -> String;`
  - `fn paste(&self, text: String);`
  - `fn is_visual_mode(&self) -> bool;`
- `impl WolfCoreHandle` (near 1388–1403):
  - `copy_selection` → `self.core.borrow().as_ref().and_then(|c| c.copy_visual_selection()).unwrap_or_default()`.
  - `paste` → guard `if let Some(core) = self.core.borrow().as_ref() { core.paste(text); }`.
  - `is_visual_mode` → `self.core.borrow().as_ref().map_or(false, |c| c.read_snapshot().is_visual_mode())`.

### 4.2 Regenerate bindings
- Run the explicit Bazel codegen target (runs `src/bin/gen_bindings.rs`) → regenerate + commit `generated/wolf-ffi.swift` + `wolf-ffi.h`. `wolf_ffi_wake.h` unchanged (no new wake).

## 5. wolf-macos changes

### 5.1 `Sources/WolfKeyMap/CommandSeam.swift`
- `CommandKind`: append `case copy`, `case paste` after `.commandPalette`. Doc (20–22): note `copy == 14`, `paste == 15`. `init?(ffi:)` (56–74): `case 14: self = .copy`, `case 15: self = .paste` before default.

### 5.2 `Sources/WolfMacOS/AppMenu.swift`
- `Tag` enum (21–36): `static let copy = 14`, `static let paste = 15`.
- `install(target:)` (51–64): insert Edit menu between File and View (App/File/**Edit**/View/Window).
- New `editMenuItem(target:)` (mirror `viewMenuItem`): `NSMenu(title: "Edit")` with two `commandItem`s — Copy (tag `.copy`, key `"c"` → ⌘C), Paste (tag `.paste`, key `"v"` → ⌘V), both via `handleCommandMenuItem`.

### 5.3 `Sources/WolfMacOS/CommandDispatcher.swift`
- `dispatch` switch (34–63): `case .copy: controller?.copy()`, `case .paste: controller?.paste()`.

### 5.4 `Sources/WolfMacOS/WolfWindowController.swift`
- Edit commands section (~466):
  - `func copy()` — `guard let s = handle?.copy_selection().toString(), !s.isEmpty else { return }`; `NSPasteboard.general.clearContents(); .setString(s, forType: .string)`.
  - `func paste()` — `guard let s = NSPasteboard.general.string(forType: .string) else { return }`; `handle?.paste(s)`.
- Near `paneCount` (~535): `var isVisualMode: Bool { handle?.is_visual_mode() ?? false }`.
- `import AppKit` already present.

### 5.5 `Sources/WolfMacOS/AppDelegate.swift`
- `validateMenuItem(_:)` (239–247): add branch mirroring `.closePane` — when item decodes to `.copy`, return `(keyController?.isVisualMode ?? false)` (`keyController = controllers.first { $0.owns(NSApp.keyWindow) }`). `.paste` + others fall through to `return true`.

## 6. Test plan (prefer real-nvim)

### 6.1 `crates/wolf-core/src/command.rs` (unit) — modify
- Count 14→16; `copy`/`paste` `from_id` round-trips; spec-field pins (id/ex/title/`MenuGroup::Edit`); append guards `[14]==Copy`, `[15]==Paste`.

### 6.2 `crates/wolf-core/tests/copy_paste.rs` (real nvim) — NEW
Reuse `tests/common/mod.rs` (`start_core_with_temp_md`, `waiter`, `FLUSH_TIMEOUT`), model on `input_keys.rs`.
- Mode detection: normal → `is_visual_mode()==false`; send `v`, flush, `==true`; `<Esc>`, `==false`. Repeat with `V` and `<C-v>` — all report `"visual"`.
- Copy: cursor at line start, `v` + motion to select known run, `copy_visual_selection()` == `Some(expected substring)` (stable prefix from fixture).
- Copy outside visual: normal mode → `None`.
- Paste round-trip: normal mode, `paste("WOLFPASTE")`, flush, read `active_grid().row_text(row)` contains `"WOLFPASTE"`.
- `shutdown()`.

### 6.3 `crates/wolf-ffi/tests/commands.rs` (real nvim) — modify
- Consts `CMD_KIND_COPY=14`, `CMD_KIND_PASTE=15`; extend guard array to `0..=15`.
- `:WolfCopy` → `poll_for_command_kind(COPY)`; `:WolfPaste` → `poll_for_command_kind(PASTE)`.

### 6.4 `crates/wolf-ffi/tests/copy_paste.rs` (real nvim) — NEW
Model on `close_pane.rs` (SKIP via `common::nvim_bin_or_skip`, `send_ex_command`, poll on `read_frame`).
- Type known word, enter visual selection via `send_key` chars, poll `handle.is_visual_mode()` true, `handle.copy_selection()` == selected text.
- `is_visual_mode()` false after `<Esc>`.
- `handle.paste("Z")` normal mode; poll `read_frame`/`read_frame_for_grid` cells for pasted glyph (bridge_smoke pattern).
- `shutdown()`.

### 6.5 `crates/wolf-ffi/BUILD.bazel` — modify
- Add `rust_test` target `copy_paste` (copy of `close_pane` stanza 268–289: same tags/size/data/env/deps, srcs/crate_root → `tests/copy_paste.rs`). `commands` target already covers modified `commands.rs`.

### 6.6 `crates/wolf-macos/Tests/WolfKeyMapTests/CommandSeamTests.swift` — modify
- `testCommandKindFfiDiscriminants`: add `(ffi:14)==.copy`, `(ffi:15)==.paste`.
- `testCommandKindOutOfRangeIsNil`: first out-of-range 14→16 (keep 255).

Note: `CommandDispatcher`/`AppMenu`/`validateMenuItem`/NSPasteboard are AppKit `WolfMacOS` target = compile-verified + manual-smoke only (existing policy); the pure decode seam is headlessly unit-tested.

## 7. Design decisions (architect recommendations, ACCEPTED)
- Copy yank: `nvim_feedkeys('y','nx',…)` + `getreg` in ONE mode-guarded `nvim_exec_lua` (atomic/synchronous). Unnamed register `"` (matches vim visual-y). Overwriting unnamed register is expected.
- Paste: `nvim_paste(text, true, -1)` (correct in all modes; no mode branching).
- Yank Lua: inline static string in `nvim.rs` (no channel-id/init.lua churn). [init.lua-helper alternative noted but not chosen.]
- Paste menu: always-enabled (matches request).
- Palette: Copy/Paste auto-become palette rows; Copy from palette outside visual mode is a harmless empty no-op.

## 8. Documentation to update (then `prettier --write`)
- `architecture.md` — Decision 10 discriminant list; "Output: commands" (~195); wolf-ffi paragraph (~42): extend command_kind table `…COMMAND_PALETTE=13 / COPY=14 / PASTE=15`, describe Edit menu (Copy ⌘C / Paste ⌘V), Copy visual-mode enablement, new FFI methods. Update macOS Phase status menu inventory (~64).
- `crates/wolf-core/README.md` + `src/README.md` — registry adds copy/paste (Edit); `copy_visual_selection`/`paste`; `yank_visual_selection`; `is_visual_mode`; `InputEvent::Paste`.
- `crates/wolf-ffi/README.md` + `src/README.md` — command_kind→15; handle methods; regenerated bindings.
- `crates/wolf-macos/Sources/README.md` — Edit menu, `CommandKind.copy/.paste`, dispatcher cases, `validateMenuItem` Copy gating, controller `copy()/paste()/isVisualMode`, NSPasteboard.
- `crates/wolf-macos/Tests/README.md` + `crates/wolf-ffi/tests/README.md` — new `copy_paste` tests + extended discriminant coverage.

## 9. Implementation order
1. `command.rs` (enum/specs/group/tests). 2. `input.rs` (Paste + nvim_paste). 3. `nvim.rs` (yank_visual_selection). 4. `render.rs` (is_visual_mode). 5. `api.rs` (copy_visual_selection/paste). 6. `wolf-ffi/lib.rs` (command_kind/from_command/bridge/impl). 7. regenerate `generated/`. 8. `CommandSeam.swift`. 9. `WolfWindowController.swift`. 10. `CommandDispatcher.swift`. 11. `AppMenu.swift`. 12. `AppDelegate.swift`. 13. Tests. 14. Docs + prettier.

## 10. Test targets
- Cargo: `cargo test -p wolf-core`, `cargo test -p wolf-ffi`.
- Bazel (macOS): `bazel test //crates/wolf-core:...`, `//crates/wolf-ffi:commands`, `//crates/wolf-ffi:copy_paste` (new), `//crates/wolf-ffi:bridge_smoke`, `//crates/wolf-macos:WolfKeyMapTests`. Build `//crates/wolf-macos:Wolf`.

## 11. Dependencies & lint
- No new external packages (`nvim_paste`/`nvim_get_mode`/`getreg` are core RPC; NSPasteboard is AppKit). No new `#[allow]`. `#[must_use]` on value-returning methods. Static Lua string = no injection concern.
