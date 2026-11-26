//! The pure, widget-free rulebook for NATIVE TABS in `wolf-gnome`: what closing one
//! does, the title and tooltip a tab shows, and which tab's Neovim holds the UI
//! focus.
//!
//! The tabs counterpart of [`crate::nav_ui`] and [`crate::clipboard_ui`]. Data in,
//! data out — no widget type, no `adw::TabView`, no `WolfCore` — which is exactly
//! what lets the whole rulebook be unit-tested headlessly, the same way
//! [`crate::command_action`], [`mod@crate::liveness_ui`] and [`crate::title`] keep
//! their decisions pure. The widget work of carrying the answers out lives in
//! `tab_view.rs` and [`crate::shell`].
//!
//! Every tab runs its own captive Neovim
//! (`specs/application-state.md#8-one-neovim-per-tab`), so no rule here routes a
//! call to one tab through another tab's Neovim.
//!
//! ## The last tab closing is the WINDOW closing
//!
//! A window with no tab holds a tab bar and no editor, with no way to reach one.
//! [`close_outcome`] therefore routes the last tab's close to
//! [`TabClose::CloseWindow`], which runs the window's own close handler and shuts
//! every core down.
//!
//! ## A close asks the tab's Neovim first
//!
//! A user's close of a tab, from its close button, Close Tab or `:WolfCloseTab`, must
//! not drop unsaved changes, and only the tab's own Neovim knows of them. So
//! [`close_page_now`] refuses the page and asks it, and Neovim notifies `close_tab`
//! once nothing is unsaved. That drained `close_tab` is Neovim's own verdict, so it
//! arrives confirmed and closes without asking again. A dead, stalled or gone core, or
//! one that gives no answer, closes unchecked: a wedged tab must stay closable, and its
//! swap files keep its edits recoverable.
//!
//! ## Closing a window and quitting ask every tab
//!
//! Close Window and Quit ask each tab they would close, silently, through
//! [`tab_unsaved`]; [`window_close`] and [`quit`] then pick the first unsaved tab to
//! show, or close everything. A dead, stalled or gone core, or one that gives no answer,
//! counts as clean, as for Close Tab. A tab whose own close is already confirmed counts
//! as clean unasked, since its check has passed. Only the tab shown is then asked to
//! echo E37 or E162, once it is selected, so the message shows in the tab it names.
//!
//! ## The focus trigger
//!
//! [`focused_tab`] and [`focus_edges`] decide which tab's Neovim is told it gained
//! or lost the UI focus (`WolfCore::set_ui_focus`). Neovim fires `FocusGained` on
//! every call, with no de-duplication, so the edges are computed against the last
//! focus sent: an unchanged focus sends nothing, and a switch sends the loss before
//! the gain, so two Neovims never both hold it.

use std::path::Path;

use wolf_core::{CloseTabAnswer, Liveness, UnsavedAnswer};

/// What closing one tab's page does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabClose {
    /// Close the tab: deregister its page and shut its Neovim down.
    CloseTab,
    /// Close the whole WINDOW, which shuts every core down. See the module docs.
    CloseWindow,
}

/// One tab's identity within its window, minted when the tab opens and never
/// reused while the window lives.
///
/// What [`focused_tab`] and [`focus_edges`] speak in, so the focus rule needs no
/// widget to name a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

/// One UI-focus change for one tab's Neovim: `WolfCore::set_ui_focus(gained)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusEdge {
    /// The tab whose Neovim is told.
    pub tab: TabId,
    /// `true` for focus gained, `false` for focus lost.
    pub gained: bool,
}

/// What closing one page does, given how many pages the view still holds.
///
/// `live_pages` COUNTS THE PAGE BEING CLOSED — it is read inside the `close-page`
/// handler, before the entry is deregistered — so `1` means "this is the last tab".
/// One and zero both answer [`TabClose::CloseWindow`]; see the module docs.
#[must_use]
pub fn close_outcome(live_pages: usize) -> TabClose {
    if live_pages <= 1 {
        TabClose::CloseWindow
    } else {
        TabClose::CloseTab
    }
}

/// Whether the `close-page` veto closes a page at once, rather than refusing it and
/// leaving the close to the tab's Neovim. See the module docs.
///
/// `confirmed` is a drained `close_tab` or a programmatic close, and closes without
/// asking. `liveness` is `None` when the tab has no core. Only an
/// [`Liveness::Alive`] core is asked, once, through `ask`: [`CloseTabAnswer::Refused`]
/// and [`CloseTabAnswer::Closing`] keep the page, since a `Closing` tab closes on its
/// drained `close_tab`, and [`CloseTabAnswer::NoAnswer`] closes it unchecked.
#[must_use]
pub fn close_page_now(
    confirmed: bool,
    liveness: Option<Liveness>,
    ask: impl FnOnce() -> CloseTabAnswer,
) -> bool {
    if confirmed {
        return true;
    }
    match liveness {
        Some(Liveness::Alive) => {}
        Some(Liveness::NotResponding | Liveness::Dead) | None => return true,
    }
    match ask() {
        CloseTabAnswer::Refused | CloseTabAnswer::Closing => false,
        CloseTabAnswer::NoAnswer => true,
    }
}

