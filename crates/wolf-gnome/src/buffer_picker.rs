//! The GNOME BUFFER-PICKER overlay for `wolf-gnome`.
//!
//! Trigger: the **`win.buffer-picker`** action — bound to **Ctrl+Shift+E** and
//! listed in the primary menu as "Buffer Picker…". Like the file picker and the
//! command palette, the chord is resolved from [`crate::actions::ACTION_TABLE`] by
//! a CAPTURE-phase controller ([`crate::shell`]), so it is guaranteed not to also
//! be forwarded to Neovim (see `crates/wolf-gnome/DECISIONS.md`, "The accelerator
//! namespace").
//!
//! The overlay is a search entry over the captive Neovim's `buflisted` buffers,
//! ranked by [`wolf_core::rank_buffers`]. `Esc` closes it, Up/Down move the
//! selection, and `Enter` (or a row click) switches the ACTIVE Neovim window to the
//! activated row's buffer through the fire-and-forget
//! [`WolfCore::switch_buffer`](wolf_core::WolfCore::switch_buffer), then closes.
//!
//! **The candidate set is fixed at open, so this module has no wake bridge at
//! all.** The snapshot is one `WolfCore::list_buffers` call — a BLOCKING RPC on the
//! GTK main thread — plus the live cwd, taken once in [`BufferPickerInner::open`]
//! and re-ranked synchronously per keystroke. Adding [`crate::picker`]'s
//! `async-channel` + `glib::spawn_future_local` drain plumbing "for symmetry" would
//! mean re-issuing that RPC on the typing path, i.e. up to one full `rpc_timeout`
//! budget of frozen main loop per key against a wedged Neovim. Do not add it.
//!
//! **Opening may be REFUSED, and the refusal is the point.** [`open_decision`] is
//! consulted before any widget is touched: only a core reading
//! [`Liveness::Alive`] is snapshotted, and every other reading — including the
//! absent core the window's close handler leaves behind — refuses, prints its
//! reason, and issues no RPC at all.
//!
//! **The guard cannot prevent the FIRST stall, and is not meant to.** While the
//! core still reads `Alive` nothing has observed a wedge, so the first press is
//! what discovers it: it burns one `rpc_timeout` budget, marks the core
//! `NotResponding` and raises the stall overlay. What the guard prevents is the
//! repeat freeze — every later press, and every press against a `Dead` core, costs
//! nothing. Do not "strengthen" it into something that can never raise a stall.
//!
//! Liveness is NOT re-checked after the snapshot call returns. After a timeout the
//! overlay opens with zero rows for one frame and `shell::apply_overlay`'s
//! hidden→visible transition closes it as it raises the stall overlay.
//!
//! Its pure, display-free seams are [`open_decision`], [`initial_selection`],
//! [`switch_target`] and [`row_label`], unit-tested below. Selection movement and
//! activation reuse [`crate::picker`]'s [`next_selection`], [`restored_index`] and
//! [`activation_index`]; [`initial_selection`] deliberately does NOT come from
//! there, because it is the one selection rule the three overlays do not share.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use wolf_core::{rank_buffers, BufInfo, BufferRow, Liveness, WolfCore};

use crate::picker::{activation_index, next_selection, restored_index};

/// The maximum number of ranked rows shown in the buffer list.
const BUFFER_MAX_ROWS: usize = 200;

/// Top margin (px) of the floating buffer-picker box within the overlay.
const BUFFER_BOX_MARGIN: i32 = 24;

/// Width (px) of the floating buffer-picker box.
const BUFFER_BOX_WIDTH: i32 = 600;

/// Maximum height (px) of the buffer picker's scrolled result list. The list
/// propagates its natural height below this, so the handful of rows a real buffer
/// list usually holds is not drawn as a tall empty panel.
const BUFFER_LIST_MAX_HEIGHT: i32 = 400;

/// The marker appended to a MODIFIED buffer's row label by [`row_label`].
const MODIFIED_SUFFIX: &str = " •";

/// What pressing Ctrl+Shift+E does, given the captive Neovim's health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenDecision {
    /// Snapshot the captive nvim's listed buffers (one blocking RPC) and show
    /// the overlay.
    Snapshot,
    /// Do not open, and do not issue the RPC; the string names why, as a
    /// mid-sentence clause (the [`crate::command_action`] / [`crate::palette`]
    /// convention).
    Refuse(&'static str),
}

/// Whether the buffer picker may open, given the core's liveness reading (or
/// `None` when there is no core at all).
///
/// TOTAL by construction: the implementation is a `match` over `Option<Liveness>`
/// with **no `_` arm**, so a newly appended [`Liveness`] variant fails to compile
/// here rather than silently inheriting whichever behaviour a catch-all had.
///
/// `Option<Liveness>` — not `Liveness` — is the input type precisely so the absent
/// core is a CLASSIFIED arm rather than an early return at the call site, which no
/// headless test could see.
///
/// Only [`Liveness::Alive`] is snapshotted. The snapshot is a blocking RPC on the
/// GTK main thread, so issuing one against a core already known to be wedged
/// freezes the window for a full timeout budget on every press, and one against a
/// dead handle is not reliably fast either. See the module docs for why this cannot
/// prevent the FIRST stall.
#[must_use]
pub fn open_decision(liveness: Option<Liveness>) -> OpenDecision {
    match liveness {
        Some(Liveness::Alive) => OpenDecision::Snapshot,
        Some(Liveness::NotResponding) => OpenDecision::Refuse(
            "the captive Neovim is already known not to be responding, and the candidate snapshot \
             is a blocking RPC that would freeze the window for another full timeout budget",
        ),
        Some(Liveness::Dead) => OpenDecision::Refuse(
            "the captive Neovim has died, so there is no buffer list to ask for and an RPC \
             against a dead handle is not reliably fast",
        ),
        None => OpenDecision::Refuse(
            "the core has already been taken out by the window's close handler, so there is \
             nothing left to ask",
        ),
    }
}

/// Which row the FIRST refresh after an open preselects: `min(1, n_rows - 1)`, and
/// `None` for an empty list.
///
/// **This is the one selection rule the three overlays do NOT share.** The file
/// picker and the command palette preselect row 0, because their top row is their
/// best match. Here row 0 is the buffer the user is ALREADY in — `rank_buffers`
/// sorts by `lastused` DESC — so preselecting it would make a bare `Enter` on the
/// empty-query open appear to do nothing. Row 1 is the most-recently-used OTHER
/// buffer, which is what makes Ctrl+Shift+E ⏎ a telescope-style "switch back".
///
/// A faithful port of `BufferPickerView.swift`'s `min(1, rows.count - 1)` behind
/// its own first-open flag; every LATER refresh falls through to
/// [`restored_index`], exactly as macOS falls through to `PickerSeam.restoredIndex`.
#[must_use]
pub(crate) fn initial_selection(n_rows: usize) -> Option<usize> {
    if n_rows == 0 {
        None
    } else {
        Some(1.min(n_rows - 1))
    }
}

