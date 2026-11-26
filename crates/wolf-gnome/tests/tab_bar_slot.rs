//! Table-driven tests for the placement seam of `wolf_gnome::tab_bar_slot`: given
//! `gtk-titlebar-style` and the tab view's page count, whether the tab bar sits in its own row
//! below the header bar or is the header bar's title widget.
//!
//! Pure: no display, no Neovim and no compositor. Every expected value is written out by hand,
//! never computed by the function under test. Whether the window applies the answer, and applies
//! it again as tabs open and close, is `smoke_headless/titlebar_tabs.rs`'s question.

use wolf_core::GtkTitlebarStyle;
use wolf_gnome::tab_bar_slot::{tab_bar_placement, TabBarPlacement};

/// TS1.
#[test]
fn one_tab_keeps_the_bar_below_the_header() {
    for (style, pages) in [
        (GtkTitlebarStyle::Tabs, 0),
        (GtkTitlebarStyle::Tabs, 1),
        (GtkTitlebarStyle::Native, 0),
        (GtkTitlebarStyle::Native, 1),
    ] {
        assert_eq!(
            tab_bar_placement(style, pages),
            TabBarPlacement::BelowHeader,
            "{style:?} at {pages} pages must keep the bar in its autohiding row below the header. \
             InHeader replaces the window title with a bar of one tab or none, which changes a \
             one-tab window's look and its header's height, and so `PANE_ORIGIN_Y_EXPECTED`"
        );
    }
}

/// TS2.
#[test]
fn two_or_more_tabs_put_the_bar_in_the_header_under_tabs() {
    for pages in [2, 3, 16, usize::MAX] {
        assert_eq!(
            tab_bar_placement(GtkTitlebarStyle::Tabs, pages),
            TabBarPlacement::InHeader,
            "`tabs` at {pages} pages must make the bar the header bar's title widget. BelowHeader \
             keeps the separate tab bar row the style exists to save; at 2 pages it means the \
             threshold is above two tabs"
        );
    }
}

/// TS3.
#[test]
fn native_keeps_the_bar_below_the_header_at_every_count() {
    for pages in [2, 3, 16] {
        assert_eq!(
            tab_bar_placement(GtkTitlebarStyle::Native, pages),
            TabBarPlacement::BelowHeader,
            "`native` at {pages} pages must keep the bar in its own row below the header, today's \
             layout. InHeader means `native` answers as `tabs` does"
        );
    }
}
