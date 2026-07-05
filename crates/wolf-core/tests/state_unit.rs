//! Headless coverage for the PUBLIC surface of `wolf_core::state` (plan §2 +
//! test plan §6.1): `StateStore`, `load_state`, `state_dir`, `ToolbarMode`, and
//! the `WolfState`/`WindowState` value types re-exported at the crate root.
//!
//! ## Why this lives in `tests/` rather than `src/state.rs`'s `#[cfg(test)]`
//!
//! The architect plan places these cases in an in-module `#[cfg(test)] mod
//! tests` inside `src/state.rs`. That module does not exist yet (tests are
//! written first), so the cases live here as an integration-style binary
//! exercising ONLY the planned public, crate-root-re-exported API. The coder may
//! keep this file as-is (everything asserted is public behavior) or fold the
//! cases into `src/state.rs`'s test module — either satisfies the plan.
//!
//! Needs NO nvim and no network: everything is driven through explicit
//! `tempfile` tempdirs passed to `StateStore::open` / `load_state`. The single
//! env-mutating case (`WOLF_STATE_DIR`) is serialized behind a LOCAL `ENV_LOCK`:
//! this binary does not share `config.rs`'s in-module lock, and cargo runs the
//! tests in this file concurrently within one process.
//!
//! NOTE to coder — Bazel wiring: a `state_unit` `rust_test` target (no nvim
//! data, no `no-sandbox` tag) is added to `crates/wolf-core/BUILD.bazel` by the
//! test writer alongside this file.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use tempfile::tempdir;
use wolf_core::state::STATE_FILE_NAME;
use wolf_core::{load_state, state_dir, StateStore, ToolbarMode};

/// Generous upper bound for `StateStore::flush` in tests: well beyond the
/// planned ~100ms write-coalesce delay, so a timeout always means a real bug.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

// -- Distinctive, non-default window-0 values for the round-trip cases --------
/// Round-trip window-0 x (AppKit bottom-left points).
const GEO_X: f64 = 128.0;
/// Round-trip window-0 y.
const GEO_Y: f64 = 64.5;
/// Round-trip window-0 width.
const GEO_WIDTH: f64 = 1180.0;
/// Round-trip window-0 height.
const GEO_HEIGHT: f64 = 760.0;

// -- Distinctive values for the hand-written multi-window fixtures ------------
/// Hand-written window-1 x — must survive a window-0 rewrite verbatim.
const WIN1_X: f64 = 555.0;
/// Hand-written window-1 y.
const WIN1_Y: f64 = 44.0;
/// Hand-written window-1 width.
const WIN1_WIDTH: f64 = 900.0;
/// Hand-written window-1 height.
const WIN1_HEIGHT: f64 = 600.0;
/// Hand-written window-1 cwd (never validated by `load_state`, so it need not
/// exist on disk).
const WIN1_CWD: &str = "/hand/written/window-one";
/// Hand-written window-0 cwd for the multi-window fixtures.
const WIN0_CWD: &str = "/hand/written/window-zero";

/// Replacement window-0 geometry pushed over the hand-written fixture.
const NEW_X: f64 = 10.0;
/// Replacement window-0 y.
const NEW_Y: f64 = 20.0;
/// Replacement window-0 width.
const NEW_WIDTH: f64 = 1024.0;
/// Replacement window-0 height.
const NEW_HEIGHT: f64 = 768.0;

// -- Concurrency / coalescing knobs (named per the magic-number rule) ---------
/// Number of hammering worker threads in the concurrency case.
const HAMMER_THREADS: usize = 8;
/// Setter iterations per hammering thread.
const HAMMER_ITERATIONS: usize = 50;
/// Deterministic final x written after the hammering, so the end state is known.
const FINAL_X: f64 = 4242.0;
/// Rapid back-to-back mutations for the coalescing case.
const RAPID_MUTATIONS: usize = 25;

/// Serializes every test in this binary that mutates PROCESS-GLOBAL environment
/// (here, only `state_dir_honors_env_override`, which sets `WOLF_STATE_DIR`).
///
/// cargo runs this file's tests concurrently on multiple threads within ONE
/// process, so an env var set by one test would be visible to (and race with)
/// any other test reading the same var. This is a LOCAL lock: `config.rs`'s
/// in-module `ENV_LOCK` is not visible from a separate integration binary. The
/// non-env cases below pass explicit dirs to `StateStore::open` / `load_state`
/// and never touch the environment, so they do not need the lock.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Reads a file to a `String`, panicking with context on failure (errcheck).
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Writes `contents` to `path`, panicking with context on failure (errcheck).
fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents)
        .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
}

