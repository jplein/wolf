//! The GNOME file-picker UI for `wolf-gnome`.
//!
//! Trigger: **Ctrl+Shift+P** (a non-conflicting shortcut — every Neovim key,
//! including `<C-p>`, is preserved) plus a header-bar button. While the picker is
//! open, all keys are routed to the picker widget (forwarding to nvim/core is
//! suppressed); `Esc` closes it; `Enter` / row-activation sends
//! `InputEvent::Open { path }` (MRU is auto-recorded by the core input pump) and
//! closes.
//!
//! Almost all of this module is GTK widget wiring (an `Overlay` over the
//! `DrawingArea`, a `SearchEntry` + `ListBox` fed by `wolf_core::start_file_search`
//! / `rank_matches`) which cannot be unit-tested headlessly. The ONE pure,
//! display-free seam is [`is_picker_trigger`], which decides whether a given GDK
//! keyval + modifier state is the picker trigger. It is unit-tested below exactly
//! like `gdk_key` / `cursor` — `gdk::Key` constants are compile-time and no display
//! server is required.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::glib;

use wolf_core::{rank_matches, FileSearch, InputEvent, WakeFn, WolfCore};

/// The maximum number of ranked rows shown in the picker list. The streaming
/// traversal can collect far more entries than are useful to display, so only the
/// top results (after ranking) are materialized into `ListBox` rows on each
/// refresh.
const PICKER_MAX_ROWS: usize = 200;

/// Capacity of the picker's wake channel. One slot is enough: it coalesces a burst
/// of traversal-batch wakes into a single queued refresh (a dropped "full" send
/// just means a refresh is already pending), mirroring the app's main wake channel.
const PICKER_WAKE_CHANNEL_CAP: usize = 1;

/// Returns `true` iff `keyval` + `state` is the picker trigger chord
/// **Ctrl+Shift+P**.
///
/// The rule is `CONTROL && SHIFT && letter == 'p'`, case-insensitive on the letter
/// (with SHIFT held the compositor delivers the uppercase `P` keyval; without an
/// explicit shift glyph it may deliver `p`, so both are accepted). Other modifiers
/// (e.g. ALT, SUPER) are ignored — they neither satisfy nor disqualify the
/// trigger. This is a pure, display-free predicate, testable headlessly like
/// [`crate::gdk_key`].
#[must_use]
pub fn is_picker_trigger(keyval: gdk::Key, state: gdk::ModifierType) -> bool {
    if !(state.contains(gdk::ModifierType::CONTROL_MASK)
        && state.contains(gdk::ModifierType::SHIFT_MASK))
    {
        return false;
    }
    keyval
        .to_unicode()
        .map(|c| c.to_ascii_lowercase())
        .is_some_and(|c| c == 'p')
}

/// Computes the next list selection index after moving by `delta` rows from
/// `current`, clamped into `[0, n_rows - 1]` with NO wrap. Returns `None` when the
/// list is empty (`n_rows <= 0`).
///
/// The formula is exactly `clamp(current + delta, 0, n_rows - 1)`: an out-of-range
/// or negative `current` is handled by the same clamp on the sum (e.g.
/// `next_selection(-5, 1, 5) == Some(0)`, `next_selection(99, 1, 5) == Some(4)`).
/// Pure and display-free, unit-tested below.
#[must_use]
fn next_selection(current: i32, delta: i32, n_rows: i32) -> Option<usize> {
    if n_rows <= 0 {
        None
    } else {
        Some(usize::try_from((current + delta).clamp(0, n_rows - 1)).unwrap_or(0))
    }
}

/// Chooses which row index to select after a refresh rebuild. Returns `None` for an
/// empty list (`new_len == 0`); otherwise `Some(0)` when resetting to the top (query
/// changed / first open), else the preserved `prev` index (defaulting to `0`)
/// clamped into the new range. Pure and display-free, unit-tested below.
#[must_use]
fn restored_index(reset_to_top: bool, prev: Option<usize>, new_len: usize) -> Option<usize> {
    if new_len == 0 {
        None
    } else if reset_to_top {
        Some(0)
    } else {
        Some(prev.unwrap_or(0).min(new_len - 1))
    }
}

