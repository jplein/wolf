//! The terminal drawer's divider smoke: a pointer drag on the handle between the editor tree
//! and the drawer resizes both, reaches the drawer's session, survives the drawer closing and
//! reopening, stops at both floors, and carries into a second tab of the same window.

use super::terminal::{drawer_strip_edges, pin_terminal_smoke_shell};
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
/// [`POINTER_PREAMBLE`].
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
# The drawer chord's round trip; see DRAWER_SCENARIO in terminal.rs.
DRAWER_SETTLE=2.0
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
nvim_tab "$READ_TAB" 80
toggle_drawer
shell_size "$READ_T" 81

hsend quit
exit 0
"#;

/// The drawer's divider is a draggable `gtk::Paned` handle whose position lives in the
/// window's drawer share: a drag resizes the editor and the drawer and the drawer's session,
/// the share survives the drawer closing and reopening and reaches a second tab, both floors
/// hold, and with the drawer closed the editor has the whole pane area back.
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
/// | C9 | A5: T rows > O rows [6 > 1] | in `pane_view::build`, `drawer_share: drawer_share.clone(),` → `drawer_share: Rc::new(Cell::new(DrawerShare::new())),` | T rows equal O's 1 |
///
/// C6a is the snap back on the next wake and C8 the snap back on a rebuild.
///
/// A5 holds only because the share is the window's: the second tab's pane area is
/// [`TAB_BAR_HEIGHT_EXPECTED`] shorter, so a new share there gives the drawer no more rows
/// than O. The share the limit drag left clamps to the editor's floor in that tab too.
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

    let mut cmd = pointer_command(DRAWER_DIVIDER_BODY, &cwd, &runtime_dir, &out_png);
    pin_terminal_smoke_shell(&mut cmd)
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
        .env("READ_T", &read_t);
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
         `notify::position` handler did not write the drag into the window's share, so the \
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
         the window's share, not from DRAWER_WEIGHT.",
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

    // --- A5 (C9): the share is the window's, not the tab's ----------------------
    leg(&[read_tab.as_path(), read_t.as_path()], "A5");
    let tab = read_ints(&read_tab, 2, SMOKE);
    assert_eq!(
        (tab[0], tab[1]),
        (2, 1),
        "{SMOKE}: after Ctrl+Shift+T Neovim reports (tabpagenr(), window count) = ({}, {}), want \
         (2, 1): a second tab holding its own editor window alone. Without it T measures the \
         first tab's drawer.",
        tab[0],
        tab[1]
    );
    let t = read_ints(&read_t, 2, SMOKE);
    assert!(
        t[0] > o[0],
        "{SMOKE}: the second tab's drawer opened with {} rows, no more than the {} a new \
         window's drawer has. The second tab's pane area is {TAB_BAR_HEIGHT_EXPECTED} px \
         shorter, so a default share there gives at most O's rows: the tab's pane view got a \
         share of its own rather than the window's.",
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
