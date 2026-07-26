# Command lifecycle

Where a Wolf **command** is configured, and how it travels from a keystroke or menu click all the way into the captive Neovim and back. This is the end-to-end companion to [architecture.md](architecture.md) §10 ("Commands: one canonical id → two faces"); read that section first for the design rationale, then use this document to trace the code and to add a new command.

## 1. Overview

A **Command** is a no-parameter, no-return user action — "New Window", "Close Pane", "Terminal". It is ONLY user-triggered: unlike a layout intent there is no emit-on-attach, so a freshly attached core has an empty command queue.

One canonical `snake_case` id (e.g. `close_window`, `terminal`) is the single source of truth, and it derives two faces:

- **Face A — inside Neovim**: a PascalCase Ex-command `:WolfCloseWindow` (Neovim forces the uppercase-first user-command name) and an idiomatic Lua function `require('wolf').close_window()`.
- **Face B — the native front-end**: a `command_kind` discriminant (a `u8`) carried across the FFI and, on macOS, stored in an `NSMenuItem.tag`.

The `Command` enum's VARIANT ORDER is that discriminant contract. It is **append-only**: existing variants never move (a reorder is a breaking change that would silently remap every menu tag and every drained command), and a new command is always appended LAST. The same integer means the same command in Rust (`command.rs`), in the FFI (`command_kind`), and in Swift (`CommandKind`).

## 2. The registry (source of truth) — `crates/wolf-core/src/command.rs`

