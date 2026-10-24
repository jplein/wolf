//! Pure, headless tests for the per-pane navigation-history data model
//! (`wolf_core::nav_history`): `NavTarget`, `PaneHistory`, `NavHistory`, the free
//! `derive_target` / `disambiguate` functions, and the `menu_rows` label pipeline.
//! No mocks, no Neovim, no filesystem — the whole module is path MATH only
//! (`disambiguate` never touches the disk), exactly like `rank_palette` /
//! `pane_status` which it sits beside.
//!
//! These live in a dedicated integration file (with its own `nav_history` BUILD
//! target) rather than an in-module `#[cfg(test)] mod tests` inside the NEW
//! `src/nav_history.rs`, to avoid colliding with the coder who creates that file.
//! This mirrors the crate's existing precedent (`tests/palette_rank.rs`,
//! `tests/buffer_rank.rs`, `tests/pane_status_unit.rs`).
//!
//! ## Dependency on coder work (plan §B9 / §B11)
//!
//! None of this exists yet — the coder adds `pub mod nav_history;` (+ the
//! re-exports this file imports) in `crates/wolf-core/src/lib.rs`, adds
//! `WinBuffer` to the `pub use render::{…}` list, and adds the
//! `//crates/wolf-core:nav_history` `rust_test` target to `BUILD.bazel`. This file
//! does NOT touch non-test source. It assumes the plan's exact API surface:
//!
//! - `pub enum NavTarget { Edit { path: PathBuf }, Preview { source: PathBuf },
//!   Terminal { bufnr: i64 }, Untitled { bufnr: i64 } }`
//!   (derives `Clone, Debug, PartialEq, Eq, Hash`)
//! - `pub struct NavMove { pub from: usize, pub to: usize, pub target: NavTarget }`
//!   (asserted FIELD-WISE here, so it needs no derives)
//! - `pub struct MenuRow { pub display: String, pub index: usize, pub is_current: bool }`
//!   (read FIELD-WISE here, so it needs no derives)
//! - `PaneHistory` accessors return BORROWS: `entries(&self) -> &[NavTarget]`,
//!   `cursor(&self) -> usize`, `current(&self) -> Option<&NavTarget>`,
//!   `expected(&self) -> Option<&NavTarget>`
//! - `NavHistory::pane(&self, win: i64) -> Option<&PaneHistory>`
//! - `pub fn derive_target(buf: Option<&WinBuffer>, preview_source: Option<&Path>)
//!   -> Option<NavTarget>`
//! - `pub fn disambiguate(paths: &[PathBuf], home: Option<&Path>)
//!   -> HashMap<PathBuf, String>`
//!
//! `disambiguate` is imported through the MODULE path (`wolf_core::nav_history::`)
//! because plan §B11's crate-root re-export list does not name it; everything else
//! comes through the crate-root re-exports the plan does specify.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use wolf_core::nav_history::disambiguate;
use wolf_core::{
    derive_target, MenuRow, NavHistory, NavTarget, PaneHistory, WinBuffer, MAX_ENTRIES,
    MAX_MENU_ROWS,
};

// --- Small constructors to cut repetition ---------------------------------

/// An `Edit` target for `p`.
fn edit(p: &str) -> NavTarget {
    NavTarget::Edit {
        path: PathBuf::from(p),
    }
}

/// A `Preview` target sourced from `p`.
fn preview(p: &str) -> NavTarget {
    NavTarget::Preview {
        source: PathBuf::from(p),
    }
}

/// A `Terminal` target for buffer `bufnr`.
fn terminal(bufnr: i64) -> NavTarget {
    NavTarget::Terminal { bufnr }
}

/// An `Untitled` target for buffer `bufnr`.
fn untitled(bufnr: i64) -> NavTarget {
    NavTarget::Untitled { bufnr }
}

/// The pane's stack as an owned `Vec`, so it can be compared against a literal
/// regardless of whether `entries()` yields `&[NavTarget]` or `&Vec<NavTarget>`.
fn stack(h: &PaneHistory) -> Vec<NavTarget> {
    h.entries().to_vec()
}

/// The `display` strings of `rows`, in row order.
fn displays(rows: &[MenuRow]) -> Vec<String> {
    rows.iter().map(|r| r.display.clone()).collect()
}

/// The `index` values of `rows`, in row order.
fn indices(rows: &[MenuRow]) -> Vec<usize> {
    rows.iter().map(|r| r.index).collect()
}

/// The single checkmarked row, or `None` when the cursor's target fell outside
/// the row cap.
fn current_row(rows: &[MenuRow]) -> Option<&MenuRow> {
    rows.iter().find(|r| r.is_current)
}

/// How many rows carry the checkmark (must never exceed 1).
fn current_count(rows: &[MenuRow]) -> usize {
    rows.iter().filter(|r| r.is_current).count()
}

/// A named `WinBuffer` (an ordinary file-backed editor buffer).
fn named_buffer(bufnr: i64, path: &str) -> WinBuffer {
    WinBuffer {
        bufnr,
        path: Some(PathBuf::from(path)),
        is_terminal: false,
        is_drawer: false,
    }
}

// === 1. Feedback-loop suppression ===

