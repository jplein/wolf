//! The pure, GTK-free rulebook for NATIVE TABS in `wolf-gnome`: which of a
//! window's tabs a render wake may touch, what closing one does, whether a New Tab
//! press may be issued at all, the working directory a new tab inherits, and the
//! string one tab draws in the bar.
//!
//! The tabs counterpart of [`crate::nav_ui`] and [`crate::clipboard_ui`]. Data in,
//! data out — no widget type, no `adw::TabView`, no `WolfCore` — which is exactly
//! what lets the whole rulebook be unit-tested headlessly, the same way
//! [`crate::command_action`], [`crate::liveness_ui`] and [`crate::title`] keep
//! their decisions pure. The widget work of carrying the answers out lives in
//! `tab_view.rs` and [`crate::shell`].
//!
//! ## The selection gate makes a BACKGROUND tab's hazard unreachable
//!
//! One window owns ONE captive Neovim, and N tabs share it. `WolfCore::pane_status`
//! joins a grid to `Snapshot::windows`, and `windows` describes the CURRENT tabpage
//! alone: ask it about a background tab's grid and it answers a filename of
//! `[No Name]` with an EMPTY absolute path, because that tab's Neovim windows are
//! not in the map at all. What that damages is the background tab's **statusbar
//! text, its title and its grid resize** — everything [`TabWake::Refresh`] drives.
//! It does not reach `state.json`: the persistence gather reads
//! `Snapshot::previews` and `Snapshot::win_buffers`, which ARE global across
//! tabpages, keyed by each pane's own Neovim window handle
//! ([`crate::pane_layout::resolve_leaves`]).
//!
//! [`tab_wake`] answers by REFUSING. A tab whose handle is not the one Neovim
//! reports as current is never refreshed, so it is never asked for a pane status
//! and never receives the wrong answer. That is a hazard made UNREACHABLE rather
//! than handled, and it is why [`TabWake::Hold`] does nothing at all — no pane
//! refresh, no title, no status read, no resize. A held tab keeps its last paint,
//! which costs nothing: GTK never asks an unmapped background page's
//! `DrawingArea` to draw, and the background grid it would have drawn is frozen
//! rather than pruned.
//!
//! **The authority is Neovim, not the widget.** Both directions of the
//! one-wake-behind window are classified: a tab GTK has already selected while
//! Neovim is still on the previous tabpage HOLDS (refreshing it would read the
//! other tab's `Snapshot::windows`), and a tab Neovim considers current while GTK's
//! selection has not caught up REFRESHES. `selected` is consulted only where the
//! tab has no handle of its own yet.
//!
//! ## Two adjacent `i64` parameters
//!
//! [`tab_wake`] takes `current_tabpage` FIRST and `my_tabpage` second. The two are
//! trivially transposable at a call site and the compiler cannot help; what catches
//! a swap is that the rule is ASYMMETRIC — an unbound selected tab adopts, a bound
//! tab whose handle differs holds — so a transposed call turns every first bind
//! into a hold and freezes the window before its first paint.
//!
//! ## The last tab closing is the WINDOW closing
//!
//! `close_tabpage` on the last remaining tabpage is a measured NO-OP: Neovim
//! refuses to close the final tabpage and the `pcall` in `runtime/nvim/init.lua`
//! swallows the error, so liveness, the current tabpage and every grid are
//! unchanged afterwards. [`close_outcome`] therefore routes that case to
//! [`TabClose::CloseWindow`], which runs the window's own close handler and shuts
//! the core down. The wrong answer here does not crash and does not print: it
//! leaves the window holding a live Neovim behind a page that quietly refuses to
//! close.
//!
//! ## The blocking call, and totality
//!
//! `WolfCore::create_tabpage` is a bounded BLOCKING RPC on the GTK main thread, so
//! [`new_tab_press`] is liveness-gated exactly the way
//! [`crate::buffer_picker::open_decision`], [`crate::nav_ui::nav_press`] and
//! [`crate::clipboard_ui::copy_decision`] are, and for the same reason. The
//! inventory of this crate's blocking calls and the reasoning behind the gate live
//! in `crates/wolf-gnome/DECISIONS.md`, "Crash and stall recovery", and are
//! deliberately not restated here.
//!
//! [`new_tab_press`] matches `Option<Liveness>` with **no `_` arm**, so appending a
//! variant to [`wolf_core::Liveness`] is an `E0004` compile error here until it is
//! classified. `Option<Liveness>` (not `Liveness`) is the input type precisely so
//! the ABSENT core, which the window's close handler leaves behind, is a classified
//! arm rather than an early return no headless test could see.
//!
//! ## The `0` handle is a live value, not a hypothetical
//!
//! `WolfCore::create_tabpage` returns `0` on an RPC error or timeout, and a tab
//! carries `0` from the moment its page exists until [`tab_wake`] adopts a handle.
//! Neovim reads a tabpage handle of `0` as "the current tabpage", so
//! `set_current_tabpage(0)` binds the switch to whichever tabpage Neovim happens to
//! be on and `close_tabpage(0)` aims at the same. [`switch_target`] is the one
//! place that refusal is stated, following
//! [`crate::nav_ui::nav_query_win`]'s and [`crate::buffer_picker::switch_target`]'s
//! rule.

use std::path::{Path, PathBuf};

use wolf_core::Liveness;

/// The title a tab draws when its pane shows no named file.
///
/// Deliberately NOT Neovim's own `[No Name]`: that exact string is what
/// `WolfCore::pane_status` answers for a tab the selection gate should have held,
/// so reusing it would make a genuinely unnamed buffer indistinguishable from the
/// gate having leaked. [`crate::title::WINDOW_TITLE_FALLBACK`] is the same idea one
/// widget up.
pub const TAB_TITLE_FALLBACK: &str = "Untitled";

/// The handle a tab carries before [`tab_wake`] has adopted one, and the handle
/// `WolfCore::create_tabpage` answers on an RPC error or timeout.
///
/// Neovim reads it as "the current tabpage" rather than as "no tabpage", which is
/// what [`switch_target`] refuses and what makes the `my_tabpage == 0` half of
/// [`tab_wake`] a bind rather than a comparison.
const UNBOUND_TABPAGE: i64 = 0;

/// What ONE render wake does to ONE tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabWake {
    /// Bind the carried handle to this tab, then refresh it.
    ///
    /// The handle is `Snapshot::current_tabpage` — a cheap snapshot read, never
    /// `WolfCore::current_tabpage()`, which is a blocking RPC.
    Adopt(i64),
    /// Refresh this tab: its panes, then its title.
    Refresh,
    /// Do NOTHING at all — no pane refresh, no title, no status read, no resize.
    ///
    /// See the module docs: this arm is the whole gate.
    Hold,
}

/// What closing one tab's page does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabClose {
    /// Close the tab's Neovim tabpage and deregister the page.
    CloseTabpage,
    /// Close the whole WINDOW, which shuts the core down. See the module docs:
    /// `close_tabpage` on the last tabpage is a no-op.
    CloseWindow,
}

/// What one render wake does to a window's QUEUED restore tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorePress {
    /// Materialize every queued tab now: one blocking `create_tabpage` and one
    /// blocking `rebuild_layout` each.
    Create,
    /// Not yet. Leave the queue where it is; a later wake asks again.
    Wait,
    /// Nothing left to materialize: nothing is queued, or the restore was ABANDONED
    /// by an abort and what remains queued is a seed source rather than work. The
    /// steady state, on every wake for the rest of the session.
    Idle,
}

/// What pressing New Tab does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewTabPress {
    /// Issue the BLOCKING `create_tabpage` RPC and append a page.
    Open,
    /// Do nothing and issue no RPC; the string names why, as a mid-sentence clause
    /// (the [`crate::command_action`] / [`crate::nav_ui`] convention).
    Refuse(&'static str),
}

/// What a render wake does to the tab whose handle is `my_tabpage`, given the
/// tabpage Neovim reports as current and whether GTK has this page selected.
///
/// `current_tabpage` is `Snapshot::current_tabpage` — `0` until Neovim's first
/// `wolf_tabpage` notification. `my_tabpage` is `0` until this tab has adopted one.
/// Note the parameter ORDER; see the module docs on why a swap is caught by the
/// asymmetry of the rule rather than by the compiler.
///
/// The rule, total over every `(my_tabpage, current_tabpage, selected)`:
///
/// | `my_tabpage` | `current_tabpage` | `selected` | answer |
/// | --- | --- | --- | --- |
/// | `0` | `!= 0` | `true` | [`TabWake::Adopt`]`(current)` |
/// | `0` | `0` | `true` | [`TabWake::Refresh`] |
/// | `0` | any | `false` | [`TabWake::Hold`] |
/// | `!= 0` | `== my` | any | [`TabWake::Refresh`] |
/// | `!= 0` | `!= my` | any | [`TabWake::Hold`] |
///
/// An unbound tab that is NOT selected holds rather than adopting, because the
/// handle Neovim reports belongs to whichever tab IS current — two pages claiming
/// one tabpage would put both of them through the refresh path the gate exists to
/// keep them out of.
#[must_use]
pub fn tab_wake(current_tabpage: i64, my_tabpage: i64, selected: bool) -> TabWake {
    match (my_tabpage, selected) {
        // An unbound BACKGROUND page must not adopt: the handle Neovim reports
        // belongs to whichever tab IS current.
        (UNBOUND_TABPAGE, false) => TabWake::Hold,
        // Launch, before Neovim's first `wolf_tabpage`: there is nothing to adopt
        // yet, and delaying first paint behind a notification would show an empty
        // window.
        (UNBOUND_TABPAGE, true) if current_tabpage == UNBOUND_TABPAGE => TabWake::Refresh,
        // The first bind, and the post-restart re-bind. The handle is passed
        // THROUGH from the snapshot; nothing here derives one.
        (UNBOUND_TABPAGE, true) => TabWake::Adopt(current_tabpage),
        // A bound tab acts only while Neovim is on it. The authority is Neovim, not
        // GTK's selection, so `selected` is not consulted below this point.
        _ if my_tabpage == current_tabpage => TabWake::Refresh,
        _ => TabWake::Hold,
    }
}

/// The tabpage handle to hand `WolfCore::set_current_tabpage`, or `None` to issue
/// no switch at all.
///
/// `0` is refused: Neovim reads it as "the current tabpage", so a switch aimed at
/// it silently binds to whatever tabpage is already current instead of the one the
/// user selected. A tab still carries `0` before its first
/// [`TabWake::Adopt`], and `WolfCore::create_tabpage` itself answers `0` on an RPC
/// error or timeout, so this is a value that genuinely arrives — not only a stated
/// rule. Same guard as [`crate::nav_ui::nav_query_win`] and
/// [`crate::buffer_picker::switch_target`].
#[must_use]
pub fn switch_target(my_tabpage: i64) -> Option<i64> {
    (my_tabpage != UNBOUND_TABPAGE).then_some(my_tabpage)
}

