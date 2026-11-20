//! The pure, GTK-free Zoom Pane rules: which pane is zoomed, what unzooms it, and what a
//! zoom freezes or redirects.
//!
//! Zoom is front-end geometry. Nothing here reaches Neovim, the [`crate::layout::PaneTree`]
//! or its weights, which is what lets an unzoom restore the layout exactly: the pane view
//! holds only the zoomed pane's id and asks these functions what it means.

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
    /// The view's panes changed, as after a close.
    PanesChanged,
}

/// The zoomed pane after `event`, or `None` for an unzoomed view.
///
/// `panes` is every pane of the view, the tree's leaves plus the drawer when it is open, so
/// the drawer can be zoomed, and one pane with no drawer, which has nothing to hide, never
/// is. Only the zoomed pane leaving `panes` unzooms on [`ZoomEvent::PanesChanged`]: a hidden
/// pane closing keeps the zoom.
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
        ZoomEvent::Split | ZoomEvent::DrawerOpen => None,
        ZoomEvent::PanesChanged => zoomed.filter(|z| panes.contains(z)),
    }
}

/// Whether a zoom freezes focus moves: `<C-w>h/j/k/l` does nothing while zoomed, since no
/// other pane is visible to move to. The user chose this over unzooming.
///
/// It covers focus moves only. No divider exists while zoomed, so there is none to freeze.
#[must_use]
pub fn freezes_focus(zoomed: Option<PaneId>) -> bool {
    zoomed.is_some()
}

/// Whether Toggle Terminal acts: always unzoomed, and while zoomed only when the drawer is
/// the zoomed pane, where closing it is closing the zoomed pane. The user chose this over
/// unzooming on a drawer toggle.
#[must_use]
pub fn allows_toggle_terminal(zoomed: Option<PaneId>, drawer: Option<PaneId>) -> bool {
    zoomed.is_none() || zoomed == drawer
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

/// Whether Neovim's current window, resolved to pane `resolved`, must be sent back to the
/// zoomed pane: it left by a path the view does not intercept (`<C-w>w`, `:wincmd`), and the
/// user would otherwise type into a hidden pane.
#[must_use]
pub fn refocuses(resolved: Option<PaneId>, zoomed: Option<PaneId>) -> bool {
    zoomed.is_some() && resolved.is_some() && resolved != zoomed
}

/// The pane the pane area shows alone, or `None` to show the whole layout.
///
/// `bound` is every pane with widgets. A zoomed id not among them shows the whole layout,
/// so a missed unzoom never blanks the pane area.
#[must_use]
pub fn shown_alone(zoomed: Option<PaneId>, bound: &[PaneId]) -> Option<PaneId> {
    zoomed.filter(|z| bound.contains(z))
}
