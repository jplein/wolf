//! Guard: no wolf-gnome real-nvim test ever reaches the user's own Wolf state dir.
//!
//! A port of `crates/wolf-ffi/tests/state_isolation.rs`, which documents the design in
//! full. Every test that builds its `CoreConfig` through `common::config_in` must run
//! against a throwaway `WOLF_STATE_DIR`. Without one, [`state_dir`] resolves to
//! `$HOME/.local/state/wolf`, so the captive nvim sources the user's real
//! `runtime_user/init_user.lua` (`runtime/nvim/init.lua` loads it last) and the core
//! writes swap, MRU, spell and parser files beside it.
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
//!   `<home>/.local/state/wolf/runtime_user/init_user.lua` that sets `ignorecase`;
//! - `WOLF_STATE_DIR`, `XDG_STATE_HOME` and `XDG_DATA_HOME` removed, so the default
//!   under `HOME` is the only place Wolf can look;
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
//! ## Differences from the wolf-ffi guard
//!
//! - Linux only, and no skip: like every real-nvim test in this crate, the parent
//!   panics through `common::require_nvim_bin` when `WOLF_NVIM_BIN` is unset.
//! - The scratch home is made with `std` (`common::unique_temp_dir`) rather than
//!   `tempfile`: a new dev-dependency would move `Cargo.lock`, the repin obligation
//!   explained in `release_profile.rs`'s header.
//! - It drives `wolf_core::WolfCore` directly, through `common::config`.
//!
//! ## Premises, each measured rather than assumed
//!
//! - With no override, `state_dir()` derives from `$HOME`: `dirs` 5 reads `HOME` before
//!   falling back to `getpwuid`, and on Linux `dirs::state_dir()` is `$XDG_STATE_HOME`
//!   or `$HOME/.local/state`. [`isolated_payload`] asserts it before anything else.
//! - `init_user.lua` has been sourced by the time `ignorecase()` answers, and setting
//!   `vim.o.ignorecase` there is visible to it. [`control_payload`] asserts it.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use wolf_core::{state_dir, WolfCore};

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
/// with no override: `dirs::state_dir()/wolf`, i.e. `$HOME/.local/state/wolf` once
/// `XDG_STATE_HOME` is removed.
const PLATFORM_STATE_REL: &str = ".local/state/wolf";

/// A scratch directory removed, with everything under it, when dropped.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Creates a scratch home holding `<home>/<PLATFORM_STATE_REL>/runtime_user/init_user.lua`
/// with [`PLANT_BODY`]. Returns the home (deleted on drop) and the planted state root.
/// Never touches the real `HOME`.
fn plant_home() -> (ScratchDir, PathBuf) {
    let home = ScratchDir(common::unique_temp_dir("wolf-gnome-test-home"));
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

/// The scratch home a payload must check, or `None` when this process is not a child
/// of the parent test (the payload then returns without checking anything).
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

/// Starts a real core from `cfg`, waits for its first flush, and returns the captive
/// nvim's `ignorecase` before shutting the core down.
fn ignorecase_of_a_fresh_core(name: &str, cfg: wolf_core::CoreConfig) -> bool {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(cfg, wake).unwrap_or_else(|e| {
        panic!("{name}: WolfCore::start failed ({e}) — is WOLF_NVIM_BIN a valid nvim?")
    });
    waiter.wait_for_flush(common::FLUSH_TIMEOUT);
    let ignorecase = core.ignorecase();
    core.shutdown().expect("shutdown should be Ok");
    ignorecase
}

/// The parent: plants one scratch home per payload and requires both payloads to pass.
#[test]
fn a_planted_user_init_lua_never_reaches_a_test_core() {
    common::require_nvim_bin();

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

    let cfg = common::config(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    let resolved = state_dir();
    let isolated = resolved
        .as_deref()
        .is_some_and(|d| d != planted_root && !d.starts_with(&home));
    assert!(
        isolated,
        "the wolf-gnome harness left state_dir() at {resolved:?}; every real-nvim test reads \
         the user's init_user.lua and writes swap/MRU files into their state dir — see \
         common::config_in. (None means no state dir at all; a path under {} means it still \
         derives from HOME.)",
        home.display()
    );

    let ignorecase = ignorecase_of_a_fresh_core(ISOLATED_PAYLOAD, cfg);

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
         (swap/, *-mru.sqlite, spell/ or parsers_user/ there means the core still used the \
         default)",
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

    let cfg = common::config(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    assert_eq!(
        state_dir(),
        Some(planted_root.clone()),
        "the wolf-gnome harness overwrote a WOLF_STATE_DIR the caller had set; it must only \
         set one when absent, or a test that inspects its own state dir loses it"
    );

    let ignorecase = ignorecase_of_a_fresh_core(CONTROL_PAYLOAD, cfg);

    assert!(
        ignorecase,
        "the planted init_user.lua did not take effect even with Wolf pointed at it, so the \
         isolated payload's ignorecase assertion proves nothing"
    );
}
