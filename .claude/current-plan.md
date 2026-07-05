# Plan: macOS menu/command changes (New, View font-size, dynamic Window menu, Close→:close)

## Status: COMPLETE

All 5 requirements implemented, reviewed (code APPROVED), documented, and prettier-clean
(all 8 edited Markdown files unchanged by prettier). Tests pass: `//crates/wolf-core/...`,
`//crates/wolf-ffi:{commands,file_actions,close_pane,bridge_smoke}`,
`//crates/wolf-macos:WolfKeyMapTests`; `bazel build //crates/wolf-macos:Wolf` succeeds;
`generated/wolf_ffi_wake.h` unchanged. Not committed (per workflow). Orchestrator decision
recorded: `ShortcutSeam`/`ShortcutSeamTests` kept as a now-dormant pure seam (docs note it
is unwired).

### Reviewer verdict: code APPROVED (all 9 points pass); docs must be synced
Code is correct/complete/conformant across all four contract layers + AppKit glue.
Discriminants 0..7 unchanged, 8 renamed in place, 9..12 appended; input path, bridge,
Cmd-N routing, View/font, dynamic Window menu, Close validation, leak checks all verified.
BLOCKING = stale source-of-truth READMEs (see technical-writer task list below).

### Coder report
Implemented all of wolf-core / wolf-ffi / WolfKeyMap / WolfMacOS per plan. Regenerated
FFI bindings — `generated/wolf_ffi_wake.h` UNCHANGED (only wolf-ffi.h/.swift changed for
the close_buffer→close_pane rename + new_buffer add). Tests: `//crates/wolf-core/...`
7 pass; `//crates/wolf-ffi:commands :file_actions :close_pane :bridge_smoke` 4/4 pass;
`//crates/wolf-macos:WolfKeyMapTests` pass (incl. new FontSizeSeamTests + updated
CommandSeamTests); `bazel build //crates/wolf-macos:Wolf` succeeds. No test files touched.
DECISION (orchestrator): keep `ShortcutSeam.swift` + `ShortcutSeamTests` as a now-dormant
pure seam (its Cmd-N keyDown interception was removed); technical-writer to note it's
currently unwired. SourceKit "No such module" diagnostics are IDE-only false positives.

### Test-writer report
Created: `WolfKeyMapTests/FontSizeSeamTests.swift`, `wolf-ffi/tests/close_pane.rs`.
Modified: `wolf-core/tests/commands.rs`, `wolf-ffi/tests/commands.rs`,
`wolf-ffi/tests/file_actions.rs` (test fn now `file_actions_chdir_and_new_buffer`),
`WolfKeyMapTests/CommandSeamTests.swift`, `wolf-ffi/BUILD.bazel` (new `close_pane`
rust_test), `wolf-macos/BUILD.bazel` (FontSizeSeam.swift + FontSizeSeamTests.swift srcs).
Notes: FontSizeSeam API MUST be `Double -> Double` with `minPointSize/maxPointSize/step`
as `Double` (tests assert with accuracy eps). `close_pane.rs` types `:vsplit` to set up
but calls `handle.close_pane()` directly (RPC, not typed). SourceKit "No such module
XCTest" is an IDE-only false positive (Bazel build authoritative).

## Problem statement

Implement a set of macOS menu/command changes for Wolf. The command architecture is a
fixed `wolf_core::Command` enum + `COMMAND_SPECS` registry
(`crates/wolf-core/src/command.rs`) whose variant order is a discriminant contract
shared with wolf-ffi's `command_kind` module (`crates/wolf-ffi/src/lib.rs`) and the
Swift `CommandKind` port (`crates/wolf-macos/Sources/WolfKeyMap/CommandSeam.swift`).
Adding a command = append a `Command` variant + a `COMMAND_SPECS` row + an FFI
discriminant + a Swift `CommandKind` case + a `CommandDispatcher` action; `init.lua` is
fed `COMMAND_SPECS` over FFI so the `:Wolf*` Ex-command and `require('wolf').*()` Lua
faces register automatically. The native menu is hardcoded in `AppMenu.swift`; every
item carries its `command_kind` byte in `NSMenuItem.tag`. **Reordering existing
`Command` variants is BREAKING — only APPEND new variants (new discriminants).** Current
discriminants: NewWindow(0), CloseWindow(1), QuitApplication(2), HideApplication(3),
MinimizeWindow(4), GoToFile(5), OpenFile(6), OpenFolder(7), CloseFile(8).

