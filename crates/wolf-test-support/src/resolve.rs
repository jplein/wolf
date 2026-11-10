//! Resolution of the pinned nvim and rg binaries, the captive `init.lua`, and Bazel runfiles.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Resolves `value`, an absolute path or a runfiles-relative one, to an existing path.
///
/// An absolute `value` is returned when it exists. A relative one is a Bazel runfiles path, set
/// via `$(rootpath …)` in a `rust_test`'s `env`. The runfiles root differs by Bazel version and,
/// for external repos, carries a repo segment, so this probes the known bases (`TEST_SRCDIR`,
/// `+ TEST_WORKSPACE`, `+ _main`, `RUNFILES_DIR`, the cwd) and returns the first that exists.
///
/// Returns `None` when nothing exists.
#[must_use]
pub fn resolve_runfile_value(value: &str) -> Option<PathBuf> {
    let relp = PathBuf::from(value);
    if relp.is_absolute() {
        return relp.exists().then_some(relp);
    }

    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(srcdir) = std::env::var("TEST_SRCDIR") {
        if let Ok(wsp) = std::env::var("TEST_WORKSPACE") {
            bases.push(PathBuf::from(&srcdir).join(&wsp));
        }
        bases.push(PathBuf::from(&srcdir).join("_main"));
        bases.push(PathBuf::from(&srcdir));
    }
    if let Ok(rfdir) = std::env::var("RUNFILES_DIR") {
        bases.push(PathBuf::from(&rfdir).join("_main"));
        bases.push(PathBuf::from(&rfdir));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }

    bases
        .into_iter()
        .map(|b| b.join(&relp))
        .find(|c| c.exists())
}

/// Resolves the non-empty value of the variable `env_key` through [`resolve_runfile_value`].
///
/// Returns `None` when the variable is unset or empty (the cargo / Linux-devShell path, where
/// callers fall back to `CARGO_MANIFEST_DIR` or another variable) or when nothing resolves.
#[must_use]
pub fn resolve_test_runfile(env_key: &str) -> Option<PathBuf> {
    let value = std::env::var(env_key).ok().filter(|v| !v.is_empty())?;
    resolve_runfile_value(&value)
}

/// Resolves a usable pinned-nvim binary as an absolute path, with no side effects:
///
/// 1. An explicit `WOLF_NVIM_BIN` that is non-empty, absolute, and exists: the cargo /
///    Linux-devShell path. It wins so a developer can point at a specific nvim. A relative or
///    missing value is not trusted, because the core spawns nvim with `current_dir(cfg.cwd)`, a
///    temp dir, where a relative path fails to spawn; it falls through to the staged tree.
/// 2. The Bazel-staged pinned Neovim tree (`WOLF_TEST_NVIM_DIR` → the `//bazel/nvim:nvim_tree`
///    runfiles dir), whose binary is `nvim/bin/nvim` (the `replace_prefixes` inner `nvim/`),
///    with `bin/nvim` as a fallback.
///
/// Returns `None` when neither is available.
fn resolve_nvim_bin() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("WOLF_NVIM_BIN") {
        let p = PathBuf::from(v.trim());
        if p.is_absolute() && p.exists() {
            return Some(p);
        }
    }

    if let Some(dir) = resolve_test_runfile("WOLF_TEST_NVIM_DIR") {
        // The tree is `nvim_tree/nvim/bin/nvim` (the `replace_prefixes` inner
        // `nvim/`); accept a flat `bin/nvim` too, defensively.
        let candidates = [
            dir.join("nvim").join("bin").join("nvim"),
            dir.join("bin").join("nvim"),
        ];
        if let Some(bin) = candidates.into_iter().find(|b| b.exists()) {
            return Some(bin);
        }
    }

    None
}

/// Caches the pinned-nvim resolution for the whole test binary, so `WOLF_NVIM_BIN` is written at
/// most once and every reader is ordered after that write.
///
/// `setenv(3)` is not safe against a concurrent `getenv(3)`: glibc may `realloc` the `environ`
/// array, so the failure mode is a rare use-after-free rather than a wrong value, which is why
/// edition 2024 makes [`std::env::set_var`] `unsafe`. `cargo test` runs a binary's `#[test]`s on
/// parallel threads and several helpers read the environment, so a per-test write would race its
/// neighbours' reads. [`OnceLock::get_or_init`] collapses it to one write that every caller of
/// [`require_nvim_bin`] and [`nvim_bin_or_skip`] happens-after.
static NVIM_BIN: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Resolves the pinned nvim and publishes it in `WOLF_NVIM_BIN`. Runs exactly once per test
/// binary, behind [`NVIM_BIN`].
///
/// The publish is what the Bazel path needs: there the binary comes out of the staged
/// `WOLF_TEST_NVIM_DIR` tree, while wolf-core's `NvimConfig::from_env` looks only at
/// `WOLF_NVIM_BIN`. On the cargo / devShell path the variable already holds exactly the resolved
/// path, so nothing is written at all.
fn resolve_and_publish_nvim_bin() -> Option<PathBuf> {
    let bin = resolve_nvim_bin()?;
    if std::env::var_os("WOLF_NVIM_BIN").as_deref() != Some(bin.as_os_str()) {
        std::env::set_var("WOLF_NVIM_BIN", &bin);
    }
    Some(bin)
}

