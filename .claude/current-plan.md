# Plan: "Commands" architecture — first cut

## Status: COMPLETE

Docs verified/updated by technical-writer (macOS README got the `originatingCwd` late-fix + command
eyeball checklist; architecture.md got the wolf-macos menu note; wolf-ffi README got the "commands ride
the existing render wake, `wolf_ffi_wake.h` unchanged" note; wolf-core/src + runtime/nvim READMEs already
accurate). `prettier --write` run on the three edited docs — all unchanged (already conformant).

OUT-OF-SCOPE side effect to surface: `.claude/agents/architect.md` was self-edited by the architect agent
to remove stale tarjeta/Go conventions. Left staged for the user to accept or revert — NOT part of the
Commands feature.

## Status: TECHNICAL-WRITER (final docs pass)

## Reviewer verdict: APPROVED

All 7 focus areas conform to the plan; no discriminant drift, id/ex_command typos, multi-window
targeting bugs, missing `force=true`, or seq-bump mistakes. Two non-blocking notes were raised and
FIXED (coder):
1. `:WolfNewWindow` now inherits the draining window's live cwd (matches Cmd-N). `CommandDispatcher.dispatch`
   gained an `originatingCwd: String` param; drained path passes `handle?.cwd()`, menu path passes the
   key window's controller cwd (empty → home-dir fallback). Only `.newWindow` behavior changed.
2. `close_window` registry `MenuGroup` corrected `Window` → `File` (matches the shipped menu + macOS
   Cmd-W convention). Pinned by a new pure test `command_specs_menu_groups_match_macos_convention`.

Declined (documented AppKit coverage boundary): a spy-seam Swift test over `makeNewWindow`.

Final test state — ALL PASS: `//crates/wolf-core:commands` (6 tests incl. real-nvim 5-variant drain +
menu-group pin), `:wolf_core_lib_test`, `//crates/wolf-ffi:commands`, `:state`, `:panes`,
`:bridge_smoke`, `//crates/wolf-macos:WolfKeyMapTests`; `bazel build //crates/wolf-macos:Wolf` succeeds;
`gen_bindings` regenerated (`wolf_ffi_wake.h` untouched).

---

## Status history: REVIEWER

## Coder report (all tests PASS)

Implemented per plan. Files: `command.rs` (new registry, PUBLIC `CommandSpec` fields), `lib.rs`
re-export, `redraw.rs` (`CommandQueue`/`wolf_command` arm), `api.rs` (field + `drain_commands` +
`setup_commands` after theme tracking), `nvim.rs` (`setup_commands` passing `COMMAND_SPECS` as
`[id, ex_command]` pairs), `init.lua` (`_G.wolf_setup_commands` seeding `package.loaded['wolf']`,
`nargs=0, force=true`), `wolf-ffi/src/lib.rs` (`command_kind` module, opaque `FfiCommand::kind()`,
`drain_commands`, exhaustive `from_command`), regenerated `generated/wolf-ffi.{h,swift}`
(`wolf_ffi_wake.h` UNCHANGED), 3 BUILD targets wired, Swift `CommandSeam`/`CommandDispatcher`/`AppMenu`
+ `AppDelegate` menu install + `handleCommandMenuItem` + `WolfWindowController.drainAndDispatchCommands`.
Docs updated (architecture.md decision 10 + Output: commands, wolf-core README + src, wolf-ffi README,
wolf-macos README, runtime/nvim README), prettier-clean.

Test results:
- `//crates/wolf-core:wolf_core_lib_test` PASS
- `//crates/wolf-core:commands` PASS (real-nvim: 5 variants now drain end-to-end — CloseWindow+
  HideApplication via `:Wolf*`, QuitApplication+NewWindow via `require('wolf').*`; MinimizeWindow via
  the ffi test; empty-at-attach; + 4 pure registry tests)
