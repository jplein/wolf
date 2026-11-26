//! Integration + pure-registry tests for the COMMAND CHANNEL: the Neovim->shell
//! channel that generalizes the `wolf_layout` intent channel.
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
//! The tests rely on `wolf_core::Command` deriving `PartialEq` and `Debug` (for
//! `.contains(&Command::...)` and panic messages) and on `CommandSpec`'s public
//! `command`, `id`, and `ex_command` fields (so `COMMAND_SPECS` rows can be
//! inspected).

mod common;

use std::time::{Duration, Instant};

use common::{
    ch, key, poll_until, probe_values, settle_for, type_str, FlushWaiter, FLUSH_TIMEOUT,
    POLL_INTERVAL,
};
use wolf_core::{
    CloseTabAnswer, Command, InputEvent, Key, Liveness, MenuGroup, UnsavedAnswer, WolfCore,
    COMMAND_SPECS,
};

/// The outer/global grid id. Window content lives on grids `>= 2`; grid 1 is
/// never a window grid in these scenarios.
const GLOBAL_GRID: u64 = 1;

/// How long to let a (non-)event settle before asserting that something did NOT
/// happen — long enough that a stray `close_tab` command would have arrived if
/// it were going to. Generous on purpose so the negative "refused close" assertion
/// cannot pass by mere luck of timing. Mirrors `layout_intents.rs`'s `SETTLE`.
const SETTLE: Duration = Duration::from_secs(1);

/// Placeholder content for the temp markdown file.
const DOC: &str = "# Commands\nbody\n";

/// Distinctive text inserted before writing in the `:wq` test, so we can prove the
/// buffer's edited contents actually reached disk.
const WQ_MARKER: &str = "wolfwqmarker";

/// Text inserted before a normal-mode `ZZ`, absent from `DOC`, so only a write puts it on disk.
const ZZ_MARKER: &str = "wolfzzmarker";

/// Text inserted before a normal-mode `ZQ`, absent from `DOC`, so a write would show on disk.
const ZQ_MARKER: &str = "wolfzqmarker";

/// Polls `core.drain_commands()` until it yields a non-empty batch (or panics
/// after `timeout`), returning the first non-empty drain. This is the
/// command-channel sibling of `layout_intents.rs`'s `drain_until_intent`. Each
/// `:Wolf*` Ex-command / `wolf` Lua call under test enqueues exactly one
/// `Command`, so the first non-empty drain is the batch we assert on. `desc`
/// describes the awaited command.
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
        waiter,
        "the initial editor window grid (id != 1) to appear",
        || {
            let snap = core.read_snapshot();
            !snap.windows.is_empty() && snap.grids.keys().any(|&id| id != GLOBAL_GRID)
        },
        |&ok| ok,
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

    // Lua face: `require('wolf')` resolves under `--clean` (via package.loaded seeding), and
    // `quit_application` rpcnotifies the same `wolf_command` channel.
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

/// The second file's contents in the tests that hide a modified buffer behind the first.
const SECOND_DOC: &str = "# Second\nother body\n";

/// Text typed into the second file before it is hidden, absent from both files.
const HIDDEN_MARKER: &str = "wolfhiddenmarker";

/// Text typed into the first file in the tests that also modify a hidden second one.
const FIRST_MARKER: &str = "wolffirstmarker";

/// Vim's refusal of a quit that would lose the current buffer's changes.
const E37: &str = "E37: No write since last change (add ! to override)";

/// How long the sweep of every quit form waits, after a form's `CloseTab`, for a second
/// notification the same quit sent.
const SWEEP_SETTLE: Duration = Duration::from_millis(300);

/// Vim's refusal of a quit that would lose the changes of another buffer, the one Vim names
/// `name`.
fn e162(name: &str) -> String {
    format!("E162: No write since last change for buffer \"{name}\"")
}

/// Evaluates `exprs` in Neovim, behind every input sent before. Its typed command clears the
/// toast, so a test reads the toast before it probes.
fn probe(core: &WolfCore, exprs: &[&str]) -> Vec<String> {
    let dir = tempfile::tempdir().expect("failed to create the probe dir");
    probe_values(core, exprs, &dir.path().join("probe.txt"))
}

/// The toast's lines, each its chunks joined, joined with newlines.
fn toast_text(core: &WolfCore) -> String {
    core.read_snapshot()
        .toast
        .lines()
        .iter()
        .map(|line| line.iter().map(|chunk| chunk.text.as_str()).collect())
        .collect::<Vec<String>>()
        .join("\n")
}

