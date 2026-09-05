//! Headless coverage for the NEW TREE-shaped `wolf_core::state` surface (plan
//! Phase 1 + OD1): the `TabState` / `WindowState{geometry, tabs}` / `WolfState`
//! value types (all re-exported at the crate root), `StateStore::set_snapshot`,
//! `load_state`, and `state_dir`.
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
//! ## What changed vs the pre-tabs schema
//!
//! The old granular setters (`set_geometry` / `set_cwd` / `set_sidebar_visible`
//! / `truncate_windows`) are GONE. State is now written as a WHOLE TREE via
//! `set_snapshot(WolfState)`: the front-end walks its live windows/tabs and
//! submits the entire tree at once. `state.json` is a tree — each window carries
//! geometry + a `tabs` list, each tab carries `{ cwd, sidebar_visible }`. There
//! is NO migration path: a flat (pre-tabs) file is unparseable in the new format
//! and is silently ignored.
//!
//! Also headless coverage for the durable user-directory surface: the
//! `spell_dir` / `runtime_user_dir` / `parsers_user_dir` resolvers,
//! `seed_runtime_user_dir`'s one-time-seed contract, and
//! `NvimConfig::from_env`'s ephemeral fallback when `DurableDirs` is empty.
//!
//! Needs NO nvim and no network: everything is driven through explicit
//! `tempfile` tempdirs passed to `StateStore::open` / `load_state` (or to the
//! seed function directly). Env-mutating cases (`WOLF_STATE_DIR`,
//! `WOLF_NVIM_BIN`) are serialized behind a LOCAL `ENV_LOCK`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

use tempfile::tempdir;
use wolf_core::nvim::{DurableDirs, NvimConfig};
use wolf_core::state::STATE_FILE_NAME;
use wolf_core::{
    load_state, parsers_user_dir, runtime_user_dir, seed_runtime_user_dir, spell_dir, state_dir,
    StateStore, TabState, WindowState, WolfState, INIT_USER_FILE_NAME,
};

/// Generous upper bound for `StateStore::flush` in tests: well beyond the
/// planned ~100ms write-coalesce delay, so a timeout always means a real bug.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// A pause inserted BEFORE re-submitting an unchanged snapshot, so that a
/// (buggy) rewrite would land at a strictly later mtime than the one captured
/// before the pause — guarding the "unchanged snapshot skips a write" assertion
/// against coarse filesystem mtime resolution. Longer than the ~100ms coalesce
/// window so a scheduled write would already have fired.
const REWRITE_OBSERVE_DELAY: Duration = Duration::from_millis(200);

// -- Distinctive window-0 geometry (AppKit bottom-left points; named per mnd) --
/// Window-0 x.
const W0_X: f64 = 128.0;
/// Window-0 y.
const W0_Y: f64 = 64.5;
/// Window-0 width.
const W0_WIDTH: f64 = 1180.0;
/// Window-0 height.
const W0_HEIGHT: f64 = 760.0;

// -- Distinctive window-1 geometry (different on every axis from window 0) -----
/// Window-1 x.
const W1_X: f64 = 555.0;
/// Window-1 y.
const W1_Y: f64 = 44.0;
/// Window-1 width.
const W1_WIDTH: f64 = 900.0;
/// Window-1 height.
const W1_HEIGHT: f64 = 600.0;

// -- Replacement tree geometry for the "snapshot replaces prior tree" case -----
/// Replacement single-window x.
const REPLACE_X: f64 = 10.0;
/// Replacement single-window y.
const REPLACE_Y: f64 = 20.0;
/// Replacement single-window width.
const REPLACE_WIDTH: f64 = 1024.0;
/// Replacement single-window height.
const REPLACE_HEIGHT: f64 = 768.0;

/// An unsupported on-disk schema version (only version 1 is accepted).
const UNSUPPORTED_VERSION: u64 = 2;

