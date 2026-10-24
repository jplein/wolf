//! Integration test for the `wolf-ffi` FILE-ACTION bridge surface:
//! two native-facing Neovim actions on `WolfCoreHandle` that back the macOS
//! "Open Folder…" and "New" commands:
//!
//!   * `WolfCoreHandle::chdir(&self, path: String)` -> `InputEvent::Chdir` ->
//!     `:cd <path>` (GLOBAL working-directory change)
//!   * `WolfCoreHandle::new_buffer(&self)` -> `InputEvent::NewBuffer` ->
//!     `wolf_new_untitled()` (a fresh, MODIFIABLE, unsaved buffer in the current
//!     window, named `Untitled.md` / `Untitled 2.md` / … — the first name that
//!     collides with neither an existing buffer nor a file on disk in cwd — so
//!     `current_file()` ends with an `Untitled( N)?.md` name, NOT an empty
//!     `[No Name]`. In THIS test the earlier `chdir` re-homes the startup
//!     `Untitled.md` into the target dir (a listed buffer collision), so the
//!     `new_buffer()` collision walk skips index 1 and lands on `Untitled 2.md`.)
//!
//! (The `:close`/`close_pane` split-drop action lives in its own real-nvim test,
//! `close_pane.rs` — it is observed structurally via `windows()`, not via
//! `current_file()`.)
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is the sibling of `commands.rs` / `state.rs` (same
//! staging / SKIP-when-unset machinery, reusing `tests/common/mod.rs`, and the
//! same "poll `read_frame()` until attached" pattern).
//!
//! ## What backs `new_buffer`
//!
//! The `wolf_core::InputEvent::NewBuffer` variant runs the `wolf_new_untitled()` Lua
//! helper (naming + `filetype=markdown`, no auto-write). The
//! `//crates/wolf-ffi:file_actions` `rust_test` target runs this file under Bazel.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.
//!
//! ## SINGLE #[test]
//!
//! Both file actions share ONE `#[test]` (like `commands.rs` / `state.rs`): a
//! single real nvim is started, then `chdir` and `new_buffer` are exercised in
//! sequence against it. Neither action mutates the process-global `WOLF_STATE_DIR`,
//! so no env staging or lock is needed here.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::wolf_ffi_start;

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (the `:cd`/`:enew` notification round-trip) to settle. Mirrors the sibling
/// tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Placeholder content for the temp file opened before `new_buffer`.
const FILE_CONTENT: &str = "# new me\nbody\n";

/// True when `name` is a valid anonymous-buffer BASENAME — bare `Untitled.md` or
/// `Untitled <n>.md` for a run of digits `n` — i.e. it matches the regex
/// `^Untitled( \d+)?\.md$`. Implemented WITHOUT a regex dependency (the wolf-ffi
/// test deps do not carry one).
fn is_untitled_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("Untitled") else {
        return false;
    };
    let Some(mid) = rest.strip_suffix(".md") else {
        return false;
    };
    // `mid` is either empty (bare `Untitled.md`) or ` <digits>` (`Untitled 2.md`).
    if mid.is_empty() {
        return true;
    }
    match mid.strip_prefix(' ') {
        Some(digits) => !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

#[test]
fn file_actions_chdir_and_new_buffer() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd (it is nvim's working dir + search root, so it must outlive the session),
    // kept alive until after `shutdown()`, then dropped explicitly at the end.
    let tmp_cwd = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
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

    // --- 2. chdir(dir): cwd() follows, compared canonicalized (macOS `/var` -> `/private/var`).
    let target_dir = tempfile::tempdir().expect("failed to create temp chdir target dir");
    let canonical_target = target_dir
        .path()
        .canonicalize()
        .expect("failed to canonicalize the chdir target");
    handle.chdir(target_dir.path().to_string_lossy().into_owned());

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut reported_cwd = handle.cwd();
    while Instant::now() < deadline {
        reported_cwd = handle.cwd();
        if !reported_cwd.is_empty() {
            if let Ok(canonical_reported) = Path::new(&reported_cwd).canonicalize() {
                if canonical_reported == canonical_target {
                    break;
                }
            }
        }
        sleep(POLL_INTERVAL);
    }
    let canonical_reported = Path::new(&reported_cwd)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(&reported_cwd).to_path_buf());
    assert_eq!(
        canonical_reported, canonical_target,
        "cwd() should follow handle.chdir (reported={reported_cwd:?}, target={canonical_target:?})"
    );

    // --- 3. open(file): current_file() reports a file under the original cwd, opened by
    // absolute path.
    let file_path = tmp_cwd.path().join("new_me.md");
    std::fs::write(&file_path, FILE_CONTENT).expect("failed to write the temp file to open");
    let canonical_file = file_path
        .canonicalize()
        .expect("failed to canonicalize the file to open");
    handle.open(file_path.to_string_lossy().into_owned());

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut current = handle.current_file();
    while Instant::now() < deadline {
        current = handle.current_file();
        if !current.is_empty() {
            if let Ok(canonical_current) = Path::new(&current).canonicalize() {
                if canonical_current == canonical_file {
                    break;
                }
            }
        }
        sleep(POLL_INTERVAL);
    }
    let canonical_current = Path::new(&current)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(&current).to_path_buf());
    assert_eq!(
        canonical_current, canonical_file,
        "current_file() should report the opened file \
         (reported={current:?}, target={canonical_file:?})"
    );

    // --- 4. new_buffer() names the fresh buffer `Untitled 2.md`: the chdir re-homed the startup
    // `Untitled.md` into the target dir, so the collision walk skips index 1.
    const UNTITLED_2: &str = "Untitled 2.md";
    handle.new_buffer();

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after_new = handle.current_file();
    while Instant::now() < deadline {
        after_new = handle.current_file();
        if after_new.ends_with(UNTITLED_2) {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    let basename = Path::new(&after_new)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    assert!(
        is_untitled_name(&basename),
        "current_file() basename should match ^Untitled( \\d+)?\\.md$ after new_buffer() \
         (wolf_new_untitled names the fresh, unsaved buffer), got basename {basename:?} \
         from {after_new:?}"
    );
    assert!(
        after_new.ends_with(UNTITLED_2),
        "with the re-homed startup Untitled.md occupying index 1, the new_buffer() \
         collision walk must yield {UNTITLED_2:?}, got {after_new:?}"
    );

    // --- 5. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(target_dir);
    drop(tmp_cwd);
}
