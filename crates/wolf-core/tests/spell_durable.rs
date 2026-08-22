//! Integration tests for the DURABLE spell dictionary (plan §5): a `zg`-added
//! word lands in `<state_dir()>/spell/en.utf-8.add` and survives a full core
//! restart, and a state dir whose path contains a literal comma still gets a
//! working swap `'directory'` AND `'spellfile'` — pinning init.lua's
//! comma-escaping, because both options are comma-separated lists that an
//! unescaped comma silently splits into two nonexistent paths. Drives a REAL,
//! hermetic nvim (via `WOLF_NVIM_BIN`) — no mocks.
//!
//! State-dir strategy: each test must inspect physical files under ITS OWN
//! state dir, so each holds the file-local `ENV_LOCK` for its WHOLE body and
//! `set_var`s `WOLF_STATE_DIR` to a fresh tempdir. No test ever `remove_var`s:
//! the tempdir guards live for the body, and removal would race the harness's
//! set-if-absent hermetic default.
//!
//! The comma-escaping coverage lives HERE rather than in `swap_recovery.rs`
//! deliberately: it needs this exclusive `WOLF_STATE_DIR` mutation, which that
//! binary's shared-hermetic tests must never see.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, WolfCore};

/// Serializes the tests in this binary: every one mutates the process-global
/// `WOLF_STATE_DIR`, and cargo runs them concurrently within one process.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Gap between polls.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Polls `pred` until `true` (draining wakes each iteration) or panics after
/// [`FLUSH_TIMEOUT`]. `desc` names the awaited condition. Mirrors
/// `swap_recovery.rs`'s `poll_until`.
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

/// Starts a `WolfCore` rooted at `cwd` and waits for the initial attach flush.
/// Both cores in the restart test go through here with the SAME env, so they
/// resolve the SAME durable spell dir.
fn start_core_in(cwd: &Path) -> (WolfCore, FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(cwd.to_path_buf(), common::DEFAULT_COLS, common::DEFAULT_ROWS),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}

/// Opens `fixture` through the core and waits for the buffer to render.
fn open_fixture(core: &WolfCore, waiter: &FlushWaiter, fixture: &Path) {
    core.send_input(InputEvent::Open {
        path: fixture.to_path_buf(),
    });
    waiter.wait_for_flush(core, FLUSH_TIMEOUT);
}

/// An owned, canonicalized temp dir to use as a core cwd (macOS `/tmp` ->
/// `/private/tmp`, so nvim's own view of paths lines up). Returned with its
/// guard.
fn owned_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// A `zg`'d word must be readable by a SECOND core over the same state dir:
/// the durable dictionary is the whole point of `WOLF_SPELL_DIR`. A word that
/// reads back bad after the restart means the export (or the pre-launch
/// `create_dir_all` of the spell dir) is gone and words die with the process.
#[test]
fn zg_word_survives_core_restart() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");
    let state = tempfile::tempdir().expect("create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state.path());

    let (_cwd_guard, cwd) = owned_cwd();
    let fixture = cwd.join("words.md");
    // The made-up token is the FIRST word, so the cursor already sits on it at
    // open and a bare `zg` targets it.
    std::fs::write(&fixture, "zzqxwolf is a made-up word\n").expect("write fixture");

    let addfile = state.path().join("spell").join("en.utf-8.add");

    // First core: open the markdown fixture (the WolfSpell autocmd turns spell
    // on — without it, `zg` errors instead of recording) and add the word.
    let (core, waiter) = start_core_in(&cwd);
    open_fixture(&core, &waiter, &fixture);
    type_str(&core, "zg");
    poll_until(
        &waiter,
        "zg to append the word to the durable en.utf-8.add (a timeout means 'spellfile' \
         does not point into the durable spell dir, or spell never turned on)",
        || std::fs::read_to_string(&addfile).is_ok_and(|t| t.contains("zzqxwolf")),
    );
    core.shutdown().expect("first core shutdown should be Ok");

    // Second core, SAME env: the durable dictionary must load, so the word is
    // GOOD. The fixture is reopened so the probe window has spell state.
    let (core2, waiter2) = start_core_in(&cwd);
    open_fixture(&core2, &waiter2, &fixture);
    let probe_dir = tempfile::tempdir().expect("create probe dir");
    let vals = common::probe_values(
        &core2,
        &["&spellfile", "join(spellbadword('zzqxwolf'), '|')"],
        &probe_dir.path().join("restart.txt"),
    );
    assert_eq!(
        vals[0],
        addfile.to_string_lossy(),
        "the second core's 'spellfile' must be the SAME durable path the first core wrote \
         (a mismatch means the WOLF_SPELL_DIR export changed between spawns)"
    );
    assert_eq!(
        vals[1], "|",
        "spellbadword must report the zg'd word GOOD after a restart (two empty items \
         joined). `zzqxwolf|bad` means the durable dictionary was not loaded, so the \
         first session's zg was lost"
    );
    core2.shutdown().expect("second core shutdown should be Ok");
}

/// A state dir with a literal comma in its path: `'directory'` and
/// `'spellfile'` are comma-separated option lists, so WITHOUT init.lua's
/// escaping the value silently splits in two and both the swap file and the
/// `zg` write land nowhere. Each half pins one gsub site.
#[test]
fn comma_state_dir_swap_and_spellfile_escaped() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");
    let root = tempfile::tempdir().expect("create temp dir");
    // The comma in the leaf is the whole fixture.
    let state = root.path().join("wolf,state");
    std::fs::create_dir_all(&state).expect("create comma'd state dir");
    std::env::set_var("WOLF_STATE_DIR", &state);

    let (_cwd_guard, cwd) = owned_cwd();
    let fixture = cwd.join("probe.md");
    std::fs::write(&fixture, "zzcommaqx body words\n").expect("write fixture");

    let (core, waiter) = start_core_in(&cwd);
    open_fixture(&core, &waiter, &fixture);

    // Spell half: `zg` the first word; the write landing under the comma'd
    // state dir pins the 'spellfile' escape.
    type_str(&core, "zg");
    let addfile = state.join("spell").join("en.utf-8.add");
    poll_until(
        &waiter,
        "zg to append into the comma'd spell dir (an unescaped comma in 'spellfile' \
         splits the path in two and the write lands nowhere)",
        || std::fs::read_to_string(&addfile).is_ok_and(|t| t.contains("zzcommaqx")),
    );

    // Swap half: an unsaved edit plus `:preserve` must produce a swap under
    // swap_dir_for(cwd), which lives under the comma'd state dir — pinning the
    // 'directory' escape.
    core.send_input(ch('i'));
    type_str(&core, "WOLFEDIT ");
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);
    common::send_ex(&core, "preserve");

    let swap_dir = wolf_core::state::swap_dir_for(&cwd)
        .expect("swap_dir_for should resolve under the comma'd WOLF_STATE_DIR");
    poll_until(
        &waiter,
        "a .swp entry in the comma'd durable swap dir (an unescaped comma in 'directory' \
         splits the path in two and the swap silently lands elsewhere, degrading crash \
         recovery)",
        || {
            std::fs::read_dir(&swap_dir)
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .any(|e| e.path().extension().is_some_and(|x| x == "swp"))
        },
    );

    core.shutdown().expect("shutdown should be Ok");
}
