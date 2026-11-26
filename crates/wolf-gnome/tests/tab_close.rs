//! Tests for Close Tab's two pure seams: `wolf_gnome::tabs_ui::close_page_now`, which decides
//! whether the tab bar's `close-page` closes a page at once or asks the tab's Neovim first, and
//! the `win.close-tab` row of `wolf_gnome::actions`, a menu-only action beside Close Window.
//!
//! Pure: no display, no Neovim and no compositor. Every expected value is written out by hand.
//! Whether `on_close_page` passes the page's own confirmation, liveness and ask, and selects a
//! background page before asking, is the Close Tab smoke's question, in
//! `smoke_headless/close_tab.rs`. No smoke opens the primary menu.

use wolf_core::{CloseTabAnswer, Command, Liveness};
use wolf_gnome::actions::{
    action_for_command, binding_for_action, BindingStatus, MenuItemSpec, MENU_SECTIONS,
};
use wolf_gnome::tabs_ui::close_page_now;

/// One row of the `close_page_now` table: its inputs, then whether the page closes at once and
/// how many times the tab's Neovim is asked.
struct Row {
    case: &'static str,
    confirmed: bool,
    liveness: Option<Liveness>,
    answer: CloseTabAnswer,
    closes: bool,
    asks: usize,
}

/// H1.
#[test]
fn close_page_now_table() {
    let rows = [
        Row {
            case: "a modified tab, which Neovim refuses and shows E37 or E162 in",
            confirmed: false,
            liveness: Some(Liveness::Alive),
            answer: CloseTabAnswer::Refused,
            closes: false,
            asks: 1,
        },
        Row {
            case: "an unmodified tab, whose close_tab drain closes it confirmed",
            confirmed: false,
            liveness: Some(Liveness::Alive),
            answer: CloseTabAnswer::Closing,
            closes: false,
            asks: 1,
        },
        Row {
            case: "a tab whose Neovim gives no answer, which must stay closable",
            confirmed: false,
            liveness: Some(Liveness::Alive),
            answer: CloseTabAnswer::NoAnswer,
            closes: true,
            asks: 1,
        },
        Row {
            case: "a dead or restarting Neovim, which is never asked",
            confirmed: false,
            liveness: Some(Liveness::Dead),
            answer: CloseTabAnswer::Refused,
            closes: true,
            asks: 0,
        },
        Row {
            case: "a stalled Neovim, whose ask would stall the UI for another timeout",
            confirmed: false,
            liveness: Some(Liveness::NotResponding),
            answer: CloseTabAnswer::Refused,
            closes: true,
            asks: 0,
        },
        Row {
            case: "a tab whose core is gone",
            confirmed: false,
            liveness: None,
            answer: CloseTabAnswer::Refused,
            closes: true,
            asks: 0,
        },
        Row {
            case: "a confirmed close, a drained close_tab or a replacement",
            confirmed: true,
            liveness: Some(Liveness::Alive),
            answer: CloseTabAnswer::Refused,
            closes: true,
            asks: 0,
        },
    ];
    for row in rows {
        let mut asks = 0;
        let closes = close_page_now(row.confirmed, row.liveness, || {
            asks += 1;
            row.answer
        });
        assert_eq!(
            closes, row.closes,
            "{} ({}, {:?}, {:?}) must close at once: {}. true for Refused closes over unsaved \
             edits, true for Closing closes before Neovim's close_tab drains, and false for \
             NoAnswer, a dead, stalled or gone core, or a confirmed close leaves the page \
             unclosable",
            row.case, row.confirmed, row.liveness, row.answer, row.closes
        );
        assert_eq!(
            asks, row.asks,
            "{} must ask the tab's Neovim {} time(s): an ask of a dead, stalled or gone core, or \
             of a confirmed close, means a guard is missing, and more than one that it asks twice",
            row.case, row.asks
        );
    }
}

/// H2.
#[test]
fn the_close_tab_row_is_menu_only_and_listed() {
    let binding = binding_for_action("win.close-tab")
        .expect("ACTION_TABLE must hold a win.close-tab row; none means Close Tab has no action");
    assert_eq!(
        (
            binding.command,
            binding.accel,
            binding.ctrl_fold,
            binding.status
        ),
        (Some(Command::CloseTab), None, None, BindingStatus::MenuOnly),
        "win.close-tab must be Close Tab's menu-only action, with no chord: (command, accel, \
         ctrl_fold, status). Reserved hides it from the menu, and an accel takes a chord the task \
         leaves free"
    );
    assert_eq!(
        action_for_command(Command::CloseTab),
        Some("win.close-tab"),
        "Command::CloseTab must dispatch through win.close-tab; another action means an earlier \
         row claims the command"
    );

    let close_tab = MenuItemSpec {
        label: "Close Tab",
        action: "win.close-tab",
    };
    let close_window = MenuItemSpec {
        label: "Close Window",
        action: "win.close-window",
    };
    let listed = MENU_SECTIONS.iter().any(|section| {
        section
            .items
            .windows(2)
            .any(|pair| pair == [close_tab, close_window])
    });
    assert!(
        listed,
        "one primary-menu section must list {close_tab:?} directly before {close_window:?}; \
         neither order nor section holds, so the item is missing, misnamed or misplaced: \
         {MENU_SECTIONS:#?}"
    );
}
