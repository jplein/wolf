//! The Zoom Pane smokes. From the palette, one of three panes zooms over the open drawer and
//! fills the pane area while the others stay live at their last size, and an unzoom restores
//! every pane, both dragged dividers included. A focus move, by chord or by Neovim's own
//! `<C-w>w`, Toggle Terminal with the drawer open or closed, a split and an other-pane
//! command while zoomed each unzoom. A pane zoomed at quit is saved in `state.json` and
//! zoomed again at the next launch.

use super::pointer::MIN_DRAG_CELLS;
use super::preview::PREVIEW_OTHER_PANE_QUERY;
use super::terminal::{drawer_strip_edges, pin_terminal_smoke_shell};
use super::*;

/// Typed into the palette to reach the Zoom Pane row: a subsequence of its title and of no
/// other, which [`assert_zoom_query_is_unique`] checks before launch.
const ZOOM_QUERY: &str = "zoom";

/// The registry id of the command [`ZOOM_QUERY`] must select.
const ZOOM_PANE_ID: &str = "zoom_pane";

/// The Markdown file every pane shows, so the other-pane leg has a source to preview. Its
/// name carries no `z`, so no file row the palette might list can match [`ZOOM_QUERY`].
const ZOOM_SOURCE_FILE: &str = "layoutsrc.md";

/// The body of [`ZOOM_SOURCE_FILE`].
const ZOOM_SOURCE_BODY: &str = "# Layout source\n\nA paragraph to preview.\n";

/// Pixels the tree divider between A and the right column is dragged LEFT: about six and a
/// half cells at [`SMOKE_FONT_PT`], leaving A about 365 px wide, well above its floor.
const TREE_DRAG_PX: i64 = 150;

/// Pixels the drawer handle is dragged UP, as `drawer_divider.rs`'s first drag: the tree
/// goes from about 418 px to 268 px, clear of both floors.
const DRAWER_DRAG_PX: i64 = 150;

/// The fewest rows the drawer's window must gain from [`DRAWER_DRAG_PX`]: three whole 44 px
/// cells, less one for the sub-cell phase.
const MIN_DRAWER_DRAG_ROWS: i64 = 2;

/// The tree paned's handle width, in output pixels: the one-pixel separator Adwaita draws,
/// as for the drawer's handle. GTK's grab area reaches several pixels either side.
const TREE_HANDLE_PX: i32 = 1;

/// The fraction a two-leaf split's divider sits at before any drag: equal weights.
const NEW_SPLIT_FRACTION: f64 = 0.5;

/// The inclusive band of exit codes [`ZOOM_PANE_BODY`] adds to [`POINTER_PREAMBLE`]'s 11-15
/// and 40.
const OWN_EXITS: std::ops::RangeInclusive<i32> = 82..=94;

/// Body of [`zoom_pane_hides_restores_and_unzooms_under_cage`], appended to
/// [`POINTER_PREAMBLE`].
///
/// Every readback is a Neovim one, typed into an editor pane. A readback typed where Neovim
/// does not answer, into the drawer's shell or into a palette still open, never appears, and
/// its exit code names the leg.
///
/// `nvim_geom` writes ten integers: `winwidth` and `winheight` of `g:zwa`, `g:zwb`, `g:zwc`
/// and `g:zwd`, the window count, and `win_getid()`. Its third and fourth arguments append
/// to the format and the arguments. `nvim_focus` writes the window count, `win_getid()`,
/// `winwidth(0)` and `winheight(0)`.
///
/// ## Exit codes it adds to [`POINTER_PREAMBLE`]'s 11-15
///
/// | code | readback | meaning |
/// | --- | --- | --- |
/// | `82` | L | Neovim did not answer after `:e` at launch |
/// | `83` | I | Neovim did not answer after `<C-w>k` out of the drawer |
/// | `84` | Q | Neovim did not answer in C before the drags |
/// | `85` | P | Neovim did not answer after the drags and `<C-w>k`: a press left focus off the editor |
/// | `86` | Z | Neovim did not answer after the palette's `zoom`: no row matched and the palette kept the keys |
/// | `87` | H | Neovim did not answer after Ctrl+` closed the drawer while B was zoomed |
/// | `88` | F | Neovim did not answer after `<C-w>h` while zoomed |
/// | `89` | W | Neovim did not answer after `<C-w>w`: focus stayed on a pane that is not Neovim's, such as the drawer |
/// | `90` | U | Neovim did not answer after the unzoom |
/// | `91` | S | Neovim did not answer after the zoom and `<C-w>v` |
/// | `92` | C | Neovim did not answer after the zoom, `<C-w>c` and Ctrl+` |
/// | `93` | O | Neovim did not answer after the zoom and Preview in Another Pane |
/// | `94` | N | Neovim did not answer after Ctrl+` opened the drawer while B was zoomed and `<C-w>k` left it |
const ZOOM_PANE_BODY: &str = r#"
# The drawer chord's round trip; see DRAWER_SCENARIO in terminal.rs.
DRAWER_SETTLE=2.0
# Preview in Another Pane starts a WebKit view.
PREVIEW_SETTLE=3.0

GEOM_FMT='%d %d %d %d %d %d %d %d %d %d'
GEOM_ARGS='winwidth(g:zwa), winheight(g:zwa), winwidth(g:zwb), winheight(g:zwb), winwidth(g:zwc), winheight(g:zwc), winwidth(g:zwd), winheight(g:zwd), len(nvim_tabpage_list_wins(0)), win_getid()'

