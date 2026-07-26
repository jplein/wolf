//! Integration test for the central RPC-timeout chokepoint (effort (a)).
//!
//! This drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks, per the project's real-nvim policy. It
//! reproduces the beach-ball hang deterministically by WEDGING the captive nvim
//! and proves that the interactive synchronous RPCs return a bounded fallback
//! instead of parking the caller forever.
//!
//! The mechanism under test: `WolfCore` resolves `WOLF_RPC_TIMEOUT_MS` once at
//! `WolfCore::start` into a `Duration` field and routes each interactive
//! `runtime.block_on(self.nvim.<rpc>())` through a `block_on_rpc` chokepoint that
//! wraps the future in `tokio::time::timeout`. When nvim is wedged and never
//! replies, the timeout elapses and the method degrades to its documented
//! fallback (`0` for `current_tabpage`).
//!
//! NOTE: `start_file_search` -> `resolve_search_root` is now OUT-OF-BAND — it
//! reads the tracked `live_cwd()` cell instead of issuing a `current_dir` RPC — so
//! it no longer routes through `block_on_rpc` at all. Its promptness while wedged
//! is therefore trivial (there is no RPC to block on), but we still time it below
//! as a cheap regression guard. `current_tabpage` (api.rs:719, a real
//! `block_on_rpc` on `nvim_get_current_tabpage`) is the remaining live
//! demonstration of the RPC-timeout chokepoint in this test.
//!
//! ## Timing / coalescing race (READ BEFORE TIGHTENING ANYTHING)
//!
//! The wedge is delivered through the ordered input pump, which coalesces a burst
//! and applies it asynchronously on a tokio worker (an 8ms settle + one
//! `nvim_input`), while `nvim` executes the queued `:lua vim.uv.sleep(..)` on its
//! own main loop. So there is an inherent race between "nvim has actually started
//! sleeping" and "the test issues the RPC". We defend against it with a wall-clock
//! settle after wedging (so nvim is reliably busy) and a wedge sleep MANY times
//! the RPC timeout (so nvim is still busy when BOTH RPCs land). The load-bearing
//! assertion is the BOUND: each call returns in `< 2s` (>> the 200ms timeout, and
//! << the ~24s real hang the crash report captured).
//!
//! If this ever proves flaky, the fix is to WIDEN the wedge sleep and/or the
//! return bound, NEVER to tighten them: a tighter bound re-introduces the exact
//! race this test exists to pin. The finite `vim.uv.sleep` (NOT `while true do
//! end`) is deliberate so nvim RECOVERS on its own and teardown/`shutdown` do not
//! block.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use wolf_core::{Key, WakeFn};

/// Small, deterministic RPC timeout for the wedged core (milliseconds, as the env
/// var is read as a string). Large enough to clear real sub-ms round-trips, small
/// enough that two timeouts still finish well within [`RETURN_BOUND`].
const RPC_TIMEOUT_MS: &str = "200";

/// How long the test thread waits after feeding the wedge before issuing the
/// bounded RPCs, so nvim has reliably STARTED executing the `vim.uv.sleep` (past
/// the input pump's coalesce + apply hop) and the RPCs land while it is busy.
const WEDGE_SETTLE: Duration = Duration::from_millis(200);

/// Upper bound each bounded RPC must return within while nvim is wedged. Set well
/// above the 200ms timeout (plus a couple of timeouts back-to-back) and far below
/// the multi-second real hang, so it proves the bound without racing it.
const RETURN_BOUND: Duration = Duration::from_secs(2);

/// A no-op wake: the file-search handle is dropped immediately; this test only
/// measures how long `start_file_search` (i.e. `resolve_search_root`) BLOCKS.
fn noop_wake() -> WakeFn {
    Arc::new(|| {})
}

/// The wedged captive nvim does not stall the interactive picker path: both
/// `start_file_search` and `current_tabpage` return promptly inside a finite
/// `vim.uv.sleep`.
///
/// `current_tabpage` (a real `block_on_rpc` on `nvim_get_current_tabpage`) is the
/// live RPC-timeout demonstration: it is asserted to return `< RETURN_BOUND` AND
/// to degrade to its `0` fallback, showing the timeout mapped onto the documented
/// no-op value. `start_file_search` -> `resolve_search_root` is now OUT-OF-BAND (a
/// `live_cwd()` read, NO RPC), so its `< RETURN_BOUND` assertion is trivially
/// satisfied — kept as a cheap regression guard that the picker path never
/// reintroduces a blocking getcwd.
#[test]
fn start_file_search_returns_within_timeout_while_wedged() {
    // Resolve the timeout deterministically BEFORE the core starts (it is read
    // once at WolfCore::start). This is a separate test BINARY, so the env var is
    // isolated to this process.
    std::env::set_var("WOLF_RPC_TIMEOUT_MS", RPC_TIMEOUT_MS);

    let (core, _waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // Wedge the captive nvim with a FINITE block: enter cmdline, run a blocking
    // `vim.uv.sleep` on nvim's main loop, execute with <CR>. The sleep is MANY
    // times the RPC timeout so nvim is still busy when both RPCs below land.
    // Deliberately do NOT wait_for_flush here: we want to fire the RPCs WHILE nvim
    // is parked in the sleep (a flush would only arrive once it recovers).
    core.send_input(common::ch(':'));
    common::type_str(&core, "lua vim.uv.sleep(1500)");
    core.send_input(common::key(Key::Enter));

    // Let nvim actually enter the sleep before issuing the bounded RPCs.
    std::thread::sleep(WEDGE_SETTLE);

    // start_file_search -> resolve_search_root is now OUT-OF-BAND (a live_cwd()
    // read, NO RPC), so this returns promptly regardless of the wedge. Kept as a
    // cheap guard that the picker path never reintroduces a blocking getcwd.
    let start = Instant::now();
    let search = core.start_file_search(noop_wake());
    let elapsed_search = start.elapsed();
    drop(search);

    assert!(
        elapsed_search < RETURN_BOUND,
        "start_file_search must return within {RETURN_BOUND:?} while nvim is wedged \
         (out-of-band live_cwd() read, not a blocking getcwd RPC); took {elapsed_search:?}"
    );

    // Site 672: current_tabpage blocks on nvim_get_current_tabpage; the timeout
    // degrades it to the documented `0` fallback.
    let start = Instant::now();
    let tabpage = core.current_tabpage();
    let elapsed_tabpage = start.elapsed();

    assert!(
        elapsed_tabpage < RETURN_BOUND,
        "current_tabpage must return within {RETURN_BOUND:?} while nvim is wedged; \
         took {elapsed_tabpage:?}"
    );
    assert_eq!(
        tabpage, 0,
        "current_tabpage must degrade to its 0 fallback on RPC timeout while wedged"
    );

    // Record the measured bounds so a passing run still proves the numbers.
    eprintln!(
        "rpc_timeout: start_file_search={elapsed_search:?}, current_tabpage={elapsed_tabpage:?} \
         (timeout={RPC_TIMEOUT_MS}ms, bound={RETURN_BOUND:?})"
    );

    // nvim's finite sleep has (or soon will) elapsed, so it recovers on its own and
    // this shutdown does not block.
    core.shutdown().expect("shutdown should be Ok");
}
