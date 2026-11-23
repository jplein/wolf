//! The platform state directory and the directories Wolf keeps under it.
//!
//! [`state_dir`] resolves the per-platform application-state directory (macOS
//! `~/Library/Application Support/Wolf`, Linux `~/.local/state/wolf`), which holds
//! `wolf.sqlite` ([`crate::db`]). The other resolvers name the per-project swap
//! directories, the global spell directory and the user override and parser
//! directories under it, and [`seed_runtime_user_dir`] seeds the override directory
//! once.

use std::path::{Path, PathBuf};

/// Resolves the per-platform application-state directory that holds
/// `wolf.sqlite`.
///
/// The `WOLF_STATE_DIR` environment variable overrides everything (a
/// test/determinism hook); otherwise it is macOS
/// `~/Library/Application Support/Wolf` (via [`dirs::data_dir`]) or Linux
/// `~/.local/state/wolf` (via [`dirs::state_dir`], falling back to
/// [`dirs::data_dir`]). Returns `None` only when the platform dir cannot be
/// resolved (no override and no home directory).
#[must_use]
pub fn state_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("WOLF_STATE_DIR") {
        return Some(PathBuf::from(dir));
    }
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir().map(|d| d.join("Wolf"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::state_dir()
            .or_else(dirs::data_dir)
            .map(|d| d.join("wolf"))
    }
}

/// Resolves the durable, Wolf-owned, per-project swap directory for `cwd`.
///
/// Returns `state_dir()/swap/<stable-hex-hash-of-cwd>` — a STABLE, DETERMINISTIC
/// path derived from the (assumed already-canonical) project `cwd`, so the SAME
/// project reuses the SAME swap dir across respawns AND across separate Wolf
/// processes (letting a fresh nvim find a dead nvim's swap), while DISTINCT
/// projects get distinct dirs. The hash is a plain [`std::hash::DefaultHasher`]
/// rendered as hex — not cryptographic, just a stable, collision-unlikely name.
///
/// `None` only when [`state_dir`] cannot be resolved (no `WOLF_STATE_DIR`
/// override and no home directory); [`crate::api::WolfCore::start`] then falls
/// back to an ephemeral swap dir under the nvim state root (documented, no panic).
#[must_use]
pub fn swap_dir_for(cwd: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cwd.hash(&mut hasher);
    let name = format!("{:016x}", hasher.finish());
    state_dir().map(|d| d.join("swap").join(name))
}

/// Resolves the durable, GLOBAL spell directory: `state_dir()/spell`.
///
/// Deliberately NOT per-project (unlike [`swap_dir_for`]): a word the user
/// teaches Wolf via `zg` belongs to the language, not to the open folder, so
/// every project shares one dictionary. init.lua points `'spellfile'` at
/// `<dir>/en.utf-8.add` via the `WOLF_SPELL_DIR` env var.
///
/// `None` only when [`state_dir`] cannot be resolved (no `WOLF_STATE_DIR`
/// override and no home directory); [`crate::nvim::NvimConfig::from_env`] then
/// falls back to an ephemeral `state_root/spell` (documented — `zg` still
/// works, the words just die with the process).
#[must_use]
pub fn spell_dir() -> Option<PathBuf> {
    state_dir().map(|d| d.join("spell"))
}

/// Resolves the user override directory: `state_dir()/runtime_user`.
///
/// Seeded once by [`seed_runtime_user_dir`]; init.lua sources
/// `<dir>/init_user.lua` LAST (via the `WOLF_RUNTIME_USER_DIR` env var), so
/// anything in it overrides Wolf's defaults. `None` only when [`state_dir`]
/// cannot be resolved; the feature is then skipped entirely.
#[must_use]
pub fn runtime_user_dir() -> Option<PathBuf> {
    state_dir().map(|d| d.join("runtime_user"))
}

/// Resolves the durable user parser directory: `state_dir()/parsers_user`.
///
/// The `:TSInstall` target (`install_dir`) of the vendored nvim-treesitter,
/// handed to init.lua via the `WOLF_PARSERS_USER_DIR` env var; user-installed
/// parsers there shadow the bundled ones. `None` only when [`state_dir`]
/// cannot be resolved; the feature is then skipped entirely.
#[must_use]
pub fn parsers_user_dir() -> Option<PathBuf> {
    state_dir().map(|d| d.join("parsers_user"))
}

/// The file name init.lua sources from the runtime_user dir, and the name
/// [`seed_runtime_user_dir`] seeds.
pub const INIT_USER_FILE_NAME: &str = "init_user.lua";

/// The comment-only template [`seed_runtime_user_dir`] writes: sourcing it can
/// break nothing, and it tells the user what the file is for.
const INIT_USER_TEMPLATE: &str = "\
-- This file is yours. Wolf seeds it once and never overwrites it after the
-- directory has anything in it.
--
-- Wolf loads this file last, so anything here overrides Wolf's defaults.
--
-- Errors in this file are reported as warnings (see :messages); they never
-- stop Wolf from starting.
";

/// Seeds `dir` with the `init_user.lua` template — ONCE.
///
/// This is the sole exception to "Wolf never writes into the user's state-dir
/// subtrees", and it fires only while the directory is provably untouched:
///
/// - `dir` missing: create it (`create_dir_all`), write the template,
///   `Ok(true)`.
/// - `dir` exists with ZERO entries: write the template, `Ok(true)`.
/// - `dir` non-empty: touch NOTHING — even when `init_user.lua` itself is
///   absent (the user may have deleted the template on purpose; a non-empty
///   dir is theirs) — `Ok(false)`.
pub fn seed_runtime_user_dir(dir: &Path) -> std::io::Result<bool> {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Ok(false);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(dir)?;
        }
        Err(e) => return Err(e),
    }
    std::fs::write(dir.join(INIT_USER_FILE_NAME), INIT_USER_TEMPLATE)?;
    Ok(true)
}
