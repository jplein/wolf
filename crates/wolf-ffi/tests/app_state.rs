//! Headless test of the `wolf-ffi` application-state surface that macOS builds its snapshot
//! through: the `wolf_ffi_app_state_*` builder, the submit with tab recency, and the two
//! readers, `wolf_ffi_app_state_window_state` for launch and
//! `wolf_ffi_app_state_project_state` for Open Folder.
//!
//! It drives the builder over the process-wide app-state store, which opens `wolf.sqlite` in
//! `WOLF_STATE_DIR`, and reads every value back through the accessors. The project state read
//! picks its tab by `specs/application-state.md` §4's rule, so the recency the builder submits
//! decides which of two tabs in one directory is read back.
//!
//! One `#[test]`: it sets the process-global `WOLF_STATE_DIR` before the store opens, and the
//! store is a process-wide `OnceLock`, so a second test could race it or find it opened
//! elsewhere. The discriminants are local constants mirroring the bridge's private tables.

use wolf_ffi::{
    wolf_ffi_app_state_add_tab, wolf_ffi_app_state_add_window, wolf_ffi_app_state_begin,
    wolf_ffi_app_state_commit, wolf_ffi_app_state_commit_root, wolf_ffi_app_state_flush,
    wolf_ffi_app_state_pane_cursor, wolf_ffi_app_state_pane_scroll,
    wolf_ffi_app_state_project_state, wolf_ffi_app_state_push_layout, wolf_ffi_app_state_push_pane,
    wolf_ffi_app_state_push_recency, wolf_ffi_app_state_tab_drawer, wolf_ffi_app_state_tab_sidebar,
    wolf_ffi_app_state_window_state,
};

// `pane_kind`
const EDIT: u8 = 0;
const PREVIEW: u8 = 1;
const TERMINAL: u8 = 2;
// `pane_ref`
const REF_NONE: u8 = 0;
const REF_PANE: u8 = 1;
const REF_DRAWER: u8 = 2;
// `sidebar_mode`
const MODE_UNSET: u8 = 0;
const MODE_SEARCH: u8 = 2;
// `tristate`
const UNSET: u8 = 0;
const OFF: u8 = 1;
const ON: u8 = 2;
// `orientation`
const VERTICAL: u8 = 1;

/// Window 0's frame, every field distinct from window 1's.
const W0: (f64, f64, f64, f64) = (100.0, 200.0, 1400.0, 900.0);
/// Window 1's frame; its position is passed but marked unknown, so it must read back as 0.
const W1: (f64, f64, f64, f64) = (55.0, 66.0, 1024.0, 768.0);

const D1: &str = "/ffi/first";
const D3: &str = "/ffi/third";
const D1_FILE: &str = "/ffi/first/README.md";
const D3_RECENT_FILE: &str = "/ffi/third/notes.md";

