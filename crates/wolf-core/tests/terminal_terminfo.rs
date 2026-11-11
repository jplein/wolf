//! Integration tests of the terminal type a session's shell gets: `xterm-ghostty` with Wolf's
//! own `TERMINFO` when `CoreConfig.terminfo_dir` holds the compiled entry, and `xterm-256color`
//! with no `TERMINFO` from Wolf otherwise. A real `/bin/sh` runs in a real pty beside a real,
//! hermetic Neovim, and the shell itself reports what it sees.
//!
//! The entry is the one the build compiles from the pinned libghostty-vt release, staged in
//! `WOLF_TEST_TERMINFO_DIR`: Bazel `data` on macOS, the devShell's export of
//! `${wolfGhosttyVt}/share/terminfo` on Linux. A test that needs it panics when it does not
//! resolve, because both environments always stage it.
//!
//! ## Premises about the components
//!
//! Each is measured by these tests against the real component, not assumed:
//!
//! - `infocmp -x <name>` exits 0 and prints the entry's extended capabilities, `setrgbf` among
//!   them, when `TERMINFO` names a database that holds `<name>` compiled with `tic -x`. Without
//!   `-x` at compile time the extended capabilities are absent. `infocmp` comes from `/usr/bin`
//!   on macOS, which Bazel's strict test `PATH` keeps, and from the devShell's ncurses on Linux.
//! - A session's shell inherits the app's environment, so the decoys set before any core starts
//!   reach it unless core replaces them.
//! - The shell compares `$TERMINFO` with the expected path itself, as `terminal_session.rs`
//!   compares `pwd`, because a runfiles or temp path wraps an 80-column frame.

mod common;

use std::path::PathBuf;
use std::sync::Once;

use common::{
    config_in, frame_text, fresh_cwd, open_session, resolve_test_runfile, sh_script, wait_for_text,
    wake_channel, FlushWaiter, DEFAULT_COLS, DEFAULT_ROWS, FLUSH_TIMEOUT,
};
use wolf_core::{CoreConfig, TerminalTarget, WolfCore};

/// The variable that names the compiled terminfo directory the build staged for the tests.
const TEST_TERMINFO_ENV: &str = "WOLF_TEST_TERMINFO_DIR";

/// A `TERM` no session should ever see: a shell showing it inherited the test's value.
const DECOY_TERM: &str = "dumb";

/// A `TERMINFO` that names no database: a shell showing it inherited the test's value.
const DECOY_TERMINFO: &str = "/nonexistent/wolf-decoy-terminfo";

/// The terminal type Wolf's own entry defines.
const GHOSTTY_TERM: &str = "xterm-ghostty";

/// The terminal type of the fallback, which every host database knows.
const FALLBACK_TERM: &str = "xterm-256color";

/// The entry's path in the hex layout of macOS's ncurses, then in ncurses' letter layout.
const ENTRY_PATHS: [&str; 2] = ["78/xterm-ghostty", "x/xterm-ghostty"];

/// The marker the shell prints last, once every field before it is on the row.
const DONE: &str = "|done";

/// Sets the decoy `TERM` and `TERMINFO` once per binary, before any core starts, so no
/// `setenv` races a `getenv` on a neighbouring test's thread.
fn set_decoys() {
    static DECOYS: Once = Once::new();
    DECOYS.call_once(|| {
        std::env::set_var("TERM", DECOY_TERM);
        std::env::set_var("TERMINFO", DECOY_TERMINFO);
    });
}

/// The staged terminfo directory, canonical, after checking that it holds the entry. Panics
/// when the variable is unset or names nothing, since both environments always stage it.
fn staged_terminfo_dir() -> PathBuf {
    let raw = std::env::var(TEST_TERMINFO_ENV).unwrap_or_default();
    let dir = resolve_test_runfile(TEST_TERMINFO_ENV).unwrap_or_else(|| {
        panic!(
            "{TEST_TERMINFO_ENV} is {raw:?}, which resolves to no directory. Bazel stages \
             //bazel/ghostty-vt:ghostty_terminfo through `data` and `env`, and the Linux devShell \
             exports ${{wolfGhosttyVt}}/share/terminfo; unset means the wiring or the export was \
             lost, a path that does not exist means the build produced no tree."
        )
    });
    let dir = std::fs::canonicalize(&dir)
        .unwrap_or_else(|e| panic!("failed to canonicalize {}: {e}", dir.display()));
    assert!(
        ENTRY_PATHS.iter().any(|entry| dir.join(entry).is_file()),
        "precondition: the staged terminfo directory {} must hold the compiled entry at one of \
         {ENTRY_PATHS:?}. Neither means the generator or `tic` wrote nothing there, so the tests \
         below would fail on the build, not on core.",
        dir.display()
    );
    dir
}

/// Starts a core in a fresh cwd whose sessions get `terminfo_dir`, and waits for its first
/// flush.
fn start_core_with_terminfo(terminfo_dir: PathBuf) -> (WolfCore, FlushWaiter) {
    set_decoys();
    let config = CoreConfig {
        terminfo_dir: Some(terminfo_dir),
        ..config_in(fresh_cwd(), DEFAULT_COLS, DEFAULT_ROWS)
    };
    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config, wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}

