//! The pure, GTK-free Zoom Pane rules: which pane is zoomed, which events unzoom it or
//! re-zoom it, and where a focus move or an other-pane command goes while zoomed.
//!
//! Zoom is front-end geometry. Nothing here reaches Neovim, the [`crate::layout::PaneTree`]
//! or its weights, which is what lets an unzoom restore the layout exactly: the pane view
//! holds only the zoomed pane's id and asks these functions what it means.

use wolf_core::app_state::PaneRef;

use crate::layout::PaneId;
use crate::other_pane_seam::Target;

/// What happened to the pane view, for [`zoomed_after`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomEvent {
    /// Zoom Pane: zoom the active pane, or unzoom.
    Toggle,
    /// The view applies a split, from any source. It unzooms first, so the new pane shows.
    Split,
    /// The view opens the drawer: a pane joins the layout, as with a split.
    DrawerOpen,
    /// A file opened from outside Wolf lands in the view; it unzooms first, so its pane shows.
    OpenFile,
    /// The view's panes changed, as after a close.
    PanesChanged,
    /// A focus move, by chord or by Neovim's own window change, to `target`: the pane the move
    /// names in the unzoomed layout, or `None` when it names none.
    FocusMove { target: Option<PaneId> },
    /// Toggle Terminal, with `drawer` the open drawer's pane, or `None` when it is closed.
    ToggleTerminal { drawer: Option<PaneId> },
    /// A launch rebuild re-zooms the pane restored from the saved zoomed leaf.
    Restore(PaneId),
}

/// The zoomed pane after `event`, or `None` for an unzoomed view.
///
/// `panes` is every pane of the view, the tree's leaves plus the drawer when it is open, so
/// the drawer can be zoomed, and one pane with no drawer, which has nothing to hide, never
/// is. Only the zoomed pane leaving `panes` unzooms on [`ZoomEvent::PanesChanged`]: a hidden
/// pane closing keeps the zoom.
///
/// A focus move unzooms when it has a target other than the zoomed pane. Toggle Terminal
/// unzooms unless the drawer is the zoomed pane, whose close then unzooms on
/// [`ZoomEvent::PanesChanged`]. [`ZoomEvent::Restore`] ignores `zoomed` and `active`.
#[must_use]
pub fn zoomed_after(
    event: ZoomEvent,
    zoomed: Option<PaneId>,
    active: Option<PaneId>,
    panes: &[PaneId],
) -> Option<PaneId> {
    match event {
        ZoomEvent::Toggle => {
            if zoomed.is_some() {
                return None;
            }
            active.filter(|a| panes.contains(a) && panes.len() > 1)
        }
        ZoomEvent::Split | ZoomEvent::DrawerOpen | ZoomEvent::OpenFile => None,
        ZoomEvent::PanesChanged => zoomed.filter(|z| panes.contains(z)),
        ZoomEvent::FocusMove { target } => match target {
            Some(t) if Some(t) != zoomed => None,
            _ => zoomed,
        },
        ZoomEvent::ToggleTerminal { drawer } => zoomed.filter(|z| Some(*z) == drawer),
        ZoomEvent::Restore(p) => Some(p).filter(|p| panes.contains(p) && panes.len() > 1),
    }
}

/// The pane a focus move starts from: the zoomed pane while zoomed, whatever window the move
/// was made in, else `from`.
#[must_use]
pub fn focus_origin(zoomed: Option<PaneId>, from: Option<PaneId>) -> Option<PaneId> {
    zoomed.or(from)
}

/// The other-pane commands' target while zoomed. The zoomed pane is the only pane shown, so
/// a tree pane takes the split path, which unzooms, and the drawer gets `None`, as it does
/// unzoomed. Read only while zoomed, in place of [`crate::other_pane_seam::target`].
#[must_use]
pub fn other_pane_target(zoomed: PaneId, drawer: Option<PaneId>) -> Option<Target> {
    if Some(zoomed) == drawer {
        None
    } else {
        Some(Target::Split)
    }
}

/// The pane the pane area shows alone, or `None` to show the whole layout.
///
/// `bound` is every pane with widgets. A zoomed id not among them shows the whole layout,
/// so a missed unzoom never blanks the pane area.
#[must_use]
pub fn shown_alone(zoomed: Option<PaneId>, bound: &[PaneId]) -> Option<PaneId> {
    zoomed.filter(|z| bound.contains(z))
}

/// A saved `zoomed` or `focused` reference, mapped onto a restored tab's panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreTarget {
    /// The rebuilt tree leaf.
    Pane(PaneId),
    /// The drawer, which binds only once the restore reopens it.
    Drawer,
}

/// What a restored tab zooms and focuses, from [`restore_targets`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreTargets {
    /// The pane to re-zoom, or `None`.
    pub zoom: Option<RestoreTarget>,
    /// The pane to focus, or `None` to keep the first leaf.
    pub focus: Option<RestoreTarget>,
}

/// Maps a saved tab's `zoomed` and `focused` onto its restored panes: `Pane(i)` onto
/// `leaf_ids[i]`, and `Drawer` onto the drawer only when it restores open. Anything
/// else maps to `None`.
///
/// The zoom wins the focus: a restored zoom focuses the zoomed pane, since focusing a
/// pane the zoom hides would unzoom it on the next focus move.
#[must_use]
pub fn restore_targets(
    zoomed: Option<PaneRef>,
    focused: Option<PaneRef>,
    drawer_open: bool,
    leaf_ids: &[PaneId],
) -> RestoreTargets {
    let map = |r: Option<PaneRef>| match r? {
        PaneRef::Pane(i) => leaf_ids.get(i).copied().map(RestoreTarget::Pane),
        PaneRef::Drawer => drawer_open.then_some(RestoreTarget::Drawer),
    };
    let zoom = map(zoomed);
    RestoreTargets {
        zoom,
        focus: zoom.or(map(focused)),
    }
}
