//! Headless test of `wolf_ffi_app_state_project_tab`, which macOS asks which open tab Open Folder
//! focuses (`specs/application-state.md` §6 step 1, choosing by §4's rule).
//!
//! The live windows are fed through the same `wolf_ffi_app_state_*` builder the snapshot uses,
//! minus the commit, and the answer is read through `FfiProjectTab`. Two scratches pin the two
//! halves of §4's rule: rule 1, a window whose active tab is in the directory, wins over an
//! earlier window holding it in a background tab; rule 2, with no active tab in it, the pushed
//! recency picks the later of two background tabs, where tab order alone would pick the earlier.
//! The window and tab answers differ in both scratches, so a swap of the two shows.
//!
//! The lookup never opens the store: `OpenFolderSeamTests.swift` calls it outside the store's
//! fence. `WOLF_STATE_DIR` points at a fresh dir, and the test asserts no `wolf.sqlite` appears.
//!
//! One `#[test]`: the builder's scratch is process-wide, so two tests in one binary would race
//! it.

use wolf_ffi::{
    wolf_ffi_app_state_add_tab, wolf_ffi_app_state_add_window, wolf_ffi_app_state_begin,
    wolf_ffi_app_state_project_tab, wolf_ffi_app_state_push_recency,
};

// `pane_ref`
const REF_NONE: u8 = 0;

const D: &str = "/ffi/open/project";
const E: &str = "/ffi/open/elsewhere";
const UNSEEN: &str = "/ffi/open/unseen";

/// Pushes a window whose `active_tab` is `active`, holding `tabs` in order, with `recency`.
fn push_window(active: u32, tabs: &[&str], recency: &[u32]) {
    wolf_ffi_app_state_add_window(true, 0.0, 0.0, 800.0, 600.0, active);
    for directory in tabs {
        wolf_ffi_app_state_add_tab((*directory).to_owned(), true, REF_NONE, 0, REF_NONE, 0);
    }
    for &tab in recency {
        wolf_ffi_app_state_push_recency(tab);
    }
}

/// The lookup's answer for `directory` as `Some((window, tab))`, `None` when not found.
fn lookup(directory: &str) -> Option<(u32, u32)> {
    let answer = wolf_ffi_app_state_project_tab(directory.to_owned());
    answer.found().then(|| (answer.window(), answer.tab()))
}

#[test]
fn project_tab_picks_the_tab_open_folder_focuses() {
    let state_dir = tempfile::tempdir().expect("failed to create the state tempdir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    // Scratch A: window 0 holds D in a background tab; window 1's active tab is in D.
    wolf_ffi_app_state_begin();
    push_window(0, &[E, D], &[0, 1]);
    push_window(0, &[D], &[0]);
    assert_eq!(
        lookup(D),
        Some((1, 0)),
        "§4 rule 1: window 1's active tab is in D, so it wins over window 0's background D tab. \
         Some((0, 1)) means the window and tab indices are swapped, or rule 1 was skipped for \
         window 0's first D tab; None means the lookup ignores the scratch"
    );

    // Scratch B: no active tab in D; the recency ranks tab 2 above tab 1.
    wolf_ffi_app_state_begin();
    push_window(0, &[E, D, D], &[0, 2, 1]);
    assert_eq!(
        lookup(D),
        Some((0, 2)),
        "§4 rule 2: the most recently selected D tab is tab 2. Some((0, 1)) means the pushed \
         recency did not reach project_tab and tab order decided; Some((2, 0)) means the \
         window and tab indices are swapped"
    );

    assert_eq!(
        lookup(""),
        None,
        "an empty directory names an empty tab and is never found, though window 0 has tabs"
    );
    assert_eq!(
        lookup(UNSEEN),
        None,
        "a directory no tab is open on must not be found"
    );

    let store = state_dir.path().join("wolf.sqlite");
    assert!(
        !store.exists(),
        "wolf_ffi_app_state_project_tab must answer from the builder's scratch alone and never \
         open the app-state store; {} exists",
        store.display()
    );
}
