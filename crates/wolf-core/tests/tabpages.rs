//! Integration tests for `wolf-core`'s TABPAGE lifecycle surface (plan §1 + §5.1):
//! the project/tabpage-per-view model where one captive Neovim holds multiple
//! nvim TABPAGES, one per front-end view. These drive a REAL, hermetic nvim (no
//! mocks) via `WOLF_NVIM_BIN`; the common helpers panic loudly if it is unset.
//!
//! ## Dependency on coder work (plan §1.5 + §1.1)
//!
//! These tests reference NEW `WolfCore` methods + a NEW `Snapshot` field the coder
//! adds; the file will not compile until they land — that is expected:
//!
//!   * `WolfCore::create_tabpage(cwd: Option<&Path>) -> i64` — blocking; `tabnew`
//!     (+ `:tcd cwd` when `Some`); returns the new tabpage HANDLE (0 on error).
//!   * `WolfCore::cwd_for_tabpage(handle: i64) -> Option<PathBuf>` — blocking;
//!     CANONICALIZED per-tabpage cwd (`getcwd(-1, <number>)`); `None` on error / a
//!     closed handle. Authoritative for a NON-current view's cwd.
//!   * `WolfCore::current_tabpage() -> i64` — blocking; the current tabpage handle.
//!   * `WolfCore::set_current_tabpage(handle: i64)` — fire-and-forget (ordered).
//!   * `WolfCore::close_tabpage(handle: i64)` — fire-and-forget (ordered).
//!   * `Snapshot.current_tabpage: i64` — the current tabpage handle, updated
//!     out-of-band via the `wolf_tabpage` notification (default 0 = unknown).
//!
//! ## Testing conventions (mirroring `redraw_multigrid.rs`)
//!
//!   * A tabpage switch churns GRID ids (nvim destroys the old tabpage's grids and
//!     creates fresh ones) but nvim WINDOW handles are STABLE — the load-bearing
//!     assumption OD2 rests on (see `window_handles_survive_a_tabpage_switch`). We
//!     assert on window-HANDLE membership + `grid_for_win` round-trips, never on
//!     literal grid-id values or HashMap order.
//!   * `Snapshot.current_tabpage` and the window/grid churn arrive across several
//!     flushes (and out-of-band notifications), so we POLL with a generous timeout,
//!     draining wakes, rather than waiting for a single flush.
//!   * macOS `/var`/`/tmp` are symlinks, so both sides of every cwd comparison are
//!     canonicalized (`cwd_for_tabpage` is already canonical core-side).

mod common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, Snapshot, WolfCore};

/// The outer/global grid id (never carries window content; always present).
const GLOBAL_GRID: u64 = 1;

/// Gap between polls while waiting for a structural change to settle.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Expected live window count on a single-window tabpage.
const ONE_WINDOW: usize = 1;
/// Expected live window count after a `:vsplit` (the keystone test).
const TWO_WINDOWS: usize = 2;

/// Polls `pred` until it returns `true`, draining wakes each iteration, or panics
/// after [`FLUSH_TIMEOUT`]. `desc` describes the awaited condition for the panic
/// message. A general-purpose variant of `redraw_multigrid.rs`'s `poll_until`: the
/// predicate is a plain closure, so it works for BOTH snapshot reads and blocking
/// core calls (`cwd_for_tabpage`, `list_buffers`).
fn poll_until(waiter: &FlushWaiter, desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if pred() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for: {desc}");
        }
        waiter.drain();
        sleep(POLL_INTERVAL);
    }
}

/// The set of live nvim WINDOW handles currently in the snapshot (the `win` field
/// of every `WinInfo`, which is keyed by grid id). Order-independent.
fn window_handles(snap: &Snapshot) -> HashSet<i64> {
    snap.windows.values().map(|w| w.win).collect()
}

/// The single window handle on a single-window tabpage, or panics if there is not
/// exactly one live window.
fn only_window_handle(core: &WolfCore) -> i64 {
    let snap = core.read_snapshot();
    let handles = window_handles(&snap);
    assert_eq!(
        handles.len(),
        ONE_WINDOW,
        "expected exactly one live window; got handles {handles:?}"
    );
    *handles.iter().next().expect("one window handle")
}

/// Types an Ex command `:<cmd><CR>` one key at a time. Used here for `:vsplit` and
/// `:tcd <path>` (NOT a `:tab*` command — those are locked out by OD4).
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Canonicalizes `p` for a symlink-robust comparison (macOS `/var` -> `/private/var`).
fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).expect("path should exist and canonicalize")
}

