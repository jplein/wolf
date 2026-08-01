//! Per-pane file navigation history (back / forward), the single source of truth
//! for Wolf's History menu and its two toolbar buttons.
//!
//! Pure, GUI-free and IO-free — the label pipeline is path MATH only and never
//! touches the filesystem — so this module sits beside [`crate::buffer_picker`]
//! and [`crate::pane_status`] rather than in the redraw fold. The fold in
//! [`crate::redraw`] derives a [`NavTarget`] per WINDOW after every `wolf_buffers`
//! / `wolf_preview` emit and hands it to [`NavHistory::observe`]; the six
//! `WolfCore::history_*` methods read and move it.
//!
//! ## What is tracked
//!
//! History is keyed by the nvim **window handle** (`i64`), not by grid: a pane's
//! stack follows the window across repaints. It is **not persisted** across
//! restarts, and the front-ends hold **no copy** of it.
//!
//! A push is triggered by ANY change to a pane's current file, including
//! Neovim-internal navigation (`:e`, `gd`, the buffer picker). The stack is a
//! literal VISIT LOG — `A -> B -> A -> B` is FOUR entries; de-duplication happens
//! only when rendering [`MenuRow`]s. New navigation from a mid-stack cursor
//! TRUNCATES everything ahead of it (browser semantics), and the stack is capped
//! at [`MAX_ENTRIES`] (oldest dropped, cursor shifted).
//!
//! The terminal DRAWER is EXCLUDED (it produces no entry at all); a `:terminal` in
//! a NORMAL split IS tracked as [`NavTarget::Terminal`]. Unnamed buffers are
//! tracked by bufnr as [`NavTarget::Untitled`]. [`NavTarget`] is the dedup key, so
//! `Edit(B)` and `Preview(B)` are DISTINCT entries.
//!
//! ## The "in NEITHER map" rule
//!
//! [`derive_target`] returns `None` when a window is in neither the buffer map nor
//! the preview map, and the fold iterates only the UNION of the two maps' keys —
//! so such a window yields **no entry, no push, and no prune**. This is what makes
//! the arrival ORDER of the two notifications irrelevant during a preview
//! transition (there is a transient frame in which the window is in neither map).
//!
//! ## Feedback-loop suppression
//!
//! [`PaneHistory::back`] / [`PaneHistory::forward`] / [`PaneHistory::jump`] arm a
//! per-pane `expected` target as part of the same mutation that moves the cursor —
//! BEFORE the caller issues the nvim RPC, so a fold racing the RPC's autocmds
//! already sees it. The very next [`PaneHistory::observe`] for that pane clears it
//! unconditionally, on every branch: a MATCHING change is absorbed (it is our own
//! move landing, echoed back through `wolf_buffers`), a NON-matching one is a
//! genuine navigation and PUSHES.
//!
//! **`expected` is never a gate on pushing.** Branch 4 of the observe rule fires
//! regardless of it, so a stale `expected` can never wedge history: if a Back fails
//! silently and the pane stays put, the next genuine navigation truncates ahead of
//! the moved cursor, appends, and clears `expected`. History **self-heals in one
//! step** — no timer, no epoch, no reaper.
//!
//! Failed moves are ALSO actively reverted: `back`/`forward`/`jump` return a
//! [`NavMove`], and when the nvim-side helper reports failure the caller calls
//! [`NavHistory::revert`], which restores the cursor **only if it is still where
//! the move left it** (a concurrent `observe` that already reconciled the pane
//! wins).
//!
//! ## Known limits (documented, deliberately NOT fixed)
//!
//! * Two Back presses issued faster than the first `wolf_buffers` emit is folded
//!   leave that first emit non-matching (`expected` has already been overwritten),
//!   producing one spurious push. This is exactly what the locked rule specifies.
//!   The window is sub-millisecond because `WolfCore::history_back` blocks on the
//!   RPC before returning to the main thread; the worst outcome is slightly odd
//!   history, never a wedge.
//! * A `restart_nvim` clones the SAME redraw handler, so the history cell survives
//!   a respawn by construction. That is acceptable: the new nvim issues fresh
//!   window handles, so stale panes are never queried.
//! * `win_hide` (fired for every window of a tabpage being left) removes the
//!   `Snapshot::windows` entry, so a later `win_close` for that grid cannot resolve
//!   its window handle and that pane's history is orphaned. nvim never reuses
//!   window handles, so an orphan is only memory, bounded at [`MAX_ENTRIES`] per
//!   orphaned pane per nvim-process lifetime.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crate::render::WinBuffer;

