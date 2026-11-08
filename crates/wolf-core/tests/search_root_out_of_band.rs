//! Regression guard for the blocking `getcwd` RPC on the ⌘O file-picker path
//! (the residual recovery-tail of the sporadic-crash / never-recovers hang).
//!
//! `WolfCore::resolve_search_root` (the root the ⌘O picker searches under) USED
//! to issue a BLOCKING `current_dir` (getcwd) RPC on the main thread. When a
//! window's nvim was wedged, every ⌘O blocked the full RPC timeout (~5s) and
//! piled getcwd requests into nvim's undrained stdin, so recovery had a tail
//! proportional to the wedge length. The fix makes `resolve_search_root` a cheap,
//! out-of-band read of the already-tracked `live_cwd()` (the `tracked_cwd` cell,
//! populated by the `wolf_cwd`/`DirChanged` notification and already canonical),
//! falling back to the configured `cwd` only before nvim first reports — NO RPC.
//! These tests pin that behavior: the picker root comes from `live_cwd()`, so the
//! search opens PROMPTLY even while nvim is wedged, and it FOLLOWS nvim's `:cd`.
//!
//! ## Timing / wedge (READ BEFORE TIGHTENING ANYTHING)
//!
//! The wedge is fed through the ordered input pump (an ~8ms coalesce + one
//! `nvim_input`) while nvim executes the queued finite `:lua vim.uv.sleep(..)` on
//! its own main loop, so there is an inherent race between "nvim has actually
//! started sleeping" and "the test times the call". We defend with a wall-clock
//! [`WEDGE_SETTLE`] after feeding the wedge (so nvim is reliably busy) and a wedge
//! sleep MANY times the call's expected latency (so nvim is still busy when the
//! timed call lands). The load-bearing assertion is the BOUND: `start_file_search`
//! returns in `< PROMPT_BOUND` (>> a cheap out-of-band read, << the ~5s the old
//! RPC blocked). If this ever proves flaky the fix is to WIDEN [`WEDGE_SLEEP_MS`]
//! and/or [`PROMPT_BOUND`], NEVER to tighten them — a tighter bound re-introduces
//! the exact race this test exists to pin. The finite `vim.uv.sleep` (NOT `while
//! true do end`) is deliberate so nvim RECOVERS on its own and teardown/`shutdown`
//! does not block. We deliberately do NOT set `WOLF_RPC_TIMEOUT_MS`: there is no
//! RPC left on this path to bound — that is the whole point.
//!
//! ## Surface under test
//!
//! `fn resolve_search_root(&self) -> PathBuf` is a `live_cwd()` read, not a
//! blocking `current_dir` RPC.

mod common;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{poll_until, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{FileEntry, FileSearch, Key, WolfCore};

/// The finite nvim wedge sleep, in milliseconds. Long enough that nvim is still
/// parked when the timed call lands; finite so nvim recovers on its own for
/// teardown. Widen (never tighten) this if the test proves flaky.
const WEDGE_SLEEP_MS: u64 = 1500;

/// Wall-clock settle after feeding the wedge so nvim has reliably STARTED the
/// `vim.uv.sleep` (past the input pump's coalesce + apply hop) before we time the
/// call.
const WEDGE_SETTLE: Duration = Duration::from_millis(200);

/// The regression bound: the out-of-band `resolve_search_root` must return FAR
/// below the old ~5s blocking RPC. Deliberately generous — this proves "not a 5s
/// block", not a tight latency target. Widen (never tighten) if flaky.
const PROMPT_BOUND: Duration = Duration::from_millis(500);

// --- Helpers ---------------------------------------------------------------

/// Creates a fresh temp dir to serve as a core `cwd` and returns both the guard
/// (kept alive by the caller) and the CANONICAL path to it. Mirrors
/// `file_search.rs::temp_cwd`: canonicalizing up front keeps the traversal root,
/// the relativized `display` strings, AND the `live_cwd()` equality polls
/// consistent — `live_cwd()` is already canonical core-side, and on macOS a
/// tempdir like `/var/folders/...` is a symlink to `/private/var/folders/...`, so
/// the expected side of every `live_cwd()` compare must be the canonical form.
fn temp_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// Writes `contents` to `path`, creating parent directories as needed.
fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create parent dir");
    }
    fs::write(path, contents).expect("failed to write file");
}

/// Starts a `WolfCore` rooted at `cwd` (default grid size), retaining the
/// `FlushWaiter` so the caller can drive a `:cd` and poll `live_cwd()` before
/// searching. Mirrors `file_search.rs::start_core_in_with_waiter`.
fn start_core_in_with_waiter(cwd: &Path) -> (WolfCore, FlushWaiter) {
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
    // The attach flush (seq 0 -> 1) already landed during WolfCore::start, so use
    // the absolute-threshold initial-flush wait (an advance-based wait would hang).
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}

/// Drives `:cd <dir>` one key at a time through the input pump (mirrors
/// `file_search.rs`'s `:cd` keystroke pattern). The tempdir paths here are ASCII
/// and space-free, so the absolute path types literally without `fnameescape`.
fn drive_cd(core: &WolfCore, dir: &Path) {
    let path = dir.to_str().expect("temp path should be valid UTF-8");
    core.send_input(common::ch(':'));
    common::type_str(core, "cd ");
    common::type_str(core, path);
    core.send_input(common::key(Key::Enter));
}

