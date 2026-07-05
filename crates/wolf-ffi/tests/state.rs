//! Integration test for the `wolf-ffi` state-persistence bridge surface
//! (plan §3, wolf-ffi):
//!
//!   * free fn `wolf_ffi_load_state() -> FfiState` (pre-start restore read;
//!     `state_dir().and_then(load_state)` defaulting to an empty window list)
//!   * `FfiState::windows() -> Vec<FfiWindowState>` with the accessors
//!     `x()/y()/width()/height() -> f64`, `cwd() -> String`, and
//!     `sidebar_visible() -> bool`
//!   * `WolfCoreHandle::state_set_geometry(x, y, width, height)`,
//!     `state_set_sidebar_visible(visible)`, and `state_flush()`
//!   * the `wolf_ffi_start` glue: the handle opens the `StateStore` at
//!     `state_dir()` and attaches it to the core, so a `:cd` typed through
//!     `send_key` persists with no explicit front-end call
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per
//! the project testing policy. Modeled on `tests/tree_search.rs` /
//! `tests/mouse_scroll.rs` (same staging / SKIP-when-unset machinery, reusing
//! `tests/common/mod.rs`, and the same local-consts discriminant-contract
//! guard pattern).
//!
//! ## SINGLE #[test]
//!
//! The whole flow lives in ONE `#[test]` because it mutates the process-global
//! `WOLF_STATE_DIR` env var (the `state_dir()` determinism hook): with a single
//! test in this binary there is nothing to race, so no env lock is needed
//! (the precedent is the plan's §6.3 "single binary -> env mutation safe").
//!
//! ## Substring assertions on state.json
//!
//! `serde_json` is NOT a dev-dependency of wolf-ffi, so the on-disk facets are
//! asserted via distinctive substrings of the pretty-printed JSON (each float
//! value below prints uniquely, e.g. `"x": 321.0`); the typed round-trip goes
//! through `wolf_ffi_load_state()`'s accessors instead.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_load_state, wolf_ffi_start, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (cwd notification, coalesced state write) to settle. Mirrors `tree_search`'s
/// `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors `tree_search`'s
/// `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The on-disk state file name the core-side store writes into `state_dir()`.
const STATE_FILE_NAME: &str = "state.json";

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- Distinctive geometry facets (chosen so each pretty-printed float substring
//    is unambiguous in state.json) ------------------------------------------------
/// Pushed window-0 x.
const GEO_X: f64 = 321.0;
/// Pushed window-0 y.
const GEO_Y: f64 = 87.5;
/// Pushed window-0 width.
const GEO_WIDTH: f64 = 1234.0;
/// Pushed window-0 height.
const GEO_HEIGHT: f64 = 567.0;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false. Mirrors `mouse_scroll.rs::char_key`.
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

/// Types `cmd` as an Ex command and presses Enter, through the same
/// `send_key`/`FfiKeyEvent` path the generated Swift glue uses. `cmd` must
/// include the leading `:`. Mirrors `mouse_scroll.rs::send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// Reads `state.json` from `dir`, returning `""` when it does not exist yet
/// (poll-friendly).
fn read_state(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(STATE_FILE_NAME)).unwrap_or_default()
}