/// The maximum number of entries kept in ONE pane's stack. Pushing past it drops
/// the OLDEST entries and shifts the cursor by the drained count.
pub const MAX_ENTRIES: usize = 100;

/// The maximum number of DISTINCT rows the History menu renders.
pub const MAX_MENU_ROWS: usize = 10;

/// The display label used for a pathless terminal row (numbered `Terminal`,
/// `Terminal 2`, ... by first appearance in the newest-first row order).
const TERMINAL_LABEL: &str = "Terminal";

/// The display label used for a pathless unnamed-buffer row (numbered `Untitled`,
/// `Untitled 2`, ... by first appearance in the newest-first row order).
const UNTITLED_LABEL: &str = "Untitled";

/// The prefix a preview row's label carries, ahead of its source file's label.
const PREVIEW_PREFIX: &str = "Preview: ";

/// One thing a pane can be showing, and the dedup key of the whole module.
///
/// `Edit(X)` and `Preview(X)` are DELIBERATELY distinct: previewing the file you
/// are editing is a real, separate history entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NavTarget {
    /// A file-backed buffer shown in the pane, by absolute canonical path.
    Edit {
        /// The buffer's absolute, canonicalized path.
        path: PathBuf,
    },
    /// A Markdown preview shown in the pane, by its SOURCE file's absolute path.
    Preview {
        /// The previewed source file's absolute path.
        source: PathBuf,
    },
    /// A `:terminal` buffer in a NORMAL split (never the terminal drawer), by
    /// buffer number.
    Terminal {
        /// The terminal buffer's number.
        bufnr: i64,
    },
    /// A genuinely UNNAMED buffer, by buffer number.
    Untitled {
        /// The unnamed buffer's number.
        bufnr: i64,
    },
}

impl NavTarget {
    /// The file path this target is about, or `None` for a pathless target
    /// ([`Terminal`](NavTarget::Terminal) / [`Untitled`](NavTarget::Untitled)).
    ///
    /// Pathless targets are EXCLUDED from the label-collision groups and numbered
    /// instead (see [`PaneHistory::menu_rows`]).
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Edit { path } => Some(path.as_path()),
            Self::Preview { source } => Some(source.as_path()),
            Self::Terminal { .. } | Self::Untitled { .. } => None,
        }
    }
}

/// A cursor move produced by [`PaneHistory::back`] / [`PaneHistory::forward`] /
/// [`PaneHistory::jump`].
///
/// The caller issues the nvim RPC for `target` and, when the helper reports
/// failure, reverts with `revert(from: mv.to, to: mv.from)`.
pub struct NavMove {
    /// The cursor index BEFORE the move.
    pub from: usize,
    /// The cursor index AFTER the move (where the cursor now sits).
    pub to: usize,
    /// The entry navigated TO (`entries[to]`).
    pub target: NavTarget,
}

/// One rendered History-menu row.
pub struct MenuRow {
    /// The finished, disambiguated display string (already home-abbreviated).
    pub display: String,
    /// The STACK index to hand back to [`PaneHistory::jump`]. For the checkmarked
    /// row this is the CURSOR's own index, so clicking it is a true no-op.
    pub index: usize,
    /// Whether this row draws the checkmark (at most one row per menu).
    pub is_current: bool,
}

/// One pane's navigation stack: the visit log, the cursor into it, and the
/// feedback-loop suppression cell.
#[derive(Clone, Debug, Default)]
pub struct PaneHistory {
    /// The visit log, oldest first. Capped at [`MAX_ENTRIES`].
    entries: Vec<NavTarget>,
    /// Index into [`entries`](Self::entries) of the pane's CURRENT target. `0`
    /// (and meaningless) while `entries` is empty.
    cursor: usize,
    /// The target of the most recent self-issued move, consulted AT MOST ONCE and
    /// never a gate on pushing. See the module docs.
    expected: Option<NavTarget>,
}

