//! The tab bar's smoke: each tab is titled by its own Neovim's working directory, live, a
//! background tab included.
//!
//! The launch restores one planted window of two tabs, both with the sidebar hidden, so nothing
//! but the tab bar and the header bar draws a directory's name: the sidebar's root row would
//! draw the tab's directory too. The header draws only the selected tab's directory, so each
//! assertion reads a background tab's bar entry.
//!
//! The pointer helper's files and every readback live under the runtime dir, never in a
//! directory a tab is rooted at.
//!
//! The smoke pins `gtk-titlebar-style: native`, because its clicks and its band are the tab
//! bar's own row.

use super::extras::{plant_window_state, planted_window, tab_bar_y, two_tab_x, EXTRAS_HELPERS};
use super::open_folder::{canonical, make_dir};
use super::recovery::with_nvim_pid_helper;
use super::*;
use wolf_core::app_state::{Sidebar, Tab, TabContent};

/// `A`, a sibling of `project` under the smoke's `HOME`: tab 0 `:cd`s into it while selected.
pub(crate) const TB_DIR_A: &str = "ttkestrel";

/// `B`, tab 1's planted directory, which it leaves while in the background.
pub(crate) const TB_DIR_B: &str = "ttpelican";

/// `C`, where tab 1's `FocusLost` autocmd `:cd`s once tab 1 is no longer selected.
pub(crate) const TB_DIR_C: &str = "ttplover";

/// What OCR must find in capture Y: tab 0's bar entry, `~/`[`TB_DIR_A`].
pub(crate) const TB_OCR_A: &str = "ttkestrel";

/// What OCR must not find in capture Z: tab 1's title before its background `:cd`.
pub(crate) const TB_OCR_B: &str = "ttpelican";

/// What OCR must find in capture Z: tab 1's bar entry after its background `:cd`.
pub(crate) const TB_OCR_C: &str = "ttplover";

/// The readback tab 1's Neovim writes after capture Y: its `getcwd()`, which must be `B`.
pub(crate) const TB_READ_B_NAME: &str = "tabbar_read_b.txt";

/// The readback tab 1's `DirChanged` autocmd writes once its `:cd` fires that event: `getcwd()`,
/// which must be `C`.
pub(crate) const TB_READ_C_NAME: &str = "tabbar_read_c.txt";

/// The readback tab 0's Neovim writes after capture Z: its `getcwd()`, which must be `A`.
pub(crate) const TB_READ_A_NAME: &str = "tabbar_read_a.txt";

/// The exit codes [`TAB_BAR_BODY`] adds to [`POINTER_PREAMBLE`]'s.
const TB_EXITS: std::ops::RangeInclusive<i32> = 160..=163;

/// Body of [`each_tab_is_titled_by_its_own_directory_under_cage`], appended to
/// [`POINTER_PREAMBLE`], [`EXTRAS_HELPERS`] and `recovery::with_nvim_pid_helper`'s helper.
///
/// 1. Two restored Neovims, then the restore's and the bar's settles.
/// 2. `:cd ~/A` in tab 0, selected at launch.
/// 3. A click on tab 1's entry, capture Y, then readback B from tab 1.
/// 4. In tab 1, a one-shot `DirChanged` autocmd that writes readback C, then a one-shot nested
///    `FocusLost` autocmd: `cd ~/C`, then `redraw!`.
/// 5. A click on tab 0's entry, which fires the `FocusLost` autocmd once tab 1 has left the
///    selection; readback C, capture Z, then readback A from tab 0.
///
/// Every capture is taken before anything is typed in the tab it shows, except Z's tab 0,
/// whose cmdline strip may still draw step 2's `:cd ~/A`, which carries no term Z asserts on.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15 and 40
///
/// | code | meaning |
/// | --- | --- |
/// | `160` | the launch did not restore exactly two Neovims |
/// | `161` | readback B never appeared: no Ex command reached tab 1's Neovim after the click |
/// | `162` | readback C never appeared: tab 1's `:cd` fired no `DirChanged` |
/// | `163` | readback A never appeared: no Ex command reached tab 0's Neovim after the click |
const TAB_BAR_BODY: &str = r#"
wait_nvim_count 2 160 "at launch, with two restored tabs"
sleep "$RESTORE_SETTLE"
sleep "$TABS_BAR_SETTLE"