/// What `Enter` does, as one total function: the `(win, bufnr)` pair to switch, or
/// `None` for "do nothing at all".
///
/// The row is resolved through [`activation_index`], so zero rows is a true no-op
/// and an absent / negative / out-of-range selection is the top row.
///
/// **An `active_win` of `Some(0)` is refused as firmly as `None` is.** That is
/// `wolf-ffi`'s rule (`WolfCoreHandle::switch_buffer` resolves
/// `active_win.unwrap_or(0)` and no-ops on `0`) and it must not be dropped here: an
/// `InputEvent::SwitchBuffer { win: 0 }` would bind the switch into whichever
/// window Neovim happens to consider current, replacing a buffer in a pane the user
/// never selected.
#[must_use]
pub fn switch_target(
    rows: &[BufferRow],
    selected: Option<i32>,
    active_win: Option<i64>,
) -> Option<(i64, i64)> {
    let win = active_win.filter(|win| *win != 0)?;
    let index = activation_index(selected, rows.len())?;
    let row = rows.get(index)?;
    Some((win, row.bufnr))
}

/// The label one row draws: the ranked `display`, plus [`MODIFIED_SUFFIX`] when the
/// buffer has unsaved changes.
///
/// The suffix is DISPLAY-ONLY and is never the string the query is ranked against:
/// [`rank_buffers`] ranks [`BufferRow::display`], which core computes. Appending it
/// to the ranked string instead would let a query match the marker.
#[must_use]
pub(crate) fn row_label(row: &BufferRow) -> String {
    if row.modified {
        format!("{}{MODIFIED_SUFFIX}", row.display)
    } else {
        row.display.clone()
    }
}

/// The buffer picker's shared, interior-mutable state. All access happens on the
/// single GTK main thread, so `Cell`/`RefCell` (not a `Mutex`) is sufficient.
struct BufferPickerInner {
    /// Whether the picker is currently open (its widgets visible and focused).
    open: Cell<bool>,
    /// The listed buffers, snapshotted ONCE at [`BufferPickerInner::open`]. See the
    /// module docs: re-taking this means re-issuing a blocking RPC.
    buffers: RefCell<Vec<BufInfo>>,
    /// The captive nvim's working directory as of the same snapshot, which is what
    /// [`rank_buffers`] projects each absolute buffer name against.
    cwd: RefCell<Option<PathBuf>>,
    /// The rows currently materialized into the `ListBox`, parallel to it.
    rows: RefCell<Vec<BufferRow>>,
    /// The overlay child holding the search entry + results list.
    buffer_box: gtk::Box,
    /// The search entry whose text filters/ranks the rows.
    entry: gtk::SearchEntry,
    /// The results list, rebuilt on each refresh.
    list: gtk::ListBox,
    /// The editor drawing area, re-focused when the picker closes.
    area: gtk::DrawingArea,
    /// The live core, shared with the rest of the app.
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The query string used by the previous [`BufferPickerInner::refresh`]. It
    /// decides whether the next refresh resets the selection to the top (query
    /// changed) or preserves it. `None` forces a reset and is set on `open`.
    last_query: RefCell<Option<String>>,
    /// Whether the next refresh is the FIRST one after an open, and so takes
    /// [`initial_selection`] rather than [`restored_index`].
    initial_open: Cell<bool>,
}

impl BufferPickerInner {
    /// Opens the picker: decides whether it may open at all, snapshots the listed
    /// buffers and the cwd, then shows + focuses the entry and refreshes. No-op if
    /// already open.
    ///
    /// **Nothing is shown until the blocking call has returned, and a refusal
    /// touches no widget at all**, so a refused chord never flashes an overlay. The
    /// liveness read and the snapshot each take their own short core borrow, per
    /// this crate's borrow discipline.
    fn open(&self) {
        if self.open.get() {
            return;
        }
        let liveness = {
            let core_ref = self.core.borrow();
            core_ref.as_ref().map(WolfCore::liveness)
        };
        match open_decision(liveness) {
            OpenDecision::Refuse(reason) => {
                eprintln!("wolf-gnome: buffer picker not opened: {reason}");
                return;
            }
            OpenDecision::Snapshot => {}
        }

        // The one blocking RPC a running wolf-gnome issues, plus the RPC-free cwd
        // read, under a borrow of their own.
        let snapshot = {
            let core_ref = self.core.borrow();
            core_ref
                .as_ref()
                .map(|core| (core.list_buffers(), core.live_cwd()))
        };
        let Some((buffers, cwd)) = snapshot else {
            return;
        };
        *self.buffers.borrow_mut() = buffers;
        *self.cwd.borrow_mut() = cwd;

        self.open.set(true);
        self.buffer_box.set_visible(true);
        self.entry.set_text("");
        self.entry.grab_focus();
        // Force the first refresh to take `initial_selection`, regardless of any
        // stale selection/query left over from a previous session.
        *self.last_query.borrow_mut() = None;
        self.initial_open.set(true);
        self.refresh();
    }

    /// Closes the picker: hides the widgets and grabs focus back onto `area`. No-op
    /// if already closed.
    ///
    /// `area` is the persistent cmdline strip, never a pane drawing area, so every
    /// overlay in this crate leaves focus on the same widget (`DECISIONS.md`,
    /// "Window structure").
    ///
    /// The buffer snapshot is deliberately NOT cleared here: the next `open`
    /// overwrites both fields before anything reads them. Clearing them "for
    /// tidiness" would only add a window in which a stale, empty vector is what a
    /// reader finds.
    fn close(&self) {
        if !self.open.get() {
            return;
        }
        self.open.set(false);
        self.buffer_box.set_visible(false);
        self.area.grab_focus();
    }

    /// Rebuilds the results list for the current query from the snapshot taken at
    /// open. No-op when closed. Issues no RPC — see the module docs.
    ///
    /// Selection handling: the FIRST refresh after an open takes
    /// [`initial_selection`] (row 1, the most-recently-used other buffer); every
    /// later one matches the other overlays — a changed query resets to the top row
    /// so the best match is preselected, an unchanged query preserves the user's
    /// index, clamped.
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

