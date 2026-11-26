//! Tests for `wolf_gnome::tabs_ui::tab_label`, the title and tooltip one tab shows: a folder tab
//! is titled by its working directory as the window title renders it, `~/…` under home and the
//! absolute path elsewhere, with the absolute path, Pango-escaped, as its tooltip; an empty tab, or
//! a folder tab whose Neovim has reported no directory yet, is titled "Wolf" with no tooltip.
//!
//! Pure: no display, no Neovim and no compositor. Every expected value is written out by hand.
//! Whether `TabHandle::refresh_label` passes the tab's own emptiness is the New Tab smoke's
//! question, in `smoke_headless/new_tab.rs`; whether each tab follows its own Neovim's directory
//! live is the tab bar smoke's, in `smoke_headless/tab_bar.rs`.

use std::path::Path;

use wolf_gnome::tabs_ui::{tab_label, TabLabel};
use wolf_gnome::WINDOW_TITLE_FALLBACK;

/// The home directory every row is abbreviated against, injected as the shell injects its own.
const HOME: &str = "/home/jane";

/// One row's call, `tab_label(empty, cwd, HOME)`.
fn label(empty: bool, cwd: Option<&str>) -> TabLabel {
    tab_label(empty, cwd.map(Path::new), HOME)
}

/// TL1.
#[test]
fn a_folder_tab_under_home_is_titled_with_a_tilde() {
    for (cwd, want) in [("/home/jane/ledger", "~/ledger"), ("/home/jane", "~")] {
        assert_eq!(
            label(false, Some(cwd)).title,
            want,
            "a folder tab at {cwd:?} under home {HOME:?} is titled as the window title renders \
             it. {cwd:?} verbatim means the cwd skipped title::window_title's abbreviation; \
             \"ledger\" or \"jane\" means the tab was titled by the directory's last component"
        );
    }
}

/// TL2.
#[test]
fn a_folder_tab_outside_home_is_titled_by_its_absolute_path() {
    // `/home/janet` shares `/home/jane` as a string prefix but not a path segment.
    for cwd in ["/srv/ledger", "/home/janet/x"] {
        assert_eq!(
            label(false, Some(cwd)).title,
            cwd,
            "a folder tab outside home is titled by its whole absolute path. A last component \
             (\"ledger\", \"x\") means the tab was titled by Path::file_name; \"~t/x\" means the \
             home match ignored the path-segment boundary"
        );
    }
}

/// TL3.
#[test]
fn an_empty_tab_is_titled_wolf_with_no_tooltip() {
    for cwd in [Some("/home/jane"), Some("/srv/ledger"), None] {
        assert_eq!(
            label(true, cwd),
            TabLabel {
                title: "Wolf".to_owned(),
                tooltip: None,
            },
            "an empty tab is titled as an empty window is, with no tooltip of its own, whatever \
             its Neovim's cwd ({cwd:?}). A directory as the title, or Some(path) as the tooltip, \
             means tab_label ignored `empty`"
        );
    }
    assert_eq!(
        WINDOW_TITLE_FALLBACK, "Wolf",
        "premise: an empty tab's title is the window's own fallback, which the rows above spell"
    );
}

/// TL4.
#[test]
fn a_folder_tab_with_no_directory_yet_is_titled_wolf_with_no_tooltip() {
    assert_eq!(
        label(false, None),
        TabLabel {
            title: "Wolf".to_owned(),
            tooltip: None,
        },
        "a folder tab whose Neovim has reported no cwd is titled as its window is then, with no \
         tooltip. A title of \"\" means title::display_title was used without the window's \
         fallback; a tooltip of Some(\"\") means the missing cwd was rendered rather than omitted"
    );
}

/// TL5.
#[test]
fn the_tooltip_is_the_absolute_directory() {
    for cwd in ["/home/jane/ledger", "/srv/ledger"] {
        assert_eq!(
            label(false, Some(cwd)).tooltip.as_deref(),
            Some(cwd),
            "a folder tab's tooltip is its cwd as a full absolute path, never abbreviated. \
             Some(\"~/ledger\") means the tooltip was taken from the title; None means a folder \
             tab with a known cwd set no tooltip"
        );
    }
}

/// TL6.
#[test]
fn markup_is_escaped_in_the_tooltip_and_not_in_the_title() {
    for (cwd, title, tooltip) in [
        ("/srv/a&b<c>d", "/srv/a&b<c>d", "/srv/a&amp;b&lt;c&gt;d"),
        (
            "/home/jane/R&D <x>",
            "~/R&D <x>",
            "/home/jane/R&amp;D &lt;x&gt;",
        ),
    ] {
        let got = label(false, Some(cwd));
        assert_eq!(
            got.tooltip.as_deref(),
            Some(tooltip),
            "AdwTabPage:tooltip is Pango markup, so the tooltip escapes `&`, `<` and `>`. The \
             raw path means markup_escape_text was skipped and the tooltip fails to parse; a \
             `~` means it was built from the title"
        );
        assert_eq!(
            got.title, title,
            "AdwTabPage:title is plain text, so the title keeps `&`, `<` and `>` literally. \
             `&amp;` or `&lt;` in it means the title was escaped too and draws the entities"
        );
    }
}
