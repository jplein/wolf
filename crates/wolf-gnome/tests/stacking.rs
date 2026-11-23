//! Table-driven tests for `wolf_gnome::stacking`: the window stacking order and the per-window
//! tab recency of `specs/application-state.md` §3, kept from focus and selection changes because
//! Wayland offers nothing to query.
//!
//! Pure: no display, no Neovim and no compositor. Window ids are bare `WindowId`s and tab ids
//! `TabId`s chosen so no id equals an index of the lists asserted on, and every expected order
//! is written out by hand, never computed by the function under test. Whether the window's
//! `is-active` handler feeds `focus` is the two-window restore smoke's question, in
//! `smoke_headless/restore.rs`.

use wolf_gnome::coordinator::WindowId;
use wolf_gnome::stacking::{StackingOrder, TabRecency};
use wolf_gnome::tabs_ui::TabId;

/// The live windows, in registry order, of every stacking case but the unranked one.
const LIVE: [WindowId; 3] = [1, 2, 3];

/// Three tabs in tab order. Their ids are not 0, 1 or 2, so an index mistaken for an id shows.
const TABS: [TabId; 3] = [TabId(10), TabId(11), TabId(12)];

/// An order seeded with every window of [`LIVE`] in that order, none minimized.
fn seeded_123() -> StackingOrder {
    StackingOrder::seeded(&[(1, false), (2, false), (3, false)])
}

/// G1a.
#[test]
fn a_focused_window_moves_to_the_front() {
    let mut order = seeded_123();
    assert_eq!(
        order.order(&LIVE),
        vec![1, 2, 3],
        "premise: an order seeded [1, 2, 3] must answer that order before any focus"
    );

    order.focus(3);
    assert_eq!(
        order.order(&LIVE),
        vec![3, 1, 2],
        "window 3 gained focus, so it must be frontmost and the others keep their order. \
         [1, 2, 3] means focus leaves the window where it was or moves it to the back; [3, 2, 1] \
         means it reverses the others"
    );
}

/// G1b.
#[test]
fn a_minimized_window_moves_behind_every_window_that_is_not() {
    let mut order = seeded_123();
    order.minimize(1);
    assert!(
        order.is_minimized(1),
        "window 1 was minimized, so it must read minimized"
    );
    assert!(
        !order.is_minimized(2) && !order.is_minimized(3),
        "minimizing window 1 must mark no other window"
    );
    assert_eq!(
        order.order(&LIVE),
        vec![2, 3, 1],
        "the minimized window 1 must sit behind every window that is not, the others keeping \
         their order. [1, 2, 3] means the order ignores the minimized mark"
    );
}

/// G1c.
#[test]
fn focus_unmarks_a_minimized_window() {
    let mut order = seeded_123();
    order.minimize(1);
    order.focus(1);
    assert!(
        !order.is_minimized(1),
        "a minimized window that gains focus has been restored, so it must no longer read \
         minimized; it still does, so focus leaves the mark and the next save writes a visible \
         window as minimized"
    );
    assert_eq!(
        order.order(&LIVE),
        vec![1, 2, 3],
        "window 1, focused after its minimize, must be frontmost again. [2, 3, 1] means the \
         minimized mark survived the focus"
    );
}

/// G1d.
#[test]
fn the_seed_keeps_the_saved_order_and_minimized_marks() {
    let order = StackingOrder::seeded(&[(1, false), (2, true), (3, false)]);
    assert!(
        order.is_minimized(2),
        "the seed marks window 2 minimized, so it must read minimized; it does not, so restore \
         loses a saved minimized window"
    );
    assert!(
        !order.is_minimized(1) && !order.is_minimized(3),
        "the seed marks only window 2 minimized"
    );
    assert_eq!(
        order.order(&[3, 2, 1]),
        vec![1, 3, 2],
        "seeded [1, 2 minimized, 3] and asked over the live windows [3, 2, 1], the order must \
         be the seed's with the minimized window last. [1, 2, 3] means the seed's flags are \
         ignored; [3, 2, 1] means the seed's ranks are ignored and the live order wins"
    );
}

/// G1e.
#[test]
fn unranked_windows_follow_ranked_ones_in_registry_order() {
    let order = StackingOrder::seeded(&[(5, false)]);
    assert_eq!(
        order.order(&[7, 5, 6]),
        vec![5, 7, 6],
        "window 5 is ranked and 7 and 6 never gained focus, so 5 must lead and 7 and 6 follow \
         in the order the registry lists them. [7, 5, 6] means unranked windows rank before \
         ranked ones; [5, 6, 7] means they are sorted by id rather than by registry order; a \
         missing id means `order` drops a live window it has no rank for"
    );
}

/// G2a.
#[test]
fn recency_starts_with_the_active_tab() {
    let recency = TabRecency::seeded(&TABS, 1);
    assert_eq!(
        recency.indices(&TABS),
        vec![1, 0, 2],
        "seeded with tab 1 active, the active tab must be the most recent and the rest follow \
         in tab order. [0, 1, 2] means the seed ignores the active tab"
    );
}

/// G2b.
#[test]
fn a_selected_tab_becomes_the_most_recent() {
    let mut recency = TabRecency::seeded(&TABS, 1);
    recency.select(TABS[2]);
    assert_eq!(
        recency.indices(&TABS),
        vec![2, 1, 0],
        "after selecting tab 2 it must be the most recent, ahead of the active tab it replaced. \
         [1, 0, 2] means select appends at the back, or changes nothing"
    );
}
