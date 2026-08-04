# Command lifecycle

Where a Wolf **command** is configured, and how it travels from a keystroke or a menu click into the captive Neovim and back. This is the end-to-end companion to [architecture.md](architecture.md) §10 ("Commands: one canonical id → two faces"): read that section for the design rationale, then use this document to trace the code and to add a command.

## 1. Overview

A **Command** is a no-parameter, no-return user action — "New Window", "Close Pane", "Terminal". Commands are user-triggered only: unlike a layout intent there is no emit-on-attach, so a freshly attached core drains an empty queue.

One canonical `snake_case` id (`close_window`, `terminal`) is the single source of truth, and it derives two faces:

- **Face A, inside Neovim** — a PascalCase Ex-command `:WolfCloseWindow` (Neovim requires an uppercase-first user-command name) and an idiomatic Lua function `require('wolf').close_window()`.
- **Face B, the native front-end** — a `command_kind` discriminant (a `u8`) carried across the FFI and, on macOS, stored in an `NSMenuItem.tag`.

The `Command` enum's variant order is that discriminant contract, and it is append-only. A reorder silently remaps every menu tag and every drained command, so existing variants never move and a new command is appended last. The same integer means the same command in `command.rs`, in the FFI's `command_kind`, and in Swift's `CommandKind`; the table of values is in [`crates/wolf-ffi/src/README.md`](crates/wolf-ffi/src/README.md#discriminant-tables).

## 2. The registry — `crates/wolf-core/src/command.rs`