#[test]
fn the_app_state_builder_round_trips_through_the_shared_store() {
    let state_dir = tempfile::tempdir().expect("failed to create the state tempdir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    assert!(
        wolf_ffi_app_state_window_state().windows().is_empty(),
        "premise: a fresh state dir has no saved window state"
    );

    wolf_ffi_app_state_begin();
    wolf_ffi_app_state_add_window(true, W0.0, W0.1, W0.2, W0.3, 0);

    // Tab 0, D1: a vertical layout of an edit pane and a preview pane, sidebar and drawer set.
    wolf_ffi_app_state_add_tab(D1.to_owned(), true, REF_PANE, 1, REF_NONE, 0);
    wolf_ffi_app_state_tab_sidebar(MODE_SEARCH, "TODO".to_owned(), true, false, true);
    wolf_ffi_app_state_tab_drawer(true, 0.375);
    wolf_ffi_app_state_push_pane(EDIT, D1_FILE.to_owned(), 0.625);
    wolf_ffi_app_state_pane_cursor(12, 4, 7);
    wolf_ffi_app_state_push_pane(PREVIEW, D1_FILE.to_owned(), 0.375);
    wolf_ffi_app_state_pane_scroll(0.25);
    wolf_ffi_app_state_push_layout(VERTICAL, 2, 1.0);
    wolf_ffi_app_state_commit_root();

    // Tab 1, D3: a lone terminal pane, focused on the drawer and zoomed on its pane.
    wolf_ffi_app_state_add_tab(D3.to_owned(), false, REF_DRAWER, 0, REF_PANE, 0);
    wolf_ffi_app_state_push_pane(TERMINAL, D3.to_owned(), 1.0);
    wolf_ffi_app_state_commit_root();

    // Tab 2, D3 again: a lone edit pane, which the recency below makes D3's project state.
    wolf_ffi_app_state_add_tab(D3.to_owned(), true, REF_NONE, 0, REF_NONE, 0);
    wolf_ffi_app_state_push_pane(EDIT, D3_RECENT_FILE.to_owned(), 1.0);
    wolf_ffi_app_state_commit_root();

    for tab in [0, 2, 1] {
        wolf_ffi_app_state_push_recency(tab);
    }

    // Window 1: no position, one empty tab with no root.
    wolf_ffi_app_state_add_window(false, W1.0, W1.1, W1.2, W1.3, 0);
    wolf_ffi_app_state_add_tab(String::new(), true, REF_NONE, 0, REF_NONE, 0);

    wolf_ffi_app_state_commit();
    assert!(
        wolf_ffi_app_state_flush(),
        "wolf_ffi_app_state_flush must return true once the committed state's write is attempted"
    );

    let windows = wolf_ffi_app_state_window_state().windows();
    assert_eq!(
        windows.len(),
        2,
        "the committed state must read back as two windows; 0 means commit submitted nothing or \
         the read does not reach the shared store"
    );

    // --- Window 0 ---
    let w0 = &windows[0];
    assert!(w0.has_position(), "window 0 was added with a position");
    assert_eq!(
        (w0.x(), w0.y(), w0.width(), w0.height()),
        W0,
        "window 0's frame (x, y, width, height)"
    );
    assert_eq!(w0.active_tab(), 0, "window 0's active tab");
    let tabs = w0.tabs();
    assert_eq!(tabs.len(), 3, "window 0 holds three tabs");

    let t0 = &tabs[0];
    assert_eq!(t0.directory(), D1, "tab 0's directory");
    assert_eq!(
        (t0.focused_kind(), t0.focused_index()),
        (REF_PANE, 1),
        "tab 0 is focused on pane 1"
    );
    assert_eq!(t0.zoomed_kind(), REF_NONE, "tab 0 is not zoomed");
    assert!(t0.sidebar_visible(), "tab 0's sidebar is visible");
    assert_eq!(
        t0.sidebar_mode(),
        MODE_SEARCH,
        "tab 0's sidebar shows search"
    );
    assert!(t0.has_query(), "tab 0's search query was set");
    assert_eq!(t0.query(), "TODO", "tab 0's search query");
    assert_eq!(
        (t0.case_sensitive(), t0.use_regex(), t0.all_files()),
        (ON, OFF, ON),
        "tab 0's toggles (case_sensitive, use_regex, all_files) as tristates"
    );
    assert!(t0.has_drawer(), "tab 0's drawer was set");
    assert!(t0.drawer_open(), "tab 0's drawer is open");
    assert_eq!(t0.drawer_share(), 0.375, "tab 0's drawer share");

    assert!(t0.has_root(), "tab 0 has a root");
    let root = t0.root();
    assert!(!root.is_pane(), "tab 0's root is a layout");
    assert_eq!(root.orientation(), VERTICAL, "tab 0's layout is vertical");
    assert_eq!(root.child_count(), 2, "tab 0's layout holds two panes");
    assert_eq!(
        (root.weight(0), root.weight(1)),
        (0.625, 0.375),
        "tab 0's layout weights, in child order"
    );
    let editor = root.child(0);
    assert!(editor.is_pane(), "tab 0's child 0 is a pane");
    assert_eq!(editor.kind(), EDIT, "tab 0's child 0 is an edit pane");
    assert_eq!(editor.path(), D1_FILE, "tab 0's edit pane path");
    assert!(editor.has_cursor(), "the edit pane's cursor was set");
    assert_eq!(
        (editor.cursor_line(), editor.cursor_col()),
        (12, 4),
        "the edit pane's cursor (line, col)"
    );
    assert!(editor.has_top_line(), "the edit pane's top line was set");
    assert_eq!(editor.top_line(), 7, "the edit pane's top line");
    assert!(!editor.has_scroll(), "an edit pane has no scroll");
    let viewer = root.child(1);
    assert_eq!(
        viewer.kind(),
        PREVIEW,
        "tab 0's child 1 was pushed as a preview pane; EDIT means push_pane drops the kind"
    );
    assert_eq!(viewer.path(), D1_FILE, "the preview pane's source path");
    assert!(viewer.has_scroll(), "the preview pane's scroll was set");
    assert_eq!(viewer.scroll(), 0.25, "the preview pane's scroll");
    assert!(
        !viewer.has_cursor() && !viewer.has_top_line(),
        "a preview pane has no cursor or top line"
    );

    let t1 = &tabs[1];
    assert_eq!(t1.directory(), D3, "tab 1's directory");
    assert_eq!(
        t1.focused_kind(),
        REF_DRAWER,
        "tab 1 is focused on the drawer"
    );
    assert_eq!(
        (t1.zoomed_kind(), t1.zoomed_index()),
        (REF_PANE, 0),
        "tab 1 is zoomed on pane 0"
    );
    assert!(!t1.sidebar_visible(), "tab 1's sidebar is hidden");
    assert_eq!(
        t1.sidebar_mode(),
        MODE_UNSET,
        "tab 1 never set a sidebar mode"
    );
    assert!(!t1.has_query(), "tab 1 never set a query");
    assert_eq!(
        (t1.case_sensitive(), t1.use_regex(), t1.all_files()),
        (UNSET, UNSET, UNSET),
        "tab 1 never set its toggles, so each reads UNSET"
    );
    assert!(!t1.has_drawer(), "tab 1 never set a drawer");
    let shell = t1.root();
    assert!(
        shell.is_pane() && shell.kind() == TERMINAL,
        "tab 1's root is a lone terminal pane"
    );
    assert_eq!(shell.path(), D3, "the terminal pane's directory");

    let t2 = &tabs[2];
    assert_eq!(t2.focused_kind(), REF_NONE, "tab 2 has no focus saved");
    let lone = t2.root();
    assert!(
        lone.is_pane() && lone.kind() == EDIT,
        "tab 2's root is a lone edit pane"
    );
    assert_eq!(lone.path(), D3_RECENT_FILE, "tab 2's edit pane path");
    assert!(
        !lone.has_cursor() && !lone.has_top_line(),
        "tab 2's edit pane never set a cursor or top line"
    );

    // --- Window 1 ---
    let w1 = &windows[1];
    assert!(
        !w1.has_position(),
        "window 1 was added without a position, so it must read back without one; true means \
         add_window stores x and y regardless"
    );
    assert_eq!(
        (w1.x(), w1.y(), w1.width(), w1.height()),
        (0.0, 0.0, W1.2, W1.3),
        "window 1 reads x and y as 0 without a position, and keeps its size"
    );
    let w1_tabs = w1.tabs();
    assert_eq!(w1_tabs.len(), 1, "window 1 holds one tab");
    let empty = &w1_tabs[0];
    assert_eq!(empty.directory(), "", "window 1's tab is an empty tab");
    assert!(!empty.has_root(), "an empty tab has no root");
    let placeholder = empty.root();
    assert!(
        placeholder.is_pane() && placeholder.kind() == EDIT && placeholder.path().is_empty(),
        "a tab with no root reads its root as an empty edit pane"
    );

    // --- Project states ---
    let project = wolf_ffi_app_state_project_state(D3.to_owned());
    assert!(
        project.found(),
        "D3 has open tabs, so it must have a project state"
    );
    assert!(
        project.has_position(),
        "D3's project state carries window 0's position"
    );
    assert_eq!(
        (project.x(), project.y(), project.width(), project.height()),
        W0,
        "D3's project state carries the frame of window 0, the window its tab came from"
    );
    let chosen = project.tab();
    assert_eq!(
        chosen.directory(),
        D3,
        "a project state's tab reads the directory asked for"
    );
    let chosen_root = chosen.root();
    assert_eq!(
        (chosen_root.kind(), chosen_root.path()),
        (EDIT, D3_RECENT_FILE.to_owned()),
        "no active tab is in D3, so window 0's most recently selected D3 tab, tab 2, must be \
         stored. The terminal of tab 1 means commit submitted no recency and the rule fell back \
         to tab order"
    );

    assert!(
        !wolf_ffi_app_state_project_state("/nowhere".to_owned()).found(),
        "a directory never opened has no project state"
    );
}
