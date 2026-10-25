//! Guard: no wolf-ffi real-nvim test ever reaches the user's own Wolf state dir.
//!
//! Every test that goes through `common::nvim_bin_or_skip` must run against a throwaway
//! `WOLF_STATE_DIR`. Without one, [`state_dir`] resolves to the platform default under
//! `$HOME`, so the captive nvim sources the user's real `runtime_user/init_user.lua`
//! (`runtime/nvim/init.lua` loads it last, through `WOLF_RUNTIME_USER_DIR`) and the core
//! writes swap, MRU and state files beside it. A user who sets `ignorecase` there turns
//! `search.rs` red, and every run pollutes their state.
//!
//! ## Re-exec design
//!
//! The property depends on the process environment (`HOME`, `WOLF_STATE_DIR`), which a
//! test cannot safely rewrite in its own process: `setenv` races the other threads'
//! `getenv`, and the harness publishes its state dir once per binary. So the one
//! visible test, [`a_planted_user_init_lua_never_reaches_a_test_core`], re-executes
//! this same test binary to run one `#[ignore]`d payload in a fresh process, with:
//!
//! - `HOME` set to a scratch directory holding a planted
//!   `<home>/<PLATFORM_STATE_REL>/runtime_user/init_user.lua` that sets `ignorecase`;
//! - `WOLF_STATE_DIR`, `XDG_STATE_HOME` and `XDG_DATA_HOME` removed, so the platform
//!   default is the only place Wolf can look;
//! - [`PLANT_ENV`] naming the scratch home. A payload without it returns at once, so
//!   `cargo test -- --include-ignored` never runs one against the real `HOME`.
//!
//! [`isolated_payload`] proves the harness moves `state_dir()` off the plant, that the
//! captive nvim never sources it (`ignorecase` stays off), and that nothing is written
//! under it. [`control_payload`] points `WOLF_STATE_DIR` at a second plant and proves
//! the plant does take effect when Wolf is aimed at it, so the isolated payload's
//! `!ignorecase` is not vacuous, and that the harness leaves a caller's own
//! `WOLF_STATE_DIR` alone (set-if-absent).
//!
//! The parent requires each child to exit successfully and report exactly one passed
//! test, so a renamed payload (which libtest would report as "0 passed") cannot pass.
//!
//! ## Premises, each measured rather than assumed
//!
//! - With no override, `state_dir()` derives from `$HOME`: `dirs` 5 reads `HOME` before
//!   falling back to `getpwuid`, and on Linux `dirs::state_dir()` is `$XDG_STATE_HOME`
//!   or `$HOME/.local/state`. [`isolated_payload`] asserts it before anything else.
//! - `init_user.lua` has been sourced by the time `ignorecase()` answers, and setting
//!   `vim.o.ignorecase` there is visible to it. [`control_payload`] asserts it.
//! - The SKIP-when-unset policy of `common::nvim_bin_or_skip` applies to the parent.
//!   A child that cannot resolve the nvim its parent resolved is a failure, not a skip.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use wolf_core::state_dir;
use wolf_ffi::wolf_ffi_start;

/// Names the scratch `HOME` a payload runs under. Its presence is what marks a process
/// as a child of [`a_planted_user_init_lua_never_reaches_a_test_core`].
const PLANT_ENV: &str = "WOLF_TEST_PLANTED_HOME";

/// The exact test name of the payload run without a `WOLF_STATE_DIR`.
const ISOLATED_PAYLOAD: &str = "isolated_payload";

/// The exact test name of the payload run with `WOLF_STATE_DIR` aimed at the plant.
const CONTROL_PAYLOAD: &str = "control_payload";

/// The planted `init_user.lua`. `ignorecase` is off in nvim's defaults and in the
/// captive `init.lua`, so reading it back on is proof the plant was sourced.
const PLANT_BODY: &str = "vim.o.ignorecase = true\n";

/// Wolf's default state dir relative to `$HOME`, as `wolf_core::state_dir` resolves it
/// with no override: `dirs::data_dir()/Wolf` on macOS.
#[cfg(target_os = "macos")]
const PLATFORM_STATE_REL: &str = "Library/Application Support/Wolf";

