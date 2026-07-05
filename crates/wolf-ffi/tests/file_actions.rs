//! Integration test for the `wolf-ffi` FILE-ACTION bridge surface (plan §3): the
//! two new native-facing Neovim actions on `WolfCoreHandle` that back the macOS
//! "Open Folder…" and "Close File" commands:
//!
//!   * `WolfCoreHandle::chdir(&self, path: String)` -> `InputEvent::Chdir` ->
//!     `:cd <path>` (GLOBAL working-directory change)
//!   * `WolfCoreHandle::close_buffer(&self)` -> `InputEvent::CloseBuffer` ->
//!     `:bdelete` (non-bang; nvim replaces the deleted buffer with a fresh
//!     `[No Name]`, so `current_file()` -> "")
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is the sibling of `commands.rs` / `state.rs` (same
//! staging / SKIP-when-unset machinery, reusing `tests/common/mod.rs`, and the
//! same "poll `read_frame()` until attached" pattern).
//!
//! ## Dependency on coder work (plan §3 + §5)
//!
//! `chdir` and `close_buffer` do NOT exist on `WolfCoreHandle` yet: the coder adds
//! them to the bridge (`extern "Rust"`) and `impl WolfCoreHandle` in
//! `crates/wolf-ffi/src/lib.rs`, backed by the new `wolf_core::InputEvent::Chdir` /
//! `InputEvent::CloseBuffer` variants, AND adds the `//crates/wolf-ffi:file_actions`
//! `rust_test` target to `BUILD.bazel` (copy of `commands`). This test file does
//! NOT touch BUILD files, and it will not compile until those methods land — that
//! is expected.
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
//! single real nvim is started, then `chdir` and `close_buffer` are exercised in
//! sequence against it. Neither action mutates the process-global `WOLF_STATE_DIR`,
//! so no env staging or lock is needed here.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::wolf_ffi_start;

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (the `:cd`/`:bdelete` notification round-trip) to settle. Mirrors the sibling
/// tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Placeholder content for the temp file opened before `close_buffer`.
const FILE_CONTENT: &str = "# close me\nbody\n";

#[test]
fn file_actions_chdir_and_close_buffer() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd (its `<cwd>/.wolf/` MRU cache must outlive the session),
    // kept alive until after `shutdown()`, then dropped explicitly at the end.
    let tmp_cwd = tempfile::tempdir().expect("failed to create temp cwd directory");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp_cwd.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
        0,
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

    // --- 2. chdir(dir): cwd() must follow the requested working directory ----
    // A fresh temp dir; canonicalize-compare the reported cwd against it so the
    // assertion is robust to macOS `/var` -> `/private/var` symlinking.
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

    // --- 3. open(file): current_file() must report the opened file ----------
    // Written under the ORIGINAL temp cwd; opened via its absolute path (works
    // regardless of the just-changed working directory).
    let file_path = tmp_cwd.path().join("close_me.md");
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

    // --- 4. close_buffer(): :bdelete makes a fresh [No Name] -> current_file "" -
    handle.close_buffer();

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut after_close = handle.current_file();
    while Instant::now() < deadline {
        after_close = handle.current_file();
        if after_close.is_empty() {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        after_close.is_empty(),
        "current_file() should become empty after close_buffer() \
         (:bdelete replaces the buffer with a fresh [No Name]), got {after_close:?}"
    );

    // --- 5. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(target_dir);
    drop(tmp_cwd);
}
