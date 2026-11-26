//! Tests for Close Tab's two pure seams: `wolf_gnome::tabs_ui::close_page_now`, which decides
//! whether the tab bar's `close-page` closes a page at once or asks the tab's Neovim first, and
//! the `win.close-tab` row of `wolf_gnome::actions`, a menu-only action beside Close Window.
//! Also Close Window's and Quit's seams in `wolf_gnome::tabs_ui`: `tab_unsaved`, a tab's answer,
//! and `window_close` and `quit`, which pick the tab a refusal shows or close everything.
//!
//! Pure: no display, no Neovim and no compositor. Every expected value is written out by hand.
//! Whether `on_close_page` passes the page's own confirmation, liveness and ask, and selects a
//! background page before asking, is the Close Tab smoke's question, in
//! `smoke_headless/close_tab.rs`. Whether `close-request` and `Coordinator::quit` feed the
//! window and quit seams each tab's answer, and show the tab they pick, is the question of
//! `smoke_headless/close_window.rs`. No smoke opens the primary menu.

use std::cell::RefCell;

use wolf_core::{CloseTabAnswer, Command, Liveness, UnsavedAnswer};
use wolf_gnome::actions::{
    action_for_command, binding_for_action, BindingStatus, MenuItemSpec, MENU_SECTIONS,
};
use wolf_gnome::tabs_ui::{
    close_page_now, quit, tab_unsaved, window_close, Quit, TabUnsaved, WindowClose,
};

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

/// One row of the `tab_unsaved` table: its inputs, then the tab's answer and how many times the
/// tab's Neovim is asked.
struct UnsavedRow {
    case: &'static str,
    confirmed: bool,
    liveness: Option<Liveness>,
    answer: UnsavedAnswer,
    want: TabUnsaved,
    asks: usize,
}

/// CW8.
#[test]
fn tab_unsaved_table() {
    let rows = [
        UnsavedRow {
            case: "a tab whose close is already confirmed, by its own passed close",
            confirmed: true,
            liveness: Some(Liveness::Alive),
            answer: UnsavedAnswer::Unsaved,
            want: TabUnsaved::Clean,
            asks: 0,
        },
        UnsavedRow {
            case: "a dead or restarting Neovim, which is never asked",
            confirmed: false,
            liveness: Some(Liveness::Dead),
            answer: UnsavedAnswer::Unsaved,
            want: TabUnsaved::Unreachable,
            asks: 0,
        },
        UnsavedRow {
            case: "a stalled Neovim, whose ask would stall the UI for another timeout",
            confirmed: false,
            liveness: Some(Liveness::NotResponding),
            answer: UnsavedAnswer::Unsaved,
            want: TabUnsaved::Unreachable,
            asks: 0,
        },
        UnsavedRow {
            case: "a tab whose core is gone",
            confirmed: false,
            liveness: None,
            answer: UnsavedAnswer::Unsaved,
            want: TabUnsaved::Unreachable,
            asks: 0,
        },
        UnsavedRow {
            case: "a live Neovim that gives no answer within the RPC timeout",
            confirmed: false,
            liveness: Some(Liveness::Alive),
            answer: UnsavedAnswer::NoAnswer,
            want: TabUnsaved::Unreachable,
            asks: 1,
        },
        UnsavedRow {
            case: "a live Neovim holding a modified buffer",
            confirmed: false,
            liveness: Some(Liveness::Alive),
            answer: UnsavedAnswer::Unsaved,
            want: TabUnsaved::Unsaved,
            asks: 1,
        },
        UnsavedRow {
            case: "a live Neovim with nothing modified",
            confirmed: false,
            liveness: Some(Liveness::Alive),
            answer: UnsavedAnswer::Clean,
            want: TabUnsaved::Clean,
            asks: 1,
        },
    ];
    for row in rows {
        let mut asks = 0;
        let got = tab_unsaved(row.confirmed, row.liveness, || {
            asks += 1;
            row.answer
        });
        assert_eq!(
            got, row.want,
            "{} ({}, {:?}, {:?}) must answer {:?}. Unsaved for NoAnswer, a dead, stalled or gone \
             core keeps a wedged tab's window and the app open; Clean or Unreachable for Unsaved \
             closes over unsaved edits; anything but Clean for a confirmed tab refuses a close \
             that has already passed its check",
            row.case, row.confirmed, row.liveness, row.answer, row.want
        );
        assert_eq!(
            asks, row.asks,
            "{} must ask the tab's Neovim {} time(s): an ask of a dead, stalled or gone core, or \
             of a confirmed tab, means a guard is missing, and more than one that it asks twice",
            row.case, row.asks
        );
    }
}

