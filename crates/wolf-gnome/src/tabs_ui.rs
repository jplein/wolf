//! The pure, GTK-free rulebook for NATIVE TABS in `wolf-gnome`: what closing one
//! does, the string one tab draws in the bar, and which tab's Neovim holds the UI
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

/// The title a folder tab draws when its pane shows no named file.
///
/// Deliberately NOT Neovim's own `[No Name]`: that string is what
/// `WolfCore::pane_status` answers for a window it cannot resolve, so reusing it
/// would make a genuinely unnamed buffer indistinguishable from a failed lookup.
/// An empty tab draws [`crate::title::WINDOW_TITLE_FALLBACK`] instead, the title
/// of its window ([`tab_title`]).
pub const TAB_TITLE_FALLBACK: &str = "Untitled";

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

/// The text one tab draws in the bar: the FILE NAME of the pane's current file, or,
/// when there is none, [`crate::title::WINDOW_TITLE_FALLBACK`] for an `empty` tab and
/// [`TAB_TITLE_FALLBACK`] for a folder tab.
///
/// An empty tab with no file is titled as its window is, so tab and window agree that
/// no folder is open (`specs/application-state.md#7-new-windows-and-tabs`).
///
/// Never a path. A tab bar allots each tab a small fraction of the window's width,
/// so an absolute path renders as an ellipsised middle with neither the project nor
/// the file legible. The same final-component rule as
/// [`crate::workspace::root_label`], with a different fallback: a path with no final
/// component (`/`, or the empty path) carries no name to show, so it falls back
/// rather than drawing the separator.
#[must_use]
pub fn tab_title(empty: bool, current_file: Option<&Path>) -> String {
    let fallback = if empty {
        crate::title::WINDOW_TITLE_FALLBACK
    } else {
        TAB_TITLE_FALLBACK
    };
    current_file.and_then(Path::file_name).map_or_else(
        || fallback.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::ffi::OsStr;

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

    // === tab_title ===

    /// A file several directories deep. None of its directory components is the
    /// basename, so a title that answered with a parent directory — or with the
    /// whole path — is visible rather than accidentally equal.
    const NESTED_FILE: &str = "/home/jane/ledger/vendor/entry.md";

    /// [`NESTED_FILE`]'s basename: what the tab must draw.
    const NESTED_BASENAME: &str = "entry.md";

    /// One [`tab_title`] table row.
    struct TitleCase {
        desc: &'static str,
        current_file: Option<&'static str>,
        want: &'static str,
    }

    /// The string one tab draws, for every shape the pane's current file can take.
    ///
    /// FALSIFYING EDIT: return the whole path. Rows 1 and 3 fail. The cost is that
    /// a tab bar allots each tab a small fraction of the window's width, so an
    /// absolute path renders as an ellipsised middle with neither the project nor
    /// the file legible — every tab looks the same.
    ///
    /// The premise assertions come first: the last three rows rest on
    /// `std::path::Path::file_name`'s behaviour for a trailing separator, for the
    /// filesystem root and for the empty path, and a premise about the standard
    /// library is exactly the kind that is assumed rather than measured.
    #[test]
    fn tab_title_table() {
        assert_eq!(
            Path::new("/a/b/").file_name(),
            Some(OsStr::new("b")),
            "precondition: a trailing separator must contribute no path component, or the \
             trailing-separator row below is not testing the branch it names"
        );
        assert!(
            Path::new("/").file_name().is_none(),
            "precondition: the filesystem root must have no final component, or the fallback row \
             for it below never reaches the branch it exists to pin"
        );
        assert!(
            Path::new("").file_name().is_none(),
            "precondition: the empty path must have no final component, or the fallback row for it \
             below never reaches the branch it exists to pin"
        );

        let cases = [
            TitleCase {
                desc: "THE ROW THIS TABLE EXISTS FOR — a real file is drawn by its NAME, never by \
                       its path",
                current_file: Some(NESTED_FILE),
                want: NESTED_BASENAME,
            },
            TitleCase {
                desc: "a relative path is drawn by its final component too",
                current_file: Some("vendor/entry.md"),
                want: NESTED_BASENAME,
            },
            TitleCase {
                desc: "a file at the filesystem root is still just its name",
                current_file: Some("/entry.md"),
                want: NESTED_BASENAME,
            },
            TitleCase {
                desc: "a trailing separator contributes no component, so the directory name shows",
                current_file: Some("/home/jane/ledger/"),
                want: "ledger",
            },
            TitleCase {
                desc: "the filesystem root has no name to show, so the fallback does",
                current_file: Some("/"),
                want: TAB_TITLE_FALLBACK,
            },
            TitleCase {
                desc: "the empty path has no name either",
                current_file: Some(""),
                want: TAB_TITLE_FALLBACK,
            },
            TitleCase {
                desc: "no current file at all — an unnamed buffer, or a pane not yet bound",
                current_file: None,
                want: TAB_TITLE_FALLBACK,
            },
        ];

        for case in &cases {
            let got = tab_title(false, case.current_file.map(Path::new));
            assert_eq!(
                got, case.want,
                "tab_title({:?}) must be {:?}, got {got:?} — {}. The tab bar gives each tab a \
                 small fraction of the window's width: a path there is ellipsised in the middle, \
                 so neither the project nor the file is legible and every tab of one project looks \
                 identical. An empty title makes the tab a bare close button with nothing to aim \
                 at.",
                case.current_file, case.want, case.desc
            );
        }

        // --- Non-vacuity, in both directions ----------------------------------
        assert!(
            cases.iter().any(|case| case.want != TAB_TITLE_FALLBACK),
            "no tab_title row expects a real file name, so a seam that answered the fallback for \
             everything would satisfy every row above while no tab ever showed what it holds."
        );
        assert!(
            cases.iter().any(|case| case.want == TAB_TITLE_FALLBACK),
            "no tab_title row expects the fallback, so the no-name branch is unmeasured and a \
             panicking or empty answer there would go unnoticed."
        );
    }

    /// A tab title is a NAME, and specifically not any of the look-alikes the same
    /// path could have produced.
    ///
    /// The discriminating negatives are the point: asserting only that the title is
    /// `"entry.md"` would also pass for a seam that happened to return a substring
    /// of the path, and asserting only that it is short would pass for a seam that
    /// returned a parent directory. Both wrong answers are named here and both are
    /// excluded.
    #[test]
    fn a_tab_title_is_a_file_name_and_never_a_path() {
        let got = tab_title(false, Some(Path::new(NESTED_FILE)));

        assert_eq!(
            got, NESTED_BASENAME,
            "tab_title({NESTED_FILE:?}) must be {NESTED_BASENAME:?}, got {got:?}"
        );
        assert_ne!(
            got, NESTED_FILE,
            "tab_title returned the whole path. The tab bar would render an ellipsised absolute \
             path in a few dozen pixels, which names neither the project nor the file."
        );
        for look_alike in ["vendor", "ledger", "jane", "home"] {
            assert_ne!(
                got, look_alike,
                "tab_title returned {look_alike:?} — a DIRECTORY component of {NESTED_FILE:?} \
                 rather than its final one. A seam that took the wrong end of the path, or that \
                 took the parent, would label every tab of one project identically."
            );
        }
        assert!(
            !got.contains(std::path::MAIN_SEPARATOR),
            "tab_title({NESTED_FILE:?}) returned {got:?}, which still contains a path separator. \
             The rule is a file NAME, never a path — see the module docs."
        );
        assert!(
            !got.trim().is_empty(),
            "tab_title({NESTED_FILE:?}) returned {got:?}, which draws as an empty tab: a bare \
             close button with nothing to aim at and nothing to tell it from its neighbour."
        );

        // The fallback obeys the same two rules, being drawn in the same place; its literal is not
        // pinned, since nothing outside this crate reads it.
        assert!(
            !TAB_TITLE_FALLBACK.trim().is_empty(),
            "TAB_TITLE_FALLBACK is empty or all whitespace, so a tab holding an unnamed buffer \
             draws as a bare close button"
        );
        assert!(
            !TAB_TITLE_FALLBACK.contains(std::path::MAIN_SEPARATOR),
            "TAB_TITLE_FALLBACK ({TAB_TITLE_FALLBACK:?}) contains a path separator, so the \
             no-name case renders as a path in a bar that has no room for one"
        );
    }

    /// A file name that is not valid UTF-8 is rendered LOSSILY rather than dropped.
    ///
    /// Neovim will hand back whatever bytes the filesystem holds, and a tab with no
    /// title is unclickable in practice — the user cannot tell it from its
    /// neighbour. The replacement character is the honest rendering; the fallback
    /// would be a silent lie about the buffer being unnamed.
    ///
    /// The premise — that `to_string_lossy` substitutes `U+FFFD` for the invalid
    /// byte — is measured against the real standard library first, because the
    /// assertion below is meaningless if it does not.
    #[cfg(unix)]
    #[test]
    fn a_non_utf8_file_name_is_rendered_lossily_rather_than_dropped() {
        use std::os::unix::ffi::OsStrExt;

        // `0xE9` is `é` in Latin-1 and an invalid UTF-8 byte on its own.
        let raw = OsStr::from_bytes(b"/home/jane/ledger/caf\xE9.md");
        let path = Path::new(raw);

        let expected = "caf\u{FFFD}.md";
        let measured = path.file_name().map(|name| name.to_string_lossy());
        assert_eq!(
            measured.as_deref(),
            Some(expected),
            "precondition: `Path::file_name` followed by `to_string_lossy` must substitute U+FFFD \
             for the invalid byte, or the assertion below is measuring something other than the \
             rule it names"
        );

        let got = tab_title(false, Some(path));
        assert_eq!(
            got, expected,
            "tab_title must render an invalid-UTF-8 file name lossily, got {got:?}. Falling back \
             to {TAB_TITLE_FALLBACK:?} here would tell the user the buffer is unnamed when it is \
             not, and dropping the tab's title entirely leaves a tab that cannot be told from its \
             neighbour."
        );
        assert_ne!(
            got, TAB_TITLE_FALLBACK,
            "tab_title fell back to {TAB_TITLE_FALLBACK:?} for a file that HAS a name, merely not \
             a UTF-8 one. The bytes came from the filesystem and Neovim will keep handing them \
             back; the tab must show what it can of them."
        );
    }
}
