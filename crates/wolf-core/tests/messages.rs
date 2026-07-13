//! Integration tests for `wolf-core`'s MESSAGE-TOAST surface (feature plan
//! §2/§3/§4): Neovim's `ext_messages` `msg_show` / `msg_history_show` folded into
//! `Snapshot.toast`, with `search_count` suppressed and the toast cleared on the
//! next user input (OD8). These drive a REAL, hermetic nvim (no mocks) via
//! `WOLF_NVIM_BIN`; the common helpers panic loudly if it is unset.
//!
//! ## Dependency on coder work (plan §3 + §5)
//!
//! These tests reference a NEW `Snapshot.toast` field + methods the coder adds in
//! `crates/wolf-core/src/render.rs`; the file will not compile until they land —
//! that is expected:
//!
//!   * `Snapshot.toast: Toast` where `Toast { visible: bool, messages:
//!     Vec<Message> }`, `Message { kind: String, id: i64, lines:
//!     Vec<Vec<ToastChunk>> }`, and `ToastChunk { text: String, attr_id: u64 }`.
//!   * `Toast::lines(&self) -> Vec<Vec<ToastChunk>>` (all messages' lines flattened
//!     in order) and `Toast::kind(&self) -> String` (last message's kind, or "").
//!   * The `api.rs` input pump clears the toast on any `Key` / `Mouse` / `Scroll`
//!     input (gated by `!toast.is_prompt()`), BEFORE applying that input.
//!
//! Attaching with `ext_messages` (which routes messages off the grid) is what makes
//! `nvim_echo` / `:echomsg` / `:messages` populate `Snapshot.toast` — coder work.
//!
//! ## Testing conventions (mirroring `theme.rs` / `tabpages.rs`)
//!
//! Message state arrives across redraw flushes, so we POLL with a generous timeout,
//! draining wakes, rather than waiting for a single flush.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, WolfCore};

/// Gap between `read_snapshot` polls while waiting for toast state to settle.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long to let message activity settle before asserting a NEGATIVE property
/// (e.g. that `search_count` never surfaced). Generous so a late `msg_show`
/// cannot sneak past the assertion.
const SETTLE: Duration = Duration::from_secs(2);

/// The message `kind` Neovim reports for the search-count indicator (`[1/3]`),
/// which the toast fold MUST suppress.
const SEARCH_COUNT_KIND: &str = "search_count";

/// The `kind` requested for the multi-line `nvim_echo` in the first test.
const LIST_CMD_KIND: &str = "list_cmd";

/// The message `kind` Neovim reports for a `confirm()` / `:confirm` dialog under
/// `ext_messages` (documented in `:help ui-messages`). `Toast::is_prompt()` keys
/// on this kind to keep the toast from being dismissed by a passing input.
const CONFIRM_KIND: &str = "confirm";

