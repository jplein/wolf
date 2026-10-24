//! Integration test: spawn a real nvim, attach as an ext_linegrid UI, and verify
//! the initial render-state snapshot is well-formed — plus that every core start
//! creates the durable user directories and exports their `WOLF_*` env vars.
//! Requires `WOLF_NVIM_BIN`.

mod common;

use common::{DEFAULT_COLS, DEFAULT_ROWS};

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

/// Every core start must create the durable user-writeable directories under
/// `state_dir()` and export their paths into the captive nvim's environment.
/// One assertion per create/seed call and one per `.env(…)` export, so
/// disabling any single one fails exactly one assertion.
///
/// Uses the SHARED hermetic state dir (`common::hermetic_state_dir()`, the same
/// dir the core resolves via `state_dir()`), so no env mutation is needed and
/// the test stays concurrent-safe within this binary.
#[test]
fn durable_dirs_created_and_exported() {
    let (core, _waiter) = common::start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);
    let state = common::hermetic_state_dir();

    // On-disk halves — each pins one create_dir_all / seed call in core.
    assert!(
        state.join("spell").is_dir(),
        "spawn must create <state_dir>/spell before launch: nvim never creates the parent \
         of an explicit 'spellfile', so a missing dir makes zg fail silently (E484)"
    );
    assert!(
        state.join("parsers_user").is_dir(),
        "spawn must create <state_dir>/parsers_user before launch (it is the durable \
         :TSInstall target)"
    );
    assert!(
        state.join("runtime_user").join("init_user.lua").is_file(),
        "WolfCore::start must seed <state_dir>/runtime_user with init_user.lua when the \
         dir is missing or empty (a missing file means the seed call in api.rs is gone)"
    );

    // In-nvim halves pin each `.env(…)` export in spawn: `$VAR` reads the child's environment, so
    // an empty value means that export is gone even though the dir above exists.
    let probe_dir = tempfile::tempdir().expect("create probe dir");
    let vals = common::probe_values(
        &core,
        &[
            "$WOLF_SPELL_DIR",
            "$WOLF_RUNTIME_USER_DIR",
            "$WOLF_PARSERS_USER_DIR",
        ],
        &probe_dir.path().join("env.txt"),
    );
    assert_eq!(
        vals[0],
        state.join("spell").to_string_lossy(),
        "WOLF_SPELL_DIR must be exported to the captive nvim and point at <state_dir>/spell"
    );
    assert_eq!(
        vals[1],
        state.join("runtime_user").to_string_lossy(),
        "WOLF_RUNTIME_USER_DIR must be exported to the captive nvim and point at \
         <state_dir>/runtime_user"
    );
    assert_eq!(
        vals[2],
        state.join("parsers_user").to_string_lossy(),
        "WOLF_PARSERS_USER_DIR must be exported to the captive nvim and point at \
         <state_dir>/parsers_user"
    );

    core.shutdown().expect("shutdown should be Ok");
}