/// Compares two paths by their canonical form when resolvable, else raw
/// equality — robust to the macOS `/var` -> `/private/var` tempdir symlink.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// A hand-written, VALID version-1 two-window file: window 0 with `WIN0_CWD`,
/// window 1 fully populated (incl. `toolbar_display_mode`) with the `WIN1_*`
/// values. Every per-window required field is present.
fn two_window_fixture() -> String {
    format!(
        r#"{{
  "version": 1,
  "windows": [
    {{
      "x": 1.0,
      "y": 2.0,
      "width": 3.0,
      "height": 4.0,
      "cwd": "{WIN0_CWD}",
      "sidebar_visible": true
    }},
    {{
      "x": {WIN1_X},
      "y": {WIN1_Y},
      "width": {WIN1_WIDTH},
      "height": {WIN1_HEIGHT},
      "cwd": "{WIN1_CWD}",
      "sidebar_visible": false,
      "toolbar_display_mode": "text_only"
    }}
  ]
}}
"#
    )
}

// =========================================================================
// Round-trip: open -> setters -> flush -> load_state, pretty-printed output
// =========================================================================

#[test]
fn round_trip_through_store_and_load() {
    let dir = tempdir().expect("create temp state dir");
    let cwd_dir = tempdir().expect("create temp cwd dir");
    let cwd = std::fs::canonicalize(cwd_dir.path()).expect("canonicalize temp cwd");

    let store = StateStore::open(dir.path());
    store.set_geometry(0, GEO_X, GEO_Y, GEO_WIDTH, GEO_HEIGHT);
    store.set_cwd(0, &cwd);
    store.set_sidebar_visible(0, false);
    store.set_toolbar_mode(0, Some(ToolbarMode::IconOnly));
    assert!(
        store.flush(FLUSH_TIMEOUT),
        "flush after mutations should complete within {FLUSH_TIMEOUT:?}"
    );

    // The in-memory truth reflects the mutations immediately.
    let mem = store.state();
    assert_eq!(mem.windows.len(), 1, "one window mutated -> one entry");

    // load_state reads back exactly what the store wrote.
    let loaded = load_state(dir.path()).expect("load_state should parse the file the store wrote");
    assert_eq!(loaded.windows.len(), 1, "loaded window count");
    let w = &loaded.windows[0];
    assert_eq!(w.x, GEO_X, "loaded x");
    assert_eq!(w.y, GEO_Y, "loaded y");
    assert_eq!(w.width, GEO_WIDTH, "loaded width");
    assert_eq!(w.height, GEO_HEIGHT, "loaded height");
    assert!(
        paths_equal(Path::new(&w.cwd), &cwd),
        "loaded cwd {:?} should equal the set cwd {}",
        w.cwd,
        cwd.display()
    );
    assert!(!w.sidebar_visible, "loaded sidebar_visible");
    assert_eq!(
        w.toolbar_mode,
        Some(ToolbarMode::IconOnly),
        "loaded toolbar_mode"
    );

    // The on-disk form is PRETTY-PRINTED JSON with a version-1 header:
    // serde_json::to_string_pretty indents nested lines with two spaces.
    let text = read(&dir.path().join(STATE_FILE_NAME));
    assert!(
        text.contains("\"version\": 1"),
        "state.json must carry a `\"version\": 1` header, got:\n{text}"
    );
    assert!(
        text.contains("\n  "),
        "state.json must be pretty-printed (newline + indentation), got:\n{text}"
    );
}

// =========================================================================
// Empty-cwd ("no folder open") sentinel round-trip
// =========================================================================

