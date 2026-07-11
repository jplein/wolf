//! Integration test for the `wolf-ffi` MULTI-CORE WAKE-ROUTING surface.
//!
//! This drives the RUST half of the multi-core bridge against TWO REAL, hermetic
//! Neovim instances — NO mocks, per the project testing policy. It is the sibling
//! of `tests/state.rs` / `tests/panes.rs` (same staging / SKIP-when-unset
//! machinery, reusing `tests/common/mod.rs`) and exercises the per-core identity
//! + wake-routing surface:
//!
//!   * `wolf_ffi_start(init_lua, cwd, cols, rows)` — NO trailing `window_index`
//!     argument any more (state moved to the coordinator-owned snapshot builder;
//!     see `tests/state.rs`), so each core is started with exactly four args.
//!   * `WolfCoreHandle::wake_token() -> u64` — the per-core wake-routing identity
//!     token (`#[must_use]`), minted fresh per `wolf_ffi_start`.
//!   * `wolf_ffi_set_wake(Option<extern "C" fn(u64)>)` — the wake callback ABI
//!     carries the firing core's `token`, so a process-global callback can route
//!     each wake to the right window WITHOUT clobbering.
//!
//! ## What was REMOVED
//!
//! The former per-index state section (`state_set_geometry` /
//! `state_set_sidebar_visible` / `state_set_window_count` + the `:cd`
//! persists-per-slot assertions) is GONE: those granular setters and the
//! `window_index` slot were removed with the per-window-tabs change. The
//! tree-shaped snapshot round-trip is covered headlessly by `tests/state.rs`.
//! This test keeps ONLY the wake-token identity + routing assertions.
//!
//! ## SINGLE #[test]
//!
//! The whole flow lives in ONE `#[test]` because it registers the process-global
//! wake callback (and sets `WOLF_STATE_DIR` for shutdown-flush hygiene): with a
//! single test in this binary there is nothing to race.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::collections::HashMap;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_set_wake, wolf_ffi_start, FfiFrame, FfiKeyEvent};

/// Generous overall timeout for two nvim `--embed` startups plus each async
/// effect (input redraw, wake) to settle. Mirrors the sibling tests'
/// `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the sibling
/// tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A short quiescence pause after both cores attach, to let any trailing
/// startup redraw wakes drain into the baselines BEFORE the wake-routing checks
/// capture them (so an idle window B's late startup wake is not mistaken for a
/// clobbered route). Ten poll intervals (~0.5s).
const SETTLE: Duration = Duration::from_millis(500);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;

/// A distinctive marker typed into window A's buffer; unlikely to appear in an
/// empty window B, so it proves the two snapshots are independent.
const MARKER_A: &str = "ZZWOLFZZ";

/// Process-global tally of wake callbacks by their firing core's token. The
/// wake fires on a tokio worker thread, so a `Mutex` guards it. Wrapped in an
/// `Option` because `HashMap::new()` is not `const`; the callback lazily
/// initializes it on first fire.
static WAKE_COUNTS: Mutex<Option<HashMap<u64, u32>>> = Mutex::new(None);

/// The registered wake callback (`extern "C" fn(u64)` ABI): records one wake for
/// `token`. Installed process-globally via `wolf_ffi_set_wake`, it is the SAME
/// callback for BOTH cores — so a clobbered wake-routing layer would
/// mis-attribute wakes to the wrong token, which the assertions below catch.
extern "C" fn recording_wake(token: u64) {
    let mut guard = WAKE_COUNTS.lock().expect("wake-count mutex poisoned");
    let map = guard.get_or_insert_with(HashMap::new);
    *map.entry(token).or_insert(0) += 1;
}

/// Reads the recorded wake count for `token` (0 if it never fired).
fn wake_count(token: u64) -> u32 {
    let guard = WAKE_COUNTS.lock().expect("wake-count mutex poisoned");
    guard
        .as_ref()
        .and_then(|m| m.get(&token).copied())
        .unwrap_or(0)
}

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false. Mirrors `state.rs::char_key`.
fn char_key(c: char) -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_CHAR,
        ch: c as u32,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Types `text` into `handle` in insert mode: an `i` to enter insert, then each
/// character. Produces redraws (hence wakes) and observable grid text.
fn type_in_insert(handle: &wolf_ffi::WolfCoreHandle, text: &str) {
    handle.send_key(char_key('i'));
    for c in text.chars() {
        handle.send_key(char_key(c));
    }
}

/// Flattens a frame's cell texts (row-major) into one string, for a robust
/// "did the marker appear" comparison that does not pin exact coordinates.
/// Mirrors `bridge_smoke.rs::frame_text`.
fn frame_text(frame: &FfiFrame) -> String {
    frame.cells().into_iter().map(|cell| cell.text).collect()
}