/// Serializes every test in this binary that mutates PROCESS-GLOBAL environment
/// (`WOLF_STATE_DIR` in the `state_dir`/resolver cases, `WOLF_NVIM_BIN` in the
/// `from_env` fallback case). cargo runs this file's tests concurrently within
/// ONE process. The non-env cases pass explicit dirs and never touch the
/// environment, so they do not need the lock.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// The greppable sentence `seed_runtime_user_dir`'s template must carry, re-typed
/// here deliberately: the test states its expectation independently of the source
/// constant, so changing the template silently fails here instead of moving with
/// it.
const TEMPLATE_SENTENCE: &str =
    "-- Wolf loads this file last, so anything here overrides Wolf's defaults.";

/// Reads a file to a `String`, panicking with context on failure (errcheck).
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Writes `contents` to `path`, panicking with context on failure (errcheck).
fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
}

/// Reads a file's modification time, panicking with context on failure.
fn mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
}

/// A tab with the given cwd + sidebar visibility (test builder). The new
/// per-tab pane `layout` (feature plan §4.1) is irrelevant to this file's
/// geometry/round-trip cases, so it is `None`; the dedicated `state_layout`
/// target covers the pane-tree round-trip.
fn tab(cwd: &str, sidebar_visible: bool) -> TabState {
    TabState {
        cwd: cwd.to_string(),
        sidebar_visible,
        layout: None,
    }
}

/// A window with the given geometry + tabs (test builder).
fn window(x: f64, y: f64, width: f64, height: f64, tabs: Vec<TabState>) -> WindowState {
    WindowState {
        x,
        y,
        width,
        height,
        tabs,
    }
}

/// The canonical two-window fixture tree: window 0 has TWO tabs (distinct cwd +
/// sidebar per tab), window 1 has ONE tab; every window has distinct geometry.
/// The cwds are literal strings — `load_state` never validates them, so they
/// need not exist on disk.
fn sample_tree() -> WolfState {
    WolfState {
        windows: vec![
            window(
                W0_X,
                W0_Y,
                W0_WIDTH,
                W0_HEIGHT,
                vec![
                    tab("/proj/window-zero/tab-a", true),
                    tab("/proj/window-zero/tab-b", false),
                ],
            ),
            window(
                W1_X,
                W1_Y,
                W1_WIDTH,
                W1_HEIGHT,
                vec![tab("/proj/window-one/only-tab", false)],
            ),
        ],
    }
}

/// A DIFFERENT tree (one window, one tab) used to prove a later snapshot fully
/// replaces the prior one.
fn replacement_tree() -> WolfState {
    WolfState {
        windows: vec![window(
            REPLACE_X,
            REPLACE_Y,
            REPLACE_WIDTH,
            REPLACE_HEIGHT,
            vec![tab("/proj/replacement/tab", true)],
        )],
    }
}

// =========================================================================
// Tree round-trip: set_snapshot -> flush -> load_state == the whole tree
// =========================================================================

#[test]
fn tree_round_trips_through_store_and_load() {
    let dir = tempdir().expect("create temp state dir");
    let tree = sample_tree();

    let store = StateStore::open(dir.path());
    store.set_snapshot(tree.clone());
    assert!(
        store.flush(FLUSH_TIMEOUT),
        "flush after set_snapshot should complete within {FLUSH_TIMEOUT:?}"
    );

    // The in-memory truth reflects the snapshot immediately.
    assert_eq!(store.state(), tree, "in-memory state == the submitted tree");

    // load_state reads back EXACTLY the whole tree (relies on WolfState: PartialEq).
    let loaded = load_state(dir.path()).expect("load_state should parse the file the store wrote");
    assert_eq!(
        loaded, tree,
        "the entire loaded tree (windows, geometry, per-tab cwd/sidebar) must equal the submitted tree"
    );

    // Spot-check the structural shape so a bad PartialEq derive can't hide a bug.
    assert_eq!(loaded.windows.len(), 2, "two windows");
    assert_eq!(loaded.windows[0].tabs.len(), 2, "window 0 has two tabs");
    assert_eq!(loaded.windows[1].tabs.len(), 1, "window 1 has one tab");

    // The on-disk form is PRETTY-PRINTED JSON with a version-1 header.
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
// Old (flat, pre-tabs) format is rejected: None + file left untouched
// =========================================================================

#[test]
fn old_flat_format_is_rejected_and_file_preserved() {
    let dir = tempdir().expect("create temp state dir");
    let path = dir.path().join(STATE_FILE_NAME);

    // A FLAT, pre-tabs version-1 file: window-level `cwd` / `sidebar_visible`,
    // NO `tabs`. The new schema requires `tabs` (no `#[serde(default)]`), so this
    // fails deserialization -> load_state returns None (no migration path).
    let flat = r#"{
  "version": 1,
  "windows": [
    {
      "x": 128.0,
      "y": 64.0,
      "width": 1180.0,
      "height": 760.0,
      "cwd": "/legacy/window-zero",
      "sidebar_visible": true
    }
  ]
}
"#;
    write(&path, flat);
    let before = read(&path);

    assert!(
        load_state(dir.path()).is_none(),
        "a flat (pre-tabs) file must load as None (no migration path)"
    );
    assert_eq!(
        read(&path),
        before,
        "load_state must leave the rejected flat file byte-for-byte unchanged"
    );

    // Opening the store over the ignored flat file degrades to defaults (empty).
    let store = StateStore::open(dir.path());
    assert!(
        store.state().windows.is_empty(),
        "open over a rejected flat file must yield the default (empty) tree"
    );
}

