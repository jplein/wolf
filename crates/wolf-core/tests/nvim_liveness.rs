//! Integration tests for LIVENESS detection: the captive Neovim CRASHING
//! (process death) or STALLING (wedged event loop) must flip
//! `WolfCore::liveness()` and fire the front-end wake, so a frozen window stops
//! silently dropping input and can notify + recover.
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks, per the project's real-nvim policy.
//!
//! ## Surface under test
//!
//!   * `WolfCore::liveness(&self) -> wolf_core::Liveness` — a cheap, lock-free
//!     read of the atomic liveness cell (NOT an RPC).
//!   * `wolf_core::Liveness { Alive, NotResponding, Dead }` (re-exported), the
//!     supervisor marking `Dead` on nvim stdout EOF and `block_on_rpc`'s timeout
//!     path escalating repeated `Timeout` into `NotResponding`.
//!   * The input pump's own stall detection: a batch Neovim does not answer within
//!     the RPC budget marks `NotResponding`, and its answer clears it, with no
//!     `block_on_rpc` call involved.
//!   * `wolf_core::liveness::clear_not_responding`, the shared auto-clear, tested
//!     headless with a counting `WakeFn`.
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

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FLUSH_TIMEOUT, POLL_INTERVAL};
use wolf_core::{liveness, Key, Liveness, WakeFn, WolfCore};

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

/// The finite nvim wedge sleep for the PUMP-STALL test (ms). Many multiples of
/// [`STALL_RPC_TIMEOUT_MS`], so the pump's stalled batch outlasts its budget long
/// before nvim recovers, and short enough that nvim recovers within the test's
/// second poll.
const PUMP_WEDGE_SLEEP_MS: u64 = 3_000;

/// How long the pump-stall test waits for the `NotResponding` wake once the state
/// is seen. The wake is fired at the transition, so it is already queued; the window
/// ends long before nvim recovers and flushes, so a recovery frame cannot stand in
/// for it.
const STALL_WAKE_WINDOW: Duration = Duration::from_millis(500);

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
/// polls WITHOUT draining wakes. `desc` names the awaited condition.
///
/// Not `common::poll_until`: that helper drains the waiter between tries, which would
/// consume the transition wake each case asserts afterward with `wait_for_wake`.
fn poll_without_draining(desc: &str, mut pred: impl FnMut() -> bool) {
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
    poll_without_draining("liveness() to become Dead after the SIGKILL", || {
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
    poll_without_draining(
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

/// STALL → recovery AUTO-CLEAR: a transient stall escalates to
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
    poll_without_draining(
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
    poll_without_draining(
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

/// STALL seen by TYPING alone: a key sent into a wedged nvim leaves the input pump
/// waiting on its call, the pump marks `NotResponding` once the batch outlasts the
/// RPC budget, and clears it back to `Alive` when nvim answers.
///
/// No `block_on_rpc` method may be called anywhere in this test: `block_on_rpc`
/// detects stalls and clears them by itself, so one call would make both polls pass
/// without the pump. That rules out `list_buffers`, `current_tabpage` and the probe
/// helpers.
///
/// ## Falsifying edits and their predicted reds
///
/// * Remove the pump's `mark_not_responding` (or run the base code): red at the
///   first poll, which times out at `Alive`.
/// * Remove the pump's `clear_not_responding`: red at the second poll, which times
///   out at `NotResponding`.
#[test]
fn typing_into_a_hung_nvim_marks_not_responding_then_clears() {
    // Read once at start; see `stall_marks_not_responding` for why the process-global var is safe.
    std::env::set_var("WOLF_RPC_TIMEOUT_MS", STALL_RPC_TIMEOUT_MS);

    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "a freshly started core must report Alive"
    );

    ex_command(
        &core,
        &format!("lua (vim.uv or vim.loop).sleep({PUMP_WEDGE_SLEEP_MS})"),
    );
    sleep(WEDGE_SETTLE);

    // The key's call now waits on the parked nvim; the drain attributes the next wake to it.
    waiter.drain();
    core.send_input(key(Key::Esc));

    poll_without_draining(
        "liveness() to become NotResponding while the pump waits on a wedged nvim",
        || core.liveness() == Liveness::NotResponding,
    );
    assert!(
        waiter.wait_for_wake(STALL_WAKE_WINDOW),
        "the pump's NotResponding transition must fire a front-end wake within \
         {STALL_WAKE_WINDOW:?}, so onWake shows the stall overlay; no wake means the state \
         changed silently and the overlay never appears"
    );

    poll_without_draining(
        "liveness() to clear back to Alive once nvim answers the pump's call",
        || core.liveness() == Liveness::Alive,
    );
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "a pump stall must clear to Alive once nvim answers; NotResponding means the overlay \
         stays up over a working editor, Dead means the wedge was taken for a crash"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A `WakeFn` that counts its calls, paired with the counter.
fn counting_wake() -> (WakeFn, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let sink = Arc::clone(&count);
    let wake: WakeFn = Arc::new(move || {
        sink.fetch_add(1, Ordering::SeqCst);
    });
    (wake, count)
}

/// HEADLESS: `clear_not_responding` returns a stall to `Alive` with exactly one
/// wake, and is silent on a cell that is already `Alive`.
///
/// FALSIFYING EDIT: drop the helper's wake. Predicted red: zero wakes at the first
/// count.
#[test]
fn clear_not_responding_returns_a_stall_to_alive_with_one_wake() {
    let state = AtomicU8::new(Liveness::NotResponding.to_u8());
    let (wake, count) = counting_wake();

    liveness::clear_not_responding(&state, &wake);
    assert_eq!(
        liveness::get(&state),
        Liveness::Alive,
        "clear_not_responding must move NotResponding to Alive, or the stall overlay never \
         dismisses itself"
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "NotResponding -> Alive must fire exactly one wake: zero leaves the overlay up until an \
         unrelated wake, more means a transition is reported twice"
    );

    liveness::clear_not_responding(&state, &wake);
    assert_eq!(
        liveness::get(&state),
        Liveness::Alive,
        "clearing an Alive cell must leave it Alive"
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "clearing an already-Alive cell must not wake: the pump clears after every answered \
         batch, so a wake here would fire on every keystroke"
    );
}

/// HEADLESS: `clear_not_responding` never touches `Dead`, so a late answer from a
/// dying nvim cannot hide its crash.
///
/// FALSIFYING EDIT: make the helper an unconditional store of `Alive`. Predicted
/// red: the cell reads `Alive`.
#[test]
fn clear_not_responding_never_resurrects_dead() {
    let state = AtomicU8::new(Liveness::Dead.to_u8());
    let (wake, count) = counting_wake();

    liveness::clear_not_responding(&state, &wake);
    assert_eq!(
        liveness::get(&state),
        Liveness::Dead,
        "clear_not_responding must leave Dead alone: Alive here hides a crash, and the overlay \
         and its restart never appear"
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "a no-op on a Dead cell must not wake"
    );
}
