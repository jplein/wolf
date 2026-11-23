//! The sidebar's PROJECT-SEARCH panel for `wolf-gnome`: a ripgrep-backed content
//! search over the captive Neovim's live working directory, drawn as the Search
//! page of [`crate::sidebar`].
//!
//! Named `search_panel` rather than `search` so it never reads as
//! [`wolf_core::search`], whose [`SearchSession`] it is a thin front-end for.
//!
//! Trigger: the **`win.search-in-project`** action — bound to **Ctrl+Shift+F** and
//! listed in the primary menu as "Search in project" — the palette row, and
//! `:WolfSearchInProject`. All three reach `shell::AppShell::perform`, which shows
//! the sidebar if it is hidden, switches it to this page and focuses the field.
//!
//! ## Typing runs nothing; Enter runs, clears or activates
//!
//! There is deliberately no `connect_search_changed`: a search runs only when the
//! user presses Enter, so no keystroke can spawn `rg` and no debounce timer is
//! needed. [`enter_action`] owns the rule. An empty or whitespace-only query
//! CLEARS the panel and is never run, because core builds `rg -F -e ''`, which
//! matches every line of every file in the search's scope.
//!
//! A toggle runs nothing either. It turns the status line stale while the rows stay
//! drawn and clickable, and the next Enter re-runs, because [`enter_action`] compares
//! the toggles as well as the query against the live session's.
//!
//! ## The toggles
//!
//! Under the field, left-aligned and above the status line, sit `Aa` (match case),
//! `.*` (regular expression) and `All` (every file rather than only Markdown), the
//! counterpart's labels, each with a tooltip saying so. [`SearchToggles::INITIAL`]
//! gives their states on opening, and says why.
//!
//! No toggle takes focus on click. A focused toggle takes Enter as its own
//! activation and flips itself back, so the Enter after a click must still reach the
//! field.
//!
//! ## A line row is a LOCATION, not a file
//!
//! Activating a line row calls [`WolfCore::open_at`](wolf_core::WolfCore::open_at)
//! with the row's own `(path, line, col)`, which the core's input pump performs as
//! `:edit` followed by `nvim_win_set_cursor` inside ONE pump step.
//! [`SearchMatch::line_number`] is 1-based and [`SearchMatch::column`] is a 0-based
//! BYTE offset — exactly the two conventions `nvim_win_set_cursor` takes — so
//! [`row_activation`] widens the column losslessly and adjusts neither value.
//! Neovim's own `col('.')` is 1-based, so a readback of it is `column + 1`; that
//! `+ 1` belongs only where a readback is compared, never here.
//!
//! Over a PREVIEW pane the row repoints that preview instead, through the blocking
//! `WolfCore::preview_file`, which is the routing `specs/architecture.md` gives a
//! search result. [`activation_route`] decides, and its liveness arms are what keep
//! that blocking call off a wedged or dead Neovim: the panel sits outside the
//! opaque crash overlay, so nothing covers its rows.
//!
//! ## The list never takes keyboard focus
//!
//! Every streamed batch REPLACES every row widget, so a focused row would be
//! destroyed under the user. The `gtk::ListBox` is therefore not focusable, focus
//! stays in the field, and Up and Down move the list's selection from there.
//!
//! ## Nothing here blocks the GTK main thread, except the preview repoint
//!
//! [`WolfCore::start_search`](wolf_core::WolfCore::start_search) resolves
//! `WOLF_RG_BIN`, resolves the search root from the tracked-cwd cell and
//! `spawn_blocking`s onto the tokio runtime; `open_at` is fire-and-forget on the
//! ordered input channel; `liveness` is a lock-free atomic read. `preview_file` is
//! one of the ten blocking calls `crate::window` inventories, and this panel adds a
//! trigger for it rather than a call. `WolfCore::ignorecase()`, which IS a blocking
//! RPC, is deliberately never called, so the case toggle starts off instead of being
//! seeded from it.
//!
//! ## Streaming, and what a stale `rg` cannot do
//!
//! Results arrive over time, so this module uses [`crate::picker`]'s wake bridge:
//! one `async_channel::bounded` slot, a [`WakeFn`] that only `try_send`s, and a
//! `glib::spawn_future_local` drain holding a `Weak`, so dropping the handle ends
//! the task. The wake is tied to a SESSION that is replaced on each run. That is
//! safe because a run takes and drops the old session — the drop is what kills the
//! old `rg` child — before starting the new one, and because a refresh always
//! re-reads the CURRENT session.
//!
//! A refresh passes a [`TreeRefreshGate`], seeded hidden: it is refused while the
//! panel is not on screen and owed once when it returns, and it rebuilds only when
//! the session's version advanced or the character budget changed. Each session
//! numbers its versions from 1, so a run resets the version tracking.
//!
//! ## Where this panel differs from its counterpart
//!
//! It follows the macOS sidebar's search panel
//! (`crates/wolf-macos/Sources/WolfMacOS/README.md#searchviewswift`) except in four
//! ways, each a GNOME convention or a GTK constraint:
//!
//! - **Escape** returns keyboard focus to the editor and leaves the panel, its query,
//!   its rows and any running `rg` alone. The counterpart's field has no Escape
//!   behaviour of its own.
//! - **Focus stays in the field.** Up and Down move the selection, and a second
//!   Enter on an unchanged query and unchanged toggles opens it. There, Return always
//!   re-runs and the outline is reached with Tab; here a streaming rebuild replaces
//!   every row, so a focused row would be destroyed.
//! - **The case toggle starts off.** It is not seeded from the blocking
//!   `ignorecase()`.
//! - **A status line and a `SEARCH_MAX_ROWS` cap on line rows**, which the
//!   counterpart does not have.
//!
//! Its pure, display-free seams are [`show_decision`], [`SearchToggles`], [`SearchFields`],
//! [`search_options_for`], [`panel_rows`], [`char_budget`], [`first_line_row`], [`selection_after_rebuild`],
//! [`enter_action`], [`row_activation`], [`activation_route`] and [`search_status`],
//! tested in `crates/wolf-gnome/tests/sidebar_search.rs`.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use adw::prelude::*;
use gtk::glib;

