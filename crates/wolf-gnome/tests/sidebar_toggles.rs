//! Table-driven tests for the pure half of `wolf_gnome::sidebar_toggles`: which toggle of the
//! header bar's Files/Search group shows as active, what a click on either toggle does, and
//! which toggle a pointer `x` falls on.
//!
//! Pure: no display, no Neovim and no compositor. Every expected value is written out by hand,
//! never computed by the function under test, and together the cases cover every combination of
//! shown, page and clicked. Whether the window wires the answers up is the two smokes' question,
//! in `smoke_headless/sidebar_toggles.rs`.

use wolf_gnome::sidebar::SidebarPage;
use wolf_gnome::sidebar_toggles::{active_toggle, toggle_at, toggle_click, ToggleClick};

/// The group's width in every `toggle_at` case. Its half, 37, is a whole pixel, so the cases
/// either side of it are unambiguous.
const WIDTH: f64 = 74.0;

/// Half a pixel left of the group's middle: the last point of the start half.
const JUST_UNDER_HALF: f64 = 36.5;

/// The group's middle, the first point of the end half.
const HALF: f64 = 37.0;

/// The group's last whole pixel.
const LAST: f64 = 73.0;

/// C1.
#[test]
fn the_shown_page_is_the_active_toggle() {
    assert_eq!(
        active_toggle(true, SidebarPage::Files),
        Some(SidebarPage::Files),
        "with the sidebar shown on Files, the Files toggle must be the active one. None means \
         the group goes blank while the sidebar is up; Some(Search) means the page is ignored"
    );
    assert_eq!(
        active_toggle(true, SidebarPage::Search),
        Some(SidebarPage::Search),
        "with the sidebar shown on Search, the Search toggle must be the active one. \
         Some(Files) means the page is ignored and the group always shows Files; None means \
         the group goes blank while the sidebar is up"
    );
}

/// C2.
#[test]
fn no_toggle_is_active_while_the_sidebar_is_hidden() {
    assert_eq!(
        active_toggle(false, SidebarPage::Files),
        None,
        "with the sidebar hidden on Files, neither toggle may be active. Some(Files) means the \
         group keeps showing the hidden page, which is what a stale group looks like after \
         Ctrl+Shift+B hides the sidebar"
    );
    assert_eq!(
        active_toggle(false, SidebarPage::Search),
        None,
        "with the sidebar hidden on Search, neither toggle may be active. Some(Search) means the \
         group keeps showing the hidden page"
    );
}

/// C3.
#[test]
fn a_click_on_the_active_toggle_hides_the_sidebar() {
    assert_eq!(
        toggle_click(true, SidebarPage::Files, SidebarPage::Files),
        ToggleClick::Hide,
        "a click on Files while the sidebar shows Files must hide the sidebar. Show(Files) \
         means a click on the active toggle changes nothing, which is adw::ToggleGroup's own \
         behaviour and the one the user asked to replace"
    );
    assert_eq!(
        toggle_click(true, SidebarPage::Search, SidebarPage::Search),
        ToggleClick::Hide,
        "a click on Search while the sidebar shows Search must hide the sidebar. \
         Show(Search) means a click on the active toggle changes nothing"
    );
}

/// C4.
#[test]
fn a_click_on_the_other_toggle_switches_to_its_page() {
    assert_eq!(
        toggle_click(true, SidebarPage::Files, SidebarPage::Search),
        ToggleClick::Show(SidebarPage::Search),
        "a click on Search while the sidebar shows Files must switch it to Search. Hide means \
         every click on a shown sidebar hides it, whichever toggle it was on; Show(Files) means \
         the clicked toggle is ignored"
    );
    assert_eq!(
        toggle_click(true, SidebarPage::Search, SidebarPage::Files),
        ToggleClick::Show(SidebarPage::Files),
        "a click on Files while the sidebar shows Search must switch it to Files. Hide means \
         every click on a shown sidebar hides it; Show(Search) means the clicked toggle is \
         ignored"
    );
}

/// C5.
#[test]
fn a_click_while_hidden_shows_the_clicked_page() {
    for (page, clicked) in [
        (SidebarPage::Files, SidebarPage::Files),
        (SidebarPage::Files, SidebarPage::Search),
        (SidebarPage::Search, SidebarPage::Files),
        (SidebarPage::Search, SidebarPage::Search),
    ] {
        assert_eq!(
            toggle_click(false, page, clicked),
            ToggleClick::Show(clicked),
            "with the sidebar hidden on {page:?}, a click on {clicked:?} must show the sidebar \
             on {clicked:?}. Hide, where the page and the click agree, means the rule compares \
             the page without checking that the sidebar is shown, so the hidden page's toggle \
             can never bring the sidebar back; Show of the other page means the hidden page \
             wins over the click"
        );
    }
}

/// C6.
#[test]
fn the_start_half_is_files_and_the_end_half_is_search() {
    for (x, expected) in [
        (0.0, SidebarPage::Files),
        (JUST_UNDER_HALF, SidebarPage::Files),
        (HALF, SidebarPage::Search),
        (LAST, SidebarPage::Search),
    ] {
        assert_eq!(
            toggle_at(x, WIDTH, false),
            expected,
            "left to right, x = {x} in a {WIDTH} px group must fall on {expected:?}: Files is \
             the start half, x < {HALF}, and Search the rest. The opposite page at both ends \
             means the halves are swapped, so each click reaches the other toggle's page; the \
             wrong page only next to {HALF} means the boundary is off"
        );
    }
}

/// C7.
#[test]
fn right_to_left_mirrors_the_halves() {
    for (x, expected) in [
        (0.0, SidebarPage::Search),
        (JUST_UNDER_HALF, SidebarPage::Search),
        (HALF, SidebarPage::Files),
        (LAST, SidebarPage::Files),
    ] {
        assert_eq!(
            toggle_at(x, WIDTH, true),
            expected,
            "right to left, x = {x} in a {WIDTH} px group must fall on {expected:?}: the group \
             lays Files out at its start, which is now the right edge, so the halves mirror the \
             left-to-right ones. The left-to-right answer at every x means `rtl` is ignored, and \
             a right-to-left click reaches the other toggle's page"
        );
    }
}