/// A session given a directory that holds the compiled `xterm-ghostty` entry runs its shell
/// with `TERM=xterm-ghostty` and `TERMINFO` naming that directory over the inherited decoys,
/// and `infocmp -x xterm-ghostty` there finds the entry with its extended `setrgbf`.
#[test]
fn a_session_given_wolfs_terminfo_dir_gets_xterm_ghostty() {
    let dir = staged_terminfo_dir();
    let (core, waiter) = start_core_with_terminfo(dir.clone());
    let script = format!(
        "printf 'term=%s|' \"$TERM\"; \
         if [ \"$TERMINFO\" = '{dir}' ]; then printf 'terminfo-match|'; \
         else printf 'terminfo-mismatch|'; fi; \
         out=$(infocmp -x {GHOSTTY_TERM} 2>&1); rc=$?; \
         case \"$out\" in *setrgbf=*) s=yes;; *) s=no;; esac; \
         printf 'infocmp=%s|setrgbf=%s{DONE}\\n' \"$rc\" \"$s\"; \
         printf 'TERMINFO=%s\\n' \"$TERMINFO\"; exec sleep 1000",
        dir = dir.display()
    );
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(&script));

    let frame = wait_for_text(&core, &waiter, id, DONE);
    let text = frame_text(&frame);
    assert!(
        text.contains(&format!("term={GHOSTTY_TERM}|")),
        "the shell must see TERM={GHOSTTY_TERM}: term={FALLBACK_TERM} means core did not find \
         the entry in {} or ignored the configured directory, term={DECOY_TERM} means TERM was \
         inherited rather than set. The frame reads:\n{text}",
        dir.display()
    );
    assert!(
        text.contains("|terminfo-match|"),
        "the shell must see TERMINFO={}; terminfo-mismatch means it was not set, so the inherited \
         decoy {DECOY_TERMINFO} or nothing shows on the TERMINFO= row. The frame reads:\n{text}",
        dir.display()
    );
    assert!(
        text.contains("|infocmp=0|"),
        "`infocmp -x {GHOSTTY_TERM}` must exit 0 in the shell: 1 means the database TERMINFO \
         names has no {GHOSTTY_TERM} entry the host's ncurses can read, 127 means no infocmp on \
         the shell's PATH. The frame reads:\n{text}"
    );
    assert!(
        text.contains("|setrgbf=yes|done"),
        "`infocmp -x {GHOSTTY_TERM}` must print the extended capability setrgbf; setrgbf=no with \
         infocmp=0 means the entry was compiled without `tic -x`, which drops extended \
         capabilities. The frame reads:\n{text}"
    );
    eprintln!("the shell given {} printed:\n{text}", dir.display());

    core.shutdown().expect("shutdown should be Ok");
}

/// A session given a directory without the entry falls back to `TERM=xterm-256color` and
/// leaves the inherited `TERMINFO` alone. The existing `terminal_session.rs` test covers a
/// core given no directory at all.
#[test]
fn a_terminfo_dir_without_the_entry_falls_back_to_xterm_256color() {
    let empty = tempfile::tempdir().expect("failed to create an empty terminfo directory");
    let dir = std::fs::canonicalize(empty.path()).expect("failed to canonicalize the temp dir");
    let (core, waiter) = start_core_with_terminfo(dir.clone());
    let script = format!(
        "printf 'term=%s|' \"$TERM\"; \
         case \"$TERMINFO\" in \
         '{DECOY_TERMINFO}') s=inherited;; '{dir}') s=configured;; '') s=unset;; *) s=other;; \
         esac; \
         printf 'terminfo-%s{DONE}\\n' \"$s\"; \
         printf 'TERMINFO=%s\\n' \"$TERMINFO\"; exec sleep 1000",
        dir = dir.display()
    );
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(&script));

    let frame = wait_for_text(&core, &waiter, id, DONE);
    let text = frame_text(&frame);
    assert!(
        text.contains(&format!("term={FALLBACK_TERM}|")),
        "a directory with no {GHOSTTY_TERM} entry must give the shell TERM={FALLBACK_TERM}: \
         term={GHOSTTY_TERM} means core took any configured directory without looking for the \
         entry, term={DECOY_TERM} means TERM was inherited rather than set. The frame reads:\n\
         {text}"
    );
    assert!(
        text.contains(&format!("|terminfo-inherited{DONE}")),
        "without the entry Wolf must set no TERMINFO, so the shell sees the inherited \
         {DECOY_TERMINFO}: terminfo-configured means the fallback still set TERMINFO to the \
         empty directory {}, terminfo-unset means it removed the inherited one, terminfo-other \
         means something else set it, as the TERMINFO= row shows. The frame reads:\n{text}",
        dir.display()
    );

    core.shutdown().expect("shutdown should be Ok");
}