/// Types `:{cmd}<CR>` as keystrokes: `:` enters command-line mode, `cmd` is typed
/// literally, Enter executes it. `cmd` must NOT include the leading `:`.
fn send_ex(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Flattens the current toast into one newline-joined string: each line is the
/// concatenation of its chunk texts, and lines are joined with `\n`. Used for
/// substring assertions on the visible message content.
fn toast_text(core: &WolfCore) -> String {
    let snap = core.read_snapshot();
    snap.toast
        .lines()
        .iter()
        .map(|line| line.iter().map(|c| c.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Polls `pred` until it returns `true`, draining wakes each iteration, or panics
/// after [`FLUSH_TIMEOUT`]. `desc` describes the awaited condition for the panic
/// message. Mirrors `tabpages.rs`'s `poll_until`.
fn poll_until(waiter: &FlushWaiter, desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if pred() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
        }
        waiter.drain();
        sleep(POLL_INTERVAL);
    }
}

/// A multi-line `nvim_echo` with an explicit `kind` populates `Snapshot.toast`:
/// `visible == true`, `kind()` reflects the requested kind, and the flattened
/// `lines()` contain BOTH echoed lines.
#[test]
fn multiline_echo_populates_toast_with_kind() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // Drive a two-line message via nvim_echo with a `kind`. The `\n` inside the
    // Lua string literal (typed as the two chars `\` `n`) becomes a real newline
    // that the fold splits into two toast lines. Typed as an Ex `:lua` command.
    send_ex(
        &core,
        r#"lua vim.api.nvim_echo({{"line one\n"},{"line two"}}, false, { kind = "list_cmd" })"#,
    );

    poll_until(
        &waiter,
        "the two-line nvim_echo to surface in Snapshot.toast",
        || {
            let snap = core.read_snapshot();
            if !snap.toast.visible {
                return false;
            }
            let text = toast_text(&core);
            text.contains("line one") && text.contains("line two")
        },
    );

    let text = toast_text(&core);
    assert!(
        text.contains("line one") && text.contains("line two"),
        "the toast should carry BOTH echoed lines; got {text:?}"
    );

    let snap = core.read_snapshot();
    assert_eq!(
        snap.toast.kind(),
        LIST_CMD_KIND,
        "the toast kind() should reflect the requested `{LIST_CMD_KIND}` kind"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `:messages` populates the toast from message HISTORY (`msg_history_show`): an
/// earlier `:echomsg` is recalled into `Snapshot.toast` when `:messages` runs.
#[test]
fn messages_command_populates_toast_from_history() {
    const MARKER: &str = "history marker alpha";

    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // `:echomsg` both shows the message AND records it in history.
    send_ex(&core, &format!("echomsg \"{MARKER}\""));
    poll_until(&waiter, "the :echomsg to surface in the toast", || {
        let snap = core.read_snapshot();
        snap.toast.visible && toast_text(&core).contains(MARKER)
    });

    // `:messages` emits `msg_history_show`. Typing `:` first is itself a dismissing
    // key (it clears the live toast), so a match here proves the toast was
    // REPOPULATED from history, not merely still showing the earlier echomsg.
    send_ex(&core, "messages");
    poll_until(
        &waiter,
        "`:messages` to repopulate the toast from history",
        || {
            let snap = core.read_snapshot();
            snap.toast.visible && toast_text(&core).contains(MARKER)
        },
    );

    assert!(
        toast_text(&core).contains(MARKER),
        "`:messages` should show the historical `{MARKER}` message; got {:?}",
        toast_text(&core)
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The toast is cleared on the next user input (OD8): once a message is visible, a
/// plain movement key dismisses it (`Snapshot.toast.visible` -> false).
#[test]
fn toast_clears_on_next_input() {
    const MARKER: &str = "dismiss me please";

    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    send_ex(&core, &format!("echomsg \"{MARKER}\""));
    poll_until(&waiter, "the message to become visible before dismissing", || {
        let snap = core.read_snapshot();
        snap.toast.visible && toast_text(&core).contains(MARKER)
    });

    // A movement key is a dismissing `Key` input; the input pump clears the toast
    // before applying it (and nothing new repopulates), so `visible` goes false.
    core.send_input(key(Key::Left));
    poll_until(&waiter, "the toast to clear on the next input", || {
        !core.read_snapshot().toast.visible
    });

    assert!(
        !core.read_snapshot().toast.visible,
        "the toast should be cleared after a movement key"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Consecutive user-initiated messages do NOT accumulate: because typing `:` for
/// the second command is itself a dismissing key that clears the live toast BEFORE
/// the new message lands, the toast reflects only the LATEST message — the
/// non-accumulation behavior the plan's `replace_last` handling targets.
///
/// NOTE (test-writer): a true `msg_show.replace_last` (same-keystroke in-place
/// replacement) cannot be reliably triggered from real headless nvim via the
/// keystroke-only driving surface, so this asserts the equivalent observable
/// contract — the clear-on-input path yields a replaced, non-accumulated toast.
#[test]
fn consecutive_messages_do_not_accumulate() {
    const FIRST: &str = "first message zzz";
    const SECOND: &str = "second message yyy";

    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    send_ex(&core, &format!("echomsg \"{FIRST}\""));
    poll_until(&waiter, "the first message to become visible", || {
        let snap = core.read_snapshot();
        snap.toast.visible && toast_text(&core).contains(FIRST)
    });

    send_ex(&core, &format!("echomsg \"{SECOND}\""));
    poll_until(
        &waiter,
        "the toast to show the second message without the first",
        || {
            let snap = core.read_snapshot();
            let text = toast_text(&core);
            snap.toast.visible && text.contains(SECOND) && !text.contains(FIRST)
        },
    );

    let text = toast_text(&core);
    assert!(
        text.contains(SECOND),
        "the toast should reflect the latest message; got {text:?}"
    );
    assert!(
        !text.contains(FIRST),
        "the earlier message must NOT accumulate in the toast; got {text:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A real `confirm` prompt is NOT dismissed by a passing input (OD8 gate). The
/// input pump clears the toast on the next Key/Mouse/Scroll UNLESS
/// `toast.is_prompt()` (last kind == `"confirm"`); this drives a live
/// `confirm()` dialog, sends a non-answering key, and asserts the prompt toast
/// SURVIVES — proving the `!is_prompt()` gate prevented the dismissal.
///
/// NOTE (test-writer): this drives a REAL nvim `confirm()` (per CLAUDE.md's
/// prefer-real-nvim policy), which emits `msg_show` kind `"confirm"` and then
/// blocks reading a char (our async-sent keys still reach it). The benign key
/// (`z`) is not a valid accelerator, so nvim keeps prompting; we then press
/// `<Esc>` to abort the dialog so the process is not left wedged.
#[test]
fn confirm_prompt_is_not_dismissed_by_input() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // Drive a confirm() dialog: it surfaces as a `msg_show` kind "confirm" and
    // then blocks awaiting an answer key.
    send_ex(&core, r#"call confirm("Really do it?", "&yes\n&no")"#);

    poll_until(
        &waiter,
        "the confirm() dialog to surface as a prompt toast (kind `confirm`)",
        || {
            let snap = core.read_snapshot();
            snap.toast.visible && snap.toast.is_prompt()
        },
    );
    {
        let snap = core.read_snapshot();
        assert!(snap.toast.is_prompt(), "the confirm dialog should be a prompt toast");
        assert_eq!(
            snap.toast.kind(),
            CONFIRM_KIND,
            "the prompt toast kind() should be `{CONFIRM_KIND}`"
        );
    }

    // Send a benign, NON-answering key. Were the pump to dismiss on this input,
    // the toast would clear; the `!is_prompt()` gate must prevent that.
    core.send_input(ch('z'));

    // Let the pump process the key (and nvim keep prompting), then assert the
    // prompt toast is STILL visible.
    sleep(SETTLE);
    waiter.drain();
    {
        let snap = core.read_snapshot();
        assert!(
            snap.toast.visible,
            "a confirm prompt must survive a passing input (the is_prompt gate)"
        );
        assert!(
            snap.toast.is_prompt(),
            "the surviving toast should still be the confirm prompt"
        );
    }

    // Abort the dialog so nvim unblocks for a clean shutdown.
    core.send_input(key(Key::Esc));
    core.shutdown().expect("shutdown should be Ok");
}

/// A `search_count` message (`[1/3]`) is SUPPRESSED — it never surfaces in the
/// toast. Tolerant by design: if the headless nvim does not emit `search_count`
/// at all, the negative assertion still holds (the point is that the toast is
/// NEVER driven by a search-count kind).
#[test]
fn search_count_is_suppressed() {
    // Content with repeated matches so `/alpha` yields a "[1/3]"-style count. The
    // temp file guard is kept alive for the test's duration.
    let (core, waiter, _file) =
        common::start_core_with_temp_md("alpha beta\nalpha beta\nalpha beta\n");

    // Ensure the search-count indicator is enabled (it rides `shortmess` flag S).
    send_ex(&core, "set shortmess-=S");

    // Run a search that has multiple matches: `/alpha<CR>`. If nvim emits a
    // search-count message it will carry kind `search_count`, which the fold drops.
    core.send_input(ch('/'));
    type_str(&core, "alpha");
    core.send_input(key(Key::Enter));

    // Let any message activity settle, then assert the toast was never driven by a
    // search-count kind (and does not display a `/3]`-style count).
    sleep(SETTLE);
    // Drain any wakes accumulated during the settle so the channel does not back up.
    waiter.drain();

    let snap = core.read_snapshot();
    assert_ne!(
        snap.toast.kind(),
        SEARCH_COUNT_KIND,
        "a `{SEARCH_COUNT_KIND}` message must never surface as a toast kind"
    );
    let text = toast_text(&core);
    assert!(
        !text.contains("/3]"),
        "the suppressed search count `[1/3]` must not appear in the toast; got {text:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}
