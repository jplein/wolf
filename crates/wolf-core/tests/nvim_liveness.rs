//! Integration tests for LIVENESS detection (architect plan §9 case a): the
//! captive Neovim CRASHING (process death) or STALLING (wedged event loop) must
//! flip `WolfCore::liveness()` and fire the front-end wake, so a frozen window
//! stops silently dropping input and can notify + recover.
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks, per the project's real-nvim policy.
//!
//! ## Dependency on coder work (tests-first)
//!
//! These reference NEW surface the coder adds; the file will not compile until it
//! lands — expected:
//!
//!   * `WolfCore::liveness(&self) -> wolf_core::Liveness` — a cheap, lock-free
//!     read of the atomic liveness cell (NOT an RPC).
//!   * `wolf_core::Liveness { Alive, NotResponding, Dead }` (re-exported), the
//!     supervisor marking `Dead` on nvim stdout EOF and `block_on_rpc`'s timeout
//!     path escalating repeated `Timeout` into `NotResponding`.
//!
//! ## Provocation notes (read before trusting green)
//!
//! Both provocations are fed through the ORDERED INPUT PUMP as `:lua …<CR>`, the
//! same seam the front-end uses, so the tests exercise the real path:
//!
//!   * CRASH: `:lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)<CR>` — a
//!     SIGKILL of nvim's OWN pid. SIGKILL (not `qa!`) simulates a CRASH: the io
//!     loop sees stdout EOF WITHOUT `shutting_down` set, so the supervisor reports
//!     `Dead` (a clean `qa!` would be suppressed).
//!   * STALL: `:lua (vim.uv or vim.loop).sleep(<ms>)<CR>` wedges nvim's main loop
//!     so a subsequent synchronous `block_on_rpc` call elapses to `Timeout`, which
//!     escalates to `NotResponding`. The wedge is finite and TEARDOWN force-kills
//!     the wedged child (the `shutdown` reap: bounded `child.wait()` + SIGKILL),
//!     so nothing hangs for the full sleep.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FLUSH_TIMEOUT};
use wolf_core::{Key, Liveness, WolfCore};

/// Gap between liveness polls.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// A small, deterministic RPC timeout (ms) for the STALL core, so a single wedged
/// `block_on_rpc` call elapses quickly. Large enough to clear real sub-ms
/// round-trips.
const STALL_RPC_TIMEOUT_MS: &str = "200";

/// The finite nvim wedge sleep for the STALL test (ms). Long enough that nvim is
/// still parked across the escalation poll below; teardown force-kills it, so this
/// does NOT delay shutdown. Widen (never tighten) if flaky.
const WEDGE_SLEEP_MS: u64 = 60_000;

/// The finite nvim wedge sleep for the AUTO-CLEAR test (ms). Unlike
/// [`WEDGE_SLEEP_MS`] this is SHORT — a handful of multiples of the RPC timeout —
/// so a `block_on_rpc` call issued DURING it times out and escalates to
/// `NotResponding`, yet nvim RECOVERS on its own well within the test, letting a
/// later successful `block_on_rpc` auto-clear back to `Alive`. Widen (never
/// tighten) if flaky. Matches the proven `rpc_timeout.rs` wedge duration.
const RECOVERY_WEDGE_SLEEP_MS: u64 = 1_500;

/// Wall-clock settle after feeding the wedge so nvim has reliably STARTED the
/// `vim.uv.sleep` (past the input pump's coalesce + apply hop) before the timed
/// RPC lands.
const WEDGE_SETTLE: Duration = Duration::from_millis(200);

/// Types an Ex command `:<cmd><CR>` one key at a time (the front-end input seam).
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Feeds the captive nvim a SIGKILL of its own pid via the input pump — a crash,
/// distinct from a clean `qa!`.
fn self_sigkill(core: &WolfCore) {
    ex_command(core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
}

/// Polls `pred` until `true` or panics after [`FLUSH_TIMEOUT`], SLEEPING between
/// polls WITHOUT draining wakes (so a queued liveness wake survives to be observed
/// afterward). `desc` names the awaited condition.
fn poll_until_no_drain(desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if pred() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
        }
        sleep(POLL_INTERVAL);
    }
}

/// CRASH → `Dead`: a SIGKILL of the captive nvim (an unexpected death, NOT a clean
/// `qa!`) must flip `liveness()` to `Dead` and fire the front-end wake so the
/// frozen window is signalled to notify + restart.
#[test]
fn crash_marks_dead() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "a freshly started core must report Alive"
    );

    // Clear the startup/attach wake backlog so the wake we assert below belongs to
    // the crash episode.
    waiter.drain();

    // Kill nvim's own process (SIGKILL) through the input pump.
    self_sigkill(&core);

    // DETECTION (load-bearing): the supervisor observes stdout EOF and marks Dead.
    // Poll WITHOUT draining so the death wake stays queued for the seam assertion.
    poll_until_no_drain("liveness() to become Dead after the SIGKILL", || {
        core.liveness() == Liveness::Dead
    });
    assert_eq!(
        core.liveness(),
        Liveness::Dead,
        "an unexpected nvim death (SIGKILL) must mark liveness Dead"
    );

    // The crash must wake the front-end so `onWake` can re-read `liveness()`. The wake does not bump
    // `seq`, so it is observed via the raw-wake seam, not a flush.
    assert!(
        waiter.wait_for_wake(FLUSH_TIMEOUT),
        "the crash must fire a front-end wake so onWake can react"
    );

    // Shutdown of an already-dead core reaps the dead child and returns Ok.
    core.shutdown()
        .expect("shutdown should be Ok even after a crash");
}