/// **The load-bearing case (Part A, "Feedback-loop suppression").**
///
/// `back` / `forward` / `jump` record an `expected` target for the pane. The very
/// next derived change that MATCHES it is ABSORBED (our own move landing, echoed
/// back through `wolf_buffers`); a NON-matching one is a genuine navigation and
/// PUSHES. Without this the pane's own back-move would immediately push a new
/// entry and Back could never actually go back.
///
/// (Implementation note for the coder: after `back()` the cursor ALREADY points at
/// the target, so branch 2 of the `observe` rule — `current() == Some(target)` —
/// may absorb the echo before branch 3 — `expected == Some(target)` — is consulted.
/// Both branches are "absorb"; this test asserts the OBSERVABLE contract, namely
/// that nothing is pushed and `expected` is cleared either way.)
#[test]
fn back_sets_expected_and_the_echoed_emit_is_absorbed_not_pushed() {
    let (a, b, c) = (edit("/p/a.md"), edit("/p/b.md"), edit("/p/c.md"));

    let mut h = PaneHistory::default();
    h.push(a.clone());
    h.push(b.clone());
    assert_eq!(stack(&h), vec![a.clone(), b.clone()], "seeded stack");
    assert_eq!(h.cursor(), 1, "the cursor sits on the newest entry");

    // --- back() reports the move AND arms `expected` -----------------------
    let mv = h.back().expect("back() must move from index 1 to index 0");
    assert_eq!(mv.from, 1, "NavMove.from is the pre-move cursor");
    assert_eq!(mv.to, 0, "NavMove.to is the post-move cursor");
    assert_eq!(mv.target, a, "NavMove.target is the entry navigated TO");
    assert_eq!(h.cursor(), 0, "back() moves the cursor itself");
    assert_eq!(
        h.expected(),
        Some(&a),
        "back() must arm `expected` with the target BEFORE the RPC is issued, so a \
         fold racing the RPC's autocmds already sees it"
    );

    // --- the resulting emit for A is ABSORBED (no push, no truncate) -------
    h.observe(a.clone());
    assert_eq!(
        stack(&h),
        vec![a.clone(), b.clone()],
        "the self-caused emit must NOT push (the stack is untouched)"
    );
    assert_eq!(h.cursor(), 0, "the cursor stays where back() put it");
    assert_eq!(
        h.expected(),
        None,
        "`expected` is consulted at most once and cleared on the very next observe"
    );

    // --- a GENUINE navigation still pushes (and truncates ahead) -----------
    h.observe(c.clone());
    assert_eq!(
        stack(&h),
        vec![a.clone(), c.clone()],
        "a non-matching change is a genuine navigation: it truncates ahead of the \
         cursor and appends"
    );
    assert_eq!(h.cursor(), 1, "the cursor follows the newly pushed entry");
    assert_eq!(h.expected(), None, "a push clears `expected`");
}

/// A stale `expected` can NEVER wedge history: `expected` is not a gate on
/// pushing, so a silently-failed Back (the pane stayed put) self-heals in ONE
/// step — no timer, no epoch, no reaper.
#[test]
fn stale_expected_self_heals_in_one_step() {
    let (a, b) = (edit("/p/a.md"), edit("/p/b.md"));

    let mut h = PaneHistory::default();
    h.push(a.clone());
    h.push(b.clone());
    let mv = h.back().expect("back() must move");
    assert_eq!(mv.to, 0);
    assert_eq!(h.expected(), Some(&a), "expected is armed with A");

    // The nvim-side move failed silently, so the pane still shows B, matching neither `current()`
    // nor `expected` (both A): branch 4 fires regardless of the stale `expected`.
    h.observe(b.clone());
    assert_eq!(
        stack(&h),
        vec![a.clone(), b.clone()],
        "the failed-nav emit pushes B after truncating ahead of the moved cursor"
    );
    assert_eq!(h.cursor(), 1, "the cursor follows the pushed entry");
    assert_eq!(
        h.expected(),
        None,
        "the stale `expected` is cleared by the push"
    );

    // And history keeps working normally afterwards — nothing is wedged.
    h.observe(a.clone());
    assert_eq!(
        stack(&h),
        vec![a.clone(), b.clone(), a.clone()],
        "a subsequent genuine navigation pushes normally (no permanent wedge)"
    );
    assert_eq!(h.cursor(), 2);
}

// === 2. Browser stack semantics ===

/// Browser semantics: navigating from a mid-stack cursor TRUNCATES everything
/// ahead of it. `A -> B -> C` with `A` active, then `D`, leaves `A -> D`.
#[test]
fn new_navigation_truncates_everything_ahead_of_the_cursor() {
    let (a, b, c, d) = (
        edit("/p/a.md"),
        edit("/p/b.md"),
        edit("/p/c.md"),
        edit("/p/d.md"),
    );

    let mut h = PaneHistory::default();
    h.push(a.clone());
    h.push(b.clone());
    h.push(c.clone());
    h.back().expect("back() to B");
    h.back().expect("back() to A");
    assert_eq!(h.cursor(), 0, "the cursor is parked on A");

    h.observe(d.clone());
    assert_eq!(
        stack(&h),
        vec![a.clone(), d.clone()],
        "B and C (ahead of the cursor) must be dropped"
    );
    assert_eq!(h.cursor(), 1, "the cursor lands on the new entry");
}

/// Locked decision "keep every visit": the stack is a literal VISIT LOG, so
/// `A -> B -> A -> B` is FOUR entries. There is no dedup in the stack (dedup
/// happens only when rendering menu rows).
#[test]
fn every_visit_is_kept_the_stack_never_dedups() {
    let (a, b) = (edit("/p/a.md"), edit("/p/b.md"));

    let mut h = PaneHistory::default();
    for t in [a.clone(), b.clone(), a.clone(), b.clone()] {
        h.observe(t);
    }

    assert_eq!(
        stack(&h),
        vec![a.clone(), b.clone(), a.clone(), b.clone()],
        "A -> B -> A -> B must be four entries"
    );
    assert_eq!(h.cursor(), 3, "the cursor is on the newest visit");
}

/// An IDEMPOTENT re-emit of the current target pushes nothing (`observe` branch
/// 2). nvim re-emits `wolf_buffers` on many autocmds, so this fires constantly.
#[test]
fn idempotent_reemit_pushes_only_once() {
    let x = edit("/p/x.md");

    let mut h = PaneHistory::default();
    h.observe(x.clone());
    h.observe(x.clone());
    h.observe(x.clone());

    assert_eq!(
        stack(&h),
        vec![x],
        "repeat emits of the CURRENT target push once"
    );
    assert_eq!(h.cursor(), 0);
}

