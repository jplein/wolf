//! Table-driven tests for the focus seam of `wolf_gnome::tabs_ui`: which tab's Neovim holds the
//! UI focus, and which `set_ui_focus` edges a change of it sends.
//!
//! Pure: no display, no Neovim and no compositor. Every expected value is written out by hand,
//! never computed by the function under test. Whether `TabsHandle::sync_ui_focus` sends these
//! edges to the right core is the reload smoke's question, in `smoke_headless/panes.rs`; the
//! window's activation and deactivation are covered here alone, since under `cage` the window
//! turns active once and never resigns.

use wolf_gnome::tabs_ui::{focus_edges, focused_tab, FocusEdge, TabId};

/// The tab that is selected, or focused, before a change.
const ONE: TabId = TabId(1);

/// The tab selected by a switch away from [`ONE`].
const TWO: TabId = TabId(2);

/// TF1.
#[test]
fn only_the_selected_tab_of_an_active_window_is_focused() {
    assert_eq!(
        focused_tab(true, Some(ONE)),
        Some(ONE),
        "an active window's selected tab must hold the focus. None means no tab's Neovim ever \
         gains it, so no tab runs `:checktime` on a switch or on activation"
    );
    assert_eq!(
        focused_tab(false, Some(ONE)),
        None,
        "an inactive window holds no focus, whatever tab it has selected. Some(1) means the \
         window becoming inactive sends no loss, so its return sends no gain and the selected \
         tab never re-checks the files another program saved meanwhile"
    );
    assert_eq!(
        focused_tab(true, None),
        None,
        "with no tab selected nothing can hold the focus, active window or not"
    );
    assert_eq!(
        focused_tab(false, None),
        None,
        "an inactive window with no tab selected holds no focus"
    );
}

/// TF2.
#[test]
fn a_switch_reports_the_loss_then_the_gain() {
    assert_eq!(
        focus_edges(Some(ONE), Some(TWO)),
        vec![
            FocusEdge {
                tab: ONE,
                gained: false,
            },
            FocusEdge {
                tab: TWO,
                gained: true,
            },
        ],
        "a switch from tab 1 to tab 2 must report tab 1's loss and then tab 2's gain, in that \
         order. The gain first means the two Neovims briefly both hold the focus; a missing loss \
         means tab 1 keeps it, so its next gain is no edge and runs no `:checktime`"
    );
}

/// TF3.
#[test]
fn an_unchanged_focus_reports_nothing() {
    assert_eq!(
        focus_edges(Some(ONE), Some(ONE)),
        Vec::<FocusEdge>::new(),
        "a wake that leaves tab 1 focused must send no edge. A loss and a gain here would run \
         `:checktime` in that tab on every wake, which the drain calls this on"
    );
    assert_eq!(
        focus_edges(None, None),
        Vec::<FocusEdge>::new(),
        "a wake of an inactive window must send no edge"
    );
}

/// TF4.
#[test]
fn activation_and_deactivation_report_one_edge() {
    assert_eq!(
        focus_edges(None, Some(ONE)),
        vec![FocusEdge {
            tab: ONE,
            gained: true,
        }],
        "the window turning active with tab 1 selected must report tab 1's gain alone. An empty \
         answer means activation runs no `:checktime`, so a file saved by another program while \
         the window was away is never reloaded; a loss in it names a tab that held nothing"
    );
    assert_eq!(
        focus_edges(Some(ONE), None),
        vec![FocusEdge {
            tab: ONE,
            gained: false,
        }],
        "the window turning inactive must report tab 1's loss alone. An empty answer leaves tab \
         1's Neovim focused, so the window's return is no edge and runs no `:checktime`"
    );
}