nvim_ex "cd ~/$TB_DIR_A"

click_tab "$TAB1_X" "$TAB_Y"
grim "$TB_SHOT_Y"
nvim_ex "call writefile([getcwd()], '$TB_READ_B')"
wait_file "$TB_READ_B" || exit 161

# The switch away sends tab 1 set_ui_focus(false), so the :cd runs in a background tab.
# ++nested lets that :cd fire DirChanged, which writes readback C and Wolf's WolfDir handler hears.
nvim_ex "autocmd DirChanged * ++once call writefile([getcwd()], '$TB_READ_C')"
nvim_ex "autocmd FocusLost * ++once ++nested cd ~/$TB_DIR_C | redraw!"

click_tab "$TAB0_X" "$TAB_Y"
wait_file "$TB_READ_C" || exit 162
sleep "$SETTLE"
grim "$TB_SHOT_Z"
nvim_ex "call writefile([getcwd()], '$TB_READ_A')"
wait_file "$TB_READ_A" || exit 163

hsend quit
exit 0
"#;

/// The tab bar's band, the full output width from the header's bottom edge down
/// [`TAB_BAR_HEIGHT_EXPECTED`], as `extras::tab_bar_y` places the bar.
pub(crate) fn tab_bar_band() -> SampleRect {
    let width = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    SampleRect {
        x0: 0,
        x1: pixel_index(width, "the tab bar's right edge"),
        y0: pixel_index(OVERLAY_ORIGIN_Y_EXPECTED, "the tab bar's top edge"),
        y1: pixel_index(
            OVERLAY_ORIGIN_Y_EXPECTED + TAB_BAR_HEIGHT_EXPECTED,
            "the tab bar's bottom edge",
        ),
    }
}

/// A saved tab on `directory` with its sidebar hidden, so no root row draws its name.
fn hidden_sidebar_tab(directory: &str) -> Tab {
    Tab {
        directory: directory.to_owned(),
        content: TabContent {
            sidebar: Sidebar {
                visible: false,
                ..Sidebar::default()
            },
            ..TabContent::default()
        },
    }
}