/// The FIRST `observe` on an empty pane SEEDS the stack: no truncate, no push
/// bookkeeping, cursor 0, `expected` clear.
#[test]
fn first_observe_seeds_the_empty_stack() {
    let a = edit("/p/a.md");

    let mut h = PaneHistory::default();
    assert!(h.entries().is_empty(), "a default PaneHistory is empty");
    assert_eq!(h.cursor(), 0, "an empty pane's cursor is 0");
    assert_eq!(h.current(), None, "an empty pane has no current entry");

    h.observe(a.clone());
    assert_eq!(
        stack(&h),
        vec![a.clone()],
        "the first observe seeds the stack"
    );
    assert_eq!(h.cursor(), 0);
    assert_eq!(h.current(), Some(&a));
    assert_eq!(h.expected(), None);
}

/// The stack is capped at [`MAX_ENTRIES`]: the OLDEST entries are dropped and the
/// cursor is shifted by exactly the drained count (never underflowing).
#[test]
fn cap_drops_oldest_entries_and_shifts_the_cursor() {
    /// How far past the cap this test pushes.
    const OVERFLOW: usize = 5;

    let mut h = PaneHistory::default();
    let all: Vec<NavTarget> = (0..MAX_ENTRIES + OVERFLOW)
        .map(|i| edit(&format!("/p/f{i}.md")))
        .collect();
    for t in &all {
        h.push(t.clone());
    }

    assert_eq!(
        h.entries().len(),
        MAX_ENTRIES,
        "the stack is capped at MAX_ENTRIES"
    );
    assert_eq!(
        h.cursor(),
        MAX_ENTRIES - 1,
        "the cursor stays on the newest entry after the drain"
    );
    assert_eq!(
        h.entries().first(),
        Some(&all[OVERFLOW]),
        "the {OVERFLOW} oldest entries were dropped"
    );
    assert_eq!(
        h.entries().last(),
        Some(&all[MAX_ENTRIES + OVERFLOW - 1]),
        "the newest entry survives"
    );
    assert_eq!(
        h.current(),
        Some(&all[MAX_ENTRIES + OVERFLOW - 1]),
        "current() follows the shifted cursor"
    );

    // A push from a MID-STACK cursor truncates first, so it frees room and can
    // never drain (the cursor arithmetic therefore can never underflow).
    let mid = MAX_ENTRIES / 2;
    h.jump(mid).expect("jump to the middle of the stack");
    assert_eq!(h.cursor(), mid);
    let fresh = edit("/p/fresh.md");
    h.push(fresh.clone());
    assert_eq!(
        h.entries().len(),
        mid + 2,
        "a mid-stack push truncates ahead of the cursor, so nothing is drained"
    );
    assert_eq!(h.cursor(), mid + 1, "the cursor never underflows");
    assert_eq!(h.current(), Some(&fresh));
}

/// `can_back` / `can_forward` at both ends and in the middle.
#[test]
fn can_back_and_can_forward_track_the_cursor() {
    let (a, b, c) = (edit("/p/a.md"), edit("/p/b.md"), edit("/p/c.md"));

    let mut h = PaneHistory::default();
    assert!(!h.can_back(), "an EMPTY pane cannot go back");
    assert!(!h.can_forward(), "an EMPTY pane cannot go forward");

    h.push(a.clone());
    assert!(!h.can_back(), "a single-entry pane cannot go back");
    assert!(!h.can_forward(), "a single-entry pane cannot go forward");

    h.push(b.clone());
    h.push(c.clone());
    assert!(h.can_back(), "at the TOP of a 3-stack, back is available");
    assert!(!h.can_forward(), "at the TOP, forward is not");

    h.back().expect("back() to B");
    assert!(h.can_back(), "in the MIDDLE, back is available");
    assert!(h.can_forward(), "in the MIDDLE, forward is available");

    h.back().expect("back() to A");
    assert!(!h.can_back(), "at the BOTTOM, back is not available");
    assert!(h.can_forward(), "at the BOTTOM, forward is");

    // `forward` mirrors `back`: it reports the move and arms `expected`.
    let mv = h.forward().expect("forward() must move from 0 to 1");
    assert_eq!(mv.from, 0);
    assert_eq!(mv.to, 1);
    assert_eq!(mv.target, b);
    assert_eq!(h.expected(), Some(&b), "forward() arms `expected` too");
}

/// `jump(index)`: a valid index moves and arms `expected`; jumping to the CURSOR's
/// own index is a true no-op returning `None` (the locked checkmark rule); an
/// out-of-range index returns `None` and leaves the pane untouched.
#[test]
fn jump_moves_only_for_a_valid_other_index() {
    let (a, b, c) = (edit("/p/a.md"), edit("/p/b.md"), edit("/p/c.md"));

    let mut h = PaneHistory::default();
    h.push(a.clone());
    h.push(b.clone());
    h.push(c.clone());

    let mv = h.jump(0).expect("jump(0) from cursor 2 must move");
    assert_eq!(mv.from, 2);
    assert_eq!(mv.to, 0);
    assert_eq!(mv.target, a);
    assert_eq!(h.cursor(), 0);
    assert_eq!(h.expected(), Some(&a), "jump() arms `expected`");

    assert!(
        h.jump(0).is_none(),
        "jump() to the CURSOR's own index is a true no-op (returns None), so \
         clicking the checkmarked menu row does nothing"
    );
    assert_eq!(h.cursor(), 0, "the no-op jump left the cursor alone");
    assert_eq!(stack(&h), vec![a.clone(), b.clone(), c.clone()]);

    assert!(
        h.jump(99).is_none(),
        "an out-of-range index must return None"
    );
    assert_eq!(h.cursor(), 0, "an out-of-range jump left the cursor alone");
    assert_eq!(
        stack(&h),
        vec![a, b, c],
        "an out-of-range jump left the stack alone"
    );
}