        // The snapshot borrows are held for the ranking call and nothing else.
        let ranked = {
            let buffers = self.buffers.borrow();
            let cwd = self.cwd.borrow();
            rank_buffers(buffers.as_slice(), cwd.as_deref(), &query, BUFFER_MAX_ROWS)
        };

        for row in &ranked {
            let label = gtk::Label::builder()
                .label(row_label(row))
                .halign(gtk::Align::Start)
                .build();
            let list_row = gtk::ListBoxRow::new();
            list_row.set_child(Some(&label));
            self.list.append(&list_row);
        }
        let new_len = ranked.len();
        *self.rows.borrow_mut() = ranked;

        let chosen = if self.initial_open.get() {
            self.initial_open.set(false);
            initial_selection(new_len)
        } else {
            let prev_index = prev_selected.and_then(|index| usize::try_from(index).ok());
            restored_index(reset_to_top, prev_index, new_len)
        };
        if let Some(index) = chosen {
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

    /// Switches the ACTIVE Neovim window to the selected row's buffer, then closes.
    /// When [`switch_target`] refuses it is a true no-op — nothing is sent and the
    /// picker stays open, matching [`crate::picker`]'s zero-row rule.
    ///
    /// SEND then CLOSE, like the file picker. The palette inverts that order because
    /// a dispatched `CloseWindow` destroys the widget synchronously; nothing here
    /// can, since [`WolfCore::switch_buffer`](wolf_core::WolfCore::switch_buffer) is
    /// fire-and-forget.
    ///
    /// `active_win` is bound in a statement of its own so the `SnapshotReadGuard` is
    /// dropped before anything else touches the core (the `shell::capture_open_files`
    /// precedent: a re-entrant read can deadlock behind a queued writer).
    fn activate_selection(&self) {
        let selected = self.list.selected_row().map(|row| row.index());
        let active_win = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            let snapshot = core.read_snapshot();
            snapshot.active_win
        };
        let target = {
            let rows = self.rows.borrow();
            switch_target(rows.as_slice(), selected, active_win)
        };
        let Some((win, bufnr)) = target else {
            return;
        };
        if let Some(core) = self.core.borrow().as_ref() {
            core.switch_buffer(win, bufnr);
        }
        self.close();
    }
}

/// A handle to the built buffer picker, used by the shell to drive it (the
/// `win.buffer-picker` action, close-on-shutdown, the one-overlay-at-a-time rule)
/// and to query whether it currently owns the keyboard (so key forwarding to the
/// core can be suppressed).
#[derive(Clone)]
pub struct BufferPickerHandle {
    inner: Rc<BufferPickerInner>,
}

impl BufferPickerHandle {
    /// Opens the picker (idempotent), snapshotting the listed buffers — unless
    /// [`open_decision`] refuses, in which case it prints the reason and does
    /// nothing.
    pub fn open(&self) {
        self.inner.open();
    }

    /// Closes the picker (idempotent), returning focus to the editor.
    pub fn close(&self) {
        self.inner.close();
    }

    /// Whether the picker is currently open. The app uses this to decide whether to
    /// forward keys to the core (it must not while the picker has focus).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.inner.open.get()
    }
}