/// Wolf's default state dir relative to `$HOME`, as `wolf_core::state_dir` resolves it
/// with no override: `dirs::state_dir()/wolf`, i.e. `$HOME/.local/state/wolf` once
/// `XDG_STATE_HOME` is removed.
#[cfg(not(target_os = "macos"))]
const PLATFORM_STATE_REL: &str = ".local/state/wolf";

/// Creates a scratch home holding `<home>/<PLATFORM_STATE_REL>/runtime_user/init_user.lua`
/// with [`PLANT_BODY`]. Returns the home (deleted on drop) and the planted state root.
/// Never touches the real `HOME`.
fn plant_home() -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().expect("failed to create the scratch HOME");
    let root = home.path().join(PLATFORM_STATE_REL);
    let runtime_user = root.join("runtime_user");
    std::fs::create_dir_all(&runtime_user).expect("failed to create the planted runtime_user/");
    std::fs::write(runtime_user.join("init_user.lua"), PLANT_BODY)
        .expect("failed to write the planted init_user.lua");
    (home, root)
}

/// Re-executes this test binary to run exactly the ignored payload `name`, under `home`
/// and with every variable that could redirect Wolf's state dir removed.
fn run_child(name: &str, home: &Path, extra_env: Option<(&str, &Path)>) -> Output {
    let exe = std::env::current_exe().expect("failed to resolve this test binary's path");
    let mut cmd = Command::new(exe);
    cmd.args([
        name,
        "--exact",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ])
    .env("HOME", home)
    .env(PLANT_ENV, home)
    .env_remove("WOLF_STATE_DIR")
    .env_remove("XDG_STATE_HOME")
    .env_remove("XDG_DATA_HOME");
    if let Some((key, value)) = extra_env {
        cmd.env(key, value);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to re-execute the test binary for {name}: {e}"))
}

/// Panics, with the child's full output, unless it exited successfully having run and
/// passed exactly one test.
fn assert_child_passed(name: &str, out: &Output) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let ran_one = stdout.contains("test result: ok. 1 passed;");
    assert!(
        out.status.success() && ran_one,
        "child payload `{name}` did not pass (status {status}, exactly one test passed: \
         {ran_one}). A failed status is the payload's own assertion, quoted below; a success \
         that ran no test means `{name}` no longer names an ignored test in this file, so \
         nothing was checked.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        status = out.status,
    );
}