/// `revert(from, to)` undoes a FAILED navigation, but ONLY when the cursor is
/// still where the move left it — a racing `observe` that already reconciled the
/// pane WINS.
#[test]
fn revert_restores_only_when_the_cursor_has_not_moved_on() {
    let (a, b, c) = (edit("/p/a.md"), edit("/p/b.md"), edit("/p/c.md"));

    // (a) Nothing raced: the cursor is still at `mv.to`, so revert restores it.
    let mut h = PaneHistory::default();
    h.push(a.clone());
    h.push(b.clone());
    let mv = h.back().expect("back() must move");
    assert_eq!((mv.from, mv.to), (1, 0));
    // The core reverts with from/to SWAPPED (`revert(win, from: mv.to, to: mv.from)`).
    h.revert(mv.to, mv.from);
    assert_eq!(h.cursor(), 1, "a failed move reverts the cursor");
    assert_eq!(h.expected(), None, "revert clears `expected`");
    assert_eq!(
        stack(&h),
        vec![a.clone(), b.clone()],
        "revert never edits entries"
    );

    // (b) A racing observe already reconciled the pane: revert is a NO-OP.
    let mut h = PaneHistory::default();
    h.push(a.clone());
    h.push(b.clone());
    let mv = h.back().expect("back() must move");
    h.observe(c.clone()); // genuine navigation lands first; cursor is now 1
    assert_eq!(stack(&h), vec![a.clone(), c.clone()]);
    assert_eq!(h.cursor(), 1);
    h.revert(mv.to, mv.from);
    assert_eq!(
        h.cursor(),
        1,
        "revert must NOT fire once a racing observe has moved the cursor off `mv.to`"
    );
    assert_eq!(stack(&h), vec![a, c], "the reconciled stack is untouched");
}

// === 3. `derive_target` + `NavTarget` identity ===

/// The full `derive_target` precedence matrix (plan §B9). A window present in
/// NEITHER map yields `None` — the locked "no entry, no push, no prune" clause
/// that makes the two notifications' arrival order irrelevant.
#[test]
fn derive_target_precedence_matrix() {
    let src = PathBuf::from("/p/source.md");

    struct Case {
        name: &'static str,
        buf: Option<WinBuffer>,
        preview: Option<PathBuf>,
        want: Option<NavTarget>,
    }

    let cases = vec![
        Case {
            name: "a preview source WINS over the window's buffer",
            buf: Some(named_buffer(7, "/p/a.md")),
            preview: Some(src.clone()),
            want: Some(preview("/p/source.md")),
        },
        Case {
            name: "a preview pane is absent from win_buffers: source alone suffices",
            buf: None,
            preview: Some(src.clone()),
            want: Some(preview("/p/source.md")),
        },
        Case {
            name: "a preview source even wins over a DRAWER buffer (stated precedence)",
            buf: Some(WinBuffer {
                bufnr: 9,
                path: None,
                is_terminal: true,
                is_drawer: true,
            }),
            preview: Some(src),
            want: Some(preview("/p/source.md")),
        },
        Case {
            name: "in NEITHER map -> no entry at all",
            buf: None,
            preview: None,
            want: None,
        },
        Case {
            name: "the terminal DRAWER is excluded from history",
            buf: Some(WinBuffer {
                bufnr: 9,
                path: None,
                is_terminal: true,
                is_drawer: true,
            }),
            preview: None,
            want: None,
        },
        Case {
            name: "a drawer is excluded even when it somehow carries a path",
            buf: Some(WinBuffer {
                bufnr: 9,
                path: Some(PathBuf::from("/p/a.md")),
                is_terminal: true,
                is_drawer: true,
            }),
            preview: None,
            want: None,
        },
        Case {
            name: "a :terminal in a NORMAL split IS tracked",
            buf: Some(WinBuffer {
                bufnr: 11,
                path: None,
                is_terminal: true,
                is_drawer: false,
            }),
            preview: None,
            want: Some(terminal(11)),
        },
        Case {
            name: "a named buffer is an Edit target",
            buf: Some(named_buffer(3, "/p/a.md")),
            preview: None,
            want: Some(edit("/p/a.md")),
        },
        Case {
            name: "an UNNAMED buffer is tracked by bufnr",
            buf: Some(WinBuffer {
                bufnr: 4,
                path: None,
                is_terminal: false,
                is_drawer: false,
            }),
            preview: None,
            want: Some(untitled(4)),
        },
    ];

    for case in cases {
        let got = derive_target(case.buf.as_ref(), case.preview.as_deref());
        assert_eq!(got, case.want, "derive_target case: {}", case.name);
    }
}

/// `NavTarget` is the dedup key, and `Edit(X)` / `Preview(X)` are DISTINCT — so
/// previewing the file you are editing is a real, separate history entry.
#[test]
fn edit_and_preview_of_the_same_path_are_distinct_entries() {
    let e = edit("/p/b.md");
    let p = preview("/p/b.md");
    assert_ne!(e, p, "Edit(B) and Preview(B) must NOT be equal");

    let mut h = PaneHistory::default();
    h.observe(e.clone());
    h.observe(p.clone());
    assert_eq!(
        stack(&h),
        vec![e, p],
        "previewing the edited file appends a SECOND entry"
    );
    assert_eq!(h.cursor(), 1);
}

// === 4. `NavHistory`: per-window fan-out and pruning ===