- `//crates/wolf-ffi:commands` PASS (close=1, minimize=4 via `FfiCommand::kind()`)
- `//crates/wolf-ffi:state`, `:panes`, `:bridge_smoke` PASS (regression — bridge change safe)
- `//crates/wolf-macos:WolfKeyMapTests` PASS (incl. `CommandSeamTests` 0..4→cases, 5/255→nil)
- `bazel build //crates/wolf-macos:Wolf` SUCCEEDS (AppKit menu/dispatcher compile-verified)
- `bazel run //crates/wolf-ffi:gen_bindings` ran; committed bindings updated
- `cargo test -p wolf-core` NOT run — cargo not installed on this host; Bazel targets cover the same.

Coverage follow-up (test-writer 2nd pass): extended the core test so all 5 variants drain live. Done.
AppKit command glue (AppMenu/CommandDispatcher/handleCommandMenuItem/drainAndDispatchCommands) is
compile-verified + manual-smoke only, per the existing AppKit coverage boundary; pure `CommandSeam`
decode is headlessly tested.

---

## Architect's plan

## Test-writer report

Created 3 test files (no BUILD edits — coder wires targets):

1. `crates/wolf-core/tests/commands.rs` — modeled on real `tests/layout_intents.rs`. Real-nvim
   `ex_and_lua_faces_emit_commands` (ports `poll_until`/`wait_for_initial_window`/`ex_command` +
   local `drain_until_command`; uses real common API `start_core_with_temp_md`, `FlushWaiter`,
   `FLUSH_TIMEOUT`, `ch`, `key`, `nvim_bin_or_skip`/`init_lua_path`). Asserts drain empty after attach;
   `:WolfCloseWindow<CR>`→`Command::CloseWindow`; `:lua require('wolf').quit_application()<CR>`→
   `Command::QuitApplication`. Plus 4 PURE `#[test]` fns (no core): id round-trip, unknown→None, no dup
   id/ex_command, ex_command starts "Wolf".
2. `crates/wolf-ffi/tests/commands.rs` — modeled on `tests/panes.rs` (`char_key`/`enter_key`/
   `send_ex_command`, `nvim_bin_or_skip()` SKIP guard). Local `CMD_KIND_*` consts (0..4) contract guard;
   `:WolfCloseWindow`→`kind()==1`; `:WolfMinimizeWindow`→`kind()==4`; `shutdown()`.
3. `crates/wolf-macos/Tests/WolfKeyMapTests/CommandSeamTests.swift` — modeled on `PaneTreeTests`.
   `CommandKind(ffi:0..4)`→5 cases; `5`/`255`→nil.

### Test-writer assumptions the CODER MUST honor or adapt

- wolf-core `common` has NO skip — it PANICS via `require_nvim_bin` when nvim absent (every existing
  wolf-core integration test does this). The core test follows that (panic-if-no-nvim); only the wolf-ffi
  test truly SKIPs. Real reusable common API used: `start_core_with_temp_md`, `FlushWaiter`, `FLUSH_TIMEOUT`,
  `ch`, `key`, `DEFAULT_COLS/DEFAULT_ROWS`, `nvim_bin_or_skip`, `init_lua_path`. (NOTE: report mentions both
  `require_nvim_bin` and `nvim_bin_or_skip` — coder verify which the core harness actually exposes.)
- Pure tests access `spec.command` / `spec.id` / `spec.ex_command` as PUBLIC FIELDS. Coder: make
  `CommandSpec` fields `pub` (or adapt the 4 pure tests to accessors — one site each).
- `Command` derives `Debug + Copy + PartialEq + Eq` (used by `.contains(&Command::…)` + `{:?}`).
- `wolf_ffi_start` call copies panes.rs exactly incl. the trailing `0` (window_index) arg.
- XCTest SourceKit "No such module" diagnostic is expected (Swift tests build under Bazel only).

---

## Architect's plan

## Problem statement

Introduce a **Commands** architecture to Wolf and implement the first cut. A Command is a
no-parameter, no-return action a user can trigger. It can be invoked from inside Neovim (an
Ex-command or a Lua function), from the native menu, and — in future work, NOT built here — a
command picker.

### Locked design decisions

1. **Naming.** One canonical command id (snake_case, e.g. `close_window`) derives both faces: a
   PascalCase Ex-command `:WolfCloseWindow` (Neovim forces the uppercase start) and a snake_case Lua
   function `require('wolf').close_window()` (idiomatic nvim Lua).
