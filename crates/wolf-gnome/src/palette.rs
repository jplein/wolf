//! The GNOME COMMAND-PALETTE overlay for `wolf-gnome`.
//!
//! Trigger: the **`win.command-palette`** action — bound to **Ctrl+Shift+P** and
//! listed in the primary menu as "Command Palette…". Like the file picker, the
//! chord is resolved from [`crate::actions::ACTION_TABLE`] by a CAPTURE-phase
//! controller ([`crate::shell`]), so it is guaranteed not to also be forwarded to
//! Neovim (see `crates/wolf-gnome/DECISIONS.md`, "The accelerator namespace").
//!
//! The overlay is a search entry over a ranked list of the registry commands this
//! front-end can actually perform. `Esc` closes it, Up/Down move the selection, and
//! `Enter` (or a row click) records the row's opaque MRU key, closes the overlay,
//! and hands the command to [`crate::shell`], which runs it through the same
//! `Command` → [`UiAction`] funnel a command drained from Neovim takes.
//!
//! **The candidate set is fixed at open, so this module has no wake bridge at
//! all.** [`wolf_core::rank_palette`] assembles its rows from
//! [`wolf_core::COMMAND_SPECS`] plus a command-MRU snapshot; neither can change
//! while the palette is up, because only an activation writes the MRU and an
//! activation closes the palette. Re-filtering per keystroke is therefore a
//! synchronous, IO-free call, and the picker's `async-channel` + wake-drain
//! plumbing would be dead weight here. Do not add it back for symmetry.
//!
//! Only [`PaletteRowKind::Static`] rows are ever produced: `rank_palette` is fed an
//! empty window slice and an empty recent-directories slice, because `wolf-gnome`
//! is single-window and the recent-directories MRU has no activation path here.
//! [`palette_action_for`] classifies the other two kinds as
//! [`PaletteAction::Unsupported`] anyway, so a row this front-end cannot perform
//! fails the filter rather than appearing as a dead entry.
//!
//! Almost all of this module is GTK widget wiring, which cannot be unit-tested
//! headlessly. Its pure, display-free seams are [`palette_rows`] and
//! [`palette_action_for`], unit-tested below; selection movement and activation
//! reuse [`crate::picker`]'s [`next_selection`], [`restored_index`] and
//! [`activation_index`], so every overlay in this crate navigates identically.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use wolf_core::{rank_palette, Command, PaletteRow, PaletteRowKind, WolfCore};

use crate::command_action::{ui_action_for, UiAction};
use crate::picker::{activation_index, next_selection, restored_index};

/// The maximum number of ranked rows shown in the palette list. Applied AFTER the
/// classification filter — see [`palette_rows`].
const PALETTE_MAX_ROWS: usize = 200;

/// Top margin (px) of the floating palette box within the overlay.
const PALETTE_BOX_MARGIN: i32 = 24;

/// Width (px) of the floating palette box.
const PALETTE_BOX_WIDTH: i32 = 600;

/// Maximum height (px) of the palette's scrolled result list. The list propagates
/// its natural height below this, so a one-row result is not a tall empty panel.
const PALETTE_LIST_MAX_HEIGHT: i32 = 400;

/// The activation handler [`PaletteHandle::connect_activate`] stores.
///
/// A named alias rather than an inline `RefCell<Option<Rc<dyn Fn(Command)>>>`: the
/// inline spelling trips `clippy::type_complexity`, and the `Rc` is what lets
/// [`PaletteInner::activate_selection`] clone the handler OUT of the `RefCell`
/// before calling it.
type ActivateHandler = Rc<dyn Fn(Command)>;

/// What activating one palette row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    /// Dispatch this registry command through the shell's [`UiAction`] funnel.
    Dispatch(Command),
    /// `wolf-gnome` cannot perform this row; the string names why, as a
    /// mid-sentence clause (matching [`crate::command_action`]'s convention).
    Unsupported(&'static str),
}

/// Classifies one palette row into the action this front-end performs for it.
///
/// TOTAL by construction: the implementation is a `match` over
/// [`PaletteRowKind`] with **no `_` arm**, so a newly appended core row kind fails
/// to compile here rather than showing up in the list as a row that does nothing.
///
/// A `Static` row is [`PaletteAction::Dispatch`] exactly when
/// [`ui_action_for`] gives its command a real [`UiAction`]. That is the whole
/// filter: it is what keeps the palette from becoming the one surface in this crate
/// that lists a no-op, the same rule `actions::MENU_SECTIONS` and
/// `shell::AppShell::install_actions` already hold to.
#[must_use]
pub fn palette_action_for(kind: PaletteRowKind, command: Option<Command>) -> PaletteAction {
    match kind {
        PaletteRowKind::Static => match command {
            Some(command) => {
                if matches!(
                    ui_action_for(command),
                    UiAction::Deferred(_) | UiAction::NoGnomeEquivalent(_)
                ) {
                    PaletteAction::Unsupported(
                        "this command is not dispatched by the GTK front-end; ui_action_for \
                         classifies it as deferred or as having no GNOME equivalent",
                    )
                } else {
                    PaletteAction::Dispatch(command)
                }
            }
            None => PaletteAction::Unsupported(
                "a static palette row carrying no registry command has nothing to dispatch",
            ),
        },
        PaletteRowKind::SwitchWindow => PaletteAction::Unsupported(
            "wolf-gnome is a single-window application, so there is no other window to raise",
        ),
        PaletteRowKind::OpenRecent => PaletteAction::Unsupported(
            "the recent-directories MRU is not fed to this palette, so no folder row can be \
             opened from it",
        ),
    }
}

