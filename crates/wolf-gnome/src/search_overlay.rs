//! The GNOME PROJECT-SEARCH overlay for `wolf-gnome`: a ripgrep-backed content
//! search over the captive Neovim's live working directory.
//!
//! Named `search_overlay` rather than `search` so it never reads as
//! [`wolf_core::search`], whose [`SearchSession`] it is a thin front-end for.
//!
//! Trigger: the **`win.search-in-project`** action — bound to **Ctrl+Shift+F** and
//! listed in the primary menu as "Search in project". Like the file picker, the
//! command palette and the buffer picker, the chord is resolved from
//! [`crate::actions::ACTION_TABLE`] by a CAPTURE-phase controller
//! ([`crate::shell`]), so it is guaranteed not to also be forwarded to Neovim (see
//! `crates/wolf-gnome/DECISIONS.md`, "The accelerator namespace").
//!
//! ## A result row is a LOCATION, not a file
//!
//! That is the whole point of this overlay, and it is what makes it different from
//! the other three. Activating a row calls
//! [`WolfCore::open_at`](wolf_core::WolfCore::open_at) with the row's own
//! `(path, line, col)`, which the core's input pump performs as `:edit` followed by
//! `nvim_win_set_cursor` inside ONE pump step. [`SearchMatch::line_number`] is
//! 1-based and [`SearchMatch::column`] is a 0-based BYTE offset — exactly the two
//! conventions `nvim_win_set_cursor` takes — so [`activation_target`] widens the
//! column losslessly and adjusts neither value. (Neovim's own `col('.')` is
//! 1-based, so a readback of it is `column + 1`; that `+ 1` belongs only where a
//! readback is compared, never here.)
//!
//! ## Enter is a TWO-WAY decision, and typing runs nothing
//!
//! There is deliberately no `connect_search_changed`: a search runs only when the
//! user presses Enter, so no keystroke can spawn `rg` and no debounce timer is
//! needed. The macOS front-end takes the same decision and says so in
//! `crates/wolf-macos/Sources/WolfMacOS/SearchView.swift:364-366` — the search
//! field's action fires on Return and `controlTextDidChange` is deliberately not
//! wired.
//!
//! The FIRST Enter (a query the live session was not started with) runs the search;
//! the SECOND (the entry unchanged) activates the selected row. [`enter_action`]
//! owns that rule, and it refuses an empty or whitespace-only query outright:
//! core builds `rg -F -e ''`, which matches every line of every file under the
//! search root.
//!
//! ## Nothing here blocks the GTK main thread
//!
//! [`WolfCore::start_search`](wolf_core::WolfCore::start_search) resolves
//! `WOLF_RG_BIN`, resolves the search root from the tracked-cwd cell and
//! `spawn_blocking`s onto the tokio runtime; `open_at` is fire-and-forget on the
//! ordered input channel; `liveness` is a lock-free atomic read. So this overlay
//! adds NO blocking-RPC path to the crate at all, and
//! `WolfCore::ignorecase()` — which IS a blocking RPC — is deliberately never
//! called: it would buy only the seed for a case-sensitivity toggle this front-end
//! does not ship.
//!
//! ## Streaming, and what a stale `rg` cannot do
//!
//! Results arrive over time, so this module copies [`crate::picker`]'s wake bridge
//! verbatim: one `async_channel::bounded` slot, a [`WakeFn`] that only `try_send`s,
//! and a `glib::spawn_future_local` drain holding a `Weak` so dropping the handle
//! ends the task. The one difference is that the wake is tied to a SESSION that is
//! replaced on each Enter rather than to the overlay's lifetime. That is safe
//! because [`SearchInner::run_search`] takes and drops the old session — the drop is
//! what kills the old `rg` child — before starting the new one, because
//! [`SearchInner::refresh`] always re-reads the CURRENT session, and because it
//! early-returns while closed.
//!
//! ## Opening may be REFUSED, in two ways that are both needed
//!
//! [`open_decision`] is consulted before any widget is touched. A core that is not
//! `Alive` refuses, and — unlike [`crate::buffer_picker::open_decision`] — so does a
//! window with NO project open, because the search root would then fall back to the
//! directory Neovim was started in (the home directory), which is exactly the tree
//! [`crate::workspace::launch_decision`] refuses to hand the file search.
//! `shell::set_empty_state` ALSO disables the `gio::SimpleAction`; the action guard
//! is what the chord and the menu item see, and this seam guard is what the command
//! palette's row sees.
//!
//! Its pure, display-free seams are [`open_decision`], [`search_options_for`],
//! [`result_rows`], [`activation_target`], [`enter_action`] and [`search_status`],
//! unit-tested below. Selection movement and activation reuse [`crate::picker`]'s
//! [`next_selection`], [`restored_index`] and [`activation_index`], which all four
//! overlays share.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::glib;

use wolf_core::{
    context_window, project, Liveness, SearchMatch, SearchOptions, SearchSession, WakeFn, WolfCore,
};

use crate::picker::{activation_index, next_selection, restored_index, FocusTarget};

/// The maximum number of result rows materialized into the `ListBox` on each
/// refresh ([`crate::picker`]'s `PICKER_MAX_ROWS` precedent).
///
/// This bounds the WIDGETS, which is what the GTK main thread pays for. It does
/// NOT bound the work behind them: [`result_rows`] hands the WHOLE match slice to
/// [`wolf_core::project`], which builds a group per file over every match, sorts
/// each group's matches and then sorts the groups — the `take(max)` applies only to
/// the row iterator downstream of that. Every refresh therefore walks and sorts all
/// N matches, and only the last step is capped.
///
/// `SearchState::matches` itself is unbounded while `rg` runs and is deliberately
/// left that way: the session is transient, and is killed on the next Enter and on
/// close.
const SEARCH_MAX_ROWS: usize = 200;

/// The CHARACTER budget handed to [`context_window`] when trimming a matched line
/// down to a row label.
///
/// Fixed, unlike macOS's `SearchBudgetSeam.charBudget`, which derives its budget
/// from the measured advance of a resizable sidebar: this overlay's box is a fixed
/// [`SEARCH_BOX_WIDTH`] wide, so a measurement would return the same number every
/// time.
const SEARCH_CHAR_BUDGET: usize = 90;

/// Capacity of the search's wake channel. One slot is enough: it coalesces a burst
/// of streaming-batch wakes into a single queued refresh (a dropped "full" send
/// just means a refresh is already pending). Mirrors `PICKER_WAKE_CHANNEL_CAP`.
const SEARCH_WAKE_CHANNEL_CAP: usize = 1;

/// Top margin (px) of the floating search box within the overlay.
const SEARCH_BOX_MARGIN: i32 = 24;

/// Width (px) of the floating search box.
const SEARCH_BOX_WIDTH: i32 = 600;

/// Maximum height (px) of the scrolled result list. The list propagates its natural
/// height below this, so a two-row result is not drawn as a tall empty panel.
const SEARCH_LIST_MAX_HEIGHT: i32 = 400;

/// The glyph a row label carries on each side [`context_window`] reported it had
/// trimmed. Adding it is the front-end's job; see [`wolf_core::ContextWindow`] for
/// why. The same glyph as `SearchView.swift`'s `ellipsis` (`U+2026`).
const ELLIPSIS: &str = "…";

/// Separates a row label's `"<file>:<line>"` prefix from its windowed line text.
const LABEL_SEPARATOR: &str = ": ";

/// The status line before anything has been run — the state the overlay opens in.
const SEARCH_IDLE_STATUS: &str = "Type a query and press Enter to search";

/// The status line for a finished search that matched nothing.
const SEARCH_NO_MATCHES_STATUS: &str = "No matches";

/// Prefix of the status line for a session carrying an error. This label is the
/// ONLY surface core's message (e.g. `"WOLF_RG_BIN unset"`) ever reaches.
const SEARCH_ERROR_PREFIX: &str = "Search failed: ";

/// Prefix of the status line while `rg` is still running.
const SEARCH_RUNNING_PREFIX: &str = "Searching… ";

/// What pressing Ctrl+Shift+F does, given the captive Neovim's health and whether a
/// project is open at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOpenDecision {
    /// Show the overlay and focus its entry. It starts NO search.
    Open,
    /// Do not open, and touch no widget; the string names why, as a mid-sentence
    /// clause (the [`crate::command_action`] / [`crate::palette`] convention).
    Refuse(&'static str),
}

/// Whether the project-search overlay may open, given the core's liveness reading
/// (or `None` when there is no core at all) and whether a project is open.
///
/// TOTAL by construction: the implementation is a `match` over `Option<Liveness>`
/// with **no `_` arm**, so a newly appended [`Liveness`] variant fails to compile
/// here rather than silently inheriting whichever behaviour a catch-all had.
///
/// `Option<Liveness>` — not `Liveness` — is the input type precisely so the absent
/// core is a CLASSIFIED arm rather than an early return at the call site, which no
/// headless test could see.
///
/// **Precedence is liveness FIRST, then project**, so a user whose Neovim has
/// crashed is told their Neovim has crashed rather than that they have no folder
/// open.
///
/// The reason the two overlay-covered arms refuse is NOT the one
/// [`crate::buffer_picker::open_decision`] gives, and copying that reasoning here
/// would be wrong: nothing on this path blocks. They refuse because the stall and
/// crash overlays are OPAQUE and cover the editor, so the search box would open
/// invisibly beneath one of them while its open flag kept `window::wire_keys`
/// suppressing every key to Neovim — and an `OpenAt` into a dead Neovim performs
/// nothing anyway.
#[must_use]
pub fn open_decision(liveness: Option<Liveness>, project_open: bool) -> SearchOpenDecision {
    match liveness {
        Some(Liveness::Alive) => {
            if project_open {
                SearchOpenDecision::Open
            } else {
                SearchOpenDecision::Refuse(
                    "no project is open, so the search would be rooted at the directory Neovim was \
                     started in — the home directory — which is the whole tree \
                     `workspace::launch_decision` refuses to hand the file search",
                )
            }
        }
        Some(Liveness::NotResponding) => SearchOpenDecision::Refuse(
            "the captive Neovim is not responding, so the opaque stall overlay already covers the \
             editor and the search box would open invisibly beneath it while its open flag kept \
             every key from reaching Neovim",
        ),
        Some(Liveness::Dead) => SearchOpenDecision::Refuse(
            "the captive Neovim has died, so the opaque crash overlay already covers the editor \
             and an OpenAt into a dead Neovim performs nothing",
        ),
        None => SearchOpenDecision::Refuse(
            "the core has already been taken out by the window's close handler, so there is \
             nothing left to search",
        ),
    }
}

/// The [`SearchOptions`] this front-end ships for `query`: a LITERAL,
/// CASE-INSENSITIVE search over EVERY file.
///
/// All three are decisions, and none of them is exposed as a toggle (macOS's three
/// toggle buttons have no Linux counterpart, and `SearchToggleSeam`'s seeds are
/// therefore unread here):
///
/// - `use_regex: false` — macOS defaults regex ON *and gives the user a `.*` button
///   to turn it off*. Without the button a regex default is a trap: `foo(bar` gives
///   rg exit code 2, an error status, zero results and no way to recover. A literal
///   search is total over every query string.
/// - `case_sensitive: false` — with no toggle and no `ignorecase()` read (that call
///   is a blocking RPC this crate deliberately does not add), the forgiving default
///   is the only one a user can reach.
/// - `all_files: true` — core's DEFAULT restricts to `--glob '*.md' --glob
///   '*.markdown'`, while this front-end's file picker and tree sidebar list every
///   file in the project. A "Search in project" that silently searched only
///   Markdown, with no toggle to widen it, would be a search that lies about its
///   own name.
///
/// The query is passed through VERBATIM — never trimmed, cased or escaped —
/// because [`enter_action`] compares the RAW entry text against the query the live
/// session was started with.
#[must_use]
pub fn search_options_for(query: &str) -> SearchOptions {
    SearchOptions {
        query: query.to_string(),
        use_regex: false,
        case_sensitive: false,
        all_files: true,
    }
}

/// One row of the flat result list: the LOCATION it opens, plus the text it draws.
///
/// The list is flat — one row per matched line, with no expandable file header —
/// which is what lets [`crate::picker`]'s `next_selection` / `restored_index` /
/// `activation_index` trio drive it unchanged. The ordering and the file labelling
/// still come from core's projection; see [`result_rows`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRow {
    /// Absolute path of the matched file, copied from the [`SearchMatch`] — never
    /// rebuilt from the relative path or from the label.
    pub abs_path: PathBuf,
    /// 1-based line number of the match (rg's `line_number`, and the row
    /// `nvim_win_set_cursor` takes).
    pub line_number: u64,
    /// 0-based BYTE column of the match (rg's first-submatch start, and the column
    /// `nvim_win_set_cursor` takes in the same convention).
    pub column: u32,
    /// What the row draws: `"<file label>:<line>: <windowed line text>"`, with an
    /// [`ELLIPSIS`] on each side core reported it had trimmed.
    pub label: String,
}

