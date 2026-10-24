//! Regression guard for the looped blocking `tab_cwd` RPC (the sporadic-crash /
//! never-recovers hang).
//!
//! `WolfCore::cwd_for_tabpage(handle)` USED to issue a blocking `tab_cwd`
//! (`nvim_exec_lua "return wolf_tab_cwd(...)"`) RPC on the main thread. When nvim
//! wedged, that call blocked the full RPC timeout (~5s) and — because the
//! state-snapshot walk + Window-menu rebuild re-fired it per tab/per window — it
//! turned an infinite hang into an infinite SEQUENCE of ~5s hangs (bounded but
//! unusable). The fix makes `cwd_for_tabpage` a cheap, lock-free SNAPSHOT READ:
//! nvim pushes every tabpage's cwd out-of-band via the `wolf_tab_cwds`
//! notification into `Snapshot.tab_cwds`, and `cwd_for_tabpage` just reads that
//! map — NO RPC. This test pins that behavior: while nvim is WEDGED,
//! `cwd_for_tabpage` must return PROMPTLY (there is no RPC left to block on),
//! for BOTH the current and a NON-current tabpage (the case the old blocking RPC
//! served).
//!
//! ## Timing / wedge (READ BEFORE TIGHTENING ANYTHING)
//!
//! The wedge is fed through the ordered input pump (an ~8ms coalesce + one
//! `nvim_input`) while nvim executes the queued finite `:lua vim.uv.sleep(..)` on
//! its own main loop, so there is an inherent race between "nvim has actually
//! started sleeping" and "the test times the read". We defend with a wall-clock
//! [`WEDGE_SETTLE`] after feeding the wedge (so nvim is reliably busy) and a wedge
//! sleep MANY times the read's expected latency (so nvim is still busy when the
//! timed reads land). The load-bearing assertion is the BOUND: each read returns
//! in `< PROMPT_BOUND` (>> a cheap snapshot read, << the ~5s the old RPC blocked).
//! If this ever proves flaky the fix is to WIDEN [`WEDGE_SLEEP_MS`] and/or
//! [`PROMPT_BOUND`], NEVER to tighten them — a tighter bound re-introduces the
//! exact race this test exists to pin. The finite `vim.uv.sleep` (NOT `while true
//! do end`) is deliberate so nvim RECOVERS on its own and teardown/`shutdown` does
//! not block. We deliberately do NOT set `WOLF_RPC_TIMEOUT_MS`: there is no RPC to
//! bound here — that is the whole point.
//!
//! ## Dependency on coder work
//!
//! The signature is UNCHANGED (`fn cwd_for_tabpage(&self, handle: i64) ->
//! Option<PathBuf>`); only the impl changes from a blocking RPC to a
//! `Snapshot.tab_cwds` read. Until the coder adds the `tab_cwds` field + the
//! `wolf_tab_cwds` notify handler + rewrites `cwd_for_tabpage`, this file (and the
//! rest of the tabpage suite) will not compile — that tests-first state is
//! expected.

mod common;

use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, WolfCore};

/// Gap between polls while waiting for an out-of-band update to arrive.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The finite nvim wedge sleep, in milliseconds. Long enough that nvim is still
/// parked when the timed reads land; finite so nvim recovers on its own for
/// teardown. Widen (never tighten) this if the test proves flaky.
const WEDGE_SLEEP_MS: u64 = 1500;

/// Wall-clock settle after feeding the wedge so nvim has reliably STARTED the
/// `vim.uv.sleep` (past the input pump's coalesce + apply hop) before we time the
/// reads.
const WEDGE_SETTLE: Duration = Duration::from_millis(200);

/// The regression bound: an out-of-band snapshot read must return FAR below the
/// old ~5s blocking RPC. Deliberately generous — this proves "not a 5s block",
/// not a tight latency target. Widen (never tighten) if flaky.
const PROMPT_BOUND: Duration = Duration::from_millis(500);

/// Polls `pred` until it returns `true`, draining wakes each iteration, or panics
/// after [`FLUSH_TIMEOUT`]. `desc` describes the awaited condition. Mirrors
/// `tabpages.rs::poll_until` (kept local — it is not a shared helper).
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

/// Canonicalizes `p` for a symlink-robust comparison (macOS `/var` ->
/// `/private/var`). `cwd_for_tabpage` is already canonical core-side.
fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).expect("path should exist and canonicalize")
}

/// Types an Ex command `:<cmd><CR>` one key at a time (mirrors
/// `tabpages.rs::ex_command`).
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Waits for the initial editor window after attach, then returns the first
/// tabpage handle (mirrors `tabpages.rs::attach_and_first_tabpage`).
fn attach_and_first_tabpage(core: &WolfCore, waiter: &FlushWaiter) -> i64 {
    poll_until(
        waiter,
        "the initial editor window to appear after attach",
        || !core.read_snapshot().windows.is_empty(),
    );
    let t1 = core.current_tabpage();
    assert_ne!(
        t1, 0,
        "current_tabpage() should be a nonzero handle after attach"
    );
    t1
}

