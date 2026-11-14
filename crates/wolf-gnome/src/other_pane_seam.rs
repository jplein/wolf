//! The pure, GTK-free rule for which pane Preview or Edit in Another Pane binds into:
//! an existing pane of the [`PaneTree`], or a split core makes for the bind.
//!
//! Data in, data out — no widget, no `WolfCore`, no display — so the whole rule runs in
//! the fast unit tier. [`crate::pane_view`] resolves the answer to a Neovim window and
//! [`crate::shell`] hands that window to core.

use wolf_core::FocusDir;

use crate::layout::{PaneId, PaneTree};

/// The side a command prefers: Preview in Another Pane prefers [`Side::Right`], Edit in
/// Another Pane prefers [`Side::Left`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Prefer the pane to the left of the active pane.
    Left,
    /// Prefer the pane to the right of the active pane.
    Right,
}

/// Where the bind lands: a fresh split, which core's helper makes and binds in one step,
/// or an existing pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// No other pane exists, so core is asked to split.
    Split,
    /// Reuse this existing pane, displacing what it shows.
    Existing(PaneId),
}

/// Resolves the pane an other-pane bind lands in. Purely geometric: no pane is preferred
/// for what it shows, so a fallback pane that holds a preview is repointed like any other.
///
/// Rules, in order:
///
/// 1. `active` is not a leaf of `tree` (an unknown pane, or the terminal drawer, which
///    lives outside the tree) — `None`, so the command is a no-op rather than a stray split.
/// 2. The tree is a single leaf — [`Target::Split`].
/// 3. [`PaneTree::neighbor`] resolves on the preferred side ([`Side::Right`] asks
///    [`FocusDir::Right`], [`Side::Left`] asks [`FocusDir::Left`]) — that pane.
/// 4. Otherwise the next leaf after `active` in [`PaneTree::leaves`] order for
///    [`Side::Right`], or the previous one for [`Side::Left`], wrapping around.
///
/// `width`/`height` must be the bounds the caller's focus resolution uses, so rule 3's
/// neighbour is the one a `<C-w>h/l` from `active` would reach.
#[must_use]
pub fn target(
    active: PaneId,
    prefer: Side,
    tree: &PaneTree,
    width: f64,
    height: f64,
) -> Option<Target> {
    let leaves = tree.leaves();
    let index = leaves.iter().position(|&id| id == active)?;
    if leaves.len() == 1 {
        return Some(Target::Split);
    }

    let dir = match prefer {
        Side::Right => FocusDir::Right,
        Side::Left => FocusDir::Left,
    };
    if let Some(neighbor) = tree.neighbor(active, dir, width, height) {
        return Some(Target::Existing(neighbor));
    }

    let offset = match prefer {
        Side::Right => 1,
        Side::Left => leaves.len() - 1,
    };
    Some(Target::Existing(leaves[(index + offset) % leaves.len()]))
}