Everything starts here. The module is pure data + enums (no GUI toolkit, honoring wolf-core's one hard rule):

- **`enum Command`** — one variant per command, in discriminant order (`NewWindow`(0), `CloseWindow`(1), … `NewTab`(17), `Preview`(18), `Edit`(19), `Terminal`(20), `PreviewInBrowser`(21), `FullWidth`(22)). The doc comment on the enum spells out the full ordered list and restates the append-only rule.
- **`struct CommandSpec { command, id, ex_command, title, menu }`** — the canonical metadata for one command, all fields public. The `id` → `ex_command` mapping is stored EXPLICITLY as data (there is no snake→Pascal transform code), so the registry is the one source of truth.
- **`const COMMAND_SPECS: &[CommandSpec]`** — one row per command, in variant order, so the **slice index equals the FFI discriminant**. Example row:

  ```rust
  CommandSpec {
      command: Command::Terminal,
      id: "terminal",
      ex_command: "WolfTerminal",
      title: "Terminal",
      menu: MenuGroup::View,
  }
  ```

- **`enum MenuGroup`** — `App` / `File` / `View` / `Window` / `Edit`, the menu-grouping metadata (its order is cosmetic, NOT a discriminant).
- **`Command::from_id(id) -> Option<Command>`** — a linear scan of `COMMAND_SPECS` resolving a `wolf_command` payload id back to its `Command` (unknown id → `None`).
- **`Command::spec(self) -> &'static CommandSpec`** — the reverse lookup.

The registry is guarded by `#[cfg(test)]` unit tests that pin the count (`EXPECTED_COMMAND_COUNT`), pin each command's slice index (== its discriminant), and round-trip each id. Reordering or dropping a variant breaks them.

## 3. Face A — the Neovim Ex-command + Lua function

`crates/wolf-core/src/nvim.rs::setup_commands` builds an array of `[id, ex_command]` string pairs from `COMMAND_SPECS` and hands it, together with this RPC channel's id, to the `_G.wolf_setup_commands(chan, commands)` helper in `runtime/nvim/init.lua` over `nvim_exec_lua` (both passed as Lua varargs, never interpolated into the code string). `WolfCore::start` calls it after the current-file / buffer / layout / dir / theme setups.

For each `{id, ex_command}` pair the helper (see [`runtime/nvim/README.md`](runtime/nvim/README.md)):

- seeds `package.loaded["wolf"][id] = function() vim.rpcnotify(chan, "wolf_command", id) end` — the **Lua face** `require('wolf').<id>()` (seeding `package.loaded` is required so `require('wolf')` resolves under `--clean`); and
- registers `:<ex_command>` via `nvim_create_user_command(ex_command, <same callback>, { nargs = 0, force = true })` — the **Ex-command face** `:Wolf<Pascal>`.

Both faces `rpcnotify` the SAME `wolf_command` channel with the canonical `id`. There is no autocmd and no emit-on-attach — commands are user-triggered only.

## 4. Face B — the native menu FFI discriminant

The native front-end never learns the ids; it speaks only the discriminant. Three ports keep it aligned with the registry:

- **`mod command_kind`** in `crates/wolf-ffi/src/lib.rs` — the `u8` constants (`NEW_WINDOW = 0` … `NEW_TAB = 17`, `PREVIEW = 18`, `EDIT = 19`, `TERMINAL = 20`, `PREVIEW_IN_BROWSER = 21`, `FULL_WIDTH = 22`), in `wolf_core::Command` order. `FfiCommand::from_command` is an EXHAUSTIVE match over `Command` that packs each variant to its `command_kind` (so adding or reordering a variant is a compile error here).
- **`enum CommandKind`** in `crates/wolf-macos/Sources/WolfKeyMap/CommandSeam.swift` — the AppKit-free Swift mirror. `init?(ffi: UInt8)` decodes the byte (`0` → `.newWindow` … `20` → `.terminal`, `21` → `.previewInBrowser`, `22` → `.fullWidth`, anything else → `nil`), guarding the contract.
- **`AppMenu`'s `Tag` enum** in `crates/wolf-macos/Sources/WolfMacOS/AppMenu.swift` — the macOS menu is HARDCODED (it does not read the registry over the FFI); each Wolf menu item carries its `command_kind` discriminant in `NSMenuItem.tag`. The menu titles mirror `COMMAND_SPECS`.

## 5. Invocation → drain

Both faces converge on the same drain path.

**Face A (inside nvim):** a `:WolfXxx` Ex-command or `require('wolf').xxx()` call fires the `wolf_command` notification. (The layout helper's `:q`-family interception is a THIRD, internal trigger of the same notification: on the LAST pane `do_quit` fires `wolf_command "close_window"` directly — reusing the existing `close_window` registry command with no new command, FFI, or discriminant — see [`runtime/nvim/README.md`](runtime/nvim/README.md).) In `crates/wolf-core/src/redraw.rs`, `handle_notify` routes `COMMAND_EVENT` (`"wolf_command"`) through `Command::from_id` (an unknown id is dropped) and pushes the resulting `Command` onto the sibling `CommandQueue` (a brief `Mutex`, never held across an `.await`, no `seq` bump), then wakes the front-end. `crates/wolf-core/src/api.rs::drain_commands` consumes that queue in arrival order; the FFI `WolfCoreHandle::drain_commands` maps each through `FfiCommand::from_command` into the `command_kind` discriminant. On macOS, `WolfWindowController.drainAndDispatchCommands` (called each render wake) decodes each `FfiCommand.kind()` via `CommandKind(ffi:)` and dispatches it against THIS window.

**Face B (native menu):** `AppDelegate.handleCommandMenuItem` reads the clicked item's `NSMenuItem.tag`, decodes it via `CommandKind(ffi:)`, finds the key window's controller, and dispatches. No round trip through nvim — the discriminant was already in the tag.

## 6. Dispatch — `crates/wolf-macos/Sources/WolfMacOS/CommandDispatcher.swift`

`CommandDispatcher.dispatch(_ kind:window:coordinator:originatingCwd:controller:)` is the single funnel every command flows through, so a command has ONE implementation regardless of face. It maps a decoded `CommandKind` to its native action, e.g. `.closePane → controller?.closePane()`, `.newBuffer → controller?.newBuffer()`, `.terminal → controller?.toggleTerminal()`, `.newWindow → coordinator?.makeNewWindow(...)`, `.previewInBrowser → controller?.openInBrowser()`, `.fullWidth → controller?.toggleFullWidth()`.

`.previewInBrowser` is a HYBRID flavor worth calling out: like the `.preview` / `.edit` arms (which reach the blocking `open_preview` / `render_preview` / `edit_source` core calls), its dispatcher arm invokes a BLOCKING core method — `handle.open_in_browser()` returns the loopback preview server's `http://127.0.0.1:<port>/<relpath>` URL (the pinned Markdown file's root-relative path) — and then the front-end opens that URL in the system default browser via `NSWorkspace`. It touches the captive nvim (a blocking RPC to resolve the pinned Markdown buffer + lazily start the per-project loopback HTTP server) but issues NO `InputEvent`, so it is neither the pure native-only flavor (a) nor the ordered-input editor-affecting flavor (b) — it is the same blocking-read-through-core shape as Preview.

All three entry points route here:

- the native-menu path (`AppDelegate.handleCommandMenuItem`, targeting `NSApp.keyWindow`);
- the nvim-drained path (`WolfWindowController.drainAndDispatchCommands`, passing `self` as the controller); and
- the command palette (`WolfWindowController.activatePaletteRow`, whose `.staticCommand` rows decode `row.commandKind` and dispatch the same way).

## 7. The editor-affecting path back into nvim

Many commands only touch the front-end (see §8a). Others must change editor state, and those loop BACK into the captive Neovim through a fixed chain:

```
WolfCoreHandle::<method>            (crates/wolf-ffi/src/lib.rs)
  → WolfCore::<wrapper>             (crates/wolf-core/src/api.rs)
    → send_input(InputEvent::…)     (the ordered input channel)
      → input.rs::apply arm         (crates/wolf-core/src/input.rs)
        → nvim_command / nvim_exec_lua
```

The input channel is ORDERED, so an editor-affecting command never races pending keystrokes. The `apply` arm is either a simple Ex-command or a call into a `wolf_*` Lua helper. Four worked examples span the range of complexity:

- **`NewBuffer` (simple):** `WolfCoreHandle::new_buffer` → `WolfCore::new_buffer` → `InputEvent::NewBuffer` → `apply` runs `command(nvim, "enew")` — a plain `:enew`.
- **`SwitchBuffer` (helper):** `WolfCoreHandle::switch_buffer(bufnr)` (resolving the active window live) → `WolfCore::switch_buffer(win, bufnr)` → `InputEvent::SwitchBuffer { win, bufnr }` → `apply` calls `nvim_exec_lua("wolf_switch_buffer(...)", [win, bufnr])`, whose `init.lua` helper does a window-scoped `nvim_win_set_buf` plus per-pane view restore atomically.
- **`Terminal` (helper, no args):** `WolfCoreHandle::toggle_terminal` → `WolfCore::toggle_terminal` → `InputEvent::ToggleTerminal` → `apply` calls `nvim_exec_lua("wolf_toggle_terminal()", [])`. The `wolf_toggle_terminal()` helper in `init.lua` finds the shared, tagged (`b:wolf_terminal` + `buftype == 'terminal'`) per-project terminal buffer, hides its per-tabpage drawer window if visible (the buffer/job persist), or opens a bottom `:terminal` split showing it (recreating the buffer if the user exited the shell). On the OPEN path it ALSO emits a `drawer_open` **LAYOUT** intent (`_G.wolf_emit_drawer(win)` → `wolf_layout { action = "drawer_open", win }` → `LayoutIntent::DrawerOpen`, drained via `WolfCore::drain_intents` — distinct from this command-drain path) so the front-end renders the drawer as a root-bottom pane; hiding stays intent-less (the front-end observes the `win_close`). Terminal is thus a good illustration that an editor-affecting command can ALSO drive the layout-intent channel — the two channels are independent. See [`runtime/nvim/README.md`](runtime/nvim/README.md).
- **`FullWidth` (helper, no args, source-buffer redirect):** `WolfCoreHandle::toggle_full_width` → `WolfCore::toggle_full_width` → `InputEvent::ToggleFullWidth` → `apply` calls `nvim_exec_lua("wolf_toggle_full_width()", [])`. The `wolf_toggle_full_width()` helper in `init.lua` resolves the EFFECTIVE buffer — the current buffer, UNLESS the active pane is a preview scratch (it carries `b:wolf_preview_source_bufnr`), in which case it targets the SOURCE buffer (OQ-1) so the visible preview resizes and the document's editor + preview views stay consistent — then flips that buffer's `b:wolf_full_width`, and re-emits the three state channels the toggle affects: `wolf_emit_current_file()` (the menu checkmark), `wolf_emit_full_width()` (the per-window editor-pane geometry), and `wolf_emit_previews()` (the preview-pane framing). The macOS dispatcher arm (`.fullWidth → controller?.toggleFullWidth()`) also calls `paneContainer?.onRenderWake()` so the panes resize promptly on the next wake. The View-menu item is ALWAYS enabled (OQ-2); only its checkmark tracks `is_full_width_active()`. See [`architecture.md`](architecture.md) §10 and [`full-width-decisions.md`](full-width-decisions.md).

## 8. Checklist: adding a new command

There are three flavors. Pick one, then do its steps PLUS the shared steps at the end.

### (a) Native-only (e.g. `MinimizeWindow`)

The action lives entirely in AppKit; nvim is never touched.

1. Append a `Command` variant + a `COMMAND_SPECS` row (`command.rs`).
2. Add its `command_kind` constant + the `FfiCommand::from_command` arm (`wolf-ffi/src/lib.rs`).
3. Add its `CommandKind` case + the `init?(ffi:)` arm (`CommandSeam.swift`).
4. Add a `CommandDispatcher` arm mapping it to the AppKit action.
5. No `InputEvent`, no `input.rs` arm, no Lua helper.

### (b) Editor-affecting (e.g. `NewBuffer`, `Terminal`)

Everything in (a), PLUS the round trip of §7:

1. Add a `WolfCoreHandle` method (bridge `extern "Rust"` declaration + `impl`) that calls `core.send_input(InputEvent::…)` (`wolf-ffi/src/lib.rs`).
2. Add the public `WolfCore` wrapper (`api.rs`).
3. Add the `InputEvent` variant + its `apply` arm (`input.rs`) — a simple `nvim_command`, or an `nvim_exec_lua` call into a new `wolf_*` helper in `runtime/nvim/init.lua` when the logic is non-trivial.
4. Wire the dispatcher arm to the controller method that calls the handle.
5. **Regenerate the swift-bridge bindings** — `bazel run //crates/wolf-ffi:gen_bindings` — and commit `crates/wolf-ffi/generated/`.

### (c) Front-end overlay (e.g. `CommandPalette`, `BufferPicker`, `GoToFile`)

Everything in (a); the `CommandDispatcher` arm opens a Swift overlay (`openCommandPalette()`, `openBufferPicker()`, `openFilePicker()`) rather than changing editor state, so there is no `InputEvent`. Note that `CommandPalette` is EXCLUDED from the palette's own candidate list (a palette cannot dispatch itself), so it does not appear as a palette row.

### Shared steps (every flavor)

- **Append** the `Command` variant LAST — never reorder (the discriminant contract is append-only).
- Add the `COMMAND_SPECS` row (id / ex_command / title / menu group).
- Bump `EXPECTED_COMMAND_COUNT` and add the index / round-trip / spec-fields tests in `command.rs` (mirror the existing `preview_in_browser_*` tests; demote the previous "last row" guard to a fixed-index guard).
- Add the `command_kind` constant and extend `FfiCommand::from_command`.
- Extend the Swift `CommandKind` `init?(ffi:)` and its `CommandSeamTests`.
- Extend the `crates/wolf-ffi/tests/commands.rs` contract-guard array (the contiguous `0..=N` assertion) and add a `:Wolf<Pascal>` drain step.
- If the command has a menu group, add an `AppMenu` `Tag` value + the menu item (with any accelerator). If it should only be enabled in some states, add a `Tag` case to `AppDelegate.validateMenuItem` backed by a `Snapshot` predicate bridged onto `WolfCoreHandle` (the `is_visual_mode` / `is_markdown_active` / `is_preview_active` precedent); commands with no such case — `Terminal`, for one — are always enabled.
- If it becomes a command-palette candidate (any command except `CommandPalette`), update the `STATIC_COMMAND_COUNT` constant in `crates/wolf-core/tests/palette_rank.rs`.
- Update the affected directory READMEs and this document, then run `prettier --write` on every edited Markdown file.
