//! The pure, GTK-free Open Folder seam of `specs/application-state.md` §6: which
//! step opening a directory takes ([`step`]), the plan of the tab it fills or
//! opens ([`plan`]), and the size a fill applies ([`fill_size`]).
//!
//! The coordinator gathers the live windows and acts on the answer; nothing here
//! touches a widget, a Neovim or the store.

use std::path::Path;

use wolf_core::app_state::{self, ProjectState, Tab, TabContent};

use crate::coordinator::WindowId;
use crate::stacking::{StackingOrder, TabRecency};
use crate::state_seam::{self, RestorePlan};
use crate::tabs_ui::TabId;

/// One live window, as [`step`] reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveWindow {
    pub id: WindowId,
    /// The tabs in page order, each with its live directory, `""` for an empty tab.
    pub tabs: Vec<(TabId, String)>,
    pub selected: Option<TabId>,
    pub recency: TabRecency,
}

/// The step Open Folder takes for a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// A tab is open on the directory: select it and raise its window.
    Focus { window: WindowId, tab: TabId },
    /// The invoking tab is empty: replace it with a tab on the directory.
    FillCurrent,
    /// Open a new window on the directory.
    NewWindow,
}

/// The step for `directory`, which is already canonical, from a tab that is empty
/// when `current_empty`.
///
/// The windows are put in `stacking` order and handed, directory-only, to
/// [`app_state::project_tab`], so the tab focused is the one §4's rule picks.
/// Live directories, never a tab's cached contribution: a tab whose gate is shut
/// contributes its cache, which can name another directory.
#[must_use]
pub fn step(
    directory: &str,
    windows: &[LiveWindow],
    stacking: &StackingOrder,
    current_empty: bool,
) -> Step {
    let ids: Vec<WindowId> = windows.iter().map(|w| w.id).collect();
    let entries = windows
        .iter()
        .filter_map(|window| {
            let tabs = window
                .tabs
                .iter()
                .map(|(id, dir)| {
                    let tab = Tab {
                        directory: dir.clone(),
                        content: TabContent::default(),
                    };
                    (*id, tab)
                })
                .collect();
            state_seam::window_entry(0, 0, tabs, window.selected, &window.recency)
                .map(|(entry, recent)| (window.id, entry, recent))
        })
        .collect();
    let found = state_seam::window_state(entries, stacking)
        .and_then(|(state, recency)| app_state::project_tab(&state, &recency, directory));
    if let Some((w, t)) = found {
        // `window_state` keeps the windows `window_entry` answered, in stacking order.
        let ordered: Vec<&LiveWindow> = stacking
            .order(&ids)
            .into_iter()
            .filter_map(|id| windows.iter().find(|w| w.id == id && !w.tabs.is_empty()))
            .collect();
        if let Some((tab, _)) = ordered.get(w).and_then(|window| window.tabs.get(t)) {
            return Step::Focus {
                window: ordered[w].id,
                tab: *tab,
            };
        }
    }
    if current_empty {
        Step::FillCurrent
    } else {
        Step::NewWindow
    }
}

/// The plan of the one tab Open Folder opens on `directory`: a folder tab there
/// whatever [`crate::workspace::launch_decision`] would say of it, since opening a
/// folder is an explicit act.
///
/// From `project`, its saved tab at `directory` and its stored size; without one,
/// a fresh tab and no size.
#[must_use]
pub fn plan(directory: &Path, project: Option<&ProjectState>) -> RestorePlan {
    let cwd = directory.to_path_buf();
    let (tab, window_size) = match project {
        Some(project) => {
            let saved = Tab {
                directory: directory.to_string_lossy().into_owned(),
                content: project.tab.clone(),
            };
            (
                state_seam::saved_tab_plan(&saved, cwd, false),
                state_seam::restored_window_size(project.frame.width, project.frame.height),
            )
        }
        None => (state_seam::fresh_tab_plan(cwd, false), None),
    };
    RestorePlan {
        tabs: vec![tab],
        active_tab: 0,
        window_size,
        minimized: false,
    }
}

/// The size a fill applies to its window: `plan`'s, only when the filled tab is
/// the window's only one of `window_tabs`.
#[must_use]
pub fn fill_size(plan: &RestorePlan, window_tabs: usize) -> Option<(i32, i32)> {
    plan.window_size.filter(|_| window_tabs == 1)
}
