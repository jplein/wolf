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
//! [`wolf_core::COMMAND_SPECS`] plus three snapshots taken at open — the command
//! MRU, the recent directories and the other open windows. Neither store is
//! written except by an activation, and an activation closes the palette, so
//! re-filtering per keystroke is a synchronous, IO-free call and the picker's
//! `async-channel` + wake-drain plumbing would be dead weight here. Do not add it
//! back for symmetry. (Both stores are global and shared with any other Wolf
//! process, so a concurrent one can write either while the palette is up; the list
//! shows the snapshot it opened with. So can the window list change under it, for
//! the same reason and with the same answer.)
//!
//! ## Which row kinds are reachable
//!
//! All THREE core row kinds are produced. `rank_palette` is fed a real
//! recent-directories slice — [`WolfCore::recent_dirs`] put through
//! [`open_recent_candidates`] — and a real window slice, the coordinator's
//! [`crate::coordinator::Coordinator::palette_windows`] for every window EXCEPT
//! the one this palette belongs to. So [`PaletteRowKind::Static`],
//! [`PaletteRowKind::SwitchWindow`] and [`PaletteRowKind::OpenRecent`] rows all
//! appear.
//!
//! One directory can therefore produce TWO rows — "Switch to Window: /proj/x" and
//! "Open Recent Folder: /proj/x" — and that is the honest surface rather than a
//! duplicate: the first raises the window already editing that project, the second
//! re-roots THIS window onto it. [`open_recent_candidates`] refuses only the
//! invoking window's own directory, and refuses it for a reason a second window
//! does not change.
//!
//! A `SwitchWindow` row carrying no window index is still
//! [`PaletteAction::Unsupported`] and fails [`palette_rows`]' filter rather than
//! appearing as a dead entry. `rank_palette` always sets one, so that arm is a
//! shape guard on core's contract, not a case this crate produces.
//!
//! ## The Open Recent label is the ranked string
//!
//! An Open Recent row is shown as [`wolf_core::PaletteRow::display`] gives it —
//! the full absolute path, verbatim — so the string ranked against the query and
//! the string on screen are one by construction. [`crate::title`]'s home
//! abbreviation is deliberately NOT applied here: `rank_matches` is a subsequence
//! matcher over `display`, so a `~/…` label over a `/home/…` ranked string returns
//! nothing for a character the user can plainly see. The width is paid in the
//! widget layer instead, with `xalign` plus a Pango ellipsis, exactly as
//! [`crate::search_overlay`] pays it.
//!
//! Opening costs two synchronous SQLite reads (the command MRU and the recent
//! directories), one `live_cwd` lock read, one `stat` per recorded directory
//! through [`open_recent_candidates`]' injected predicate — bounded by the store's
//! own cap — and one walk of the coordinator's window registry. All of it happens
//! at open, never per keystroke, and none of it is an RPC to Neovim, so this
//! module adds no `block_on_rpc` route at all (see
//! `crates/wolf-gnome/DECISIONS.md`, "Crash and stall recovery"). The window walk
//! in particular reads each window's already-cached persisted subtree, so no other
//! window's core is touched.
//!
//! Almost all of this module is GTK widget wiring, which cannot be unit-tested
//! headlessly. Its pure, display-free seams are [`open_recent_candidates`],
//! [`palette_rows`] and [`palette_action_for`], unit-tested below; selection
//! movement and activation reuse [`crate::picker`]'s [`next_selection`],
//! [`restored_index`] and [`activation_index`], so every overlay in this crate
//! navigates identically.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use wolf_core::{rank_palette, Command, PaletteRow, PaletteRowKind, PaletteWindow, WolfCore};

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
/// A named alias rather than an inline `RefCell<Option<Rc<dyn Fn(PaletteAction)>>>`:
/// the inline spelling trips `clippy::type_complexity`, and the `Rc` is what lets
/// [`PaletteInner::activate_selection`] clone the handler OUT of the `RefCell`
/// before calling it.
type ActivateHandler = Rc<dyn Fn(PaletteAction)>;

/// Where a palette's window rows come from: the coordinator's live registry, minus
/// the window this palette belongs to.
///
/// A named alias for the same reason [`ActivateHandler`] is one — the inline
/// spelling trips `clippy::type_complexity` — and a closure rather than a slice
/// because the answer must be taken fresh at each open: windows come and go while
/// the palette is closed.
pub type WindowSource = Rc<dyn Fn() -> Vec<PaletteWindow>>;

/// What activating one palette row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    /// Dispatch this registry command through the shell's [`UiAction`] funnel.
    Dispatch(Command),
    /// Raise the window with this id, leaving THIS window's project alone.
    ///
    /// The payload is the coordinator's `WindowId`, which is what
    /// `wolf_core::PaletteWindow::index` carries, so the palette needs no second
    /// numbering. Best-effort on Wayland: presenting a backgrounded toplevel is the
    /// compositor's decision, and `cage` declines it.
    SwitchWindow(u16),
    /// Re-root THIS window at this directory. The payload is a directory and no
    /// window target: the row acts on the window whose palette it was picked in,
    /// and raising a different one is [`PaletteAction::SwitchWindow`]'s row.
    /// `shell::AppShell::on_palette_activation` is the one place that directory
    /// becomes a window action.
    OpenRecentFolder(PathBuf),
    /// `wolf-gnome` cannot perform this row; the string names why, as a
    /// mid-sentence clause (matching [`crate::command_action`]'s convention).
    Unsupported(&'static str),
}

/// The recent directories this palette offers, keeping `recorded`'s newest-first
/// order among the survivors.
///
/// `recorded` is [`WolfCore::recent_dirs`] verbatim. `live_cwd` is the window's own
/// working directory ([`WolfCore::live_cwd`]), and `is_dir` is injected rather than
/// called, in the house style of [`crate::workspace::launch_decision`], so the whole
/// rule is decidable with no temp directory and no filesystem race. Production
/// passes [`std::path::Path::is_dir`].
///
/// A directory is kept exactly when all three hold, and each refusal is a row this
/// front-end would otherwise list with nothing useful behind it:
///
/// * `Some(dir) != live_cwd` — the INVOKING window's own working directory is
///   refused, because this row re-roots that window and re-rooting it where it
///   already is changes nothing. It is also the commonest entry in the store, so
///   without this the top folder row is usually the dead one. A second window
///   editing that directory does not revive the row: raising that window is
///   [`PaletteAction::SwitchWindow`]'s job, and re-rooting THIS window onto
///   another's project would put two Neovims on one swap directory for a raise the
///   user could have asked for directly.
/// * `is_dir(dir)` — a directory that has gone is refused, because a failed `:tcd`
///   is only a `tracing::warn!` in [`wolf_core::input`]: Neovim never reports
///   `DirChanged`, so `shell`'s pending re-root never settles and the front-end
///   cell stays wedged until the next successful chdir. A stale row is worse than a
///   no-op.
/// * `dir.is_absolute()` — a relative path is refused, because
///   `shell::AppShell::chdir_to` canonicalizes against the application process's
///   working directory rather than Neovim's, so one could only be a corrupt store
///   row.
///
/// The store itself is never pruned. It lives at `<state_dir>/folder-mru.sqlite`
/// and is shared with the macOS front-end, so deleting a temporarily unmounted
/// network path would permanently forget it for both; filtering on read lets it
/// return.
///
/// Limitation: a directory that exists but is unreadable (mode `000`) still passes
/// `is_dir` and is still offered.
#[must_use]
pub fn open_recent_candidates(
    recorded: &[PathBuf],
    live_cwd: Option<&Path>,
    is_dir: impl Fn(&Path) -> bool,
) -> Vec<PathBuf> {
    recorded
        .iter()
        .filter(|dir| {
            let dir = dir.as_path();
            dir.is_absolute() && is_dir(dir) && Some(dir) != live_cwd
        })
        .cloned()
        .collect()
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
///
/// `window_index` is [`PaletteRow::window_index`] and `path` is
/// [`PaletteRow::path`], each of which core sets on exactly one row kind. A kind
/// that does not read one IGNORES it rather than refusing it, so the
/// classification never depends on a field its rows do not carry — the arguments
/// are a projection of the row, in the row's own field order.
#[must_use]
pub fn palette_action_for(
    kind: PaletteRowKind,
    command: Option<Command>,
    window_index: Option<u16>,
    path: Option<&Path>,
) -> PaletteAction {
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
        PaletteRowKind::SwitchWindow => match window_index {
            Some(index) => PaletteAction::SwitchWindow(index),
            None => PaletteAction::Unsupported(
                "a switch-window palette row carrying no window index has no window to raise",
            ),
        },
        PaletteRowKind::OpenRecent => match path {
            Some(path) => PaletteAction::OpenRecentFolder(path.to_path_buf()),
            None => PaletteAction::Unsupported(
                "an open-recent palette row carrying no folder path has nothing to change the \
                 working directory to",
            ),
        },
    }
}