## Requirements

1. **File > New (`:enew`)** — NEW command, Cmd-N. New empty buffer in the CURRENT
   window. New Command variant (suggested id `new_buffer`/`WolfNewBuffer`/title "New"/
   `MenuGroup::File`), FFI discriminant, Swift CommandKind, dispatcher action, and a core
   input path (InputEvent + FFI handle method) running `:enew`. All three faces act on
   the right window (menu → key window; nvim-drained → typed window).

2. **New Window** — remap shortcut Cmd-N → Shift-Cmd-N (AppMenu only: lowercase "n" +
   `[.command, .shift]`). Command/item otherwise unchanged.

3. **View menu (NEW top-level menu)** — three NEW commands, PER-WINDOW + EPHEMERAL (not
   persisted; a fresh/Cmd-N window starts at config default):
   - Reset font size (Cmd-0) → config default.
   - Increase font size (Cmd-=) → +1pt, clamp max (~72).
   - Decrease font size (Cmd--) → -1pt, clamp min (~6).
   New `MenuGroup::View`, new Command variants, FFI discriminants, Swift CommandKind,
   dispatcher actions. Each WolfWindowController owns its OWN font size + CellMetrics
   (today measured once in AppDelegate and shared). Action re-measures CellMetrics at the
   new size and reflows: keep OS window frame, grid cols/rows reflow, re-sync grid sizes
   to nvim, repaint. Acts on the right window.

4. **Window menu** — Minimize (Cmd-M, unchanged), separator, then a DYNAMIC list: one
   item per open window, titled with that window's FULL cwd path; clicking raises/
   activates that window (makeKeyAndOrderFront + activate). NOT in COMMAND_SPECS —
   native dynamic items built by AppDelegate, rebuilt on window open/close/cwd-change.

5. **Close command** — currently `Command::CloseFile` (menu "Close File", Cmd-W) runs
   `:bdelete` via `InputEvent::CloseBuffer`. CHANGE to run `C-w c` / `:close` (close the
   split, NOT the buffer). Rename label to "Close". No last-split fallback (matches
   Neovim E444). Menu item shown DISABLED when it's the last split (NSMenuItem
   validation driven by the key window's pane count / PaneTree leaf count > 1). Architect
   decides whether to rename id/ex_command (`close_file`/`WolfCloseFile` →
   e.g. `close_pane`/`WolfClosePane`). Change `InputEvent::CloseBuffer`'s action from
   `bdelete` to `close` (or add a distinct event). Close Window (Shift-Cmd-W) UNCHANGED.

## Clarified decisions (from user)

- New → Cmd-N; New Window → Shift-Cmd-N.
- Close on last split: NO fallback; show the menu item DISABLED on the last split.
- View font-size commands are per-window and ephemeral (not persisted).

## Constraints

- Documentation is source of truth: update every affected README
  (`crates/wolf-core/src/README.md`, `crates/wolf-ffi/src/README.md`,
  `crates/wolf-macos/README.md`, `crates/wolf-macos/Sources/README.md`, crate-level
  READMEs, and `architecture.md` if it enumerates commands/menus). Reformat edited
  Markdown with `prettier --write`; no hard line breaks within paragraphs.
- Prefer real-nvim end-to-end/integration tests (wolf-ffi tests start a real Neovim).
  New commands' core behavior (`:enew` creating a new buffer, `:close` closing a split)
  covered by real-nvim tests. Pure Swift seam (CommandSeam discriminant contract)
  headlessly tested by WolfKeyMapTests.
- AppKit glue (AppMenu, CommandDispatcher, per-window font relayout, dynamic Window menu,
  menu validation) is compile-verified + manual-smoke only; the pure decode (CommandSeam)
  and core/FFI surface MUST have tests.
