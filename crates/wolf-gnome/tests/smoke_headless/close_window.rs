//! The Close Window and Quit smokes: Close Window asks every tab of its window, and Quit every
//! tab of every window, whether its Neovim holds unsaved changes. While one does, nothing
//! closes and that tab is selected and shows E37; once nothing does, everything closes.
//!
//! Each launch restores one planted window of two tabs, each on a file of the project. Every
//! observable is a count of the app's live Neovims, a readback a Neovim writes about itself, or
//! the app's exit. Nothing is OCRed, so no fixture name can satisfy an assertion.
//!
//! The pointer helper's files and every readback live under the runtime dir, never in the
//! project the tabs are rooted at. Both smokes pin `gtk-titlebar-style: native`, because their
//! clicks are on the tab bar's own row.

use super::close_tab::READBACK_HELPERS;
use super::extras::{plant_window_state, planted_window, tab_bar_y, two_tab_x, EXTRAS_HELPERS};
use super::open_folder::canonical;
use super::recovery::with_nvim_pid_helper;
use super::restore::TWOWIN_NEW_WINDOW_QUERY;
use super::*;
use wolf_core::app_state::{Tab, TabContent};

/// The palette query that selects Close Window; [`assert_query_ranks_first`] checks that it
/// ranks Close Window first.
const CW_QUERY: &str = "close window";

/// Shell helpers both bodies share. They call `app_children` and `pid_is_live`, so they follow
/// `recovery::with_nvim_pid_helper`'s helper.
const CLOSE_WINDOW_HELPERS: &str = r#"
# Runs the palette row that `$1` ranks first.
palette_run() {
    wtype -M ctrl -M shift -k p -m shift -m ctrl
    sleep "$SETTLE"
    wtype -- "$1"
    sleep "$SETTLE"
    wtype -k Return
    sleep "$TABS_BAR_SETTLE"
}

# Prints the pids of the app's live Neovim children, space-separated.
live_nvims() {
    _live=""
    for _p in $(app_children); do
        pid_is_live "$_p" || continue
        if [ "$(sed -n 's/^Name:[[:space:]]*//p' "/proc/$_p/status" 2>/dev/null)" = nvim ]; then
            _live="$_live $_p"
        fi
    done
    echo "$_live"
}

# $1 is the exit code when the app has died, $2 the moment, for the message.
expect_app_alive() {
    if ! pid_is_live "$app_pid"; then
        echo "wolf-gnome close-window smoke: the app exited $2." >&2
        exit "$1"
    fi
}
"#;

// --- CW19: Close Window ---------------------------------------------------------------------

/// Tab 1's file, selected at launch. Its name is what tab 1's readbacks must report.
const CW_FILE_A: &str = "cwplover.md";

/// Tab 2's file, the tab that is modified in the background. Its name is what tab 2's
/// readbacks report.
const CW_FILE_B: &str = "cwstilt.md";

/// The exit codes [`CLOSE_WINDOW_BODY`] adds to [`POINTER_PREAMBLE`]'s.
const CW_EXITS: std::ops::RangeInclusive<i32> = 200..=209;

/// What each of the exit codes in [`CW_EXITS`] means, for the failure message.
const CW_CODES: &str = "Exit 200: the launch did not restore exactly two Neovims, so the planted \
     window was not restored as two tabs.\n\
     Exit 201: setup: after the click on tab 2's entry and `o`, tab 2's readback is missing or \
     is not `1` and tab 2's file, so the click did not select tab 2 or the edit did not modify it.\n\
     Exit 202: setup: after the click on tab 1's entry, the readback is missing or does not name \
     tab 1's file, so keys do not reach tab 1.\n\
     Exit 203: leg A: Close Window from the palette, with tab 2 modified in the background, did \
     not leave two Neovims and a live app, so the window closed without asking its tabs, or \
     closed a tab before the check refused.\n\
     Exit 204: leg A: the readback typed after Close Window is missing or does not name tab 2's \
     file. Tab 1's file means the refusal did not select the unsaved tab, so its message shows \
     in a tab nobody sees; a missing readback means keys reach no tab after the refusal.\n\
     Exit 205: leg A: tab 2's `:messages` holds no E37, so the check selected tab 2 but never \
     asked its Neovim to show the refusal.\n\
     Exit 206: leg B: after `:w` and `:qa` in tab 2, the app did not keep exactly one Neovim, \
     or died: `:qa` closed no tab, or closed the whole window rather than its own tab.\n\
     Exit 207: leg B: the readback after `:qa` is missing or does not name tab 1's file, so the \
     wrong tab closed or keys do not reach the tab left selected.\n\
     Exit 208: leg C: Ctrl+Shift+T did not give two Neovims, so no new tab opened.\n\
     Exit 209: leg C: Close Window from the palette on a window of two clean tabs did not end \
     the app within the wait, the app exited non-zero, or one of its tabs' Neovims was still \
     live after it exited; the cage log says which.";