/// Returns the pinned nvim binary, published once per binary in `WOLF_NVIM_BIN`. Panics
/// with an actionable message when none resolves.
///
/// For test bodies that handle no skip. On both real environments (the cargo devShell and the
/// Bazel host) resolution succeeds, so the panic never fires there.
pub fn require_nvim_bin() -> PathBuf {
    match NVIM_BIN.get_or_init(resolve_and_publish_nvim_bin) {
        Some(bin) => bin.clone(),
        None => panic!(
            "No pinned Neovim could be resolved. These integration tests require a \
             real, pinned Neovim binary. Provide one of:\n\
             - an ABSOLUTE, existing WOLF_NVIM_BIN (exported by the flake devShell; \
             enter it via `direnv allow`), or\n\
             - the Bazel-staged tree via WOLF_TEST_NVIM_DIR (run the test through \
             `bazel test`).\n\
             nvim is NOT taken from PATH."
        ),
    }
}

/// Returns the pinned nvim binary, published once per binary in `WOLF_NVIM_BIN`, or
/// `None` after printing a loud skip banner to stderr.
///
/// The publish is how a caller that goes through `wolf_ffi_start` gets the Bazel-staged binary
/// launched: the bridge offers no per-spawn environment and reads that one variable.
///
/// It is also a state-dir funnel: it points `WOLF_STATE_DIR` at a throwaway dir first, as
/// [`crate::config_in`] does, so a test that starts a core without a `CoreConfig` from this crate
/// calls it before starting, even when it already has a binary.
///
/// A caller that gets `None` must `return` from the test (a no-op pass), never panic and never
/// assert success. The banner makes the skip visible in `--nocapture` output so a misconfigured
/// environment is never mistaken for coverage, and it is printed on every skipping call.
#[must_use]
pub fn nvim_bin_or_skip() -> Option<PathBuf> {
    crate::core_config::ensure_hermetic_state_dir();
    if let Some(bin) = NVIM_BIN.get_or_init(resolve_and_publish_nvim_bin) {
        return Some(bin.clone());
    }

    eprintln!(
        "\n========================================================================\n\
         SKIP: no pinned Neovim available.\n\
         This test drives a REAL pinned Neovim (no mocks).\n\
         It runs when WOLF_NVIM_BIN is set (Linux/Cargo devShell) or when the\n\
         Bazel `rust_test` stages //bazel/nvim:nvim_tree via WOLF_TEST_NVIM_DIR.\n\
         Enter the devShell (`direnv allow`) or run via `bazel test` to run it.\n\
         ========================================================================\n"
    );
    None
}

/// Resolves the pinned `rg` to an absolute path and publishes it in `WOLF_RG_BIN`, or prints a
/// loud skip banner and returns `None`.
///
/// `WOLF_RG_BIN` is absolute on the cargo / Nix devShell path, where the flake exports
/// `pkgs.ripgrep`, and runfiles-relative under the Bazel `rust_test`, which sets it to
/// `$(rootpath //bazel/rg:rg_tree)/rg/rg`. Either goes through [`resolve_runfile_value`]. The
/// publish makes the core spawn the right binary whatever its cwd. A caller that gets `None`
/// must `return` (a no-op pass), as with [`nvim_bin_or_skip`].
#[must_use]
pub fn rg_bin_or_skip() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("WOLF_RG_BIN") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            if let Some(bin) = resolve_runfile_value(trimmed) {
                std::env::set_var("WOLF_RG_BIN", &bin);
                return Some(bin);
            }
        }
    }
    eprintln!(
        "\n========================================================================\n\
         SKIP: no pinned ripgrep (rg) available.\n\
         This test drives a REAL pinned rg subprocess (no mocks). It runs when\n\
         WOLF_RG_BIN is an absolute path (Nix/Cargo devShell) or when the Bazel\n\
         `rust_test` stages //bazel/rg:rg_tree via a runfiles-relative WOLF_RG_BIN.\n\
         ========================================================================\n"
    );
    None
}

/// Resolves the captive `runtime/nvim/init.lua`.
///
/// Prefers the Bazel-staged runfile (`WOLF_TEST_INIT_LUA` → `$(rootpath
/// //runtime/nvim:init.lua)`, resolved to an absolute path); under cargo it falls back to this
/// crate's `CARGO_MANIFEST_DIR`, i.e. `<manifest>/../../runtime/nvim/init.lua`.
///
/// This matters: under Bazel, `CARGO_MANIFEST_DIR` is an ephemeral build-sandbox path that does
/// not exist at test runtime, so launching nvim with `-u <that>` makes nvim fail to read its init
/// and block on a "Press ENTER" prompt forever, hanging `WolfCore::start`.
#[must_use]
pub fn init_lua_path() -> PathBuf {
    if let Some(p) = resolve_test_runfile("WOLF_TEST_INIT_LUA") {
        return p;
    }
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("runtime")
        .join("nvim")
        .join("init.lua")
}