- Builds: `bazel build //crates/wolf-macos:Wolf`,
  `bazel test //crates/wolf-macos:WolfKeyMapTests`, wolf-ffi real-nvim tests. Rust core
  builds with Bazel (bazelisk) on macOS. Regenerate FFI bindings via
  `bazel run //crates/wolf-ffi:gen_bindings`; `generated/wolf_ffi_wake.h` must stay
  unchanged unless the wake ABI changes.

## Final menu layout (target)

- **File**: New (Cmd-N) / New Window (Shift-Cmd-N) / sep / Go to File (Cmd-O) / Open
  File / Open Folder (Shift-Cmd-O) / sep / Close (Cmd-W) / Close Window (Shift-Cmd-W).
- **View**: Reset font size (Cmd-0) / Increase font size (Cmd-=) / Decrease font size
  (Cmd--).
- **Window**: Minimize (Cmd-M) / sep / [dynamic window list].

## Architect plan (authoritative)

### Discriminant contract (final, APPEND-ONLY)

`wolf_core::Command` order == `command_kind` u8 == Swift `CommandKind`:
NewWindow(0), CloseWindow(1), QuitApplication(2), HideApplication(3), MinimizeWindow(4),
GoToFile(5), OpenFile(6), OpenFolder(7), **ClosePane(8, renamed in-place from CloseFile)**,
**NewBuffer(9)**, **ResetFontSize(10)**, **IncreaseFontSize(11)**, **DecreaseFontSize(12)**.
`MenuGroup` gains a `View` variant (not FFI-numbered; append after `Window`).

### Files to CREATE

- `crates/wolf-macos/Sources/WolfKeyMap/FontSizeSeam.swift` — pure Foundation namespace
  `public enum FontSizeSeam`: `minPointSize=6`, `maxPointSize=72`, `step=1`,
  `clamped(_:)`, `increased(_:)=clamped(current+step)`, `decreased(_:)=clamped(current-step)`.
  Reset is NOT in the seam (controller resets to its stored default).
- `crates/wolf-macos/Tests/WolfKeyMapTests/FontSizeSeamTests.swift` — step ±1, clamp at
  6/72, in-range pass-through, 71→72, 72→72, 7→6, 6→6.
- `crates/wolf-ffi/tests/close_pane.rs` — real-nvim (model on `panes.rs`, reuse
  `tests/common/mod.rs`, loud-skip w/o `WOLF_NVIM_BIN`): start → 1 grid → `:vsplit` →
  `windows()` live editor grids == 2 → `handle.close_pane()` → poll `windows()` == 1.
  Use the FFI method directly (nvim_command bypasses init.lua interactive overrides).

### wolf-core

- `command.rs`: rename `CloseFile`→`ClosePane` (stays pos 8); append `NewBuffer`,
  `ResetFontSize`, `IncreaseFontSize`, `DecreaseFontSize` (9–12); add `MenuGroup::View`;
  update doc comments. `COMMAND_SPECS`: pos-8 row →
  `{ClosePane, "close_pane", "WolfClosePane", "Close", File}`; append
  `{NewBuffer,"new_buffer","WolfNewBuffer","New",File}`,
  `{ResetFontSize,"reset_font_size","WolfResetFontSize","Reset Font Size",View}`,
  `{IncreaseFontSize,"increase_font_size","WolfIncreaseFontSize","Increase Font Size",View}`,
  `{DecreaseFontSize,"decrease_font_size","WolfDecreaseFontSize","Decrease Font Size",View}`.
- `input.rs`: rename `CloseBuffer`→`ClosePane` (doc `:close`); append `NewBuffer`
  (doc `:enew`). `apply()`: `ClosePane => command(nvim,"close")`,
  `NewBuffer => command(nvim,"enew")`.
- No other core files (api.rs/lib.rs re-exports carry new variants automatically).

### wolf-ffi (`crates/wolf-ffi/src/lib.rs`)

- `mod command_kind`: rename `CLOSE_FILE`→`CLOSE_PANE` (=8); add `NEW_BUFFER=9`,
  `RESET_FONT_SIZE=10`, `INCREASE_FONT_SIZE=11`, `DECREASE_FONT_SIZE=12`; update doc range.
- `FfiCommand::from_command`: rename CloseFile arm→ClosePane; add 4 arms. Keep the match
  EXHAUSTIVE (no wildcard).