2. **User keybindings — DEFERRED.** The captive Neovim stays hermetic (`--clean`, never reads
   `~/.config/nvim`). Expose the `wolf` Lua module + `:Wolf*` Ex-commands now for Wolf's own bundled
   config/plugins only. A real user keybinding/config hook is a separate later change. Do NOT add a
   user-config source path.
3. **Scope for this change.** Spec into the docs, then implement: (a) core command registry + drain
   channel, (b) FFI mirror, (c) init.lua registration, (d) a first set of STATIC app/window
   commands — New Window, Close Window, Quit Application, Hide Application, Minimize Window — wired to
   a NEW macOS NSMenu app menu bar. DEFER (do not build): the command picker, and dynamic "Open
   recent folder" commands.

### Architectural placement

Commands generalize the existing layout-intent channel; mirror the `wolf_layout` → `drain_intents`
pattern end to end.

- **wolf-core**: canonical platform-agnostic command registry (id, title, ex-command name
  `Wolf<Pascal>`, menu-group metadata; static-only for now). A `wolf_command` rpcnotify handler
  folding into a drainable `Vec<Command>` queue, exposed as `WolfCore::drain_commands()`. Core
  registers the commands in Neovim over RPC after theme tracking, via a new
  `_G.wolf_setup_commands(chan)` in `runtime/nvim/init.lua` modeled on `_G.wolf_setup_layout`. Keep
  the one-hard-rule (no GUI toolkit in core).
- **wolf-ffi**: `WolfCoreHandle::drain_commands() -> Vec<FfiCommand>`, opaque `FfiCommand` with a
  `kind() -> u8` accessor, and a `command_kind` discriminant module as the single source of truth
  (same pattern as `intent_kind` / `focus_dir`). Regenerate committed bindings.
- **wolf-macos**: an NSMenu app menu bar whose items map to command ids; a dispatcher (switch on
  command id) executing native actions — New Window → `AppDelegate.makeNewWindow`, Close Window →
  performClose, Quit → `NSApp.terminate`, Hide → `NSApp.hide`, Minimize → `window.miniaturize`. Drain
  commands on the render wake and dispatch the same way native menu items do. A pure AppKit-free Swift
  seam for command-kind decoding (headlessly testable in WolfKeyMap).
- **wolf-gnome**: NOT wired to a menu here, but `drain_commands()` must stay front-end-agnostic so
  gnome can adopt it later (deferred parity).

### Testing (prefer real-nvim integration tests over mocks)

- wolf-core: real-nvim test — send `:WolfCloseWindow<CR>` (and/or call the Lua fn), assert a matching
  Command drains from `drain_commands()`; assert the `wolf` module + `:Wolf*` commands exist.
- wolf-ffi: a `:commands` integration test modeled on `tests/panes.rs`, exercising `drain_commands()`
  / `FfiCommand::kind()` and guarding the `command_kind` discriminant contract.
- wolf-macos: a headless WolfKeyMap test for the command-kind decoding seam.

### Documentation (source of truth — update as part of the change)

architecture.md (Commands section + core↔front-end contract note + consider a numbered decision),
crates/wolf-core/README.md + src/README.md, crates/wolf-ffi/README.md, crates/wolf-macos/README.md,
runtime/nvim/README.md. Reformat edited Markdown with `prettier --write`; no hard line breaks inside
paragraphs.

---

## Architect's plan

### 1. Summary

Introduce a **Commands** channel that generalizes the existing `wolf_layout` layout-intent channel
end-to-end. A Command is a no-parameter, no-return user action. `wolf-core` gains a canonical, GUI-free
command registry (a Rust enum + a static metadata table) plus a drainable `Vec<Command>` queue fed by a
new `wolf_command` rpcnotify handler; the captive `init.lua` gains `_G.wolf_setup_commands(chan, commands)`
which — from a list Rust passes in — registers one `:Wolf<Pascal>` Ex-command and one
`require('wolf').<snake>()` Lua function per command, each `rpcnotify`ing `wolf_command '<id>'`. `wolf-ffi`
mirrors this with an opaque `FfiCommand { kind() -> u8 }`, a `command_kind` discriminant module, and
`WolfCoreHandle::drain_commands()`. `wolf-macos` gains a from-scratch `NSMenu` app menu bar and a single
`CommandDispatcher` that both the menu items and the nvim-drained commands funnel through, wired to a new
`CommandKind` decode seam in `WolfKeyMap`. First STATIC command set: New Window, Close Window, Quit
Application, Hide Application, Minimize Window. Picker + dynamic "open recent folder" deferred. `wolf-gnome`
untouched (consumes `wolf-core` directly, gets `drain_commands()` for free; no GTK menu work now).