/// The palette's candidate rows for `query`, ranked best-first and filtered to the
/// rows this front-end can actually perform, capped at `max`.
///
/// The two empty slices handed to [`rank_palette`] are where the single-window and
/// no-recent-directories decisions live: with them, only [`PaletteRowKind::Static`]
/// rows exist. `rank_palette` excludes [`Command::CommandPalette`] itself, so the
/// palette cannot dispatch itself.
///
/// **Ranking is unbounded and `max` is applied AFTER the filter**, deliberately.
/// [`rank_palette`] truncates to its own `max` before this crate's classification
/// filter can run, so passing `max` through would let deferred rows eat the budget
/// and hand the user a short — or empty — list.
#[must_use]
pub fn palette_rows(command_mru_recent: &[String], query: &str, max: usize) -> Vec<PaletteRow> {
    rank_palette(&[], &[], command_mru_recent, query, usize::MAX)
        .into_iter()
        .filter(|row| {
            matches!(
                palette_action_for(row.kind, row.command),
                PaletteAction::Dispatch(_)
            )
        })
        .take(max)
        .collect()
}

/// The palette's shared, interior-mutable state. All access happens on the single
/// GTK main thread, so `Cell`/`RefCell` (not a `Mutex`) is sufficient.
struct PaletteInner {
    /// Whether the palette is currently open (its widgets visible and focused).
    open: Cell<bool>,
    /// The rows currently materialized into the `ListBox`, parallel to it.
    rows: RefCell<Vec<PaletteRow>>,
    /// The command-MRU keys, snapshotted ONCE at [`PaletteInner::open`]. Only an
    /// activation writes that store and an activation closes the palette, so the
    /// value cannot change while the palette is up; re-reading it per keystroke
    /// would put a SQLite query on the typing path for nothing.
    mru_recent: RefCell<Vec<String>>,
    /// The overlay child holding the search entry + results list.
    palette_box: gtk::Box,
    /// The search entry whose text filters/ranks the rows.
    entry: gtk::SearchEntry,
    /// The results list, rebuilt on each refresh.
    list: gtk::ListBox,
    /// The editor drawing area, re-focused when the palette closes.
    area: gtk::DrawingArea,
    /// The live core, shared with the rest of the app. Read for the command MRU.
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The query string used by the previous [`PaletteInner::refresh`]. It decides
    /// whether the next refresh resets the selection to the top (query changed, or
    /// first refresh after open) or preserves it. `None` forces a reset and is set
    /// on `open`.
    last_query: RefCell<Option<String>>,
    /// The shell's activation handler, installed by
    /// [`PaletteHandle::connect_activate`].
    on_activate: RefCell<Option<ActivateHandler>>,
}

impl PaletteInner {
    /// Opens the palette: snapshots the command MRU, shows + focuses the entry, and
    /// does an initial refresh so the MRU-first list appears immediately. No-op if
    /// already open.
    ///
    /// Unlike [`crate::picker`]'s `open` this does NOT early-return when the core is
    /// gone: the candidate list is a function of the registry, not of the core, and
    /// only the MRU ordering is lost.
    fn open(&self) {
        if self.open.get() {
            return;
        }
        // Short core borrow in its own statement, dropped before the SQLite read.
        let mru = {
            let core_ref = self.core.borrow();
            core_ref.as_ref().map(WolfCore::command_mru)
        };
        let recent = mru.map(|mru| mru.recent()).unwrap_or_default();
        *self.mru_recent.borrow_mut() = recent;

        self.open.set(true);
        self.palette_box.set_visible(true);
        self.entry.set_text("");
        self.entry.grab_focus();
        // Force the first refresh to select the top row, regardless of any stale
        // selection/query left over from a previous session.
        *self.last_query.borrow_mut() = None;
        self.refresh();
    }

    /// Closes the palette: hides the widgets and returns focus to the editor. No-op
    /// if already closed.
    ///
    /// The `grab_focus` parks focus back on the persistent cmdline strip, exactly as
    /// [`crate::picker`]'s `close` does, so every overlay in this crate leaves focus
    /// in the same place. It is a convention, not a load-bearing guard: deleting it
    /// leaves the command-palette smoke green, because GTK4 routes a key event with
    /// no focus widget to the window itself, where the bubble-phase controller
    /// `app::wire_keys` installs still runs.
    fn close(&self) {
        if !self.open.get() {
            return;
        }
        self.open.set(false);
        self.palette_box.set_visible(false);
        self.area.grab_focus();
    }