use wolf_core::{
    context_window, project, Liveness, SearchMatch, SearchOptions, SearchReadGuard, SearchSession,
    WakeFn, WolfCore,
};

use crate::picker::{next_selection, FocusTarget};
use crate::tab_view::{with_core, CoreCell, CoreSource};
use crate::tree_gate::TreeRefreshGate;
use crate::window::TREE_PANE_WIDTH;

/// The maximum number of LINE rows materialized into the `ListBox` on each
/// rebuild ([`crate::picker`]'s `PICKER_MAX_ROWS` precedent).
///
/// This bounds the WIDGETS, which is what the GTK main thread pays for. It does
/// NOT bound the work behind them: [`panel_rows`] hands the WHOLE match slice to
/// [`wolf_core::project`], which groups and sorts every match before the cap
/// applies. `SearchState::matches` itself is unbounded while `rg` runs.
const SEARCH_MAX_ROWS: usize = 200;

/// The number of lines a line row's label may wrap to before it ellipsizes, and
/// the multiplier [`char_budget`] applies.
const SEARCH_LINE_ROWS: usize = 2;

/// Start indent (px) of a line row's label under its file row.
const SEARCH_LINE_INDENT: i32 = 24;

/// Spacing and outer margin (px) of the panel's box.
const SEARCH_PANEL_SPACING: i32 = 6;

/// Capacity of the search's wake channel. One slot is enough: it coalesces a burst
/// of streaming-batch wakes into a single queued refresh (a dropped "full" send
/// just means a refresh is already pending). Mirrors `PICKER_WAKE_CHANNEL_CAP`.
const SEARCH_WAKE_CHANNEL_CAP: usize = 1;

/// The glyph a line row carries on each side [`context_window`] reported it had
/// trimmed. Adding it is the front-end's job; see [`wolf_core::ContextWindow`] for
/// why.
const ELLIPSIS: &str = "…";

/// [`ELLIPSIS`]'s length in BYTES, the shift a leading one applies to every bold
/// range.
const ELLIPSIS_BYTES: u32 = ELLIPSIS.len() as u32;

/// The field's placeholder text.
const SEARCH_PLACEHOLDER: &str = "Search";

/// The libadwaita style class on the result list: GNOME's sidebar-list look, whose
/// selected row is a neutral tint rather than the accent fill.
///
/// Measured under `cage`: under the accent fill, tesseract reads no text from the
/// preselected line row in the sidebar band, so the search smokes cannot see a
/// result. Without the class the unselected row reads `zzmango` as `2zmango`.
const SEARCH_LIST_CSS: &str = "navigation-sidebar";

/// Icon of an expanded file row.
const FILE_EXPANDED_ICON: &str = "pan-down-symbolic";

/// Icon of a collapsed file row.
const FILE_COLLAPSED_ICON: &str = "pan-end-symbolic";

/// The status line before anything has been run, and after an empty query clears.
const SEARCH_IDLE_STATUS: &str = "Type a query and press Enter to search";

/// The status line for a finished search that matched nothing.
const SEARCH_NO_MATCHES_STATUS: &str = "No matches";

/// Prefix of the status line for a session carrying an error. This label is the
/// ONLY surface core's message (e.g. `"WOLF_RG_BIN unset"`) ever reaches.
const SEARCH_ERROR_PREFIX: &str = "Search failed: ";

/// Prefix of the status line while `rg` is still running.
const SEARCH_RUNNING_PREFIX: &str = "Searching… ";

/// The status line while a toggle differs from the one the live search ran with.
const SEARCH_STALE_STATUS: &str = "Options changed: press Enter";

/// The case toggle's label.
const CASE_TOGGLE_LABEL: &str = "Aa";

/// The case toggle's tooltip.
const CASE_TOGGLE_TOOLTIP: &str = "Match case";

/// The regex toggle's label.
const REGEX_TOGGLE_LABEL: &str = ".*";

/// The regex toggle's tooltip.
const REGEX_TOGGLE_TOOLTIP: &str = "Use regular expression";

/// The all-files toggle's label.
const ALL_TOGGLE_LABEL: &str = "All";

/// The all-files toggle's tooltip.
const ALL_TOGGLE_TOOLTIP: &str = "Search all files";

/// Whether the panel may be shown at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowDecision {
    /// Show the sidebar's Search page and focus its field. It starts NO search.
    Show,
    /// Touch no widget; the string names why, as a mid-sentence clause (the
    /// [`crate::command_action`] / [`crate::palette`] convention).
    Refuse(&'static str),
}

/// Whether the panel may be shown, given whether a project is open.
///
/// Without one the search root would fall back to the directory Neovim was started
/// in — the home directory — which is exactly the tree
/// [`crate::workspace::launch_decision`] refuses to hand the file search. The
/// palette row and `:WolfSearchInProject` meet this refusal in the empty state; the
/// chord and the menu item meet the disabled `win.search-in-project` action first.
///
/// Liveness is deliberately not an input: showing the panel issues nothing, and
/// [`activation_route`] gates the one step that reaches Neovim.
#[must_use]
pub fn show_decision(project_open: bool) -> ShowDecision {
    if project_open {
        ShowDecision::Show
    } else {
        ShowDecision::Refuse(
            "no project is open, so the search would be rooted at the directory Neovim was \
             started in — the home directory — which is the whole tree \
             `workspace::launch_decision` refuses to hand the file search",
        )
    }
}

/// The states of the panel's three toggles, which decide how the next search treats
/// its query and which files it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchToggles {
    /// Match case (rg `-s`) rather than ignore it (rg `-i`).
    pub case_sensitive: bool,
    /// Treat the query as a regular expression rather than a literal (rg `-F`).
    pub use_regex: bool,
    /// Search every file rather than only Markdown (core's `*.md` and `*.markdown`
    /// globs).
    pub all_files: bool,
}

impl SearchToggles {
    /// The states the panel opens with: regex on, case off and all files off.
    ///
    /// Regex on and all files off are the counterpart's defaults
    /// (`SearchToggleSeam.defaultUseRegex` and `SearchToggleSeam.defaultAllFiles`), so
    /// both platforms open with the same scope, Markdown only. The counterpart seeds
    /// case from nvim's `ignorecase`, which is a blocking RPC this crate never calls,
    /// so case starts off, the forgiving answer.
    pub const INITIAL: Self = Self {
        case_sensitive: false,
        use_regex: true,
        all_files: false,
    };
}