/// Waits for the initial single editor window to appear after attach, then returns
/// the first tabpage handle (blocking) — the "first-view bind".
fn attach_and_first_tabpage(core: &WolfCore, waiter: &FlushWaiter) -> i64 {
    poll_until(waiter, "the initial editor window to appear after attach", || {
        !core.read_snapshot().windows.is_empty()
    });
    let t1 = core.current_tabpage();
    assert_ne!(t1, 0, "current_tabpage() should be a nonzero handle after attach");
    t1
}

/// 1. Creating a second tabpage adds a fresh window grid and moves
///    `Snapshot.current_tabpage` to the new tabpage handle.
#[test]
fn create_tabpage_adds_window_and_updates_current_tabpage() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let t1 = attach_and_first_tabpage(&core, &waiter);
    let w1 = only_window_handle(&core);

    // A real directory so the new tabpage's `:tcd` succeeds.
    let dir_b = tempfile::tempdir().expect("failed to create dir B");
    let t2 = core.create_tabpage(Some(dir_b.path()));
    assert_ne!(t2, 0, "create_tabpage should return a nonzero new tabpage handle");
    assert_ne!(t2, t1, "the new tabpage handle must differ from the first tabpage");

    // Snapshot.current_tabpage must reflect the new tabpage, and a FRESH window
    // (distinct handle from the first tabpage's window) must appear.
    poll_until(&waiter, "Snapshot.current_tabpage to become the new tabpage T2", || {
        core.read_snapshot().current_tabpage == t2
    });
    poll_until(&waiter, "a fresh window grid on T2 (distinct from T1's window)", || {
        let snap = core.read_snapshot();
        !snap.windows.is_empty() && !window_handles(&snap).contains(&w1)
    });

    let snap = core.read_snapshot();
    assert_eq!(
        snap.current_tabpage, t2,
        "Snapshot.current_tabpage should be the new tabpage handle {t2}"
    );
    assert!(
        snap.grids.keys().any(|&id| id != GLOBAL_GRID),
        "T2 should have a window grid (id != {GLOBAL_GRID})"
    );

    core.shutdown().expect("shutdown should be Ok");
    drop(dir_b);
}

