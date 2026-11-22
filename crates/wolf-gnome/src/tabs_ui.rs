//! The pure, GTK-free rulebook for NATIVE TABS in `wolf-gnome`: what closing one
//! does, the working directory a new tab's Neovim starts in, the string one tab
//! draws in the bar, and which tab's Neovim holds the UI focus.
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

use std::path::{Path, PathBuf};

/// The title a tab draws when its pane shows no named file.
///
/// Deliberately NOT Neovim's own `[No Name]`: that string is what
/// `WolfCore::pane_status` answers for a window it cannot resolve, so reusing it
/// would make a genuinely unnamed buffer indistinguishable from a failed lookup.
/// [`crate::title::WINDOW_TITLE_FALLBACK`] is the window title's fallback, one
/// widget down.
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

/// The directory a new tab's Neovim starts in, or `None` when the window has no
/// project to give it.
///
/// A new tab inherits the SELECTED tab's folder, which is what keeps a new tab
/// rooted where its sidebar, picker and project search already are.
///
/// The EMPTY state answers `None` ALWAYS — never the fallback working directory a
/// project-less launch handed Neovim, which is `$HOME` (or `/`). Treating that as
/// the tab's folder would silently declare the user's home directory a project.
/// With a project open it is Neovim's LIVE cwd once reported, else the launch cwd,
/// so a New Tab pressed before the first `DirChanged` does not lose the project.
///
/// The same three rules as [`crate::state_seam::snapshot_cwd`], which persists the
/// tab's cwd — with `None` where that one answers `""`. The two must agree, or a
/// restored window would open a different directory than its tabs are rooted at.
#[must_use]
pub fn new_tab_cwd(empty: bool, launch_cwd: &Path, live_cwd: Option<&Path>) -> Option<PathBuf> {
    if empty {
        return None;
    }
    Some(live_cwd.unwrap_or(launch_cwd).to_path_buf())
}

