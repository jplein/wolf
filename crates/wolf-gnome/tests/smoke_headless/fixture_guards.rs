//! The fixture-name guards: compositor-free tests that no fixture a smoke writes, and no
//! fixed component of the temp cwd, can satisfy an OCR term some smoke asserts on.
//!
//! Every smoke that OCRs a capture registers each term it asserts on in
//! [`ASSERTED_OCR_TERMS`], and a smoke that OCRs nothing registers none.

use super::compose::{
    COMPOSE_BEFORE_NAME, COMPOSE_FIXTURE_FILE, COMPOSE_READBACK_NAME, IMCHORD_AFTER_NAME,
    IMCHORD_BEFORE_NAME, IMCHORD_FIXTURE_FILE, IMCHORD_ONE_NAME, IMCHORD_SPLIT_NAME,
    IMCHORD_START_NAME,
};
use super::editor::{
    BUFPICK_FILE_CURRENT, BUFPICK_FILE_PRESELECT, BUFPICK_FILE_TARGET, BUFPICK_OCR_ABSENT,
    BUFPICK_OCR_CURRENT, BUFPICK_OCR_PRESENT, BUFPICK_READBACK_NAME, CONFIRM_OCR_OPTIONS,
    CONFIRM_OCR_QUESTION, EMPTY_STATE_OCR, MSG_OCR_FIRST, MSG_OCR_SECOND, NAVBACK_FILE_FIRST,
    NAVBACK_FILE_SECOND, NAVBACK_OCR_ABSENT, NAVBACK_OCR_PRESENT, NAVBACK_READBACK_AFTER,
    NAVBACK_READBACK_BEFORE, NEWBUF_OCR_ABSENT, NEWBUF_OCR_PRESENT, OCR_WORD, PALETTE_FILE,
    PALETTE_OCR_ABSENT, PALETTE_OCR_PRESENT, PICKER_NAV_OCR_ABSENT, PICKER_NAV_OCR_PRESENT,
    PICKER_OCR, PUM_OCR_ITEM, PUM_OCR_ITEM2, SAVEAS_READBACK_NAMELESS_NAME, SAVEAS_TYPED_NAME,
    SAVE_FIXTURE_FILE, SAVE_READBACK_BEFORE_NAME,
};
use super::panes::{
    CHORD_CLOSE_OCR_GONE, CHORD_CLOSE_OCR_KEEP, CLOSE_OCR_GONE, CLOSE_OCR_KEEP, FOCUS_OCR_LEFT,
    FOCUS_OCR_MARKER, FOCUS_OCR_RIGHT, HSPLIT_OCR_BOTTOM, HSPLIT_OCR_TOP, QUIT_OCR_GONE,
    QUIT_OCR_KEEP, STATUS_FILE_A, STATUS_FILE_B, STATUS_FIXTURE_BODY, STATUS_FIXTURE_DIR,
    STATUS_OCR_A, STATUS_OCR_B, STATUS_OCR_MARKER, STATUS_READBACK_NAME, TABS_FIRST_FILE,
    TABS_FIRST_HEADING, TABS_OCR_FIRST, TABS_OCR_SECOND, TABS_READ_A_NAME, TABS_READ_C_NAME,
    TABS_READ_E_NAME, TABS_READ_F_NAME, TABS_SECOND_FILE, TABS_SECOND_HEADING, TABS_SECOND_NAME,
    TABS_SUBDIR, TRI_OCR_A, TRI_OCR_B, TRI_OCR_C, VSPLIT_OCR_LEFT, VSPLIT_OCR_RIGHT,
};
use super::pointer::{WEDGE_OCR_CLICK, WEDGE_OCR_STAY};
use super::preview::{
    LINK_READBACK_AFTER_NAME, LINK_READBACK_BEFORE_NAME, LINK_SOURCE_FILE, LINK_TARGET_FILE,
    NEWVIEW_WINS_AFTER_NAME, NEWVIEW_WINS_BEFORE_NAME, PREVIEW_LIVE_MARKER, PREVIEW_OCR_BODY,
    PREVIEW_OCR_LIVE, PREVIEW_OCR_OTHER, PREVIEW_OCR_STATUS, PREVIEW_OTHER_FILE,
    PREVIEW_OTHER_HEADING, PREVIEW_READBACK_A_NAME, PREVIEW_READBACK_B_NAME, PREVIEW_SOURCE_FILE,
    PREVIEW_SOURCE_HEADING, PREVIEW_SOURCE_PARAGRAPH, UNSAVED_BUFFER_NAME, UNSAVED_COVER_FILE,
    UNSAVED_COVER_HEADING, UNSAVED_MARKER, UNSAVED_OCR_COVER, UNSAVED_OCR_MARKER,
    UNSAVED_READBACK_NAME,
};
use super::preview_render::{PICKOVER_OTHER_FILE, PICKOVER_SOURCE_FILE, PICKOVER_WINS_NAME};
use super::recovery::{
    RESTART_OCR_BANNER, RESTART_OCR_HEADING, RESTART_OCR_OVERLAY, RESTART_OCR_UNSAVED, STALL_FILE,
    STALL_OCR_ABSENT, STALL_OCR_PRESENT,
};
use super::restore::{
    divider_right_fixture_body, DIVIDER_BODY_LINE_COUNT, DIVIDER_FILE_LEFT, DIVIDER_FILE_RIGHT,
    DIVIDER_HEADING_RIGHT, DIVIDER_OCR_LEFT, DIVIDER_OCR_RIGHT, DIVIDER_W0_NAME, DIVIDER_W1_NAME,
    DIVIDER_W2_NAME, RESTORE_FILE_LEFT, RESTORE_FILE_MID, RESTORE_FILE_RIGHT, RESTORE_OCR_ALFA,
    RESTORE_OCR_BRAVO, RESTORE_OCR_CHARLIE, RESTORE_READBACK_NAME, SOLO_DECOY_FILE, SOLO_FILE,
    SOLO_OCR_ABSENT, SOLO_OCR_PRESENT, SOLO_READBACK_NAME, TABS_RESTORE_FILES,
    TABS_RESTORE_FILE_ONE, TABS_RESTORE_FILE_THREE, TABS_RESTORE_FILE_TWO,
    TABS_RESTORE_READBACK_NAME, TABS_RESTORE_RUN1_READBACK_NAME, TWOWIN_FIRST_FILE,
    TWOWIN_NEW_WINDOW_QUERY, TWOWIN_READBACK_B_NAME, TWOWIN_READBACK_R2_NAME, TWOWIN_SECOND_FILE,
    TWOWIN_SECOND_NAME, TWOWIN_SUBDIR,
};
use super::sidebar::{
    search_target_fixture_body, CD_BASE_FILE, CD_OCR_ROOTED, CD_OCR_ROOT_NODE,
    CD_OCR_UNABBREVIATED, CD_OUTER_DIR, CD_READBACK_NAME, CD_ROOT_DIR, CD_START_DIR,
    CD_TARGET_FILE, OPENRECENT_OCR_PRESENT, OPENRECENT_READBACK_NAME, OPENRECENT_START_FILE,
    OPENRECENT_TARGET_DIR, OPENRECENT_TARGET_FILE, PROJSEARCH_BODY_LINE_COUNT,
    PROJSEARCH_ESCAPE_READBACK_NAME, PROJSEARCH_FILE_START, PROJSEARCH_FILE_TARGET,
    PROJSEARCH_HEADING_START, PROJSEARCH_HEADING_TARGET, PROJSEARCH_MATCH_LINE,
    PROJSEARCH_MATCH_PREFIX, PROJSEARCH_OCR_ABSENT, PROJSEARCH_OCR_PRESENT, PROJSEARCH_OCR_RESULT,
    PROJSEARCH_OCR_TREE_ROW, PROJSEARCH_QUERY, PROJSEARCH_READBACK_NAME, SIDEBARTOGGLE_FILE,
    SIDEBARTOGGLE_HEADING, SIDEBARTOGGLE_OCR_ABSENT, SIDEBARTOGGLE_OCR_KEEP,
    SIDEBARTOGGLE_OPEN_FILE, TITLE_HOME_DIR, TITLE_OCR_ABSENT, TITLE_OCR_PRESENT,
    TITLE_PROJECT_DIR, TREE_OCR,
};
use super::terminal::{
    drawer_fixture_body, near_fixture_body, DRAWER_DOWN_NAME, DRAWER_FIXTURE_FILE,
    DRAWER_FIXTURE_LINE_COUNT, DRAWER_GEOMETRY_READBACK_NAME, DRAWER_HIDDEN_NAME, DRAWER_MARKER,
    DRAWER_OCR_EDITOR_ABSENT, DRAWER_OCR_MARKER, DRAWER_REOPEN_NAME, DRAWER_SESSION_SIZE_NAME,
    DRAWER_SPLITCHORD_NAME, DRAWER_SPLITCHORD_STAY_NAME, DRAWER_UP_NAME, DRAWER_VSPLIT_NAME,
    DRAWER_VSPLIT_WINS_NAME, NEAR_FIXTURE_FILE, NEAR_FIXTURE_LINE_COUNT, NEAR_G1_PID_NAME,
    NEAR_G2_NVIM_NAME, NEAR_G3_PID_NAME, NEAR_G4_NVIM_NAME, NEAR_G5_PID_NAME, NEAR_G5_TAB_NAME,
    NEAR_G6_NVIM_NAME, NEAR_G7_NVIM_NAME, NEAR_G7_PID_NAME, NEAR_OCR_FILL, TERMFLOW_NVIM_NAME,
    TERMFLOW_PID_BACK_NAME, TERMFLOW_PID_OPENED_NAME, TERMFLOW_PID_SPLIT_NAME,
    TERMRESTART_OCR_DRAWER, TERMRESTART_OCR_PANE, TERMRESTART_PID_AFTER_NAME,
    TERMRESTART_PID_DRAWER_NAME, TERMRESTART_PID_SPLIT_NAME, TP_OCR_WORD,
};
use super::*;