#[test]
fn empty_cwd_round_trips_through_store_and_load() {
    // The launch-behavior change makes `cwd == ""` the "no folder open" sentinel
    // for empty-state windows. It must survive the real serde path: an empty
    // string serializes into `state.json` verbatim and loads back as `""`, with
    // every OTHER field intact. A front-end reports geometry/sidebar/toolbar for
    // an empty-state window while its cwd stays at the `""` default.
    let dir = tempdir().expect("create temp state dir");

    let store = StateStore::open(dir.path());
    store.set_geometry(0, GEO_X, GEO_Y, GEO_WIDTH, GEO_HEIGHT);
    store.set_sidebar_visible(0, false);
    store.set_toolbar_mode(0, Some(ToolbarMode::IconOnly));
    // cwd is deliberately never set: it stays at the `""` default (the sentinel).
    assert!(
        store.flush(FLUSH_TIMEOUT),
        "flush after mutations should complete within {FLUSH_TIMEOUT:?}"
    );

    let loaded = load_state(dir.path()).expect("load_state should parse the file the store wrote");
    assert_eq!(loaded.windows.len(), 1, "one window mutated -> one entry");
    let w = &loaded.windows[0];
    assert_eq!(w.cwd, "", "an empty cwd must load back as the empty sentinel");
    // Every other field survives alongside the empty cwd.
    assert_eq!(w.x, GEO_X, "loaded x survives alongside the empty cwd");
    assert_eq!(w.y, GEO_Y, "loaded y survives alongside the empty cwd");
    assert_eq!(w.width, GEO_WIDTH, "loaded width survives alongside the empty cwd");
    assert_eq!(w.height, GEO_HEIGHT, "loaded height survives alongside the empty cwd");
    assert!(!w.sidebar_visible, "loaded sidebar_visible survives alongside the empty cwd");
    assert_eq!(
        w.toolbar_mode,
        Some(ToolbarMode::IconOnly),
        "loaded toolbar_mode survives alongside the empty cwd"
    );

    // The empty string is written literally, not omitted: `cwd` is a required
    // field (no skip_serializing_if), so the on-disk form carries `"cwd": ""`.
    let text = read(&dir.path().join(STATE_FILE_NAME));
    assert!(
        text.contains("\"cwd\": \"\""),
        "an empty cwd must serialize as `\"cwd\": \"\"`, got:\n{text}"
    );
}

// =========================================================================
// Unset-geometry sentinel + toolbar omission
// =========================================================================

#[test]
fn set_cwd_alone_writes_zero_geometry_and_omits_toolbar() {
    let dir = tempdir().expect("create temp state dir");
    let cwd_dir = tempdir().expect("create temp cwd dir");
    let cwd = std::fs::canonicalize(cwd_dir.path()).expect("canonicalize temp cwd");

    // Only a cwd push — the geometry was never reported by any front-end.
    let store = StateStore::open(dir.path());
    store.set_cwd(0, &cwd);
    assert!(store.flush(FLUSH_TIMEOUT), "flush should complete");

    let loaded = load_state(dir.path()).expect("load_state should parse");
    let w = &loaded.windows[0];
    // The "unset" sentinel: width/height (and x/y) default to zero, which the
    // restore side treats as absent (plan §1).
    assert_eq!(w.x, 0.0, "unset x sentinel");
    assert_eq!(w.y, 0.0, "unset y sentinel");
    assert_eq!(w.width, 0.0, "unset width sentinel");
    assert_eq!(w.height, 0.0, "unset height sentinel");
    // The remaining defaults: sidebar visible, no toolbar mode.
    assert!(w.sidebar_visible, "default sidebar_visible is true");
    assert_eq!(w.toolbar_mode, None, "default toolbar_mode is None");

    // `toolbar_display_mode` is OMITTED (skip_serializing_if) when unset.
    let text = read(&dir.path().join(STATE_FILE_NAME));
    assert!(
        !text.contains("toolbar_display_mode"),
        "an unset toolbar mode must be omitted from the file, got:\n{text}"
    );
}

// =========================================================================
// Silently-ignore matrix (load_state -> None; open -> defaults; file untouched)
// =========================================================================

#[test]
fn absent_file_loads_none_and_open_yields_defaults() {
    let dir = tempdir().expect("create temp state dir");
    let path = dir.path().join(STATE_FILE_NAME);
    assert!(!path.exists(), "precondition: no state file yet");

    // Absent -> None, silently; load_state NEVER creates the file.
    assert!(
        load_state(dir.path()).is_none(),
        "absent file must load as None"
    );
    assert!(
        !path.exists(),
        "load_state must NEVER create the state file"
    );

    // open over the same dir degrades to defaults (empty window list).
    let store = StateStore::open(dir.path());
    assert!(
        store.state().windows.is_empty(),
        "open over an absent file must yield the default (empty) state"
    );
}