/// Two windows keep INDEPENDENT stacks, and `prune` / `prune_many` drop exactly
/// the named panes (an unknown handle is a silent no-op).
#[test]
fn nav_history_panes_are_independent_and_prunable() {
    const WIN_A: i64 = 1001;
    const WIN_B: i64 = 1002;
    const WIN_UNKNOWN: i64 = 4242;

    let (a, b, c) = (edit("/p/a.md"), edit("/p/b.md"), edit("/p/c.md"));

    let mut nav = NavHistory::default();
    nav.observe(WIN_A, a.clone());
    nav.observe(WIN_A, b.clone());
    nav.observe(WIN_B, c.clone());

    assert!(nav.can_back(WIN_A), "WIN_A has two entries");
    assert!(!nav.can_back(WIN_B), "WIN_B has one entry");
    assert!(
        !nav.can_back(WIN_UNKNOWN),
        "an unknown window can never go back"
    );
    assert!(
        nav.pane(WIN_UNKNOWN).is_none(),
        "an unknown window has no PaneHistory"
    );
    assert_eq!(
        nav.pane(WIN_A).map(|p| p.entries().len()),
        Some(2),
        "WIN_A's stack is its own"
    );
    assert_eq!(nav.pane(WIN_B).map(|p| p.entries().len()), Some(1));

    // Moving WIN_A does not disturb WIN_B.
    let mv = nav.back(WIN_A).expect("WIN_A can go back");
    assert_eq!((mv.from, mv.to), (1, 0));
    assert_eq!(nav.pane(WIN_B).map(|p| p.cursor()), Some(0));
    assert!(nav.back(WIN_B).is_none(), "WIN_B cannot go back");
    assert!(
        nav.back(WIN_UNKNOWN).is_none(),
        "an unknown window's back() is None"
    );

    // prune drops exactly that pane.
    nav.prune(WIN_A);
    assert!(nav.pane(WIN_A).is_none(), "WIN_A was pruned");
    assert!(!nav.can_back(WIN_A));
    assert!(
        nav.menu_rows(WIN_A, MAX_MENU_ROWS, None).is_empty(),
        "a pruned pane renders no rows"
    );
    assert_eq!(
        nav.pane(WIN_B).map(|p| p.entries().len()),
        Some(1),
        "WIN_B is untouched by WIN_A's prune"
    );

    // prune_many drops the listed panes and ignores unknown handles.
    nav.prune_many(&[WIN_B, WIN_UNKNOWN]);
    assert!(nav.pane(WIN_B).is_none(), "WIN_B was pruned");
    assert!(
        nav.menu_rows(WIN_B, MAX_MENU_ROWS, None).is_empty(),
        "a pruned pane renders no rows"
    );

    // Pruning an already-empty history is still a no-op.
    nav.prune_many(&[]);
    nav.prune(WIN_UNKNOWN);
}

/// `NavHistory` forwards `jump` / `revert` / `menu_rows` to the right pane.
#[test]
fn nav_history_forwards_jump_and_revert_per_window() {
    const WIN: i64 = 7;

    let (a, b, c) = (edit("/p/a.md"), edit("/p/b.md"), edit("/p/c.md"));

    let mut nav = NavHistory::default();
    nav.observe(WIN, a.clone());
    nav.observe(WIN, b);
    nav.observe(WIN, c);

    let mv = nav.jump(WIN, 0).expect("jump(0) must move from cursor 2");
    assert_eq!((mv.from, mv.to), (2, 0));
    assert_eq!(mv.target, a);
    assert!(nav.can_forward(WIN), "the cursor is at the bottom");

    // A failed navigation reverts.
    nav.revert(WIN, mv.to, mv.from);
    assert_eq!(nav.pane(WIN).map(|p| p.cursor()), Some(2));
    assert!(!nav.can_forward(WIN), "the cursor is back at the top");

    // Unknown windows are silent no-ops.
    assert!(nav.jump(999, 0).is_none());
    nav.revert(999, 0, 1);
}

// === 4b. `NavHistory::observe`: the emit-level repeat filter above the four-branch rule ===
// It drops a target repeating the last one folded for its window; its doc comment says why.

/// **The D2 case.** A late repeat of the target we just LEFT is DROPPED, not
/// pushed — and the filter does not swallow the genuine navigation that follows.
#[test]
fn a_late_repeat_of_the_target_we_left_is_dropped_not_pushed() {
    const WIN: i64 = 1000;

    let (a, b, c) = (edit("/p/a.md"), edit("/p/b.md"), edit("/p/c.md"));

    let mut nav = NavHistory::default();
    nav.observe(WIN, a.clone());
    nav.observe(WIN, b.clone());
    let mv = nav.back(WIN).expect("back() must move from B to A");
    assert_eq!((mv.from, mv.to), (1, 0), "the cursor moved off B onto A");

    // A late emit still reporting B, the state just left: folded into `PaneHistory` it would hit
    // branch 4 (neither `current()` nor `expected` is B) and push, truncating A's future.
    nav.observe(WIN, b.clone());

    {
        let pane = nav.pane(WIN).expect("the pane still exists");
        assert_eq!(
            stack(pane),
            vec![a.clone(), b.clone()],
            "the repeat of an already-folded target must be DROPPED: the stack is \
             untouched"
        );
        assert_eq!(pane.cursor(), 0, "the cursor stays where back() put it");
        assert_eq!(
            pane.expected(),
            Some(&a),
            "the filter sits ABOVE PaneHistory::observe, so a DROPPED emit does not \
             consume the armed `expected`"
        );
    }
    assert!(
        nav.can_forward(WIN),
        "LOAD-BEARING: B must still be ahead of the cursor. A pushed repeat would \
         have truncated it away and parked the cursor at the top."
    );
    assert!(!nav.can_back(WIN), "the cursor is on the OLDEST entry");

    // The filter must NOT swallow genuine navigation.
    nav.observe(WIN, c.clone());
    let pane = nav.pane(WIN).expect("the pane still exists");
    assert_eq!(
        stack(pane),
        vec![a, c],
        "a target that DIFFERS from the last folded one still reaches the stack and \
         pushes (truncating ahead of the cursor)"
    );
    assert_eq!(pane.cursor(), 1, "the cursor follows the pushed entry");
    assert!(
        !nav.can_forward(WIN),
        "the push parked the cursor at the top"
    );
}