/// The picker's shared, interior-mutable state. All access happens on the single
/// GTK main thread, so `Cell`/`RefCell` (not a `Mutex`) is sufficient.
struct PickerInner {
    /// Whether the picker is currently open (its widgets visible and focused).
    open: Cell<bool>,
    /// The live file-search session while open; `None` when closed. Dropping it
    /// cancels the background traversal.
    search: RefCell<Option<FileSearch>>,
    /// Absolute paths parallel to the `ListBox` rows (row index -> path to open).
    row_paths: RefCell<Vec<PathBuf>>,
    /// The overlay child holding the search entry + results list.
    picker_box: gtk::Box,
    /// The search entry whose text filters/ranks the results.
    entry: gtk::SearchEntry,
    /// The results list, rebuilt on each refresh.
    list: gtk::ListBox,
    /// The editor drawing area, re-focused when the picker closes.
    area: gtk::DrawingArea,
    /// The live core, shared with the rest of the app.
    core: Rc<RefCell<Option<WolfCore>>>,
    /// The wake closure handed to `start_file_search`; firing it queues a refresh.
    wake: WakeFn,
    /// The query string used by the previous [`PickerInner::refresh`]. It decides
    /// whether the next refresh resets the selection to the top (query changed, or
    /// first refresh after open) or PRESERVES the user's current selection across a
    /// streaming traversal rebuild. `None` forces a reset and is set on `open`.
    last_query: RefCell<Option<String>>,
}

impl PickerInner {
    /// Opens the picker: starts a file search, shows + focuses the entry, and does
    /// an initial refresh so the MRU seed appears immediately. No-op if already
    /// open or if the core is gone.
    fn open(&self) {
        if self.open.get() {
            return;
        }
        let search = {
            let core_ref = self.core.borrow();
            let Some(core) = core_ref.as_ref() else {
                return;
            };
            core.start_file_search(self.wake.clone())
        };
        *self.search.borrow_mut() = Some(search);
        self.open.set(true);
        self.picker_box.set_visible(true);
        self.entry.set_text("");
        self.entry.grab_focus();
        // Force the first refresh to select the top row, regardless of any stale
        // selection/query left over from a previous session.
        *self.last_query.borrow_mut() = None;
        self.refresh();
    }

    /// Closes the picker: drops the file search (cancelling traversal), hides the
    /// widgets, and returns focus to the editor. No-op if already closed.
    fn close(&self) {
        if !self.open.get() {
            return;
        }
        self.open.set(false);
        // Take + drop the FileSearch to cancel the background traversal.
        let _ = self.search.borrow_mut().take();
        self.picker_box.set_visible(false);
        self.area.grab_focus();
    }

    /// Rebuilds the results list for the current query from the latest search
    /// snapshot. No-op when closed. Keeps the snapshot read guard's scope short.
    ///
    /// Selection handling: if the query is unchanged since the previous refresh
    /// (i.e. this is a streaming traversal wake, not a keystroke), the user's
    /// current selection index is PRESERVED across the rebuild (clamped to the new
    /// row count) so an incoming batch does not yank the selection back to the top.
    /// When the query changed (typing) or it is the first refresh after `open`, the
    /// selection resets to the top row so the best match is pre-selected.
    fn refresh(&self) {
        if !self.open.get() {
            return;
        }
        let query = self.entry.text().to_string();

        // Decide reset-vs-preserve, then record this query for the next refresh.
        let reset_to_top = self.last_query.borrow().as_deref() != Some(query.as_str());
        *self.last_query.borrow_mut() = Some(query.clone());

        // Capture the currently-selected index BEFORE the rebuild so we can
        // restore it when preserving the user's selection.
        let prev_selected = self.list.selected_row().map(|row| row.index());

        // Clear the previous rows + parallel path list.
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let mut paths = self.row_paths.borrow_mut();
        paths.clear();

        let search = self.search.borrow();
        let Some(fs) = search.as_ref() else {
            return;
        };
        let view = fs.read();
        let matches = rank_matches(&query, &view.entries);
        for m in matches.iter().take(PICKER_MAX_ROWS) {
            let entry = &view.entries[m.index];
            let label = gtk::Label::builder()
                .label(&entry.display)
                .halign(gtk::Align::Start)
                .build();
            let row = gtk::ListBoxRow::new();
            row.set_child(Some(&label));
            self.list.append(&row);
            paths.push(entry.path.clone());
        }
        let new_len = paths.len();
        drop(view);
        drop(search);
        drop(paths);

        // Choose the selection: top row on a query change / first open, else the
        // preserved index clamped into the new range. (Empty list => nothing to
        // select.)
        let prev_index = prev_selected.and_then(|i| usize::try_from(i).ok());
        if let Some(idx) = restored_index(reset_to_top, prev_index, new_len) {
            if let Some(row) = self.list.row_at_index(i32::try_from(idx).unwrap_or(0)) {
                self.list.select_row(Some(&row));
            }
        }
    }

