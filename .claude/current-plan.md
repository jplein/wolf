# Plan: Four new File-menu commands (macOS)

## Status: COMPLETE

Docs updated (wolf-core/src, wolf-ffi, wolf-macos, wolf-macos/Sources READMEs + architecture.md decision 10) and prettier-clean (all unchanged by prettier). Stale "Discriminant contract" header comment in wolf-ffi/tests/commands.rs fixed to 0..8. Final state: all tests pass, `bazel build //crates/wolf-macos:Wolf` succeeds, `generated/wolf_ffi_wake.h` unchanged (empty diff). Not committed (per workflow).

---

## Status history: REVIEWER APPROVED — dispatching technical-writer

### Reviewer verdict: APPROVED

Discriminant contract consistent 5/6/7/8 across all layers; ⌘W/⇧⌘W swap + ⇧⌘O correct (lowercase keyEquiv + explicit mask); NSOpenPanel sheet closures leak-free ([weak self]); all exhaustive matches complete; wolf_ffi_wake.h unchanged. One NON-BLOCKING cosmetic note: the header doc block of `crates/wolf-ffi/tests/commands.rs` (~lines 20-26 "Discriminant contract") still enumerates only 0..4 — stale; the consts + 0..8 guard below it are correct. Route this comment fix to the technical-writer.

### Coder report