/// The filter's memory is "the LAST target folded", not a permanent ban: our own
/// move landing still reaches `PaneHistory::observe` (where branch 2 absorbs it),
/// and a later genuine navigation BACK to the dropped target pushes normally.
#[test]
fn the_move_landing_reaches_the_stack_and_the_filter_is_not_a_permanent_ban() {
    const WIN: i64 = 1000;

    let (a, b) = (edit("/p/a.md"), edit("/p/b.md"));

    let mut nav = NavHistory::default();
    nav.observe(WIN, a.clone());
    nav.observe(WIN, b.clone());
    nav.back(WIN).expect("back() must move from B to A");
    nav.observe(WIN, b.clone()); // the late repeat: dropped

    // Our own move landing is a real change since the last fold, so it reaches
    // `PaneHistory::observe`, where branch 2 absorbs it and clears `expected`.
    nav.observe(WIN, a.clone());
    {
        let pane = nav.pane(WIN).expect("the pane still exists");
        assert_eq!(
            stack(pane),
            vec![a.clone(), b.clone()],
            "the landing emit is ABSORBED, not pushed"
        );
        assert_eq!(pane.cursor(), 0, "the cursor is still on A");
        assert_eq!(
            pane.expected(),
            None,
            "the landing emit DID reach PaneHistory::observe, which cleared `expected`"
        );
    }

    // Navigating genuinely back to B is not blocked by B having been dropped
    // earlier — the filter only remembers the LAST folded target (now A).
    nav.observe(WIN, b.clone());
    let pane = nav.pane(WIN).expect("the pane still exists");
    assert_eq!(
        stack(pane),
        vec![a, b],
        "a genuine navigation to B pushes (truncating ahead of the cursor first)"
    );
    assert_eq!(pane.cursor(), 1, "the cursor follows the pushed entry");
}

/// Pruning a window clears its FILTER as well as its stack, so the very next
/// observation of the same target seeds a FRESH stack instead of being dropped as
/// a repeat. Covers both `prune` and `prune_many`.
#[test]
fn prune_clears_the_repeat_filter_so_the_pane_can_be_reseeded() {
    const WIN_A: i64 = 1000;
    const WIN_B: i64 = 1001;

    let a = edit("/p/a.md");

    let mut nav = NavHistory::default();
    nav.observe(WIN_A, a.clone());
    nav.observe(WIN_B, a.clone());

    nav.prune(WIN_A);
    nav.prune_many(&[WIN_B]);
    assert!(nav.pane(WIN_A).is_none(), "WIN_A was pruned");
    assert!(nav.pane(WIN_B).is_none(), "WIN_B was pruned");

    // The SAME target that was folded before the prune. A filter that survived the
    // prune would drop this and leave the pane empty forever.
    nav.observe(WIN_A, a.clone());
    nav.observe(WIN_B, a.clone());

    assert_eq!(
        nav.pane(WIN_A).map(stack),
        Some(vec![a.clone()]),
        "prune must clear the filter too, so the pane re-seeds"
    );
    assert_eq!(
        nav.pane(WIN_A).map(|p| p.cursor()),
        Some(0),
        "a re-seeded pane starts at cursor 0"
    );
    assert_eq!(
        nav.pane(WIN_B).map(stack),
        Some(vec![a]),
        "prune_many must clear the filter too"
    );
}

/// The filter is keyed PER WINDOW. Every `wolf_buffers` emit reports every window,
/// so the same target is routinely folded for two panes; a global filter would
/// silently swallow the second pane's first sighting.
#[test]
fn the_repeat_filter_is_keyed_per_window() {
    const WIN_W: i64 = 1000;
    const WIN_V: i64 = 1001;

    let (a, b) = (edit("/p/a.md"), edit("/p/b.md"));

    let mut nav = NavHistory::default();
    nav.observe(WIN_W, a.clone());
    nav.observe(WIN_W, b.clone());
    nav.observe(WIN_W, b.clone()); // a repeat in W, dropped by W's own filter

    nav.observe(WIN_V, b.clone());
    assert_eq!(
        nav.pane(WIN_V).map(stack),
        Some(vec![b.clone()]),
        "W's filter must NOT suppress V's FIRST observation of the same target"
    );
    assert_eq!(
        nav.pane(WIN_V).map(|p| p.cursor()),
        Some(0),
        "V seeded its own stack"
    );

    // V then keeps its OWN memory: its repeat is dropped, its change lands.
    nav.observe(WIN_V, b.clone());
    assert_eq!(
        nav.pane(WIN_V).map(|p| p.entries().len()),
        Some(1),
        "V's own repeat is dropped"
    );
    nav.observe(WIN_V, a.clone());
    assert_eq!(
        nav.pane(WIN_V).map(stack),
        Some(vec![b.clone(), a.clone()]),
        "a genuine change in V still pushes"
    );

    assert_eq!(
        nav.pane(WIN_W).map(stack),
        Some(vec![a, b]),
        "W's stack is untouched by any of V's folds"
    );
}

// === 5. Menu rows: selection, cap, ordering, checkmark ===

/// Rows are NEWEST-FIRST, carry real stack indices, and exactly ONE is
/// checkmarked. The indices must be usable verbatim by `jump`.
#[test]
fn menu_rows_are_newest_first_with_exactly_one_checkmark() {
    let (a, b, c) = (edit("/p/a.md"), edit("/p/b.md"), edit("/p/c.md"));

    let mut h = PaneHistory::default();
    h.observe(a);
    h.observe(b.clone());
    h.observe(c);

    let rows = h.menu_rows(MAX_MENU_ROWS, None);
    assert_eq!(
        displays(&rows),
        vec!["c.md".to_string(), "b.md".to_string(), "a.md".to_string()],
        "rows are newest-first"
    );
    assert_eq!(
        indices(&rows),
        vec![2, 1, 0],
        "row indices are STACK indices"
    );
    assert_eq!(current_count(&rows), 1, "exactly one row is checkmarked");
    assert_eq!(
        current_row(&rows).map(|r| r.index),
        Some(2),
        "the checkmark is on the cursor's entry"
    );

    // A row index round-trips through jump().
    let mv = h
        .jump(rows[1].index)
        .expect("jumping to a row index must move");
    assert_eq!(mv.to, 1);
    assert_eq!(mv.target, b, "row index 1 addresses the B entry");
}

