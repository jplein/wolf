//! Integration test for the `parsers_user/` durable `:TSInstall` target (plan
//! §4.3 create/export + §4.5 nvim-treesitter activation): core creates
//! `<state_dir()>/parsers_user` and exports `WOLF_PARSERS_USER_DIR`; init.lua
//! sources the vendored nvim-treesitter's plugin file (registering
//! `:TSInstall`), passes the dir to `setup({ install_dir = … })`, and prepends
//! it to `'runtimepath'` AHEAD of the bundled parsers tree, so user-installed
//! parsers shadow bundled ones. Drives a REAL, hermetic nvim — no mocks.
//!
//! The vendored trees resolve Bazel-first (`WOLF_TEST_NVIM_TREESITTER_DIR` /
//! `WOLF_TEST_PARSERS_DIR`, staged runfiles), devShell-second (the exported
//! `WOLF_NVIM_TREESITTER_DIR` / `WOLF_PARSERS_DIR`), mirroring
//! `crates/wolf-ffi/tests/common/mod.rs`'s `*_dir_or_none` helpers. A host with
//! NEITHER panics loudly: a silent skip would read as a pass for the whole
//! activation path.
//!
//! Assertion ORDER is load-bearing (pre-flight caveat): the on-disk and rtp
//! assertions run BEFORE the `get_install_dir('parser')` probe, because that
//! accessor mkdirs `<install_dir>/parser/` and appends to rtp itself — probed
//! first, it would mask a missing `create_dir_all` or a missing prepend.

mod common;

use std::path::PathBuf;
use std::sync::Mutex;

/// Serializes env mutation (`WOLF_STATE_DIR` plus the vendored-tree vars) —
/// the `state_unit.rs` precedent, kept even for a single test so an added
/// sibling cannot silently race it.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Points `env_name` at the staged Bazel tree when `test_key` resolves
/// (Bazel-first: a stray devShell export inherited into a Bazel invocation must
/// never shadow the staged runfiles); otherwise requires an EXISTING devShell
/// export already in `env_name`. Panics when neither resolves.
fn stage_vendored_dir(test_key: &str, inner: &str, env_name: &str) {
    if let Some(root) = common::resolve_test_runfile(test_key) {
        // Bazel TreeArtifacts carry an inner directory (`nvim-treesitter/`,
        // `parsers/`); the devShell layout is the directory itself.
        let inner_dir = root.join(inner);
        let dir = if inner_dir.exists() { inner_dir } else { root };
        std::env::set_var(env_name, &dir);
        return;
    }
    let devshell_ok = std::env::var(env_name)
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .is_some_and(|p| p.exists());
    assert!(
        devshell_ok,
        "neither {test_key} (Bazel staged runfiles) nor {env_name} (devShell export) \
         resolves to an existing directory. This test needs the vendored \
         nvim-treesitter + parsers trees and refuses to skip silently — on such a host \
         a skip would read as a pass for the whole :TSInstall activation path"
    );
}

#[test]
fn parsers_user_dir_created_activated_and_first_on_rtp() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");

    let state = tempfile::tempdir().expect("create temp state dir");
    std::env::set_var("WOLF_STATE_DIR", state.path());
    stage_vendored_dir(
        "WOLF_TEST_NVIM_TREESITTER_DIR",
        "nvim-treesitter",
        "WOLF_NVIM_TREESITTER_DIR",
    );
    stage_vendored_dir("WOLF_TEST_PARSERS_DIR", "parsers", "WOLF_PARSERS_DIR");

    let (core, _waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // 1. On disk first: only spawn's create_dir_all can have made this dir, since the accessor's
    // mkdir has not run yet. init.lua's gate stats the exported path, so no env probe is needed.
    let parsers_user = state.path().join("parsers_user");
    assert!(
        parsers_user.is_dir(),
        "core must create <state_dir>/parsers_user before launch; a missing dir means \
         the WOLF_PARSERS_USER_DIR resolution or spawn's create_dir_all is gone (and \
         init.lua's fs_stat gate then skips the whole activation)"
    );

    // 2 + 3. rtp precedence and :TSInstall, probed before the accessor below, which appends
    // install_dir to rtp itself and would mask a missing prepend.
    let probe_dir = tempfile::tempdir().expect("create probe dir");
    let vals = common::probe_values(
        &core,
        &["&runtimepath", "exists(':TSInstall')"],
        &probe_dir.path().join("rtp.txt"),
    );
    let entries: Vec<&str> = vals[0].split(',').collect();
    let user_entry = parsers_user.to_string_lossy();
    let bundled = std::env::var("WOLF_PARSERS_DIR").expect("staged by stage_vendored_dir");
    let user_idx = entries
        .iter()
        .position(|e| *e == user_entry)
        .unwrap_or_else(|| {
            panic!(
                "<state_dir>/parsers_user must be ON 'runtimepath' (init.lua's explicit \
                 prepend after setup()); rtp = {}",
                vals[0]
            )
        });
    let bundled_idx = entries
        .iter()
        .position(|e| *e == bundled.as_str())
        .unwrap_or_else(|| {
            panic!(
                "precondition: the bundled parsers tree ({bundled}) must be on \
                 'runtimepath' (the vendored-tree gate) — without it the shadowing \
                 assertion below is untestable; rtp = {}",
                vals[0]
            )
        });
    assert!(
        user_idx < bundled_idx,
        "parsers_user (rtp index {user_idx}) must come STRICTLY BEFORE the bundled \
         parsers tree (rtp index {bundled_idx}), so a user-installed parser shadows the \
         bundled one for the same language; rtp = {}",
        vals[0]
    );
    assert_eq!(
        vals[1], "2",
        ":TSInstall must exist as a full command (exists() == 2) once the treesitter \
         gate ran with WOLF_PARSERS_USER_DIR set. Its absence means \
         plugin/nvim-treesitter.lua was neither runtime-sourced during init nor loaded \
         in the post-init plugin phase ('loadplugins' stays on under --clean) — e.g. \
         the vendored tree fell off 'runtimepath' or the gate never fired — so users \
         have no install entry point"
    );

    // 4. LAST: the plugin-internal accessor (it mkdirs <install_dir>/parser/
    //    and rtp-appends, which is why it must not run before 1 and 2).
    let vals = common::probe_values(
        &core,
        &[r#"luaeval("require('nvim-treesitter.config').get_install_dir('parser')")"#],
        &probe_dir.path().join("install_dir.txt"),
    );
    // Canonicalize both sides: the Lua side is vim.fs.normalize'd and macOS spells /var vs
    // /private/var. The accessor's own mkdir has created the dir by now.
    let got = std::fs::canonicalize(&vals[0]).unwrap_or_else(|e| {
        panic!(
            "canonicalizing the install dir nvim-treesitter reported ({:?}): {e} — the \
             accessor mkdirs its target, so a nonexistent path means it returned garbage",
            vals[0]
        )
    });
    let want = std::fs::canonicalize(&parsers_user)
        .expect("parsers_user exists (asserted above)")
        .join("parser");
    assert_eq!(
        got, want,
        "nvim-treesitter's install dir must be <state_dir>/parsers_user/parser. A \
         stdpath-default path here means setup() lost its install_dir argument, so \
         :TSInstall would compile parsers into the ephemeral XDG root and they would \
         die with the process"
    );

    core.shutdown().expect("shutdown should be Ok");
}
