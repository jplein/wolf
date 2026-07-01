//! Shared helpers for `wolf-ffi` bridge integration tests.
//!
//! These mirror `crates/wolf-core/tests/common/mod.rs`, but with one deliberate
//! policy difference: where the wolf-core suite PANICS when `WOLF_NVIM_BIN` is
//! unset (it is meaningless without a real nvim and CI always provides one), the
//! wolf-ffi bridge test must **loudly SKIP** instead. Bazel-on-macOS cannot yet
//! provision a pinned nvim, so this test must be a no-op (not a failure) there,
//! while still running on Linux/Cargo devShells where `WOLF_NVIM_BIN` is set.
//!
//! Resolution of nvim, init.lua, and the temp cwd otherwise follows the wolf-core
//! pattern exactly (real nvim, no mocks).

#![allow(dead_code)] // Not every helper is used by every test binary.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Default grid width used by the bridge smoke test, in columns.
pub const DEFAULT_COLS: u16 = 80;
/// Default grid height used by the bridge smoke test, in rows.
pub const DEFAULT_ROWS: u16 = 24;

/// Resolves a data file/dir the test needs at RUNTIME from a Bazel-injected env
/// var holding a runfiles-relative path (set via `$(rootpath …)` in the
/// `rust_test`'s `env`). Under Bazel the file lives in the test's runfiles tree,
/// whose root differs by Bazel version and (for external repos) carries a repo
/// segment — so we probe the known bases (`TEST_SRCDIR`, `+ TEST_WORKSPACE`,
/// `+ _main`, `RUNFILES_DIR`, the cwd) and return the first that exists.
///
/// Returns `None` when the env var is unset (the cargo / Linux-devShell path,
/// where callers fall back to `CARGO_MANIFEST_DIR` or an explicit `WOLF_NVIM_BIN`)
/// or when nothing resolves.
fn resolve_runfile(env_key: &str) -> Option<PathBuf> {
    let rel = std::env::var(env_key).ok().filter(|v| !v.is_empty())?;
    let relp = PathBuf::from(&rel);
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

/// Returns a usable pinned-nvim binary path, or `None` after printing a LOUD skip
/// banner to stderr.
///
/// Resolution order:
/// 1. An explicit `WOLF_NVIM_BIN` (the cargo / Linux-devShell path — wins so a
///    developer can point at a specific nvim).
/// 2. The Bazel-staged pinned Neovim tree (`WOLF_TEST_NVIM_DIR` → the
///    `//bazel/nvim:nvim_tree` runfiles dir), whose binary is `nvim/bin/nvim`
///    (the `replace_prefixes` inner `nvim/`), with `bin/nvim` as a fallback.
///
/// A caller that gets `None` must `return` from the test (a no-op pass), NOT panic
/// and NOT silently assert success. The banner makes the skip visible in
/// `--nocapture` output so a misconfigured env is never mistaken for real coverage.
#[must_use]
pub fn nvim_bin_or_skip() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("WOLF_NVIM_BIN") {
        if !v.trim().is_empty() {
            return Some(PathBuf::from(v));
        }
    }

    if let Some(dir) = resolve_runfile("WOLF_TEST_NVIM_DIR") {
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

    eprintln!(
        "\n========================================================================\n\
         SKIP: wolf-ffi bridge_smoke — no pinned Neovim available.\n\
         This test drives a REAL pinned Neovim through the FFI bridge (no mocks).\n\
         It runs when WOLF_NVIM_BIN is set (Linux/Cargo devShell) or when the\n\
         Bazel `rust_test` stages //bazel/nvim:nvim_tree via WOLF_TEST_NVIM_DIR.\n\
         Enter the devShell (`direnv allow`) or run via `bazel test` to run it.\n\
         ========================================================================\n"
    );
    None
}

/// Resolves the captive `runtime/nvim/init.lua`.
///
/// Prefers the Bazel-staged runfile (`WOLF_TEST_INIT_LUA` → `$(rootpath
/// //runtime/nvim:init.lua)`); under cargo it falls back to `CARGO_MANIFEST_DIR`
/// (which points at `crates/wolf-ffi`), i.e. `<manifest>/../../runtime/nvim/init.lua`.
///
/// This matters: under Bazel, `CARGO_MANIFEST_DIR` is an EPHEMERAL build-sandbox
/// path that does not exist at test runtime, so launching nvim with `-u <that>`
/// made nvim fail to read its init and block on a "Press ENTER" prompt forever
/// (hanging `WolfCore::start`). Resolving the staged runfile fixes that.
#[must_use]
pub fn init_lua_path() -> PathBuf {
    if let Some(p) = resolve_runfile("WOLF_TEST_INIT_LUA") {
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

/// Resolves the Bazel-staged vendored orlando plugin tree
/// (`WOLF_TEST_ORLANDO_DIR` → `$(rootpath //bazel/orlando:orlando_tree)`): the
/// `orlando_tree` TreeArtifact whose inner content is `orlando/` (i.e.
/// `<dir>/orlando/plugin/orlando.lua`).
///
/// Returns `None` when the env var is unset (the cargo / Linux path, where the
/// vendored plugin is not built at all) or when nothing resolves — so the
/// `orlando_load` test SKIPS gracefully rather than failing. Mirrors the
/// `*_or_none` / `*_or_skip` convention of the other runfile helpers and reuses
/// the same [`resolve_runfile`] mechanism as [`init_lua_path`] /
/// [`nvim_bin_or_skip`].
#[must_use]
pub fn orlando_dir_or_none() -> Option<PathBuf> {
    resolve_runfile("WOLF_TEST_ORLANDO_DIR")
}

/// Resolves the Bazel-staged vendored lighthouse colorscheme tree
/// (`WOLF_TEST_LIGHTHOUSE_DIR` → `$(rootpath //bazel/lighthouse:lighthouse_tree)`):
/// the `lighthouse_tree` TreeArtifact whose inner content is `lighthouse/` (i.e.
/// `<dir>/lighthouse/colors/lighthouse.lua`).
///
/// Returns `None` when the env var is unset (the cargo / Linux path, where the
/// vendored colorscheme is not built at all) or when nothing resolves — so the
/// `lighthouse_load` test SKIPS gracefully rather than failing. Mirrors the
/// `*_or_none` / `*_or_skip` convention of the other runfile helpers and reuses
/// the same [`resolve_runfile`] mechanism as [`init_lua_path`] /
/// [`nvim_bin_or_skip`].
#[must_use]
pub fn lighthouse_dir_or_none() -> Option<PathBuf> {
    resolve_runfile("WOLF_TEST_LIGHTHOUSE_DIR")
}

/// Recursively copies the directory tree at `src` into `dst` (creating `dst` and
/// any parent dirs), preserving the relative layout.
///
/// Used by the `orlando_load` test to stage a sibling `orlando/` next to a temp
/// `init.lua`, because the captive `init.lua` self-locates the plugin relative to
/// ITS OWN directory. Entries are classified with [`std::fs::metadata`] (which
/// FOLLOWS symlinks) rather than [`std::fs::DirEntry::file_type`], because under
/// Bazel the runfiles entries are symlinks into the output tree and would
/// otherwise be skipped; [`std::fs::copy`] likewise follows the source symlink to
/// copy the real file content.
pub fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        // metadata() follows symlinks so a symlinked dir is recursed into and a
        // symlinked file is copied by content (correct for Bazel runfiles).
        let meta = std::fs::metadata(&from)?;
        if meta.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Monotonic counter to disambiguate temp `cwd`s created within one process.
static CWD_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Creates a fresh, unique temp directory to serve as the core `cwd` and returns
/// its path. Mirrors wolf-core's `fresh_cwd` (intentionally NOT auto-deleted: the
/// MRU cache lives under `<cwd>/.wolf/` and must outlive `wolf_ffi_start`).
#[must_use]
pub fn fresh_cwd() -> PathBuf {
    let n = CWD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is before the unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("wolf-ffi-test-cwd-{pid}-{nanos}-{n}"));
    std::fs::create_dir_all(&dir).expect("failed to create temp cwd directory");
    dir
}
