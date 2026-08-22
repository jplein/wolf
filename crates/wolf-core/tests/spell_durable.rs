//! Integration test for the DURABLE user spell dictionary: a word taught with
//! `zg` must survive the death of the nvim that learned it.
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks, per the project's real-nvim policy.
//!
//! The behavior under test spans three layers: `state::spell_dir()` (a GLOBAL
//! `state_dir()/spell`, deliberately not cwd-hashed), `NvimConfig`/`spawn`
//! (creates that dir and exports `WOLF_SPELL_DIR`), and `runtime/nvim/init.lua`
//! (points `'spellfile'` at `<WOLF_SPELL_DIR>/en.utf-8.add`). Only an end-to-end
//! run proves the chain: each layer in isolation can look right while the word
//! still lands in the ephemeral `XDG_DATA_HOME` that `spawn` pins per process,
//! which is exactly the bug this file guards.
//!
//! ## Why this is a SIBLING of `spell.rs` rather than another test in it
//!
//! 1. **Exactly ONE `#[test]` lives in this binary, and that is load-bearing.**
//!    The test sets `WOLF_STATE_DIR` process-wide before any core starts. The
//!    shared harness's `ensure_hermetic_state_dir` is set-if-absent, so a peer
//!    `#[test]` thread in the same binary racing it is a genuine data race over
//!    one process-global variable. A binary with a single test has no peer. Do
//!    NOT add a second `#[test]` here — add a new target instead.
//! 2. **The spell dictionary is GLOBAL**, so every test sharing a binary shares
//!    one word list. Isolation by target is the only isolation that holds.
//! 3. It starts two cores sequentially and should not slow the fast `spell`
//!    target.
//!
//! ## Never hand-write the `.add` file
//!
//! nvim reads the COMPILED `en.utf-8.add.spl` at load time, not the plain-text
//! `en.utf-8.add`. Appending to the `.add` from Rust is silently ignored (no
//! recompile, no warning), so a test that fabricated the dictionary would assert
//! nothing. This test therefore types the user's real gesture — `zg` — and lets
//! nvim produce both files.
//!
//! ## Why assertions go through a toast
//!
//! `WolfCore` exposes no generic `exec_lua`, so each probe echoes a Lua
//! expression through `nvim_echo`, which `ext_messages` routes into
//! `Snapshot.toast`. Same technique as `spell.rs`; the helpers below are
//! deliberately DUPLICATED from it rather than hoisted into `tests/common`,
//! which two consumers do not justify (it would recompile every test target).

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, WolfCore};

/// Gap between polls while waiting for a probe echo or an on-disk file.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Prefix of every probe echo. Each probe appends a MONOTONIC nonce and a
/// trailing `-`, so a probe can never match the residue of an earlier one — the
/// trailing `-` is what keeps `WOLFSPELL-1-` from prefix-matching `WOLFSPELL-10-`.
const PROBE: &str = "WOLFSPELL";

/// Source of the per-probe nonce.
static PROBE_SEQ: AtomicU32 = AtomicU32::new(0);

/// The word taught to nvim with `zg`.
///
/// Lowercase ASCII on purpose: unusual capitalization triggers separate
/// `spellbadword()` rules (`caps`/`rare`) and would muddy the assertions. The
/// literal must be nonsense the bundled `en` dictionary rejects — the "before"
/// assertion below is the resolver for that, so a poor choice fails loudly
/// instead of letting the test pass vacuously.
const BAD_WORD: &str = "zblornak";

/// Subdirectory of `state_dir()` that holds the durable user dictionary.
const SPELL_SUBDIR: &str = "spell";

/// The plain-text dictionary `zg` appends to, named from `'spelllang'` (`en`)
/// and `'encoding'` (`utf-8`).
const ADD_FILE: &str = "en.utf-8.add";

/// The COMPILED sibling `zg` regenerates in-process. This — not the `.add` — is
/// what a later nvim actually loads.
const SPL_FILE: &str = "en.utf-8.add.spl";

/// What the `spellbadword` probe echoes when the word is considered GOOD.
const NONE: &str = "<none>";

