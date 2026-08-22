//! Integration test: spawn a real nvim, attach as an ext_linegrid UI, and verify
//! the initial render-state snapshot is well-formed. Requires `WOLF_NVIM_BIN`.
//!
//! Also covers the launch-configuration contract around the DURABLE directories
//! `spawn` prepares for the child: which paths `NvimConfig::from_env` resolves
//! (including the fallback taken when a durable dir cannot be resolved at all),
//! and the fact that starting a core leaves the spell dir on disk. The `from_env`
//! cases are headless — they build a config and start no nvim.

mod common;

use common::{DEFAULT_COLS, DEFAULT_ROWS};
use wolf_core::nvim::NvimConfig;

/// Subdirectory of a state root that holds the user spell dictionary. Spelled
/// out here rather than derived from `state::spell_dir()`, so the assertions
/// cannot agree with the implementation through a shared bug.
const SPELL_SUBDIR: &str = "spell";

/// Subdirectory of a state root that holds swap files when no durable swap dir
/// resolves.
const SWAP_SUBDIR: &str = "swap";

#[test]
fn spawn_and_attach() {
    // start_core_sized panics loudly if WOLF_NVIM_BIN is unset.
    let (core, _waiter) = common::start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);

    let snap = core.read_snapshot();

    // Under ext_multigrid the global/outer grid (id 1) is sized to exactly what we
    // requested in the ui_attach; window grids (>= 2) take the window content area.
    let global = snap.grid(1).expect("global grid 1 must exist after attach");
    assert_eq!(
        global.cols, DEFAULT_COLS as usize,
        "grid cols should match the requested attach size"
    );
    assert_eq!(
        global.rows, DEFAULT_ROWS as usize,
        "grid rows should match the requested attach size"
    );

    // default_colors_set should have arrived during attach, populating defaults.
    let defaults = &snap.highlights.defaults;
    assert!(
        defaults.fg.is_some() || defaults.bg.is_some(),
        "highlight defaults should be populated after default_colors_set \
         (got fg={:?}, bg={:?})",
        defaults.fg,
        defaults.bg
    );

    // mode_info_set should have populated the cached mode descriptors.
    assert!(
        !snap.mode_infos.is_empty(),
        "mode_infos should be non-empty after mode_info_set"
    );

    // Clean shutdown must succeed.
    core.shutdown().expect("shutdown should be Ok");
}

/// `NvimConfig::from_env(…, None, None, …)` — the shape used when `state_dir()`
/// cannot be resolved — must still name REAL, private directories for both
/// durable dirs, rooted at the config's own ephemeral `state_root`.
///
/// The spell half is the one with a silent failure mode: nvim does not create the
/// parent of an explicitly-set `'spellfile'`, and a `None` that leaked through as
/// an empty or shared path would make `zg` fail `E484` with no visible error. The
/// documented contract is that the fallback degrades only DURABILITY (the words
/// do not outlive the process), never the gesture itself.
///
/// Headless: `from_env` builds a config and starts no child.
#[test]
fn from_env_falls_back_to_ephemeral_durable_dirs_under_the_state_root() {
    // from_env reads WOLF_NVIM_BIN; resolve it to an absolute path first.
    let _ = common::require_nvim_bin();

    let cfg = NvimConfig::from_env(
        common::init_lua_path(),
        common::fresh_cwd(),
        None,
        None,
        DEFAULT_COLS,
        DEFAULT_ROWS,
    )
    .expect("from_env should build a config once WOLF_NVIM_BIN resolves");

    assert_eq!(
        cfg.spell_dir,
        cfg.state_root.join(SPELL_SUBDIR),
        "an unresolvable durable spell dir must fall back to a private, writable dir under the \
         config's own state root, so `zg` still succeeds within the session"
    );
    assert_eq!(
        cfg.swap_dir,
        cfg.state_root.join(SWAP_SUBDIR),
        "an unresolvable durable swap dir must fall back under the same state root"
    );
    assert_ne!(
        cfg.spell_dir, cfg.swap_dir,
        "the spell and swap fallbacks must be separate dirs; sharing one would let nvim's swap \
         files sit where the dictionary is compiled"
    );
}

/// An explicitly supplied durable dir reaches the field it belongs to.
///
/// The two `Option<PathBuf>` parameters are adjacent and identically typed, so a
/// transposition compiles cleanly and would send every taught word into the swap
/// dir (and every swap file into the dictionary dir) — invisible to a test that
/// only ever passes `None` for both. Distinct values are what catch it.
///
/// Headless: `from_env` builds a config and starts no child.
#[test]
fn from_env_routes_explicit_durable_dirs_to_the_matching_fields() {
    let _ = common::require_nvim_bin();

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let swap = dir.path().join("durable-swap");
    let spell = dir.path().join("durable-spell");

    let cfg = NvimConfig::from_env(
        common::init_lua_path(),
        common::fresh_cwd(),
        Some(swap.clone()),
        Some(spell.clone()),
        DEFAULT_COLS,
        DEFAULT_ROWS,
    )
    .expect("from_env should build a config once WOLF_NVIM_BIN resolves");

    assert_eq!(
        cfg.swap_dir, swap,
        "an explicit durable swap dir must reach `swap_dir` unchanged"
    );
    assert_eq!(
        cfg.spell_dir, spell,
        "an explicit durable spell dir must reach `spell_dir` unchanged; holding the swap value \
         here means the two Option parameters are transposed"
    );
}

/// Starting a core must leave the durable spell directory PRESENT on disk.
///
/// `spawn` creates it because nvim does not: with an explicitly-set `'spellfile'`
/// whose parent is missing, `zg` fails `E484` and the word is dropped with no
/// file and no error the user ever sees. nvim auto-creates only for the
/// runtimepath-derived default, which is why the omission looks harmless.
///
/// The durable path is derived here from the hermetic `WOLF_STATE_DIR` the shared
/// harness pins, so it is the same dir the core resolves through `state_dir()`
/// without this test calling `spell_dir()` to compute its own expectation.
#[test]
fn spawn_creates_the_durable_spell_dir() {
    let spell_dir = common::hermetic_state_dir().join(SPELL_SUBDIR);

    let (core, _waiter) = common::start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);

    assert!(
        spell_dir.is_dir(),
        "spawn must create {} before launching nvim, or `zg` fails E484 and silently drops the \
         word the user taught",
        spell_dir.display()
    );

    core.shutdown().expect("shutdown should be Ok");
}