/// What closing one page does, given how many pages the view still holds.
///
/// `live_pages` COUNTS THE PAGE BEING CLOSED — it is read inside the `close-page`
/// handler, before the entry is deregistered — so `1` means "this is the last tab".
/// One and zero both answer [`TabClose::CloseWindow`]; see the module docs for why
/// the alternative is a silent no-op rather than an error.
#[must_use]
pub fn close_outcome(live_pages: usize) -> TabClose {
    if live_pages <= 1 {
        TabClose::CloseWindow
    } else {
        TabClose::CloseTabpage
    }
}

/// Whether a New Tab press may issue its `create_tabpage` RPC at all, given the
/// core's liveness reading (or `None` when there is no core).
///
/// TOTAL by construction: a `match` over `Option<Liveness>` with **no `_` arm**
/// (see the module docs). Only [`Liveness::Alive`] yields [`NewTabPress::Open`];
/// every other input refuses, prints its reason, and issues no RPC.
#[must_use]
pub fn new_tab_press(liveness: Option<Liveness>) -> NewTabPress {
    match liveness {
        Some(Liveness::Alive) => NewTabPress::Open,
        Some(Liveness::NotResponding) => NewTabPress::Refuse(
            "the captive Neovim is already known not to be responding, and create_tabpage is a \
             blocking RPC that would freeze the window for another whole timeout budget",
        ),
        Some(Liveness::Dead) => NewTabPress::Refuse(
            "the captive Neovim has died, so there is no instance left to create a tabpage in and \
             an RPC against a dead handle is not reliably fast",
        ),
        None => NewTabPress::Refuse(
            "the window is shutting down and the core has already been taken out by its close \
             handler, so there is nothing left to open a tab over",
        ),
    }
}

/// Whether a window's QUEUED restore tabs may be materialized on this wake, given
/// the FIRST tab's tabpage handle, the core's liveness reading (or `None` when
/// there is no core), how many tabs are still queued and whether creation may be
/// attempted for them at all.
///
/// Total, and the three answers are not interchangeable:
///
/// | `pending` | `may_create` | `first_tabpage` | `liveness` | answer |
/// | --- | --- | --- | --- | --- |
/// | `0` | any | any | any | [`RestorePress::Idle`] |
/// | `> 0` | `false` | any | any | [`RestorePress::Idle`] |
/// | `> 0` | `true` | `0` | any | [`RestorePress::Wait`] |
/// | `> 0` | `true` | `!= 0` | not `Alive` | [`RestorePress::Wait`] |
/// | `> 0` | `true` | `!= 0` | `Alive` | [`RestorePress::Create`] |
///
/// Each conjunct answers a different failure:
///
/// - **`may_create`.** A restore whose `create_tabpage` has already answered `0` is
///   ABANDONED: `tab_view::TabsInner::materialize_restore` re-seats the
///   layouts it never reached as a seed source and clears this flag, so the queue
///   goes on speaking for those tabs in `state.json` while no further RPC is ever
///   issued for them. Without it, a retained queue would be retried on the very next
///   wake and on every wake after it — a blocking `create_tabpage` per frame against
///   a Neovim that has just failed to answer one.
/// - **`first_tabpage != 0`.** Creating before tab 0 has adopted leaves tab 0 to
///   adopt the LAST tabpage created on a later wake, because
///   `Snapshot::current_tabpage` follows Neovim into each new page — two pages
///   claiming one tabpage, which is [`crate::tab_view::TabParts::tabpage`]'s
///   recorded hazard, and from which every later switch and close aims at the wrong
///   tab.
/// - **`Alive`.** Materialization is `T - 1` blocking `create_tabpage` calls plus
///   up to `T` blocking `rebuild_layout` calls on the GTK main thread; issuing them
///   against a Neovim already known not to answer freezes the window for a multiple
///   of the timeout budget at the one moment the user is watching it start.
/// - **`Wait` rather than `Idle` on a refusal.** `Idle` asserts there is nothing
///   left to materialize. For a queue that may still be created that is false —
///   those tabs have no pages, so a queue declared empty is tabs that never open,
///   and nothing re-queues them. For an ABANDONED queue it is true, which is why
///   that row answers `Idle`: `Wait` promises a later wake will finish the restore,
///   and after an abort none ever will. What survives either refusal is the SAVE,
///   because the queue itself is what speaks for those tabs —
///   `window::WindowHandle::cached_subtree` contributes a seed per queued layout
///   ([`crate::state_seam::pending_tab_states`]) for as long as one is held.
#[must_use]
pub fn restore_press(
    first_tabpage: i64,
    liveness: Option<Liveness>,
    pending: usize,
    may_create: bool,
) -> RestorePress {
    if pending == 0 || !may_create {
        return RestorePress::Idle;
    }
    if first_tabpage == UNBOUND_TABPAGE || liveness != Some(Liveness::Alive) {
        return RestorePress::Wait;
    }
    RestorePress::Create
}

/// The directory to hand `WolfCore::create_tabpage`, or `None` to create the
/// tabpage with no `:tcd` at all.
///
/// A new tab inherits the WINDOW's folder, which is what keeps every tab of a
/// window rooted where its sidebar, picker and project search already are.
///
/// The EMPTY state answers `None` ALWAYS — never the fallback working directory a
/// project-less launch handed Neovim, which is `$HOME` (or `/`). Rooting a new tab
/// there with `:tcd` would silently declare the user's home directory a project.
/// With a project open it is Neovim's LIVE cwd once reported, else the launch cwd,
/// so a New Tab pressed before the first `DirChanged` does not lose the project.
///
/// The same three rules as [`crate::state_seam::snapshot_cwd`], which persists the
/// window's cwd — with `None` where that one answers `""`. The two must agree, or a
/// restored window would open a different directory than its tabs are rooted at.
#[must_use]
pub fn new_tab_cwd(empty: bool, launch_cwd: &Path, live_cwd: Option<&Path>) -> Option<PathBuf> {
    if empty {
        return None;
    }
    Some(live_cwd.unwrap_or(launch_cwd).to_path_buf())
}

