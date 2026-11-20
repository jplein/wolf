//! Table-driven tests for `wolf_gnome::pane_zoom`, the Zoom Pane rules: which pane a toggle
//! zooms, what unzooms it, and what a zoom freezes or redirects.
//!
//! The cases mirror `crates/wolf-macos/Tests/WolfKeyMapTests/PaneZoomTests.swift` case for
//! case, K1 to K10, with the same ids, so the two ports cannot drift silently: a case added
//! on one side belongs on the other. K6 tests `freezes_focus` where the Swift tests
//! `freezesLayout`, since no divider exists while a GNOME view is zoomed, and K10 tests
//! `shown_alone` over bound ids where the Swift tests `visibleRects` over rects, since GTK
//! allocates the one host shown and needs no rect.
//!
//! Pure: no display, no Neovim and no compositor. What the pane area shows for an answer is
//! `smoke_headless/zoom_pane.rs`'s question.

use wolf_gnome::layout::PaneId;
use wolf_gnome::other_pane_seam::Target;
use wolf_gnome::pane_zoom::{
    allows_toggle_terminal, freezes_focus, other_pane_target, refocuses, shown_alone, zoomed_after,
    ZoomEvent,
};

const P1: PaneId = 1;
const P2: PaneId = 2;

/// The drawer's pane id, kept apart from the tree's ids as the pane view's are.
const DRAWER: PaneId = 9;

/// A pane id absent from every fixture: a zoomed pane that is gone, or an active pane no
/// pane list holds.
const ABSENT: PaneId = 5;

/// K1: an unzoomed toggle zooms the active pane, a tree leaf or the drawer.
#[test]
fn toggle_zooms_the_active_pane_the_drawer_included() {
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, Some(P2), &[P1, P2]),
        Some(P2),
        "an unzoomed toggle must zoom the active pane 2. None means the toggle never zooms; \
         Some(1) means it zooms a pane other than the active one"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, Some(DRAWER), &[P1, DRAWER]),
        Some(DRAWER),
        "an unzoomed toggle with the drawer active must zoom the drawer, which counts among the \
         view's panes. None means the drawer cannot be zoomed"
    );
}

/// K2: a toggle while zoomed unzooms, whichever pane is active.
#[test]
fn toggle_again_unzooms() {
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, Some(P2), Some(P2), &[P1, P2]),
        None,
        "a toggle while pane 2 is zoomed and active must unzoom. Some(2) means Zoom Pane cannot \
         be undone by Zoom Pane"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, Some(P2), Some(P1), &[P1, P2]),
        None,
        "a toggle while pane 2 is zoomed must unzoom even when the active pane reads 1. Some(1) \
         means the toggle rezooms the active pane instead of unzooming; Some(2) that it keeps \
         the zoom"
    );
}

/// K3: a toggle with nothing to hide, or with no active pane among the view's panes, does
/// nothing.
#[test]
fn toggle_with_nothing_to_hide_is_a_no_op() {
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, Some(P1), &[P1]),
        None,
        "a toggle over a lone pane must leave the view unzoomed: Some(1) means a pane with \
         nothing to hide is reported zoomed"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, Some(ABSENT), &[P1, P2]),
        None,
        "a toggle whose active pane 5 is not among the view's panes must leave it unzoomed: \
         Some(5) means the toggle zooms a pane the view does not hold"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::Toggle, None, None, &[P1, P2]),
        None,
        "a toggle with no active pane must leave the view unzoomed: Some(_) zooms a pane nobody \
         chose"
    );
}

/// K4: a split and a drawer opening unzoom, since a pane joins the layout.
#[test]
fn a_split_and_a_drawer_open_unzoom() {
    assert_eq!(
        zoomed_after(ZoomEvent::Split, Some(P2), Some(P2), &[P1, P2]),
        None,
        "a split while pane 2 is zoomed must unzoom before the split, so the new pane shows; \
         Some(2) means the split lands hidden behind the zoomed pane"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::DrawerOpen, Some(P2), Some(P2), &[P1, P2]),
        None,
        "a drawer opening while pane 2 is zoomed must unzoom, as a split does; Some(2) means the \
         drawer opens hidden"
    );
}

/// K5: a change of panes unzooms only when the zoomed pane is gone.
#[test]
fn only_closing_the_zoomed_pane_unzooms() {
    assert_eq!(
        zoomed_after(ZoomEvent::PanesChanged, Some(P2), Some(P1), &[P1, DRAWER]),
        None,
        "the zoomed pane 2 closing must unzoom; Some(2) means the view stays zoomed on a pane \
         that is gone"
    );
    assert_eq!(
        zoomed_after(ZoomEvent::PanesChanged, Some(P2), Some(P2), &[P2, DRAWER]),
        Some(P2),
        "a hidden pane 1 closing must keep pane 2 zoomed; None means any close unzooms"
    );
}

