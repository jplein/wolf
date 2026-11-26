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
//! ## The focus trigger
//!
//! [`focused_tab`] and [`focus_edges`] decide which tab's Neovim is told it gained
//! or lost the UI focus (`WolfCore::set_ui_focus`). Neovim fires `FocusGained` on
//! every call, with no de-duplication, so the edges are computed against the last
//! focus sent: an unchanged focus sends nothing, and a switch sends the loss before
//! the gain, so two Neovims never both hold it.

use std::path::Path;

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
