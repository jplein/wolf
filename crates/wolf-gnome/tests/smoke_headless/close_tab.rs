//! The Close Tab smoke: a tab's close, from its close button or the palette's Close Tab, asks
//! the tab's own Neovim first, refuses while the tab has unsaved changes, and closes it once
//! they are written; Close Tab on the last tab closes the window, and with it the app.
//!
//! The launch restores one planted window of two tabs, each on a file of the project. Every
//! observable is a count of the app's live Neovims, a readback a Neovim writes about itself, or
//! the app's exit. Nothing is OCRed, so no fixture name can satisfy an assertion.
//!
//! The pointer helper's files and every readback live under the runtime dir, never in the
//! project the tabs are rooted at.
//!
//! The smoke pins `gtk-titlebar-style: native`, because its clicks are on the tab bar's own row.

use super::extras::{plant_window_state, planted_window, tab_bar_y, two_tab_x, EXTRAS_HELPERS};
use super::open_folder::canonical;
use super::recovery::with_nvim_pid_helper;
use super::*;
use wolf_core::app_state::{Tab, TabContent};

/// Tab 1's file, selected at launch. Its name is what tab 1's readbacks must report.
const CT_FILE_A: &str = "ctosprey.md";

/// Tab 2's file, the tab every refusal is made in. Its name is what tab 2's readbacks report.
const CT_FILE_B: &str = "ctwren.md";

/// The palette query that selects Close Tab; [`assert_close_tab_query_selects_close_tab`]
/// checks that it ranks Close Tab first.
const CT_QUERY: &str = "close tab";

/// How far left of the output's right edge the second tab's close button has its centre.
///
/// The second of two tabs ends at the bar's right edge, inside the bar's 1 px side padding.
/// The close button is the tab's last child, 24 px wide inside the tab's 4 px padding, so its
/// centre is 1 + 4 + 12 px in. Read from libadwaita 1.9.3's `tabbar .box`, `tabbar tab` and
/// `tabbar tab button.image-button` rules, in the flake's `libadwaita-1.so` stylesheet
/// resource `/org/gnome/Adwaita/styles/gtk.css`.
const CT_CLOSE_BUTTON_INSET_PX: f64 = 17.0;

/// The exit codes [`CLOSE_TAB_BODY`] adds to [`POINTER_PREAMBLE`]'s.
const CT_EXITS: std::ops::RangeInclusive<i32> = 180..=189;

/// What each of the exit codes in [`CT_EXITS`] means, for the failure message.
const CT_CODES: &str = "Exit 180: the launch did not restore exactly two Neovims, so the planted \
     window was not restored as two tabs.\n\
     Exit 181: setup: after the click on tab 2's entry and `o`, tab 2's readback is missing or \
     is not `1` and tab 2's file, so the click did not select tab 2 or the edit did not modify it.\n\
     Exit 182: setup: after the click on tab 1's entry, the readback is missing or does not name \
     tab 1's file, so keys do not reach tab 1.\n\
     Exit 183: leg A: the click on modified tab 2's close button did not leave two Neovims, so \
     the close-page veto closed the page without asking its Neovim.\n\
     Exit 184: leg A: the readback typed after the click is missing or does not name tab 2's \
     file. Either the ask did not select the background page first, so its refusal shows in a \
     tab nobody sees, or the click hit neither tab 2's entry nor its close button.\n\
     Exit 185: leg A: tab 2's `:messages` holds no E37, so the page was neither closed nor \
     refused by its Neovim: the click landed on the tab's entry rather than its close button. \
     Fix CT_CLOSE_BUTTON_INSET_PX, not the code.\n\
     Exit 186: leg B: Close Tab from the palette did not leave two Neovims, or the readback \
     after it does not name tab 2's file, so the palette's Close Tab closed the modified tab \
     without asking.\n\
     Exit 187: leg B: tab 2's `:messages`, cleared before the palette, holds no fresh E37, so \
     the palette's Close Tab never asked tab 2's Neovim.\n\
     Exit 188: leg C: after `:w` and Close Tab from the palette, the app did not keep one \
     Neovim, died, or keys no longer reach tab 1's file: the drained close_tab asked \
     again instead of closing, or closed the wrong tab.\n\
     Exit 189: leg D: Close Tab on the last tab did not end the app within the wait, or the \
     app exited non-zero; the cage log says which.";

