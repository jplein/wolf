//! Integration tests for the `runtime_user/` USER OVERRIDE directory (plan
//! §4.1 seed + §4.4 seed call + §4.5 end-of-file dofile): a fresh
//! `<state_dir()>/runtime_user` is seeded once with the `init_user.lua`
//! template; a pre-existing `init_user.lua` is sourced LAST (after every Wolf
//! default) and never rewritten; a non-empty dir is never touched; and an
//! erroring user file degrades to a `vim.notify` warning without breaking
//! startup. Drives a REAL, hermetic nvim (via `WOLF_NVIM_BIN`) — no mocks.
//! The headless seed-function contract is covered in `state_unit.rs`; this
//! binary covers the same contract observed THROUGH a full core start.
//!
//! State-dir strategy: every test needs its own fresh, inspectable state dir,
//! so each holds the file-local `ENV_LOCK` for its WHOLE body and `set_var`s
//! `WOLF_STATE_DIR` to its own tempdir. No test ever `remove_var`s: the guards
//! live for the body, and removal would race the harness's set-if-absent
//! hermetic default.

mod common;

use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::WolfCore;

/// Serializes the tests in this binary: every one mutates the process-global
/// `WOLF_STATE_DIR`, and cargo runs them concurrently within one process.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// The greppable sentence the seeded template must carry, re-typed here
/// deliberately so the test states its expectation independently of the source
/// constant.
const TEMPLATE_SENTENCE: &str =
    "-- Wolf loads this file last, so anything here overrides Wolf's defaults.";

/// Gap between snapshot polls.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Polls `pred` until `true` (draining wakes each iteration) or panics after
/// [`FLUSH_TIMEOUT`]. `desc` names the awaited condition.
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