/// The panel's field text and toggles, as a tab records them and a restore applies
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchFields {
    /// The field's RAW text, untrimmed.
    pub query: String,
    /// The toggles' states.
    pub toggles: SearchToggles,
}

impl Default for SearchFields {
    /// An empty field and [`SearchToggles::INITIAL`], the panel as it opens.
    fn default() -> Self {
        Self {
            query: String::new(),
            toggles: SearchToggles::INITIAL,
        }
    }
}

/// The [`SearchOptions`] this front-end ships for `query` under `toggles`: every
/// flag is a toggle's.
///
/// - `use_regex` and `case_sensitive` are the toggles'. An invalid pattern such as
///   `foo(bar` makes rg exit 2, which [`search_status`] reports as an error; turning
///   the regex toggle off and pressing Enter then searches it literally.
/// - `all_files` is the `All` toggle's. Off, core restricts rg to `--glob '*.md'
///   --glob '*.markdown'`; on, every file is searched, as the file picker and the
///   tree sidebar list them. A Markdown-only start is honest because the toggle that
///   widens it sits beside the field: without it, a "Search in project" would skip
///   every other file without saying so.
///
/// The query is passed through VERBATIM — never trimmed, cased or escaped —
/// because [`enter_action`] compares the RAW field text against the query the live
/// session was started with.
#[must_use]
pub fn search_options_for(query: &str, toggles: SearchToggles) -> SearchOptions {
    SearchOptions {
        query: query.to_string(),
        use_regex: toggles.use_regex,
        case_sensitive: toggles.case_sensitive,
        all_files: toggles.all_files,
    }
}

/// One row of the panel's list: a matched FILE, or one matched LINE under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelRow {
    /// A matched file. Activating it toggles whether its lines are shown.
    File {
        /// Absolute path of the file, copied from the projection's group.
        abs_path: PathBuf,
        /// `SearchFileGroup::label`: `"<basename> - <parent>"`, or the basename for
        /// a root file.
        label: String,
        /// `false` when the user collapsed it, in which case no line row follows.
        expanded: bool,
    },
    /// One matched line. Activating it opens its location.
    Line {
        /// Absolute path of the matched file, copied from the [`SearchMatch`].
        abs_path: PathBuf,
        /// 1-based line number of the match.
        line_number: u64,
        /// 0-based BYTE column of the match, the convention `nvim_win_set_cursor`
        /// takes.
        column: u32,
        /// The windowed line text, with an ellipsis (`…`) on each trimmed side.
        text: String,
        /// Byte `[start, end)` ranges into `text` that are drawn bold.
        bold: Vec<(u32, u32)>,
    },
}

/// Assembles the panel's rows from the raw matches, in the CORE projection's order.
///
/// The order is [`wolf_core::project`]'s and not rg's: files sorted
/// case-insensitively by relative path, each file's matches ascending by line
/// number. Each file row is followed by its line rows unless its path is in
/// `collapsed`.
///
/// `max_lines` caps the line rows across all groups in projection order, and a
/// COLLAPSED file's lines count toward it, so collapsing a file never pulls a later
/// match in. A group with no line under the cap has no file row either.
///
/// A line row's `text` is [`context_window`]'s window over `budget` characters,
/// with an ellipsis (`…`) on each truncated side; `bold` is the window's ranges,
/// shifted past a leading ellipsis. The `(abs_path, line_number, column)` triple
/// is copied STRAIGHT from the [`SearchMatch`].
#[must_use]
pub fn panel_rows(
    matches: &[SearchMatch],
    budget: usize,
    max_lines: usize,
    collapsed: &HashSet<PathBuf>,
) -> Vec<PanelRow> {
    let mut rows = Vec::new();
    let mut remaining = max_lines;
    for group in project(matches) {
        if remaining == 0 {
            break;
        }
        let taken = group.match_indices.len().min(remaining);
        remaining -= taken;
        let expanded = !collapsed.contains(&group.abs_path);
        rows.push(PanelRow::File {
            abs_path: group.abs_path,
            label: group.label,
            expanded,
        });
        if !expanded {
            continue;
        }
        for index in group.match_indices.iter().take(taken) {
            if let Some(found) = matches.get(*index) {
                rows.push(line_row(found, budget));
            }
        }
    }
    rows
}

/// One match as a [`PanelRow::Line`], windowed to `budget` characters.
fn line_row(found: &SearchMatch, budget: usize) -> PanelRow {
    let window = context_window(&found.line_text, &found.match_ranges, budget);
    let (lead, shift) = if window.truncated_left {
        (ELLIPSIS, ELLIPSIS_BYTES)
    } else {
        ("", 0)
    };
    let trail = if window.truncated_right { ELLIPSIS } else { "" };
    PanelRow::Line {
        abs_path: found.abs_path.clone(),
        line_number: found.line_number,
        column: found.column,
        text: format!("{lead}{}{trail}", window.text),
        bold: window
            .ranges
            .iter()
            .map(|&(start, end)| (start + shift, end + shift))
            .collect(),
    }
}

/// The CHARACTER budget a line row of `width_px` holds over `lines` wrapped lines,
/// at `char_advance_px` per character: `max(1, floor(width / advance)) * lines`.
///
/// A non-positive or non-finite advance, or a non-finite or sub-one quotient,
/// counts as one character per line, so [`context_window`] always gets a usable
/// budget.
#[must_use]
pub fn char_budget(width_px: f64, char_advance_px: f64, lines: usize) -> usize {
    let per_line = if char_advance_px.is_finite() && char_advance_px > 0.0 {
        let columns = (width_px / char_advance_px).floor();
        if columns.is_finite() && columns >= 1.0 {
            // Saturating: a float-to-int `as` clamps, and the quotient is finite and at least 1 here.
            columns as usize
        } else {
            1
        }
    } else {
        1
    };
    per_line.saturating_mul(lines)
}

/// The index of the first [`PanelRow::Line`], or `None` when there is none.
#[must_use]
pub fn first_line_row(rows: &[PanelRow]) -> Option<usize> {
    rows.iter()
        .position(|row| matches!(row, PanelRow::Line { .. }))
}

