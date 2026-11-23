//! The pure, GTK-free routing of a path opened from outside Wolf
//! (`specs/cli-and-file-open.md` §3, §4 and §5): which tab is current, whether a request goes
//! to Open Folder, to a named tab or to a file's project ([`route`]), which leaf core's pane
//! choice names ([`placement`]), and when a file may be placed at all ([`may_place`]).
//!
//! The coordinator gathers the live windows and acts on the answer; nothing here touches a
//! widget, a Neovim or the store.

use std::collections::HashMap;
use std::path::PathBuf;

use wolf_core::{Liveness, OpenTarget, PanePlacement};

use crate::coordinator::WindowId;
use crate::layout::PaneId;
use crate::open_folder::LiveWindow;
use crate::stacking::StackingOrder;
use crate::tabs_ui::TabId;

/// A tab of a live window.
pub type TabRef = (WindowId, TabId);

/// What the coordinator does with one request. `current` is §4's current tab, `None` with no
/// live window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// No path: select the current tab and raise its window.
    Activate { current: Option<TabRef> },
    /// Open Folder on `directory`, with `current` as the tab it fills when empty.
    OpenFolder {
        directory: PathBuf,
        current: Option<TabRef>,
    },
    /// §5.1 rule 1: open `file` in `tab`, which the request's live `WOLF_TAB_ID` names.
    OpenFile { file: PathBuf, tab: TabRef },
    /// Open Folder on `file`'s project from `current`, then open `file` in the tab that lands.
    OpenFileInProject {
        file: PathBuf,
        project: PathBuf,
        current: Option<TabRef>,
    },
}

/// The route for a request whose path names `target`, carrying `tab_id`.
///
/// `tab_ids` maps each tab's `WOLF_TAB_ID` to its tab. An ID is live only when that window
/// holds that tab in `windows`, since a [`TabId`] is unique only within its window, and a
/// request with no live ID is handled as if it had none (§3). A file's project is
/// [`wolf_core::project_directory`] over every live tab's non-empty directory.
#[must_use]
pub fn route(
    target: &OpenTarget,
    tab_id: Option<wolf_core::TabId>,
    windows: &[LiveWindow],
    tab_ids: &HashMap<wolf_core::TabId, TabRef>,
    stacking: &StackingOrder,
) -> Route {
    let live = tab_id
        .and_then(|id| tab_ids.get(&id).copied())
        .filter(|(window, tab)| {
            windows
                .iter()
                .any(|w| w.id == *window && w.tabs.iter().any(|(t, _)| t == tab))
        });
    let current = live.or_else(|| current_tab(windows, stacking));
    match target {
        OpenTarget::Activate => Route::Activate { current },
        OpenTarget::Directory(directory) => Route::OpenFolder {
            directory: directory.clone(),
            current,
        },
        OpenTarget::File(file) => match live {
            Some(tab) => Route::OpenFile {
                file: file.clone(),
                tab,
            },
            None => {
                let open: Vec<PathBuf> = windows
                    .iter()
                    .flat_map(|w| &w.tabs)
                    .filter(|(_, dir)| !dir.is_empty())
                    .map(|(_, dir)| PathBuf::from(dir))
                    .collect();
                Route::OpenFileInProject {
                    file: file.clone(),
                    project: wolf_core::project_directory(file, &open),
                    current,
                }
            }
        },
    }
}

/// The frontmost window's active tab, §4's current tab when no live ID names one: the first
/// window of `stacking`'s order, its selected tab, else its most recently selected one. `None`
/// with no window, or a frontmost window with no tab.
#[must_use]
pub fn current_tab(windows: &[LiveWindow], stacking: &StackingOrder) -> Option<TabRef> {
    let ids: Vec<WindowId> = windows.iter().map(|w| w.id).collect();
    let front = stacking.order(&ids).into_iter().next()?;
    let window = windows.iter().find(|w| w.id == front)?;
    let tabs: Vec<TabId> = window.tabs.iter().map(|(t, _)| *t).collect();
    let tab = window
        .selected
        .filter(|selected| tabs.contains(selected))
        .or_else(|| {
            window
                .recency
                .indices(&tabs)
                .first()
                .map(|&index| tabs[index])
        })?;
    Some((front, tab))
}

/// Where a file goes in its tab: core's [`PanePlacement`] with its index resolved to a leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// An editor pane already shows the file: focus it.
    Focus(PaneId),
    /// Open the file in this editor pane.
    Open(PaneId),
    /// Make this preview pane an editor pane showing the file.
    Convert(PaneId),
    /// Split the whole tab, the file in a new editor pane on the left.
    SplitLeft,
}

/// `choice` with its depth-first index mapped to the leaf at that index of `leaves`, the
/// tab's [`crate::layout::PaneTree::leaves`]; `None` for an index past the last leaf.
#[must_use]
pub fn placement(choice: PanePlacement, leaves: &[PaneId]) -> Option<Placement> {
    match choice {
        PanePlacement::Focus(i) => leaves.get(i).copied().map(Placement::Focus),
        PanePlacement::Open(i) => leaves.get(i).copied().map(Placement::Open),
        PanePlacement::Convert(i) => leaves.get(i).copied().map(Placement::Convert),
        PanePlacement::SplitLeft => Some(Placement::SplitLeft),
    }
}

/// Whether a file may be placed in a tab whose Neovim is `liveness`: `Alive` alone, since
/// the whole-tab split is a blocking call that a stalled or dead Neovim never answers.
///
/// TOTAL, with no `_` arm, as [`crate::state_seam::may_capture_cursor`] is.
#[must_use]
pub fn may_place(liveness: Option<Liveness>) -> bool {
    match liveness {
        Some(Liveness::Alive) => true,
        Some(Liveness::NotResponding | Liveness::Dead) | None => false,
    }
}