    /// Moves the LIST selection by `delta` rows (clamped to `[0, n_rows - 1]`,
    /// no wrap), leaving keyboard focus on the search entry so typing continues to
    /// work. Bound to Up (`-1`) / Down (`+1`). No-op when there are no rows.
    fn move_selection(&self, delta: i32) {
        let n_rows = i32::try_from(self.row_paths.borrow().len()).unwrap_or(0);
        let current = self.list.selected_row().map_or(0, |row| row.index());
        if let Some(next) = next_selection(current, delta, n_rows) {
            if let Some(row) = self.list.row_at_index(i32::try_from(next).unwrap_or(0)) {
                self.list.select_row(Some(&row));
            }
        }
    }

    /// Resolves the selected row (or the first row if none is selected), sends
    /// `InputEvent::Open { path }` to the core, then closes the picker. With ZERO
    /// rows it is a true no-op: it neither sends `Open` nor closes the picker.
    fn activate_selection(&self) {
        let path = {
            let paths = self.row_paths.borrow();
            if paths.is_empty() {
                return;
            }
            let idx = self
                .list
                .selected_row()
                .map_or(0, |row| usize::try_from(row.index()).unwrap_or(0));
            paths.get(idx).or_else(|| paths.first()).cloned()
        };
        let Some(path) = path else {
            return;
        };
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_input(InputEvent::Open { path });
        }
        self.close();
    }
}

/// A handle to the built picker, used by the app to drive it (header-bar button,
/// trigger chord, close-on-shutdown) and to query whether it currently owns the
/// keyboard (so key forwarding to the core can be suppressed).
#[derive(Clone)]
pub struct PickerHandle {
    inner: Rc<PickerInner>,
}

impl PickerHandle {
    /// Opens the picker (idempotent).
    pub fn open(&self) {
        self.inner.open();
    }

    /// Closes the picker (idempotent), cancelling any in-flight traversal.
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

/// Builds the picker widgets as an overlay child over `area` (the overlay's main
/// child), wires all signals, and returns a [`PickerHandle`]. The picker starts
/// hidden; call [`PickerHandle::open`] (via the trigger chord or the header-bar
/// button) to show it.
#[must_use]
pub fn build(
    overlay: &gtk::Overlay,
    area: &gtk::DrawingArea,
    core: &Rc<RefCell<Option<WolfCore>>>,
) -> PickerHandle {
    let entry = gtk::SearchEntry::new();
    entry.set_hexpand(true);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);

    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();

    let picker_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    picker_box.add_css_class("background");
    picker_box.set_halign(gtk::Align::Center);
    picker_box.set_valign(gtk::Align::Start);
    picker_box.set_margin_top(PICKER_BOX_MARGIN);
    picker_box.set_width_request(PICKER_BOX_WIDTH);
    picker_box.set_height_request(PICKER_BOX_HEIGHT);
    picker_box.append(&entry);
    picker_box.append(&scroller);
    picker_box.set_visible(false);

    overlay.add_overlay(&picker_box);

    // Wake plumbing: traversal batches fire `picker_wake` from a tokio worker
    // (Send + Sync); it only pushes onto the channel. A dropped "full" send is
    // correct (a refresh is already pending). DROP-SAFE: ignore try_send errors so
    // a late cancel-path wake after the channel closes is harmless.
    let (wake_tx, wake_rx) = async_channel::bounded::<()>(PICKER_WAKE_CHANNEL_CAP);
    let wake: WakeFn = Arc::new(move || {
        let _ = wake_tx.try_send(());
    });

    let inner = Rc::new(PickerInner {
        open: Cell::new(false),
        search: RefCell::new(None),
        row_paths: RefCell::new(Vec::new()),
        picker_box,
        entry: entry.clone(),
        list: list.clone(),
        area: area.clone(),
        core: core.clone(),
        wake,
        last_query: RefCell::new(None),
    });

    // Drain the wake channel on the GTK main thread, refreshing the (open) picker
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

