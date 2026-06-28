//! Real-nvim end-to-end test of the record-on-open path (plan §D5): when a
//! drained input batch contains `InputEvent::Open { path }`, the api.rs input
//! pump must record that path in the MRU cache.
//!
//! This drives a REAL, hermetic Neovim process (no mocks) and requires
//! `WOLF_NVIM_BIN` (the shared harness fails loudly if it is unset).

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use wolf_core::WolfCore;

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