/// Flattens the current toast into one newline-joined string (each line is its
/// chunk texts concatenated). Mirrors `messages.rs`'s `toast_text`.
fn toast_text(core: &WolfCore) -> String {
    let snap = core.read_snapshot();
    snap.toast
        .lines()
        .iter()
        .map(|line| line.iter().map(|c| c.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Creates a fresh state tempdir and points `WOLF_STATE_DIR` at it. The caller
/// must hold [`ENV_LOCK`] and keep the returned guard alive for the test body.
fn fresh_state_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", dir.path());
    dir
}

/// A fresh state dir is seeded with the template, and the core comes up
/// normally with the seeded template in place (the template must be
/// comment-only, so sourcing it can break nothing).
#[test]
fn empty_state_dir_is_seeded_and_template_sourced() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");
    let state = fresh_state_dir();

    let (core, _waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let user_file = state.path().join("runtime_user").join("init_user.lua");
    assert!(
        user_file.is_file(),
        "WolfCore::start must seed <state_dir>/runtime_user/init_user.lua into a fresh \
         state dir (a missing file means the seed_runtime_user_dir call in api.rs is gone)"
    );
    let text = std::fs::read_to_string(&user_file).expect("read seeded template");
    assert!(
        text.contains(TEMPLATE_SENTENCE),
        "the seeded template must carry the greppable override sentence \
         {TEMPLATE_SENTENCE:?}; got:\n{text}"
    );

    // The core is healthy after sourcing the seeded template: a probe
    // round-trips through the live nvim.
    let probe_dir = tempfile::tempdir().expect("create probe dir");
    let vals = common::probe_values(&core, &["1 + 1"], &probe_dir.path().join("alive.txt"));
    assert_eq!(
        vals[0], "2",
        "nvim must stay responsive after sourcing the seeded template"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A pre-existing `init_user.lua` runs, runs LAST, and is never rewritten.
/// `linebreak` is the position oracle: init.lua sets it `true` in its wrap
/// block, so reading `0` back proves the user file executed AND executed after
/// that default — a block moved above the wrap settings flips this to `1`.
#[test]
fn user_file_is_sourced_last_and_overrides_defaults() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");
    let state = fresh_state_dir();
    let user_dir = state.path().join("runtime_user");
    std::fs::create_dir_all(&user_dir).expect("create runtime_user dir");
    let user_file = user_dir.join("init_user.lua");
    let contents = "vim.g.wolf_user_marker = \"loaded\"\nvim.o.linebreak = false\n";
    std::fs::write(&user_file, contents).expect("write user init_user.lua");

    let (core, _waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let probe_dir = tempfile::tempdir().expect("create probe dir");
    let vals = common::probe_values(
        &core,
        &[
            "get(g:, 'wolf_user_marker', 'MISSING')",
            "string(&linebreak)",
        ],
        &probe_dir.path().join("override.txt"),
    );
    assert_eq!(
        vals[0], "loaded",
        "a pre-existing init_user.lua must be dofile'd at startup (MISSING means the \
         end-of-file load block in init.lua is gone)"
    );
    assert_eq!(
        vals[1], "0",
        "the user file must run AFTER init.lua's `opt.linebreak = true` default — a 1 \
         here means the load point moved above Wolf's defaults (or never ran), so user \
         overrides lose to them"
    );

    // One-time-seed contract, observed through a full start: the user's file is
    // never reseeded or rewritten.
    assert_eq!(
        std::fs::read_to_string(&user_file).expect("re-read user init_user.lua"),
        contents,
        "a pre-existing init_user.lua must be byte-identical after startup"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A non-empty dir WITHOUT `init_user.lua` is left entirely alone: once the
/// user has put anything in the dir it is theirs, and seeding on mere template
/// absence would resurrect a template the user may have deleted on purpose.
#[test]
fn non_empty_dir_without_init_user_is_untouched() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");
    let state = fresh_state_dir();
    let user_dir = state.path().join("runtime_user");
    std::fs::create_dir_all(&user_dir).expect("create runtime_user dir");
    let notes = user_dir.join("notes.txt");
    let notes_body = "user notes, hands off\n";
    std::fs::write(&notes, notes_body).expect("write user notes.txt");

    let (core, _waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    assert!(
        !user_dir.join("init_user.lua").exists(),
        "a NON-EMPTY runtime_user dir must never be seeded, even when init_user.lua is \
         absent (seeding on absence would rewrite a dir the user has claimed)"
    );
    assert_eq!(
        std::fs::read_to_string(&notes).expect("re-read notes.txt"),
        notes_body,
        "the user's file must be byte-identical after startup"
    );

    // The core still works with no user init file to source.
    let probe_dir = tempfile::tempdir().expect("create probe dir");
    let vals = common::probe_values(&core, &["1 + 1"], &probe_dir.path().join("alive.txt"));
    assert_eq!(
        vals[0], "2",
        "nvim must start normally with an unseeded, init_user-less runtime_user dir"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// An erroring user file must not break startup (the `pcall` half) and must
/// surface a warning naming the file (the `vim.notify` half). The warning fires
/// before the UI attaches, so it is only observable through `:messages`
/// history, folded into `Snapshot.toast`.
#[test]
fn erroring_user_file_warns_but_startup_succeeds() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");
    let state = fresh_state_dir();
    let user_dir = state.path().join("runtime_user");
    std::fs::create_dir_all(&user_dir).expect("create runtime_user dir");
    std::fs::write(
        user_dir.join("init_user.lua"),
        "error(\"wolf-test-boom\")\n",
    )
    .expect("write erroring init_user.lua");

    // Startup completing at all (initial flush + open) is itself the pcall
    // assertion: without the pcall, init.lua raises and the attach never lands.
    let (core, waiter, _file) = common::start_core_with_temp_md("# Boom Fixture\nbody\n");

    // Defaults survived: the WolfSpell autocmd, installed long before the user
    // file loads, still fires for the opened markdown buffer.
    let probe_dir = tempfile::tempdir().expect("create probe dir");
    let vals = common::probe_values(
        &core,
        &["string(&l:spell)"],
        &probe_dir.path().join("spell.txt"),
    );
    assert_eq!(
        vals[0], "1",
        "startup must complete with every Wolf default intact despite the erroring user \
         file (spell off means earlier init.lua setup was lost to the error)"
    );

    // Warning half: the probe command above was itself a toast-dismissing input, so a match here
    // proves :messages repopulated the toast from the startup vim.notify(WARN) history.
    common::send_ex(&core, "messages");
    poll_until(
        &waiter,
        "the init_user.lua warning to surface via :messages (its absence means the \
         vim.notify branch of the dofile error handler is gone, so user-file errors \
         become silent)",
        || toast_text(&core).contains("init_user.lua"),
    );

    core.shutdown().expect("shutdown should be Ok");
}