/// Lists every path under `root`, relative to it, recursively and sorted.
fn list_tree(root: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("failed to list {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("failed to read a directory entry").path();
            let rel = path.strip_prefix(root).expect("entry outside its root");
            out.insert(rel.to_string_lossy().into_owned());
            if path.is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

/// The planted state root a payload must check, or `None` when this process is not a
/// child of the parent test (the payload then returns without checking anything).
fn planted_home_or_return(name: &str) -> Option<PathBuf> {
    let Some(home) = std::env::var_os(PLANT_ENV) else {
        eprintln!(
            "{name}: {PLANT_ENV} is unset, so this is not a child of \
             a_planted_user_init_lua_never_reaches_a_test_core; nothing to check."
        );
        return None;
    };
    Some(PathBuf::from(home))
}

/// The parent: plants one scratch home per payload and requires both payloads to pass.
#[test]
fn a_planted_user_init_lua_never_reaches_a_test_core() {
    if common::nvim_bin_or_skip().is_none() {
        return;
    }

    let (home_a, _root_a) = plant_home();
    let out = run_child(ISOLATED_PAYLOAD, home_a.path(), None);
    assert_child_passed(ISOLATED_PAYLOAD, &out);

    let (home_b, root_b) = plant_home();
    let out = run_child(
        CONTROL_PAYLOAD,
        home_b.path(),
        Some(("WOLF_STATE_DIR", &root_b)),
    );
    assert_child_passed(CONTROL_PAYLOAD, &out);
}

/// Child payload with no `WOLF_STATE_DIR`: the harness must move Wolf off the planted
/// default, the captive nvim must not source the plant, and nothing may be written there.
#[test]
#[ignore = "child payload; run only by a_planted_user_init_lua_never_reaches_a_test_core"]
fn isolated_payload() {
    let Some(home) = planted_home_or_return(ISOLATED_PAYLOAD) else {
        return;
    };
    let planted_root = home.join(PLATFORM_STATE_REL);
    let planted_file = planted_root.join("runtime_user").join("init_user.lua");

    // Preconditions, checked before the harness runs: the plant sits where Wolf looks.
    assert!(
        std::env::var_os("WOLF_STATE_DIR").is_none(),
        "the parent must remove WOLF_STATE_DIR from this payload's environment; with it set \
         Wolf never looks at the planted default, and the guard would be vacuous"
    );
    assert_eq!(
        state_dir(),
        Some(planted_root.clone()),
        "with HOME={} and no override, state_dir() should be the planted default; the plant \
         is not where Wolf looks by default, so the guard would be vacuous. Either \
         PLATFORM_STATE_REL is wrong for this platform or `dirs` stopped deriving from $HOME",
        home.display()
    );
    assert!(
        planted_file.is_file(),
        "the planted {} is missing before any core started",
        planted_file.display()
    );

    assert!(
        common::nvim_bin_or_skip().is_some(),
        "the parent resolved a pinned nvim but this child cannot; is the env inherited?"
    );

    let resolved = state_dir();
    let isolated = resolved
        .as_deref()
        .is_some_and(|d| d != planted_root && !d.starts_with(&home));
    assert!(
        isolated,
        "the wolf-ffi harness left state_dir() at {resolved:?}; every real-nvim test reads the \
         user's init_user.lua and writes swap/MRU files into their state dir — see \
         common::nvim_bin_or_skip. (None means no state dir at all; a path under {} means it \
         still derives from HOME.)",
        home.display()
    );

    let cwd = tempfile::tempdir().expect("failed to create temp cwd directory");
    let handle = wolf_ffi_start(
        common::init_lua_path().to_string_lossy().into_owned(),
        cwd.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    let ignorecase = handle.ignorecase();
    handle.shutdown();
    drop(cwd);

    assert!(
        !ignorecase,
        "captive nvim sourced the planted {}: ignorecase is on, and only the plant sets it",
        planted_file.display()
    );

    let tree = list_tree(&planted_root);
    let expected: BTreeSet<String> = ["runtime_user", "runtime_user/init_user.lua"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(
        tree,
        expected,
        "a test core wrote into the planted state dir {}; everything under it is listed \
         (swap/, file-mru.sqlite or state.json there means the core still used the default)",
        planted_root.display()
    );
    let body = std::fs::read(&planted_file).expect("failed to read back the planted file");
    assert_eq!(
        body,
        PLANT_BODY.as_bytes(),
        "a test core rewrote the planted {}",
        planted_file.display()
    );
}

/// Child payload with `WOLF_STATE_DIR` aimed at the plant: the harness must keep it, and
/// the plant must take effect, which is what makes [`isolated_payload`] non-vacuous.
#[test]
#[ignore = "child payload; run only by a_planted_user_init_lua_never_reaches_a_test_core"]
fn control_payload() {
    let Some(home) = planted_home_or_return(CONTROL_PAYLOAD) else {
        return;
    };
    let planted_root = home.join(PLATFORM_STATE_REL);

    assert_eq!(
        std::env::var_os("WOLF_STATE_DIR").map(PathBuf::from),
        Some(planted_root.clone()),
        "the parent must aim WOLF_STATE_DIR at the second plant for the control payload"
    );

    assert!(
        common::nvim_bin_or_skip().is_some(),
        "the parent resolved a pinned nvim but this child cannot; is the env inherited?"
    );

    assert_eq!(
        state_dir(),
        Some(planted_root.clone()),
        "the wolf-ffi harness overwrote a WOLF_STATE_DIR the caller had set; it must only set \
         one when absent, or a test that inspects its own state dir loses it"
    );

    let cwd = tempfile::tempdir().expect("failed to create temp cwd directory");
    let handle = wolf_ffi_start(
        common::init_lua_path().to_string_lossy().into_owned(),
        cwd.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    let ignorecase = handle.ignorecase();
    handle.shutdown();
    drop(cwd);

    assert!(
        ignorecase,
        "the planted init_user.lua did not take effect even with Wolf pointed at it, so the \
         isolated payload's ignorecase assertion proves nothing"
    );
}