// =========================================================================
// Unchanged snapshot skips a write (no generation bump, no rewrite)
// =========================================================================

#[test]
fn unchanged_snapshot_skips_a_rewrite() {
    let dir = tempdir().expect("create temp state dir");
    let store = StateStore::open(dir.path());
    let tree = sample_tree();

    // First submission writes the file.
    store.set_snapshot(tree.clone());
    assert!(store.flush(FLUSH_TIMEOUT), "initial flush should complete");
    let path = dir.path().join(STATE_FILE_NAME);
    let first_mtime = mtime(&path);

    // Pause so a (buggy) rewrite would land at a strictly later mtime.
    sleep(REWRITE_OBSERVE_DELAY);

    // Re-submit the IDENTICAL tree: set_snapshot must detect equality and NOT
    // bump the generation, so the writer never re-fires. A behavioral proxy that
    // reaches into no private fields: the on-disk mtime is unchanged.
    store.set_snapshot(tree.clone());
    assert!(store.flush(FLUSH_TIMEOUT), "second flush should complete");
    let second_mtime = mtime(&path);

    assert_eq!(
        first_mtime, second_mtime,
        "re-submitting an unchanged snapshot must NOT rewrite state.json (mtime moved)"
    );
    // The in-memory tree is still the same value.
    assert_eq!(
        store.state(),
        tree,
        "state() unchanged after the no-op snapshot"
    );
}

// =========================================================================
// A later snapshot fully replaces the prior tree
// =========================================================================

#[test]
fn later_snapshot_replaces_prior_tree() {
    let dir = tempdir().expect("create temp state dir");
    let store = StateStore::open(dir.path());

    let first = sample_tree();
    store.set_snapshot(first);
    assert!(store.flush(FLUSH_TIMEOUT), "first flush should complete");

    let second = replacement_tree();
    store.set_snapshot(second.clone());
    assert!(store.flush(FLUSH_TIMEOUT), "second flush should complete");

    // The in-memory state is the REPLACEMENT, not a merge of the two.
    assert_eq!(
        store.state(),
        second,
        "state() must equal the replacement tree"
    );

    // The on-disk file parses back to exactly the replacement tree.
    let loaded = load_state(dir.path()).expect("rewritten file should parse");
    assert_eq!(
        loaded, second,
        "the on-disk tree must be fully REPLACED by the second snapshot (no window 1 residue)"
    );
    assert_eq!(
        loaded.windows.len(),
        1,
        "only the replacement's single window remains"
    );
}

// =========================================================================
// Silently-ignore matrix: absent / corrupt / wrong-version -> None
// =========================================================================

#[test]
fn absent_file_loads_none_and_open_yields_defaults() {
    let dir = tempdir().expect("create temp state dir");
    let path = dir.path().join(STATE_FILE_NAME);
    assert!(!path.exists(), "precondition: no state file yet");

    assert!(
        load_state(dir.path()).is_none(),
        "absent file must load as None"
    );
    assert!(
        !path.exists(),
        "load_state must NEVER create the state file"
    );

    let store = StateStore::open(dir.path());
    assert!(
        store.state().windows.is_empty(),
        "open over an absent file must yield the default (empty) tree"
    );
}