/// Polls `handle.read_frame()` until it attaches (reports `cols() > 0`) or the
/// timeout elapses, asserting attachment. Mirrors the sibling tests' attach
/// poll.
fn wait_attached(handle: &wolf_ffi::WolfCoreHandle) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut frame = handle.read_frame();
    while Instant::now() < deadline {
        frame = handle.read_frame();
        if frame.cols() > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        frame.cols() > 0,
        "core should attach and report a non-empty active frame (cols, got {})",
        frame.cols()
    );
}

#[test]
fn multi_core_identity_snapshots_and_wake_routing() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Point the state machinery at a private tempdir for the WHOLE test, so the
    // best-effort shutdown flush lands somewhere hermetic (never the user's real
    // state dir). Safe to mutate because this binary has exactly one test.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    // Register the process-global wake callback BEFORE starting either core, so
    // every wake (including startup redraws) is recorded against its token. This
    // is the SAME callback for both cores — the whole point of the token ABI.
    wolf_ffi_set_wake(Some(recording_wake));

    // Two distinct cwds so the two cores are unmistakably separate sessions; kept
    // alive until after both `shutdown()`s (their `<cwd>/.wolf/` MRU caches must
    // outlive the sessions).
    let cwd_a = tempfile::tempdir().expect("failed to create temp cwd A");
    let cwd_b = tempfile::tempdir().expect("failed to create temp cwd B");

    // --- 1. Start TWO cores (no window_index argument any more) --------------
    let handle_a = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd_a.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start(A) should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    let handle_b = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd_b.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start(B) should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    wait_attached(&handle_a);
    wait_attached(&handle_b);

    // Let trailing startup redraw wakes drain before the wake-routing baselines.
    sleep(SETTLE);

    // --- 2. Distinct identity -----------------------------------------------
    let token_a = handle_a.wake_token();
    let token_b = handle_b.wake_token();
    assert_ne!(
        token_a, token_b,
        "each core must mint a DISTINCT wake token (A={token_a}, B={token_b})"
    );

    // --- 3. Independent snapshots + wake routing into A ----------------------
    // Capture wake baselines just before driving A: after typing into A, A's
    // token must advance while B's stays put (a clobbered wake layer would bump
    // B). B is idle+attached, so it should not redraw on its own during this
    // window; the robust core assertion is that A's OWN token advanced.
    let baseline_a = wake_count(token_a);
    let baseline_b = wake_count(token_b);

    type_in_insert(&handle_a, MARKER_A);

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut text_a = String::new();
    while Instant::now() < deadline {
        text_a = frame_text(&handle_a.read_frame());
        if text_a.contains(MARKER_A) && wake_count(token_a) > baseline_a {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        text_a.contains(MARKER_A),
        "window A's frame should reflect the typed marker {MARKER_A:?} (got {text_a:?})"
    );
    assert!(
        wake_count(token_a) > baseline_a,
        "typing into A must fire A's OWN wake token {token_a} \
         (baseline {baseline_a}, now {})",
        wake_count(token_a)
    );

    // B's snapshot must NOT reflect A's marker (independent per-core snapshots).
    let text_b = frame_text(&handle_b.read_frame());
    assert!(
        !text_b.contains(MARKER_A),
        "window B's frame must NOT contain window A's marker {MARKER_A:?} (got {text_b:?})"
    );
    // And A's edits must NOT have bumped B's wake token (no clobbering).
    assert_eq!(
        wake_count(token_b),
        baseline_b,
        "driving input into A must NOT bump B's wake token {token_b} \
         (baseline {baseline_b}, now {})",
        wake_count(token_b)
    );

    // --- 4. Wake routing into B ---------------------------------------------
    // The mirror check: driving B bumps B's token, never A's.
    let baseline_a2 = wake_count(token_a);
    let baseline_b2 = wake_count(token_b);
    type_in_insert(&handle_b, "b");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        if wake_count(token_b) > baseline_b2 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        wake_count(token_b) > baseline_b2,
        "typing into B must fire B's OWN wake token {token_b} \
         (baseline {baseline_b2}, now {})",
        wake_count(token_b)
    );
    assert_eq!(
        wake_count(token_a),
        baseline_a2,
        "driving input into B must NOT bump A's wake token {token_a} \
         (baseline {baseline_a2}, now {})",
        wake_count(token_a)
    );

    // --- 5. Teardown --------------------------------------------------------
    // Clear the process-global wake so it cannot outlive this test's callback.
    wolf_ffi_set_wake(None);
    handle_a.shutdown();
    handle_b.shutdown();

    std::env::remove_var("WOLF_STATE_DIR");
    drop(cwd_a);
    drop(cwd_b);
    drop(state_dir);
}