### 2. Files to create

**`crates/wolf-core/src/command.rs`** — canonical registry, modeled on `src/layout.rs`:

- `pub enum Command` (variant order = the discriminant contract shared with wolf-ffi + Swift):
  `NewWindow`(0), `CloseWindow`(1), `QuitApplication`(2), `HideApplication`(3), `MinimizeWindow`(4). Derive
  `Debug, Clone, Copy, PartialEq, Eq`.
- `pub enum MenuGroup` — `App`, `File`, `Window`. Derive same.
- `pub struct CommandSpec { command: Command, id: &'static str, ex_command: &'static str, title: &'static str, menu: MenuGroup }`.
- `pub const COMMAND_SPECS: &[CommandSpec]` — one static table in variant order; the `id`→`ex_command`
  mapping stored EXPLICITLY as data (no snake→Pascal transform code — least-clever, single source of truth).
  Rows: `{NewWindow,"new_window","WolfNewWindow","New Window",File}`,
  `{CloseWindow,"close_window","WolfCloseWindow","Close Window",Window}`,
  `{QuitApplication,"quit_application","WolfQuitApplication","Quit Wolf",App}`,
  `{HideApplication,"hide_application","WolfHideApplication","Hide Wolf",App}`,
  `{MinimizeWindow,"minimize_window","WolfMinimizeWindow","Minimize",Window}`.
- `impl Command`: `pub fn from_id(id: &str) -> Option<Command>` (linear scan), `pub fn spec(self) -> &'static CommandSpec`.
- `#[cfg(test)] mod tests` — id round-trips; unknown→None; specs in variant order; no dup id/ex_command;
  every ex_command starts `"Wolf"` + valid Ex-command name. Runs under existing `wolf_core_lib_test`.

**`crates/wolf-core/tests/commands.rs`** — real-nvim integration, modeled on `tests/layout_intents.rs`
(reuse `tests/common/mod.rs`). See §4.

**`crates/wolf-ffi/tests/commands.rs`** — real-nvim integration, modeled on `tests/panes.rs`. See §4.

**`crates/wolf-macos/Sources/WolfKeyMap/CommandSeam.swift`** — pure Foundation-only seam (sibling of
`Orientation`/`FocusDir`/`ShortcutSeam`): `public enum CommandKind { newWindow, closeWindow,
quitApplication, hideApplication, minimizeWindow }` (FFI order) + `public init?(ffi: UInt8)` guarded decode
(0..4 → cases, else nil). No AppKit import.

**`crates/wolf-macos/Sources/WolfMacOS/AppMenu.swift`** — `AppMenu.install(target:)` builds `NSApp.mainMenu`
once from wolf-macos's OWN hardcoded 5-item description (does NOT read core registry over FFI yet). Standard
3 menus: App (About / Hide `h` / Quit `q`), File (New Window `n` / Close Window `w`), Window (Minimize `m`).
Each Wolf item carries the FFI `u8` in `NSMenuItem.tag`, target = AppDelegate, action =
`#selector(AppDelegate.handleCommandMenuItem(_:))`.

**`crates/wolf-macos/Sources/WolfMacOS/CommandDispatcher.swift`** — the single dispatcher:
`static func dispatch(_ kind: CommandKind, window: NSWindow?, coordinator: AppDelegate?)`: `.newWindow`→
`coordinator?.makeNewWindow(originatingCwd:)`, `.closeWindow`→`window?.performClose(nil)`, `.quitApplication`→
`NSApp.terminate(nil)`, `.hideApplication`→`NSApp.hide(nil)`, `.minimizeWindow`→`window?.miniaturize(nil)`.

