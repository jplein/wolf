//! Integration tests for `wolf-core`'s NATIVE command-line + completion-popup
//! surface (feature plan §2/§3/§4). These drive a REAL, hermetic nvim (no mocks)
//! via `WOLF_NVIM_BIN`; the common helpers panic loudly if it is unset.
//!
//! ## Dependency on coder work (plan §3 + §5)
//!
//! These tests reference NEW `Snapshot` fields + methods the coder adds in
//! `crates/wolf-core/src/render.rs`; the file will not compile until they land —
//! that is expected:
//!
//!   * `Snapshot.cmdline: Cmdline` where
//!     `Cmdline { active: bool, firstc: String, prompt: String,
//!                content: Vec<CmdlineChunk>, pos: usize, indent: usize,
//!                level: u32 }` + `Cmdline::text(&self) -> String` (concatenated
//!     content chunk text).
//!   * `Snapshot.pum: Pum` where
//!     `Pum { visible: bool, items: Vec<PumItem>, selected: i64, row: i64,
//!            col: i64, grid: i64 }` and `PumItem { word, kind, menu, info }`
//!     (all `String`).
//!
//! Attaching with `ext_popupmenu` + `ext_messages` (which implicitly enables
//! `ext_cmdline` and sets `cmdheight=0`) plus `runtime/nvim/init.lua`'s
//! `wildoptions+=pum` is what makes `:` populate `Snapshot.cmdline` and `<Tab>`
//! populate `Snapshot.pum` as a cmdline-anchored vertical popup — all coder work.
//!
//! ## Testing conventions (mirroring `tabpages.rs` / `file_search.rs`)
//!
//! The cmdline / pum state arrives across several redraw flushes, so we POLL with
//! a generous timeout, draining wakes, rather than waiting for a single flush.

mod common;

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, wake_channel, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, WolfCore};

/// Gap between `read_snapshot` polls while waiting for cmdline / pum state to
/// settle.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The command-line-mode prefix Neovim reports in `cmdline_show.firstc` for an
/// Ex command (`:`).
const EX_FIRSTC: &str = ":";

/// The two `Fi`-prefixed files that Neovim's prefix completion must offer.
const FILE_A: &str = "File A.md";
const FILE_B: &str = "File B.md";
/// A third file that does NOT share the `Fi` prefix and therefore must NOT be a
/// completion candidate.
const FILE_OTHER: &str = "Something else.md";

/// The minimum number of popupmenu items expected for the two-way `Fi` completion.
const MIN_PUM_ITEMS: usize = 2;

/// The cmdline text after `e Fi` is typed (Ex `:edit` + a `Fi` prefix). The
/// leading `:` is the `firstc`, not part of the content text.
const CMDLINE_TEXT: &str = "e Fi";

/// Command-line content containing a multi-byte UTF-8 character (`é`, U+00E9, two
/// bytes), so its BYTE length (3) differs from its CHARACTER count (2). Used to
/// pin that `cmdline.pos` is a byte offset, not a char count.
const MULTIBYTE_CONTENT: &str = "eé";

/// The `grid` value Neovim reports on `popupmenu_show` when the popup is anchored
/// to the command line (rather than to a buffer window): `-1`.
const CMDLINE_ANCHORED_GRID: i64 = -1;

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

