//! The terminal drawer's divider smokes: a pointer drag on the handle between the editor tree
//! and the drawer resizes both, reaches the drawer's session, survives the drawer closing and
//! reopening, stops at both floors, and stays out of a new tab opened from that tab. A dragged
//! share also survives a window shrink that clamps the editor and a grow back, and each tab of
//! a window keeps its own share across a relaunch.
//!
//! The smokes that open a second tab pin `gtk-titlebar-style: native`, because their
//! two-tab pane area and their tab clicks include the tab bar's own row.

use super::extras::{tab_bar_y, two_tab_x, EXTRAS_HELPERS};
use super::terminal::{drawer_paned_at, drawer_strip_edges, pin_terminal_smoke_shell};
use super::*;

/// Pixels the handle is dragged UP in the first drag: about three and a half cells at
/// [`SMOKE_FONT_PT`], against a drawer that opens one row tall. It moves the editor from
/// 418 px to 268 px, well clear of both floors, so the drag lands unclamped.
const DRAWER_DRAG_PX: i64 = 150;

/// The fewest extra rows the drawer's session must gain from [`DRAWER_DRAG_PX`]. The drag
/// adds 150 px, three whole 44 px cells, so two leaves a cell for the sub-cell phase.
const MIN_DRAG_ROWS: i64 = 2;

/// How far ABOVE the pane area's top edge the limit drag releases. Above the edge, so the
/// position GTK computes is negative and only the editor's floor can stop it.
const LIMIT_OVERSHOOT_PX: f64 = 20.0;

/// The inclusive band of exit codes [`DRAWER_DIVIDER_BODY`] adds to
/// [`POINTER_PREAMBLE`]'s 11-15 and 40.
const OWN_EXITS: std::ops::RangeInclusive<i32> = 71..=81;

/// Body of [`drawer_divider_drag_resizes_and_is_remembered_under_cage`], appended to
/// [`POINTER_PREAMBLE`] and [`DRAWER_DIVIDER_HELPERS`].
///
/// Each readback is typed where focus should be: `nvim_*` into an editor pane, which
/// Neovim answers, and `shell_size` into the drawer, whose shell answers with `stty size`.
/// A readback typed where focus is not never appears, and its exit code names the leg.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | readback | meaning |
/// | --- | --- | --- |
/// | `71` | L | Neovim did not answer at launch |
/// | `72` | O | no shell answered after Ctrl+`: the chord opened no focused drawer |
/// | `73` | G-O | Neovim did not answer after `<C-w>k` out of the drawer |
/// | `74` | G-D | Neovim did not answer after the drag: keyboard focus left the editor |
/// | `75` | D | no shell answered after `<C-w>j` into the dragged drawer |
/// | `76` | C | Neovim did not answer once the drawer was hidden |
/// | `77` | R | no shell answered after the drawer reopened |
/// | `78` | G-R | Neovim did not answer after `<C-w>k` out of the reopened drawer |
/// | `79` | M | no shell answered after the limit drag and `<C-w>j` |
/// | `80` | tab | Neovim did not answer after Ctrl+Shift+T |
/// | `81` | T | no shell answered after Ctrl+` in the second tab |
const DRAWER_DIVIDER_BODY: &str = r#"
# The adw::TabBar animates its height in over about a second; see POINTER_TABS_BODY.
TABS_BAR_SETTLE=2.0