/// The text one tab draws in the bar: the FILE NAME of the pane's current file, or
/// [`TAB_TITLE_FALLBACK`] when there is none.
///
/// Never a path. A tab bar allots each tab a small fraction of the window's width,
/// so an absolute path renders as an ellipsised middle with neither the project nor
/// the file legible. The same final-component rule as
/// [`crate::workspace::root_label`], with a different fallback: a path with no final
/// component (`/`, or the empty path) carries no name to show, so it falls back
/// rather than drawing the separator.
#[must_use]
pub fn tab_title(current_file: Option<&Path>) -> String {
    current_file.and_then(Path::file_name).map_or_else(
        || TAB_TITLE_FALLBACK.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::ffi::OsStr;

    use crate::state_seam::snapshot_cwd;

    // =========================================================================
    // The whole input domain
    // =========================================================================

    /// The handle a tab carries before it has adopted one, and the handle
    /// `WolfCore::create_tabpage` answers on an RPC error. Neovim reads it as "the
    /// current tabpage", which is why [`switch_target`] refuses it.
    const UNBOUND: i64 = 0;

    /// One real Neovim tabpage handle.
    ///
    /// Deliberately not `1`: Neovim's FIRST tabpage is `1`, so a rule that
    /// fabricated "the first tabpage" rather than passing a handle through is
    /// visible in the failure message.
    const TABPAGE_A: i64 = 3;

    /// A SECOND real Neovim tabpage handle, in the same window and over the same
    /// core. Distinct from [`TABPAGE_A`] and from [`UNBOUND`], so a rule stated
    /// about one handle cannot be confused with a rule stated about "some handle".
    const TABPAGE_B: i64 = 2;

    /// Every shape a tabpage handle can take on either axis of [`tab_wake`]: no
    /// handle at all, this tab's, and another tab's.
    const ALL_TABPAGE_INPUTS: [i64; 3] = [UNBOUND, TABPAGE_A, TABPAGE_B];

    /// Both readings of GTK's own selection.
    const ALL_SELECTED_INPUTS: [bool; 2] = [true, false];

    /// How many inputs [`tab_wake`] is total over: three handles for the tab, times
    /// three for Neovim's current tabpage, times two selection readings.
    ///
    /// Written as LITERALS rather than as the product of the two domain consts'
    /// lengths, and that is the whole point of it — the `nav_ui::tests::SWEEP_INPUTS`
    /// and `clipboard_ui::tests::NAMED_LIVENESS_INPUTS` precedent. Comparing a
    /// sweep's iteration count to the product of the arrays it walked is a
    /// TAUTOLOGY: an array that lost an entry shrinks both sides together, so the
    /// check passes while the sweep covers less than the domain it claims to.
    const NAMED_WAKE_CASES: usize = 3 * 3 * 2;

    /// Every input [`new_tab_press`] is total over: the three readings a core can
    /// produce, plus the ABSENT core (`None`) the window's close handler leaves
    /// behind.
    const ALL_LIVENESS_INPUTS: [Option<Liveness>; 4] = [
        Some(Liveness::Alive),
        Some(Liveness::NotResponding),
        Some(Liveness::Dead),
        None,
    ];

    /// How many inputs [`new_tab_press`] is total over, as a LITERAL. Same
    /// reasoning as [`NAMED_WAKE_CASES`]; it is also the number that makes the
    /// `_`-free match legible from the test side, since four arms and four rows
    /// means every arm is measured.
    const NAMED_LIVENESS_INPUTS: usize = 4;

    /// The fallback working directory a project-less launch hands Neovim. Never
    /// the answer [`new_tab_cwd`] may give.
    const HOME: &str = "/home/jane";

    /// The directory a window with a project open was launched at.
    const PROJECT: &str = "/home/jane/ledger";

    /// A directory Neovim reported LATER, after a `:cd`. Distinct from
    /// [`PROJECT`], so "the live cwd wins" is distinguishable from "the launch cwd
    /// is returned".
    const WANDERED: &str = "/home/jane/ledger/vendor";

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a mid-sentence
    /// clause. Copied from `nav_ui.rs`'s helper of the same name, including its
    /// deliberate allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    // =========================================================================
    // tab_wake — the selection gate
    // =========================================================================

    /// One [`tab_wake`] table row.
    struct WakeCase {
        desc: &'static str,
        my_tabpage: i64,
        current_tabpage: i64,
        selected: bool,
        want: TabWake,
    }

    /// The eighteen rows of the gate, one per `(my_tabpage, current_tabpage,
    /// selected)` input.
    ///
    /// A `const` item rather than a local so the sweeps below can walk the same
    /// rows without copying them.
    const WAKE_CASES: &[WakeCase] = &[
        // --- An UNBOUND tab: it has no handle of its own yet ------------------
        WakeCase {
            desc: "launch, before Neovim's first wolf_tabpage: the one selected tab refreshes, \
                   so first paint is not delayed behind a notification",
            my_tabpage: UNBOUND,
            current_tabpage: UNBOUND,
            selected: true,
            want: TabWake::Refresh,
        },
        WakeCase {
            desc: "an unbound tab that is NOT selected holds: there is nothing to adopt and \
                   nothing says it is the tab Neovim is on",
            my_tabpage: UNBOUND,
            current_tabpage: UNBOUND,
            selected: false,
            want: TabWake::Hold,
        },
        WakeCase {
            desc: "THE FIRST BIND: a selected unbound tab adopts the handle Neovim reports, \
                   from the snapshot rather than from a blocking RPC",
            my_tabpage: UNBOUND,
            current_tabpage: TABPAGE_A,
            selected: true,
            want: TabWake::Adopt(TABPAGE_A),
        },
        WakeCase {
            desc: "an unbound BACKGROUND tab must not adopt: that handle belongs to whichever \
                   tab is current, and two pages claiming it would both refresh",
            my_tabpage: UNBOUND,
            current_tabpage: TABPAGE_A,
            selected: false,
            want: TabWake::Hold,
        },
        WakeCase {
            desc: "the adopted handle is whatever Neovim reports NOW, not a remembered or \
                   derived one",
            my_tabpage: UNBOUND,
            current_tabpage: TABPAGE_B,
            selected: true,
            want: TabWake::Adopt(TABPAGE_B),
        },
        WakeCase {
            desc: "and an unbound background tab holds for that handle too",
            my_tabpage: UNBOUND,
            current_tabpage: TABPAGE_B,
            selected: false,
            want: TabWake::Hold,
        },
        // --- A tab bound to TABPAGE_A -----------------------------------------
        WakeCase {
            desc: "a bound tab holds while Neovim has reported no tabpage at all: nothing says \
                   this is the one on screen",
            my_tabpage: TABPAGE_A,
            current_tabpage: UNBOUND,
            selected: true,
            want: TabWake::Hold,
        },
        WakeCase {
            desc: "same, unselected",
            my_tabpage: TABPAGE_A,
            current_tabpage: UNBOUND,
            selected: false,
            want: TabWake::Hold,
        },
        WakeCase {
            desc: "Neovim's current tabpage, and GTK agrees: the ordinary refresh",
            my_tabpage: TABPAGE_A,
            current_tabpage: TABPAGE_A,
            selected: true,
            want: TabWake::Refresh,
        },
        WakeCase {
            desc: "Neovim's current tabpage while GTK's selection has NOT caught up still \
                   refreshes: the authority is Neovim, not the widget",
            my_tabpage: TABPAGE_A,
            current_tabpage: TABPAGE_A,
            selected: false,
            want: TabWake::Refresh,
        },
        WakeCase {
            desc: "GTK has selected this tab but Neovim is still on the OTHER one — the \
                   one-wake-behind window, and the row the whole gate exists for",
            my_tabpage: TABPAGE_A,
            current_tabpage: TABPAGE_B,
            selected: true,
            want: TabWake::Hold,
        },
        WakeCase {
            desc: "the plain background tab: a different handle, not selected",
            my_tabpage: TABPAGE_A,
            current_tabpage: TABPAGE_B,
            selected: false,
            want: TabWake::Hold,
        },
        // --- A tab bound to TABPAGE_B: the same rule, stated about the other
        //     handle, so nothing here is a property of TABPAGE_A's value ---------
        WakeCase {
            desc: "the second tab holds while Neovim has reported no tabpage",
            my_tabpage: TABPAGE_B,
            current_tabpage: UNBOUND,
            selected: true,
            want: TabWake::Hold,
        },
        WakeCase {
            desc: "same, unselected",
            my_tabpage: TABPAGE_B,
            current_tabpage: UNBOUND,
            selected: false,
            want: TabWake::Hold,
        },
        WakeCase {
            desc: "the second tab holds while Neovim is on the first, even when GTK has \
                   selected it",
            my_tabpage: TABPAGE_B,
            current_tabpage: TABPAGE_A,
            selected: true,
            want: TabWake::Hold,
        },
        WakeCase {
            desc: "the second tab as a plain background tab",
            my_tabpage: TABPAGE_B,
            current_tabpage: TABPAGE_A,
            selected: false,
            want: TabWake::Hold,
        },
        WakeCase {
            desc: "the second tab refreshes when Neovim is on it and GTK agrees",
            my_tabpage: TABPAGE_B,
            current_tabpage: TABPAGE_B,
            selected: true,
            want: TabWake::Refresh,
        },
        WakeCase {
            desc: "and when Neovim is on it while GTK's selection lags",
            my_tabpage: TABPAGE_B,
            current_tabpage: TABPAGE_B,
            selected: false,
            want: TabWake::Refresh,
        },
    ];

    /// Pins the wake decision for EVERY `(my_tabpage, current_tabpage, selected)`
    /// input, and asserts the table is neither short nor made of duplicates.
    ///
    /// ## What it discriminates, and the edits that redden it
    ///
    /// FALSIFYING EDIT 1 — make the last arm answer [`TabWake::Refresh`], i.e. drop
    /// the gate. Both bound-tab rows whose handle differs from Neovim's current
    /// tabpage fail, the first being the one-wake-behind row.
    ///
    /// FALSIFYING EDIT 2 — make the `my_tabpage == 0 && current != 0` arm answer
    /// [`TabWake::Refresh`]. The two [`TabWake::Adopt`] rows report `Refresh`: the
    /// tab never learns its handle, so from the first switch onwards it holds
    /// forever with a live Neovim behind it.
    ///
    /// FALSIFYING EDIT 3 — transpose the two `i64` parameters at the call inside
    /// the seam. The first-bind rows report `Hold`.
    #[test]
    fn tab_wake_table() {
        assert_eq!(
            WAKE_CASES.len(),
            NAMED_WAKE_CASES,
            "the tab_wake table must carry exactly one row per input the gate is total over: \
             three handles for this tab, times three for Neovim's current tabpage, times two \
             selection readings, is {NAMED_WAKE_CASES} rows, not {}. A missing row means an input \
             nothing checks, and the row most likely to go missing is the one-wake-behind case — a \
             tab GTK has already selected while Neovim is still on the previous tabpage.",
            WAKE_CASES.len()
        );

        // Every row of the domain really is present, so the count above cannot be
        // satisfied by eighteen copies of one row.
        let mut seen: HashSet<(i64, i64, bool)> = HashSet::new();
        for case in WAKE_CASES {
            assert!(
                seen.insert((case.my_tabpage, case.current_tabpage, case.selected)),
                "two tab_wake rows carry the same (my_tabpage = {}, current_tabpage = {}, \
                 selected = {}) input, so the table is shorter than its length suggests and some \
                 other input is unmeasured — {}",
                case.my_tabpage,
                case.current_tabpage,
                case.selected,
                case.desc
            );
        }
        for my_tabpage in ALL_TABPAGE_INPUTS {
            for current_tabpage in ALL_TABPAGE_INPUTS {
                for selected in ALL_SELECTED_INPUTS {
                    assert!(
                        seen.contains(&(my_tabpage, current_tabpage, selected)),
                        "no tab_wake row covers (my_tabpage = {my_tabpage}, current_tabpage = \
                         {current_tabpage}, selected = {selected}), so that input's answer is \
                         whatever the seam happens to do. Every one of these is reachable: a tab \
                         carries 0 until it adopts, Neovim's current tabpage is 0 until its first \
                         wolf_tabpage notification, and GTK's selection and Neovim's current \
                         tabpage disagree for at least one wake after every switch."
                    );
                }
            }
        }

        for case in WAKE_CASES {
            let got = tab_wake(case.current_tabpage, case.my_tabpage, case.selected);
            assert_eq!(
                got, case.want,
                "tab_wake(current_tabpage = {}, my_tabpage = {}, selected = {}) must be {:?}, got \
                 {got:?} — {}. A REFRESH that should have been a Hold asks WolfCore::pane_status \
                 about a grid whose Neovim window is not in Snapshot::windows at all, and gets a \
                 filename of \"[No Name]\" with an empty absolute path back: that background tab's \
                 statusbar text goes blank, its title follows the same read, and its grid is \
                 resized against a window list that is not its own. It does not reach state.json — \
                 the persistence gather resolves each leaf from Snapshot::previews and \
                 Snapshot::win_buffers, keyed by the pane's own Neovim window, and those maps are \
                 global across tabpages. A HOLD that should have been a Refresh freezes the tab at \
                 its last paint with a live Neovim behind it, and nothing on screen says so. An \
                 ADOPT that should not have happened binds a second page to a tabpage another page \
                 already owns.",
                case.current_tabpage, case.my_tabpage, case.selected, case.want, case.desc
            );
        }

        // --- Non-vacuity: all three answers are actually produced -------------
        //
        // BACKSTOPS, not the red a source edit produces. The row loop above dominates
        // all three for every single-arm edit, and dropping the gate's last arm leaves
        // the unbound-background row still answering Hold, so even the Hold check
        // survives it. What they catch is the table and the seam moving TOGETHER: a
        // rewrite that made `tab_wake` a constant function and rewrote every `want`
        // to agree would satisfy the length check, the coverage sweep and every row,
        // and only these three would say the gate had stopped deciding anything.
        //
        // The Adopt check keeps `matches!` because that variant carries a handle and
        // so has no single value to look for; the other two are plain membership.
        let answers: Vec<TabWake> = WAKE_CASES
            .iter()
            .map(|case| tab_wake(case.current_tabpage, case.my_tabpage, case.selected))
            .collect();

        assert!(
            answers.iter().any(|got| matches!(got, TabWake::Adopt(_))),
            "NOT ONE of the {NAMED_WAKE_CASES} inputs answered Adopt. A tab_wake that never \
             adopted would leave every tab's handle at 0 forever: the first bind never happens, \
             the post-restart re-bind never happens, and every wake after the first tab switch \
             answers Hold — a window frozen at its last paint with a live Neovim behind it."
        );
        assert!(
            answers.contains(&TabWake::Refresh),
            "NOT ONE of the {NAMED_WAKE_CASES} inputs answered Refresh. A tab_wake that answered \
             Hold for everything would satisfy every Hold row above while no tab in any window \
             ever repainted, took a resize, or updated its title."
        );
        assert!(
            answers.contains(&TabWake::Hold),
            "NOT ONE of the {NAMED_WAKE_CASES} inputs answered Hold, so the selection gate is not \
             stated at all. This is NOT where dropping the gate's last arm reddens — an unbound \
             background tab still holds through the FIRST arm, so the row loop above is what goes \
             there. With no Hold arm at all, every background tab is refreshed against the CURRENT \
             tabpage's Snapshot::windows, which does not contain that tab's Neovim windows: \
             pane_status answers \"[No Name]\" with an empty absolute path, so every background \
             tab's statusbar text goes blank, its title follows that same read, and its grid takes \
             a resize computed against another tabpage's windows. What that costs is the \
             background tab's PAINT — its statuses, its title and its geometry — and not \
             state.json: the persistence gather resolves each leaf from the window-keyed maps by \
             the pane's own Neovim window, and those are global across tabpages."
        );
    }

    /// The gate as ONE rule over the whole domain, so a failure names the rule
    /// rather than quoting a row index — the
    /// `nav_ui::only_a_live_core_with_a_real_window_may_move` convention.
    ///
    /// A tab that Neovim is not currently on is never acted on, unless it has no
    /// handle at all and is therefore asking to be bound. The complementary
    /// non-vacuity assertion is what stops a `tab_wake` of `|_, _, _| Hold`
    /// satisfying it while freezing every window.
    ///
    /// That guard is REACHED, unlike the trailing ones in [`close_outcome_table`] and
    /// [`switch_target_refuses_zero`]: this test carries no expected-answer table, so
    /// there is no earlier row for a constant-Hold seam to fail on and the sweep runs
    /// to the bottom. A guard sitting after a table of expectations is a row-deletion
    /// backstop instead — see the comments at those two.
    #[test]
    fn a_tab_neovim_is_not_on_is_never_acted_on() {
        let mut probed = 0_usize;
        let mut acted = 0_usize;

        for my_tabpage in ALL_TABPAGE_INPUTS {
            for current_tabpage in ALL_TABPAGE_INPUTS {
                for selected in ALL_SELECTED_INPUTS {
                    probed += 1;
                    let got = tab_wake(current_tabpage, my_tabpage, selected);
                    if got == TabWake::Hold {
                        continue;
                    }
                    acted += 1;

                    assert!(
                        my_tabpage == UNBOUND || my_tabpage == current_tabpage,
                        "tab_wake(current_tabpage = {current_tabpage}, my_tabpage = {my_tabpage}, \
                         selected = {selected}) answered {got:?}, which acts on a tab bound to a \
                         tabpage Neovim is NOT currently on. That refresh reads \
                         Snapshot::windows, which describes the current tabpage alone, so every \
                         one of this tab's panes resolves to no Neovim window: pane_status answers \
                         a filename of \"[No Name]\" with an empty absolute path, and the tab's \
                         statuses are replaced with that. The only tab allowed to act without \
                         matching is one with no handle at all ({UNBOUND}), which is asking to be \
                         bound rather than to be read."
                    );
                }
            }
        }

        assert_eq!(
            probed, NAMED_WAKE_CASES,
            "the sweep visited {probed} inputs, not the {NAMED_WAKE_CASES} the gate is total over. \
             It walked {} handle(s) for this tab x {} for Neovim's current tabpage x {} selection \
             reading(s); whichever of those is not 3, 3 and 2 is the one that changed. A SHRUNK \
             domain means the rule above is stated over less than the whole input space.",
            ALL_TABPAGE_INPUTS.len(),
            ALL_TABPAGE_INPUTS.len(),
            ALL_SELECTED_INPUTS.len()
        );
        assert!(
            acted > 0,
            "NOT ONE of the {probed} inputs produced anything but Hold, so the rule above held \
             vacuously and this test proves nothing. A tab_wake that answered Hold unconditionally \
             would pass it while every tab in every window froze at its last paint — and \
             `tab_wake_table` is the test that would be red alongside it."
        );
    }

    /// [`TabWake::Adopt`] happens exactly where a tab has no handle, GTK has it
    /// selected, and Neovim has reported one — and it carries NEOVIM's handle.
    ///
    /// The pass-through half is the discriminating one: an `Adopt(my_tabpage)`
    /// would bind every unbound tab to `0` forever, which reads as a permanent
    /// hold; an `Adopt` of a fabricated handle would bind the page to a tabpage
    /// nobody is on, so its every later switch and close aims at the wrong one.
    #[test]
    fn adopt_binds_the_handle_neovim_reports_and_only_an_unbound_selected_tab() {
        let mut adopting: Vec<(i64, i64, bool)> = Vec::new();

        for my_tabpage in ALL_TABPAGE_INPUTS {
            for current_tabpage in ALL_TABPAGE_INPUTS {
                for selected in ALL_SELECTED_INPUTS {
                    let TabWake::Adopt(handle) = tab_wake(current_tabpage, my_tabpage, selected)
                    else {
                        continue;
                    };
                    adopting.push((my_tabpage, current_tabpage, selected));

                    assert_eq!(
                        handle, current_tabpage,
                        "tab_wake(current_tabpage = {current_tabpage}, my_tabpage = {my_tabpage}, \
                         selected = {selected}) adopted handle {handle}, which is not the one it \
                         was given. Neovim's handle is passed THROUGH, never derived: any other \
                         value binds this page to a tabpage it does not own, so its later \
                         set_current_tabpage switches to someone else's tab and its later \
                         close_tabpage closes someone else's."
                    );
                    assert_ne!(
                        handle, UNBOUND,
                        "tab_wake(current_tabpage = {current_tabpage}, my_tabpage = {my_tabpage}, \
                         selected = {selected}) adopted {UNBOUND}, which is not a tabpage. Neovim \
                         reads 0 as \"the current tabpage\", so the page would stay unbound while \
                         appearing bound, and switch_target would refuse every switch it ever made."
                    );
                }
            }
        }

        let expected = [(UNBOUND, TABPAGE_A, true), (UNBOUND, TABPAGE_B, true)];
        assert_eq!(
            adopting.as_slice(),
            expected.as_slice(),
            "exactly TWO of the {NAMED_WAKE_CASES} inputs may adopt — an unbound tab that GTK has \
             selected, for each of the two real handles Neovim might report. An EXTRA entry means \
             a tab that already has a handle overwrites it (which loses the tabpage it owns) or an \
             unselected page adopts the current tab's handle (which puts two pages on one tabpage, \
             and then both through the refresh path). An entry MISSING means a tab never binds at \
             all: it holds from the first switch onwards, forever."
        );
    }

    // =========================================================================
    // switch_target
    // =========================================================================

    /// One [`switch_target`] table row.
    struct SwitchCase {
        desc: &'static str,
        my_tabpage: i64,
        want: Option<i64>,
    }

    /// Which tabpage a page selection switches Neovim to, and the one handle that
    /// switches it to nothing.
    ///
    /// FALSIFYING EDIT: return `Some(my_tabpage)` unconditionally. The zero row
    /// fails; every other row stays green, which is why the zero row is the whole
    /// point of this table.
    #[test]
    fn switch_target_refuses_zero() {
        let cases = [
            SwitchCase {
                desc: "the unbound handle switches NOTHING: Neovim reads 0 as \"the current \
                       tabpage\", so the switch would bind to whatever is already on screen",
                my_tabpage: UNBOUND,
                want: None,
            },
            SwitchCase {
                desc: "Neovim's FIRST tabpage is a real handle and must switch, not be mistaken \
                       for the sentinel",
                my_tabpage: 1,
                want: Some(1),
            },
            SwitchCase {
                desc: "a real handle is passed THROUGH, never derived or rewritten",
                my_tabpage: TABPAGE_A,
                want: Some(TABPAGE_A),
            },
            SwitchCase {
                desc: "and so is another one",
                my_tabpage: TABPAGE_B,
                want: Some(TABPAGE_B),
            },
        ];

        for case in &cases {
            assert_eq!(
                switch_target(case.my_tabpage),
                case.want,
                "switch_target({}) must be {:?} — {}. A handle of 0 is what a tab carries before \
                 its first bind and what WolfCore::create_tabpage answers on an RPC error, so \
                 answering Some(0) sends nvim_set_current_tabpage(0), which Neovim resolves to \
                 whichever tabpage is already current: the click appears to do nothing and the \
                 page and the editor silently disagree about which tab is showing. Rewriting a \
                 real handle switches to the WRONG tab, which is worse than switching to none.",
                case.my_tabpage,
                case.want,
                case.desc
            );
        }

        // A ROW-DELETION BACKSTOP, not an assertion a source edit can make bite.
        // MEASURED: a `switch_target` of `|_| None` reddens the table above at its
        // SECOND row (`1 -> Some(1)`) and execution never reaches here. What is left
        // for it to catch is a table that kept only its zero row, which would let a
        // seam refusing everything look measured while every tab click became a no-op.
        assert_eq!(
            switch_target(TABPAGE_A),
            Some(TABPAGE_A),
            "switch_target must ACCEPT a real handle. A function that refused everything would \
             satisfy the zero row while making the GTK selection never reach Neovim at all — the \
             tab bar would move and the editor would not."
        );
    }

    // =========================================================================
    // close_outcome
    // =========================================================================

    /// One [`close_outcome`] table row.
    struct CloseCase {
        desc: &'static str,
        live_pages: usize,
        want: TabClose,
    }

    /// What closing a page does, for every count of pages the view might hold.
    ///
    /// FALSIFYING EDIT: test `live_pages == 0` instead of `live_pages <= 1`. The
    /// `1` row then reports [`TabClose::CloseTabpage`], which is the exact wrong
    /// answer: `close_tabpage` on the last tabpage is a measured no-op, so nothing
    /// at all happens and the window keeps a live Neovim behind a page that refuses
    /// to close.
    #[test]
    fn close_outcome_table() {
        let cases = [
            CloseCase {
                desc: "no pages at all: there is nothing left to be a window for",
                live_pages: 0,
                want: TabClose::CloseWindow,
            },
            CloseCase {
                desc: "THE ROW THIS TABLE EXISTS FOR — the page being closed is the only one, so \
                       closing it IS closing the window",
                live_pages: 1,
                want: TabClose::CloseWindow,
            },
            CloseCase {
                desc: "two pages: one survives, so Neovim is told to close the tabpage",
                live_pages: 2,
                want: TabClose::CloseTabpage,
            },
            CloseCase {
                desc: "three pages, the same",
                live_pages: 3,
                want: TabClose::CloseTabpage,
            },
            CloseCase {
                desc: "many pages, the same — there is no upper bound on this rule",
                live_pages: 16,
                want: TabClose::CloseTabpage,
            },
            CloseCase {
                desc: "and no cap hidden anywhere above it",
                live_pages: usize::MAX,
                want: TabClose::CloseTabpage,
            },
        ];

        for case in &cases {
            assert_eq!(
                close_outcome(case.live_pages),
                case.want,
                "close_outcome({}) must be {:?} — {}. `live_pages` COUNTS the page being closed, \
                 so 1 means this is the last tab. Answering CloseTabpage there sends \
                 close_tabpage for Neovim's last tabpage, which is a literal no-op: Neovim refuses \
                 to close the final tabpage, init.lua's pcall swallows the error, and liveness, \
                 the current tabpage and every grid are unchanged — the page stays, the window \
                 stays, the core is never shut down, and nothing anywhere reports a failure. \
                 Answering CloseWindow where a sibling page survives tears down the whole window \
                 and its Neovim over one tab's close button.",
                case.live_pages,
                case.want,
                case.desc
            );
        }

        // --- Non-vacuity: both answers are produced, and the boundary is real --
        //
        // ROW-DELETION BACKSTOPS, not assertions a source edit can make bite.
        // MEASURED: under a constant-function stub the table above fires at ROW 0 and
        // execution never reaches here. The first two do not call the seam at all —
        // they read `case.want` — and the third is reachable only from a table whose
        // expectations were rewritten to agree with the implementation.
        assert!(
            cases.iter().any(|case| case.want == TabClose::CloseWindow),
            "no close_outcome row expects CloseWindow, so the last-tab rule is unmeasured"
        );
        assert!(
            cases.iter().any(|case| case.want == TabClose::CloseTabpage),
            "no close_outcome row expects CloseTabpage, so the ordinary close is unmeasured"
        );
        assert_ne!(
            close_outcome(1),
            close_outcome(2),
            "close_outcome answered the same thing for the last page and for a page with a \
             sibling, so it is a constant function and the rows above measure nothing. A constant \
             CloseTabpage leaves the last tab uncloseable behind a live Neovim; a constant \
             CloseWindow destroys the whole window whenever any one tab is closed."
        );
    }

    // =========================================================================
    // new_tab_press
    // =========================================================================

    /// What a [`PressCase`] row expects [`new_tab_press`] to return.
    ///
    /// A refusal is matched on its CLASS, not on its reason string: the reasons are
    /// documentation, and their real contract (non-empty, distinct, clause-shaped)
    /// is asserted by [`new_tab_press_reasons_are_distinct_and_non_empty`] — the
    /// `nav_ui::tests::WantPress` precedent.
    #[derive(Clone, Copy)]
    enum WantPress {
        /// `NewTabPress::Open` — the BLOCKING create_tabpage RPC is issued.
        Open,
        /// `NewTabPress::Refuse(_)` — no RPC at all.
        Refuse,
    }

    /// One [`new_tab_press`] table row.
    struct PressCase {
        desc: &'static str,
        liveness: Option<Liveness>,
        want: WantPress,
    }

    /// Pins the New Tab decision for EVERY `Option<Liveness>` input.
    ///
    /// FALSIFYING EDIT: add `Some(Liveness::NotResponding) => Open`. That row then
    /// fails, and it is the one that matters: `create_tabpage` is a blocking RPC on
    /// the GTK main thread, so a press against a core already known not to answer
    /// freezes the window for another whole timeout budget — and the user, seeing
    /// nothing happen, presses again.
    #[test]
    fn new_tab_press_table() {
        let cases = [
            PressCase {
                desc: "a live core is the ONLY input that may issue the blocking create_tabpage",
                liveness: Some(Liveness::Alive),
                want: WantPress::Open,
            },
            PressCase {
                desc: "a wedged core refuses: the RPC would block behind a Neovim already known \
                       not to answer",
                liveness: Some(Liveness::NotResponding),
                want: WantPress::Refuse,
            },
            PressCase {
                desc: "a dead core refuses: there is no Neovim to create a tabpage in, and an RPC \
                       against a dead handle is not reliably fast",
                liveness: Some(Liveness::Dead),
                want: WantPress::Refuse,
            },
            PressCase {
                desc: "no core at all (the close handler took it out) refuses",
                liveness: None,
                want: WantPress::Refuse,
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_LIVENESS_INPUTS,
            "the new_tab_press table must carry exactly one row per input the gate is total over: \
             {NAMED_LIVENESS_INPUTS} (three liveness readings plus the absent core), not {}. A \
             missing row means an arm nothing checks.",
            cases.len()
        );
        assert_eq!(
            ALL_LIVENESS_INPUTS.len(),
            NAMED_LIVENESS_INPUTS,
            "ALL_LIVENESS_INPUTS carries {} entries but the gate is total over \
             {NAMED_LIVENESS_INPUTS}. If `wolf_core::Liveness` gained a variant, the `_`-free \
             match in this module is an E0004 already and this constant is the second thing to \
             update; if an entry was DELETED, the sweep below covers less than the domain it \
             claims to.",
            ALL_LIVENESS_INPUTS.len()
        );

        for case in &cases {
            let got = new_tab_press(case.liveness);
            match case.want {
                WantPress::Open => assert_eq!(
                    got,
                    NewTabPress::Open,
                    "case: {} — new_tab_press({:?}) returned {got:?}, but this input MUST be Open. \
                     A refusal here makes Ctrl+Shift+T, the New Tab menu item and the New Tab \
                     palette row all do nothing at all on a perfectly healthy editor, and print a \
                     reason for a hazard that is not present.",
                    case.desc,
                    case.liveness
                ),
                WantPress::Refuse => assert!(
                    matches!(got, NewTabPress::Refuse(_)),
                    "case: {} — new_tab_press({:?}) returned {got:?}, but this input MUST refuse. \
                     `WolfCore::create_tabpage` funnels into `block_on_rpc` — a BLOCKING call on \
                     the GTK main thread, and the tenth this crate issues — so an Open against a \
                     core known not to answer, already dead, or already taken out freezes the \
                     window for a full rpc_timeout budget and then hands back the 0 handle an \
                     error produces, which is not a tabpage at all.",
                    case.desc,
                    case.liveness
                ),
            }
        }

        // The rule stated over the WHOLE domain rather than row by row, so a failure
        // names the rule instead of quoting an index — the
        // `clipboard_ui::copy_decision_table` convention. This is also what catches a
        // table whose four rows became four copies of one.
        let accepting: Vec<Option<Liveness>> = ALL_LIVENESS_INPUTS
            .into_iter()
            .filter(|liveness| new_tab_press(*liveness) == NewTabPress::Open)
            .collect();
        let only_alive = [Some(Liveness::Alive)];
        assert_eq!(
            accepting.as_slice(),
            only_alive.as_slice(),
            "exactly ONE of the {NAMED_LIVENESS_INPUTS} liveness inputs may issue the blocking \
             create_tabpage — a core reading Alive. An EXTRA entry means the RPC is issued against \
             a core already known to be wedged, dead or absent, so every press after the first \
             stall re-freezes the main loop. An entry MISSING means New Tab can never work at \
             all, since Alive is what a healthy editor always reports."
        );
    }

    /// The three refusal reasons are non-empty, mutually DISTINCT, and read as a
    /// mid-sentence clause.
    ///
    /// Distinctness is the load-bearing half, for the same reason it is in
    /// `nav_ui::nav_press_reasons_are_distinct_and_non_empty`: the reason is the
    /// whole of what a reader sees when the chord appears to do nothing, and two
    /// arms sharing a string make "the editor is wedged" indistinguishable from
    /// "the editor has crashed" — two states with completely different routes back,
    /// one of which resolves by itself.
    #[test]
    fn new_tab_press_reasons_are_distinct_and_non_empty() {
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
            let decision = new_tab_press(liveness);
            let NewTabPress::Refuse(reason) = decision else {
                panic!(
                    "{source} produced {decision:?} rather than a refusal, so this test cannot \
                     check the reason it exists to check. Either that arm now issues the blocking \
                     create_tabpage (in which case `new_tab_press_table` is also red, and that is \
                     the finding) or the probe no longer selects the arm it names."
                )
            };

            assert!(
                !reason.trim().is_empty(),
                "the refusal reason for {source} is empty. It is printed as \
                 \"wolf-gnome: new tab not opened: {{reason}}\", so an empty one makes a \
                 deliberate refusal indistinguishable from a press that silently did nothing."
            );
            assert!(
                !is_sentence_cased(reason),
                "the refusal reason for {source} is sentence-cased ({reason:?}). It is \
                 interpolated MID-SENTENCE into the printed diagnostic, exactly as \
                 `command_action.rs`'s, `nav_ui.rs`'s and `clipboard_ui.rs`'s reasons are, so it \
                 must start lowercase (an ALL-CAPS proper noun such as \"GNOME\" is fine)."
            );
            assert!(
                seen.insert(reason),
                "the refusal reason for {source} ({reason:?}) duplicates another arm's reason. The \
                 reasons must be DISTINCT so the printed line names the real blocker: a wedged \
                 Neovim is recoverable and a dead one is not, and an absent core means the window \
                 is already tearing down. One shared string collapses them into a message that \
                 cannot be acted on."
            );
        }

        assert_eq!(
            seen.len(),
            probes.len(),
            "the three probed refusals produced {} distinct reason(s), not {}. See the per-probe \
             message above for which pair collapsed.",
            seen.len(),
            probes.len()
        );
    }

    // =========================================================================
    // restore_press — the launch-path gate on the queued tabs
    // =========================================================================

    /// Both readings of the FIRST tab's handle: not yet adopted, and adopted.
    ///
    /// Only two, and that is the whole domain: what the rule reads is whether tab 0
    /// has a handle at all, never WHICH handle, so a second real value would be a
    /// duplicate row rather than a new input.
    const ALL_FIRST_TABPAGE_INPUTS: [i64; 2] = [UNBOUND, TABPAGE_A];

    /// The queue depths probed: nothing queued, and a real queue.
    ///
    /// `2` rather than `1` so a rule that keyed on "exactly one" — or that consumed
    /// the queue one entry at a time — is visible.
    const ALL_PENDING_INPUTS: [usize; 2] = [0, 2];

    /// How many inputs [`restore_press`] is total over ONCE CREATION IS STILL
    /// PERMITTED: two handles for tab 0, times the four readings a core can produce,
    /// times two queue depths.
    ///
    /// The other half of the domain — the same sixteen inputs against a restore that
    /// has been ABANDONED — is swept by
    /// [`an_abandoned_restore_never_attempts_creation_again`], which is a rule of its
    /// own rather than a variation on these rows: it answers `Idle` for all sixteen.
    ///
    /// Written as LITERALS rather than as the product of the arrays' lengths, for the
    /// reason [`NAMED_WAKE_CASES`] gives: comparing a sweep's count to the product of
    /// the arrays it walked is a tautology.
    const NAMED_RESTORE_PRESS_CASES: usize = 2 * 4 * 2;

    /// One [`restore_press`] table row. The expectation is DATA, deliberately not
    /// re-derived from the same conjunction the function under test uses.
    ///
    /// There is no `may_create` field: every row here is a restore that may still be
    /// created, which is every launch that has not aborted. The abandoned half is
    /// [`an_abandoned_restore_never_attempts_creation_again`].
    struct RestorePressCase {
        desc: &'static str,
        first_tabpage: i64,
        liveness: Option<Liveness>,
        pending: usize,
        want: RestorePress,
    }

    /// A hashable key for one `Option<Liveness>`, matched TOTALLY with no `_` arm so
    /// a new [`Liveness`] variant is an `E0004` here rather than a silently
    /// uncovered input. `Option<Liveness>` is not `Hash`, and the duplicate-row check
    /// below needs one.
    fn liveness_key(liveness: Option<Liveness>) -> u8 {
        match liveness {
            Some(Liveness::Alive) => 0,
            Some(Liveness::NotResponding) => 1,
            Some(Liveness::Dead) => 2,
            None => 3,
        }
    }

    /// The sixteen rows of the restore gate, one per `(first_tabpage, liveness,
    /// pending)` input, against a restore that may still be created.
    ///
    /// A `const` item rather than a local so the sweeps below walk the same rows the
    /// assertions do — the [`WAKE_CASES`] convention.
    const RESTORE_PRESS_CASES: &[RestorePressCase] = &[
        // --- Nothing queued: the steady state, on every wake, forever -----------
        RestorePressCase {
            desc: "no queue and a healthy core: the steady state, and it must cost one \
                   Option::is_none() per wake",
            first_tabpage: TABPAGE_A,
            liveness: Some(Liveness::Alive),
            pending: 0,
            want: RestorePress::Idle,
        },
        RestorePressCase {
            desc: "no queue, tab 0 not yet adopted",
            first_tabpage: UNBOUND,
            liveness: Some(Liveness::Alive),
            pending: 0,
            want: RestorePress::Idle,
        },
        RestorePressCase {
            desc: "no queue, wedged core: nothing to wait FOR, so Idle rather than Wait",
            first_tabpage: TABPAGE_A,
            liveness: Some(Liveness::NotResponding),
            pending: 0,
            want: RestorePress::Idle,
        },
        RestorePressCase {
            desc: "no queue, wedged core, tab 0 unbound",
            first_tabpage: UNBOUND,
            liveness: Some(Liveness::NotResponding),
            pending: 0,
            want: RestorePress::Idle,
        },
        RestorePressCase {
            desc: "no queue, dead core",
            first_tabpage: TABPAGE_A,
            liveness: Some(Liveness::Dead),
            pending: 0,
            want: RestorePress::Idle,
        },
        RestorePressCase {
            desc: "no queue, dead core, tab 0 unbound",
            first_tabpage: UNBOUND,
            liveness: Some(Liveness::Dead),
            pending: 0,
            want: RestorePress::Idle,
        },
        RestorePressCase {
            desc: "no queue and no core at all (the close handler took it out)",
            first_tabpage: TABPAGE_A,
            liveness: None,
            pending: 0,
            want: RestorePress::Idle,
        },
        RestorePressCase {
            desc: "no queue, no core, tab 0 unbound — the post-restart shape, where the queue has \
                   already been cleared",
            first_tabpage: UNBOUND,
            liveness: None,
            pending: 0,
            want: RestorePress::Idle,
        },
        // --- A real queue, and tab 0 has NOT adopted yet ------------------------
        RestorePressCase {
            desc: "THE ROW THE FIRST CONJUNCT EXISTS FOR — a queue and a healthy core, but tab 0 \
                   has no handle yet, so creating now would leave tab 0 to adopt the LAST tabpage \
                   created",
            first_tabpage: UNBOUND,
            liveness: Some(Liveness::Alive),
            pending: 2,
            want: RestorePress::Wait,
        },
        RestorePressCase {
            desc: "a queue, an unbound tab 0 and a wedged core: both reasons at once",
            first_tabpage: UNBOUND,
            liveness: Some(Liveness::NotResponding),
            pending: 2,
            want: RestorePress::Wait,
        },
        RestorePressCase {
            desc: "a queue, an unbound tab 0 and a dead core",
            first_tabpage: UNBOUND,
            liveness: Some(Liveness::Dead),
            pending: 2,
            want: RestorePress::Wait,
        },
        RestorePressCase {
            desc: "a queue, an unbound tab 0 and no core at all",
            first_tabpage: UNBOUND,
            liveness: None,
            pending: 2,
            want: RestorePress::Wait,
        },
        // --- A real queue, tab 0 adopted ----------------------------------------
        RestorePressCase {
            desc: "THE ONE ROW THAT MAY CREATE: a queue, tab 0 bound, and a core reading Alive",
            first_tabpage: TABPAGE_A,
            liveness: Some(Liveness::Alive),
            pending: 2,
            want: RestorePress::Create,
        },
        RestorePressCase {
            desc: "THE ROW THE SECOND CONJUNCT EXISTS FOR — a wedged core must not be handed N \
                   blocking create_tabpage calls",
            first_tabpage: TABPAGE_A,
            liveness: Some(Liveness::NotResponding),
            pending: 2,
            want: RestorePress::Wait,
        },
        RestorePressCase {
            desc: "a dead core: there is no Neovim to create a tabpage in",
            first_tabpage: TABPAGE_A,
            liveness: Some(Liveness::Dead),
            pending: 2,
            want: RestorePress::Wait,
        },
        RestorePressCase {
            desc: "no core at all, with tabs still queued — the window is tearing down and the \
                   queue must survive rather than be discarded",
            first_tabpage: TABPAGE_A,
            liveness: None,
            pending: 2,
            want: RestorePress::Wait,
        },
    ];

    /// Pins the restore decision for EVERY `(first_tabpage, liveness, pending)`
    /// input of a restore that MAY STILL BE CREATED, and asserts the table is neither
    /// short nor made of duplicates. The abandoned half of the domain is
    /// [`an_abandoned_restore_never_attempts_creation_again`].
    ///
    /// ## What it discriminates, and the edits that redden it
    ///
    /// FALSIFYING EDIT 1 — drop the `first_tabpage != 0` conjunct. The four unbound
    /// rows with a queue report `Create` (or, for the non-Alive ones, still `Wait`),
    /// and the Alive one is the damaging case: the queued tabpages are created before
    /// tab 0 has adopted, so tab 0's next wake adopts the LAST created tabpage and
    /// two pages own one tabpage — `TabParts::tabpage`'s recorded hazard, from which
    /// every later switch and close aims at the wrong tab.
    ///
    /// FALSIFYING EDIT 2 — drop the liveness conjunct. The three non-Alive rows with
    /// a bound tab 0 report `Create`, putting `T - 1` blocking `create_tabpage` RPCs
    /// and up to `T` blocking `rebuild_layout` RPCs on the GTK main thread behind a
    /// Neovim already known not to answer — the window freezes for a multiple of the
    /// timeout budget at the one moment the user is watching it start.
    ///
    /// FALSIFYING EDIT 3 — answer `Idle` where `Wait` belongs. The seven `Wait` rows
    /// fail, and what they protect is the seam's MEANING rather than a behaviour of
    /// today's caller: `materialize_restore` matches `Idle | Wait` in one arm, so the
    /// queue is retained either way, and the SAVE survives either way too —
    /// `window::WindowHandle::cached_subtree` contributes a seed per queued layout
    /// ([`crate::state_seam::pending_tab_states`]) for as long as a queue is held.
    /// What `Idle` asserts is that there is nothing to materialize, which for a queue
    /// that may still be created is false, and any caller acting on that assertion —
    /// dropping the queue, or not asking again on a later wake — loses those tabs for
    /// the whole session, because a tab with no page has no cache to fall back on.
    #[test]
    fn restore_press_table() {
        assert_eq!(
            RESTORE_PRESS_CASES.len(),
            NAMED_RESTORE_PRESS_CASES,
            "the restore_press table must carry exactly one row per input the gate is total over \
             for a restore that may still be created: two handles for tab 0, times four liveness \
             readings, times two queue depths, is {NAMED_RESTORE_PRESS_CASES} rows, not {}. A \
             missing row means an input nothing checks, and the row most likely to go missing is \
             the one that must WAIT while tab 0 is still unbound — the only moment at which \
             creating would put two pages on one tabpage.",
            RESTORE_PRESS_CASES.len()
        );

        // Every row of the domain really is present, so the count above cannot be
        // satisfied by sixteen copies of one row.
        let mut seen: HashSet<(i64, u8, usize)> = HashSet::new();
        for case in RESTORE_PRESS_CASES {
            assert!(
                seen.insert((
                    case.first_tabpage,
                    liveness_key(case.liveness),
                    case.pending
                )),
                "two restore_press rows carry the same (first_tabpage = {}, liveness = {:?}, \
                 pending = {}) input, so the table is shorter than its length suggests and some \
                 other input is unmeasured — {}",
                case.first_tabpage,
                case.liveness,
                case.pending,
                case.desc
            );
        }
        for first_tabpage in ALL_FIRST_TABPAGE_INPUTS {
            for liveness in ALL_LIVENESS_INPUTS {
                for pending in ALL_PENDING_INPUTS {
                    assert!(
                        seen.contains(&(first_tabpage, liveness_key(liveness), pending)),
                        "no restore_press row covers (first_tabpage = {first_tabpage}, liveness = \
                         {liveness:?}, pending = {pending}), so that input's answer is whatever \
                         the seam happens to do. Every one of these is reachable at a real launch: \
                         tab 0 carries 0 until the first wake at which Neovim has reported a \
                         tabpage, the queue is non-empty from `window::build` until \
                         materialization, and a core can be wedged, dead or already taken out \
                         while a restore is still pending."
                    );
                }
            }
        }

        // Every row is a restore whose creation may still be attempted, which is every
        // launch that has not aborted; the abandoned half is
        // `an_abandoned_restore_never_attempts_creation_again`.
        for case in RESTORE_PRESS_CASES {
            let got = restore_press(case.first_tabpage, case.liveness, case.pending, true);
            assert_eq!(
                got, case.want,
                "restore_press(first_tabpage = {}, liveness = {:?}, pending = {}, may_create = \
                 true) must be {:?}, got {got:?} — {}. Each wrong answer costs something \
                 different. A CREATE where a Wait is due either binds the queued tabpages before \
                 tab 0 has adopted one — so tab 0 later adopts the LAST tabpage created and two \
                 pages own one, which is `TabParts::tabpage`'s recorded hazard — or issues N \
                 blocking RPCs against a Neovim already known not to answer, freezing the window \
                 at launch. An IDLE where a Wait is due declares a queue that is merely not ready \
                 yet to be one with nothing left to do: the SAVE survives that, because the \
                 retained queue is what speaks for those tabs (`state_seam::pending_tab_states`), \
                 but nothing else does — a tab with no page has no cache — so any caller acting on \
                 the declaration, by dropping the queue or by not asking again, loses them for the \
                 whole session. A WAIT where Create is due leaves the tabs queued forever, which \
                 looks identical to a restore that never ran. An IDLE or a WAIT where the queue is \
                 empty is the steady state and must cost nothing.",
                case.first_tabpage, case.liveness, case.pending, case.want, case.desc
            );
        }

        // --- Exactly one input may CREATE -------------------------------------
        //
        // The rule stated over the whole domain rather than row by row, so a failure
        // names the rule instead of quoting an index — the `new_tab_press_table`
        // convention. This is also what catches a table whose sixteen rows became
        // sixteen copies of one.
        let mut creating: Vec<(i64, Option<Liveness>, usize)> = Vec::new();
        for first_tabpage in ALL_FIRST_TABPAGE_INPUTS {
            for liveness in ALL_LIVENESS_INPUTS {
                for pending in ALL_PENDING_INPUTS {
                    let press = restore_press(first_tabpage, liveness, pending, true);
                    if press == RestorePress::Create {
                        creating.push((first_tabpage, liveness, pending));
                    }
                }
            }
        }
        let only_ready = [(TABPAGE_A, Some(Liveness::Alive), 2_usize)];
        assert_eq!(
            creating.as_slice(),
            only_ready.as_slice(),
            "exactly ONE of the {NAMED_RESTORE_PRESS_CASES} inputs may materialize the queued \
             tabs: a real queue, a tab 0 that has already adopted its handle, and a core reading \
             Alive. An EXTRA entry is one of the two hazards above — a second page bound to a \
             tabpage tab 0 will later claim, or a burst of blocking RPCs against a Neovim that is \
             not answering. An entry MISSING means the queue is never materialized at all, so \
             every relaunch shows one tab while `state.json` still promises the rest — the queue \
             is never drained, so it keeps seeding those tabs, and the session silently shrinks on \
             every launch while the file never explains why."
        );

        // --- Non-vacuity: all three answers are produced -----------------------
        //
        // BACKSTOPS rather than the red a source edit produces: the row loop above
        // dominates all three for every single-arm edit. What they catch is the table
        // and the seam moving TOGETHER — a rewrite that made `restore_press` a
        // constant function and rewrote every `want` to agree would satisfy the length
        // check, the coverage sweep and every row.
        let answers: Vec<RestorePress> = RESTORE_PRESS_CASES
            .iter()
            .map(|case| restore_press(case.first_tabpage, case.liveness, case.pending, true))
            .collect();
        for (want, consequence) in [
            (
                RestorePress::Create,
                "the queued tabs would never be materialized, so every relaunch opens the first \
                 tab alone while the save still promises the rest — the session shrinks to one tab \
                 on every launch and nothing in the file says why",
            ),
            (
                RestorePress::Wait,
                "a launch that is not ready yet would either create against an unbound tab 0 or a \
                 non-answering core, or be told there is nothing left to do — and the queue is the \
                 only record of those tabs, so nothing else would ask again",
            ),
            (
                RestorePress::Idle,
                "every steady-state wake would keep re-examining a queue that is empty, and the \
                 gate would no longer distinguish \"nothing to do\" from \"not yet\"",
            ),
        ] {
            assert!(
                answers.contains(&want),
                "NOT ONE of the {NAMED_RESTORE_PRESS_CASES} inputs answered {want:?}, so that arm \
                 is not stated at all: {consequence}."
            );
        }
    }

    /// A restore that has been ABANDONED never attempts creation again, whatever else
    /// is true of the window: `Idle` for every one of the
    /// [`NAMED_RESTORE_PRESS_CASES`] inputs [`restore_press_table`] walks.
    ///
    /// ## The split this measures
    ///
    /// The restore queue does two jobs, and an abort separates them. "Layouts still
    /// owed a page" goes on seeding the window's contribution, so an abandoned restore
    /// keeps speaking for its tabs in `state.json` forever — that half is
    /// `state_seam::tests::an_abandoned_restore_keeps_seeding_its_un_created_tabs`.
    /// "Creation may still be attempted" is this input, and it is false from the abort
    /// onwards, because `WolfCore::create_tabpage` has already answered `0` for this
    /// window once.
    ///
    /// FALSIFYING EDIT — ignore `may_create`. Eight of the sixteen inputs stop
    /// answering `Idle`: seven answer `Wait`, and the `(TABPAGE_A, Alive, 2)` one
    /// answers `Create`, which is the damaging one. The retained remainder is then
    /// retried on the very next wake and on every wake after it — one blocking
    /// `create_tabpage`, plus up to one blocking `rebuild_layout` per tab, on the GTK
    /// main thread PER FRAME, against a Neovim that has just failed to answer one.
    /// That is the freeze the liveness conjunct exists to prevent, arriving through a
    /// different door.
    ///
    /// `Idle` rather than `Wait` because `Wait` promises that a later wake will finish
    /// the restore, and after an abort none ever will. Those two are interchangeable
    /// at the only call site, which matches `Idle | Wait` in one arm; `Create` is not,
    /// and that is what this test is really holding shut.
    #[test]
    fn an_abandoned_restore_never_attempts_creation_again() {
        let mut swept = 0_usize;
        for first_tabpage in ALL_FIRST_TABPAGE_INPUTS {
            for liveness in ALL_LIVENESS_INPUTS {
                for pending in ALL_PENDING_INPUTS {
                    swept += 1;
                    let got = restore_press(first_tabpage, liveness, pending, false);
                    assert_eq!(
                        got,
                        RestorePress::Idle,
                        "restore_press(first_tabpage = {first_tabpage}, liveness = \
                         {liveness:?}, pending = {pending}, may_create = false) must be \
                         Idle, got {got:?}. A CREATE here re-attempts a restore that has \
                         already failed: `create_tabpage` answered 0 once for this \
                         window, and every later wake would put another blocking \
                         create_tabpage — and up to one blocking rebuild_layout per tab \
                         — on the GTK main thread behind a Neovim that is not \
                         answering. A WAIT is the milder wrong answer, and today's only \
                         call site cannot tell it from Idle, but it promises a later \
                         wake will finish the restore when none ever will. Neither \
                         answer may take the queue AWAY: the un-created layouts stay as \
                         the seed source, so an abandoned restore is a session that \
                         lost tabs rather than a save that did."
                    );
                }
            }
        }
        assert_eq!(
            swept, NAMED_RESTORE_PRESS_CASES,
            "the abandoned sweep must reach all {NAMED_RESTORE_PRESS_CASES} inputs the gate is \
             total over, not {swept}. A domain array that lost an entry shrinks this sweep \
             silently, and the entry most likely to go missing is the one with a real queue and a \
             healthy core — the only input that would otherwise answer Create."
        );

        // Non-vacuity, and the premise the rule above rests on: `may_create` is the
        // ONLY thing that changed. Without this, a gate that refused everything would
        // satisfy every assertion above — and no restore would ever materialize, so
        // every relaunch would open one tab while the save promised the rest.
        assert_eq!(
            restore_press(TABPAGE_A, Some(Liveness::Alive), 2, true),
            RestorePress::Create,
            "premise: the very same window — a real queue, a tab 0 that has adopted, a core \
             reading Alive — must answer Create while creation is still permitted. If it does \
             not, the abandoned rule above holds because nothing is ever created rather than \
             because the abort stopped this restore, and every launch would silently open one \
             tab."
        );
    }

    // =========================================================================
    // new_tab_cwd
    // =========================================================================

    /// One [`new_tab_cwd`] table row.
    struct CwdCase {
        desc: &'static str,
        empty: bool,
        launch_cwd: &'static str,
        live_cwd: Option<&'static str>,
        want: Option<&'static str>,
    }

    /// The five rows of the inherited-cwd rule.
    ///
    /// A `const` item so [`the_inherited_cwd_agrees_with_the_persisted_one`] can
    /// walk the same rows without copying them.
    const CWD_CASES: &[CwdCase] = &[
        CwdCase {
            desc: "a project-less window with a live cwd still inherits NOTHING",
            empty: true,
            launch_cwd: HOME,
            live_cwd: Some(HOME),
            want: None,
        },
        CwdCase {
            desc: "a project-less window with no live cwd inherits nothing",
            empty: true,
            launch_cwd: HOME,
            live_cwd: None,
            want: None,
        },
        CwdCase {
            desc: "a project-less window whose Neovim wandered into a real directory STILL \
                   inherits nothing",
            empty: true,
            launch_cwd: HOME,
            live_cwd: Some(PROJECT),
            want: None,
        },
        CwdCase {
            desc: "a window with a project inherits Neovim's LIVE cwd once it has reported one",
            empty: false,
            launch_cwd: PROJECT,
            live_cwd: Some(WANDERED),
            want: Some(WANDERED),
        },
        CwdCase {
            desc: "a window whose Neovim has not reported yet inherits the LAUNCH cwd, so a New \
                   Tab pressed immediately does not lose the project",
            empty: false,
            launch_cwd: PROJECT,
            live_cwd: None,
            want: Some(PROJECT),
        },
    ];

    /// The directory a new tab is `:tcd`'d into, for every shape the window can be
    /// in.
    ///
    /// FALSIFYING EDIT: return the launch cwd when `empty`. The three empty rows
    /// fail, and the cost is named in the message: a project-less window's Neovim
    /// was handed `$HOME` (or `/`) purely because a process needs a working
    /// directory, so inheriting it declares the user's home directory a project.
    #[test]
    fn new_tab_cwd_table() {
        for case in CWD_CASES {
            let got = new_tab_cwd(
                case.empty,
                Path::new(case.launch_cwd),
                case.live_cwd.map(Path::new),
            );
            assert_eq!(
                got.as_deref(),
                case.want.map(Path::new),
                "new_tab_cwd(empty = {}, launch_cwd = {:?}, live_cwd = {:?}) must be {:?} — {}. \
                 The answer is handed straight to WolfCore::create_tabpage, which turns Some(dir) \
                 into a `:tcd` for the new tabpage: a directory where there should be None roots \
                 the new tab at the fallback working directory a project-less launch handed \
                 Neovim, which is $HOME, and points that tab's sidebar, picker and project search \
                 at the whole home tree. None where there should be a directory leaves the new tab \
                 at the global cwd instead of the window's folder, so the two tabs of one window \
                 disagree about where the project is.",
                case.empty,
                case.launch_cwd,
                case.live_cwd,
                case.want,
                case.desc
            );
        }

        // Non-vacuity: a `new_tab_cwd` of `|_, _, _| None` satisfies all three empty
        // rows while every new tab in every project silently opens at the global cwd.
        assert!(
            CWD_CASES.iter().any(|case| case.want.is_some()),
            "no new_tab_cwd row expects a directory, so a seam that answered None for everything \
             would satisfy every row above while no new tab ever inherited its window's folder."
        );
        // And the complement: a seam that answered `Some(launch_cwd)` for everything
        // satisfies the two project rows only if one of them prefers the LIVE cwd.
        assert!(
            CWD_CASES
                .iter()
                .any(|case| !case.empty && case.live_cwd.is_some() && case.want == case.live_cwd),
            "no new_tab_cwd row asserts that a reported LIVE cwd wins over the launch cwd, so a \
             seam that ignored live_cwd entirely would pass. A window whose user has typed `:cd` \
             would then open every new tab at the directory the window was launched at, which is \
             no longer the project."
        );
    }

    /// The rule stated on its own, over every live cwd, so a failure NAMES it.
    ///
    /// Mirrors `state_seam::the_empty_state_persists_no_cwd`, and for the same
    /// reason: the fallback working directory a project-less launch hands Neovim is
    /// a process requirement, not a project.
    #[test]
    fn a_project_less_window_inherits_no_cwd() {
        for live_cwd in [None, Some(HOME), Some(PROJECT), Some(WANDERED), Some("/")] {
            assert_eq!(
                new_tab_cwd(true, Path::new(HOME), live_cwd.map(Path::new)),
                None,
                "a project-less window must inherit NOTHING for every live cwd (here {live_cwd:?}) \
                 and every launch cwd. Handing WolfCore::create_tabpage a directory here makes it \
                 `:tcd` the new tabpage into the fallback working directory the launch gave \
                 Neovim — $HOME — which is not a project the user opened, and which then roots \
                 that tab's file search at the whole home tree."
            );
        }
    }

    /// The inherited cwd and the PERSISTED cwd are the same directory.
    ///
    /// The two rules live in different modules because they answer different
    /// questions and have different types, but they must not diverge: a window
    /// persists [`crate::state_seam::snapshot_cwd`]'s answer and reopens there,
    /// while every tab it opens is rooted at [`new_tab_cwd`]'s. If they disagree, a
    /// relaunch puts the window's tabs somewhere the window itself is not.
    ///
    /// `None` here is `""` there — the same "no project" sentinel in the two shapes
    /// the two callers need.
    #[test]
    fn the_inherited_cwd_agrees_with_the_persisted_one() {
        for case in CWD_CASES {
            let launch = Path::new(case.launch_cwd);
            let live = case.live_cwd.map(Path::new);

            let inherited = new_tab_cwd(case.empty, launch, live);
            let persisted = snapshot_cwd(case.empty, launch, live);
            let inherited_as_persisted = inherited
                .as_ref()
                .map_or_else(String::new, |dir| dir.to_string_lossy().into_owned());

            assert_eq!(
                inherited_as_persisted, persisted,
                "new_tab_cwd(empty = {}, launch_cwd = {:?}, live_cwd = {:?}) inherits \
                 {inherited:?} while state_seam::snapshot_cwd persists {persisted:?} for the same \
                 window — {}. \
                 The two must name the same directory, with None here matching \"\" there: the \
                 window is reopened at the persisted one and its tabs are rooted at the inherited \
                 one, so a divergence puts a restored window's tabs in a directory the window is \
                 not in.",
                case.empty, case.launch_cwd, case.live_cwd, case.desc
            );
        }
    }

    // =========================================================================
    // tab_title
    // =========================================================================

    /// A file several directories deep. None of its directory components is the
    /// basename, so a title that answered with a parent directory — or with the
    /// whole path — is visible rather than accidentally equal.
    const NESTED_FILE: &str = "/home/jane/ledger/vendor/entry.md";

    /// [`NESTED_FILE`]'s basename: what the tab must draw.
    const NESTED_BASENAME: &str = "entry.md";

    /// One [`tab_title`] table row.
    struct TitleCase {
        desc: &'static str,
        current_file: Option<&'static str>,
        want: &'static str,
    }

    /// The string one tab draws, for every shape the pane's current file can take.
    ///
    /// FALSIFYING EDIT: return the whole path. Rows 1 and 3 fail. The cost is that
    /// a tab bar allots each tab a small fraction of the window's width, so an
    /// absolute path renders as an ellipsised middle with neither the project nor
    /// the file legible — every tab looks the same.
    ///
    /// The premise assertions come first: the last three rows rest on
    /// `std::path::Path::file_name`'s behaviour for a trailing separator, for the
    /// filesystem root and for the empty path, and a premise about the standard
    /// library is exactly the kind that is assumed rather than measured.
    #[test]
    fn tab_title_table() {
        assert_eq!(
            Path::new("/a/b/").file_name(),
            Some(OsStr::new("b")),
            "precondition: a trailing separator must contribute no path component, or the \
             trailing-separator row below is not testing the branch it names"
        );
        assert!(
            Path::new("/").file_name().is_none(),
            "precondition: the filesystem root must have no final component, or the fallback row \
             for it below never reaches the branch it exists to pin"
        );
        assert!(
            Path::new("").file_name().is_none(),
            "precondition: the empty path must have no final component, or the fallback row for it \
             below never reaches the branch it exists to pin"
        );

        let cases = [
            TitleCase {
                desc: "THE ROW THIS TABLE EXISTS FOR — a real file is drawn by its NAME, never by \
                       its path",
                current_file: Some(NESTED_FILE),
                want: NESTED_BASENAME,
            },
            TitleCase {
                desc: "a relative path is drawn by its final component too",
                current_file: Some("vendor/entry.md"),
                want: NESTED_BASENAME,
            },
            TitleCase {
                desc: "a file at the filesystem root is still just its name",
                current_file: Some("/entry.md"),
                want: NESTED_BASENAME,
            },
            TitleCase {
                desc: "a trailing separator contributes no component, so the directory name shows",
                current_file: Some("/home/jane/ledger/"),
                want: "ledger",
            },
            TitleCase {
                desc: "the filesystem root has no name to show, so the fallback does",
                current_file: Some("/"),
                want: TAB_TITLE_FALLBACK,
            },
            TitleCase {
                desc: "the empty path has no name either",
                current_file: Some(""),
                want: TAB_TITLE_FALLBACK,
            },
            TitleCase {
                desc: "no current file at all — an unnamed buffer, or a pane not yet bound",
                current_file: None,
                want: TAB_TITLE_FALLBACK,
            },
        ];

        for case in &cases {
            let got = tab_title(case.current_file.map(Path::new));
            assert_eq!(
                got, case.want,
                "tab_title({:?}) must be {:?}, got {got:?} — {}. The tab bar gives each tab a \
                 small fraction of the window's width: a path there is ellipsised in the middle, \
                 so neither the project nor the file is legible and every tab of one project looks \
                 identical. An empty title makes the tab a bare close button with nothing to aim \
                 at.",
                case.current_file, case.want, case.desc
            );
        }

        // --- Non-vacuity, in both directions ----------------------------------
        assert!(
            cases.iter().any(|case| case.want != TAB_TITLE_FALLBACK),
            "no tab_title row expects a real file name, so a seam that answered the fallback for \
             everything would satisfy every row above while no tab ever showed what it holds."
        );
        assert!(
            cases.iter().any(|case| case.want == TAB_TITLE_FALLBACK),
            "no tab_title row expects the fallback, so the no-name branch is unmeasured and a \
             panicking or empty answer there would go unnoticed."
        );
    }

    /// A tab title is a NAME, and specifically not any of the look-alikes the same
    /// path could have produced.
    ///
    /// The discriminating negatives are the point: asserting only that the title is
    /// `"entry.md"` would also pass for a seam that happened to return a substring
    /// of the path, and asserting only that it is short would pass for a seam that
    /// returned a parent directory. Both wrong answers are named here and both are
    /// excluded.
    #[test]
    fn a_tab_title_is_a_file_name_and_never_a_path() {
        let got = tab_title(Some(Path::new(NESTED_FILE)));

        assert_eq!(
            got, NESTED_BASENAME,
            "tab_title({NESTED_FILE:?}) must be {NESTED_BASENAME:?}, got {got:?}"
        );
        assert_ne!(
            got, NESTED_FILE,
            "tab_title returned the whole path. The tab bar would render an ellipsised absolute \
             path in a few dozen pixels, which names neither the project nor the file."
        );
        for look_alike in ["vendor", "ledger", "jane", "home"] {
            assert_ne!(
                got, look_alike,
                "tab_title returned {look_alike:?} — a DIRECTORY component of {NESTED_FILE:?} \
                 rather than its final one. A seam that took the wrong end of the path, or that \
                 took the parent, would label every tab of one project identically."
            );
        }
        assert!(
            !got.contains(std::path::MAIN_SEPARATOR),
            "tab_title({NESTED_FILE:?}) returned {got:?}, which still contains a path separator. \
             The rule is a file NAME, never a path — see the module docs."
        );
        assert!(
            !got.trim().is_empty(),
            "tab_title({NESTED_FILE:?}) returned {got:?}, which draws as an empty tab: a bare \
             close button with nothing to aim at and nothing to tell it from its neighbour."
        );

        // And the fallback obeys the same two rules, since it is drawn in the same
        // place. Its literal is deliberately NOT pinned here — nothing outside this
        // crate reads it — but a fallback that were empty, or that were a path,
        // would break the bar exactly as a bad title would.
        assert!(
            !TAB_TITLE_FALLBACK.trim().is_empty(),
            "TAB_TITLE_FALLBACK is empty or all whitespace, so a tab holding an unnamed buffer \
             draws as a bare close button"
        );
        assert!(
            !TAB_TITLE_FALLBACK.contains(std::path::MAIN_SEPARATOR),
            "TAB_TITLE_FALLBACK ({TAB_TITLE_FALLBACK:?}) contains a path separator, so the \
             no-name case renders as a path in a bar that has no room for one"
        );
    }

    /// A file name that is not valid UTF-8 is rendered LOSSILY rather than dropped.
    ///
    /// Neovim will hand back whatever bytes the filesystem holds, and a tab with no
    /// title is unclickable in practice — the user cannot tell it from its
    /// neighbour. The replacement character is the honest rendering; the fallback
    /// would be a silent lie about the buffer being unnamed.
    ///
    /// The premise — that `to_string_lossy` substitutes `U+FFFD` for the invalid
    /// byte — is measured against the real standard library first, because the
    /// assertion below is meaningless if it does not.
    #[cfg(unix)]
    #[test]
    fn a_non_utf8_file_name_is_rendered_lossily_rather_than_dropped() {
        use std::os::unix::ffi::OsStrExt;

        // `0xE9` is `é` in Latin-1 and an invalid UTF-8 byte on its own.
        let raw = OsStr::from_bytes(b"/home/jane/ledger/caf\xE9.md");
        let path = Path::new(raw);

        let expected = "caf\u{FFFD}.md";
        let measured = path.file_name().map(|name| name.to_string_lossy());
        assert_eq!(
            measured.as_deref(),
            Some(expected),
            "precondition: `Path::file_name` followed by `to_string_lossy` must substitute U+FFFD \
             for the invalid byte, or the assertion below is measuring something other than the \
             rule it names"
        );

        let got = tab_title(Some(path));
        assert_eq!(
            got, expected,
            "tab_title must render an invalid-UTF-8 file name lossily, got {got:?}. Falling back \
             to {TAB_TITLE_FALLBACK:?} here would tell the user the buffer is unnamed when it is \
             not, and dropping the tab's title entirely leaves a tab that cannot be told from its \
             neighbour."
        );
        assert_ne!(
            got, TAB_TITLE_FALLBACK,
            "tab_title fell back to {TAB_TITLE_FALLBACK:?} for a file that HAS a name, merely not \
             a UTF-8 one. The bytes came from the filesystem and Neovim will keep handing them \
             back; the tab must show what it can of them."
        );
    }
}
