//! The Close Pane smoke: Ctrl+Shift+W on a tab's last pane takes `:q`'s path in the tab's own
//! Neovim, so it refuses while the tab has unsaved changes, closes the tab once they are
//! written, and on the window's last tab closes the window, and with it the app.
//!
//! The launch restores one planted window of two tabs of one pane each, each on a file of the
//! project, with tab 2 selected. Every observable is a count of the app's live Neovims, a
//! readback a Neovim writes about itself, or the app's exit. Nothing is OCRed, so no fixture
//! name can satisfy an assertion.
//!
//! Every readback lives under the runtime dir, never in the project the tabs are rooted at.

use super::close_tab::READBACK_HELPERS;
use super::extras::{plant_window_state, planted_window, EXTRAS_HELPERS};
use super::open_folder::canonical;
use super::recovery::with_nvim_pid_helper;
use super::*;
use wolf_core::app_state::{Tab, TabContent};

/// Tab 1's file. Its name is what tab 1's readbacks must report.
const CP_FILE_A: &str = "cpheron.md";

/// Tab 2's file, selected at launch and the tab the refusal is made in. Its name is what tab 2's
/// readbacks report.
const CP_FILE_B: &str = "cplark.md";

/// The exit codes [`CLOSE_PANE_BODY`] adds to [`POINTER_PREAMBLE`]'s.
const CP_EXITS: std::ops::RangeInclusive<i32> = 190..=199;

/// What each of the exit codes in [`CP_EXITS`] means, for the failure message.
const CP_CODES: &str = "Exit 190: the launch did not restore exactly two Neovims, so the planted \
     window was not restored as two tabs.\n\
     Exit 191: setup: after `o` in the selected tab 2, the readback is missing or is not `1` \
     and tab 2's file, so keys do not reach the restored selected tab or the edit did not \
     modify it.\n\
     Exit 192: leg A: Ctrl+Shift+W on modified tab 2's only pane did not leave two Neovims, so \
     Close Pane closed the tab over its unsaved change.\n\
     Exit 193: leg A: the readback typed after Ctrl+Shift+W is missing or does not name tab 2's \
     file, so keys no longer reach tab 2 after the refusal.\n\
     Exit 194: leg A: tab 2's `:messages` holds no E37, so Close Pane was neither carried out \
     nor refused by `:q`'s check: the last pane was refused silently, as `wolf_do_close` \
     refuses it, or Ctrl+Shift+W never sent InputEvent::ClosePane.\n\
     Exit 195: leg B: after `:w` and Ctrl+Shift+W in tab 2, the app did not keep exactly one \
     Neovim, or died: Close Pane on the written last pane closed no tab, or closed the window.\n\
     Exit 196: leg B: the readback after tab 2's close is missing or does not name tab 1's \
     file, so keys do not reach the tab left selected.\n\
     Exit 197: leg C: Ctrl+Shift+W on the window's last tab left the app alive, so the tab's \
     close did not close its window.\n\
     Exit 198: leg C: the app exited non-zero after Ctrl+Shift+W on the last tab; the cage log \
     says why.";

/// Body of [`close_pane_closes_the_tab_on_its_last_pane_under_cage`], appended to
/// [`POINTER_PREAMBLE`], [`EXTRAS_HELPERS`], [`READBACK_HELPERS`] and
/// `recovery::with_nvim_pid_helper`'s helper.
///
/// 1. Two restored Neovims, then the restore's and the bar's settles.
/// 2. `o` and Escape in the selected tab 2, and readback S from it.
/// 3. Leg A: Ctrl+Shift+W; two Neovims, readback A and tab 2's messages.
/// 4. Leg B: `:w`, then Ctrl+Shift+W; one Neovim, a live app and readback B from tab 1.
/// 5. Leg C: Ctrl+Shift+W in tab 1, and the app's exit.
const CLOSE_PANE_BODY: &str = r#"
close_pane_chord() {
    wtype -M ctrl -M shift -k w -m shift -m ctrl
    sleep "$TABS_BAR_SETTLE"
}

wait_nvim_count 2 190 "at launch, with two restored tabs"
sleep "$RESTORE_SETTLE"
sleep "$TABS_BAR_SETTLE"

wtype -- o
wtype -k Escape
sleep "$SETTLE"
nvim_ex "call writefile([&modified . ' ' . expand('%:t')], '$CP_READ_S')"
expect_read "$CP_READ_S" "1 $CP_FILE_B" 191

# --- Leg A: Close Pane on modified tab 2's only pane --------------------------
close_pane_chord
wait_nvim_count 2 192 "after Ctrl+Shift+W on modified tab 2's only pane"
nvim_ex "call writefile([expand('%:t')], '$CP_READ_A')"
expect_read "$CP_READ_A" "$CP_FILE_B" 193
expect_e37 "$CP_MSG_A" 194

