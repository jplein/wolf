//! The pure, GTK-free window stacking order and per-window tab recency of
//! `specs/application-state.md#3-stacking-order-and-tab-recency`.
//!
//! Wayland lets a client query neither, so both are kept from events: a window that
//! gains focus moves to the front of [`StackingOrder`], Wolf's own Minimize marks a
//! window minimized, and a selected tab moves to the front of [`TabRecency`]. The
//! coordinator and the tab view feed them; nothing here touches a widget.

use std::collections::HashSet;

use crate::coordinator::WindowId;
use crate::tabs_ui::TabId;

/// Every window this process has ranked, most recently focused first, and which of
/// them are minimized.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StackingOrder {
    /// Most recently focused first.
    ranked: Vec<WindowId>,
    /// The windows Wolf's own Minimize marked, until they next gain focus.
    minimized: HashSet<WindowId>,
}

impl StackingOrder {
    /// An order seeded from a saved window state: `windows` frontmost first, each
    /// with whether it is minimized.
    #[must_use]
    pub fn seeded(windows: &[(WindowId, bool)]) -> Self {
        let mut order = Self::default();
        for &(window, minimized) in windows {
            if !order.ranked.contains(&window) {
                order.ranked.push(window);
            }
            if minimized {
                order.minimized.insert(window);
            }
        }
        order
    }

    /// `window` gained focus: it moves to the front and is no longer minimized.
    pub fn focus(&mut self, window: WindowId) {
        self.ranked.retain(|&w| w != window);
        self.ranked.insert(0, window);
        self.minimized.remove(&window);
    }

    /// `window` was minimized. Its rank is kept, so the windows minimized together
    /// keep their relative order behind the others.
    pub fn minimize(&mut self, window: WindowId) {
        self.minimized.insert(window);
    }

    /// Forgets `window`, which has closed.
    pub fn remove(&mut self, window: WindowId) {
        self.ranked.retain(|&w| w != window);
        self.minimized.remove(&window);
    }

    /// Whether `window` is marked minimized.
    #[must_use]
    pub fn is_minimized(&self, window: WindowId) -> bool {
        self.minimized.contains(&window)
    }

    /// Every window of `live` exactly once, frontmost first: the windows that are
    /// not minimized before those that are, each class by rank, and windows never
    /// ranked after the ranked ones of their class, in `live`'s order.
    #[must_use]
    pub fn order(&self, live: &[WindowId]) -> Vec<WindowId> {
        let mut out: Vec<WindowId> = live.to_vec();
        // A stable sort, so the unranked keep `live`'s order.
        out.sort_by_key(|w| {
            let rank = self
                .ranked
                .iter()
                .position(|r| r == w)
                .unwrap_or(usize::MAX);
            (self.is_minimized(*w), rank)
        });
        out
    }
}

/// One window's tabs, most recently selected first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabRecency {
    /// Most recently selected first.
    recent: Vec<TabId>,
}

impl TabRecency {
    /// A recency seeded with `tabs[active]` first and the rest in `tabs`' order, or
    /// in `tabs`' order alone when `active` is out of range.
    #[must_use]
    pub fn seeded(tabs: &[TabId], active: usize) -> Self {
        let mut recent: Vec<TabId> = tabs.get(active).copied().into_iter().collect();
        recent.extend(tabs.iter().copied().filter(|t| Some(t) != tabs.get(active)));
        Self { recent }
    }

    /// `tab` was selected: it becomes the most recent.
    pub fn select(&mut self, tab: TabId) {
        self.recent.retain(|&t| t != tab);
        self.recent.insert(0, tab);
    }

    /// Forgets `tab`, which has closed.
    pub fn remove(&mut self, tab: TabId) {
        self.recent.retain(|&t| t != tab);
    }

    /// Indices into `tabs`, most recently selected first; tabs never selected follow
    /// in `tabs`' order.
    #[must_use]
    pub fn indices(&self, tabs: &[TabId]) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .recent
            .iter()
            .filter_map(|t| tabs.iter().position(|x| x == t))
            .collect();
        let rest: Vec<usize> = (0..tabs.len()).filter(|i| !out.contains(i)).collect();
        out.extend(rest);
        out
    }
}