/// Body of [`close_tab_asks_the_tabs_neovim_under_cage`], appended to [`POINTER_PREAMBLE`],
/// [`EXTRAS_HELPERS`] and `recovery::with_nvim_pid_helper`'s helper.
///
/// 1. Two restored Neovims, then the restore's and the bar's settles.
/// 2. A click on tab 2's entry, `o` and Escape, and readback S2 from tab 2; a click on tab 1's
///    entry and readback S1.
/// 3. Leg A: a hover over tab 2's close button, then a click on it; two Neovims, readback A and
///    tab 2's messages.
/// 4. Leg B: `:messages clear`, then Close Tab from the palette; two Neovims, readback B and
///    tab 2's messages.
/// 5. Leg C: `:w`, then Close Tab from the palette; one Neovim, a live app and readback C from
///    tab 1.
/// 6. Leg D: Close Tab from the palette in tab 1, and the app's exit.
///
/// Every messages readback starts with the line `ctmsgs`, so an empty history still writes a
/// file and is told apart from a command that never ran.
const CLOSE_TAB_BODY: &str = r#"
# $1 is the readback, $2 its expected content and $3 the exit code when it differs or is missing.
expect_read() {
    wait_file "$1" || exit "$3"
    _got=$(cat "$1")
    if [ "$_got" != "$2" ]; then
        echo "wolf-gnome close-tab smoke: $1 reads '$_got', not '$2'." >&2
        exit "$3"
    fi
}

# $1 is the messages readback and $2 the exit code when it holds no E37.
expect_e37() {
    nvim_ex "call writefile(['ctmsgs'] + split(execute('messages'), '\n'), '$1')"
    wait_file "$1" || exit "$2"
    if ! grep -q 'E37' "$1"; then
        echo "wolf-gnome close-tab smoke: $1 holds no E37:" >&2
        cat "$1" >&2
        exit "$2"
    fi
}

close_tab_from_palette() {
    wtype -M ctrl -M shift -k p -m shift -m ctrl
    sleep "$SETTLE"
    wtype -- "$CT_QUERY"
    sleep "$SETTLE"
    wtype -k Return
    sleep "$TABS_BAR_SETTLE"
}

wait_nvim_count 2 180 "at launch, with two restored tabs"
sleep "$RESTORE_SETTLE"
sleep "$TABS_BAR_SETTLE"

click_tab "$TAB1_X" "$TAB_Y"
wtype -- o
wtype -k Escape
sleep "$SETTLE"
nvim_ex "call writefile([&modified . ' ' . expand('%:t')], '$CT_READ_S2')"
expect_read "$CT_READ_S2" "1 $CT_FILE_B" 181

click_tab "$TAB0_X" "$TAB_Y"
nvim_ex "call writefile([expand('%:t')], '$CT_READ_S1')"
expect_read "$CT_READ_S1" "$CT_FILE_A" 182

# --- Leg A: tab 2's close button, while tab 1 is selected ---------------------
# A background tab shows its close button only while hovered.
hsend motion "$CT_CLOSE_X" "$TAB_Y"
sleep "$SETTLE"
hsend press "$BTN_LEFT"
hsend release "$BTN_LEFT"
hsend motion "$PROBE_X" "$PROBE_Y"
sleep "$TABS_BAR_SETTLE"
wait_nvim_count 2 183 "after the click on modified tab 2's close button"
nvim_ex "call writefile([expand('%:t')], '$CT_READ_A')"
expect_read "$CT_READ_A" "$CT_FILE_B" 184
expect_e37 "$CT_MSG_A" 185

