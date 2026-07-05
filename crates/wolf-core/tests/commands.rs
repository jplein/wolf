//! Integration + pure-registry tests for the S-Commands ARCHITECTURE: the new
//! Neovim->shell COMMAND CHANNEL that generalizes the `wolf_layout` intent channel.
//!
//! A Command is a no-parameter, no-return user action. Unlike layout intents,
//! commands are ONLY user-triggered (an Ex-command `:Wolf<Pascal>` or the Lua
//! `require('wolf').<snake>()` function) — there is no emit-on-attach, so a freshly
//! attached core has an EMPTY command queue (the load-bearing "no spurious commands
//! at startup" assertion). Nothing in core "acts" on a drained command; the shell
//! does. So typing `:WolfCloseWindow` / `:WolfQuitApplication` here is safe — core
//! merely folds the notification into its drainable `Vec<Command>` queue.
//!
//! Structure mirrors `layout_intents.rs`:
//!   * a REAL, hermetic nvim (no mocks) driven via typed Ex input, exactly the way
//!     `layout_intents.rs` types `:vsplit<CR>`;
//!   * the same `tests/common/mod.rs` helpers (`start_core_with_temp_md`,
//!     `FlushWaiter`, `FLUSH_TIMEOUT`, `ch`, `key`);
//!   * a `drain_until_command` helper modeled on `drain_until_intent`.
//!
//! The `Command`/`COMMAND_SPECS` PURE tests at the bottom do NOT start a core, so
//! they run regardless of whether a pinned nvim is available.
//!
//! NOTE to coder / ASSUMPTIONS (flagged for alignment):
//!   * `wolf_core::Command` derives `Debug, Clone, Copy, PartialEq, Eq` (used in
//!     `.contains(&Command::...)` and panic messages), per plan §2.
//!   * `CommandSpec` exposes its `command`, `id`, and `ex_command` fields publicly
//!     (so `COMMAND_SPECS` rows can be inspected). If the coder instead exposes
//!     accessor methods, adapt the four pure tests below (the single field-access
//!     site each).
//!   * `WolfCore::drain_commands(&self) -> Vec<Command>` mirrors `drain_intents`.

mod common;

use std::time::{Duration, Instant};

use common::{ch, key, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Command, Key, MenuGroup, Snapshot, WolfCore, COMMAND_SPECS};

/// The outer/global grid id. Window content lives on grids `>= 2`; grid 1 is
/// never a window grid in these scenarios.
const GLOBAL_GRID: u64 = 1;

/// Gap between polls while waiting for a structural change or a command.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Placeholder content for the temp markdown file.
const DOC: &str = "# Commands\nbody\n";