impl PaneHistory {
    /// Appends `target` as a genuine navigation: truncates everything ahead of the
    /// cursor (browser semantics), pushes, parks the cursor on the new entry,
    /// enforces the [`MAX_ENTRIES`] cap (dropping the oldest and shifting the
    /// cursor by exactly the drained count), and clears `expected`.
    pub fn push(&mut self, target: NavTarget) {
        if !self.entries.is_empty() {
            self.entries.truncate(self.cursor + 1);
        }
        self.entries.push(target);
        self.cursor = self.entries.len() - 1;

        let overflow = self.entries.len().saturating_sub(MAX_ENTRIES);
        if overflow > 0 {
            self.entries.drain(..overflow);
            // A push from a mid-stack cursor truncates FIRST, freeing room, so it
            // can never drain; the cursor is therefore always >= overflow here.
            self.cursor = self.cursor.saturating_sub(overflow);
        }
        self.expected = None;
    }

    /// Folds one derived target for this pane. The four-branch rule:
    ///
    /// 1. empty stack → SEED (`entries = [target]`, cursor `0`, `expected` clear);
    /// 2. `current() == Some(target)` → absorb (idempotent re-emit, or our own
    ///    move landing) and clear `expected`;
    /// 3. `expected == Some(target)` → absorb (the locked feedback-loop rule) and
    ///    clear `expected`;
    /// 4. otherwise → [`push`](Self::push) (a genuine navigation).
    ///
    /// Branch 3 is unreachable single-threaded through the public API (branch 2
    /// catches it first, because a move has ALREADY placed the cursor on the
    /// target); it is the cross-thread safety net for a fold racing the RPC.
    pub fn observe(&mut self, target: NavTarget) {
        if self.entries.is_empty() {
            self.entries.push(target);
            self.cursor = 0;
            self.expected = None;
            return;
        }
        if self.current() == Some(&target) {
            self.expected = None;
            return;
        }
        if self.expected.as_ref() == Some(&target) {
            self.expected = None;
            return;
        }
        self.push(target);
    }

    /// Whether there is an older entry to go Back to.
    #[must_use]
    pub fn can_back(&self) -> bool {
        self.cursor > 0
    }

    /// Whether there is a newer entry to go Forward to.
    #[must_use]
    pub fn can_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    /// Moves the cursor one entry OLDER, arms `expected` with the entry navigated
    /// to, and reports the move. `None` (nothing changed) at the bottom.
    pub fn back(&mut self) -> Option<NavMove> {
        if !self.can_back() {
            return None;
        }
        self.move_cursor_to(self.cursor - 1)
    }

    /// Moves the cursor one entry NEWER, arms `expected` with the entry navigated
    /// to, and reports the move. `None` (nothing changed) at the top.
    pub fn forward(&mut self) -> Option<NavMove> {
        if !self.can_forward() {
            return None;
        }
        self.move_cursor_to(self.cursor + 1)
    }

    /// Moves the cursor to the STACK index `index`, arms `expected`, and reports
    /// the move.
    ///
    /// Returns `None` — a true no-op, leaving the pane untouched — for an
    /// out-of-range index AND for the cursor's OWN index (so clicking the
    /// checkmarked menu row does nothing, per the locked checkmark rule).
    pub fn jump(&mut self, index: usize) -> Option<NavMove> {
        if index >= self.entries.len() || index == self.cursor {
            return None;
        }
        self.move_cursor_to(index)
    }

