//! Integration test for the `wolf-ffi` COMMAND-PALETTE bridge session (plan
//! §Q1 / §5): the opaque `FfiCommandPalette` returned by
//! `WolfCoreHandle::command_palette()`, its `add_window` / `record` mutators, and
//! its `ranked(query, max) -> Vec<FfiCommandRow>` ranker with the per-row
//! accessors.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is modeled on `tests/recent_dirs.rs` (staging
//! `WOLF_STATE_DIR` to a tempdir BEFORE start, `chdir` + poll to populate the
//! recent-dirs MRU), reusing `tests/common/mod.rs`.
//!
//! ## Dependency on coder work (plan §Q1 / §5)
//!
//! None of this exists yet: the coder adds `command_palette()` to `WolfCoreHandle`
//! plus the `FfiCommandPalette` / `FfiCommandRow` opaque types and the
//! `mod palette_row_kind` discriminants in `crates/wolf-ffi/src/lib.rs`,
//! regenerates `generated/`, AND adds the `//crates/wolf-ffi:command_palette`
//! `rust_test` target in `BUILD.bazel` (copy of `recent_dirs`). This test file does
//! NOT touch non-test source, and it will not compile until that lands — expected.
//!
//! Bridge surface assumed (plan §Q1):
//!   * `WolfCoreHandle::command_palette(&self) -> FfiCommandPalette`
//!   * `FfiCommandPalette::add_window(&self, index: u16, cwd: String)`
//!   * `FfiCommandPalette::record(&self, key: String)`
//!   * `FfiCommandPalette::ranked(&self, query: String, max: usize) -> Vec<FfiCommandRow>`
//!   * `FfiCommandRow`: `display() -> String`, `row_kind() -> u8`,
//!     `command_kind() -> u8`, `window_index() -> u16`, `path() -> String`,
//!     `mru_key() -> String`
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available.
//!
//! ## SINGLE #[test]
//!
//! The whole flow lives in ONE `#[test]` because it mutates the process-global
//! `WOLF_STATE_DIR` env var (the `command-mru.sqlite` + `folder-mru.sqlite` stores
//! open at `state_dir()` once at core start): with a single test in this binary
//! there is nothing to race, so no env lock is needed (the precedent is
//! `tests/recent_dirs.rs`).

mod common;

use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::wolf_ffi_start;

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (the `chdir` notification round-trip and the fire-and-forget MRU write) to
/// settle. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A `max` large enough that the assembled candidate set is never truncated (13
/// static rows + a handful of windows/recent dirs stays well under this).
const RANKED_MAX: usize = 100;

/// The window index handed to `add_window` (a distinctive value so the returned
/// switch-window row's `window_index()` is unambiguous).
const WINDOW_INDEX: u16 = 7;

/// The cwd handed to `add_window`. It is a plain label carried into the row's
/// display + mru_key; the row's `path()` stays empty (only OPEN_RECENT rows carry
/// a path).
const WINDOW_CWD: &str = "/tmp/x";

// -- palette_row_kind discriminants (mirror plan §Q1, `mod palette_row_kind`) ---
/// `FfiCommandRow::row_kind()` for a static registry command.
const ROW_KIND_STATIC: u8 = 0;
/// `FfiCommandRow::row_kind()` for a "Switch to Window" row.
const ROW_KIND_SWITCH_WINDOW: u8 = 1;
/// `FfiCommandRow::row_kind()` for an "Open Recent Folder" row.
const ROW_KIND_OPEN_RECENT: u8 = 2;

// -- command_kind discriminants used in the static-row assertions ---------------
/// The last valid command_kind: Command Palette (discriminant 13). It must NOT
/// appear as a row (the palette excludes its own invoking command).
const CMD_KIND_COMMAND_PALETTE: u8 = 13;
/// The highest static command_kind that DOES appear (Decrease Font Size = 12);
/// static rows for kinds `0..=12` must all be present.
const CMD_KIND_LAST_PRESENT: u8 = 12;
/// `command_kind()` for Minimize Window — the key recorded for the MRU round-trip.
const CMD_KIND_MINIMIZE_WINDOW: u8 = 4;
/// The registry `id` (== static mru_key) of Minimize Window.
const MINIMIZE_MRU_KEY: &str = "minimize_window";

/// Canonicalizes a path string, falling back to the raw path when it cannot be
/// resolved on disk. Robust to macOS `/var` -> `/private/var` symlinking.
fn canonicalize(path: &str) -> PathBuf {
    Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(path).to_path_buf())
}

/// Blocks (bounded by `FLUSH_TIMEOUT`) until `handle.cwd()` canonically equals
/// `target`, then asserts it. Mirrors the poll-`cwd()` loop in `recent_dirs.rs`.
fn wait_for_cwd(handle: &wolf_ffi::WolfCoreHandle, target: &Path) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut reported = handle.cwd();
    while Instant::now() < deadline {
        reported = handle.cwd();
        if !reported.is_empty() && canonicalize(&reported) == target {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        canonicalize(&reported),
        target,
        "cwd() should follow handle.chdir (reported={reported:?}, target={target:?})"
    );
}

/// Blocks (bounded by `FLUSH_TIMEOUT`) until `handle.recent_dirs()` contains
/// `target` (the fire-and-forget MRU write can land a beat after `cwd()` flips).
fn wait_for_recent_dir(handle: &wolf_ffi::WolfCoreHandle, target: &Path) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let present = handle
            .recent_dirs()
            .iter()
            .any(|p| canonicalize(p) == target);
        if present {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out waiting for recent_dirs() to contain {target:?}");
}