/// 2. Switching back to the first tabpage makes the SECOND tabpage's grids VANISH
///    and the first's reappear (the stale-binding contract): the snapshot only ever
///    holds the CURRENT tabpage's windows.
#[test]
fn switch_back_makes_second_tabpage_grids_vanish() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let t1 = attach_and_first_tabpage(&core, &waiter);
    let w1 = only_window_handle(&core);

    let t2 = core.create_tabpage(None);
    assert_ne!(t2, 0, "create_tabpage should return a nonzero handle");
    // Wait for T2 to be current AND its (fresh) window to have replaced T1's, so
    // the capture below is not racing the grid churn.
    poll_until(&waiter, "current_tabpage to become T2 with its own window present", || {
        let snap = core.read_snapshot();
        snap.current_tabpage == t2 && !snap.windows.is_empty() && !window_handles(&snap).contains(&w1)
    });

    // Capture T2's window handle + its window grid ids while T2 is current.
    let (w2, t2_grids) = {
        let snap = core.read_snapshot();
        let w2 = *window_handles(&snap)
            .iter()
            .next()
            .expect("T2 should have a window");
        let grids: HashSet<u64> = snap.windows.keys().copied().collect();
        (w2, grids)
    };
    assert_ne!(w2, w1, "T2's window handle should differ from T1's window handle");

    // Switch back to T1: T2's grids must vanish, and T1's window (stable handle w1)
    // must reappear on a live grid.
    core.set_current_tabpage(t1);
    poll_until(
        &waiter,
        "current_tabpage to return to T1 with T2's grids gone and T1's window back",
        || {
            let snap = core.read_snapshot();
            let cur = snap.current_tabpage == t1;
            let t2_gone = snap.windows.keys().all(|g| !t2_grids.contains(g))
                && snap.grid_for_win(w2).is_none();
            let t1_back = snap.grid_for_win(w1).is_some();
            cur && t2_gone && t1_back
        },
    );

    let snap = core.read_snapshot();
    assert_eq!(snap.current_tabpage, t1, "current_tabpage should be back at T1");
    assert!(
        snap.grid_for_win(w2).is_none(),
        "T2's window {w2} must NOT resolve to a grid while T1 is current (stale binding)"
    );
    let g1 = snap
        .grid_for_win(w1)
        .expect("T1's window w1 must resolve to a live grid again");
    assert!(
        snap.grids.contains_key(&g1),
        "T1's rebound grid {g1} must be a live grid in the snapshot"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// 3. **KEYSTONE (OD2):** nvim WINDOW handles are STABLE across a tabpage switch —
///    only GRID ids churn. Split T1 into W1,W2; switch to T2 and back; assert the
///    SAME W1,W2 reappear and both `grid_for_win` resolve to live grids.
///
/// The whole refocus-rebind design (OD2) rests on this. The coder runs it FIRST; if
/// it FAILS the coder must STOP and report. Assertions are deliberately explicit.
#[test]
fn window_handles_survive_a_tabpage_switch() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let t1 = attach_and_first_tabpage(&core, &waiter);

    // Split T1 so there are two live windows W1, W2.
    ex_command(&core, "vsplit");
    poll_until(&waiter, "T1 to reach two live windows after :vsplit", || {
        core.read_snapshot().windows.len() == TWO_WINDOWS
    });
    let split_handles = window_handles(&core.read_snapshot());
    assert_eq!(
        split_handles.len(),
        TWO_WINDOWS,
        "after :vsplit T1 should have exactly two window handles; got {split_handles:?}"
    );

    // Switch to a fresh tabpage T2, then back to T1.
    let t2 = core.create_tabpage(None);
    assert_ne!(t2, 0, "create_tabpage should return a nonzero handle");
    poll_until(&waiter, "current_tabpage to become T2", || {
        core.read_snapshot().current_tabpage == t2
    });

    core.set_current_tabpage(t1);
    poll_until(
        &waiter,
        "current_tabpage to return to T1 with two windows again",
        || {
            let snap = core.read_snapshot();
            snap.current_tabpage == t1 && snap.windows.len() == TWO_WINDOWS
        },
    );

    // The load-bearing assertion: the SAME two window handles reappear.
    let after = window_handles(&core.read_snapshot());
    assert_eq!(
        after, split_handles,
        "KEYSTONE FAILURE (OD2): window handles did NOT survive the tabpage switch — \
         before={split_handles:?}, after={after:?}. The refocus-rebind design depends \
         on stable window handles; if this fails the coder must STOP and report."
    );

    // And each surviving handle rebinds to a live (fresh) grid via grid_for_win.
    let snap = core.read_snapshot();
    for &win in &after {
        let grid = snap.grid_for_win(win).unwrap_or_else(|| {
            panic!(
                "KEYSTONE FAILURE (OD2): grid_for_win({win}) did not resolve after the \
                 switch back to T1 (windows={after:?})"
            )
        });
        assert_ne!(grid, 0, "grid_for_win({win}) must be a nonzero live grid, got 0");
        assert!(
            snap.grids.contains_key(&grid),
            "grid_for_win({win}) = {grid} must name a live grid in the snapshot"
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// 4. Per-tabpage cwd, INCLUDING a non-current read: `:tcd dirA` on T1 and create
///    T2 with `cwd = dirB`; assert `cwd_for_tabpage(T1) == dirA` AND
///    `cwd_for_tabpage(T2) == dirB` while T1 is current and T2 is NOT.
#[test]
fn per_tabpage_cwd_including_non_current() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let t1 = attach_and_first_tabpage(&core, &waiter);

    let dir_a = tempfile::tempdir().expect("failed to create dir A");
    let dir_b = tempfile::tempdir().expect("failed to create dir B");
    let canon_a = canon(dir_a.path());
    let canon_b = canon(dir_b.path());

    // `:tcd dirA` on T1 (tab-local cwd, per OD3). Path has no spaces (tempdir), so
    // it can be typed literally on the command line.
    let path_a = dir_a
        .path()
        .to_str()
        .expect("dir A path should be valid UTF-8");
    ex_command(&core, &format!("tcd {path_a}"));
    poll_until(&waiter, "cwd_for_tabpage(T1) to become dir A after :tcd", || {
        core.cwd_for_tabpage(t1).as_deref() == Some(canon_a.as_path())
    });

    // Create T2 rooted at dirB, then switch back to T1 so T2 is NON-current.
    let t2 = core.create_tabpage(Some(dir_b.path()));
    assert_ne!(t2, 0, "create_tabpage(dirB) should return a nonzero handle");
    poll_until(&waiter, "current_tabpage to become T2", || {
        core.read_snapshot().current_tabpage == t2
    });
    core.set_current_tabpage(t1);
    poll_until(&waiter, "current_tabpage to return to T1", || {
        core.read_snapshot().current_tabpage == t1
    });

    // T1 current: its cwd is dirA; T2 NOT current: its cwd is still dirB.
    assert_eq!(
        core.cwd_for_tabpage(t1).as_deref(),
        Some(canon_a.as_path()),
        "cwd_for_tabpage(T1) should be dir A while T1 is current"
    );
    assert_eq!(
        core.cwd_for_tabpage(t2).as_deref(),
        Some(canon_b.as_path()),
        "cwd_for_tabpage(T2) should be dir B for the NON-current tabpage"
    );

    core.shutdown().expect("shutdown should be Ok");
    drop((dir_a, dir_b));
}

/// 5. The buffer list is SHARED across tabpages: open `a.md` on T1 and `b.md` on
///    T2; `list_buffers()` returns BOTH regardless of which tabpage is current.
#[test]
fn buffer_list_is_shared_across_tabpages() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let t1 = attach_and_first_tabpage(&core, &waiter);

    // Two named files in one temp dir.
    let dir = tempfile::tempdir().expect("failed to create temp dir for buffers");
    let a_md = dir.path().join("a.md");
    let b_md = dir.path().join("b.md");
    std::fs::write(&a_md, "# a\n").expect("write a.md");
    std::fs::write(&b_md, "# b\n").expect("write b.md");

    // Does any listed buffer's basename equal `name`?
    let has_buffer = |core: &WolfCore, name: &str| {
        core.list_buffers()
            .iter()
            .any(|b| Path::new(&b.name).file_name().and_then(|f| f.to_str()) == Some(name))
    };

    // Open a.md on T1.
    core.send_input(InputEvent::Open { path: a_md.clone() });
    poll_until(&waiter, "a.md to appear in list_buffers on T1", || {
        has_buffer(&core, "a.md")
    });

    // Create T2 and open b.md there.
    let t2 = core.create_tabpage(None);
    assert_ne!(t2, 0, "create_tabpage should return a nonzero handle");
    poll_until(&waiter, "current_tabpage to become T2", || {
        core.read_snapshot().current_tabpage == t2
    });
    core.send_input(InputEvent::Open { path: b_md.clone() });
    poll_until(&waiter, "both a.md and b.md to be listed while T2 is current", || {
        has_buffer(&core, "a.md") && has_buffer(&core, "b.md")
    });

    assert!(
        has_buffer(&core, "a.md") && has_buffer(&core, "b.md"),
        "list_buffers() should contain BOTH a.md and b.md while T2 is current"
    );

    // Switch back to T1: the shared list still has both.
    core.set_current_tabpage(t1);
    poll_until(&waiter, "current_tabpage to return to T1", || {
        core.read_snapshot().current_tabpage == t1
    });
    assert!(
        has_buffer(&core, "a.md") && has_buffer(&core, "b.md"),
        "list_buffers() should still contain BOTH a.md and b.md after switching back to T1"
    );

    core.shutdown().expect("shutdown should be Ok");
    drop(dir);
}

/// 6. A tabpage's HANDLE survives a renumber: create T2, T3; close T2; assert T3's
///    HANDLE value is unchanged and `cwd_for_tabpage(T3)` still resolves (T3's
///    tabpage NUMBER shifted from 3 to 2, but its HANDLE did not).
#[test]
fn stable_handle_survives_tabpage_renumber() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let t1 = attach_and_first_tabpage(&core, &waiter);

    let dir_c = tempfile::tempdir().expect("failed to create dir C");
    let canon_c = canon(dir_c.path());

    let t2 = core.create_tabpage(None);
    assert_ne!(t2, 0, "create_tabpage(T2) should return a nonzero handle");
    poll_until(&waiter, "current_tabpage to become T2", || {
        core.read_snapshot().current_tabpage == t2
    });

    let t3 = core.create_tabpage(Some(dir_c.path()));
    assert_ne!(t3, 0, "create_tabpage(T3) should return a nonzero handle");
    assert_ne!(t3, t1, "T3 handle must differ from T1");
    assert_ne!(t3, t2, "T3 handle must differ from T2");
    poll_until(&waiter, "current_tabpage to become T3", || {
        core.read_snapshot().current_tabpage == t3
    });
    // Sanity: T3's cwd is dirC before the close.
    poll_until(&waiter, "cwd_for_tabpage(T3) to resolve to dir C", || {
        core.cwd_for_tabpage(t3).as_deref() == Some(canon_c.as_path())
    });

    // Close T2 (a NON-current tabpage). Its number is 2; T3's number shifts 3 -> 2.
    core.close_tabpage(t2);
    poll_until(&waiter, "T2's handle to become invalid after close_tabpage(T2)", || {
        core.cwd_for_tabpage(t2).is_none()
    });

    // The invariant: T3's HANDLE is unchanged (we still hold `t3`) and it still
    // resolves to dir C despite the renumber; T3 is still the current tabpage.
    assert_eq!(
        core.cwd_for_tabpage(t3).as_deref(),
        Some(canon_c.as_path()),
        "cwd_for_tabpage(T3) must still resolve to dir C after T2 closed (handle stable across renumber)"
    );
    assert_eq!(
        core.current_tabpage(),
        t3,
        "T3 should still be the current tabpage after closing the non-current T2"
    );

    core.shutdown().expect("shutdown should be Ok");
    drop(dir_c);
}