/// The menu is capped at [`MAX_MENU_ROWS`] DISTINCT rows over a repeat-heavy
/// stack, keeping the FIRST (newest) occurrence of each distinct target.
#[test]
fn menu_rows_cap_at_ten_distinct_rows() {
    /// Distinct files cycled through, > MAX_MENU_ROWS so the cap really bites.
    const DISTINCT: usize = 12;
    /// Total visits recorded (a repeat-heavy log).
    const VISITS: usize = 30;

    let mut h = PaneHistory::default();
    for i in 0..VISITS {
        h.observe(edit(&format!("/p/f{}.md", i % DISTINCT)));
    }
    assert_eq!(
        h.entries().len(),
        VISITS,
        "every visit is kept in the STACK (dedup is a MENU concern)"
    );

    let rows = h.menu_rows(MAX_MENU_ROWS, None);
    assert_eq!(
        rows.len(),
        MAX_MENU_ROWS,
        "the menu caps at 10 distinct rows"
    );

    let want_indices: Vec<usize> = (0..MAX_MENU_ROWS).map(|k| VISITS - 1 - k).collect();
    assert_eq!(
        indices(&rows),
        want_indices,
        "the FIRST (newest) occurrence of each distinct target is kept"
    );

    let want_displays: Vec<String> = want_indices
        .iter()
        .map(|i| format!("f{}.md", i % DISTINCT))
        .collect();
    assert_eq!(
        displays(&rows),
        want_displays,
        "labels follow the kept rows"
    );

    let mut seen = displays(&rows);
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), MAX_MENU_ROWS, "the kept rows are all DISTINCT");
    assert_eq!(current_count(&rows), 1, "exactly one row is checkmarked");
    assert!(rows[0].is_current, "the cursor is on the newest visit");
}

/// **The locked checkmark rule.** When the cursor's target ALSO appears NEWER in
/// the stack, the checkmarked row keeps that newest row's POSITION but its `index`
/// is overridden to the CURSOR's index — so clicking the checkmark is a true
/// no-op, not a jump forward.
#[test]
fn checkmarked_row_index_is_the_cursor_index_not_the_newest_instance() {
    let (a, b) = (edit("/p/a.md"), edit("/p/b.md"));

    let mut h = PaneHistory::default();
    h.observe(a.clone()); // index 0
    h.observe(b.clone()); // index 1
    h.observe(a.clone()); // index 2 — a NEWER instance of A
    h.back().expect("back() to B");
    h.back().expect("back() to A at index 0");
    assert_eq!(h.cursor(), 0, "the cursor is on the OLDEST A");
    assert_eq!(h.current(), Some(&a));

    let rows = h.menu_rows(MAX_MENU_ROWS, None);
    assert_eq!(
        displays(&rows),
        vec!["a.md".to_string(), "b.md".to_string()],
        "the newest-first walk keeps A (drawn from index 2) then B"
    );
    assert_eq!(current_count(&rows), 1);
    assert!(rows[0].is_current, "the A row carries the checkmark");
    assert_eq!(
        rows[0].index, 0,
        "the checkmarked row's index is the CURSOR's index (0), NOT the newer \
         instance it was drawn from (2)"
    );
    assert_eq!(rows[1].index, 1, "the other row keeps its real stack index");
    assert!(!rows[1].is_current);

    // Clicking the checkmarked row is a true no-op.
    assert!(
        h.jump(rows[0].index).is_none(),
        "jumping to the checkmarked row's index must be a no-op"
    );
}

/// When the cursor's target does NOT survive the distinct-row cap, NO row is
/// checkmarked (documented behavior, not a bug).
#[test]
fn no_checkmark_when_the_cursor_target_falls_outside_the_cap() {
    /// A deliberately tiny cap so the cursor's entry is squeezed out.
    const TINY_LIMIT: usize = 2;

    let (a, b, c) = (edit("/p/a.md"), edit("/p/b.md"), edit("/p/c.md"));

    let mut h = PaneHistory::default();
    h.observe(a);
    h.observe(b);
    h.observe(c);
    h.back().expect("back() to B");
    h.back().expect("back() to A");

    let rows = h.menu_rows(TINY_LIMIT, None);
    assert_eq!(
        displays(&rows),
        vec!["c.md".to_string(), "b.md".to_string()],
        "only the two newest distinct rows fit"
    );
    assert_eq!(
        current_count(&rows),
        0,
        "the cursor's target (A) was squeezed out, so NO row is checkmarked"
    );
}

/// An empty pane renders no rows at all.
#[test]
fn menu_rows_of_an_empty_pane_are_empty() {
    let h = PaneHistory::default();
    assert!(h.menu_rows(MAX_MENU_ROWS, None).is_empty());
}

// === 6. Label pipeline: prefix form, minimal growth ===

/// **The worked example from the locked decisions.** `foo/README.md`,
/// `bar/README.md`, `bar/some-other-file.md` render (newest-first) as
/// `some-other-file.md`, `bar/README.md`, `foo/README.md`: the singleton stays
/// bare while the colliding `README.md` group grows by exactly ONE parent segment.
#[test]
fn worked_example_prefix_labels() {
    let mut h = PaneHistory::default();
    h.observe(edit("/proj/foo/README.md"));
    h.observe(edit("/proj/bar/README.md"));
    h.observe(edit("/proj/bar/some-other-file.md"));

    let rows = h.menu_rows(MAX_MENU_ROWS, None);
    assert_eq!(
        displays(&rows),
        vec![
            "some-other-file.md".to_string(),
            "bar/README.md".to_string(),
            "foo/README.md".to_string(),
        ],
        "PREFIX form with minimal growth (the locked worked example)"
    );
}