#[test]
fn state_bridge_saves_and_restores_all_facets() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Point the state machinery at a private tempdir for the WHOLE test. This is
    // the `state_dir()` determinism hook; safe to mutate because this binary has
    // exactly one test.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());
    let state_path = state_dir.path().join(STATE_FILE_NAME);

    // --- 1. Pre-start restore read on an EMPTY dir -> zero windows -----------
    let empty = wolf_ffi_load_state();
    assert!(
        empty.windows().is_empty(),
        "wolf_ffi_load_state() over an empty state dir must yield 0 windows"
    );
    assert!(
        !state_path.exists(),
        "the restore read must NEVER create state.json"
    );

    // --- 2. Corrupt state.json -> still zero windows, file untouched ---------
    const CORRUPT: &str = "definitely {{{ not json\n";
    std::fs::write(&state_path, CORRUPT).expect("failed to seed corrupt state.json");
    let corrupt = wolf_ffi_load_state();
    assert!(
        corrupt.windows().is_empty(),
        "wolf_ffi_load_state() over a corrupt state.json must silently yield 0 windows"
    );
    assert_eq!(
        std::fs::read_to_string(&state_path).expect("failed to re-read corrupt state.json"),
        CORRUPT,
        "the restore read must preserve a corrupt file byte-for-byte"
    );
    std::fs::remove_file(&state_path).expect("failed to delete the corrupt state.json");

    // --- 3. Start the core: the handle opens + attaches the live store -------
    // A fresh temp cwd for nvim (its `<cwd>/.wolf/` MRU cache must outlive the
    // session), kept alive until after `shutdown()`.
    let tmp_cwd = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp_cwd.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
        0,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // Poll read_frame() until attached (cols > 0), like the siblings.
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

    // --- 4. Push the front-end facets over the bridge ------------------------
    handle.state_set_geometry(GEO_X, GEO_Y, GEO_WIDTH, GEO_HEIGHT);
    handle.state_set_sidebar_visible(false);

    // --- 5. `:cd <C>` through the normal key path; the CORE saves the cwd ----
    let cd_dir = tempfile::tempdir().expect("failed to create temp :cd target dir");
    let canonical_cd = cd_dir
        .path()
        .canonicalize()
        .expect("failed to canonicalize the :cd target");
    send_ex_command(&handle, &format!(":cd {}", cd_dir.path().display()));

    // Wait until nvim has actually re-rooted (cwd() equals the target,
    // canonicalize-compared, robust to macOS /var -> /private/var).
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut reported_cwd = handle.cwd();
    while Instant::now() < deadline {
        reported_cwd = handle.cwd();
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
        "cwd() should follow the :cd (reported={reported_cwd:?}, target={canonical_cd:?})"
    );

    // --- 6. state_flush(), then assert every facet in state.json -------------
    // The cwd hook (apply_cwd -> StateStore) can land a beat AFTER cwd() flips,
    // so flush+re-read in a bounded poll until the cwd shows up in the file.
    // Either path spelling (raw temp path or canonical /private form) is valid:
    // the file stores whatever nvim reported.
    let raw_cd = cd_dir.path().to_string_lossy().into_owned();
    let canon_cd = canonical_cd.to_string_lossy().into_owned();
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut text = String::new();
    while Instant::now() < deadline {
        let _ = handle.state_flush();
        text = read_state(state_dir.path());
        if text.contains(&canon_cd) || text.contains(&raw_cd) {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        text.contains(&canon_cd) || text.contains(&raw_cd),
        "state.json should hold the :cd target cwd within {FLUSH_TIMEOUT:?}, got:\n{text}"
    );
    assert!(
        text.contains("\"version\": 1"),
        "state.json must carry a `\"version\": 1` header, got:\n{text}"
    );
    assert!(
        text.contains("\n  "),
        "state.json must be pretty-printed (newline + indentation), got:\n{text}"
    );
    assert!(
        text.contains("\"x\": 321.0"),
        "state.json should hold the pushed x ({GEO_X}), got:\n{text}"
    );
    assert!(
        text.contains("\"y\": 87.5"),
        "state.json should hold the pushed y ({GEO_Y}), got:\n{text}"
    );
    assert!(
        text.contains("\"width\": 1234.0"),
        "state.json should hold the pushed width ({GEO_WIDTH}), got:\n{text}"
    );
    assert!(
        text.contains("\"height\": 567.0"),
        "state.json should hold the pushed height ({GEO_HEIGHT}), got:\n{text}"
    );
    assert!(
        text.contains("\"sidebar_visible\": false"),
        "state.json should hold the pushed sidebar visibility, got:\n{text}"
    );

    // --- 7. wolf_ffi_load_state() round-trips through the accessors ----------
    let restored = wolf_ffi_load_state();
    let windows = restored.windows();
    assert!(
        !windows.is_empty(),
        "the restore read should now see the saved window"
    );
    let w = &windows[0];
    assert_eq!(w.x(), GEO_X, "restored x");
    assert_eq!(w.y(), GEO_Y, "restored y");
    assert_eq!(w.width(), GEO_WIDTH, "restored width");
    assert_eq!(w.height(), GEO_HEIGHT, "restored height");
    assert!(!w.sidebar_visible(), "restored sidebar_visible");
    let restored_cwd = w.cwd();
    let canonical_restored = Path::new(&restored_cwd)
        .canonicalize()
        .expect("restored cwd should be a path that exists on disk");
    assert_eq!(
        canonical_restored, canonical_cd,
        "restored cwd should equal the :cd target (restored={restored_cwd:?})"
    );

    // --- 8. Shutdown: clean teardown (shutdown itself flushes the store) -----
    handle.shutdown();

    // --- 9. Empty-cwd ("no folder open") sentinel round-trips through FFI -----
    // The launch-behavior change persists `cwd == ""` for empty-state windows.
    // With the core shut down the store's writer is stopped, so state.json is
    // stable: overwrite it with a version-1 window whose cwd is empty and confirm
    // `wolf_ffi_load_state()` surfaces `cwd() == ""` UNCHANGED (the sentinel is
    // pure data — it must pass through the accessor without substitution). Written
    // as a raw JSON string because serde_json is not a wolf-ffi dev-dependency
    // (same reason the corrupt fixture above is a literal).
    const EMPTY_CWD_STATE_JSON: &str = r#"{
  "version": 1,
  "windows": [
    {
      "x": 0.0,
      "y": 0.0,
      "width": 0.0,
      "height": 0.0,
      "cwd": "",
      "sidebar_visible": false
    }
  ]
}
"#;
    std::fs::write(&state_path, EMPTY_CWD_STATE_JSON)
        .expect("failed to seed the empty-cwd state.json");
    let empty_cwd = wolf_ffi_load_state();
    let empty_windows = empty_cwd.windows();
    assert_eq!(
        empty_windows.len(),
        1,
        "the empty-cwd fixture must load back as exactly one window"
    );
    assert_eq!(
        empty_windows[0].cwd(),
        "",
        "an empty cwd (the no-folder sentinel) must round-trip through \
         wolf_ffi_load_state() as \"\", got {:?}",
        empty_windows[0].cwd()
    );
    assert!(
        !empty_windows[0].sidebar_visible(),
        "the seeded sidebar_visible=false must survive alongside the empty cwd"
    );

    std::env::remove_var("WOLF_STATE_DIR");
    drop(cd_dir);
    drop(tmp_cwd);
    drop(state_dir);
}