/// Waits for the toast to show `want` after the quit `what`, then drains commands for
/// [`SETTLE`]. Returns the toast's text and every command drained since the quit was typed.
fn refused(
    core: &WolfCore,
    waiter: &FlushWaiter,
    what: &str,
    want: &str,
) -> (String, Vec<Command>) {
    let toast = poll_until(
        waiter,
        &format!(
            "the toast to show {want:?} after {what}; another message means the refusal named \
             the wrong buffer, and none means the quit was not refused"
        ),
        || toast_text(core),
        |text| text.contains(want),
    );
    let mut drained = core.drain_commands();
    let deadline = Instant::now() + SETTLE;
    while Instant::now() < deadline {
        drained.extend(core.drain_commands());
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
    (toast, drained)
}

/// Drains the first batch of commands after the quit `what` and asserts it holds
/// `Command::CloseTab`.
fn expect_close_tab(core: &WolfCore, what: &str) {
    let commands = drain_until_command(
        core,
        FLUSH_TIMEOUT,
        &format!(
            "CloseTab from {what}; a timeout means the quit was refused, closed a pane, or \
             notified nothing"
        ),
    );
    assert!(
        commands.contains(&Command::CloseTab),
        "{what} must enqueue Command::CloseTab; got {commands:?}. CloseWindow alone means the \
         quit still asks for the whole window rather than the tab."
    );
}

/// Inserts `text` at the cursor, returns to normal mode and waits for the edit's flush.
fn insert_text(core: &WolfCore, waiter: &FlushWaiter, text: &str) {
    core.send_input(ch('i'));
    type_str(core, text);
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(core, FLUSH_TIMEOUT);
}

/// Edits a new second file, types [`HIDDEN_MARKER`] into it, then edits `first` again, which
/// leaves the second buffer modified and in no window. Returns the second file, kept alive,
/// and its `bufname()`.
fn hide_a_modified_second_file(
    core: &WolfCore,
    waiter: &FlushWaiter,
    first: &std::path::Path,
) -> (tempfile::NamedTempFile, String) {
    let second = common::temp_md(SECOND_DOC);
    let first_buf = probe(core, &["bufnr()"]).remove(0);
    ex_command(core, &format!("e {}", second.path().display()));
    insert_text(core, waiter, HIDDEN_MARKER);
    let shown = probe(core, &["bufnr()", "bufname('%')", "&modified"]);
    assert!(
        shown[0] != first_buf && shown[2] == "1",
        "premise: `:e` of the second file must make a buffer other than the first file's \
         ({first_buf}) current, and the typed marker must leave it modified; (bufnr, bufname, \
         &modified) reads {shown:?}"
    );
    ex_command(core, &format!("e {}", first.display()));
    let second_modified = format!("getbufvar({}, '&modified')", shown[0]);
    let second_win = format!("bufwinnr({})", shown[0]);
    let back = probe(
        core,
        &["bufnr()", second_modified.as_str(), second_win.as_str()],
    );
    assert_eq!(
        back,
        [first_buf.clone(), "1".to_owned(), "-1".to_owned()],
        "premise: `:e` of the first file must make buffer {first_buf} current again and leave \
         the second, buffer {}, modified and in no window: (bufnr, its &modified, its \
         bufwinnr). The second buffer still current means `:e` was refused with E37, because \
         'hidden' is off.",
        shown[0]
    );
    (second, shown[1].clone())
}

/// Typing `:q<CR>` on the SOLE (unmodified) pane routes through the `WolfQuit` user command's
/// `do_quit`, whose last-pane branch notifies `close_tab`. A freshly attached core has no
/// queued commands (no emit-on-attach), so the drained `Command::CloseTab` can only be the one
/// `:q` produced.
#[test]
fn ex_q_on_last_pane_emits_close_tab() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // No emit-on-attach: the command queue is empty right after the window settles.
    assert!(
        core.drain_commands().is_empty(),
        "a freshly attached core must have NO queued commands before :q"
    );

    ex_command(&core, "q");
    expect_close_tab(&core, ":q on the sole (unmodified) pane");

    core.shutdown().expect("shutdown should be Ok");
}

/// A plain `:q` on the last pane is refused while a buffer no window shows is modified: the
/// check covers the whole tab's Neovim, and names that buffer with E162 rather than E37.
#[test]
fn ex_q_refuses_hidden_modified_buffer_with_e162() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, "x");
    let file = probe(&core, &["bufnr()", "bufname('%')"]);
    ex_command(&core, "enew");
    let file_modified = format!("getbufvar({}, '&modified')", file[0]);
    let premise = probe(
        &core,
        &["bufname('%')", "&modified", file_modified.as_str()],
    );
    assert_eq!(
        premise,
        ["", "0", "1"],
        "premise: `:enew` must show a new unnamed, unmodified buffer and leave the file's \
         buffer {} modified: (bufname, &modified, the file's &modified). The file still current \
         means `:enew` was refused with E37, because 'hidden' is off.",
        file[0]
    );
    // Discard anything queued during setup so we observe only `:q`'s (non-)effect.
    let _ = core.drain_commands();

    ex_command(&core, "q");
    let (toast, drained) = refused(&core, &waiter, ":q", &e162(&file[1]));
    assert!(
        !toast.contains("E37"),
        "the refusal must name the hidden buffer with E162 alone; the toast reads {toast:?}, so \
         E37 was echoed as well"
    );
    assert!(
        drained.is_empty(),
        "a refused :q must notify nothing; got {drained:?}. CloseTab means the check looked at \
         the current buffer only; CloseWindow means the quit still closes the window."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A plain `:q` on a MODIFIED last pane is refused with E37 (native `:q` parity) and notifies
/// nothing, so no tab is torn out from under unsaved edits.
#[test]
fn ex_q_refuses_modified_current_buffer_with_e37() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, "x");
    // Discard anything queued during setup so we observe only `:q`'s (non-)effect.
    let _ = core.drain_commands();

    ex_command(&core, "q");
    let (toast, drained) = refused(&core, &waiter, ":q", E37);
    assert!(
        !toast.contains("E162"),
        "a refusal for the current buffer must be E37 alone; the toast reads {toast:?}, so the \
         current buffer was reported as another buffer"
    );
    assert!(
        drained.is_empty(),
        "a plain :q on a MODIFIED last pane must be refused and notify nothing; got {drained:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `:q!` on a last pane whose buffer is MODIFIED, beside a hidden modified buffer, still
/// notifies `close_tab`: the trailing `!` rides through the bare-`q` abbrev to `:WolfQuit!`,
/// whose callback passes `force = true`, which skips the whole tab's check.
#[test]
fn ex_q_bang_forces_close_tab_when_modified() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    let (_second, _) = hide_a_modified_second_file(&core, &waiter, file.path());
    insert_text(&core, &waiter, FIRST_MARKER);
    assert_eq!(
        probe(&core, &["&modified"]),
        ["1"],
        "premise: the typed marker must leave the current buffer modified"
    );
    // Discard anything queued during setup so we observe only `:q!`'s effect.
    let _ = core.drain_commands();

    ex_command(&core, "q!");
    expect_close_tab(
        &core,
        ":q! on a modified last pane beside a hidden modified buffer (the bang must reach the \
         callback and skip the check)",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `:wq` on the sole pane writes the buffer to disk FIRST (so the distinctive inserted text
/// lands in the file) and THEN, the buffer no longer modified, notifies `close_tab`.
#[test]
fn ex_wq_writes_to_disk_and_closes_tab() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let path = file.path().to_path_buf();
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, WQ_MARKER);
    // Discard anything queued during setup so we observe only `:wq`'s command.
    let _ = core.drain_commands();

    ex_command(&core, "wq");
    wait_for_file_to_hold(&path, WQ_MARKER, ":wq");
    expect_close_tab(&core, ":wq on the last pane, after writing");

    core.shutdown().expect("shutdown should be Ok");
}

/// `:wq` writes the current buffer, then runs the same check as `:q`: a hidden buffer still
/// modified refuses the close with E162 naming it.
#[test]
fn ex_wq_writes_then_refuses_a_hidden_modified_buffer() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let path = file.path().to_path_buf();
    wait_for_initial_window(&core, &waiter);

    let (_second, second_name) = hide_a_modified_second_file(&core, &waiter, &path);
    insert_text(&core, &waiter, FIRST_MARKER);
    // Discard anything queued during setup so we observe only `:wq`'s (non-)effect.
    let _ = core.drain_commands();

    ex_command(&core, "wq");
    wait_for_file_to_hold(&path, FIRST_MARKER, ":wq on the current buffer");
    let (toast, drained) = refused(&core, &waiter, ":wq", &e162(&second_name));
    assert!(
        !toast.contains("E37"),
        "the current buffer was written, so the refusal must be E162 alone; the toast reads \
         {toast:?}"
    );
    assert!(
        drained.is_empty(),
        "a :wq that leaves a hidden buffer modified must notify nothing; got {drained:?}. \
         CloseTab means the check is skipped after a write."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Typing `:qa<CR>` on the SOLE (unmodified) pane routes through the `WolfQuitAll` user
/// command's `do_quit({ all = true })`, which notifies `close_tab`. In the single-pane case
/// `:qa` is identical to `:q`.
#[test]
fn ex_qa_on_last_pane_emits_close_tab() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // No emit-on-attach: the command queue is empty right after the window settles.
    assert!(
        core.drain_commands().is_empty(),
        "a freshly attached core must have NO queued commands before :qa"
    );

    ex_command(&core, "qa");
    expect_close_tab(&core, ":qa on the sole (unmodified) pane");

    core.shutdown().expect("shutdown should be Ok");
}

/// `:qa!` on a MODIFIED sole pane still notifies `close_tab`: the trailing `!` rides through
/// the bare-`qa` abbrev to `:WolfQuitAll!`, whose callback passes `force = true`.
#[test]
fn ex_qa_bang_forces_close_tab_when_modified() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, "x");
    // Discard anything queued during setup so we observe only `:qa!`'s effect.
    let _ = core.drain_commands();

    ex_command(&core, "qa!");
    expect_close_tab(
        &core,
        ":qa! on a modified last pane (the bang must reach the callback and skip the check)",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `:wqa` writes every modified buffer, the hidden one as well as the current one, as Vim's
/// `:wqa` does, and THEN notifies `close_tab`.
#[test]
fn ex_wqa_writes_every_modified_buffer_then_closes_tab() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let path = file.path().to_path_buf();
    wait_for_initial_window(&core, &waiter);

    let (second, _) = hide_a_modified_second_file(&core, &waiter, &path);
    insert_text(&core, &waiter, FIRST_MARKER);
    // Discard anything queued during setup so we observe only `:wqa`'s command.
    let _ = core.drain_commands();

    ex_command(&core, "wqa");
    wait_for_file_to_hold(&path, FIRST_MARKER, ":wqa, for the current buffer,");
    wait_for_file_to_hold(
        second.path(),
        HIDDEN_MARKER,
        ":wqa, for the hidden buffer (unchanged alone means it wrote the current buffer only),",
    );
    expect_close_tab(&core, ":wqa after writing every modified buffer");

    core.shutdown().expect("shutdown should be Ok");
}