/// Body of [`close_window_asks_every_tab_under_cage`], appended to [`POINTER_PREAMBLE`],
/// [`EXTRAS_HELPERS`], [`READBACK_HELPERS`], `recovery::with_nvim_pid_helper`'s helper and
/// [`CLOSE_WINDOW_HELPERS`].
///
/// 1. Two restored Neovims, then the restore's and the bar's settles.
/// 2. A click on tab 2's entry, `o` and Escape, and readback S2 from tab 2; a click on tab 1's
///    entry and readback S1.
/// 3. Leg A: Close Window from the palette in tab 1; two Neovims, a live app, readback A and
///    tab 2's messages.
/// 4. Leg B: `:w`, then `:qa` in tab 2; one Neovim, a live app and readback B from tab 1.
/// 5. Leg C: Ctrl+Shift+T and two Neovims, then Close Window from the palette; the app's exit,
///    and neither Neovim live after it.
const CLOSE_WINDOW_BODY: &str = r#"
wait_nvim_count 2 200 "at launch, with two restored tabs"
sleep "$RESTORE_SETTLE"
sleep "$TABS_BAR_SETTLE"

click_tab "$TAB1_X" "$TAB_Y"
wtype -- o
wtype -k Escape
sleep "$SETTLE"
nvim_ex "call writefile([&modified . ' ' . expand('%:t')], '$CW_READ_S2')"
expect_read "$CW_READ_S2" "1 $CW_FILE_B" 201

click_tab "$TAB0_X" "$TAB_Y"
nvim_ex "call writefile([expand('%:t')], '$CW_READ_S1')"
expect_read "$CW_READ_S1" "$CW_FILE_A" 202

# --- Leg A: Close Window in tab 1, while tab 2 is modified in the background ---
palette_run "$CW_QUERY"
wait_nvim_count 2 203 "after Close Window with tab 2 modified"
expect_app_alive 203 "after Close Window with tab 2 modified"
nvim_ex "call writefile([expand('%:t')], '$CW_READ_A')"
expect_read "$CW_READ_A" "$CW_FILE_B" 204
expect_e37 "$CW_MSG_A" 205

# --- Leg B: :qa in tab 2, once it is written ----------------------------------
nvim_ex "w"
nvim_ex "qa"
sleep "$TABS_BAR_SETTLE"
wait_nvim_count 1 206 "after :w and :qa in tab 2"
expect_app_alive 206 "after :qa in one of two tabs"
nvim_ex "call writefile([expand('%:t')], '$CW_READ_B')"
expect_read "$CW_READ_B" "$CW_FILE_A" 207

# --- Leg C: Close Window on two clean tabs ------------------------------------
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
wait_nvim_count 2 208 "after Ctrl+Shift+T"
sleep "$SETTLE"
closed_nvims=$(live_nvims)
palette_run "$CW_QUERY"
reap_app 209 209
for _p in $closed_nvims; do
    if pid_is_live "$_p"; then
        echo "wolf-gnome close-window smoke: Neovim $_p outlived the app's Close Window." >&2
        exit 209
    fi
done

hsend quit
exit 0
"#;

/// Premise: `query` ranks `command` first in the palette, whose other rows here are the
/// registry's commands and an Open Recent row for `project`.
fn assert_query_ranks_first(smoke: &str, project: &str, query: &str, command: wolf_core::Command) {
    let rows = wolf_core::rank_palette(&[PathBuf::from(project)], &[], &[], query, 3);
    let first = rows.first().map(|row| (row.kind, row.command));
    assert_eq!(
        first,
        Some((wolf_core::PaletteRowKind::Static, Some(command))),
        "{smoke}: premise: the palette query {query:?} must rank {command:?} first, or Return \
         runs another row; it ranks {:?}",
        rows.iter()
            .map(|row| row.display.as_str())
            .collect::<Vec<_>>()
    );
}