# --- Leg B: the palette's Close Tab, in modified tab 2 ------------------------
nvim_ex "messages clear"
close_tab_from_palette
wait_nvim_count 2 186 "after Close Tab from the palette in modified tab 2"
nvim_ex "call writefile([expand('%:t')], '$CT_READ_B')"
expect_read "$CT_READ_B" "$CT_FILE_B" 186
expect_e37 "$CT_MSG_B" 187

# --- Leg C: the palette's Close Tab, once tab 2 is written --------------------
nvim_ex "w"
close_tab_from_palette
wait_nvim_count 1 188 "after :w and Close Tab from the palette in tab 2"
if ! pid_is_live "$app_pid"; then
    echo "wolf-gnome close-tab smoke: the app exited when one of two tabs was closed." >&2
    exit 188
fi
nvim_ex "call writefile([expand('%:t')], '$CT_READ_C')"
expect_read "$CT_READ_C" "$CT_FILE_A" 188

# --- Leg D: the palette's Close Tab, on the last tab ---------------------------
close_tab_from_palette
reap_app 189 189

hsend quit
exit 0
"#;

/// The second tab's close button's centre x; see [`CT_CLOSE_BUTTON_INSET_PX`].
fn close_button_x() -> i64 {
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "the expected cage output width");
    pixel_target(
        output_w - CT_CLOSE_BUTTON_INSET_PX,
        "the second tab's close button's centre x",
    )
}

/// Premise: [`CT_QUERY`] ranks Close Tab first in the palette, whose other rows here are the
/// registry's commands and an Open Recent row for `project`.
fn assert_close_tab_query_selects_close_tab(smoke: &str, project: &str) {
    let rows = wolf_core::rank_palette(&[PathBuf::from(project)], &[], &[], CT_QUERY, 3);
    let first = rows.first().map(|row| (row.kind, row.command));
    assert_eq!(
        first,
        Some((
            wolf_core::PaletteRowKind::Static,
            Some(wolf_core::Command::CloseTab)
        )),
        "{smoke}: premise: the palette query {CT_QUERY:?} must rank Close Tab first, or Return \
         runs another row; it ranks {:?}",
        rows.iter()
            .map(|row| row.display.as_str())
            .collect::<Vec<_>>()
    );
}