/// `:wqa!` whose write fails, here for a modified buffer with no file name, aborts and
/// notifies nothing, bang or not. Once the buffer is named, the same `:wqa!` closes the tab,
/// so the abort is the failed write's and not a route that never ran.
#[test]
fn ex_wqa_bang_aborts_when_a_write_fails() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    ex_command(&core, "enew");
    insert_text(&core, &waiter, "unnamed text");
    assert_eq!(
        probe(&core, &["bufname('%')", "&modified"]),
        ["", "1"],
        "premise: `:enew` and the typed text must leave an unnamed, modified buffer current"
    );
    // Discard anything queued during setup so we observe only `:wqa!`'s (non-)effect.
    let _ = core.drain_commands();

    ex_command(&core, "wqa!");
    settle_for(&waiter, SETTLE);
    let drained = core.drain_commands();
    assert!(
        drained.is_empty(),
        ":wqa! with an unnamed modified buffer cannot write it, so it must abort and notify \
         nothing; got {drained:?}. CloseTab means the failed write did not abort the quit."
    );

    let dir = tempfile::tempdir().expect("failed to create the named file's dir");
    let named = dir.path().join("named.md");
    ex_command(&core, &format!("file {}", named.display()));
    ex_command(&core, "wqa!");
    wait_for_file_to_hold(&named, "unnamed text", ":wqa! once the buffer is named");
    expect_close_tab(&core, ":wqa! once every write can succeed");

    core.shutdown().expect("shutdown should be Ok");
}