    /// Undoes a FAILED navigation: restores the cursor to `to` and clears
    /// `expected`, but ONLY when the cursor is still at `from` (where the move
    /// left it). A racing [`observe`](Self::observe) that already reconciled the
    /// pane WINS, and this is a no-op.
    ///
    /// The caller passes the [`NavMove`]'s fields SWAPPED:
    /// `revert(from: mv.to, to: mv.from)`.
    ///
    /// `to` is ALSO bounds-checked against the current entries. No reachable
    /// out-of-range case exists today (every `to` is a live `NavMove.from`, and
    /// the only thing that shrinks `entries` — [`push`](Self::push) — clears
    /// `expected` and moves the cursor, so the `cursor == from` guard already
    /// fails), but the invariant is made explicit rather than implicit.
    pub fn revert(&mut self, from: usize, to: usize) {
        if self.cursor != from || to >= self.entries.len() {
            return;
        }
        self.cursor = to;
        self.expected = None;
    }

    /// The visit log, oldest first.
    #[must_use]
    pub fn entries(&self) -> &[NavTarget] {
        &self.entries
    }

    /// The cursor's STACK index (`0` and meaningless while the stack is empty).
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The entry the cursor sits on, or `None` for an empty stack.
    #[must_use]
    pub fn current(&self) -> Option<&NavTarget> {
        self.entries.get(self.cursor)
    }

    /// The armed feedback-loop suppression target, if any. See the module docs.
    #[must_use]
    pub fn expected(&self) -> Option<&NavTarget> {
        self.expected.as_ref()
    }

    /// Renders this pane's History-menu rows, NEWEST-FIRST, capped at `limit`
    /// DISTINCT targets, with disambiguated PREFIX-form labels.
    ///
    /// The pipeline, in this exact order:
    ///
    /// 1. **Row selection.** Walk `entries` newest-first, keeping the FIRST
    ///    occurrence of each distinct [`NavTarget`] and the stack index it was
    ///    drawn from, stopping at `limit` distinct rows. Then, if a kept row's
    ///    target equals `entries[cursor]`, its `index` is OVERRIDDEN to `cursor`
    ///    and it is marked `is_current` — so clicking the checkmark is a true
    ///    no-op rather than a jump forward to a newer instance. When the cursor's
    ///    target did NOT survive the cap, NO row is checkmarked.
    /// 2. **Distinct paths.** `Preview(X)` and `Edit(X)` contribute the SAME path
    ///    ONCE.
    /// 3. **Minimal-growth prefix labels** — see [`disambiguate`].
    /// 4. **Decorate.** `Edit` renders its label; `Preview` prefixes
    ///    `"Preview: "`. Pathless rows are EXCLUDED from steps 2-3 and numbered by
    ///    first appearance in the newest-first ROW order (`Terminal`,
    ///    `Terminal 2`, ...; `Untitled`, `Untitled 2`, ...).
    #[must_use]
    pub fn menu_rows(&self, limit: usize, home: Option<&Path>) -> Vec<MenuRow> {
        // --- 1. Row selection (newest-first, first occurrence wins) -----------
        let mut kept: Vec<(usize, &NavTarget)> = Vec::new();
        let mut seen: HashSet<&NavTarget> = HashSet::new();
        for index in (0..self.entries.len()).rev() {
            if kept.len() >= limit {
                break;
            }
            let target = &self.entries[index];
            if seen.insert(target) {
                kept.push((index, target));
            }
        }

        // --- 2. Distinct paths over the kept rows -----------------------------
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut path_seen: HashSet<&Path> = HashSet::new();
        for (_, target) in &kept {
            if let Some(p) = target.path() {
                if path_seen.insert(p) {
                    paths.push(p.to_path_buf());
                }
            }
        }

        // --- 3. Minimal-growth prefix labels ----------------------------------
        let labels = disambiguate(&paths, home);

        // --- 4. Decorate ------------------------------------------------------
        let current = self.current();
        let mut terminals = 0_usize;
        let mut untitleds = 0_usize;
        kept.into_iter()
            .map(|(index, target)| {
                let display = match target {
                    NavTarget::Edit { path } => label_for(&labels, path),
                    NavTarget::Preview { source } => {
                        format!("{PREVIEW_PREFIX}{}", label_for(&labels, source))
                    }
                    NavTarget::Terminal { .. } => {
                        terminals += 1;
                        numbered(TERMINAL_LABEL, terminals)
                    }
                    NavTarget::Untitled { .. } => {
                        untitleds += 1;
                        numbered(UNTITLED_LABEL, untitleds)
                    }
                };
                let is_current = current == Some(target);
                MenuRow {
                    display,
                    // The checkmarked row addresses the CURSOR, not the newer
                    // instance it happened to be drawn from.
                    index: if is_current { self.cursor } else { index },
                    is_current,
                }
            })
            .collect()
    }