/// **A tab's close asks the tab's own Neovim, and Close Tab on the last tab closes the
/// window.**
///
/// The planted window holds tab 1 on [`CT_FILE_A`] and tab 2 on [`CT_FILE_B`], both rooted at
/// the temp cwd `project`, tab 1 selected. The setup modifies tab 2 and reads back that it is
/// modified, then selects tab 1 again, so leg A's click is on a background tab.
///
/// Each leg's own exit code is in [`CT_CODES`]. A count is read only after the bar's settle, so
/// a close still shutting its Neovim down cannot pass for a refusal.
///
/// Premises, measured by the green run: a background tab shows its close button while hovered,
/// at [`CT_CLOSE_BUTTON_INSET_PX`] from the output's right edge; selecting the page from inside
/// `close-page` makes it the tab keys reach, which leg A's readback reads; and a refusal echoed
/// inside the ask's RPC request lands in that Neovim's `:messages`, which legs A and B read.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: two Neovims at launch, tab 2 modified, keys reach tab 1 | none | 180, 181 or 182 |
/// | G3 | leg A: two Neovims, the readback names [`CT_FILE_B`], tab 2's messages hold E37 | G3a: drop the selection in `on_close_page`'s ask | 184, the readback names [`CT_FILE_A`] |
/// | G3 | as above | G3b: in `on_close_page`, close a page that is not selected without calling `close_page_now` | 183, one Neovim |
/// | G1 | leg B: two Neovims, the readback names [`CT_FILE_B`], a fresh E37 | G1a: `AppShell::close_tab` confirms the selected tab before `TabsHandle::close` | 186, one Neovim |
/// | G2 | leg C: one Neovim, the app alive, the readback names [`CT_FILE_A`] | G2a: `drain_and_dispatch_commands` no longer confirms a drained `CloseTab` | 188, the one-Neovim wait |
/// | G4 | leg D: the app exits 0 within the wait | G4a: `tabs_ui::close_outcome` answers `CloseWindow` only for `live_pages == 0` | 189, the app stays alive |
#[test]
fn close_tab_asks_the_tabs_neovim_under_cage() {
    const SMOKE: &str = "close_tab_asks_the_tabs_neovim_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let project = canonical(SMOKE, &cwd);
    assert_close_tab_query_selects_close_tab(SMOKE, &project);
    for (name, heading) in [(CT_FILE_A, "# CT OSPREY"), (CT_FILE_B, "# CT WREN")] {
        let path = cwd.join(name);
        std::fs::write(&path, format!("{heading}\n"))
            .unwrap_or_else(|e| panic!("{SMOKE}: failed to write {}: {e}", path.display()));
    }
    let file_tab = |name: &str| Tab {
        directory: project.clone(),
        content: TabContent {
            root: Some(Node::Pane(Pane::Edit {
                path: Path::new(&project)
                    .join(name)
                    .to_string_lossy()
                    .into_owned(),
                cursor: None,
                top_line: None,
            })),
            ..TabContent::default()
        },
    };
    let planted = WindowState {
        windows: vec![planted_window(
            0,
            vec![file_tab(CT_FILE_A), file_tab(CT_FILE_B)],
        )],
    };
    plant_window_state(SMOKE, &cwd, &planted);

    let rt: &Path = &runtime_dir;
    let read_s1 = rt.join("ct_read_s1.txt");
    let read_s2 = rt.join("ct_read_s2.txt");
    let read_a = rt.join("ct_read_a.txt");
    let read_b = rt.join("ct_read_b.txt");
    let read_c = rt.join("ct_read_c.txt");
    let msg_a = rt.join("ct_msg_a.txt");
    let msg_b = rt.join("ct_msg_b.txt");
    let out_png = rt.join("wolf_gnome_close_tab_unused.png");
    for path in [
        &read_s1, &read_s2, &read_a, &read_b, &read_c, &msg_a, &msg_b,
    ] {
        clear_evidence(path, SMOKE);
    }
    let (tab0_x, tab1_x, tab_y) = (two_tab_x(0), two_tab_x(1), tab_bar_y());
    let close_x = close_button_x();

    let body = with_nvim_pid_helper(&format!("{EXTRAS_HELPERS}\n{CLOSE_TAB_BODY}"));
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
        .env("CT_CLOSE_X", close_x.to_string())
        .env("CT_FILE_A", CT_FILE_A)
        .env("CT_FILE_B", CT_FILE_B)
        .env("CT_QUERY", CT_QUERY)
        .env("CT_READ_S1", &read_s1)
        .env("CT_READ_S2", &read_s2)
        .env("CT_READ_A", &read_a)
        .env("CT_READ_B", &read_b)
        .env("CT_READ_C", &read_c)
        .env("CT_MSG_A", &msg_a)
        .env("CT_MSG_B", &msg_b)
        .status()
        .expect("failed to spawn `cage` for the headless Close Tab scenario");

    assert!(
        !status.code().is_some_and(|code| CT_EXITS.contains(&code)),
        "{SMOKE}: the cage Close Tab scenario exited with one of its own codes ({status}). The \
         cage log in the runtime dir carries the scenario's own lines.\n{CT_CODES}"
    );
    assert_pointer_scenario_succeeded(SMOKE, status, rt, &out_png);
    let extents = read_output_extents(rt, SMOKE);
    for (label, point) in [
        ("tab 1's bar entry", (tab0_x, tab_y)),
        ("tab 2's bar entry", (tab1_x, tab_y)),
        ("tab 2's close button", (close_x, tab_y)),
    ] {
        assert_inside_extents(SMOKE, label, point, extents);
    }
}
