//! Real-Neovim tests that the key answering a Neovim prompt reaches Neovim while the input
//! pump's call that raised the prompt is still unanswered.
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`), no mocks, per the project's real-nvim policy.
//!
//! ## Premises about Neovim (measured against the pinned 0.12.6, not assumed)
//!
//! - Neovim answers an RPC request only when its handler returns, and a prompt raised inside
//!   the handler holds it open until the user answers.
//! - `InputEvent::Save` (`nvim_command("write")`) on a file whose mtime moved since it was read
//!   raises "Do you really want to write to it (y/n)?" inside the request, always. It shows as an
//!   active command line whose `prompt` carries `(y/n)`.
//! - `InputEvent::SetUiFocus { gained: true }` (`nvim_ui_set_focus`) runs Neovim's `FocusGained`
//!   file check inside the request. On a modified buffer whose file changed, that is W12: a
//!   `confirm` toast naming `W12` and a command-line prompt `[O]K, (L)oad File, ...`. Neovim
//!   throttles that check to once per 2 s and defers a throttled one to its main loop, where keys
//!   always flowed; each test starts its own Neovim, which has made no check to throttle against.
//! - A request sent while Neovim waits on a half-typed chord (`<C-w>`) stays queued until the next
//!   key, and `nvim_get_mode` reports `blocking: true` meanwhile.
//! - `nvim_get_mode` is answered at once while Neovim waits on a key, and in order behind a call
//!   that is merely busy. Neovim runs typeahead before queued requests.
//!
//! Without a pump that keeps input flowing, the answering key waits behind the call that waits on
//! it, so every `poll_until` below for the answer's effect runs out its deadline and panics.
//!
//! Each answer is sent only once its prompt is visible, as a user would.

mod common;

use std::path::Path;
use std::time::Duration;

use common::{ch, ctrl, key, poll_until, probe_values, send_ex, type_str, DiskConflict};
use common::{CONFLICT_DISK, CONFLICT_EDIT, CONFLICT_ORIGINAL, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, Snapshot, WolfCore};

/// The id of Neovim's outer grid, which holds no editor window.
const GLOBAL_GRID: u64 = 1;

/// The gap between the steps of the chord and busy-call cases: long enough for the pump to have
/// applied the step before, and well inside Neovim's default 1000 ms `timeoutlen`.
const STEP_GAP: Duration = Duration::from_millis(100);

/// The gap between the steps of [`typing_behind_a_busy_call_waits_for_it`]. Widen it if the case
/// flakes; never tighten it.
const BUSY_STEP_GAP: Duration = Duration::from_millis(150);

/// What [`refocus_w12_answered_load_past_a_held_paste`] pastes. It shares no word with
/// [`CONFLICT_ORIGINAL`], [`CONFLICT_EDIT`] or [`CONFLICT_DISK`].
const PASTE_TEXT: &str = "PASTED";

/// How long [`typing_behind_a_busy_call_waits_for_it`] keeps Neovim's main loop busy. It must
/// outlast both [`BUSY_STEP_GAP`]s, so the open and the typing arrive while Neovim sleeps.
const BUSY_SLEEP_MS: u64 = 800;

/// What a test reads of the snapshot, as owned values, so no read guard outlives the read.
#[derive(Debug)]
struct Seen {
    cmdline_active: bool,
    cmdline_prompt: String,
    toast_is_prompt: bool,
    toast_text: String,
    row0: String,
}

impl Seen {
    fn read(core: &WolfCore) -> Self {
        let snap = core.read_snapshot();
        Self {
            cmdline_active: snap.cmdline.active,
            cmdline_prompt: snap.cmdline.prompt.clone(),
            toast_is_prompt: snap.toast.is_prompt(),
            toast_text: toast_text(&snap),
            row0: snap.active_grid().row_text(0),
        }
    }

    /// Neovim waits on the user: a confirm toast, or an active command line carrying a prompt.
    fn prompt_showing(&self) -> bool {
        self.toast_is_prompt || (self.cmdline_active && !self.cmdline_prompt.is_empty())
    }

    /// `:write`'s overwrite question is up.
    fn save_prompt_showing(&self) -> bool {
        self.cmdline_active && self.cmdline_prompt.contains("(y/n)")
    }

    /// W12's question is up, as both its confirm toast and its command-line prompt.
    fn w12_showing(&self) -> bool {
        self.toast_is_prompt
            && self.toast_text.contains("W12")
            && self.cmdline_active
            && self.cmdline_prompt.contains("(L)oad File")
    }
}

/// Every toast line's text, joined by newlines.
fn toast_text(snap: &Snapshot) -> String {
    snap.toast
        .lines()
        .iter()
        .map(|line| line.iter().map(|c| c.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The file's text, or the read error's text, so a poll can print either.
fn disk_text(file: &Path) -> String {
    std::fs::read_to_string(file).unwrap_or_else(|e| format!("<unreadable: {e}>"))
}

/// Sends `InputEvent::Save` on the conflicted buffer, and waits for `:write`'s y/n question.
fn save_and_wait_for_prompt(c: &DiskConflict) {
    c.core.send_input(InputEvent::Save);
    poll_until(
        &c.waiter,
        "`:write`'s (y/n) prompt for a file changed on disk",
        || Seen::read(&c.core),
        Seen::save_prompt_showing,
    );
}

/// Refocuses the UI on the conflicted buffer, and waits for W12's question. Sends `false` first,
/// since Neovim fires `FocusGained` only on an unfocused-to-focused edge.
fn refocus_and_wait_for_w12(c: &DiskConflict) {
    c.core.set_ui_focus(false);
    c.core.set_ui_focus(true);
    poll_until(
        &c.waiter,
        "W12's confirm toast and its (L)oad File prompt after the refocus",
        || Seen::read(&c.core),
        Seen::w12_showing,
    );
}

/// Waits for the prompt to go away after its answer was sent.
fn wait_for_no_prompt(c: &DiskConflict, answer: char) {
    poll_until(
        &c.waiter,
        &format!(
            "the prompt to close after `{answer}`; still up means the key never reached Neovim, \
             because it waited behind the call that waits on the prompt"
        ),
        || Seen::read(&c.core),
        |seen| !seen.prompt_showing(),
    );
}

/// `y` to `:write`'s overwrite question reaches Neovim, which writes the buffer over the file.
#[test]
fn save_prompt_answered_yes_writes_the_buffer() {
    let c = common::start_core_with_disk_conflict();
    save_and_wait_for_prompt(&c);

    c.core.send_input(ch('y'));
    let (seen, disk) = poll_until(
        &c.waiter,
        "note.txt on disk to hold the edit and the prompt to close after `y`; a timeout with the \
         prompt up means `y` never reached Neovim",
        || (Seen::read(&c.core), disk_text(&c.file)),
        |(seen, disk)| disk.contains(CONFLICT_EDIT) && !seen.prompt_showing(),
    );
    assert!(
        !disk.contains(CONFLICT_DISK.trim_end()),
        "after `y` the file must hold the buffer, not the outside rewrite; got {disk:?} \
         (last seen {seen:?})"
    );

    let DiskConflict { core, .. } = c;
    core.shutdown().expect("shutdown should be Ok");
}

/// `n` to `:write`'s overwrite question reaches Neovim, which leaves the file alone and keeps the
/// buffer's unsaved edit.
#[test]
fn save_prompt_answered_no_keeps_the_file_on_disk() {
    let c = common::start_core_with_disk_conflict();
    save_and_wait_for_prompt(&c);

    c.core.send_input(ch('n'));
    wait_for_no_prompt(&c, 'n');

    let probe = c.dir.path().join("probe.out");
    let values = probe_values(&c.core, &["&modified", "getline(1)"], &probe);
    assert_eq!(
        values[0], "1",
        "after `n` the buffer must still be modified; 0 means the write went through anyway"
    );
    assert!(
        values[1].contains(CONFLICT_EDIT),
        "after `n` line 1 must keep the unsaved edit; got {:?}",
        values[1]
    );
    assert_eq!(
        disk_text(&c.file),
        CONFLICT_DISK,
        "after `n` the file on disk must keep the outside rewrite; anything else means `n` was \
         taken for a yes, or another key answered the prompt"
    );

    let DiskConflict { core, .. } = c;
    core.shutdown().expect("shutdown should be Ok");
}

/// `L` to W12, raised by a refocus inside `nvim_ui_set_focus`, reaches Neovim, which reloads the
/// file over the unsaved edit.
#[test]
fn refocus_w12_answered_load_reloads_the_file() {
    let c = common::start_core_with_disk_conflict();
    refocus_and_wait_for_w12(&c);

    c.core.send_input(ch('L'));
    let disk_line = CONFLICT_DISK.trim_end();
    let seen = poll_until(
        &c.waiter,
        "row 0 to show the file's rewritten text and the prompt to close after `L`; a timeout \
         with W12 up means `L` never reached Neovim",
        || Seen::read(&c.core),
        |seen| seen.row0.contains(disk_line) && !seen.prompt_showing(),
    );
    assert!(
        !seen.row0.contains(CONFLICT_EDIT),
        "after `L` the unsaved edit must be gone from row 0; got {:?}",
        seen.row0
    );

    let probe = c.dir.path().join("probe.out");
    let values = probe_values(&c.core, &["&modified"], &probe);
    assert_eq!(
        values[0], "0",
        "a buffer reloaded by `L` must be unmodified; 1 means the reload did not replace the edit"
    );

    let DiskConflict { core, .. } = c;
    core.shutdown().expect("shutdown should be Ok");
}

/// `O` to W12 reaches Neovim, which keeps the buffer's unsaved edit and leaves the file alone.
#[test]
fn refocus_w12_answered_ok_keeps_the_edit() {
    let c = common::start_core_with_disk_conflict();
    refocus_and_wait_for_w12(&c);

    c.core.send_input(ch('O'));
    wait_for_no_prompt(&c, 'O');

    let probe = c.dir.path().join("probe.out");
    let values = probe_values(&c.core, &["&modified", "getline(1)"], &probe);
    assert_eq!(
        values[0], "1",
        "after `O` the buffer must still be modified; 0 means it was reloaded or written"
    );
    assert!(
        values[1].contains(CONFLICT_EDIT) && values[1].contains(CONFLICT_ORIGINAL.trim_end()),
        "after `O` line 1 must keep the original text with the unsaved edit; got {:?}",
        values[1]
    );
    assert_eq!(
        disk_text(&c.file),
        CONFLICT_DISK,
        "after `O` the file on disk must keep the outside rewrite; anything else means the buffer \
         was written"
    );

    let DiskConflict { core, .. } = c;
    core.shutdown().expect("shutdown should be Ok");
}

/// `L` to W12, sent right behind a paste, passes the paste held behind `nvim_ui_set_focus` and
/// reaches Neovim, which reloads the file; the paste then applies to the reloaded line.
///
/// `nvim_paste` is not a fast call, so Neovim cannot apply the paste before the key that ends
/// W12's wait. A pump that holds `L` behind the paste deadlocks here.
#[test]
fn refocus_w12_answered_load_past_a_held_paste() {
    let c = common::start_core_with_disk_conflict();
    refocus_and_wait_for_w12(&c);

    c.core.paste(PASTE_TEXT.to_string());
    c.core.send_input(ch('L'));
    let seen = poll_until(
        &c.waiter,
        "the prompt to close after a paste then `L`, and row 0 to show the pasted text; a \
         timeout with W12 up means `L` waited behind the held paste, and one with W12 gone means \
         the paste was lost",
        || Seen::read(&c.core),
        |seen| !seen.prompt_showing() && seen.row0.contains(PASTE_TEXT),
    );

    let probe = c.dir.path().join("probe.out");
    let values = probe_values(&c.core, &["getline(1)"], &probe);
    assert_eq!(
        values[0].replacen(PASTE_TEXT, "", 1),
        CONFLICT_DISK.trim_end(),
        "line 1 less one paste must be the file's rewritten text: `{}` still there means `L` was \
         taken as `O` or never reloaded, and any other difference means the paste did not land \
         once in the reloaded line; got {:?} (last seen {seen:?})",
        CONFLICT_EDIT,
        values[0]
    );
    assert_eq!(
        disk_text(&c.file),
        CONFLICT_DISK,
        "the file on disk must keep the outside rewrite; anything else means the buffer was \
         written"
    );

    let DiskConflict { core, .. } = c;
    core.shutdown().expect("shutdown should be Ok");
}

/// A call sent while Neovim waits on the rest of `<C-w>` stays queued in Neovim, and the `v` that
/// completes the chord must reach Neovim anyway, which splits the window.
#[test]
fn a_key_completes_a_chord_that_holds_a_call() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    poll_until(
        &waiter,
        "the initial editor window grid (id != 1) to appear",
        || {
            let snap = core.read_snapshot();
            !snap.windows.is_empty() && snap.grids.keys().any(|&id| id != GLOBAL_GRID)
        },
        |&ok| ok,
    );
    let base = core.read_snapshot().windows.len();

    core.send_input(ctrl(Key::Char('w')));
    std::thread::sleep(STEP_GAP);
    core.send_input(InputEvent::SetBackground { dark: true });
    std::thread::sleep(STEP_GAP);
    core.send_input(ch('v'));

    poll_until(
        &waiter,
        "a second window after `<C-w>` then `v`; a timeout at the base count means `v` waited \
         behind the background call, which Neovim holds until the chord's next key",
        || core.read_snapshot().windows.len(),
        |&n| n == base + 1,
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Typing sent while a call waits on a busy Neovim, not on input, lands after that call: the
/// open of `b.txt` happens first, and the typing edits `b.txt`.
///
/// An ordering guard, green on a pump that never releases held typing: it fails if held typing
/// is sent to Neovim before the probe says Neovim waits for input, because Neovim runs typeahead
/// before queued requests and the typing would land in `a.txt`.
#[test]
fn typing_behind_a_busy_call_waits_for_it() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize temp dir");
    let a = cwd.join("a.txt");
    let b = cwd.join("b.txt");
    std::fs::write(&a, "alpha line\n").expect("failed to write a.txt");
    std::fs::write(&b, "bravo line\n").expect("failed to write b.txt");

    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(cwd.clone(), common::DEFAULT_COLS, common::DEFAULT_ROWS),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);

    core.send_input(InputEvent::Open { path: a.clone() });
    poll_until(
        &waiter,
        "row 0 to show a.txt",
        || core.read_snapshot().active_grid().row_text(0),
        |row0| row0.contains("alpha line"),
    );

    send_ex(&core, &format!("lua vim.uv.sleep({BUSY_SLEEP_MS})"));
    std::thread::sleep(BUSY_STEP_GAP);
    core.send_input(InputEvent::Open { path: b.clone() });
    std::thread::sleep(BUSY_STEP_GAP);
    core.send_input(ch('i'));
    type_str(&core, "TYPED");
    core.send_input(key(Key::Esc));

    let probe = cwd.join("probe.out");
    let values = probe_values(
        &core,
        &["expand('%:t')", "getline(1)", "getbufline('a.txt', 1)[0]"],
        &probe,
    );
    assert_eq!(
        values[0], "b.txt",
        "the open sent before the typing must have run; a.txt still current means the typing \
         overtook the open (got {values:?})"
    );
    assert!(
        values[1].contains("TYPED"),
        "the typing must land in b.txt, the buffer opened before it; got line 1 {:?}",
        values[1]
    );
    assert_eq!(
        values[2], "alpha line",
        "a.txt must be untouched; TYPED there means the typing reached Neovim before the queued \
         open, i.e. it was released while Neovim was busy rather than waiting on input"
    );

    core.shutdown().expect("shutdown should be Ok");
}