/// K6: a zoomed view freezes focus moves; an unzoomed one does not.
#[test]
fn a_zoomed_view_freezes_focus_moves() {
    assert!(
        freezes_focus(Some(P2)),
        "a zoomed view must freeze focus moves: false lets <C-w>h/j/k/l move to a pane the user \
         cannot see"
    );
    assert!(
        !freezes_focus(None),
        "an unzoomed view must not freeze focus moves: true disables every <C-w>h/j/k/l"
    );
}

/// K7: Toggle Terminal acts unzoomed, and while zoomed only when the drawer is the zoomed
/// pane.
#[test]
fn toggle_terminal_acts_unzoomed_or_on_the_zoomed_drawer() {
    assert!(
        allows_toggle_terminal(None, Some(DRAWER)),
        "unzoomed with a drawer, Toggle Terminal must act, closing the drawer"
    );
    assert!(
        allows_toggle_terminal(None, None),
        "unzoomed with no drawer, Toggle Terminal must act, opening one"
    );
    assert!(
        allows_toggle_terminal(Some(DRAWER), Some(DRAWER)),
        "with the drawer zoomed, Toggle Terminal must act, closing the zoomed pane; false means \
         a zoomed drawer cannot be closed by its own toggle"
    );
    assert!(
        !allows_toggle_terminal(Some(P2), Some(DRAWER)),
        "with tree pane 2 zoomed, Toggle Terminal must do nothing; true closes a drawer the user \
         cannot see"
    );
    assert!(
        !allows_toggle_terminal(Some(P2), None),
        "with tree pane 2 zoomed and no drawer, Toggle Terminal must do nothing; true opens a \
         drawer that would unzoom the view"
    );
}

/// K8: while zoomed, the other-pane commands take their split path from a tree pane and do
/// nothing from the drawer.
#[test]
fn the_other_pane_commands_split_while_zoomed() {
    assert_eq!(
        other_pane_target(P2, Some(DRAWER)),
        Some(Target::Split),
        "a zoomed tree pane beside a drawer is the only visible pane, so the other-pane commands \
         must split; None makes them do nothing, Some(Existing(_)) binds into a hidden pane"
    );
    assert_eq!(
        other_pane_target(P2, None),
        Some(Target::Split),
        "a zoomed tree pane with no drawer is the only visible pane, so the other-pane commands \
         must split; None makes them do nothing"
    );
    assert_eq!(
        other_pane_target(DRAWER, Some(DRAWER)),
        None,
        "a zoomed drawer must leave the other-pane commands doing nothing, as from the drawer \
         unzoomed; Some(Split) means the drawer is split"
    );
}

/// K9: Neovim's current window resolving to a hidden pane is sent back to the zoomed pane.
#[test]
fn neovim_on_a_hidden_pane_is_sent_back() {
    assert!(
        refocuses(Some(P1), Some(P2)),
        "Neovim on hidden pane 1 while 2 is zoomed must be refocused; false lets the user type \
         into a pane they cannot see"
    );
    assert!(
        !refocuses(Some(P2), Some(P2)),
        "Neovim on the zoomed pane needs no refocus; true sends focus_win on every wake"
    );
    assert!(
        !refocuses(None, Some(P2)),
        "an unresolved window needs no refocus; true refocuses on a window no pane holds"
    );
    assert!(
        !refocuses(Some(P1), None),
        "an unzoomed view never refocuses; true pins focus where no zoom asked for it"
    );
}

/// K10: a zoomed view shows its pane alone; an unzoomed one, and one zoomed on a pane that is
/// not bound, show the whole layout.
#[test]
fn a_zoomed_view_shows_its_pane_alone() {
    assert_eq!(
        shown_alone(Some(P2), &[P1, P2, DRAWER]),
        Some(P2),
        "zoomed on 2, the pane area must show pane 2 alone; None means the whole layout shows \
         and the zoom is invisible"
    );
    assert_eq!(
        shown_alone(Some(DRAWER), &[P1, DRAWER]),
        Some(DRAWER),
        "zoomed on the drawer, the pane area must show the drawer alone; None means the drawer \
         cannot be shown zoomed"
    );
    assert_eq!(
        shown_alone(None, &[P1, P2, DRAWER]),
        None,
        "unzoomed, the pane area must show the whole layout; Some(_) hides panes nobody zoomed"
    );
    assert_eq!(
        shown_alone(Some(ABSENT), &[P1, P2, DRAWER]),
        None,
        "zoomed on pane 5, which is not bound, the whole layout must show rather than nothing; \
         Some(5) blanks the pane area after a missed unzoom"
    );
}