/// Every lowercased substring the OCR-asserting cage smokes must — or must not —
/// find, as `(term, const name, the smoke that asserts it)`.
///
/// The terms are the CONSTS themselves, never re-typed literals: a table of copies
/// could drift away from the values the smokes actually assert without anything
/// noticing, which would leave the check below quietly guarding nothing.
///
/// [`FOCUS_OCR_MARKER`] and [`NEWBUF_OCR_PRESENT`] are the lowercased forms of
/// `FOCUS_MARKER` and `NEWBUF_MARKER`, so the typed markers are covered too.
/// The must-NOT-contain terms ([`PICKER_NAV_OCR_ABSENT`], [`CLOSE_OCR_GONE`],
/// [`NEWBUF_OCR_ABSENT`], [`QUIT_OCR_GONE`], [`CHORD_CLOSE_OCR_GONE`]) are listed
/// deliberately: a filler row carrying one of those would make its smoke fail
/// SPURIOUSLY rather than pass vacuously — the opposite symptom, but the same
/// fixture bug.
///
/// [`RESTART_OCR_HEADING`] is listed once, against the smoke that asserts it is
/// PRESENT, but BOTH crash smokes assert on it — the crash-overlay smoke needs it
/// present after the button click and ABSENT during the overlay dwell, which is how
/// it proves the overlay is opaque. A filler row carrying it would break both.
///
/// The two BUFPICK terms are the same shape reached differently: each is listed once,
/// against the smoke that asserts it PRESENT, and the OTHER buffer-picker smoke
/// asserts the same term ABSENT. [`BUFPICK_OCR_PRESENT`] is present after the query
/// smoke's typed query and absent after the bare-Enter smoke's preselection;
/// [`BUFPICK_OCR_ABSENT`] is the reverse. A filler row, a temp-cwd component or a
/// fixture name carrying either would break one smoke and vacuously satisfy the
/// other, which is why one row per term is enough to guard both.
///
/// [`RESTART_OCR_UNSAVED`] is likewise asserted in BOTH directions by the SAME smoke:
/// present in the post-recovery capture (the unsaved edit came back out of the dead
/// Neovim's swap file) and absent from the pre-crash control (which is taken before
/// the edit is typed). A filler row carrying it would satisfy the first half and fail
/// the second, i.e. it would turn a real proof into a spurious red — the same fixture
/// bug either way, which is why it belongs here.
///
/// The three STATE-PERSISTENCE terms that are asserted ABSENT
/// ([`SOLO_OCR_ABSENT`], [`TITLE_OCR_ABSENT`]) are listed for the same reason as
/// the other must-NOT-contain rows. [`TITLE_OCR_ABSENT`] is the one whose fixture
/// deliberately CONTAINS it — the smoke's HOME directory is named after it, and
/// the assertion is precisely that the home segment is abbreviated to `~` and
/// therefore never drawn. That is why
/// [`restore_fixture_names_cannot_satisfy_their_own_assertions`] carries an
/// explicit, documented exemption for the two title directory names rather than
/// widening this table's guards to cover them.
///
/// [`OPENRECENT_OCR_PRESENT`] is the one term in this table whose SUBJECT is a
/// fixture FILENAME rather than a file's content — the sidebar row for
/// [`OPENRECENT_TARGET_FILE`] is the observable that proves the sidebar re-rooted,
/// the same shape [`TREE_OCR`] has. That is why that file is deliberately absent
/// from [`RELAUNCH_FIXTURE_NAMES`], and why
/// [`restore_fixture_names_cannot_satisfy_their_own_assertions`] carries a second
/// documented exemption block for it and for the target DIRECTORY the header bar
/// draws beside it.
///
/// [`SIDEBARTOGGLE_OCR_ABSENT`] is the third of that kind and the only one asserted
/// in BOTH directions by the SAME smoke: its subject is the sidebar ROW for
/// [`SIDEBARTOGGLE_FILE`], present in the first capture (the tree was populated) and
/// absent from the second (the toggle hid it). That file is therefore likewise
/// absent from [`RELAUNCH_FIXTURE_NAMES`], with its own exemption block.
/// [`SIDEBARTOGGLE_OCR_KEEP`] is an ordinary content term and follows the ordinary
/// rule.
///
/// [`PROJSEARCH_OCR_TREE_ROW`] is the fourth: the sidebar row for
/// [`PROJSEARCH_FILE_START`], absent while the Search page is up and present once the
/// Files page is back, so that file has an exemption block too.
/// [`PROJSEARCH_OCR_RESULT`] is content, the last word of the target's match line.
///
/// Every smoke that OCRs a capture registers each term it asserts on here, and a
/// smoke that OCRs nothing registers none; the second project-search smoke is covered
/// through the first's rows, since it asserts on the same terms. The smokes that
/// contribute no rows do so on purpose, and it is worth being explicit about which
/// and why:
///
/// * `colon_qa_closes_the_window_under_cage` captures no PNG and asserts only that
///   the app process is gone, so it has no OCR term a fixture filename could satisfy.
/// * `two_windows_two_neovims_and_two_saved_sessions_under_cage` captures no PNG
///   either, and could not use one: `cage` shows a single toplevel and declines a
///   `present()` on any other, so no image can show two windows. Its observables are
///   the count of live `nvim` children of the app pid, the two-window tree in a
///   frozen `state.json`, and `getcwd()` / `bufname('%')` strings the captive
///   Neovims write about themselves — none of which a rendered row can move. Its
///   fixtures are registered in [`RELAUNCH_FIXTURE_NAMES`] all the same, and carry
///   the one exemption in this file that goes the other way; see the eighth block of
///   [`restore_fixture_names_cannot_satisfy_their_own_assertions`].
/// * `three_tabs_survive_a_relaunch_under_cage` captures no PNG and could not use
///   one: a background tab's pane view is never drawn at all, and a tab TITLE is the
///   current file's name, which the per-pane statusbar strip and nvim's `:e` echo
///   also draw, so no term could discriminate one. Its observables are two frozen
///   `state.json` copies and a `writefile` readback from the restored Neovim. Its
///   fixtures are registered in [`RELAUNCH_FIXTURE_NAMES`] all the same, for the
///   generic loop's sake.
/// * `config_font_size_changes_the_grid_under_cage` likewise captures no PNG: its
///   two observables are the `winwidth(0)` integers the captive Neovim writes about
///   its own grid, so no fixture filename can help or hurt it.
/// * `column_cap_and_full_width_toggle_change_the_grid_under_cage` for the same
///   reason: two `winwidth(0)` integers either side of `:WolfFullWidth`, no capture
///   at all. It is also the one smoke that could not use OCR even if it wanted to —
///   what it measures is a COLUMN COUNT and a horizontal offset, and tesseract's
///   `contains` is position-agnostic by construction.
/// * `click_positions_the_cursor_under_cage`,
///   `drag_selects_a_range_under_cage` and
///   `wheel_scroll_moves_the_viewport_under_cage` assert on FILES the captive
///   Neovim writes about its own state — exact `line('.')`/`col('.')`,
///   `visualmode()` plus `'<`/`'>`, and `line('w0')` — not on pixels. They still
///   capture and retain a screenshot for post-mortem, but no assertion reads it, so
///   no fixture filename can help or hurt them.
/// * `divider_drag_survives_a_pane_relayout_under_cage` for the same reason: it
///   asserts on the pane WIDTHS Neovim reports for its own windows, writes no
///   fixture file at all, and its retained screenshot is post-mortem evidence
///   rather than an observable.
/// * `configured_appearance_reaches_neovims_background_under_cage` invokes `grim`
///   nowhere: its two observables are the `&background` strings the captive Neovim
///   writes about its own option, one per launch, so no fixture filename can help
///   or hurt it. The same treatment the configured-font-size smoke gets, and for
///   the same reason.
/// * `copy_and_paste_round_trip_through_the_clipboard_under_cage` likewise
///   captures nothing: its observable is the buffer readback the captive Neovim
///   writes about its own lines, and the assertion is an exact OCCURRENCE COUNT of
///   a marker that smoke seeds itself — which no sidebar row and no `:e` toast can
///   move in either direction.
/// * `preview_document_follows_the_configured_appearance_under_cage` is one of the
///   entries here that DO capture — two PNGs — and still contribute nothing,
///   because it runs `tesseract` over neither. Its observables are pixel COUNTS at
///   a derived rectangle inside one pane, so no rendered word can satisfy or break
///   an assertion of its: a filler row is text, and text is not the colour of the
///   background it is drawn on.
/// * `preview_link_click_repoints_the_pane_under_cage` is another, and for a
///   related reason: it captures twice and OCRs neither. Its evidence is the pair of
///   `tabpagebuflist()` readbacks either side of the click, and the one thing it does
///   read out of a capture is the BOUNDING BOX of the rendered link's ink — a colour
///   claim inside the preview pane, which no sidebar row can reach and no rendered
///   word can move.
/// * `preview_highlights_a_fenced_code_block_under_cage` is another: one capture,
///   OCRed nowhere. Its observables are a CHROMATIC-PIXEL count inside each of two
///   self-located code blocks, plus the Neovim readback naming the injected langtree.
///   No rendered word is a colour, so no fixture filename can reach either half.
/// * `preview_inline_code_keeps_the_baseline_grid_under_cage` is another, the
///   same way: one capture, no OCR, and its observables are a DISTANCE in pixel rows
///   between two self-located landmarks and a count of exact chip pixels. A filename
///   can put text on the page; it cannot move a rule.
/// * `file_picker_covers_the_live_preview_under_cage` is another: two captures,
///   OCRed neither. Its observables are pixel counts inside one self-located
///   rectangle, read once with the picker closed and once with it open, and the
///   discriminating half is a pair of EXACT ZEROS. Its rows in
///   [`RELAUNCH_FIXTURE_NAMES`] exist for the generic loop's sake alone: the picker
///   draws its candidates as filenames and so does the tree sidebar, which is exactly
///   why "the picker's rows are on screen" is not an assertion this smoke could make
///   — that sentence is true with the picker CLOSED.
/// * `preview_created_after_launch_renders_without_a_portal_under_cage` is
///   another: two captures, OCRed neither, and its observables are the preview's
///   background and heading-ink counts inside that background's extent on each frame. It reuses the picker smoke's two fixtures
///   and registers only its two readbacks, whose paths the cmdline strip echoes.
/// * `terminal_drawer_defaults_to_full_width_under_cage` and
///   `terminal_pane_splits_and_focus_under_cage` capture no image at all: the first
///   reads integers its shell and Neovim write, the second the pids and marks each
///   focused program writes. Their siblings
///   `terminal_drawer_opens_and_is_not_persisted_under_cage` and
///   `terminals_survive_a_neovim_restart_under_cage` DO OCR, and their terms are
///   listed below — read in a band cropped from the drawer strip, and, for the
///   restart smoke, in the pane area above it.
/// * `ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage` and
///   `the_save_dialog_writes_a_named_file_under_cage` capture nothing either: each
///   asserts on the BYTES its chord put in a file, read by the Rust side. A sidebar
///   row cannot write a file, so no fixture filename can reach either half — though
///   their four names ARE registered in [`RELAUNCH_FIXTURE_NAMES`], which is the
///   table that keeps them clear of everybody else's terms.
/// * `composed_text_reaches_neovim_under_cage` the same way, and it could not use
///   OCR even if it wanted to: its claim is an EQUALITY over three composed glyphs,
///   where `tesseract`'s output is a `contains` on a lowercased page — which could
///   not tell a doubled or reordered line from a correct one, and reads `é` against
///   `e` worst of all. Its three names are registered in
///   [`RELAUNCH_FIXTURE_NAMES`] for the generic loop's sake.
/// * `a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage`
///   likewise: its observables are `winnr('$')` counts the captive Neovim writes
///   about its own window layout, one per arm, and a rendered row cannot add or
///   remove a window. It reads no buffer text on purpose — see its scenario — so
///   there is not even a page for a term to sit on. Its six names are registered in
///   [`RELAUNCH_FIXTURE_NAMES`] for the generic loop's sake.
/// * `frame_stats_report_covers_the_live_typing_path_under_cage` captures nothing
///   at all: its observable is the report file the running application appended to,
///   and its assertions are bucket LABELS matched as rows of that file —
///   `FRAME_STATS_REQUIRED_LABELS` present, `FRAME_STATS_ABSENT_LABELS` absent. A
///   rendered word cannot put a row in a file the application wrote, so no fixture
///   filename can reach either half.
///
/// The other pointer smokes DO OCR and their terms are listed below. For
/// `click_then_close_keeps_keyboard_input_alive_under_cage` the OCR pair is
/// corroboration (the close really happened, the survivor kept rendering, the app
/// was still drawing at all) and the load-bearing assertions are its two `ALIVE*`
/// readback files; for `crash_overlay_and_restart_button_recover_under_cage` the
/// OCR IS the assertion, because "the overlay is up" and "the overlay is opaque"
/// are properties of pixels and of nothing else; and for
/// `native_tabs_open_switch_and_close_under_cage` the split is explicit — its four
/// Neovim readbacks carry every discriminator, and the OCR pair carries the one
/// claim they cannot make, that exactly ONE page's pane view is on screen at a time.
/// `neovim_panes_stay_visible_beside_terminals_under_cage` is the reverse: its
/// readbacks are preconditions, and its OCR term is the whole claim.
const ASSERTED_OCR_TERMS: &[(&str, &str, &str)] = &[
    (OCR_WORD, "OCR_WORD", "window_renders_typed_text_under_cage"),
    (
        PICKER_OCR,
        "PICKER_OCR",
        "picker_opens_and_loads_selected_file_under_cage",
    ),
    (
        PICKER_NAV_OCR_PRESENT,
        "PICKER_NAV_OCR_PRESENT",
        "picker_arrow_navigation_selects_second_match_under_cage",
    ),
    (
        PICKER_NAV_OCR_ABSENT,
        "PICKER_NAV_OCR_ABSENT",
        "picker_arrow_navigation_selects_second_match_under_cage",
    ),
    (TREE_OCR, "TREE_OCR", "tree_sidebar_shows_files_under_cage"),
    (
        VSPLIT_OCR_LEFT,
        "VSPLIT_OCR_LEFT",
        "vertical_split_shows_two_panes_under_cage",
    ),
    (
        VSPLIT_OCR_RIGHT,
        "VSPLIT_OCR_RIGHT",
        "vertical_split_shows_two_panes_under_cage",
    ),
    (
        HSPLIT_OCR_TOP,
        "HSPLIT_OCR_TOP",
        "horizontal_split_shows_two_panes_under_cage",
    ),
    (
        HSPLIT_OCR_BOTTOM,
        "HSPLIT_OCR_BOTTOM",
        "horizontal_split_shows_two_panes_under_cage",
    ),
    (
        FOCUS_OCR_LEFT,
        "FOCUS_OCR_LEFT",
        "focus_move_returns_to_first_pane_under_cage",
    ),
    (
        FOCUS_OCR_RIGHT,
        "FOCUS_OCR_RIGHT",
        "focus_move_returns_to_first_pane_under_cage",
    ),
    (
        FOCUS_OCR_MARKER,
        "FOCUS_OCR_MARKER",
        "focus_move_returns_to_first_pane_under_cage",
    ),
    (
        CLOSE_OCR_KEEP,
        "CLOSE_OCR_KEEP",
        "close_pane_leaves_one_under_cage",
    ),
    (
        CLOSE_OCR_GONE,
        "CLOSE_OCR_GONE",
        "close_pane_leaves_one_under_cage",
    ),
    (
        TRI_OCR_A,
        "TRI_OCR_A",
        "three_panes_keep_all_files_visible_under_cage",
    ),
    (
        TRI_OCR_B,
        "TRI_OCR_B",
        "three_panes_keep_all_files_visible_under_cage",
    ),
    (
        TRI_OCR_C,
        "TRI_OCR_C",
        "three_panes_keep_all_files_visible_under_cage",
    ),
    (
        EMPTY_STATE_OCR,
        "EMPTY_STATE_OCR",
        "empty_state_shows_open_folder_button_under_cage",
    ),
    (
        NEWBUF_OCR_PRESENT,
        "NEWBUF_OCR_PRESENT",
        "new_untitled_buffer_replaces_open_file_under_cage",
    ),
    (
        NEWBUF_OCR_ABSENT,
        "NEWBUF_OCR_ABSENT",
        "new_untitled_buffer_replaces_open_file_under_cage",
    ),
    (
        PALETTE_OCR_PRESENT,
        "PALETTE_OCR_PRESENT",
        "command_palette_chord_runs_the_selected_command_under_cage",
    ),
    (
        PALETTE_OCR_ABSENT,
        "PALETTE_OCR_ABSENT",
        "command_palette_chord_runs_the_selected_command_under_cage",
    ),
    (
        OPENRECENT_OCR_PRESENT,
        "OPENRECENT_OCR_PRESENT",
        "open_recent_palette_row_reroots_the_window_under_cage",
    ),
    (
        BUFPICK_OCR_PRESENT,
        "BUFPICK_OCR_PRESENT",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_OCR_ABSENT,
        "BUFPICK_OCR_ABSENT",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_OCR_CURRENT,
        "BUFPICK_OCR_CURRENT",
        "buffer_picker_bare_enter_switches_to_the_previous_buffer_under_cage",
    ),
    (
        PROJSEARCH_OCR_PRESENT,
        "PROJSEARCH_OCR_PRESENT",
        "search_panel_streams_results_and_opens_the_match_under_cage",
    ),
    (
        PROJSEARCH_OCR_ABSENT,
        "PROJSEARCH_OCR_ABSENT",
        "search_panel_streams_results_and_opens_the_match_under_cage",
    ),
    // The project-search smokes share these two terms; each row names the first.
    (
        PROJSEARCH_OCR_RESULT,
        "PROJSEARCH_OCR_RESULT",
        "search_panel_streams_results_and_opens_the_match_under_cage",
    ),
    (
        PROJSEARCH_OCR_TREE_ROW,
        "PROJSEARCH_OCR_TREE_ROW",
        "search_panel_streams_results_and_opens_the_match_under_cage",
    ),
    (
        STALL_OCR_PRESENT,
        "STALL_OCR_PRESENT",
        "stalled_nvim_raises_the_stall_overlay_under_cage",
    ),
    (
        STALL_OCR_ABSENT,
        "STALL_OCR_ABSENT",
        "stalled_nvim_raises_the_stall_overlay_under_cage",
    ),
    (
        QUIT_OCR_KEEP,
        "QUIT_OCR_KEEP",
        "colon_q_closes_focused_pane_under_cage",
    ),
    (
        QUIT_OCR_GONE,
        "QUIT_OCR_GONE",
        "colon_q_closes_focused_pane_under_cage",
    ),
    (
        CHORD_CLOSE_OCR_KEEP,
        "CHORD_CLOSE_OCR_KEEP",
        "close_pane_chord_closes_focused_pane_under_cage",
    ),
    (
        CHORD_CLOSE_OCR_GONE,
        "CHORD_CLOSE_OCR_GONE",
        "close_pane_chord_closes_focused_pane_under_cage",
    ),
    (
        WEDGE_OCR_STAY,
        "WEDGE_OCR_STAY",
        "click_then_close_keeps_keyboard_input_alive_under_cage",
    ),
    (
        WEDGE_OCR_CLICK,
        "WEDGE_OCR_CLICK",
        "click_then_close_keeps_keyboard_input_alive_under_cage",
    ),
    (
        RESTART_OCR_HEADING,
        "RESTART_OCR_HEADING",
        "nvim_crash_recovers_and_reopens_the_file_under_cage",
    ),
    (
        RESTART_OCR_UNSAVED,
        "RESTART_OCR_UNSAVED",
        "nvim_crash_recovers_and_reopens_the_file_under_cage",
    ),
    (
        RESTART_OCR_BANNER,
        "RESTART_OCR_BANNER",
        "nvim_crash_recovers_and_reopens_the_file_under_cage",
    ),
    (
        RESTART_OCR_OVERLAY,
        "RESTART_OCR_OVERLAY",
        "crash_overlay_and_restart_button_recover_under_cage",
    ),
    (
        RESTORE_OCR_ALFA,
        "RESTORE_OCR_ALFA",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_OCR_BRAVO,
        "RESTORE_OCR_BRAVO",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_OCR_CHARLIE,
        "RESTORE_OCR_CHARLIE",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        SOLO_OCR_PRESENT,
        "SOLO_OCR_PRESENT",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        SOLO_OCR_ABSENT,
        "SOLO_OCR_ABSENT",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        TITLE_OCR_PRESENT,
        "TITLE_OCR_PRESENT",
        "window_title_shows_the_working_directory_under_cage",
    ),
    (
        TITLE_OCR_ABSENT,
        "TITLE_OCR_ABSENT",
        "window_title_shows_the_working_directory_under_cage",
    ),
    (
        DIVIDER_OCR_LEFT,
        "DIVIDER_OCR_LEFT",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_OCR_RIGHT,
        "DIVIDER_OCR_RIGHT",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        NAVBACK_OCR_PRESENT,
        "NAVBACK_OCR_PRESENT",
        "alt_left_navigates_back_to_the_previous_file_under_cage",
    ),
    (
        NAVBACK_OCR_ABSENT,
        "NAVBACK_OCR_ABSENT",
        "alt_left_navigates_back_to_the_previous_file_under_cage",
    ),
    (
        SIDEBARTOGGLE_OCR_ABSENT,
        "SIDEBARTOGGLE_OCR_ABSENT",
        "sidebar_toggle_chord_hides_the_file_tree_under_cage",
    ),
    (
        SIDEBARTOGGLE_OCR_KEEP,
        "SIDEBARTOGGLE_OCR_KEEP",
        "sidebar_toggle_chord_hides_the_file_tree_under_cage",
    ),
    (
        CD_OCR_ROOTED,
        "CD_OCR_ROOTED",
        "typed_cd_reroots_the_sidebar_under_cage",
    ),
    (
        CD_OCR_ROOT_NODE,
        "CD_OCR_ROOT_NODE",
        "typed_cd_reroots_the_sidebar_under_cage",
    ),
    (
        CD_OCR_UNABBREVIATED,
        "CD_OCR_UNABBREVIATED",
        "typed_cd_reroots_the_sidebar_under_cage",
    ),
    (
        STATUS_OCR_A,
        "STATUS_OCR_A",
        "pane_status_bar_shows_each_panes_filename_under_cage",
    ),
    (
        STATUS_OCR_B,
        "STATUS_OCR_B",
        "pane_status_bar_shows_each_panes_filename_under_cage",
    ),
    (
        STATUS_OCR_MARKER,
        "STATUS_OCR_MARKER",
        "pane_status_bar_shows_each_panes_filename_under_cage",
    ),
    (
        PUM_OCR_ITEM,
        "PUM_OCR_ITEM",
        "completion_popup_lists_matches_under_cage",
    ),
    (
        PUM_OCR_ITEM2,
        "PUM_OCR_ITEM2",
        "completion_popup_lists_matches_under_cage",
    ),
    (
        MSG_OCR_FIRST,
        "MSG_OCR_FIRST",
        "multi_line_message_shows_more_than_its_last_line_under_cage",
    ),
    (
        MSG_OCR_SECOND,
        "MSG_OCR_SECOND",
        "multi_line_message_shows_more_than_its_last_line_under_cage",
    ),
    (
        CONFIRM_OCR_QUESTION,
        "CONFIRM_OCR_QUESTION",
        "confirm_prompt_shows_then_clears_once_answered_under_cage",
    ),
    (
        CONFIRM_OCR_OPTIONS,
        "CONFIRM_OCR_OPTIONS",
        "confirm_prompt_shows_then_clears_once_answered_under_cage",
    ),
    (
        PREVIEW_OCR_BODY,
        "PREVIEW_OCR_BODY",
        "preview_pane_renders_and_edit_source_returns_under_cage",
    ),
    (
        PREVIEW_OCR_OTHER,
        "PREVIEW_OCR_OTHER",
        "preview_pane_renders_and_edit_source_returns_under_cage",
    ),
    (
        PREVIEW_OCR_STATUS,
        "PREVIEW_OCR_STATUS",
        "preview_pane_renders_and_edit_source_returns_under_cage",
    ),
    (
        PREVIEW_OCR_LIVE,
        "PREVIEW_OCR_LIVE",
        "preview_pane_renders_and_edit_source_returns_under_cage",
    ),
    (
        UNSAVED_OCR_MARKER,
        "UNSAVED_OCR_MARKER",
        "preview_of_an_unsaved_buffer_renders_under_cage",
    ),
    (
        UNSAVED_OCR_COVER,
        "UNSAVED_OCR_COVER",
        "preview_of_an_unsaved_buffer_renders_under_cage",
    ),
    (
        DRAWER_OCR_MARKER,
        "DRAWER_OCR_MARKER",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        DRAWER_OCR_EDITOR_ABSENT,
        "DRAWER_OCR_EDITOR_ABSENT",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        TERMRESTART_OCR_PANE,
        "TERMRESTART_OCR_PANE",
        "terminals_survive_a_neovim_restart_under_cage",
    ),
    (
        TERMRESTART_OCR_DRAWER,
        "TERMRESTART_OCR_DRAWER",
        "terminals_survive_a_neovim_restart_under_cage",
    ),
    // Each native-tabs term is present in one capture and absent from another, so one row per term
    // guards both halves of the "exactly one page on screen" claim.
    (
        TABS_OCR_FIRST,
        "TABS_OCR_FIRST",
        "native_tabs_open_switch_and_close_under_cage",
    ),
    (
        TABS_OCR_SECOND,
        "TABS_OCR_SECOND",
        "native_tabs_open_switch_and_close_under_cage",
    ),
    (
        TP_OCR_WORD,
        "TP_OCR_WORD",
        "typed_keys_reach_a_terminal_pane_shell_under_cage",
    ),
    (
        NEAR_OCR_FILL,
        "NEAR_OCR_FILL",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
];

/// Every fixture NAME a smoke creates inside its OWN working directory, as
/// `(name, const name, the smoke that owns it)`.
///
/// The tree sidebar renders every cwd entry BY NAME, so each of these is page text
/// in its own smoke's capture — the same hazard [`SIDEBAR_FILLER_FILES`] carries,
/// aimed at a THIRD fixture. The readback files are listed too: they land in the
/// temp cwd alongside the markdown, so the sidebar draws them as well.
///
/// The state-persistence smokes were the first entries here and the name records
/// that, but the table is not limited to them: any smoke whose own fixture could
/// satisfy — or spuriously break — an [`ASSERTED_OCR_TERMS`] entry belongs in it,
/// and the command-palette smoke's base file is registered here for exactly that
/// reason. A smoke whose fixture is absent from this table is a smoke whose fixture
/// nothing checks, which is precisely how
/// `picker_opens_and_loads_selected_file_under_cage` once passed green while the
/// picker never opened.
///
/// The values are the CONSTS themselves, never re-typed literals, for the same
/// reason [`ASSERTED_OCR_TERMS`] is.
const RELAUNCH_FIXTURE_NAMES: &[(&str, &str, &str)] = &[
    (
        RESTORE_FILE_LEFT,
        "RESTORE_FILE_LEFT",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_FILE_MID,
        "RESTORE_FILE_MID",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_FILE_RIGHT,
        "RESTORE_FILE_RIGHT",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        RESTORE_READBACK_NAME,
        "RESTORE_READBACK_NAME",
        "pane_layout_survives_relaunch_under_cage",
    ),
    (
        SOLO_FILE,
        "SOLO_FILE",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        SOLO_DECOY_FILE,
        "SOLO_DECOY_FILE",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        SOLO_READBACK_NAME,
        "SOLO_READBACK_NAME",
        "single_pane_file_survives_relaunch_under_cage",
    ),
    (
        DIVIDER_FILE_LEFT,
        "DIVIDER_FILE_LEFT",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_FILE_RIGHT,
        "DIVIDER_FILE_RIGHT",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_W0_NAME,
        "DIVIDER_W0_NAME",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_W1_NAME,
        "DIVIDER_W1_NAME",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    (
        DIVIDER_W2_NAME,
        "DIVIDER_W2_NAME",
        "dragged_divider_weights_survive_relaunch_under_cage",
    ),
    // Two-window fixtures and readbacks: no OCR runs, so these rows serve the generic loop only.
    // TWOWIN_SECOND_FILE is a path, which puts TWOWIN_SUBDIR inside the same substring loop.
    (
        TWOWIN_FIRST_FILE,
        "TWOWIN_FIRST_FILE",
        "two_windows_two_neovims_and_two_saved_sessions_under_cage",
    ),
    (
        TWOWIN_SECOND_FILE,
        "TWOWIN_SECOND_FILE",
        "two_windows_two_neovims_and_two_saved_sessions_under_cage",
    ),
    (
        TWOWIN_READBACK_B_NAME,
        "TWOWIN_READBACK_B_NAME",
        "two_windows_two_neovims_and_two_saved_sessions_under_cage",
    ),
    (
        TWOWIN_READBACK_R2_NAME,
        "TWOWIN_READBACK_R2_NAME",
        "two_windows_two_neovims_and_two_saved_sessions_under_cage",
    ),
    // Three-tab fixtures and readbacks: no OCR runs, so these rows serve the generic loop only; an
    // exemption block in the guard covers their names.
    (
        TABS_RESTORE_FILE_ONE,
        "TABS_RESTORE_FILE_ONE",
        "three_tabs_survive_a_relaunch_under_cage",
    ),
    (
        TABS_RESTORE_FILE_TWO,
        "TABS_RESTORE_FILE_TWO",
        "three_tabs_survive_a_relaunch_under_cage",
    ),
    (
        TABS_RESTORE_FILE_THREE,
        "TABS_RESTORE_FILE_THREE",
        "three_tabs_survive_a_relaunch_under_cage",
    ),
    (
        TABS_RESTORE_RUN1_READBACK_NAME,
        "TABS_RESTORE_RUN1_READBACK_NAME",
        "three_tabs_survive_a_relaunch_under_cage",
    ),
    (
        TABS_RESTORE_READBACK_NAME,
        "TABS_RESTORE_READBACK_NAME",
        "three_tabs_survive_a_relaunch_under_cage",
    ),
    (
        PALETTE_FILE,
        "PALETTE_FILE",
        "command_palette_chord_runs_the_selected_command_under_cage",
    ),
    (
        OPENRECENT_START_FILE,
        "OPENRECENT_START_FILE",
        "open_recent_palette_row_reroots_the_window_under_cage",
    ),
    (
        OPENRECENT_READBACK_NAME,
        "OPENRECENT_READBACK_NAME",
        "open_recent_palette_row_reroots_the_window_under_cage",
    ),
    (
        BUFPICK_FILE_TARGET,
        "BUFPICK_FILE_TARGET",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_FILE_PRESELECT,
        "BUFPICK_FILE_PRESELECT",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_FILE_CURRENT,
        "BUFPICK_FILE_CURRENT",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    (
        BUFPICK_READBACK_NAME,
        "BUFPICK_READBACK_NAME",
        "buffer_picker_chord_switches_to_the_queried_buffer_under_cage",
    ),
    // PROJSEARCH_FILE_START is exempt: its name carries PROJSEARCH_OCR_TREE_ROW. An exemption
    // block in the guard covers it.
    (
        PROJSEARCH_FILE_TARGET,
        "PROJSEARCH_FILE_TARGET",
        "search_panel_streams_results_and_opens_the_match_under_cage",
    ),
    (
        PROJSEARCH_READBACK_NAME,
        "PROJSEARCH_READBACK_NAME",
        "search_panel_streams_results_and_opens_the_match_under_cage",
    ),
    (
        PROJSEARCH_ESCAPE_READBACK_NAME,
        "PROJSEARCH_ESCAPE_READBACK_NAME",
        "search_chord_reveals_the_sidebar_and_escape_returns_to_the_editor_under_cage",
    ),
    (
        STALL_FILE,
        "STALL_FILE",
        "stalled_nvim_raises_the_stall_overlay_under_cage",
    ),
    (
        NAVBACK_FILE_FIRST,
        "NAVBACK_FILE_FIRST",
        "alt_left_navigates_back_to_the_previous_file_under_cage",
    ),
    (
        NAVBACK_FILE_SECOND,
        "NAVBACK_FILE_SECOND",
        "alt_left_navigates_back_to_the_previous_file_under_cage",
    ),
    (
        NAVBACK_READBACK_BEFORE,
        "NAVBACK_READBACK_BEFORE",
        "alt_left_navigates_back_to_the_previous_file_under_cage",
    ),
    (
        NAVBACK_READBACK_AFTER,
        "NAVBACK_READBACK_AFTER",
        "alt_left_navigates_back_to_the_previous_file_under_cage",
    ),
    // SIDEBARTOGGLE_FILE is absent: its name is its smoke's subject, so the loop below would reject it.
    // An exemption block in the guard asserts its crossed pair instead.
    (
        SIDEBARTOGGLE_OPEN_FILE,
        "SIDEBARTOGGLE_OPEN_FILE",
        "sidebar_toggle_chord_hides_the_file_tree_under_cage",
    ),
    // CD_TARGET_FILE and the `:cd` tree's directories are absent: the name is the asserted term and the
    // directories are not files. An exemption block in the guard covers them.
    (
        CD_BASE_FILE,
        "CD_BASE_FILE",
        "typed_cd_reroots_the_sidebar_under_cage",
    ),
    // CD_READBACK_NAME is never a sidebar row, but its absolute path is echoed in the typed
    // `:call writefile(...)` line in the cmdline strip, so it must not carry an asserted term.
    (
        CD_READBACK_NAME,
        "CD_READBACK_NAME",
        "typed_cd_reroots_the_sidebar_under_cage",
    ),
    // STATUS_FILE_A and STATUS_FILE_B are absent: their names are the statusbar smoke's terms. An
    // exemption block in the guard covers them and STATUS_FIXTURE_DIR.
    (
        STATUS_READBACK_NAME,
        "STATUS_READBACK_NAME",
        "pane_status_bar_shows_each_panes_filename_under_cage",
    ),
    (
        PREVIEW_SOURCE_FILE,
        "PREVIEW_SOURCE_FILE",
        "preview_pane_renders_and_edit_source_returns_under_cage",
    ),
    (
        PREVIEW_OTHER_FILE,
        "PREVIEW_OTHER_FILE",
        "preview_pane_renders_and_edit_source_returns_under_cage",
    ),
    (
        PREVIEW_READBACK_A_NAME,
        "PREVIEW_READBACK_A_NAME",
        "preview_pane_renders_and_edit_source_returns_under_cage",
    ),
    (
        PREVIEW_READBACK_B_NAME,
        "PREVIEW_READBACK_B_NAME",
        "preview_pane_renders_and_edit_source_returns_under_cage",
    ),
    // Registered for the generic loop; this smoke reads no OCR term. `preview` is an asserted
    // term, hence `lnk*`; the paths carry LINK_FIXTURE_DIR so the loop covers the directory too.
    (
        LINK_SOURCE_FILE,
        "LINK_SOURCE_FILE",
        "preview_link_click_repoints_the_pane_under_cage",
    ),
    (
        LINK_TARGET_FILE,
        "LINK_TARGET_FILE",
        "preview_link_click_repoints_the_pane_under_cage",
    ),
    (
        LINK_READBACK_BEFORE_NAME,
        "LINK_READBACK_BEFORE_NAME",
        "preview_link_click_repoints_the_pane_under_cage",
    ),
    (
        LINK_READBACK_AFTER_NAME,
        "LINK_READBACK_AFTER_NAME",
        "preview_link_click_repoints_the_pane_under_cage",
    ),
    (
        UNSAVED_COVER_FILE,
        "UNSAVED_COVER_FILE",
        "preview_of_an_unsaved_buffer_renders_under_cage",
    ),
    (
        UNSAVED_READBACK_NAME,
        "UNSAVED_READBACK_NAME",
        "preview_of_an_unsaved_buffer_renders_under_cage",
    ),
    // Registered for the generic loop; this smoke reads no OCR term. The readback lives under
    // XDG_RUNTIME_DIR, but its path is echoed in the cmdline, as CD_READBACK_NAME's is.
    (
        PICKOVER_SOURCE_FILE,
        "PICKOVER_SOURCE_FILE",
        "file_picker_covers_the_live_preview_under_cage",
    ),
    (
        PICKOVER_OTHER_FILE,
        "PICKOVER_OTHER_FILE",
        "file_picker_covers_the_live_preview_under_cage",
    ),
    (
        PICKOVER_WINS_NAME,
        "PICKOVER_WINS_NAME",
        "file_picker_covers_the_live_preview_under_cage",
    ),
    // The new-view smoke reuses the two PICKOVER fixtures above, so only its readbacks are new.
    (
        NEWVIEW_WINS_BEFORE_NAME,
        "NEWVIEW_WINS_BEFORE_NAME",
        "preview_created_after_launch_renders_without_a_portal_under_cage",
    ),
    (
        NEWVIEW_WINS_AFTER_NAME,
        "NEWVIEW_WINS_AFTER_NAME",
        "preview_created_after_launch_renders_without_a_portal_under_cage",
    ),
    // The drawer readbacks live under XDG_RUNTIME_DIR, but their paths are typed into the cmdline or
    // a shell. The capture precedes them, so this is a tripwire for a later whole-page assertion.
    (
        DRAWER_FIXTURE_FILE,
        "DRAWER_FIXTURE_FILE",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        DRAWER_UP_NAME,
        "DRAWER_UP_NAME",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        DRAWER_DOWN_NAME,
        "DRAWER_DOWN_NAME",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        DRAWER_HIDDEN_NAME,
        "DRAWER_HIDDEN_NAME",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        DRAWER_REOPEN_NAME,
        "DRAWER_REOPEN_NAME",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        DRAWER_VSPLIT_WINS_NAME,
        "DRAWER_VSPLIT_WINS_NAME",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        DRAWER_VSPLIT_NAME,
        "DRAWER_VSPLIT_NAME",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        DRAWER_SPLITCHORD_STAY_NAME,
        "DRAWER_SPLITCHORD_STAY_NAME",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    (
        DRAWER_SPLITCHORD_NAME,
        "DRAWER_SPLITCHORD_NAME",
        "terminal_drawer_opens_and_is_not_persisted_under_cage",
    ),
    // The full-width and flow smokes capture no image; these rows serve the generic loop only.
    (
        DRAWER_GEOMETRY_READBACK_NAME,
        "DRAWER_GEOMETRY_READBACK_NAME",
        "terminal_drawer_defaults_to_full_width_under_cage",
    ),
    (
        DRAWER_SESSION_SIZE_NAME,
        "DRAWER_SESSION_SIZE_NAME",
        "terminal_drawer_defaults_to_full_width_under_cage",
    ),
    (
        TERMFLOW_PID_OPENED_NAME,
        "TERMFLOW_PID_OPENED_NAME",
        "terminal_pane_splits_and_focus_under_cage",
    ),
    (
        TERMFLOW_NVIM_NAME,
        "TERMFLOW_NVIM_NAME",
        "terminal_pane_splits_and_focus_under_cage",
    ),
    (
        TERMFLOW_PID_BACK_NAME,
        "TERMFLOW_PID_BACK_NAME",
        "terminal_pane_splits_and_focus_under_cage",
    ),
    (
        TERMFLOW_PID_SPLIT_NAME,
        "TERMFLOW_PID_SPLIT_NAME",
        "terminal_pane_splits_and_focus_under_cage",
    ),
    // The restart smoke's pid readback paths are typed into shells the capture shows.
    (
        TERMRESTART_PID_SPLIT_NAME,
        "TERMRESTART_PID_SPLIT_NAME",
        "terminals_survive_a_neovim_restart_under_cage",
    ),
    (
        TERMRESTART_PID_DRAWER_NAME,
        "TERMRESTART_PID_DRAWER_NAME",
        "terminals_survive_a_neovim_restart_under_cage",
    ),
    (
        TERMRESTART_PID_AFTER_NAME,
        "TERMRESTART_PID_AFTER_NAME",
        "terminals_survive_a_neovim_restart_under_cage",
    ),
    // Native-tabs fixtures need no exemption, as every OCR term is content. TABS_SECOND_FILE is a path
    // so TABS_SUBDIR is in the loop; the readbacks are drawn as rows and echoed in the cmdline.
    (
        TABS_FIRST_FILE,
        "TABS_FIRST_FILE",
        "native_tabs_open_switch_and_close_under_cage",
    ),
    (
        TABS_SECOND_FILE,
        "TABS_SECOND_FILE",
        "native_tabs_open_switch_and_close_under_cage",
    ),
    (
        TABS_READ_A_NAME,
        "TABS_READ_A_NAME",
        "native_tabs_open_switch_and_close_under_cage",
    ),
    (
        TABS_READ_C_NAME,
        "TABS_READ_C_NAME",
        "native_tabs_open_switch_and_close_under_cage",
    ),
    (
        TABS_READ_E_NAME,
        "TABS_READ_E_NAME",
        "native_tabs_open_switch_and_close_under_cage",
    ),
    (
        TABS_READ_F_NAME,
        "TABS_READ_F_NAME",
        "native_tabs_open_switch_and_close_under_cage",
    ),
    // The save smokes OCR nothing; these rows serve the generic loop. SAVEAS_PROPOSED_NAME is absent:
    // it is asserted not to exist on disk, so registering it would say the opposite.
    (
        SAVE_FIXTURE_FILE,
        "SAVE_FIXTURE_FILE",
        "ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage",
    ),
    (
        SAVE_READBACK_BEFORE_NAME,
        "SAVE_READBACK_BEFORE_NAME",
        "ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage",
    ),
    (
        SAVEAS_TYPED_NAME,
        "SAVEAS_TYPED_NAME",
        "the_save_dialog_writes_a_named_file_under_cage",
    ),
    (
        SAVEAS_READBACK_NAMELESS_NAME,
        "SAVEAS_READBACK_NAMELESS_NAME",
        "the_save_dialog_writes_a_named_file_under_cage",
    ),
    // The composed-text smoke OCRs nothing; these rows serve the generic loop. The readbacks live
    // under XDG_RUNTIME_DIR, but their paths are echoed in the cmdline.
    (
        COMPOSE_FIXTURE_FILE,
        "COMPOSE_FIXTURE_FILE",
        "composed_text_reaches_neovim_under_cage",
    ),
    (
        COMPOSE_BEFORE_NAME,
        "COMPOSE_BEFORE_NAME",
        "composed_text_reaches_neovim_under_cage",
    ),
    (
        COMPOSE_READBACK_NAME,
        "COMPOSE_READBACK_NAME",
        "composed_text_reaches_neovim_under_cage",
    ),
    // The composed-chord smoke OCRs nothing; these rows serve the generic loop. The readbacks live
    // under XDG_RUNTIME_DIR, but their paths are echoed in the cmdline.
    (
        IMCHORD_FIXTURE_FILE,
        "IMCHORD_FIXTURE_FILE",
        "a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage",
    ),
    (
        IMCHORD_BEFORE_NAME,
        "IMCHORD_BEFORE_NAME",
        "a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage",
    ),
    (
        IMCHORD_START_NAME,
        "IMCHORD_START_NAME",
        "a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage",
    ),
    (
        IMCHORD_SPLIT_NAME,
        "IMCHORD_SPLIT_NAME",
        "a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage",
    ),
    (
        IMCHORD_ONE_NAME,
        "IMCHORD_ONE_NAME",
        "a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage",
    ),
    (
        IMCHORD_AFTER_NAME,
        "IMCHORD_AFTER_NAME",
        "a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage",
    ),
    // The neighbour-pane readbacks live under XDG_RUNTIME_DIR, but their paths are typed on screen
    // before later captures, so they are checked like the fixture they sit beside.
    (
        NEAR_FIXTURE_FILE,
        "NEAR_FIXTURE_FILE",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
    (
        NEAR_G1_PID_NAME,
        "NEAR_G1_PID_NAME",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
    (
        NEAR_G2_NVIM_NAME,
        "NEAR_G2_NVIM_NAME",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
    (
        NEAR_G3_PID_NAME,
        "NEAR_G3_PID_NAME",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
    (
        NEAR_G4_NVIM_NAME,
        "NEAR_G4_NVIM_NAME",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
    (
        NEAR_G5_TAB_NAME,
        "NEAR_G5_TAB_NAME",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
    (
        NEAR_G5_PID_NAME,
        "NEAR_G5_PID_NAME",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
    (
        NEAR_G6_NVIM_NAME,
        "NEAR_G6_NVIM_NAME",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
    (
        NEAR_G7_PID_NAME,
        "NEAR_G7_PID_NAME",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
    (
        NEAR_G7_NVIM_NAME,
        "NEAR_G7_NVIM_NAME",
        "neovim_panes_stay_visible_beside_terminals_under_cage",
    ),
];

/// Mechanically enforces the naming invariant documented on [`SIDEBAR_FILLER_FILES`]:
/// no filler filename may contain any substring a cage smoke asserts on.
///
/// ## Why a comment was promoted to a test
///
/// The invariant is load-bearing, and until now a comment was the only thing holding
/// it. Were a filler ever named `fillhello.md`, the tree sidebar would render the word
/// `hello` on a row of its own, `window_renders_typed_text_under_cage`'s OCR
/// assertion would be satisfied by that sidebar row, and the smoke would pass green
/// forever — with an editor pane that drew nothing at all. That is precisely the
/// silently-passing test `CLAUDE.md` warns about, and it is invisible in review:
/// [`SIDEBAR_FILLER_FILES`] and the assertion it can defeat sit in different files,
/// and adding a filler file does not look like touching an assertion. A fixture that
/// can disarm the assertion it exists to support needs an executable guard, not prose.
///
/// ## The terms it guards
///
/// [`ASSERTED_OCR_TERMS`] is the authority; as a reader's summary the terms are
/// `hello`, `pickertarget`, `picktwo`/`pickone`, `treenode`,
/// `paneleft`/`paneright`, `htopfile`/`hbotfile`, `focusone`/`focustwo`/`zzleft`,
/// `closekeep`/`closegone`, `triaone`/`tribtwo`/`tricthree`, `folder`
/// ([`EMPTY_STATE_OCR`]), `zzfresh` ([`NEWBUF_OCR_PRESENT`]), `newbufbase`
/// ([`NEWBUF_OCR_ABSENT`]), `quitkeep` ([`QUIT_OCR_KEEP`]), `quitgone`
/// ([`QUIT_OCR_GONE`]), `chordkeep` ([`CHORD_CLOSE_OCR_KEEP`]) and `chordgone`
/// ([`CHORD_CLOSE_OCR_GONE`]); `wedgestay` ([`WEDGE_OCR_STAY`]) and `wedgeclick`
/// ([`WEDGE_OCR_CLICK`]); `bounceback` ([`RESTART_OCR_HEADING`]), `unsavedburst`
/// ([`RESTART_OCR_UNSAVED`]), `restarted` ([`RESTART_OCR_BANNER`]) and `crashed`
/// ([`RESTART_OCR_OVERLAY`]); `restorealfa` / `restorebravo` / `restorecharlie`
/// ([`RESTORE_OCR_ALFA`] / [`RESTORE_OCR_BRAVO`] / [`RESTORE_OCR_CHARLIE`]),
/// `solozulu` ([`SOLO_OCR_PRESENT`]), `soloalfa` ([`SOLO_OCR_ABSENT`]), `titlezulu`
/// ([`TITLE_OCR_PRESENT`]) and `titlehome` ([`TITLE_OCR_ABSENT`]); `divideralfa`
/// and `dividerbravo` ([`DIVIDER_OCR_LEFT`] / [`DIVIDER_OCR_RIGHT`]); `zzpalrun`
/// ([`PALETTE_OCR_PRESENT`]) and `palcmdbase` ([`PALETTE_OCR_ABSENT`]);
/// `bufpickalfa` ([`BUFPICK_OCR_PRESENT`]), `bufpickbravo` ([`BUFPICK_OCR_ABSENT`]),
/// `bufpickcharlie` ([`BUFPICK_OCR_CURRENT`]), `responding` ([`STALL_OCR_PRESENT`])
/// and `stallfixture` ([`STALL_OCR_ABSENT`]); `projsearchalfa`
/// ([`PROJSEARCH_OCR_PRESENT`]), `projsearchdelta` ([`PROJSEARCH_OCR_ABSENT`]),
/// `zzmango` ([`PROJSEARCH_OCR_RESULT`]) and `psrxray` ([`PROJSEARCH_OCR_TREE_ROW`]);
/// `ortango` ([`OPENRECENT_OCR_PRESENT`]); `navbackalfa` ([`NAVBACK_OCR_PRESENT`]),
/// `navbackbravo` ([`NAVBACK_OCR_ABSENT`]), `sbtangoleaf`
/// ([`SIDEBARTOGGLE_OCR_ABSENT`]) and `sidebarkeepalfa` ([`SIDEBARTOGGLE_OCR_KEEP`]);
/// `cdtango` ([`CD_OCR_ROOTED`]), `cdrootzulu` ([`CD_OCR_ROOT_NODE`]) and
/// `cdouterhome` ([`CD_OCR_UNABBREVIATED`]); `statusalfa` ([`STATUS_OCR_A`]),
/// `statusbravo` ([`STATUS_OCR_B`]) and `statusready` ([`STATUS_OCR_MARKER`]);
/// `zzbodyoscar` ([`PREVIEW_OCR_BODY`]), `zzothertango` ([`PREVIEW_OCR_OTHER`]),
/// `zzlivekilo` ([`PREVIEW_OCR_LIVE`]) and `preview` ([`PREVIEW_OCR_STATUS`]), the
/// only ordinary English word in the table and therefore the only one a filler could
/// plausibly acquire by accident; `zzunsavedromeo` ([`UNSAVED_OCR_MARKER`]) and
/// `zzcoversierra` ([`UNSAVED_OCR_COVER`]); `zzdrawermark` ([`DRAWER_OCR_MARKER`])
/// and `zzeditorfill` ([`DRAWER_OCR_EDITOR_ABSENT`]); `zzpanekilo`
/// ([`TERMRESTART_OCR_PANE`]) and `zzdrawerlima` ([`TERMRESTART_OCR_DRAWER`]); and `zztabalfa`
/// ([`TABS_OCR_FIRST`]) and `zztabbravo` ([`TABS_OCR_SECOND`]), which are asserted
/// in BOTH directions across three captures of one smoke. None is a substring of any
/// [`SIDEBAR_FILLER_FILES`] entry, and no filler may be renamed to make one so.
///
/// The smokes that contribute no term at all are enumerated in
/// [`ASSERTED_OCR_TERMS`]'s own doc comment, which is the one place that list lives:
/// a second copy of it here drifted from the first once already, and a count that
/// disagrees with the table it describes is worse than no count.
///
/// ## What it does NOT guard (and why that matters)
///
/// It inspects [`SIDEBAR_FILLER_FILES`] only — never a smoke's OWN fixture
/// filenames — so it cannot catch a smoke whose fixture file is named after the
/// word that smoke asserts. (The state-persistence smokes' own fixtures ARE covered,
/// but by a separate guard:
/// [`restore_fixture_names_cannot_satisfy_their_own_assertions`], so the two
/// failures stay separately diagnosable.) That gap was live, and an audit found
/// exactly two instances:
///
/// * `picker_opens_and_loads_selected_file_under_cage` shipped a
///   `pickertarget.md` fixture against a `pickertarget` assertion, and PASSED green
///   during a run in which the picker provably never opened — the sidebar row alone
///   satisfied it. See the note on `PICKER_FILE_NAME`.
/// * `vertical_split_shows_two_panes_under_cage` shipped `paneleft.md` /
///   `paneright.md` against `paneleft` / `paneright`, so BOTH of its assertions
///   were satisfiable without `<C-w>v` splitting anything. See the note above the
///   `VSPLIT_*` consts.
///
/// Both are fixed, and the convention they now follow is the one to apply to any
/// new smoke: **when a smoke asserts on a file's CONTENT, the file's NAME must not
/// contain the asserted word.** There are two deliberate exceptions, and both are
/// smokes whose whole subject IS the rendered NAME:
/// `tree_sidebar_shows_files_under_cage` (so [`TREE_OCR`] is a filename word on
/// purpose) and `window_title_shows_the_working_directory_under_cage` (whose two
/// DIRECTORY names are its asserted terms). That is why this guard cannot simply be
/// widened to cover fixture names without an opt-out list, and why the
/// state-persistence fixtures got their own guard with the exemption written down.
///
/// This test is PURE: no compositor, no `require_smoke_env`, no cage slot, no
/// subprocess. It reads two const tables and finishes in microseconds, so it adds
/// nothing to the smoke suite's runtime and — unlike the cage smokes — still
/// guards the fixture on a machine where the smoke toolchain is unavailable.
#[test]
fn sidebar_filler_names_cannot_satisfy_any_smoke_assertion() {
    // Both OCR readers lowercase tesseract's output, so a term that is not lowercase never matches and
    // would make the filename check below vacuous.
    for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !term.chars().any(char::is_uppercase),
            "asserted OCR term {const_name} = {term:?} is not all-lowercase. `ocr_to_lowercase` \
             lowercases tesseract's output before every `contains`, so `{smoke}` could never \
             match this term against a real capture. Lowercase the const."
        );
    }

    for name in SIDEBAR_FILLER_FILES {
        let lowercased = name.to_lowercase();
        for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
            assert!(
                !lowercased.contains(term),
                "sidebar filler file {name:?} contains {const_name} = {term:?}. The tree sidebar \
                 renders filler FILENAMES, so that row would satisfy `{smoke}`'s OCR assertion \
                 all on its own: the smoke would then pass green forever even if the editor pane \
                 drew nothing — a permanent silent false positive, not a flake. Rename the filler \
                 (see the doc comment on `SIDEBAR_FILLER_FILES`); do NOT change the OCR const and \
                 do NOT touch the smoke's assertion."
            );
        }
    }

    // Names must also be distinct: duplicates collapse on disk, so the fixture would
    // write fewer files than it claims to and thin the page density silently.
    let mut seen = std::collections::HashSet::new();
    for name in SIDEBAR_FILLER_FILES {
        assert!(
            seen.insert(name),
            "sidebar filler file {name:?} is listed twice in `SIDEBAR_FILLER_FILES`. Duplicate \
             names collapse on disk, so {SIDEBAR_FILLER_COUNT} entries would write fewer than \
             {SIDEBAR_FILLER_COUNT} files and silently thin the page density \
             `window_renders_typed_text_under_cage` depends on (measured floor: five files). \
             Below that floor tesseract misreads the isolated leading `h` of the typed word as a \
             ligature, and the smoke fails with no visible cause. Give every entry a distinct \
             name."
        );
    }
}

/// The same invariant as [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`],
/// aimed at a SECOND fixture: the two fixed parts of every [`make_temp_cwd`] path —
/// [`TEMP_CWD_BASENAME`], which the header bar actually draws, and
/// [`TEMP_CWD_PREFIX`], which it draws only if the home abbreviation regresses.
///
/// ## Why the temp directory's NAME became page text
///
/// It did not used to be. The window title was the constant `"Wolf"`, so the only
/// on-screen text a smoke could not account for came from file names. Since the
/// window title renders the app's WORKING DIRECTORY, every smoke now draws part of
/// its own temp-cwd path in the header bar, in the theme font, dark-on-light — i.e.
/// in the most legible text on the page — and `tesseract` reads it along with
/// everything else. A basename of `folder` would satisfy
/// `empty_state_shows_open_folder_button_under_cage`'s assertion from the header
/// bar alone, and that smoke would pass green with no status page drawn at all.
///
/// The two directions both matter, exactly as they do for the filler names: a
/// must-CONTAIN term in the path makes a smoke pass vacuously, and a must-NOT-CONTAIN
/// term makes it fail spuriously. [`ASSERTED_OCR_TERMS`] carries both kinds and this
/// guard checks against all of them.
///
/// ## What is drawn, and what this guard therefore covers
///
/// [`make_temp_cwd`] returns `<unique>/project` and [`cage_command`] sets `HOME` to
/// `<unique>`, so `title::display_title` abbreviates and the rendered title is the
/// constant `~/project`. So [`TEMP_CWD_BASENAME`] is the part that is REALLY on the
/// page, and it is checked first. [`TEMP_CWD_PREFIX`] and its `{pid}-{nanos}-{seq}` tail
/// are normally abbreviated away — but they are still checked, because a regression
/// in the abbreviation draws the whole path, and a prefix carrying an asserted term
/// would turn that clean red into a mysterious pass (or a mysterious spurious
/// failure) in a different smoke entirely. Guarding the invisible half costs one loop
/// and keeps the diagnosis honest.
///
/// ## Why it is a separate test from the filler guard
///
/// So the two failures stay separately diagnosable: one says "rename a filler file",
/// the other says "rename the temp-directory basename or prefix", and they have
/// different fixes. The filler guard is deliberately left untouched.
///
/// ## What it does not cover
///
/// Only the two fixed components. The variable part of the path is the process id, a
/// nanosecond timestamp and a sequence number — ASCII digits and `-` — which the third loop below proves
/// cannot contain any asserted term rather than merely asserting it in prose. The
/// PARENT of the unique directory (`std::env::temp_dir()`, i.e. `TMPDIR` or `/tmp`)
/// is NOT checked, because it is a property of the host rather than of this fixture;
/// it is abbreviated away today, but a `TMPDIR` containing an asserted term would
/// show up as a mysterious pass or failure if the abbreviation ever broke, and this
/// note is the place to start looking.
///
/// PURE, like its two siblings: no compositor, no `require_smoke_env`, no
/// cage slot, no subprocess.
#[test]
fn temp_cwd_name_cannot_satisfy_any_smoke_assertion() {
    // Guard the guard: an empty term table would make every loop below run zero times
    // and pass for the wrong reason.
    assert!(
        !ASSERTED_OCR_TERMS.is_empty(),
        "`ASSERTED_OCR_TERMS` is empty, so this guard checks nothing at all. It is the single \
         registry of every substring the cage smokes assert OCR must or must not contain; if a \
         smoke's term is missing from it, neither this guard nor \
         `sidebar_filler_names_cannot_satisfy_any_smoke_assertion` can protect that smoke."
    );

    // The basename is one ordinary segment: empty or `.` makes HOME the temp root, a separator adds
    // a level `smoke_home` ignores, and `..` climbs out.
    let mut components = Path::new(TEMP_CWD_BASENAME).components();
    let first = components.next();
    let extra = components.next();
    assert!(
        matches!(first, Some(std::path::Component::Normal(_))) && extra.is_none(),
        "TEMP_CWD_BASENAME = {TEMP_CWD_BASENAME:?} is not a single ordinary path segment (it \
         parses as {first:?} followed by {extra:?}). `make_temp_cwd` joins it onto a unique parent \
         and `cage_command` hands that parent to the app as HOME, so the header-bar title reads \
         `~/{TEMP_CWD_BASENAME}`. An empty value collapses the two into one directory — HOME == \
         cwd, which also flips `workspace::launch_decision` into its EmptyState branch for EVERY \
         smoke — and a separator or a `..` moves the level HOME is taken from; both put \
         run-varying text back on the page. See `make_temp_cwd` for the measured cost of that."
    );

    // The component that IS rendered: `HOME` is the cwd's parent, so the header bar
    // reads `~/<TEMP_CWD_BASENAME>` in every smoke that does not override HOME.
    let lowercased = TEMP_CWD_BASENAME.to_lowercase();
    for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !lowercased.contains(term),
            "the temp-cwd basename {TEMP_CWD_BASENAME:?} contains {const_name} = {term:?}. Every \
             smoke's window title renders its working directory home-abbreviated, so this word is \
             drawn in the header bar of EVERY capture as `~/{TEMP_CWD_BASENAME}`: it would satisfy \
             (or, for a must-NOT-contain term, spuriously break) `{smoke}`'s OCR assertion with \
             nothing else on screen having to work at all. Rename `TEMP_CWD_BASENAME` — keep it \
             short and word-like, since it is glyph mass on every page — and do NOT change the OCR \
             const or touch the smoke's assertion."
        );
    }

    // Checked though abbreviated away: a title that stops abbreviating draws it, and this keeps
    // that bug from passing for another smoke's failure.
    let lowercased = TEMP_CWD_PREFIX.to_lowercase();
    for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !lowercased.contains(term),
            "the temp-cwd prefix {TEMP_CWD_PREFIX:?} contains {const_name} = {term:?}. It names \
             the UNIQUE parent directory, which `cage_command` hands the app as HOME and which \
             `title::display_title` therefore abbreviates to `~` — so it is not drawn today. But a \
             regression in that abbreviation draws the whole path in the header bar, at which \
             point this word would satisfy (or, for a must-NOT-contain term, spuriously break) \
             `{smoke}`'s OCR assertion and the abbreviation bug would surface as an unrelated \
             smoke behaving strangely instead of as a clean red. Rename `TEMP_CWD_PREFIX`; do NOT \
             change the OCR const and do NOT touch the smoke's assertion."
        );
    }

    // The path's variable part is digits and `-`, safe only if no asserted term is spellable in
    // them, a property of the terms checked here.
    for &(term, const_name, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            term.chars().any(|c| c.is_ascii_alphabetic()),
            "asserted OCR term {const_name} = {term:?} contains no ASCII letter, so it could be a \
             substring of the digits-and-hyphens tail `make_temp_cwd` appends to \
             {TEMP_CWD_PREFIX:?} (the process id, a nanosecond timestamp and a sequence number). \
             `{smoke}` would \
             then be satisfied — or spuriously broken — by whatever pid the test harness happened \
             to get, on any run where the title's home abbreviation is not doing its job. Give the \
             term a letter, or stop deriving the temp-dir name from numbers."
        );
    }
}

/// The same invariant again, aimed at a THIRD fixture: the SMOKE-OWNED files listed
/// in [`RELAUNCH_FIXTURE_NAMES`] — the ones a smoke writes into its own working
/// directory, of which the state-persistence smokes' were the first — plus the two
/// directory names the title smoke builds.
///
/// ## Why these fixtures need their own guard
///
/// [`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`] inspects
/// [`SIDEBAR_FILLER_FILES`] and nothing else, and its own doc comment records the
/// gap: it "cannot catch a smoke whose fixture file is named after the word that
/// smoke asserts", which is exactly how
/// `picker_opens_and_loads_selected_file_under_cage` once passed green while the
/// picker provably never opened, and how
/// `vertical_split_shows_two_panes_under_cage` shipped `paneleft.md` /
/// `paneright.md` against `paneleft` / `paneright`. Every smoke in this table
/// asserts on FILE CONTENT (a rendered heading) while dropping its fixture into a
/// cwd the tree sidebar lists BY NAME, so each is in precisely that hazard class —
/// the state-persistence smokes, which brought the table into being, and the
/// command-palette smoke, whose `pbase.md` must never come to contain `palcmdbase`.
/// Both directions matter: a must-CONTAIN term in a fixture name makes its smoke
/// pass vacuously, and a must-NOT-CONTAIN term makes it fail spuriously.
///
/// ## The seven deliberate, documented exemptions
///
/// The title smoke's two DIRECTORY names are not files and are not in
/// [`RELAUNCH_FIXTURE_NAMES`], because they are the assertion's subject rather
/// than a hazard to it: [`TITLE_PROJECT_DIR`] must contain
/// [`TITLE_OCR_PRESENT`] (the header bar rendering it is the whole point) and
/// [`TITLE_HOME_DIR`] must contain [`TITLE_OCR_ABSENT`] (the assertion is that
/// the home segment is abbreviated to `~` and therefore NOT drawn). This is the
/// same shape as `tree_sidebar_shows_files_under_cage`'s exemption from the
/// filler guard. Rather than leave that as prose, the third block below asserts
/// the exemption's PRECONDITIONS — including the crossed pair, where a mistake
/// really would be fatal.
///
/// The Open Recent smoke needs the same treatment for the same reason and one
/// more. Its subject is [`OPENRECENT_TARGET_FILE`]'s NAME — the sidebar row is
/// what proves the sidebar re-rooted — so that file must CONTAIN
/// [`OPENRECENT_OCR_PRESENT`] and is likewise absent from
/// [`RELAUNCH_FIXTURE_NAMES`], whose generic loop forbids exactly that property.
/// The crossed half is the one that matters: the header bar draws
/// `~/`[`OPENRECENT_TARGET_DIR`] on every capture of that smoke, so a target
/// DIRECTORY carrying the asserted term would satisfy its OCR assertion from the
/// title alone — with the sidebar never re-rooting and `settle_reroot`
/// never firing. The fourth block below asserts both halves, plus that neither
/// name carries any OTHER asserted term.
///
/// The sidebar-toggle smoke is the third, and its crossed pair is the sharpest of
/// the three because both halves are asserted by the SAME smoke on the SAME pair of
/// captures. [`SIDEBARTOGGLE_FILE`]'s NAME is the observable — the file is never
/// opened, so only a tree row can draw it — and it must therefore contain
/// [`SIDEBARTOGGLE_OCR_ABSENT`], which the generic loop forbids; it is likewise
/// absent from [`RELAUNCH_FIXTURE_NAMES`]. The crossed half is that it must NOT
/// contain [`SIDEBARTOGGLE_OCR_KEEP`]: that term is the ANTI-VACUITY assertion on
/// the second capture, so it has to come from the EDITOR rather than the sidebar,
/// or hiding the sidebar would remove the proof-of-life along with the subject and
/// the two assertions would contradict each other on a correct run. The fifth block
/// below asserts both halves, that the editor fixture carries neither term in its
/// NAME, that its heading carries the keep-term, and that
/// [`SIDEBARTOGGLE_FILE`] carries no OTHER asserted term.
///
/// The typed-`:cd` smoke is the fourth, and it needs the widest block of the five
/// because THREE of its fixed path components are page text at once.
/// [`CD_TARGET_FILE`]'s NAME is the subject of assertion 4 — it lives only inside the
/// `:cd` target, so a sidebar row is the sole thing that can draw it — and it must
/// therefore contain [`CD_OCR_ROOTED`], which the generic loop forbids; it is
/// likewise absent from [`RELAUNCH_FIXTURE_NAMES`]. Beside it sit three directories.
/// [`CD_ROOT_DIR`] must contain [`CD_OCR_ROOT_NODE`] (assertion 6 reads the synthetic
/// root row, which draws that basename) and must NOT contain [`CD_OCR_ROOTED`] or
/// [`CD_OCR_UNABBREVIATED`]. [`CD_OUTER_DIR`] must contain [`CD_OCR_UNABBREVIATED`]
/// (or assertion 5 is vacuous) and must NOT contain [`CD_OCR_ROOT_NODE`] — that is
/// the sharpest crossed pair here, because a failed title abbreviation draws the
/// whole path and assertion 5 is what tells that apart from a real root row.
/// [`CD_START_DIR`] is the launch cwd: the header bar draws `~/startdir` before the
/// `:cd` and a sidebar `Dir` row draws it after, so it must neither carry nor be
/// carried by any asserted term. The sixth block below asserts all of that.
///
/// The per-pane statusbar smoke is the fifth, and its crossed pair is the only one
/// whose two halves are both must-CONTAIN terms. [`STATUS_FILE_A`] and
/// [`STATUS_FILE_B`] are each the subject of their own assertion — the strip above a
/// pane drawing THAT pane's filename is the whole feature — so each must contain its
/// own term, which the generic loop forbids, and both are absent from
/// [`RELAUNCH_FIXTURE_NAMES`]. The crossing is what makes the pair prove
/// per-pane-ness rather than mere presence: if either filename also contained the
/// OTHER's term, one pane's strip would satisfy both assertions and the documented
/// revert — drawing the ACTIVE pane's status in every strip — would leave the smoke
/// green. Beside them sit two more names. [`STATUS_FIXTURE_DIR`] is drawn by the
/// strip's second line (the display path) and by the typed `:e` command line, and
/// [`STATUS_FIXTURE_BODY`] is drawn by the editor grid itself, since these are the
/// only exempt fixtures this binary OPENS; either one carrying an asserted term would
/// satisfy that term from a place the filename line never drew. The seventh block
/// below asserts all of that, plus that neither filename carries
/// [`STATUS_OCR_MARKER`] — which is the capture's proof of life and must not be
/// satisfiable by the very thing it is proving was drawn.
///
/// The two-window smoke is the sixth and the only one of a different KIND. The five
/// above are fixtures that must CARRY an asserted OCR term; its two fixtures are
/// named after the strings its assertions compare against, which the rule at the top
/// of this file forbids outright. It is exempt because it takes no capture at all —
/// it invokes `grim` nowhere and `tesseract` nowhere — and what it compares are
/// `TabState::layout` leaf paths out of a frozen `state.json` and `getcwd()` /
/// `bufname('%')` strings the captive Neovim writes about itself. A sidebar row
/// cannot write a `PaneNode::Leaf` and an `:e` toast cannot either, so the hazard
/// the rule exists for has no route in. The names are registered in
/// [`RELAUNCH_FIXTURE_NAMES`] regardless, so the generic loop still keeps them clear
/// of every other smoke's terms; what the eighth block adds is the CROSSING —
/// [`TWOWIN_FIRST_FILE`] against [`TWOWIN_SECOND_NAME`] and [`TWOWIN_SUBDIR`]
/// against [`TEMP_CWD_BASENAME`] — without which "the two sessions were saved and
/// restored in order" is satisfied by either window twice.
///
/// The two project-search smokes are the seventh. [`PROJSEARCH_FILE_START`]'s NAME
/// carries [`PROJSEARCH_OCR_TREE_ROW`], because a tree row is what says which sidebar
/// page is up, so it is absent from [`RELAUNCH_FIXTURE_NAMES`]. The crossed half is
/// everything the Search page draws: the target's name, the query and the match
/// line must not carry the tree-row term, or the panel itself would draw it. The
/// block after the project-search fixture checks asserts that, the loop's own checks
/// on that one name, and that [`PROJSEARCH_HEADING_START`], on screen beside the
/// panel, lacks [`PROJSEARCH_OCR_RESULT`].
///
/// ## The fixtures whose CONTENT is checked too
///
/// Most relaunch fixtures are a bare heading, so the name is the only thing that
/// can collide with an assertion. Four carry a built body, and each body renders in
/// the same pane as a heading its smoke asserts on — so a body line carrying an
/// asserted term would satisfy that assertion with the heading never drawn, which is
/// the `PICKER_FILE_NAME` failure reached through CONTENT instead of through a
/// name.
///
/// [`DIVIDER_FILE_RIGHT`] carries [`divider_right_fixture_body`]: page density for a
/// 15-cell pane, see [`DIVIDER_BODY_LINE_COUNT`].
/// [`PROJSEARCH_FILE_TARGET`] carries [`search_target_fixture_body`], and its
/// content is load-bearing in a stronger sense than page density — the first
/// project-search smoke's three exact assertions (the file, the LINE and the COLUMN
/// the cursor landed on) are all properties of that body, and the result word both
/// smokes read in the sidebar is its match line's last word, so the block below pins
/// the match's line index, its byte offset and that word as well as the vocabulary.
/// [`PREVIEW_SOURCE_FILE`] carries [`PREVIEW_SOURCE_HEADING`] plus
/// [`PREVIEW_SOURCE_PARAGRAPH`], and it is the only one of the three whose body
/// reaches a pixel through WebKit rather than through the editor grid. Three rules
/// there, and the other two fixtures need none of them. [`PREVIEW_OCR_STATUS`] is an
/// ordinary English word, so every string that smoke puts on screen — both headings,
/// the paragraph and the live marker — is checked against it. The live marker is
/// checked NOT to be in the source already, or the capture that discriminates the
/// body swap would hold before a single keystroke reached the DOM. And the two
/// headings are CROSSED, because they are drawn by different renderers: an editor
/// heading carrying the web view's term would satisfy a web-view assertion from the
/// grid beside it.
///
/// [`DRAWER_FIXTURE_FILE`] carries [`drawer_fixture_body`], and it is the one body
/// here whose job is to be ABSENT from what its smoke reads: the terminal-drawer
/// smoke OCRs a band inside the drawer strip, so the editor rows that would occupy
/// those pixels with no drawer painted have to be recognisable. Hence a LINE COUNT
/// check (too short a body and the band's rows are wordless, so the must-NOT-contain
/// assertion holds either way) and a per-line one, plus the crossing against
/// [`DRAWER_MARKER`] — the marker is drawn by the SHELL and the fill by this crate's
/// painter, and each carrying the other's term would let one satisfy the other's
/// assertion.
///
/// The native-tabs smoke's two fixtures are checked here as well, and they are the
/// plainest case in the list: their content is a bare heading, neither is exempt
/// from anything, and both go through the generic loop above like any ordinary
/// fixture — because both of that smoke's asserted terms are file CONTENT rather
/// than a name. What the ninth block adds is the one thing the generic loop cannot
/// reach, the CROSSING between [`TABS_FIRST_HEADING`] and [`TABS_SECOND_HEADING`].
/// That crossing is the whole discriminator: the smoke claims exactly ONE page's
/// pane view is on screen at a time, states it as two must-NOT-contain halves on two
/// captures, and a heading carrying the other tab's term makes both halves hold on a
/// page showing both panes at once. The same block pins the fixture SHAPE the legs
/// depend on — the second fixture's path being its subdirectory joined to the bare
/// name leg (d) types, the first being a bare name `bufname('%')` can equal, and
/// [`TABS_SUBDIR`] being a single component that is not [`TEMP_CWD_BASENAME`], which
/// is what keeps a new tab rooted where the window was distinguishable from one left
/// at the project root.
///
/// [`NEAR_FIXTURE_FILE`] carries [`near_fixture_body`], whose word the neighbour-pane
/// smoke asserts PRESENT in every capture, so the block that checks it pins the line
/// count and the word on every line: a row without it would read as an unpainted editor.
///
/// All of them are checked here rather than from guards of their own.
///
/// ## Measured red run
///
/// The fifth exemption block was demonstrated RED by renaming [`SIDEBARTOGGLE_FILE`]
/// to `"zzmikeleaf.md"`, i.e. by making the sidebar-toggle smoke's fixture stop
/// carrying the term that smoke reads off it. The guard failed with
/// `SIDEBARTOGGLE_FILE = "zzmikeleaf.md" no longer contains SIDEBARTOGGLE_OCR_ABSENT
/// = "sbtangoleaf"`, naming the consequence — capture A would be VACUOUS, so the
/// smoke's must-not-contain discriminator would hold permanently while proving
/// nothing. Recorded here because this guard has no `measured` table of its own, and
/// evidence that lives only in a run log is evidence that is about to be lost.
///
/// PURE, like its two siblings: no compositor, no `require_smoke_env`, no
/// cage slot, no subprocess.
#[test]
fn restore_fixture_names_cannot_satisfy_their_own_assertions() {
    // Guard the guard: either empty table would make the loops below run zero times
    // and pass for the wrong reason.
    assert!(
        !ASSERTED_OCR_TERMS.is_empty(),
        "`ASSERTED_OCR_TERMS` is empty, so this guard checks nothing at all."
    );
    assert!(
        !RELAUNCH_FIXTURE_NAMES.is_empty(),
        "`RELAUNCH_FIXTURE_NAMES` is empty, so the smoke-owned fixture files are unguarded — \
         which is the exact gap that let `picker_opens_and_loads_selected_file_under_cage` pass \
         green while the picker never opened."
    );

    for &(name, name_const, owner) in RELAUNCH_FIXTURE_NAMES {
        let lowercased = name.to_lowercase();
        for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
            assert!(
                !lowercased.contains(term),
                "fixture {name_const} = {name:?} (created by `{owner}`) contains {term_const} = \
                 {term:?}. The tree sidebar renders every cwd entry BY NAME, so that row would \
                 satisfy — or, for a must-NOT-contain term, spuriously break — `{smoke}`'s OCR \
                 assertion with the editor pane drawing nothing at all. Follow the convention the \
                 sibling smokes already use: put the asserted word in the file's CONTENT and give \
                 the file a name whose lowercase does not contain it. Do NOT change the OCR const \
                 and do NOT touch the smoke's assertion."
            );
        }
    }

    // Distinct names, distinct from the fillers: all land in one temp cwd, so a collision silently
    // overwrites a fixture.
    let mut seen = std::collections::HashSet::new();
    for &(name, name_const, owner) in RELAUNCH_FIXTURE_NAMES {
        assert!(
            seen.insert(name),
            "fixture {name_const} = {name:?} (owned by `{owner}`) is listed twice in \
             `RELAUNCH_FIXTURE_NAMES`; two fixtures sharing a name overwrite each other on disk."
        );
        assert!(
            !SIDEBAR_FILLER_FILES.contains(&name),
            "fixture {name_const} = {name:?} (owned by `{owner}`) collides with a \
             `SIDEBAR_FILLER_FILES` entry. Both are written into the same temp cwd, so one would \
             clobber the other and the smoke would assert against the wrong contents."
        );
    }

    // The title smoke's exemption, with its preconditions made executable rather
    // than merely documented above.
    assert!(
        TITLE_PROJECT_DIR.to_lowercase().contains(TITLE_OCR_PRESENT),
        "TITLE_PROJECT_DIR = {TITLE_PROJECT_DIR:?} no longer contains TITLE_OCR_PRESENT = \
         {TITLE_OCR_PRESENT:?}. The header-bar title renders that directory's BASENAME, so if the \
         two ever stop agreeing the smoke asserts on a word nothing draws and fails with no \
         visible cause."
    );
    assert!(
        TITLE_HOME_DIR.to_lowercase().contains(TITLE_OCR_ABSENT),
        "TITLE_HOME_DIR = {TITLE_HOME_DIR:?} no longer contains TITLE_OCR_ABSENT = \
         {TITLE_OCR_ABSENT:?}. The must-NOT-contain assertion is what proves the home prefix was \
         ABBREVIATED to `~`; if the home directory is not named after that term, the assertion \
         holds trivially and proves nothing — a vacuous test, not a passing one."
    );
    assert!(
        !TITLE_PROJECT_DIR.to_lowercase().contains(TITLE_OCR_ABSENT),
        "TITLE_PROJECT_DIR = {TITLE_PROJECT_DIR:?} contains TITLE_OCR_ABSENT = \
         {TITLE_OCR_ABSENT:?}. The project basename IS rendered in the title, so the \
         must-NOT-contain assertion would fire on a perfectly correct abbreviation."
    );
    assert!(
        !TITLE_HOME_DIR.to_lowercase().contains(TITLE_OCR_PRESENT),
        "TITLE_HOME_DIR = {TITLE_HOME_DIR:?} contains TITLE_OCR_PRESENT = {TITLE_OCR_PRESENT:?}. \
         If the home segment ever did get rendered it would then satisfy the must-CONTAIN \
         assertion on its own, and a broken abbreviation would look like a pass."
    );

    // The Open Recent exemption: one fixture whose name is the subject, one drawn beside it on every
    // capture that must stay clear of the term.
    assert!(
        OPENRECENT_TARGET_FILE
            .to_lowercase()
            .contains(OPENRECENT_OCR_PRESENT),
        "OPENRECENT_TARGET_FILE = {OPENRECENT_TARGET_FILE:?} no longer contains \
         OPENRECENT_OCR_PRESENT = {OPENRECENT_OCR_PRESENT:?}. That FILENAME is the subject of the \
         smoke's OCR assertion — the tree sidebar draws it, and it is the only observable proving \
         `settle_reroot` -> `tree_view::reroot` restarted the traversal at the new root. \
         If the two stop agreeing the smoke asserts on a word nothing draws and fails with no \
         visible cause."
    );
    assert!(
        !OPENRECENT_TARGET_DIR
            .to_lowercase()
            .contains(OPENRECENT_OCR_PRESENT),
        "OPENRECENT_TARGET_DIR = {OPENRECENT_TARGET_DIR:?} contains OPENRECENT_OCR_PRESENT = \
         {OPENRECENT_OCR_PRESENT:?}. This is the crossed pair that matters. The header bar draws \
         `~/{OPENRECENT_TARGET_DIR}` on EVERY capture of that smoke (the target directory is a \
         child of the app's HOME, which is what pins the title to a constant string), so a target \
         directory whose name carried the asserted term would satisfy the smoke's OCR assertion \
         from the TITLE alone — with the sidebar never re-rooting, `settle_reroot` never \
         firing and `tree_view::reroot` never running. The smoke would then pass green against \
         exactly the build its revert produces. Rename the DIRECTORY; do NOT change \
         OPENRECENT_OCR_PRESENT and do NOT touch the assertion."
    );
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        if term == OPENRECENT_OCR_PRESENT {
            continue;
        }
        for (name, name_const) in [
            (OPENRECENT_TARGET_FILE, "OPENRECENT_TARGET_FILE"),
            (OPENRECENT_TARGET_DIR, "OPENRECENT_TARGET_DIR"),
        ] {
            assert!(
                !name.to_lowercase().contains(term),
                "{name_const} = {name:?} contains {term_const} = {term:?}. Both of these are page \
                 text in the Open Recent smoke's capture — one as a tree-sidebar row, one as the \
                 header-bar title — so this word would satisfy, or for a must-NOT-contain term \
                 spuriously break, `{smoke}`'s OCR assertion. These two names are EXEMPT from the \
                 RELAUNCH_FIXTURE_NAMES loop above only for OPENRECENT_OCR_PRESENT itself; every \
                 other asserted term still applies to them."
            );
        }
    }

    // The sidebar-toggle exemption. The crossed half matters: only SIDEBARTOGGLE_FILE's row can draw
    // the term, so a rename fails capture A's precondition as if the sidebar never populated.
    assert!(
        SIDEBARTOGGLE_FILE
            .to_lowercase()
            .contains(SIDEBARTOGGLE_OCR_ABSENT),
        "SIDEBARTOGGLE_FILE = {SIDEBARTOGGLE_FILE:?} no longer contains SIDEBARTOGGLE_OCR_ABSENT = \
         {SIDEBARTOGGLE_OCR_ABSENT:?}. That FILENAME is the sidebar's only observable — the file is \
         never opened, so nothing but a tree row can draw the word — and capture A asserts it is \
         PRESENT. If the two stop agreeing, capture A is VACUOUS: the term is absent from BOTH \
         captures whatever the toggle did, so the smoke's must-NOT-contain discriminator would \
         hold permanently while proving nothing. This file is exempt from the \
         RELAUNCH_FIXTURE_NAMES loop above for exactly this reason."
    );
    assert!(
        !SIDEBARTOGGLE_FILE
            .to_lowercase()
            .contains(SIDEBARTOGGLE_OCR_KEEP),
        "SIDEBARTOGGLE_FILE = {SIDEBARTOGGLE_FILE:?} contains SIDEBARTOGGLE_OCR_KEEP = \
         {SIDEBARTOGGLE_OCR_KEEP:?}. The keep-term is the ANTI-VACUITY assertion on capture B — it \
         is what proves the window is still DRAWING, so that a crashed or blank window cannot \
         satisfy the must-NOT-contain assertion beside it. Sourcing it from a sidebar row would \
         destroy that: the row and the thing being hidden are the same object, so hiding the \
         sidebar would remove the proof-of-life along with the subject and the two assertions \
         would contradict each other on a perfectly correct run."
    );
    for (name, name_const) in [
        (SIDEBARTOGGLE_OPEN_FILE, "SIDEBARTOGGLE_OPEN_FILE"),
        (SIDEBARTOGGLE_HEADING, "SIDEBARTOGGLE_HEADING"),
    ] {
        assert!(
            !name.to_lowercase().contains(SIDEBARTOGGLE_OCR_ABSENT),
            "{name_const} = {name:?} contains SIDEBARTOGGLE_OCR_ABSENT = \
             {SIDEBARTOGGLE_OCR_ABSENT:?}. Both of these are drawn by the EDITOR — the filename by \
             nvim's `:e` toast and the sidebar row, the heading by the pane itself — and the editor \
             keeps drawing after the sidebar is hidden. Either one carrying the term would make \
             capture B's must-NOT-contain assertion fail on a correct run, i.e. a hidden sidebar \
             would look like a broken one."
        );
    }
    assert!(
        SIDEBARTOGGLE_HEADING
            .to_lowercase()
            .contains(SIDEBARTOGGLE_OCR_KEEP),
        "SIDEBARTOGGLE_HEADING = {SIDEBARTOGGLE_HEADING:?} does not contain \
         SIDEBARTOGGLE_OCR_KEEP = {SIDEBARTOGGLE_OCR_KEEP:?}. That heading is the only place the \
         keep-term exists, so if the two stop agreeing both captures assert on a word nothing \
         draws and the smoke fails with no visible cause."
    );
    assert!(
        !SIDEBARTOGGLE_OPEN_FILE
            .to_lowercase()
            .contains(SIDEBARTOGGLE_OCR_KEEP),
        "SIDEBARTOGGLE_OPEN_FILE = {SIDEBARTOGGLE_OPEN_FILE:?} contains SIDEBARTOGGLE_OCR_KEEP = \
         {SIDEBARTOGGLE_OCR_KEEP:?}. The keep-term must come from a RENDERED BUFFER: with the term \
         in the filename, the sidebar row and the `:e` toast would satisfy capture B's \
         proof-of-life assertion even with the editor pane drawing nothing at all, and the \
         must-NOT-contain assertion beside it would then be unguarded again."
    );
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        if term == SIDEBARTOGGLE_OCR_ABSENT {
            continue;
        }
        assert!(
            !SIDEBARTOGGLE_FILE.to_lowercase().contains(term),
            "SIDEBARTOGGLE_FILE = {SIDEBARTOGGLE_FILE:?} contains {term_const} = {term:?}. It is \
             page text in its smoke's capture A as a tree-sidebar row, so this word would satisfy \
             — or, for a must-NOT-contain term, spuriously break — `{smoke}`'s OCR assertion. This \
             name is EXEMPT from the RELAUNCH_FIXTURE_NAMES loop above only for \
             SIDEBARTOGGLE_OCR_ABSENT itself; every other asserted term still applies to it."
        );
    }

    // The typed-`:cd` exemption: three components of `<base>/cdouterhome/cdrootzulu/` are page text
    // and cross both ways. `cdrootzulu` is HOME, so after `:cd ..` the title is the bare `~`.

    // Assertion 4's subject: `cdtango.md` exists only inside the `:cd` target, so only a sidebar row
    // draws it, which is why it is exempt from the loop above.
    assert!(
        CD_TARGET_FILE.to_lowercase().contains(CD_OCR_ROOTED),
        "CD_TARGET_FILE = {CD_TARGET_FILE:?} no longer contains CD_OCR_ROOTED = \
         {CD_OCR_ROOTED:?}. That FILENAME is the subject of the typed-`:cd` smoke's assertion 4 — \
         the file is never opened, so nothing but a tree row under the re-rooted synthetic root can \
         draw the word. If the two stop agreeing the smoke asserts on a word nothing draws and \
         fails with no visible cause, reporting a re-root failure that did not happen."
    );

    // The SUBJECT of assertion 6: the synthetic root row draws this directory's
    // BASENAME, and after the `:cd` nothing else on the page can.
    assert!(
        CD_ROOT_DIR.to_lowercase().contains(CD_OCR_ROOT_NODE),
        "CD_ROOT_DIR = {CD_ROOT_DIR:?} no longer contains CD_OCR_ROOT_NODE = \
         {CD_OCR_ROOT_NODE:?}. The synthetic root row draws the working directory's BASENAME, so if \
         the two stop agreeing assertion 6 asserts on a word nothing draws and the smoke reports a \
         missing root node that is in fact present under another name."
    );

    // Assertion 5's non-vacuity: unless the outer directory carries that term, the must-not-contain
    // holds trivially and assertion 6 loses its guard.
    assert!(
        CD_OUTER_DIR.to_lowercase().contains(CD_OCR_UNABBREVIATED),
        "CD_OUTER_DIR = {CD_OUTER_DIR:?} no longer contains CD_OCR_UNABBREVIATED = \
         {CD_OCR_UNABBREVIATED:?}. Assertion 5 is what proves the header-bar title abbreviated the \
         home prefix to `~`; if the directory that would be drawn by an UNABBREVIATED title is not \
         named after that term, assertion 5 holds no matter what the title did — and assertion 6, \
         which it guards, becomes satisfiable by a full-path title instead of by the synthetic root \
         row. A vacuous guard is worse than none, because it reads as coverage."
    );

    // The crossed pair that matters: an unabbreviated title has both terms, so if the outer directory
    // carried the root term, assertion 6 would pass from the title alone.
    assert!(
        !CD_OUTER_DIR.to_lowercase().contains(CD_OCR_ROOT_NODE),
        "CD_OUTER_DIR = {CD_OUTER_DIR:?} contains CD_OCR_ROOT_NODE = {CD_OCR_ROOT_NODE:?}. This is \
         the crossed pair that matters most in this block. Assertion 6's whole claim is that the \
         word came from the SYNTHETIC ROOT ROW; the only other thing that could draw it is a \
         header-bar title that failed to abbreviate and rendered the full path, which is what \
         assertion 5 exists to rule out. With the outer directory named after the root-node term, \
         that ruling-out is defeated in a second way — the term is on the page from the outer \
         segment as well — so a completely broken re-root could still pass assertion 6. Rename the \
         DIRECTORY; do NOT change CD_OCR_ROOT_NODE and do NOT touch the assertion."
    );

    // The `:cd` target carries neither other term: the rooted one passes assertion 4 with no child row,
    // the unabbreviated term fires assertion 5 on a correct run.
    for (term, term_const, why) in [
        (
            CD_OCR_ROOTED,
            "CD_OCR_ROOTED",
            "the root row would satisfy assertion 4 on its own, so a synthetic root that never \
             auto-expanded — its children hidden under a collapsed row — would look exactly like a \
             correct one",
        ),
        (
            CD_OCR_UNABBREVIATED,
            "CD_OCR_UNABBREVIATED",
            "the root row draws this name on every CORRECT run, so assertion 5 would fire even \
             though the title abbreviated exactly as it should",
        ),
    ] {
        assert!(
            !CD_ROOT_DIR.to_lowercase().contains(term),
            "CD_ROOT_DIR = {CD_ROOT_DIR:?} contains {term_const} = {term:?}: {why}. Rename the \
             DIRECTORY; do not change the OCR const and do not touch the assertion."
        );
    }

    // The launch cwd's basename is page text (title, then a `Dir` row), so it avoids every term. The
    // reverse direction is a drift tripwire: a term built from it couples the two.
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !CD_START_DIR.to_lowercase().contains(term),
            "CD_START_DIR = {CD_START_DIR:?} contains {term_const} = {term:?}. It is the typed-`:cd` \
             smoke's LAUNCH working directory, so it is drawn by the header bar before the `:cd` \
             (as `~/{CD_START_DIR}`) and by a sidebar Dir row after it — on the very capture the \
             assertions read. This word would therefore satisfy, or for a must-NOT-contain term \
             spuriously break, `{smoke}`'s OCR assertion with nothing having re-rooted. Rename the \
             DIRECTORY."
        );
        assert!(
            !term.to_lowercase().contains(CD_START_DIR),
            "asserted OCR term {term_const} = {term:?} (owned by `{smoke}`) CONTAINS \
             CD_START_DIR = {CD_START_DIR:?}. The two are supposed to be independent: this \
             directory is inert page text on the typed-`:cd` smoke's capture and no assertion may \
             be derived from it. A term spelled out of this name means a later rename of the \
             directory silently retargets the term, which is exactly the drift the tables in this \
             file exist to prevent."
        );
    }

    // Every OTHER asserted term still applies to the three exempted names, exactly as
    // it does to the Open Recent and sidebar-toggle exemptions above.
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        for (name, name_const, exempt) in [
            (CD_TARGET_FILE, "CD_TARGET_FILE", CD_OCR_ROOTED),
            (CD_ROOT_DIR, "CD_ROOT_DIR", CD_OCR_ROOT_NODE),
            (CD_OUTER_DIR, "CD_OUTER_DIR", CD_OCR_UNABBREVIATED),
        ] {
            if term == exempt {
                continue;
            }
            assert!(
                !name.to_lowercase().contains(term),
                "{name_const} = {name:?} contains {term_const} = {term:?}. All three of these are \
                 page text in the typed-`:cd` smoke's capture — one as a tree-sidebar row, one as \
                 the synthetic root row, and one only if the title's abbreviation regresses — so \
                 this word would satisfy, or for a must-NOT-contain term spuriously break, \
                 `{smoke}`'s OCR assertion. These names are EXEMPT from the \
                 RELAUNCH_FIXTURE_NAMES loop above only for their OWN term ({exempt:?}); every \
                 other asserted term still applies to them."
            );
        }
    }

    // The statusbar exemption: two must-contain terms, since two panes draw two names. The files under
    // `<unique>/sbfixtures/` are outside the search root, so only each strip draws its name.

    // The subjects: a name that stops containing its term makes that assertion vacuous.
    for (name, name_const, term, term_const, which) in [
        (
            STATUS_FILE_A,
            "STATUS_FILE_A",
            STATUS_OCR_A,
            "STATUS_OCR_A",
            "first",
        ),
        (
            STATUS_FILE_B,
            "STATUS_FILE_B",
            STATUS_OCR_B,
            "STATUS_OCR_B",
            "split",
        ),
    ] {
        assert!(
            name.to_lowercase().contains(term),
            "{name_const} = {name:?} no longer contains {term_const} = {term:?}. That FILENAME is \
             what the {which} pane's status bar must draw, and it is the ONLY thing on the page \
             that can: the file lives outside the app's search root, so no sidebar row bears it, \
             and the cmdline carries STATUS_OCR_MARKER by capture time. If the two stop agreeing \
             the smoke asserts on a word nothing draws and reports a missing status bar that is in \
             fact present. This file is exempt from the RELAUNCH_FIXTURE_NAMES loop above for \
             exactly this reason."
        );
    }

    // The crossed pair is the discriminator: under the revert every strip draws the active pane's name,
    // and crossed names would leave the smoke green.
    for (name, name_const, other_term, other_const) in [
        (STATUS_FILE_A, "STATUS_FILE_A", STATUS_OCR_B, "STATUS_OCR_B"),
        (STATUS_FILE_B, "STATUS_FILE_B", STATUS_OCR_A, "STATUS_OCR_A"),
    ] {
        assert!(
            !name.to_lowercase().contains(other_term),
            "{name_const} = {name:?} contains {other_const} = {other_term:?}, which is the OTHER \
             pane's asserted term. The two assertions would then both be satisfiable by a single \
             strip, so the smoke would stop proving that each pane draws its OWN file and would \
             pass green against exactly the build its revert produces (draw_pane reading the \
             ACTIVE pane's cached status instead of the pane it is painting). Rename the FILE; do \
             NOT change either OCR const and do NOT touch the assertions."
        );
    }

    // The marker must not spell any term, nor may the fixture directory, which is drawn in the strip's
    // display path and in the typed `:e` line.
    for (name, name_const, why) in [
        (
            STATUS_FILE_A,
            "STATUS_FILE_A",
            "a filename carrying the marker would satisfy the capture's proof-of-life assertion \
             from the strip it is supposed to be independent of, so a window that had stopped \
             painting the cmdline would read as healthy",
        ),
        (
            STATUS_FILE_B,
            "STATUS_FILE_B",
            "the same: the marker is the one term on that page that must come from somewhere \
             other than a status bar",
        ),
    ] {
        assert!(
            !name.to_lowercase().contains(STATUS_OCR_MARKER),
            "{name_const} = {name:?} contains STATUS_OCR_MARKER = {STATUS_OCR_MARKER:?}: {why}."
        );
    }
    for (term, term_const) in [
        (STATUS_OCR_A, "STATUS_OCR_A"),
        (STATUS_OCR_B, "STATUS_OCR_B"),
        (STATUS_OCR_MARKER, "STATUS_OCR_MARKER"),
    ] {
        assert!(
            !STATUS_FIXTURE_DIR.to_lowercase().contains(term),
            "STATUS_FIXTURE_DIR = {STATUS_FIXTURE_DIR:?} contains {term_const} = {term:?}. The \
             directory is drawn by the strip's SECOND line (the display path) and by the typed `:e` \
             command line, neither of which is the filename line the assertions are about — so this \
             word would satisfy its assertion from a component that says nothing about whether the \
             pane's own file was drawn. Rename the DIRECTORY; do NOT change the OCR const."
        );
    }

    // These two exempt fixtures are opened, so their body renders beside the strips; a body carrying a
    // term would satisfy it from the grid with no strip drawn.
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !STATUS_FIXTURE_BODY.to_lowercase().contains(term),
            "STATUS_FIXTURE_BODY = {STATUS_FIXTURE_BODY:?} contains {term_const} = {term:?}. It is \
             the CONTENT of both files the per-pane statusbar smoke opens, so it is drawn inside \
             the editor grid of both panes: this word would satisfy — or, for a must-NOT-contain \
             term, spuriously break — `{smoke}`'s OCR assertion with neither status bar painted. \
             Keep the body wordless and unlike every asserted term."
        );
    }

    // Every OTHER asserted term still applies to the two exempted filenames, exactly
    // as it does to the three exemptions above.
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        for (name, name_const, exempt) in [
            (STATUS_FILE_A, "STATUS_FILE_A", STATUS_OCR_A),
            (STATUS_FILE_B, "STATUS_FILE_B", STATUS_OCR_B),
        ] {
            if term == exempt {
                continue;
            }
            assert!(
                !name.to_lowercase().contains(term),
                "{name_const} = {name:?} contains {term_const} = {term:?}. It is page text in the \
                 per-pane statusbar smoke's capture, drawn by a status bar, so this word would \
                 satisfy — or, for a must-NOT-contain term, spuriously break — `{smoke}`'s OCR \
                 assertion. This name is EXEMPT from the RELAUNCH_FIXTURE_NAMES loop above only \
                 for its OWN term ({exempt:?}); every other asserted term still applies to it."
            );
        }
    }

    // STATUS_FIXTURE_DIR is one ordinary segment: empty or `.` lands the fixtures in HOME, and `..`
    // climbs into the shared temp dir, reading another run's files.
    let mut status_components = Path::new(STATUS_FIXTURE_DIR).components();
    let status_first = status_components.next();
    let status_extra = status_components.next();
    assert!(
        matches!(status_first, Some(std::path::Component::Normal(_))) && status_extra.is_none(),
        "STATUS_FIXTURE_DIR = {STATUS_FIXTURE_DIR:?} is not a single ordinary path segment (it \
         parses as {status_first:?} followed by {status_extra:?}). The per-pane statusbar smoke \
         joins it onto `smoke_home(cwd)` — the temp cwd's parent, which `cage_command` hands the \
         app as HOME — so the fixtures sit beside the search root rather than inside it. A value \
         that resolves to the parent itself, or that climbs out of it, moves the fixtures \
         somewhere the smoke's whole anti-vacuity argument no longer holds."
    );

    // This body is editor text in the pane whose heading the divider smoke asserts on, so a body
    // line carrying a term would satisfy it without the heading. Checked in both directions.
    let fixture = divider_right_fixture_body();
    let lines: Vec<&str> = fixture.lines().collect();
    assert_eq!(
        lines.len(),
        DIVIDER_BODY_LINE_COUNT + 1,
        "`divider_right_fixture_body` produced {} line(s), expected the heading plus \
         DIVIDER_BODY_LINE_COUNT = {DIVIDER_BODY_LINE_COUNT} body lines. The body's whole job is \
         to fill the narrow pane past its visible row count (measured 2026-08-04 at 11, and 14 \
         before the per-pane chrome took three) so no `~` end-of-buffer column is drawn; a short \
         body puts that column back and tesseract misreads the heading above it (see \
         DIVIDER_BODY_LINE_COUNT for the ablations that isolated it).",
        lines.len()
    );
    assert_eq!(
        lines.first().copied(),
        Some(DIVIDER_HEADING_RIGHT),
        "`divider_right_fixture_body`'s FIRST line must be DIVIDER_HEADING_RIGHT = \
         {DIVIDER_HEADING_RIGHT:?} — that is the line the smoke's OCR assertion reads, and a body \
         line ahead of it would push it down the pane for no stated reason."
    );
    for (index, line) in lines.iter().enumerate().skip(1) {
        let lowercased = line.to_lowercase();
        for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
            assert!(
                !lowercased.contains(term),
                "body line {index} of `divider_right_fixture_body` ({line:?}) contains \
                 {term_const} = {term:?}. That line renders in the SAME pane as \
                 DIVIDER_HEADING_RIGHT, so it would satisfy — or, for a must-NOT-contain term, \
                 spuriously break — `{smoke}`'s OCR assertion with the heading itself unrendered: \
                 the silent false positive that let \
                 `picker_opens_and_loads_selected_file_under_cage` pass while the picker never \
                 opened. The body is PAGE DENSITY and nothing else, so keep it to digits and `x`; \
                 do NOT change the OCR const and do NOT touch the smoke's assertion."
            );
        }
        assert!(
            line.chars().all(|c| c.is_ascii_digit() || c == 'x'),
            "body line {index} of `divider_right_fixture_body` is {line:?}, which is not digits \
             and `x` alone. The loop above only rejects the terms asserted on TODAY; keeping the \
             body wordless is what keeps it safe against a term added tomorrow by a smoke that \
             never looks at this file."
        );
    }

    // The project-search fixture: the smoke's file, line and column assertions derive from this body's
    // shape, so the shape is pinned here.
    let search_fixture = search_target_fixture_body();
    let search_lines: Vec<&str> = search_fixture.lines().collect();

    assert_eq!(
        search_lines.len(),
        PROJSEARCH_BODY_LINE_COUNT + 1,
        "`search_target_fixture_body` produced {} line(s), expected the heading plus \
         PROJSEARCH_BODY_LINE_COUNT = {PROJSEARCH_BODY_LINE_COUNT} body lines. The body fills the \
         pane past its visible row count (measured 2026-08-04 at 11, and 14 before the per-pane \
         chrome took three) so no `~` end-of-buffer column is drawn beside the heading the smoke \
         OCRs (see DIVIDER_BODY_LINE_COUNT for the ablations that isolated that hazard), and it \
         must be long enough to hold PROJSEARCH_MATCH_LINE = {PROJSEARCH_MATCH_LINE} at all.",
        search_lines.len()
    );
    assert_eq!(
        search_lines.first().copied(),
        Some(PROJSEARCH_HEADING_TARGET),
        "`search_target_fixture_body`'s FIRST line must be PROJSEARCH_HEADING_TARGET = \
         {PROJSEARCH_HEADING_TARGET:?} — that is the line the smoke's must-contain OCR assertion \
         reads, and a body line ahead of it would push it down the pane for no stated reason."
    );

    // Exactly one line matches, or the activated row becomes a ranking question. Compared lowercased,
    // because `search_options_for` leaves `case_sensitive` false and rg runs with `-i`.
    let matching_lines: Vec<usize> = search_lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.to_lowercase().contains(PROJSEARCH_QUERY))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        matching_lines.as_slice(),
        [PROJSEARCH_MATCH_LINE - 1].as_slice(),
        "the lines of `search_target_fixture_body` containing PROJSEARCH_QUERY = \
         {PROJSEARCH_QUERY:?} (compared case-insensitively, as the search itself does) are at \
         indices {matching_lines:?}, expected exactly one at index PROJSEARCH_MATCH_LINE - 1 = {}. \
         TWO matches make the result list hold two rows, so which one the second Enter activates \
         depends on the projection's ordering rather than on the fixture, and the smoke's line and \
         column assertions would be measuring whichever row won. ZERO matches make the list empty, \
         the second Enter a no-op, and all three location assertions report the file the smoke \
         started in.",
        PROJSEARCH_MATCH_LINE - 1
    );

    let match_index = matching_lines[0];
    let match_line = search_lines[match_index];
    let match_offset = match_line
        .to_lowercase()
        .find(PROJSEARCH_QUERY)
        .unwrap_or_else(|| {
            panic!(
                "the match line {match_line:?} does not contain PROJSEARCH_QUERY = \
             {PROJSEARCH_QUERY:?} after all, which contradicts the assertion above. Both look at \
             the LOWERCASED line, so the one way they can disagree is a PROJSEARCH_QUERY that is \
             not itself lowercase — in which case fix the const, not this scan."
            )
        });

    assert_eq!(
        match_offset,
        PROJSEARCH_MATCH_PREFIX.len(),
        "PROJSEARCH_QUERY starts at byte offset {match_offset} of the match line {match_line:?}, \
         but PROJSEARCH_MATCH_PREFIX is {} byte(s) long. PROJSEARCH_EXPECTED_COL is derived from \
         that length, so a disagreement makes the smoke's `col('.')` assertion expect a column the \
         cursor will never be at — and the failure would read as a bug in `open_at`'s column \
         handling rather than as a fixture that drifted.",
        PROJSEARCH_MATCH_PREFIX.len()
    );
    assert!(
        match_index > 0,
        "the match sits on buffer line {}, but it must be below line 1. `:edit` alone leaves the \
         cursor on line 1, so with the match there the smoke's `line('.')` readback could not tell \
         `WolfCore::open_at` apart from a plain file open — assertion 4 would pass for a build \
         that never placed the cursor at all.",
        match_index + 1
    );
    assert!(
        match_offset > 0,
        "the match starts at byte offset 0 of its line, i.e. PROJSEARCH_MATCH_PREFIX is empty. A \
         plain file open also leaves the cursor in column 1, so the smoke's `col('.')` readback \
         could not tell the two apart and assertion 5 would stop discriminating."
    );

    // The query must be findable in exactly one FILE as well as on exactly one line,
    // so the two other things in the cwd that rg reads are checked here.
    for (body, body_const) in [
        (SIDEBAR_FILLER_BODY, "SIDEBAR_FILLER_BODY"),
        (PROJSEARCH_HEADING_START, "PROJSEARCH_HEADING_START"),
    ] {
        assert!(
            !body.to_lowercase().contains(PROJSEARCH_QUERY),
            "{body_const} = {body:?} contains PROJSEARCH_QUERY = {PROJSEARCH_QUERY:?}. That is \
             file CONTENT in the project-search smokes' working directory, so ripgrep would \
             return it as a second match: the result list would hold two rows, and which one the \
             second Enter activates would stop being a property of the fixture."
        );
    }

    // rg does not search names, but the tree and the panel's file row draw them, so a name carrying the
    // query makes a post-mortem PNG ambiguous about which row was selected.
    for name in SIDEBAR_FILLER_FILES {
        assert!(
            !name.to_lowercase().contains(PROJSEARCH_QUERY),
            "sidebar filler {name:?} contains PROJSEARCH_QUERY = {PROJSEARCH_QUERY:?}. The name is \
             drawn in the sidebar and would be drawn in a result row if that file ever gained \
             matching content, so a reader of a failing capture could not tell the query's real \
             match from a filename that merely looks like one."
        );
    }
    for &(name, name_const, owner) in RELAUNCH_FIXTURE_NAMES {
        assert!(
            !name.to_lowercase().contains(PROJSEARCH_QUERY),
            "fixture {name_const} = {name:?} (owned by `{owner}`) contains PROJSEARCH_QUERY = \
             {PROJSEARCH_QUERY:?}, for the same reason the filler names may not: the name is drawn \
             and would make a failing capture ambiguous about which row the query selected."
        );
    }

    // The query is drawn in the capture, so a term in it would satisfy or break an assertion without
    // the search doing anything.
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !PROJSEARCH_QUERY.contains(term),
            "PROJSEARCH_QUERY = {PROJSEARCH_QUERY:?} contains {term_const} = {term:?}. The query \
             is typed into the entry AND is content of the matched line, so it is on screen in the \
             capture: `{smoke}`'s OCR assertion would then be satisfied — or, for a \
             must-NOT-contain term, spuriously broken — by the fixture text alone. Choose a query \
             that shares no substring with any asserted term; do NOT change the OCR const."
        );
    }

    // The crossed pair the project-search headings rest on, the TITLE_* exemption's shape.
    assert!(
        PROJSEARCH_HEADING_TARGET
            .to_lowercase()
            .contains(PROJSEARCH_OCR_PRESENT),
        "PROJSEARCH_HEADING_TARGET = {PROJSEARCH_HEADING_TARGET:?} does not contain \
         PROJSEARCH_OCR_PRESENT = {PROJSEARCH_OCR_PRESENT:?}. That heading is the only place the \
         term exists, so if the two stop agreeing the smoke asserts on a word nothing draws and \
         fails with no visible cause."
    );
    assert!(
        !PROJSEARCH_HEADING_TARGET
            .to_lowercase()
            .contains(PROJSEARCH_OCR_ABSENT),
        "PROJSEARCH_HEADING_TARGET = {PROJSEARCH_HEADING_TARGET:?} contains PROJSEARCH_OCR_ABSENT \
         = {PROJSEARCH_OCR_ABSENT:?}. The target's heading is exactly what a SUCCESSFUL jump puts \
         on screen, so the must-not-contain assertion would fire on a perfectly correct run."
    );
    assert!(
        PROJSEARCH_HEADING_START
            .to_lowercase()
            .contains(PROJSEARCH_OCR_ABSENT),
        "PROJSEARCH_HEADING_START = {PROJSEARCH_HEADING_START:?} does not contain \
         PROJSEARCH_OCR_ABSENT = {PROJSEARCH_OCR_ABSENT:?}. The must-not-contain assertion is what \
         names WHICH file is on screen when the jump fails; if the starting file is not the one \
         carrying that term, the assertion holds trivially and proves nothing — a vacuous check, \
         not a passing one."
    );
    assert!(
        !PROJSEARCH_HEADING_START
            .to_lowercase()
            .contains(PROJSEARCH_OCR_PRESENT),
        "PROJSEARCH_HEADING_START = {PROJSEARCH_HEADING_START:?} contains PROJSEARCH_OCR_PRESENT = \
         {PROJSEARCH_OCR_PRESENT:?}. The starting file is on screen before the chord ever fires, \
         so the must-contain assertion would be satisfied by a build in which the search never \
         opened anything at all."
    );

    // Every body line but the heading and the match line is density; the match line is checked below.
    for (index, line) in search_lines.iter().enumerate().skip(1) {
        if index == match_index {
            continue;
        }
        let lowercased = line.to_lowercase();
        for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
            assert!(
                !lowercased.contains(term),
                "body line {index} of `search_target_fixture_body` ({line:?}) contains \
                 {term_const} = {term:?}. That line renders in the SAME pane as \
                 PROJSEARCH_HEADING_TARGET, so it would satisfy — or, for a must-NOT-contain term, \
                 spuriously break — `{smoke}`'s OCR assertion with the heading itself unrendered."
            );
        }
        assert!(
            line.chars().all(|c| c.is_ascii_digit() || c == 'x'),
            "body line {index} of `search_target_fixture_body` is {line:?}, which is not digits \
             and `x` alone. The loop above only rejects the terms asserted on TODAY; keeping the \
             filler wordless is what keeps it safe against a term added tomorrow by a smoke that \
             never looks at this file. Only the match line is exempt, and only because the query \
             has to be a word."
        );
    }

    let lowercased_match = match_line.to_lowercase();
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        if term == PROJSEARCH_OCR_RESULT {
            continue;
        }
        assert!(
            !lowercased_match.contains(term),
            "the match line {match_line:?} contains {term_const} = {term:?}. It is drawn in the \
             search panel's line row and, once the jump lands, in the same pane as \
             PROJSEARCH_HEADING_TARGET, so it would satisfy — or spuriously break — `{smoke}`'s \
             OCR assertion on its own. It is exempt from the digits-and-`x` rule above because \
             the query and the result word must be words, and from this rule only for \
             PROJSEARCH_OCR_RESULT itself."
        );
    }

    // The result word is the panel smokes' subject: the match line's last word, after the query.
    let result_tail = format!("{PROJSEARCH_QUERY} {PROJSEARCH_OCR_RESULT}");
    assert!(
        lowercased_match.ends_with(&result_tail),
        "the match line {match_line:?} does not end with {result_tail:?}. PROJSEARCH_OCR_RESULT \
         must be ON the match line, or no result row draws it and both panel smokes fail their \
         first sidebar assertion with no visible cause; and it must sit right after the query, \
         because `context_window` centres a long line on the match, so a word far from it is \
         the first thing a narrow panel trims away."
    );
    assert_eq!(
        search_fixture.matches(PROJSEARCH_OCR_RESULT).count(),
        1,
        "`search_target_fixture_body` must carry PROJSEARCH_OCR_RESULT = \
         {PROJSEARCH_OCR_RESULT:?} exactly once, on the match line. Anywhere else in the target \
         it changes nothing the smokes read before the jump, but it would make the word stop \
         meaning \"this row is a search result\" to whoever reads a failing capture B."
    );

    // The `all_files: true` premise: core's default search globs only Markdown, so a Markdown target
    // would be found either way and the smoke would not discriminate that flag.
    let target_extension = Path::new(PROJSEARCH_FILE_TARGET)
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    assert!(
        matches!(target_extension.as_deref(), Some(ext) if ext != "md" && ext != "markdown"),
        "PROJSEARCH_FILE_TARGET = {PROJSEARCH_FILE_TARGET:?} has extension {target_extension:?}, \
         which must be present and must be neither `md` nor `markdown`. Core's DEFAULT \
         SearchOptions restricts the search to those two globs, so a Markdown target would be \
         searched whatever `search_panel::search_options_for` set `all_files` to — and an \
         `all_files: false` build, which leaves the panel empty and reddens both panel smokes' \
         first sidebar assertion, would stop reddening anything."
    );

    // The panel smokes' tree-row exemption: PROJSEARCH_FILE_START's NAME is the subject, so it is
    // out of RELAUNCH_FIXTURE_NAMES and every check that loop made is made here instead.
    assert!(
        PROJSEARCH_FILE_START
            .to_lowercase()
            .contains(PROJSEARCH_OCR_TREE_ROW),
        "PROJSEARCH_FILE_START = {PROJSEARCH_FILE_START:?} no longer contains \
         PROJSEARCH_OCR_TREE_ROW = {PROJSEARCH_OCR_TREE_ROW:?}. That file's tree row is the only \
         thing in the sidebar band that draws the term. If the two stop agreeing, the panel \
         smoke's must-not-contain assertion holds whatever page is up, and the reveal smoke's \
         switch-back assertion fails on a word nothing draws."
    );
    for (name, name_const) in [
        (PROJSEARCH_FILE_TARGET, "PROJSEARCH_FILE_TARGET"),
        (PROJSEARCH_QUERY, "PROJSEARCH_QUERY"),
        (match_line, "the match line"),
        (PROJSEARCH_OCR_RESULT, "PROJSEARCH_OCR_RESULT"),
    ] {
        assert!(
            !name.to_lowercase().contains(PROJSEARCH_OCR_TREE_ROW),
            "{name_const} = {name:?} contains PROJSEARCH_OCR_TREE_ROW = \
             {PROJSEARCH_OCR_TREE_ROW:?}. This is the crossed pair that matters: the search panel \
             draws the matched file's name, the query and the match line, so the Search page \
             itself would draw the tree-only term. The panel smoke's must-not-contain assertion \
             would then fail on a correct run, and the reveal smoke's switch-back assertion \
             would pass with the panel still showing."
        );
    }
    assert!(
        !PROJSEARCH_OCR_TREE_ROW.contains(PROJSEARCH_OCR_RESULT),
        "PROJSEARCH_OCR_TREE_ROW = {PROJSEARCH_OCR_TREE_ROW:?} contains PROJSEARCH_OCR_RESULT = \
         {PROJSEARCH_OCR_RESULT:?}, so the file tree alone would satisfy every assertion that the \
         results reached the sidebar."
    );
    assert!(
        !PROJSEARCH_HEADING_START
            .to_lowercase()
            .contains(PROJSEARCH_OCR_RESULT),
        "PROJSEARCH_HEADING_START = {PROJSEARCH_HEADING_START:?} contains PROJSEARCH_OCR_RESULT = \
         {PROJSEARCH_OCR_RESULT:?}. The starting file is on screen when capture A is taken, so \
         the panel smoke's no-overlay assertion would fail on a correct run, and with the \
         sidebar hidden the reveal smoke's band would read the word from the editor."
    );
    assert!(
        !PROJSEARCH_FILE_START
            .to_lowercase()
            .contains(PROJSEARCH_QUERY),
        "PROJSEARCH_FILE_START = {PROJSEARCH_FILE_START:?} contains PROJSEARCH_QUERY = \
         {PROJSEARCH_QUERY:?}, for the reason the other fixture names may not: the name is drawn \
         and would make a failing capture ambiguous about which row the query selected."
    );
    assert!(
        !RELAUNCH_FIXTURE_NAMES
            .iter()
            .any(|&(name, _, _)| name == PROJSEARCH_FILE_START)
            && !SIDEBAR_FILLER_FILES.contains(&PROJSEARCH_FILE_START),
        "PROJSEARCH_FILE_START = {PROJSEARCH_FILE_START:?} collides with a RELAUNCH_FIXTURE_NAMES \
         or SIDEBAR_FILLER_FILES entry. Both are written into the same temp cwd, so one would \
         clobber the other."
    );
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        if term == PROJSEARCH_OCR_TREE_ROW {
            continue;
        }
        assert!(
            !PROJSEARCH_FILE_START.to_lowercase().contains(term),
            "PROJSEARCH_FILE_START = {PROJSEARCH_FILE_START:?} contains {term_const} = {term:?}. \
             It is page text as a tree row in every smoke that writes it, so this word would \
             satisfy — or, for a must-NOT-contain term, spuriously break — `{smoke}`'s OCR \
             assertion. This name is EXEMPT from the RELAUNCH_FIXTURE_NAMES loop above only for \
             PROJSEARCH_OCR_TREE_ROW itself; every other asserted term still applies to it."
        );
    }

    // This body reaches pixels through WebKit. `preview` is an ordinary word, so it must
    // appear on the page only in the preview pane's statusbar, never in the rendered document.
    let preview_body = format!("{PREVIEW_SOURCE_HEADING}\n{PREVIEW_SOURCE_PARAGRAPH}");
    for (label, text) in [
        ("PREVIEW_SOURCE_HEADING", PREVIEW_SOURCE_HEADING),
        ("PREVIEW_SOURCE_PARAGRAPH", PREVIEW_SOURCE_PARAGRAPH),
        ("PREVIEW_OTHER_HEADING", PREVIEW_OTHER_HEADING),
        ("PREVIEW_LIVE_MARKER", PREVIEW_LIVE_MARKER),
    ] {
        assert!(
            !text.to_lowercase().contains(PREVIEW_OCR_STATUS),
            "{label} = {text:?} contains PREVIEW_OCR_STATUS = {PREVIEW_OCR_STATUS:?}. That word is \
             asserted to be on the page BECAUSE a preview pane's statusbar draws it, and it is an \
             ordinary English word rather than a coined one — so any rendered text spelling it \
             satisfies `preview_pane_renders_and_edit_source_returns_under_cage`'s statusbar \
             assertion with `refresh_pane_state` taking `core.pane_status` for the preview pane, \
             which is precisely the revert that assertion exists to catch. Reword the fixture; do \
             NOT change PREVIEW_OCR_STATUS and do NOT touch the assertion."
        );
    }
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        // The loop above checks the status word with its own message; any other term is a must-not-appear.
        if term == PREVIEW_OCR_STATUS {
            continue;
        }
        assert!(
            !PREVIEW_SOURCE_PARAGRAPH.to_lowercase().contains(term),
            "PREVIEW_SOURCE_PARAGRAPH = {PREVIEW_SOURCE_PARAGRAPH:?} contains {term_const} = \
             {term:?}. It is rendered in the SAME pane as PREVIEW_SOURCE_HEADING, so it would \
             satisfy — or, for a must-NOT-contain term, spuriously break — `{smoke}`'s OCR \
             assertion with the heading itself unrendered. The paragraph is page density for the \
             web view and nothing else; reword it."
        );
    }
    assert!(
        preview_body.to_lowercase().contains(PREVIEW_OCR_BODY),
        "the markdown-preview source body {preview_body:?} no longer contains PREVIEW_OCR_BODY = \
         {PREVIEW_OCR_BODY:?}. That heading word is the ONLY thing proving the web view rendered \
         anything: if the fixture stops carrying it the assertion fails with no visible cause, and \
         the same term is what capture B reads to tell a frozen preview apart from a blank one."
    );
    assert!(
        PREVIEW_OTHER_HEADING
            .to_lowercase()
            .contains(PREVIEW_OCR_OTHER),
        "PREVIEW_OTHER_HEADING = {PREVIEW_OTHER_HEADING:?} no longer contains PREVIEW_OCR_OTHER = \
         {PREVIEW_OCR_OTHER:?}. The editor pane drawing that heading is capture A's proof of life; \
         if the fixture stops carrying it the smoke fails at its own precondition and says nothing \
         about the preview."
    );
    // The crossed pair: the headings come from different renderers, so a heading carrying the other's
    // term lets the editor pane satisfy a web-view assertion with no preview rendered.
    for (label, text, forbidden, forbidden_const) in [
        (
            "PREVIEW_OTHER_HEADING",
            PREVIEW_OTHER_HEADING,
            PREVIEW_OCR_BODY,
            "PREVIEW_OCR_BODY",
        ),
        (
            "PREVIEW_OTHER_HEADING",
            PREVIEW_OTHER_HEADING,
            PREVIEW_OCR_LIVE,
            "PREVIEW_OCR_LIVE",
        ),
        (
            "PREVIEW_SOURCE_HEADING",
            PREVIEW_SOURCE_HEADING,
            PREVIEW_OCR_OTHER,
            "PREVIEW_OCR_OTHER",
        ),
    ] {
        assert!(
            !text.to_lowercase().contains(forbidden),
            "{label} = {text:?} contains {forbidden_const} = {forbidden:?}. That is the crossed \
             pair: the editor pane and the preview pane must be readable apart, or an OCR hit \
             attributed to WebKit could have come from the grid beside it — and the reverse, an \
             editor-pane term satisfied by the rendered document."
        );
    }
    assert!(
        PREVIEW_LIVE_MARKER
            .to_lowercase()
            .contains(PREVIEW_OCR_LIVE),
        "PREVIEW_LIVE_MARKER = {PREVIEW_LIVE_MARKER:?} no longer contains PREVIEW_OCR_LIVE = \
         {PREVIEW_OCR_LIVE:?}. The typed marker and the asserted term are two halves of one fact — \
         that the body swap reached the DOM — and a marker that does not spell the term makes that \
         assertion unfalsifiable."
    );
    assert!(
        !preview_body.to_lowercase().contains(PREVIEW_OCR_LIVE),
        "the markdown-preview source body {preview_body:?} already contains PREVIEW_OCR_LIVE = \
         {PREVIEW_OCR_LIVE:?}, so the FIRST load into the web view would render the live-edit \
         marker. Capture B's discriminator would then hold whether or not a single keystroke ever \
         reached the DOM — the vacuity this whole guard exists for, aimed at the one assertion in \
         this binary that covers `preview_swap_payload`."
    );

    // The unsaved-buffer marker is in no file, so the loop above cannot reach it; this checks the cover
    // file's content and the crossing between the editor's term and WebKit's.
    assert!(
        UNSAVED_MARKER.to_lowercase().contains(UNSAVED_OCR_MARKER),
        "UNSAVED_MARKER = {UNSAVED_MARKER:?} no longer contains UNSAVED_OCR_MARKER = \
         {UNSAVED_OCR_MARKER:?}. The typed marker and the asserted term are two halves of one \
         fact — that the web view rendered a document held only in an unsaved buffer — and a \
         marker that does not spell the term makes that assertion unfalsifiable."
    );
    assert!(
        UNSAVED_COVER_HEADING
            .to_lowercase()
            .contains(UNSAVED_OCR_COVER),
        "UNSAVED_COVER_HEADING = {UNSAVED_COVER_HEADING:?} no longer contains UNSAVED_OCR_COVER = \
         {UNSAVED_OCR_COVER:?}. The editor pane drawing that heading is the smoke's proof of life \
         AND its proof that the `:e` took the marker's raw text off screen; if the fixture stops \
         carrying it, the smoke fails at its own precondition and says nothing about the preview."
    );
    assert!(
        !UNSAVED_COVER_HEADING
            .to_lowercase()
            .contains(UNSAVED_OCR_MARKER),
        "UNSAVED_COVER_HEADING = {UNSAVED_COVER_HEADING:?} contains UNSAVED_OCR_MARKER = \
         {UNSAVED_OCR_MARKER:?}. That is the CROSSED pair: the cover file is drawn by the EDITOR \
         pane, so a heading carrying the marker would satisfy the web-view assertion from the grid \
         and the smoke would pass against a build whose preview never rendered at all."
    );
    assert!(
        !UNSAVED_MARKER.to_lowercase().contains(UNSAVED_OCR_COVER),
        "UNSAVED_MARKER = {UNSAVED_MARKER:?} contains UNSAVED_OCR_COVER = {UNSAVED_OCR_COVER:?}. \
         The other half of the crossing: the rendered marker would then satisfy the proof-of-life \
         precondition on its own, so a `:e` that never landed — leaving the marker's raw text in \
         the editor pane as well — would look like a clean run."
    );
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        if term == UNSAVED_OCR_COVER {
            continue;
        }
        assert!(
            !UNSAVED_COVER_HEADING.to_lowercase().contains(term),
            "UNSAVED_COVER_HEADING = {UNSAVED_COVER_HEADING:?} contains {term_const} = {term:?}. \
             It is RENDERED text in that smoke's only capture, so it would satisfy — or, for a \
             must-NOT-contain term, spuriously break — `{smoke}`'s OCR assertion. It is exempt \
             from this loop only for its OWN term; reword the heading."
        );
    }
    let unsaved_name = UNSAVED_BUFFER_NAME.to_lowercase();
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        assert!(
            !unsaved_name.contains(term),
            "UNSAVED_BUFFER_NAME = {UNSAVED_BUFFER_NAME:?} contains {term_const} = {term:?}. That \
             name is page text twice over on the capture the marker assertion reads — once as the \
             preview pane's statusbar filename and once in the readback path echoed in the cmdline \
             strip — so it would satisfy, or for a must-NOT-contain term spuriously break, \
             `{smoke}`'s OCR assertion with the document never rendered. It is NOT a fixture file \
             (the buffer is never written to disk), so the RELAUNCH_FIXTURE_NAMES loop above \
             cannot reach it and this is the only place the rule is enforced for it. The name \
             comes from `runtime/nvim/init.lua`'s naming rule rather than from this binary, so a \
             failure here means an ASSERTED TERM has to change, not the name."
        );
    }

    // The drawer terms cross: a fill line with the marker passes the must-contain from the editor, no
    // drawer, and a marker with the fill term fires the must-not-contain on a correct run.
    let drawer_fixture = drawer_fixture_body();
    let drawer_lines: Vec<&str> = drawer_fixture.lines().collect();
    assert_eq!(
        drawer_lines.len(),
        DRAWER_FIXTURE_LINE_COUNT,
        "`drawer_fixture_body` produced {} line(s), expected DRAWER_FIXTURE_LINE_COUNT = \
         {DRAWER_FIXTURE_LINE_COUNT}. The line count is what puts the fill term inside the sampled \
         band's ROWS when the drawer is not painted (buffer lines 10 and 11 at SMOKE_FONT_PT), so \
         a shorter body makes the band's must-NOT-contain assertion hold whether the drawer is on \
         screen or not — a vacuous guard, which reads as coverage.",
        drawer_lines.len()
    );
    for (index, line) in drawer_lines.iter().enumerate() {
        assert!(
            line.to_lowercase().contains(DRAWER_OCR_EDITOR_ABSENT),
            "line {index} of `drawer_fixture_body` ({line:?}) does not contain \
             DRAWER_OCR_EDITOR_ABSENT = {DRAWER_OCR_EDITOR_ABSENT:?}. Every line has to carry it: \
             which buffer rows land inside the sampled band depends on the pane chrome, so a body \
             that carried the term on only some lines could leave the band's rows wordless and the \
             must-NOT-contain assertion would then hold with the drawer unpainted."
        );
    }
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        if term == DRAWER_OCR_EDITOR_ABSENT {
            continue;
        }
        assert!(
            !drawer_fixture.to_lowercase().contains(term),
            "`drawer_fixture_body` contains {term_const} = {term:?}. It is EDITOR text in the \
             terminal-drawer smoke's capture, so it would satisfy — or, for a must-NOT-contain \
             term, spuriously break — `{smoke}`'s OCR assertion. The sharpest case is \
             DRAWER_OCR_MARKER itself: the marker is asserted PRESENT in the sampled band on the \
             grounds that only the drawer's shell can draw it, and a fixture line carrying it would satisfy that \
             from the editor pane with no drawer on screen at all."
        );
    }
    assert!(
        !DRAWER_MARKER
            .to_lowercase()
            .contains(DRAWER_OCR_EDITOR_ABSENT),
        "DRAWER_MARKER = {DRAWER_MARKER:?} contains DRAWER_OCR_EDITOR_ABSENT = \
         {DRAWER_OCR_EDITOR_ABSENT:?}. The other half of the crossing: the drawer's own marker \
         would then fire the band's must-NOT-contain assertion on a perfectly correct run, so the \
         two assertions would contradict each other and no build could pass both."
    );
    assert!(
        DRAWER_MARKER.to_lowercase().contains(DRAWER_OCR_MARKER),
        "DRAWER_MARKER = {DRAWER_MARKER:?} no longer contains DRAWER_OCR_MARKER = \
         {DRAWER_OCR_MARKER:?}. The typed command and the asserted term are two halves of one fact \
         — that the drawer's shell wrote to a terminal this front-end painted — and a marker that \
         does not spell the term makes that assertion unfalsifiable."
    );

    // The neighbour-pane body: the term on every line, so any editor row a leg shows can answer it,
    // and no other smoke's term, which those captures would then satisfy or break.
    let near_fixture = near_fixture_body();
    let near_lines: Vec<&str> = near_fixture.lines().collect();
    assert_eq!(
        near_lines.len(),
        NEAR_FIXTURE_LINE_COUNT,
        "`near_fixture_body` produced {} line(s), expected NEAR_FIXTURE_LINE_COUNT = \
         {NEAR_FIXTURE_LINE_COUNT}. Fewer lines than the editor has rows leave rows with no term, \
         and a leg whose capture shows only those would read as an unpainted editor.",
        near_lines.len()
    );
    for (index, line) in near_lines.iter().enumerate() {
        assert!(
            line.to_lowercase().contains(NEAR_OCR_FILL),
            "line {index} of `near_fixture_body` ({line:?}) does not contain NEAR_OCR_FILL = \
             {NEAR_OCR_FILL:?}. Every line must carry it: which rows a leg shows depends on the \
             drawer, the split and the cursor, and the neighbour-pane smoke asserts the term in \
             every capture."
        );
    }
    for &(term, term_const, smoke) in ASSERTED_OCR_TERMS {
        if term == NEAR_OCR_FILL {
            continue;
        }
        assert!(
            !near_fixture.to_lowercase().contains(term),
            "`near_fixture_body` contains {term_const} = {term:?}. It is EDITOR text in every \
             capture of the neighbour-pane smoke, so it would satisfy, or for a must-NOT-contain \
             term spuriously break, `{smoke}`'s OCR assertion if that smoke ever drew it."
        );
    }

    // The two-window exemption: fixtures named after the compared strings, safe as it reads no image.
    // See notes/crates/wolf-gnome/tests/smoke_headless/restore.rs.md#two-window-smoke-fixtures
    assert_eq!(
        TWOWIN_SECOND_FILE,
        format!("{TWOWIN_SUBDIR}/{TWOWIN_SECOND_NAME}"),
        "TWOWIN_SECOND_FILE = {TWOWIN_SECOND_FILE:?} is not TWOWIN_SUBDIR = {TWOWIN_SUBDIR:?} \
         joined to TWOWIN_SECOND_NAME = {TWOWIN_SECOND_NAME:?}. The three are handed to the \
         scenario separately — the joined path is what the fixture is WRITTEN at, the directory \
         is what window B `:cd`s into, and the bare name is what it types at `:e` — so a \
         disagreement puts the file somewhere the `:e` will not find it. The smoke would then \
         persist a layout naming a buffer that was never loaded, and the failure would read as a \
         restore bug."
    );
    assert!(
        !TWOWIN_FIRST_FILE.contains('/'),
        "TWOWIN_FIRST_FILE = {TWOWIN_FIRST_FILE:?} carries a path separator. It is written \
         directly into the temp cwd and typed at `:e` while window A is rooted there, so it must \
         be a bare filename; a path would make the `:e` resolve somewhere else and window A would \
         persist a leaf naming a file that does not exist."
    );
    assert_ne!(
        TWOWIN_FIRST_FILE, TWOWIN_SECOND_NAME,
        "TWOWIN_FIRST_FILE and TWOWIN_SECOND_NAME are the same name ({TWOWIN_FIRST_FILE:?}). The \
         smoke asserts that the saved leaf of window 0 names the first and the leaf of window 1 \
         names the second, so with one name both assertions are satisfied by either window and \
         \"the two sessions were saved in order\" stops being falsifiable — a build that wrote one \
         window's subtree twice would pass. The comparison is on the whole final path component, \
         which is also why a name that merely ENDS with the other's would not slip through: it is \
         equality, not a suffix test."
    );
    assert_ne!(
        TWOWIN_SUBDIR, TEMP_CWD_BASENAME,
        "TWOWIN_SUBDIR and TEMP_CWD_BASENAME are the same name ({TWOWIN_SUBDIR:?}). The scenario's \
         exit-24 check keys on the FINAL COMPONENT of the restored window's `getcwd()` to decide \
         which of the two saved sessions it is looking at, and the Rust side compares the two \
         saved `cwd` values the same way. With one name the two projects are indistinguishable by \
         that key: a window restored at the wrong one of them would satisfy the check, and a \
         crossed restore — each window handed the other's plan — would pass."
    );
    assert!(
        !TWOWIN_SUBDIR.is_empty() && !TWOWIN_SUBDIR.contains('/'),
        "TWOWIN_SUBDIR = {TWOWIN_SUBDIR:?} must be a single non-empty path component: it is used \
         both as a directory to create under the temp cwd and as the basename the restored `cwd` \
         is matched against. An empty one puts window B's project at the temp cwd itself, which \
         is the collision the fixture layout exists to avoid (two windows on one project share \
         the durable swap directory and meet Neovim's W325)."
    );

    // A copy of `palette::tests::NEW_WINDOW_PALETTE_QUERY`; this pins its shape, since a stray space or
    // capital changes `rank_matches`' terms and surfaces only as an exit 21 much later.
    let query_terms: Vec<&str> = TWOWIN_NEW_WINDOW_QUERY.split(' ').collect();
    assert_eq!(
        query_terms.len(),
        2,
        "TWOWIN_NEW_WINDOW_QUERY = {TWOWIN_NEW_WINDOW_QUERY:?} splits on a single ASCII space into \
         {} term(s), not 2. `rank_matches` requires EVERY whitespace-separated term to be a \
         subsequence of a row's label, so a doubled or trailing space adds an empty term and a \
         missing one collapses the query back to something `New` also matches. The two-term shape \
         is the whole mechanism: `window` is not a subsequence of `New`, which is what drops the \
         row `PALETTE_QUERY` selects instead of merely out-ranking it.",
        query_terms.len()
    );
    for term in &query_terms {
        assert!(
            !term.is_empty() && term.chars().all(|c| c.is_ascii_lowercase()),
            "term {term:?} of TWOWIN_NEW_WINDOW_QUERY = {TWOWIN_NEW_WINDOW_QUERY:?} is empty or \
             carries a character that is not a lowercase ASCII letter. The literal is carried \
             verbatim from `palette::tests::NEW_WINDOW_PALETTE_QUERY`, which is two lowercase \
             words separated by one space; anything else is a copy that drifted, and the unit test \
             that pins the query's meaning would keep passing while this smoke typed something \
             else."
        );
    }

    // The native-tabs crossed pair: if either heading carries the other's term, both must-not-contain
    // halves hold on a page showing both panes, so "one page on screen" becomes vacuous.
    for (heading, heading_const, term, term_const, which) in [
        (
            TABS_FIRST_HEADING,
            "TABS_FIRST_HEADING",
            TABS_OCR_FIRST,
            "TABS_OCR_FIRST",
            "first",
        ),
        (
            TABS_SECOND_HEADING,
            "TABS_SECOND_HEADING",
            TABS_OCR_SECOND,
            "TABS_OCR_SECOND",
            "second",
        ),
    ] {
        assert!(
            heading.to_lowercase().contains(term),
            "{heading_const} = {heading:?} no longer contains {term_const} = {term:?}. That \
             heading is the ONLY thing on the page that can draw the term — the {which} tab's \
             editor grid is its sole source, since no filename carries it — so if the two stop \
             agreeing the smoke asserts on a word nothing draws, and its two must-NOT-contain \
             halves hold permanently while proving nothing."
        );
    }
    for (heading, heading_const, other_term, other_const) in [
        (
            TABS_FIRST_HEADING,
            "TABS_FIRST_HEADING",
            TABS_OCR_SECOND,
            "TABS_OCR_SECOND",
        ),
        (
            TABS_SECOND_HEADING,
            "TABS_SECOND_HEADING",
            TABS_OCR_FIRST,
            "TABS_OCR_FIRST",
        ),
    ] {
        assert!(
            !heading.to_lowercase().contains(other_term),
            "{heading_const} = {heading:?} contains {other_const} = {other_term:?}, which is the \
             OTHER tab's asserted term. One capture would then satisfy both directions at once and \
             the smoke would stop proving that only ONE page's pane view is drawn — it would pass \
             green against exactly the build its structural revert produces, with both tabs' \
             widgets visible in one container. Rename the HEADING; do NOT change either OCR const \
             and do NOT touch the assertions."
        );
    }
    // The name half of the same crossing, stated here as well, since fixtures do leave the registration
    // table under documented exemptions.
    for (name, name_const, other_term, other_const) in [
        (
            TABS_FIRST_FILE,
            "TABS_FIRST_FILE",
            TABS_OCR_SECOND,
            "TABS_OCR_SECOND",
        ),
        (
            TABS_SECOND_FILE,
            "TABS_SECOND_FILE",
            TABS_OCR_FIRST,
            "TABS_OCR_FIRST",
        ),
    ] {
        assert!(
            !name.to_lowercase().contains(other_term),
            "{name_const} = {name:?} contains {other_const} = {other_term:?}. The `adw::TabBar` \
             draws BOTH tab titles — and a tab's title IS its file's name — in the very captures \
             whose discriminating half is a must-NOT-contain, so this term would be on the page \
             from the bar alone with the other page's pane view never drawn. That is this smoke's \
             own reason for asserting on headings rather than on titles, and it only holds while \
             the names stay clear of the terms."
        );
    }
    // Leg (d)'s `:e` is a bare name resolved in the new tab's cwd, so it is the second fixture's last
    // component, or capture B fails for a fixture reason that reads as a tab failure.
    assert_eq!(
        TABS_SECOND_FILE,
        format!("{TABS_SUBDIR}/{TABS_SECOND_NAME}"),
        "TABS_SECOND_FILE = {TABS_SECOND_FILE:?} is not TABS_SUBDIR = {TABS_SUBDIR:?} joined to \
         TABS_SECOND_NAME = {TABS_SECOND_NAME:?}. The three are used separately — the joined path \
         is where the fixture is WRITTEN, the directory is what leg (b) `:tcd`s into and what the \
         new tab must inherit, and the bare name is what leg (d) types at `:e` — so a disagreement \
         means the file is written at one path and opened at another."
    );
    assert!(
        !TABS_FIRST_FILE.contains('/'),
        "TABS_FIRST_FILE = {TABS_FIRST_FILE:?} carries a path separator. It is written directly \
         into the temp cwd and typed at `:e` while the first tab is still rooted there, and leg \
         (e) compares the TAIL of `bufname('%')` against this exact string — so a value with a \
         directory in it could never match what that readback can produce."
    );
    assert!(
        !TABS_SUBDIR.is_empty() && !TABS_SUBDIR.contains('/'),
        "TABS_SUBDIR = {TABS_SUBDIR:?} must be a single non-empty path component: the scenario \
         `:tcd`s into it RELATIVELY (so the temp path never reaches the command line the cmdline \
         strip draws) and then compares the FINAL COMPONENT of the new tab's `getcwd()` against \
         it. An empty value leaves the tab rooted at the project root, whose final component is \
         TEMP_CWD_BASENAME, so assertion 5 would compare against a name no correct run can \
         produce; a value carrying a separator is never the final component at all."
    );
    assert_ne!(
        TABS_SUBDIR, TEMP_CWD_BASENAME,
        "TABS_SUBDIR and TEMP_CWD_BASENAME are the same name ({TABS_SUBDIR:?}). Where the new tab \
         is rooted is checked on the final component of its `getcwd()`, and the one wrong answer \
         that assertion can still catch — a tab left at the PROJECT ROOT, because something other \
         than the window's live folder reached `create_tabpage` — has TEMP_CWD_BASENAME as its \
         final component. With one name for both, that key stops separating them and the last \
         thing assertion 5 discriminates is gone."
    );

    // The tab-persistence exemption, the two-window shape: its names match the compared strings, which
    // is safe because it reads no image. This block adds the crossing the saved-order claim needs.
    assert_eq!(
        TABS_RESTORE_FILES.len(),
        3,
        "the tab-persistence smoke must carry THREE fixtures, one per tab, not {}. The count is \
         load-bearing rather than generous. After materialization Neovim is current on the LAST \
         tabpage created until the queued `set_current_tabpage` for tab 0 lands, so the last tab \
         may or may not get one refresh — only a MIDDLE tab is guaranteed never to be \
         Neovim-current at launch. Tab 1 is therefore the one deterministically never-selected \
         tab, and it is the whole subject of assertion 3: the only tab whose persisted leaf can \
         ONLY have been resolved from the window-keyed snapshot maps, because the current \
         tabpage's `Snapshot::windows` never held its windows for a single wake of run 2. Cut the \
         smoke to two tabs and that assertion silently starts measuring a tab the launch may well \
         have had current.",
        TABS_RESTORE_FILES.len()
    );
    let mut tab_fixture_names = std::collections::HashSet::new();
    for (index, name) in TABS_RESTORE_FILES.into_iter().enumerate() {
        assert!(
            !name.contains('/'),
            "TABS_RESTORE_FILES[{index}] = {name:?} carries a path separator. It is written \
             directly into the temp cwd and typed at `:e` while that tab is rooted there, and BOTH \
             the readback comparison and the saved-leaf comparison are against the final path \
             component alone — so a value with a directory in it could never match what either \
             observable can produce, and the smoke would fail for a fixture reason while naming a \
             persistence bug."
        );
        assert!(
            tab_fixture_names.insert(name),
            "TABS_RESTORE_FILES[{index}] = {name:?} duplicates another of the tab-persistence \
             smoke's fixtures. With two tabs sharing a name, \"the three tabs came back in saved \
             order\" stops being falsifiable: a build that wrote one tab's subtree twice, or that \
             restored the three in the wrong order, would satisfy the comparison at the position \
             where the names coincide. Tab ORDER is the user's own and the next gather writes it \
             back, so a swap compounds every run."
        );
    }
}