/// Wedges the captive nvim with a FINITE `vim.uv.sleep`, then settles so nvim is
/// reliably parked before the caller times a call. Copies the `tabpage_cwd.rs` /
/// `rpc_timeout.rs` wedge pattern (`:`, `lua vim.uv.sleep(..)`, `<CR>`, settle).
/// Deliberately does NOT `wait_for_flush`: a flush would only arrive once nvim
/// RECOVERS, and we want to call WHILE it is parked.
fn wedge_nvim(core: &WolfCore) {
    core.send_input(common::ch(':'));
    common::type_str(core, &format!("lua vim.uv.sleep({WEDGE_SLEEP_MS})"));
    core.send_input(common::key(Key::Enter));
    sleep(WEDGE_SETTLE);
}

/// Polls `search` until its view reports `complete`, then returns the collected entries.
fn wait_for_search_complete(search: &FileSearch, wakes: &FlushWaiter) -> Vec<FileEntry> {
    poll_until(
        wakes,
        "the file search to reach `complete`",
        || {
            let view = search.read();
            view.complete.then(|| view.entries.to_vec())
        },
        Option::is_some,
    )
    .expect("poll_until returned only once the search was complete")
}

/// The set of `display` strings of `entries` (traversal order is nondeterministic,
/// so tests assert on the SET rather than a sequence).
fn displays(entries: &[FileEntry]) -> HashSet<String> {
    entries.iter().map(|e| e.display.clone()).collect()
}

// --- Tests -----------------------------------------------------------------

/// `start_file_search` -> `resolve_search_root` is an out-of-band `live_cwd()`
/// read: it returns PROMPTLY (does NOT block on a ~5s getcwd RPC) even while nvim
/// is wedged, and roots at the TRACKED cwd (so a sentinel file placed in that dir
/// is found), NOT a stale or fallback dir.
#[test]
fn file_search_root_is_out_of_band_and_prompt_when_wedged() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    // Load-bearing precondition: the picker root is the tracked `live_cwd()`, so drive `:cd` and
    // confirm the out-of-band `wolf_cwd` notify landed.
    let (core, waiter) = start_core_in_with_waiter(&cwd);
    drive_cd(&core, &cwd);
    poll_until(
        &waiter,
        "live_cwd() to become the tempdir via the out-of-band wolf_cwd notify",
        || core.live_cwd().as_deref() == Some(cwd.as_path()),
        |&ok| ok,
    );

    // Seed a sentinel BEFORE searching. If the root is the tracked cwd, it appears.
    write_file(&cwd.join("wedged_sentinel.md"), "s");

    // WEDGE nvim. With the old blocking getcwd RPC, start_file_search would park
    // ~5s here; as an out-of-band `live_cwd()` read it must return under the bound.
    wedge_nvim(&core);

    let (wake, wakes) = common::wake_channel();
    let start = Instant::now();
    let search = core.start_file_search(wake);
    let elapsed = start.elapsed();

    assert!(
        elapsed < PROMPT_BOUND,
        "start_file_search must return within {PROMPT_BOUND:?} while nvim is wedged \
         (out-of-band live_cwd() read, NOT a ~5s getcwd RPC block); took {elapsed:?}"
    );

    // The root is the tracked cwd, so the sentinel is listed, proving the root is not a stale or
    // fallback dir.
    let entries = wait_for_search_complete(&search, &wakes);
    let set = displays(&entries);
    assert!(
        set.contains("wedged_sentinel.md"),
        "the search must root at the tracked live_cwd() (where the sentinel lives); got {set:?}"
    );

    eprintln!("search_root_out_of_band: start_file_search={elapsed:?} (bound={PROMPT_BOUND:?}, wedge={WEDGE_SLEEP_MS}ms)");

    // Let the whole finite wedge sleep elapse so nvim recovers; a still-wedged shutdown could block.
    // This over-waits on purpose, since wait_for_search_complete has likely outlasted it.
    sleep(Duration::from_millis(WEDGE_SLEEP_MS));
    drop(search);
    core.shutdown().expect("shutdown should be Ok");
    drop(_dir);
}

/// The search root FOLLOWS nvim's live cwd out-of-band: start the core in dir A,
/// `:cd` nvim into dir B, poll until `live_cwd()` is B, then search. Results must
/// be rooted at B (`alpha.md` present) and NOT at A (`only_in_a.md` absent).
#[test]
fn file_search_root_follows_cd_out_of_band() {
    let _ = common::require_nvim_bin();
    let (_dir_a, cwd_a) = temp_cwd();
    let (_dir_b, cwd_b) = temp_cwd();

    // A holds a sentinel that must NOT appear once nvim is in B; B holds the file
    // we expect. Both are otherwise empty, so no MRU seed can muddy the assertion.
    write_file(&cwd_a.join("only_in_a.md"), "a");
    write_file(&cwd_b.join("alpha.md"), "b");

    let (core, waiter) = start_core_in_with_waiter(&cwd_a);

    drive_cd(&core, &cwd_b);
    poll_until(
        &waiter,
        "live_cwd() to follow the :cd into dir B via the out-of-band wolf_cwd notify",
        || core.live_cwd().as_deref() == Some(cwd_b.as_path()),
        |&ok| ok,
    );

    let (wake, wakes) = common::wake_channel();
    let search = core.start_file_search(wake);
    let entries = wait_for_search_complete(&search, &wakes);
    let set = displays(&entries);

    assert!(
        set.contains("alpha.md"),
        "search rooted at nvim's live cwd (B) should list alpha.md; got {set:?}"
    );
    assert!(
        !set.contains("only_in_a.md"),
        "search must follow the tracked live_cwd() (B), NOT the configured cwd (A); got {set:?}"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
    drop((_dir_a, _dir_b));
}