/// One tab's answer to Close Window's and Quit's ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabUnsaved {
    /// The tab's Neovim holds a modified buffer: the close is refused.
    Unsaved,
    /// Nothing is unsaved, or the tab's close is already confirmed.
    Clean,
    /// The tab's Neovim is dead, stalled, gone or gave no answer: counted as clean.
    Unreachable,
}

/// What Close Window does once every tab of the window has answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowClose {
    /// Close every tab, and the window with them.
    CloseAll,
    /// Close nothing, and show the tab at this page index.
    Show(usize),
}

/// What Quit does once every tab of every window has answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quit {
    /// Quit: commit the session and shut every Neovim down.
    QuitAll,
    /// Quit nothing, and show tab `tab` of window `window`, both indices of the order given.
    Show {
        /// The window's index in the order given to [`quit`].
        window: usize,
        /// The tab's page index within that window.
        tab: usize,
    },
}

/// One tab's answer for Close Window and Quit, given whether its own close is already
/// `confirmed` and its core's `liveness` (`None` with no core).
///
/// A confirmed tab is [`TabUnsaved::Clean`] and a core that is not
/// [`Liveness::Alive`] is [`TabUnsaved::Unreachable`], neither asked. Otherwise `ask`
/// runs once, and its [`UnsavedAnswer::NoAnswer`] is [`TabUnsaved::Unreachable`].
#[must_use]
pub fn tab_unsaved(
    confirmed: bool,
    liveness: Option<Liveness>,
    ask: impl FnOnce() -> UnsavedAnswer,
) -> TabUnsaved {
    if confirmed {
        return TabUnsaved::Clean;
    }
    match liveness {
        Some(Liveness::Alive) => {}
        Some(Liveness::NotResponding | Liveness::Dead) | None => return TabUnsaved::Unreachable,
    }
    match ask() {
        UnsavedAnswer::Unsaved => TabUnsaved::Unsaved,
        UnsavedAnswer::Clean => TabUnsaved::Clean,
        UnsavedAnswer::NoAnswer => TabUnsaved::Unreachable,
    }
}

/// Close Window's decision over the window's tabs, in page order:
/// [`WindowClose::Show`] for the first [`TabUnsaved::Unsaved`], else
/// [`WindowClose::CloseAll`].
///
/// `tabs` is pulled lazily and not past the first unsaved tab, so no tab after it is
/// asked and a wedged one there costs no RPC timeout.
#[must_use]
pub fn window_close(tabs: impl IntoIterator<Item = TabUnsaved>) -> WindowClose {
    tabs.into_iter()
        .position(|tab| tab == TabUnsaved::Unsaved)
        .map_or(WindowClose::CloseAll, WindowClose::Show)
}

/// Quit's decision over every window's tabs, the windows in the order given (frontmost
/// first) and each window's tabs in page order: [`Quit::Show`] for the first window
/// whose [`window_close`] shows a tab, else [`Quit::QuitAll`].
///
/// Pulls no window after the one shown.
#[must_use]
pub fn quit<W>(windows: W) -> Quit
where
    W: IntoIterator,
    W::Item: IntoIterator<Item = TabUnsaved>,
{
    windows
        .into_iter()
        .enumerate()
        .find_map(|(window, tabs)| match window_close(tabs) {
            WindowClose::Show(tab) => Some(Quit::Show { window, tab }),
            WindowClose::CloseAll => None,
        })
        .unwrap_or(Quit::QuitAll)
}

/// The tab whose Neovim holds the UI focus: the `selected` tab while the window
/// is active, and none while it is not.
#[must_use]
pub fn focused_tab(window_active: bool, selected: Option<TabId>) -> Option<TabId> {
    if window_active {
        selected
    } else {
        None
    }
}

/// The edges that move the UI focus from `previous` to `next`, in the order they
/// are sent: nothing when the two are equal, else the loss of `previous` if any,
/// then the gain of `next` if any.
#[must_use]
pub fn focus_edges(previous: Option<TabId>, next: Option<TabId>) -> Vec<FocusEdge> {
    if previous == next {
        return Vec::new();
    }
    let mut edges = Vec::with_capacity(2);
    if let Some(tab) = previous {
        edges.push(FocusEdge { tab, gained: false });
    }
    if let Some(tab) = next {
        edges.push(FocusEdge { tab, gained: true });
    }
    edges
}