- `extern "Rust"` bridge: rename `close_buffer`→`close_pane`; add `new_buffer`
  (update comments `:close`/`:enew`).
- `impl WolfCoreHandle`: rename `close_buffer`→`close_pane` (send `InputEvent::ClosePane`);
  add `new_buffer` (send `InputEvent::NewBuffer`). Font/View commands need NO FFI handle
  method (only their discriminants cross for the drained face).
- Regenerate: `bazel run //crates/wolf-ffi:gen_bindings`; `generated/wolf-ffi.swift`
  reflects the rename + added fn; `generated/wolf_ffi_wake.h` MUST stay unchanged.

### Swift WolfKeyMap

- `CommandSeam.swift`: rename `.closeFile`→`.closePane`; add `.newBuffer`,
  `.resetFontSize`, `.increaseFontSize`, `.decreaseFontSize`; `init?(ffi:)`
  `8→.closePane`, `9..12→new cases`, default nil. Update header doc.

### Swift WolfMacOS (AppKit glue — `srcs=glob`, no BUILD change for WolfMacOS)

- `AppMenu.swift`: `Tag` rename `closeFile`→`closePane`(8) + `newBuffer=9`,
  `resetFontSize=10`, `increaseFontSize=11`, `decreaseFontSize=12`. `install(target:)`
  returns the Window `NSMenu` and inserts a View menu between File and Window. File menu:
  prepend "New" (newBuffer, "n", .command); "New Window" → modifiers `[.command,.shift]`
  (keyEquiv "n"); "Close File"→"Close" (closePane, "w", .command). View menu: Reset Font
  Size ("0"), Increase Font Size ("="), Decrease Font Size ("-"), all .command. New
  `static rebuildWindowMenu(_:controllers:target:)`: removeAllItems → Minimize("m") +
  separator → one item per controller titled `currentCwd` (fallback "Wolf"),
  action `#selector(AppDelegate.raiseWindowMenuItem(_:))`, target, `tag=windowIndex`.
- `CommandDispatcher.swift`: rename `.closeFile`→`.closePane`→`controller?.closePane()`;
  add `.newBuffer→newBuffer()`, `.resetFontSize→resetFontSize()`,
  `.increaseFontSize→increaseFontSize()`, `.decreaseFontSize→decreaseFontSize()`.
- `AppDelegate.swift`: store `windowMenu` from `install`; call `rebuildWindowMenu()` at
  startup, in `makeNewWindow`, in `removeController`; `@objc raiseWindowMenuItem(_:)`
  finds `controllers.first{ $0.windowIndex==sender.tag }`→`raise()`; add
  `windowCwdDidChange(_:)`→rebuild; implement `validateMenuItem(_:)` — true except for the
  `.closePane` command item, which is `(keyWindow controller?.paneCount ?? 0) > 1`.
- `WolfWindowController.swift`: `metrics` → `var`; add `defaultPointSize` +
  `currentPointSize` (seed from `CellMetrics.defaultPointSize(config:)`, no re-measure at
  start); rename `closeCurrentBuffer`→`closePane()`→`handle?.close_pane()`; add
  `newBuffer()`→`handle?.new_buffer()`; add `resetFontSize/increaseFontSize/
  decreaseFontSize` → `setPointSize(...)`; `setPointSize(_:)` guards change, re-measures
  `CellMetrics.measured(config:pointSize:)`, `paneContainer?.applyMetrics(metrics)`, NO
  `setFrame`; add `raise()`; add `paneCount`; add `lastKnownCwd` + cwd-change hook in
  `onRenderWake()` → `coordinator?.windowCwdDidChange(self)` only on real change. Cleanup:
  remove the `onNewWindow`/`requestNewWindow()` wiring (Cmd-N now = New).
- `CellMetrics.swift`: add `static measured(config:pointSize:)`; keep `measured(config:)`
  delegating with `defaultPointSize(config:)`; rename private `pointSize(config:)`→
  internal `static defaultPointSize(config:)` (honors `WOLF_FONT_PT` else `config.font_size`).
  Reset target = this seed (honors `WOLF_FONT_PT`).