/// Wedges the captive nvim with a FINITE `vim.uv.sleep`, then settles so nvim is
/// reliably parked before the caller times a read. Copies the `rpc_timeout.rs`
/// wedge pattern (`:`, `lua vim.uv.sleep(..)`, `<CR>`, settle). Deliberately does
/// NOT `wait_for_flush`: a flush would only arrive once nvim RECOVERS, and we want
/// to read WHILE it is parked.
fn wedge_nvim(core: &WolfCore) {
    core.send_input(ch(':'));
    type_str(core, &format!("lua vim.uv.sleep({WEDGE_SLEEP_MS})"));
    core.send_input(key(Key::Enter));
    sleep(WEDGE_SETTLE);
}

/// `cwd_for_tabpage` is an out-of-band snapshot read: it returns PROMPTLY (does
/// NOT block on a ~5s RPC) even while nvim is wedged, for BOTH the current tab and
/// a NON-current tab, and still yields each tab's correct tab-local cwd.
#[test]
fn cwd_for_tabpage_is_out_of_band_and_does_not_block_when_nvim_wedged() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let t1 = attach_and_first_tabpage(&core, &waiter);

    let dir_a = tempfile::tempdir().expect("failed to create dir A");
    let dir_b = tempfile::tempdir().expect("failed to create dir B");
    let canon_a = canon(dir_a.path());
    let canon_b = canon(dir_b.path());

    // `:tcd dirA` on T1, then wait for the OUT-OF-BAND map to populate (no RPC).
    // A tempdir path has no spaces, so it can be typed literally on the cmdline.
    let path_a = dir_a
        .path()
        .to_str()
        .expect("dir A path should be valid UTF-8");
    ex_command(&core, &format!("tcd {path_a}"));
    poll_until(
        &waiter,
        "cwd_for_tabpage(T1) to become dir A via the out-of-band snapshot",
        || core.cwd_for_tabpage(t1).as_deref() == Some(canon_a.as_path()),
    );

    // Create T2 rooted at dirB and wait for ITS cwd to arrive out-of-band, then
    // switch back to T1 so T2 is NON-current (the case the old blocking RPC served).
    let t2 = core.create_tabpage(Some(dir_b.path()));
    assert_ne!(t2, 0, "create_tabpage(dirB) should return a nonzero handle");
    poll_until(&waiter, "current_tabpage to become T2", || {
        core.read_snapshot().current_tabpage == t2
    });
    poll_until(
        &waiter,
        "cwd_for_tabpage(T2) to become dir B via the out-of-band snapshot",
        || core.cwd_for_tabpage(t2).as_deref() == Some(canon_b.as_path()),
    );
    core.set_current_tabpage(t1);
    poll_until(&waiter, "current_tabpage to return to T1", || {
        core.read_snapshot().current_tabpage == t1
    });

    // WEDGE nvim. With the old blocking `tab_cwd` RPC each read below would park
    // ~5s; as an out-of-band snapshot read they must return well under PROMPT_BOUND.
    wedge_nvim(&core);

    // Current tab (T1): prompt + correct.
    let start = Instant::now();
    let got_t1 = core.cwd_for_tabpage(t1);
    let elapsed_t1 = start.elapsed();

    // Non-current tab (T2): prompt + correct — the read the old blocking RPC served.
    let start = Instant::now();
    let got_t2 = core.cwd_for_tabpage(t2);
    let elapsed_t2 = start.elapsed();

    assert!(
        elapsed_t1 < PROMPT_BOUND,
        "cwd_for_tabpage(T1) must return within {PROMPT_BOUND:?} while nvim is wedged \
         (out-of-band snapshot read, NOT a ~5s RPC block); took {elapsed_t1:?}"
    );
    assert_eq!(
        got_t1.as_deref(),
        Some(canon_a.as_path()),
        "cwd_for_tabpage(T1) must still report dir A while nvim is wedged"
    );
    assert!(
        elapsed_t2 < PROMPT_BOUND,
        "cwd_for_tabpage(T2) (NON-current) must return within {PROMPT_BOUND:?} while nvim \
         is wedged (out-of-band snapshot read, NOT a ~5s RPC block); took {elapsed_t2:?}"
    );
    assert_eq!(
        got_t2.as_deref(),
        Some(canon_b.as_path()),
        "cwd_for_tabpage(T2) must still report dir B for the NON-current tab while wedged"
    );

    // Record the measured bounds so a passing run still proves the numbers.
    eprintln!(
        "tabpage_cwd: cwd_for_tabpage(T1)={elapsed_t1:?}, cwd_for_tabpage(T2)={elapsed_t2:?} \
         (bound={PROMPT_BOUND:?}, wedge={WEDGE_SLEEP_MS}ms)"
    );

    // Let the whole finite wedge sleep elapse so nvim recovers; a still-wedged shutdown could block.
    // This over-waits on purpose, since WEDGE_SETTLE has already passed.
    sleep(Duration::from_millis(WEDGE_SLEEP_MS));
    core.shutdown().expect("shutdown should be Ok");
    drop((dir_a, dir_b));
}