Implemented all of A–E per plan. Tests: `bazel test //crates/wolf-core/...` (7 pass), `//crates/wolf-ffi:commands //crates/wolf-ffi:file_actions //crates/wolf-ffi:bridge_smoke` (3/3 pass), `//crates/wolf-macos:WolfKeyMapTests` (pass), `bazel build //crates/wolf-macos:Wolf` (success). `gen_bindings` ran; `generated/wolf_ffi_wake.h` diff EMPTY (unchanged). Added `file_actions` rust_test target (tempfile via all_crate_deps normal_dev). Test files were pre-existing/plan-conformant; coder did not touch them. No README edits (technical-writer's job). SourceKit "No such module"/"Cannot find AppDelegate" diagnostics are IDE-only false positives (Bazel-built modules not resolved outside the Bazel graph) — the passing `bazel build` is authoritative.

## Architect's detailed plan (authoritative)

### wolf-core

- `command.rs`: append `Command` variants `GoToFile`(5), `OpenFile`(6), `OpenFolder`(7), `CloseFile`(8); append 4 `COMMAND_SPECS` rows (all `MenuGroup::File`), exact titles: `"Go to File…"`, `"Open File…"`, `"Open Folder…"`, `"Close File"` (ellipsis = single U+2026 for the 3 dialog/picker commands; none on Close File). ids/ex: `go_to_file`/`WolfGoToFile`, `open_file`/`WolfOpenFile`, `open_folder`/`WolfOpenFolder`, `close_file`/`WolfCloseFile`. Update enum discriminant doc.
- `input.rs`: add `InputEvent::Chdir { path: PathBuf }` and `InputEvent::CloseBuffer` (derive unaffected). `apply()` arms: `Chdir` → `command(nvim, &format!("cd {}", fnameescape(path)))`; `CloseBuffer` → `command(nvim, "bdelete")`. Decisions: GLOBAL `cd` (not lcd/tcd); non-bang `:bdelete` (protects unsaved edits, no last-buffer guard — nvim makes a fresh `[No Name]`, current_file → "").

### wolf-ffi (`crates/wolf-ffi/src/lib.rs`)

- `mod command_kind`: add `GO_TO_FILE=5, OPEN_FILE=6, OPEN_FOLDER=7, CLOSE_FILE=8`.
- `FfiCommand::from_command`: add 4 arms; update kind() doc + bridge comment `.. CLOSE_FILE`.
- Bridge `extern "Rust"` (next to `open`): add `fn chdir(&self, path: String);` and `fn close_buffer(&self);`.
- `impl WolfCoreHandle` (next to `open`): `chdir` → `send_input(InputEvent::Chdir{path: PathBuf::from(path)})`; `close_buffer` → `send_input(InputEvent::CloseBuffer)`. Modeled on `open`.
- **Run `bazel run //crates/wolf-ffi:gen_bindings`** and commit regenerated `generated/wolf-ffi.swift` + `wolf-ffi.h`; `generated/wolf_ffi_wake.h` MUST stay unchanged.

### Swift seam (`WolfKeyMap/CommandSeam.swift`)

- Add cases `goToFile/openFile/openFolder/closeFile`; `init?(ffi:)` arms `5..8`; update docs (0..8 → cases, else nil).

### macOS AppKit

- `CommandDispatcher.swift`: new signature adds `controller: WolfWindowController?`; arms: `.goToFile→controller?.openFilePicker()`, `.openFile→presentOpenFileDialog()`, `.openFolder→presentOpenFolderDialog()`, `.closeFile→closeCurrentBuffer()`.
- `WolfWindowController.swift`: thread `controller: self` into drainAndDispatchCommands dispatch. Add methods (all touched members are same-type private props, no visibility change): `openFilePicker()`→picker?.open(); `presentOpenFileDialog()`→NSOpenPanel (files, no dirs, no multi, directoryURL=currentCwd when non-empty, beginSheetModal(for: window), on .OK → handle.open(url.path) + refocusEditor()); `presentOpenFolderDialog()` → same but dirs-only → handle.chdir(url.path)+refocus; `closeCurrentBuffer()`→handle?.close_buffer(); private `refocusEditor()`→window?.makeFirstResponder(paneContainer). `[weak self]` in sheet closures.
- `AppDelegate.handleCommandMenuItem`: find `target = controllers.first { $0.owns(key) }`, pass `controller: target` (+ cwd = target?.currentCwd ?? "").
- `AppMenu.swift`: Tag consts 5..8; extend `commandItem` with `modifiers: NSEvent.ModifierFlags = .command` setting `keyEquivalentModifierMask`. Rebuild File menu in required order + separators: New Window(n) / sep / Go to File…(o) / Open File…("") / Open Folder…(o,[.command,.shift]) / sep / Close File(w) / Close Window(w,[.command,.shift]). ⇧⌘ = lowercase keyEquiv + explicit mask (NOT capital letter).

### Tests (real nvim, no mocks)

- `wolf-core/tests/commands.rs`: extend `command_specs_menu_groups_match_macos_convention` with the 4 new File rows. (Other pure tests already loop all specs.) Optional: one new-command drain in `ex_and_lua_faces_emit_commands`.
- `wolf-ffi/tests/commands.rs`: add local consts CMD*KIND*\* 5..8; extend contract-guard array to 0..8; add one `:WolfGoToFile` → kind()==5 drain.
- NEW `wolf-ffi/tests/file_actions.rs` (sibling; model on state.rs + commands.rs, reuse common/mod.rs): chdir → poll cwd() canonicalized == target; open a file → poll current_file() == file, then close_buffer() → poll current_file() == "". shutdown(). Loud-SKIP when no nvim.
- `CommandSeamTests.swift`: add ffi 5..8 → cases; change out-of-range boundary to 9 (and keep 255) → nil; update header comment 0..8.
- AppKit glue (NSOpenPanel/menu/dispatcher) stays compile-verified + manual-smoke.

### BUILD

- `crates/wolf-ffi/BUILD.bazel`: add `rust_test` `file_actions` (copy of `commands`, srcs = common/mod.rs + file_actions.rs, crate_root file_actions.rs, no-sandbox, nvim+init.lua data/env).
- wolf-macos BUILD: NO change (globs Sources/WolfMacOS/\*.swift; CommandSeam/CommandSeamTests already in explicit srcs). wolf-core BUILD: NO change.

### Implementation order

A wolf-core (command.rs+input.rs) → B wolf-ffi Rust + gen_bindings → C CommandSeam.swift → D AppKit group (CommandDispatcher→WolfWindowController→AppDelegate→AppMenu) → E tests → F docs.

### Docs (prettier --write each)

- `wolf-core/src/README.md` (Command list + InputEvent Chdir/CloseBuffer)
- `wolf-ffi/README.md` (chdir/close_buffer methods, command_kind table to 8, :file_actions target, wake.h unchanged)
- `wolf-macos/README.md` (AppMenu items+shortcuts, CommandDispatcher controller param+arms, WolfWindowController methods, command eyeball checklist +4)
- `wolf-macos/Sources/README.md` (CommandSeam/CommandDispatcher/WolfWindowController/AppMenu/AppDelegate bullets)
- `architecture.md` decision 10: stale "5 items"/"5-item" wording → nine commands + File additions.

### Lint

- `///` docs on new command_kind consts + InputEvent variants. Reference all 4 new wolf-ffi test consts in the guard array (dead_code). `[weak self]` in sheet closures.

### Verification

- `bazel run //crates/wolf-ffi:gen_bindings` (+ `git status` confirm wolf_ffi_wake.h unchanged)
- `bazel test //crates/wolf-core/...`
- `bazel test //crates/wolf-ffi:commands //crates/wolf-ffi:file_actions //crates/wolf-ffi:bridge_smoke`
- `bazel test //crates/wolf-macos:WolfKeyMapTests`
- `bazel build //crates/wolf-macos:Wolf`

## Problem statement

Add four new user commands to the macOS File menu, following the EXISTING command architecture (already shipped in a prior increment):

`wolf_core::Command` / `COMMAND_SPECS` → `wolf-ffi` `command_kind` → Swift `CommandKind` / `CommandDispatcher` → `AppMenu`.

### The four commands (all `MenuGroup::File`)

1. **Go to file** (`go_to_file` / `WolfGoToFile`): opens the existing file picker overlay (`PickerView`). macOS shortcut **⌘O**.
2. **Open file** (`open_file` / `WolfOpenFile`): native `NSOpenPanel` to choose a file anywhere, then opens it in this window's Neovim (`handle.open`). No shortcut.
3. **Open folder** (`open_folder` / `WolfOpenFolder`): native `NSOpenPanel` (directories), then updates Neovim's working directory to the chosen folder (`:cd`). Shortcut **⇧⌘O**.
4. **Close file** (`close_file` / `WolfCloseFile`): tells Neovim to delete the current buffer (`:bdelete`). Shortcut **⌘W**.

### Desired File menu order + shortcuts

- New window (⌘N) [keep existing item + shortcut]
- separator
- Go to file (⌘O)
- Open file (no shortcut)
- Open folder (⇧⌘O)
- separator
- Close file (⌘W)
- Close window (⇧⌘W) [moved from ⌘W to ⇧⌘W]

## Key implementation notes (from initial investigation)

- Discriminants are an ordered contract shared across `wolf_core::Command`, `wolf-ffi mod command_kind`, `FfiCommand::from_command`, and Swift `CommandKind`. Reordering is breaking; **APPEND** new variants after `MinimizeWindow`: `5=GoToFile, 6=OpenFile, 7=OpenFolder, 8=CloseFile`. AppMenu order is independent of discriminant order.
- Add rows to `COMMAND_SPECS` (`crates/wolf-core/src/command.rs`) in variant order (slice index == discriminant); all four are `MenuGroup::File`.
- Ex-command/Lua faces auto-register from `COMMAND_SPECS` via `runtime/nvim/init.lua` (data-driven) — no init.lua change needed.
- Two new native-facing Neovim actions on `WolfCoreHandle`: a chdir (`:cd <path>`, for Open folder) and a close-buffer (`:bdelete`, for Close file). Add `InputEvent` variants (`Chdir { path }`, `CloseBuffer`) in `crates/wolf-core/src/input.rs` with `apply()` arms using the existing `command()` / `fnameescape()` helpers, and expose `handle.chdir(path)` / `handle.close_buffer()` over the FFI bridge (`crates/wolf-ffi/src/lib.rs`), then regenerate bindings via `bazel run //crates/wolf-ffi:gen_bindings`.
- Go to file / Open file need NO new core action: Go to file opens the existing `PickerView`; Open file uses the existing `handle.open(path)`.
- Swift `CommandDispatcher.dispatch` needs the originating `WolfWindowController` to reach its picker/handle for the four new commands. Thread a `controller: WolfWindowController?` param through both call sites (`AppDelegate.handleCommandMenuItem` finds it via `controllers.first { $0.owns(key) }`; `WolfWindowController.drainAndDispatchCommands` passes `self`). Add controller methods: `openFilePicker()` (picker?.open()), `presentOpenFileDialog()` (NSOpenPanel files → handle.open + refocus paneContainer), `presentOpenFolderDialog()` (NSOpenPanel dirs → handle.chdir + refocus), `closeCurrentBuffer()` (handle.close_buffer()). NSOpenPanels start at `currentCwd`, `beginSheetModal` on the window.
- Update Swift `CommandKind` (`WolfKeyMap/CommandSeam.swift`) with the four new cases + `init?(ffi:)` arms 5..8.
- Update `AppMenu.swift`: add Tag constants, extend `commandItem` to accept a `keyEquivalentModifierMask` (default `.command`) so ⇧⌘O and ⇧⌘W work, and rebuild the File menu in the order above with separators. Change Close Window → ⇧⌘W.

## Tests (real Neovim, no mocks)

- `crates/wolf-core/tests/commands.rs`: menu-group expectations (4 new File commands), round-trip already loops over all specs.
- New/extended real-nvim integration test for the new `chdir` / `close_buffer` FFI methods (start nvim, chdir, assert `cwd()` changed; close_buffer and assert buffer count / current_file).
- `crates/wolf-macos/Tests/WolfKeyMapTests/CommandSeamTests.swift`: new discriminants (5..8 → cases; out-of-range → nil).

## Docs to update (source of truth; prettier --write after editing)

- `crates/wolf-core/src/README.md` (command list, new InputEvent variants)
- `crates/wolf-ffi/README.md` (new handle methods, command_kind list, note bindings regenerated)
- `crates/wolf-macos/README.md` + `Sources/README.md` (CommandKind cases, CommandDispatcher/AppMenu/WolfWindowController new behavior, command eyeball checklist + shortcuts)
- `architecture.md` if the command list / menu is enumerated there.
