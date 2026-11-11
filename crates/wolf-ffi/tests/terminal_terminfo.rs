//! Integration test that `wolf_ffi_start` takes Wolf's terminfo directory from
//! `WOLF_TERMINFO_DIR`, which the macOS bundle defaults through `Project.swift`, so a session's
//! shell gets `TERM=xterm-ghostty` and `TERMINFO` naming that directory.
//!
//! A real, hermetic Neovim starts through `wolf_ffi_start` and a real `/bin/sh` runs in a pty;
//! the shell itself reports what it sees. Core's choice between the entry and the fallback is
//! proved by `crates/wolf-core/tests/terminal_terminfo.rs`; this test proves the bridge passes
//! the variable on.
//!
//! The test sets `WOLF_TERMINFO_DIR`, `TERM` and `TERMINFO` in its own process, so it is the
//! only test in its binary: a `setenv` would race a `getenv` on a neighbour's thread.
//!
//! The directory is the compiled entry the build stages in `WOLF_TEST_TERMINFO_DIR`: Bazel
//! `data` on macOS, the devShell's export of `${wolfGhosttyVt}/share/terminfo` on Linux. The
//! test panics when it does not resolve, since both environments always stage it, and loudly
//! skips without a pinned Neovim, as the suite does.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiTerminalFrame};
use wolf_test_support::resolve_test_runfile;

/// How long nvim startup or the shell's output may take to surface.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// `terminal_target::WINDOW`: show the session in the given window.
const TARGET_WINDOW: u8 = 0;

/// The variable the front-ends read Wolf's terminfo directory from.
const TERMINFO_DIR_ENV: &str = "WOLF_TERMINFO_DIR";

/// The variable that names the compiled terminfo directory the build staged for the tests.
const TEST_TERMINFO_ENV: &str = "WOLF_TEST_TERMINFO_DIR";

/// A `TERMINFO` that names no database: a shell showing it inherited the test's value.
const DECOY_TERMINFO: &str = "/nonexistent/wolf-decoy-terminfo";

/// The marker the shell prints last, once every field before it is on the row.
const DONE: &str = "|done";

/// Row `row`'s text from `row_cells`, an empty cell as one space, trailing spaces trimmed.
fn row_text(frame: &FfiTerminalFrame, row: u16) -> String {
    let mut text: String = frame
        .row_cells(row)
        .iter()
        .map(|cell| {
            let t = cell.text();
            if t.is_empty() {
                " ".to_owned()
            } else {
                t
            }
        })
        .collect();
    text.truncate(text.trim_end_matches(' ').len());
    text
}

/// Every row's text, joined with newlines.
fn frame_text(frame: &FfiTerminalFrame) -> String {
    (0..frame.rows())
        .map(|row| row_text(frame, row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `wolf_ffi_start` reads `WOLF_TERMINFO_DIR`, and a session it opens gives its shell
/// `TERM=xterm-ghostty` and that `TERMINFO`, over the inherited decoys.
#[test]
fn wolf_ffi_start_takes_the_terminfo_dir_from_its_variable() {
    if common::nvim_bin_or_skip().is_none() {
        return;
    }
    let raw = std::env::var(TEST_TERMINFO_ENV).unwrap_or_default();
    let dir = resolve_test_runfile(TEST_TERMINFO_ENV).unwrap_or_else(|| {
        panic!(
            "{TEST_TERMINFO_ENV} is {raw:?}, which resolves to no directory. Bazel stages \
             //bazel/ghostty-vt:ghostty_terminfo through `data` and `env`, and the Linux devShell \
             exports ${{wolfGhosttyVt}}/share/terminfo; unset means the wiring or the export was \
             lost."
        )
    });
    let dir = std::fs::canonicalize(&dir)
        .unwrap_or_else(|e| panic!("failed to canonicalize {}: {e}", dir.display()));
    std::env::set_var(TERMINFO_DIR_ENV, &dir);
    std::env::set_var("TERM", "dumb");
    std::env::set_var("TERMINFO", DECOY_TERMINFO);

    let init_lua = common::init_lua_path();
    let cwd = common::fresh_cwd();
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    let deadline = Instant::now() + TIMEOUT;
    while handle.read_frame().cols() == 0 {
        assert!(
            Instant::now() < deadline,
            "timed out after {TIMEOUT:?} waiting for the core to attach (read_frame cols > 0)"
        );
        sleep(POLL_INTERVAL);
    }

    let script = format!(
        "printf 'term=%s|' \"$TERM\"; \
         if [ \"$TERMINFO\" = '{}' ]; then printf 'terminfo-match{DONE}\\n'; \
         else printf 'terminfo-mismatch{DONE}\\n'; printf 'TERMINFO=%s\\n' \"$TERMINFO\"; fi; \
         exec sleep 1000",
        dir.display()
    );
    let id = handle.open_terminal(0, TARGET_WINDOW, script);
    assert_ne!(
        id, 0,
        "open_terminal returned 0: the pty or the shell could not be started"
    );

    let deadline = Instant::now() + TIMEOUT;
    let text = loop {
        let text = frame_text(&handle.terminal_frame(id));
        if text.contains(DONE) {
            break text;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {TIMEOUT:?} waiting for session {id} to print {DONE:?}; its frame \
             reads:\n{text}"
        );
        sleep(POLL_INTERVAL);
    };
    assert!(
        text.contains("term=xterm-ghostty|"),
        "the shell must see TERM=xterm-ghostty once {TERMINFO_DIR_ENV} names a directory holding \
         the entry: term=xterm-256color means wolf_ffi_start did not read {TERMINFO_DIR_ENV} into \
         CoreConfig.terminfo_dir, term=dumb means TERM was inherited rather than set. The frame \
         reads:\n{text}"
    );
    assert!(
        text.contains("|terminfo-match|done"),
        "the shell must see TERMINFO={}; terminfo-mismatch means it was not set, so the inherited \
         decoy {DECOY_TERMINFO} or nothing shows on the TERMINFO= row. The frame reads:\n{text}",
        dir.display()
    );

    handle.shutdown();
}
