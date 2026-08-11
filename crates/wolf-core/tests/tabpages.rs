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
//!   * `WolfCore::cwd_for_tabpage(handle: i64) -> Option<PathBuf>` — a cheap
//!     SNAPSHOT read of `Snapshot.tab_cwds`, NOT a blocking RPC: the map is fed out
//!     of band by the `wolf_tab_cwds` notification and wholesale-replaced on every
//!     `DirChanged` / `TabNewEntered` / `TabClosed`. CANONICALIZED per-tabpage cwd;
//!     `None` for a closed handle or one no notification has covered yet.
//!     Authoritative for a NON-current view's cwd. `tabpage_cwd.rs` is the test
//!     that pins the no-RPC half, by resolving both a current and a non-current
//!     tabpage while nvim is wedged. Because the map arrives asynchronously, a
//!     test here POLLS for a value rather than reading once.
//!   * `WolfCore::current_tabpage() -> i64` — blocking; the current tabpage handle.
//!   * `WolfCore::set_current_tabpage(handle: i64)` — fire-and-forget (ordered).
//!   * `WolfCore::close_tabpage(handle: i64)` — fire-and-forget (ordered).
//!   * `Snapshot.current_tabpage: i64` — the current tabpage handle, updated
//!     out-of-band via the `wolf_tabpage` notification (default 0 = unknown).
//!
//! ## Testing conventions (mirroring `redraw_multigrid.rs`)
//!
//!   * A tabpage switch does NOT churn GRID ids. nvim emits `win_hide` for the
//!     LEAVING tabpage's windows rather than `win_close`/`grid_destroy`, and reuses
//!     the same grid id AND the same window handle on the way back —
//!     `win_hide_removes_window_but_keeps_grid_on_tabpage_switch` pins exactly that,
//!     and a round trip was measured to return `grid_for_win(w1)` unchanged. Window
//!     HANDLES being stable is the load-bearing assumption OD2 rests on (see
//!     `window_handles_survive_a_tabpage_switch`). We nevertheless assert on
//!     window-HANDLE membership + `grid_for_win` round-trips and never on literal
//!     grid-id values or HashMap order, because id REUSE is an observation about
//!     nvim rather than a contract this crate is entitled to lean on.
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

/// Tabpage count on a freshly attached core, which is the whole subject of
/// [`the_captive_config_turns_showtabline_off_at_one_tabpage`] and the baseline
/// [`a_second_tabpage_draws_no_neovim_tabline`] compares against. Named separately
/// from [`ONE_WINDOW`] because it counts a different thing: the two happen to be
/// equal on a freshly attached core and would stop being so the moment a case split
/// a window.
const ONE_TABPAGE: usize = 1;
/// Tabpage count after one `create_tabpage` — the only count at which Neovim's
/// default `showtabline` draws anything at all.
const TWO_TABPAGES: usize = 2;

/// The value `&showtabline` must hold, as a LITERAL rather than as a reference to
/// anything in `runtime/nvim/init.lua`.
///
/// Re-typed on purpose, the `markdown_render.rs` convention: an assertion must state
/// its expectation independently, so changing the captive config's value fails this
/// test rather than moving with it.
const SHOWTABLINE_OFF: i64 = 0;

/// Marker word the ONE-tabpage `showtabline` probe echoes — the whole reading of
/// [`the_captive_config_turns_showtabline_off_at_one_tabpage`], and the baseline
/// round trip of [`a_second_tabpage_draws_no_neovim_tabline`].
///
/// Unique per probe so each poll waits for ITS OWN message rather than for a value.
/// That is what keeps every red below an assertion failure carrying the text Neovim
/// actually produced, instead of a bare `poll_until` timeout that says only that
/// something never arrived.
const TABLINE_PROBE_ONE: &str = "WOLFTABPROBEONE";

/// Marker word the SECOND probe echoes, once the second tabpage is current. See
/// [`TABLINE_PROBE_ONE`].
const TABLINE_PROBE_TWO: &str = "WOLFTABPROBETWO";

/// The number of live windows the WHOLE nvim INSTANCE holds in
/// [`per_window_buffers_are_global_across_tabpages`]: one on each of its two
/// tabpages, which is the count the window-keyed maps must report if they are
/// global.
///
/// Named separately from [`TWO_WINDOWS`], which counts two windows on ONE tabpage:
/// the two are equal here and would stop being so the moment a case split a window,
/// and the whole point of this test is that the two counts are measured against
/// different maps. Follows the [`ONE_WINDOW`] / [`ONE_TABPAGE`] precedent above.
const TWO_WINDOWS_ACROSS_TWO_TABPAGES: usize = 2;

/// Basename of the fixture opened on the FIRST tabpage — the one that becomes a
/// BACKGROUND tabpage's window and whose entry in the window-keyed maps is the
/// subject of [`per_window_buffers_are_global_across_tabpages`].
const T1_FILE: &str = "a.md";