/// Starts a `WolfCore` rooted at `cwd` (default grid size) and waits for the
/// initial attach/flush so startup never races the test body. Built inline from
/// the same `config_in` / `wake_channel` / `WolfCore::start` parts the common
/// helpers use, because none of them lets a test pin nvim's cwd.
fn start_core_in(cwd: &Path) -> (WolfCore, FlushWaiter) {
    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(
        common::config_in(
            cwd.to_path_buf(),
            common::DEFAULT_COLS,
            common::DEFAULT_ROWS,
        ),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    // The attach flush (seq 0 -> 1) already landed during WolfCore::start; key on
    // the absolute threshold (an advance-based wait would hang).
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}

/// Strips backslash escapes from a completion word so a space-containing filename
/// compares the same whether Neovim reports it raw (`File A.md`) or shell-escaped
/// (`File\ A.md`). Which form the popupmenu item carries is environment-dependent,
/// so the assertions normalize first.
fn deescape(s: &str) -> String {
    s.chars().filter(|&c| c != '\\').collect()
}

/// Native command line + Tab-triggered completion popup, end to end:
///
///   * `:` then `e Fi` populates `Snapshot.cmdline` (active, firstc `:`,
///     `text() == "e Fi"`).
///   * `<Tab>` populates `Snapshot.pum` with the two `Fi`-prefixed files (and NOT
///     the non-matching third file).
///   * `<Esc>` tears BOTH down (cmdline inactive, pum hidden).
#[test]
fn cmdline_completion_popup_lifecycle() {
    // A temp dir seeded with the three files, used as nvim's cwd so `:e Fi`
    // completes against it (no `:cd` needed — the core starts rooted here).
    let dir = tempfile::tempdir().expect("failed to create temp cwd directory");
    for name in [FILE_A, FILE_B, FILE_OTHER] {
        fs::write(dir.path().join(name), "x\n").expect("failed to write seed file");
    }

    let (core, waiter) = start_core_in(dir.path());

    // --- 1. `:` then `e Fi` -> Snapshot.cmdline active with the typed text ------
    core.send_input(ch(':'));
    type_str(&core, CMDLINE_TEXT);

    poll_until(
        &waiter,
        "Snapshot.cmdline to become active with firstc `:` and text `e Fi`",
        || {
            let snap = core.read_snapshot();
            snap.cmdline.active
                && snap.cmdline.firstc == EX_FIRSTC
                && snap.cmdline.text() == CMDLINE_TEXT
        },
    );

    {
        let snap = core.read_snapshot();
        assert!(
            snap.cmdline.active,
            "cmdline should be active while typing `:e Fi`"
        );
        assert_eq!(
            snap.cmdline.firstc, EX_FIRSTC,
            "cmdline firstc should be the Ex `:` prefix"
        );
        assert_eq!(
            snap.cmdline.text(),
            CMDLINE_TEXT,
            "cmdline text() should be the concatenated content `e Fi`"
        );
    }

    // --- 2. <Tab> -> Snapshot.pum visible with the two `Fi` files --------------
    core.send_input(key(Key::Tab));

    poll_until(
        &waiter,
        "Snapshot.pum to become visible with the two `Fi`-prefixed files",
        || {
            let snap = core.read_snapshot();
            if !snap.pum.visible {
                return false;
            }
            let words: Vec<String> = snap.pum.items.iter().map(|it| deescape(&it.word)).collect();
            words.iter().any(|w| w.contains(FILE_A)) && words.iter().any(|w| w.contains(FILE_B))
        },
    );

    {
        let snap = core.read_snapshot();
        assert!(snap.pum.visible, "pum should be visible after <Tab>");
        assert!(
            snap.pum.items.len() >= MIN_PUM_ITEMS,
            "the two-way `Fi` completion should offer at least {MIN_PUM_ITEMS} items; got {}",
            snap.pum.items.len()
        );

        let words: Vec<String> = snap.pum.items.iter().map(|it| deescape(&it.word)).collect();
        assert!(
            words.iter().any(|w| w.contains(FILE_A)),
            "the pum items should include `{FILE_A}` (deescaped); got {words:?}"
        );
        assert!(
            words.iter().any(|w| w.contains(FILE_B)),
            "the pum items should include `{FILE_B}` (deescaped); got {words:?}"
        );
        assert!(
            !words.iter().any(|w| w.contains(FILE_OTHER)),
            "`{FILE_OTHER}` does not share the `Fi` prefix and must NOT be a candidate; got {words:?}"
        );

        // Soft / tolerant: with ext_cmdline active the popup anchors to the command
        // line, which Neovim signals via `popupmenu_show` grid == -1 and a `col`
        // that is a BYTE offset into the cmdline text. The exact value is
        // environment-dependent, so we only sanity-check it when it holds rather
        // than hard-failing (per the plan's "keep this assertion soft" note).
        if snap.pum.grid == CMDLINE_ANCHORED_GRID {
            assert!(
                snap.pum.col >= 0,
                "a cmdline-anchored pum col should be a non-negative byte offset; got {}",
                snap.pum.col
            );
        } else {
            eprintln!(
                "note: pum.grid == {} (expected the cmdline-anchored {CMDLINE_ANCHORED_GRID}); \
                 tolerated as environment-dependent",
                snap.pum.grid
            );
        }
    }

    // --- 3. <Esc> -> cmdline inactive AND pum hidden ---------------------------
    core.send_input(key(Key::Esc));

    poll_until(
        &waiter,
        "<Esc> to deactivate the cmdline and hide the pum",
        || {
            let snap = core.read_snapshot();
            !snap.cmdline.active && !snap.pum.visible
        },
    );

    {
        let snap = core.read_snapshot();
        assert!(
            !snap.cmdline.active,
            "cmdline should be inactive after <Esc>"
        );
        assert!(!snap.pum.visible, "pum should be hidden after <Esc>");
    }

    core.shutdown().expect("shutdown should be Ok");
    drop(dir);
}

/// `cmdline.pos` is a BYTE offset into the content, not a character count.
///
/// Typing content with a multi-byte UTF-8 char (`é`) makes the two differ: after
/// `:eé` the caret sits at the end of a 3-BYTE, 2-CHAR string, so `pos` must be 3
/// (the byte length) — the contract both front-ends rely on to place the caret.
#[test]
fn cmdline_pos_is_a_byte_offset() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // Enter cmdline mode and type the multi-byte content.
    core.send_input(ch(':'));
    type_str(&core, MULTIBYTE_CONTENT);

    // Poll until the cmdline reflects the typed text before reading `pos`.
    poll_until(
        &waiter,
        "the cmdline to reflect the multi-byte content `eé`",
        || {
            let snap = core.read_snapshot();
            snap.cmdline.active && snap.cmdline.text() == MULTIBYTE_CONTENT
        },
    );

    let byte_len = MULTIBYTE_CONTENT.len();
    let char_count = MULTIBYTE_CONTENT.chars().count();
    // Guard: the fixture must genuinely be multi-byte, else the assertion is
    // vacuous (byte length == char count for pure-ASCII content).
    assert_ne!(
        byte_len, char_count,
        "the fixture `{MULTIBYTE_CONTENT}` must have a byte length ({byte_len}) that \
         differs from its char count ({char_count})"
    );

    let snap = core.read_snapshot();
    assert_eq!(
        snap.cmdline.pos, byte_len,
        "cmdline.pos must be the BYTE offset ({byte_len}), not the char count ({char_count})"
    );

    // Tidy up: leave cmdline mode and shut down cleanly.
    core.send_input(key(Key::Esc));
    core.shutdown().expect("shutdown should be Ok");
}