/// The deeper case: `a/b/README.md` vs `c/b/README.md`. ONE parent segment leaves
/// BOTH as `b/README.md`, so the labels must grow to THREE components each.
#[test]
fn two_parent_segments_are_needed_when_one_still_collides() {
    let p1 = PathBuf::from("/root/a/b/README.md");
    let p2 = PathBuf::from("/root/c/b/README.md");

    let labels = disambiguate(&[p1.clone(), p2.clone()], None);

    assert_eq!(
        labels.get(&p1).map(String::as_str),
        Some("a/b/README.md"),
        "one parent segment (b/README.md) still collides, so growth continues"
    );
    assert_eq!(
        labels.get(&p2).map(String::as_str),
        Some("c/b/README.md"),
        "one parent segment (b/README.md) still collides, so growth continues"
    );
    assert_eq!(labels.len(), 2, "one label per distinct path");
}

/// `Preview(X)` and `Edit(X)` are DISTINCT stack entries but contribute the SAME
/// path ONCE to the collision groups — so they share a label body and only the
/// preview row is `"Preview: "`-prefixed.
#[test]
fn preview_and_edit_of_one_path_contribute_a_single_label_body() {
    let mut h = PaneHistory::default();
    h.observe(edit("/proj/bar/README.md"));
    h.observe(edit("/proj/foo/README.md"));
    h.observe(preview("/proj/bar/README.md"));

    let rows = h.menu_rows(MAX_MENU_ROWS, None);
    assert_eq!(
        displays(&rows),
        vec![
            "Preview: bar/README.md".to_string(),
            "foo/README.md".to_string(),
            "bar/README.md".to_string(),
        ],
        "the shared path contributes ONE entry to the README.md collision group; \
         the preview row is decorated, not re-disambiguated"
    );
    assert_eq!(
        rows.len(),
        3,
        "the two bar/README.md rows stay DISTINCT rows"
    );
}

/// Pathless rows (`Terminal` / `Untitled`) are EXCLUDED from the collision groups
/// and numbered by first appearance in the newest-first ROW order.
#[test]
fn pathless_rows_are_excluded_from_collisions_and_numbered() {
    let mut h = PaneHistory::default();
    h.observe(terminal(1));
    h.observe(untitled(2));
    h.observe(terminal(3));
    h.observe(untitled(4));
    h.observe(edit("/proj/README.md"));

    let rows = h.menu_rows(MAX_MENU_ROWS, None);
    assert_eq!(
        displays(&rows),
        vec![
            "README.md".to_string(),
            "Untitled".to_string(),
            "Terminal".to_string(),
            "Untitled 2".to_string(),
            "Terminal 2".to_string(),
        ],
        "numbering follows first appearance NEWEST-FIRST; the lone path stays a \
         bare basename (pathless rows never join a collision group)"
    );
}

/// A label that ends up as a FULL ABSOLUTE path gets the home directory
/// abbreviated to `~`.
///
/// Contract assumption (stated in the test-writer report): a path with FEWER
/// components than the current `k` contributes its FULL ABSOLUTE form. Here the
/// `README.md` group needs `k == 4` to separate `a/b/c` from `x/b/c`, and
/// `/Users/alice/README.md` has only three components — so it falls back to its
/// absolute form and is then home-abbreviated.
#[test]
fn full_absolute_labels_abbreviate_home_to_tilde() {
    let home = PathBuf::from("/Users/alice");
    let shallow = PathBuf::from("/Users/alice/README.md");
    let deep_a = PathBuf::from("/a/b/c/README.md");
    let deep_x = PathBuf::from("/x/b/c/README.md");

    let labels: HashMap<PathBuf, String> = disambiguate(
        &[shallow.clone(), deep_a.clone(), deep_x.clone()],
        Some(home.as_path()),
    );

    assert_eq!(
        labels.get(&shallow).map(String::as_str),
        Some("~/README.md"),
        "a full-absolute label under home is abbreviated to ~"
    );
    assert_eq!(
        labels.get(&deep_a).map(String::as_str),
        Some("a/b/c/README.md")
    );
    assert_eq!(
        labels.get(&deep_x).map(String::as_str),
        Some("x/b/c/README.md")
    );
}

/// A SINGLETON group always renders bare — including a single-component,
/// root-level path — and home abbreviation never kicks in for it.
#[test]
fn singletons_render_bare_including_root_level_paths() {
    let root_level = PathBuf::from("/README.md");
    let nested = PathBuf::from("/a/b/c.md");
    let home = PathBuf::from("/a");

    let labels = disambiguate(&[root_level.clone(), nested.clone()], Some(home.as_path()));
    assert_eq!(
        labels.get(&root_level).map(String::as_str),
        Some("README.md"),
        "a root-level singleton renders as its bare basename"
    );
    assert_eq!(
        labels.get(&nested).map(String::as_str),
        Some("c.md"),
        "a nested singleton renders as its bare basename (no home abbreviation)"
    );

    assert!(disambiguate(&[], None).is_empty(), "no paths => no labels");
}

/// The label pipeline is IO-free: it must never touch the filesystem, so labels
/// are produced for paths that do not exist.
#[test]
fn labels_are_computed_for_nonexistent_paths() {
    let a = PathBuf::from("/definitely/not/on/disk/foo/README.md");
    let b = PathBuf::from("/definitely/not/on/disk/bar/README.md");
    let labels = disambiguate(&[a.clone(), b.clone()], None);
    assert_eq!(labels.get(&a).map(String::as_str), Some("foo/README.md"));
    assert_eq!(labels.get(&b).map(String::as_str), Some("bar/README.md"));
}

/// `Path`-typed home is accepted as `Option<&Path>` (compile-shape guard that the
/// signature is `home: Option<&Path>`, not `Option<&PathBuf>`).
#[test]
fn disambiguate_accepts_a_borrowed_path_home() {
    let home: &Path = Path::new("/Users/alice");
    let p = PathBuf::from("/Users/alice/notes/a.md");
    let labels = disambiguate(std::slice::from_ref(&p), Some(home));
    assert_eq!(labels.get(&p).map(String::as_str), Some("a.md"));
}