    /// Moves the cursor to `index` (assumed in range and != cursor), arms
    /// `expected` with the entry navigated to, and reports the move.
    fn move_cursor_to(&mut self, index: usize) -> Option<NavMove> {
        let target = self.entries.get(index)?.clone();
        let from = self.cursor;
        self.cursor = index;
        self.expected = Some(target.clone());
        Some(NavMove {
            from,
            to: index,
            target,
        })
    }
}

/// Every pane's navigation history, keyed by nvim WINDOW handle.
///
/// Lives behind the `NavHistoryCell` mutex (see [`crate::redraw`]), written by the
/// redraw fold and read/moved by the six `WolfCore::history_*` methods.
#[derive(Clone, Debug, Default)]
pub struct NavHistory {
    /// Per-window stacks. A window with no entry has never been observed (or was
    /// pruned when its window closed).
    panes: HashMap<i64, PaneHistory>,
    /// The last target actually FOLDED for each window — the emit-level idempotence
    /// filter. See [`NavHistory::observe`].
    last: HashMap<i64, NavTarget>,
}

impl NavHistory {
    /// Folds one derived target for window `win`, creating its stack on first
    /// sight — but ONLY when it DIFFERS from the last target folded for that
    /// window. See [`PaneHistory::observe`] for the four-branch rule applied once
    /// the change is real.
    ///
    /// ## Why the change filter lives here
    ///
    /// `wolf_buffers` / `wolf_preview` are WHOLESALE re-emits of every window's
    /// current state, and Neovim fires the driving autocmds several times for one
    /// user action (`BufEnter`, `BufWinEnter`, `BufFilePost`, `WinEnter`, ... — and
    /// across several event-loop ticks, so the Lua side's per-tick coalescing cannot
    /// merge them all). A pane's state therefore arrives 2-4 times per navigation.
    ///
    /// Those repeats are harmless while the cursor is parked on the newest entry
    /// ([`PaneHistory::observe`] branch 2 absorbs them), but they are NOT harmless
    /// straight after a `back`/`forward`/`jump`: the cursor has already moved off
    /// the OLD target and `expected` names the NEW one, so a late repeat of the OLD
    /// state hits branch 4 and is pushed as if it were a genuine navigation —
    /// truncating the entry we were moving to. A DISTINCT `expected` cannot cover
    /// this, because the locked rule deliberately requires a genuine (non-matching)
    /// change to push, which is exactly what the self-heal in
    /// [`PaneHistory::observe`] relies on.
    ///
    /// Filtering on "did the pane's derived target actually CHANGE since the last
    /// fold" separates the two cases exactly: a repeat of an already-folded state is
    /// not a navigation at all and is dropped BEFORE it reaches the stack, while a
    /// real change — including our own move landing, and including a failed move's
    /// stale state re-appearing after some OTHER change — still reaches
    /// [`PaneHistory::observe`] and its unmodified four-branch rule.
    pub fn observe(&mut self, win: i64, target: NavTarget) {
        if self.last.get(&win) == Some(&target) {
            return;
        }
        self.last.insert(win, target.clone());
        self.panes.entry(win).or_default().observe(target);
    }

    /// Drops window `win`'s history entirely (its window closed). An unknown
    /// handle is a silent no-op.
    pub fn prune(&mut self, win: i64) {
        self.panes.remove(&win);
        self.last.remove(&win);
    }

    /// Drops every listed window's history. Unknown handles are silent no-ops.
    pub fn prune_many(&mut self, wins: &[i64]) {
        for win in wins {
            self.panes.remove(win);
            self.last.remove(win);
        }
    }