#[test]
fn invalid_files_load_none_and_are_preserved_byte_for_byte() {
    /// One silently-ignore row: a file body that must make `load_state` return
    /// `None` without the file being created, modified, or deleted.
    struct Case {
        name: &'static str,
        contents: &'static str,
    }
    let cases = [
        Case {
            name: "junk bytes (not JSON)",
            contents: "this is {{{ not json\n\t- at all\n",
        },
        Case {
            name: "valid JSON, wrong shape (windows is an object)",
            contents: r#"{ "version": 1, "windows": { "x": 1.0 } }"#,
        },
        Case {
            name: "missing required per-window field (only x present)",
            contents: r#"{ "version": 1, "windows": [ { "x": 1.0 } ] }"#,
        },
        Case {
            name: "unsupported version 2 (well-formed windows)",
            contents: r#"{
  "version": 2,
  "windows": [
    {
      "x": 1.0,
      "y": 2.0,
      "width": 3.0,
      "height": 4.0,
      "cwd": "/somewhere",
      "sidebar_visible": true
    }
  ]
}"#,
        },
    ];

    for case in &cases {
        let dir = tempdir().expect("create temp state dir");
        let path = dir.path().join(STATE_FILE_NAME);
        write(&path, case.contents);
        let before = read(&path);

        assert!(
            load_state(dir.path()).is_none(),
            "{}: load_state must silently return None",
            case.name
        );
        assert_eq!(
            read(&path),
            before,
            "{}: load_state must preserve the file byte-for-byte",
            case.name
        );

        // The store degrades to defaults over the same ignored file — never a
        // crash, never a partial parse.
        let store = StateStore::open(dir.path());
        assert!(
            store.state().windows.is_empty(),
            "{}: open over an ignored file must yield the default (empty) state",
            case.name
        );
    }
}

#[test]
fn unknown_extra_fields_still_parse() {
    let dir = tempdir().expect("create temp state dir");
    let path = dir.path().join(STATE_FILE_NAME);
    // A future writer added fields this version does not know: both a top-level
    // extra and a per-window extra must be TOLERATED (no deny_unknown_fields).
    let contents = format!(
        r#"{{
  "version": 1,
  "future_top_level_field": "ignored",
  "windows": [
    {{
      "x": {GEO_X},
      "y": {GEO_Y},
      "width": {GEO_WIDTH},
      "height": {GEO_HEIGHT},
      "cwd": "{WIN0_CWD}",
      "sidebar_visible": false,
      "future_window_field": 42
    }}
  ]
}}
"#
    );
    write(&path, &contents);

    let loaded = load_state(dir.path())
        .expect("unknown extra fields must be tolerated, not treated as corruption");
    assert_eq!(loaded.windows.len(), 1, "window count with extra fields");
    let w = &loaded.windows[0];
    assert_eq!(w.x, GEO_X, "known x still surfaces");
    assert_eq!(w.cwd, WIN0_CWD, "known cwd still surfaces");
    assert!(!w.sidebar_visible, "known sidebar_visible still surfaces");
}

// =========================================================================
// Multi-window preservation
// =========================================================================

#[test]
fn rewriting_window_zero_preserves_window_one() {
    let dir = tempdir().expect("create temp state dir");
    write(&dir.path().join(STATE_FILE_NAME), &two_window_fixture());

    // Open over the two-window file and mutate ONLY window 0's geometry.
    let store = StateStore::open(dir.path());
    store.set_geometry(0, NEW_X, NEW_Y, NEW_WIDTH, NEW_HEIGHT);
    assert!(store.flush(FLUSH_TIMEOUT), "flush should complete");

    let loaded = load_state(dir.path()).expect("rewritten file should parse");
    assert_eq!(loaded.windows.len(), 2, "both windows must survive a rewrite");

    // Window 0: geometry replaced, everything else intact from the fixture.
    let w0 = &loaded.windows[0];
    assert_eq!(w0.x, NEW_X, "window 0 x updated");
    assert_eq!(w0.y, NEW_Y, "window 0 y updated");
    assert_eq!(w0.width, NEW_WIDTH, "window 0 width updated");
    assert_eq!(w0.height, NEW_HEIGHT, "window 0 height updated");
    assert_eq!(w0.cwd, WIN0_CWD, "window 0 cwd untouched");
    assert!(w0.sidebar_visible, "window 0 sidebar untouched");
    assert_eq!(w0.toolbar_mode, None, "window 0 toolbar untouched");

    // Window 1: EVERY field survives verbatim.
    let w1 = &loaded.windows[1];
    assert_eq!(w1.x, WIN1_X, "window 1 x verbatim");
    assert_eq!(w1.y, WIN1_Y, "window 1 y verbatim");
    assert_eq!(w1.width, WIN1_WIDTH, "window 1 width verbatim");
    assert_eq!(w1.height, WIN1_HEIGHT, "window 1 height verbatim");
    assert_eq!(w1.cwd, WIN1_CWD, "window 1 cwd verbatim");
    assert!(!w1.sidebar_visible, "window 1 sidebar verbatim");
    assert_eq!(
        w1.toolbar_mode,
        Some(ToolbarMode::TextOnly),
        "window 1 toolbar verbatim"
    );
}

