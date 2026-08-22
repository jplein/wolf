//! Integration test for PROSE-ONLY spell checking (`init.lua`'s `WolfSpell`
//! augroup).
//!
//! Drives a REAL, hermetic Neovim (via `WOLF_NVIM_BIN`, resolved by
//! `common::require_nvim_bin`) — no mocks, per the project's real-nvim policy.
//!
//! The behavior under test: `runtime/nvim/init.lua` registers a `FileType`
//! autocmd that sets `spell` (and pins `spelllang` to `en`) buffer-locally for
//! `markdown`, `text`, and `gitcommit`, and for NOTHING else. The negative half
//! is as load-bearing as the positive half: a global `spell` would underline
//! source code and the terminal drawer, which is exactly what the buffer-local
//! autocmd exists to avoid.
//!
//! ## Why the assertion goes through a toast
//!
//! `WolfCore` exposes no generic `exec_lua`, and `spell` is a WINDOW-local option
//! that never reaches `Snapshot` on its own. So each case echoes the live
//! `vim.bo.filetype` / `vim.wo.spell` through `nvim_echo`, which `ext_messages`
//! routes into `Snapshot.toast` — the same channel `messages.rs` asserts on.
//! Typing the leading `:` of the Ex command is itself a toast-dismissing key, so
//! each probe reads a FRESH echo rather than a stale one.
//!
//! ## `spelllang` is asserted, not incidental
//!
//! The bundled Neovim runtime ships exactly one spell file (`en.utf-8.spl`). A
//! `spelllang` naming anything else would make the runtime `spellfile.lua` plugin
//! open a blocking download prompt, which under `ext_messages` (`cmdheight = 0`)
//! has nowhere to draw. `spell_flags_a_misspelling` closes that loop by proving
//! the dictionary actually LOADED: `spellbadword()` only reports a bad word when
//! a real `.spl` is in effect.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key, WolfCore};

/// Gap between `read_snapshot` polls while waiting for a probe echo to land.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Prefix of every probe echo. Each probe appends a MONOTONIC nonce and a
/// trailing `-`, so a probe can never match the residue of an earlier one — the
/// trailing `-` is what keeps `WOLFPROBE-1-` from prefix-matching `WOLFPROBE-10-`.
const PROBE: &str = "WOLFPROBE";

/// Source of the per-probe nonce. Shared across tests, which only makes the tags
/// more unique than the per-core uniqueness the assertions need.
static PROBE_SEQ: AtomicU32 = AtomicU32::new(0);

/// Types `:{cmd}<CR>` as keystrokes. Mirrors `messages.rs`'s helper of the same
/// name; `cmd` must NOT include the leading `:`.
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
/// probe's residue immediately and reports stale state. Every call mints a fresh
/// tag, so a match proves this echo landed.
fn echo(core: &WolfCore, waiter: &FlushWaiter, lua_expr: &str) -> String {
    let tag = format!("{PROBE}-{}-", PROBE_SEQ.fetch_add(1, Ordering::Relaxed));
    send_ex(
        core,
        &format!(r#"lua vim.api.nvim_echo({{{{"{tag} "..({lua_expr})}}}}, false, {{}})"#),
    );
    poll_until(waiter, &format!("the `{tag}` probe echo to reach the toast"), || {
        toast_text(core).contains(&tag)
    });

    let text = toast_text(core);
    text.split(&tag)
        .nth(1)
        .unwrap_or_else(|| panic!("probe tag {tag} missing from toast {text:?}"))
        .trim()
        .to_string()
}

/// Echoes the ACTIVE window's `filetype` / `spell` / `spelllang`.
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

/// Writes `name` into `dir` with prose content and returns its path.
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

/// Every prose filetype gets `spell` on, and every non-prose filetype does not —
/// asserted in ONE core so the cases share a process and cannot disagree about
/// startup state. The filetype nvim actually detected is asserted alongside
/// `spell`, so a fixture whose detection silently changed fails loudly here
/// rather than passing vacuously.
#[test]
fn spell_is_on_for_prose_filetypes_only() {
    // (fixture name, expected filetype, expected `spell`)
    const CASES: &[(&str, &str, bool)] = &[
        ("doc.md", "markdown", true),
        ("notes.txt", "text", true),
        ("COMMIT_EDITMSG", "gitcommit", true),
        ("main.rs", "rust", false),
        ("data.json", "json", false),
        ("build.sh", "sh", false),
    ];

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    for (name, want_ft, want_spell) in CASES {
        let path = write_fixture(dir.path(), name, "teh wrold\n");
        open(&core, &waiter, path);

        let (ft, spell, lang) = probe(&core, &waiter);
        assert_eq!(
            &ft, want_ft,
            "{name} should be detected as filetype {want_ft}"
        );
        assert_eq!(
            spell, *want_spell,
            "{name} (filetype {ft}) should have spell = {want_spell}"
        );
        if *want_spell {
            assert_eq!(
                lang, "en",
                "{name} should pin spelllang to the one bundled .spl (en)"
            );
        }
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// `spell` being ON is worthless if no dictionary loaded, so this proves the
/// bundled `en.utf-8.spl` is actually in effect: `spellbadword()` reports a
/// misspelling only when a real spell file backs `spelllang`. It also proves no
/// blocking "download the spell file?" prompt was raised — that prompt would
/// leave the toast in a prompt state and the probe would never match.
#[test]
fn spell_flags_a_misspelling_in_markdown() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let path = write_fixture(dir.path(), "doc.md", "teh wrold\n");
    open(&core, &waiter, path);

    // Sanity-check the precondition through the same channel the other test uses.
    let (ft, spell, _) = probe(&core, &waiter);
    assert_eq!(ft, "markdown", "the fixture should open as markdown");
    assert!(spell, "markdown should have spell on");

    let bad = echo(
        &core,
        &waiter,
        r#"(vim.fn.spellbadword("teh wrold")[1] or "<none>")"#,
    );
    assert_eq!(
        bad, "teh",
        "spellbadword() should flag `teh`, which proves the bundled en.utf-8.spl \
         actually loaded"
    );

    core.shutdown().expect("shutdown should be Ok");
}
