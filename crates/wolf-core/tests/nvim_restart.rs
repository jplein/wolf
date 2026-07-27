//! Integration test for RESTART + Tier-1 RESTORE (architect plan §9 case b): when
//! the captive Neovim dies, `WolfCore::restart_nvim()` respawns it in place, and
//! the front-end reopens the file-backed buffers that were open (the Tier-1
//! open-file mirror captured BEFORE the restart wiped it).
//!
//! Tier-1 now also includes named-but-unsaved `Untitled.md` buffers (the
//! anonymous-buffer feature names the startup buffer `Untitled.md`). Reopening such a
//! buffer via `:e` is exactly what drives Neovim's swap-based recovery of unsaved
//! edits, so it legitimately joins the crash-reopen set; excluding it would
//! permanently lose those edits.
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks.
//!
//! ## Dependency on coder work (tests-first)
//!
//! References NEW surface the coder adds; will not compile until it lands:
//!
//!   * `WolfCore::liveness(&self) -> wolf_core::Liveness`.
//!   * `WolfCore::restart_nvim(&self) -> Result<(), _>` — kill-if-needed, respawn
//!     in the same cwd, re-run the setup chain; post-`Ok` contract: `liveness()`
//!     is `Alive`, `current_tabpage()` is the fresh nvim's tabpage-1, and
//!     `list_buffers()` is empty.
//!   * `WolfCore::open_files_for_tabpage(handle: i64) -> Option<TabFiles>` where
//!     `TabFiles { active: Option<PathBuf>, files: Vec<PathBuf> }` — the
//!     out-of-band Tier-1 mirror (buflisted file set + active per tabpage).
//!
//! ## Conventions (mirroring `tabpages.rs`)
//!
//! The mirror + buffer list arrive across flushes / out-of-band notifications, so
//! we POLL with a generous timeout, draining wakes. macOS `/tmp`/`/var` are
//! symlinks, so every path comparison is canonicalized.

mod common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, temp_md, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, Liveness, WolfCore};

/// Gap between polls while waiting for an out-of-band update.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Distinctive fixture bodies so a wrong buffer cannot fake a pass.
const DOC_A: &str = "# Alpha fixture\n";
/// The second fixture's body.
const DOC_B: &str = "# Bravo fixture\n";

/// Polls `pred` until `true` (draining wakes each iteration) or panics after
/// [`FLUSH_TIMEOUT`]. Mirrors `tabpages.rs::poll_until`.
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

