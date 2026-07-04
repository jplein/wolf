//! Integration test for the `wolf-ffi` MULTI-WINDOW surface (plan §1-§3).
//!
//! This drives the RUST half of the multi-window bridge against TWO REAL,
//! hermetic Neovim instances — NO mocks, per the project testing policy. It is
//! the sibling of `tests/state.rs` / `tests/panes.rs` (same staging /
//! SKIP-when-unset machinery, reusing `tests/common/mod.rs`) but exercises the
//! NEW per-window surface the coder adds:
//!
//!   * `wolf_ffi_start(init_lua, cwd, cols, rows, window_index: u16)` — the added
//!     trailing `window_index` selects the `state.json` `windows` slot the handle
//!     writes into.
//!   * `WolfCoreHandle::wake_token() -> u64` — the per-core wake-routing identity
//!     token (`#[must_use]`), minted fresh per `wolf_ffi_start`.
//!   * `wolf_ffi_set_wake(Option<extern "C" fn(u64)>)` — the wake callback ABI now
//!     carries the firing core's `token`, so a process-global callback can route
//!     each wake to the right window WITHOUT clobbering.
//!   * `WolfCoreHandle::state_set_window_count(count: u16)` — truncates the
//!     `windows` list to `count` (ghost cleanup at terminate time).
//!
//! ## SINGLE #[test]
//!
//! The whole flow lives in ONE `#[test]` because it mutates the process-global
//! `WOLF_STATE_DIR` env var AND registers the process-global wake callback: with a
//! single test in this binary there is nothing to race. This mirrors the
//! single-#[test] rationale in `tests/state.rs`.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.
//!
//! NOTE: the accessor names below (`read_frame`, `send_key`, `cwd`,
//! `state_set_geometry`, `state_set_sidebar_visible`, `state_flush`,
//! `wolf_ffi_load_state().windows()`) mirror exactly what `tests/state.rs` and
//! `tests/panes.rs` call. The three NEW names — `wolf_ffi_start`'s trailing
//! `window_index`, `wake_token()`, and `state_set_window_count()` — are the
//! contract the coder must satisfy.

mod common;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_load_state, wolf_ffi_set_wake, wolf_ffi_start, FfiFrame, FfiKeyEvent};

/// Generous overall timeout for two nvim `--embed` startups plus each async
/// effect (input redraw, wake, cwd notification, coalesced state write) to
/// settle. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
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
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;
/// The Escape key (leaves insert mode).
const KIND_ESC: u8 = 2;

// -- Window slots -------------------------------------------------------------
/// The first window's `state.json` slot (and its `wolf_ffi_start` index).
const WINDOW_INDEX_A: u16 = 0;
/// The second window's `state.json` slot.
const WINDOW_INDEX_B: u16 = 1;
/// The count `state_set_window_count` truncates down to.
const TRUNCATED_COUNT: u16 = 1;
/// Expected number of `windows` after two cores have attached their shared store.
const TWO_WINDOWS: usize = 2;
/// Expected number of `windows` after truncation.
const ONE_WINDOW: usize = 1;

// -- Distinctive per-window geometry (each unique so a mixed-up slot is caught) -
/// Window A geometry (slot 0).
const GEO_A_X: f64 = 100.0;
const GEO_A_Y: f64 = 200.0;
const GEO_A_WIDTH: f64 = 800.0;
const GEO_A_HEIGHT: f64 = 600.0;
/// Window B geometry (slot 1) — deliberately different from A's on every axis.
const GEO_B_X: f64 = 321.0;
const GEO_B_Y: f64 = 87.5;
const GEO_B_WIDTH: f64 = 1234.0;
const GEO_B_HEIGHT: f64 = 567.0;

/// A distinctive marker typed into window A's buffer; unlikely to appear in an
/// empty window B, so it proves the two snapshots are independent.
const MARKER_A: &str = "ZZWOLFZZ";

/// Process-global tally of wake callbacks by their firing core's token. The
/// wake fires on a tokio worker thread, so a `Mutex` guards it. Wrapped in an
/// `Option` because `HashMap::new()` is not `const`; the callback lazily
/// initializes it on first fire.
static WAKE_COUNTS: Mutex<Option<HashMap<u64, u32>>> = Mutex::new(None);

/// The registered wake callback (new `extern "C" fn(u64)` ABI): records one
/// wake for `token`. Installed process-globally via `wolf_ffi_set_wake`, it is
/// the SAME callback for BOTH cores — so a clobbered wake-routing layer would
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

/// Builds an Enter `FfiKeyEvent` (kind=Enter), all modifiers false.
fn enter_key() -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_ENTER,
        ch: 0,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Builds an Escape `FfiKeyEvent` (kind=Esc), all modifiers false.
fn esc_key() -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_ESC,
        ch: 0,
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