/// The palette's candidate rows for `query`, ranked best-first and filtered to the
/// rows this front-end can actually perform, capped at `max`.
///
/// `recent_dirs` is [`open_recent_candidates`]' output, so every folder row offered
/// here is one an activation can act on. `windows` is every OTHER live window (the
/// caller excludes its own), so every switch row offered here has a window to
/// raise. `rank_palette` excludes [`Command::CommandPalette`] itself, so the
/// palette cannot dispatch itself.
///
/// **Ranking is unbounded and `max` is applied AFTER the filter**, deliberately.
/// [`rank_palette`] truncates to its own `max` before this crate's classification
/// filter can run, so passing `max` through would let deferred rows eat the budget
/// and hand the user a short — or empty — list.
#[must_use]
pub fn palette_rows(
    recent_dirs: &[PathBuf],
    windows: &[PaletteWindow],
    command_mru_recent: &[String],
    query: &str,
    max: usize,
) -> Vec<PaletteRow> {
    rank_palette(recent_dirs, windows, command_mru_recent, query, usize::MAX)
        .into_iter()
        .filter(|row| {
            !matches!(
                palette_action_for(row.kind, row.command, row.window_index, row.path.as_deref()),
                PaletteAction::Unsupported(_)
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
    /// The recent directories offered as Open Recent rows, snapshotted ONCE at
    /// [`PaletteInner::open`] through [`open_recent_candidates`], for the same
    /// reason [`Self::mru_recent`] is. `folder-mru.sqlite` is a GLOBAL store: a
    /// second Wolf process can write it while the palette is up, and the list keeps
    /// showing the snapshot it opened with until the next open.
    recent_dirs: RefCell<Vec<PathBuf>>,
    /// The OTHER live windows offered as Switch to Window rows, snapshotted ONCE at
    /// [`PaletteInner::open`] from [`Self::window_source`]. Taken at open for the
    /// same reason the two above are: a window opened or closed while this palette
    /// is up is not worth a registry walk per keystroke, and the raise is
    /// best-effort anyway — a row for a window that has since closed raises
    /// nothing.
    windows: RefCell<Vec<PaletteWindow>>,
    /// Where [`Self::windows`] comes from: the coordinator's registry minus this
    /// window.
    window_source: WindowSource,
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
    /// Opens the palette: snapshots the command MRU, the recent directories and the
    /// other live windows, shows + focuses the entry, and does an initial refresh so
    /// the MRU-first list appears immediately. No-op if already open.
    ///
    /// Unlike [`crate::picker`]'s `open` this does NOT early-return when the core is
    /// gone: the registry rows are a function of `COMMAND_SPECS`, not of the core.
    /// With no core there is no MRU ordering and no folder row, and the palette
    /// still opens on the registry.
    fn open(&self) {
        if self.open.get() {
            return;
        }
        // ONE short core borrow, in its own statement, yielding everything the two
        // snapshots need. `WolfCore::recent_dirs` is a SQLite `SELECT` and runs
        // INSIDE this borrow deliberately: it cannot re-enter the `RefCell` (it
        // touches neither GTK nor this widget), and splitting it out would mean a
        // second borrow of the same cell for no gain. `shell::capture_open_files`
        // is the precedent. `WolfCore::live_cwd` is a tracked-cell read, not an RPC.
        let (mru, recorded, live) = {
            let core_ref = self.core.borrow();
            match core_ref.as_ref() {
                Some(core) => (
                    Some(core.command_mru()),
                    core.recent_dirs(),
                    core.live_cwd(),
                ),
                None => (None, Vec::new(), None),
            }
        };
        let recent = mru.map(|mru| mru.recent()).unwrap_or_default();
        *self.mru_recent.borrow_mut() = recent;
        *self.recent_dirs.borrow_mut() =
            open_recent_candidates(&recorded, live.as_deref(), Path::is_dir);
        // The third snapshot, and the cheapest: a walk of the coordinator's window
        // registry reading each window's already-cached subtree. Called with no
        // borrow of this cell live, because it re-enters the coordinator.
        let windows = (self.window_source)();
        *self.windows.borrow_mut() = windows;

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
    /// in the same place — see [DECISIONS.md](../DECISIONS.md), "Window structure",
    /// for why focus never rests on an ephemeral widget.
    ///
    /// Deleting this call left the command-palette smoke green across three runs.
    /// That is the measurement; the mechanism behind it was not established, and no
    /// explanation for it should be written here without one. It licenses nothing
    /// elsewhere: the focus returns in [`crate::picker`]'s `close`, and in
    /// `pane_view`'s `rebuild_pane_area` and `on_pane_clicked`, are proven
    /// load-bearing by a smoke that reddens when they are removed.
    fn close(&self) {
        if !self.open.get() {
            return;
        }
        self.open.set(false);
        self.palette_box.set_visible(false);
        self.area.grab_focus();
    }

    /// Rebuilds the results list for the current query from the two snapshots taken
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

        // All three snapshot borrows are held for the ranking call and nothing else.
        let ranked = {
            let dirs = self.recent_dirs.borrow();
            let windows = self.windows.borrow();
            let recent = self.mru_recent.borrow();
            palette_rows(
                dirs.as_slice(),
                windows.as_slice(),
                recent.as_slice(),
                &query,
                PALETTE_MAX_ROWS,
            )
        };

        for row in &ranked {
            // An Open Recent row's label is a full absolute path, so it can be
            // wider than the box: left-align it and let Pango ellipsize the TAIL
            // rather than rewriting the string, which would come apart from the
            // string `rank_matches` ranked. `search_overlay`'s result rows are
            // built the same way for the same reason.
            let label = gtk::Label::builder()
                .label(&row.display)
                .halign(gtk::Align::Start)
                .xalign(0.0)
                .ellipsize(pango::EllipsizeMode::End)
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
    /// RECORDS its MRU key, CLOSES the palette, then hands the row's
    /// [`PaletteAction`] to the shell's handler. With ZERO rows it is a true no-op.
    ///
    /// **That order is load-bearing and diverges from macOS's record → dispatch →
    /// close.** A dispatched `CloseWindow` / `QuitApplication` synchronously runs
    /// `window::wire_close`'s handler, which takes the core out and tears the window
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
                        palette_action_for(
                            row.kind,
                            row.command,
                            row.window_index,
                            row.path.as_deref(),
                        ),
                    )
                })
        };
        let Some((mru_key, action)) = chosen else {
            return;
        };

        // Unreachable while `palette_rows`' filter stands, but never silent — and
        // refused BEFORE the record, so an unperformable row leaves no MRU trace.
        if let PaletteAction::Unsupported(reason) = action {
            eprintln!("wolf-gnome: palette row {mru_key:?} not activated: {reason}");
            return;
        }

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
            handler(action);
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
    /// The shell wires this to `AppShell::on_palette_activation`, whose `Dispatch`
    /// arm is the same funnel a command drained from Neovim takes, so the two entry
    /// paths cannot classify a command differently.
    pub fn connect_activate<F: Fn(PaletteAction) + 'static>(&self, handler: F) {
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
///
/// `windows` is asked for the OTHER live windows on each open. It is a closure
/// rather than a value because this palette outlives any particular set of
/// windows, and it must hold the coordinator only WEAKLY: the coordinator owns
/// every window, a window owns its overlays, and an overlay owning the coordinator
/// back would leak the entire graph.
#[must_use]
pub fn build(
    overlay: &gtk::Overlay,
    area: &gtk::DrawingArea,
    core: &Rc<RefCell<Option<WolfCore>>>,
    windows: WindowSource,
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
        recent_dirs: RefCell::new(Vec::new()),
        windows: RefCell::new(Vec::new()),
        window_source: windows,
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
    use std::path::{Path, PathBuf};

    use wolf_core::{rank_palette, Command, CommandSpec, PaletteRowKind, COMMAND_SPECS};

    use crate::command_action::{ui_action_for, UiAction};

    /// The query the cage smoke `command_palette_chord_runs_the_selected_command_under_cage`
    /// types into the palette's search entry (its `PALETTE_QUERY`).
    ///
    /// Duplicated here rather than shared because `tests/smoke_headless.rs` is a
    /// separate crate; [`palette_rows_query_new_ranks_the_new_buffer_row_first`] is
    /// what keeps the two from drifting, by proving in the FAST suite that this
    /// query still puts the New row at the TOP of the list. It matches more than
    /// one row, and no query can match fewer — see that test.
    const SMOKE_QUERY: &str = "new";

    /// The MRU key that same smoke asserts the activation recorded (its
    /// `PALETTE_MRU_KEY`). Checked against the registry rather than trusted.
    const SMOKE_MRU_KEY: &str = "new_buffer";

    /// The query the cage smoke
    /// `two_windows_two_neovims_and_two_saved_sessions_under_cage` types into the
    /// palette to open its second window (its `TWOWIN_NEW_WINDOW_QUERY`).
    ///
    /// TWO terms, and the second is what does the work: `rank_matches` requires
    /// every whitespace-separated term to be a subsequence of the row's label, and
    /// `window` is not a subsequence of `New` — so this query drops the row that
    /// [`SMOKE_QUERY`] selects rather than merely out-ranking it.
    /// [`the_new_window_palette_query_selects_the_new_window_row`] is where that is
    /// asserted, and it is the measurement the smoke's own copy of this literal
    /// comes from.
    ///
    /// The smoke carries its own copy of the literal for the reason [`SMOKE_QUERY`]
    /// does — `tests/smoke_headless.rs` is a separate crate — and that test is the
    /// join that keeps the two from drifting.
    const NEW_WINDOW_PALETTE_QUERY: &str = "new window";

    /// The query the cage smoke `open_recent_palette_row_reroots_the_window_under_cage`
    /// types into the palette's search entry (its `OPENRECENT_QUERY`, which that file
    /// defines AS its `OPENRECENT_TARGET_DIR` so the two cannot drift there).
    ///
    /// Duplicated here for the same reason [`SMOKE_QUERY`] is — `tests/smoke_headless.rs`
    /// is a separate crate and cannot import these — and
    /// [`palette_rows_query_orzulu_selects_exactly_the_recent_folder_row`] is the join
    /// that keeps the two files honest.
    const SMOKE_RECENT_QUERY: &str = "orzulu";

    /// The prefix [`rank_palette`] builds an Open Recent row's opaque MRU key from,
    /// duplicated from the same smoke's assertion 4 (`open_recent:<path>`).
    ///
    /// The shape is core's, not this crate's — see the note on
    /// [`palette_rows_query_orzulu_selects_exactly_the_recent_folder_row`] for why
    /// that makes the key assertion a JOIN rather than a discriminator.
    const SMOKE_RECENT_MRU_PREFIX: &str = "open_recent:";

    /// One query the two markdown-preview cage smokes type into the palette, with
    /// everything about it those smokes cannot check for themselves.
    ///
    /// A struct rather than a four-field tuple because two of the four are `usize`
    /// and sit next to each other, which is exactly the shape a later edit transposes
    /// silently — and a swapped floor and row count would leave both numbers looking
    /// plausible.
    struct PreviewSmokeQuery {
        /// The literal typed into the palette's search entry.
        query: &'static str,
        /// The command the query's TOP row must carry.
        command: Command,
        /// That command's canonical registry id, which the smokes read back out of
        /// the command MRU.
        id: &'static str,
        /// The smallest number of candidates [`rank_palette`] may match for the query
        /// BEFORE this crate's classification filter runs.
        unfiltered_floor: usize,
        /// How many ACTIONABLE rows the palette shows for it — the number a user sees
        /// and the number Enter is pressed against.
        actionable_rows: usize,
    }

    /// The two queries the cage smokes
    /// `preview_pane_renders_and_edit_source_returns_under_cage` and
    /// `preview_of_an_unsaved_buffer_renders_under_cage` type into the palette (their
    /// `PREVIEW_QUERY` and `EDIT_QUERY`).
    ///
    /// Duplicated here for the reason [`SMOKE_QUERY`] is, and joined to the smokes by
    /// [`palette_rows_for_the_preview_smoke_rank_the_named_command_first`] — which is
    /// also where the `prv` collision and its tie-break are pinned.
    ///
    /// The two rows differ in both numbers, and the difference is the point. `prv` is
    /// a subsequence of `Preview` AND of `Preview in Browser`, and both commands are
    /// DISPATCHED, so the classification filter removes neither: the query really
    /// shows two rows and the smokes depend on the RANKING rather than on uniqueness.
    /// The floor of 2 states that the collision is still there, which is what keeps
    /// "the top row carries `Command::Preview`" from silently becoming a claim about a
    /// query that matches one title. `edts` matches exactly one title in the whole
    /// registry, filtered or not, so claiming either of those of it would be false.
    const PREVIEW_SMOKE_QUERIES: [PreviewSmokeQuery; 2] = [
        PreviewSmokeQuery {
            query: "prv",
            command: Command::Preview,
            id: "preview",
            unfiltered_floor: 2,
            actionable_rows: 2,
        },
        PreviewSmokeQuery {
            query: "edts",
            command: Command::Edit,
            id: "edit",
            unfiltered_floor: 1,
            actionable_rows: 1,
        },
    ];

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
        /// An exact `PaletteAction::SwitchWindow(<this window id>)`.
        SwitchWindow(u16),
        /// An exact `PaletteAction::OpenRecentFolder(<this path>)`.
        OpenRecentFolder(&'static str),
        /// `PaletteAction::Unsupported(_)` — the palette must not offer this row.
        Unsupported,
    }

    /// One `palette_action_for` table row.
    struct PaletteCase {
        desc: &'static str,
        kind: PaletteRowKind,
        command: Option<Command>,
        /// The row's kind-specific raise target, as `PaletteRow::window_index`
        /// carries it. `Some` on a `SwitchWindow` row; deliberately also exercised
        /// on the two kinds that must IGNORE it.
        window_index: Option<u16>,
        /// The row's kind-specific folder path, as `PaletteRow::path` carries it.
        /// `Some` on an `OpenRecent` row; deliberately also exercised on the two
        /// kinds that must IGNORE it.
        path: Option<&'static str>,
        want: WantAction,
    }

    /// One [`open_recent_candidates`] table row.
    ///
    /// `dirs` is the injected `is_dir` SET: the closure the case builds answers true
    /// for exactly these paths and false for everything else. Injecting it is what
    /// makes the "the directory has gone" rule decidable with no temp directory and
    /// no filesystem race, in the house style of `workspace::launch_decision`.
    struct RecentCase {
        desc: &'static str,
        recorded: &'static [&'static str],
        live: Option<&'static str>,
        dirs: &'static [&'static str],
        want: &'static [&'static str],
    }

    /// [`open_recent_candidates`] keeps a recorded directory exactly when it is
    /// absolute, still a directory, and not the window's live working directory —
    /// and keeps the store's newest-first order among the survivors.
    ///
    /// ## What each refusal buys, and what its absence costs
    ///
    /// * **The window's own working directory.** This row acts on the INVOKING
    ///   window and re-roots it, so for the directory that window is already rooted
    ///   at it changes nothing. Two consequences follow and neither is the reason:
    ///   the full sidebar re-traversal would be wasted, and whether Neovim fires
    ///   `DirChanged` for a same-directory `:tcd` is unestablished, which this design
    ///   avoids needing to know. It is also the commonest entry in the store, so
    ///   without this refusal the palette's top folder row is usually the dead one. A
    ///   SECOND window editing that directory does not revive it either — raising
    ///   that window is [`PaletteAction::SwitchWindow`]'s row, and re-rooting this
    ///   one onto another's project would put two Neovims on one swap directory.
    ///   [`open_recent_targets_the_invoking_window`] is where that shape is named.
    /// * **A directory that has gone.** `RecentDirs::recent` deliberately does not
    ///   prune vanished paths, and a failed `:tcd` in `wolf_core::input` degrades to
    ///   a `tracing::warn!` — Neovim never fires `DirChanged`, so `shell`'s
    ///   `pending_reroot` stays set until the next successful chdir. A stale row is
    ///   therefore worse than a no-op: it wedges a front-end cell.
    /// * **A relative path.** Nothing here resolves one against anything, so it can
    ///   only be a corrupt store row.
    ///
    /// Honest limitation, recorded rather than tested: a directory that exists but is
    /// unreadable (mode `000`) still passes `is_dir` and is still offered.
    ///
    /// ## Reverts
    ///
    /// Delete the `Some(dir) != live_cwd` conjunct and both window-cwd rows fail;
    /// delete the `is_dir(dir)` conjunct and the vanished-directory and middle-stale
    /// rows fail; delete the `dir.is_absolute()` conjunct and the relative row fails.
    ///
    /// The `assert_eq!` sits inside the `for`, so a single red run reports only the
    /// FIRST failing row of the pair — the window-cwd row rather than the
    /// trailing-separator one, the vanished row rather than the middle-stale one.
    /// Both are reached and both are covered; only the evidence from one run is
    /// partial. A per-row failure vector would fix that and would diverge from every
    /// other table test in this crate, so the shape stays.
    ///
    /// **The order-preservation row has no revert of its own, and none is to be
    /// invented.** Order is plain `Vec` iteration, so "reverting" it would mean
    /// ADDING a sort that has never existed. It is a precondition against a future
    /// "tidy this alphabetically" edit — the store's newest-first order is the only
    /// ranking an empty-query palette gives these rows — not a discriminator of
    /// anything the seam does.
    #[test]
    fn open_recent_candidates_table() {
        let cases = [
            RecentCase {
                desc: "an empty store offers nothing",
                recorded: &[],
                live: Some("/proj/live"),
                dirs: &["/proj/live"],
                want: &[],
            },
            RecentCase {
                desc: "a directory that exists and is not the window's own cwd is KEPT",
                recorded: &["/proj/alpha"],
                live: Some("/proj/live"),
                dirs: &["/proj/alpha", "/proj/live"],
                want: &["/proj/alpha"],
            },
            RecentCase {
                desc: "the window's own working directory is excluded",
                recorded: &["/proj/live"],
                live: Some("/proj/live"),
                dirs: &["/proj/live"],
                want: &[],
            },
            RecentCase {
                desc: "the window's own working directory with a trailing separator is still it",
                recorded: &["/proj/live/"],
                live: Some("/proj/live"),
                dirs: &["/proj/live"],
                want: &[],
            },
            RecentCase {
                desc: "a recorded path `is_dir` rejects (deleted, or now a file) is excluded",
                recorded: &["/proj/vanished"],
                live: Some("/proj/live"),
                dirs: &["/proj/live"],
                want: &[],
            },
            RecentCase {
                desc: "a relative path is excluded even though `is_dir` accepts it",
                recorded: &["relative/dir"],
                live: Some("/proj/live"),
                dirs: &["relative/dir", "/proj/live"],
                want: &[],
            },
            RecentCase {
                desc: "live_cwd None excludes nothing on that ground",
                recorded: &["/proj/alpha"],
                live: None,
                dirs: &["/proj/alpha"],
                want: &["/proj/alpha"],
            },
            RecentCase {
                desc: "a window cwd that is not in the store drops nothing",
                recorded: &["/proj/alpha", "/proj/beta"],
                live: Some("/proj/live"),
                dirs: &["/proj/alpha", "/proj/beta", "/proj/live"],
                want: &["/proj/alpha", "/proj/beta"],
            },
            RecentCase {
                desc: "three entries, middle one stale: the survivors keep their relative order",
                recorded: &["/proj/alpha", "/proj/vanished", "/proj/beta"],
                live: Some("/proj/live"),
                dirs: &["/proj/alpha", "/proj/beta", "/proj/live"],
                want: &["/proj/alpha", "/proj/beta"],
            },
        ];

        for case in &cases {
            let recorded: Vec<PathBuf> = case.recorded.iter().map(|p| PathBuf::from(*p)).collect();
            let want: Vec<PathBuf> = case.want.iter().map(|p| PathBuf::from(*p)).collect();
            let live = case.live.map(Path::new);
            let got = open_recent_candidates(&recorded, live, |candidate: &Path| {
                case.dirs.iter().any(|d| Path::new(d) == candidate)
            });

            assert_eq!(
                got, want,
                "case: {} — open_recent_candidates(recorded: {:?}, live_cwd: {:?}, is_dir true \
                 for: {:?}) returned {got:?}, expected {want:?}. An EXTRA entry means one of the \
                 three refusals is gone and the palette will list a folder row that re-roots the \
                 window to where it already is, or wedges the pending re-root on a directory that \
                 no longer exists (a failed `:tcd` is only a tracing::warn!, so the sidebar never \
                 settles). A MISSING entry means a usable directory was refused, and the user \
                 cannot reach that project from the palette at all. A REORDERED result means the \
                 store's newest-first order was not preserved, which is the only ranking these \
                 rows have under an empty query.",
                case.desc, case.recorded, case.live, case.dirs,
            );
        }
    }

    /// Open Recent acts on the window whose palette the row was picked in, with a
    /// name of its own.
    ///
    /// Three facts are what that means, and no other test states them together:
    ///
    /// 1. A recorded directory that IS the invoking window's working directory is not
    ///    offered, because re-rooting that window where it already is changes nothing.
    /// 2. A recorded directory that is NOT it classifies to
    ///    [`PaletteAction::OpenRecentFolder`], whose payload is a directory and
    ///    NOTHING ELSE — no window index, no raise-or-open choice for the shell to
    ///    make.
    /// 3. The capability that payload lacks is not missing from the palette: the same
    ///    directory, seen as ANOTHER window's, is offered as a Switch to Window row
    ///    and classifies to [`PaletteAction::SwitchWindow`], carrying the window id
    ///    and no path.
    ///
    /// Half 2's destructuring is the mechanical half of that claim: a variant that
    /// grew a window target would stop compiling here rather than quietly giving the
    /// palette a decision to make. The classification's full domain is
    /// [`palette_action_table`]'s; this asserts only the shape.
    ///
    /// **Two rows for one directory is the honest surface, not a duplicate.** Raising
    /// the window already editing a project and re-rooting THIS window onto it are
    /// different acts with different costs — the second puts a second Neovim on that
    /// project's swap directory, which is a thing to ask for deliberately rather than
    /// to be given when a raise was meant. Half 3 is where the pair is pinned
    /// together, so a later unit that "deduplicates" them has to delete an assertion
    /// that says why they are two.
    ///
    /// ## It is not an independent discriminator, and must not be dressed as one
    ///
    /// Half 1 shares [`open_recent_candidates_table`]'s revert — delete the
    /// `Some(dir) != live_cwd` conjunct and both go red together. Half 2 shares
    /// [`palette_action_table`]'s `OpenRecent` revert and half 3 its `SwitchWindow`
    /// one. What this test adds is that the shape has a greppable name and one home,
    /// not a falsifier the table does not already carry.
    #[test]
    fn open_recent_targets_the_invoking_window() {
        // The other window's id, deliberately not `0`: a row that lost its payload
        // and defaulted would then be visibly wrong rather than accidentally right.
        const OTHER_WINDOW: u16 = 4;

        let window_cwd = PathBuf::from("/proj/live");
        let other = PathBuf::from("/proj/alpha");

        // The store holds the invoking window's own working directory and one other
        // project. `is_dir` answers true for EVERYTHING, so the only thing that can
        // drop a row below is the window-cwd rule — the other two refusals are given
        // no opportunity to explain the result.
        let recorded = vec![window_cwd.clone(), other.clone()];
        let offered =
            open_recent_candidates(&recorded, Some(window_cwd.as_path()), |_: &Path| true);

        // (1) Only the other project is offered.
        assert_eq!(
            offered,
            vec![other.clone()],
            "with the invoking window rooted at {window_cwd:?}, `open_recent_candidates` offered \
             {offered:?} out of {recorded:?}. This row acts on the window whose palette it was \
             picked in and re-roots it, so for the directory that window is already rooted at it \
             does nothing at all — and it is the commonest entry in the store, so the dead row \
             would usually be the TOP one. Raising a DIFFERENT window that happens to be editing \
             this directory is not what un-hiding this row would do: that is \
             `PaletteAction::SwitchWindow`'s row, asserted in half 3, and re-rooting this window \
             onto another's project would put a second Neovim on that project's swap directory."
        );

        // (2) The offered directory classifies to an action carrying a directory and
        //     no window target. The `let … else` is the assertion's mechanical half:
        //     a variant that grew a window field stops compiling here.
        let action = palette_action_for(
            PaletteRowKind::OpenRecent,
            None,
            None,
            Some(other.as_path()),
        );
        let PaletteAction::OpenRecentFolder(target) = &action else {
            panic!(
                "an Open Recent row for {other:?} classified to {action:?}, not \
                 OpenRecentFolder. The only thing this row does is re-root the invoking window, \
                 and that is the action that says so; an Unsupported here makes every folder row \
                 fail `palette_rows`' filter and the palette offers none at all."
            )
        };
        assert_eq!(
            target, &other,
            "the Open Recent action for {other:?} carries {target:?}. The payload is a DIRECTORY \
             and nothing else — no window index, because the row's target is the window \
             `shell::on_palette_activation` was called for and there is nothing to choose. It is \
             what `AppShell::chdir_to` `:tcd`s to, so a rewritten path re-roots that window \
             somewhere the user did not pick."
        );

        // (3) The same directory, seen as ANOTHER window's, is a Switch to Window row
        //     — so hiding it from half 1 hides a dead row, not a capability.
        let windows = [PaletteWindow {
            index: OTHER_WINDOW,
            cwd: other.to_string_lossy().into_owned(),
        }];
        let rows = palette_rows(&offered, &windows, &[], "", PALETTE_MAX_ROWS);

        let switch_row = rows
            .iter()
            .find(|row| row.kind == PaletteRowKind::SwitchWindow)
            .unwrap_or_else(|| {
                panic!(
                    "with one other window rooted at {other:?}, `palette_rows` offered NO \
                     SwitchWindow row. Half 1 refuses to offer a directory as an Open Recent row \
                     on the ground that raising is SwitchWindow's job; with no switch row the \
                     directory another window is editing is unreachable from the palette \
                     altogether, and half 1's justification becomes false. Rows were: {:?}",
                    rows.iter().map(|row| &row.display).collect::<Vec<_>>()
                )
            });
        let recent_row = rows
            .iter()
            .find(|row| row.kind == PaletteRowKind::OpenRecent)
            .unwrap_or_else(|| {
                panic!(
                    "the Open Recent row for {other:?} vanished once a window was editing that \
                     same directory. The two rows are DIFFERENT acts — raise that window, or \
                     re-root this one onto its project — and dropping either is a decision to \
                     make deliberately, not a deduplication. Rows were: {:?}",
                    rows.iter().map(|row| &row.display).collect::<Vec<_>>()
                )
            });

        assert_eq!(
            palette_action_for(
                switch_row.kind,
                switch_row.command,
                switch_row.window_index,
                switch_row.path.as_deref()
            ),
            PaletteAction::SwitchWindow(OTHER_WINDOW),
            "the Switch to Window row {:?} does not classify to SwitchWindow({OTHER_WINDOW}). An \
             Unsupported makes it fail `palette_rows`' filter, so the row disappears and the only \
             thing the palette offers for {other:?} is the Open Recent row that re-roots THIS \
             window — the choice half 1 says the user has is then not there.",
            switch_row.display
        );
        assert_ne!(
            switch_row.display, recent_row.display,
            "the Switch to Window row and the Open Recent row for {other:?} render identically \
             ({:?}). They perform different acts on different windows, so a user picking between \
             them by sight cannot: `rank_palette` prefixes each with what it does, and a label \
             that dropped the prefix would leave the palette showing one directory twice with no \
             way to tell which row raises and which re-roots.",
            switch_row.display
        );
    }

    /// A row is actionable exactly when its command classifies to a real
    /// [`UiAction`], it is a `SwitchWindow` row carrying a window index, or it is an
    /// `OpenRecent` row carrying a folder path; every other
    /// `(kind, command, window_index, path)` tuple is `Unsupported`.
    ///
    /// The regression this pins is a palette row that prints "not dispatched" when
    /// the user presses Enter — the one surface in this crate that would list a
    /// no-op, which `actions.rs`'s `menu_never_lists_a_reserved_action` and
    /// `shell::install_actions` both refuse to do.
    ///
    /// The two payload columns are exercised on all three kinds deliberately, not
    /// only on the kind that reads each: a `Static` row that started honouring a
    /// path, or an `OpenRecent` row that started honouring a window index, would
    /// make the classification depend on a field core sets only for another kind,
    /// and nothing else in this crate would notice. The arguments are a PROJECTION
    /// of the row, in the row's own field order.
    ///
    /// The `Some(0)` row is not decoration: `0` is the first id the coordinator
    /// mints, so it is the id the very first window holds, and an implementation
    /// that reached for `unwrap_or_default()` or treated a zero index as "no window"
    /// would make the palette refuse to raise exactly that window while every other
    /// row worked.
    ///
    /// FALSIFYING EDIT: restore `PaletteRowKind::SwitchWindow => Unsupported(…)`.
    /// PREDICTED: the assertion sits inside the loop, so the run reports the first
    /// failing row — `SwitchWindow + a window index`, expecting `SwitchWindow(7)`.
    /// PREDICTED NOT ISOLATED: [`open_recent_targets_the_invoking_window`]'s third
    /// half and [`palette_rows_offer_one_switch_row_per_other_window`] redden beside
    /// it, both because every switch row then fails [`palette_rows`]' filter and
    /// disappears from the list.
    /// [`palette_unsupported_reasons_are_distinct_and_non_empty`] stays GREEN
    /// through it, and deliberately: its probe passes no window index, so it
    /// exercises the arm that is `Unsupported` either way.
    #[test]
    fn palette_action_table() {
        let undispatched = undispatched_probe();

        let cases = [
            PaletteCase {
                desc: "Static + NewBuffer: dispatched, so the row runs the command",
                kind: PaletteRowKind::Static,
                command: Some(Command::NewBuffer),
                window_index: None,
                path: None,
                want: WantAction::Dispatch(Command::NewBuffer),
            },
            PaletteCase {
                desc: "Static + NewBuffer + a folder path: the path is IGNORED",
                kind: PaletteRowKind::Static,
                command: Some(Command::NewBuffer),
                window_index: None,
                path: Some("/proj/alpha"),
                want: WantAction::Dispatch(Command::NewBuffer),
            },
            PaletteCase {
                desc: "Static + NewBuffer + a window index: the index is IGNORED",
                kind: PaletteRowKind::Static,
                command: Some(Command::NewBuffer),
                window_index: Some(3),
                path: None,
                want: WantAction::Dispatch(Command::NewBuffer),
            },
            PaletteCase {
                desc: "Static + GoToFile: dispatched (it opens the file picker)",
                kind: PaletteRowKind::Static,
                command: Some(Command::GoToFile),
                window_index: None,
                path: None,
                want: WantAction::Dispatch(Command::GoToFile),
            },
            PaletteCase {
                desc: "Static + Copy: dispatched, so the row runs the clipboard yank",
                kind: PaletteRowKind::Static,
                command: Some(Command::Copy),
                window_index: None,
                path: None,
                want: WantAction::Dispatch(Command::Copy),
            },
            // No HARDCODED example of the Deferred class follows, and that is
            // deliberate rather than an omission. The class is covered in this same
            // table by the `undispatched_probe()` row below, which resolves from the
            // registry and therefore cannot go stale, and totally by
            // `every_registered_command_is_classified_by_the_palette`. A hardcoded
            // Deferred row adds no coverage and goes red the day someone SHIPS the
            // feature it names — the false-alarm direction — which is how the Copy
            // row came to need moving. `HideApplication` below stays hardcoded
            // because NoGnomeEquivalent means "will never be implemented", so that
            // one cannot move.
            PaletteCase {
                desc: "Static + HideApplication: NoGnomeEquivalent, so unsupported",
                kind: PaletteRowKind::Static,
                command: Some(Command::HideApplication),
                window_index: None,
                path: None,
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "Static + the first registry row this front-end does not dispatch",
                kind: PaletteRowKind::Static,
                command: Some(undispatched),
                window_index: None,
                path: None,
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "Static + no command at all: nothing to dispatch",
                kind: PaletteRowKind::Static,
                command: None,
                window_index: None,
                path: None,
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "SwitchWindow + a window index: raise that window",
                kind: PaletteRowKind::SwitchWindow,
                command: None,
                window_index: Some(7),
                path: None,
                want: WantAction::SwitchWindow(7),
            },
            PaletteCase {
                desc: "SwitchWindow + window index 0: the FIRST window's id is not \"no window\"",
                kind: PaletteRowKind::SwitchWindow,
                command: None,
                window_index: Some(0),
                path: None,
                want: WantAction::SwitchWindow(0),
            },
            PaletteCase {
                desc: "SwitchWindow + a window index + a folder path: the path is IGNORED",
                kind: PaletteRowKind::SwitchWindow,
                command: None,
                window_index: Some(7),
                path: Some("/proj/alpha"),
                want: WantAction::SwitchWindow(7),
            },
            PaletteCase {
                desc: "SwitchWindow + NO window index: no window to raise",
                kind: PaletteRowKind::SwitchWindow,
                command: None,
                window_index: None,
                path: None,
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "SwitchWindow + a folder path but no index: still nothing to raise",
                kind: PaletteRowKind::SwitchWindow,
                command: None,
                window_index: None,
                path: Some("/proj/alpha"),
                want: WantAction::Unsupported,
            },
            PaletteCase {
                desc: "OpenRecent + a folder path: re-root the window to that folder",
                kind: PaletteRowKind::OpenRecent,
                command: None,
                window_index: None,
                path: Some("/proj/alpha"),
                want: WantAction::OpenRecentFolder("/proj/alpha"),
            },
            PaletteCase {
                desc: "OpenRecent + a folder path + a window index: the index is IGNORED",
                kind: PaletteRowKind::OpenRecent,
                command: None,
                window_index: Some(9),
                path: Some("/proj/alpha"),
                want: WantAction::OpenRecentFolder("/proj/alpha"),
            },
            PaletteCase {
                desc: "OpenRecent + NO folder path: nothing to change the cwd to",
                kind: PaletteRowKind::OpenRecent,
                command: None,
                window_index: None,
                path: None,
                want: WantAction::Unsupported,
            },
        ];

        for case in &cases {
            let path = case.path.map(Path::new);
            let got = palette_action_for(case.kind, case.command, case.window_index, path);
            match case.want {
                WantAction::Dispatch(expected) => assert_eq!(
                    got,
                    PaletteAction::Dispatch(expected),
                    "case: {} — palette_action_for({:?}, {:?}, {:?}, {:?}) returned {got:?}. A \
                     command `ui_action_for` classifies as a real UiAction MUST resolve to \
                     Dispatch(command): anything else silently drops the row out of the \
                     palette's list, so a working command becomes unreachable from the palette.",
                    case.desc,
                    case.kind,
                    case.command,
                    case.window_index,
                    case.path
                ),
                WantAction::SwitchWindow(expected) => assert_eq!(
                    got,
                    PaletteAction::SwitchWindow(expected),
                    "case: {} — palette_action_for({:?}, {:?}, {:?}, {:?}) returned {got:?}, not \
                     SwitchWindow({expected}). An Unsupported here makes every switch row fail \
                     `palette_rows`' filter, so the palette lists no window rows at all and a \
                     user with two windows cannot reach the other one from it. The INDEX must \
                     come through unchanged: it is the coordinator's `WindowId`, which \
                     `Coordinator::raise` looks up in its registry, so a rewritten one presents a \
                     different window — or, for an id no window holds, silently nothing.",
                    case.desc,
                    case.kind,
                    case.command,
                    case.window_index,
                    case.path
                ),
                WantAction::OpenRecentFolder(expected) => assert_eq!(
                    got,
                    PaletteAction::OpenRecentFolder(PathBuf::from(expected)),
                    "case: {} — palette_action_for({:?}, {:?}, {:?}, {:?}) returned {got:?}, not \
                     OpenRecentFolder({expected:?}). An Unsupported here makes every Open Recent \
                     row fail `palette_rows`' filter, so the palette lists no folder rows at all \
                     and the feature is unreachable; a Dispatch here would hand the shell a \
                     Command it has no folder path for. The PATH must also come through \
                     unchanged: it is what `shell::chdir_to` `:tcd`s to, so a dropped or \
                     rewritten path re-roots the window somewhere the user did not pick.",
                    case.desc,
                    case.kind,
                    case.command,
                    case.window_index,
                    case.path
                ),
                WantAction::Unsupported => assert!(
                    matches!(got, PaletteAction::Unsupported(_)),
                    "case: {} — palette_action_for({:?}, {:?}, {:?}, {:?}) returned {got:?}, but \
                     this row is one wolf-gnome could not perform when the row was written. A \
                     Dispatch, a SwitchWindow or an OpenRecentFolder here puts a row in the \
                     palette that does nothing (or something unintended) when the user presses \
                     Enter — the exact failure `menu_never_lists_a_reserved_action` exists to \
                     prevent for the menu. If the row was GENUINELY given a payload since, move \
                     it to the matching WantAction and tick its feat-list entry; do not relax \
                     the arm.",
                    case.desc,
                    case.kind,
                    case.command,
                    case.window_index,
                    case.path
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
    ///
    /// Every probe passes `None` for the folder path, because that is what
    /// `rank_palette` sets on a `Static` row; the `Static` arm ignoring a `Some` is
    /// pinned by [`palette_action_table`]'s dedicated row instead.
    #[test]
    fn every_registered_command_is_classified_by_the_palette() {
        let mut dispatched = 0usize;
        let mut unsupported = 0usize;

        for spec in COMMAND_SPECS {
            let got = palette_action_for(PaletteRowKind::Static, Some(spec.command), None, None);
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
    /// a string make "this folder row carries no path" indistinguishable from "this
    /// switch row carries no window to raise". Mid-sentence casing matters because
    /// the reason is interpolated into a printed diagnostic, never used as a
    /// sentence of its own.
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
                palette_action_for(
                    PaletteRowKind::Static,
                    Some(undispatched_probe()),
                    None,
                    None,
                ),
            ),
            (
                "a Static row carrying no command at all",
                palette_action_for(PaletteRowKind::Static, None, None, None),
            ),
            (
                "a SwitchWindow row carrying no window index",
                palette_action_for(PaletteRowKind::SwitchWindow, None, None, None),
            ),
            (
                "an OpenRecent row carrying no folder path",
                palette_action_for(PaletteRowKind::OpenRecent, None, None, None),
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
                 cause: with two arms sharing a string, \"this folder row carries no path\" \
                 and \"this switch row carries no window to raise\" become the same message."
            );
        }
    }

    /// Given an EMPTY recent-directories slice, `palette_rows` lists exactly the
    /// registry commands this front-end performs — no deferred row, no `SwitchWindow`
    /// row, no `OpenRecent` row, and never the palette's own invoking command.
    ///
    /// The empty slices are this test's fixture, not a claim about production:
    /// [`palette_rows_include_recent_dirs`] and
    /// [`palette_rows_offer_one_switch_row_per_other_window`] are the ones that pin
    /// what a NON-empty recent-dirs or window slice adds.
    ///
    /// The expected count is computed from `COMMAND_SPECS` here, never written as a
    /// literal, so appending a command or promoting one cannot leave this test
    /// asserting a stale number.
    #[test]
    fn palette_rows_lists_only_actionable_commands() {
        let rows = palette_rows(&[], &[], &[], "", PALETTE_MAX_ROWS);
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
                "palette row {:?} has kind {:?}. This test passes an EMPTY recent-dirs slice AND \
                 an empty window slice, so a non-`Static` row here means `palette_rows` is \
                 manufacturing rows from somewhere other than its arguments. Note this says \
                 nothing about production, which DOES feed both (`PaletteInner::open` -> \
                 `open_recent_candidates` and the coordinator's window registry); the tests that \
                 pin what a non-empty slice adds are `palette_rows_include_recent_dirs` and \
                 `palette_rows_offer_one_switch_row_per_other_window`.",
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
                palette_action_for(row.kind, row.command, row.window_index, row.path.as_deref()),
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

        let cold = palette_rows(&[], &[], &[], "", PALETTE_MAX_ROWS);
        assert_eq!(
            cold.first().map(|row| row.mru_key.as_str()),
            Some(first.id),
            "with an EMPTY command MRU the empty-query list must open on the registry's first \
             actionable row ({:?}). A different row means the candidate order is no longer \
             COMMAND_SPECS order, which is what the MRU assertion below is measured against.",
            first.id
        );

        let recent = vec![probe.to_string()];
        let warm = palette_rows(&[], &[], &recent, "", PALETTE_MAX_ROWS);
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
    /// `command_palette_chord_runs_the_selected_command_under_cage` types ranks the
    /// New (`new_buffer`) row FIRST, and the one MRU state that inverts that is
    /// asserted rather than assumed away.
    ///
    /// Without this, a registry rename would not fail anything in the fast suite. It
    /// would instead make the smoke stochastic: `restored_index` selects the top row
    /// whatever it is, so which command Enter runs would be decided by ranking
    /// tie-breaks nobody chose.
    ///
    /// ## Why this is an ORDER guard and not a uniqueness guard
    ///
    /// "Exactly one row" is not available for this query, and the mechanism is what
    /// says so: `score_entry` matches a term as a case-insensitive SUBSEQUENCE of
    /// `display`, and `New` is a PREFIX of `New Window`, so every query that matches
    /// one matches the other. No query separates them, exactly as no query separates
    /// `Preview` from `Preview in Browser` (see
    /// [`palette_rows_for_the_preview_smoke_rank_the_named_command_first`], which
    /// this test is shaped like). Chasing uniqueness here is therefore work that
    /// cannot succeed.
    ///
    /// What makes the smoke deterministic is therefore the TIE-BREAK. `rank_matches`
    /// sorts on `(mru_group, mru_rank, Reverse(score), match_start, span, len,
    /// display)`, and for the term `new` the two rows tie on the first five: both
    /// score 124 (component start + word start + contiguous), both match from index
    /// 0, and both span 2. They separate at `len` — 3 characters against 10 — so the
    /// shorter title wins. That is the whole of why Enter runs New.
    ///
    /// ## The MRU premise is load-bearing, so it is asserted
    ///
    /// `mru_group` is `MatchKey`'s LEADING field, so a command MRU holding
    /// `new_window` puts that row first however much better `New` matches, and the
    /// smoke would open a second WINDOW instead of a new buffer. Its capture-based
    /// assertions could not tell those apart — `cage` shows one toplevel and keeps
    /// the newest on top, so a fresh window hides the base file's heading exactly as
    /// a fresh buffer replaces it — which is why the guard has to be here.
    ///
    /// The smoke is safe from it for a reason outside this crate, and the last block
    /// below is what keeps that reason stated rather than lucky: its `WOLF_STATE_DIR`
    /// is a fresh per-run directory, so `command-mru.sqlite` starts empty, and
    /// nothing but a palette activation ever writes that store.
    ///
    /// ## The recent-directories premise
    ///
    /// Every assertion here holds GIVEN an empty recent-dirs slice, which is what
    /// this test passes. The smoke gets that from two independent facts, neither of
    /// which this test can check: the same fresh `WOLF_STATE_DIR` leaves
    /// `folder-mru.sqlite` empty, and `wolf_core::redraw::apply_cwd` never records
    /// the LAUNCH working directory (the initial `wolf_cwd` emit is flagged), so
    /// nothing is added before the chord fires. A recorded directory whose path
    /// happened to contain `n`, `e` and `w` in that order would otherwise rank
    /// against the same query.
    #[test]
    fn palette_rows_query_new_ranks_the_new_buffer_row_first() {
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

        // Precondition: the query COLLIDES, so everything below is a statement about
        // ranking rather than about a query that happens to be rare. Unfiltered it
        // matches `New`, `New Window` and `New Tab`; the filter removes `New Tab`
        // alone.
        let unfiltered = rank_palette(&[], &[], &[], SMOKE_QUERY, usize::MAX);
        assert!(
            unfiltered.len() > 1,
            "`rank_palette` matches only {} candidate(s) for {SMOKE_QUERY:?} before the \
             classification filter runs, so the ranking assertions below would be about a query \
             that selects one title and would stop being statements about ranking at all. Titles \
             matching {SMOKE_QUERY:?} were: {:?}",
            unfiltered.len(),
            unfiltered
                .iter()
                .map(|row| &row.display)
                .collect::<Vec<_>>()
        );

        let mrus: [Vec<String>; 2] = [Vec::new(), vec![Command::ClosePane.spec().id.to_string()]];
        for recent in &mrus {
            let rows = palette_rows(&[], &[], recent, SMOKE_QUERY, PALETTE_MAX_ROWS);
            assert!(
                rows.len() > 1,
                "`palette_rows` returned {} actionable row(s) for {SMOKE_QUERY:?} with command \
                 MRU {recent:?}. The collision survives the classification filter — `New Window` \
                 is dispatched — so one row here means it was demoted back to Deferred, and the \
                 top-row assertion below would be about uniqueness again rather than about the \
                 `len` tie-break it names. Rows were: {:?}",
                rows.len(),
                rows.iter().map(|row| &row.display).collect::<Vec<_>>()
            );

            let row = rows.first().unwrap_or_else(|| {
                panic!(
                    "`palette_rows` returned NO rows for {SMOKE_QUERY:?} with command MRU \
                     {recent:?}, so the smoke would press Enter on an empty palette and nothing \
                     would be dispatched at all."
                )
            });
            assert_eq!(
                row.command,
                Some(Command::NewBuffer),
                "the TOP row for {SMOKE_QUERY:?} (with command MRU {recent:?}) carries {:?}, not \
                 Command::NewBuffer. The smoke presses Enter on the preselected first row and \
                 asserts the base file's heading is GONE from the capture — which a second \
                 WINDOW satisfies just as well as a new buffer, since `cage` keeps the newest \
                 toplevel on top, so this is the guard and the capture is not. The two rows tie \
                 down to `len`, so a title rename is all it takes to invert them. Rows were: {:?}",
                row.command,
                rows.iter().map(|r| &r.display).collect::<Vec<_>>()
            );
            assert_eq!(
                row.mru_key.as_str(),
                key,
                "the TOP row for {SMOKE_QUERY:?} (with command MRU {recent:?}) records {:?} on \
                 activation, not {key:?}. The smoke reads the command-MRU sqlite back and looks \
                 for exactly that key.",
                row.mru_key
            );
        }

        // --- The adversarial MRU, asserted as the inversion it is --------------
        //
        // Stated here rather than as a third `mrus` entry, because its expected answer
        // is the OPPOSITE of the loop's and folding it in would need a per-row
        // expectation reading as though either answer were acceptable. It is not: it
        // is a live fragility of the smoke, and this assertion is what keeps it from
        // being rediscovered as a mystery.
        let window_id = Command::NewWindow.spec().id;
        let adversarial: Vec<String> = vec![window_id.to_string()];
        let inverted = palette_rows(&[], &[], &adversarial, SMOKE_QUERY, PALETTE_MAX_ROWS);
        let top = inverted.first().unwrap_or_else(|| {
            panic!(
                "`palette_rows` returned NO rows for {SMOKE_QUERY:?} with command MRU \
                 [{window_id:?}], which contradicts the row count asserted above for the same \
                 query."
            )
        });
        assert_eq!(
            top.command,
            Some(Command::NewWindow),
            "with a command MRU holding {window_id:?}, the top row for {SMOKE_QUERY:?} carries \
             {:?} rather than Command::NewWindow. This test asserts the INVERSION on purpose: \
             `MatchKey`'s leading field is `mru_group`, so one MRU hit outranks every \
             match-quality key, and a state directory that had once opened a window from the \
             palette would make the smoke open another one instead of a buffer. The smoke is safe \
             from that only because `smoke_state_dir` is fresh per run and nothing but a palette \
             activation writes `command-mru.sqlite`. If THIS assertion is what went red, the \
             tie-break itself changed — re-derive `rank_matches`' key order before touching the \
             smoke, because the blocks above have stopped being statements about ranking. Rows \
             were: {:?}",
            top.command,
            inverted.iter().map(|row| &row.display).collect::<Vec<_>>()
        );
    }

    /// The query the New Window smoke types selects the New Window row, and does it
    /// by MATCHING rather than by out-ranking anything.
    ///
    /// This is the sibling of
    /// [`palette_rows_query_new_ranks_the_new_buffer_row_first`] and the measurement
    /// [`NEW_WINDOW_PALETTE_QUERY`] comes from, so the two are read together. They
    /// look like they disagree and do not: `new` ranks New (`new_buffer`) first
    /// because `New` is the SHORTER of two titles that tie down to `len`, while
    /// `new window` drops that row entirely, because `rank_matches` requires EVERY
    /// whitespace-separated term to be a subsequence of the label and `window` is not
    /// a subsequence of `New`. One query cannot serve both smokes, and the second
    /// term is exactly what makes this one unambiguous.
    ///
    /// Uniqueness here is a property of the QUERY, not of the classification filter,
    /// which is why the count is asserted on `rank_palette`'s output as well: no
    /// deferred row is being hidden behind it, so the count cannot change meaning if
    /// a command is promoted.
    ///
    /// ## What the smoke needs from this
    ///
    /// `two_windows_two_neovims_and_two_saved_sessions_under_cage` types this query
    /// into the palette of the window it launched, then presses Enter on the
    /// preselected top row. Two of its premises are outside this test and are
    /// recorded rather than checked: it opens the palette while that window is the
    /// ONLY one, so the coordinator offers no Switch to Window rows (whose labels
    /// would otherwise rank against these terms), and its per-run `WOLF_STATE_DIR`
    /// leaves both MRU stores empty.
    ///
    /// A third lives in the smoke's own file, because it is about the COPY rather
    /// than about the query: the eighth block of
    /// `restore_fixture_names_cannot_satisfy_their_own_assertions` asserts that
    /// `TWOWIN_NEW_WINDOW_QUERY` still splits into exactly two lowercase ASCII terms
    /// on a single space. This test would keep passing against a copy that had picked
    /// up a doubled space or a capital on the way across the crate boundary.
    ///
    /// FALSIFYING EDIT: set [`NEW_WINDOW_PALETTE_QUERY`] to `"new"`. PREDICTED: the
    /// UNFILTERED count reddens first, reporting 3 candidates and printing `New`,
    /// `New Window` and `New Tab`; relaxing it, the actionable count reports 2; and
    /// relaxing that, the command assertion names `New` / `Command::NewBuffer` as
    /// the row that would run instead — the smoke would replace the buffer in the
    /// window it already had and never open a second one, so its `nvim`-child count
    /// would stay at 1. PREDICTED ISOLATED: yes — no other test reads this constant.
    #[test]
    fn the_new_window_palette_query_selects_the_new_window_row() {
        let key = Command::NewWindow.spec().id;

        // Precondition: the query really does what its name says, before any
        // classification filter runs. If `rank_palette` matched more than one
        // candidate, the assertions below would be about the filter or about a
        // tie-break rather than about the query.
        let unfiltered = rank_palette(&[], &[], &[], NEW_WINDOW_PALETTE_QUERY, usize::MAX);
        assert_eq!(
            unfiltered.len(),
            1,
            "`rank_palette` matches {} candidate(s) for {NEW_WINDOW_PALETTE_QUERY:?} before the \
             classification filter runs, not 1. This query is chosen so that MATCHING selects the \
             row — the second term `window` is not a subsequence of `New`, which is what \
             separates it from {SMOKE_QUERY:?} — so a second candidate means the smoke's Enter \
             now lands on whichever row a tie-break favours. Titles matching \
             {NEW_WINDOW_PALETTE_QUERY:?} were: {:?}",
            unfiltered.len(),
            unfiltered
                .iter()
                .map(|row| &row.display)
                .collect::<Vec<_>>()
        );

        // Both MRU states the smoke can meet: a fresh store, and one warmed by an
        // unrelated activation. A single row cannot be reordered, so the MRU can only
        // change the answer by changing the SET — which is what these two check.
        let mrus: [Vec<String>; 2] = [Vec::new(), vec![Command::ClosePane.spec().id.to_string()]];
        for recent in &mrus {
            let rows = palette_rows(&[], &[], recent, NEW_WINDOW_PALETTE_QUERY, PALETTE_MAX_ROWS);
            assert_eq!(
                rows.len(),
                1,
                "`palette_rows` returned {} actionable row(s) for {NEW_WINDOW_PALETTE_QUERY:?} \
                 with command MRU {recent:?}, not 1. ZERO means the New Window row was demoted \
                 back to Deferred, so the smoke would type its query into an empty list and Enter \
                 would open nothing — its `nvim`-child count would still read 1 and it would fail \
                 with no visible cause. MORE than one means the query stopped being unambiguous \
                 and which row Enter runs is decided by a tie-break nobody chose. Rows were: {:?}",
                rows.len(),
                rows.iter().map(|row| &row.display).collect::<Vec<_>>()
            );

            let row = &rows[0];
            assert_eq!(
                row.command,
                Some(Command::NewWindow),
                "the sole row matching {NEW_WINDOW_PALETTE_QUERY:?} (with command MRU {recent:?}) \
                 carries {:?}, not Command::NewWindow. The smoke asserts the app process gains a \
                 SECOND `nvim` child, which only opening a window produces; every other `New …` \
                 command acts inside the window that is already there. Rows were: {:?}",
                row.command,
                rows.iter().map(|r| &r.display).collect::<Vec<_>>()
            );
            assert_eq!(
                row.mru_key.as_str(),
                key,
                "the sole row matching {NEW_WINDOW_PALETTE_QUERY:?} (with command MRU {recent:?}) \
                 records {:?} on activation, not {key:?} — the canonical registry id, and what a \
                 command-MRU readback would have to look for.",
                row.mru_key
            );
        }
    }

    /// The markdown-preview smokes' two queries each rank the row they name FIRST,
    /// and the ONE state that would invert `prv` is stated outright rather than
    /// assumed away.
    ///
    /// ## What this pins for those smokes
    ///
    /// `preview_pane_renders_and_edit_source_returns_under_cage` and
    /// `preview_of_an_unsaved_buffer_renders_under_cage` drive Preview (and, in the
    /// first, Edit Source) by typing a query and pressing Enter on the preselected TOP
    /// row, so "the top row carries the command the query names" is what makes which
    /// command runs deterministic. That premise cannot be checked from a smoke — the
    /// overlay is closed before `grim` fires and a screenshot can neither count rows
    /// nor read their order — so a registry change that reordered them would make both
    /// smokes stochastic rather than red. They redden here instead.
    ///
    /// The `mru_key` half is the other end of the same join: those smokes assert the
    /// command MRU holds `preview` and `edit` after the activations, which only means
    /// anything while those are the ids these rows record.
    ///
    /// ## `prv` matches TWO rows, and NO query can separate them
    ///
    /// `Command::PreviewInBrowser` is dispatched, so [`palette_rows`]' classification
    /// filter does not remove it and `prv` selects `Preview` and `Preview in Browser`
    /// both. Uniqueness is unrecoverable and must not be chased: `rank_matches`
    /// matches a term as a case-insensitive SUBSEQUENCE, and `Preview` is a PREFIX of
    /// `Preview in Browser`, so every subsequence of the first is a subsequence of the
    /// second. What decides is therefore the tie-break — the two rows tie on
    /// `mru_group`, `mru_rank`, `score`, `match_start` and `span`, and separate only at
    /// `len`, where `Preview`'s 7 characters sort ahead of the other's 18.
    ///
    /// ## The adversarial MRU INVERTS the order, and that is asserted, not hidden
    ///
    /// `mru_group` is the LEADING key of `MatchKey`, so a command MRU holding
    /// `preview_in_browser` puts that row first however much better `Preview` matches —
    /// and the smoke would then hand a document to the system browser instead of
    /// opening a preview pane. The third block below asserts that inversion as a
    /// measured fact rather than weakening the first two to accommodate it, because it
    /// is precisely the condition the smokes' determinism rests on NOT holding: each
    /// runs against a fresh `WOLF_STATE_DIR` (`smoke_state_dir`) and neither ever
    /// activates Preview in Browser, so the MRU they meet holds `preview` at most. If
    /// that block ever stops inverting, the tie-break has changed and the first two
    /// blocks have stopped being statements about ranking at all.
    ///
    /// Falsifier: rename `Command::Preview`'s title to `Preview the document` — still
    /// a `prv` subsequence, but 20 characters against `Preview in Browser`'s 18, so
    /// the `len` tie-break inverts and the empty-MRU block reddens naming
    /// `Preview in Browser` as the top row. Deleting the classification filter from
    /// [`palette_rows`] reddens the `actionable_rows` clause instead, which is what
    /// the unfiltered floor exists to keep meaningful.
    ///
    /// Reverting `Command::PreviewInBrowser` to `UiAction::Deferred` reddens the
    /// `actionable_rows` clause too, and MEASURED it is that clause and not the
    /// unfiltered floor: `rank_palette` runs before the filter, so the floor of 2 still
    /// holds while `palette_rows` drops to one row.
    /// `command_action::tests::dispatched_commands_are_the_expected_set` reddens beside
    /// it, which is how the pair is told apart from a registry change.
    #[test]
    fn palette_rows_for_the_preview_smoke_rank_the_named_command_first() {
        for case in PREVIEW_SMOKE_QUERIES {
            let PreviewSmokeQuery {
                query,
                command,
                id,
                unfiltered_floor,
                actionable_rows,
            } = case;

            // Precondition: the literal the smokes read back is still this command's
            // registry id. They are a separate crate and cannot import it.
            assert_eq!(
                command.spec().id,
                id,
                "{command:?}'s registry id is {:?}, but the cage smokes assert the palette \
                 activation recorded the literal {id:?} in the command MRU. Update \
                 `PREVIEW_MRU_KEYS` in `tests/smoke_headless.rs` and PREVIEW_SMOKE_QUERIES here \
                 together, or those smokes' MRU precondition becomes unfalsifiable.",
                command.spec().id
            );

            let unfiltered = rank_palette(&[], &[], &[], query, usize::MAX);
            assert!(
                unfiltered.len() >= unfiltered_floor,
                "`rank_palette` matches only {} candidate(s) for {query:?} before the \
                 classification filter runs, below the floor of {unfiltered_floor} this query \
                 claims. For a floor of 2 that means the `Preview` / `Preview in Browser` \
                 COLLISION is gone, so the top-row assertion below would hold for a query that \
                 matches one title and would no longer be a statement about ranking; for a floor \
                 of 1 it means the query matches NOTHING and the smokes would find an empty \
                 palette. Titles matching {query:?} were: {:?}",
                unfiltered.len(),
                unfiltered
                    .iter()
                    .map(|row| &row.display)
                    .collect::<Vec<_>>()
            );

            // Both MRU states a smoke can actually meet: a fresh store, and the store
            // the FIRST activation warmed. `preview_in_browser` is not among them —
            // see the third block below, which is where that case is pinned.
            let mrus: [Vec<String>; 2] = [Vec::new(), vec![Command::Preview.spec().id.to_string()]];
            for recent in &mrus {
                let rows = palette_rows(&[], &[], recent, query, PALETTE_MAX_ROWS);
                assert_eq!(
                    rows.len(),
                    actionable_rows,
                    "`palette_rows` returned {} actionable row(s) for {query:?} with command MRU \
                     {recent:?}, not the {actionable_rows} this query claims. FEWER means the \
                     classification filter removed a row that is dispatched today — for {query:?} \
                     that is the `Preview in Browser` collision disappearing, which makes the \
                     top-row assertion below vacuous. MORE means a registry addition matched the \
                     query too, and whether it outranks the named row is then decided by a \
                     tie-break nobody chose. Rows were: {:?}",
                    rows.len(),
                    rows.iter().map(|row| &row.display).collect::<Vec<_>>()
                );

                let row = rows.first().unwrap_or_else(|| {
                    panic!(
                        "`palette_rows` returned NO rows for {query:?} with command MRU \
                         {recent:?}, so the smokes would press Enter on an empty palette and \
                         nothing at all would be dispatched. The assertion above should have \
                         caught this first."
                    )
                });
                assert_eq!(
                    row.command,
                    Some(command),
                    "the TOP row for {query:?} (with command MRU {recent:?}) carries {:?}, not \
                     {command:?}. The smokes press Enter on the preselected first row, so they \
                     would drive a command they do not name. For {query:?} = \"prv\" the row that \
                     displaces it is `Preview in Browser`, which serves the buffer over loopback \
                     and hands the URL to the system browser — on a host where that launcher works \
                     it opens a real browser tab, so this is a safety failure and not only a \
                     ranking one. Rows were: {:?}",
                    row.command,
                    rows.iter().map(|r| &r.display).collect::<Vec<_>>()
                );
                assert_eq!(
                    row.mru_key.as_str(),
                    id,
                    "the TOP row for {query:?} (with command MRU {recent:?}) records {:?} on \
                     activation, not {id:?}. The smokes read the command-MRU sqlite back and look \
                     for exactly that key.",
                    row.mru_key
                );
            }
        }

        // --- The adversarial MRU, asserted as the measured inversion it is ----
        //
        // Stated here once rather than as a third `mrus` entry, because its expected
        // answer is the OPPOSITE of the loop's and folding it in would need a per-row
        // expectation that reads as though either answer were acceptable. It is not: it
        // is a live fragility of both preview smokes, and the assertion is what keeps
        // it from being rediscovered as a mystery.
        // Taken from the table rather than re-typed, so the two blocks can never end up
        // inverting a query the loop above never ran.
        let preview_query = PREVIEW_SMOKE_QUERIES
            .iter()
            .find(|case| case.command == Command::Preview)
            .map(|case| case.query)
            .unwrap_or_else(|| {
                panic!(
                    "PREVIEW_SMOKE_QUERIES carries no row for Command::Preview, so the block \
                     below has no query to invert and the fragility it records goes unstated."
                )
            });
        let browser_id = Command::PreviewInBrowser.spec().id;
        let adversarial: Vec<String> = vec![browser_id.to_string()];
        let inverted = palette_rows(&[], &[], &adversarial, preview_query, PALETTE_MAX_ROWS);
        let top = inverted.first().unwrap_or_else(|| {
            panic!(
                "`palette_rows` returned NO rows for {preview_query:?} with command MRU \
                 [{browser_id:?}], which contradicts the row count asserted above for the same \
                 query."
            )
        });
        assert_eq!(
            top.command,
            Some(Command::PreviewInBrowser),
            "with a command MRU holding {browser_id:?}, the top row for {preview_query:?} carries \
             {:?} rather than Command::PreviewInBrowser. This test asserts the INVERSION on \
             purpose: `MatchKey`'s leading field is `mru_group`, so an MRU hit outranks every \
             match-quality key, and a state directory that had once run Preview in Browser would \
             make the preview smokes launch a browser instead of opening a preview pane. Both \
             smokes are safe from it only because `smoke_state_dir` is fresh per run and neither \
             ever activates that command. If this assertion is what went red, the tie-break itself \
             changed — re-derive `rank_matches`' key order before touching either smoke, because \
             the two blocks above have stopped being statements about ranking. Rows were: {:?}",
            top.command,
            inverted.iter().map(|row| &row.display).collect::<Vec<_>>()
        );
    }

    /// `max` is applied AFTER the classification filter, so a cap counts only rows
    /// the user can actually run.
    ///
    /// Ranking unbounded and truncating afterwards is not a micro-optimization in
    /// reverse: `rank_palette` truncates before this crate's filter can see the
    /// list, so passing `max` through would let unsupported rows eat the budget and
    /// hand the user a short — or empty — palette.
    ///
    /// **The cap is COMPUTED, and that is the whole design of this test.** A cap
    /// discriminates only if it reaches PAST the first candidate the filter removes,
    /// and where that candidate sits is a property of the REGISTRY rather than of
    /// this crate. A hardcoded cap is what that rules out: `max == 1` discriminates
    /// exactly while `COMMAND_SPECS[0]` is a command this front-end does not
    /// dispatch, and the day it is promoted the same literal quietly stops
    /// discriminating anything — the first row survives either way, the assertion
    /// still passes, and nothing says the test has gone inert. Reading the position
    /// out of `rank_palette`'s own output means the row can move without disarming
    /// the guard.
    #[test]
    fn palette_rows_cap_counts_only_actionable_rows() {
        let full = palette_rows(&[], &[], &[], "", PALETTE_MAX_ROWS);

        // Where the first unsupported candidate sits in the UNFILTERED empty-query
        // order — which is `COMMAND_SPECS` order, since nothing is in the MRU.
        let candidates = rank_palette(&[], &[], &[], "", usize::MAX);
        let first_unsupported = candidates
            .iter()
            .position(|row| {
                matches!(
                    palette_action_for(
                        row.kind,
                        row.command,
                        row.window_index,
                        row.path.as_deref()
                    ),
                    PaletteAction::Unsupported(_)
                )
            })
            .unwrap_or_else(|| {
                panic!(
                    "no ranked candidate is Unsupported, so a pre-filter cap and a post-filter \
                     cap return the same list for every `max` and this test cannot tell them \
                     apart. That would mean wolf-gnome dispatches every registered command — a \
                     milestone, not a failure — and this test should then be deleted along with \
                     the filter it guards rather than left passing vacuously."
                )
            });
        let cap = first_unsupported + 1;

        assert!(
            full.len() > cap,
            "the uncapped empty-query list holds {} row(s) and the cap under test is {cap}, so \
             capping removes nothing and a pre-filter cap would be indistinguishable from a \
             post-filter one.",
            full.len()
        );

        let capped = palette_rows(&[], &[], &[], "", cap);
        assert_eq!(
            capped.len(),
            cap,
            "`palette_rows(.., {cap})` returned {} row(s), not {cap}. FEWER means `max` is being \
             handed to `rank_palette`, which truncates BEFORE the classification filter runs: the \
             first {cap} ranked candidates include one the filter then removes, so the user is \
             handed a short list — and with a small enough cap, an empty one. Rank unbounded and \
             `.take(max)` after the filter. Rows were: {:?}",
            capped.len(),
            capped.iter().map(|row| &row.display).collect::<Vec<_>>()
        );
        assert_eq!(
            capped
                .iter()
                .map(|row| row.mru_key.as_str())
                .collect::<Vec<_>>(),
            full.iter()
                .take(cap)
                .map(|row| row.mru_key.as_str())
                .collect::<Vec<_>>(),
            "the capped list is not the uncapped list's first {cap} rows, so applying the cap \
             reordered the results. `.take(max)` must preserve the ranking order it truncates."
        );
    }

    /// A seeded recent directory really reaches [`rank_palette`]: the empty-query
    /// list is the actionable registry PLUS exactly one `OpenRecent` row, and that
    /// row carries the seeded path and shows it in full.
    ///
    /// This is the unit's primary falsifier for the whole feature. **Revert:**
    /// restore `rank_palette(&[], &[], …)` in [`palette_rows`] — i.e. drop the
    /// `recent_dirs` argument on the floor. The count assertion then reports the
    /// registry's size instead of one more than it, and every other Open Recent test
    /// in this module goes red with it.
    ///
    /// The `display` clause is the D2 label rule stated where the row is built: the
    /// shown string carries the FULL absolute path, unabbreviated, because it is also
    /// the string ranked against the query (see
    /// [`open_recent_row_is_matched_by_its_visible_label`]).
    #[test]
    fn palette_rows_include_recent_dirs() {
        let dir = PathBuf::from("/proj/alpha");
        let dir_text = dir.to_string_lossy().into_owned();
        let actionable = actionable_specs();

        let rows = palette_rows(std::slice::from_ref(&dir), &[], &[], "", PALETTE_MAX_ROWS);

        assert_eq!(
            rows.len(),
            actionable.len() + 1,
            "`palette_rows` returned {} row(s) for the empty query with ONE recent directory \
             seeded, but the registry has {} actionable command(s), so exactly {} were expected. \
             The registry's own count means the seeded directory produced NO row: `palette_rows` \
             is not passing its `recent_dirs` argument through to `rank_palette`, so the palette \
             would list no Open Recent rows however full the store is and the SQLite read at open \
             would be dead weight. More than that means something else is manufacturing rows. \
             Rows were: {:?}",
            rows.len(),
            actionable.len(),
            actionable.len() + 1,
            rows.iter().map(|row| &row.display).collect::<Vec<_>>()
        );

        let recent_rows: Vec<&PaletteRow> = rows
            .iter()
            .filter(|row| row.kind == PaletteRowKind::OpenRecent)
            .collect();
        assert_eq!(
            recent_rows.len(),
            1,
            "the empty-query list holds {} row(s) of kind OpenRecent for ONE seeded directory, \
             not 1. Zero means the seeded directory was ranked but then dropped by \
             `palette_rows`' classification filter, which is the shape a reverted \
             `palette_action_for` produces; more than one means a candidate was duplicated. Rows \
             were: {:?}",
            recent_rows.len(),
            rows.iter().map(|row| &row.display).collect::<Vec<_>>()
        );

        let row = recent_rows[0];
        assert_eq!(
            row.path.as_deref(),
            Some(dir.as_path()),
            "the Open Recent row carries path {:?}, not the seeded {dir:?}. That path is what \
             `shell::chdir_to` `:tcd`s to, so a wrong one re-roots the window somewhere the user \
             did not pick — and a None makes the row classify as Unsupported and vanish.",
            row.path
        );
        assert!(
            row.display.contains(&dir_text),
            "the Open Recent row's label is {:?}, which does not contain the seeded path \
             {dir_text:?} in full. The label is also the string `rank_matches` ranks against, so \
             an abbreviated or rewritten one makes the user unable to type what they can see — \
             see `open_recent_row_is_matched_by_its_visible_label`, and D2 in the plan for why \
             `title::display_title` is deliberately NOT applied here.",
            row.display
        );
    }

    /// The seeded Open Recent row classifies to an action the shell can perform, and
    /// therefore survives [`palette_rows`]' filter.
    ///
    /// **Revert:** restore [`palette_action_for`]'s old
    /// `OpenRecent => Unsupported(…)` arm. Every Open Recent row then fails the
    /// filter, the `find` below fails, and the palette silently offers no folder rows
    /// at all — which is exactly the state this front-end shipped in before the
    /// feature landed, and exactly the state a reader could not tell apart from "the
    /// store is empty".
    #[test]
    fn palette_rows_open_recent_row_is_activatable() {
        let dir = PathBuf::from("/proj/alpha");

        let rows = palette_rows(std::slice::from_ref(&dir), &[], &[], "", PALETTE_MAX_ROWS);

        let row = rows
            .iter()
            .find(|row| row.kind == PaletteRowKind::OpenRecent)
            .unwrap_or_else(|| {
                panic!(
                    "no OpenRecent row survived `palette_rows` for the seeded directory {dir:?}. \
                     The filter keeps everything that is not Unsupported, so an absent row means \
                     `palette_action_for` classifies OpenRecent as Unsupported again — the \
                     palette then lists no folder rows at all, and the user cannot tell that from \
                     an empty recent-directories store. Rows were: {:?}",
                    rows.iter().map(|row| &row.display).collect::<Vec<_>>()
                )
            });

        assert_eq!(
            palette_action_for(row.kind, row.command, row.window_index, row.path.as_deref()),
            PaletteAction::OpenRecentFolder(dir.clone()),
            "the surviving Open Recent row {:?} does not classify to \
             OpenRecentFolder({dir:?}). `PaletteInner::activate_selection` performs whatever this \
             returns, so anything else means pressing Enter on the row prints a refusal (or \
             dispatches something unrelated) rather than re-rooting the window.",
            row.display
        );
    }

    /// A live window really reaches [`rank_palette`]: one OTHER window yields
    /// exactly one Switch to Window row, carrying that window's id and core's
    /// verbatim label.
    ///
    /// This is the switch half's primary falsifier, and it is the twin of
    /// [`palette_rows_include_recent_dirs`]. **Revert:** restore
    /// `rank_palette(recent_dirs, &[], …)` in [`palette_rows`] — i.e. drop the
    /// `windows` argument on the floor. PREDICTED: the count assertion reports the
    /// registry's size instead of one more than it, and the switch-row count reports
    /// 0. PREDICTED NOT ISOLATED — [`open_recent_targets_the_invoking_window`]'s
    /// third half reddens beside it, which is the pairing that says the argument was
    /// dropped rather than that the classification changed.
    ///
    /// The CONTROL case is what makes the rest attributable: with no windows passed,
    /// no switch row exists at all, so a row found afterwards came from the argument
    /// and not from something else manufacturing rows.
    ///
    /// The label is asserted against a string BUILT from the fixture's own cwd
    /// rather than typed out, so an implementation that rewrote the label — a
    /// basename, a `~` abbreviation — fails it while a passthrough cannot satisfy it
    /// by accident. It matters beyond cosmetics: `rank_matches` is a subsequence
    /// matcher over `display`, so the shown string is also the string the user's
    /// query is matched against.
    ///
    /// The TWO-window case is not a repetition of the one-window case, and it is the
    /// only part of this test that can see a `.first()`-shaped defect: a source that
    /// offered the first other window and dropped the rest satisfies every assertion
    /// above it, since with one other window "the first" and "all of them" are the
    /// same list. It asserts the SET of ids rather than a count, so a failure names
    /// which window became unreachable.
    ///
    /// The empty-cwd row is the shape a fresh New Window has: it has started no
    /// project, contributes `""` as its persisted directory, and core renders that as
    /// `Untitled`. Without a row for it, the palette's only offer for the window a
    /// user just opened would be a blank label.
    #[test]
    fn palette_rows_offer_one_switch_row_per_other_window() {
        const BETA: &str = "/proj/beta";
        const BETA_INDEX: u16 = 7;
        const GAMMA: &str = "/proj/gamma";
        const GAMMA_INDEX: u16 = 5;
        const UNTITLED_INDEX: u16 = 2;

        let actionable = actionable_specs();

        // --- Control: no windows, no switch rows --------------------------------
        let control = palette_rows(&[], &[], &[], "", PALETTE_MAX_ROWS);
        assert!(
            control
                .iter()
                .all(|row| row.kind != PaletteRowKind::SwitchWindow),
            "with an EMPTY window slice the palette already offers a SwitchWindow row, so every \
             assertion below would hold whether or not the `windows` argument reached \
             `rank_palette` at all. Rows were: {:?}",
            control.iter().map(|row| &row.display).collect::<Vec<_>>()
        );

        // --- One other window ---------------------------------------------------
        let windows = [PaletteWindow {
            index: BETA_INDEX,
            cwd: BETA.to_string(),
        }];
        let rows = palette_rows(&[], &windows, &[], "", PALETTE_MAX_ROWS);

        assert_eq!(
            rows.len(),
            actionable.len() + 1,
            "`palette_rows` returned {} row(s) for the empty query with ONE other window, but \
             the registry has {} actionable command(s), so exactly {} were expected. The \
             registry's own count means the window produced NO row: `palette_rows` is not passing \
             its `windows` argument through to `rank_palette`, so the palette would offer no way \
             to reach another window however many are open — and on Wayland, where a backgrounded \
             toplevel cannot raise itself, the palette is the only way to ask. Rows were: {:?}",
            rows.len(),
            actionable.len(),
            actionable.len() + 1,
            rows.iter().map(|row| &row.display).collect::<Vec<_>>()
        );

        let switch_rows: Vec<&PaletteRow> = rows
            .iter()
            .filter(|row| row.kind == PaletteRowKind::SwitchWindow)
            .collect();
        assert_eq!(
            switch_rows.len(),
            1,
            "the empty-query list holds {} row(s) of kind SwitchWindow for ONE other window, not \
             1. Zero means the row was ranked and then dropped by `palette_rows`' classification \
             filter, which is the shape a reverted `palette_action_for` produces; more than one \
             means a window was duplicated, and two rows for one window would raise the same \
             window whichever the user picked. Rows were: {:?}",
            switch_rows.len(),
            rows.iter().map(|row| &row.display).collect::<Vec<_>>()
        );

        let row = switch_rows[0];
        assert_eq!(
            row.window_index,
            Some(BETA_INDEX),
            "the Switch to Window row carries window index {:?}, not the {BETA_INDEX:?} it was \
             given. That index is the coordinator's `WindowId` and the argument \
             `Coordinator::raise` looks up in its registry, so a rewritten one presents a \
             different window — and an id no window holds presents nothing at all, silently.",
            row.window_index
        );
        assert_eq!(
            row.display,
            format!("Switch to Window: {BETA}"),
            "the Switch to Window row is labelled {:?}. The label is `rank_palette`'s verbatim — \
             the prefix plus the window's directory — and this crate rewrites it nowhere. It is \
             also the string `rank_matches` ranks the query against, so an abbreviated or \
             prettified label makes the user unable to type what they can see, exactly as it \
             would for an Open Recent row.",
            row.display
        );
        assert_eq!(
            palette_action_for(row.kind, row.command, row.window_index, row.path.as_deref()),
            PaletteAction::SwitchWindow(BETA_INDEX),
            "the surviving Switch to Window row {:?} does not classify to \
             SwitchWindow({BETA_INDEX}). `PaletteInner::activate_selection` performs whatever \
             this returns, so anything else means pressing Enter on the row prints a refusal \
             rather than asking the coordinator to raise that window.",
            row.display
        );

        // --- Two other windows: one row EACH, both raise targets preserved -------
        let two = [
            PaletteWindow {
                index: BETA_INDEX,
                cwd: BETA.to_string(),
            },
            PaletteWindow {
                index: GAMMA_INDEX,
                cwd: GAMMA.to_string(),
            },
        ];
        let rows = palette_rows(&[], &two, &[], "", PALETTE_MAX_ROWS);
        let mut indices: Vec<Option<u16>> = rows
            .iter()
            .filter(|row| row.kind == PaletteRowKind::SwitchWindow)
            .map(|row| row.window_index)
            .collect();
        indices.sort_unstable();
        assert_eq!(
            indices,
            vec![Some(GAMMA_INDEX), Some(BETA_INDEX)],
            "with TWO other windows the palette offered switch rows for {indices:?}. One row per \
             window is the contract: a missing id means that window is unreachable from every \
             other window's palette, and since the raise is best-effort on Wayland anyway, an \
             absent row leaves the user no way to ask at all. Rows were: {:?}",
            rows.iter().map(|row| &row.display).collect::<Vec<_>>()
        );

        // --- An empty-state window: the shape a fresh New Window has -------------
        let untitled = [PaletteWindow {
            index: UNTITLED_INDEX,
            cwd: String::new(),
        }];
        let rows = palette_rows(&[], &untitled, &[], "", PALETTE_MAX_ROWS);
        let row = rows
            .iter()
            .find(|row| row.kind == PaletteRowKind::SwitchWindow)
            .unwrap_or_else(|| {
                panic!(
                    "a window whose persisted directory is \"\" — the state a window opened by \
                     New Window is in until it is given a project — produced no SwitchWindow row \
                     at all, so the window the user just opened is the one they cannot get back \
                     to. Rows were: {:?}",
                    rows.iter().map(|row| &row.display).collect::<Vec<_>>()
                )
            });
        assert_eq!(
            row.display, "Switch to Window: Untitled",
            "an empty-state window is labelled {:?}. `rank_palette` renders an empty cwd as \
             `Untitled`, and that is the label this front-end depends on: the raw string would \
             render as a bare prefix with nothing after it, and a row a user cannot name is one \
             they cannot type a query for.",
            row.display
        );
        assert_eq!(
            row.window_index,
            Some(UNTITLED_INDEX),
            "the empty-state window's row carries window index {:?}, not {UNTITLED_INDEX:?}. A \
             window with no project still has an id, and it is the one thing that row needs.",
            row.window_index
        );
    }

    /// D2's "type what you see" property, made executable: the string the palette
    /// RANKS is the string it SHOWS, so a query taken verbatim from the visible label
    /// selects that row.
    ///
    /// ## No revert exists, and none is to be invented
    ///
    /// Option (a) — reuse `PaletteRow::display` unchanged — is the ABSENCE of a
    /// post-rank label rewrite, so there is no production line to delete. This test
    /// reddens under [`palette_rows_include_recent_dirs`]' revert and adds no
    /// independent discrimination today; what it guards is a FUTURE edit that
    /// prettifies the label (a `~` abbreviation, a basename, an ellipsis in the
    /// string rather than in Pango) while leaving the ranked string alone. Since
    /// `rank_matches` is a subsequence matcher over `display`, such an edit would
    /// return zero rows for a character the user can plainly see.
    ///
    /// The first assertion is a precondition with no revert either: it establishes
    /// that the chosen basename matches no registry command, so "exactly one row"
    /// below is a statement about the seeded directory rather than an accident of the
    /// query.
    #[test]
    fn open_recent_row_is_matched_by_its_visible_label() {
        // A basename no COMMAND_SPECS title contains as a case-insensitive
        // subsequence — the property the first assertion below checks rather than
        // assumes.
        const DISTINCTIVE: &str = "zephyrdock";

        let dir = PathBuf::from(format!("/proj/{DISTINCTIVE}"));

        let registry_only = palette_rows(&[], &[], &[], DISTINCTIVE, PALETTE_MAX_ROWS);
        assert!(
            registry_only.is_empty(),
            "with NO recent directories the query {DISTINCTIVE:?} already matches {} registry \
             row(s): {:?}. The \"exactly one row\" assertion below would then be measuring the \
             registry rather than the seeded folder, so pick a basename that matches no command \
             title — do not relax the count.",
            registry_only.len(),
            registry_only
                .iter()
                .map(|row| &row.display)
                .collect::<Vec<_>>()
        );

        let rows = palette_rows(
            std::slice::from_ref(&dir),
            &[],
            &[],
            DISTINCTIVE,
            PALETTE_MAX_ROWS,
        );
        assert_eq!(
            rows.len(),
            1,
            "`palette_rows` returned {} row(s) for the query {DISTINCTIVE:?} with {dir:?} seeded, \
             not 1. ZERO is the regression this test exists for: the ranked string and the shown \
             string have diverged, so the user typed exactly what the palette drew and the row \
             disappeared. Rows were: {:?}",
            rows.len(),
            rows.iter().map(|row| &row.display).collect::<Vec<_>>()
        );

        let row = &rows[0];
        assert_eq!(
            row.path.as_deref(),
            Some(dir.as_path()),
            "the sole row matching {DISTINCTIVE:?} carries path {:?}, not the seeded {dir:?}.",
            row.path
        );
        assert!(
            row.display.contains(DISTINCTIVE),
            "the sole row matching {DISTINCTIVE:?} is labelled {:?}, which does not contain the \
             query VERBATIM. It was still matched, so the ranked string does contain it — which \
             means the two have come apart and the label is being rewritten after ranking. That \
             is the divergence D2 exists to prevent: `wolf_core::palette`'s own doc calls \
             `display` \"the label shown in the palette (and the string ranked against the \
             query)\", so a rewrite here also makes a core doc false.",
            row.display
        );
    }

    /// The Open Recent cage smoke's premise, made executable — the sibling of
    /// [`palette_rows_query_new_ranks_the_new_buffer_row_first`], for the folder
    /// row instead of the command row.
    ///
    /// `open_recent_palette_row_reroots_the_window_under_cage` seeds ONE directory
    /// into `folder-mru.sqlite`, types that directory's basename, and presses Enter on
    /// whatever `restored_index` selected. If the query ever matched a second row the
    /// smoke would stop being deterministic — it would still pass or fail, but on
    /// whichever row won the ranking tie-breaks.
    ///
    /// ## Which assertions here can fail, and how
    ///
    /// Assertions 2-4 redden under
    /// [`palette_rows_include_recent_dirs`]' revert (restore `rank_palette(&[], &[],
    /// …)`), because with the argument dropped the query matches nothing at all.
    ///
    /// Assertion 1 is a **precondition with no revert**: it says the smoke's query
    /// matches no registry command, which is a property of the chosen word and of
    /// `COMMAND_SPECS`, not of any line in this crate.
    ///
    /// Assertion 4's key SHAPE is likewise a precondition. `wolf_core::rank_palette`
    /// builds `open_recent:<path>`; nothing in `wolf-gnome` constructs or rewrites it,
    /// so there is no local edit that could change it. The assertion is the JOIN that
    /// stops the smoke's literal and core's format drifting apart silently — it is
    /// not a discriminator of anything this crate does.
    #[test]
    fn palette_rows_query_orzulu_selects_exactly_the_recent_folder_row() {
        // The smoke's target directory is a child of the app's HOME named after the
        // query; only the basename matters here.
        let dir = PathBuf::from(format!("/proj/{SMOKE_RECENT_QUERY}"));

        // Assertion 1 (precondition): the query is unique because of the SEEDED
        // FOLDER, not because the palette happens to be small.
        let registry_only = palette_rows(&[], &[], &[], SMOKE_RECENT_QUERY, PALETTE_MAX_ROWS);
        assert!(
            registry_only.is_empty(),
            "the smoke's query {SMOKE_RECENT_QUERY:?} matches {} registry row(s) with NO recent \
             directories seeded: {:?}. The smoke presses Enter on the top row and asserts the \
             window re-rooted, so a matching COMMAND row would make which row runs depend on \
             ranking tie-breaks — and a Close or Quit row would tear the window down instead. \
             Rename the smoke's `OPENRECENT_TARGET_DIR` (which is also its `OPENRECENT_QUERY`) \
             rather than relaxing this.",
            registry_only.len(),
            registry_only
                .iter()
                .map(|row| &row.display)
                .collect::<Vec<_>>()
        );

        // Assertion 2: with the store holding exactly the seeded directory, the
        // query yields exactly its row.
        let rows = palette_rows(
            std::slice::from_ref(&dir),
            &[],
            &[],
            SMOKE_RECENT_QUERY,
            PALETTE_MAX_ROWS,
        );
        assert_eq!(
            rows.len(),
            1,
            "`palette_rows` returned {} row(s) for the smoke's query {SMOKE_RECENT_QUERY:?} with \
             {dir:?} seeded, not 1. ZERO means the seeded directory never became a row, so the \
             smoke would type its query into an empty list and Enter would do nothing at all — \
             its readback would report the launch cwd and it would fail with no visible cause. \
             Rows were: {:?}",
            rows.len(),
            rows.iter().map(|row| &row.display).collect::<Vec<_>>()
        );

        // Assertion 3: it is the FOLDER row, carrying the seeded path.
        let row = &rows[0];
        assert_eq!(
            row.kind,
            PaletteRowKind::OpenRecent,
            "the sole row matching {SMOKE_RECENT_QUERY:?} has kind {:?}, not OpenRecent. The \
             smoke asserts Neovim's working directory CHANGED, which only an Open Recent \
             activation produces.",
            row.kind
        );
        assert_eq!(
            row.path.as_deref(),
            Some(dir.as_path()),
            "the sole row matching {SMOKE_RECENT_QUERY:?} carries path {:?}, not the seeded \
             {dir:?}. The smoke compares the canonicalized readback against the directory it \
             seeded, so a different path here is a different directory there.",
            row.path
        );

        // Assertion 4: the opaque MRU key the smoke reads back out of SQLite.
        let want_key = format!("{SMOKE_RECENT_MRU_PREFIX}{}", dir.to_string_lossy());
        assert_eq!(
            row.mru_key, want_key,
            "the sole row matching {SMOKE_RECENT_QUERY:?} records {:?} on activation, not \
             {want_key:?}. The cage smoke reads `command-mru.sqlite` back and looks for exactly \
             `open_recent:<canonical target>`; that format belongs to \
             `wolf_core::rank_palette`, so a mismatch here means core's key shape moved and the \
             smoke's literal must move with it — this assertion is the join between the two \
             crates, not a check on anything wolf-gnome does.",
            row.mru_key
        );
    }
}