    /// Whether window `win` can go Back. `false` for an unknown handle.
    #[must_use]
    pub fn can_back(&self, win: i64) -> bool {
        self.panes.get(&win).is_some_and(PaneHistory::can_back)
    }

    /// Whether window `win` can go Forward. `false` for an unknown handle.
    #[must_use]
    pub fn can_forward(&self, win: i64) -> bool {
        self.panes.get(&win).is_some_and(PaneHistory::can_forward)
    }

    /// Moves window `win` one entry Back. `None` for an unknown handle or at the
    /// bottom of the stack.
    pub fn back(&mut self, win: i64) -> Option<NavMove> {
        self.panes.get_mut(&win).and_then(PaneHistory::back)
    }

    /// Moves window `win` one entry Forward. `None` for an unknown handle or at
    /// the top of the stack.
    pub fn forward(&mut self, win: i64) -> Option<NavMove> {
        self.panes.get_mut(&win).and_then(PaneHistory::forward)
    }

    /// Moves window `win`'s cursor to STACK index `index`. `None` for an unknown
    /// handle, an out-of-range index, or the cursor's own index.
    pub fn jump(&mut self, win: i64, index: usize) -> Option<NavMove> {
        self.panes.get_mut(&win).and_then(|p| p.jump(index))
    }

    /// Undoes a failed navigation on window `win`. See [`PaneHistory::revert`].
    pub fn revert(&mut self, win: i64, from: usize, to: usize) {
        if let Some(pane) = self.panes.get_mut(&win) {
            pane.revert(from, to);
        }
    }

    /// Renders window `win`'s History-menu rows. Empty for an unknown handle. See
    /// [`PaneHistory::menu_rows`].
    #[must_use]
    pub fn menu_rows(&self, win: i64, limit: usize, home: Option<&Path>) -> Vec<MenuRow> {
        self.panes
            .get(&win)
            .map(|p| p.menu_rows(limit, home))
            .unwrap_or_default()
    }

    /// Window `win`'s stack, or `None` when it has never been observed.
    #[must_use]
    pub fn pane(&self, win: i64) -> Option<&PaneHistory> {
        self.panes.get(&win)
    }
}

/// Derives the [`NavTarget`] for ONE window from the two out-of-band maps.
///
/// Precedence:
///
/// 1. a preview source is present → [`NavTarget::Preview`] (it wins over the
///    window's buffer; a preview pane is ABSENT from `win_buffers` anyway);
/// 2. no [`WinBuffer`] at all → `None`;
/// 3. the terminal DRAWER (`is_drawer`) → `None` — the drawer is EXCLUDED from
///    history entirely;
/// 4. `is_terminal` → [`NavTarget::Terminal`] (a `:terminal` in a normal split);
/// 5. a named buffer → [`NavTarget::Edit`];
/// 6. otherwise → [`NavTarget::Untitled`], tracked by bufnr.
///
/// A window present in NEITHER map yields `None`: **no entry, no push, no prune**.
/// The fold iterates only the union of the two maps' keys, so such a window is
/// never even observed — which is what makes the arrival order of the two
/// notifications irrelevant during a preview transition.
#[must_use]
pub fn derive_target(buf: Option<&WinBuffer>, preview_source: Option<&Path>) -> Option<NavTarget> {
    if let Some(source) = preview_source {
        return Some(NavTarget::Preview {
            source: source.to_path_buf(),
        });
    }
    let buf = buf?;
    if buf.is_drawer {
        return None;
    }
    if buf.is_terminal {
        return Some(NavTarget::Terminal { bufnr: buf.bufnr });
    }
    match &buf.path {
        Some(path) => Some(NavTarget::Edit { path: path.clone() }),
        None => Some(NavTarget::Untitled { bufnr: buf.bufnr }),
    }
}