/// Basename of the fixture opened on the SECOND tabpage, which is the CURRENT one
/// while the assertions above are taken. See [`T1_FILE`].
const T2_FILE: &str = "b.md";

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

/// The BASENAME of the buffer `Snapshot.win_buffers` reports for the nvim WINDOW
/// handle `win`, or `None` when the window is absent from the map (or is present
/// holding an UNNAMED buffer, which `WinBuffer.path` reports as `None`).
///
/// A tail rather than the whole path on purpose: the assertions built on it say
/// WHICH window carries WHICH file, and the containing temp directory's absolute
/// prefix differs per run and per platform (macOS `/var` and `/tmp` are symlinks).
/// The full canonical-path claim is made exactly once, against `Snapshot.buffers`,
/// where the canonical form is the documented contract.
///
/// The two absent cases collapse to one `None` deliberately: neither is a legal
/// answer anywhere this is used, and the assertion messages name the map's whole
/// key set, which is what distinguishes them.
fn win_buffer_tail(snap: &Snapshot, win: i64) -> Option<String> {
    snap.win_buffers
        .get(&win)?
        .path
        .as_deref()
        .and_then(|p| p.file_name())
        .and_then(|f| f.to_str())
        .map(str::to_owned)
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
    poll_until(
        waiter,
        "the initial editor window to appear after attach",
        || !core.read_snapshot().windows.is_empty(),
    );
    let t1 = core.current_tabpage();
    assert_ne!(
        t1, 0,
        "current_tabpage() should be a nonzero handle after attach"
    );
    t1
}