/// Which row to select after a rebuild.
///
/// Without `reset`, the row `previous` names keeps the selection wherever it now
/// sits: a file row is matched on its path, a line row on its path, line and
/// column, and `expanded`, `text` and `bold` are ignored. Matching by identity
/// rather than by index matters because a newly streamed file can sort above the
/// selection. Otherwise — a reset, nothing selected, or a row that is gone — the
/// answer is [`first_line_row`].
///
/// `reset` is true only on the first rebuild of a new run, so a fresh search
/// preselects its first match.
#[must_use]
pub fn selection_after_rebuild(
    reset: bool,
    previous: Option<&PanelRow>,
    rows: &[PanelRow],
) -> Option<usize> {
    if !reset {
        if let Some(previous) = previous {
            if let Some(index) = rows.iter().position(|row| same_row(previous, row)) {
                return Some(index);
            }
        }
    }
    first_line_row(rows)
}

/// Whether `a` and `b` are the same row, by identity rather than by content.
fn same_row(a: &PanelRow, b: &PanelRow) -> bool {
    match (a, b) {
        (PanelRow::File { abs_path: a, .. }, PanelRow::File { abs_path: b, .. }) => a == b,
        (
            PanelRow::Line {
                abs_path: a_path,
                line_number: a_line,
                column: a_column,
                ..
            },
            PanelRow::Line {
                abs_path: b_path,
                line_number: b_line,
                column: b_column,
                ..
            },
        ) => a_path == b_path && a_line == b_line && a_column == b_column,
        (PanelRow::File { .. }, PanelRow::Line { .. })
        | (PanelRow::Line { .. }, PanelRow::File { .. }) => false,
    }
}

/// What one press of `Enter` in the field does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnterAction {
    /// Start a fresh [`SearchSession`] for the field's current text, killing any
    /// session already running.
    RunSearch,
    /// Drop the session, empty the rows and show the idle status.
    Clear,
    /// Activate the row at this index.
    Activate(usize),
    /// A true no-op: nothing is started and nothing is opened.
    Nothing,
}

/// The `Enter` decision. The rules, in order:
///
/// 1. A query whose `trim()` is empty CLEARS, and is never run: core builds
///    `rg -F -e ''`, which matches every line of every file in the search's scope.
/// 2. A query or toggles the live session was not started with runs a fresh
///    search; `ran` is that session's query and toggles, or `None`. The query
///    comparison is RAW, so a trailing space is a different search, exactly as it
///    is to rg.
/// 3. An in-range `selected` row is activated, a file row included (which toggles
///    it).
/// 4. Otherwise the first line row is activated, so "type a query, Enter, Enter"
///    opens the first match with no arrow key.
/// 5. With no line row at all, nothing happens.
#[must_use]
pub fn enter_action(
    query: &str,
    toggles: SearchToggles,
    ran: Option<(&str, SearchToggles)>,
    selected: Option<usize>,
    rows: &[PanelRow],
) -> EnterAction {
    if query.trim().is_empty() {
        return EnterAction::Clear;
    }
    if ran != Some((query, toggles)) {
        return EnterAction::RunSearch;
    }
    if let Some(index) = selected.filter(|index| *index < rows.len()) {
        return EnterAction::Activate(index);
    }
    match first_line_row(rows) {
        Some(index) => EnterAction::Activate(index),
        None => EnterAction::Nothing,
    }
}

/// What activating one row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowActivation {
    /// Flip whether this file's lines are shown.
    Toggle(PathBuf),
    /// Open this location: a 1-based line and a 0-based byte column.
    Open {
        /// Absolute path of the matched file.
        path: PathBuf,
        /// 1-based line number.
        line: u64,
        /// 0-based byte column.
        col: u64,
    },
}

/// What activating `row` does: a file row toggles, a line row opens its location.
///
/// The column is widened `u32` -> `u64` with [`u64::from`], which is lossless and
/// must not ADJUST the value: rg's column and `nvim_win_set_cursor`'s column are
/// both 0-based byte offsets. A `+ 1` here would land the cursor one byte past every
/// match.
#[must_use]
pub fn row_activation(row: &PanelRow) -> RowActivation {
    match row {
        PanelRow::File { abs_path, .. } => RowActivation::Toggle(abs_path.clone()),
        PanelRow::Line {
            abs_path,
            line_number,
            column,
            ..
        } => RowActivation::Open {
            path: abs_path.clone(),
            line: *line_number,
            col: u64::from(*column),
        },
    }
}

/// Where an opened line row goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenRoute {
    /// `WolfCore::open_at` in the active pane.
    OpenAt,
    /// Repoint the preview in Neovim window `win` through the blocking
    /// `WolfCore::preview_file`.
    PreviewFile {
        /// The preview pane's Neovim window handle.
        win: i64,
    },
    /// Open nothing; the string names why, as a mid-sentence clause.
    Refuse(&'static str),
}

/// Routes an opened line row, given the core's liveness reading (or `None` when
/// there is no core at all) and the active window's handle when it is a PREVIEW
/// pane.
///
/// TOTAL by construction: a `match` over `Option<Liveness>` with **no `_` arm**, so
/// a newly appended [`Liveness`] variant fails to compile here.
///
/// **Liveness comes first.** The rows sit outside the opaque crash overlay, so this
/// gate stands in for the cover the overlay gives the editor, and it is what keeps
/// the blocking `preview_file` on the `Alive` arm alone.
#[must_use]
pub fn activation_route(liveness: Option<Liveness>, preview_win: Option<i64>) -> OpenRoute {
    match liveness {
        Some(Liveness::Alive) => match preview_win {
            Some(win) => OpenRoute::PreviewFile { win },
            None => OpenRoute::OpenAt,
        },
        Some(Liveness::NotResponding) => OpenRoute::Refuse(
            "the captive Neovim is not responding, so an open would queue behind the stall and \
             a preview repoint would block the window for another whole timeout",
        ),
        Some(Liveness::Dead) => OpenRoute::Refuse(
            "the captive Neovim has died, and an open into a dead Neovim performs nothing",
        ),
        None => OpenRoute::Refuse(
            "the core has already been taken out by the window's close handler, so there is \
             nothing left to open the result in",
        ),
    }
}