#[test]
fn set_cwd_beyond_len_grows_the_list_with_defaults() {
    let dir = tempdir().expect("create temp state dir");
    // A ONE-window fixture (reuse the two-window body's first entry).
    let one_window = format!(
        r#"{{
  "version": 1,
  "windows": [
    {{
      "x": 1.0,
      "y": 2.0,
      "width": 3.0,
      "height": 4.0,
      "cwd": "{WIN0_CWD}",
      "sidebar_visible": true
    }}
  ]
}}
"#
    );
    write(&dir.path().join(STATE_FILE_NAME), &one_window);

    let cwd_dir = tempdir().expect("create temp cwd dir");
    let cwd = std::fs::canonicalize(cwd_dir.path()).expect("canonicalize temp cwd");

    // Mutating index 2 on a 1-window state must grow the list with defaults.
    const GROWN_INDEX: usize = 2;
    const GROWN_LEN: usize = 3;
    let store = StateStore::open(dir.path());
    store.set_cwd(GROWN_INDEX, &cwd);
    assert!(store.flush(FLUSH_TIMEOUT), "flush should complete");

    let loaded = load_state(dir.path()).expect("grown file should parse");
    assert_eq!(
        loaded.windows.len(),
        GROWN_LEN,
        "set_cwd({GROWN_INDEX}, …) on a 1-window state must grow the list to {GROWN_LEN}"
    );

    // Window 0 is untouched.
    assert_eq!(loaded.windows[0].cwd, WIN0_CWD, "window 0 cwd untouched");

    // The gap entry (index 1) is a pure default.
    let gap = &loaded.windows[1];
    assert_eq!(gap.x, 0.0, "gap window default x");
    assert_eq!(gap.width, 0.0, "gap window default width (unset sentinel)");
    assert_eq!(gap.height, 0.0, "gap window default height (unset sentinel)");
    assert_eq!(gap.cwd, "", "gap window default cwd is empty");
    assert!(gap.sidebar_visible, "gap window default sidebar is visible");
    assert_eq!(gap.toolbar_mode, None, "gap window default toolbar is None");

    // The target entry got the cwd, with everything else defaulted.
    let target = &loaded.windows[GROWN_INDEX];
    assert!(
        paths_equal(Path::new(&target.cwd), &cwd),
        "grown window cwd {:?} should equal the set cwd {}",
        target.cwd,
        cwd.display()
    );
    assert_eq!(target.width, 0.0, "grown window keeps the unset-geometry sentinel");
    assert_eq!(target.height, 0.0, "grown window keeps the unset-geometry sentinel");
}

// =========================================================================
// Concurrency: hammering setters from many threads, single-writer output
// =========================================================================