/// Types an Ex command `:<cmd><CR>` one key at a time.
fn ex_command(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// SIGKILLs the captive nvim's own pid via the input pump (a crash).
fn self_sigkill(core: &WolfCore) {
    ex_command(core, "lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)");
}

/// Canonicalizes `p`, falling back to the raw path when it cannot be resolved
/// (the mirror uses the same raw fallback), for symlink-robust comparison.
fn norm(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Waits for the initial editor window after attach, then returns the first
/// tabpage handle (mirrors `tabpages.rs::attach_and_first_tabpage`).
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

/// Whether `list_buffers()` currently contains a buffer whose (canonicalized) name
/// equals `want`.
fn listed_has(core: &WolfCore, want: &Path) -> bool {
    core.list_buffers()
        .iter()
        .any(|b| !b.name.is_empty() && norm(Path::new(&b.name)) == want)
}

/// A crashed nvim restarts in place and the front-end reopens the Tier-1 open-file
/// set captured before the restart.
#[test]
fn restart_respawns_and_reopens_open_files() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let tab = attach_and_first_tabpage(&core, &waiter);

    // Open two file-backed buffers in the current tabpage. `b` opens LAST, so it is
    // the active buffer.
    let file_a = temp_md(DOC_A);
    let file_b = temp_md(DOC_B);
    let canon_a = norm(file_a.path());
    let canon_b = norm(file_b.path());

    core.send_input(InputEvent::Open {
        path: file_a.path().to_path_buf(),
    });
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);
    core.send_input(InputEvent::Open {
        path: file_b.path().to_path_buf(),
    });
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // The mirror reflects BOTH file paths AND the active path (b) for this tabpage.
    poll_until(
        &waiter,
        "open_files_for_tabpage to mirror both files with active = b",
        || match core.open_files_for_tabpage(core.current_tabpage()) {
            Some(tf) => {
                let files: HashSet<PathBuf> = tf.files.iter().map(|p| norm(p)).collect();
                let active_is_b = tf.active.as_deref().map(norm) == Some(canon_b.clone());
                files.contains(&canon_a) && files.contains(&canon_b) && active_is_b
            }
            None => false,
        },
    );

    // Capture the Tier-1 set the front-end would grab BEFORE restart (respawn wipes
    // the mirror). Reopen order: non-active first, ACTIVE last (so it lands focused).
    let mirror = core
        .open_files_for_tabpage(tab)
        .expect("mirror must be present before the restart");
    let active = mirror.active.clone();
    let mut reopen: Vec<PathBuf> = mirror
        .files
        .iter()
        .filter(|p| active.as_deref() != Some(p.as_path()))
        .cloned()
        .collect();
    if let Some(a) = active {
        reopen.push(a);
    }
    // Three, not two: the two file fixtures PLUS the startup `Untitled.md`. The
    // never-saved `Untitled.md` is a legitimate Tier-1 member — it is carried across
    // the crash-restart so the reopen `:e` can drive Neovim's swap-based recovery of
    // any unsaved edits; excluding it would permanently lose them.
    assert_eq!(
        reopen.len(),
        3,
        "the captured Tier-1 set must contain the two open files PLUS the startup \
         named-but-unsaved Untitled.md, got {reopen:?}"
    );
    // Positive proof that the startup Untitled.md is in the mirror (matched by
    // basename — its path is `<cwd>/Untitled.md`, which is never written to disk).
    assert!(
        mirror
            .files
            .iter()
            .any(|p| p.file_name().and_then(|s| s.to_str()) == Some("Untitled.md")),
        "the Tier-1 mirror must include the named-but-unsaved startup Untitled.md, got {:?}",
        mirror.files
    );

    // Crash nvim and wait for the Dead transition.
    self_sigkill(&core);
    poll_until(&waiter, "liveness() to become Dead after the SIGKILL", || {
        core.liveness() == Liveness::Dead
    });

    // Restart from a DEAD state: an ACTUAL respawn, so `restart_nvim()` returns
    // `Ok(true)` (true = really respawned; false would mean a no-op). Post-restart
    // contract: Alive, a fresh (valid) current tabpage, and NONE of the
    // previously-open NAMED files still open (the fresh nvim carries only its
    // default pristine unnamed buffer — bufnr 1 — which is NOT filtered from
    // `list_buffers()`, so we assert absence of the named files rather than
    // emptiness).
    let restarted = core
        .restart_nvim()
        .expect("restart_nvim should be Ok from a Dead state");
    assert!(
        restarted,
        "restart_nvim() from a Dead state must report an ACTUAL respawn (Ok(true))"
    );
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "after restart the captive nvim must be Alive again"
    );
    let fresh_tab = core.current_tabpage();
    assert_ne!(
        fresh_tab, 0,
        "current_tabpage() must be a valid (nonzero) handle after restart"
    );
    assert!(
        !listed_has(&core, &canon_a) && !listed_has(&core, &canon_b),
        "a freshly restarted nvim must carry NONE of the previously-open named files, got {:?}",
        core.list_buffers()
    );

    // Drive the reopen the front-end performs off the captured set.
    core.set_current_tabpage(fresh_tab);
    for path in &reopen {
        core.send_input(InputEvent::Open { path: path.clone() });
    }
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Both files are open again in the restarted nvim.
    poll_until(
        &waiter,
        "both fixtures to be listed again after the reopen",
        || listed_has(&core, &canon_a) && listed_has(&core, &canon_b),
    );
    assert!(
        listed_has(&core, &canon_a) && listed_has(&core, &canon_b),
        "list_buffers() must contain BOTH reopened files after restart"
    );

    core.shutdown().expect("shutdown should be Ok");
    drop((file_a, file_b));
}

/// A restart request on a HEALTHY (Alive) core is intentionally a NO-OP: at the
/// core level `restart_nvim()` returns `Ok(false)` (false = no respawn happened)
/// without tearing down the live nvim, so the FFI `restart()` reports "nothing
/// changed" and the front-end does NOT redundantly rebind + reopen (which would
/// duplicate tabpages). The actual teardown+respawn path (`Ok(true)`) is covered
/// by `restart_respawns_and_reopens_open_files` from a Dead state. This pins the
/// no-op contract (plan §5 step 1 + §11 reentrancy guard).
#[test]
fn restart_on_live_core_is_a_noop_ok_false() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let _t1 = attach_and_first_tabpage(&core, &waiter);

    // A restart on an already-Alive core must be a no-op: Ok(false), NOT a respawn.
    let respawned = core
        .restart_nvim()
        .expect("restart_nvim on a live core should be Ok");
    assert!(
        !respawned,
        "restart_nvim() on an Alive core must be a NO-OP (Ok(false)), not a respawn"
    );
    assert_eq!(
        core.liveness(),
        Liveness::Alive,
        "a no-op restart must leave the live nvim Alive"
    );

    // A second request is likewise a harmless no-op (Ok(false)).
    let respawned_again = core
        .restart_nvim()
        .expect("a repeated restart on a live core should be Ok");
    assert!(
        !respawned_again,
        "a repeated restart_nvim() on an Alive core must also be a NO-OP (Ok(false))"
    );

    // The nvim was never disturbed: blocking RPCs answer normally (a valid current
    // tabpage; the buffer list query succeeds).
    assert_ne!(
        core.current_tabpage(),
        0,
        "current_tabpage() must still resolve after the no-op restart requests"
    );
    let _ = core.list_buffers();

    core.shutdown().expect("shutdown should be Ok");
}
