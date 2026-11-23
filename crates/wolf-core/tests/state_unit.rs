//! Headless coverage for the directory surface of `wolf_core::state`: `state_dir` and its
//! `WOLF_STATE_DIR` override, `swap_dir_for`, the `spell_dir` / `runtime_user_dir` /
//! `parsers_user_dir` resolvers, `seed_runtime_user_dir`'s one-time-seed contract, and
//! `NvimConfig::from_env`'s ephemeral fallback when `DurableDirs` is empty.
//!
//! Everything asserted is public behavior, so the cases run as an integration-style binary
//! against the crate-root re-exports and need no access to the module's private items.
//!
//! Needs no nvim and no network: everything is driven through explicit `tempfile` tempdirs.
//! Env-mutating cases (`WOLF_STATE_DIR`, `WOLF_NVIM_BIN`) are serialized behind a local
//! `ENV_LOCK`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tempfile::tempdir;
use wolf_core::nvim::{DurableDirs, NvimConfig};
use wolf_core::{
    parsers_user_dir, runtime_user_dir, seed_runtime_user_dir, spell_dir, state_dir,
    INIT_USER_FILE_NAME,
};

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

// === state_dir: WOLF_STATE_DIR override (env-mutating, serialized) ===

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

// === swap_dir_for: stable per canonical cwd, distinct across cwds (env-mutating, ENV_LOCK) ===

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

// === spell / runtime_user / parsers_user resolvers (env-mutating, serialized) ===

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

// === seed_runtime_user_dir: the one-time-seed contract (no env) ===

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
    // Non-empty without init_user.lua: nothing is written, not even the template, since the user
    // has claimed the dir and may have deleted the template on purpose.
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

// === NvimConfig::from_env: ephemeral fallback for an empty DurableDirs (env-mutating) ===

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