    // Re-rank on every query change.
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

    // Clicking / activating a row opens it.
    {
        let inner_weak = Rc::downgrade(&inner);
        list.connect_row_activated(move |_list, _row| {
            if let Some(inner) = inner_weak.upgrade() {
                inner.activate_selection();
            }
        });
    }

    // Picker key handling (Capture phase so the SearchEntry does not swallow these
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
        inner.picker_box.add_controller(key);
    }

    PickerHandle { inner }
}

/// Top margin (px) of the floating picker box within the overlay.
const PICKER_BOX_MARGIN: i32 = 24;
/// Width (px) of the floating picker box.
const PICKER_BOX_WIDTH: i32 = 600;
/// Height (px) of the floating picker box.
const PICKER_BOX_HEIGHT: i32 = 400;

#[cfg(test)]
mod tests {
    use super::*;

    use gdk::glib::translate::from_glib;
    use gdk::ModifierType;

    /// Builds the GDK keyval for a Unicode scalar exactly as GDK would from a real
    /// keypress (gdk4-rs has no `Key::from_unicode`). Copied from
    /// `tests/gdk_key.rs` so the trigger predicate is exercised with the same
    /// keyval shape a live `EventControllerKey` callback receives — layout
    /// independent, since the glyph is carried directly in the keyval.
    fn key_from_char(c: char) -> gdk::Key {
        unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) }
    }

    /// One `is_picker_trigger` table row: (description, keyval, modifier state,
    /// expected result).
    struct Case {
        desc: &'static str,
        keyval: gdk::Key,
        state: ModifierType,
        expected: bool,
    }

    #[test]
    fn is_picker_trigger_table() {
        let ctrl = ModifierType::CONTROL_MASK;
        let shift = ModifierType::SHIFT_MASK;
        let alt = ModifierType::ALT_MASK;

        let cases = [
            // --- The trigger: Ctrl+Shift held, letter p ----------------------
            Case {
                desc: "Ctrl+Shift+'p' (lowercase keyval) => true",
                keyval: key_from_char('p'),
                state: ctrl | shift,
                expected: true,
            },
            Case {
                // With SHIFT held the compositor delivers the uppercase `P`
                // keyval; the predicate must match the letter case-insensitively.
                desc: "Ctrl+Shift+'P' (uppercase keyval) => true",
                keyval: key_from_char('P'),
                state: ctrl | shift,
                expected: true,
            },
            Case {
                // Extra ALT must NOT disqualify the trigger: the rule is
                // CONTROL && SHIFT && letter==p, ignoring other modifiers.
                desc: "Ctrl+Shift+Alt+'P' (extra mod) => true",
                keyval: key_from_char('P'),
                state: ctrl | shift | alt,
                expected: true,
            },
            // --- Missing a required modifier ---------------------------------
            Case {
                desc: "Ctrl+'p' (no shift) => false",
                keyval: key_from_char('p'),
                state: ctrl,
                expected: false,
            },
            Case {
                desc: "Shift+'P' (no ctrl) => false",
                keyval: key_from_char('P'),
                state: shift,
                expected: false,
            },
            Case {
                desc: "plain 'p' (no modifiers) => false",
                keyval: key_from_char('p'),
                state: ModifierType::empty(),
                expected: false,
            },
            // --- Right modifiers, wrong letter -------------------------------
            Case {
                desc: "Ctrl+Shift+'A' (wrong letter) => false",
                keyval: key_from_char('A'),
                state: ctrl | shift,
                expected: false,
            },
            Case {
                desc: "Ctrl+Shift+'a' (wrong letter, lowercase) => false",
                keyval: key_from_char('a'),
                state: ctrl | shift,
                expected: false,
            },
        ];

        for case in &cases {
            let got = is_picker_trigger(case.keyval, case.state);
            assert_eq!(got, case.expected, "case: {}", case.desc);
        }
    }

    /// One `next_selection` table row:
    /// (description, current, delta, n_rows, expected).
    struct NextCase {
        desc: &'static str,
        current: i32,
        delta: i32,
        n_rows: i32,
        expected: Option<usize>,
    }

    // NOTE to coder: these tests assume `next_selection` computes exactly
    //   if n_rows <= 0 { None } else { Some(clamp(current + delta, 0, n_rows - 1)) }
    // i.e. clamp the *sum* (current + delta) into [0, n_rows - 1] with NO wrap.
    // For a negative/out-of-range `current` the same formula applies (the sum is
    // clamped), so e.g. next_selection(-5, 1, 5) == Some(0) and
    // next_selection(99, 1, 5) == Some(4).
    #[test]
    fn next_selection_table() {
        let n: i32 = 5;
        let cases = [
            // --- move down within range ---------------------------------------
            NextCase {
                desc: "down from top stays in range",
                current: 0,
                delta: 1,
                n_rows: n,
                expected: Some(1),
            },
            NextCase {
                desc: "down to last row",
                current: 3,
                delta: 1,
                n_rows: n,
                expected: Some(4),
            },
            // --- move up within range -----------------------------------------
            NextCase {
                desc: "up within range",
                current: 3,
                delta: -1,
                n_rows: n,
                expected: Some(2),
            },
            // --- clamp at the ends (NO wrap) ----------------------------------
            NextCase {
                desc: "down at bottom clamps (no wrap)",
                current: 4,
                delta: 1,
                n_rows: n,
                expected: Some(4),
            },
            NextCase {
                desc: "up at top clamps (no wrap)",
                current: 0,
                delta: -1,
                n_rows: n,
                expected: Some(0),
            },
            // --- empty list ---------------------------------------------------
            NextCase {
                desc: "empty list, down => None",
                current: 0,
                delta: 1,
                n_rows: 0,
                expected: None,
            },
            NextCase {
                desc: "empty list, up => None",
                current: 0,
                delta: -1,
                n_rows: 0,
                expected: None,
            },
            // --- single row ---------------------------------------------------
            NextCase {
                desc: "single row, down clamps to 0",
                current: 0,
                delta: 1,
                n_rows: 1,
                expected: Some(0),
            },
            NextCase {
                desc: "single row, up clamps to 0",
                current: 0,
                delta: -1,
                n_rows: 1,
                expected: Some(0),
            },
            // --- current out of range / negative ------------------------------
            NextCase {
                desc: "current above range clamps to last (sum clamped)",
                current: 99,
                delta: 1,
                n_rows: n,
                expected: Some(4),
            },
            NextCase {
                desc: "current negative clamps to 0 (sum clamped)",
                current: -5,
                delta: 1,
                n_rows: n,
                expected: Some(0),
            },
        ];

        for case in &cases {
            let got = next_selection(case.current, case.delta, case.n_rows);
            assert_eq!(got, case.expected, "case: {}", case.desc);
        }
    }

    /// One `restored_index` table row:
    /// (description, reset_to_top, prev, new_len, expected).
    struct RestoredCase {
        desc: &'static str,
        reset_to_top: bool,
        prev: Option<usize>,
        new_len: usize,
        expected: Option<usize>,
    }

    #[test]
    fn restored_index_table() {
        const LEN: usize = 5;
        let cases = [
            // --- empty list always None, regardless of reset/prev -------------
            RestoredCase {
                desc: "empty list with reset => None",
                reset_to_top: true,
                prev: Some(3),
                new_len: 0,
                expected: None,
            },
            RestoredCase {
                desc: "empty list, preserve => None",
                reset_to_top: false,
                prev: Some(3),
                new_len: 0,
                expected: None,
            },
            // --- reset to top -------------------------------------------------
            RestoredCase {
                desc: "reset with prev set => Some(0)",
                reset_to_top: true,
                prev: Some(3),
                new_len: LEN,
                expected: Some(0),
            },
            RestoredCase {
                desc: "reset with no prev => Some(0)",
                reset_to_top: true,
                prev: None,
                new_len: LEN,
                expected: Some(0),
            },
            // --- preserve previous selection ----------------------------------
            RestoredCase {
                desc: "preserve in range",
                reset_to_top: false,
                prev: Some(3),
                new_len: LEN,
                expected: Some(3),
            },
            RestoredCase {
                desc: "preserve clamped when prev beyond new len",
                reset_to_top: false,
                prev: Some(9),
                new_len: LEN,
                expected: Some(4),
            },
            RestoredCase {
                desc: "preserve with no prev defaults to 0",
                reset_to_top: false,
                prev: None,
                new_len: LEN,
                expected: Some(0),
            },
        ];

        for case in &cases {
            let got = restored_index(case.reset_to_top, case.prev, case.new_len);
            assert_eq!(got, case.expected, "case: {}", case.desc);
        }
    }
}
