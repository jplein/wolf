//! Tests for `wolf_gnome::tabs_ui::tab_title`'s empty-tab fallback: an empty tab showing no file
//! is titled as an empty window is, and one showing a file is titled by it, as a folder tab is.
//!
//! Pure: no display, no Neovim and no compositor. Every expected value is written out by hand.
//! Whether `TabHandle::refresh` passes the tab's own emptiness is the New Tab smoke's question,
//! in `smoke_headless/new_tab.rs`.

use std::path::Path;

use wolf_gnome::tabs_ui::{tab_title, TAB_TITLE_FALLBACK};
use wolf_gnome::WINDOW_TITLE_FALLBACK;

/// A file opened into an empty tab from outside, named unlike either fallback.
const OUTSIDE_FILE: &str = "/x/notes/entry.md";

/// TT1.
#[test]
fn an_empty_tab_with_no_file_is_titled_wolf() {
    assert_ne!(
        WINDOW_TITLE_FALLBACK, TAB_TITLE_FALLBACK,
        "premise: the two fallbacks differ, or the assertions below cannot tell them apart"
    );
    assert_eq!(
        tab_title(true, None),
        WINDOW_TITLE_FALLBACK,
        "an empty tab showing no file is titled as an empty window is. {TAB_TITLE_FALLBACK:?} \
         means tab_title ignored `empty` and answered a folder tab's fallback"
    );
    assert_eq!(
        tab_title(false, None),
        TAB_TITLE_FALLBACK,
        "a folder tab showing no file keeps its own fallback. {WINDOW_TITLE_FALLBACK:?} means \
         every tab with no file is titled as an empty one, whatever `empty` says"
    );
}

/// TT2.
#[test]
fn an_empty_tab_showing_a_file_is_titled_by_it() {
    assert_eq!(
        tab_title(true, Some(Path::new(OUTSIDE_FILE))),
        "entry.md",
        "an empty tab showing a file opened from outside is titled by that file's name. \
         {WINDOW_TITLE_FALLBACK:?} means the empty fallback was answered before the file was read"
    );
    assert_eq!(
        tab_title(false, Some(Path::new(OUTSIDE_FILE))),
        "entry.md",
        "a folder tab is titled by its file's name, as it is with no empty-tab rule at all"
    );
}