/// Computes a PREFIX-form, minimal-growth display label for every path in
/// `paths`.
///
/// Paths are grouped by file name. A SINGLETON group renders BARE (just the file
/// name). For a colliding group, for `k = 2, 3, ...` the candidate is that path's
/// last `k` components joined with `/`; a path with FEWER than `k` NORMAL
/// components (the root `/` is not counted) contributes its FULL ABSOLUTE form
/// instead. The first `k` at which every candidate in the group is distinct wins;
/// the loop terminates at the group's maximum component count (the paths are
/// distinct by construction), falling back to full absolute forms if it somehow
/// does not.
///
/// Finally, any label that ended up a full absolute path has `home` abbreviated to
/// `~`.
///
/// Worked example — `foo/README.md`, `bar/README.md`, `bar/some-other-file.md`
/// render as `bar/README.md`, `foo/README.md`, and a bare `some-other-file.md`.
/// The deeper case `a/b/README.md` vs `c/b/README.md` must grow to TWO parent
/// segments, because one still collides on `b/README.md`.
///
/// IO-FREE: this never touches the filesystem, so labels are produced for paths
/// that do not exist.
#[must_use]
pub fn disambiguate(paths: &[PathBuf], home: Option<&Path>) -> HashMap<PathBuf, String> {
    // Group by file name (a path with no file name groups under its full form).
    let mut groups: HashMap<String, Vec<&PathBuf>> = HashMap::new();
    let mut seen: HashSet<&Path> = HashSet::new();
    for path in paths {
        if !seen.insert(path.as_path()) {
            continue;
        }
        groups.entry(file_name_of(path)).or_default().push(path);
    }

    let mut labels: HashMap<PathBuf, String> = HashMap::new();
    for (name, group) in groups {
        if group.len() == 1 {
            labels.insert(group[0].clone(), name);
            continue;
        }

        let max_k = group
            .iter()
            .map(|p| normal_components(p).len())
            .max()
            .unwrap_or(1);
        let mut resolved = false;
        for k in 2..=max_k.max(2) {
            let candidates: Vec<String> = group.iter().map(|p| candidate_at(p, k)).collect();
            let distinct: HashSet<&String> = candidates.iter().collect();
            if distinct.len() == group.len() {
                for (path, candidate) in group.iter().zip(candidates) {
                    labels.insert((*path).clone(), candidate);
                }
                resolved = true;
                break;
            }
        }
        if !resolved {
            // Unreachable for distinct paths; a defensive, always-unambiguous
            // fallback rather than a panic.
            for path in &group {
                labels.insert((*path).clone(), path.to_string_lossy().into_owned());
            }
        }
    }

    if let Some(home) = home {
        for label in labels.values_mut() {
            if let Some(abbreviated) = abbreviate_home(label, home) {
                *label = abbreviated;
            }
        }
    }
    labels
}

/// The disambiguated label for `path`, falling back to its bare file name when
/// (impossibly) absent from the map.
fn label_for(labels: &HashMap<PathBuf, String>, path: &Path) -> String {
    labels
        .get(path)
        .cloned()
        .unwrap_or_else(|| file_name_of(path))
}

/// `base` for `n == 1`, `"<base> <n>"` otherwise (index 1 is un-numbered, matching
/// the anonymous-buffer naming convention).
fn numbered(base: &str, n: usize) -> String {
    if n <= 1 {
        base.to_string()
    } else {
        format!("{base} {n}")
    }
}

/// `path`'s file name (lossy), falling back to its full lossy form when it has
/// none (e.g. `/`).
fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// `path`'s NORMAL components (the root `/`, `.` and `..` are not counted).
fn normal_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

/// `path`'s last `k` normal components joined with `/`, or its FULL absolute form
/// when it has fewer than `k` of them.
fn candidate_at(path: &Path, k: usize) -> String {
    let comps = normal_components(path);
    if comps.len() < k {
        return path.to_string_lossy().into_owned();
    }
    comps[comps.len() - k..].join("/")
}

/// Abbreviates `home` to `~` in `label` when `label` is a full absolute path under
/// it. `None` when it does not apply.
fn abbreviate_home(label: &str, home: &Path) -> Option<String> {
    let path = Path::new(label);
    if !path.is_absolute() {
        return None;
    }
    if path == home {
        return Some("~".to_string());
    }
    let rest = path.strip_prefix(home).ok()?;
    Some(format!("~/{}", rest.to_string_lossy()))
}