/// True if any row is a STATIC row for the given `command_kind`.
fn has_static_command(rows: &[wolf_ffi::FfiCommandRow], command_kind: u8) -> bool {
    rows.iter()
        .any(|r| r.row_kind() == ROW_KIND_STATIC && r.command_kind() == command_kind)
}

#[test]
fn command_palette_ranks_static_dynamic_and_mru_rows() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Point the state machinery at a private tempdir for the WHOLE test, BEFORE
    // starting the core: the command-MRU + recent-dirs stores open under
    // `state_dir()` exactly once in `WolfCore::start`. Safe to mutate because this
    // binary has exactly one test. Keep the guard alive until after `shutdown()`.
    let state_dir = tempfile::tempdir().expect("failed to create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state_dir.path());

    // A fresh temp cwd for nvim's launch, kept alive until after `shutdown()`.
    let tmp_cwd = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows,
    // window_index).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp_cwd.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Poll read_frame() until attached (cols > 0) ---------------------
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut frame = handle.read_frame();
    while Instant::now() < deadline {
        frame = handle.read_frame();
        if frame.cols() > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        frame.cols() > 0,
        "core should attach and report a non-empty active frame (cols, got {})",
        frame.cols()
    );

    // --- 2. A fresh palette session: static rows, CommandPalette EXCLUDED ----
    let session = handle.command_palette();
    let rows = session.ranked(String::new(), RANKED_MAX);

    // Every static command_kind 0..=12 is present as a STATIC row.
    for kind in 0..=CMD_KIND_LAST_PRESENT {
        assert!(
            has_static_command(&rows, kind),
            "ranked(\"\") must include a STATIC row for command_kind {kind}"
        );
    }
    // The palette's own invoking command (CommandPalette = 13) is EXCLUDED.
    assert!(
        !has_static_command(&rows, CMD_KIND_COMMAND_PALETTE),
        "ranked(\"\") must EXCLUDE the CommandPalette row (command_kind {CMD_KIND_COMMAND_PALETTE})"
    );

    // --- 3. add_window -> a SWITCH_WINDOW row with the given index, empty path
    session.add_window(WINDOW_INDEX, WINDOW_CWD.to_string());
    let rows = session.ranked(String::new(), RANKED_MAX);
    let switch = rows
        .iter()
        .find(|r| r.row_kind() == ROW_KIND_SWITCH_WINDOW && r.window_index() == WINDOW_INDEX)
        .expect("a SWITCH_WINDOW row with the added window index must be present");
    assert_eq!(
        switch.path(),
        "",
        "a switch-window row carries no path (only OPEN_RECENT rows do)"
    );
    assert_eq!(
        switch.mru_key(),
        format!("switch_window:{WINDOW_CWD}"),
        "switch-window mru_key is switch_window:<cwd>"
    );

    // --- 4. A `:cd` populates recent_dirs -> an OPEN_RECENT row appears -------
    let dir_a = tempfile::tempdir().expect("failed to create temp chdir target");
    let canonical_a = dir_a
        .path()
        .canonicalize()
        .expect("failed to canonicalize chdir target");
    handle.chdir(dir_a.path().to_string_lossy().into_owned());
    wait_for_cwd(&handle, &canonical_a);
    wait_for_recent_dir(&handle, &canonical_a);

    // Recent-dir candidates are fixed at session construction, so open a FRESH
    // session to observe the new recent dir.
    let session2 = handle.command_palette();
    let rows2 = session2.ranked(String::new(), RANKED_MAX);
    let open_recent = rows2
        .iter()
        .find(|r| r.row_kind() == ROW_KIND_OPEN_RECENT && canonicalize(&r.path()) == canonical_a)
        .expect("an OPEN_RECENT row for the chdir target must be present");
    assert!(
        open_recent.display().starts_with("Open Recent Folder: "),
        "open-recent display should be labeled, got {:?}",
        open_recent.display()
    );

    // --- 5. MRU round-trip: record(key) then a fresh session ranks it first --
    // The command-MRU is empty under the fresh WOLF_STATE_DIR, so recording a
    // single static key makes it the sole MRU row -> rank 0 of ranked("").
    session2.record(MINIMIZE_MRU_KEY.to_string());
    let session3 = handle.command_palette();
    let rows3 = session3.ranked(String::new(), RANKED_MAX);
    let first = rows3
        .first()
        .expect("ranked(\"\") must return at least the static rows");
    assert_eq!(
        first.mru_key(),
        MINIMIZE_MRU_KEY,
        "the recorded key must float to rank 0 (MRU-first) after a fresh open"
    );
    assert_eq!(
        first.row_kind(),
        ROW_KIND_STATIC,
        "the recorded minimize_window row is a static row"
    );
    assert_eq!(
        first.command_kind(),
        CMD_KIND_MINIMIZE_WINDOW,
        "the recorded row is Minimize Window (command_kind {CMD_KIND_MINIMIZE_WINDOW})"
    );

    // --- 6. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();

    std::env::remove_var("WOLF_STATE_DIR");
    drop(dir_a);
    drop(tmp_cwd);
    drop(state_dir);
}
