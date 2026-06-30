//! Real-nvim end-to-end test of the record-on-open path (plan §D5): when a
//! drained input batch contains `InputEvent::Open { path }`, the api.rs input
//! pump must record that path in the MRU cache.
//!
//! This drives a REAL, hermetic Neovim process (no mocks) and requires
//! `WOLF_NVIM_BIN` (the shared harness fails loudly if it is unset).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wolf_core::{InputEvent, Key, MruStore, WolfCore};

/// Markdown opened by the test.
const DOC: &str = "# mru open test\nbody\n";

/// Recording happens in the input pump and may land slightly AFTER the open's
/// redraw flush (the record and the `edit` RPC are both issued by the pump, but
/// the wake fires from nvim's async redraw). So we poll rather than guess a sleep.
const MRU_POLL_TIMEOUT: Duration = Duration::from_secs(10);
const MRU_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[test]
fn open_records_file_in_mru() {
    // Fail loud if the real nvim binary is missing.
    let _ = common::require_nvim_bin();

    // start_core_with_temp_md opens the file via InputEvent::Open (see the shared
    // harness), which is exactly the path that should record into the MRU.
    let (core, _waiter, file) = common::start_core_with_temp_md(DOC);

    // record() canonicalizes the path, so compare against the canonical form.
    let opened = std::fs::canonicalize(file.path()).expect("failed to canonicalize opened file");

    wait_for_mru_contains(&core, &opened);

    core.shutdown().expect("shutdown should be Ok");
}

/// Polls `core.mru_recent()` until it contains `path`, or panics after a timeout.
fn wait_for_mru_contains(core: &WolfCore, path: &Path) {
    let deadline = Instant::now() + MRU_POLL_TIMEOUT;
    loop {
        let recent = core.mru_recent();
        if recent.iter().any(|e| paths_equal(&e.path, path)) {
            return;
        }
        if Instant::now() >= deadline {
            let listed: Vec<_> = recent.iter().map(|e| e.path.clone()).collect();
            panic!(
                "timed out after {MRU_POLL_TIMEOUT:?} waiting for the MRU to contain {}; \
                 mru_recent() = {listed:?}",
                path.display()
            );
        }
        std::thread::sleep(MRU_POLL_INTERVAL);
    }
}

/// Compares two paths by their canonical form when resolvable, else raw equality.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// Creates a fresh temp dir to serve as a core `cwd`, returning both the guard
/// (kept alive by the caller) and the CANONICAL path. Mirrors
/// `file_search.rs::temp_cwd`: temp dirs can sit behind symlinks (macOS
/// `/tmp -> /private/tmp`) and the MRU store canonicalizes its cwd internally, so
/// passing an already-canonical cwd keeps stored paths and relative `display`
/// strings consistent.
fn temp_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// Starts a `WolfCore` rooted at `cwd` and returns it together with the
/// `FlushWaiter`, so the test can drive a `:cd` via keystrokes and then wait for
/// nvim to process it before exercising the record path. Replicated locally
/// (rather than reaching into `file_search.rs`'s private `start_core_in_with_waiter`)
/// from the same `wake_channel`/`config_in`/`WolfCore::start` parts.
fn start_core_in_with_waiter(cwd: &Path) -> (WolfCore, common::FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(cwd.to_path_buf(), common::DEFAULT_COLS, common::DEFAULT_ROWS),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    // The attach flush (seq 0 -> 1) already landed during WolfCore::start, so use
    // the absolute-threshold initial-flush wait (an advance-based wait would hang).
    waiter.wait_for_initial_flush(&core, common::FLUSH_TIMEOUT);
    (core, waiter)
}

/// True if any entry in `recent` has the given `display` string.
fn has_display(recent: &[wolf_core::FileEntry], display: &str) -> bool {
    recent.iter().any(|e| e.display == display)
}

/// The WRITE path follows nvim's live cwd: start the core in dir A, `:cd` nvim
/// into dir B, then open a file under B via `InputEvent::Open`. The input pump
/// must record that open into B's MRU (`<B>/.wolf/mru.sqlite`), B-relative — NOT
/// into A's store.
///
/// NOTE to coder: this asserts the plan's WRITE-path behavior — the pump records
/// `InputEvent::Open` into `<nvim-cwd>/.wolf` (the tracked cwd, B), and
/// `mru_recent()` reports B's store with B-relative `display`. It depends on the
/// not-yet-written per-cwd MRU + `wolf_cwd` tracking; it passes once that lands.
#[test]
fn open_after_cd_records_in_nvim_cwd_mru() {
    let _ = common::require_nvim_bin();
    let (_dir_a, cwd_a) = temp_cwd();
    let (_dir_b, cwd_b) = temp_cwd();

    // Start nvim in A, retaining the waiter so we can confirm the `:cd` landed.
    let (core, waiter) = start_core_in_with_waiter(&cwd_a);

    // Drive `:cd <B>` one key at a time, mirroring the file_search `:cd` tests.
    let path_b = cwd_b.to_str().expect("temp path B should be valid UTF-8");
    core.send_input(common::ch(':'));
    common::type_str(&core, "cd ");
    common::type_str(&core, path_b);
    core.send_input(common::key(Key::Enter));

    // Wait for nvim to process the `:cd` and flush, so the pump's tracked cwd is B.
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);

    // Create the file under B, then exercise the pump's record path explicitly.
    let written = cwd_b.join("written.md");
    fs::write(&written, "# written\nbody\n").expect("failed to write B/written.md");
    core.send_input(InputEvent::Open {
        path: written.clone(),
    });

    // Poll until the record lands (it may trail the open's redraw flush).
    wait_for_mru_contains(&core, &written);

    // mru_recent() now reports B's store, B-relative ("written.md", not absolute).
    let recent = core.mru_recent();
    assert!(
        has_display(&recent, "written.md"),
        "mru_recent() should report the open B-RELATIVE as \"written.md\"; got {:?}",
        recent.iter().map(|e| e.display.clone()).collect::<Vec<_>>()
    );

    // The row physically lives in <B>/.wolf/mru.sqlite.
    let store_b = MruStore::open(&cwd_b).expect("MruStore::open(B) should succeed");
    assert!(
        has_display(&store_b.recent(), "written.md"),
        "B's store should contain the recorded open as \"written.md\""
    );

    // The pump wrote to B's store, NOT A's.
    let store_a = MruStore::open(&cwd_a).expect("MruStore::open(A) should succeed");
    assert!(
        !store_a.recent().iter().any(|e| paths_equal(&e.path, &written)),
        "the open must be recorded in B's store, NOT A's; A's store leaked it"
    );

    core.shutdown().expect("shutdown should be Ok");
}
