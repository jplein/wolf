//! Integration test for the `SwapExists` autocmd (effort (b)).
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks, per the project's real-nvim policy.
//!
//! The fix under test: `runtime/nvim/init.lua` registers a `SwapExists` autocmd
//! that sets `vim.v.swapchoice = "e"` (edit anyway), so a stale swap file can
//! never raise the E325 ATTENTION prompt and block the captive editor. Wolf is
//! hermetic and does not manage recovery, so "edit anyway, ignore the stale swap"
//! is the correct policy.
//!
//! ## Provocation is BEST-EFFORT (coverage boundary — read before trusting green)
//!
//! `init.lua` sets `swapfile = false`, which suppresses Neovim's swap check on a
//! normal open — so with the shipped defaults there is nothing to provoke. To
//! actually run the check we:
//!   1. plant a stale swap file next to the fixture, matching Neovim's SAME-DIR
//!      swap naming (`directory` default begins with `.`, meaning "the file's own
//!      directory"): `.<name>.swp`; and
//!   2. re-enable `'swapfile'` in the captive nvim (`:set swapfile`) BEFORE opening
//!      the fixture, so the load-time swap check runs and — absent the autocmd —
//!      would raise E325.
//!
//! Whether a HAND-PLANTED (empty) swap file deterministically triggers `SwapExists`
//! in `--clean` nvim, and whether the re-enable is applied before the open in every
//! scheduling, are the two things this test cannot fully guarantee. It is therefore
//! written so the LOAD-BEARING assertion holds regardless: opening a file with a
//! stale swap present must COMPLETE (the grid reflects the content) and leave NO
//! confirm prompt pending (`!snapshot.toast.is_prompt()`). When the provocation
//! does fire and the autocmd is missing, the E325 prompt makes `is_prompt()` true
//! (or the buffer never loads), and this test FAILS — catching the regression. When
//! the provocation does not fire, the assertion still documents the safe end-state.

mod common;

use std::path::Path;
use std::time::Duration;

use wolf_core::{InputEvent, Key, WolfCore};

/// Fixture document; row 0's heading is asserted to prove the buffer opened.
const FIXTURE_DOC: &str = "# Swap Fixture\nbody line\n";

/// Wall-clock settle after `:set swapfile` so the re-enable is applied on nvim's
/// loop BEFORE the fixture open runs its swap check (the two are sent as separate
/// input bursts; this ordering hop is not otherwise observable via a redraw flush).
const SET_SETTLE: Duration = Duration::from_millis(200);

/// Starts a `WolfCore` rooted at `cwd` and waits for the initial attach flush, so
/// startup never races the test body. Mirrors `file_search.rs`'s local
/// `start_core_in`, built from the shared `common` parts.
fn start_core_in(cwd: &Path) -> (WolfCore, common::FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(
            cwd.to_path_buf(),
            common::DEFAULT_COLS,
            common::DEFAULT_ROWS,
        ),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, common::FLUSH_TIMEOUT);
    (core, waiter)
}

/// A stale swap file next to a fixture must NOT block the open: the file opens and
/// no confirm prompt is pending, because the `SwapExists` autocmd chose "e".
#[test]
fn stale_swap_file_does_not_block_open() {
    let _ = common::require_nvim_bin();

    // Own temp dir so we control BOTH the fixture and its planted swap file.
    // Canonicalized so the path lines up with nvim's own view of it (macOS
    // /tmp -> /private/tmp), like the other real-nvim tests.
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");

    let fixture = cwd.join("note.md");
    std::fs::write(&fixture, FIXTURE_DOC).expect("failed to write fixture");

    // Plant a STALE swap file matching Neovim's same-dir swap naming for `note.md`.
    let swap = cwd.join(".note.md.swp");
    std::fs::write(&swap, b"").expect("failed to write stale swap file");

    let (core, waiter) = start_core_in(&cwd);

    // Re-enable 'swapfile' (init.lua sets it OFF) so opening the fixture runs the
    // swap check. Sent as its OWN burst, then settled, so it lands before the Open.
    core.send_input(common::ch(':'));
    common::type_str(&core, "set swapfile");
    core.send_input(common::key(Key::Enter));
    std::thread::sleep(SET_SETTLE);

    // Open the fixture through the core; with the autocmd in place this completes
    // without a prompt. wait_for_flush returns once nvim emits the redraw for the
    // opened buffer (the ATTENTION prompt, if it ever appeared, also flushes — and
    // then trips the is_prompt() assertion below).
    core.send_input(InputEvent::Open {
        path: fixture.clone(),
    });
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);

    let snap = core.read_snapshot();
    let row0 = snap.active_grid().row_text(0);
    assert!(
        row0.contains("# Swap Fixture"),
        "the fixture must open and render despite the stale swap; got row0 = {row0:?}"
    );
    assert!(
        !snap.toast.is_prompt(),
        "a stale swap must NOT raise a confirm prompt (SwapExists -> swapchoice 'e'); \
         toast kind was {:?}",
        snap.toast.kind()
    );
    drop(snap);

    core.shutdown().expect("shutdown should be Ok");
}