**`crates/wolf-macos/Tests/WolfKeyMapTests/CommandSeamTests.swift`** — headless decode tests. See §4.

### 3. Files to modify

**`crates/wolf-core/src/lib.rs`** — `pub mod command;` + re-export `Command, CommandSpec, MenuGroup, COMMAND_SPECS`; module doc line.

**`crates/wolf-core/src/redraw.rs`** — mirror `LayoutIntent`/`IntentQueue`: `pub type CommandQueue = Arc<Mutex<VecDeque<Command>>>;`;
`const COMMAND_EVENT: &str = "wolf_command";`; add `commands: CommandQueue` field to `RedrawHandler` + `new(...)`
param (after `intents`); a `COMMAND_EVENT` arm in `handle_notify` reading `args.first().as_str()` → `Command::from_id`
→ push → `(self.wake)()`, no `seq` bump.

**`crates/wolf-core/src/api.rs`** — `commands: CommandQueue` field on `WolfCore`; build + pass into `RedrawHandler::new`;
add `proc.setup_commands().await?;` AFTER `setup_theme_tracking`; `pub fn drain_commands(&self) -> Vec<Command>`
(`#[must_use]`, copy of `drain_intents`).

**`crates/wolf-core/src/nvim.rs`** — `pub async fn setup_commands(&self)` modeled on `setup_theme_tracking`: look up
`chan`; build args from `COMMAND_SPECS` as a `Value::Array` of `[id, ex_command]` pairs; `nvim_exec_lua("wolf_setup_commands(...)", [chan, commands_array])`
(varargs, never interpolated).

**`runtime/nvim/init.lua`** — `_G.wolf_setup_commands(chan, commands)` after `_G.wolf_setup_layout`: seed
`package.loaded['wolf'] = wolf_tbl` (REQUIRED so `require('wolf')` works under `--clean`; a global would NOT); for each
`{id, ex_command}`: `wolf_tbl[id] = function() vim.rpcnotify(chan,'wolf_command',id) end` AND
`vim.api.nvim_create_user_command(ex_command, function() vim.rpcnotify(chan,'wolf_command',id) end, {nargs=0, force=true})`.
No autocmd (user-triggered).

**`crates/wolf-ffi/src/lib.rs`** — add `Command` import; `mod command_kind` (NEW_WINDOW=0..MINIMIZE_WINDOW=4, single
source of truth); in `mod ffi`: `WolfCoreHandle::drain_commands(&self) -> Vec<FfiCommand>` + `extern "Rust" { type FfiCommand;
fn kind(&self) -> u8; }`; hand-glue `pub struct FfiCommand { kind: u8 }` + `from_command(cmd)` (exhaustive match →
discriminant), `impl WolfCoreHandle { drain_commands }`. **Run `bazel run //crates/wolf-ffi:gen_bindings`** after (commit
`generated/`). `wolf_ffi_wake.h` NOT touched (commands ride the existing editor render wake). No `CARGO_BAZEL_REPIN`.

**`crates/wolf-ffi/BUILD.bazel`** — add `rust_test` `commands` (copy of `panes`: srcs common+commands.rs, no-sandbox, stage nvim+init.lua).

**`crates/wolf-core/BUILD.bazel`** — add `rust_test` `commands` (copy of `theme`).

**`crates/wolf-macos/BUILD.bazel`** — `wolf_key_map` srcs += `CommandSeam.swift`; `WolfKeyMapTests` srcs += `CommandSeamTests.swift`.
(`wolf_macos_lib` globs `Sources/WolfMacOS/*.swift` — AppMenu/CommandDispatcher auto-picked-up.)

**`crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift`** — in `applicationDidFinishLaunching` after wake bridges install:
`AppMenu.install(target: self)` (net-new; there is NO menu bar today). Add `@objc func handleCommandMenuItem(_ sender:
NSMenuItem)`: decode `CommandKind(ffi: UInt8(sender.tag))` → `CommandDispatcher.dispatch(kind, window: NSApp.keyWindow,
coordinator: self)`. `makeNewWindow` reused by `.newWindow`.