/// Polls `core`'s snapshot until `pred` holds, draining wakes each iteration, or
/// panics after [`FLUSH_TIMEOUT`]. `desc` describes the awaited condition.
/// (Port of `layout_intents.rs`'s `poll_until`.)
fn poll_until(
    core: &WolfCore,
    waiter: &FlushWaiter,
    desc: &str,
    pred: impl Fn(&Snapshot) -> bool,
) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        {
            let snap = core.read_snapshot();
            if pred(&snap) {
                return;
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Polls `core.drain_commands()` until it yields a non-empty batch (or panics
/// after `timeout`), returning the first non-empty drain. This is the plan §4
/// `drain_until_command` helper, the command-channel sibling of
/// `layout_intents.rs`'s `drain_until_intent`. Each `:Wolf*` Ex-command / `wolf`
/// Lua call under test enqueues exactly one `Command`, so the first non-empty
/// drain is the batch we assert on. `desc` describes the awaited command.
fn drain_until_command(core: &WolfCore, timeout: Duration, desc: &str) -> Vec<Command> {
    let deadline = Instant::now() + timeout;
    loop {
        let commands = core.drain_commands();
        if !commands.is_empty() {
            return commands;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {timeout:?} waiting for command: {desc}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Types an Ex command `:<cmd><CR>` one key at a time — the exact mechanism
/// `layout_intents.rs` uses to send `:vsplit`. Used here to drive both the
/// `:Wolf<Pascal>` Ex-command face and the `:lua require('wolf').<snake>()` Lua
/// face (both are just typed command-line input).
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    for c in cmd.chars() {
        core.send_input(ch(c));
    }
    core.send_input(key(Key::Enter));
}

/// Waits until a single editor window grid (`>= 2`) exists (the per-test
/// baseline). Port of `layout_intents.rs`'s `wait_for_initial_window`, minus the
/// returned count (this suite does not assert on window counts).
fn wait_for_initial_window(core: &WolfCore, waiter: &FlushWaiter) {
    poll_until(
        core,
        waiter,
        "the initial editor window grid (id != 1) to appear",
        |snap| !snap.windows.is_empty() && snap.grids.keys().any(|&id| id != GLOBAL_GRID),
    );
}

/// End-to-end command channel: a freshly attached core has NO queued commands
/// (commands are user-triggered only); typing the `:WolfCloseWindow` Ex-command
/// drains a `Command::CloseWindow`; and calling `require('wolf').quit_application()`
/// from Lua drains a `Command::QuitApplication` — the latter proving the `wolf`
/// module is reachable via `require` under the captive `--clean` nvim (the
/// `package.loaded['wolf']` seeding).
///
/// Drives EVERYTHING via typed command-line input, exactly like
/// `layout_intents.rs` types `:vsplit` (there is no arbitrary-eval API). Nothing
/// in core acts on a drained command, so triggering "close"/"quit" here destroys
/// nothing.
#[test]
fn ex_and_lua_faces_emit_commands() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // No emit-on-attach: the command queue is empty right after the initial window
    // settles. (Layout intents CAN arrive during setup; commands never do.)
    assert!(
        core.drain_commands().is_empty(),
        "a freshly attached core must have NO queued commands (commands are \
         user-triggered only; there is no emit-on-attach)"
    );

    // --- Ex-command face: `:WolfCloseWindow<CR>` -> Command::CloseWindow ---------
    ex_command(&core, "WolfCloseWindow");
    let commands = drain_until_command(&core, FLUSH_TIMEOUT, "CloseWindow from :WolfCloseWindow");
    assert!(
        commands.contains(&Command::CloseWindow),
        ":WolfCloseWindow must enqueue Command::CloseWindow; got {commands:?}"
    );

    // --- Lua face: `:lua require('wolf').quit_application()<CR>` -----------------
    // Proves `require('wolf')` resolves under `--clean` (via package.loaded seeding)
    // and that the snake_case function `quit_application` rpcnotifies the same
    // `wolf_command` channel.
    ex_command(&core, "lua require('wolf').quit_application()");
    let commands = drain_until_command(
        &core,
        FLUSH_TIMEOUT,
        "QuitApplication from require('wolf').quit_application()",
    );
    assert!(
        commands.contains(&Command::QuitApplication),
        "require('wolf').quit_application() must enqueue Command::QuitApplication \
         (proves the `wolf` module loads under --clean); got {commands:?}"
    );

    // --- Ex-command face: `:WolfHideApplication<CR>` -> Command::HideApplication -
    ex_command(&core, "WolfHideApplication");
    let commands =
        drain_until_command(&core, FLUSH_TIMEOUT, "HideApplication from :WolfHideApplication");
    assert!(
        commands.contains(&Command::HideApplication),
        ":WolfHideApplication must enqueue Command::HideApplication; got {commands:?}"
    );

    // --- Lua face: `:lua require('wolf').new_window()<CR>` -> Command::NewWindow --
    ex_command(&core, "lua require('wolf').new_window()");
    let commands = drain_until_command(
        &core,
        FLUSH_TIMEOUT,
        "NewWindow from require('wolf').new_window()",
    );
    assert!(
        commands.contains(&Command::NewWindow),
        "require('wolf').new_window() must enqueue Command::NewWindow; got {commands:?}"
    );

    // --- Ex-command face: `:WolfCloseFile<CR>` -> Command::CloseFile ------------
    ex_command(&core, "WolfCloseFile");
    let commands = drain_until_command(&core, FLUSH_TIMEOUT, "CloseFile from :WolfCloseFile");
    assert!(
        commands.contains(&Command::CloseFile),
        ":WolfCloseFile must enqueue Command::CloseFile; got {commands:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

// --- Pure registry tests (no nvim; run regardless of the skip guard) ----------
//
// These pin the canonical `COMMAND_SPECS` table + `Command::from_id` contract
// without starting a core, so they pass on hosts with no pinned Neovim. They are
// the integration-file home for the plan §2 `command.rs` unit assertions (kept
// here to avoid colliding with the coder creating `command.rs` itself).

/// Every spec's canonical `id` round-trips back to its `Command` via `from_id`.
#[test]
fn command_specs_ids_round_trip() {
    for spec in COMMAND_SPECS {
        assert_eq!(
            Command::from_id(spec.id),
            Some(spec.command),
            "id {:?} must round-trip via Command::from_id",
            spec.id
        );
    }
}

/// An unknown id yields `None`.
#[test]
fn command_from_id_unknown_is_none() {
    assert_eq!(
        Command::from_id("nope"),
        None,
        "an unknown command id must map to None"
    );
}

/// No two specs share an `id` or an `ex_command` (each face is unambiguous).
#[test]
fn command_specs_have_no_duplicate_ids_or_ex_commands() {
    use std::collections::HashSet;

    let mut ids = HashSet::new();
    let mut ex_commands = HashSet::new();
    for spec in COMMAND_SPECS {
        assert!(
            ids.insert(spec.id),
            "duplicate command id {:?} in COMMAND_SPECS",
            spec.id
        );
        assert!(
            ex_commands.insert(spec.ex_command),
            "duplicate ex_command {:?} in COMMAND_SPECS",
            spec.ex_command
        );
    }
}

/// Every `ex_command` uses the required `Wolf` prefix (Neovim forces an
/// uppercase-first user-command name; Wolf namespaces them all under `Wolf`).
#[test]
fn command_specs_ex_commands_start_with_wolf() {
    for spec in COMMAND_SPECS {
        assert!(
            spec.ex_command.starts_with("Wolf"),
            "ex_command {:?} (id {:?}) must start with \"Wolf\"",
            spec.ex_command,
            spec.id
        );
    }
}

/// Pins each command's `menu` group to the intended macOS menu-bar convention,
/// so the grouping (unused by the current hardcoded macOS menu) cannot silently
/// regress: `NewWindow`/`CloseWindow` live under File, `QuitApplication`/
/// `HideApplication` under App, and `MinimizeWindow` under Window.
#[test]
fn command_specs_menu_groups_match_macos_convention() {
    let expected = [
        (Command::NewWindow, MenuGroup::File),
        (Command::CloseWindow, MenuGroup::File),
        (Command::QuitApplication, MenuGroup::App),
        (Command::HideApplication, MenuGroup::App),
        (Command::MinimizeWindow, MenuGroup::Window),
        (Command::GoToFile, MenuGroup::File),
        (Command::OpenFile, MenuGroup::File),
        (Command::OpenFolder, MenuGroup::File),
        (Command::CloseFile, MenuGroup::File),
    ];
    for (command, menu) in expected {
        assert_eq!(
            command.spec().menu,
            menu,
            "{command:?} must belong to menu group {menu:?}"
        );
    }
}