# $1 is the readback file, $2 the exit code; $3 and $4 extend the format and the arguments.
nvim_geom() {
    wtype -- ":call writefile([printf('$GEOM_FMT${3:-}', $GEOM_ARGS${4:-})], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# $1 is the readback file, $2 the exit code.
nvim_focus() {
    wtype -- ":call writefile([printf('%d %d %d %d', len(nvim_tabpage_list_wins(0)), win_getid(), winwidth(0), winheight(0))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

nvim_ex() {
    wtype -- ":$1"
    wtype -k Return
    sleep "$SETTLE"
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

# $1 is the query; Enter runs the palette's top row.
palette() {
    wtype -M ctrl -M shift -k p -m shift -m ctrl
    sleep "$SETTLE"
    wtype -- "$1"
    sleep "$SETTLE"
    wtype -k Return
    sleep "$SETTLE"
}

# $1 $2 is the press, on a handle, and $3 $4 the release. `hstep` for every event of the
# gesture -- see `hstep` in POINTER_PREAMBLE.
drag_handle() {
    hstep motion "$1" "$2"
    hstep press "$BTN_LEFT"
    hstep motion "$(( ($1 + $3) / 2 ))" "$(( ($2 + $4) / 2 ))"
    hstep motion "$3" "$4"
    hstep release "$BTN_LEFT"
    sleep "$SETTLE"
}

# --- L: the launch pane alone, on the source ------------------------------------
nvim_ex "e $ZOOM_SOURCE"
nvim_focus "$READ_L" 82

# --- I: vertical[A, horizontal[B, C]] over the drawer D, each id recorded ---------
nvim_ex "let g:zwa = win_getid()"
chord_w v
nvim_ex "let g:zwb = win_getid()"
chord_w s
nvim_ex "let g:zwc = win_getid()"
toggle_drawer
chord_w k
nvim_ex "let g:zwd = win_getid(winnr('\$'))"
wtype -- ":call writefile([printf('%d %d %d %d %d', g:zwa, g:zwb, g:zwc, g:zwd, getwinvar(g:zwd, 'wolf_drawer', 0))], '$READ_I')"
wtype -k Return
wait_file "$READ_I" || exit 83

# --- Q: before the drags, in C ----------------------------------------------------
nvim_geom "$READ_Q" 84

# --- P: the tree divider left, the drawer handle up, then <C-w>k into B ------------
drag_handle "$TREE_X" "$PROBE_Y" "$TREE_DRAGGED_X" "$PROBE_Y"
drag_handle "$PROBE_X" "$DRAWER_Y" "$PROBE_X" "$DRAWER_DRAGGED_Y"
# Post-mortem only: no assertion reads a pixel here.
grim "$OUT_PNG"
chord_w k
nvim_geom "$READ_P" 85

# --- Z: Zoom Pane on B --------------------------------------------------------------
palette "$ZOOM_QUERY"
nvim_geom "$READ_Z" 86

# --- U: Zoom Pane again unzooms -----------------------------------------------------
palette "$ZOOM_QUERY"
nvim_geom "$READ_U" 90

# --- F: zoom B, then a focus move left --------------------------------------------
palette "$ZOOM_QUERY"
chord_w h
nvim_geom "$READ_F" 88

# --- W: zoom B, then Neovim's own <C-w>w, to the window it numbers after B ----------
nvim_ex "call win_gotoid(g:zwb)"
nvim_ex "let g:zwnext = win_getid(winnr() % winnr('\$') + 1)"
palette "$ZOOM_QUERY"
chord_w w
nvim_geom "$READ_W" 89 ' %d' ', g:zwnext'

# --- H: zoom B, then Toggle Terminal with the drawer open and hidden ----------------
nvim_ex "call win_gotoid(g:zwb)"
palette "$ZOOM_QUERY"
toggle_drawer
nvim_geom "$READ_H" 87

# --- N: zoom B, then Toggle Terminal with no drawer; <C-w>k leaves the new drawer ---
palette "$ZOOM_QUERY"
toggle_drawer
chord_w k
nvim_ex "let g:zwd = win_getid(winnr('\$'))"
nvim_geom "$READ_N" 94 ' %d' ", getwinvar(g:zwd, 'wolf_drawer', 0)"

# --- S: zoom B, then <C-w>v --------------------------------------------------------
nvim_ex "call win_gotoid(g:zwb)"
palette "$ZOOM_QUERY"
chord_w v
nvim_geom "$READ_S" 91 ' %d %d' ', winwidth(0), winheight(0)'

# --- C: zoom the new pane, close it, then Toggle Terminal ----------------------------
palette "$ZOOM_QUERY"
chord_w c
toggle_drawer
nvim_focus "$READ_C" 92

# --- O: zoom the current pane, then Preview in Another Pane --------------------------
palette "$ZOOM_QUERY"
palette "$PREVIEW_OTHER_PANE_QUERY"
sleep "$PREVIEW_SETTLE"
nvim_focus "$READ_O" 93

hsend quit
exit 0
"#;

/// One `nvim_geom` readback: each recorded window's `(winwidth, winheight)`, the window
/// count and the current window's id, in the order [`ZOOM_PANE_BODY`] writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Geom {
    /// A, the launch pane, on the left.
    a: (i64, i64),
    /// B, the right column's top pane.
    b: (i64, i64),
    /// C, B's lower half.
    c: (i64, i64),
    /// D, the drawer.
    d: (i64, i64),
    /// `len(nvim_tabpage_list_wins(0))`.
    count: i64,
    /// `win_getid()`.
    current: i64,
}

/// Reads an `nvim_geom` readback of ten integers plus `extra`, returning the extra ones
/// after the [`Geom`].
fn read_geom(path: &Path, extra: usize, smoke: &str) -> (Geom, Vec<i64>) {
    let v = read_ints(path, 10 + extra, smoke);
    let geom = Geom {
        a: (v[0], v[1]),
        b: (v[2], v[3]),
        c: (v[4], v[5]),
        d: (v[6], v[7]),
        count: v[8],
        current: v[9],
    };
    (geom, v[10..].to_vec())
}

/// One `nvim_focus` readback.
#[derive(Clone, Copy, Debug)]
struct Focus {
    /// `len(nvim_tabpage_list_wins(0))`.
    count: i64,
    /// `win_getid()`.
    current: i64,
    /// `(winwidth(0), winheight(0))`.
    size: (i64, i64),
}

/// Reads an `nvim_focus` readback.
fn read_focus(path: &Path, smoke: &str) -> Focus {
    let v = read_ints(path, 4, smoke);
    Focus {
        count: v[0],
        current: v[1],
        size: (v[2], v[3]),
    }
}

/// Checks [`ZOOM_QUERY`] selects exactly the Zoom Pane title as a case-insensitive
/// subsequence, so which row Enter runs rests on no ranking.
fn assert_zoom_query_is_unique(smoke: &str) {
    let is_subsequence = |query: &str, title: &str| {
        let mut chars = title.chars().map(|c| c.to_ascii_lowercase());
        query.chars().all(|q| chars.any(|c| c == q))
    };
    let matched: Vec<(&str, &str)> = wolf_core::COMMAND_SPECS
        .iter()
        .filter(|spec| is_subsequence(ZOOM_QUERY, spec.title))
        .map(|spec| (spec.id, spec.title))
        .collect();
    assert!(
        matched.len() == 1 && matched[0].0 == ZOOM_PANE_ID,
        "{smoke}: the palette query {ZOOM_QUERY:?} must match exactly the {ZOOM_PANE_ID:?} \
         command's title as a case-insensitive subsequence; it matches {matched:?}. No match \
         means the command is not registered; more than one means Enter runs whichever ranks \
         first, so the smoke could run another command at every zoom step."
    );
}

/// Rounds a derived length to the whole pixels a `gtk::Paned` position is measured in.
fn paned_px(value: f64, what: &str) -> i32 {
    let px = pixel_target(value, what);
    i32::try_from(px).unwrap_or_else(|e| {
        panic!("{what} = {px} px does not fit a paned position ({e}); the derivation is nonsense")
    })
}

/// The tree divider's x in output pixels, for a `vertical[A, …]` split at equal weights:
/// the pane area's left edge plus the position `geometry::divider_position` gives the paned.
///
/// The paned spans the pane area's whole width, from [`OVERLAY_ORIGIN_X_EXPECTED`] to the
/// output's right edge, with the drawer open below it, and its children's floor is
/// `editor_inset::min_pane_size`'s, as in `drawer_strip_edges`. This models the handle as
/// that function models the drawer's, rather than bisecting for it as the pointer divider
/// smokes do: P's precondition reports a press that missed, and the retained capture shows
/// where the handle was.
fn tree_handle_x() -> f64 {
    let cell = cell_metrics_for_smoke();
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let statusbar = wolf_gnome::statusbar_layout::statusbar_layout(
        cell.size_px,
        wolf_gnome::statusbar_layout::secondary_font_size(cell.size_px),
        wolf_gnome::statusbar_layout::STATUSBAR_VERTICAL_PADDING,
        wolf_gnome::statusbar_layout::STATUSBAR_LINE_GAP,
    );
    let (min_w, _) = wolf_gnome::editor_inset::min_pane_size(
        cell.cell_w,
        cell.cell_h,
        statusbar.height,
        wolf_gnome::editor_inset::EDITOR_PADDING_PX,
    );
    let span = paned_px(
        output_w - OVERLAY_ORIGIN_X_EXPECTED,
        "the tree paned's span",
    );
    let floor = paned_px(min_w.ceil(), "the tree paned's child floor");
    let max = span - TREE_HANDLE_PX - floor;
    assert!(
        max > floor,
        "tree_handle_x: a {span} px pane area leaves the tree paned a position range of \
         [{floor}, {max}], so two {floor} px floors do not fit side by side. Re-derive the \
         span from CAGE_OUTPUT_W_EXPECTED and OVERLAY_ORIGIN_X_EXPECTED, or the pane minimum \
         grew past half the pane area."
    );
    let position = wolf_gnome::geometry::divider_position(floor, max, span, NEW_SPLIT_FRACTION);
    OVERLAY_ORIGIN_X_EXPECTED + f64::from(position)
}

/// **Zoom Pane from the palette zooms the active pane over the whole pane area, leaves the
/// hidden panes live at their last size, and unzooms to the exact layout, dragged dividers
/// included; a focus move, by chord or by Neovim's own `<C-w>w`, Toggle Terminal with the
/// drawer open or closed, a split and an other-pane command each unzoom.**
///
/// ## The scenario
///
/// One launch through [`pointer_command`], with [`pin_terminal_smoke_shell`]'s shell for the
/// drawer. Every observable is a readback the captive Neovim writes. Under `ext_multigrid`,
/// `winwidth` and `winheight` report the grid size the front-end drives, which is what a
/// zoomed, hidden or restored pane is; nothing is OCRed and no pixel is read.
///
/// Neovim's window numbering does not follow the screen, so the scenario records each
/// window's id as it builds `vertical[A, horizontal[B, C]]` over the drawer D: `:e`, then
/// `<C-w>v` (B, the right column), `<C-w>s` (C, below B), Ctrl+` (D), and `<C-w>k` back
/// into C. Each zoom after Z first puts Neovim on the pane it zooms, with `win_gotoid`
/// where that pane is not already current, since Zoom Pane zooms the active pane.
///
/// Two premises about Neovim carry the focus legs. `<C-w>h/j/k/l` is a pure layout intent
/// that Neovim declines to act on (`PaneInner::apply_focus`'s doc), so in F only the
/// front-end's own `focus_win` moves Neovim to A. `<C-w>w` is not intercepted, so in W
/// Neovim really moves, to the window it numbers after B. Its numbering follows its own
/// split order rather than the screen, so the scenario records that window as `g:zwnext`
/// before the zoom rather than assuming it, and W's precondition requires an editor pane.
///
/// The tree handle is pressed where [`tree_handle_x`] models it, at [`PROBE_Y`], and the
/// drawer handle where [`drawer_strip_edges`] puts it for a new window's share, at
/// [`PROBE_X`]. Both drags are 150 px, so a pane that kept its size across the zoom is told
/// apart from one re-derived at the default weight or share by several cells.
///
/// L is the launch pane alone, which has the whole pane area. Z-b holds because a zoomed
/// pane gets that same area, and H's A spans that same height once the drawer is gone.
///
/// ## The legs
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition L: one window | none | n/a |
/// | — | precondition I: four distinct ids, A the launch window, D tagged `w:wolf_drawer` | none | n/a |
/// | — | precondition Q: four windows, current C | none | n/a |
/// | — | precondition P: A ≥ [`MIN_DRAG_CELLS`] narrower and D ≥ [`MIN_DRAWER_DRAG_ROWS`] taller than in Q, current B, B not already L | none | n/a |
/// | Z2 | Z-a: four windows; A, C and D equal P | in the zoomed branch of `rebuild_pane_area`, also append every other bound pane's host to `pane_area` | Z-a |
/// | Z1a | Z-b: B equals L, current B | in `rebuild_pane_area`, treat the `shown_alone` answer as `None` | Z-b: B equals P |
/// | Z1b | Z-b | `ui_action_for` maps `Command::ZoomPane` back to `UiAction::Deferred(…)` | exit 86, or Z-b |
/// | U1 | U: A, B, C and D equal P | in `toggle_zoom`, after the state changes, `let _ = self.tree.borrow_mut().set_divider_fraction(&[], 0, 0.5);` | U: A's width back at Q's |
/// | U2 | U | in `toggle_zoom`, `self.drawer_share.set(DrawerShare::new());` | U: D's height back at Q's |
/// | F3 | F-a: current A | in `apply_focus`, `if self.zoomed.get().is_some() { return; }` at the top | F-a: current still B |
/// | F2 | F-b: four windows; A, B, C and D equal P | in `zoomed_after`, `ZoomEvent::FocusMove { .. } => zoomed` | F-b: B still at L's size |
/// | — | precondition W: `g:zwnext` is A or C | none | n/a |
/// | W2 | W: four windows, current `g:zwnext`; A, B, C and D equal P | in `update_active_pane`, delete the `FocusMove` application and its rebuild | W: B still at L's size |
/// | H1 | H: three windows, current B, A at P's width and L's height, B at P's width | in `zoomed_after`, `ZoomEvent::ToggleTerminal { .. } => zoomed` | H: B still at L's size |
/// | N1 | N: four windows, the last tagged `w:wolf_drawer`; A, B, C and D equal P | in `PaneInner::toggle_terminal`, return before `core.toggle_terminal()` while `self.zoomed.get().is_some() && self.drawer.get().id().is_none()` | N: three windows |
/// | S1 | S: five windows; B halved in width at P's height; A, C and D equal P; the new pane current | delete the `ZoomEvent::Split` call in `apply_split` | S: B at L's width, or current back on B |
/// | — | precondition C: three windows, the closed pane not current | none | n/a |
/// | O1 | O: four windows, current is C's, halved in width at C's height | in `PaneInner::other_pane_target`, delete the zoomed branch | O: three windows |
///
/// U1 is the falsifier that loses a dragged divider across zoom and unzoom. F2 is the one
/// that leaves the pane zoomed after a focus move.
///
/// U asserts equality, not a tolerance. The window never changes size, so every paned rebuilt
/// on unzoom has the span the drag was read against, and a fraction read back from a
/// position and scaled by the same span gives the same pixel.
///
/// N's D equals P's because the drawer's share is the window's and survives the drawer
/// closing, which `drawer_divider.rs` measures. N re-records D's id, since the reopened
/// drawer is a new window, and S reads that id.
///
/// C is a precondition only. Closing the zoomed pane moves Neovim's current window off it,
/// and `update_active_pane` reads that as a focus move, which unzooms. Deleting the
/// `ZoomEvent::PanesChanged` call in `detect_closes` therefore turns nothing here red;
/// `tests/pane_zoom.rs`'s K5 pins that rule.
///
/// ## Order
///
/// The legs are asserted in the order above, each once its readbacks exist. A leg whose
/// readback is missing ends the run on the scenario's exit code, so the earliest red is the
/// one reported.
#[test]
fn zoom_pane_hides_restores_and_unzooms_under_cage() {
    const SMOKE: &str = "zoom_pane_hides_restores_and_unzooms_under_cage";

    assert_zoom_query_is_unique(SMOKE);

    let runtime_dir = require_smoke_env();
    let cwd = make_temp_cwd();
    let source = cwd.join(ZOOM_SOURCE_FILE);
    std::fs::write(&source, ZOOM_SOURCE_BODY)
        .unwrap_or_else(|e| panic!("{SMOKE}: failed to write {}: {e}", source.display()));

    // Readbacks live under the runtime dir, as drawer_divider.rs's do.
    let read = |name: &str| runtime_dir.join(format!("wolf_gnome_zoom_pane_{name}.txt"));
    let read_l = read("launch");
    let read_i = read("ids");
    let read_q = read("before_drags");
    let read_p = read("after_drags");
    let read_z = read("zoomed");
    let read_u = read("unzoomed");
    let read_f = read("focus_move");
    let read_w = read("wincmd_w");
    let read_h = read("drawer_close");
    let read_n = read("drawer_open");
    let read_s = read("split");
    let read_c = read("close");
    let read_o = read("other_pane");
    let out_png = runtime_dir.join("wolf_gnome_zoom_pane_smoke.png");
    let evidence = [
        &read_l, &read_i, &read_q, &read_p, &read_z, &read_u, &read_f, &read_w, &read_h, &read_n,
        &read_s, &read_c, &read_o, &out_png,
    ];
    for path in evidence {
        clear_evidence(path, SMOKE);
    }

    let probe_y = i64::from(PROBE_Y);
    let tree_x = pixel_target(tree_handle_x(), "the tree handle's x");
    let tree_dragged_x = tree_x - TREE_DRAG_PX;
    let drawer_x = i64::from(PROBE_X);
    let drawer_y = pixel_target(drawer_strip_edges().handle_y, "the drawer handle's y");
    let drawer_dragged_y = drawer_y - DRAWER_DRAG_PX;

    let mut cmd = pointer_command(ZOOM_PANE_BODY, &cwd, &runtime_dir, &out_png);
    pin_terminal_smoke_shell(&mut cmd)
        .env("ZOOM_SOURCE", ZOOM_SOURCE_FILE)
        .env("ZOOM_QUERY", ZOOM_QUERY)
        .env("PREVIEW_OTHER_PANE_QUERY", PREVIEW_OTHER_PANE_QUERY)
        .env("TREE_X", tree_x.to_string())
        .env("TREE_DRAGGED_X", tree_dragged_x.to_string())
        .env("DRAWER_Y", drawer_y.to_string())
        .env("DRAWER_DRAGGED_Y", drawer_dragged_y.to_string())
        .env("READ_L", &read_l)
        .env("READ_I", &read_i)
        .env("READ_Q", &read_q)
        .env("READ_P", &read_p)
        .env("READ_Z", &read_z)
        .env("READ_U", &read_u)
        .env("READ_F", &read_f)
        .env("READ_W", &read_w)
        .env("READ_H", &read_h)
        .env("READ_N", &read_n)
        .env("READ_S", &read_s)
        .env("READ_C", &read_c)
        .env("READ_O", &read_o);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless zoom-pane scenario");

    // The harness's own failures first: after one, no readback is evidence of anything.
    if !status.code().is_some_and(|code| OWN_EXITS.contains(&code)) {
        assert_pointer_scenario_succeeded(SMOKE, status, &cwd, &out_png);
    }

    // Extents next: a clamped injection lands elsewhere and misdirects every message below.
    let extents = read_output_extents(&cwd, SMOKE);
    for (label, point) in [
        ("the tree handle", (tree_x, probe_y)),
        ("the tree drag's release", (tree_dragged_x, probe_y)),
        ("the drawer handle", (drawer_x, drawer_y)),
        ("the drawer drag's release", (drawer_x, drawer_dragged_y)),
    ] {
        assert_inside_extents(SMOKE, label, point, extents);
    }

    let leg = |paths: &[&Path], name: &str| require_leg(SMOKE, status, paths, name);

    // --- Preconditions L, I, Q: the layout the legs describe ---------------------
    leg(
        &[read_l.as_path(), read_i.as_path(), read_q.as_path()],
        "L-I-Q",
    );
    let l = read_focus(&read_l, SMOKE);
    assert_eq!(
        l.count, 1,
        "{SMOKE}: Neovim reports {} windows at launch, want 1. L's size is the pane with the \
         whole pane area only when it is the one window, and every zoomed leg compares against \
         it.",
        l.count
    );
    assert!(
        l.size.0 > 0 && l.size.1 > 0,
        "{SMOKE}: the launch pane reports a {}x{} grid, so the front-end never sized it and \
         every comparison against L would be against a placeholder.",
        l.size.0,
        l.size.1
    );
    let lsize = l.size;

    let ids = read_ints(&read_i, 5, SMOKE);
    let (id_a, id_b, id_c, id_d) = (ids[0], ids[1], ids[2], ids[3]);
    let mut distinct = vec![id_a, id_b, id_c, id_d];
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        distinct.len() == 4 && id_a == l.current && ids[4] == 1,
        "{SMOKE}: the recorded window ids are (A, B, C, D) = ({id_a}, {id_b}, {id_c}, {id_d}) \
         with w:wolf_drawer {} on D, and the launch window was {}. Want four distinct ids, A \
         the launch window and D's flag 1. A repeated id means a `<C-w>v`, `<C-w>s` or Ctrl+` \
         made no window or did not focus it; a D without the flag means the drawer is not the \
         last window or never opened.",
        ids[4],
        l.current
    );

    let (q, _) = read_geom(&read_q, 0, SMOKE);
    assert!(
        q.count == 4 && q.current == id_c,
        "{SMOKE}: before the drags Neovim reports {} windows with current {}, want 4 with C \
         ({id_c}) current. The drags and the `<C-w>k` into B start from C.",
        q.count,
        q.current
    );
    for (name, (w, h)) in [("A", q.a), ("B", q.b), ("C", q.c), ("D", q.d)] {
        assert!(
            w > 0 && h > 0,
            "{SMOKE}: {name} reports a {w}x{h} grid before the drags; a window the front-end \
             never sized makes every comparison below meaningless. Q: {q:?}"
        );
    }

    // --- Precondition P: both drags moved their divider --------------------------
    leg(&[read_p.as_path()], "P");
    let (p, _) = read_geom(&read_p, 0, SMOKE);
    assert!(
        p.a.0 <= q.a.0 - MIN_DRAG_CELLS,
        "{SMOKE}: after dragging the tree divider {TREE_DRAG_PX} px left, A is {} columns wide, \
         against {} before; want at least {MIN_DRAG_CELLS} fewer. Read this FIRST as the press \
         missing the handle: it was pressed at ({tree_x}, {probe_y}), where `tree_handle_x` \
         models a fresh split's divider, and the retained capture {} shows where it is. Without \
         a moved divider U cannot tell a restored layout from a re-derived one.",
        p.a.0,
        q.a.0,
        out_png.display()
    );
    assert!(
        p.d.1 >= q.d.1 + MIN_DRAWER_DRAG_ROWS,
        "{SMOKE}: after dragging the drawer handle {DRAWER_DRAG_PX} px up, D is {} rows tall, \
         against {} before; want at least {MIN_DRAWER_DRAG_ROWS} more. Read this FIRST as the \
         press missing the handle at ({drawer_x}, {drawer_y}), where `drawer_strip_edges` puts \
         a new window's handle; the retained capture {} shows where it is. Without a moved \
         share U cannot tell a restored drawer from a re-derived one.",
        p.d.1,
        q.d.1,
        out_png.display()
    );
    assert!(
        p.count == 4 && p.current == id_b && p.b != lsize && p.b.0 < lsize.0,
        "{SMOKE}: after the drags and `<C-w>k`, Neovim reports {} windows with current {} and B \
         at {:?}; want 4 windows, B ({id_b}) current, and B narrower than L's {lsize:?}. Another \
         current means a press focused a pane or `<C-w>k` did not reach B; B at L's size or \
         width means B already fills the pane area, and Z-b, F-b, W and H would hold without a \
         zoom or an unzoom.",
        p.count,
        p.current,
        p.b
    );

    // --- Z-a (Z2), Z-b (Z1a, Z1b): B fills the pane area, the rest live on hidden ---
    leg(&[read_z.as_path()], "Z");
    let (z, _) = read_geom(&read_z, 0, SMOKE);
    assert!(
        z.count == 4 && z.a == p.a && z.c == p.c && z.d == p.d,
        "{SMOKE}: zoomed on B, Neovim reports {} windows with (A, C, D) = ({:?}, {:?}, {:?}), \
         against ({:?}, {:?}, {:?}) before the zoom; want 4 windows and every hidden pane at its \
         last size. Fewer windows means the zoom closed a pane rather than hiding it. A changed \
         size means a hidden pane was still allocated, so the zoomed pane shares the pane area \
         with hosts that should be unparented.",
        z.count,
        z.a,
        z.c,
        z.d,
        p.a,
        p.c,
        p.d
    );
    assert!(
        z.b == lsize && z.current == id_b,
        "{SMOKE}: zoomed on B, B is {:?} with current {}; want L's {lsize:?}, the whole pane \
         area, and B ({id_b}) current. B at its unzoomed {:?} means nothing zoomed: the palette \
         has no Zoom Pane row (`ui_action_for` classifies it deferred), or the pane area was \
         rebuilt with the whole layout. Any other size means B shares the pane area.",
        z.b,
        z.current,
        p.b
    );

    // --- U (U1, U2): every pane restored; D first, as a reset share moves A's height too ---
    leg(&[read_u.as_path()], "U");
    let (u, _) = read_geom(&read_u, 0, SMOKE);
    assert_eq!(
        u.count, 4,
        "{SMOKE}: after the unzoom Neovim reports {} windows, want 4: a pane was closed across \
         the zoom.",
        u.count
    );
    assert_eq!(
        u.d, p.d,
        "{SMOKE}: after the unzoom the drawer is {:?}, against {:?} before the zoom and {:?} \
         before the handle drag. Back at its pre-drag height means the drawer's share was reset \
         across zoom and unzoom.",
        u.d, p.d, q.d
    );
    assert_eq!(
        u.a, p.a,
        "{SMOKE}: after the unzoom A is {:?}, against {:?} before the zoom and {:?} before the \
         tree drag. A back at its pre-drag width means the dragged tree divider was lost across \
         zoom and unzoom: the tree's weight was reset rather than left alone. Any other width \
         means the rebuilt paned took its position from something other than the tree.",
        u.a, p.a, q.a
    );
    assert!(
        u.b == p.b && u.c == p.c,
        "{SMOKE}: after the unzoom (B, C) is ({:?}, {:?}), against ({:?}, {:?}) before the zoom; \
         want each restored. B at L's {lsize:?} means the toggle did not unzoom; another size \
         means the right column's divider was not restored from the tree.",
        u.b,
        u.c,
        p.b,
        p.c
    );

    // --- F-a (F3), F-b (F2): a focus move while zoomed unzooms and lands on A -------
    leg(&[read_f.as_path()], "F");
    let (f, _) = read_geom(&read_f, 0, SMOKE);
    assert_eq!(
        f.current, id_a,
        "{SMOKE}: after zooming B and `<C-w>h`, current is {}; want A ({id_a}), B's left \
         neighbour in the unzoomed layout. B ({id_b}) means the move was refused while zoomed, \
         or resolved in the zoomed view, where B has no neighbour; any other window means the \
         move started from a pane other than the zoomed one.",
        f.current
    );
    assert!(
        f.count == 4 && f.a == p.a && f.b == p.b && f.c == p.c && f.d == p.d,
        "{SMOKE}: after zooming B and `<C-w>h`, Neovim reports {} windows with (A, B, C, D) = \
         ({:?}, {:?}, {:?}, {:?}); want 4 and P's ({:?}, {:?}, {:?}, {:?}), both dragged \
         dividers included. B at L's {lsize:?} means the focus move left B zoomed, so A has \
         focus while hidden; A or D at its pre-drag {:?} or {:?} means the unzoom lost a drag.",
        f.count,
        f.a,
        f.b,
        f.c,
        f.d,
        p.a,
        p.b,
        p.c,
        p.d,
        q.a,
        q.d
    );

    // --- W (W2): Neovim's own move to a hidden pane unzooms --------------------------
    leg(&[read_w.as_path()], "W");
    let (w, w_extra) = read_geom(&read_w, 1, SMOKE);
    let next = w_extra[0];
    assert!(
        next == id_a || next == id_c,
        "{SMOKE}: Neovim numbers {next} after B ({id_b}), want A ({id_a}) or C ({id_c}), an \
         editor pane B's zoom hides. The drawer ({id_d}) means `<C-w>w` would leave the keys in \
         its shell and W could read nothing; B itself means Neovim holds one window. Pick the \
         zoomed pane so that its next window is an editor pane."
    );
    assert!(
        w.count == 4 && w.current == next,
        "{SMOKE}: after zooming B and `<C-w>w`, Neovim reports {} windows with current {}; want \
         4 with {next}, the window Neovim numbers after B, current. B ({id_b}) means the \
         front-end sent Neovim back to the zoomed pane.",
        w.count,
        w.current
    );
    assert!(
        w.a == p.a && w.b == p.b && w.c == p.c && w.d == p.d,
        "{SMOKE}: after zooming B and `<C-w>w`, (A, B, C, D) is ({:?}, {:?}, {:?}, {:?}); want \
         P's ({:?}, {:?}, {:?}, {:?}). B at L's {lsize:?} means Neovim's own move to a hidden \
         pane left B zoomed: `update_active_pane` did not apply the focus move, and keys now \
         reach a pane nobody can see.",
        w.a,
        w.b,
        w.c,
        w.d,
        p.a,
        p.b,
        p.c,
        p.d
    );

    // --- H (H1): Toggle Terminal with the drawer open and hidden unzooms, then closes it ---
    leg(&[read_h.as_path()], "H");
    let (h, _) = read_geom(&read_h, 0, SMOKE);
    assert!(
        h.count == 3 && h.current == id_b,
        "{SMOKE}: after zooming B and Ctrl+` with the drawer open and hidden, Neovim reports {} \
         windows with current {}; want 3, the drawer closed, with B ({id_b}) current. Four \
         means Toggle Terminal did not close the drawer; another current means it moved focus.",
        h.count,
        h.current
    );
    assert!(
        h.b.0 == p.b.0 && h.a == (p.a.0, lsize.1),
        "{SMOKE}: after zooming B and Ctrl+`, B is {:?} and A {:?}; want B at P's width {} and A \
         at P's width {} and L's height {}, the tree over the whole pane area with the dragged \
         divider kept. B at L's {lsize:?} means Toggle Terminal closed the drawer and left B \
         zoomed; A at P's {:?} means the drawer's space was not given back to the tree.",
        h.b,
        h.a,
        p.b.0,
        p.a.0,
        lsize.1,
        p.a
    );

    // --- N (N1): Toggle Terminal with no drawer unzooms, then opens it ----------------
    leg(&[read_n.as_path()], "N");
    let (n, n_extra) = read_geom(&read_n, 1, SMOKE);
    assert!(
        n.count == 4 && n_extra[0] == 1,
        "{SMOKE}: after zooming B, Ctrl+` with no drawer and `<C-w>k`, Neovim reports {} \
         windows and its last window's w:wolf_drawer is {}; want 4 and 1, the drawer opened \
         below the tree. Three windows and 0 mean Toggle Terminal opened nothing while B was \
         zoomed; four and 0 mean the window that opened is not the drawer.",
        n.count,
        n_extra[0]
    );
    assert!(
        n.a == p.a && n.b == p.b && n.c == p.c && n.d == p.d,
        "{SMOKE}: after zooming B and opening the drawer, (A, B, C, D) is ({:?}, {:?}, {:?}, \
         {:?}); want P's ({:?}, {:?}, {:?}, {:?}). B at L's {lsize:?} means the drawer opened \
         behind the zoomed pane; D at its pre-drag {:?} means the window's drawer share was \
         reset.",
        n.a,
        n.b,
        n.c,
        n.d,
        p.a,
        p.b,
        p.c,
        p.d,
        q.d
    );

    // --- S (S1): a split while zoomed unzooms, then splits ------------------------
    leg(&[read_s.as_path()], "S");
    let (s, s_extra) = read_geom(&read_s, 2, SMOKE);
    let new_pane = (s_extra[0], s_extra[1]);
    assert!(
        s.count == 5 && ![id_a, id_b, id_c].contains(&s.current),
        "{SMOKE}: after zooming B and `<C-w>v`, Neovim reports {} windows with current {}; want \
         5, with the new window current rather than one of ({id_a}, {id_b}, {id_c}). Current \
         back on B means the view stayed zoomed on B and sent Neovim back to it.",
        s.count,
        s.current
    );
    assert!(
        s.b.1 == p.b.1 && 2 * s.b.0 <= p.b.0 + 1 && new_pane.1 == p.b.1,
        "{SMOKE}: after zooming B and `<C-w>v`, B is {:?} and the new pane {new_pane:?}, against \
         B's unzoomed {:?}; want both at B's height with B no more than half its width. B at \
         L's {lsize:?} means the split did not unzoom and the new pane is hidden behind B; a \
         new pane at L's size means the zoom moved to it.",
        s.b,
        p.b
    );
    assert!(
        s.a == p.a && s.c == p.c && s.d == p.d,
        "{SMOKE}: after the split (A, C, D) is ({:?}, {:?}, {:?}), against ({:?}, {:?}, {:?}) \
         unzoomed; want each unchanged, since only B was split. A change means the unzoom \
         before the split did not restore the layout.",
        s.a,
        s.c,
        s.d,
        p.a,
        p.c,
        p.d
    );

    // --- Precondition C: the zoomed pane closed, then the drawer -----------------
    leg(&[read_c.as_path()], "C");
    let c = read_focus(&read_c, SMOKE);
    assert!(
        c.count == 3 && c.current != s.current,
        "{SMOKE}: after zooming the new pane, `<C-w>c` and Ctrl+`, Neovim reports {} windows \
         with current {}; want 3, A, B and C, with the closed pane {} not current. Four windows \
         means the drawer stayed open; five means `<C-w>c` closed nothing. O starts from this \
         layout.",
        c.count,
        c.current,
        s.current
    );

    // --- O (O1): an other-pane command while zoomed splits ------------------------
    leg(&[read_o.as_path()], "O");
    let o = read_focus(&read_o, SMOKE);
    assert!(
        o.count == 4
            && o.current == c.current
            && 2 * o.size.0 <= c.size.0 + 1
            && o.size.1 == c.size.1,
        "{SMOKE}: after zooming the current pane and Preview in Another Pane, Neovim reports {} \
         windows with current {} at {:?}, against {} at {:?} before; want 4 windows, the same \
         current, at no more than half its width and the same height. Three windows means the \
         preview went into an existing, hidden pane instead of a split; the current at L's \
         {lsize:?} means the view stayed zoomed.",
        o.count,
        o.current,
        o.size,
        c.current,
        c.size
    );
}

/// Ends the run on the scenario's exit status when any of `paths` is missing, so the leg
/// that needs them is never asserted on a readback the scenario did not reach.
fn require_leg(smoke: &str, status: ExitStatus, paths: &[&Path], leg: &str) {
    let missing: Vec<String> = paths
        .iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "{smoke}: leg {leg}'s readbacks {missing:?} do not exist, and the scenario exited \
         {status}. Every readback is a Neovim one, so a missing one means the keys went \
         elsewhere: into the drawer's shell, or into a palette that matched no row. 82 L, 83 I \
         (`<C-w>k` out of the drawer reached no editor), 84 Q, 85 P (a press took keyboard \
         focus off the editor), 86 Z (the palette has no Zoom Pane row), 87 H, 88 F, 89 W \
         (Neovim was left on the drawer after `<C-w>w`), 90 U, 91 S, 92 C, 93 O, 94 N (`<C-w>k` \
         out of the drawer reached no editor). An exit 0 with a file missing means it was \
         written somewhere else."
    );
}