**`crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift`** — in `onRenderWake()` after tree, add
`drainAndDispatchCommands()`: `for cmd in handle?.drain_commands() ?? [] { guard let kind = CommandKind(ffi: cmd.kind())
else { continue }; CommandDispatcher.dispatch(kind, window: window, coordinator: coordinator) }` — passes THIS controller's
`window`/`coordinator` so a command from window A acts on window A. (No `PaneContainerView.swift` change.)

### 4. Test plan

- **wolf-core unit (`command.rs`):** `from_id` round-trips all specs; unknown→None; `COMMAND_SPECS` order == variant order;
  no dup id/ex_command; every ex_command starts `"Wolf"` + uppercase-first alnum.
- **`crates/wolf-core/tests/commands.rs` (real nvim, mirrors `layout_intents.rs`):** add `drain_until_command` helper.
  Ex-command path: type `:WolfCloseWindow<CR>` → assert `Command::CloseWindow` drains within `FLUSH_TIMEOUT`. Lua fn path:
  `:lua require('wolf').quit_application()<CR>` → assert `Command::QuitApplication` (proves `package.loaded` seeding under
  `--clean`). "No spurious commands at startup": after initial window, `drain_commands()` empty. Drive via typed Ex input
  (no arbitrary-eval API), exactly like `layout_intents.rs` types `:vsplit`.
- **`crates/wolf-ffi/tests/commands.rs` (real nvim, mirrors `panes.rs`):** poll `read_frame()` until attached; local
  discriminant consts mirroring `command_kind` (contract guard, like `panes.rs` re-declares `INTENT_KIND_SPLIT`); type
  `:WolfCloseWindow<CR>` → poll `drain_commands()` until an `FfiCommand` with `kind()==1` appears. **Do NOT trigger a command
  that actually quits/closes** — these are pure FFI drains here (no front-end acting), nothing destroyed. Optionally also
  `:WolfMinimizeWindow<CR>` → `kind()==4`. Then `shutdown()`.
- **`CommandSeamTests.swift` (headless):** `CommandKind(ffi:0..4)` → 5 cases in order; `5`/`255` → nil. Mirrors
  `PaneTreeTests`' `Orientation(ffi:)` coverage.
- **Not tested (manual/compile-verified, per existing AppKit coverage boundary):** `AppMenu`, `CommandDispatcher`,
  `handleCommandMenuItem`, `drainAndDispatchCommands`. The pure `CommandSeam` decode IS headlessly tested.

### 5. Implementation order (no broken intermediate compiles)

1. `command.rs` + `lib.rs` register (compiles standalone; unit tests pass).
2. `redraw.rs` (queue/event/handler field+param+arm) → `api.rs` (field, start wiring, `drain_commands`, `setup_commands`
   call) → `nvim.rs` (`setup_commands`) together (the `RedrawHandler::new` signature change + its only caller).
3. `runtime/nvim/init.lua` (`_G.wolf_setup_commands`).
4. `crates/wolf-core/tests/commands.rs` + `wolf-core/BUILD.bazel`.
5. `crates/wolf-ffi/src/lib.rs` → `bazel run //crates/wolf-ffi:gen_bindings` (refresh `generated/`).
6. `crates/wolf-ffi/tests/commands.rs` + `wolf-ffi/BUILD.bazel`.
7. `CommandSeam.swift` + wolf_key_map srcs; `CommandSeamTests.swift` + WolfKeyMapTests srcs.
8. `CommandDispatcher.swift`, `AppMenu.swift`, then `AppDelegate.swift` + `WolfWindowController.swift` (depend on regen +
   `CommandKind`).
9. Docs (§7).

### 6. Dependencies

No new external packages. No `Cargo.toml` change → **no `CARGO_BAZEL_REPIN`**. `wolf-core` GUI-denylist honored (`command.rs`
pure data + enums). **`bazel run //crates/wolf-ffi:gen_bindings` IS required** (committed bindings) — commit the diff.

### 7. Docs to update (reformat each with `prettier --write`, no hard line breaks in paragraphs)

