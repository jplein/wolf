//! Integration test: spawn a real nvim, attach as an ext_linegrid UI, and verify
//! the initial render-state snapshot is well-formed. Requires `WOLF_NVIM_BIN`.

mod common;

use common::{DEFAULT_COLS, DEFAULT_ROWS};

#[test]
fn spawn_and_attach() {
    // start_core_sized panics loudly if WOLF_NVIM_BIN is unset.
    let (core, _waiter) = common::start_core_sized(DEFAULT_COLS, DEFAULT_ROWS);

    let snap = core.read_snapshot();

    // Grid was sized to exactly what we requested in the ui_attach.
    assert_eq!(
        snap.grid.cols, DEFAULT_COLS as usize,
        "grid cols should match the requested attach size"
    );
    assert_eq!(
        snap.grid.rows, DEFAULT_ROWS as usize,
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