/// Assembles the flat row list from the raw matches, in the CORE projection's order
/// and capped at `max`.
///
/// The order is [`wolf_core::project`]'s and not rg's: files sorted
/// case-insensitively by relative path, each file's matches ascending by line
/// number. Core's projection is what imposes a display order — `SearchState::matches`
/// holds rg's emission order — so this function walks the projection's GROUPS rather
/// than the flat match slice, and the row `Enter` activates is the row the projection
/// put under the selection.
///
/// Each label starts with `SearchFileGroup::label` (`"<basename> - <parent>"`),
/// because a flat list has no file header row to carry it, then the line number,
/// then [`context_window`]'s windowed text with an [`ELLIPSIS`] on each truncated
/// side. The `(abs_path, line_number, column)` triple is copied STRAIGHT from the
/// [`SearchMatch`]; nothing about a row's location is derived.
#[must_use]
pub fn result_rows(matches: &[SearchMatch], budget: usize, max: usize) -> Vec<SearchRow> {
    let groups = project(matches);
    groups
        .iter()
        .flat_map(|group| {
            group
                .match_indices
                .iter()
                .filter_map(move |index| matches.get(*index).map(|found| (group, found)))
        })
        .take(max)
        .map(|(group, found)| {
            let window = context_window(&found.line_text, &found.match_ranges, budget);
            let lead = if window.truncated_left { ELLIPSIS } else { "" };
            let trail = if window.truncated_right { ELLIPSIS } else { "" };
            SearchRow {
                abs_path: found.abs_path.clone(),
                line_number: found.line_number,
                column: found.column,
                label: format!(
                    "{}:{}{LABEL_SEPARATOR}{lead}{}{trail}",
                    group.label, found.line_number, window.text
                ),
            }
        })
        .collect()
}

/// What the SECOND `Enter` opens, as one total function: the
/// `(path, line, col)` triple to hand [`WolfCore::open_at`](wolf_core::WolfCore::open_at),
/// or `None` for "do nothing at all".
///
/// The row is resolved through [`activation_index`], so zero rows is a true no-op
/// and an absent / negative / out-of-range selection is the top row — which is what
/// makes "type a query, Enter, Enter" work with no arrow key.
///
/// The column is widened `u32` -> `u64` with [`u64::from`], which is lossless and
/// must not ADJUST the value: rg's column and `nvim_win_set_cursor`'s column are
/// both 0-based byte offsets. A `+ 1` here would land the cursor one byte past every
/// match.
#[must_use]
pub fn activation_target(rows: &[SearchRow], selected: Option<i32>) -> Option<(PathBuf, u64, u64)> {
    let index = activation_index(selected, rows.len())?;
    let row = rows.get(index)?;
    Some((row.abs_path.clone(), row.line_number, u64::from(row.column)))
}

/// What one press of `Enter` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterAction {
    /// Start a fresh [`SearchSession`] for the entry's current text, killing any
    /// session already running.
    RunSearch,
    /// Open the result row at this index.
    Activate(usize),
    /// A true no-op: nothing is started and nothing is opened.
    Nothing,
}

/// The two-way `Enter` decision: run the search the entry describes, or activate
/// the row the search already produced.
///
/// The rules, in order:
///
/// 1. A query whose `trim()` is empty is NEVER run and never activates. This is the
///    real bound on a runaway search — core builds `rg -F -e ''`, which matches
///    every line of every file under the search root — so the guard comes first,
///    ahead of the activation path a blanked entry would otherwise still reach.
/// 2. A query the live session was not started with runs a fresh search. The
///    comparison is RAW, so a trailing space is a different literal search, exactly
///    as it is to `rg -F`.
/// 3. Otherwise the selection resolves through [`activation_index`], which is
///    `None` — and therefore [`EnterAction::Nothing`] — for a search that found
///    nothing.
#[must_use]
pub fn enter_action(
    query: &str,
    ran_query: Option<&str>,
    selected: Option<i32>,
    n_rows: usize,
) -> EnterAction {
    if query.trim().is_empty() {
        return EnterAction::Nothing;
    }
    if ran_query != Some(query) {
        return EnterAction::RunSearch;
    }
    match activation_index(selected, n_rows) {
        Some(index) => EnterAction::Activate(index),
        None => EnterAction::Nothing,
    }
}

/// The one-line status drawn under the search entry.
///
/// Arms in precedence order: nothing run yet; the session carries an error; `rg` is
/// still running; finished with nothing found; finished but capped; finished and
/// complete.
///
/// **The error arm precedes the completeness arms deliberately**: a session that
/// failed before it started is ALSO `complete`, and reporting it as "no matches"
/// would send a user looking for a better query when what they need is a
/// provisioned ripgrep. The `ran` guard comes first for the mirror-image reason:
/// the overlay opens with a stale session's counts and error still readable, and
/// reporting them would tell the user their brand-new empty entry had already
/// failed.
#[must_use]
pub fn search_status(
    ran: bool,
    total_matches: usize,
    shown_rows: usize,
    complete: bool,
    error: Option<&str>,
) -> String {
    if !ran {
        return SEARCH_IDLE_STATUS.to_string();
    }
    if let Some(error) = error {
        return format!("{SEARCH_ERROR_PREFIX}{error}");
    }
    if !complete {
        return format!(
            "{SEARCH_RUNNING_PREFIX}{} so far",
            match_count(total_matches)
        );
    }
    if total_matches == 0 {
        return SEARCH_NO_MATCHES_STATUS.to_string();
    }
    if shown_rows < total_matches {
        return format!(
            "Showing first {shown_rows} of {}",
            match_count(total_matches)
        );
    }
    match_count(total_matches)
}

/// `"1 match"` / `"<n> matches"`, so the three counting arms of [`search_status`]
/// agree on one spelling.
fn match_count(total_matches: usize) -> String {
    if total_matches == 1 {
        "1 match".to_string()
    } else {
        format!("{total_matches} matches")
    }
}

/// The overlay's shared, interior-mutable state. All access happens on the single
/// GTK main thread, so `Cell`/`RefCell` (not a `Mutex`) is sufficient.
struct SearchInner {
    /// Whether the overlay is currently open (its widgets visible and focused).
    open: Cell<bool>,
    /// Whether a project is open, mirrored from `shell::set_empty_state`. Read by
    /// [`open_decision`], which refuses without one.
    project_open: Cell<bool>,
    /// The live content-search session, or `None` when nothing has been run. **The
    /// DROP is the cancellation**: it kills the `rg` child.
    search: RefCell<Option<SearchSession>>,
    /// The query [`SearchInner::search`] was started with, or `None` when nothing
    /// has been run. [`enter_action`] compares the entry's raw text against it.
    ran_query: RefCell<Option<String>>,
    /// The `ran_query` the previous [`SearchInner::refresh`] drew, which decides
    /// whether the next refresh resets the selection to the top (a NEW search) or
    /// preserves the user's index across a streaming rebuild.
    last_rendered_query: RefCell<Option<String>>,
    /// The rows currently materialized into the `ListBox`, parallel to it.
    rows: RefCell<Vec<SearchRow>>,
    /// The overlay child holding the entry, the status line and the result list.
    search_box: gtk::Box,
    /// The search entry. It has NO `connect_search_changed`: typing runs nothing.
    entry: gtk::SearchEntry,
    /// The one-line status under the entry, drawn from [`search_status`].
    status: gtk::Label,
    /// The results list, rebuilt on each refresh.
    list: gtk::ListBox,
    /// Where focus returns when the overlay closes: the SELECTED tab's cmdline
    /// strip, resolved at close time. See [`crate::picker::FocusTarget`].
    area: FocusTarget,
    /// The live core, shared with the rest of the app.
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The wake closure handed to `start_search`; firing it queues a refresh.
    wake: WakeFn,
}

impl SearchInner {
    /// Opens the overlay: decides whether it may open at all, then shows and focuses
    /// the entry over an empty list and an idle status. No-op if already open.
    ///
    /// **It starts NO search.** Nothing runs until the user presses Enter.
    ///
    /// A refusal touches no widget and issues nothing, so a refused chord never
    /// flashes an overlay; the reason is printed rather than swallowed.
    fn open(&self) {
        if self.open.get() {
            return;
        }
        // Short borrow of its own; `WolfCore::liveness` is a lock-free atomic read,
        // never an RPC.
        let liveness = {
            let core_ref = self.core.borrow();
            core_ref.as_ref().map(WolfCore::liveness)
        };
        match open_decision(liveness, self.project_open.get()) {
            SearchOpenDecision::Refuse(reason) => {
                eprintln!("wolf-gnome: project search not opened: {reason}");
                return;
            }
            SearchOpenDecision::Open => {}
        }

        // Any session left over from a previous open is taken in a statement of its
        // own, so no `RefCell` borrow is live when the drop — which kills its rg
        // child — runs.
        let leftover = self.search.borrow_mut().take();
        drop(leftover);

        self.open.set(true);
        self.search_box.set_visible(true);
        self.entry.set_text("");
        self.entry.grab_focus();
        *self.ran_query.borrow_mut() = None;
        *self.last_rendered_query.borrow_mut() = None;
        self.rows.borrow_mut().clear();
        self.clear_list();
        self.status.set_text(SEARCH_IDLE_STATUS);
    }

    /// Closes the overlay: drops the live session (which is what CANCELS the search
    /// and kills the `rg` child), hides the widgets, and returns focus to `area`.
    /// No-op if already closed.
    ///
    /// Unlike the palette's and the buffer picker's, this close is not teardown
    /// symmetry — it is the cancellation.
    fn close(&self) {
        if !self.open.get() {
            return;
        }
        self.open.set(false);
        // Take + drop the session to cancel the search and kill the rg subprocess.
        let _ = self.search.borrow_mut().take();
        self.search_box.set_visible(false);
        if let Some(area) = (self.area)() {
            area.grab_focus();
        }
    }

    /// Removes every row widget currently in the list.
    fn clear_list(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
    }

    /// Rebuilds the result list and the status line from the CURRENT session's
    /// latest snapshot. No-op when closed, which is what makes a wake fired by a
    /// dying session harmless.
    ///
    /// The read guard's scope covers [`result_rows`] and the three scalar reads and
    /// nothing else, so no widget work happens under it ([`crate::picker`]'s
    /// guard-scope discipline).
    ///
    /// Selection handling matches the other three overlays: a NEW search resets to
    /// the top row so the first match is preselected, while a streaming wake for the
    /// same search preserves the user's index, clamped.
    fn refresh(&self) {
        if !self.open.get() {
            return;
        }

        // Bound to locals so no borrow is live across the write below.
        let ran_query = self.ran_query.borrow().clone();
        let ran = ran_query.is_some();
        let reset_to_top = *self.last_rendered_query.borrow() != ran_query;
        *self.last_rendered_query.borrow_mut() = ran_query;

        // Captured BEFORE the rebuild so a preserved selection can be restored.
        let prev_selected = self.list.selected_row().map(|row| row.index());

        let (rows, status) = {
            let search = self.search.borrow();
            match search.as_ref() {
                Some(session) => {
                    let view = session.read();
                    let rows = result_rows(&view.matches, SEARCH_CHAR_BUDGET, SEARCH_MAX_ROWS);
                    let total = view.matches.len();
                    let complete = view.complete;
                    let error = view.error.clone();
                    drop(view);
                    let status = search_status(ran, total, rows.len(), complete, error.as_deref());
                    (rows, status)
                }
                None => (Vec::new(), search_status(false, 0, 0, false, None)),
            }
        };

        self.clear_list();
        for row in &rows {
            let label = gtk::Label::builder()
                .label(&row.label)
                .halign(gtk::Align::Start)
                .xalign(0.0)
                .ellipsize(pango::EllipsizeMode::End)
                .build();
            let list_row = gtk::ListBoxRow::new();
            list_row.set_child(Some(&label));
            self.list.append(&list_row);
        }
        let new_len = rows.len();
        *self.rows.borrow_mut() = rows;
        self.status.set_text(&status);

        let prev_index = prev_selected.and_then(|index| usize::try_from(index).ok());
        if let Some(index) = restored_index(reset_to_top, prev_index, new_len) {
            if let Some(row) = self.list.row_at_index(i32::try_from(index).unwrap_or(0)) {
                self.list.select_row(Some(&row));
            }
        }
    }

    /// Moves the LIST selection by `delta` rows (clamped, no wrap), leaving keyboard
    /// focus on the search entry so typing continues to work. Bound to Up (`-1`) /
    /// Down (`+1`). No-op when there are no rows.
    fn move_selection(&self, delta: i32) {
        let n_rows = i32::try_from(self.rows.borrow().len()).unwrap_or(0);
        let current = self.list.selected_row().map_or(0, |row| row.index());
        if let Some(next) = next_selection(current, delta, n_rows) {
            if let Some(row) = self.list.row_at_index(i32::try_from(next).unwrap_or(0)) {
                self.list.select_row(Some(&row));
            }
        }
    }

    /// Routes one `Enter` through [`enter_action`]. Every input it needs is read
    /// under a short borrow of its own, before anything is decided.
    fn on_enter(&self) {
        let query = self.entry.text().to_string();
        let ran_query = self.ran_query.borrow().clone();
        let selected = self.list.selected_row().map(|row| row.index());
        let n_rows = self.rows.borrow().len();
        match enter_action(&query, ran_query.as_deref(), selected, n_rows) {
            EnterAction::RunSearch => self.run_search(query),
            EnterAction::Activate(_) => self.activate_selection(),
            EnterAction::Nothing => {}
        }
    }

