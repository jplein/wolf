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

/// How long to let a (non-)event settle before asserting that something did NOT
/// happen — long enough that a stray `close_window` command would have arrived if
/// it were going to. Generous on purpose so the negative "refused close" assertion
/// cannot pass by mere luck of timing. Mirrors `layout_intents.rs`'s `SETTLE`.
const SETTLE: Duration = Duration::from_secs(1);

/// Placeholder content for the temp markdown file.
const DOC: &str = "# Commands\nbody\n";

/// Distinctive text inserted before writing in the `:wq` test, so we can prove the
/// buffer's edited contents actually reached disk.
const WQ_MARKER: &str = "wolfwqmarker";

/// Polls `core`'s snapshot until `pred` holds, draining wakes each iteration, or
/// panics after [`FLUSH_TIMEOUT`]. `desc` describes the awaited condition.
/// (Port of `layout_intents.rs`'s `poll_until`.)
fn poll_until(core: &WolfCore, waiter: &FlushWaiter, desc: &str, pred: impl Fn(&Snapshot) -> bool) {
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
    let commands = drain_until_command(
        &core,
        FLUSH_TIMEOUT,
        "HideApplication from :WolfHideApplication",
    );
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

    // --- Ex-command face: `:WolfClosePane<CR>` -> Command::ClosePane ------------
    ex_command(&core, "WolfClosePane");
    let commands = drain_until_command(&core, FLUSH_TIMEOUT, "ClosePane from :WolfClosePane");
    assert!(
        commands.contains(&Command::ClosePane),
        ":WolfClosePane must enqueue Command::ClosePane; got {commands:?}"
    );

    // --- Ex-command face: `:WolfNewBuffer<CR>` -> Command::NewBuffer ------------
    ex_command(&core, "WolfNewBuffer");
    let commands = drain_until_command(&core, FLUSH_TIMEOUT, "NewBuffer from :WolfNewBuffer");
    assert!(
        commands.contains(&Command::NewBuffer),
        ":WolfNewBuffer must enqueue Command::NewBuffer; got {commands:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Typing `:q<CR>` on the SOLE (unmodified) pane routes through the `WolfQuit`
/// user command's `do_quit`, whose last-pane branch fires the existing
/// `close_window` registry command over RPC. A freshly attached core has no queued
/// commands (no emit-on-attach), so the drained `Command::CloseWindow` can only be
/// the one `:q` produced.
#[test]
fn ex_q_on_last_pane_emits_close_window() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // No emit-on-attach: the command queue is empty right after the window settles.
    assert!(
        core.drain_commands().is_empty(),
        "a freshly attached core must have NO queued commands before :q"
    );

    // `:q` on the sole unmodified pane -> last-pane branch -> close_window.
    ex_command(&core, "q");
    let commands = drain_until_command(&core, FLUSH_TIMEOUT, "CloseWindow from :q on the last pane");
    assert!(
        commands.contains(&Command::CloseWindow),
        ":q on the sole (unmodified) pane must enqueue Command::CloseWindow; got {commands:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `:q!` on a MODIFIED sole pane must still fire `close_window`: the trailing `!`
/// rides through the bare-`q` abbrev to `:WolfQuit!`, whose callback sees
/// `o.bang == true` and passes `force = true`, overriding the modified-buffer
/// guard. This exercises the FRAGILE bang path.
#[test]
fn ex_q_bang_forces_close_window_when_modified() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // Modify the buffer so a plain `:q` would be refused; only the bang closes.
    core.send_input(ch('i'));
    core.send_input(ch('x'));
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Discard anything queued during setup so we observe only `:q!`'s effect.
    let _ = core.drain_commands();

    // `:q!` -> WolfQuit! -> force = true -> last-pane branch fires close_window.
    ex_command(&core, "q!");
    let commands =
        drain_until_command(&core, FLUSH_TIMEOUT, "CloseWindow from :q! on a modified last pane");
    assert!(
        commands.contains(&Command::CloseWindow),
        ":q! on a modified last pane must force Command::CloseWindow (bang reached the \
         callback and overrode the modified-buffer guard); got {commands:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `:wq` on the sole pane writes the buffer to disk FIRST (so the distinctive
/// inserted text lands in the file) and THEN, the buffer no longer modified, fires
/// `close_window`. Proves the write side of the write-quit family and that the
/// last-pane branch still closes the front-end window afterward.
#[test]
fn ex_wq_writes_to_disk_and_closes_window() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let path = file.path().to_path_buf();
    wait_for_initial_window(&core, &waiter);

    // Insert distinctive text so we can prove the edited buffer reached disk.
    core.send_input(ch('i'));
    for c in WQ_MARKER.chars() {
        core.send_input(ch(c));
    }
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Discard anything queued during setup so we observe only `:wq`'s command.
    let _ = core.drain_commands();

    // `:wq` -> WolfWriteQuit -> write then last-pane close_window.
    ex_command(&core, "wq");

    // The write is async over RPC: poll the file until the inserted text appears.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if contents.contains(WQ_MARKER) {
                break;
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for :wq to write {WQ_MARKER:?} to disk"
            );
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    // The write completed; the last-pane branch must then fire close_window.
    let commands = drain_until_command(&core, FLUSH_TIMEOUT, "CloseWindow from :wq on the last pane");
    assert!(
        commands.contains(&Command::CloseWindow),
        ":wq must close the front-end window after writing; got {commands:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A plain `:q` on a MODIFIED last pane must be REFUSED (native `:q` parity): the
/// unsaved-changes guard surfaces an error and returns WITHOUT firing
/// `close_window`, so no window is torn out from under unsaved edits. Mirrors the
/// negative style of `ctrl_w_c_noop_on_last_window`.
#[test]
fn ex_q_refuses_modified_last_pane() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // Modify the buffer so the unsaved-changes guard should trip.
    core.send_input(ch('i'));
    core.send_input(ch('x'));
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Discard anything queued during setup so we observe only `:q`'s (non-)effect.
    let _ = core.drain_commands();

    // Plain `:q` (no bang) on the modified last pane: must NOT close.
    ex_command(&core, "q");

    // Let any (erroneous) command settle, accumulating drained commands (draining
    // is what wakes the queue side), then assert none was a close_window.
    let mut drained: Vec<Command> = Vec::new();
    let settle_deadline = Instant::now() + SETTLE;
    while Instant::now() < settle_deadline {
        drained.extend(core.drain_commands());
        std::thread::sleep(POLL_INTERVAL);
    }
    assert!(
        !drained.contains(&Command::CloseWindow),
        "a plain :q on a MODIFIED last pane must be refused (no close_window); got {drained:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Typing `:qa<CR>` on the SOLE (unmodified) pane routes through the
/// `WolfQuitAll` user command's `do_quit({ all = true })`, which always takes the
/// close-window branch and fires the existing `close_window` registry command over
/// RPC. In the single-pane case `:qa` is identical to `:q`. A freshly attached
/// core has no queued commands (no emit-on-attach), so the drained
/// `Command::CloseWindow` can only be the one `:qa` produced.
#[test]
fn ex_qa_on_last_pane_emits_close_window() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // No emit-on-attach: the command queue is empty right after the window settles.
    assert!(
        core.drain_commands().is_empty(),
        "a freshly attached core must have NO queued commands before :qa"
    );

    // `:qa` on the sole unmodified pane -> always-close branch -> close_window.
    ex_command(&core, "qa");
    let commands =
        drain_until_command(&core, FLUSH_TIMEOUT, "CloseWindow from :qa on the last pane");
    assert!(
        commands.contains(&Command::CloseWindow),
        ":qa on the sole (unmodified) pane must enqueue Command::CloseWindow; got {commands:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `:qa!` on a MODIFIED sole pane must still fire `close_window`: the trailing `!`
/// rides through the bare-`qa` abbrev to `:WolfQuitAll!`, whose callback sees
/// `o.bang == true` and passes `force = true`, overriding the modified-buffer
/// guard. This exercises the FRAGILE bang path for the qa family.
#[test]
fn ex_qa_bang_forces_close_window_when_modified() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // Modify the buffer so a plain `:qa` would be refused; only the bang closes.
    core.send_input(ch('i'));
    core.send_input(ch('x'));
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Discard anything queued during setup so we observe only `:qa!`'s effect.
    let _ = core.drain_commands();

    // `:qa!` -> WolfQuitAll! -> force = true -> always-close branch fires close_window.
    ex_command(&core, "qa!");
    let commands = drain_until_command(
        &core,
        FLUSH_TIMEOUT,
        "CloseWindow from :qa! on a modified last pane",
    );
    assert!(
        commands.contains(&Command::CloseWindow),
        ":qa! on a modified last pane must force Command::CloseWindow (bang reached the \
         callback and overrode the modified-buffer guard); got {commands:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `:wqa` on the sole pane writes the buffer to disk FIRST (so the distinctive
/// inserted text lands in the file) and THEN, the buffer no longer modified, fires
/// `close_window`. Proves the write side of the qa write-quit family and that the
/// always-close branch still closes the front-end window afterward.
#[test]
fn ex_wqa_writes_to_disk_and_closes_window() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let path = file.path().to_path_buf();
    wait_for_initial_window(&core, &waiter);

    // Insert distinctive text so we can prove the edited buffer reached disk.
    core.send_input(ch('i'));
    for c in WQ_MARKER.chars() {
        core.send_input(ch(c));
    }
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Discard anything queued during setup so we observe only `:wqa`'s command.
    let _ = core.drain_commands();

    // `:wqa` -> WolfWriteQuitAll -> write then always-close close_window.
    ex_command(&core, "wqa");

    // The write is async over RPC: poll the file until the inserted text appears.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if contents.contains(WQ_MARKER) {
                break;
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for :wqa to write {WQ_MARKER:?} to disk"
            );
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    // The write completed; the always-close branch must then fire close_window.
    let commands =
        drain_until_command(&core, FLUSH_TIMEOUT, "CloseWindow from :wqa on the last pane");
    assert!(
        commands.contains(&Command::CloseWindow),
        ":wqa must close the front-end window after writing; got {commands:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A plain `:qa` on a MODIFIED pane must be REFUSED (native `:qa` parity, and the
/// same current-buffer-scoped guard as `:q`): the unsaved-changes guard surfaces an
/// error and returns WITHOUT firing `close_window`. Mirrors
/// `ex_q_refuses_modified_last_pane`.
#[test]
fn ex_qa_refuses_modified_last_pane() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // Modify the buffer so the unsaved-changes guard should trip.
    core.send_input(ch('i'));
    core.send_input(ch('x'));
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Discard anything queued during setup so we observe only `:qa`'s (non-)effect.
    let _ = core.drain_commands();

    // Plain `:qa` (no bang) on the modified pane: must NOT close.
    ex_command(&core, "qa");

    // Let any (erroneous) command settle, accumulating drained commands (draining
    // is what wakes the queue side), then assert none was a close_window.
    let mut drained: Vec<Command> = Vec::new();
    let settle_deadline = Instant::now() + SETTLE;
    while Instant::now() < settle_deadline {
        drained.extend(core.drain_commands());
        std::thread::sleep(POLL_INTERVAL);
    }
    assert!(
        !drained.contains(&Command::CloseWindow),
        "a plain :qa on a MODIFIED pane must be refused (no close_window); got {drained:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// THE differentiator between `:qa` and `:q`. With MULTIPLE panes open, `:qa` must
/// still fire `close_window` (the whole front-end window closes), whereas `:q`
/// would instead close just the active pane and fire NO command (see
/// `layout_intents.rs::ex_q_closes_pane_when_multiple`, which proves `:q` merely
/// drops the pane count). We make a second pane via `:vsplit` (using only
/// `ex_command` + a snapshot poll, so this test needs no intent machinery), then
/// `:qa` and assert a `Command::CloseWindow` is drained.
#[test]
fn ex_qa_emits_close_window_when_multiple_panes() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    let base = core.read_snapshot().windows.len();

    // Make a second pane so this is genuinely the multi-pane case. Drive it via a
    // plain `:vsplit` and observe the window count grow (no intent/ctrl imports).
    ex_command(&core, "vsplit");
    poll_until(
        &core,
        &waiter,
        "windows.len() to grow by one after :vsplit",
        |snap| snap.windows.len() == base + 1,
    );

    // Discard anything queued during setup so we observe only `:qa`'s command.
    let _ = core.drain_commands();

    // `:qa` with multiple panes -> always-close branch (unlike `:q`) -> close_window.
    ex_command(&core, "qa");
    let commands = drain_until_command(
        &core,
        FLUSH_TIMEOUT,
        "CloseWindow from :qa with multiple panes",
    );
    assert!(
        commands.contains(&Command::CloseWindow),
        ":qa with multiple panes must close the WHOLE window (Command::CloseWindow), \
         not merely drop the pane count; got {commands:?}"
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
/// regress: `NewWindow`/`CloseWindow`/`ClosePane`/`NewBuffer` live under File,
/// `QuitApplication`/`HideApplication` under App, `MinimizeWindow` under Window,
/// and the font-size commands (`ResetFontSize`/`IncreaseFontSize`/
/// `DecreaseFontSize`) under View.
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
        (Command::ClosePane, MenuGroup::File),
        (Command::NewBuffer, MenuGroup::File),
        (Command::ResetFontSize, MenuGroup::View),
        (Command::IncreaseFontSize, MenuGroup::View),
        (Command::DecreaseFontSize, MenuGroup::View),
    ];
    for (command, menu) in expected {
        assert_eq!(
            command.spec().menu,
            menu,
            "{command:?} must belong to menu group {menu:?}"
        );
    }
}