#[test]
fn corrupt_and_wrong_version_files_load_none_and_are_preserved() {
    /// One silently-ignore row: a file body that must make `load_state` return
    /// `None` without the file being created, modified, or deleted.
    struct Case {
        name: &'static str,
        contents: String,
    }

    let cases = [
        Case {
            name: "junk bytes (not JSON)",
            contents: "this is {{{ not json\n\t- at all\n".to_string(),
        },
        Case {
            name: "valid JSON, wrong shape (windows is an object)",
            contents: r#"{ "version": 1, "windows": { "x": 1.0 } }"#.to_string(),
        },
        Case {
            name: "missing the required `tabs` list (only geometry present)",
            contents: r#"{ "version": 1, "windows": [ { "x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0 } ] }"#
                .to_string(),
        },
        Case {
            name: "unsupported version (well-formed tree)",
            contents: format!(
                r#"{{
  "version": {UNSUPPORTED_VERSION},
  "windows": [
    {{
      "x": 1.0,
      "y": 2.0,
      "width": 3.0,
      "height": 4.0,
      "tabs": [ {{ "cwd": "/somewhere", "sidebar_visible": true }} ]
    }}
  ]
}}"#
            ),
        },
    ];

    for case in &cases {
        let dir = tempdir().expect("create temp state dir");
        let path = dir.path().join(STATE_FILE_NAME);
        write(&path, &case.contents);
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

        let store = StateStore::open(dir.path());
        assert!(
            store.state().windows.is_empty(),
            "{}: open over an ignored file must yield the default (empty) tree",
            case.name
        );
    }
}

#[test]
fn version_must_be_one() {
    let dir = tempdir().expect("create temp state dir");
    // Write a valid version-1 tree through the store, then confirm the header.
    let store = StateStore::open(dir.path());
    store.set_snapshot(sample_tree());
    assert!(store.flush(FLUSH_TIMEOUT), "flush should complete");

    let text = read(&dir.path().join(STATE_FILE_NAME));
    assert!(
        text.contains("\"version\": 1"),
        "the writer must stamp version 1 into state.json, got:\n{text}"
    );
}

// =========================================================================
// state_dir — WOLF_STATE_DIR override (env-mutating: serialized)
// =========================================================================