/// The one-line status drawn under the field.
///
/// `toggles` is the panel's current toggles, and `ran` the toggles the live search
/// ran with, or `None` when nothing has run or the panel was cleared.
///
/// Arms in precedence order: nothing run yet; the toggles changed since the run;
/// the session carries an error; `rg` is still running; finished with nothing
/// found; finished but capped; finished and complete.
///
/// **The error arm precedes the completeness arms deliberately**: a session that
/// failed before it started is ALSO `complete`, and reporting it as "no matches"
/// would send a user looking for a better query when what they need is a
/// provisioned ripgrep. The `ran` guard comes first for the mirror-image reason: a
/// stale session's counts and error must not be reported over a cleared panel.
///
/// The stale arm sits above the error and count arms because they describe a search
/// the toggles no longer match; turning regex off is how a user recovers from rg's
/// error, and its message must not outlive that.
#[must_use]
pub fn search_status(
    toggles: SearchToggles,
    ran: Option<SearchToggles>,
    total_matches: usize,
    shown_rows: usize,
    complete: bool,
    error: Option<&str>,
) -> String {
    let Some(ran) = ran else {
        return SEARCH_IDLE_STATUS.to_string();
    };
    if ran != toggles {
        return SEARCH_STALE_STATUS.to_string();
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

/// What [`search_status`] needs of a session, read under one guard.
struct SessionSummary {
    /// Every match so far.
    total: usize,
    /// Whether `rg` has finished.
    complete: bool,
    /// The session's error, if any.
    error: Option<String>,
}

impl SessionSummary {
    /// Reads the summary out of a session's read guard.
    fn of(view: &SearchReadGuard) -> Self {
        Self {
            total: view.matches.len(),
            complete: view.complete,
            error: view.error.clone(),
        }
    }
}

/// The panel's shared, interior-mutable state. All access happens on the single
/// GTK main thread, so `Cell`/`RefCell` (not a `Mutex`) is sufficient.
struct PanelInner {
    /// The live content-search session, or `None` when nothing has been run or the
    /// panel was cleared. **The DROP is the cancellation**: it kills the `rg` child.
    search: RefCell<Option<SearchSession>>,
    /// The query and toggles [`PanelInner::search`] was started with, or `None`.
    /// [`enter_action`] compares the field's raw text and the current toggles against
    /// them, and [`search_status`] the toggles.
    ran: RefCell<Option<(String, SearchToggles)>>,
    /// Set by a run and consumed by the next rebuild, so a new search preselects
    /// its first line row.
    reset_selection: Cell<bool>,
    /// The visibility + version gate consulted by every streamed refresh.
    gate: RefCell<TreeRefreshGate>,
    /// The character budget the rows were last built with.
    budget: Cell<usize>,
    /// The files the user collapsed, by path; they survive rebuilds and runs.
    collapsed: RefCell<HashSet<PathBuf>>,
    /// The rows currently materialized into the `ListBox`, parallel to it.
    rows: RefCell<Vec<PanelRow>>,
    /// The panel's root box, which is the sidebar's Search page.
    panel: gtk::Box,
    /// The search field. It has NO `connect_search_changed`: typing runs nothing.
    entry: gtk::SearchEntry,
    /// The match-case toggle, the source of [`SearchToggles::case_sensitive`].
    case_toggle: gtk::ToggleButton,
    /// The regular-expression toggle, the source of [`SearchToggles::use_regex`].
    regex_toggle: gtk::ToggleButton,
    /// The all-files toggle, the source of [`SearchToggles::all_files`].
    all_toggle: gtk::ToggleButton,
    /// The one-line status under the toggles, drawn from [`search_status`].
    status: gtk::Label,
    /// The scrolled window over [`PanelInner::list`].
    scroller: gtk::ScrolledWindow,
    /// The results list, rebuilt on each refresh. Never focusable.
    list: gtk::ListBox,
    /// Where Escape and an open send focus: the SELECTED tab's cmdline strip,
    /// resolved at that time. See [`crate::picker::FocusTarget`].
    area: FocusTarget,
    /// Resolves the SELECTED tab's core, which a run starts on and a result opens
    /// in.
    core: CoreSource,
    /// The core [`PanelInner::search`] was started on. A session stays on it across
    /// a tab switch, since a rerun would only repeat it, and
    /// [`SearchPanelHandle::release_core`] drops it when that tab closes.
    session_core: RefCell<Weak<RefCell<Option<WolfCore>>>>,
    /// The wake closure handed to `start_search`; firing it queues a refresh.
    wake: WakeFn,
}

impl PanelInner {
    /// The budget for the list's current width: [`char_budget`] over the width less
    /// the line indent and the advance of `"0"` in the list's font.
    /// [`TREE_PANE_WIDTH`] stands in while the list is unallocated.
    fn current_budget(&self) -> usize {
        let allocated = self.list.width();
        let width = if allocated > 0 {
            allocated
        } else {
            TREE_PANE_WIDTH
        };
        let advance = self.list.create_pango_layout(Some("0")).pixel_size().0;
        char_budget(
            f64::from(width - SEARCH_LINE_INDENT),
            f64::from(advance),
            SEARCH_LINE_ROWS,
        )
    }

    /// Rebuilds from the current session when the gate allows it: refused while
    /// hidden (and owed back), and otherwise only when the session's version
    /// advanced or the budget changed.
    fn refresh(&self) {
        // Own statement: `set_sidebar_visible` consults the same gate.
        let may_refresh = self.gate.borrow_mut().request_refresh();
        if !may_refresh {
            return;
        }
        let budget = self.current_budget();
        let rebuild = {
            let search = self.search.borrow();
            let Some(session) = search.as_ref() else {
                return;
            };
            let version = session.read().version;
            let advanced = self.gate.borrow_mut().should_rebuild(version);
            advanced || budget != self.budget.get()
        };
        if rebuild {
            self.rebuild(budget);
        }
    }

    /// Rebuilds the rows and the status line from the CURRENT session, and restores
    /// the selection through [`selection_after_rebuild`].
    ///
    /// The session's read guard is scoped to the projection and the three scalar
    /// reads, so no widget work happens under it ([`crate::picker`]'s guard-scope
    /// discipline).
    fn rebuild(&self, budget: usize) {
        self.budget.set(budget);
        let (rows, summary) = {
            let search = self.search.borrow();
            match search.as_ref() {
                Some(session) => {
                    let view = session.read();
                    let rows = panel_rows(
                        &view.matches,
                        budget,
                        SEARCH_MAX_ROWS,
                        &self.collapsed.borrow(),
                    );
                    (rows, Some(SessionSummary::of(&view)))
                }
                None => (Vec::new(), None),
            }
        };
        let status = self.status_text(summary.as_ref());

        // Captured before the rows are replaced, so the selection follows its row.
        let previous = self.selected_row();
        let reset = self.reset_selection.replace(false);

        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for row in &rows {
            self.list.append(&row_widget(row));
        }
        let selection = selection_after_rebuild(reset, previous.as_ref(), &rows);
        *self.rows.borrow_mut() = rows;
        self.status.set_text(&status);
        if let Some(index) = selection {
            self.select_index(index, false);
        }
    }

    /// The toggles' current states.
    fn toggles(&self) -> SearchToggles {
        SearchToggles {
            case_sensitive: self.case_toggle.is_active(),
            use_regex: self.regex_toggle.is_active(),
            all_files: self.all_toggle.is_active(),
        }
    }

    /// The status line for the session `summary` describes (`None` when there is no
    /// session), through [`search_status`].
    fn status_text(&self, summary: Option<&SessionSummary>) -> String {
        let toggles = self.toggles();
        let Some(summary) = summary else {
            return search_status(toggles, None, 0, 0, false, None);
        };
        let ran = self.ran.borrow().as_ref().map(|(_, ran)| *ran);
        search_status(
            toggles,
            ran,
            summary.total,
            summary.total.min(SEARCH_MAX_ROWS),
            summary.complete,
            summary.error.as_deref(),
        )
    }

    /// Redraws the status line alone, leaving the rows and the selection as they are.
    fn update_status(&self) {
        let summary = {
            let search = self.search.borrow();
            search
                .as_ref()
                .map(|session| SessionSummary::of(&session.read()))
        };
        self.status.set_text(&self.status_text(summary.as_ref()));
    }

    /// The row under the list's selection, cloned out of [`PanelInner::rows`].
    fn selected_row(&self) -> Option<PanelRow> {
        let index = usize::try_from(self.list.selected_row()?.index()).ok()?;
        self.rows.borrow().get(index).cloned()
    }

    /// Selects the row at `index`, scrolling it into view when `scroll` is set.
    fn select_index(&self, index: usize, scroll: bool) {
        let Some(row) = self
            .list
            .row_at_index(i32::try_from(index).unwrap_or(i32::MAX))
        else {
            return;
        };
        self.list.select_row(Some(&row));
        if scroll {
            self.scroll_to(&row);
        }
    }

    /// Scrolls the list just far enough to show `row`. The list never holds focus,
    /// so GTK's own focus scrolling never runs for it.
    fn scroll_to(&self, row: &gtk::ListBoxRow) {
        let Some(bounds) = row.compute_bounds(&self.list) else {
            return;
        };
        let adjustment = self.scroller.vadjustment();
        let top = f64::from(bounds.y());
        let bottom = top + f64::from(bounds.height());
        if top < adjustment.value() {
            adjustment.set_value(top);
        } else if bottom > adjustment.value() + adjustment.page_size() {
            adjustment.set_value(bottom - adjustment.page_size());
        }
    }

    /// Moves the LIST selection by `delta` rows (clamped, no wrap), leaving focus in
    /// the field. Bound to Up (`-1`) / Down (`+1`).
    fn move_selection(&self, delta: i32) {
        let n_rows = i32::try_from(self.rows.borrow().len()).unwrap_or(i32::MAX);
        let current = self.list.selected_row().map_or(0, |row| row.index());
        if let Some(next) = next_selection(current, delta, n_rows) {
            self.select_index(next, true);
        }
    }

    /// Routes one `Enter` through [`enter_action`].
    fn on_enter(&self) {
        let query = self.entry.text().to_string();
        let toggles = self.toggles();
        let ran = self.ran.borrow().clone();
        let selected = self
            .list
            .selected_row()
            .and_then(|row| usize::try_from(row.index()).ok());
        let action = {
            let rows = self.rows.borrow();
            let ran = ran.as_ref().map(|(query, ran)| (query.as_str(), *ran));
            enter_action(&query, toggles, ran, selected, &rows)
        };
        match action {
            EnterAction::RunSearch => self.run_search(query, toggles),
            EnterAction::Clear => self.clear(),
            EnterAction::Activate(index) => self.activate(index),
            EnterAction::Nothing => {}
        }
    }

    /// Starts a fresh [`SearchSession`] for `query` under `toggles`, killing the
    /// previous one first.
    ///
    /// **The statement order is load-bearing.** The old session is taken in a
    /// statement of its own (so no `RefCell` borrow is live afterwards) and dropped
    /// BEFORE `start_search` is called, because the drop is what kills the old `rg`
    /// child — otherwise two ripgreps would run over the same tree at once.
    fn run_search(&self, query: String, toggles: SearchToggles) {
        let previous = self.search.borrow_mut().take();
        drop(previous);

        let Some(cell) = (self.core)() else {
            return;
        };
        let session = {
            let core_ref = cell.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            core.start_search(search_options_for(&query, toggles), self.wake.clone())
        };
        *self.search.borrow_mut() = Some(session);
        *self.session_core.borrow_mut() = Rc::downgrade(&cell);
        *self.ran.borrow_mut() = Some((query, toggles));
        self.reset_selection.set(true);
        self.gate.borrow_mut().reset_version_tracking();
        // An errored run's first refresh comes from here: `start_search` fires `wake()` before returning
        // when `WOLF_RG_BIN` is unset, so the wake may precede storing the session.
        self.refresh();
    }

    /// Drops the session, empties the rows and shows the idle status.
    fn clear(&self) {
        let previous = self.search.borrow_mut().take();
        drop(previous);
        *self.ran.borrow_mut() = None;
        let budget = self.current_budget();
        self.rebuild(budget);
    }

    /// Activates the row at `index`: a file row toggles, a line row opens.
    fn activate(&self, index: usize) {
        let row = self.rows.borrow().get(index).cloned();
        let Some(row) = row else {
            return;
        };
        match row_activation(&row) {
            RowActivation::Toggle(path) => {
                {
                    let mut collapsed = self.collapsed.borrow_mut();
                    if !collapsed.remove(&path) {
                        collapsed.insert(path);
                    }
                }
                // Directly, bypassing the version half: nothing in the session changed.
                let budget = self.current_budget();
                self.rebuild(budget);
            }
            RowActivation::Open { path, line, col } => self.open(path, line, col),
        }
    }

    /// Opens a location through [`activation_route`], then returns focus to the
    /// editor. A refusal prints its reason and moves nothing.
    ///
    /// The snapshot guard is bound to a local inside one scope, dropped before any
    /// core call: `preview_file` and `open_at` take core locks of their own.
    fn open(&self, path: PathBuf, line: u64, col: u64) {
        let (liveness, preview_win) = with_core(&self.core, |core| {
            let liveness = core.liveness();
            let snapshot = core.read_snapshot();
            let preview_win = if snapshot.is_preview_active() {
                snapshot.active_win
            } else {
                None
            };
            (Some(liveness), preview_win)
        })
        .unwrap_or((None, None));
        match activation_route(liveness, preview_win) {
            OpenRoute::Refuse(reason) => {
                eprintln!("wolf-gnome: search result not opened: {reason}");
                return;
            }
            OpenRoute::OpenAt => self.open_at(path, line, col),
            OpenRoute::PreviewFile { win } => {
                let previewed = match path.to_str() {
                    Some(path) => {
                        with_core(&self.core, |core| core.preview_file(win, path)).unwrap_or(0)
                    }
                    None => 0,
                };
                // Core declined (not Markdown, unreadable, or a failed RPC), or the path is not UTF-8:
                // the location still opens, in the preview's pane.
                if previewed == 0 {
                    self.open_at(path, line, col);
                }
            }
        }
        if let Some(area) = (self.area)() {
            area.grab_focus();
        }
    }

    /// Sends `open_at`, fire-and-forget on the ordered input channel.
    fn open_at(&self, path: PathBuf, line: u64, col: u64) {
        with_core(&self.core, |core| core.open_at(path, line, col));
    }

    /// Records whether the panel is on screen, refreshing once if a refresh was
    /// refused while it was not.
    fn set_sidebar_visible(&self, visible: bool) {
        // Own statement: `refresh` consults the same gate.
        let owed = self.gate.borrow_mut().set_visible(visible);
        if owed {
            self.refresh();
        }
    }
}

/// The widget for one row: a file row is a disclosure icon beside its label, a line
/// row an indented label with its matches in bold.
fn row_widget(row: &PanelRow) -> gtk::ListBoxRow {
    let child: gtk::Widget = match row {
        PanelRow::File {
            label, expanded, ..
        } => {
            let icon = if *expanded {
                FILE_EXPANDED_ICON
            } else {
                FILE_COLLAPSED_ICON
            };
            let file = gtk::Box::new(gtk::Orientation::Horizontal, SEARCH_PANEL_SPACING);
            file.append(&gtk::Image::from_icon_name(icon));
            file.append(
                &gtk::Label::builder()
                    .label(label)
                    .xalign(0.0)
                    .hexpand(true)
                    .ellipsize(pango::EllipsizeMode::Middle)
                    .build(),
            );
            file.upcast()
        }
        PanelRow::Line { text, bold, .. } => {
            let attrs = pango::AttrList::new();
            for &(start, end) in bold {
                let mut weight = pango::AttrInt::new_weight(pango::Weight::Bold);
                weight.set_start_index(start);
                weight.set_end_index(end);
                attrs.insert(weight);
            }
            gtk::Label::builder()
                .label(text)
                .attributes(&attrs)
                .xalign(0.0)
                .margin_start(SEARCH_LINE_INDENT)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .lines(i32::try_from(SEARCH_LINE_ROWS).unwrap_or(1))
                .ellipsize(pango::EllipsizeMode::End)
                .build()
                .upcast()
        }
    };
    let list_row = gtk::ListBoxRow::new();
    list_row.set_child(Some(&child));
    list_row
}

/// One of the panel's toggles, in the default button style. It never takes focus on
/// click, so the next Enter still reaches the field (see the module docs).
fn search_toggle(label: &str, tooltip: &str, active: bool) -> gtk::ToggleButton {
    gtk::ToggleButton::builder()
        .label(label)
        .tooltip_text(tooltip)
        .active(active)
        .focus_on_click(false)
        .build()
}

/// A handle to the built search panel, used by [`crate::sidebar`] to place and
/// relay it, by the shell to focus it, and by the window to gate key forwarding and
/// to cancel `rg` on close.
#[derive(Clone)]
pub struct SearchPanelHandle {
    inner: Rc<PanelInner>,
}

impl SearchPanelHandle {
    /// The panel's root widget, the sidebar's Search page.
    #[must_use]
    pub fn widget(&self) -> gtk::Widget {
        self.inner.panel.clone().upcast()
    }

    /// Moves keyboard focus into the field, synchronously. Its callers close the
    /// palette before dispatching (`palette::activate_selection`), so no later
    /// focus return can take it back.
    pub fn focus_entry(&self) {
        self.inner.entry.grab_focus();
    }

    /// Whether keyboard focus is inside the panel. `window::wire_keys` forwards
    /// nothing to Neovim, and offers nothing to the input method, while it is.
    #[must_use]
    pub fn owns_keyboard(&self) -> bool {
        let Some(root) = self.inner.panel.root() else {
            return false;
        };
        let Some(focus) = root.focus() else {
            return false;
        };
        focus.is_ancestor(&self.inner.panel)
            || &focus == self.inner.panel.upcast_ref::<gtk::Widget>()
    }

    /// Relays whether the panel is on screen into its refresh gate, catching up
    /// once if a refresh was refused while it was not.
    pub fn set_sidebar_visible(&self, visible: bool) {
        self.inner.set_sidebar_visible(visible);
    }

    /// The field's raw text and the toggles' states.
    #[must_use]
    pub fn fields(&self) -> SearchFields {
        SearchFields {
            query: self.inner.entry.text().to_string(),
            toggles: self.inner.toggles(),
        }
    }

    /// Sets the field's text and the toggles to `fields`, and runs the search on the
    /// selected tab's core when `run` is set, as Enter would. It moves no focus.
    pub fn restore(&self, fields: &SearchFields, run: bool) {
        let inner = &self.inner;
        inner.entry.set_text(&fields.query);
        inner.case_toggle.set_active(fields.toggles.case_sensitive);
        inner.regex_toggle.set_active(fields.toggles.use_regex);
        inner.all_toggle.set_active(fields.toggles.all_files);
        if run {
            inner.run_search(fields.query.clone(), fields.toggles);
        }
    }

    /// Drops the session, CANCELLING any in-flight `rg`. The rows stay drawn.
    pub fn close(&self) {
        let session = self.inner.search.borrow_mut().take();
        drop(session);
    }

    /// Drops the session when it runs on `core`, a closing tab's, CANCELLING any
    /// in-flight `rg` before that core shuts down. The rows stay drawn, as after
    /// [`SearchPanelHandle::close`], and a result still opens in the selected tab.
    pub fn release_core(&self, core: &CoreCell) {
        let ours = self
            .inner
            .session_core
            .borrow()
            .upgrade()
            .is_some_and(|session_core| Rc::ptr_eq(&session_core, core));
        if ours {
            self.close();
        }
    }
}

/// Builds the panel's widgets, wires their signals and returns a
/// [`SearchPanelHandle`]. The panel starts idle, with its refresh gate shut until
/// the sidebar relays that it is shown.
///
/// `area` resolves the focus-return widget when it is used — the SELECTED tab's
/// cmdline strip (`notes/wolf-gnome/window-structure.md`). See
/// [`crate::picker::FocusTarget`] for why it is a closure.
#[must_use]
pub fn build(core: CoreSource, area: FocusTarget) -> SearchPanelHandle {
    let entry = gtk::SearchEntry::builder()
        .placeholder_text(SEARCH_PLACEHOLDER)
        .hexpand(true)
        .build();

    let case_toggle = search_toggle(
        CASE_TOGGLE_LABEL,
        CASE_TOGGLE_TOOLTIP,
        SearchToggles::INITIAL.case_sensitive,
    );
    let regex_toggle = search_toggle(
        REGEX_TOGGLE_LABEL,
        REGEX_TOGGLE_TOOLTIP,
        SearchToggles::INITIAL.use_regex,
    );
    let all_toggle = search_toggle(
        ALL_TOGGLE_LABEL,
        ALL_TOGGLE_TOOLTIP,
        SearchToggles::INITIAL.all_files,
    );
    let toggle_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(SEARCH_PANEL_SPACING)
        .halign(gtk::Align::Start)
        .build();
    toggle_row.append(&case_toggle);
    toggle_row.append(&regex_toggle);
    toggle_row.append(&all_toggle);

    let status = gtk::Label::builder()
        .label(SEARCH_IDLE_STATUS)
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .ellipsize(pango::EllipsizeMode::End)
        .build();
    status.add_css_class("dim-label");

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_can_focus(false);
    list.add_css_class(SEARCH_LIST_CSS);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();

    let panel = gtk::Box::new(gtk::Orientation::Vertical, SEARCH_PANEL_SPACING);
    panel.set_margin_top(SEARCH_PANEL_SPACING);
    panel.set_margin_start(SEARCH_PANEL_SPACING);
    panel.set_margin_end(SEARCH_PANEL_SPACING);
    panel.append(&entry);
    panel.append(&toggle_row);
    panel.append(&status);
    panel.append(&scroller);

    // Batches fire this from a tokio worker and it only pushes; a full or closed channel means a
    // refresh is pending or the panel is gone, so `try_send` errors are ignored.
    let (wake_tx, wake_rx) = async_channel::bounded::<()>(SEARCH_WAKE_CHANNEL_CAP);
    let wake: WakeFn = Arc::new(move || {
        let _ = wake_tx.try_send(());
    });

    let inner = Rc::new(PanelInner {
        search: RefCell::new(None),
        ran: RefCell::new(None),
        reset_selection: Cell::new(false),
        // Seeded hidden: the sidebar relays the real visibility before the window is presented.
        gate: RefCell::new(TreeRefreshGate::new(false)),
        budget: Cell::new(0),
        collapsed: RefCell::new(HashSet::new()),
        rows: RefCell::new(Vec::new()),
        panel,
        entry: entry.clone(),
        case_toggle: case_toggle.clone(),
        regex_toggle: regex_toggle.clone(),
        all_toggle: all_toggle.clone(),
        status,
        scroller,
        list: list.clone(),
        area,
        core,
        session_core: RefCell::new(Weak::new()),
        wake,
    });

    // Drained on the main thread. The task holds only a `Weak`, so dropping the handle ends it
    // instead of leaking a cycle (inner owns the wake's sender).
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

    // Enter runs, clears or activates. There is deliberately NO `connect_search_changed`: typing
    // must never spawn rg.
    {
        let inner_weak = Rc::downgrade(&inner);
        entry.connect_activate(move |_entry| {
            if let Some(inner) = inner_weak.upgrade() {
                inner.on_enter();
            }
        });
    }

    // A toggle runs nothing: it only redraws the status line, which turns stale until Enter re-runs.
    for toggle in [&case_toggle, &regex_toggle, &all_toggle] {
        let inner_weak = Rc::downgrade(&inner);
        toggle.connect_toggled(move |_toggle| {
            if let Some(inner) = inner_weak.upgrade() {
                inner.update_status();
            }
        });
    }

    // A click selects the row and activates it: a file row toggles, a line row opens.
    {
        let inner_weak = Rc::downgrade(&inner);
        list.connect_row_activated(move |_list, row| {
            if let Some(inner) = inner_weak.upgrade() {
                if let Ok(index) = usize::try_from(row.index()) {
                    inner.activate(index);
                }
            }
        });
    }

    // Capture phase so the SearchEntry does not swallow these. Escape hands focus to the editor and
    // leaves the panel as it is; Up/Down move the list selection, not the caret.
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
                    if let Some(area) = (inner.area)() {
                        area.grab_focus();
                    }
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
        inner.panel.add_controller(key);
    }

    SearchPanelHandle { inner }
}