/// A plain `:qa` on a MODIFIED pane is refused with E37 (native `:qa` parity) and notifies
/// nothing.
#[test]
fn ex_qa_refuses_modified_current_buffer() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, "x");
    // Discard anything queued during setup so we observe only `:qa`'s (non-)effect.
    let _ = core.drain_commands();

    ex_command(&core, "qa");
    let (_toast, drained) = refused(&core, &waiter, ":qa", E37);
    assert!(
        drained.is_empty(),
        "a plain :qa on a MODIFIED pane must be refused and notify nothing; got {drained:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// With two panes, `:qa` refuses while the buffer shown in the other pane is modified, naming
/// it with E162, and closes neither pane.
#[test]
fn ex_qa_refuses_a_modified_buffer_in_another_pane() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);
    let base = core.read_snapshot().windows.len();

    insert_text(&core, &waiter, "x");
    let file = probe(&core, &["bufnr()", "bufname('%')"]);
    ex_command(&core, "vsplit");
    poll_until(
        &waiter,
        "windows.len() to grow by one after :vsplit",
        || core.read_snapshot().windows.len() == base + 1,
        |&ok| ok,
    );
    ex_command(&core, "enew");
    let file_modified = format!("getbufvar({}, '&modified')", file[0]);
    let file_shown = format!("bufwinnr({}) > 0", file[0]);
    let premise = probe(
        &core,
        &[
            "bufname('%')",
            "winnr('$')",
            file_modified.as_str(),
            file_shown.as_str(),
        ],
    );
    assert_eq!(
        premise,
        ["", "2", "1", "1"],
        "premise: `:enew` in the new pane must show an unnamed buffer there, beside the other \
         pane still showing the modified file: (bufname, windows, the file's &modified, the \
         file shown)"
    );
    // Discard anything queued during setup so we observe only `:qa`'s (non-)effect.
    let _ = core.drain_commands();

    ex_command(&core, "qa");
    let (_toast, drained) = refused(&core, &waiter, ":qa with two panes", &e162(&file[1]));
    assert!(
        drained.is_empty(),
        "a refused :qa must notify nothing; got {drained:?}. CloseTab means :qa skips the check."
    );
    assert_eq!(
        probe(&core, &["winnr('$')"]),
        ["2"],
        "a refused :qa must close no pane; one window left means it closed the current pane"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The differentiator between `:qa` and `:q`: with MULTIPLE panes open, `:qa` still notifies
/// `close_tab`, whereas `:q` closes just the active pane and notifies nothing (see
/// `layout_intents.rs::ex_q_closes_pane_when_multiple`).
#[test]
fn ex_qa_emits_close_tab_when_multiple_panes() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    let base = core.read_snapshot().windows.len();

    // Make a second pane so this is genuinely the multi-pane case.
    ex_command(&core, "vsplit");
    poll_until(
        &waiter,
        "windows.len() to grow by one after :vsplit",
        || core.read_snapshot().windows.len() == base + 1,
        |&ok| ok,
    );

    // Discard anything queued during setup so we observe only `:qa`'s command.
    let _ = core.drain_commands();

    ex_command(&core, "qa");
    expect_close_tab(
        &core,
        ":qa with multiple panes (it closes the tab, not merely the current pane)",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Every form of the `:q` and `:qa` families, on an unmodified last pane, notifies `close_tab`
/// and never `close_window`. Nothing in core acts on `close_tab`, so one Neovim serves every
/// form in turn.
#[test]
fn no_quit_command_notifies_close_window() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);
    assert!(
        core.drain_commands().is_empty(),
        "a freshly attached core must have NO queued commands before the sweep"
    );

    let ex_forms = [
        "q", "quit", "q!", "wq", "x", "xit", "qa", "qall", "quitall", "qa!", "wqa", "wqall", "xa",
        "xall",
    ];
    let forms = ex_forms
        .iter()
        .map(|form| format!(":{form}"))
        .chain(["ZZ".to_owned(), "ZQ".to_owned()]);
    for name in forms {
        match name.strip_prefix(':') {
            Some(form) => ex_command(&core, form),
            None => type_str(&core, &name),
        }
        let mut drained = drain_until_command(
            &core,
            FLUSH_TIMEOUT,
            &format!("a command from {name}; a timeout means it notified nothing"),
        );
        let deadline = Instant::now() + SWEEP_SETTLE;
        while Instant::now() < deadline {
            drained.extend(core.drain_commands());
            std::thread::sleep(POLL_INTERVAL);
        }
        assert!(
            drained.contains(&Command::CloseTab) && !drained.contains(&Command::CloseWindow),
            "{name} on an unmodified last pane must notify close_tab and never close_window; \
             got {drained:?}. CloseWindow alone means the form still closes the window; both \
             mean it notifies close_window after close_tab."
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// Types `:q` on the sole, unmodified pane and asserts it refuses with E162 for the buffer Vim
/// names `name` and notifies nothing. `what` says which modified buffer the step made.
fn expect_q_refused_naming(core: &WolfCore, waiter: &FlushWaiter, what: &str, name: &str) {
    // Discard anything queued during setup so we observe only `:q`'s (non-)effect.
    let _ = core.drain_commands();
    ex_command(core, "q");
    let (toast, drained) = refused(core, waiter, &format!(":q beside {what}"), &e162(name));
    assert!(
        !toast.contains("E37"),
        "the refusal beside {what} must be E162 alone; the toast reads {toast:?}, so the current \
         buffer was reported as modified"
    );
    assert!(
        drained.is_empty(),
        "a :q refused beside {what} must notify nothing; got {drained:?}. CloseTab means {what} \
         did not count as unsaved."
    );
}

/// A hidden, unnamed buffer refuses `:q` whatever its `'buftype'`, once Neovim reads it as
/// modified, and E162 names it as Vim's `:ls!` does: `[Prompt]`, `[Scratch]` for `acwrite`, and
/// `[No Name]` for a normal buffer.
#[test]
fn ex_q_refuses_unnamed_hidden_buffers_by_their_vim_names() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // (buftype, the Lua that makes buffer `b` modified, the name Vim gives it)
    let steps = [
        ("prompt", "vim.bo[b].modified = true", "[Prompt]"),
        (
            "acwrite",
            "vim.api.nvim_buf_set_lines(b, 0, -1, false, { 'wolf' })",
            "[Scratch]",
        ),
        (
            "",
            "vim.api.nvim_buf_set_lines(b, 0, -1, false, { 'wolf' })",
            "[No Name]",
        ),
    ];
    for (buftype, modify, name) in steps {
        ex_command(
            &core,
            &format!(
                "lua local b = vim.api.nvim_create_buf(false, false); vim.g.wolf_test_buf = b; \
                 vim.bo[b].buftype = '{buftype}'; {modify}"
            ),
        );
        let premise = probe(
            &core,
            &[
                "getbufvar(g:wolf_test_buf, '&modified')",
                "getbufvar(g:wolf_test_buf, '&buftype')",
                "bufname(g:wolf_test_buf)",
                "&modified",
            ],
        );
        assert_eq!(
            premise,
            ["1", buftype, "", "0"],
            "premise: the new buffer must be an unnamed, modified {buftype:?} buffer beside the \
             unmodified current one: (its &modified, its &buftype, its bufname, the current \
             &modified). A 0 first means Neovim does not mark this buftype modified."
        );

        expect_q_refused_naming(
            &core,
            &waiter,
            &format!("a modified, unnamed {buftype:?} buffer"),
            name,
        );

        ex_command(
            &core,
            "lua vim.api.nvim_buf_delete(vim.g.wolf_test_buf, { force = true })",
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// A hidden, modified quickfix list refuses `:q` as `[Quickfix List]`, and a hidden, modified
/// location list as `[Location List]`, the names Vim's `:ls!` gives them.
#[test]
fn ex_q_refuses_hidden_quickfix_and_location_lists_by_their_vim_names() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    // (the list's setter, its open and close commands, its getter, the name Vim gives it)
    let steps = [
        (
            "setqflist(",
            "copen",
            "cclose",
            "getqflist(",
            "[Quickfix List]",
        ),
        (
            "setloclist(0, ",
            "lopen",
            "lclose",
            "getloclist(0, ",
            "[Location List]",
        ),
    ];
    for (set, open, close, get, name) in steps {
        ex_command(
            &core,
            &format!("lua vim.fn.{set}{{ {{ text = 'wolf' }} }})"),
        );
        ex_command(&core, open);
        ex_command(&core, close);
        ex_command(
            &core,
            &format!(
                "lua local b = vim.fn.{get}{{ qfbufnr = 0 }}).qfbufnr; vim.g.wolf_test_buf = b; \
                 vim.bo[b].modifiable = true; vim.api.nvim_buf_set_lines(b, 0, -1, false, \
                 {{ 'wolf' }})"
            ),
        );
        let premise = probe(
            &core,
            &[
                "g:wolf_test_buf != 0",
                "getbufvar(g:wolf_test_buf, '&modified')",
                "getbufvar(g:wolf_test_buf, '&buftype')",
                "bufwinnr(g:wolf_test_buf)",
                "winnr('$')",
                "&modified",
                "g:wolf_test_buf == getqflist({'qfbufnr': 0}).qfbufnr",
            ],
        );
        let is_quickfix = if open == "copen" { "1" } else { "0" };
        assert_eq!(
            premise,
            ["1", "1", "quickfix", "-1", "1", "0", is_quickfix],
            "premise: after `:{open}` and `:{close}`, the list's buffer must exist, be modified, \
             be a quickfix buffer in no window, beside the sole, unmodified pane, and be the \
             quickfix list's buffer only for `:copen`: (it exists, its &modified, its \
             &buftype, its bufwinnr, windows, the current &modified, it is the quickfix list's)"
        );

        expect_q_refused_naming(
            &core,
            &waiter,
            &format!("the modified list `:{open}` showed"),
            name,
        );

        ex_command(&core, "lua vim.bo[vim.g.wolf_test_buf].modified = false");
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// Polls `path` until it holds `marker`, failing with `what` after [`FLUSH_TIMEOUT`].
fn wait_for_file_to_hold(path: &std::path::Path, marker: &str, what: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let contents = std::fs::read_to_string(path);
        if matches!(&contents, Ok(text) if text.contains(marker)) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{what} must write {marker:?} to disk within {FLUSH_TIMEOUT:?}; the file reads \
             {contents:?}. Unchanged means the key closed or quit without writing, or the buffer \
             was never modified."
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Asserts the captive Neovim is still `Alive` after [`SETTLE`], since `what` closed the last
/// pane through `close_tab` rather than by quitting Neovim.
fn assert_alive_after_settle(core: &WolfCore, waiter: &FlushWaiter, what: &str) {
    settle_for(waiter, SETTLE);
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "{what} on the last pane must leave Neovim running. Dead means Neovim quit itself, \
         the native key's crash, even though close_tab drained; NotResponding means it hung."
    );
}

/// `ZZ` on a MODIFIED sole pane behaves as `:x` there: it writes the buffer through `do_quit`,
/// then notifies `close_tab`, and Neovim keeps running. Native `ZZ` would quit Neovim instead.
#[test]
fn zz_on_last_pane_writes_and_emits_close_tab() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let path = file.path().to_path_buf();
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, ZZ_MARKER);
    // Discard anything queued during setup so we observe only `ZZ`'s command.
    let _ = core.drain_commands();

    type_str(&core, "ZZ");
    wait_for_file_to_hold(&path, ZZ_MARKER, "ZZ on a modified last pane");
    expect_close_tab(
        &core,
        "ZZ on the last pane, after writing (a timeout may mean Neovim quit instead)",
    );
    assert_alive_after_settle(&core, &waiter, "ZZ");

    core.shutdown().expect("shutdown should be Ok");
}

/// `ZZ` on an UNMODIFIED sole pane writes nothing, as native `ZZ` does and unlike Wolf's `:x`,
/// which always writes. The file is deleted from disk first, so any write would recreate it.
#[test]
fn zz_on_unmodified_last_pane_writes_nothing_and_emits_close_tab() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let path = file.path().to_path_buf();
    wait_for_initial_window(&core, &waiter);

    file.close()
        .expect("failed to delete the temp file from disk");
    let premise = probe(&core, &["&modified", "filereadable(expand('%:p'))"]);
    assert_eq!(
        premise,
        ["0", "0"],
        "premise: the buffer must be unmodified (first) and its file gone from disk (second)"
    );
    // Discard anything queued during setup so we observe only `ZZ`'s command.
    let _ = core.drain_commands();

    type_str(&core, "ZZ");
    expect_close_tab(
        &core,
        "ZZ on an unmodified last pane (a timeout may mean Neovim quit instead)",
    );
    assert_alive_after_settle(&core, &waiter, "ZZ");
    assert!(
        !path.exists(),
        "ZZ on an unmodified buffer must write nothing, as native ZZ does; {} exists again, so \
         ZZ wrote it, as :x does",
        path.display()
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `ZQ` on a MODIFIED sole pane behaves as `:q!` there: it discards the edit, notifies
/// `close_tab` and leaves Neovim running.
#[test]
fn zq_on_last_pane_discards_and_emits_close_tab() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let path = file.path().to_path_buf();
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, ZQ_MARKER);
    // Discard anything queued during setup so we observe only `ZQ`'s command.
    let _ = core.drain_commands();

    type_str(&core, "ZQ");
    expect_close_tab(
        &core,
        "ZQ on a modified last pane (a timeout may mean Neovim quit instead, or ZQ was refused \
         because it did not force)",
    );
    assert_alive_after_settle(&core, &waiter, "ZQ");
    let contents = std::fs::read_to_string(&path).expect("failed to read the temp file back");
    assert_eq!(
        contents, DOC,
        "ZQ must discard the edit and write nothing; a file holding {ZQ_MARKER:?} means ZQ wrote"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// What `request_close_tab()` refusing would show: the toast it reads back is the echo Neovim
/// made inside the request itself, so an empty toast beside `Refused` means that echo is lost.
const ASKED: &str = "request_close_tab() (Refused with no toast means an echo made inside the \
                     RPC request does not reach the toast)";

/// On an unmodified tab, `request_close_tab()` answers `Closing`, and Neovim has notified
/// `close_tab`, which the front-end's drain then acts on.
#[test]
fn request_close_tab_answers_closing_and_notifies_close_tab() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    assert_eq!(
        probe(&core, &["&modified"]),
        ["0"],
        "premise: a freshly opened file must be unmodified"
    );
    // Discard anything queued during setup so we observe only the request's command.
    let _ = core.drain_commands();

    let answer = core.request_close_tab();
    assert_eq!(
        answer,
        CloseTabAnswer::Closing,
        "an unmodified tab must answer Closing; Refused means the check counted an unmodified \
         buffer or the answer is mapped the wrong way round, and NoAnswer that the request failed \
         or wolf_close_tab returned no boolean"
    );
    expect_close_tab(
        &core,
        "request_close_tab() on an unmodified tab (Closing without it means wolf_close_tab \
         answered true without running the quit's check and notify)",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// With the current buffer modified, `request_close_tab()` answers `Refused`, the tab shows
/// E37 alone, and nothing notifies `close_tab`.
#[test]
fn request_close_tab_refuses_with_e37_for_the_current_buffer() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, "x");
    assert_eq!(
        probe(&core, &["&modified"]),
        ["1"],
        "premise: the typed text must leave the current buffer modified"
    );
    // Discard anything queued during setup so we observe only the request's (non-)effect.
    let _ = core.drain_commands();

    let answer = core.request_close_tab();
    assert_eq!(
        answer,
        CloseTabAnswer::Refused,
        "a modified current buffer must refuse the close; Closing means the check was skipped or \
         forced, and NoAnswer that the request failed"
    );
    let (toast, drained) = refused(&core, &waiter, ASKED, E37);
    assert!(
        !toast.contains("E162"),
        "a refusal for the current buffer must be E37 alone; the toast reads {toast:?}, so the \
         current buffer was reported as another buffer"
    );
    assert!(
        !drained.contains(&Command::CloseTab),
        "a refused request must not notify close_tab; got {drained:?}, so the tab would close \
         over its unsaved edit"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// With a modified buffer that no window shows, `request_close_tab()` answers `Refused` and
/// names that buffer with E162: the check covers the whole tab's Neovim, as `:qa` does.
#[test]
fn request_close_tab_refuses_with_e162_for_a_hidden_buffer() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    let (_second, second_name) = hide_a_modified_second_file(&core, &waiter, file.path());
    // Discard anything queued during setup so we observe only the request's (non-)effect.
    let _ = core.drain_commands();

    let answer = core.request_close_tab();
    assert_eq!(
        answer,
        CloseTabAnswer::Refused,
        "a hidden modified buffer must refuse the close; Closing means the check looked at the \
         current buffer or the tab's windows only"
    );
    let (toast, drained) = refused(&core, &waiter, ASKED, &e162(&second_name));
    assert!(
        !toast.contains("E37"),
        "the current buffer is unmodified, so the refusal must be E162 alone; the toast reads \
         {toast:?}"
    );
    assert!(
        !drained.contains(&Command::CloseTab),
        "a refused request must not notify close_tab; got {drained:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `:WolfCloseTab` and `require('wolf').close_tab()` run the same check before they notify:
/// with the current buffer modified, each refuses with E37 and notifies no `close_tab`.
#[test]
fn close_tab_faces_check_before_they_notify() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, "x");
    assert_eq!(
        probe(&core, &["&modified"]),
        ["1"],
        "premise: the typed text must leave the current buffer modified"
    );
    // Discard anything queued during setup so we observe only each face's (non-)effect.
    let _ = core.drain_commands();

    // (what is typed, the face it reaches)
    let faces = [
        ("WolfCloseTab", ":WolfCloseTab"),
        (
            "lua require('wolf').close_tab()",
            "require('wolf').close_tab()",
        ),
    ];
    for (typed, face) in faces {
        ex_command(&core, typed);
        let (_toast, drained) =
            refused(&core, &waiter, &format!("{face} on a modified buffer"), E37);
        assert!(
            !drained.contains(&Command::CloseTab),
            "{face} on a modified buffer must refuse and notify no close_tab; got {drained:?}, \
             so this face still notifies without asking"
        );

        assert_eq!(
            probe(&core, &["&modified"]),
            ["1"],
            "premise: the buffer must still be modified after {face}'s refusal"
        );
        poll_until(
            &waiter,
            &format!(
                "premise: the probe's typed command to clear {face}'s toast, so the next face's \
                 E37 can only be its own"
            ),
            || toast_text(&core),
            String::is_empty,
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// Close Pane on a split closes that split as `<C-w>c` does and asks the front-end for nothing,
/// since a pane that can close never takes the last pane's `:q` path.
#[test]
fn close_pane_closes_a_split_and_notifies_nothing() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    ex_command(&core, "vsplit");
    assert_eq!(
        probe(&core, &["winnr('$')"]),
        ["2"],
        "premise: a typed `:vsplit` must leave the tab two windows"
    );
    // Discard anything queued during setup so we observe only Close Pane's (non-)effect.
    let _ = core.drain_commands();

    core.send_input(InputEvent::ClosePane);
    poll_until(
        &waiter,
        "Close Pane on one of two splits to leave one window; two means it closed nothing",
        || probe(&core, &["winnr('$')"]).remove(0),
        |count| count == "1",
    );
    settle_for(&waiter, SETTLE);
    let drained = core.drain_commands();
    assert!(
        !drained.contains(&Command::CloseTab),
        "Close Pane on a split must close only that split and notify nothing; got {drained:?}. \
         CloseTab means the closable pane also asked the front-end to close the whole tab."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Close Pane on the tab's only, unmodified pane takes `:q`'s path there and notifies
/// `close_tab`, rather than refusing the last pane as `<C-w>c` does.
#[test]
fn close_pane_on_the_last_pane_notifies_close_tab() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);
    assert!(
        core.drain_commands().is_empty(),
        "a freshly attached core must have NO queued commands before Close Pane"
    );

    core.send_input(InputEvent::ClosePane);
    expect_close_tab(
        &core,
        "InputEvent::ClosePane on the sole, unmodified pane (a timeout also means it still runs \
         wolf_do_close, which refuses the last pane silently)",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Close Pane on the tab's last pane, its buffer modified, is refused with E37 as `:q` is, and
/// notifies nothing.
#[test]
fn close_pane_refuses_a_modified_current_buffer_with_e37() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, "x");
    // Discard anything queued during setup so we observe only Close Pane's (non-)effect.
    let _ = core.drain_commands();

    core.send_input(InputEvent::ClosePane);
    let (toast, drained) = refused(&core, &waiter, "Close Pane on the last pane", E37);
    assert!(
        !toast.contains("E162"),
        "a refusal for the current buffer must be E37 alone; the toast reads {toast:?}, so the \
         current buffer was reported as another buffer"
    );
    assert!(
        drained.is_empty(),
        "Close Pane on a MODIFIED last pane must be refused and notify nothing; got {drained:?}. \
         CloseTab means the close skipped the check, as a forced quit does."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Close Pane on the tab's last pane is refused with E162 while a buffer no window shows is
/// modified: its check covers the whole tab's Neovim, as `:q`'s does.
#[test]
fn close_pane_refuses_a_hidden_modified_buffer_with_e162() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    let (_second, second_name) = hide_a_modified_second_file(&core, &waiter, file.path());
    // Discard anything queued during setup so we observe only Close Pane's (non-)effect.
    let _ = core.drain_commands();

    core.send_input(InputEvent::ClosePane);
    let (toast, drained) = refused(
        &core,
        &waiter,
        "Close Pane on the last pane",
        &e162(&second_name),
    );
    assert!(
        !toast.contains("E37"),
        "the current buffer is unmodified, so the refusal must be E162 alone; the toast reads \
         {toast:?}"
    );
    assert!(
        drained.is_empty(),
        "Close Pane beside a hidden modified buffer must be refused and notify nothing; got \
         {drained:?}. CloseTab means the check looked at the current buffer only."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A Neovim killed under the core cannot answer, so `request_close_tab()` answers `NoAnswer`,
/// on which the front-end closes the tab unchecked.
#[test]
fn request_close_tab_gives_no_answer_from_a_dead_neovim() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    ex_command(&core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
    poll_until(
        &waiter,
        "premise: liveness() to become Dead after Neovim's SIGKILL of itself",
        || core.liveness(),
        |liveness| *liveness == Liveness::Dead,
    );

    let answer = core.request_close_tab();
    assert_eq!(
        answer,
        CloseTabAnswer::NoAnswer,
        "a dead Neovim must give NoAnswer, so the front-end closes the tab unchecked; Refused \
         means a failed request reads as a refusal, which would leave a crashed tab unclosable"
    );

    core.shutdown()
        .expect("shutdown should be Ok even after a crash");
}

/// Neovim's message history, `:messages`, on one line. Its probe runs after any request made
/// before it, so an echo that request made is already in the history.
fn messages(core: &WolfCore) -> String {
    probe(
        core,
        &["substitute(execute('messages'), '\\n', ' | ', 'g')"],
    )
    .remove(0)
}

/// Asserts that `history`, read by [`messages`] after `what`, holds neither E37 nor E162.
fn assert_no_refusal(history: &str, what: &str) {
    assert!(
        !history.contains("E37") && !history.contains("E162"),
        "{what} must echo nothing; `:messages` reads {history:?}, so the silent ask echoed a \
         refusal, which Close Window and Quit would show in every unsaved tab, background ones \
         included"
    );
}

/// Drains commands for [`SETTLE`] and asserts that none is `Command::CloseTab`.
fn assert_no_close_tab(core: &WolfCore, waiter: &FlushWaiter, what: &str) {
    settle_for(waiter, SETTLE);
    let drained = core.drain_commands();
    assert!(
        !drained.contains(&Command::CloseTab),
        "{what} must notify no close_tab; got {drained:?}, so the ask ran Close Tab's helper, \
         and the tab would close on its drain before Close Window or Quit decided anything"
    );
}

/// CW1. On an unmodified tab, `request_unsaved()` answers `Clean`, echoes nothing and notifies
/// nothing: it only asks.
#[test]
fn request_unsaved_answers_clean_on_an_unmodified_tab() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    assert_eq!(
        probe(&core, &["&modified"]),
        ["0"],
        "premise: a freshly opened file must be unmodified"
    );
    // Discard anything queued during setup so we observe only the request's (non-)effect.
    let _ = core.drain_commands();

    let answer = core.request_unsaved();
    assert_eq!(
        answer,
        UnsavedAnswer::Clean,
        "an unmodified tab must answer Clean; Unsaved means the check counted an unmodified \
         buffer or the answer is mapped the wrong way round, and NoAnswer that the request \
         failed or wolf_has_unsaved returned no boolean"
    );
    assert_no_refusal(&messages(&core), "request_unsaved() on an unmodified tab");
    assert_no_close_tab(&core, &waiter, "request_unsaved() on an unmodified tab");

    core.shutdown().expect("shutdown should be Ok");
}

/// CW2. With the current buffer modified, `request_unsaved()` answers `Unsaved` and echoes
/// nothing. The same tab's `request_close_tab()` then echoes E37, which shows that an echo made
/// inside an RPC request reaches `:messages`, so the silence is the ask's own.
#[test]
fn request_unsaved_answers_unsaved_without_a_message() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, "x");
    assert_eq!(
        probe(&core, &["&modified"]),
        ["1"],
        "premise: the typed text must leave the current buffer modified"
    );
    // Discard anything queued during setup so we observe only the request's (non-)effect.
    let _ = core.drain_commands();

    let answer = core.request_unsaved();
    assert_eq!(
        answer,
        UnsavedAnswer::Unsaved,
        "a modified current buffer must answer Unsaved; Clean means the check was skipped or \
         found nothing, and NoAnswer that the request failed or returned no boolean"
    );
    assert_no_refusal(&messages(&core), "request_unsaved() on a modified buffer");
    assert_no_close_tab(&core, &waiter, "request_unsaved() on a modified buffer");

    assert_eq!(
        core.request_close_tab(),
        CloseTabAnswer::Refused,
        "premise: Close Tab's check must refuse the same modified buffer"
    );
    let history = messages(&core);
    assert!(
        history.contains(E37),
        "premise: the E37 that request_close_tab() echoes inside its RPC request must reach \
         `:messages`, or the silence asserted above proves nothing; `:messages` reads {history:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// CW3. A modified buffer that no window shows makes `request_unsaved()` answer `Unsaved`: the
/// ask covers the whole tab's Neovim, as `:qa` does.
#[test]
fn request_unsaved_counts_a_hidden_modified_buffer() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    let (_second, _second_name) = hide_a_modified_second_file(&core, &waiter, file.path());
    assert_eq!(
        probe(&core, &["&modified"]),
        ["0"],
        "premise: the current buffer must be unmodified, so only the hidden buffer can answer"
    );
    // Discard anything queued during setup so we observe only the request's (non-)effect.
    let _ = core.drain_commands();

    let answer = core.request_unsaved();
    assert_eq!(
        answer,
        UnsavedAnswer::Unsaved,
        "a hidden modified buffer must answer Unsaved; Clean means the ask looked at the current \
         buffer or the tab's windows only, and NoAnswer that the request failed"
    );
    assert_no_refusal(
        &messages(&core),
        "request_unsaved() beside a hidden modified buffer",
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// CW4. A Neovim killed under the core cannot answer, so `request_unsaved()` answers
/// `NoAnswer`, which the front-ends count as clean.
#[test]
fn request_unsaved_gives_no_answer_from_a_dead_neovim() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    ex_command(&core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
    poll_until(
        &waiter,
        "premise: liveness() to become Dead after Neovim's SIGKILL of itself",
        || core.liveness(),
        |liveness| *liveness == Liveness::Dead,
    );

    let answer = core.request_unsaved();
    assert_eq!(
        answer,
        UnsavedAnswer::NoAnswer,
        "a dead Neovim must give NoAnswer; Clean means a failed request is mistaken for an answer, \
         and Unsaved that it reads as unsaved, which would keep a crashed tab's window and the \
         app from ever closing"
    );

    core.shutdown()
        .expect("shutdown should be Ok even after a crash");
}

/// CW5. With the current buffer modified, `show_unsaved()` answers `true` and the tab shows E37
/// alone, and nothing notifies `close_tab`.
#[test]
fn show_unsaved_shows_e37_and_notifies_nothing() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    insert_text(&core, &waiter, "x");
    assert_eq!(
        probe(&core, &["&modified"]),
        ["1"],
        "premise: the typed text must leave the current buffer modified"
    );
    // Discard anything queued during setup so we observe only the request's (non-)effect.
    let _ = core.drain_commands();

    assert!(
        core.show_unsaved(),
        "show_unsaved() on a modified buffer must answer true; false means it found nothing \
         unsaved or its request failed"
    );
    let (toast, drained) = refused(
        &core,
        &waiter,
        "show_unsaved() (true with no toast means it ran the silent ask rather than the echoing \
         one)",
        E37,
    );
    assert!(
        !toast.contains("E162"),
        "a refusal for the current buffer must be E37 alone; the toast reads {toast:?}, so the \
         current buffer was reported as another buffer"
    );
    assert!(
        !drained.contains(&Command::CloseTab),
        "show_unsaved() must notify no close_tab; got {drained:?}, so it ran Close Tab's helper"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// CW6. On an unmodified tab, `show_unsaved()` answers `false` and echoes nothing.
#[test]
fn show_unsaved_on_an_unmodified_tab_shows_nothing() {
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_initial_window(&core, &waiter);

    assert_eq!(
        probe(&core, &["&modified"]),
        ["0"],
        "premise: a freshly opened file must be unmodified"
    );
    // Discard anything queued during setup so we observe only the request's (non-)effect.
    let _ = core.drain_commands();

    assert!(
        !core.show_unsaved(),
        "show_unsaved() on an unmodified tab must answer false; true means it answers true for \
         any reply, so a buffer written between the two phases would still keep the tab open"
    );
    assert_no_refusal(&messages(&core), "show_unsaved() on an unmodified tab");
    assert_no_close_tab(&core, &waiter, "show_unsaved() on an unmodified tab");

    core.shutdown().expect("shutdown should be Ok");
}

// Pure registry tests: they start no core, so they pin `COMMAND_SPECS` and `Command::from_id`
// even on hosts with no pinned Neovim.

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

/// `close_tab`, the command the quit families notify, is registered under File as "Close Tab",
/// with its `:WolfCloseTab` face.
#[test]
fn close_tab_is_registered_in_the_file_group() {
    let spec = Command::CloseTab.spec();
    assert_eq!(
        (spec.id, spec.ex_command, spec.title, spec.menu),
        ("close_tab", "WolfCloseTab", "Close Tab", MenuGroup::File),
        "Command::CloseTab's registry row must read (id, ex_command, title, menu) as shown; \
         another id means init.lua's wolf_command \"close_tab\" names no command, and another \
         title or menu shows the command misnamed or misplaced"
    );
}

/// `close_pane`, the command Cmd-W and Ctrl+Shift+W run, is registered under File as "Close
/// Pane", with its `:WolfClosePane` face.
#[test]
fn close_pane_is_registered_in_the_file_group() {
    let spec = Command::ClosePane.spec();
    assert_eq!(
        (spec.id, spec.ex_command, spec.title, spec.menu),
        ("close_pane", "WolfClosePane", "Close Pane", MenuGroup::File),
        "Command::ClosePane's registry row must read (id, ex_command, title, menu) as shown; \
         \"Close\" means the menus and the palette still show the old title, and another id or \
         ex_command means the front-ends' and Neovim's names for it no longer match"
    );
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