- `PaneContainerView.swift`: `metrics`→`var`; add
  `applyMetrics(_:)`{metrics=…; syncGridSizes(); needsDisplay=true}; add
  `paneCount { tree.leaves().count }`. Cleanup: remove `onNewWindow` +
  `ShortcutSeam.isNewWindowTrigger` interception in `keyDown` (menu key-equivalents
  preempt keyDown; leaving it would wrongly open a window on Cmd-N).
- `MainWindow.swift`: NO change (metrics consumed only at init for initial content size;
  font changes deliberately keep the OS frame).

### Tests to MODIFY

- `wolf-core/tests/commands.rs`: rename CloseFile→ClosePane steps; menu-group test →
  `(ClosePane,File)`, append `(NewBuffer,File)`, `(ResetFontSize,View)`,
  `(IncreaseFontSize,View)`, `(DecreaseFontSize,View)`; generic round-trip tests unchanged.
- `wolf-ffi/tests/commands.rs`: rename `CMD_KIND_CLOSE_FILE`→`_CLOSE_PANE`(8); add
  `_NEW_BUFFER=9`, `_RESET_FONT_SIZE=10`, `_INCREASE_FONT_SIZE=11`,
  `_DECREASE_FONT_SIZE=12`; extend contract guard to contiguous `0..=12`; optional new drains.
- `wolf-ffi/tests/file_actions.rs`: repoint the `close_buffer`/`:bdelete` step to
  `handle.new_buffer()`/`:enew` (same "current_file becomes empty" assertion); update prose.
  The `:close` split-drop coverage lives in the new `close_pane.rs`.
- `WolfKeyMapTests/CommandSeamTests.swift`: `8→.closePane`, add `9..12`; move out-of-range
  boundary from 9 to 13 (keep 255→nil).

### BUILD

- `crates/wolf-ffi/BUILD.bazel`: add `rust_test` `close_pane` (copy `commands`/`panes`
  target: no-sandbox, nvim+init.lua data/env, all_crate_deps(normal_dev=True),
  srcs=[tests/close_pane.rs, tests/common/mod.rs], crate_root=tests/close_pane.rs).
- `crates/wolf-macos/BUILD.bazel`: add `Sources/WolfKeyMap/FontSizeSeam.swift` to the
  WolfKeyMap `swift_library` srcs; add `Tests/WolfKeyMapTests/FontSizeSeamTests.swift` to
  `WolfKeyMapTests` srcs. WolfMacOS glob unchanged.
- `runtime/nvim/init.lua`: NO change (command registration is data-driven from COMMAND_SPECS).

### Implementation order

1. wolf-core (command.rs + input.rs) + wolf-core tests. 2. wolf-ffi src.
3. gen_bindings (confirm wake.h unchanged). 4. wolf-ffi tests + BUILD (close_pane).
5. WolfKeyMap (CommandSeam + FontSizeSeam) + BUILD + tests. 6. WolfMacOS glue
(CellMetrics→PaneContainerView→WolfWindowController→CommandDispatcher→AppMenu→AppDelegate).
7. Docs + prettier. 8. Builds/tests.

### Docs to update (then `prettier --write`)

`wolf-core/src/README.md`, `wolf-core/README.md` (if it enumerates), `wolf-ffi/src/README.md`,
`wolf-ffi/README.md` (if it names close_buffer), `wolf-macos/README.md`,
`wolf-macos/Sources/README.md`, `wolf-ffi/tests/README.md`, `wolf-macos/Tests/README.md`,
`architecture.md` (if it enumerates commands/menus).

### Key risks / decisions

- Cmd-N routing: menu key-equivalents preempt `keyDown`; remove the legacy ShortcutSeam
  Cmd-N interception so plain Cmd-N = `:enew`, not new window.
- Close: rename keeps discriminant 8; `:close` errors harmlessly (E444) on last split, and
  the menu item is additionally disabled via `validateMenuItem`. Pre-release id/ex_command
  rename to `WolfClosePane` is intentional.
- Font size is front-end-only: no core/FFI/state.json; per-window size+metrics; reset
  target = launch seed (honors `WOLF_FONT_PT`).
- Keep `FfiCommand::from_command` exhaustive (no catch-all).