Everything starts here. The module is pure data and enums (no GUI toolkit, honoring wolf-core's one hard rule): `Command`, `CommandSpec`, the `COMMAND_SPECS` slice, `MenuGroup`, and the `Command::from_id` / `Command::spec` lookups. Their contracts are in the [`command.rs` entry](crates/wolf-core/src/README.md#commandrs).

Two properties matter to the rest of this trace. `COMMAND_SPECS` holds one row per command in variant order, so **the slice index equals the FFI discriminant**. And the id → `ex_command` mapping is stored as data rather than computed by a snake-to-Pascal transform, so a command's Ex-command name is chosen in exactly one place. One row:

```rust
CommandSpec {
    command: Command::Terminal,
    id: "terminal",
    ex_command: "WolfTerminal",
    title: "Terminal",
    menu: MenuGroup::View,
}
```

`MenuGroup` is grouping metadata; its own variant order is cosmetic and backs no discriminant. The `#[cfg(test)]` tests in `command.rs` pin the row count (`EXPECTED_COMMAND_COUNT`), pin each command's slice index, and round-trip each id, so reordering or dropping a variant fails the suite.

## 3. Face A — the Ex-command and the Lua function

`crates/wolf-core/src/nvim.rs::setup_commands` builds an array of `[id, ex_command]` pairs from `COMMAND_SPECS` and hands it, together with this RPC channel's id, to `_G.wolf_setup_commands(chan, commands)` in `runtime/nvim/init.lua` over `nvim_exec_lua`. Both arguments travel as Lua varargs, never interpolated into the code string. It is one step of `WolfCore::start`'s attach sequence.

For each pair the helper seeds `package.loaded["wolf"][id]` — the Lua face, and seeding `package.loaded` is what makes `require('wolf')` resolve under `--clean` — and creates the `:<ex_command>` user command, the Ex-command face. Both faces `rpcnotify` the same `wolf_command` channel with the canonical id. See [`runtime/nvim/README.md`](runtime/nvim/README.md#wolf_setup_commandschan-commands).

## 4. Face B — the native discriminant

The native front-end never learns the ids; it speaks only the discriminant. Three ports keep it aligned with the registry.

- `mod command_kind` in `crates/wolf-ffi/src/lib.rs` — the `u8` constants, in `wolf_core::Command` order ([the table](crates/wolf-ffi/src/README.md#discriminant-tables)). `FfiCommand::from_command` is an exhaustive match over `Command`, so adding or reordering a variant is a compile error here.
- `enum CommandKind` in `crates/wolf-macos/Sources/WolfKeyMap/CommandSeam.swift` — the AppKit-free Swift mirror. `init?(ffi:)` decodes the byte and returns `nil` for anything outside the contract, so an unrecognized discriminant is dropped rather than mis-dispatched.
- `AppMenu`'s private `Tag` enum in `crates/wolf-macos/Sources/WolfMacOS/AppMenu.swift` — the macOS menu is hardcoded rather than read from the registry over the FFI, and each tagged Wolf item carries its `command_kind` in `NSMenuItem.tag`. Item titles mirror `COMMAND_SPECS`, so a title change belongs in both places. See the [`AppMenu.swift` entry](crates/wolf-macos/Sources/WolfMacOS/README.md#appmenuswift).

`Tag` covers only the commands that have a tagged menu item, which is fewer than the registry holds. A command can carry a `MenuGroup` and still have no `Tag` value: the sidebar commands are the palette and Ex-command faces of View-menu items that AppKit and the `AppDelegate` already own, so they reach the dispatcher from the drained and palette paths alone.

## 5. Invocation → drain

Both faces converge on the same drain path.

**Face A (inside nvim).** A `:WolfXxx` Ex-command or a `require('wolf').xxx()` call fires the `wolf_command` notification. In `crates/wolf-core/src/redraw.rs`, `handle_notify` routes `COMMAND_EVENT` (`"wolf_command"`) through `Command::from_id` — an unknown id is dropped — and pushes the resulting `Command` onto the sibling `CommandQueue`, then wakes the front-end. That queue is a brief `Mutex`, never held across an `.await`, and a command bumps no `seq`.

`crates/wolf-core/src/api.rs::drain_commands` consumes the queue in arrival order, and the FFI's `WolfCoreHandle::drain_commands` maps each entry through `FfiCommand::from_command` into its discriminant. On macOS, `WolfWindowController.drainAndDispatchCommands` runs on each render wake, decodes every `FfiCommand.kind()` with `CommandKind(ffi:)`, and dispatches against its own window.

The layout helper's `:q` and `:qa` interception is a third, internal source of the same notification: on the last pane `do_quit` fires `wolf_command "close_window"`, and the `:qa` family (`all = true`) fires it regardless of pane count, closing the whole window. Both reuse the existing `close_window` registry command and add no command, no FFI constant, and no discriminant — see [`runtime/nvim/README.md`](runtime/nvim/README.md).

**Face B (the native menu).** `AppDelegate.handleCommandMenuItem` reads the clicked item's `NSMenuItem.tag`, decodes it with `CommandKind(ffi:)`, finds the key window's controller, and dispatches. There is no round trip through nvim — the discriminant was already in the tag.

## 6. Dispatch — `crates/wolf-macos/Sources/WolfMacOS/CommandDispatcher.swift`

`CommandDispatcher.dispatch(_:window:coordinator:originatingCwd:controller:)` is the single funnel every command flows through, so a command has one implementation regardless of face. Its switch carries no `default` case, so a new `CommandKind` fails to compile until it is handled here. See the [`CommandDispatcher.swift` entry](crates/wolf-macos/Sources/WolfMacOS/README.md#commanddispatcherswift).

Three entry points route here:

- the native menu (`AppDelegate.handleCommandMenuItem`, targeting `NSApp.keyWindow`);
- the nvim-drained path (`WolfWindowController.drainAndDispatchCommands`, passing `self` as the controller); and
- the command palette (`WolfWindowController.activatePaletteRow`, whose `.staticCommand` rows decode `row.commandKind` and dispatch the same way).

`.previewInBrowser` is worth calling out as a third shape alongside the flavors of §8. Like the `.preview` and `.edit` arms it invokes a blocking core method — `handle.open_in_browser()` returns the loopback preview server's URL for the pinned Markdown file — and the front-end then opens that URL with `NSWorkspace`. It touches the captive nvim but issues no `InputEvent`, so it is neither purely native nor an ordered-input editor-affecting command.

## 7. The editor-affecting path back into nvim

Many commands only touch the front-end (§8a). Others must change editor state, and those loop back into the captive Neovim through a fixed chain:

```
WolfCoreHandle::<method>            (crates/wolf-ffi/src/lib.rs)
  → WolfCore::<wrapper>             (crates/wolf-core/src/api.rs)
    → send_input(InputEvent::…)     (the ordered input channel)
      → input.rs::apply arm         (crates/wolf-core/src/input.rs)
        → nvim_command / nvim_exec_lua
```

The input channel is ordered, so an editor-affecting command never races pending keystrokes. The `apply` arm is either a plain Ex-command or a call into a `wolf_*` Lua helper, whose behavior is defined in [`runtime/nvim/README.md`](runtime/nvim/README.md) rather than in Rust. Four worked chains span the range:

- **`NewBuffer`** — `WolfCoreHandle::new_buffer` → `WolfCore::new_buffer` → `InputEvent::NewBuffer` → `nvim_exec_lua("wolf_new_untitled()", [])`.
- **`SwitchBuffer`** — `WolfCoreHandle::switch_buffer(bufnr)`, which resolves the active window live, → `WolfCore::switch_buffer(win, bufnr)` → `InputEvent::SwitchBuffer { win, bufnr }` → `nvim_exec_lua("wolf_switch_buffer(...)", [win, bufnr])`.
- **`Terminal`** — `WolfCoreHandle::toggle_terminal` → `WolfCore::toggle_terminal` → `InputEvent::ToggleTerminal` → `nvim_exec_lua("wolf_toggle_terminal()", [])`. The helper's open path also emits a `drawer_open` **layout intent**, drained by `WolfCore::drain_intents` — a channel separate from this command drain. Terminal is the illustration that one command can drive both channels and that the two are independent.
- **`FullWidth`** — `WolfCoreHandle::toggle_full_width` → `WolfCore::toggle_full_width` → `InputEvent::ToggleFullWidth` → `nvim_exec_lua("wolf_toggle_full_width()", [])`. Lifting the cap changes a pane's column count while its widget allocation holds still, so each front-end re-requests the grid size itself on the next wake: macOS through `paneContainer?.onRenderWake()`, GTK through `wolf_gnome::pane_view::PaneInner::sync_pane_grids`.

  The macOS View item is always enabled; only its checkmark tracks `is_full_width_active()`. GTK has no menu item and no chord, so it has no checkmark either: its surfaces are `:WolfFullWidth` and the command palette row, which appears automatically because the palette filters on `command_action::ui_action_for` — see [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md), "Present limitations".

## 8. Checklist: adding a new command

Three flavors. Pick one, do its steps, then the shared steps at the end.

### (a) Native-only (`MinimizeWindow`, the sidebar commands)

The action lives entirely in AppKit; nvim is never touched.

1. Add a `CommandDispatcher` arm mapping the command to its AppKit action.
2. No `InputEvent`, no `input.rs` arm, no Lua helper.

### (b) Editor-affecting (`NewBuffer`, `Terminal`)

Everything in (a), plus the round trip of §7:

1. Add a `WolfCoreHandle` method — the bridge `extern "Rust"` declaration and its `impl` — that calls `core.send_input(InputEvent::…)` in `crates/wolf-ffi/src/lib.rs`.
2. Add the public `WolfCore` wrapper in `api.rs`.
3. Add the `InputEvent` variant and its `apply` arm in `input.rs`: a plain `nvim_command`, or an `nvim_exec_lua` call into a new `wolf_*` helper in `runtime/nvim/init.lua` when the logic is non-trivial.
4. Point the dispatcher arm at the controller method that calls the handle.
5. **Regenerate the swift-bridge bindings** with `bazel run //crates/wolf-ffi:gen_bindings`, and commit `crates/wolf-ffi/generated/`.

### (c) Front-end overlay (`CommandPalette`, `BufferPicker`, `GoToFile`)

Everything in (a); the dispatcher arm opens a Swift overlay (`openCommandPalette()`, `openBufferPicker()`, `openFilePicker()`) instead of changing editor state, so there is no `InputEvent`. `CommandPalette` is excluded from the palette's own candidate list — a palette cannot dispatch itself — so it is the one command with no palette row.

### Shared steps (every flavor)

- **Append** the `Command` variant last, and never reorder: the discriminant contract is append-only.
- Add the `COMMAND_SPECS` row: id, ex_command, title, menu group.
- Bump `EXPECTED_COMMAND_COUNT` and add the index, round-trip, and spec-field tests in `command.rs`, mirroring the existing `search_in_project_*` tests, and turn the standing last-row guard into a fixed-index guard.
- Add the `command_kind` constant and extend `FfiCommand::from_command`.
- Extend the Swift `CommandKind` and its `init?(ffi:)`, plus `CommandSeamTests`.
- Add the `wolf_gnome::command_action::ui_action_for` arm. That match has no catch-all, so a new variant fails to compile in `wolf-gnome` until it is classified as a dispatched action, deferred, or without a GNOME equivalent. A dispatched command also becomes a row in the GTK command palette; a deferred one, or one with no GNOME equivalent, does not. To reach it from the primary menu or an accelerator, add its row in `crates/wolf-gnome/src/actions.rs` — see [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md), "The accelerator namespace", for the invariant that `parse_accel` enforces.
- Extend the contiguous-`0..=N` guard array in `crates/wolf-ffi/tests/commands.rs` and add a `:Wolf<Pascal>` drain step.
- If the command needs a tagged menu item, add an `AppMenu` `Tag` value and the item itself, with any accelerator. To enable it only in some states, add a case to `AppDelegate.validateMenuItem` backed by a `Snapshot` predicate bridged onto `WolfCoreHandle` — the `is_visual_mode` / `is_markdown_active` / `is_preview_active` precedent. A command with no such case, `Terminal` among them, is always enabled.
- Every command except `CommandPalette` becomes a palette candidate, so bump `STATIC_COMMAND_COUNT` in `crates/wolf-core/tests/palette_rank.rs`.
- Update the affected directory READMEs and this document, then run `prettier --write` on every edited Markdown file.