    /// Starts a fresh [`SearchSession`] for `query`, killing the previous one first.
    ///
    /// **The statement order is load-bearing.** The old session is taken in a
    /// statement of its own (so no `RefCell` borrow is live afterwards) and dropped
    /// BEFORE `start_search` is called, because the drop is what kills the old `rg`
    /// child — otherwise two ripgreps would run over the same tree at once.
    fn run_search(&self, query: String) {
        let previous = self.search.borrow_mut().take();
        drop(previous);

        let session = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            core.start_search(search_options_for(&query), self.wake.clone())
        };
        *self.search.borrow_mut() = Some(session);
        *self.ran_query.borrow_mut() = Some(query);
        // The first refresh of an ERRORED run comes from here, not from the wake:
        // `start_search` fires `wake()` before returning on the `WOLF_RG_BIN` unset
        // path, so nothing may assume the wake arrives after the session is stored.
        self.refresh();
    }

    /// Opens the selected row's LOCATION — file, line and column — then closes.
    ///
    /// When [`activation_target`] refuses it is a true no-op: nothing is sent and
    /// the overlay stays open, matching [`crate::picker`]'s zero-row rule.
    ///
    /// SEND then CLOSE, like the file and buffer pickers.
    /// [`WolfCore::open_at`](wolf_core::WolfCore::open_at) is fire-and-forget on the
    /// ordered input channel, and the core's pump performs the `:edit` and the
    /// `nvim_win_set_cursor` back to back inside ONE step, so the two cannot be
    /// interleaved with another event.
    fn activate_selection(&self) {
        let selected = self.list.selected_row().map(|row| row.index());
        let target = {
            let rows = self.rows.borrow();
            activation_target(rows.as_slice(), selected)
        };
        let Some((path, line, col)) = target else {
            return;
        };
        if let Some(core) = self.core.borrow().as_ref() {
            core.open_at(path, line, col);
        }
        self.close();
    }
}

/// A handle to the built project-search overlay, used by the shell to drive it (the
/// `win.search-in-project` action, the one-overlay-at-a-time rule, the empty-state
/// flag, close-on-shutdown) and to query whether it currently owns the keyboard (so
/// key forwarding to the core can be suppressed).
#[derive(Clone)]
pub struct SearchHandle {
    inner: Rc<SearchInner>,
}

impl SearchHandle {
    /// Opens the overlay (idempotent) — unless [`open_decision`] refuses, in which
    /// case it prints the reason and does nothing. It starts no search.
    pub fn open(&self) {
        self.inner.open();
    }

    /// Closes the overlay (idempotent), CANCELLING any in-flight `rg` run.
    pub fn close(&self) {
        self.inner.close();
    }

    /// Whether the overlay is currently open. The app uses this to decide whether to
    /// forward keys to the core (it must not while the overlay has focus).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.inner.open.get()
    }

    /// Mirrors the shell's empty-state flag, which [`open_decision`] reads. Without
    /// a project the search root would fall back to the home directory.
    pub fn set_project_open(&self, project_open: bool) {
        self.inner.project_open.set(project_open);
    }
}