/// The answers `tabs` as a lazy sequence that records the index of every answer pulled.
fn recorded<'a>(
    tabs: &'a [TabUnsaved],
    pulled: &'a RefCell<Vec<usize>>,
) -> impl Iterator<Item = TabUnsaved> + 'a {
    tabs.iter().enumerate().map(move |(index, tab)| {
        pulled.borrow_mut().push(index);
        *tab
    })
}

/// A row of [`quit_table`]: the case, each window's answers frontmost first, the decision and
/// the windows pulled.
type QuitRow<'a> = (&'a str, &'a [&'a [TabUnsaved]], Quit, &'a [usize]);

/// CW9.
#[test]
fn window_close_table() {
    use TabUnsaved::{Clean, Unreachable, Unsaved};
    // (case, each tab's answer in tab order, the decision, the indices pulled)
    let rows: [(&str, &[TabUnsaved], WindowClose, &[usize]); 5] = [
        ("a window with no tabs", &[], WindowClose::CloseAll, &[]),
        (
            "two clean tabs",
            &[Clean, Clean],
            WindowClose::CloseAll,
            &[0, 1],
        ),
        (
            "an unreachable tab beside a clean one",
            &[Unreachable, Clean],
            WindowClose::CloseAll,
            &[0, 1],
        ),
        (
            "one unsaved tab, in the background",
            &[Clean, Unsaved],
            WindowClose::Show(1),
            &[0, 1],
        ),
        (
            "the first of two unsaved tabs",
            &[Clean, Unsaved, Unsaved],
            WindowClose::Show(1),
            &[0, 1],
        ),
    ];
    for (case, tabs, want, want_pulled) in rows {
        let pulled = RefCell::new(Vec::new());
        let got = window_close(recorded(tabs, &pulled));
        assert_eq!(
            got, want,
            "{case} {tabs:?} must give {want:?}. CloseAll with an Unsaved tab closes over its \
             edits; Show for an Unreachable tab keeps a wedged tab's window open; Show of a later \
             index shows the last unsaved tab rather than the first in tab order"
        );
        assert_eq!(
            pulled.into_inner(),
            want_pulled,
            "{case} must ask the tabs {want_pulled:?} in tab order and stop at the first Unsaved; \
             a later index means it asked every tab before deciding, so a wedged tab after the \
             shown one costs another RPC timeout"
        );
    }
}

/// CW10.
#[test]
fn quit_table() {
    use TabUnsaved::{Clean, Unreachable, Unsaved};
    let rows: [QuitRow<'_>; 6] = [
        ("no windows", &[], Quit::QuitAll, &[]),
        (
            "every tab clean",
            &[&[Clean], &[Clean, Clean]],
            Quit::QuitAll,
            &[0, 1],
        ),
        (
            "an unreachable tab beside a clean one",
            &[&[Unreachable], &[Clean]],
            Quit::QuitAll,
            &[0, 1],
        ),
        (
            "one unsaved tab, in the background of the second window",
            &[&[Clean], &[Clean, Unsaved]],
            Quit::Show { window: 1, tab: 1 },
            &[0, 1],
        ),
        (
            "the first window's second tab and the second window's only tab unsaved",
            &[&[Clean, Unsaved], &[Unsaved]],
            Quit::Show { window: 0, tab: 1 },
            &[0],
        ),
        (
            "an unsaved first tab of the third window",
            &[&[Clean], &[Clean, Clean], &[Unsaved, Clean]],
            Quit::Show { window: 2, tab: 0 },
            &[0, 1, 2],
        ),
    ];
    for (case, windows, want, want_pulled) in rows {
        let pulled = RefCell::new(Vec::new());
        let lazy = windows.iter().enumerate().map(|(index, tabs)| {
            pulled.borrow_mut().push(index);
            tabs.iter().copied()
        });
        let got = quit(lazy);
        assert_eq!(
            got, want,
            "{case} {windows:?} must give {want:?}. QuitAll with an Unsaved tab quits over its \
             edits; Show for an Unreachable tab keeps the app open for a wedged tab; a Show of \
             another window means the windows are walked out of the order given, and window and \
             tab traded means the fields are swapped"
        );
        assert_eq!(
            pulled.into_inner(),
            want_pulled,
            "{case} must ask the windows {want_pulled:?} in the order given and stop at the first \
             with an Unsaved tab; a later window means it asked every window before deciding"
        );
    }
}