    /// Rebuilds the results list for the current query from the MRU snapshot taken
    /// at open. No-op when closed.
    ///
    /// Selection handling matches the picker's: a changed query (or the first
    /// refresh after `open`) resets to the top row so the best match is
    /// pre-selected; an unchanged query preserves the user's index, clamped.
    fn refresh(&self) {
        if !self.open.get() {
            return;
        }
        let query = self.entry.text().to_string();

        // Decide reset-vs-preserve, then record this query for the next refresh.
        // Bound to a local so the read borrow is gone before the write below.
        let reset_to_top = self.last_query.borrow().as_deref() != Some(query.as_str());
        *self.last_query.borrow_mut() = Some(query.clone());

        // Capture the currently-selected index BEFORE the rebuild.
        let prev_selected = self.list.selected_row().map(|row| row.index());

        // Clear the previous rows.
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        // The MRU borrow is held for the ranking call and nothing else.
        let ranked = {
            let recent = self.mru_recent.borrow();
            palette_rows(recent.as_slice(), &query, PALETTE_MAX_ROWS)
        };

        for row in &ranked {
            let label = gtk::Label::builder()
                .label(&row.display)
                .halign(gtk::Align::Start)
                .build();
            let list_row = gtk::ListBoxRow::new();
            list_row.set_child(Some(&label));
            self.list.append(&list_row);
        }
        let new_len = ranked.len();
        *self.rows.borrow_mut() = ranked;

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

    /// Activates the selected row (or the top row when nothing is selected):
    /// RECORDS its MRU key, CLOSES the palette, then hands the command to the
    /// shell's handler. With ZERO rows it is a true no-op.
    ///
    /// **That order is load-bearing and diverges from macOS's record → dispatch →
    /// close.** A dispatched `CloseWindow` / `QuitApplication` synchronously runs
    /// `app::wire_close`'s handler, which takes the core out and tears the window
    /// down, so nothing may touch palette widgets afterwards. The handler `Rc` is
    /// cloned OUT of its `RefCell` for the same reason: no borrow may be live across
    /// a call that can re-enter this widget.
    fn activate_selection(&self) {
        let selected = self.list.selected_row().map(|row| row.index());
        // Everything needed is copied out under a short borrow, which is dropped
        // before the record / close / dispatch sequence below runs.
        let chosen = {
            let rows = self.rows.borrow();
            activation_index(selected, rows.len())
                .and_then(|index| rows.get(index))
                .map(|row| {
                    (
                        row.mru_key.clone(),
                        palette_action_for(row.kind, row.command),
                    )
                })
        };
        let Some((mru_key, action)) = chosen else {
            return;
        };

        let command = match action {
            PaletteAction::Dispatch(command) => command,
            // Unreachable while `palette_rows`' filter stands, but never silent.
            PaletteAction::Unsupported(reason) => {
                eprintln!("wolf-gnome: palette row {mru_key:?} not activated: {reason}");
                return;
            }
        };

        // (1) RECORD. Short core borrow in its own statement; the SQLite write
        // happens with no borrow live.
        let mru = {
            let core_ref = self.core.borrow();
            core_ref.as_ref().map(WolfCore::command_mru)
        };
        if let Some(mru) = mru {
            mru.record(&mru_key);
        }

        // (2) CLOSE, while this widget is still guaranteed to exist.
        self.close();

        // (3) DISPATCH, with the handler cloned out and no borrow held.
        let handler = self.on_activate.borrow().clone();
        if let Some(handler) = handler {
            handler(command);
        }
    }
}

/// A handle to the built palette, used by the shell to drive it (the
/// `win.command-palette` action, close-on-shutdown, the one-overlay-at-a-time rule)
/// and to query whether it currently owns the keyboard (so key forwarding to the
/// core can be suppressed).
#[derive(Clone)]
pub struct PaletteHandle {
    inner: Rc<PaletteInner>,
}

impl PaletteHandle {
    /// Opens the palette (idempotent), snapshotting the command MRU.
    pub fn open(&self) {
        self.inner.open();
    }

    /// Closes the palette (idempotent), returning focus to the editor.
    pub fn close(&self) {
        self.inner.close();
    }

    /// Whether the palette is currently open. The app uses this to decide whether to
    /// forward keys to the core (it must not while the palette has focus).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.inner.open.get()
    }

    /// Installs the handler run when a row is activated, replacing any previous one.
    ///
    /// The shell wires this to its own command dispatch, so a palette activation and
    /// a command drained from Neovim funnel through exactly one path.
    pub fn connect_activate<F: Fn(Command) + 'static>(&self, handler: F) {
        *self.inner.on_activate.borrow_mut() = Some(Rc::new(handler));
    }
}