/// Builds the buffer-picker widgets as an overlay child over `area` (the overlay's
/// main child), wires all signals, and returns a [`BufferPickerHandle`]. The picker
/// starts hidden; call [`BufferPickerHandle::open`] (which the `win.buffer-picker`
/// action does) to show it.
///
/// `area` is the focus-return widget — the persistent cmdline strip, never a pane
/// drawing area (`DECISIONS.md`, "Window structure").
#[must_use]
pub fn build(
    overlay: &gtk::Overlay,
    area: &gtk::DrawingArea,
    core: &Rc<RefCell<Option<WolfCore>>>,
) -> BufferPickerHandle {
    let entry = gtk::SearchEntry::new();
    entry.set_hexpand(true);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(BUFFER_LIST_MAX_HEIGHT)
        .child(&list)
        .build();

    let buffer_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    buffer_box.add_css_class("background");
    buffer_box.set_halign(gtk::Align::Center);
    buffer_box.set_valign(gtk::Align::Start);
    buffer_box.set_margin_top(BUFFER_BOX_MARGIN);
    buffer_box.set_width_request(BUFFER_BOX_WIDTH);
    buffer_box.append(&entry);
    buffer_box.append(&scroller);
    buffer_box.set_visible(false);

    overlay.add_overlay(&buffer_box);

    let inner = Rc::new(BufferPickerInner {
        open: Cell::new(false),
        buffers: RefCell::new(Vec::new()),
        cwd: RefCell::new(None),
        rows: RefCell::new(Vec::new()),
        buffer_box,
        entry: entry.clone(),
        list: list.clone(),
        area: area.clone(),
        core: core.clone(),
        last_query: RefCell::new(None),
        initial_open: Cell::new(false),
    });

    // Re-rank on every query change. A Weak avoids a reference cycle (the closures
    // are owned by widgets `inner` owns).
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

    // Clicking / activating a row switches to it.
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
    // Proceeds to the entry so typing flows normally.
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
        inner.buffer_box.add_controller(key);
    }

    BufferPickerHandle { inner }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::path::Path;

    use wolf_core::{rank_buffers, BufInfo, BufferRow, Liveness, NO_NAME};

    // =====================================================================
    // The whole input domain of `open_decision`
    // =====================================================================

    /// Every liveness state the core can report. Mirrors
    /// `liveness_ui::tests::ALL_LIVENESS`, which is the other total table over
    /// this enum.
    const ALL_LIVENESS: [Liveness; 3] = [Liveness::Alive, Liveness::NotResponding, Liveness::Dead];

    /// Every input [`open_decision`] is total over: the three liveness readings
    /// plus the ABSENT core (`None`), which the post-shutdown
    /// `Rc<RefCell<Option<WolfCore>>>` yields once `wire_close` has taken the core
    /// out.
    ///
    /// `None` is a CLASSIFIED arm rather than an early return at the call site
    /// precisely so it appears in this table; a `let Some(core) = …else { return }`
    /// would be invisible to every test in this module.
    const ALL_OPEN_INPUTS: [Option<Liveness>; 4] = [
        Some(Liveness::Alive),
        Some(Liveness::NotResponding),
        Some(Liveness::Dead),
        None,
    ];

    /// What an [`OpenCase`] row expects [`open_decision`] to return.
    ///
    /// The refusal is matched on its CLASS, not on its reason string: the reasons
    /// are documentation, and their real contract (non-empty, distinct,
    /// clause-shaped) is asserted by [`refuse_reasons_are_distinct_and_non_empty`]
    /// — the `palette::tests::WantAction` precedent.
    #[derive(Clone, Copy)]
    enum WantOpen {
        /// `OpenDecision::Snapshot` — the blocking `list_buffers` RPC is issued.
        Snapshot,
        /// `OpenDecision::Refuse(_)` — no RPC, no overlay.
        Refuse,
    }

    /// One `open_decision` table row.
    struct OpenCase {
        desc: &'static str,
        liveness: Option<Liveness>,
        want: WantOpen,
    }

    /// Pins the open decision for every reading the core can produce: only a LIVE
    /// core is snapshotted, and every other input refuses without issuing the RPC.
    ///
    /// The row-count assertion is what makes the table non-vacuous: the domain is
    /// the three liveness states plus the absent core, so a row that quietly went
    /// missing cannot hide behind the three that remain.
    #[test]
    fn open_decision_table() {
        let cases = [
            OpenCase {
                desc: "a live core is snapshotted: this is the only arm that issues the RPC",
                liveness: Some(Liveness::Alive),
                want: WantOpen::Snapshot,
            },
            OpenCase {
                desc: "a wedged core refuses: blocking on it is the entire hazard",
                liveness: Some(Liveness::NotResponding),
                want: WantOpen::Refuse,
            },
            OpenCase {
                desc: "a dead core refuses: an RPC against a dead handle is not reliably fast",
                liveness: Some(Liveness::Dead),
                want: WantOpen::Refuse,
            },
            OpenCase {
                desc: "no core at all (post-shutdown) refuses: there is nothing to ask",
                liveness: None,
                want: WantOpen::Refuse,
            },
        ];

        assert_eq!(
            ALL_OPEN_INPUTS.len(),
            ALL_LIVENESS.len() + 1,
            "ALL_OPEN_INPUTS must hold one entry per liveness state plus the absent core \
             ({} + 1), but holds {}. The two consts drove apart, so the table below is no longer \
             measured against the real input domain and a whole arm of `open_decision` could go \
             unclassified.",
            ALL_LIVENESS.len(),
            ALL_OPEN_INPUTS.len()
        );
        assert_eq!(
            cases.len(),
            ALL_OPEN_INPUTS.len(),
            "the open_decision table must carry exactly one row per input `open_decision` is \
             total over: {} rows (three liveness readings plus the absent core), not {}. A missing \
             row means an arm nothing checks — and the arm most likely to be dropped is \
             NotResponding, whose refusal is the only thing that stops every later Ctrl+Shift+E \
             re-blocking the GTK main loop against a wedged Neovim.",
            ALL_OPEN_INPUTS.len(),
            cases.len()
        );

        for case in &cases {
            let got = open_decision(case.liveness);
            match case.want {
                WantOpen::Snapshot => assert!(
                    matches!(got, OpenDecision::Snapshot),
                    "case: {} — open_decision({:?}) returned {got:?}, but this input MUST be \
                     Snapshot. A refusal here makes Ctrl+Shift+E do nothing at all on a perfectly \
                     healthy editor, and prints a reason for a hazard that is not present.",
                    case.desc,
                    case.liveness
                ),
                WantOpen::Refuse => assert!(
                    matches!(got, OpenDecision::Refuse(_)),
                    "case: {} — open_decision({:?}) returned {got:?}, but this input MUST refuse. \
                     A Snapshot here issues `WolfCore::list_buffers` — a BLOCKING RPC on the GTK \
                     main thread — against a core that is known not to answer: every press of \
                     Ctrl+Shift+E then freezes the window for a full `rpc_timeout` budget, which \
                     is the exact regression this guard exists to prevent.",
                    case.desc,
                    case.liveness
                ),
            }
        }
    }

    /// The rule stated over the WHOLE domain rather than over one table row, so a
    /// failure names the rule instead of quoting an index. Mirrors
    /// `liveness_ui::tests::only_dead_auto_restarts`.
    #[test]
    fn only_a_live_core_is_snapshotted() {
        let mut snapshotting: Vec<Option<Liveness>> = Vec::new();
        for input in ALL_OPEN_INPUTS {
            if matches!(open_decision(input), OpenDecision::Snapshot) {
                snapshotting.push(input);
            }
        }

        let expected = [Some(Liveness::Alive)];
        assert_eq!(
            snapshotting.as_slice(),
            expected.as_slice(),
            "exactly ONE of the four inputs may be snapshotted — a core reading Alive. An EXTRA \
             entry means a blocking `list_buffers` is issued against a core already known to be \
             wedged or dead, so every Ctrl+Shift+E after the first stall re-freezes the main loop. \
             An entry MISSING means the picker can never open at all, since Alive is the only \
             state a healthy editor ever reports."
        );
    }

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a mid-sentence
    /// clause. Copied from `palette.rs`'s helper of the same name, including its
    /// deliberate allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    /// The three refusal reasons are non-empty, mutually DISTINCT, and read as a
    /// mid-sentence clause.
    ///
    /// Distinctness is the load-bearing half, for the same reason it is in
    /// `command_action::unimplemented_reasons_are_distinct_and_non_empty`: the
    /// reason is the whole of what a reader sees when the chord does nothing, and
    /// two arms sharing a string make "the editor is wedged" indistinguishable
    /// from "the editor has crashed" — two states with completely different
    /// routes back.
    #[test]
    fn refuse_reasons_are_distinct_and_non_empty() {
        let probes = [
            (
                "a wedged (NotResponding) core",
                Some(Liveness::NotResponding),
            ),
            ("a dead core", Some(Liveness::Dead)),
            ("an absent core, after shutdown took it out", None),
        ];

        let mut seen: HashSet<&'static str> = HashSet::new();
        for (source, liveness) in probes {
            let decision = open_decision(liveness);
            let OpenDecision::Refuse(reason) = decision else {
                panic!(
                    "{source} produced {decision:?} rather than a refusal, so this test cannot \
                     check the reason it exists to check. Either that arm now snapshots (in which \
                     case `open_decision_table` and `only_a_live_core_is_snapshotted` are also \
                     red, and one of them is the finding) or the probe no longer selects the arm \
                     it names."
                )
            };

            assert!(
                !reason.trim().is_empty(),
                "the refusal reason for {source} is empty. It is printed as \
                 \"wolf-gnome: buffer picker not opened: {{reason}}\", so an empty one makes a \
                 deliberate refusal indistinguishable from a chord that silently did nothing."
            );
            assert!(
                !is_sentence_cased(reason),
                "the refusal reason for {source} is sentence-cased ({reason:?}). It is \
                 interpolated MID-SENTENCE into the printed diagnostic, exactly as \
                 `command_action.rs`'s and `palette.rs`'s reasons are, so it must start lowercase \
                 (an ALL-CAPS proper noun such as \"GNOME\" is fine)."
            );
            assert!(
                seen.insert(reason),
                "the refusal reason for {source} ({reason:?}) duplicates another arm's reason. \
                 The reasons must be DISTINCT so the printed line names the real cause: a wedged \
                 Neovim is recoverable and a dead one is not, and an absent core means the window \
                 is already tearing down. One shared string collapses all three into a message \
                 that cannot be acted on."
            );
        }
    }

    // =====================================================================
    // initial_selection
    // =====================================================================

    /// One `initial_selection` table row.
    struct InitialCase {
        desc: &'static str,
        n_rows: usize,
        expected: Option<usize>,
    }

    /// Which row the FIRST refresh after `open` preselects.
    ///
    /// This is the one selection rule the three overlays do NOT share, and the
    /// reason is the whole point of a buffer picker: row 0 is the buffer the user
    /// is already in, so a bare `Enter` on the empty-query open must switch to the
    /// most-recently-used OTHER buffer. The macOS `BufferPickerView` does exactly
    /// `min(1, rows.count - 1)` behind its own first-open flag, and this is that
    /// port.
    #[test]
    fn initial_selection_table() {
        let cases = [
            InitialCase {
                desc: "no rows => nothing to preselect, and Enter must be a true no-op",
                n_rows: 0,
                expected: None,
            },
            InitialCase {
                desc: "exactly one row (only the current buffer) => that row, clamped",
                n_rows: 1,
                expected: Some(0),
            },
            InitialCase {
                desc: "two rows => row 1, the most-recently-used OTHER buffer",
                n_rows: 2,
                expected: Some(1),
            },
            InitialCase {
                desc: "five rows => still row 1; it is min(1, n-1), never a deeper index",
                n_rows: 5,
                expected: Some(1),
            },
        ];

        for case in &cases {
            assert_eq!(
                initial_selection(case.n_rows),
                case.expected,
                "case: {} — initial_selection({}) must be {:?}. With zero rows a Some here makes \
                 the first refresh select into an empty ListBox; with rows present a None leaves \
                 the overlay open with nothing selected, so a bare Enter does nothing.",
                case.desc,
                case.n_rows,
                case.expected
            );
        }

        // The rule stated once, over every list long enough for it to bite. This is
        // what a "simplification" to `restored_index(true, None, n)` — always row 0
        // — would break, and it is the difference between a telescope-style
        // buffer switcher and a chord that reopens the buffer you are already in.
        for n_rows in 2..=8 {
            assert_ne!(
                initial_selection(n_rows),
                Some(0),
                "initial_selection({n_rows}) preselected row 0. Row 0 is the buffer the user is \
                 ALREADY in (rank_buffers sorts by lastused DESC), so a bare Enter would switch \
                 to the current buffer and the chord would appear to do nothing at all. Whenever \
                 there are two or more rows the preselection must be row 1."
            );
            assert_eq!(
                initial_selection(n_rows),
                Some(1),
                "initial_selection({n_rows}) must be Some(1) for every list of two or more rows: \
                 the rule is min(1, n - 1), so the count past two changes nothing."
            );
        }
    }

    // =====================================================================
    // switch_target
    // =====================================================================

    /// The `bufnr`s of the fixture rows [`SWITCH_CASES`] ranks over.
    ///
    /// Deliberately NON-CONTIGUOUS, and no entry equals its own row index: a
    /// `switch_target` that returned the row INDEX rather than the row's `bufnr`
    /// would otherwise satisfy the in-range row of the table.
    const FIXTURE_BUFNRS: [i64; 3] = [7, 12, 30];

    /// The `active_win` handle every non-degenerate row passes in.
    ///
    /// Not `0` (which `switch_target` must refuse) and not equal to any
    /// [`FIXTURE_BUFNRS`] entry, so a result that swapped the tuple's two halves
    /// fails the table instead of passing it.
    const FIXTURE_WIN: i64 = 1000;

    /// Builds `n` fixture rows carrying the first `n` [`FIXTURE_BUFNRS`].
    ///
    /// The displays are `fixture0.md`, `fixture1.md`, … — never anything a query
    /// in this module ranks against, since `switch_target` does no ranking of its
    /// own and must not be given a reason to look as though it does.
    fn fixture_rows(n: usize) -> Vec<BufferRow> {
        assert!(
            n <= FIXTURE_BUFNRS.len(),
            "a switch_target case asked for {n} fixture rows, but FIXTURE_BUFNRS names only {}. \
             Add another distinct bufnr rather than reusing one: two rows sharing a bufnr would \
             make `switch_target_only_returns_a_listed_buffer` unable to tell them apart, so a \
             function returning the WRONG row would still satisfy it.",
            FIXTURE_BUFNRS.len()
        );
        FIXTURE_BUFNRS
            .iter()
            .take(n)
            .enumerate()
            .map(|(index, bufnr)| BufferRow {
                bufnr: *bufnr,
                display: format!("fixture{index}.md"),
                modified: false,
            })
            .collect()
    }

    /// One `switch_target` table row. `n_rows` is materialized through
    /// [`fixture_rows`] so the table can stay a `const` item that two tests walk
    /// (the `liveness_ui::tests::REOPEN_CASES` shape).
    struct SwitchCase {
        desc: &'static str,
        n_rows: usize,
        selected: Option<i32>,
        active_win: Option<i64>,
        expected: Option<(i64, i64)>,
    }

    /// What `Enter` does, as data. Walked by [`switch_target_table`] for the exact
    /// answers and by [`switch_target_only_returns_a_listed_buffer`] for the one
    /// property that must hold across all of them.
    const SWITCH_CASES: &[SwitchCase] = &[
        // --- Zero rows: a TRUE no-op, whatever the ListBox claims -----------
        SwitchCase {
            desc: "zero rows, nothing selected => no switch at all",
            n_rows: 0,
            selected: None,
            active_win: Some(FIXTURE_WIN),
            expected: None,
        },
        SwitchCase {
            desc: "zero rows, but a stale selection index => still no switch",
            n_rows: 0,
            selected: Some(0),
            active_win: Some(FIXTURE_WIN),
            expected: None,
        },
        // --- Rows present: the activation index rule ------------------------
        SwitchCase {
            desc: "rows but no selection => the TOP row (type a query, press Enter)",
            n_rows: 3,
            selected: None,
            active_win: Some(FIXTURE_WIN),
            expected: Some((FIXTURE_WIN, FIXTURE_BUFNRS[0])),
        },
        SwitchCase {
            desc: "GTK's -1 'row not in a list' index => the TOP row",
            n_rows: 3,
            selected: Some(-1),
            active_win: Some(FIXTURE_WIN),
            expected: Some((FIXTURE_WIN, FIXTURE_BUFNRS[0])),
        },
        SwitchCase {
            desc: "a selection in range => THAT row's buffer, not the top one",
            n_rows: 3,
            selected: Some(2),
            active_win: Some(FIXTURE_WIN),
            expected: Some((FIXTURE_WIN, FIXTURE_BUFNRS[2])),
        },
        SwitchCase {
            desc: "a selection past the end => the TOP row, never out of bounds",
            n_rows: 3,
            selected: Some(9),
            active_win: Some(FIXTURE_WIN),
            expected: Some((FIXTURE_WIN, FIXTURE_BUFNRS[0])),
        },
        // --- The active window, which is wolf-ffi's rule --------------------
        SwitchCase {
            desc: "no active window in the snapshot => nothing to switch",
            n_rows: 3,
            selected: Some(1),
            active_win: None,
            expected: None,
        },
        SwitchCase {
            desc: "an active window of 0 => refused, exactly as wolf-ffi refuses it",
            n_rows: 3,
            selected: Some(1),
            active_win: Some(0),
            expected: None,
        },
    ];

    /// Pins what `Enter` resolves to for every shape the widget layer can hand in.
    #[test]
    fn switch_target_table() {
        for case in SWITCH_CASES {
            let rows = fixture_rows(case.n_rows);
            let got = switch_target(&rows, case.selected, case.active_win);
            assert_eq!(
                got, case.expected,
                "case: {} — switch_target(<{} row(s)>, selected = {:?}, active_win = {:?}) \
                 returned {got:?}, expected {:?}. With ZERO rows a Some here sends an \
                 InputEvent::SwitchBuffer for a buffer that is not on the list. With an \
                 active_win of None or 0 a Some here is worse: wolf-ffi resolves \
                 `active_win.unwrap_or(0)` and no-ops on 0 precisely because a window handle of \
                 0 would bind the switch into whichever window Neovim happens to consider \
                 current, so the user's buffer would be replaced in a pane they never selected.",
                case.desc, case.n_rows, case.selected, case.active_win, case.expected
            );
        }
    }

    /// The properties that must hold across every row of [`SWITCH_CASES`]: a switch
    /// is only ever issued into the window that was passed in, for a buffer that is
    /// actually IN the ranked list, and for exactly the row the selection resolves
    /// to through [`activation_index`].
    ///
    /// The resolved-row equality is the load-bearing assertion, because membership
    /// is measurably too weak on its own: a `switch_target` returning
    /// `rows.first()`'s bufnr for every input satisfies membership on every row of
    /// the table — the top row is a listed buffer like any other — while activating
    /// the wrong buffer for every selection below the top. Equality against
    /// `rows[activation_index(selected, rows.len())]` is what refuses that, and the
    /// `nonzero_resolved` guard is what stops the equality holding vacuously over a
    /// table whose switching rows all resolve to row 0.
    ///
    /// Stated over the whole domain rather than per row so a failure names the rule
    /// instead of an index. It shares its inputs with [`switch_target_table`] but
    /// not its oracle: the table's expectations are hand-written data, while these
    /// are derived from the resolution rule [`switch_target`] documents itself as
    /// using, so a table row whose `expected` is written wrong is red here.
    #[test]
    fn switch_target_only_returns_a_listed_buffer() {
        let mut switches = 0_usize;
        let mut nonzero_resolved = 0_usize;

        for case in SWITCH_CASES {
            let rows = fixture_rows(case.n_rows);
            let Some((win, bufnr)) = switch_target(&rows, case.selected, case.active_win) else {
                continue;
            };
            switches += 1;

            assert_eq!(
                Some(win),
                case.active_win,
                "case: {} — switch_target returned window {win}, which is not the active_win \
                 {:?} it was given. The window is passed through, never derived: a different \
                 handle here means the switch lands in some other pane.",
                case.desc,
                case.active_win
            );
            assert_ne!(
                win, 0,
                "case: {} — switch_target returned window 0. wolf-ffi's `switch_buffer` no-ops \
                 on 0, so this would be a silent do-nothing at best; if that guard were ever \
                 relaxed, an InputEvent::SwitchBuffer with win = 0 binds into whichever window \
                 Neovim considers current.",
                case.desc
            );
            assert!(
                rows.iter().any(|row| row.bufnr == bufnr),
                "case: {} — switch_target returned bufnr {bufnr}, which is NOT one of the {} \
                 row(s) on offer ({:?}). The activation must resolve to a buffer the user could \
                 see and select; anything else switches the pane to a buffer that was never in \
                 the list.",
                case.desc,
                rows.len(),
                rows.iter().map(|row| row.bufnr).collect::<Vec<_>>()
            );

            let resolved = activation_index(case.selected, rows.len()).unwrap_or_else(|| {
                panic!(
                    "case: {} — switch_target returned a switch to bufnr {bufnr}, but \
                     activation_index({:?}, {}) resolves to NO row at all. Enter must be a true \
                     no-op whenever there is no row to activate, so a switch here targets a \
                     buffer the picker never put on screen.",
                    case.desc,
                    case.selected,
                    rows.len()
                )
            });
            if resolved != 0 {
                nonzero_resolved += 1;
            }
            assert_eq!(
                bufnr,
                rows[resolved].bufnr,
                "case: {} — switch_target returned bufnr {bufnr}, but the selection {:?} resolves \
                 through activation_index to row {resolved} ({:?}), whose bufnr is {}. Enter \
                 therefore activates the WRONG row: the row the user sees highlighted is not the \
                 buffer loaded into the pane. Membership in {:?} does NOT catch this — returning \
                 the top row's bufnr for every input satisfies the membership assertion above \
                 while getting every non-top selection wrong — so this must stay an equality \
                 against the RESOLVED row.",
                case.desc,
                case.selected,
                rows[resolved].display,
                rows[resolved].bufnr,
                rows.iter().map(|row| row.bufnr).collect::<Vec<_>>()
            );
        }

        assert!(
            switches > 0,
            "NOT ONE row of SWITCH_CASES produced a switch, so every assertion in this test ran \
             zero times and it proves nothing. Either `switch_target` now refuses every input — \
             in which case Enter can never switch a buffer and `switch_target_table` is red too — \
             or the table lost all of its non-degenerate rows."
        );
        assert!(
            nonzero_resolved > 0,
            "every SWITCH_CASES row that produced a switch resolved to row 0, so the \
             resolved-row equality never had to tell the selected row apart from the top one and \
             a `switch_target` returning `rows.first()`'s bufnr unconditionally passes this whole \
             test. At least one row must pair an IN-RANGE, NON-ZERO `selected` with a usable \
             active_win: `selected: Some(2)` over three rows is the row carrying that. Either \
             that row left the table, or `switch_target` has stopped switching for anything but \
             the top row — which is the regression itself, and `switch_target_table` is red too."
        );
    }

    // =====================================================================
    // row_label
    // =====================================================================

    /// One `row_label` table row.
    struct LabelCase {
        desc: &'static str,
        display: &'static str,
        modified: bool,
    }

    /// The label a row draws: the ranked `display`, plus [`MODIFIED_SUFFIX`] when
    /// and only when the buffer is modified.
    ///
    /// The suffix is DISPLAY-ONLY and is never part of the string the query is
    /// ranked against — `rank_buffers` ranks `BufferRow::display`, computed in
    /// core — so the expectations below are always the display plus a suffix,
    /// never a relabelled display.
    #[test]
    fn row_label_table() {
        let cases = [
            LabelCase {
                desc: "an unmodified file row is its display, verbatim",
                display: "notes/todo.md",
                modified: false,
            },
            LabelCase {
                desc: "a modified file row carries the marker",
                display: "notes/todo.md",
                modified: true,
            },
            LabelCase {
                desc: "the unnamed-buffer placeholder passes through unchanged",
                display: NO_NAME,
                modified: false,
            },
            LabelCase {
                desc: "the unnamed-buffer placeholder still takes the marker when modified",
                display: NO_NAME,
                modified: true,
            },
        ];

        // Without this the two halves of the table would be indistinguishable: an
        // empty suffix makes `modified: true` and `modified: false` produce the
        // same string, and every row below would pass with the marker deleted.
        assert!(
            !MODIFIED_SUFFIX.is_empty(),
            "MODIFIED_SUFFIX is empty, so a modified buffer's row is drawn exactly like an \
             unmodified one — the user has no way to see unsaved work from the picker — and every \
             row of this table would pass with the suffix logic deleted entirely."
        );

        for case in &cases {
            let row = BufferRow {
                bufnr: FIXTURE_BUFNRS[0],
                display: case.display.to_string(),
                modified: case.modified,
            };
            let got = row_label(&row);
            let expected = if case.modified {
                format!("{}{MODIFIED_SUFFIX}", case.display)
            } else {
                case.display.to_string()
            };

            assert_eq!(
                got, expected,
                "case: {} — row_label for display {:?} (modified = {}) produced {got:?}, expected \
                 {expected:?}. The label must START with the ranked display and add nothing but \
                 the modified marker: a row whose text is not the display the query was matched \
                 against reads as though the picker selected a different buffer.",
                case.desc, case.display, case.modified
            );
            assert!(
                got.starts_with(case.display),
                "case: {} — row_label produced {got:?}, which does not begin with the row's own \
                 display {:?}. The suffix is appended; it never replaces or reformats the display.",
                case.desc,
                case.display
            );
            assert_eq!(
                got.contains(MODIFIED_SUFFIX),
                case.modified,
                "case: {} — row_label produced {got:?}; the modified marker {MODIFIED_SUFFIX:?} \
                 must be present for a modified buffer and ABSENT otherwise. A marker on every row \
                 is the same as no marker at all.",
                case.desc
            );
        }
    }

    // =====================================================================
    // The cage smoke's premise, made executable in the fast suite
    // =====================================================================

    /// The buffer-picker cage smoke's TARGET file (its `BUFPICK_FILE_TARGET`) —
    /// the one `BUFPICK_QUERY` must select, opened FIRST in the scenario.
    ///
    /// The four consts below are duplicated from `tests/smoke_headless.rs` rather
    /// than shared, because that file is a separate crate — the
    /// `palette.rs::SMOKE_QUERY` precedent.
    /// [`buffer_rows_for_the_smoke_query_select_exactly_the_target`] is what keeps
    /// the two from drifting: it proves in the FAST suite that this query still
    /// selects exactly this row out of exactly this buffer list, so a ranking or
    /// ordering change reddens `cargo test -p wolf-gnome` in seconds instead of
    /// making a fifteen-second cage smoke stochastic.
    const SMOKE_TARGET: &str = "bpzulu.md";

    /// The smoke's PRESELECT file (its `BUFPICK_FILE_PRESELECT`), opened SECOND —
    /// so it is row 1 of the empty-query list and therefore the row
    /// [`initial_selection`] picks. A picker that ignored the query entirely would
    /// activate this one.
    const SMOKE_PRESELECT: &str = "bpyankee.md";

    /// The smoke's CURRENT file (its `BUFPICK_FILE_CURRENT`), opened THIRD — the
    /// buffer showing when the chord fires, and therefore row 0.
    const SMOKE_CURRENT: &str = "bpxray.md";

    /// The startup buffer the captive Neovim always has open. Not a smoke const:
    /// it is here so the fixture is the FOUR-buffer list the scenario really
    /// produces rather than a three-buffer idealisation.
    const SMOKE_STARTUP: &str = "Untitled.md";

    /// The query the smoke types into the buffer picker's search entry (its
    /// `BUFPICK_QUERY`).
    const SMOKE_QUERY: &str = "bpzulu";

    /// A stand-in for the smoke's temp working directory, so `rank_buffers`
    /// projects each absolute buffer name down to its basename exactly as it does
    /// under `cage`.
    const SMOKE_CWD: &str = "/w/project";

    /// The four buffers the smoke's captive Neovim holds when the chord fires, in
    /// the order the scenario `:e`s them.
    ///
    /// `lastused` is a WHOLE-SECOND unix timestamp, so the fixture gives the four
    /// buffers distinct, increasing seconds. That is what the scenario's `MRU_GAP`
    /// sleep buys: three `:e`s inside one second would all tie, `rank_buffers`
    /// would fall back to `bufnr` ASC, and the order would INVERT — which is
    /// exactly what the smoke's readback premise exists to catch at run time and
    /// what this fixture models here.
    fn smoke_buffers() -> Vec<BufInfo> {
        [
            (1_i64, SMOKE_STARTUP, 1_000_i64),
            (2, SMOKE_TARGET, 1_002),
            (3, SMOKE_PRESELECT, 1_004),
            (4, SMOKE_CURRENT, 1_006),
        ]
        .into_iter()
        .map(|(bufnr, name, lastused)| BufInfo {
            bufnr,
            name: format!("{SMOKE_CWD}/{name}"),
            lastused,
            modified: false,
        })
        .collect()
    }

    /// The cage smoke's premise, made executable: with the scenario's four buffers
    /// in the scenario's MRU order, the empty-query list opens on the CURRENT file
    /// with the PRESELECT file at row 1, the target is neither of those, and the
    /// typed query narrows the list to exactly the target.
    ///
    /// Every clause is load-bearing for the smoke's discriminator. The smoke
    /// asserts that the target's heading is on screen and the preselect's is not.
    /// If the target were row 0 or row 1, a picker that ignored the query entirely
    /// would still activate it and the smoke would pass forever without the
    /// filtering ever working; if the query matched two rows, which one `Enter`
    /// activated would depend on ranking tie-breaks.
    #[test]
    fn buffer_rows_for_the_smoke_query_select_exactly_the_target() {
        let buffers = smoke_buffers();
        let cwd = Path::new(SMOKE_CWD);

        let target_bufnr = buffers
            .iter()
            .find(|buf| buf.name.ends_with(SMOKE_TARGET))
            .map(|buf| buf.bufnr)
            .unwrap_or_else(|| {
                panic!(
                    "the fixture holds no buffer named {SMOKE_TARGET:?}, so this test cannot check \
                     the smoke premise it exists to check. `smoke_buffers` and the SMOKE_* consts \
                     drifted apart."
                )
            });

        // --- The unfiltered list: the MRU order the smoke's readback asserts ---
        let unfiltered = rank_buffers(&buffers, Some(cwd), "", BUFFER_MAX_ROWS);
        let displays: Vec<&str> = unfiltered.iter().map(|row| row.display.as_str()).collect();

        assert!(
            unfiltered.len() > 1,
            "the empty-query list holds {} row(s), so there is nothing for the query to narrow \
             DOWN to and the \"exactly one row\" assertion below would hold even with the ranking \
             deleted. Rows were: {displays:?}",
            unfiltered.len()
        );
        assert_eq!(
            unfiltered.len(),
            buffers.len(),
            "the empty query returned {} row(s) for {} listed buffers. `rank_buffers` must return \
             ALL buffers MRU-first for an empty query — the buffer the user is currently in \
             included — so a short list here means candidates are being dropped before the picker \
             ever ranks them, and the smoke's premise is measured against a different list than \
             the one the app shows. Rows were: {displays:?}",
            unfiltered.len(),
            buffers.len()
        );
        assert_eq!(
            displays.first().copied(),
            Some(SMOKE_CURRENT),
            "row 0 of the empty-query list is {:?}, not the CURRENT file {SMOKE_CURRENT:?}. \
             `rank_buffers` sorts by lastused DESC, so the buffer opened last must be row 0. If \
             this fails, the sort order changed (or the fixture's timestamps stopped increasing in \
             scenario order) and the smoke's readback premise would fail under cage for the same \
             reason. Rows were: {displays:?}",
            displays.first()
        );
        assert_eq!(
            displays.get(1).copied(),
            Some(SMOKE_PRESELECT),
            "row 1 of the empty-query list is {:?}, not the PRESELECT file {SMOKE_PRESELECT:?}. \
             Row 1 is the row `initial_selection` preselects, so it is the row a picker that \
             ignored the typed query would activate — the smoke's must-NOT-contain assertion is \
             written against exactly this file's heading. Rows were: {displays:?}",
            displays.get(1)
        );
        assert_eq!(
            initial_selection(unfiltered.len()),
            Some(1),
            "`initial_selection` does not preselect row 1 for a {}-row list, so the row a \
             query-ignoring picker would activate is no longer {SMOKE_PRESELECT:?} and the cage \
             smoke's must-NOT-contain term names the wrong file.",
            unfiltered.len()
        );

        let target_index = displays
            .iter()
            .position(|display| *display == SMOKE_TARGET)
            .unwrap_or_else(|| {
                panic!(
                    "the empty-query list does not contain the target {SMOKE_TARGET:?} at all, so \
                     the query below could not select it however well the ranking worked. Rows \
                     were: {displays:?}"
                )
            });
        assert!(
            target_index >= 2,
            "the target {SMOKE_TARGET:?} is at index {target_index} of the empty-query list, but \
             it must be at index 2 or later. At index 0 or 1 it is the row `initial_selection` \
             would preselect, so a picker that IGNORED the typed query entirely would still \
             activate it and the cage smoke would pass permanently while proving nothing about \
             filtering. Rows were: {displays:?}"
        );

        // --- The query itself -------------------------------------------------
        let ranked = rank_buffers(&buffers, Some(cwd), SMOKE_QUERY, BUFFER_MAX_ROWS);
        let ranked_displays: Vec<&str> = ranked.iter().map(|row| row.display.as_str()).collect();

        assert_eq!(
            ranked.len(),
            1,
            "`rank_buffers` returned {} row(s) for the smoke's query {SMOKE_QUERY:?}, not exactly \
             one. `rank_matches` keeps every entry the query is a case-insensitive SUBSEQUENCE of, \
             so a second match makes which buffer `Enter` activates depend on ranking tie-breaks \
             and the cage smoke stochastic rather than red. Rows were: {ranked_displays:?}",
            ranked.len()
        );
        assert_eq!(
            ranked_displays.first().copied(),
            Some(SMOKE_TARGET),
            "the sole row matching {SMOKE_QUERY:?} is {:?}, not the target {SMOKE_TARGET:?}. The \
             smoke asserts that file's HEADING is on screen after Enter, so a different row here \
             means the smoke's positive assertion names a file the picker would never open.",
            ranked_displays.first()
        );
        assert_eq!(
            ranked[0].bufnr, target_bufnr,
            "the sole row matching {SMOKE_QUERY:?} carries bufnr {}, not the target's {target_bufnr}. \
             `switch_target` sends exactly this number to Neovim, so a mismatch switches the pane \
             to some other buffer while the row on screen still reads {SMOKE_TARGET:?}.",
            ranked[0].bufnr
        );
    }
}