#[test]
fn state_dir_honors_env_override() {
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

// =========================================================================
// swap_dir_for — STABLE per canonical cwd, DISTINCT across cwds (plan §6)
// (env-mutating: WOLF_STATE_DIR, so serialized behind ENV_LOCK)
// =========================================================================

/// `swap_dir_for(cwd)` resolves the durable per-project swap dir
/// (`state_dir()/swap/<stable hash of the canonical cwd>`). The property the
/// design leans on: it is DETERMINISTIC for a fixed cwd (so a respawn — even in a
/// separate Wolf process — reuses the SAME dir and finds the prior swap) and
/// DISTINCT for distinct projects (so their swaps never collide). Under a
/// `WOLF_STATE_DIR` override it must resolve to `Some(...)` under that dir.
#[test]
fn swap_dir_for_is_stable_per_cwd_and_distinct_across_cwds() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

    let state = tempdir().expect("create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state.path());

    // Two DISTINCT project cwds, canonicalized so the hash input is stable.
    let proj_a = tempdir().expect("create temp cwd A");
    let proj_b = tempdir().expect("create temp cwd B");
    let cwd_a = std::fs::canonicalize(proj_a.path()).expect("canonicalize cwd A");
    let cwd_b = std::fs::canonicalize(proj_b.path()).expect("canonicalize cwd B");

    let a1 = wolf_core::state::swap_dir_for(&cwd_a);
    let a2 = wolf_core::state::swap_dir_for(&cwd_a);
    let b1 = wolf_core::state::swap_dir_for(&cwd_b);

    std::env::remove_var("WOLF_STATE_DIR");

    // Resolvable under the override.
    let a1 = a1.expect("swap_dir_for(cwd_a) should resolve under WOLF_STATE_DIR");
    let a2 = a2.expect("swap_dir_for(cwd_a) should resolve under WOLF_STATE_DIR");
    let b1 = b1.expect("swap_dir_for(cwd_b) should resolve under WOLF_STATE_DIR");

    // STABLE: same canonical cwd -> same swap dir (deterministic hash).
    assert_eq!(
        a1, a2,
        "swap_dir_for must be STABLE for a fixed canonical cwd (found {a1:?} vs {a2:?})"
    );
    // DISTINCT: different cwds -> different swap dirs.
    assert_ne!(
        a1, b1,
        "swap_dir_for must be DISTINCT for distinct cwds (both resolved to {a1:?})"
    );
    // Rooted under the overridden state dir's `swap/` subtree.
    let swap_root = state.path().join("swap");
    assert!(
        a1.starts_with(&swap_root),
        "swap_dir_for(cwd_a) = {a1:?} must live under {swap_root:?}"
    );
    assert!(
        b1.starts_with(&swap_root),
        "swap_dir_for(cwd_b) = {b1:?} must live under {swap_root:?}"
    );
}

// =========================================================================
// spell / runtime_user / parsers_user resolvers (env-mutating: serialized)
// =========================================================================

/// The three durable-dir resolvers are `state_dir()` joined with a fixed leaf.
/// The leaf names are the contract with `runtime/nvim/init.lua` and the
/// `WOLF_*` env exports, so a changed `join` argument fails here.
#[test]
fn spell_runtime_parsers_dirs_resolve_under_state_dir() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

    let dir = tempdir().expect("create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", dir.path());

    let spell = spell_dir();
    let runtime_user = runtime_user_dir();
    let parsers_user = parsers_user_dir();

    std::env::remove_var("WOLF_STATE_DIR");

    assert_eq!(
        spell,
        Some(dir.path().join("spell")),
        "spell_dir() must resolve to <state_dir>/spell (a different leaf breaks the \
         WOLF_SPELL_DIR contract with init.lua)"
    );
    assert_eq!(
        runtime_user,
        Some(dir.path().join("runtime_user")),
        "runtime_user_dir() must resolve to <state_dir>/runtime_user (a different leaf \
         breaks the WOLF_RUNTIME_USER_DIR contract with init.lua)"
    );
    assert_eq!(
        parsers_user,
        Some(dir.path().join("parsers_user")),
        "parsers_user_dir() must resolve to <state_dir>/parsers_user (a different leaf \
         breaks the WOLF_PARSERS_USER_DIR contract with init.lua)"
    );
}

// =========================================================================
// seed_runtime_user_dir — the one-time-seed contract (no env)
// =========================================================================

#[test]
fn seed_runtime_user_seeds_missing_and_empty_dirs() {
    // A MISSING dir is created and seeded.
    let root = tempdir().expect("create temp dir");
    let missing = root.path().join("runtime_user");
    assert!(
        !missing.exists(),
        "precondition: the dir must not exist yet"
    );
    let seeded = seed_runtime_user_dir(&missing).expect("seeding a missing dir should be Ok");
    assert!(
        seeded,
        "seeding a MISSING dir must report Ok(true); Ok(false) without a write \
         disables the whole seed path"
    );
    let text = read(&missing.join(INIT_USER_FILE_NAME));
    assert!(
        text.contains(TEMPLATE_SENTENCE),
        "the seeded template must carry the greppable override sentence \
         {TEMPLATE_SENTENCE:?}; got:\n{text}"
    );

    // An existing-but-EMPTY dir is seeded too (the user has claimed nothing).
    let empty_root = tempdir().expect("create temp dir");
    let empty = empty_root.path().join("runtime_user");
    std::fs::create_dir_all(&empty).expect("create empty runtime_user dir");
    let seeded = seed_runtime_user_dir(&empty).expect("seeding an empty dir should be Ok");
    assert!(seeded, "seeding an EMPTY dir must report Ok(true)");
    assert!(
        empty.join(INIT_USER_FILE_NAME).is_file(),
        "seeding an empty dir must write the template file"
    );

    // The Rust const and init.lua's hardcoded dofile path must agree on the
    // name; a renamed const silently orphans the seeded file.
    assert_eq!(
        INIT_USER_FILE_NAME, "init_user.lua",
        "init.lua dofile's the literal name `init_user.lua`"
    );
}