/// **Each tab is titled by its own Neovim's working directory, and follows it live while in the
/// background.**
///
/// The planted window holds tab 0 on the temp cwd `project` and tab 1 on [`TB_DIR_B`], tab 0
/// selected. Tab 0 must be a saved tab on `project`, or `restore_plans` opens a fresh window
/// there beside the planted one. [`TB_DIR_A`], [`TB_DIR_B`] and [`TB_DIR_C`] are siblings under
/// the smoke's `HOME`, so none is another's prefix and each title is `~/<name>`.
///
/// Both tabs hide the sidebar, whose root row would otherwise draw the selected tab's directory
/// beside its bar entry. The header draws the selected tab's directory, so Y, with tab 1 selected,
/// can draw `A` only from tab 0's entry, and Z, with tab 0 selected, can draw `C` only from tab
/// 1's.
///
/// Tab 1 leaves `B` from a one-shot `FocusLost` autocmd. The switch to tab 0 sends tab 1's
/// Neovim `set_ui_focus(false)`, which fires it once tab 1 has left the selection, so the `:cd`
/// runs in a background tab with no timing race. The autocmd's `redraw!` makes Neovim flush
/// after the `:cd`, which is the wake the core relies on to report a changed cwd (`redraw.rs`'s
/// `CWD_EVENT` arm).
///
/// The `FocusLost` autocmd is `++nested` because autocommands do not nest by default
/// (`:help autocmd-nested`): without it the `:cd` fires no `DirChanged`. `getcwd()` still moves,
/// but Wolf's `live_cwd` does not, since only the `WolfDir` handler in `runtime/nvim/init.lua`
/// moves it, and that handler runs on `DirChanged`. So readback C is written by a `DirChanged`
/// autocmd of its own, and its presence proves the event fired.
///
/// Each capture is read twice, whole and as the tab bar's band ([`tab_bar_band`]), because
/// tesseract was measured reading one of two tab titles on a whole page at the default
/// segmentation mode (`notes/crates/wolf-gnome/tests/smoke_headless/panes.rs.md`). A
/// must-contain holds when either reading has the term, and a must-not-contain only when
/// neither does.
///
/// Premise, measured by the green run: a tab title of the form `~/<name>` OCR-reads as `<name>`
/// at the smoke font, as the header bar's title does in `sidebar.rs`'s title smoke.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: readback B is `B`, readback C is `C`, readback A is `A` | none | n/a |
/// | TB1a | capture Y holds [`TB_OCR_A`] | in `TabHandle::refresh_label`, delete the title's `set_title` call | Y lacks `A`, tab 0's entry still draws `Wolf` |
/// | TB1b | capture Z holds [`TB_OCR_C`] and lacks [`TB_OCR_B`] | in `TabHandle::refresh`, return right after `panes.on_render_wake()` when `!self.is_selected()` | Z lacks `C`, tab 1 keeps `~/B`; Y stays green |
#[test]
fn each_tab_is_titled_by_its_own_directory_under_cage() {
    const SMOKE: &str = "each_tab_is_titled_by_its_own_directory_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let project = canonical(SMOKE, &cwd);
    let home = smoke_home(&cwd);
    let dir_a = make_dir(SMOKE, home, TB_DIR_A);
    let dir_b = make_dir(SMOKE, home, TB_DIR_B);
    let dir_c = make_dir(SMOKE, home, TB_DIR_C);
    let planted = WindowState {
        windows: vec![planted_window(
            0,
            vec![hidden_sidebar_tab(&project), hidden_sidebar_tab(&dir_b)],
        )],
    };
    plant_window_state(SMOKE, &cwd, &planted);

    let rt: &Path = &runtime_dir;
    let shot_y = rt.join("wolf_gnome_tab_bar_y.png");
    let shot_z = rt.join("wolf_gnome_tab_bar_z.png");
    let read_a = rt.join(TB_READ_A_NAME);
    let read_b = rt.join(TB_READ_B_NAME);
    let read_c = rt.join(TB_READ_C_NAME);
    let out_png = rt.join("wolf_gnome_tab_bar_unused.png");
    for path in [&shot_y, &shot_z, &read_a, &read_b, &read_c] {
        clear_evidence(path, SMOKE);
    }
    let (tab0_x, tab1_x, tab_y) = (two_tab_x(0), two_tab_x(1), tab_bar_y());

    let body = with_nvim_pid_helper(&format!("{EXTRAS_HELPERS}\n{TAB_BAR_BODY}"));
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
        .env("TB_DIR_A", TB_DIR_A)
        .env("TB_DIR_C", TB_DIR_C)
        .env("TB_SHOT_Y", &shot_y)
        .env("TB_SHOT_Z", &shot_z)
        .env("TB_READ_A", &read_a)
        .env("TB_READ_B", &read_b)
        .env("TB_READ_C", &read_c)
        .status()
        .expect("failed to spawn `cage` for the headless tab bar scenario");

    assert!(
        !status.code().is_some_and(|code| TB_EXITS.contains(&code)),
        "{SMOKE}: the cage tab bar scenario exited with one of its own codes ({status}). The cage \
         log in the runtime dir carries the scenario's own lines.\n\
         Exit 160: the launch did not restore exactly two Neovims, so the planted window was not \
         restored as two tabs.\n\
         Exit 161: readback B never appeared, so no Ex command reached tab 1's Neovim after the \
         click on its entry: the click missed the bar, or keys still go to tab 0.\n\
         Exit 162: readback C never appeared, so tab 1's `:cd` fired no DirChanged: the switch \
         to tab 0 sent tab 1 no set_ui_focus(false), or the FocusLost autocmd lacks ++nested, or \
         the autocmds were never defined.\n\
         Exit 163: readback A never appeared, so no Ex command reached tab 0's Neovim after the \
         click on its entry."
    );
    assert_pointer_scenario_succeeded(SMOKE, status, rt, &out_png);
    let extents = read_output_extents(rt, SMOKE);
    for (label, point) in [
        ("tab 0's bar entry", (tab0_x, tab_y)),
        ("tab 1's bar entry", (tab1_x, tab_y)),
    ] {
        assert_inside_extents(SMOKE, label, point, extents);
    }

    for (path, want, what) in [
        (
            &read_b,
            &dir_b,
            "readback B, typed into tab 1 after capture Y",
        ),
        (
            &read_c,
            &dir_c,
            "readback C, written by tab 1's DirChanged autocmd",
        ),
        (
            &read_a,
            &dir_a,
            "readback A, typed into tab 0 after capture Z",
        ),
    ] {
        let got = read_trimmed(path, SMOKE);
        assert_eq!(
            &got, want,
            "{SMOKE}: precondition: {what} reports getcwd() = {got:?}, not {want:?}. Readback B \
             wrong means tab 1 was not selected for capture Y or was not restored on B; C wrong \
             means the FocusLost autocmd's `:cd ~/` did not reach the smoke's HOME; A wrong means \
             step 2's `:cd` did not run in tab 0, or tab 0 was not selected for capture Z"
        );
    }

    let band = tab_bar_band();
    let read_twice = |png: &Path, name: &str| {
        assert_png_present(SMOKE, png);
        let page = ocr_to_lowercase(png);
        let band_png = rt.join(format!("wolf_gnome_tab_bar_{name}_band.png"));
        let bar = ocr_band_to_lowercase(png, band, &band_png, SMOKE);
        (page, bar)
    };

    let (page_y, bar_y) = read_twice(&shot_y, "y");
    assert!(
        page_y.contains(TB_OCR_A) || bar_y.contains(TB_OCR_A),
        "{SMOKE}: capture Y, tab 1 selected beside tab 0, lacks {TB_OCR_A:?}. The header draws \
         ~/{TB_DIR_B} and no sidebar is shown, so only tab 0's bar entry can draw it: tab 0's \
         title did not follow its Neovim's `:cd` (it still reads {WINDOW_TITLE_FALLBACK:?} or \
         ~/{TEMP_CWD_BASENAME}). If either reading shows a misread of the title instead, that is \
         the premise in this smoke's doc. PNG kept at {}.\nPage OCR:\n{page_y}\nBar OCR:\n{bar_y}",
        shot_y.display()
    );

    let (page_z, bar_z) = read_twice(&shot_z, "z");
    assert!(
        page_z.contains(TB_OCR_C) || bar_z.contains(TB_OCR_C),
        "{SMOKE}: capture Z, tab 0 selected beside tab 1, lacks {TB_OCR_C:?}. Readback C proves \
         tab 1's Neovim moved to ~/{TB_DIR_C} and fired DirChanged after it left the selection, \
         and the header draws ~/{TB_DIR_A}, so only tab 1's bar entry can draw it: a background \
         tab's title does not follow its own Neovim's cwd. If {TB_OCR_B:?} is in the readings \
         below, tab 1 kept the title it had while selected. PNG kept at {}.\n\
         Page OCR:\n{page_z}\nBar OCR:\n{bar_z}",
        shot_z.display()
    );
    assert!(
        !page_z.contains(TB_OCR_B) && !bar_z.contains(TB_OCR_B),
        "{SMOKE}: capture Z still draws {TB_OCR_B:?}, though tab 1's Neovim left ~/{TB_DIR_B} \
         and the header draws ~/{TB_DIR_A}, so only tab 1's bar entry can draw it: that entry \
         kept the title it had while selected, and a background tab's title does not follow its \
         own Neovim's cwd. PNG kept at {}.\n\
         Page OCR:\n{page_z}\nBar OCR:\n{bar_z}",
        shot_z.display()
    );
}