/// 7. **`win_hide` semantics (OD7):** on a tabpage switch nvim emits `win_hide`
///    (NOT `win_close`/`grid_destroy`) for the LEAVING tabpage's windows and REUSES
///    the same grid id + window handle on the way back. The `redraw.rs` `win_hide`
///    handler therefore REMOVES the window from `Snapshot.windows` (so
///    `windows`/`grid_for_win` reflect ONLY the current tabpage) while KEEPING its
///    grid in `Snapshot.grids` (a rebound window must still resolve to a live grid).
///
/// This DIRECTLY pins the window-removed / grid-kept contract that the whole
/// refocus-rebind design leans on — the other tests exercise it only indirectly via
/// the switch-back round-trip. Switch AWAY from T1 to a fresh T2 and assert, in one
/// snapshot: T1's window W1 is GONE from `windows` and `grid_for_win(W1)` no longer
/// resolves, BUT W1's original grid G1 is STILL a live entry in `grids`.
#[test]
fn win_hide_removes_window_but_keeps_grid_on_tabpage_switch() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let _t1 = attach_and_first_tabpage(&core, &waiter);
    let w1 = only_window_handle(&core);

    // Capture T1's window grid id G1 while T1 is current. It must be a real window
    // grid (id != the global grid), and W1 must resolve to it right now.
    let g1 = {
        let snap = core.read_snapshot();
        let g1 = snap
            .grid_for_win(w1)
            .expect("T1's window W1 must resolve to a live grid while T1 is current");
        assert_ne!(
            g1, GLOBAL_GRID,
            "T1's window grid must be a real window grid (id != {GLOBAL_GRID}), got {g1}"
        );
        g1
    };

    // Switch AWAY from T1 by creating (and entering) a fresh tabpage T2. nvim emits
    // `win_hide` for T1's window as it leaves.
    let t2 = core.create_tabpage(None);
    assert_ne!(t2, 0, "create_tabpage should return a nonzero handle");
    poll_until(
        &waiter,
        "current_tabpage to become T2 with T1's window W1 hidden (removed from windows)",
        || {
            let snap = core.read_snapshot();
            snap.current_tabpage == t2 && snap.grid_for_win(w1).is_none()
        },
    );

    // The load-bearing dual assertion, from a SINGLE snapshot:
    //   * WINDOW removed: W1 is absent from `windows` and no longer resolves.
    //   * GRID kept: W1's original grid G1 is STILL a live entry in `grids`.
    let snap = core.read_snapshot();
    assert!(
        snap.grid_for_win(w1).is_none(),
        "win_hide must REMOVE W1 from windows: grid_for_win({w1}) should be None after \
         leaving T1, got {:?}",
        snap.grid_for_win(w1)
    );
    assert!(
        !window_handles(&snap).contains(&w1),
        "win_hide must REMOVE W1's window entry from Snapshot.windows after leaving T1; \
         window handles are {:?}",
        window_handles(&snap)
    );
    assert!(
        snap.grids.contains_key(&g1),
        "win_hide must KEEP T1's grid {g1} in Snapshot.grids (nvim reuses it on the way \
         back); live grids are {:?}",
        snap.grids.keys().collect::<HashSet<_>>()
    );

    core.shutdown().expect("shutdown should be Ok");
}