/// Types `cmd` as an Ex command and presses Enter, through the same
/// `send_key`/`FfiKeyEvent` path the generated Swift glue uses. `cmd` must
/// include the leading `:`. Mirrors `state.rs::send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
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
fn multi_window_identity_snapshots_wake_and_state() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Point the state machinery at a private tempdir for the WHOLE test (the
    // `state_dir()` determinism hook; safe to mutate because this binary has
    // exactly one test).
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

    // --- 1. Start TWO cores at distinct window slots ------------------------
    let handle_a = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd_a.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
        WINDOW_INDEX_A,
    )
    .expect("wolf_ffi_start(A) should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    let handle_b = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd_b.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
        WINDOW_INDEX_B,
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
    // window; if that ever proves flaky headless, the robust core assertion is
    // that A's OWN token advanced.
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

    // Leave insert mode on both so the later `:cd` Ex command is parsed as an Ex
    // command, not typed literally into the buffer.
    handle_a.send_key(esc_key());
    handle_b.send_key(esc_key());

    // --- 5. Shared store + per-index state ----------------------------------
    // Each handle writes its OWN slot (slot 0 for A, slot 1 for B) into the
    // SHARED store, so the on-disk `windows` list gains two independent entries.
    handle_a.state_set_geometry(GEO_A_X, GEO_A_Y, GEO_A_WIDTH, GEO_A_HEIGHT);
    handle_a.state_set_sidebar_visible(true);
    handle_b.state_set_geometry(GEO_B_X, GEO_B_Y, GEO_B_WIDTH, GEO_B_HEIGHT);
    handle_b.state_set_sidebar_visible(false);

    // `:cd` window B into a fresh dir; the CORE persists the cwd at B's slot.
    let cd_dir = tempfile::tempdir().expect("failed to create temp :cd target dir");
    let canonical_cd = cd_dir
        .path()
        .canonicalize()
        .expect("failed to canonicalize the :cd target");
    send_ex_command(&handle_b, &format!(":cd {}", cd_dir.path().display()));

    // Wait until nvim B has actually re-rooted (canonicalize-compared, robust to
    // the macOS /var -> /private/var symlink).
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut reported_cwd = handle_b.cwd();
    while Instant::now() < deadline {
        reported_cwd = handle_b.cwd();
        if !reported_cwd.is_empty() {
            if let Ok(canonical_reported) = Path::new(&reported_cwd).canonicalize() {
                if canonical_reported == canonical_cd {
                    break;
                }
            }
        }
        sleep(POLL_INTERVAL);
    }
    let canonical_reported = Path::new(&reported_cwd)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(&reported_cwd).to_path_buf());
    assert_eq!(
        canonical_reported, canonical_cd,
        "B's cwd() should follow the :cd (reported={reported_cwd:?}, target={canonical_cd:?})"
    );

    // Flush both cores' pending mutations, then load the shared state. Poll until
    // the two windows AND B's cwd have landed (the cwd hook can trail cwd()).
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let raw_cd = cd_dir.path().to_string_lossy().into_owned();
    let canon_cd = canonical_cd.to_string_lossy().into_owned();
    loop {
        let _ = handle_a.state_flush();
        let _ = handle_b.state_flush();
        let state = wolf_ffi_load_state();
        let windows = state.windows();
        if windows.len() >= TWO_WINDOWS {
            let b_cwd = windows[WINDOW_INDEX_B as usize].cwd();
            if b_cwd.contains(&canon_cd) || b_cwd.contains(&raw_cd) {
                break;
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        sleep(POLL_INTERVAL);
    }

    let state = wolf_ffi_load_state();
    let windows = state.windows();
    assert_eq!(
        windows.len(),
        TWO_WINDOWS,
        "the shared store should hold exactly two window slots (got {})",
        windows.len()
    );

    // Slot 0 == window A's facets.
    let wa = &windows[WINDOW_INDEX_A as usize];
    assert_eq!(wa.x(), GEO_A_X, "slot 0 x should be A's");
    assert_eq!(wa.y(), GEO_A_Y, "slot 0 y should be A's");
    assert_eq!(wa.width(), GEO_A_WIDTH, "slot 0 width should be A's");
    assert_eq!(wa.height(), GEO_A_HEIGHT, "slot 0 height should be A's");
    assert!(wa.sidebar_visible(), "slot 0 sidebar_visible should be A's (true)");

    // Slot 1 == window B's facets, including the :cd cwd.
    let wb = &windows[WINDOW_INDEX_B as usize];
    assert_eq!(wb.x(), GEO_B_X, "slot 1 x should be B's");
    assert_eq!(wb.y(), GEO_B_Y, "slot 1 y should be B's");
    assert_eq!(wb.width(), GEO_B_WIDTH, "slot 1 width should be B's");
    assert_eq!(wb.height(), GEO_B_HEIGHT, "slot 1 height should be B's");
    assert!(
        !wb.sidebar_visible(),
        "slot 1 sidebar_visible should be B's (false)"
    );
    let restored_b_cwd = wb.cwd();
    let canonical_restored = Path::new(&restored_b_cwd)
        .canonicalize()
        .expect("slot 1 cwd should be a path that exists on disk");
    assert_eq!(
        canonical_restored, canonical_cd,
        "slot 1 cwd should equal B's :cd target (restored={restored_b_cwd:?})"
    );

    // --- 6. Truncate: state_set_window_count(1) collapses to one window ------
    handle_a.state_set_window_count(TRUNCATED_COUNT);
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut truncated = wolf_ffi_load_state();
    while Instant::now() < deadline {
        let _ = handle_a.state_flush();
        truncated = wolf_ffi_load_state();
        if truncated.windows().len() == ONE_WINDOW {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        truncated.windows().len(),
        ONE_WINDOW,
        "state_set_window_count({TRUNCATED_COUNT}) should truncate to {ONE_WINDOW} window (got {})",
        truncated.windows().len()
    );
    // The surviving slot is slot 0 == window A.
    assert_eq!(
        truncated.windows()[WINDOW_INDEX_A as usize].x(),
        GEO_A_X,
        "the surviving window after truncation should be slot 0 (A)"
    );

    // --- 7. Teardown --------------------------------------------------------
    // Clear the process-global wake so it cannot outlive this test's callback.
    wolf_ffi_set_wake(None);
    handle_a.shutdown();
    handle_b.shutdown();

    std::env::remove_var("WOLF_STATE_DIR");
    drop(cd_dir);
    drop(cwd_a);
    drop(cwd_b);
    drop(state_dir);
}