/// STALL → `NotResponding`: a wedged nvim event loop makes a synchronous
/// `block_on_rpc` call elapse to `Timeout`, which escalates `liveness()` to
/// `NotResponding` (NO auto-kill — the front-end drives a manual restart).
#[test]
fn stall_marks_not_responding() {
    // Set before the core starts, which reads it once. `crash_marks_dead` in this binary tolerates
    // a short timeout, so the process-global env var is harmless there.
    std::env::set_var("WOLF_RPC_TIMEOUT_MS", STALL_RPC_TIMEOUT_MS);

    let (core, _waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "a freshly started core must report Alive"
    );

    // Wedge nvim with a long finite sleep. Do not wait_for_flush: the RPCs below must land while
    // nvim is parked, and a flush only arrives once it recovers.
    ex_command(
        &core,
        &format!("lua (vim.uv or vim.loop).sleep({WEDGE_SLEEP_MS})"),
    );
    sleep(WEDGE_SETTLE);

    // Each `list_buffers()` (a block_on_rpc getbufinfo, empty on timeout) times out; repeating it
    // escalates whatever the threshold is, and the finite wedge outlasts the poll window.
    poll_until_no_drain(
        "liveness() to escalate to NotResponding while wedged",
        || {
            let _ = core.list_buffers();
            core.liveness() == Liveness::NotResponding
        },
    );
    assert_eq!(
        core.liveness(),
        Liveness::NotResponding,
        "a wedged nvim (repeated RPC Timeout) must mark liveness NotResponding, not Dead"
    );

    // Teardown must NOT block for the remaining wedge: `shutdown` reaps via a
    // bounded `child.wait()` + SIGKILL of the wedged child.
    core.shutdown()
        .expect("shutdown should force-kill the wedged child and return Ok");
}

/// STALL → recovery AUTO-CLEAR (RESOLVED §11): a transient stall escalates to
/// `NotResponding`, but once nvim answers again a SUCCESSFUL `block_on_rpc` call
/// compare-exchanges `NotResponding -> Alive` (never touching `Dead`) and fires a
/// wake, so the overlay self-dismisses without a manual restart.
#[test]
fn stall_then_recovery_clears_not_responding() {
    // Short RPC timeout, read once at start (see `stall_marks_not_responding` for
    // why the process-global env var is harmless to the sibling tests).
    std::env::set_var("WOLF_RPC_TIMEOUT_MS", STALL_RPC_TIMEOUT_MS);

    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "a freshly started core must report Alive"
    );

    // Wedge nvim with a SHORT finite sleep so it recovers on its own. Do NOT
    // wait_for_flush (a flush only lands once it recovers).
    ex_command(
        &core,
        &format!("lua (vim.uv or vim.loop).sleep({RECOVERY_WEDGE_SLEEP_MS})"),
    );
    sleep(WEDGE_SETTLE);

    // During the sleep a block_on_rpc call times out and escalates to NotResponding, well before
    // nvim recovers, since the wedge is several multiples of the timeout.
    poll_until_no_drain(
        "liveness() to escalate to NotResponding during the transient stall",
        || {
            let _ = core.list_buffers();
            core.liveness() == Liveness::NotResponding
        },
    );

    // Clear the pre-recovery wake backlog so the wake asserted below is attributed
    // to the recovery episode.
    waiter.drain();

    // After the sleep, a successful block_on_rpc auto-clears to Alive: the load-bearing assertion.
    // No drain, so the auto-clear wake stays queued for the seam assertion.
    poll_until_no_drain(
        "liveness() to auto-clear back to Alive after recovery",
        || {
            let _ = core.list_buffers();
            core.liveness() == Liveness::Alive
        },
    );
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "a transient stall must AUTO-CLEAR to Alive once nvim answers (never stuck NotResponding)"
    );

    // SEAM: the auto-clear transition (NotResponding -> Alive) fired a front-end
    // wake so the overlay can self-dismiss.
    assert!(
        waiter.wait_for_wake(FLUSH_TIMEOUT),
        "the auto-clear must fire a front-end wake so onWake can dismiss the overlay"
    );

    // nvim recovered on its own, so this shutdown does not block.
    core.shutdown().expect("shutdown should be Ok");
}