/// Builds the palette widgets as an overlay child over `area` (the overlay's main
/// child), wires all signals, and returns a [`PaletteHandle`]. The palette starts
/// hidden; call [`PaletteHandle::open`] (which the `win.command-palette` action
/// does) to show it.
///
/// `area` is the focus-return widget — the persistent cmdline strip, never a pane
/// drawing area (`DECISIONS.md`, "Window structure").
#[must_use]
pub fn build(
    overlay: &gtk::Overlay,
    area: &gtk::DrawingArea,
    core: &Rc<RefCell<Option<WolfCore>>>,
) -> PaletteHandle {
    let entry = gtk::SearchEntry::new();
    entry.set_hexpand(true);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(PALETTE_LIST_MAX_HEIGHT)
        .child(&list)
        .build();

    let palette_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    palette_box.add_css_class("background");
    palette_box.set_halign(gtk::Align::Center);
    palette_box.set_valign(gtk::Align::Start);
    palette_box.set_margin_top(PALETTE_BOX_MARGIN);
    palette_box.set_width_request(PALETTE_BOX_WIDTH);
    palette_box.append(&entry);
    palette_box.append(&scroller);
    palette_box.set_visible(false);

    overlay.add_overlay(&palette_box);

    let inner = Rc::new(PaletteInner {
        open: Cell::new(false),
        rows: RefCell::new(Vec::new()),
        mru_recent: RefCell::new(Vec::new()),
        palette_box,
        entry: entry.clone(),
        list: list.clone(),
        area: area.clone(),
        core: core.clone(),
        last_query: RefCell::new(None),
        on_activate: RefCell::new(None),
    });

    // Re-rank on every query change. A Weak avoids a reference cycle (the widgets
    // outlive nothing here, but the closures are owned by widgets `inner` owns).
    {
        let inner_weak = Rc::downgrade(&inner);
        entry.connect_search_changed(move |_entry| {
            if let Some(inner) = inner_weak.upgrade() {
                inner.refresh();
            }
        });
    }

    // Enter in the search entry activates the current selection.
    {
        let inner_weak = Rc::downgrade(&inner);
        entry.connect_activate(move |_entry| {
            if let Some(inner) = inner_weak.upgrade() {
                inner.activate_selection();
            }
        });
    }

    // Clicking / activating a row runs it.
    {
        let inner_weak = Rc::downgrade(&inner);
        list.connect_row_activated(move |_list, _row| {
            if let Some(inner) = inner_weak.upgrade() {
                inner.activate_selection();
            }
        });
    }

    // Palette key handling (Capture phase so the SearchEntry does not swallow these
    // first): Esc closes; Up/Down move the LIST selection (NOT the entry's text
    // cursor) and are consumed so the keypress does not also move the caret; every
    // other key Proceeds to the entry so typing flows normally.
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
        inner.palette_box.add_controller(key);
    }

    PaletteHandle { inner }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use wolf_core::{rank_palette, Command, CommandSpec, PaletteRowKind, COMMAND_SPECS};

    use crate::command_action::{ui_action_for, UiAction};

    /// The query the cage smoke `command_palette_chord_runs_the_selected_command_under_cage`
    /// types into the palette's search entry (its `PALETTE_QUERY`).
    ///
    /// Duplicated here rather than shared because `tests/smoke_headless.rs` is a
    /// separate crate; [`palette_rows_query_new_selects_exactly_the_new_buffer_row`]
    /// is what keeps the two from drifting, by proving in the FAST suite that this
    /// query still yields exactly one row.
    const SMOKE_QUERY: &str = "new";

    /// The MRU key that same smoke asserts the activation recorded (its
    /// `PALETTE_MRU_KEY`). Checked against the registry rather than trusted.
    const SMOKE_MRU_KEY: &str = "new_buffer";

    /// Whether `command` is one this front-end DISPATCHES — i.e. `ui_action_for`
    /// classifies it as neither of the two unimplemented variants.
    fn is_dispatched(command: Command) -> bool {
        !matches!(
            ui_action_for(command),
            UiAction::Deferred(_) | UiAction::NoGnomeEquivalent(_)
        )
    }

    /// Every [`COMMAND_SPECS`] row the palette may legitimately show: dispatched by
    /// this front-end, and not the palette's own invoking command (which
    /// `rank_palette` excludes from its candidates).
    ///
    /// COMPUTED FROM THE REGISTRY on every run, never written out as a literal
    /// count: appending a command, or promoting one from `Deferred` to a real
    /// `UiAction`, must move these tests rather than leave a stale number behind.
    fn actionable_specs() -> Vec<&'static CommandSpec> {
        COMMAND_SPECS
            .iter()
            .filter(|spec| spec.command != Command::CommandPalette && is_dispatched(spec.command))
            .collect()
    }

    /// The first registered command this front-end does NOT dispatch, used as the
    /// probe for the "Static row whose command is unsupported" arm.
    ///
    /// Resolved from the registry rather than hardcoded, so promoting whichever
    /// command happens to be first today does not turn this probe into a
    /// `Dispatch` and make the assertion below fail for an unrelated reason.
    fn undispatched_probe() -> Command {
        COMMAND_SPECS
            .iter()
            .find(|spec| !is_dispatched(spec.command))
            .map(|spec| spec.command)
            .unwrap_or_else(|| {
                panic!(
                    "every registered command is now dispatched by wolf-gnome, so there is no \
                     command left to probe `palette_action_for`'s unsupported arm with. That is a \
                     genuine milestone, not a failure — but this test must then be given a \
                     different probe (or deleted along with the arm it covers)."
                )
            })
    }

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a mid-sentence
    /// clause. Mirrors `command_action.rs`'s helper of the same name, including its
    /// deliberate allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    /// What a [`PaletteCase`] row expects [`palette_action_for`] to return.
    ///
    /// The unsupported class is matched on its CLASS, not on its reason string:
    /// the reasons are documentation, and their real contract (non-empty, distinct,
    /// clause-shaped) is asserted by
    /// [`palette_unsupported_reasons_are_distinct_and_non_empty`].
    #[derive(Clone, Copy)]
    enum WantAction {
        /// An exact `PaletteAction::Dispatch(command)`.
        Dispatch(Command),
        /// `PaletteAction::Unsupported(_)` — the palette must not offer this row.
        Unsupported,
    }

    /// One `palette_action_for` table row.
    struct PaletteCase {
        desc: &'static str,
        kind: PaletteRowKind,
        command: Option<Command>,
        want: WantAction,
    }

    /// A row is actionable exactly when its command classifies to a real
    /// [`UiAction`]; every other `(kind, command)` pair is `Unsupported`.
    ///
    /// The regression this pins is a palette row that prints "not dispatched" when
    /// the user presses Enter — the one surface in this crate that would list a
    /// no-op, which `actions.rs`'s `menu_never_lists_a_reserved_action` and
    /// `shell::install_actions` both refuse to do.
    #[test]
    fn palette_action_table() {
        let undispatched = undispatched_probe();

        let cases = [
            PaletteCase {
                desc: "Static + NewBuffer: dispatched, so the row runs the command",
                kind: PaletteRowKind::Static,
                command: Some(Command::NewBuffer),
                want: WantAction::Dispatch(Command::NewBuffer),
            },
            PaletteCase {
                desc: "Static + GoToFile: dispatched (it opens the file picker)",
                kind: PaletteRowKind::Static,
                command: Some(Command::GoToFile),
                want: WantAction::Dispatch(Command::GoToFile),
            },
            PaletteCase {
                desc: "Static + Copy: Deferred on GTK, so the row is unsupported",
                kind: PaletteRowKind::Static,
                command: Some(Command::Copy),
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "Static + HideApplication: NoGnomeEquivalent, so unsupported",
                kind: PaletteRowKind::Static,
                command: Some(Command::HideApplication),
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "Static + the first registry row this front-end does not dispatch",
                kind: PaletteRowKind::Static,
                command: Some(undispatched),
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "Static + no command at all: nothing to dispatch",
                kind: PaletteRowKind::Static,
                command: None,
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "SwitchWindow: wolf-gnome is single-window, so there is nothing to raise",
                kind: PaletteRowKind::SwitchWindow,
                command: None,
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "OpenRecent: no recent-directories rows are fed to the palette",
                kind: PaletteRowKind::OpenRecent,
                command: None,
                want: WantAction::Unsupported,
            },
        ];

        for case in &cases {
            let got = palette_action_for(case.kind, case.command);
            match case.want {
                WantAction::Dispatch(expected) => assert_eq!(
                    got,
                    PaletteAction::Dispatch(expected),
                    "case: {} — palette_action_for({:?}, {:?}) returned {got:?}. A command \
                     `ui_action_for` classifies as a real UiAction MUST resolve to \
                     Dispatch(command): anything else silently drops the row out of the \
                     palette's list, so a working command becomes unreachable from the palette.",
                    case.desc,
                    case.kind,
                    case.command
                ),
                WantAction::Unsupported => assert!(
                    matches!(got, PaletteAction::Unsupported(_)),
                    "case: {} — palette_action_for({:?}, {:?}) returned {got:?}, but this row is \
                     one wolf-gnome could not perform when the row was written. A Dispatch here \
                     puts a row in the palette that does nothing when the user presses Enter — \
                     the exact failure `menu_never_lists_a_reserved_action` exists to prevent for \
                     the menu. If the command was GENUINELY implemented since, move this row to \
                     WantAction::Dispatch and tick its feat-list entry; do not relax the arm.",
                    case.desc,
                    case.kind,
                    case.command
                ),
            }
        }
    }

    /// Belt-and-braces for the classification, walked over the REGISTRY rather than
    /// over the table above: for every registered command, a `Static` row is
    /// `Dispatch` exactly when `ui_action_for` gives it a real action.
    ///
    /// Mirrors `command_action.rs`'s `every_registered_command_maps`. Walking the
    /// registry is what makes it survive someone adding a catch-all arm to
    /// `palette_action_for`, which the `_`-free match alone would not.
    #[test]
    fn every_registered_command_is_classified_by_the_palette() {
        let mut dispatched = 0usize;
        let mut unsupported = 0usize;

        for spec in COMMAND_SPECS {
            let got = palette_action_for(PaletteRowKind::Static, Some(spec.command));
            if is_dispatched(spec.command) {
                dispatched += 1;
                assert_eq!(
                    got,
                    PaletteAction::Dispatch(spec.command),
                    "registered command {:?} (id {:?}) is DISPATCHED by `ui_action_for`, but \
                     `palette_action_for` returned {got:?} for its Static row. The palette would \
                     then hide a command wolf-gnome can actually perform.",
                    spec.command,
                    spec.id
                );
            } else {
                unsupported += 1;
                assert!(
                    matches!(got, PaletteAction::Unsupported(_)),
                    "registered command {:?} (id {:?}) is Deferred/NoGnomeEquivalent in \
                     `ui_action_for`, but `palette_action_for` returned {got:?} for its Static \
                     row. The palette would list a command that prints a reason instead of doing \
                     anything.",
                    spec.command,
                    spec.id
                );
            }
        }

        assert!(
            dispatched > 0,
            "the walk saw NO dispatched command, so every Dispatch assertion above ran zero \
             times and this test proves nothing. Either `ui_action_for` stopped dispatching \
             anything (which would leave the palette empty and the front-end inert) or the \
             registry is empty."
        );
        assert!(
            unsupported > 0,
            "the walk saw NO unsupported command, so every Unsupported assertion above ran zero \
             times and this test proves only half of what it claims. If wolf-gnome really does \
             dispatch every registered command now, replace this assertion deliberately rather \
             than deleting it."
        );
    }

    /// The reasons a `PaletteAction::Unsupported` carries are non-empty, mutually
    /// distinct, and read as a mid-sentence clause.
    ///
    /// Distinctness matters for the same reason it does in `command_action.rs`: the
    /// reason is what a reader sees when a row refuses to run, and two arms sharing
    /// a string make "no recent directories are fed" indistinguishable from "there
    /// is only one window". Mid-sentence casing matters because the reason is
    /// interpolated into a printed diagnostic, never used as a sentence of its own.
    #[test]
    fn palette_unsupported_reasons_are_distinct_and_non_empty() {
        // One probe per arm of `palette_action_for` that can return Unsupported.
        // The Deferred and NoGnomeEquivalent commands deliberately share ONE probe:
        // the plan gives both the same "not dispatched by this front-end" reason,
        // so probing them separately would demand a distinction the design does not
        // make.
        let probes = [
            (
                "a Static row whose command this front-end does not dispatch",
                palette_action_for(PaletteRowKind::Static, Some(undispatched_probe())),
            ),
            (
                "a Static row carrying no command at all",
                palette_action_for(PaletteRowKind::Static, None),
            ),
            (
                "a SwitchWindow row",
                palette_action_for(PaletteRowKind::SwitchWindow, None),
            ),
            (
                "an OpenRecent row",
                palette_action_for(PaletteRowKind::OpenRecent, None),
            ),
        ];

        let mut seen: HashSet<&'static str> = HashSet::new();
        for (source, action) in probes {
            let PaletteAction::Unsupported(reason) = action else {
                panic!(
                    "{source} produced {action:?} rather than an Unsupported reason, so this \
                     test cannot check the reason it exists to check. Either that arm now \
                     dispatches (see `palette_action_table`, which would also be red) or the \
                     probe no longer selects the arm it names."
                )
            };

            assert!(
                !reason.trim().is_empty(),
                "the Unsupported reason for {source} is empty. The reason is printed when a row \
                 refuses to run; an empty one makes the refusal indistinguishable from a crash."
            );
            assert!(
                !is_sentence_cased(reason),
                "the Unsupported reason for {source} is sentence-cased ({reason:?}). It is \
                 interpolated mid-sentence into the printed diagnostic, exactly as \
                 `command_action.rs`'s reasons are, so it must start lowercase (an ALL-CAPS \
                 proper noun such as \"GNOME\" is fine)."
            );
            assert!(
                seen.insert(reason),
                "the Unsupported reason for {source} ({reason:?}) duplicates another arm's \
                 reason. Reasons must be DISTINCT so the printed diagnostic names the real \
                 cause: with two arms sharing a string, \"this row is not fed any recent \
                 directories\" and \"there is only one window\" become the same message."
            );
        }
    }

    /// `palette_rows` lists exactly the registry commands this front-end performs —
    /// no deferred row, no `SwitchWindow` row, no `OpenRecent` row, and never the
    /// palette's own invoking command.
    ///
    /// The expected count is computed from `COMMAND_SPECS` here, never written as a
    /// literal, so appending a command or promoting one cannot leave this test
    /// asserting a stale number.
    #[test]
    fn palette_rows_lists_only_actionable_commands() {
        let rows = palette_rows(&[], "", PALETTE_MAX_ROWS);
        let actionable = actionable_specs();

        assert!(
            !rows.is_empty(),
            "`palette_rows` returned NO rows for the empty query, so every per-row assertion \
             below runs zero times and this test proves nothing. An empty palette is also a \
             user-visible bug in its own right: the overlay would open onto a blank list."
        );
        assert!(
            !actionable.is_empty(),
            "the registry carries no dispatched, non-palette command at all, so the expected \
             count this test compares against is zero and the comparison is vacuous."
        );
        assert_eq!(
            rows.len(),
            actionable.len(),
            "`palette_rows` returned {} row(s) for the empty query, but the registry has {} \
             command(s) that are both dispatched by this front-end and not the palette itself. \
             More rows means the classification filter is not being applied and a deferred \
             command is on offer; fewer means a command wolf-gnome CAN perform is missing from \
             the palette.",
            rows.len(),
            actionable.len()
        );

        let palette_title = Command::CommandPalette.spec().title;
        for row in &rows {
            assert_eq!(
                row.kind,
                PaletteRowKind::Static,
                "palette row {:?} has kind {:?}. wolf-gnome feeds `rank_palette` an empty window \
                 slice and an empty recent-directories slice, so SwitchWindow and OpenRecent rows \
                 cannot be produced — and would not be actionable if they were. A row of either \
                 kind here means one of those two slices stopped being empty without its \
                 activation arm being wired.",
                row.display,
                row.kind
            );
            let Some(command) = row.command else {
                panic!(
                    "palette row {:?} is a Static row carrying NO command, so pressing Enter on \
                     it can do nothing at all. `rank_palette` sets `command` on every Static row \
                     it builds, so a None here means the row came from somewhere else.",
                    row.display
                )
            };
            assert_eq!(
                palette_action_for(row.kind, row.command),
                PaletteAction::Dispatch(command),
                "palette row {:?} ({command:?}) does not classify as Dispatch, so it is a row \
                 the user can select and activate to no effect. That is the one thing the \
                 post-rank filter in `palette_rows` exists to prevent.",
                row.display
            );
            assert_ne!(
                row.display.as_str(),
                palette_title,
                "the palette lists its OWN invoking command ({palette_title:?}). `rank_palette` \
                 excludes `Command::CommandPalette` from its candidates precisely so a palette \
                 cannot dispatch itself; a row here means that exclusion was lost."
            );
        }

        let mut got_keys: Vec<&str> = rows.iter().map(|row| row.mru_key.as_str()).collect();
        let mut want_keys: Vec<&str> = actionable.iter().map(|spec| spec.id).collect();
        got_keys.sort_unstable();
        want_keys.sort_unstable();
        assert_eq!(
            got_keys, want_keys,
            "the SET of palette rows is not the set of actionable registry commands. Each row's \
             `mru_key` is its canonical registry id, so this names exactly which command is \
             missing from the palette or is being offered despite not being dispatched."
        );
    }

    /// The command-MRU snapshot really reaches `rank_palette`: a key in the recent
    /// list floats its row to the front of the empty-query list, and with no MRU at
    /// all the first row is the registry's first actionable command.
    #[test]
    fn palette_rows_empty_query_is_mru_first() {
        let actionable = actionable_specs();
        let first = actionable.first().unwrap_or_else(|| {
            panic!(
                "the registry carries no dispatched, non-palette command, so there is no \
                 \"first actionable row\" for this test to expect."
            )
        });
        let probe = Command::NewBuffer.spec().id;

        assert!(
            is_dispatched(Command::NewBuffer),
            "the MRU probe {probe:?} is no longer a dispatched command, so its row is filtered \
             out of the palette entirely and the MRU-first assertion below would fail for a \
             reason that has nothing to do with the MRU. Pick a probe that is still dispatched."
        );
        assert_ne!(
            first.id, probe,
            "the MRU probe {probe:?} is ALSO the registry's first actionable row, so the warm \
             assertion below would pass with the MRU snapshot thrown away entirely — a vacuous \
             test, not a passing one. Pick a probe that is not first in COMMAND_SPECS order."
        );

        let cold = palette_rows(&[], "", PALETTE_MAX_ROWS);
        assert_eq!(
            cold.first().map(|row| row.mru_key.as_str()),
            Some(first.id),
            "with an EMPTY command MRU the empty-query list must open on the registry's first \
             actionable row ({:?}). A different row means the candidate order is no longer \
             COMMAND_SPECS order, which is what the MRU assertion below is measured against.",
            first.id
        );

        let recent = vec![probe.to_string()];
        let warm = palette_rows(&recent, "", PALETTE_MAX_ROWS);
        assert_eq!(
            warm.first().map(|row| row.mru_key.as_str()),
            Some(probe),
            "with {probe:?} in the command MRU it must be the FIRST row of the empty-query list \
             (`rank_matches` sorts the MRU group ahead of everything else). A different row here \
             means `palette_rows` is not passing its `command_mru_recent` argument through to \
             `rank_palette`, so the palette would open in registry order forever and the MRU \
             SQLite read at open would be dead weight."
        );
    }

    /// The cage smoke's premise, made executable: the query
    /// `command_palette_chord_runs_the_selected_command_under_cage` types matches
    /// EXACTLY ONE palette row, that row is New (`new_buffer`), and that holds with
    /// a non-empty command MRU too.
    ///
    /// Without this, a registry rename — or a second `new`-matching command being
    /// promoted to dispatched — would not fail anything in the fast suite. It would
    /// instead make the smoke stochastic: a second row would appear, `restored_index`
    /// would still select the top one, and which command Enter ran would depend on
    /// the ranking tie-breaks and on whatever the MRU happened to hold.
    #[test]
    fn palette_rows_query_new_selects_exactly_the_new_buffer_row() {
        let key = Command::NewBuffer.spec().id;

        // Precondition: the literals the smoke carries still name this row. The
        // smoke is a separate crate and cannot import these, so this is the join.
        assert_eq!(
            key, SMOKE_MRU_KEY,
            "`Command::NewBuffer`'s registry id is {key:?}, but the cage smoke asserts the \
             palette activation recorded the literal {SMOKE_MRU_KEY:?} in the command MRU. \
             Update `PALETTE_MRU_KEY` in `tests/smoke_headless.rs` and the const above together, \
             or that smoke's MRU assertion becomes unfalsifiable."
        );

        // Precondition: the FILTER is what makes this query unique, not the query
        // being rare. Unfiltered, `New Window` and `New Tab` match it too — both
        // Deferred — so a filter regression turns one row into three.
        let unfiltered = rank_palette(&[], &[], &[], SMOKE_QUERY, usize::MAX);
        assert!(
            unfiltered.len() > 1,
            "`rank_palette` matches only {} candidate(s) for {SMOKE_QUERY:?} before the \
             classification filter runs, so the \"exactly one row\" assertion below would hold \
             even with the filter deleted and would stop discriminating it. Titles matching \
             {SMOKE_QUERY:?} were: {:?}",
            unfiltered.len(),
            unfiltered
                .iter()
                .map(|row| &row.display)
                .collect::<Vec<_>>()
        );

        let mrus: [Vec<String>; 2] = [Vec::new(), vec![Command::ClosePane.spec().id.to_string()]];
        for recent in &mrus {
            let rows = palette_rows(recent, SMOKE_QUERY, PALETTE_MAX_ROWS);
            assert_eq!(
                rows.len(),
                1,
                "`palette_rows` returned {} row(s) for the query {SMOKE_QUERY:?} with command \
                 MRU {recent:?}; the cage smoke presses Enter on the top row and asserts a NEW \
                 BUFFER replaced the open file, so a second matching row makes which command it \
                 runs depend on ranking tie-breaks. Rows were: {:?}",
                rows.len(),
                rows.iter().map(|row| &row.display).collect::<Vec<_>>()
            );

            let row = &rows[0];
            assert_eq!(
                row.command,
                Some(Command::NewBuffer),
                "the sole row matching {SMOKE_QUERY:?} (with command MRU {recent:?}) carries \
                 {:?}, not Command::NewBuffer. The smoke asserts the base file's heading is GONE \
                 from the capture, which only a new untitled buffer produces.",
                row.command
            );
            assert_eq!(
                row.mru_key.as_str(),
                key,
                "the sole row matching {SMOKE_QUERY:?} (with command MRU {recent:?}) records \
                 {:?} on activation, not {key:?}. The smoke reads the command-MRU sqlite back \
                 and looks for exactly that key.",
                row.mru_key
            );
        }
    }

    /// `max` is applied AFTER the classification filter, so a cap counts only rows
    /// the user can actually run.
    ///
    /// Ranking unbounded and truncating afterwards is not a micro-optimization in
    /// reverse: `rank_palette` truncates before this crate's filter can see the
    /// list, so passing `max` through would let deferred rows eat the budget and
    /// hand the user a short — or empty — palette. With `max == 1` the single
    /// ranked candidate is `COMMAND_SPECS[0]` (`NewWindow`, deferred), so the
    /// regression shows up here as zero rows rather than one.
    #[test]
    fn palette_rows_cap_counts_only_actionable_rows() {
        let full = palette_rows(&[], "", PALETTE_MAX_ROWS);
        assert!(
            full.len() > 1,
            "the uncapped empty-query list holds {} row(s), so capping it at 1 removes nothing \
             and this test cannot tell a post-filter cap from a pre-filter one.",
            full.len()
        );

        let capped = palette_rows(&[], "", 1);
        assert_eq!(
            capped.len(),
            1,
            "`palette_rows(&[], \"\", 1)` returned {} row(s), not 1. Zero means `max` is being \
             handed to `rank_palette`, which truncates BEFORE the classification filter runs: \
             the one ranked candidate is the registry's first row, that row is deferred, and the \
             filter then empties the list. Rank unbounded and `.take(max)` after the filter.",
            capped.len()
        );
        assert_eq!(
            capped.first().map(|row| row.mru_key.as_str()),
            full.first().map(|row| row.mru_key.as_str()),
            "the capped list's single row is not the uncapped list's first row, so applying the \
             cap reordered the results. `.take(max)` must preserve the ranking order it \
             truncates."
        );
    }
}