/// The file run 1 opens first, in the pane left unzoomed.
const RELAUNCH_LEFT_FILE: &str = "za.md";

/// The file run 1 opens in the pane it splits off and zooms.
const RELAUNCH_ZOOMED_FILE: &str = "zb.md";

/// The two-launch scenario of [`a_zoomed_pane_survives_relaunch_under_cage`], run as one
/// `sh -c` string inside one `cage`, as `restore.rs`'s `RESTORE_LAYOUT_SCENARIO` is.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | run 1's app died before it was ready for input |
/// | `12` | run 1 was still alive after `:qa` |
/// | `13` | run 1 exited non-zero |
/// | `14` | run 2's app died before it was ready for input |
/// | `15` | a run-2 readback never appeared: the keys reached no Neovim |
/// | `16` | run 1 wrote no `state.json` |
/// | `18` | run 1's L1 readback never appeared, after `:e` |
/// | `19` | run 1's Z1 readback never appeared, after `:WolfZoomPane` |
/// | `40` | an app never reported ready: `wait_ready`'s timeout in the readiness prelude |
const ZOOM_RELAUNCH_SCENARIO: &str = r#"
set -eu

WAIT_INTERVAL=0.2
SETTLE=1.0
QUIT_TRIES=75
FILE_TRIES=100

cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

wait_file() {
    f=$1
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        if [ -s "$f" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome zoom relaunch smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

nvim_ex() {
    wtype -- ":$1"
    wtype -k Return
    sleep "$SETTLE"
}

# $1 is the readback file, $2 the exit code.
nvim_size() {
    wtype -- ":call writefile([printf('%d %d', winwidth(0), winheight(0))], '$1')"
    wtype -k Return
    wait_file "$1" || exit "$2"
}

# --- RUN 1: za alone, then zb split off beside it and zoomed, then quit -------------
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome zoom relaunch smoke"

nvim_ex "e $RELAUNCH_LEFT_FILE"
nvim_size "$READ_L1" 18
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"
nvim_ex "e $RELAUNCH_ZOOMED_FILE"
nvim_ex "WolfZoomPane"
nvim_size "$READ_Z1" 19

wtype -- ":qa"
wtype -k Return

tries=0
gone=0
while [ "$tries" -lt "$QUIT_TRIES" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        gone=1
        break
    fi
    tries=$((tries + 1))
    sleep "$WAIT_INTERVAL"
done
if [ "$gone" -ne 1 ]; then
    echo "wolf-gnome zoom relaunch smoke: run 1 (pid $app_pid) still alive after :qa." >&2
    exit 12
fi
rc=0
wait "$app_pid" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "wolf-gnome zoom relaunch smoke: run 1 (pid $app_pid) exited $rc, not 0." >&2
    exit 13
fi
app_pid=

# Run 2 saves on every render wake, so run 1's save is frozen before it starts.
if [ ! -s "$STATE_SRC" ]; then
    echo "wolf-gnome zoom relaunch smoke: run 1 wrote no state.json at $STATE_SRC." >&2
    exit 16
fi
cp "$STATE_SRC" "$STATE_COPY"

# --- RUN 2: relaunch in the same cwd and state dir, and read back -------------------
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 14 "wolf-gnome zoom relaunch smoke"

wtype -- ":call writefile([printf('%d %d %d %d', len(nvim_tabpage_list_wins(0)), winwidth(0), winheight(0), winwidth(get(filter(nvim_tabpage_list_wins(0), {_, id -> id != win_getid()}), 0, -1)))], '$READ_R')"
wtype -k Return
wait_file "$READ_R" || exit 15
wtype -- ":call writefile([fnamemodify(bufname('%'), ':t')], '$READ_RB')"
wtype -k Return
wait_file "$READ_RB" || exit 15

exit 0
"#;

/// **A pane zoomed at quit is saved as its leaf's `zoomed` mark in `state.json`, and the next
/// launch restores the layout zoomed on it.**
///
/// ## The scenario
///
/// Two launches in one cage through [`cage_command`], sharing the cwd and the state dir.
/// Run 1 opens [`RELAUNCH_LEFT_FILE`], reads its size back as L1, splits with `<C-w>v`,
/// opens [`RELAUNCH_ZOOMED_FILE`] in the new right pane, zooms it with `:WolfZoomPane` and
/// reads its size back as Z1, then quits with `:qa`. The scenario copies run 1's `state.json`
/// before run 2 can write over it. Run 2 waits until it is ready, then reads back its window
/// count, the current window's size and buffer, and the other window's width.
///
/// L1 is the launch pane alone, which has the whole pane area, and a zoomed pane gets that
/// same area, so Z1 equals L1 only when the zoom happened. Every observable is a Neovim
/// readback or the saved file; no pixel is read, so a fixture's name cannot satisfy one.
///
/// ## The assertions
///
/// | ID | assertion | falsifying edit | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: Z1 equals L1, and both are a sized grid | none | n/a |
/// | R1 | R-a: run 1's copy holds one window and one tab, laid out `vertical[za unzoomed, zb zoomed]` | in `layout_snapshot`, pass `None` to `encode` | R-a: no leaf zoomed |
/// | R2 | R-b: run 2 has 2 windows, `zb.md` current at L1, the other window narrower than L1 | in `apply_layout_rebuild`, delete the `Restore` application | R-b: `zb.md` narrower than L1 |
/// | R3 | the scenario exits 0 | in `readiness`, drop the zoom filter | exit 40: run 2 never reports ready |
///
/// R-a is asserted before R-b, so a save that lost the mark is told apart from a restore
/// that ignored it.
#[test]
fn a_zoomed_pane_survives_relaunch_under_cage() {
    const SMOKE: &str = "a_zoomed_pane_survives_relaunch_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    for (name, body) in [
        (RELAUNCH_LEFT_FILE, "# Left pane\n"),
        (RELAUNCH_ZOOMED_FILE, "# Zoomed pane\n"),
    ] {
        let path = cwd.join(name);
        std::fs::write(&path, body)
            .unwrap_or_else(|e| panic!("{SMOKE}: failed to write {}: {e}", path.display()));
    }

    let smoke_state_file = smoke_state_dir(&cwd).join(STATE_FILE_NAME);
    // A directory, because `load_state` takes one; under the runtime dir so the sidebar never
    // lists it.
    let run1_state_dir = runtime_dir.join("wolf_gnome_zoom_relaunch_run1_state");
    std::fs::create_dir_all(&run1_state_dir).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: failed to create the run-1 state snapshot dir {}: {e}",
            run1_state_dir.display()
        )
    });
    let run1_state_file = run1_state_dir.join(STATE_FILE_NAME);

    let read = |name: &str| runtime_dir.join(format!("wolf_gnome_zoom_relaunch_{name}.txt"));
    let read_l1 = read("launch");
    let read_z1 = read("zoomed");
    let read_r = read("relaunched");
    let read_rb = read("relaunched_buffer");
    for path in [&read_l1, &read_z1, &read_r, &read_rb, &run1_state_file] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(ZOOM_RELAUNCH_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("RELAUNCH_LEFT_FILE", RELAUNCH_LEFT_FILE)
        .env("RELAUNCH_ZOOMED_FILE", RELAUNCH_ZOOMED_FILE)
        .env("READ_L1", &read_l1)
        .env("READ_Z1", &read_z1)
        .env("READ_R", &read_r)
        .env("READ_RB", &read_rb)
        .env("STATE_SRC", &smoke_state_file)
        .env("STATE_COPY", &run1_state_file)
        .status()
        .expect("failed to spawn `cage` for the headless zoom relaunch scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage zoom relaunch scenario exited unsuccessfully ({status}). Exit 11: run \
         1's app died before it was ready. Exit 12: run 1 was still alive after `:qa`. Exit 13: \
         run 1 exited non-zero, which implicates the close path's gather of a zoomed tab. Exit \
         14: run 2's app died before it was ready, as a panic in the restore's re-zoom would. \
         Exit 15: a run-2 readback never appeared, so the keys reached no Neovim: the focused \
         widget may be a hidden pane's. Exit 16: run 1 wrote no state.json at {}. Exit 18 or \
         19: a run-1 readback never appeared, after `:e` or after `:WolfZoomPane`. Exit 40: an \
         app never reported ready; in run 2 that is the readiness file waiting on the grid of \
         a pane the zoom hides, which is never allocated and so never requests one.",
        smoke_state_file.display()
    );

    // --- Precondition: run 1 zoomed zb ------------------------------------------
    let l1 = read_ints(&read_l1, 2, SMOKE);
    let z1 = read_ints(&read_z1, 2, SMOKE);
    let l1 = (l1[0], l1[1]);
    let z1 = (z1[0], z1[1]);
    assert!(
        l1.0 > 0 && l1.1 > 0 && z1 == l1,
        "{SMOKE}: in run 1 the launch pane is {l1:?} and the zoomed zb {z1:?}; want both a sized \
         grid and equal, since a zoomed pane gets the launch pane's whole area. Z1 about half \
         L1's width means `:WolfZoomPane` zoomed nothing, so neither assertion below says \
         anything about a saved zoom."
    );

    // --- R-a (R1): run 1 saved zb's leaf as the zoomed one ------------------------
    let saved = load_state(&run1_state_dir).unwrap_or_else(|| {
        panic!(
            "{SMOKE}: wolf_core::load_state could not parse the frozen copy of run 1's state at \
             {}. The scenario proved it non-empty, so suspect its shape.",
            run1_state_file.display()
        )
    });
    assert!(
        saved.windows.len() == 1 && saved.windows[0].tabs.len() == 1,
        "{SMOKE}: run 1's state.json holds {} window(s), the first with {:?} tab(s); want one \
         window with one tab. File: {}",
        saved.windows.len(),
        saved.windows.first().map(|w| w.tabs.len()),
        run1_state_file.display()
    );
    let layout = &saved.windows[0].tabs[0].layout;
    let Some(PaneNode::Split {
        orientation: PaneOrientation::Vertical,
        children,
        ..
    }) = layout
    else {
        panic!(
            "{SMOKE}: run 1 saved the layout {layout:?}; want a vertical split of two leaves, \
             for the `<C-w>v`. A lone leaf or None means the split was not saved at all. File: \
             {}",
            run1_state_file.display()
        );
    };
    let leaves: Vec<(Option<&str>, Option<bool>)> = children
        .iter()
        .map(|child| (leaf_tail(child), leaf_fields(child).map(|leaf| leaf.zoomed)))
        .collect();
    assert_eq!(
        leaves,
        vec![
            (Some(RELAUNCH_LEFT_FILE), Some(false)),
            (Some(RELAUNCH_ZOOMED_FILE), Some(true)),
        ],
        "{SMOKE}: run 1 saved the leaves (file, zoomed) {leaves:?}; want {RELAUNCH_LEFT_FILE} \
         unzoomed then {RELAUNCH_ZOOMED_FILE} zoomed. Both unzoomed means the save side wrote \
         no mark: `layout_snapshot` did not pass the zoomed pane to `encode`. Both zoomed \
         means `encode` marks every leaf of a zoomed tab; only the first zoomed means it marks \
         the wrong id. File: {}",
        run1_state_file.display()
    );

    // --- R-b (R2): run 2 starts zoomed on zb --------------------------------------
    let r = read_ints(&read_r, 4, SMOKE);
    let (count, current, other_w) = (r[0], (r[1], r[2]), r[3]);
    let buffer = read_trimmed(&read_rb, SMOKE);
    assert!(
        count == 2 && buffer == RELAUNCH_ZOOMED_FILE,
        "{SMOKE}: run 2 has {count} window(s) with {buffer:?} current; want 2 with \
         {RELAUNCH_ZOOMED_FILE} current. One window means the saved layout was not rebuilt; \
         {RELAUNCH_LEFT_FILE} current means the restore left Neovim on the first leaf rather \
         than the zoomed one."
    );
    assert!(
        current == l1 && other_w < l1.0,
        "{SMOKE}: in run 2 {RELAUNCH_ZOOMED_FILE} is {current:?} and the other window {other_w} \
         columns wide; want {RELAUNCH_ZOOMED_FILE} at run 1's L1 {l1:?}, the whole pane area, \
         and the hidden {RELAUNCH_LEFT_FILE} narrower. {RELAUNCH_ZOOMED_FILE} at about half \
         L1's width means the restore read the mark (R-a holds) but did not re-zoom: \
         `apply_layout_rebuild` applied no `Restore`. Both windows at L1's width means neither \
         was split."
    );
}