#[test]
fn seed_runtime_user_never_touches_non_empty_dir() {
    // Non-empty WITHOUT init_user.lua: nothing is written — not even the
    // template. Seeding whenever the template is merely absent would write into
    // a dir the user has claimed (perhaps having deleted the template on
    // purpose).
    let root = tempdir().expect("create temp dir");
    let dir = root.path().join("runtime_user");
    std::fs::create_dir_all(&dir).expect("create runtime_user dir");
    let notes = dir.join("notes.txt");
    let notes_body = "user notes, hands off\n";
    write(&notes, notes_body);

    let seeded = seed_runtime_user_dir(&dir).expect("seeding a non-empty dir should be Ok");
    assert!(
        !seeded,
        "a NON-EMPTY dir must report Ok(false): the dir is the user's once anything is in it"
    );
    assert!(
        !dir.join(INIT_USER_FILE_NAME).exists(),
        "no template may be written into a non-empty dir, even when init_user.lua is absent"
    );
    assert_eq!(
        read(&notes),
        notes_body,
        "the user's file must be byte-identical after the seed call"
    );

    // Non-empty WITH a custom init_user.lua: byte-identical.
    let root2 = tempdir().expect("create temp dir");
    let dir2 = root2.path().join("runtime_user");
    std::fs::create_dir_all(&dir2).expect("create runtime_user dir");
    let custom = "-- the user's own init_user.lua\nvim.g.wolf_custom = 1\n";
    let user_file = dir2.join(INIT_USER_FILE_NAME);
    write(&user_file, custom);

    let seeded = seed_runtime_user_dir(&dir2).expect("seeding over a custom file should be Ok");
    assert!(
        !seeded,
        "a dir holding a custom init_user.lua must report Ok(false)"
    );
    assert_eq!(
        read(&user_file),
        custom,
        "a custom init_user.lua must never be rewritten (byte-identical)"
    );
}

// =========================================================================
// NvimConfig::from_env — ephemeral fallback for an empty DurableDirs
// (env-mutating: WOLF_NVIM_BIN, so serialized)
// =========================================================================

/// With every `DurableDirs` field `None` (an unresolvable `state_dir()`),
/// `from_env` must fall back to EPHEMERAL dirs under its private `state_root`
/// for swap and spell — so `zg` and swap recovery still work within one session
/// and nothing panics — and carry `None` for the optional dirs, so spawn skips
/// their creation and exports and init.lua no-ops.
#[test]
fn from_env_falls_back_to_ephemeral_spell_dir() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

    // from_env only requires WOLF_NVIM_BIN to be non-empty; it never validates
    // the path, so a dummy value keeps this test headless.
    let saved = std::env::var_os("WOLF_NVIM_BIN");
    std::env::set_var("WOLF_NVIM_BIN", "/nonexistent/wolf-test-nvim");

    let cfg = NvimConfig::from_env(
        PathBuf::from("/nonexistent/init.lua"),
        PathBuf::from("/nonexistent/cwd"),
        DurableDirs::default(),
        80,
        24,
    );

    match saved {
        Some(v) => std::env::set_var("WOLF_NVIM_BIN", v),
        None => std::env::remove_var("WOLF_NVIM_BIN"),
    }

    let cfg = cfg.expect("from_env should succeed with a non-empty WOLF_NVIM_BIN");
    assert_eq!(
        cfg.spell_dir,
        cfg.state_root.join("spell"),
        "with no durable spell dir, from_env must fall back to the ephemeral \
         state_root/spell (removing the fallback would leave 'spellfile' pointing \
         nowhere and zg failing)"
    );
    assert_eq!(
        cfg.swap_dir,
        cfg.state_root.join("swap"),
        "with no durable swap dir, from_env must fall back to the ephemeral state_root/swap"
    );
    assert!(
        cfg.runtime_user_dir.is_none(),
        "an absent runtime_user dir must stay None (spawn then skips creation and export)"
    );
    assert!(
        cfg.parsers_user_dir.is_none(),
        "an absent parsers_user dir must stay None (spawn then skips creation and export)"
    );
}