/// What one tab shows: its title in the bar and its hover tooltip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabLabel {
    /// Plain text, as [`crate::title::window_title`] renders the tab's directory.
    pub title: String,
    /// Pango markup for `AdwTabPage:tooltip`, or `None` for no tooltip of the tab's own.
    pub tooltip: Option<String>,
}

/// The title and tooltip of a tab whose Neovim's working directory is `cwd`.
///
/// The title is [`crate::title::window_title`] of `cwd` against the injected `home`, so
/// the selected tab's title always equals its window's. An `empty` tab is titled
/// [`crate::title::WINDOW_TITLE_FALLBACK`], as its window is
/// (`specs/application-state.md#7-new-windows-and-tabs`), whatever its Neovim's cwd.
/// The title is never shortened here: libadwaita fades one too long for its tab.
///
/// The tooltip is `cwd` as a full absolute path, escaped because `AdwTabPage:tooltip`
/// is Pango markup, so a `&` or `<` in a directory name shows literally. An empty tab,
/// or one whose cwd is not known yet, has none, and libadwaita then shows the title.
#[must_use]
pub fn tab_label(empty: bool, cwd: Option<&Path>, home: &str) -> TabLabel {
    if empty {
        return TabLabel {
            title: crate::title::WINDOW_TITLE_FALLBACK.to_owned(),
            tooltip: None,
        };
    }
    let cwd = cwd.map(|cwd| cwd.to_string_lossy());
    let cwd = cwd.as_deref().unwrap_or_default();
    let tooltip = (!crate::title::display_title(cwd, home).is_empty())
        .then(|| gtk::glib::markup_escape_text(cwd).to_string());
    TabLabel {
        title: crate::title::window_title(cwd, home),
        tooltip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === close_outcome ===

    /// One [`close_outcome`] table row.
    struct CloseCase {
        desc: &'static str,
        live_pages: usize,
        want: TabClose,
    }

    /// What closing a page does, for every count of pages the view might hold.
    ///
    /// FALSIFYING EDIT: test `live_pages == 0` instead of `live_pages <= 1`. The
    /// `1` row then reports [`TabClose::CloseTab`], which is the exact wrong
    /// answer: the window would be left holding a tab bar and no editor.
    #[test]
    fn close_outcome_table() {
        let cases = [
            CloseCase {
                desc: "no pages at all: there is nothing left to be a window for",
                live_pages: 0,
                want: TabClose::CloseWindow,
            },
            CloseCase {
                desc: "THE ROW THIS TABLE EXISTS FOR — the page being closed is the only one, so \
                       closing it IS closing the window",
                live_pages: 1,
                want: TabClose::CloseWindow,
            },
            CloseCase {
                desc: "two pages: one survives, so only the closed tab and its Neovim go",
                live_pages: 2,
                want: TabClose::CloseTab,
            },
            CloseCase {
                desc: "three pages, the same",
                live_pages: 3,
                want: TabClose::CloseTab,
            },
            CloseCase {
                desc: "many pages, the same — there is no upper bound on this rule",
                live_pages: 16,
                want: TabClose::CloseTab,
            },
            CloseCase {
                desc: "and no cap hidden anywhere above it",
                live_pages: usize::MAX,
                want: TabClose::CloseTab,
            },
        ];

        for case in &cases {
            assert_eq!(
                close_outcome(case.live_pages),
                case.want,
                "close_outcome({}) must be {:?} — {}. `live_pages` COUNTS the page being closed, \
                 so 1 means this is the last tab. Answering CloseTab there leaves a window \
                 holding a tab bar and no editor, with no way to reach one. Answering \
                 CloseWindow where a sibling page survives tears down the whole window and every \
                 tab's Neovim over one tab's close button.",
                case.live_pages,
                case.want,
                case.desc
            );
        }

        // --- Non-vacuity: row-deletion backstops; a constant stub fails the table at row 0 (measured).
        // The first two read `case.want`; the third bites only if the expectations were rewritten.
        assert!(
            cases.iter().any(|case| case.want == TabClose::CloseWindow),
            "no close_outcome row expects CloseWindow, so the last-tab rule is unmeasured"
        );
        assert!(
            cases.iter().any(|case| case.want == TabClose::CloseTab),
            "no close_outcome row expects CloseTab, so the ordinary close is unmeasured"
        );
        assert_ne!(
            close_outcome(1),
            close_outcome(2),
            "close_outcome answered the same thing for the last page and for a page with a \
             sibling, so it is a constant function and the rows above measure nothing. A constant \
             CloseTab leaves a window with no editor; a constant \
             CloseWindow destroys the whole window whenever any one tab is closed."
        );
    }
}