- `architecture.md`: add "Output: commands (core → front-end)" under the core↔front-end contract (parallel to intents):
  `wolf_command` → `drain_commands` fold, the canonical-id/two-faces rule, registry canonical in core while the macOS menu
  hardcodes its item list for this cut (only the `command_kind` discriminant crosses the FFI now; exposing the full registry
  for a picker is deferred). One-line notes in the wolf-core/wolf-ffi/wolf-macos component paragraphs. Consider a numbered
  decision (canonical id → two faces; user keybindings deferred; captive nvim hermetic). Note gnome parity deferred.
- `crates/wolf-core/README.md` + `crates/wolf-core/src/README.md`: the `command` module, `wolf_command` handler,
  `CommandQueue`, `WolfCore::drain_commands`, `setup_commands` (attach after `setup_theme_tracking`).
- `crates/wolf-ffi/README.md`: `FfiCommand` (`kind()`), the `command_kind` table (single source of truth),
  `WolfCoreHandle::drain_commands`; the `gen_bindings` regen.
- `crates/wolf-macos/README.md`: the NSMenu app menu bar, `CommandDispatcher`, `CommandSeam`, both faces funnel through the
  one dispatcher (menu → key window; nvim-drained → originating controller's window).
- `runtime/nvim/README.md`: `_G.wolf_setup_commands(chan, commands)` — the `wolf` module (via `package.loaded`), the
  `:Wolf<Pascal>` user commands (`nargs=0, force=true`), the Rust-passed list shape.

### 8. Key decisions & risks (stated)

- **Route EVERYTHING through the one dispatcher.** All 5 menu items target `handleCommandMenuItem(_:)` (FFI `u8` in `.tag`) →
  `CommandSeam` decode → `CommandDispatcher.dispatch`; nvim-drained path calls the same `dispatch`. Trade-off vs native
  first-responder selectors: loss of automatic menu enable/disable validation — negligible for this static set, revisit with the
  picker. Multi-window targeting: menu → `NSApp.keyWindow`; nvim-drained → draining controller's own `window`.
- **Registry ownership:** core owns the canonical list; front-end maps id→action. The macOS menu HARDCODES its 5 items this cut;
  only the `command_kind` u8 crosses the FFI. Exposing the registry (titles/groups) for a picker is deferred; `COMMAND_SPECS`
  is structured so that addition is additive.
- **Discriminant as a Rust enum:** the `Command` enum's variant order is the natural `u8`; `FfiCommand::from_command`'s
  exhaustive match makes drift a compile error.
- **`require('wolf')` under `--clean` (RISK — addressed):** `package.loaded['wolf'] = wolf_tbl` inside `exec_lua`; `require`
  consults `package.loaded` first. A plain global would NOT work. The core test exercises `require('wolf').<fn>()` to prove it.
- **`nvim_create_user_command` opts (confirmed):** `{nargs=0, force=true}`. Callback closes over the id; nothing interpolated.
- **Attach ordering (confirmed):** `setup_commands` last, after `setup_theme_tracking`.
- **Clippy/must_use:** doc comments + `#[must_use]` on every new public getter/value. Bridge fns added both inside `mod ffi`
  and as hand-written impls held to the normal bar. `command_kind` mirrors `intent_kind` const-module style.
- **wolf-gnome:** unchanged; gets `drain_commands` for free; can't break (not via wolf-ffi). Deferred parity documented.

### 9. Verification commands

- `cargo test -p wolf-core`
- `bazel run //crates/wolf-ffi:gen_bindings` (regenerate + commit)
- `bazel test //crates/wolf-core:wolf_core_lib_test //crates/wolf-core:commands`
- `bazel test //crates/wolf-ffi:commands`
- `bazel test //crates/wolf-macos:WolfKeyMapTests`
- `bazel build //crates/wolf-macos:Wolf`

### Note (architect)

The architect's own generic "Project conventions" referenced a different project (a Go module `app/`, golangci-lint,
depguard). Those do NOT apply to Wolf (Rust + Swift, no Go); the equivalent real concerns (clippy strictness, swift-bridge
codegen regen, Cargo/Bazel wiring) were substituted. The coder should verify the referenced helper names
(`start_core_with_temp_md`, `drain_until_intent`, `wait_for_initial_window`, the `wolf_core_lib_test` target, and the
`layout_intents.rs`/`theme` BUILD targets) actually exist before copying, and adapt to the real ones.