/// Builds the project-search widgets as an overlay child of `overlay`, wires all
/// signals, and returns a [`SearchHandle`]. The overlay starts hidden; call
/// [`SearchHandle::open`] (which the `win.search-in-project` action does) to show
/// it.
///
/// `area` resolves the focus-return widget at CLOSE time — the SELECTED tab's
/// cmdline strip, never a pane drawing area (`DECISIONS.md`, "Window structure").
/// See [`crate::picker::FocusTarget`] for why it is a closure.
#[must_use]
pub fn build(
    overlay: &gtk::Overlay,
    area: FocusTarget,
    core: &Rc<RefCell<Option<WolfCore>>>,
) -> SearchHandle {
    let entry = gtk::SearchEntry::new();
    entry.set_hexpand(true);

    let status = gtk::Label::builder()
        .label(SEARCH_IDLE_STATUS)
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .ellipsize(pango::EllipsizeMode::End)
        .build();
    status.add_css_class("dim-label");

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(SEARCH_LIST_MAX_HEIGHT)
        .child(&list)
        .build();

    let search_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    search_box.add_css_class("background");
    search_box.set_halign(gtk::Align::Center);
    search_box.set_valign(gtk::Align::Start);
    search_box.set_margin_top(SEARCH_BOX_MARGIN);
    search_box.set_width_request(SEARCH_BOX_WIDTH);
    search_box.append(&entry);
    search_box.append(&status);
    search_box.append(&scroller);
    search_box.set_visible(false);

    overlay.add_overlay(&search_box);

    // Wake plumbing: streaming batches fire this from a tokio worker (Send + Sync);
    // it only pushes onto the channel. A dropped "full" send is correct (a refresh
    // is already pending). DROP-SAFE: ignore try_send errors so a late cancel-path
    // wake after the channel closes is harmless.
    let (wake_tx, wake_rx) = async_channel::bounded::<()>(SEARCH_WAKE_CHANNEL_CAP);
    let wake: WakeFn = Arc::new(move || {
        let _ = wake_tx.try_send(());
    });

    let inner = Rc::new(SearchInner {
        open: Cell::new(false),
        // Overwritten by `shell::set_empty_state` before the window is presented.
        project_open: Cell::new(false),
        search: RefCell::new(None),
        ran_query: RefCell::new(None),
        last_rendered_query: RefCell::new(None),
        rows: RefCell::new(Vec::new()),
        search_box,
        entry: entry.clone(),
        status,
        list: list.clone(),
        area,
        core: core.clone(),
        wake,
    });

    // Drain the wake channel on the GTK main thread, refreshing the (open) overlay
    // on each batch signal. A Weak avoids a reference cycle (inner owns the wake's
    // sender; the task holds only a weak ref so dropping the handle ends the task).
    {
        let inner_weak = Rc::downgrade(&inner);
        glib::spawn_future_local(async move {
            while wake_rx.recv().await.is_ok() {
                if let Some(inner) = inner_weak.upgrade() {
                    inner.refresh();
                }
            }
        });
    }

    // Enter runs the search, or activates the selected row. There is deliberately NO
    // `connect_search_changed`: typing must never spawn rg.
    {
        let inner_weak = Rc::downgrade(&inner);
        entry.connect_activate(move |_entry| {
            if let Some(inner) = inner_weak.upgrade() {
                inner.on_enter();
            }
        });
    }

    // Clicking / activating a row opens its location.
    {
        let inner_weak = Rc::downgrade(&inner);
        list.connect_row_activated(move |_list, _row| {
            if let Some(inner) = inner_weak.upgrade() {
                inner.activate_selection();
            }
        });
    }

    // Key handling (Capture phase so the SearchEntry does not swallow these first):
    // Esc closes; Up/Down move the LIST selection (NOT the entry's text cursor) and
    // are consumed so the keypress does not also move the caret; every other key
    // Proceeds to the entry so typing flows normally. Enter deliberately rides
    // `connect_activate` rather than this controller, because that is the path the
    // three existing overlay smokes already exercise.
    {
        let inner_weak = Rc::downgrade(&inner);
        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        key.connect_key_pressed(move |_controller, keyval, _keycode, _state| {
            let Some(inner) = inner_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match keyval {
                gdk::Key::Escape => {
                    inner.close();
                    glib::Propagation::Stop
                }
                gdk::Key::Down => {
                    inner.move_selection(1);
                    glib::Propagation::Stop
                }
                gdk::Key::Up => {
                    inner.move_selection(-1);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        inner.search_box.add_controller(key);
    }

    SearchHandle { inner }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::path::PathBuf;

    use wolf_core::{context_window, project, Liveness, SearchMatch};

    use crate::picker::activation_index;

    // What the seams above must derive for this module to compile. These are not
    // preferences; each one is used by an assertion or a failure message here:
    //
    //   * `SearchOpenDecision` — `Debug` (printed when an arm returns the wrong
    //     class).
    //   * `EnterAction` — `Debug, PartialEq` (the table asserts equality and prints
    //     both sides).
    //
    // `SearchRow` needs nothing: every assertion reads its fields by name, so a
    // failure names the field rather than dumping a struct.

    // =====================================================================
    // Shared fixture vocabulary
    // =====================================================================

    /// Stand-in for the search root every fixture path hangs off, so `abs_path`
    /// values are absolute and mutually distinct. Mirrors
    /// `buffer_picker::tests::SMOKE_CWD`.
    const FIXTURE_ROOT: &str = "/w/project";

    /// The literal every fixture line "matches" on.
    ///
    /// Ten bytes of lowercase ASCII, and deliberately not a word any assertion in
    /// this module looks for on its own: the tests read positions and flags, never
    /// the token, so a label that happened to carry it proves nothing either way.
    const FIXTURE_MATCH_TOKEN: &str = "zebracross";

    /// Builds one fixture [`SearchMatch`] whose `line_text` places
    /// [`FIXTURE_MATCH_TOKEN`] at byte offset `column`, preceded by that many `x`s
    /// and followed by `tail_pad` `y`s.
    ///
    /// Building the line AROUND the column is what keeps the fixture self-consistent:
    /// `column`, `match_ranges` and `line_text` cannot drift apart, so a truncation
    /// assertion is measured against a line the column really points into rather
    /// than against a hand-typed offset that may not.
    fn fixture_match(
        rel_path: &str,
        line_number: u64,
        column: u32,
        tail_pad: usize,
    ) -> SearchMatch {
        let lead = usize::try_from(column).expect("a fixture column fits in usize");
        let token_len = u32::try_from(FIXTURE_MATCH_TOKEN.len())
            .expect("FIXTURE_MATCH_TOKEN is far shorter than u32::MAX");
        SearchMatch {
            abs_path: PathBuf::from(FIXTURE_ROOT).join(rel_path),
            rel_path: rel_path.to_string(),
            line_number,
            column,
            line_text: format!(
                "{}{FIXTURE_MATCH_TOKEN}{}",
                "x".repeat(lead),
                "y".repeat(tail_pad)
            ),
            match_ranges: vec![(column, column + token_len)],
        }
    }

    // =====================================================================
    // The whole input domain of `open_decision`
    // =====================================================================

    /// Every liveness state the core can report. Mirrors
    /// `buffer_picker::tests::ALL_LIVENESS`, which is the other total table over
    /// this enum.
    const ALL_LIVENESS: [Liveness; 3] = [Liveness::Alive, Liveness::NotResponding, Liveness::Dead];

    /// Every input [`open_decision`] is total over: each liveness reading crossed
    /// with each `project_open` value, plus the same two for the ABSENT core
    /// (`None`), which the post-shutdown `Rc<RefCell<Option<WolfCore>>>` yields once
    /// `wire_close` has taken the core out.
    ///
    /// `None` is a CLASSIFIED arm rather than an early return at the call site
    /// precisely so it appears in this table; a `let Some(core) = … else { return }`
    /// would be invisible to every test in this module.
    const ALL_OPEN_INPUTS: [(Option<Liveness>, bool); 8] = [
        (Some(Liveness::Alive), true),
        (Some(Liveness::Alive), false),
        (Some(Liveness::NotResponding), true),
        (Some(Liveness::NotResponding), false),
        (Some(Liveness::Dead), true),
        (Some(Liveness::Dead), false),
        (None, true),
        (None, false),
    ];

    /// What an [`OpenCase`] row expects [`open_decision`] to return.
    ///
    /// The refusal is matched on its CLASS, not on its reason string: the reasons
    /// are documentation, and their real contract (non-empty, distinct,
    /// clause-shaped) is asserted by [`refuse_reasons_are_distinct_and_non_empty`]
    /// — the `buffer_picker::tests::WantOpen` precedent.
    #[derive(Clone, Copy)]
    enum WantOpen {
        /// `SearchOpenDecision::Open` — the overlay is shown.
        Open,
        /// `SearchOpenDecision::Refuse(_)` — no widget is touched.
        Refuse,
    }

    /// One `open_decision` table row.
    struct OpenCase {
        desc: &'static str,
        liveness: Option<Liveness>,
        project_open: bool,
        want: WantOpen,
    }

    /// Pins the open decision for all eight `(Option<Liveness>, bool)` inputs: only
    /// a live core with a project open shows the overlay, and every other input
    /// refuses.
    ///
    /// The row-count assertion is what makes the table non-vacuous: the domain is
    /// the three liveness states plus the absent core, each crossed with
    /// `project_open`, so a row that quietly went missing cannot hide behind the
    /// seven that remain.
    #[test]
    fn open_decision_table() {
        let cases = [
            OpenCase {
                desc: "a live core with a project open: the only arm that shows anything",
                liveness: Some(Liveness::Alive),
                project_open: true,
                want: WantOpen::Open,
            },
            OpenCase {
                desc: "a live core with NO project: the search would be rooted at the home dir",
                liveness: Some(Liveness::Alive),
                project_open: false,
                want: WantOpen::Refuse,
            },
            OpenCase {
                desc: "a wedged core with a project: the stall overlay already covers the editor",
                liveness: Some(Liveness::NotResponding),
                project_open: true,
                want: WantOpen::Refuse,
            },
            OpenCase {
                desc: "a wedged core with no project: refused for the liveness reason first",
                liveness: Some(Liveness::NotResponding),
                project_open: false,
                want: WantOpen::Refuse,
            },
            OpenCase {
                desc: "a dead core with a project: an OpenAt into a dead Neovim performs nothing",
                liveness: Some(Liveness::Dead),
                project_open: true,
                want: WantOpen::Refuse,
            },
            OpenCase {
                desc: "a dead core with no project: refused for the liveness reason first",
                liveness: Some(Liveness::Dead),
                project_open: false,
                want: WantOpen::Refuse,
            },
            OpenCase {
                desc: "no core at all (post-shutdown), project flag still set: nothing to ask",
                liveness: None,
                project_open: true,
                want: WantOpen::Refuse,
            },
            OpenCase {
                desc: "no core at all and no project: still refused",
                liveness: None,
                project_open: false,
                want: WantOpen::Refuse,
            },
        ];

        assert_eq!(
            ALL_OPEN_INPUTS.len(),
            ALL_LIVENESS.len() * 2 + 2,
            "ALL_OPEN_INPUTS must hold one entry per liveness state per project flag, plus the \
             same two for the absent core ({} * 2 + 2), but holds {}. The two consts drove apart, \
             so the table below is not measured against the real input domain and a whole \
             arm of `open_decision` could go unclassified.",
            ALL_LIVENESS.len(),
            ALL_OPEN_INPUTS.len()
        );
        assert_eq!(
            cases.len(),
            ALL_OPEN_INPUTS.len(),
            "the open_decision table must carry exactly one row per input `open_decision` is \
             total over: {} rows (three liveness readings plus the absent core, each crossed with \
             `project_open`), not {}. A missing row means an arm nothing checks — and the arm most \
             likely to be dropped is the live-core-without-a-project one, whose refusal is the \
             only thing that stops the command palette rooting a `--hidden` ripgrep run at the \
             user's home directory.",
            ALL_OPEN_INPUTS.len(),
            cases.len()
        );
        for input in ALL_OPEN_INPUTS {
            let matching = cases
                .iter()
                .filter(|case| (case.liveness, case.project_open) == input)
                .count();
            assert_eq!(
                matching, 1,
                "the input {input:?} appears {matching} time(s) in the open_decision table, not \
                 exactly once. The counts above only prove the table is the right SIZE; without \
                 this, one duplicated row and one missing row cancel out and an arm goes \
                 unmeasured while every count still adds up."
            );
        }

        for case in &cases {
            let got = open_decision(case.liveness, case.project_open);
            match case.want {
                WantOpen::Open => assert!(
                    matches!(got, SearchOpenDecision::Open),
                    "case: {} — open_decision({:?}, {}) returned {got:?}, but this input MUST be \
                     Open. A refusal here makes Ctrl+Shift+F do nothing at all on a perfectly \
                     healthy editor with a project open, and prints a reason for a hazard that is \
                     not present.",
                    case.desc,
                    case.liveness,
                    case.project_open
                ),
                WantOpen::Refuse => assert!(
                    matches!(got, SearchOpenDecision::Refuse(_)),
                    "case: {} — open_decision({:?}, {}) returned {got:?}, but this input MUST \
                     refuse. With no project open the search root falls back to the directory \
                     Neovim was started in — the home directory — so an Open here points a \
                     `--hidden` ripgrep run at the whole of `$HOME`, which is exactly the tree \
                     `workspace::launch_decision` refuses to hand the FILE search. With a wedged \
                     or dead core the opaque stall / crash overlay already covers the editor, so \
                     the box would open invisibly beneath it while its open flag kept \
                     `window::wire_keys` suppressing every key to Neovim.",
                    case.desc,
                    case.liveness,
                    case.project_open
                ),
            }
        }
    }

    /// The rule stated over the WHOLE domain rather than over one table row, so a
    /// failure names the rule instead of quoting an index. Mirrors
    /// `buffer_picker::tests::only_a_live_core_is_snapshotted`.
    #[test]
    fn only_a_live_project_opens() {
        let mut opening: Vec<(Option<Liveness>, bool)> = Vec::new();
        for input in ALL_OPEN_INPUTS {
            if matches!(open_decision(input.0, input.1), SearchOpenDecision::Open) {
                opening.push(input);
            }
        }

        let expected = [(Some(Liveness::Alive), true)];
        assert_eq!(
            opening.as_slice(),
            expected.as_slice(),
            "exactly ONE of the eight inputs may open — a core reading Alive with a project open. \
             An EXTRA entry means the overlay can be reached in a state where it does harm: \
             without a project the search roots at `$HOME`, and against a wedged or dead core the \
             box opens under an opaque overlay while suppressing every key to Neovim. An entry \
             MISSING means Ctrl+Shift+F can never open the overlay at all, since Alive with a \
             project is the only state a healthy window with a folder open ever reports."
        );
    }

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a mid-sentence
    /// clause. Copied from `buffer_picker.rs`'s helper of the same name, including
    /// its deliberate allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    /// The refusal reason `open_decision` gives for one input, or a panic naming the
    /// input when that input does not refuse at all.
    ///
    /// A non-refusing input here is `open_decision_table`'s finding reaching a second
    /// test, so the message says so rather than reporting a missing string.
    fn refusal_reason(liveness: Option<Liveness>, project_open: bool) -> &'static str {
        match open_decision(liveness, project_open) {
            SearchOpenDecision::Refuse(reason) => reason,
            other => panic!(
                "open_decision({liveness:?}, {project_open}) produced {other:?} rather than a \
                 refusal, so this test cannot check the reason it exists to check. Either that \
                 arm opens — in which case `open_decision_table` and \
                 `only_a_live_project_opens` are also red, and one of them is the finding — or \
                 the probe does not select the arm it names."
            ),
        }
    }

    /// The four refusal reasons are non-empty, mutually DISTINCT, read as a
    /// mid-sentence clause, and — for the three liveness refusals — do not depend on
    /// `project_open`.
    ///
    /// Distinctness is the load-bearing half, for the same reason it is in
    /// `buffer_picker::refuse_reasons_are_distinct_and_non_empty`: the reason is the
    /// whole of what a reader sees when the chord does nothing, and two arms sharing
    /// a string make "the editor is wedged" indistinguishable from "there is no
    /// project open" — two states with completely different routes back.
    ///
    /// The project-independence half is what pins the documented PRECEDENCE
    /// (liveness first, then project). Without it, an implementation that folded the
    /// two questions into one combined reason per pair would still satisfy the four
    /// probes below while telling a user with a wedged core that their problem is a
    /// missing folder.
    #[test]
    fn refuse_reasons_are_distinct_and_non_empty() {
        let probes = [
            (
                "a live core with no project open",
                Some(Liveness::Alive),
                false,
            ),
            (
                "a wedged (NotResponding) core",
                Some(Liveness::NotResponding),
                true,
            ),
            ("a dead core", Some(Liveness::Dead), true),
            ("an absent core, after shutdown took it out", None, true),
        ];

        let mut seen: HashSet<&'static str> = HashSet::new();
        for (source, liveness, project_open) in probes {
            let reason = refusal_reason(liveness, project_open);

            assert!(
                !reason.trim().is_empty(),
                "the refusal reason for {source} is empty. It is printed as \
                 \"wolf-gnome: project search not opened: {{reason}}\", so an empty one makes a \
                 deliberate refusal indistinguishable from a chord that silently did nothing."
            );
            assert!(
                !is_sentence_cased(reason),
                "the refusal reason for {source} is sentence-cased ({reason:?}). It is \
                 interpolated MID-SENTENCE into the printed diagnostic, exactly as \
                 `buffer_picker.rs`'s and `palette.rs`'s reasons are, so it must start lowercase \
                 (an ALL-CAPS proper noun such as \"GNOME\" is fine)."
            );
            assert!(
                seen.insert(reason),
                "the refusal reason for {source} ({reason:?}) duplicates another arm's reason. \
                 The reasons must be DISTINCT so the printed line names the real cause: a missing \
                 project is fixed by opening a folder, a wedged Neovim is recoverable, a dead one \
                 is not, and an absent core means the window is already tearing down. One shared \
                 string collapses all four into a message that cannot be acted on."
            );
        }

        // Liveness decides first, so the three non-Alive refusals must read the same
        // whatever the project flag says.
        let non_alive: Vec<Option<Liveness>> = ALL_LIVENESS
            .into_iter()
            .filter(|liveness| *liveness != Liveness::Alive)
            .map(Some)
            .chain(std::iter::once(None))
            .collect();
        assert!(
            non_alive.len() > 1,
            "the precedence check covers {} input(s) — ALL_LIVENESS minus Alive, plus the absent \
             core. With fewer than two there is nothing whose reason could differ from another's, \
             so the loop below runs on a domain too small to say anything.",
            non_alive.len()
        );
        for liveness in non_alive {
            let with_reason = refusal_reason(liveness, true);
            let without_reason = refusal_reason(liveness, false);
            assert_eq!(
                with_reason, without_reason,
                "open_decision({liveness:?}, true) and open_decision({liveness:?}, false) give \
                 DIFFERENT reasons ({with_reason:?} vs {without_reason:?}). The documented \
                 precedence is liveness FIRST, then project: a reader whose Neovim has crashed \
                 must be told their Neovim has crashed, not that they have no folder open, \
                 because the two have completely different fixes and only one of them is even \
                 reachable from the chord."
            );
        }
    }

    // =====================================================================
    // search_options_for
    // =====================================================================

    /// One `search_options_for` table row.
    struct OptionsCase {
        desc: &'static str,
        query: &'static str,
    }

    /// The queries `search_options_for` is measured against.
    ///
    /// The metacharacter and whitespace rows are not padding: they are the two
    /// shapes whose handling the returned booleans decide, so
    /// [`search_options_table`] asserts they are present rather than trusting this
    /// list to keep them.
    const OPTIONS_CASES: &[OptionsCase] = &[
        OptionsCase {
            desc: "an ordinary word",
            query: "zebracross",
        },
        OptionsCase {
            desc: "a query carrying a regex metacharacter, which a regex default would break on",
            query: "foo(bar",
        },
        OptionsCase {
            desc: "leading, inner and trailing whitespace, all passed through untouched",
            query: "  two  words  ",
        },
        OptionsCase {
            desc: "an upper-case query, which the case-insensitive default must not alter",
            query: "ZebraCross",
        },
        OptionsCase {
            desc: "the empty query — total here; it is `enter_action` that refuses to run it",
            query: "",
        },
    ];

    /// The four fields this front-end ships, asserted as LITERALS rather than
    /// against named constants: a test that restated a symbol would pass whatever
    /// the symbol was changed to, which is the whole failure this table exists to
    /// prevent.
    #[test]
    fn search_options_table() {
        assert!(
            OPTIONS_CASES
                .iter()
                .any(|case| case.query.contains('(') || case.query.contains('*')),
            "no OPTIONS_CASES row carries a regex metacharacter, so the `use_regex == false` \
             assertion below is measured only against queries a regex engine would also accept. \
             The row that makes it discriminating is the one a user really types: with \
             `use_regex: true` and no `.*` toggle to turn it off, `foo(bar` gives rg exit code 2, \
             an error status, zero results and no way to recover."
        );
        assert!(
            OPTIONS_CASES.iter().any(|case| case.query.contains(' ')),
            "no OPTIONS_CASES row carries whitespace, so nothing here would notice \
             `search_options_for` trimming the query. It must not: `enter_action` compares the \
             RAW entry text against the query the live session was started with, so a trimmed \
             option would start a search for a string the front-end then believes it has not run."
        );

        for case in OPTIONS_CASES {
            let options = search_options_for(case.query);

            assert_eq!(
                options.query, case.query,
                "case: {} — search_options_for({:?}) put {:?} in `query`. The query is passed \
                 through VERBATIM: rg is handed it as a literal pattern, so any trimming, casing \
                 or escaping here searches for a different string than the one on screen.",
                case.desc, case.query, options.query
            );
            assert!(
                !options.use_regex,
                "case: {} — search_options_for({:?}) set `use_regex`. This front-end ships no \
                 `.*` toggle, so a regex default is a trap rather than a convenience: a user \
                 typing an unbalanced bracket gets rg exit code 2, an error status, zero results \
                 and no control that would let them turn it off. A literal search is total over \
                 every query string.",
                case.desc, case.query
            );
            assert!(
                !options.case_sensitive,
                "case: {} — search_options_for({:?}) set `case_sensitive`. There is no toggle and \
                 no `ignorecase()` read (that call is a blocking RPC this crate deliberately does \
                 not add), so the forgiving default is the only one a user can reach: a lowercase \
                 query must match a capitalised heading.",
                case.desc, case.query
            );
            assert!(
                options.all_files,
                "case: {} — search_options_for({:?}) left `all_files` false. Core's DEFAULT \
                 restricts the search to `--glob '*.md' --glob '*.markdown'`, while this \
                 front-end's file picker and tree sidebar list every file in the project — so a \
                 \"Search in project\" that silently searched only Markdown, with no toggle to \
                 widen it, would be a search that lies about its own name. The cage smoke's \
                 target is deliberately a `.txt` for exactly this reason, so this flag is also \
                 what makes that smoke end-to-end discriminating.",
                case.desc, case.query
            );
        }
    }

    // =====================================================================
    // result_rows: ordering
    // =====================================================================

    /// The character budget the row-assembly tests hand `result_rows`.
    ///
    /// Deliberately far below [`SEARCH_CHAR_BUDGET`] so a truncating fixture stays
    /// short enough to read in a failure message. The budget is a PARAMETER of
    /// `result_rows`, so nothing here depends on the production value.
    const TEST_BUDGET: usize = 20;

    /// The ordering fixture, in rg EMISSION order: two files interleaved, each with
    /// two matched lines, and neither file's lines emitted in ascending order.
    ///
    /// `Beta.md` sorts BEFORE `alpha.md` byte-wise (`B` is 0x42, `a` is 0x61) and
    /// AFTER it case-insensitively, so the fixture discriminates a case-sensitive
    /// file sort as well as a missing sort.
    fn ordering_fixture() -> Vec<SearchMatch> {
        vec![
            fixture_match("Beta.md", 30, 7, 4),
            fixture_match("alpha.md", 12, 5, 4),
            fixture_match("Beta.md", 4, 33, 4),
            fixture_match("alpha.md", 40, 21, 4),
        ]
    }

    /// The order [`ordering_fixture`] must come out in: files sorted
    /// case-insensitively by `rel_path`, lines ascending within a file.
    const ORDERING_EXPECTED: [(&str, u64); 4] = [
        ("alpha.md", 12),
        ("alpha.md", 40),
        ("Beta.md", 4),
        ("Beta.md", 30),
    ];

    /// Rows come out in the CORE projection's order, not in rg emission order.
    ///
    /// `wolf_core::project` owns both halves of that order — the case-insensitive
    /// file sort and the ascending line sort within a file — and `result_rows` walks
    /// its groups rather than the flat slice. The fixture emits in an order that
    /// matches neither, so an implementation that iterated `matches` directly is
    /// red here.
    #[test]
    fn result_rows_orders_by_the_core_projection() {
        let matches = ordering_fixture();

        let emitted: Vec<(&str, u64)> = matches
            .iter()
            .map(|m| (m.rel_path.as_str(), m.line_number))
            .collect();
        assert_ne!(
            emitted.as_slice(),
            ORDERING_EXPECTED.as_slice(),
            "the ordering fixture is already IN projection order as emitted, so this test would \
             pass with the projection walk replaced by a plain iteration over `matches` and it \
             discriminates nothing. Reorder `ordering_fixture` so at least one file's lines, and \
             the two files themselves, arrive out of order."
        );

        let rows = result_rows(&matches, TEST_BUDGET, SEARCH_MAX_ROWS);
        let got: Vec<(PathBuf, u64)> = rows
            .iter()
            .map(|row| (row.abs_path.clone(), row.line_number))
            .collect();
        let want: Vec<(PathBuf, u64)> = ORDERING_EXPECTED
            .iter()
            .map(|(rel, line)| (PathBuf::from(FIXTURE_ROOT).join(rel), *line))
            .collect();

        assert_eq!(
            got, want,
            "result_rows produced the rows in the wrong order. The list the user arrows through \
             must be the core projection's: files sorted case-insensitively by relative path, \
             each file's matches ascending by line number. Emission order was {emitted:?}. A list \
             in that order is a list core never asked for: `wolf_core::project` is what imposes \
             the display order, and `result_rows` must walk ITS groups rather than the flat \
             `matches` slice — check which of the two it iterates. A list sorted \
             case-SENSITIVELY puts `Beta.md` ahead of `alpha.md`, which is the other way this can \
             fail."
        );
    }

    // =====================================================================
    // result_rows: the location each row carries
    // =====================================================================

    /// The anti-swap fixture: every line number, every column and every row index is
    /// distinct from every other value in the table, so no coincidence can satisfy a
    /// swapped assignment.
    ///
    /// Read as `(rel_path, line_number, column)` in the order the rows must come out
    /// (files sorted case-insensitively, lines ascending). Line numbers `{12, 40, 4,
    /// 30}`, columns `{5, 21, 33, 7}` and indices `{0, 1, 2, 3}` are three pairwise
    /// DISJOINT sets, checked below rather than left to the reader — that is what
    /// makes an implementation which swapped `line_number` and `column` red instead
    /// of lucky.
    const LOCATION_EXPECTED: [(&str, u64, u32); 4] = [
        ("alpha.md", 12, 5),
        ("alpha.md", 40, 21),
        ("Beta.md", 4, 33),
        ("Beta.md", 30, 7),
    ];

    /// Every row carries its source match's location UNCHANGED: the file it was
    /// found in, the 1-based line and the 0-based byte column.
    ///
    /// This is the whole premise of the unit — a result row is a LOCATION, not a
    /// file — reduced to three field reads. `open_at` is handed exactly these three
    /// values, so a row whose line or column was derived, re-looked-up or swapped
    /// opens the right file at the wrong place, which is indistinguishable from
    /// "the feature works" in any screenshot that does not read the cursor back.
    #[test]
    fn result_rows_carries_the_exact_location() {
        let lines: HashSet<u64> = LOCATION_EXPECTED.iter().map(|(_, line, _)| *line).collect();
        let columns: HashSet<u64> = LOCATION_EXPECTED
            .iter()
            .map(|(_, _, column)| u64::from(*column))
            .collect();
        let indices: HashSet<u64> = (0..LOCATION_EXPECTED.len())
            .map(|index| u64::try_from(index).expect("a fixture index fits in u64"))
            .collect();

        assert_eq!(
            lines.len(),
            LOCATION_EXPECTED.len(),
            "two LOCATION_EXPECTED rows share a line number ({lines:?}), so a `result_rows` that \
             assigned every row the SAME line could still satisfy part of this table. Give every \
             row a distinct line."
        );
        assert_eq!(
            columns.len(),
            LOCATION_EXPECTED.len(),
            "two LOCATION_EXPECTED rows share a column ({columns:?}), for the same reason as the \
             line numbers above. Give every row a distinct column."
        );
        assert!(
            lines.is_disjoint(&columns)
                && lines.is_disjoint(&indices)
                && columns.is_disjoint(&indices),
            "the LOCATION_EXPECTED line numbers {lines:?}, columns {columns:?} and row indices \
             {indices:?} are not pairwise disjoint. That overlap is exactly what lets a swapped \
             `line_number` / `column` assignment — the single-line revert this test exists to \
             catch — pass anyway on the row where the two values happen to coincide."
        );

        let matches = ordering_fixture();
        let rows = result_rows(&matches, TEST_BUDGET, SEARCH_MAX_ROWS);
        assert_eq!(
            rows.len(),
            LOCATION_EXPECTED.len(),
            "result_rows produced {} row(s) for the {}-match fixture, so the per-row assertions \
             below would run against a different list than the one they describe. \
             `result_rows_orders_by_the_core_projection` and `result_rows_caps_at_max` are the \
             two tests that say why a count can move.",
            rows.len(),
            LOCATION_EXPECTED.len()
        );

        for (index, (rel, line, column)) in LOCATION_EXPECTED.iter().enumerate() {
            let row = &rows[index];
            let want_path = PathBuf::from(FIXTURE_ROOT).join(rel);

            assert_eq!(
                row.abs_path,
                want_path,
                "row {index} carries abs_path {}, expected {}. The path is copied from the \
                 SearchMatch, never rebuilt from the relative path or the label: a derived path \
                 sends `open_at` at a file that may not exist, and `:edit` would then create an \
                 empty buffer named after it.",
                row.abs_path.display(),
                want_path.display()
            );
            assert_eq!(
                row.line_number, *line,
                "row {index} ({rel}) carries line_number {}, expected {line}. This value goes \
                 straight to `nvim_win_set_cursor`, so a wrong one opens the right file at the \
                 wrong line — the failure a screenshot cannot see, which is why the cage smoke \
                 reads `line('.')` back. If this is red together with the column assertion below, \
                 the two assignments were swapped.",
                row.line_number
            );
            assert_eq!(
                row.column, *column,
                "row {index} ({rel}) carries column {}, expected {column}. It is rg's 0-based \
                 byte offset and `nvim_win_set_cursor` takes it in the same convention, so no \
                 conversion belongs here — the +1 to Neovim's 1-based `col('.')` happens only \
                 where a readback is compared. If this is red together with the line assertion \
                 above, the two assignments were swapped.",
                row.column
            );
        }
    }

    // =====================================================================
    // result_rows: the label
    // =====================================================================

    /// The label fixture's file, chosen with a parent directory so
    /// `SearchFileGroup::label` is the non-trivial `"<basename> - <parent>"` form
    /// rather than a bare basename.
    const LABEL_FIXTURE_FILE: &str = "notes/hits.md";

    /// Four matched lines covering every truncation shape [`context_window`] can
    /// produce, as `(line_number, column, tail_pad)`.
    ///
    /// With [`TEST_BUDGET`] and a ten-byte token: a short line truncates on neither
    /// side, a match far into a long line truncates on both, a match at the very end
    /// truncates only on the left, and a match at offset 0 of a long line truncates
    /// only on the right. All four shapes are asserted to be present below, so a
    /// fixture that lost one cannot leave the iff untested in that direction.
    const LABEL_FIXTURE_LINES: [(u64, u32, usize); 4] =
        [(3, 2, 3), (11, 60, 60), (22, 60, 0), (47, 0, 60)];

    /// A row's label carries the file label, the line number and
    /// [`context_window`]'s windowed text, with an ellipsis on each side that was
    /// truncated and on neither side that was not.
    ///
    /// The expected tail is DERIVED from `context_window`'s own output for the same
    /// inputs rather than hand-typed, so this test states the rule ("show the window
    /// core computed, and mark what it hid") instead of restating one fixture's
    /// characters. Dropping the truncation flags and emitting the bare
    /// `ContextWindow::text` — which reads as a harmless simplification — is what it
    /// refuses: a row would then claim to show a whole line while showing a slice of
    /// one, and the user could not tell a match at column 5 from a match at column
    /// 500.
    #[test]
    fn result_rows_labels_carry_the_file_line_and_windowed_text() {
        assert!(
            !ELLIPSIS.is_empty(),
            "ELLIPSIS is empty, so a truncated row is drawn exactly like an untruncated one and \
             every assertion below would pass with the truncation handling deleted entirely. The \
             glyph is the only thing that tells a user the line continues past what they can see."
        );

        let matches: Vec<SearchMatch> = LABEL_FIXTURE_LINES
            .iter()
            .map(|(line, column, tail_pad)| {
                fixture_match(LABEL_FIXTURE_FILE, *line, *column, *tail_pad)
            })
            .collect();

        let groups = project(&matches);
        let group_label = groups
            .first()
            .map(|group| group.label.clone())
            .unwrap_or_else(|| {
                panic!(
                    "`wolf_core::project` returned no groups for the {} label fixture matches, so \
                     there is no file label to compare against and this test cannot run.",
                    matches.len()
                )
            });
        assert!(
            group_label.contains(" - "),
            "the label fixture's group label is {group_label:?}, which is a bare basename. The \
             fixture uses a nested path precisely so `SearchFileGroup::label` takes its \
             `\"<basename> - <parent>\"` form; with a root file the `starts_with` assertion below \
             would not notice a label built from the basename alone."
        );

        let rows = result_rows(&matches, TEST_BUDGET, SEARCH_MAX_ROWS);
        assert_eq!(
            rows.len(),
            matches.len(),
            "result_rows produced {} row(s) for {} label-fixture matches, so at least one \
             truncation shape below is unmeasured.",
            rows.len(),
            matches.len()
        );

        let mut truncated_left = 0_usize;
        let mut truncated_right = 0_usize;
        let mut untruncated = 0_usize;

        for row in &rows {
            let source = matches
                .iter()
                .find(|m| m.abs_path == row.abs_path && m.line_number == row.line_number)
                .unwrap_or_else(|| {
                    panic!(
                        "row for line {} of {} has no source match in the fixture, so its label \
                         cannot be checked against the window core would compute. Either \
                         `result_rows` invented a row or it rewrote a location, and \
                         `result_rows_carries_the_exact_location` is the test to read next.",
                        row.line_number,
                        row.abs_path.display()
                    )
                });

            let window = context_window(&source.line_text, &source.match_ranges, TEST_BUDGET);
            if window.truncated_left {
                truncated_left += 1;
            }
            if window.truncated_right {
                truncated_right += 1;
            }
            if !window.truncated_left && !window.truncated_right {
                untruncated += 1;
            }

            let lead = if window.truncated_left { ELLIPSIS } else { "" };
            let trail = if window.truncated_right { ELLIPSIS } else { "" };
            let want_tail = format!("{LABEL_SEPARATOR}{lead}{}{trail}", window.text);

            assert!(
                row.label.starts_with(&group_label),
                "the label for line {} is {:?}, which does not begin with the file label \
                 {group_label:?} that `wolf_core::file_label` computed. Every row must name its \
                 file first: this is a FLAT list with no expandable file header, so the file name \
                 on the row is the only thing that says which of several matched files a row \
                 belongs to.",
                row.line_number,
                row.label
            );
            assert!(
                row.label.contains(&row.line_number.to_string()),
                "the label for line {} is {:?} and does not contain that line number. It is the \
                 only thing that distinguishes two matches in the same file, and the whole point \
                 of the row is that it names a location rather than a file.",
                row.line_number,
                row.label
            );
            assert!(
                row.label.ends_with(&want_tail),
                "the label for line {} is {:?}, which does not end with {want_tail:?} — the \
                 window `wolf_core::context_window` computed for that line at a budget of \
                 {TEST_BUDGET}, wrapped in an ellipsis on each side it truncated (left: {}, \
                 right: {}). Emitting the bare `ContextWindow::text` instead reads as a harmless \
                 simplification and is not one: the row would then claim to show a whole line \
                 while showing a slice of one, with nothing to say the match sits far off the \
                 part on screen.",
                row.line_number,
                row.label,
                window.truncated_left,
                window.truncated_right
            );
            assert_eq!(
                row.label.contains(ELLIPSIS),
                window.truncated_left || window.truncated_right,
                "the label for line {} is {:?}; the ellipsis {ELLIPSIS:?} must appear when and \
                 ONLY when core reported truncation (left: {}, right: {}). An ellipsis on every \
                 row is the same as no ellipsis at all, and one on no row leaves a trimmed line \
                 looking complete.",
                row.line_number,
                row.label,
                window.truncated_left,
                window.truncated_right
            );
        }

        assert!(
            truncated_left > 0 && truncated_right > 0 && untruncated > 0,
            "the label fixture produced {truncated_left} left-truncated, {truncated_right} \
             right-truncated and {untruncated} untruncated row(s); each must be non-zero. The \
             ellipsis assertion is an IFF, so a fixture missing one direction leaves that half \
             unmeasured: with no untruncated row, an implementation that always appends the \
             glyph passes; with no truncated row, one that never appends it passes."
        );
    }

    // =====================================================================
    // result_rows: the cap, and the empty case
    // =====================================================================

    /// The row cap the cap test applies. Far below the fixture's match count, and
    /// not equal to it, so `.take(max)` has something to do.
    const TEST_MAX_ROWS: usize = 3;

    /// The cap applies to the PROJECTED list, keeping the first `max` rows in the
    /// order a user would see them.
    ///
    /// The bound exists because every row is a widget the GTK main thread pays for.
    /// The comparison is against the seam's own uncapped output rather than a
    /// hand-written list, so the test states the rule ("the first `max` of what you
    /// would otherwise show") and stays correct if the projection order changes for
    /// a reason `result_rows_orders_by_the_core_projection` sanctions.
    #[test]
    fn result_rows_caps_at_max() {
        let matches: Vec<SearchMatch> = (0..7_u64)
            .map(|index| {
                fixture_match(
                    "capped.md",
                    // Non-contiguous and never equal to the row index, so a cap that
                    // kept the wrong rows cannot coincide with one that kept the
                    // right ones.
                    (index + 1) * 6,
                    u32::try_from(index).expect("a fixture index fits in u32") + 2,
                    4,
                )
            })
            .collect();

        let uncapped = result_rows(&matches, TEST_BUDGET, matches.len());
        assert!(
            uncapped.len() > TEST_MAX_ROWS,
            "the uncapped fixture yields only {} row(s), which is not more than TEST_MAX_ROWS = \
             {TEST_MAX_ROWS}. With nothing to drop, the capped call below returns the same list \
             whether the cap is applied or deleted, and this test proves nothing.",
            uncapped.len()
        );

        let capped = result_rows(&matches, TEST_BUDGET, TEST_MAX_ROWS);
        assert_eq!(
            capped.len(),
            TEST_MAX_ROWS,
            "result_rows returned {} row(s) for a max of {TEST_MAX_ROWS} over {} matches. Every \
             row is a `gtk::ListBoxRow` plus a `gtk::Label` built on the GTK main thread, and \
             `SearchState::matches` is UNBOUNDED while rg runs — so an unapplied cap means a \
             refresh mid-search materializes one widget per match found so far, on the thread \
             that is also drawing the editor.",
            capped.len(),
            matches.len()
        );

        let got: Vec<(PathBuf, u64, u32)> = capped
            .iter()
            .map(|row| (row.abs_path.clone(), row.line_number, row.column))
            .collect();
        let want: Vec<(PathBuf, u64, u32)> = uncapped
            .iter()
            .take(TEST_MAX_ROWS)
            .map(|row| (row.abs_path.clone(), row.line_number, row.column))
            .collect();
        assert_eq!(
            got, want,
            "the capped rows are not the FIRST {TEST_MAX_ROWS} of the uncapped list. The cap must \
             trim the tail: a user who searches and sees a truncated list expects the matches \
             nearest the top of the project, and `search_status` reports the count it hid on that \
             understanding."
        );
    }

    /// No matches means no rows.
    ///
    /// This is a PRECONDITION for the status table rather than a rule of its own —
    /// `search_status`'s no-matches arm is only reachable with `shown_rows == 0` —
    /// so it has no revert of its own and none should be invented for it.
    #[test]
    fn result_rows_empty_matches_is_empty() {
        let rows = result_rows(&[], SEARCH_CHAR_BUDGET, SEARCH_MAX_ROWS);
        assert!(
            rows.is_empty(),
            "result_rows returned {} row(s) for zero matches. A completed search with no matches \
             must leave the list EMPTY, so `activation_target` is a true no-op and the second \
             Enter does nothing at all rather than opening whatever a synthesised row pointed at.",
            rows.len()
        );
    }

    // =====================================================================
    // activation_target
    // =====================================================================

    /// The rows `activation_target` resolves against, as `(rel_path, line_number,
    /// column)` in the order they are listed.
    ///
    /// Three different FILES, three different lines and three different columns, and
    /// no value equal to any row index: a resolution that returned the top row's
    /// path, or a line derived from the index, is red rather than lucky.
    const ACTIVATION_FIXTURE: [(&str, u64, u32); 3] = [
        ("a_hits.md", 5, 4),
        ("b_hits.md", 18, 9),
        ("c_hits.md", 33, 26),
    ];

    /// Builds the first `n` [`ACTIVATION_FIXTURE`] rows by hand.
    ///
    /// Hand-built rather than routed through `result_rows`, so a failure here is
    /// unambiguously `activation_target`'s: the two seams are tested against
    /// separate fixtures precisely so one going red does not redden the other.
    fn activation_fixture_rows(n: usize) -> Vec<SearchRow> {
        assert!(
            n <= ACTIVATION_FIXTURE.len(),
            "an activation case asked for {n} fixture rows, but ACTIVATION_FIXTURE names only {}. \
             Add another row with a distinct path, line and column rather than repeating one: two \
             rows sharing a value would make the resolved-row equality unable to tell them apart, \
             so a function returning the WRONG row would still satisfy it.",
            ACTIVATION_FIXTURE.len()
        );
        ACTIVATION_FIXTURE
            .iter()
            .take(n)
            .map(|(rel, line, column)| SearchRow {
                abs_path: PathBuf::from(FIXTURE_ROOT).join(rel),
                line_number: *line,
                column: *column,
                // Never anything an assertion in this module reads: `activation_target`
                // resolves an index and copies three fields, and must not be given a
                // reason to look as though the label matters.
                label: format!("row {rel}"),
            })
            .collect()
    }

    /// The `(path, line, col)` triple [`ACTIVATION_FIXTURE`] row `index` must
    /// produce — derived from the fixture DATA, not from the rows the seam returns.
    fn expected_target(index: usize) -> (PathBuf, u64, u64) {
        let (rel, line, column) = ACTIVATION_FIXTURE[index];
        (
            PathBuf::from(FIXTURE_ROOT).join(rel),
            line,
            u64::from(column),
        )
    }

    /// One `activation_target` table row. `expected` is a fixture INDEX so the
    /// triple is derived rather than retyped.
    struct ActivationCase {
        desc: &'static str,
        n_rows: usize,
        selected: Option<i32>,
        expected: Option<usize>,
    }

    /// What the second `Enter` resolves to, as data. Walked by
    /// [`activation_target_table`] for the exact answers and by
    /// [`activation_target_resolves_to_the_selected_row`] for the property that must
    /// hold across all of them.
    const ACTIVATION_CASES: &[ActivationCase] = &[
        ActivationCase {
            desc: "zero rows, nothing selected => nothing is opened",
            n_rows: 0,
            selected: None,
            expected: None,
        },
        ActivationCase {
            desc: "zero rows, but a stale selection index => still nothing",
            n_rows: 0,
            selected: Some(0),
            expected: None,
        },
        ActivationCase {
            desc: "rows but no selection => the TOP row (type a query, Enter, Enter)",
            n_rows: 3,
            selected: None,
            expected: Some(0),
        },
        ActivationCase {
            desc: "GTK's -1 'row not in a list' index => the TOP row",
            n_rows: 3,
            selected: Some(-1),
            expected: Some(0),
        },
        ActivationCase {
            desc: "a selection in range => THAT row's location, not the top one's",
            n_rows: 3,
            selected: Some(2),
            expected: Some(2),
        },
        ActivationCase {
            desc: "a selection past the end => the TOP row, never out of bounds",
            n_rows: 3,
            selected: Some(9),
            expected: Some(0),
        },
    ];

    /// Pins the `(path, line, col)` triple for every selection shape the widget
    /// layer can hand in.
    #[test]
    fn activation_target_table() {
        for case in ACTIVATION_CASES {
            let rows = activation_fixture_rows(case.n_rows);
            let got = activation_target(&rows, case.selected);
            let want = case.expected.map(expected_target);
            assert_eq!(
                got, want,
                "case: {} — activation_target(<{} row(s)>, selected = {:?}) returned {got:?}, \
                 expected {want:?}. With ZERO rows a Some here sends `open_at` at a location that \
                 was never on the list, which `:edit`s a path the user never chose. With rows \
                 present, the triple must be the SELECTED row's three fields verbatim: the line \
                 and column are what make this a jump to a match rather than a file open, and a \
                 hard-coded line of 1 would look identical in every screenshot.",
                case.desc, case.n_rows, case.selected
            );
        }
    }

    /// The property that must hold across every [`ACTIVATION_CASES`] row: the
    /// returned triple is exactly the row `activation_index` resolves to.
    ///
    /// The resolved-row EQUALITY is the load-bearing assertion, and membership would
    /// be measurably too weak: an `activation_target` returning `rows.first()`'s
    /// triple for every input satisfies "the location is one of the rows on offer"
    /// on every row of the table — the top row is a listed location like any other —
    /// while opening the wrong match for every selection below the top. The
    /// `nonzero_resolved` guard is what stops that equality holding vacuously over a
    /// table whose opening rows all resolve to row 0.
    ///
    /// It shares its inputs with [`activation_target_table`] but not its oracle: the
    /// table's expectations are hand-written fixture data, while these are derived
    /// from the resolution rule `activation_target` documents itself as using, so a
    /// table row whose `expected` is written wrong is red here.
    #[test]
    fn activation_target_resolves_to_the_selected_row() {
        let mut opens = 0_usize;
        let mut nonzero_resolved = 0_usize;

        for case in ACTIVATION_CASES {
            let rows = activation_fixture_rows(case.n_rows);
            let Some((path, line, column)) = activation_target(&rows, case.selected) else {
                continue;
            };
            opens += 1;

            let resolved = activation_index(case.selected, rows.len()).unwrap_or_else(|| {
                panic!(
                    "case: {} — activation_target returned an open at {} line {line} column \
                     {column}, but activation_index({:?}, {}) resolves to NO row at all. The \
                     second Enter must be a true no-op whenever there is no row to activate, so \
                     an open here targets a location the overlay never put on screen.",
                    case.desc,
                    path.display(),
                    case.selected,
                    rows.len()
                )
            });
            if resolved != 0 {
                nonzero_resolved += 1;
            }

            let row = &rows[resolved];
            assert_eq!(
                (path.clone(), line, column),
                (row.abs_path.clone(), row.line_number, u64::from(row.column)),
                "case: {} — activation_target returned ({}, {line}, {column}), but the selection \
                 {:?} resolves through activation_index to row {resolved}, whose location is ({}, \
                 {}, {}). Enter therefore opens the WRONG match: the row the user sees \
                 highlighted is not the place the editor jumps to. Membership in the row list \
                 does NOT catch this — returning `rows.first()`'s triple for every input keeps \
                 every returned location on the list while getting every non-top selection wrong \
                 — so this must stay an equality against the RESOLVED row. The column half also \
                 pins the widening: `u32` to `u64` must be lossless and must not ADJUST the \
                 value, because rg's column and `nvim_win_set_cursor`'s column are both 0-based \
                 byte offsets. A +1 applied here would land the cursor one byte past every match; \
                 the only place it belongs is where a readback of Neovim's 1-based `col('.')` is \
                 compared.",
                case.desc,
                path.display(),
                case.selected,
                row.abs_path.display(),
                row.line_number,
                row.column
            );
        }

        assert!(
            opens > 0,
            "NOT ONE row of ACTIVATION_CASES produced an open, so every assertion in this test \
             ran zero times and it proves nothing. Either `activation_target` refuses every \
             input — in which case the second Enter can never open a match and \
             `activation_target_table` is red too — or the table lost all of its non-degenerate \
             rows."
        );
        assert!(
            nonzero_resolved > 0,
            "every ACTIVATION_CASES row that produced an open resolved to row 0, so the \
             resolved-row equality never had to tell the selected row apart from the top one and \
             an `activation_target` returning `rows.first()`'s triple unconditionally passes this \
             whole test. At least one row must pair an IN-RANGE, NON-ZERO `selected` with a \
             non-empty list: `selected: Some(2)` over three rows is the row carrying that. Either \
             that row left the table, or `activation_target` has stopped resolving anything but \
             the top row — which is the regression itself, and `activation_target_table` is red \
             too."
        );
    }

    // =====================================================================
    // enter_action
    // =====================================================================

    /// One `enter_action` table row.
    struct EnterCase {
        desc: &'static str,
        query: &'static str,
        ran_query: Option<&'static str>,
        selected: Option<i32>,
        n_rows: usize,
        want: EnterAction,
    }

    /// What each `Enter` does, as data.
    ///
    /// The two-way decision is the shape of the whole overlay: the FIRST Enter runs
    /// the search the entry's text describes, and the second — the entry unchanged —
    /// activates the selected row. Everything else is the degenerate cases around
    /// those two.
    const ENTER_CASES: &[EnterCase] = &[
        EnterCase {
            desc: "an empty query is never run: `rg -F -e ''` matches every line of every file",
            query: "",
            ran_query: None,
            selected: None,
            n_rows: 0,
            want: EnterAction::Nothing,
        },
        EnterCase {
            desc: "a whitespace-only query is the same runaway, so it is refused too",
            query: "   ",
            ran_query: None,
            selected: None,
            n_rows: 0,
            want: EnterAction::Nothing,
        },
        EnterCase {
            desc: "the empty guard comes FIRST: a blanked entry activates nothing either",
            query: " \t ",
            ran_query: Some(" \t "),
            selected: Some(1),
            n_rows: 3,
            want: EnterAction::Nothing,
        },
        EnterCase {
            desc: "a query that has never been run => run it; nothing is open yet to activate",
            query: "zebracross",
            ran_query: None,
            selected: None,
            n_rows: 0,
            want: EnterAction::RunSearch,
        },
        EnterCase {
            desc: "the entry changed since the last run => run the NEW query, do not activate",
            query: "zebracross",
            ran_query: Some("zebra"),
            selected: Some(1),
            n_rows: 3,
            want: EnterAction::RunSearch,
        },
        EnterCase {
            desc: "a trailing space is a DIFFERENT literal search, so it re-runs",
            query: "zebracross ",
            ran_query: Some("zebracross"),
            selected: Some(1),
            n_rows: 3,
            want: EnterAction::RunSearch,
        },
        EnterCase {
            desc: "the query already ran and rows are up, no selection => activate the top row",
            query: "zebracross",
            ran_query: Some("zebracross"),
            selected: None,
            n_rows: 3,
            want: EnterAction::Activate(0),
        },
        EnterCase {
            desc: "the query already ran, an in-range selection => activate THAT row",
            query: "zebracross",
            ran_query: Some("zebracross"),
            selected: Some(2),
            n_rows: 3,
            want: EnterAction::Activate(2),
        },
        EnterCase {
            desc: "GTK's -1 'row not in a list' index => the top row",
            query: "zebracross",
            ran_query: Some("zebracross"),
            selected: Some(-1),
            n_rows: 3,
            want: EnterAction::Activate(0),
        },
        EnterCase {
            desc: "a selection past the end => the top row, never out of bounds",
            query: "zebracross",
            ran_query: Some("zebracross"),
            selected: Some(9),
            n_rows: 3,
            want: EnterAction::Activate(0),
        },
        EnterCase {
            desc: "the query ran and found nothing => Enter is a true no-op, not a re-run",
            query: "zebracross",
            ran_query: Some("zebracross"),
            selected: Some(0),
            n_rows: 0,
            want: EnterAction::Nothing,
        },
    ];

    /// Pins the two-way `Enter` decision over every shape the widget layer can hand
    /// in, and checks each `Activate` index against `activation_index` for the same
    /// inputs.
    #[test]
    fn enter_action_table() {
        let mut runs = 0_usize;
        let mut activations = 0_usize;
        let mut nonzero_activations = 0_usize;
        let mut nothings = 0_usize;

        for case in ENTER_CASES {
            let got = enter_action(case.query, case.ran_query, case.selected, case.n_rows);
            assert_eq!(
                got, case.want,
                "case: {} — enter_action({:?}, {:?}, {:?}, {}) returned {got:?}, expected {:?}. \
                 The three outcomes are not interchangeable: a RunSearch where an Activate \
                 belongs means the second Enter re-runs the same search instead of opening the \
                 selected match, so the overlay never closes and the chord appears to do nothing; \
                 an Activate where a RunSearch belongs means a changed query opens a row from the \
                 PREVIOUS search's results; and a Nothing on a non-empty query makes Enter dead.",
                case.desc, case.query, case.ran_query, case.selected, case.n_rows, case.want
            );

            match got {
                EnterAction::RunSearch => runs += 1,
                EnterAction::Nothing => nothings += 1,
                EnterAction::Activate(index) => {
                    activations += 1;
                    if index != 0 {
                        nonzero_activations += 1;
                    }
                    assert_eq!(
                        activation_index(case.selected, case.n_rows),
                        Some(index),
                        "case: {} — enter_action activated row {index}, but \
                         activation_index({:?}, {}) — the resolution rule every overlay in this \
                         crate shares — gives {:?}. The two must agree: the widget layer selects \
                         a `ListBox` row by that index, so a divergence highlights one row and \
                         opens another.",
                        case.desc,
                        case.selected,
                        case.n_rows,
                        activation_index(case.selected, case.n_rows)
                    );
                }
            }

            assert!(
                !(matches!(got, EnterAction::Activate(_)) && case.query.trim().is_empty()),
                "case: {} — enter_action returned {got:?} for a query whose trim is empty. The \
                 empty-query guard must come FIRST: it is the bound that stops `rg -F -e ''`, \
                 which matches every line of every file under the search root.",
                case.desc
            );
        }

        assert!(
            runs > 0 && activations > 0 && nothings > 0,
            "ENTER_CASES produced {runs} RunSearch, {activations} Activate and {nothings} Nothing \
             outcome(s); each must be non-zero or a whole arm of `enter_action` is unmeasured. \
             The RunSearch rows are what refuse a one-way Enter that only ever activates; the \
             Nothing rows are what refuse an Enter that spawns rg on an empty query."
        );
        assert!(
            nonzero_activations > 0,
            "every ENTER_CASES row that activated resolved to row 0, so an `enter_action` that \
             returned `Activate(0)` for any run query at all would satisfy this table. At least \
             one row must pair an already-run query with an IN-RANGE, NON-ZERO selection: \
             `selected: Some(2)` over three rows is the row carrying that."
        );
    }

    // =====================================================================
    // search_status
    // =====================================================================

    /// The error string core produces when ripgrep was never located. It is the only
    /// message this arm ever has to carry in practice, and the status line is the
    /// only surface it reaches.
    const STATUS_ERROR_TEXT: &str = "WOLF_RG_BIN unset";

    /// Which arm of `search_status` a [`StatusCase`] expects.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum WantStatus {
        /// Nothing has been run yet.
        Idle,
        /// The session carries an error, whatever else it carries.
        Error,
        /// rg is still running.
        Running,
        /// Finished, and there were no matches at all.
        NoMatches,
        /// Finished, and the row cap hid some of them.
        Truncated,
        /// Finished, and every match is on screen.
        Complete,
    }

    /// Every arm `search_status` is total over. The table below is asserted to cover
    /// all of them.
    const ALL_STATUS_ARMS: [WantStatus; 6] = [
        WantStatus::Idle,
        WantStatus::Error,
        WantStatus::Running,
        WantStatus::NoMatches,
        WantStatus::Truncated,
        WantStatus::Complete,
    ];

    /// One `search_status` table row.
    struct StatusCase {
        desc: &'static str,
        ran: bool,
        total: usize,
        shown: usize,
        complete: bool,
        error: Option<&'static str>,
        want: WantStatus,
    }

    /// The status line, as data.
    ///
    /// Two rows deliberately share their counts and differ only in `complete`, so
    /// the distinctness assertion has to tell "still searching, 17 so far" apart
    /// from "17 matches" rather than being satisfied by different numbers.
    ///
    /// A third row exists for the count `1` alone. Every other counting row carries
    /// a plural total, so `match_count`'s singular branch is reached from no other
    /// row here, and `search_status` could return "1 matches" with the rest of this
    /// table green.
    const STATUS_CASES: &[StatusCase] = &[
        StatusCase {
            desc: "nothing run yet: the state the overlay opens in",
            ran: false,
            total: 0,
            shown: 0,
            complete: false,
            error: None,
            want: WantStatus::Idle,
        },
        StatusCase {
            desc: "nothing run yet, with stale counts and an error left over: still idle",
            ran: false,
            total: 12,
            shown: 12,
            complete: true,
            error: Some(STATUS_ERROR_TEXT),
            want: WantStatus::Idle,
        },
        StatusCase {
            desc: "a failed session is ALSO complete, so the error arm must come first",
            ran: true,
            total: 0,
            shown: 0,
            complete: true,
            error: Some(STATUS_ERROR_TEXT),
            want: WantStatus::Error,
        },
        StatusCase {
            desc: "rg is still running: report the count so far",
            ran: true,
            total: 17,
            shown: 17,
            complete: false,
            error: None,
            want: WantStatus::Running,
        },
        StatusCase {
            desc: "finished with nothing found",
            ran: true,
            total: 0,
            shown: 0,
            complete: true,
            error: None,
            want: WantStatus::NoMatches,
        },
        StatusCase {
            desc: "finished, and the cap hid most of them: name BOTH numbers",
            ran: true,
            total: 4312,
            shown: 200,
            complete: true,
            error: None,
            want: WantStatus::Truncated,
        },
        StatusCase {
            desc: "finished, every match on screen: the same counts as the running row",
            ran: true,
            total: 17,
            shown: 17,
            complete: true,
            error: None,
            want: WantStatus::Complete,
        },
        StatusCase {
            desc: "finished with exactly one match: the singular",
            ran: true,
            total: 1,
            shown: 1,
            complete: true,
            error: None,
            want: WantStatus::Complete,
        },
    ];

    /// Pins each arm of the status line, and that the six arms produce mutually
    /// distinct strings.
    ///
    /// Distinctness is what makes the line worth drawing at all: "still searching"
    /// and "finished" are the two states a user acts on differently, and a search
    /// that failed before it started must not read like one that finished with no
    /// matches — the first is fixed by provisioning ripgrep and the second by typing
    /// a different query.
    #[test]
    fn search_status_table() {
        for arm in ALL_STATUS_ARMS {
            assert!(
                STATUS_CASES.iter().any(|case| case.want == arm),
                "no STATUS_CASES row covers the {arm:?} arm, so `search_status` has a branch \
                 nothing measures. The arm most likely to be dropped is Error, which is the only \
                 surface the string {STATUS_ERROR_TEXT:?} ever reaches — without it a devShell \
                 with no ripgrep looks exactly like a query with no matches."
            );
        }

        let mut representative: Vec<(WantStatus, String)> = Vec::new();

        for case in STATUS_CASES {
            let got = search_status(case.ran, case.total, case.shown, case.complete, case.error);
            assert!(
                !got.trim().is_empty(),
                "case: {} — search_status returned an empty string. Every arm draws into the same \
                 label, so an empty one leaves the previous arm's text on screen or a blank strip \
                 where the state should be.",
                case.desc
            );

            match case.want {
                WantStatus::Idle => assert_eq!(
                    got, SEARCH_IDLE_STATUS,
                    "case: {} — search_status returned {got:?}, expected the idle text \
                     {SEARCH_IDLE_STATUS:?}. `ran = false` must win over every other input: the \
                     overlay opens with a stale session's counts and error still in the struct, \
                     and reporting them would tell the user their brand-new empty entry had \
                     already failed.",
                    case.desc
                ),
                WantStatus::Error => {
                    assert!(
                        got.starts_with(SEARCH_ERROR_PREFIX),
                        "case: {} — search_status returned {got:?}, which does not start with \
                         {SEARCH_ERROR_PREFIX:?}. The prefix is what marks the line as a failure \
                         rather than a result count.",
                        case.desc
                    );
                    assert!(
                        got.contains(STATUS_ERROR_TEXT),
                        "case: {} — search_status returned {got:?}, which does not contain the \
                         error {STATUS_ERROR_TEXT:?} verbatim. This label is the ONLY surface \
                         core's message reaches; swallowing it leaves a user with an empty result \
                         list and no way to learn that ripgrep was never located.",
                        case.desc
                    );
                }
                WantStatus::Running => {
                    assert!(
                        got.starts_with(SEARCH_RUNNING_PREFIX),
                        "case: {} — search_status returned {got:?}, which does not start with \
                         {SEARCH_RUNNING_PREFIX:?}. Without it a partial list reads as a finished \
                         one, and a user stops waiting for matches that are still arriving.",
                        case.desc
                    );
                    assert!(
                        got.contains(&case.total.to_string()),
                        "case: {} — search_status returned {got:?}, which does not name the {} \
                         matches found so far. The count is what makes a running line different \
                         from a spinner.",
                        case.desc,
                        case.total
                    );
                }
                WantStatus::NoMatches => assert_eq!(
                    got, SEARCH_NO_MATCHES_STATUS,
                    "case: {} — search_status returned {got:?}, expected the no-matches text \
                     {SEARCH_NO_MATCHES_STATUS:?}. A finished search with zero results must say \
                     so: an empty list with a count of zero and no words is indistinguishable \
                     from a search that never started.",
                    case.desc
                ),
                WantStatus::Truncated => {
                    assert!(
                        got.contains(&case.total.to_string()),
                        "case: {} — search_status returned {got:?}, which does not name the total \
                         of {}. A capped list that reports only what it shows is a list that lies \
                         about the search.",
                        case.desc,
                        case.total
                    );
                    assert!(
                        got.contains(&case.shown.to_string()),
                        "case: {} — search_status returned {got:?}, which does not name the {} \
                         rows actually on screen. Both numbers are needed: the gap between them \
                         is the whole message.",
                        case.desc,
                        case.shown
                    );
                }
                WantStatus::Complete => {
                    assert!(
                        got.contains(&case.total.to_string()),
                        "case: {} — search_status returned {got:?}, which does not name the {} \
                         matches found.",
                        case.desc,
                        case.total
                    );
                    assert_ne!(
                        got, SEARCH_NO_MATCHES_STATUS,
                        "case: {} — a finished search with {} matches produced the NO-MATCHES \
                         text {SEARCH_NO_MATCHES_STATUS:?}.",
                        case.desc, case.total
                    );
                    assert!(
                        !got.starts_with(SEARCH_ERROR_PREFIX),
                        "case: {} — a successful, finished search produced {got:?}, which reads \
                         as an error.",
                        case.desc
                    );
                    if case.total == 1 {
                        assert_eq!(
                            got, "1 match",
                            "case: {} — a finished search with exactly one match produced \
                             {got:?}. The count assertion above names the total, so a plural \
                             `1 matches` satisfies it too; this is the only check that separates \
                             the singular from the plural, and the singular is the sole reason \
                             `match_count` exists rather than a bare `format!`.",
                            case.desc
                        );
                    }
                }
            }

            if !representative.iter().any(|(arm, _)| *arm == case.want) {
                representative.push((case.want, got));
            }
        }

        let mut seen: HashSet<&str> = HashSet::new();
        for (arm, text) in &representative {
            assert!(
                seen.insert(text.as_str()),
                "the {arm:?} arm produced {text:?}, which another arm produced too. The six arms \
                 must be mutually distinguishable ON SCREEN: sharing a string collapses states \
                 whose fixes are different — a missing ripgrep, a still-running search, a query \
                 with no matches and a capped result list all become the same sentence, and the \
                 user cannot tell which one they are looking at. Representatives were: {:?}",
                representative
                    .iter()
                    .map(|(arm, text)| (*arm, text.as_str()))
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(
            seen.len(),
            ALL_STATUS_ARMS.len(),
            "only {} of the {} status arms produced a representative string, so the distinctness \
             check above ran over an incomplete set.",
            seen.len(),
            ALL_STATUS_ARMS.len()
        );
    }

    /// The four fixed status strings are non-empty, mutually distinct, and neither
    /// prefix is a prefix of the other.
    ///
    /// The check a reader might expect here — that no status string contains a term
    /// a cage smoke asserts OCR must not find — CANNOT live in this module.
    /// `crates/wolf-gnome/tests/smoke_headless.rs` is a separate crate and its
    /// `ASSERTED_OCR_TERMS` table is not reachable from `src/`, which is the same
    /// reason `buffer_picker::tests` duplicates its smoke constants rather than
    /// importing them. What is checkable here is stated instead, and the prefix
    /// clause is not decoration: [`search_status_table`] tells the error and running
    /// arms apart with `starts_with`, so one prefix being a prefix of the other
    /// would make both of those assertions satisfiable by the wrong arm.
    #[test]
    fn fixed_status_strings_are_distinct_and_non_empty() {
        let fixed = [
            ("SEARCH_IDLE_STATUS", SEARCH_IDLE_STATUS),
            ("SEARCH_NO_MATCHES_STATUS", SEARCH_NO_MATCHES_STATUS),
            ("SEARCH_ERROR_PREFIX", SEARCH_ERROR_PREFIX),
            ("SEARCH_RUNNING_PREFIX", SEARCH_RUNNING_PREFIX),
        ];

        let mut seen: HashSet<&str> = HashSet::new();
        for (name, text) in fixed {
            assert!(
                !text.trim().is_empty(),
                "{name} is empty or whitespace. It is drawn into the overlay's only status label, \
                 so an empty one leaves whatever the previous search wrote on screen."
            );
            assert!(
                seen.insert(text),
                "{name} = {text:?} duplicates another fixed status string. Two arms sharing a \
                 string make their states indistinguishable to the user, and it would make \
                 `search_status_table`'s distinctness assertion red for a cause reported one test \
                 away from here."
            );
        }

        assert!(
            !SEARCH_ERROR_PREFIX.starts_with(SEARCH_RUNNING_PREFIX)
                && !SEARCH_RUNNING_PREFIX.starts_with(SEARCH_ERROR_PREFIX),
            "one of SEARCH_ERROR_PREFIX = {SEARCH_ERROR_PREFIX:?} and SEARCH_RUNNING_PREFIX = \
             {SEARCH_RUNNING_PREFIX:?} is a prefix of the other. `search_status_table` separates \
             those two arms with `starts_with`, so a prefix relation lets a running line satisfy \
             the error assertion (or the reverse) and both checks stop discriminating."
        );
    }

    // =====================================================================
    // The cage smoke's premise, made executable in the fast suite
    // =====================================================================

    /// The project-search cage smoke's target file (its `PROJSEARCH_FILE_TARGET`) —
    /// the file the overlay must open.
    ///
    /// The five consts below are duplicated from `tests/smoke_headless.rs` rather
    /// than shared, because that file is a separate crate — the
    /// `buffer_picker::tests::SMOKE_TARGET` precedent.
    /// [`smoke_fixture_rows_select_exactly_the_target`] is what keeps the two from
    /// drifting: it proves in the FAST suite that this fixture still yields exactly
    /// this one row at exactly this location, so a row-assembly change reddens
    /// `cargo test -p wolf-gnome --lib` in milliseconds instead of making a
    /// fifteen-second cage smoke stochastic.
    ///
    /// Its `.txt` extension is deliberate and load-bearing over there: under core's
    /// `all_files: false` default it would not be searched at all. That premise is
    /// guarded in the smoke crate, next to the fixture it constrains.
    const SMOKE_TARGET_NAME: &str = "psrzulu.txt";

    /// The text the smoke types into the search entry (its `PROJSEARCH_QUERY`).
    const SMOKE_QUERY: &str = "zebracross";

    /// The 1-based line the smoke's match sits on (its `PROJSEARCH_MATCH_LINE`).
    /// Greater than 1, which is what lets `line('.')` tell an `open_at` apart from a
    /// plain file open.
    const SMOKE_MATCH_LINE: u64 = 9;

    /// The ASCII run before the query on that line (its `PROJSEARCH_MATCH_PREFIX`).
    const SMOKE_MATCH_PREFIX: &str = "xxxxxxxxxxxxx ";

    /// The 0-based byte column rg reports for the match — the value `open_at` and
    /// `nvim_win_set_cursor` both take.
    const SMOKE_MATCH_COLUMN: usize = SMOKE_MATCH_PREFIX.len();

    /// The 1-based column the smoke's readback of Neovim's `col('.')` must show
    /// (its `PROJSEARCH_EXPECTED_COL`). Derived, never typed: the +1 is the whole
    /// difference between rg's convention and Vim's.
    const SMOKE_EXPECTED_COL: usize = SMOKE_MATCH_COLUMN + 1;

    /// The cage smoke's premise, made executable: the single match its fixture
    /// produces yields exactly one row, and that row's location is the one the
    /// smoke's three readback assertions are written against.
    ///
    /// Every clause is load-bearing for the smoke. Its assertions 3, 4 and 5 read
    /// `expand('%:t')`, `line('.')` and `col('.')` out of the captive Neovim and
    /// compare them to the fixture's file, line and derived column; if the row
    /// assembly ever produced a different location, or more than one row, those
    /// three would fail fifteen seconds and one compositor later with no indication
    /// of which half was wrong.
    ///
    /// This test is a PRECONDITION on the smoke's fixture rather than a product rule
    /// of its own: it goes red under `result_rows_carries_the_exact_location`'s
    /// revert, and it has no isolated revert. Do not invent one for it.
    #[test]
    fn smoke_fixture_rows_select_exactly_the_target() {
        let column = u32::try_from(SMOKE_MATCH_COLUMN).expect("the smoke prefix is short");
        let token_len = u32::try_from(SMOKE_QUERY.len()).expect("the smoke query is short");
        let fixture = [SearchMatch {
            abs_path: PathBuf::from(FIXTURE_ROOT).join(SMOKE_TARGET_NAME),
            rel_path: SMOKE_TARGET_NAME.to_string(),
            line_number: SMOKE_MATCH_LINE,
            column,
            line_text: format!("{SMOKE_MATCH_PREFIX}{SMOKE_QUERY}"),
            match_ranges: vec![(column, column + token_len)],
        }];

        // The two properties that make the smoke's readbacks able to discriminate at
        // all, checked on the fixture rather than restated in prose.
        assert!(
            fixture[0].line_number > 1,
            "the smoke's match sits on line {}, so its `line('.')` readback could not tell an \
             `open_at` apart from a plain file open — `:edit` alone lands on line 1.",
            fixture[0].line_number
        );
        assert!(
            fixture[0].column > 0,
            "SMOKE_MATCH_PREFIX is empty, so the match starts at column 0 and the smoke's \
             `col('.')` readback of {SMOKE_EXPECTED_COL} could not tell an `open_at` apart from a \
             plain file open, which also leaves the cursor in column 1."
        );

        let rows = result_rows(&fixture, SEARCH_CHAR_BUDGET, SEARCH_MAX_ROWS);
        assert_eq!(
            rows.len(),
            1,
            "result_rows produced {} row(s) for the smoke's single match, not exactly one. The \
             smoke presses Enter a second time with NO arrow key, so which row is activated is \
             whatever `activation_index` resolves an absent selection to; more than one row makes \
             that a ranking question and the smoke stochastic rather than red.",
            rows.len()
        );

        let (path, line, col) = activation_target(&rows, None).unwrap_or_else(|| {
            panic!(
                "activation_target refused the smoke's one-row list with no selection, so the \
                 second Enter would send nothing and the smoke's file, line and column readbacks \
                 would all report the file it started in. With rows present an absent selection \
                 must resolve to the top row."
            )
        });

        assert!(
            path.ends_with(SMOKE_TARGET_NAME),
            "activation_target resolved to {}, whose final component is not the smoke's target \
             {SMOKE_TARGET_NAME:?}. The smoke's assertion 3 compares `expand('%:t')` against that \
             name.",
            path.display()
        );
        assert_eq!(
            line, SMOKE_MATCH_LINE,
            "activation_target resolved to line {line}, not the smoke's {SMOKE_MATCH_LINE}. That \
             is the value `open_at` hands `nvim_win_set_cursor`, and the smoke's assertion 4 \
             reads it back as `line('.')`."
        );
        assert_eq!(
            usize::try_from(col).ok().map(|zero_based| zero_based + 1),
            Some(SMOKE_EXPECTED_COL),
            "activation_target resolved to column {col} (0-based, rg's and \
             `nvim_win_set_cursor`'s convention), so Neovim's 1-based `col('.')` would report {}, \
             not the {SMOKE_EXPECTED_COL} the smoke's assertion 5 expects. This is the one number \
             a reviewer reads as a bug: the smoke derives its expected column as \
             `PROJSEARCH_MATCH_PREFIX.len() + 1` precisely because the two conventions differ by \
             one, and the seam must NOT apply that +1 itself.",
            usize::try_from(col)
                .map(|zero_based| zero_based + 1)
                .map_or_else(|_| "an out-of-range value".to_string(), |c| c.to_string())
        );

        let label = &rows[0].label;
        assert!(
            label.contains(SMOKE_QUERY),
            "the sole row's label is {label:?} and does not contain the typed query \
             {SMOKE_QUERY:?}. The user presses Enter a second time on a row they have to be able \
             to recognise as their match; a label that does not show the matched text is a row \
             they are activating blind."
        );
    }
}