#[test]
fn concurrent_setters_produce_a_clean_file_and_no_tmp_residue() {
    let dir = tempdir().expect("create temp state dir");
    let cwd_dir = tempdir().expect("create temp cwd dir");
    let cwd = std::fs::canonicalize(cwd_dir.path()).expect("canonicalize temp cwd");

    let store = StateStore::open(dir.path());

    // N threads hammer every setter concurrently; the store must serialize all
    // of it through its single writer thread without panicking or corrupting.
    let workers: Vec<_> = (0..HAMMER_THREADS)
        .map(|t| {
            let store = store.clone();
            let cwd = cwd.clone();
            std::thread::spawn(move || {
                for i in 0..HAMMER_ITERATIONS {
                    let v = (t * HAMMER_ITERATIONS + i) as f64;
                    store.set_geometry(0, v, v, v + 1.0, v + 1.0);
                    store.set_cwd(0, &cwd);
                    store.set_sidebar_visible(0, i % 2 == 0);
                    let mode = if i % 2 == 0 {
                        Some(ToolbarMode::IconAndText)
                    } else {
                        Some(ToolbarMode::TextOnly)
                    };
                    store.set_toolbar_mode(0, mode);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("hammering worker thread panicked");
    }

    // A deterministic final mutation so the end state is known, then flush.
    store.set_geometry(0, FINAL_X, FINAL_X, FINAL_X, FINAL_X);
    assert!(
        store.flush(FLUSH_TIMEOUT),
        "flush after the hammering should complete within {FLUSH_TIMEOUT:?}"
    );

    // The file parses cleanly and ends at the final value (latest-wins).
    let loaded =
        load_state(dir.path()).expect("state.json must parse cleanly after concurrent hammering");
    assert_eq!(loaded.windows[0].x, FINAL_X, "final x wins");
    assert_eq!(loaded.windows[0].width, FINAL_X, "final width wins");

    // Atomic tmp+rename writes leave no `.tmp` residue once the writer is idle.
    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .expect("read state dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no .tmp residue may remain after flush (got {leftovers:?})"
    );
}

// =========================================================================
// Flush semantics + write coalescing
// =========================================================================

#[test]
fn coalesced_rapid_mutations_end_at_the_last_value() {
    let dir = tempdir().expect("create temp state dir");
    let store = StateStore::open(dir.path());

    // Rapid back-to-back mutations well inside the coalesce window: the writer
    // may skip intermediates, but after flush the file must hold the LAST value.
    for i in 0..RAPID_MUTATIONS {
        let v = i as f64;
        store.set_geometry(0, v, v, v, v);
    }
    assert!(
        store.flush(FLUSH_TIMEOUT),
        "flush after rapid mutations should complete within {FLUSH_TIMEOUT:?}"
    );

    let last = (RAPID_MUTATIONS - 1) as f64;
    let loaded = load_state(dir.path()).expect("coalesced file should parse");
    assert_eq!(
        loaded.windows[0].x, last,
        "the file must reflect the LAST of the coalesced mutations"
    );

    let path = dir.path().join(STATE_FILE_NAME);
    assert!(path.exists(), "flush must have produced the state file");
}

// =========================================================================
// Drop semantics: pending writes survive an immediate drop (no flush)
// =========================================================================

/// Generous overall bound for the drop-persistence poll: far beyond the
/// ~100ms coalesce delay and ~500ms idle poll, so a timeout means a real bug
/// (the writer thread exited with a write still pending).
const DROP_PERSIST_TIMEOUT: Duration = Duration::from_secs(10);
/// Sleep between `load_state` polls while waiting for the dropped store's
/// writer thread to land the pending mutation on disk.
const DROP_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[test]
fn mutate_then_immediate_drop_is_persisted() {
    let dir = tempdir().expect("create temp state dir");
    let cwd_dir = tempdir().expect("create temp cwd dir");
    let cwd = std::fs::canonicalize(cwd_dir.path()).expect("canonicalize temp cwd");

    // One distinctive mutation, then drop EVERY handle immediately — no
    // flush(). The writer thread's exit condition is "all handles dropped AND
    // nothing pending", so it must complete this write before exiting.
    let store = StateStore::open(dir.path());
    store.set_cwd(0, &cwd);
    drop(store);

    // Poll (never a single fixed sleep — the coalesce delay is ~100ms and the
    // idle poll ~500ms, but neither is a contract) until the mutation lands.
    let deadline = std::time::Instant::now() + DROP_PERSIST_TIMEOUT;
    let mut persisted = false;
    while std::time::Instant::now() < deadline {
        if let Some(loaded) = load_state(dir.path()) {
            if loaded
                .windows
                .first()
                .is_some_and(|w| paths_equal(Path::new(&w.cwd), &cwd))
            {
                persisted = true;
                break;
            }
        }
        std::thread::sleep(DROP_POLL_INTERVAL);
    }
    assert!(
        persisted,
        "a mutation made just before dropping every store handle must reach \
         state.json within {DROP_PERSIST_TIMEOUT:?} without an explicit flush"
    );

    // Once the write has landed, the atomic tmp+rename must leave no residue.
    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .expect("read state dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no .tmp residue may remain after the drop-triggered write (got {leftovers:?})"
    );
}

// =========================================================================
// state_dir — WOLF_STATE_DIR override (env-mutating: serialized)
// =========================================================================

#[test]
fn state_dir_honors_env_override() {
    // Env-mutating: hold ENV_LOCK across the whole set/read/remove window so
    // this cannot race any other env-touching test in this process.
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

    let dir = tempdir().expect("create temp dir");
    std::env::set_var("WOLF_STATE_DIR", dir.path());

    let resolved = state_dir();
    std::env::remove_var("WOLF_STATE_DIR");

    assert_eq!(
        resolved.as_deref(),
        Some(dir.path()),
        "WOLF_STATE_DIR must override the platform state dir"
    );
}