/// Flattens the current toast into one string: each line's chunk texts concatenated,
/// lines joined with `\n`. `messages.rs`'s helper, re-typed here because each
/// `tests/*.rs` is its own crate.
fn toast_text(core: &WolfCore) -> String {
    let snap = core.read_snapshot();
    snap.toast
        .lines()
        .iter()
        .map(|line| line.iter().map(|c| c.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Asks the captive nvim, in ONE `:echo`, how many tabpages it has and what
/// `&showtabline` is, and returns the toast text carrying the answer.
///
/// `ext_messages` is what makes this readable at all: an `:echo` lands in
/// `Snapshot.toast` (see `messages.rs`), and core exposes no option-read API. It is
/// deliberately ONE message rather than two, so both numbers describe one instant —
/// a `showtabline` of `0` reported while only one tabpage exists proves nothing.
///
/// `len(gettabinfo())` rather than the idiomatic `tabpagenr('$')`: identical value,
/// and it keeps the typed command free of a `$` for the twin of this probe in
/// `crates/wolf-gnome/tests/smoke_headless.rs`, which types the same shape through a
/// double-quoted shell word.
fn tabline_probe(core: &WolfCore, waiter: &FlushWaiter, marker: &str) -> String {
    ex_command(
        core,
        &format!("echo printf('{marker} tabs=%d showtabline=%d', len(gettabinfo()), &showtabline)"),
    );
    poll_until(
        waiter,
        &format!("the {marker} :echo to surface in Snapshot.toast"),
        || toast_text(core).contains(marker),
    );
    toast_text(core)
}

/// Row counts of the GLOBAL grid and of `win`'s own window grid, read from ONE
/// snapshot so the pair describes a single instant.
fn grid_rows(core: &WolfCore, win: i64) -> (usize, usize) {
    let snap = core.read_snapshot();
    let global = snap
        .grids
        .get(&GLOBAL_GRID)
        .unwrap_or_else(|| {
            panic!(
                "the global grid {GLOBAL_GRID} must always be present; live grids are {:?}",
                snap.grids.keys().collect::<HashSet<_>>()
            )
        })
        .rows;
    let grid = snap.grid_for_win(win).unwrap_or_else(|| {
        panic!(
            "window {win} must resolve to a live grid to have a row count; window handles are {:?}",
            window_handles(&snap)
        )
    });
    let window = snap
        .grids
        .get(&grid)
        .unwrap_or_else(|| {
            panic!(
                "grid_for_win({win}) = {grid} must name a live grid; live grids are {:?}",
                snap.grids.keys().collect::<HashSet<_>>()
            )
        })
        .rows;
    (global, window)
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
    assert_ne!(
        t2, 0,
        "create_tabpage should return a nonzero new tabpage handle"
    );
    assert_ne!(
        t2, t1,
        "the new tabpage handle must differ from the first tabpage"
    );

    // Snapshot.current_tabpage must reflect the new tabpage, and a FRESH window
    // (distinct handle from the first tabpage's window) must appear.
    poll_until(
        &waiter,
        "Snapshot.current_tabpage to become the new tabpage T2",
        || core.read_snapshot().current_tabpage == t2,
    );
    poll_until(
        &waiter,
        "a fresh window grid on T2 (distinct from T1's window)",
        || {
            let snap = core.read_snapshot();
            !snap.windows.is_empty() && !window_handles(&snap).contains(&w1)
        },
    );

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
    poll_until(
        &waiter,
        "current_tabpage to become T2 with its own window present",
        || {
            let snap = core.read_snapshot();
            snap.current_tabpage == t2
                && !snap.windows.is_empty()
                && !window_handles(&snap).contains(&w1)
        },
    );

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
    assert_ne!(
        w2, w1,
        "T2's window handle should differ from T1's window handle"
    );

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
    assert_eq!(
        snap.current_tabpage, t1,
        "current_tabpage should be back at T1"
    );
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
    poll_until(
        &waiter,
        "T1 to reach two live windows after :vsplit",
        || core.read_snapshot().windows.len() == TWO_WINDOWS,
    );
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
        assert_ne!(
            grid, 0,
            "grid_for_win({win}) must be a nonzero live grid, got 0"
        );
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
    poll_until(
        &waiter,
        "cwd_for_tabpage(T1) to become dir A after :tcd",
        || core.cwd_for_tabpage(t1).as_deref() == Some(canon_a.as_path()),
    );

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
    // T2's cwd now arrives via the async wholesale `wolf_tab_cwds` notification, so
    // the T1 poll above may be satisfied by an earlier emit before T2's entry lands.
    // Poll for it rather than reading once (was safe under the old on-demand RPC).
    poll_until(
        &waiter,
        "cwd_for_tabpage(T2) to become dir B for the NON-current tabpage",
        || core.cwd_for_tabpage(t2).as_deref() == Some(canon_b.as_path()),
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
    poll_until(
        &waiter,
        "both a.md and b.md to be listed while T2 is current",
        || has_buffer(&core, "a.md") && has_buffer(&core, "b.md"),
    );

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
    poll_until(
        &waiter,
        "T2's handle to become invalid after close_tabpage(T2)",
        || core.cwd_for_tabpage(t2).is_none(),
    );

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

/// 8. **The captive config turns Neovim's own tabline off**, read at ONE tabpage —
///    where the setting draws nothing, so the reading is about the config alone.
///
/// ## Why this is a test of its own rather than test 9's first assertion
///
/// It was exactly that, and the arrangement hid the thing worth seeing. The only
/// falsifying edit that exists — deleting `opt.showtabline = 0` from
/// `runtime/nvim/init.lua` — makes this reading panic, which stopped test 9 before
/// it had created a second tabpage, so the ROW COST that same edit produces was
/// observed by no test at all and had to be reproduced with a probe outside the
/// tree. Split, one revert yields two independent reds: this test names the OPTION,
/// test 9 names what it COSTS.
///
/// ## Why this lives in `wolf-core` rather than in a front-end smoke
///
/// `showtabline` is a property of the SHARED captive config both front-ends
/// inherit — `runtime/nvim/init.lua` ships inside the macOS `.app` as
/// `Contents/Resources/init.lua` and is the same file this crate launches nvim
/// with. A `wolf-gnome` cage smoke could only ever observe the Linux half, and it
/// would spend compositor seconds to do it. This file is already enumerated as
/// `rust_test(name = "tabpages")` in `crates/wolf-core/BUILD.bazel`, and Bazel
/// enumerates FILES rather than test functions, so both cases cost no `BUILD.bazel`
/// edit and no cage time.
///
/// ## The captured red
///
/// With `opt.showtabline = 0` deleted the probe reads `tabs=1 showtabline=1`; with
/// the line present, `tabs=1 showtabline=0`.
#[test]
fn the_captive_config_turns_showtabline_off_at_one_tabpage() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let _t1 = attach_and_first_tabpage(&core, &waiter);

    let want = format!("{TABLINE_PROBE_ONE} tabs={ONE_TABPAGE} showtabline={SHOWTABLINE_OFF}");
    let probe = tabline_probe(&core, &waiter, TABLINE_PROBE_ONE);
    assert!(
        probe.contains(&want),
        "the captive nvim answered {probe:?}, which does not carry {want:?}. Two readings can be \
         wrong here and they mean different things. A `showtabline=1` is Neovim's DEFAULT, i.e. \
         `opt.showtabline = 0` is missing from `runtime/nvim/init.lua`: at ONE tabpage that draws \
         nothing, so nothing is broken on screen yet — `a_second_tabpage_draws_no_neovim_tabline` \
         is where the same edit costs the editor a real row. Both front-ends inherit that file, so \
         the fix is to restore the line; never a relaxation here. A `tabs=` other than \
         {ONE_TABPAGE} means this core did not start on a single tabpage, in which case the \
         reading is not about a fresh session at all."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// 9. **A second tabpage draws NO Neovim tabline**, and therefore costs the editor
///    no row — M8's artifact, and the whole reason test 8's option is set.
///
/// ## The artifact, measured
///
/// Neovim's default `showtabline` is `1`: draw the tabline as soon as there is
/// more than one tabpage. Neither front-end paints grid 1 — both drive
/// `ext_messages` — so the line is INVISIBLE and still real: it costs the pane
/// grid a whole row (measured 80x24 dropping to 80x23) and leaves every `WinInfo`
/// `start_row` stale until the front-end's own resize wins the row back.
///
/// ## Assertion ORDER is load-bearing
///
/// The artifact is asserted BEFORE the option is read back, which is the reverse of
/// the order the two are observed in. An option assertion placed first panics under
/// the only revert that exists and takes the row assertion down with it — the exact
/// defect test 8's split repairs, in miniature. Ordered this way, this test's red IS
/// the row cost, and test 8's red beside it is the option.
///
/// ## Assertions, classified
///
/// | # | assertion | kind | falsifying edit |
/// | --- | --- | --- | --- |
/// | 1 | probe ONE reports ONE tabpage | PREMISE — the baseline row count must be a one-tabpage reading | none |
/// | 2 | probe TWO reports TWO tabpages | PREMISE — the artifact exists only at two, so the comparison is vacuous without it | none |
/// | 3 | grid 1's row count is unchanged | PREMISE, no revert | none — see below |
/// | 4 | the WINDOW grid's row count is unchanged | **discriminator** (the artifact itself) | delete `opt.showtabline = 0` from `runtime/nvim/init.lua` |
/// | 5 | probe TWO reads `showtabline=0` | premise-guard for assertion 4 | the same edit, but **no red of its own**: assertion 4 is ordered ahead of it and that edit reddens there first. What it catches ALONE is `showtabline` back on with the rows NOT moving — a future nvim paying for the tabline somewhere else, which would leave assertion 4 green while the line was drawn |
///
/// Assertion 3 is stated as a premise rather than sold as a discriminator, and the
/// reason is worth writing down: grid 1 is sized by the UI attach and nvim does not
/// resize it to make room for a tabline — the tabline is drawn INSIDE it and the
/// row is taken from the window grids instead. So assertion 3 holds under BOTH
/// settings. It is asserted because it is exactly the premise assertion 4 rests on:
/// if a future nvim ever paid for the tabline out of the global grid, assertion 4
/// would go green while the artifact was still there, and this row is what would
/// name that.
///
/// ## The captured red
///
/// With `opt.showtabline = 0` deleted, the probes read `tabs=1 showtabline=1` and
/// `tabs=2 showtabline=1`, the global grid holds 24 rows on both sides, and the
/// WINDOW grid goes 24 -> 23: this test fails at assertion 4 naming that row, and
/// test 8 fails beside it naming the option. With the line present, `showtabline=0`
/// at both probes and the window grid flat at 24.
///
/// ## What depends on this elsewhere
///
/// `crates/wolf-gnome/tests/smoke_headless.rs`'s
/// `native_tabs_open_switch_and_close_under_cage` asserts `H2 < H1` — that opening
/// a second GTK tab costs the editor a row — and attributes that row to the
/// `adw::TabBar`. That attribution is only sound while nvim draws no tabline of its
/// own, or the row would be lost for the wrong reason and the smoke would stay
/// green with the bar never appended. That smoke reads `&showtabline` back for
/// itself as a precondition; this test and test 8 are what make the value it reads
/// correct.
#[test]
fn a_second_tabpage_draws_no_neovim_tabline() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let _t1 = attach_and_first_tabpage(&core, &waiter);
    let w1 = only_window_handle(&core);

    // Probe ONE is an ORDERING DEVICE first and a premise second. Its `showtabline`
    // VALUE is deliberately not asserted here: that reading belongs to
    // `the_captive_config_turns_showtabline_off_at_one_tabpage`, and asserting it
    // here would put a panic back in front of assertion 4 — the masking this pair of
    // tests exists to undo. The text is carried into every message below instead.
    let probe_one = tabline_probe(&core, &waiter, TABLINE_PROBE_ONE);

    // --- Assertion 1 (PREMISE): the baseline really is ONE tabpage --------
    //
    // The expectation stops at `showtabline=` on purpose: it pins the COUNT and
    // leaves the option's value to assertion 5. The trailing text is what keeps
    // `tabs=1` from also matching a `tabs=10`.
    let want_one_tab = format!("{TABLINE_PROBE_ONE} tabs={ONE_TABPAGE} showtabline=");
    assert!(
        probe_one.contains(&want_one_tab),
        "the captive nvim answered {probe_one:?}, which does not carry {want_one_tab:?}. This test \
         compares a row count taken on ONE tabpage with one taken on two, so a core that started \
         on more than one makes the comparison meaningless: the baseline would ALREADY be a \
         tabline-reduced reading and assertion 4 would pass with the tabline on. Read this red as \
         \"nothing below is about `showtabline`\", not as a `showtabline` failure."
    );

    // Read BOTH row counts after the probe's round trip, so this reading and the one
    // after the second tabpage are taken at the same phase: a cmdline round trip has
    // completed and nvim has flushed.
    let (global_rows_before, window_rows_before) = grid_rows(&core, w1);

    let t2 = core.create_tabpage(None);
    assert_ne!(
        t2, 0,
        "create_tabpage should return a nonzero new tabpage handle"
    );
    poll_until(
        &waiter,
        "current_tabpage to become T2 with its own (fresh) window present",
        || {
            let snap = core.read_snapshot();
            snap.current_tabpage == t2
                && !snap.windows.is_empty()
                && !window_handles(&snap).contains(&w1)
        },
    );

    // Probe TWO, the same ordering device on the far side: a full keystroke -> Ex
    // execution -> `msg_show` -> flush round trip, so by the time it returns nvim has
    // finished whatever redraw the new tabpage provoked. Reading the row counts before
    // it would race the resize a tabline causes, and a race that usually loses would
    // make assertion 4 pass most of the time.
    let probe_two = tabline_probe(&core, &waiter, TABLINE_PROBE_TWO);

    let w2 = only_window_handle(&core);
    let (global_rows_after, window_rows_after) = grid_rows(&core, w2);

    // --- Assertion 2 (PREMISE): a second tabpage really exists ------------
    let want_two_tabs = format!("{TABLINE_PROBE_TWO} tabs={TWO_TABPAGES} showtabline=");
    assert!(
        probe_two.contains(&want_two_tabs),
        "the captive nvim answered {probe_two:?}, which does not carry {want_two_tabs:?} (probe \
         ONE answered {probe_one:?}). `create_tabpage` returned the nonzero handle {t2} and the \
         poll above saw a fresh window, so a count other than {TWO_TABPAGES} says the tabpage went \
         away again. The artifact below appears only AT two tabpages, so assertion 4 would be \
         comparing two one-tabpage readings and could not go red at all — read this as \"assertion \
         4 cannot be trusted this run\"."
    );

    // --- Assertion 3 (PREMISE, no revert) ---------------------------------
    assert_eq!(
        global_rows_after, global_rows_before,
        "the global grid {GLOBAL_GRID} went from {global_rows_before} rows to {global_rows_after} \
         across the creation of a second tabpage. This is a PREMISE and not a feature claim: nvim \
         sizes grid 1 from the UI attach and draws a tabline INSIDE it, taking the row from the \
         WINDOW grids instead — which is why assertion 4 below looks at those. A change here means \
         that model no longer holds, so read this red as `assertion 4 cannot be trusted this run` \
         rather than as a `showtabline` failure."
    );

    // --- Assertion 4 (THE DISCRIMINATOR): the artifact itself -------------
    //
    // Ordered ahead of the option read below, and that is the point of the ordering:
    // this is the row the front-end pays for, so this is the red a future revert must
    // show.
    assert_eq!(
        window_rows_after, window_rows_before,
        "the editor's window grid went from {window_rows_before} rows on the first tabpage to \
         {window_rows_after} on the second. The probes answered {probe_one:?} and {probe_two:?}, \
         which is where the cause is legible. A DROP of exactly one row is M8's measured artifact: \
         Neovim's default `showtabline = 1` turned its own tabline on the moment a second tabpage \
         existed, and since neither front-end paints grid 1 that line is invisible while still \
         costing the editor a real row and leaving every `WinInfo` `start_row` stale until the \
         front-end's next resize — a `showtabline=1` in either probe confirms it. The fix is \
         `opt.showtabline = 0` in `runtime/nvim/init.lua`; never a wider comparison here, and \
         never a front-end resize that papers over the row. A row count that GREW is a different \
         finding: nothing in this test asks nvim for a resize, so read that as the attach geometry \
         having moved under the second tabpage rather than as a tabline."
    );

    // --- Assertion 5 (premise-guard, deliberately LAST) -------------------
    //
    // Under the one revert that exists this is never reached, because assertion 4
    // reddens first — recorded in the doc table rather than papered over. What it
    // catches on its own is the option back on with the rows NOT moving, which would
    // mean assertion 4 had stopped being the artifact's detector.
    let want_off = format!("{TABLINE_PROBE_TWO} tabs={TWO_TABPAGES} showtabline={SHOWTABLINE_OFF}");
    assert!(
        probe_two.contains(&want_off),
        "with a SECOND tabpage live the captive nvim answered {probe_two:?}, which does not carry \
         {want_off:?}. Assertion 4 is GREEN, so the editor's window grid did NOT lose a row — and \
         that pairing is the surprising part, because M8 measured Neovim's default \
         `showtabline = 1` costing the window grid exactly one row the instant a second tabpage \
         exists. Together the two readings say the tabline is drawn AND is being paid for \
         somewhere this test does not look, so assertion 4 can no longer be trusted as its \
         detector. Restore `opt.showtabline = 0` in `runtime/nvim/init.lua` and re-measure where \
         the row now comes from; do NOT relax assertion 4 to match."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// 10. **The window-keyed buffer maps are GLOBAL across tabpages**, while
///     `Snapshot.windows` is per-CURRENT-tabpage. Open `a.md` on T1, `b.md` on a
///     fresh T2, and read one snapshot from each side of a switch.
///
/// ## What this measures, and what rests on it
///
/// `Snapshot.win_buffers` and `Snapshot.buffers` are both keyed by the nvim WINDOW
/// handle (`WinInfo.win`), and both are filled from ONE `wolf_buffers` payload whose
/// producer — `wolf_setup_buffer_tracking`'s `emit_now` in `runtime/nvim/init.lua` —
/// sweeps `nvim_list_wins()`. That call is global across tabpages, so the maps
/// describe every window of the whole INSTANCE rather than the current tabpage's.
/// `Snapshot.windows` is the opposite: nvim emits `win_hide` for the leaving
/// tabpage's windows and the redraw fold REMOVES them (test 7 pins exactly that), so
/// it holds the current tabpage alone.
///
/// That is a PREMISE about Neovim plus the captive `init.lua`, not a property of any
/// Rust type, which is why it is measured against a real nvim rather than reasoned
/// about. Per-tab persistence resolves each pane's saved content by looking that
/// pane's own window handle up in these maps, INCLUDING for a tab the user is not
/// looking at. If that lookup cannot answer, a background tab's leaf path is
/// reachable only by an RPC into another tabpage, and the design has to fall back to
/// caching each tab's last-known-good subtree — correct until a `:tabdo`, a plugin or
/// a buffer switch changes a background tab's files, after which it silently persists
/// an answer that was true minutes ago.
///
/// `crates/wolf-core/src/redraw.rs`'s `win_close` comment asserts the OPPOSITE ("a
/// window in a non-current tabpage is absent from both maps by construction"). One of
/// the two is wrong; this test is what says which.
///
/// ## Assertions, classified
///
/// | # | assertion | kind | falsifying edit |
/// | --- | --- | --- | --- |
/// | 1 | `win_buffers[w1]` names the T1 fixture while T2 is current | **discriminator** — a BACKGROUND tabpage's window is in the map | `nvim_tabpage_list_wins(0)` for `nvim_list_wins()` in `emit_now` |
/// | 2 | `win_buffers[w2]` names the T2 fixture | premise — a map FROZEN after T1's open would satisfy assertion 1 while describing nothing live | none |
/// | 3 | `buffers[w1]` is the canonical T1 fixture path | the legacy map, which `pane_status` joins through, is keyed and scoped identically | the same edit; assertion 1 reddens first |
/// | 4 | `windows` holds `w2` ALONE | **control** — without it assertions 1-3 are green on a build where nothing is tabpage-scoped, including one where the switch never happened | none |
/// | 5 | `win_buffers`' key set is exactly `{w1, w2}` | the "every window of the instance" half, and the no-accumulation half | none for a MISSING key (assertion 1 owns it); an emit that MERGED rather than wholesale-replaced, for an EXTRA one |
/// | 6-8 | the same three, mirrored after switching back to T1 | assertion 6 is the discriminator in the direction the gather reads most often (`w2` is the background window) | the same edit as assertion 1 |
///
/// Assertion ORDER is deliberate: the discriminator is stated before its premise and
/// its control, because the one falsifying edit that exists reddens assertion 1 and
/// leaves 2, 4 and 5 untouched — so the red a future revert produces is the finding
/// itself, carrying the map's whole key set in its message. This follows the ordering
/// argument recorded on test 9.
///
/// ## Fixtures
///
/// The two files are necessarily named after what is asserted, because the assertion
/// IS a path. What makes each one discriminating is therefore the KEY and not the
/// name: every lookup goes through a specific window handle, never a search of the
/// map for a filename, so a build that answered the T1 fixture for the wrong window
/// fails here.
#[test]
fn per_window_buffers_are_global_across_tabpages() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let t1 = attach_and_first_tabpage(&core, &waiter);
    let w1 = only_window_handle(&core);

    let dir = tempfile::tempdir().expect("failed to create temp dir for the two fixtures");
    let a_md = dir.path().join(T1_FILE);
    let b_md = dir.path().join(T2_FILE);
    std::fs::write(&a_md, "# first\n").expect("write the T1 fixture");
    std::fs::write(&b_md, "# second\n").expect("write the T2 fixture");
    let canon_a = canon(&a_md);

    // --- T1: open the first fixture into w1, the startup window -----------
    core.send_input(InputEvent::Open { path: a_md.clone() });
    poll_until(
        &waiter,
        "win_buffers[w1] to report the T1 fixture while T1 is still current",
        || win_buffer_tail(&core.read_snapshot(), w1).as_deref() == Some(T1_FILE),
    );

    // --- T2: a fresh tabpage, its own window, its own fixture -------------
    let t2 = core.create_tabpage(None);
    assert_ne!(t2, 0, "create_tabpage should return a nonzero handle");
    poll_until(
        &waiter,
        "current_tabpage to become T2 with its own (fresh) window present",
        || {
            let snap = core.read_snapshot();
            snap.current_tabpage == t2
                && !snap.windows.is_empty()
                && !window_handles(&snap).contains(&w1)
        },
    );
    let w2 = only_window_handle(&core);
    assert_ne!(
        w2, w1,
        "T2's window handle must differ from T1's, or every lookup below is ambiguous"
    );

    core.send_input(InputEvent::Open { path: b_md.clone() });
    poll_until(
        &waiter,
        "win_buffers[w2] to report the T2 fixture while T2 is current",
        || win_buffer_tail(&core.read_snapshot(), w2).as_deref() == Some(T2_FILE),
    );

    // Everything below comes from ONE snapshot read, so the two maps' disagreement
    // about scope describes a single instant rather than two.
    let snap = core.read_snapshot();
    let keys: HashSet<i64> = snap.win_buffers.keys().copied().collect();
    let tail_1 = win_buffer_tail(&snap, w1);
    let tail_2 = win_buffer_tail(&snap, w2);
    let live = window_handles(&snap);
    let legacy_1 = snap.buffers.get(&w1).cloned();
    drop(snap);

    // --- Assertion 1 (THE DISCRIMINATOR) ----------------------------------
    assert_eq!(
        tail_1.as_deref(),
        Some(T1_FILE),
        "Snapshot.win_buffers answered {tail_1:?} for T1's window {w1} while T2 ({w2}) is the \
         current tabpage; expected {T1_FILE:?}. The map's live keys are {keys:?}. The window-keyed \
         buffer maps are therefore NOT global across tabpages — they describe the current tabpage \
         alone, exactly like Snapshot.windows. The consequences, in order: a background tab's leaf \
         buffer path is not resolvable from a snapshot at all, so it cannot be resolved without an \
         RPC into another tabpage; per-tab persistence must fall back to caching each tab's \
         last-known-good subtree, which goes stale the moment a `:tabdo`, a plugin or a buffer \
         switch changes a background tab's files, with nothing able to tell the user; and \
         `crates/wolf-core/src/redraw.rs`'s `win_close` comment (\"a window in a non-current \
         tabpage is absent from both maps by construction\") turns out to be the accurate one. \
         This is a MEASUREMENT of the pinned Neovim plus `runtime/nvim/init.lua`, whose \
         `wolf_setup_buffer_tracking` sweeps `nvim_list_wins()` (global) rather than \
         `nvim_tabpage_list_wins(0)` (current tabpage) — so never relax this assertion to match a \
         build. If it is red, STOP and report."
    );

    // --- Assertion 2 (PREMISE): the map tracks the LIVE window set --------
    assert_eq!(
        tail_2.as_deref(),
        Some(T2_FILE),
        "Snapshot.win_buffers answered {tail_2:?} for T2's window {w2}, the window on the CURRENT \
         tabpage; expected {T2_FILE:?} (live keys {keys:?}). This is a premise for the assertion \
         above rather than a claim about tabpages: a `win_buffers` that stopped being replaced \
         after T1's open — frozen, or swept once and never again — would still carry {T1_FILE:?} \
         for {w1} and satisfy assertion 1 while describing nothing live at all. Read this red as \
         \"assertion 1 cannot be trusted this run\", and look at the wholesale replacement in \
         `crates/wolf-core/src/render.rs` before looking at tabpages."
    );

    // --- Assertion 3: the legacy map is scoped identically ----------------
    assert_eq!(
        legacy_1.as_deref(),
        Some(canon_a.as_path()),
        "Snapshot.buffers answered {legacy_1:?} for T1's window {w1} while T2 is current; expected \
         the canonical {canon_a:?}. `buffers` is the map `WolfCore::pane_status` joins a grid \
         through, and it is filled from the SAME `wolf_buffers` payload as `win_buffers` — whose \
         entry for {w1} assertion 1 already found — so a failure here is about the fold in \
         `crates/wolf-core/src/render.rs` splitting one payload into two maps, not about \
         `init.lua`'s sweep. A DIFFERENT path, rather than an absent entry, means T1's window is \
         showing something other than the fixture this test opened into it, in which case nothing \
         above is about tabpages either."
    );

    // --- Assertion 4 (THE CONTROL) ----------------------------------------
    let want_live: HashSet<i64> = [w2].into_iter().collect();
    assert_eq!(
        live, want_live,
        "Snapshot.windows holds {live:?} while T2 is current; expected T2's window {w2} ALONE, and \
         in particular NOT T1's {w1}. This is the CONTROL for the three assertions above and not a \
         claim of its own: `windows` is per-current-tabpage because nvim emits `win_hide` for the \
         leaving tabpage's windows and the redraw fold removes them (pinned by \
         `win_hide_removes_window_but_keeps_grid_on_tabpage_switch`). Its holding {w2} alone is \
         what makes \"win_buffers still holds {w1}\" a statement about tabpage SCOPE. If `windows` \
         also carried {w1}, every assertion above would be green on a build where nothing is \
         tabpage-scoped at all — including one where the switch to T2 never actually took effect. \
         Read this red as \"assertions 1-3 cannot be trusted this run\"."
    );

    // --- Assertion 5: every window of the INSTANCE, and no more -----------
    let want_keys: HashSet<i64> = [w1, w2].into_iter().collect();
    assert_eq!(
        keys, want_keys,
        "Snapshot.win_buffers holds the keys {keys:?}; expected exactly T1's window {w1} and T2's \
         {w2}, i.e. {TWO_WINDOWS_ACROSS_TWO_TABPAGES} entries for the \
         {TWO_WINDOWS_ACROSS_TWO_TABPAGES} windows this nvim has, one per tabpage. The two \
         directions read differently. A key MISSING is the tabpage-scoped sweep assertion 1 names, \
         and reddens there first. An EXTRA key is what this assertion exists for: the map is \
         wholesale-replaced per emit, so a key that is neither of these two means either the emit \
         is ACCUMULATING across sweeps — in which case a background tab's path stays readable but \
         goes STALE, the precise property the design chooses these maps over a per-tab cache to \
         avoid — or this nvim holds a window that neither this test nor `init.lua`'s preview-skip \
         accounts for."
    );

    // --- The mirror: switch back to T1, so w2 is the BACKGROUND window ----
    core.set_current_tabpage(t1);
    poll_until(
        &waiter,
        "current_tabpage to return to T1 with T2's window hidden",
        || {
            let snap = core.read_snapshot();
            snap.current_tabpage == t1 && !window_handles(&snap).contains(&w2)
        },
    );

    let snap = core.read_snapshot();
    let keys_back: HashSet<i64> = snap.win_buffers.keys().copied().collect();
    let tail_1_back = win_buffer_tail(&snap, w1);
    let tail_2_back = win_buffer_tail(&snap, w2);
    let live_back = window_handles(&snap);
    drop(snap);

    // --- Assertion 6 (THE MIRROR DISCRIMINATOR) ---------------------------
    assert_eq!(
        tail_2_back.as_deref(),
        Some(T2_FILE),
        "after switching back to T1, Snapshot.win_buffers answered {tail_2_back:?} for T2's window \
         {w2} — now a BACKGROUND tabpage's window — where {T2_FILE:?} was expected (live keys \
         {keys_back:?}). This is assertion 1 in the direction per-tab persistence reads most \
         often: the tab the user has just left, gathered while another tab is on screen. A red \
         here with assertion 1 GREEN is the more interesting pairing of the two, because it says \
         the map is global only for windows swept BEFORE the switch — an entry survives being \
         backgrounded but a re-sweep drops it, so what a gather answers would depend on when it \
         ran. Either way the fallback is the same: a per-tab last-known-good cache, stale under \
         `:tabdo`."
    );

    // --- Assertion 7 (PREMISE): the newly-current window still reports ----
    assert_eq!(
        tail_1_back.as_deref(),
        Some(T1_FILE),
        "after switching back to T1, Snapshot.win_buffers answered {tail_1_back:?} for T1's own \
         window {w1}, which is CURRENT again; expected {T1_FILE:?} (live keys {keys_back:?}). Like \
         assertion 2 this is a liveness premise rather than a tabpage claim — a map frozen at the \
         instant of the switch would satisfy assertion 6 by accident — so read this red as \
         \"assertion 6 cannot be trusted this run\"."
    );

    // --- Assertion 8 (THE MIRROR CONTROL) ---------------------------------
    let want_live_back: HashSet<i64> = [w1].into_iter().collect();
    assert_eq!(
        live_back, want_live_back,
        "after switching back to T1, Snapshot.windows holds {live_back:?}; expected T1's window \
         {w1} ALONE, and in particular NOT T2's {w2}. The mirror of assertion 4, with the same \
         reading: it is what makes assertion 6 a statement about scope rather than about a switch \
         that never took effect. Read this red as \"assertion 6 cannot be trusted this run\"."
    );

    core.shutdown().expect("shutdown should be Ok");
    drop(dir);
}