# $1 is the readback file, $2 the exit code. Only Neovim writes these.
nvim_solo() {
    wtype -- ":call writefile([printf('%d %d', winheight(0), winnr('\$'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# The editor is window 1 and the botright drawer the last window.
nvim_geometry() {
    wtype -- ":call writefile([printf('%d %d %d %d', winheight(1), winheight(winnr('\$')), getwinvar(winnr('\$'), 'wolf_drawer', 0), winnr('\$'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

nvim_tab() {
    wtype -- ":call writefile([printf('%d %d', tabpagenr(), winnr('\$'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# $1 is the readback file, $2 the exit code. Only the drawer's shell writes it.
shell_size() {
    wtype -- "stty size > '$1'"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# --- L: the editor alone, at launch -------------------------------------------
nvim_solo "$READ_L" 71

# --- O, G-O: the drawer opens at a new window's share -------------------------
toggle_drawer
shell_size "$READ_O" 72
chord_w k
nvim_geometry "$READ_GO" 73

# --- G-D, D: drag the handle up -----------------------------------------------
drag_handle "$HANDLE_Y" "$DRAGGED_Y"
# Post-mortem only: no assertion reads a pixel here.
grim "$OUT_PNG"
nvim_geometry "$READ_GD" 74
chord_w j
shell_size "$READ_D" 75

# --- C: the drawer hidden, from inside it --------------------------------------
toggle_drawer
nvim_solo "$READ_C" 76

# --- R, G-R: the drawer reopened ------------------------------------------------
toggle_drawer
shell_size "$READ_R" 77
chord_w k
nvim_geometry "$READ_GR" 78

# --- M: drag the handle past the pane area's top edge ---------------------------
drag_handle "$DRAGGED_Y" "$LIMIT_Y"
chord_w j
shell_size "$READ_M" 79

# --- T: a second tab of the same window opens its own drawer --------------------
chord_w k
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
# New Tab's tab is empty, its Neovim at HOME; the relative :cd makes it a folder tab.
wtype -- ":cd $TAB_CD_TARGET"
wtype -k Return
sleep "$SETTLE"
nvim_tab "$READ_TAB" 80
toggle_drawer
shell_size "$READ_T" 81

hsend quit
exit 0
"#;

/// The shell helpers both drawer-divider bodies use, placed after [`POINTER_PREAMBLE`] and
/// before the body: the window chord, the drawer toggle and the handle drag.
const DRAWER_DIVIDER_HELPERS: &str = r#"
# The drawer chord's round trip; see DRAWER_SCENARIO in terminal.rs.
DRAWER_SETTLE=2.0

chord_w() {
    wtype -M ctrl -k w -m ctrl
    wtype -k "$1"
    sleep "$SETTLE"
}

toggle_drawer() {
    wtype -M ctrl -k grave -m ctrl
    sleep "$DRAWER_SETTLE"
}

# $1 is the press y, on the handle, and $2 the release y. `hstep` for every event of the
# gesture -- see `hstep` in POINTER_PREAMBLE.
drag_handle() {
    hstep motion "$HANDLE_X" "$1"
    hstep press "$BTN_LEFT"
    hstep motion "$HANDLE_X" "$(( ($1 + $2) / 2 ))"
    hstep motion "$HANDLE_X" "$2"
    hstep release "$BTN_LEFT"
    sleep "$SETTLE"
}
"#;

/// The drawer's divider is a draggable `gtk::Paned` handle whose position lives in the tab's
/// drawer share: a drag resizes the editor and the drawer and the drawer's session, the share
/// survives the drawer closing and reopening and stays out of a new tab opened from it, both
/// floors hold, and with the drawer closed the editor has the whole pane area back.
///
/// ## The scenario
///
/// One launch through [`pointer_command`], with [`pin_terminal_smoke_shell`]'s shell. Every
/// observable is a readback: `stty size` from the drawer's shell, or `winheight` from
/// Neovim, which under `ext_multigrid` reports the grid size the front-end drives. Nothing
/// is OCRed and no fixture is written.
///
/// The handle is pressed where [`drawer_strip_edges`] puts it for a new window's share,
/// about y 465 at 1280x720, and at [`PROBE_X`]. GTK's grab area reaches several pixels
/// either side of the one-pixel separator. The limit drag presses where the first drag
/// left it, [`DRAWER_DRAG_PX`] higher, which is only the handle if the share was kept.
///
/// ## The legs
///
/// Expected values at [`SMOKE_FONT_PT`] are in brackets: the pane area is 627 px, each
/// floor 176 px, and a pane's rows are its height less 132 px of chrome over a 44 px cell.
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | A1 | precondition: L has one window; G-O has two and the last is the drawer | none | n/a |
/// | C6a | A2a: the drawer's `winheight` in G-D exceeds G-O's and the editor's is below it | in `install_drawer_divider`'s `notify::position` handler, delete `share.set(next);` | the next wake re-applies the default share; G-D equals G-O |
/// | C6b | A2b: D rows ≥ O rows + [`MIN_DRAG_ROWS`] [5 against 1] | in `sync_terminal_sizes`, `if sent != Some(size) \|\| stale {` → `if sent.is_none() {` | D rows equal O rows |
/// | C7 | A3: C is L, the editor's `winheight(0)` and one window [11] | in `rebuild_pane_area`, with no drawer bound but a moved share, still append `build_drawer_paned` over an empty `gtk::Box` | C's `winheight(0)` is 3 |
/// | C8 | A4: R equals D, rows and columns, and G-R's drawer `winheight` equals G-D's | in `clear_drawer`, after `self.drawer.set(state);`, add `self.drawer_share.set(DrawerShare::new());` | R and G-R are back at O and G-O |
/// | C10 | A6: D rows < M rows < L's `winheight(0)` [5 < 7 < 11] | in `build_drawer_paned`, `set_shrink_start_child(false)` → `(true)` | M reaches L's 11 |
/// | E22b | A5: T rows ≤ O rows [1] | in `AppShell::new_tab`, give the new tab the selected tab's drawer share | T rows 6 > O's 1 |
///
/// C6a is the snap back on the next wake and C8 the snap back on a rebuild.
///
/// A5 holds because New Tab opens New Window's tab, whose drawer opens at a new share: the
/// second tab's pane area is [`TAB_BAR_HEIGHT_EXPECTED`] shorter, so a new share there gives the
/// drawer no more rows than O. The share the limit drag left would give it more, clamped to the
/// editor's floor in that tab.
///
/// ## Order
///
/// The legs are asserted in the order above, each once its readbacks exist. A leg whose
/// readback is missing ends the run on the scenario's exit code, so the earliest red is
/// the one reported. Under C8 the limit drag's press can miss the handle, which the
/// order puts after A4.
#[test]
fn drawer_divider_drag_resizes_and_is_remembered_under_cage() {
    const SMOKE: &str = "drawer_divider_drag_resizes_and_is_remembered_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();

    // Readbacks live under the runtime dir, as DRAWER_GEOMETRY_READBACK_NAME's do.
    let read = |name: &str| runtime_dir.join(format!("wolf_gnome_drawer_divider_{name}.txt"));
    let read_l = read("launch");
    let read_o = read("open_size");
    let read_go = read("open_geometry");
    let read_gd = read("drag_geometry");
    let read_d = read("drag_size");
    let read_c = read("closed");
    let read_r = read("reopen_size");
    let read_gr = read("reopen_geometry");
    let read_m = read("limit_size");
    let read_tab = read("tab");
    let read_t = read("tab_size");
    let out_png = runtime_dir.join("wolf_gnome_drawer_divider_smoke.png");
    let evidence = [
        &read_l, &read_o, &read_go, &read_gd, &read_d, &read_c, &read_r, &read_gr, &read_m,
        &read_tab, &read_t, &out_png,
    ];
    for path in evidence {
        clear_evidence(path, SMOKE);
    }

    let edges = drawer_strip_edges();
    let handle_x = i64::from(PROBE_X);
    let handle_y = pixel_target(edges.handle_y, "the drawer handle's y");
    let dragged_y = handle_y - DRAWER_DRAG_PX;
    let limit_y = pixel_target(
        OVERLAY_ORIGIN_Y_EXPECTED - LIMIT_OVERSHOOT_PX,
        "the limit drag's release y",
    );

    let body = format!("{DRAWER_DIVIDER_HELPERS}\n{DRAWER_DIVIDER_BODY}");
    let mut cmd = pointer_command(&body, &cwd, &runtime_dir, &out_png);
    pin_terminal_smoke_shell(&mut cmd)
        .env(
            "WOLF_CONFIG_DIR",
            titlebar_pinned_config_dir(&cwd, GTK_TITLEBAR_NATIVE),
        )
        .env("HANDLE_X", handle_x.to_string())
        .env("HANDLE_Y", handle_y.to_string())
        .env("DRAGGED_Y", dragged_y.to_string())
        .env("LIMIT_Y", limit_y.to_string())
        .env("READ_L", &read_l)
        .env("READ_O", &read_o)
        .env("READ_GO", &read_go)
        .env("READ_GD", &read_gd)
        .env("READ_D", &read_d)
        .env("READ_C", &read_c)
        .env("READ_R", &read_r)
        .env("READ_GR", &read_gr)
        .env("READ_M", &read_m)
        .env("READ_TAB", &read_tab)
        .env("READ_T", &read_t)
        .env("TAB_CD_TARGET", TEMP_CWD_BASENAME);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless drawer-divider scenario");

    // The harness's own failures first: after one, no readback is evidence of anything.
    if !status.code().is_some_and(|code| OWN_EXITS.contains(&code)) {
        assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    }

    // Extents next: a clamped injection lands elsewhere and misdirects every message below.
    let extents = read_output_extents(&cwd, SMOKE);
    for (label, y) in [
        ("the handle at a new window's share", handle_y),
        ("the handle after the first drag", dragged_y),
        ("the limit drag's release", limit_y),
    ] {
        assert_inside_extents(SMOKE, label, (handle_x, y), extents);
    }

    let leg = |paths: &[&Path], name: &str| require_leg(SMOKE, status, paths, name, handle_y);

    // --- A1 (precondition): the windows the readbacks describe -----------------
    leg(
        &[read_l.as_path(), read_o.as_path(), read_go.as_path()],
        "A1",
    );
    let l = read_ints(&read_l, 2, SMOKE);
    let o = read_ints(&read_o, 2, SMOKE);
    let go = read_ints(&read_go, 4, SMOKE);
    assert_eq!(
        l[1], 1,
        "{SMOKE}: Neovim reports {} windows at launch, want 1. L's winheight(0) is the editor \
         with the whole pane area only when it is the one window, and A3 and A6 compare against \
         it.",
        l[1]
    );
    assert_eq!(
        (go[2], go[3]),
        (1, 2),
        "{SMOKE}: with the drawer open Neovim reports (w:wolf_drawer of the last window, window \
         count) = ({}, {}), want (1, 2). A count of 1 means the chord opened no drawer; a last \
         window without w:wolf_drawer means winheight(winnr('$')) measures some other window.",
        go[2],
        go[3]
    );
    assert!(
        l[0] > 0 && o[0] > 0 && go[0] > 0 && go[1] > 0,
        "{SMOKE}: a readback came back non-positive (L winheight {}, O rows {}, G-O editor {} and \
         drawer {}), so the window or session it describes was never sized and every comparison \
         below would be against a placeholder.",
        l[0],
        o[0],
        go[0],
        go[1]
    );

    // --- A2a (C6a): the drag moved the divider and stayed moved ----------------
    leg(&[read_gd.as_path()], "A2a");
    let gd = read_ints(&read_gd, 4, SMOKE);
    assert!(
        gd[1] > go[1] && gd[0] < go[0],
        "{SMOKE}: after dragging the handle {DRAWER_DRAG_PX} px up, Neovim reports (editor, \
         drawer) winheight ({}, {}), against ({}, {}) before the drag; want the drawer taller \
         and the editor shorter. Read this FIRST as the press missing the handle: it was \
         pressed at ({handle_x}, {handle_y}), where `drawer_strip_edges` puts a new window's \
         handle, and the retained capture {} shows where it is. Equal heights with the press on \
         the handle mean the position snapped back to the default share: the paned's \
         `notify::position` handler did not write the drag into the tab's share, so the \
         next wake re-applied the old one. A taller editor means the share's direction is \
         inverted.",
        gd[0],
        gd[1],
        go[0],
        go[1],
        out_png.display()
    );

    // --- A2b (C6b): the drawer's session follows the new size ------------------
    leg(&[read_d.as_path()], "A2b");
    let d = read_ints(&read_d, 2, SMOKE);
    assert!(
        d[0] >= o[0] + MIN_DRAG_ROWS,
        "{SMOKE}: the drawer's shell reports {} rows after the drag, {} before it; want at least \
         {MIN_DRAG_ROWS} more. A2a has passed, so Neovim's drawer window grew: equal rows mean \
         the session was never resized for the new pane size, as `sync_terminal_sizes` must \
         for any pane.",
        d[0],
        o[0]
    );

    // --- A3 (C7): nothing of the divider remains with the drawer closed --------
    leg(&[read_c.as_path()], "A3");
    let c = read_ints(&read_c, 2, SMOKE);
    assert_eq!(
        (c[0], c[1]),
        (l[0], l[1]),
        "{SMOKE}: with the drawer hidden Neovim reports (winheight(0), window count) = ({}, {}), \
         against ({}, {}) at launch. A second window means the hide never arrived. One window \
         with fewer rows means the editor still shares the pane area with something: a divider \
         left in the widget tree with no drawer bound, which also moves every pointer smoke's \
         targets.",
        c[0],
        c[1],
        l[0],
        l[1]
    );

    // --- A4 (C8): the share survives the drawer closing and reopening ----------
    leg(&[read_r.as_path(), read_gr.as_path()], "A4");
    let r = read_ints(&read_r, 2, SMOKE);
    let gr = read_ints(&read_gr, 4, SMOKE);
    assert_eq!(
        (r[0], r[1]),
        (d[0], d[1]),
        "{SMOKE}: the reopened drawer's shell reports {} rows and {} columns, against {} and {} \
         before it was hidden (and {} rows when it first opened). Rows back at the first opening \
         mean the share was reset when the drawer closed, as `DrawerState::clear` must not do \
         to it. Other columns mean the session was resized as a different pane.",
        r[0],
        r[1],
        d[0],
        d[1],
        o[0]
    );
    assert_eq!(
        gr[1], gd[1],
        "{SMOKE}: the reopened drawer's Neovim window is {} rows, against {} before it was hidden \
         and {} when it first opened. The paned rebuilt on reopening must take its position from \
         the tab's share, not from DRAWER_WEIGHT.",
        gr[1], gd[1], go[1]
    );

    // --- A6 (C10): both floors hold --------------------------------------------
    leg(&[read_m.as_path()], "A6");
    let m = read_ints(&read_m, 2, SMOKE);
    assert!(
        d[0] < m[0],
        "{SMOKE}: after dragging the handle past the pane area's top edge the drawer's shell \
         reports {} rows, no more than the {} it had at the dragged position. The press at y \
         {dragged_y} missed the handle, or the drag did not move it: A4 has passed, so the \
         share put the handle there.",
        m[0],
        d[0]
    );
    assert!(
        m[0] < l[0],
        "{SMOKE}: dragged past the pane area's top edge, the drawer's session has {} rows, as \
         many as the {} the editor has with the whole pane area: the editor was squeezed below \
         its floor. The paned's start child must not shrink below `make_pane_area`'s minimum.",
        m[0],
        l[0]
    );

    // --- A5 (E22b): a new tab opens its drawer at a new share -------------------
    leg(&[read_tab.as_path(), read_t.as_path()], "A5");
    let tab = read_ints(&read_tab, 2, SMOKE);
    assert_eq!(
        (tab[0], tab[1]),
        (1, 1),
        "{SMOKE}: after Ctrl+Shift+T Neovim reports (tabpagenr(), window count) = ({}, {}), want \
         (1, 1): the second tab's own Neovim, on its one tabpage, holding its editor window \
         alone. (1, 2) is the first tab's editor and drawer, so the chord opened no tab and T \
         would measure the first tab's drawer; a tabpage number above 1 means the new tab is a \
         tabpage of another tab's Neovim.",
        tab[0],
        tab[1]
    );
    let t = read_ints(&read_t, 2, SMOKE);
    assert!(
        t[0] <= o[0],
        "{SMOKE}: the second tab's drawer opened with {} rows, more than the {} a new window's \
         drawer has. The second tab's pane area is {TAB_BAR_HEIGHT_EXPECTED} px shorter, so a \
         new share there gives at most O's rows: New Tab gave the new tab the selected tab's \
         dragged share rather than New Window's new one.",
        t[0],
        o[0]
    );
}

/// Ends the run on the scenario's exit status when any of `paths` is missing, so the leg
/// that needs them is never asserted on a readback the scenario did not reach.
fn require_leg(smoke: &str, status: ExitStatus, paths: &[&Path], leg: &str, handle_y: i64) {
    let missing: Vec<String> = paths
        .iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "{smoke}: leg {leg}'s readbacks {missing:?} do not exist, and the scenario exited \
         {status}. A shell readback that never appeared means focus was in an editor and Neovim \
         got `stty size` as keys; a Neovim one, that focus was in a terminal. 71 L, 72 O (the \
         chord opened no focused drawer), 73 G-O, 74 G-D (the drag left keyboard focus off the \
         editor), 75 D, 76 C, 77 R, 78 G-R, 79 M (the press at the dragged y, {} px above \
         {handle_y}, may have started a text drag in the editor), 80 the second tab, 81 T. An \
         exit 0 with a file missing means it was written somewhere else.",
        DRAWER_DRAG_PX
    );
}

/// How far above the editor's floor, in output pixels, [`DRAWER_RESIZE_BODY`]'s drag leaves
/// the handle at the launch size: close enough that a shrink clamps the editor at its floor,
/// and far enough that the drag itself lands unclamped.
const RESIZE_DRAG_ABOVE_FLOOR_PX: i32 = 10;

/// The output height [`DRAWER_RESIZE_BODY`] shrinks to, at the launch width.
const SHRUNK_OUTPUT_H: i64 = 500;

/// The fewest rows by which the drawer's window at the kept share must differ, at the launch
/// size, from the window at the share the shrink's clamp would leave. Two, so a drag landing
/// a few pixels off its target still leaves a row between them.
const MIN_CLAMP_ROW_GAP: i64 = 2;

/// The inclusive band of exit codes [`DRAWER_RESIZE_BODY`] adds to [`POINTER_PREAMBLE`]'s
/// 11-15 and 40, [`RESIZE_OUTPUT_FRAGMENT`]'s two included.
const RESIZE_OWN_EXITS: std::ops::RangeInclusive<i32> = 95..=101;

/// Body of [`a_dragged_share_survives_a_window_shrink_and_grow_under_cage`], appended to
/// [`POINTER_PREAMBLE`], [`DRAWER_DIVIDER_HELPERS`] and [`RESIZE_OUTPUT_FRAGMENT`].
///
/// Every readback is a Neovim one, typed into the editor: `&columns`, `&lines`, the
/// editor's and the last window's `winheight`, the last window's `w:wolf_drawer` and the
/// window count. With one window the editor is the last window too.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | readback | meaning |
/// | --- | --- | --- |
/// | `95` | L | Neovim did not answer at launch |
/// | `96` | O | Neovim did not answer after Ctrl+` and `<C-w>k` out of the drawer |
/// | `97` | G | Neovim did not answer after the drag: keyboard focus left the editor |
/// | `98` | S | Neovim did not answer after the shrink |
/// | `99` | B | Neovim did not answer after the grow back |
/// | `100` | S or B | `wlr-randr` listed no output or refused the mode |
/// | `101` | S or B | the output never reported the mode as current |
const DRAWER_RESIZE_BODY: &str = r#"
# $1 is the readback file, $2 the exit code.
nvim_sizes() {
    wtype -- ":call writefile([printf('%d %d %d %d %d %d', &columns, &lines, winheight(1), winheight(winnr('\$')), getwinvar(winnr('\$'), 'wolf_drawer', 0), winnr('\$'))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# --- L: the editor alone, at launch -------------------------------------------
nvim_sizes "$READ_L" 95

# --- O: the drawer opens at a new window's share --------------------------------
toggle_drawer
chord_w k
nvim_sizes "$READ_O" 96

# --- G: drag the handle to just above the editor's floor -------------------------
drag_handle "$HANDLE_Y" "$TARGET_Y"
# Post-mortem only: no assertion reads a pixel here.
grim "$OUT_PNG"
nvim_sizes "$READ_G" 97

# --- S: shrink the output; no pointer event follows ------------------------------
resize_output "$LAUNCH_W" "$SHRUNK_H" 100 101
grim "$SHRUNK_PNG"
nvim_sizes "$READ_S" 98

# --- B: grow it back to the launch size -----------------------------------------
resize_output "$LAUNCH_W" "$LAUNCH_H" 100 101
nvim_sizes "$READ_B" 99

hsend quit
exit 0
"#;

/// One `nvim_sizes` readback of [`DRAWER_RESIZE_BODY`].
#[derive(Clone, Copy, Debug)]
struct Sizes {
    /// `(&columns, &lines)`: the outer grid, which follows the toplevel's size.
    outer: (i64, i64),
    /// `winheight(1)`, the editor.
    editor: i64,
    /// `winheight(winnr('$'))`, the drawer while it is open.
    drawer: i64,
    /// The last window's `w:wolf_drawer`.
    tagged: i64,
    /// `winnr('$')`.
    count: i64,
}

/// Reads an `nvim_sizes` readback.
fn read_sizes(path: &Path, smoke: &str) -> Sizes {
    let v = read_ints(path, 6, smoke);
    Sizes {
        outer: (v[0], v[1]),
        editor: v[2],
        drawer: v[3],
        tagged: v[4],
        count: v[5],
    }
}

/// The derived geometry [`a_dragged_share_survives_a_window_shrink_and_grow_under_cage`]
/// rests on, checked before launch by [`resize_premises`].
struct ResizePremises {
    /// The editor tree's height the drag leaves at the launch size.
    target: i32,
    /// The editor's rows at its floor, on the shrunk output.
    floor_rows: i64,
    /// The drawer's rows at the launch size for the dragged share.
    kept_drawer_rows: i64,
    /// The drawer's rows at the launch size for the share the shrink's clamp would leave.
    clamped_drawer_rows: i64,
}

/// Derives [`ResizePremises`] through `drawer_paned_at` and `DrawerShare`, panicking when a
/// premise the scenario needs does not hold.
fn resize_premises(smoke: &str) -> ResizePremises {
    let launch = drawer_paned_at(exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED"));
    let shrunk = drawer_paned_at(exact_f64(SHRUNK_OUTPUT_H, "SHRUNK_OUTPUT_H"));
    let target = launch.floor + RESIZE_DRAG_ABOVE_FLOOR_PX;
    assert!(
        launch.floor < target && target < launch.max,
        "{smoke}: the drag target {target} px is outside ({}, {}), the editor's floor and the \
         paned's max_position at the launch size, so GTK would clamp the drag itself and G would \
         record a clamp rather than a drag. Lower RESIZE_DRAG_ABOVE_FLOOR_PX.",
        launch.floor,
        launch.max
    );

    let mut kept = wolf_gnome::drawer::DrawerShare::new();
    kept.drag_to(target, launch.span);
    let unclamped = kept.position(0, shrunk.span, shrunk.span);
    let at_shrunk = kept.position(shrunk.floor, shrunk.max, shrunk.span);
    assert!(
        unclamped < shrunk.floor && at_shrunk == shrunk.floor,
        "{smoke}: at a {SHRUNK_OUTPUT_H} px output the dragged share puts the editor at {unclamped} \
         px before the clamp and {at_shrunk} px after it, against a {} px floor; want the clamp \
         to hold it at the floor. Without that clamp the shrink takes nothing from the share, \
         and A4 holds whatever the guard does. Lower SHRUNK_OUTPUT_H.",
        shrunk.floor
    );

    let mut clamped = kept;
    clamped.drag_to(shrunk.floor, shrunk.span);
    let drawer_rows = |share: wolf_gnome::drawer::DrawerShare| {
        let position = share.position(launch.floor, launch.max, launch.span);
        launch.rows(launch.span - launch.handle - position)
    };
    let kept_drawer_rows = drawer_rows(kept);
    let clamped_drawer_rows = drawer_rows(clamped);
    assert!(
        (kept_drawer_rows - clamped_drawer_rows).abs() >= MIN_CLAMP_ROW_GAP,
        "{smoke}: at the launch size the drawer has {kept_drawer_rows} rows at the dragged share \
         and {clamped_drawer_rows} at the share the {SHRUNK_OUTPUT_H} px shrink's clamp would \
         leave; want at least {MIN_CLAMP_ROW_GAP} apart, or A4 cannot tell a kept share from a \
         clamp taken for a drag. Shrink further or drag closer to the floor."
    );

    ResizePremises {
        target,
        floor_rows: shrunk.rows(shrunk.floor),
        kept_drawer_rows,
        clamped_drawer_rows,
    }
}

/// The share a drag leaves in the drawer divider survives a window shrink that clamps the
/// editor at its floor and a grow back: the shrink's clamp is not taken for a drag.
///
/// ## The scenario
///
/// One launch through [`pointer_command`], with [`pin_terminal_smoke_shell`]'s shell. The
/// drawer opens, the handle is dragged from where [`drawer_strip_edges`] puts it to
/// [`RESIZE_DRAG_ABOVE_FLOOR_PX`] above the editor's floor, and the window is shrunk to
/// [`SHRUNK_OUTPUT_H`] and grown back by [`RESIZE_OUTPUT_FRAGMENT`], which resizes the
/// output because cage maximizes its toplevel to it. Every observable is a Neovim readback;
/// the shrunk capture is read for its size alone.
///
/// The premises are derived before launch through `drawer_paned_at`, `DrawerShare::drag_to`
/// and `DrawerShare::position`: the drag lands unclamped, the shrink clamps the dragged
/// share's position to the editor's floor, and the share that clamp would leave gives the
/// drawer at least [`MIN_CLAMP_ROW_GAP`] rows fewer than the dragged one at the launch size.
///
/// The outer-grid reference for a readback with the drawer open is G, not L: L's `&lines` is
/// read with the drawer closed, and need not match a readback with the drawer open at the
/// same window size.
///
/// ## The legs
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | A1 | precondition: the output's extents are the launch size; L has one window, O two with the last tagged | none | n/a |
/// | A2 | precondition: G's drawer is taller than O's and its editor shorter | none | n/a |
/// | A3 | precondition: the capture is the shrunk size; S's `&lines` is below G's, its editor at the floor's rows, its drawer below G's, and it has two windows | none | n/a |
/// | C1 | A4: B's `&columns` and `&lines` are G's, and its editor and drawer `winheight` are G's | in `DividerGuard::drag_span`, delete `if position == self.last_position.get() { return None; }` | A4: B's drawer at the clamp's share |
/// | C2 | A4 | in `reapply_drawer_divider`, before `guard.apply(…)`, add `if guard.last_span.get() > 0 { return; }` | A4: B's drawer at GTK's proportional rescale of the clamped position |
///
/// C2 removes every re-apply of the share after a paned's first, the `max-position`
/// notify's and the wake's: deleting the `max-position` re-apply alone leaves the wake's
/// `apply_drawer_frame` to re-apply the share on a resize.
///
/// ## Order
///
/// The legs are asserted in the order above, each once its readbacks exist. A leg whose
/// readback is missing ends the run on the scenario's exit code.
#[test]
fn a_dragged_share_survives_a_window_shrink_and_grow_under_cage() {
    const SMOKE: &str = "a_dragged_share_survives_a_window_shrink_and_grow_under_cage";

    let premises = resize_premises(SMOKE);

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();

    let read = |name: &str| runtime_dir.join(format!("wolf_gnome_drawer_resize_{name}.txt"));
    let read_l = read("launch");
    let read_o = read("open");
    let read_g = read("dragged");
    let read_s = read("shrunk");
    let read_b = read("grown");
    let out_png = runtime_dir.join("wolf_gnome_drawer_resize_smoke.png");
    let shrunk_png = runtime_dir.join("wolf_gnome_drawer_resize_shrunk.png");
    for path in [
        &read_l,
        &read_o,
        &read_g,
        &read_s,
        &read_b,
        &out_png,
        &shrunk_png,
    ] {
        clear_evidence(path, SMOKE);
    }

    let handle_x = i64::from(PROBE_X);
    let handle_y = pixel_target(drawer_strip_edges().handle_y, "the drawer handle's y");
    let target_y = pixel_target(
        OVERLAY_ORIGIN_Y_EXPECTED + f64::from(premises.target),
        "the drag's release y",
    );

    let body = format!("{DRAWER_DIVIDER_HELPERS}\n{RESIZE_OUTPUT_FRAGMENT}\n{DRAWER_RESIZE_BODY}");
    let mut cmd = pointer_command(&body, &cwd, &runtime_dir, &out_png);
    pin_terminal_smoke_shell(&mut cmd)
        .env("HANDLE_X", handle_x.to_string())
        .env("HANDLE_Y", handle_y.to_string())
        .env("TARGET_Y", target_y.to_string())
        .env("LAUNCH_W", CAGE_OUTPUT_W_EXPECTED.to_string())
        .env("LAUNCH_H", CAGE_OUTPUT_H_EXPECTED.to_string())
        .env("SHRUNK_H", SHRUNK_OUTPUT_H.to_string())
        .env("SHRUNK_PNG", &shrunk_png)
        .env("READ_L", &read_l)
        .env("READ_O", &read_o)
        .env("READ_G", &read_g)
        .env("READ_S", &read_s)
        .env("READ_B", &read_b);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless drawer-resize scenario");

    // The harness's own failures first: after one, no readback is evidence of anything.
    if !status
        .code()
        .is_some_and(|code| RESIZE_OWN_EXITS.contains(&code))
    {
        assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    }

    let leg = |paths: &[&Path], name: &str| require_resize_leg(SMOKE, status, paths, name);

    // --- A1 (precondition): the launch size and the windows ----------------------
    let extents = read_output_extents(&cwd, SMOKE);
    assert_eq!(
        extents,
        (CAGE_OUTPUT_W_EXPECTED, CAGE_OUTPUT_H_EXPECTED),
        "{SMOKE}: the helper reports output extents {extents:?} at launch, want \
         ({CAGE_OUTPUT_W_EXPECTED}, {CAGE_OUTPUT_H_EXPECTED}). The handle's y and every derived \
         premise are for that size, so the press may have missed and S and B resize from \
         somewhere else."
    );
    for (label, y) in [("the handle", handle_y), ("the drag's release", target_y)] {
        assert_inside_extents(SMOKE, label, (handle_x, y), extents);
    }
    leg(&[read_l.as_path(), read_o.as_path()], "A1");
    let l = read_sizes(&read_l, SMOKE);
    let o = read_sizes(&read_o, SMOKE);
    assert_eq!(
        l.count, 1,
        "{SMOKE}: Neovim reports {} windows at launch, want 1. With one window, winheight(1) \
         and winheight(winnr('$')) are the editor. A second window at launch means a split or a \
         drawer came from elsewhere, and O's two-window check no longer shows that the chord \
         opened the drawer.",
        l.count
    );
    assert_eq!(
        (o.tagged, o.count),
        (1, 2),
        "{SMOKE}: with the drawer open Neovim reports (w:wolf_drawer of the last window, window \
         count) = ({}, {}), want (1, 2). A count of 1 means the chord opened no drawer; a last \
         window without w:wolf_drawer means winheight(winnr('$')) measures some other window.",
        o.tagged,
        o.count
    );

    // --- A2 (precondition): the drag moved the divider --------------------------------
    leg(&[read_g.as_path()], "A2");
    let g = read_sizes(&read_g, SMOKE);
    assert!(
        g.drawer > o.drawer && g.editor < o.editor && g.count == 2,
        "{SMOKE}: after dragging the handle from y {handle_y} to y {target_y}, Neovim reports \
         (editor, drawer) winheight ({}, {}) in {} windows, against ({}, {}) before the drag; \
         want the drawer taller and the editor shorter, in 2 windows. Read this FIRST as the \
         press missing the handle: it was pressed at ({handle_x}, {handle_y}), where \
         `drawer_strip_edges` puts a new window's handle, and the retained capture {} shows \
         where it is. With no drag the share is a new window's, and A4 says nothing about a \
         dragged one.",
        g.editor,
        g.drawer,
        g.count,
        o.editor,
        o.drawer,
        out_png.display()
    );

    // --- A3 (precondition): the shrink reached the toplevel and clamped ---------------
    leg(&[read_s.as_path()], "A3");
    let shrunk = png_size(&shrunk_png, SMOKE);
    assert_eq!(
        shrunk,
        (CAGE_OUTPUT_W_EXPECTED, SHRUNK_OUTPUT_H),
        "{SMOKE}: the capture after the shrink {} is {shrunk:?}, want \
         ({CAGE_OUTPUT_W_EXPECTED}, {SHRUNK_OUTPUT_H}). `resize_output` saw the mode current, \
         so another size means grim captured an output other than the one resized.",
        shrunk_png.display()
    );
    let s = read_sizes(&read_s, SMOKE);
    assert!(
        s.outer.1 < g.outer.1 && s.count == 2,
        "{SMOKE}: after the shrink to {SHRUNK_OUTPUT_H} px Neovim reports &lines {} in {} \
         windows, against {} after the drag; want fewer lines, in 2 windows. Equal lines mean the \
         output changed size and the toplevel did not follow it: cage did not reconfigure the \
         view, or the front-end did not resize Neovim's outer grid.",
        s.outer.1,
        s.count,
        g.outer.1
    );
    assert!(
        s.editor == premises.floor_rows && s.drawer < g.drawer,
        "{SMOKE}: after the shrink Neovim reports (editor, drawer) winheight ({}, {}), against \
         ({}, {}) after the drag; want the editor at the {} rows of its floor and the drawer \
         shorter. An editor above its floor means the derived clamp did not happen, so A4 \
         cannot see the guard; a drawer as tall as G's means the paned was never resized.",
        s.editor,
        s.drawer,
        g.editor,
        g.drawer,
        premises.floor_rows
    );

    // --- A4 (C1, C2): the grow back returns the dragged share ------------------------
    leg(&[read_b.as_path()], "A4");
    let b = read_sizes(&read_b, SMOKE);
    assert_eq!(
        b.outer, g.outer,
        "{SMOKE}: after the grow back Neovim reports (&columns, &lines) {:?}, against {:?} in \
         G, the drag's readback at the launch size with the drawer open; want them equal. The \
         toplevel did not return to the launch size, so its heights below cannot be compared \
         with G's.",
        b.outer, g.outer
    );
    assert_eq!(
        (b.editor, b.drawer),
        (g.editor, g.drawer),
        "{SMOKE}: after the shrink and the grow back Neovim reports (editor, drawer) winheight \
         {:?}, against {:?} after the drag. The derived drawer is {} rows at the dragged share \
         and {} at the share the shrink's clamp leaves. A drawer at the clamp's rows means the \
         clamp was written into the share as a drag: `DividerGuard::drag_span` accepted GTK's \
         `notify::position` after the `max-position` re-apply re-armed it. The same rows also \
         follow when nothing re-applies the share on the grow and GTK rescales the clamped \
         position. Any other height means the share was re-applied against another span.",
        (b.editor, b.drawer),
        (g.editor, g.drawer),
        premises.kept_drawer_rows,
        premises.clamped_drawer_rows
    );
}

/// The `(width, height)` of the PNG at `path`, decoded through cairo as `decode_capture`
/// decodes one, which pins the launch size and so cannot read a resized capture.
fn png_size(path: &Path, smoke: &str) -> (i64, i64) {
    let mut file = std::fs::File::open(path).unwrap_or_else(|e| {
        panic!(
            "{smoke}: could not open the capture {}: {e}. The scenario runs `grim` under `set \
             -eu`, so a readback after it with no file here means grim wrote somewhere else.",
            path.display()
        )
    });
    let surface = ImageSurface::create_from_png(&mut file).unwrap_or_else(|e| {
        panic!(
            "{smoke}: cairo could not decode the capture {} as a PNG: {e}.",
            path.display()
        )
    });
    (i64::from(surface.width()), i64::from(surface.height()))
}

/// [`require_leg`] for [`DRAWER_RESIZE_BODY`]'s exit codes.
fn require_resize_leg(smoke: &str, status: ExitStatus, paths: &[&Path], leg: &str) {
    let missing: Vec<String> = paths
        .iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "{smoke}: leg {leg}'s readbacks {missing:?} do not exist, and the scenario exited \
         {status}. Every readback is a Neovim one, so a missing one means the keys went \
         elsewhere, such as into the drawer's shell. 95 L, 96 O (`<C-w>k` out of the drawer \
         reached no editor), 97 G (the drag left keyboard focus off the editor), 98 S, 99 B, \
         100 (`wlr-randr` listed no output or refused the mode; the cage log has its listing), \
         101 (the output never reported the mode as current). An exit 0 with a file missing \
         means it was written somewhere else."
    );
}

/// The inclusive band of exit codes [`TAB_SHARES_BODY`] adds to [`POINTER_PREAMBLE`]'s 11-15
/// and 40.
const TAB_SHARES_EXITS: std::ops::RangeInclusive<i32> = 140..=149;

/// How far the saved share of a tab whose drawer was never dragged may be from a third.
const UNDRAGGED_SHARE_EPS: f64 = 0.01;

/// How much larger the dragged tab's saved share must be than the other's. The drag of
/// [`DRAWER_DRAG_PX`] moves it by about a quarter at the two-tab pane area.
const MIN_SHARE_GAP: f64 = 0.05;

/// Body of [`each_tab_keeps_its_drawer_share_across_a_relaunch_under_cage`], appended to
/// [`POINTER_PREAMBLE`], [`EXTRAS_HELPERS`] and [`DRAWER_DIVIDER_HELPERS`], whose `chord_w`
/// is the same as the first's.
///
/// Every readback is the focused drawer shell's `stty size`. A readback typed where no shell
/// has focus never appears, and its exit code names the leg.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | readback | meaning |
/// | --- | --- | --- |
/// | `140` | O1 | no shell answered after Ctrl+` in tab 1 |
/// | `141` | D1 | no shell answered after the drag: focus left tab 1's drawer |
/// | `142` | O0 | no shell answered after the click on tab 0 and Ctrl+` |
/// | `143` | D1' | no shell answered after the click back on tab 1 |
/// | `144` | — | run 1 was still alive after Ctrl+Shift+Q |
/// | `145` | — | run 1 exited non-zero |
/// | `146` | — | no `wolf.sqlite` to freeze after the quit |
/// | `147` | — | run 2 died before it was ready |
/// | `148` | R1 | no shell answered in run 2's selected tab: its drawer did not reopen focused |
/// | `149` | R0 | no shell answered after the click on tab 0 in run 2 |
const TAB_SHARES_BODY: &str = r#"
# $1 is the readback file, $2 the exit code. Only a drawer's shell writes it.
shell_size() {
    wtype -- "stty size > '$1'"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# --- Two tabs first, so both drawers open over the two-tab pane area ---------------------
wtype -M ctrl -M shift -k t -m shift -m ctrl
sleep "$TABS_BAR_SETTLE"
# New Tab's tab is empty, at HOME, and saves no drawer: the :cd makes it a folder tab.
nvim_ex "cd $TAB_CD_TARGET"

# --- O1, D1: tab 1's drawer at a new share, then dragged up -------------------------------
toggle_drawer
shell_size "$READ_O1" 140
drag_handle "$HANDLE_Y" "$DRAGGED_Y"
shell_size "$READ_D1" 141

# --- O0: tab 0's drawer -------------------------------------------------------------------
click_tab "$TAB0_X" "$TAB_Y"
toggle_drawer
shell_size "$READ_O0" 142

# --- D1': back in tab 1, whose drawer still has focus -------------------------------------
click_tab "$TAB1_X" "$TAB_Y"
shell_size "$READ_D1B" 143

wtype -M ctrl -M shift -k q -m shift -m ctrl
reap_app 144 145
freeze_or_exit "$STATE_Q" 146

# --- RUN 2: tab 1 selected, then tab 0 ----------------------------------------------------
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 147 "wolf-gnome tab drawer share smoke, run 2"
sleep "$RESTORE_SETTLE"
shell_size "$READ_R1" 148
click_tab "$TAB0_X" "$TAB_Y"
shell_size "$READ_R0" 149

hsend quit
exit 0
"#;

/// **Each tab of a window keeps its own drawer share: a drag in one tab leaves the other's
/// drawer where it was, a tab switch shows each tab's own, and a relaunch restores both.**
///
/// ## The scenario
///
/// Two launches in one cage, the first through [`POINTER_PREAMBLE`]. Ctrl+Shift+T comes first,
/// so both tabs have the two-tab pane area, and selects tab 1, an empty tab, which saves no
/// drawer: a `:cd` into the project makes it a folder tab. There Ctrl+` opens the drawer, read
/// as O1, and the handle is dragged [`DRAWER_DRAG_PX`] up, read as D1. A click on tab 0
/// and Ctrl+` open its drawer, read as O0, and a click back on tab 1 reads D1' in its drawer.
/// Ctrl+Shift+Q quits and Q is frozen. Run 2 starts on tab 1 and reads R1 in its restored
/// drawer, then a click on tab 0 reads R0.
///
/// The handle is pressed where [`drawer_strip_edges`]'s derivation puts a new share's at the
/// two-tab pane area: [`drawer_paned_at`] of an output [`TAB_BAR_HEIGHT_EXPECTED`] shorter,
/// offset down by the tab bar.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: D1's rows exceed O1's by at least [`MIN_DRAG_ROWS`] | none | n/a |
/// | E21a | O0 equals O1, rows and columns | every tab's pane view shares one `Rc<Cell<DrawerShare>>` held by `TabsInner` | O0 equals D1 |
/// | — | D1' equals D1 | none | n/a |
/// | E21c | Q: both tabs' drawers saved open | the gather writes `open: false` | Q: both closed |
/// | E21b | Q: tab 0's share within [`UNDRAGGED_SHARE_EPS`] of a third, and tab 1's at least [`MIN_SHARE_GAP`] above it | the gather writes `share: DRAWER_WEIGHT` | Q: tab 1's share is a third |
/// | E21e | run 2's readbacks appear | the restore calls no `toggle_terminal` | exit 148 |
/// | E21d | R1 equals D1, and R0 equals O0 | `plan_for_window` sets `DrawerShare::new()` | R1 below D1 |
#[test]
fn each_tab_keeps_its_drawer_share_across_a_relaunch_under_cage() {
    const SMOKE: &str = "each_tab_keeps_its_drawer_share_across_a_relaunch_under_cage";

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();

    let read = |name: &str| runtime_dir.join(format!("wolf_gnome_tab_shares_{name}.txt"));
    let read_o1 = read("o1");
    let read_d1 = read("d1");
    let read_o0 = read("o0");
    let read_d1b = read("d1b");
    let read_r1 = read("r1");
    let read_r0 = read("r0");
    let out_png = runtime_dir.join("wolf_gnome_tab_shares_smoke.png");
    for path in [
        &read_o1, &read_d1, &read_o0, &read_d1b, &read_r1, &read_r0, &out_png,
    ] {
        clear_evidence(path, SMOKE);
    }
    let state_q = frozen_state_dir(&runtime_dir, "wolf_gnome_tab_shares_q_state", SMOKE);

    let output_h = exact_f64(CAGE_OUTPUT_H_EXPECTED, "CAGE_OUTPUT_H_EXPECTED");
    let paned = drawer_paned_at(output_h - TAB_BAR_HEIGHT_EXPECTED);
    let position =
        wolf_gnome::drawer::DrawerShare::new().position(paned.floor, paned.max, paned.span);
    let handle_x = i64::from(PROBE_X);
    let handle_y = pixel_target(
        OVERLAY_ORIGIN_Y_EXPECTED + TAB_BAR_HEIGHT_EXPECTED + f64::from(position),
        "the two-tab drawer handle's y",
    );
    let dragged_y = handle_y - DRAWER_DRAG_PX;
    let (tab0_x, tab1_x, tab_y) = (two_tab_x(0), two_tab_x(1), tab_bar_y());

    let body = format!("{EXTRAS_HELPERS}\n{DRAWER_DIVIDER_HELPERS}\n{TAB_SHARES_BODY}");
    let mut cmd = pointer_command(&body, &cwd, &runtime_dir, &out_png);
    pin_terminal_smoke_shell(&mut cmd)
        .env(
            "WOLF_CONFIG_DIR",
            titlebar_pinned_config_dir(&cwd, GTK_TITLEBAR_NATIVE),
        )
        .env("HANDLE_X", handle_x.to_string())
        .env("HANDLE_Y", handle_y.to_string())
        .env("DRAGGED_Y", dragged_y.to_string())
        .env("TAB0_X", tab0_x.to_string())
        .env("TAB1_X", tab1_x.to_string())
        .env("TAB_Y", tab_y.to_string())
        .env("STATE_SRC", smoke_state_dir(&cwd))
        .env("STATE_Q", &state_q)
        .env("READ_O1", &read_o1)
        .env("READ_D1", &read_d1)
        .env("READ_O0", &read_o0)
        .env("READ_D1B", &read_d1b)
        .env("READ_R1", &read_r1)
        .env("READ_R0", &read_r0)
        .env("TAB_CD_TARGET", TEMP_CWD_BASENAME);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless tab drawer share scenario");
    if !status
        .code()
        .is_some_and(|code| TAB_SHARES_EXITS.contains(&code))
    {
        assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    }
    let extents = read_output_extents(&cwd, SMOKE);
    for (label, point) in [
        ("the two-tab handle at a new share", (handle_x, handle_y)),
        ("the handle after the drag", (handle_x, dragged_y)),
        ("tab 0's centre", (tab0_x, tab_y)),
        ("tab 1's centre", (tab1_x, tab_y)),
    ] {
        assert_inside_extents(SMOKE, label, point, extents);
    }

    let leg = |paths: &[&PathBuf], name: &str| {
        let missing: Vec<String> = paths
            .iter()
            .filter(|path| !path.exists())
            .map(|path| path.display().to_string())
            .collect();
        assert!(
            missing.is_empty(),
            "{SMOKE}: leg {name}'s evidence {missing:?} does not exist, and the scenario exited \
             {status}. 140 O1 (Ctrl+` opened no focused drawer in tab 1), 141 D1 (the drag took \
             focus off the drawer), 142 O0, 143 D1' (tab 1's drawer lost focus over the switch), \
             144 or 145 the quit, 146 no wolf.sqlite, 147 run 2 died, 148 R1 (run 2's selected \
             tab reopened no focused drawer), 149 R0."
        );
    };

    // --- Precondition: the drag moved tab 1's divider ---------------------------------------
    leg(&[&read_o1, &read_d1], "D1");
    let o1 = read_ints(&read_o1, 2, SMOKE);
    let d1 = read_ints(&read_d1, 2, SMOKE);
    assert!(
        d1[0] >= o1[0] + MIN_DRAG_ROWS,
        "{SMOKE}: tab 1's drawer reports {} rows after the drag and {} before; want at least \
         {MIN_DRAG_ROWS} more. Read this first as the press missing the handle: it was pressed \
         at ({handle_x}, {handle_y}), where a new share's handle sits below the tab bar, and the \
         retained capture {} shows where it is. Nothing below can tell a share kept per tab from \
         one never dragged.",
        d1[0],
        o1[0],
        out_png.display()
    );

    // --- E21a: tab 0's drawer opened at its own share ---------------------------------------
    leg(&[&read_o0], "O0");
    let o0 = read_ints(&read_o0, 2, SMOKE);
    assert_eq!(
        o0, o1,
        "{SMOKE}: tab 0's drawer opened at {o0:?} (rows, columns), against {o1:?} for tab 1's \
         before its drag and {d1:?} after it. Equal to D1 means tab 1's drag moved tab 0's \
         drawer too: the tabs share one drawer share."
    );

    // --- D1': the switch back shows tab 1's own share ----------------------------------------
    leg(&[&read_d1b], "D1'");
    let d1b = read_ints(&read_d1b, 2, SMOKE);
    assert_eq!(
        d1b, d1,
        "{SMOKE}: back in tab 1 its drawer reports {d1b:?}, against {d1:?} before the switch. \
         O0's {o0:?} means opening tab 0's drawer reset tab 1's share, or the switch applied \
         another tab's share to it."
    );

    // --- E21c and E21b: the save holds each tab's drawer ------------------------------------
    leg(&[&state_q], "Q");
    let saved = frozen_window_state(&state_q, SMOKE, "Q, frozen after the quit");
    let drawers: Vec<Option<wolf_core::app_state::Drawer>> = saved
        .windows
        .first()
        .map(|window| window.tabs.iter().map(|tab| tab.content.drawer).collect())
        .unwrap_or_default();
    assert!(
        drawers.len() == 2 && drawers.iter().all(|d| d.is_some_and(|d| d.open)),
        "{SMOKE}: Q saved the tabs' drawers as {drawers:?}; want two, both open. Closed means \
         the gather does not write an open drawer as open; None, that it writes no drawer. File: \
         {}",
        state_q.display()
    );
    let shares: Vec<f64> = drawers.iter().flatten().map(|d| d.share).collect();
    assert!(
        (shares[0] - 1.0 / 3.0).abs() <= UNDRAGGED_SHARE_EPS
            && shares[1] >= shares[0] + MIN_SHARE_GAP,
        "{SMOKE}: Q saved the shares {shares:?}; want tab 0's within {UNDRAGGED_SHARE_EPS} of a \
         third and tab 1's at least {MIN_SHARE_GAP} above it. Both a third means the gather \
         writes no tab's live share; both equal and larger, that one share is every tab's. File: \
         {}",
        state_q.display()
    );

    // --- E21e and E21d: run 2 restores each tab's drawer at its share -------------------------
    leg(&[&read_r1], "R1");
    let r1 = read_ints(&read_r1, 2, SMOKE);
    assert_eq!(
        r1, d1,
        "{SMOKE}: run 2's tab 1 drawer reports {r1:?}, against {d1:?} in run 1. O1's {o1:?} \
         means the saved share was not restored into the tab, so the drawer reopened at a new \
         share."
    );
    leg(&[&read_r0], "R0");
    let r0 = read_ints(&read_r0, 2, SMOKE);
    assert_eq!(
        r0, o0,
        "{SMOKE}: run 2's tab 0 drawer reports {r0:?}, against {o0:?} in run 1. D1's {d1:?} \
         means tab 1's restored share reached tab 0."
    );
}
