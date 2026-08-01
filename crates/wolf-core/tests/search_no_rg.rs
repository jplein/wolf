//! Graceful-degrade integration test for the ripgrep-backed content search when
//! the pinned `rg` binary is ABSENT (`WOLF_RG_BIN` unset).
//!
//! [`WolfCore::start_search`] locates the pinned `rg` via the `WOLF_RG_BIN` env
//! var. When it is unset/empty the search must degrade GRACEFULLY — returning an
//! immediately-`complete` [`SearchSession`] whose snapshot carries
//! `error == Some("WOLF_RG_BIN unset")` and NO matches — so the front-end shows
//! an empty/error state instead of hanging or crashing.
//!
//! ## Why this lives in its OWN dedicated Bazel target/binary
//!
//! Rust runs a target's `#[test]` fns on PARALLEL threads inside ONE process.
//! Mutating a process-global env var (`remove_var("WOLF_RG_BIN")`) inside a test
//! races every OTHER test in that binary that reads it (e.g. the real-rg
//! `tests/search.rs`, which resolves `WOLF_RG_BIN` to spawn rg). Isolating this
//! single test in its own binary — where `WOLF_RG_BIN` is simply NEVER staged and
//! is defensively removed at the top — sidesteps that race entirely: this process
//! owns its environment.
//!
//! This test still needs a REAL captive Neovim (a [`WolfCore`] cannot start
//! without one), located via `WOLF_NVIM_BIN` exactly like the sibling real-nvim
//! suite. It deliberately does NOT stage `//bazel/rg:rg_tree` and does NOT wire
//! `WOLF_RG_BIN` — the whole point is that rg is absent.

mod common;

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use wolf_core::{SearchOptions, SearchSession, WakeFn, WolfCore};

/// A `WakeFn` that signals on a channel, paired with the receiver — so the test
/// can also assert that `start_search` fired the wake on the degrade path
/// (mirrors `tests/search.rs::search_wake`).
fn search_wake() -> (WakeFn, Receiver<()>) {
    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let wake: WakeFn = Arc::new(move || {
        let _ = tx.send(());
    });
    (wake, rx)
}

/// Starts a `WolfCore` rooted at `cwd` and waits for the initial attach/flush so
/// startup never races the test body (mirrors `tests/search.rs::start_core_in`).
fn start_core_in(cwd: &Path) -> WolfCore {
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
    core
}

/// With `WOLF_RG_BIN` unset, `start_search` returns an immediately-`complete`
/// session carrying `error == Some("WOLF_RG_BIN unset")` and no matches, and
/// fires the wake — the graceful-degrade contract (§4 / review note N1).
#[test]
fn start_search_without_rg_degrades_gracefully() {
    // Defensively clear WOLF_RG_BIN BEFORE starting core. This binary owns its
    // process (no sibling test reads the var here), so mutating the env is safe.
    std::env::remove_var("WOLF_RG_BIN");

    // Still needs a real captive nvim (resolved + re-exported as an absolute path).
    let _ = common::require_nvim_bin();
    let cwd = common::fresh_cwd();

    let core = start_core_in(&cwd);

    let (wake, wakes) = search_wake();
    let session: SearchSession = core.start_search(
        SearchOptions {
            query: "anything".to_string(),
            use_regex: true,
            case_sensitive: false,
            all_files: false,
        },
        wake,
    );

    // The degrade path builds an errored state and fires the wake synchronously
    // before returning, so the very first read already observes the final state.
    let view = session.read();
    assert!(
        view.complete,
        "an rg-less search should be immediately complete (got complete={})",
        view.complete
    );
    assert!(
        view.matches.is_empty(),
        "an rg-less search should surface no matches (got {} matches)",
        view.matches.len()
    );
    assert_eq!(
        view.error.as_deref(),
        Some("WOLF_RG_BIN unset"),
        "the snapshot should carry the exact graceful-degrade error string"
    );
    drop(view);

    // The wake fired so a front-end that registered it repaints into the error
    // state rather than waiting forever for a stream that never starts.
    assert!(
        wakes.try_recv().is_ok(),
        "start_search should fire the wake on the graceful-degrade path"
    );

    drop(session);
    core.shutdown().expect("shutdown should be Ok");
}