/// Name of the subdirectory of the test's tempdir that `WOLF_STATE_DIR` points
/// at. The comma and the space are DELIBERATE. Do not tidy this into a plain
/// name — doing so silently deletes the only coverage of the comma escaping.
///
/// `'spellfile'` is a COMMA-SEPARATED option: a raw comma makes nvim reject the
/// assignment and leave the option EMPTY, after which `zg` writes to the
/// runtimepath default instead. Every symptom visible from inside nvim then looks
/// like success, so the on-disk poll below is the only thing that catches it —
/// and only when the path actually contains a comma.
///
/// The space mirrors the real macOS state dir
/// (`~/Library/Application Support/Wolf`). Spaces need NO escaping through the
/// Lua option API, so an escape mistakenly added for them fails here loudly
/// rather than in a user's home directory.
const HOSTILE_STATE_SUBDIR: &str = "Application Support,Wolf";

/// Types `:{cmd}<CR>` as keystrokes. `cmd` must NOT include the leading `:`.
fn send_ex(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Flattens the current toast into one newline-joined string.
fn toast_text(core: &WolfCore) -> String {
    let snap = core.read_snapshot();
    snap.toast
        .lines()
        .iter()
        .map(|line| line.iter().map(|c| c.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Polls `pred` until true, draining wakes, or panics after [`FLUSH_TIMEOUT`].
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

/// Echoes `lua_expr` (a Lua expression evaluating to a string) through
/// `nvim_echo` behind a unique tag, and returns the echoed text.
///
/// The tag is what makes the read safe: `ext_messages` leaves the previous
/// message in `Snapshot.toast`, so polling for a FIXED marker matches the prior
/// probe's residue immediately and silently reports stale state. Every call
/// mints a fresh tag, so a match proves THIS echo landed.
fn echo(core: &WolfCore, waiter: &FlushWaiter, lua_expr: &str) -> String {
    let tag = format!("{PROBE}-{}-", PROBE_SEQ.fetch_add(1, Ordering::Relaxed));
    send_ex(
        core,
        &format!(r#"lua vim.api.nvim_echo({{{{"{tag} "..({lua_expr})}}}}, false, {{}})"#),
    );
    poll_until(
        waiter,
        &format!("the `{tag}` probe echo to reach the toast"),
        || toast_text(core).contains(&tag),
    );

    let text = toast_text(core);
    text.split(&tag)
        .nth(1)
        .unwrap_or_else(|| panic!("probe tag {tag} missing from toast {text:?}"))
        .trim()
        .to_string()
}

/// Echoes the ACTIVE window's `filetype` / `spell` / `spelllang`.
///
/// Asserted as a PRECONDITION in both sessions: `spellbadword()` is only
/// meaningful once a real `.spl` backs `spelllang`, so without this the
/// persistence assertions could pass for the wrong reason (no dictionary
/// loaded at all reports every word good).
fn probe(core: &WolfCore, waiter: &FlushWaiter) -> (String, bool, String) {
    let text = echo(
        core,
        waiter,
        r#"vim.bo.filetype.." "..tostring(vim.wo.spell).." "..vim.o.spelllang"#,
    );
    let fields: Vec<&str> = text.split_whitespace().take(3).collect();
    assert_eq!(
        fields.len(),
        3,
        "probe echo should carry filetype, spell and spelllang; got {text:?}"
    );
    (
        fields[0].to_string(),
        fields[1]
            .parse::<bool>()
            .unwrap_or_else(|_| panic!("spell should echo as a bool; got {text:?}")),
        fields[2].to_string(),
    )
}

/// Echoes `spellbadword(BAD_WORD)`'s first element, or [`NONE`] when nvim
/// considers the word good.
///
/// The emptiness test is NOT a stylistic choice. For a GOOD word
/// `vim.fn.spellbadword()` returns `{ "", "" }` — an empty STRING at index 1,
/// never `nil` — and an empty string is TRUTHY in Lua, so the tempting
/// `… [1] or "<none>"` form never falls back and hands every assertion a bare
/// `""` even when the feature works. The call is bound to a local so the
/// (dictionary-loading) lookup happens exactly once per probe.
fn spellbadword(core: &WolfCore, waiter: &FlushWaiter) -> String {
    echo(
        core,
        waiter,
        &format!(
            r#"(function() local bad = vim.fn.spellbadword("{BAD_WORD}")[1] if bad == "" then return "{NONE}" end return bad end)()"#
        ),
    )
}

/// Writes `name` into `dir` with `content` and returns its path.
fn write_fixture(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path)
        .unwrap_or_else(|e| panic!("failed to create fixture {name}: {e}"));
    f.write_all(content.as_bytes())
        .unwrap_or_else(|e| panic!("failed to write fixture {name}: {e}"));
    f.flush().expect("failed to flush fixture");
    path
}

/// Opens `path` in the running core and waits for the buffer to be shown.
fn open(core: &WolfCore, waiter: &FlushWaiter, path: PathBuf) {
    core.send_input(InputEvent::Open { path });
    waiter.wait_for_flush(core, FLUSH_TIMEOUT);
}

/// An owned, CANONICALIZED temp dir to use as a core cwd (macOS `/tmp` ->
/// `/private/tmp`), so nvim's view of paths lines up with the test's. Mirrors
/// `swap_recovery.rs`'s `owned_cwd`. Returned with its guard.
fn owned_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = std::fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// Starts a `WolfCore` rooted at `cwd` and waits for the initial attach flush.
fn start_core_in(cwd: &Path) -> (WolfCore, FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(
            cwd.to_path_buf(),
            common::DEFAULT_COLS,
            common::DEFAULT_ROWS,
        ),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, common::FLUSH_TIMEOUT);
    (core, waiter)
}

/// A word taught with `zg` in one session is still known to a LATER, separate
/// nvim started from a DIFFERENT working directory.
///
/// The different cwd is deliberate: it proves the dictionary is a property of
/// the language rather than of the open project, which is why `spell_dir()` does
/// not hash the cwd the way `swap_dir_for` does.
///
/// The same-session re-probe between the two halves is what makes a red run
/// diagnosable: if it passes and the second session fails, persistence broke; if
/// it fails, `zg` itself never took effect.
///
/// The state dir this test pins is named [`HOSTILE_STATE_SUBDIR`], so the spell
/// path carries a comma and a space. An unescaped comma empties `'spellfile'` and
/// sends `zg` to the runtimepath default, which looks like success from inside
/// nvim; that failure mode surfaces here as the on-disk poll timing out.
#[test]
fn zg_persists_the_user_dictionary_across_cores() {
    let _ = common::require_nvim_bin();

    // Pin state_dir() to a throwaway dir BEFORE anything touches the harness:
    // `config_in` installs a process-global hermetic value only if unset, so
    // ours wins and the durable dictionary lands somewhere we can inspect (and
    // never in the developer's real `~/Library/Application Support/Wolf`). The
    // guard is held for the whole test so the dir outlives both nvim children.
    //
    // The state dir is a SUBDIRECTORY named `HOSTILE_STATE_SUBDIR` rather than the
    // tempdir root: the name carries a comma and a space on purpose (see that
    // constant). Every expected artifact path below is derived from this same
    // string, never from `wolf_core::state::spell_dir()`, so the test cannot agree
    // with the implementation through a shared bug.
    let state = tempfile::tempdir().expect("failed to create temp state dir");
    let state_root = state.path().join(HOSTILE_STATE_SUBDIR);
    std::fs::create_dir_all(&state_root)
        .unwrap_or_else(|e| panic!("failed to create state dir {}: {e}", state_root.display()));
    std::env::set_var("WOLF_STATE_DIR", &state_root);
    let spell_dir = state_root.join(SPELL_SUBDIR);
    let add_path = spell_dir.join(ADD_FILE);
    let spl_path = spell_dir.join(SPL_FILE);

    // --- Session 1: teach the word ------------------------------------------

    let (_dir_a, cwd_a) = owned_cwd();
    // BAD_WORD is the FIRST line so `gg0` parks the cursor on its first char and
    // `zg` picks up exactly that word.
    let fixture_a = write_fixture(&cwd_a, "note.md", &format!("{BAD_WORD}\nsecond line\n"));

    let (core, waiter) = start_core_in(&cwd_a);
    open(&core, &waiter, fixture_a);

    let (ft, spell, lang) = probe(&core, &waiter);
    assert_eq!(ft, "markdown", "the fixture should open as markdown");
    assert!(spell, "markdown should have spell on");
    assert_eq!(
        lang, "en",
        "spelllang must be the one bundled .spl (en); the dictionary filename tracks it"
    );

    // BEFORE: the word must start out UNKNOWN. If this fails, BAD_WORD is a real
    // English word (or already in a dictionary) and the rest of the test would
    // prove nothing — pick a different literal.
    let before = spellbadword(&core, &waiter);
    assert_eq!(
        before, BAD_WORD,
        "precondition: `{BAD_WORD}` must be flagged as a bad word BEFORE `zg`; \
         got {before:?}, so the literal is already known to the dictionary and \
         the persistence assertions below would pass vacuously"
    );

    // The user's actual gesture: cursor to the word, then `zg`. Not `:spellgood`
    // and not `:normal! zg` — the keymap path is part of what is under test.
    type_str(&core, "gg0zg");

    // Keystrokes are applied asynchronously and the write is not marked by any
    // redraw flush, so poll for the artifact rather than sleeping.
    poll_until(
        &waiter,
        &format!(
            "`zg` to write `{BAD_WORD}` into the durable dictionary at {}",
            add_path.display()
        ),
        || {
            std::fs::read_to_string(&add_path)
                .map(|text| text.lines().any(|line| line.trim() == BAD_WORD))
                .unwrap_or(false)
        },
    );

    let add_text = std::fs::read_to_string(&add_path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", add_path.display()));
    assert!(
        add_text.lines().any(|line| line.trim() == BAD_WORD),
        "{} must contain `{BAD_WORD}` on a line of its own, got {add_text:?}",
        add_path.display()
    );

    // `zg` recompiles in-process; the compiled sibling is what a LATER nvim
    // actually loads, so its absence means the second session cannot pass.
    poll_until(
        &waiter,
        &format!("`zg` to compile the dictionary to {}", spl_path.display()),
        || spl_path.exists(),
    );
    assert!(
        spl_path.exists(),
        "`zg` must compile {ADD_FILE} to {SPL_FILE}; the compiled file — not the \
         plain-text one — is what nvim loads in a later session"
    );

    // SAME session: the word is good immediately. Separates "`zg` failed" from
    // "persistence failed" when the second session goes red.
    let after = spellbadword(&core, &waiter);
    assert_eq!(
        after, NONE,
        "after `zg`, `{BAD_WORD}` must be good in the SAME session; \
         got {after:?}, so `zg` itself did not take effect"
    );

    // `shutdown` takes `self`, so this both stops and DROPS the core: the nvim
    // child is definitively gone before the second session starts. That matters
    // because the point of the next half is that the SECOND nvim learns the word
    // from disk, not from a still-running peer. The waiter is dropped right after
    // so no stale wake channel outlives the session.
    core.shutdown().expect("shutdown should be Ok");
    drop(waiter);

    // --- Session 2: a fresh nvim in a DIFFERENT cwd --------------------------

    let (_dir_b, cwd_b) = owned_cwd();
    // Content is irrelevant beyond opening as markdown, so it deliberately does
    // NOT mention BAD_WORD — the dictionary, not the buffer, is under test.
    let fixture_b = write_fixture(&cwd_b, "other.md", "# Second session\n\nplain prose\n");

    let (core2, waiter2) = start_core_in(&cwd_b);
    open(&core2, &waiter2, fixture_b);

    let (ft2, spell2, lang2) = probe(&core2, &waiter2);
    assert_eq!(ft2, "markdown", "the second fixture should open as markdown");
    assert!(spell2, "markdown should have spell on in the second session");
    assert_eq!(lang2, "en", "spelllang must still be en in the second session");

    let persisted = spellbadword(&core2, &waiter2);
    assert_eq!(
        persisted, NONE,
        "`{BAD_WORD}` was taught with `zg` in a previous nvim started from a \
         DIFFERENT directory ({}), so it must still be good here ({}): the user \
         dictionary is durable and GLOBAL, not per-project. Got {persisted:?}",
        cwd_a.display(),
        cwd_b.display()
    );

    core2.shutdown().expect("second shutdown should be Ok");
}