# --- Leg B: Close Pane once tab 2 is written ----------------------------------
nvim_ex "w"
close_pane_chord
wait_nvim_count 1 195 "after :w and Ctrl+Shift+W on tab 2's only pane"
if ! pid_is_live "$app_pid"; then
    echo "wolf-gnome close-pane smoke: the app exited when one of two tabs was closed." >&2
    exit 195
fi
nvim_ex "call writefile([expand('%:t')], '$CP_READ_B')"
expect_read "$CP_READ_B" "$CP_FILE_A" 196

# --- Leg C: Close Pane on the window's last tab --------------------------------
close_pane_chord
reap_app 197 198

hsend quit
exit 0
"#;

/// **Ctrl+Shift+W on a tab's last pane closes the tab as `:q` does, and on the last tab
/// closes the window.**
///
/// The planted window holds tab 1 on [`CP_FILE_A`] and tab 2 on [`CP_FILE_B`], each one edit
/// pane rooted at the temp cwd `project`, tab 2 selected. The setup modifies tab 2 and reads
/// back that it is modified.
///
/// Each leg's own exit code is in [`CP_CODES`]. A count is read only after the bar's settle, so
/// a close still shutting its Neovim down cannot pass for a refusal.
///
/// Premises, measured by the green run: keys reach the selected tab's pane after a restore with
/// no click; a refusal echoed inside `InputEvent::ClosePane`'s `nvim_exec_lua` request lands in
/// that Neovim's `:messages`, which leg A reads; and the tab left after tab 2's close is
/// selected and takes keys, which leg B's readback reads.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: two Neovims at launch, tab 2 modified | none | 190 or 191 |
/// | S1 | leg A: two Neovims, the readback names [`CP_FILE_B`], tab 2's messages hold E37 | S1a: `wolf_close_pane`'s `do_quit({})` becomes `do_quit({ force = true })` | 192, one Neovim |
/// | S1 | as above | S1b: `shell.rs` `perform`: `UiAction::ClosePane => {}` | 194, no E37 |
/// | S2 | leg B: one Neovim, the app alive, the readback names [`CP_FILE_A`] | S2a: `do_quit({})` becomes `_G.wolf_refuse_unsaved()` | 195, two Neovims |
/// | S3 | leg C: the app exits 0 within the wait | S3a: `tabs_ui::close_outcome` answers `CloseWindow` only for `live_pages == 0` | 197, the app stays alive |
///
/// At the base, before Close Pane takes `:q`'s path, the smoke is red with 194: `wolf_do_close`
/// refuses the last pane without an echo.
#[test]
fn close_pane_closes_the_tab_on_its_last_pane_under_cage() {
    const SMOKE: &str = "close_pane_closes_the_tab_on_its_last_pane_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let project = canonical(SMOKE, &cwd);
    for (name, heading) in [(CP_FILE_A, "# CP HERON"), (CP_FILE_B, "# CP LARK")] {
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
            1,
            vec![file_tab(CP_FILE_A), file_tab(CP_FILE_B)],
        )],
    };
    plant_window_state(SMOKE, &cwd, &planted);

    let rt: &Path = &runtime_dir;
    let read_s = rt.join("cp_read_s.txt");
    let read_a = rt.join("cp_read_a.txt");
    let read_b = rt.join("cp_read_b.txt");
    let msg_a = rt.join("cp_msg_a.txt");
    let out_png = rt.join("wolf_gnome_close_pane_unused.png");
    for path in [&read_s, &read_a, &read_b, &msg_a] {
        clear_evidence(path, SMOKE);
    }

    let body = with_nvim_pid_helper(&format!(
        "{EXTRAS_HELPERS}\n{READBACK_HELPERS}\n{CLOSE_PANE_BODY}"
    ));
    let status = pointer_command(&body, &cwd, &runtime_dir, &out_png)
        .env("CMD_FIFO", rt.join(POINTER_CMD_FIFO_NAME))
        .env("ACK_FILE", rt.join(POINTER_ACK_NAME))
        .env("HELPER_ERR", pointer_err_path(rt))
        .env("READY_FILE", pointer_ready_path(rt))
        .env("CP_FILE_A", CP_FILE_A)
        .env("CP_FILE_B", CP_FILE_B)
        .env("CP_READ_S", &read_s)
        .env("CP_READ_A", &read_a)
        .env("CP_READ_B", &read_b)
        .env("CP_MSG_A", &msg_a)
        .status()
        .expect("failed to spawn `cage` for the headless Close Pane scenario");

    assert!(
        !status.code().is_some_and(|code| CP_EXITS.contains(&code)),
        "{SMOKE}: the cage Close Pane scenario exited with one of its own codes ({status}). The \
         cage log in the runtime dir carries the scenario's own lines.\n{CP_CODES}"
    );
    assert_pointer_scenario_succeeded(SMOKE, status, rt, &out_png);
}