/// The text one tab draws in the bar: the FILE NAME of the pane's current file, or
/// [`TAB_TITLE_FALLBACK`] when there is none.
///
/// Never a path. A tab bar allots each tab a small fraction of the window's width,
/// so an absolute path renders as an ellipsised middle with neither the project nor
/// the file legible. The same final-component rule as
/// [`crate::workspace::root_label`], with a different fallback: a path with no final
/// component (`/`, or the empty path) carries no name to show, so it falls back
/// rather than drawing the separator.
#[must_use]
pub fn tab_title(current_file: Option<&Path>) -> String {
    current_file.and_then(Path::file_name).map_or_else(
        || TAB_TITLE_FALLBACK.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::ffi::OsStr;

    use crate::state_seam::snapshot_cwd;

    /// The fallback working directory a project-less launch hands Neovim. Never
    /// the answer [`new_tab_cwd`] may give.
    const HOME: &str = "/home/jane";

    /// The directory a window with a project open was launched at.
    const PROJECT: &str = "/home/jane/ledger";

    /// A directory Neovim reported LATER, after a `:cd`. Distinct from
    /// [`PROJECT`], so "the live cwd wins" is distinguishable from "the launch cwd
    /// is returned".
    const WANDERED: &str = "/home/jane/ledger/vendor";

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

    // === new_tab_cwd ===

    /// One [`new_tab_cwd`] table row.
    struct CwdCase {
        desc: &'static str,
        empty: bool,
        launch_cwd: &'static str,
        live_cwd: Option<&'static str>,
        want: Option<&'static str>,
    }

    /// The five rows of the inherited-cwd rule.
    ///
    /// A `const` item so [`the_inherited_cwd_agrees_with_the_persisted_one`] can
    /// walk the same rows without copying them.
    const CWD_CASES: &[CwdCase] = &[
        CwdCase {
            desc: "a project-less window with a live cwd still inherits NOTHING",
            empty: true,
            launch_cwd: HOME,
            live_cwd: Some(HOME),
            want: None,
        },
        CwdCase {
            desc: "a project-less window with no live cwd inherits nothing",
            empty: true,
            launch_cwd: HOME,
            live_cwd: None,
            want: None,
        },
        CwdCase {
            desc: "a project-less window whose Neovim wandered into a real directory STILL \
                   inherits nothing",
            empty: true,
            launch_cwd: HOME,
            live_cwd: Some(PROJECT),
            want: None,
        },
        CwdCase {
            desc: "a window with a project inherits Neovim's LIVE cwd once it has reported one",
            empty: false,
            launch_cwd: PROJECT,
            live_cwd: Some(WANDERED),
            want: Some(WANDERED),
        },
        CwdCase {
            desc: "a window whose Neovim has not reported yet inherits the LAUNCH cwd, so a New \
                   Tab pressed immediately does not lose the project",
            empty: false,
            launch_cwd: PROJECT,
            live_cwd: None,
            want: Some(PROJECT),
        },
    ];

    /// The directory a new tab's Neovim starts in, for every shape the window can
    /// be in.
    ///
    /// FALSIFYING EDIT: return the launch cwd when `empty`. The three empty rows
    /// fail, and the cost is named in the message: a project-less window's Neovim
    /// was handed `$HOME` (or `/`) purely because a process needs a working
    /// directory, so inheriting it declares the user's home directory a project.
    #[test]
    fn new_tab_cwd_table() {
        for case in CWD_CASES {
            let got = new_tab_cwd(
                case.empty,
                Path::new(case.launch_cwd),
                case.live_cwd.map(Path::new),
            );
            assert_eq!(
                got.as_deref(),
                case.want.map(Path::new),
                "new_tab_cwd(empty = {}, launch_cwd = {:?}, live_cwd = {:?}) must be {:?} — {}. \
                 The answer is the new tab's project: a directory where there should be None \
                 treats the fallback working directory a project-less launch handed Neovim, \
                 which is $HOME, as a project, and points that tab's sidebar, picker and project \
                 search at the whole home tree. None where there should be a directory starts \
                 the new tab's Neovim at the launch cwd instead of the selected tab's folder, so \
                 the two tabs of one window disagree about where the project is.",
                case.empty,
                case.launch_cwd,
                case.live_cwd,
                case.want,
                case.desc
            );
        }

        // Non-vacuity: a `new_tab_cwd` of `|_, _, _| None` satisfies all three empty
        // rows while every new tab in every project silently opens at the global cwd.
        assert!(
            CWD_CASES.iter().any(|case| case.want.is_some()),
            "no new_tab_cwd row expects a directory, so a seam that answered None for everything \
             would satisfy every row above while no new tab ever inherited its window's folder."
        );
        // And the complement: a seam that answered `Some(launch_cwd)` for everything
        // satisfies the two project rows only if one of them prefers the LIVE cwd.
        assert!(
            CWD_CASES
                .iter()
                .any(|case| !case.empty && case.live_cwd.is_some() && case.want == case.live_cwd),
            "no new_tab_cwd row asserts that a reported LIVE cwd wins over the launch cwd, so a \
             seam that ignored live_cwd entirely would pass. A window whose user has typed `:cd` \
             would then open every new tab at the directory the window was launched at, which is \
             no longer the project."
        );
    }

    /// The rule stated on its own, over every live cwd, so a failure NAMES it.
    ///
    /// Mirrors `state_seam::the_empty_state_persists_no_cwd`, and for the same
    /// reason: the fallback working directory a project-less launch hands Neovim is
    /// a process requirement, not a project.
    #[test]
    fn a_project_less_window_inherits_no_cwd() {
        for live_cwd in [None, Some(HOME), Some(PROJECT), Some(WANDERED), Some("/")] {
            assert_eq!(
                new_tab_cwd(true, Path::new(HOME), live_cwd.map(Path::new)),
                None,
                "a project-less window must inherit NOTHING for every live cwd (here {live_cwd:?}) \
                 and every launch cwd. A directory here treats the fallback working directory \
                 the launch gave Neovim — $HOME — as a project the user opened, and roots that \
                 tab's file search at the whole home tree."
            );
        }
    }

    /// The inherited cwd and the PERSISTED cwd are the same directory.
    ///
    /// The two rules live in different modules because they answer different
    /// questions and have different types, but they must not diverge: a window
    /// persists [`crate::state_seam::snapshot_cwd`]'s answer and reopens there,
    /// while every tab it opens is rooted at [`new_tab_cwd`]'s. If they disagree, a
    /// relaunch puts the window's tabs somewhere the window itself is not.
    ///
    /// `None` here is `""` there — the same "no project" sentinel in the two shapes
    /// the two callers need.
    #[test]
    fn the_inherited_cwd_agrees_with_the_persisted_one() {
        for case in CWD_CASES {
            let launch = Path::new(case.launch_cwd);
            let live = case.live_cwd.map(Path::new);

            let inherited = new_tab_cwd(case.empty, launch, live);
            let persisted = snapshot_cwd(case.empty, launch, live);
            let inherited_as_persisted = inherited
                .as_ref()
                .map_or_else(String::new, |dir| dir.to_string_lossy().into_owned());

            assert_eq!(
                inherited_as_persisted, persisted,
                "new_tab_cwd(empty = {}, launch_cwd = {:?}, live_cwd = {:?}) inherits \
                 {inherited:?} while state_seam::snapshot_cwd persists {persisted:?} for the same \
                 window — {}. \
                 The two must name the same directory, with None here matching \"\" there: the \
                 window is reopened at the persisted one and its tabs are rooted at the inherited \
                 one, so a divergence puts a restored window's tabs in a directory the window is \
                 not in.",
                case.empty, case.launch_cwd, case.live_cwd, case.desc
            );
        }
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
            let got = tab_title(case.current_file.map(Path::new));
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
        let got = tab_title(Some(Path::new(NESTED_FILE)));

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

        let got = tab_title(Some(path));
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