/// Writes each `(name, heading)` fixture into `cwd` and plants one window of a tab on each,
/// rooted at `project`, with the first tab selected.
fn plant_two_file_tabs(smoke: &str, cwd: &Path, project: &str, files: [(&str, &str); 2]) {
    for (name, heading) in files {
        let path = cwd.join(name);
        std::fs::write(&path, format!("{heading}\n"))
            .unwrap_or_else(|e| panic!("{smoke}: failed to write {}: {e}", path.display()));
    }
    let file_tab = |name: &str| Tab {
        directory: project.to_owned(),
        content: TabContent {
            root: Some(Node::Pane(Pane::Edit {
                path: Path::new(project).join(name).to_string_lossy().into_owned(),
                cursor: None,
                top_line: None,
            })),
            ..TabContent::default()
        },
    };
    let planted = WindowState {
        windows: vec![planted_window(
            0,
            files.into_iter().map(|(name, _)| file_tab(name)).collect(),
        )],
    };
    plant_window_state(smoke, cwd, &planted);
}

/// **Close Window asks every tab of its window, and `:qa` closes only its own tab.**
///
/// The planted window holds tab 1 on [`CW_FILE_A`] and tab 2 on [`CW_FILE_B`], both rooted at
/// the temp cwd `project`, tab 1 selected. The setup modifies tab 2 and reads back that it is
/// modified, then selects tab 1 again, so leg A's Close Window is made in the clean tab.
///
/// Each leg's own exit code is in [`CW_CODES`]. A count is read only after the bar's settle, so
/// a close still shutting its Neovims down cannot pass for a refusal.
///
/// Premises: [`CW_QUERY`] ranks Close Window first, which the test asserts before the launch.
/// Measured by the green run: selecting the page from inside `close-request` makes it the tab
/// keys reach, which leg A's readback reads, and a refusal echoed inside the RPC request lands
/// in that Neovim's `:messages`.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: two Neovims at launch, tab 2 modified, keys reach tab 1 | none | 200, 201 or 202 |
/// | CW19 | leg A: two Neovims and a live app, the readback names [`CW_FILE_B`], tab 2's messages hold E37 | CW19a: `wire_close`'s handler skips the check | 203, the app exits |
/// | CW19 | as above | CW19b: `TabsHandle::show_unsaved` echoes without selecting | 204, the readback names [`CW_FILE_A`] |
/// | CW19 | leg B: one Neovim and a live app, the readback names [`CW_FILE_A`] | CW19c: `AppShell::close_tab` calls `self.close_window()` | 206, the app exits |
/// | CW19 | leg C: the app exits 0 within the wait, and neither Neovim outlives it | CW19d: the handler returns `glib::Propagation::Stop` after `CloseAll` too | 209, the app stays alive |
#[test]
fn close_window_asks_every_tab_under_cage() {
    const SMOKE: &str = "close_window_asks_every_tab_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let project = canonical(SMOKE, &cwd);
    assert_query_ranks_first(SMOKE, &project, CW_QUERY, wolf_core::Command::CloseWindow);
    plant_two_file_tabs(
        SMOKE,
        &cwd,
        &project,
        [(CW_FILE_A, "# CW PLOVER"), (CW_FILE_B, "# CW STILT")],
    );

    let rt: &Path = &runtime_dir;
    let read_s1 = rt.join("cw_read_s1.txt");
    let read_s2 = rt.join("cw_read_s2.txt");
    let read_a = rt.join("cw_read_a.txt");
    let read_b = rt.join("cw_read_b.txt");
    let msg_a = rt.join("cw_msg_a.txt");
    let out_png = rt.join("wolf_gnome_close_window_unused.png");
    for path in [&read_s1, &read_s2, &read_a, &read_b, &msg_a] {
        clear_evidence(path, SMOKE);
    }
    let (tab0_x, tab1_x, tab_y) = (two_tab_x(0), two_tab_x(1), tab_bar_y());

    let body = with_nvim_pid_helper(&format!(
        "{EXTRAS_HELPERS}\n{READBACK_HELPERS}\n{CLOSE_WINDOW_HELPERS}\n{CLOSE_WINDOW_BODY}"
    ));
    let status = pointer_command(&body, &cwd, &runtime_dir, &out_png)
        .env(
            "WOLF_CONFIG_DIR",
            titlebar_pinned_config_dir(&cwd, GTK_TITLEBAR_NATIVE),
        )
        .env("CMD_FIFO", rt.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", rt.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(rt))
        .env("READY_FILE", pointer_ready_path(rt))
        .env("TAB0_X", tab0_x.to_string())
        .env("TAB1_X", tab1_x.to_string())
        .env("TAB_Y", tab_y.to_string())
        .env("CW_FILE_A", CW_FILE_A)
        .env("CW_FILE_B", CW_FILE_B)
        .env("CW_QUERY", CW_QUERY)
        .env("CW_READ_S1", &read_s1)
        .env("CW_READ_S2", &read_s2)
        .env("CW_READ_A", &read_a)
        .env("CW_READ_B", &read_b)
        .env("CW_MSG_A", &msg_a)
        .status()
        .expect("failed to spawn `cage` for the headless Close Window scenario");

    assert!(
        !status.code().is_some_and(|code| CW_EXITS.contains(&code)),
        "{SMOKE}: the cage Close Window scenario exited with one of its own codes ({status}). The \
         cage log in the runtime dir carries the scenario's own lines.\n{CW_CODES}"
    );
    assert_pointer_scenario_succeeded(SMOKE, status, rt, &out_png);
    let extents = read_output_extents(rt, SMOKE);
    for (label, point) in [
        ("tab 1's bar entry", (tab0_x, tab_y)),
        ("tab 2's bar entry", (tab1_x, tab_y)),
    ] {
        assert_inside_extents(SMOKE, label, point, extents);
    }
}

// --- CW20: Quit -----------------------------------------------------------------------------

/// Window 1's first tab's file, selected at launch. Its name is what that tab's readbacks
/// report.
const Q_FILE_A: &str = "qtern.md";

/// Window 1's second tab's file, modified in the background. Its name is what that tab's
/// readbacks report.
const Q_FILE_B: &str = "qgannet.md";

/// The name of the buffer of the tab New Window opens, which `runtime/nvim/init.lua`'s
/// `wolf_next_untitled` gives at index 1 in the new window's fresh Neovim. The production
/// const's doc keeps it equal to that name.
const Q_W2_BUFFER: &str = wolf_gnome::save_ui::UNTITLED_SAVE_NAME;

/// The exit codes [`QUIT_BODY`] adds to [`POINTER_PREAMBLE`]'s.
const Q_EXITS: std::ops::RangeInclusive<i32> = 210..=219;

/// What each of the exit codes in [`Q_EXITS`] means, for the failure message.
const Q_CODES: &str = "Exit 210: the launch did not restore exactly two Neovims, so the planted \
     window was not restored as two tabs.\n\
     Exit 211: setup: after the click on tab 2's entry and `o`, tab 2's readback is missing or \
     is not `1` and tab 2's file, so the click did not select tab 2 or the edit did not modify it.\n\
     Exit 212: setup: after the click on tab 1's entry, the readback is missing or does not name \
     tab 1's file, so keys do not reach tab 1.\n\
     Exit 213: setup: New Window from the palette did not give a third Neovim, or the readback \
     typed after it is missing or names another buffer than `Untitled.md`, so keys do not reach \
     the new window.\n\
     Exit 214: leg A: Ctrl+Shift+Q in window 2, with window 1's tab 2 modified, did not leave \
     three Neovims and a live app, so the quit did not ask the other window's tabs.\n\
     Exit 215: leg B: `:WolfCloseWindow` in window 2, whose only tab is clean, did not leave two \
     Neovims and a live app: window 2 did not close, so Close Window asked other windows' tabs \
     too, or the app quit with it.\n\
     Exit 216: leg B: premise: the readback typed once window 2 closed is missing, so keys do \
     not pass to window 1 when cage's newest toplevel closes. Fix the smoke, not the code.\n\
     Exit 217: leg B: the readback names another file than window 1's tab 2, so the refused quit \
     did not select the unsaved tab in its window.\n\
     Exit 218: leg B: window 1's tab 2's `:messages` holds no E37, so the refused quit selected \
     the tab but never asked its Neovim to show the refusal.\n\
     Exit 219: leg C: after `:w` and Ctrl+Shift+Q, the app did not exit within the wait, or \
     exited non-zero; the cage log says which.";

/// Body of [`quit_asks_every_window_under_cage`], appended to [`POINTER_PREAMBLE`],
/// [`EXTRAS_HELPERS`], [`READBACK_HELPERS`], `recovery::with_nvim_pid_helper`'s helper and
/// [`CLOSE_WINDOW_HELPERS`].
///
/// 1. Two restored Neovims, then the restore's and the bar's settles.
/// 2. A click on tab 2's entry, `o` and Escape, and readback S2; a click on tab 1's entry and
///    readback S1.
/// 3. New Window from the palette; three Neovims and readback W2 from window 2.
/// 4. Leg A: Ctrl+Shift+Q in window 2; three Neovims and a live app after the settles.
/// 5. Leg B: `:WolfCloseWindow` in window 2; two Neovims, a live app, readback B from window 1
///    and its messages.
/// 6. Leg C: `:w`, then Ctrl+Shift+Q, and the app's exit.
const QUIT_BODY: &str = r#"
quit_chord() {
    wtype -M ctrl -M shift -k q -m shift -m ctrl
    sleep "$TABS_BAR_SETTLE"
}

wait_nvim_count 2 210 "at launch, with two restored tabs"
sleep "$RESTORE_SETTLE"
sleep "$TABS_BAR_SETTLE"

click_tab "$TAB1_X" "$TAB_Y"
wtype -- o
wtype -k Escape
sleep "$SETTLE"
nvim_ex "call writefile([&modified . ' ' . expand('%:t')], '$Q_READ_S2')"
expect_read "$Q_READ_S2" "1 $Q_FILE_B" 211

click_tab "$TAB0_X" "$TAB_Y"
nvim_ex "call writefile([expand('%:t')], '$Q_READ_S1')"
expect_read "$Q_READ_S1" "$Q_FILE_A" 212

# --- Setup: a second window, which cage shows and keys reach ------------------
palette_run "$Q_NEW_WINDOW_QUERY"
wait_nvim_count 3 213 "after New Window from the palette"
sleep "$SETTLE"
sleep "$RESTORE_SETTLE"
nvim_ex "call writefile(['[' . expand('%:t') . ']'], '$Q_READ_W2')"
expect_read "$Q_READ_W2" "[$Q_W2_BUFFER]" 213

# --- Leg A: Ctrl+Shift+Q in window 2, while window 1's tab 2 is modified -------
quit_chord
sleep "$RESTORE_SETTLE"
wait_nvim_count 3 214 "after Ctrl+Shift+Q with window 1's tab 2 modified"
expect_app_alive 214 "after Ctrl+Shift+Q with window 1's tab 2 modified"

# --- Leg B: Close Window in clean window 2, which hands the keys to window 1 ---
nvim_ex "WolfCloseWindow"
sleep "$TABS_BAR_SETTLE"
wait_nvim_count 2 215 "after :WolfCloseWindow in clean window 2"
expect_app_alive 215 "after :WolfCloseWindow in window 2"
nvim_ex "call writefile([expand('%:t')], '$Q_READ_B')"
wait_file "$Q_READ_B" || exit 216
expect_read "$Q_READ_B" "$Q_FILE_B" 217
expect_e37 "$Q_MSG_B" 218

# --- Leg C: Ctrl+Shift+Q once window 1's tab 2 is written ---------------------
nvim_ex "w"
quit_chord
reap_app 219 219

hsend quit
exit 0
"#;

/// **Quit asks every tab of every window, and shows the unsaved one in its own window.**
///
/// The planted window 1 holds tab 1 on [`Q_FILE_A`] and tab 2 on [`Q_FILE_B`], tab 1 selected.
/// The setup modifies tab 2, selects tab 1 again, and opens window 2 from the palette, whose
/// one tab is clean. Leg A quits from window 2. Leg B closes window 2, so the keys pass to
/// window 1, and its readback shows which tab the refused quit left selected there. That window
/// 2 closes at all also shows that Close Window asks only its own window's tabs.
///
/// Each leg's own exit code is in [`Q_CODES`].
///
/// Premises: [`TWOWIN_NEW_WINDOW_QUERY`] ranks New Window first, which the test asserts before
/// the launch. Measured by the green run, with its own exit code, 216: once cage's newest
/// toplevel closes, the one below it takes the keys. The quit's raise of window 1 is
/// unobservable here, because cage declines `present()`.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: two Neovims at launch, tab 2 modified, keys reach tab 1, then window 2 | none | 210 to 213 |
/// | CW20 | leg A: three Neovims and a live app; leg B: the readback names [`Q_FILE_B`] and its messages hold E37 | CW20a: `Coordinator::quit` skips the check | 214, the app exits |
/// | CW20 | as above | CW20b: `quit`'s refusal echoes without selecting the tab | 217, the readback names [`Q_FILE_A`] |
/// | CW20 | leg C: the app exits 0 within the wait | CW20d: `quit` returns after its check whatever it decides | 219, the app stays alive |
#[test]
fn quit_asks_every_window_under_cage() {
    const SMOKE: &str = "quit_asks_every_window_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let project = canonical(SMOKE, &cwd);
    assert_query_ranks_first(
        SMOKE,
        &project,
        TWOWIN_NEW_WINDOW_QUERY,
        wolf_core::Command::NewWindow,
    );
    plant_two_file_tabs(
        SMOKE,
        &cwd,
        &project,
        [(Q_FILE_A, "# Q TERN"), (Q_FILE_B, "# Q GANNET")],
    );

    let rt: &Path = &runtime_dir;
    let read_s1 = rt.join("q_read_s1.txt");
    let read_s2 = rt.join("q_read_s2.txt");
    let read_w2 = rt.join("q_read_w2.txt");
    let read_b = rt.join("q_read_b.txt");
    let msg_b = rt.join("q_msg_b.txt");
    let out_png = rt.join("wolf_gnome_quit_unused.png");
    for path in [&read_s1, &read_s2, &read_w2, &read_b, &msg_b] {
        clear_evidence(path, SMOKE);
    }
    let (tab0_x, tab1_x, tab_y) = (two_tab_x(0), two_tab_x(1), tab_bar_y());

    let body = with_nvim_pid_helper(&format!(
        "{EXTRAS_HELPERS}\n{READBACK_HELPERS}\n{CLOSE_WINDOW_HELPERS}\n{QUIT_BODY}"
    ));
    let status = pointer_command(&body, &cwd, &runtime_dir, &out_png)
        .env(
            "WOLF_CONFIG_DIR",
            titlebar_pinned_config_dir(&cwd, GTK_TITLEBAR_NATIVE),
        )
        .env("CMD_FIFO", rt.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", rt.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(rt))
        .env("READY_FILE", pointer_ready_path(rt))
        .env("TAB0_X", tab0_x.to_string())
        .env("TAB1_X", tab1_x.to_string())
        .env("TAB_Y", tab_y.to_string())
        .env("Q_FILE_A", Q_FILE_A)
        .env("Q_FILE_B", Q_FILE_B)
        .env("Q_NEW_WINDOW_QUERY", TWOWIN_NEW_WINDOW_QUERY)
        .env("Q_READ_S1", &read_s1)
        .env("Q_READ_S2", &read_s2)
        .env("Q_READ_W2", &read_w2)
        .env("Q_W2_BUFFER", Q_W2_BUFFER)
        .env("Q_READ_B", &read_b)
        .env("Q_MSG_B", &msg_b)
        .status()
        .expect("failed to spawn `cage` for the headless Quit scenario");

    assert!(
        !status.code().is_some_and(|code| Q_EXITS.contains(&code)),
        "{SMOKE}: the cage Quit scenario exited with one of its own codes ({status}). The cage \
         log in the runtime dir carries the scenario's own lines.\n{Q_CODES}"
    );
    assert_pointer_scenario_succeeded(SMOKE, status, rt, &out_png);
    let extents = read_output_extents(rt, SMOKE);
    for (label, point) in [
        ("tab 1's bar entry", (tab0_x, tab_y)),
        ("tab 2's bar entry", (tab1_x, tab_y)),
    ] {
        assert_inside_extents(SMOKE, label, point, extents);
    }
}
