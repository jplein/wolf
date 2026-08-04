//! The pure, GTK-free WORKSPACE decision: is a project open, and where does the
//! captive Neovim start?
//!
//! The Linux rule is **process cwd wins, then the restored cwd**, under ONE
//! shared predicate. Both candidates are offered to [`launch_decision`], which
//! takes the first that [`is_usable_project`] accepts:
//!
//! - A terminal launch (`cargo run` from a project) yields a real project
//!   directory, which is a project — and it WINS over anything `state.json`
//!   remembers, because launching Wolf from a directory is the Linux idiom and
//!   silently reopening yesterday's project instead would be surprising.
//! - A desktop-launcher / `.desktop` launch yields `$HOME` or `/`, which is
//!   exactly "no project" — and THAT is when the cwd restored from `state.json`
//!   (see [`crate::state_seam::restore_plan`]) gets its turn, which is the macOS
//!   behaviour (`StateSeam.launchDecision` consults only the restored cwd).
//! - With neither usable, the empty state.
//!
//! Applying the SAME predicate to the restored cwd is deliberate: it means a
//! hand-edited `state.json` pointing at `/` can never make the sidebar's
//! `FileSearch` traverse the whole filesystem. Treating `$HOME` as a project
//! would point it at the whole home tree — the pathological case
//! `wolf_core::files::SearchBounds` already exists to prune.
//!
//! [`launch_decision`] takes `is_dir` as an injected predicate rather than calling
//! the filesystem, which is what makes the whole rule unit-testable headlessly
//! (no temp dirs, no real `$HOME`), exactly like `layout.rs` and `geometry.rs`.
//! The only impure function here is [`home_dir`], which reads `$HOME`.
//!
//! Callers CANONICALIZE both paths before calling, so every comparison below is a
//! plain `==`. `Path` equality compares COMPONENTS, so it already ignores a
//! trailing separator but does NOT resolve `.` / `..` / symlinks — canonicalizing
//! at the call site is what makes the comparison mean what it says.

use std::path::{Path, PathBuf};

/// The fallback nvim working directory when no home directory can be resolved.
pub const FALLBACK_CWD: &str = "/";

/// What the app should present at launch, and where the captive nvim starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchDecision {
    /// No project is open: show the empty state, keep the sidebar hidden, start
    /// no `FileSearch`, and launch the captive nvim at `nvim_cwd`.
    EmptyState {
        /// The working directory handed to the captive nvim even though no
        /// project is open: the home directory when it resolves to a real
        /// directory, else [`FALLBACK_CWD`].
        nvim_cwd: PathBuf,
    },
    /// A real project directory is open.
    Folder {
        /// The project directory: the process working directory, canonicalized by
        /// the caller.
        cwd: PathBuf,
    },
}

/// Decides whether the app launches into a project or into the empty state.
///
/// Tries `process_cwd` FIRST and `restored` (the cwd recovered from
/// `state.json`) SECOND, returning [`LaunchDecision::Folder`] for the first that
/// satisfies [`is_usable_project`] — it is a directory, it is not `home`, and it
/// is not [`FALLBACK_CWD`]. With neither usable it returns
/// [`LaunchDecision::EmptyState`], whose `nvim_cwd` is `home` when `home` is
/// `Some` and `is_dir(home)` holds, else [`FALLBACK_CWD`].
///
/// Passing `restored: None` reproduces the pre-restore behaviour EXACTLY, which
/// is why every historical table row still holds — the parameter is a strict
/// superset, not a change of rule.
///
/// `is_dir` is injected so the rule is pure and testable; production passes
/// `Path::is_dir`.
#[must_use]
pub fn launch_decision(
    process_cwd: Option<&Path>,
    restored: Option<&Path>,
    home: Option<&Path>,
    is_dir: impl Fn(&Path) -> bool,
) -> LaunchDecision {
    // Order is the rule: the process cwd is the terminal-launch idiom and wins;
    // the restored cwd is what a desktop-launcher start falls back to.
    for candidate in [process_cwd, restored].into_iter().flatten() {
        if is_usable_project(candidate, home, &is_dir) {
            return LaunchDecision::Folder {
                cwd: candidate.to_path_buf(),
            };
        }
    }
    // No project: start nvim in the home directory when it really is one, else at
    // the fallback. `$HOME` pointing at a non-directory is no better a place to
    // start than `/`.
    let nvim_cwd = home
        .filter(|home| is_dir(home))
        .map_or_else(|| PathBuf::from(FALLBACK_CWD), Path::to_path_buf);
    LaunchDecision::EmptyState { nvim_cwd }
}

/// Whether `path` is a usable project root: a real directory that is neither the
/// home directory (the desktop-launcher shape) nor the filesystem root.
///
/// ONE predicate applied to BOTH candidates, so a restored cwd can never open a
/// project the process cwd would have been refused for — in particular a
/// hand-edited `state.json` naming `/` cannot point the sidebar's `FileSearch` at
/// the whole filesystem.
fn is_usable_project(path: &Path, home: Option<&Path>, is_dir: &impl Fn(&Path) -> bool) -> bool {
    is_dir(path) && Some(path) != home && path != Path::new(FALLBACK_CWD)
}

/// Whether a pending re-root for `pending` is satisfied by the core's live cwd.
///
/// `false` unless BOTH are `Some` and equal. A front-end directory change — the
/// Open Folder dialog or an Open Recent palette row — sets `pending` and then
/// waits: `start_file_search` resolves its root from `live_cwd()`, which only
/// updates once nvim reports `DirChanged`, so re-rooting the sidebar before this
/// returns `true` would re-root it at the OLD cwd.
#[must_use]
pub fn reroot_ready(pending: Option<&Path>, live: Option<&Path>) -> bool {
    // A plain `==`: `Path: PartialEq` compares COMPONENTS, so a trailing separator
    // is already ignored. It does NOT resolve `.` / `..` / symlinks, which is why
    // callers canonicalize both sides first.
    match (pending, live) {
        (Some(pending), Some(live)) => pending == live,
        _ => false,
    }
}

/// The canonicalized `$HOME`, or `None` when it is unset, empty, or does not
/// canonicalize.
///
/// The one impure function in this module; read with `std::env::var_os` rather
/// than adding a `dirs` dependency to `wolf-gnome`.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    let raw = std::env::var_os("HOME")?;
    if raw.is_empty() {
        return None;
    }
    std::fs::canonicalize(raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Paths used across the launch-decision table ------------------------
    //
    // All absolute and all distinct, so `==` comparisons are unambiguous.
    /// A real project directory (the terminal-launch case).
    const PROJECT: &str = "/projects/wolf";
    /// The user's home directory (the desktop-launch case).
    const HOME: &str = "/home/tester";
    /// The filesystem root, which `launch_decision` never treats as a project.
    const ROOT: &str = "/";
    /// A path that does NOT exist as a directory (the injected `is_dir` says no).
    const MISSING: &str = "/projects/deleted";
    /// A second real project directory, used as the cwd recovered from `state.json`
    /// so it is always distinguishable from [`PROJECT`].
    const RESTORED: &str = "/projects/restored";

    /// What a [`Case`] expects, as the path each `LaunchDecision` variant carries.
    enum Want {
        /// `LaunchDecision::Folder { cwd }`.
        Folder(&'static str),
        /// `LaunchDecision::EmptyState { nvim_cwd }`.
        EmptyState(&'static str),
    }

    /// One `launch_decision` table row: a description, the three inputs, the set of
    /// paths the injected `is_dir` reports as directories, and the expectation.
    ///
    /// `restored` is the cwd recovered from `state.json`. Every row that predates the
    /// state-persistence unit carries `restored: None`, which is the COMPATIBILITY
    /// PROOF made executable: the parameter is a strict superset of the old rule, so
    /// each historical expectation must still hold unchanged.
    struct Case {
        desc: &'static str,
        process_cwd: Option<&'static str>,
        restored: Option<&'static str>,
        home: Option<&'static str>,
        dirs: &'static [&'static str],
        want: Want,
    }

    /// Materializes a [`Want`] into the [`LaunchDecision`] it describes.
    fn expected_decision(want: &Want) -> LaunchDecision {
        match *want {
            Want::Folder(cwd) => LaunchDecision::Folder {
                cwd: PathBuf::from(cwd),
            },
            Want::EmptyState(nvim_cwd) => LaunchDecision::EmptyState {
                nvim_cwd: PathBuf::from(nvim_cwd),
            },
        }
    }

    #[test]
    fn launch_decision_table() {
        let cases = [
            // =============================================================
            // The PRE-RESTORE rows, re-run verbatim with `restored: None`.
            //
            // Their expectations are UNCHANGED, which is the whole compatibility
            // argument for adding the parameter: with nothing saved (or nothing
            // usable saved) `launch_decision` must behave exactly as it always did,
            // and every existing cage smoke — all of which now launch with an empty
            // state dir — must therefore see the same first launch as before.
            // =============================================================
            // --- the one Folder case, and its near misses --------------------
            Case {
                desc: "a real project dir that is neither home nor / is a project",
                process_cwd: Some(PROJECT),
                restored: None,
                home: Some(HOME),
                dirs: &[PROJECT, HOME],
                want: Want::Folder(PROJECT),
            },
            Case {
                // A desktop-launcher start lands here: $HOME is not a project, and
                // treating it as one would point FileSearch at the whole home tree.
                desc: "cwd == home is NOT a project; nvim still starts in home",
                process_cwd: Some(HOME),
                restored: None,
                home: Some(HOME),
                dirs: &[HOME],
                want: Want::EmptyState(HOME),
            },
            Case {
                desc: "cwd == / is NOT a project; nvim starts in home",
                process_cwd: Some(ROOT),
                restored: None,
                home: Some(HOME),
                dirs: &[ROOT, HOME],
                want: Want::EmptyState(HOME),
            },
            Case {
                desc: "no process cwd at all => empty state at home",
                process_cwd: None,
                restored: None,
                home: Some(HOME),
                dirs: &[HOME],
                want: Want::EmptyState(HOME),
            },
            Case {
                // The injected is_dir reports false for MISSING (it is absent from
                // `dirs`), e.g. the launch directory was deleted underneath us.
                desc: "cwd is not a directory => empty state at home",
                process_cwd: Some(MISSING),
                restored: None,
                home: Some(HOME),
                dirs: &[HOME],
                want: Want::EmptyState(HOME),
            },
            // --- home unresolvable -------------------------------------------
            Case {
                desc: "home unknown but cwd is a real project => still a project",
                process_cwd: Some(PROJECT),
                restored: None,
                home: None,
                dirs: &[PROJECT],
                want: Want::Folder(PROJECT),
            },
            Case {
                desc: "home unknown and cwd unusable => empty state at FALLBACK_CWD",
                process_cwd: Some(MISSING),
                restored: None,
                home: None,
                dirs: &[],
                want: Want::EmptyState(FALLBACK_CWD),
            },
            Case {
                // $HOME is set but points at something that is not a directory, so
                // it is no better a place to start nvim than the fallback.
                desc: "home set but not a directory => empty state at FALLBACK_CWD",
                process_cwd: None,
                restored: None,
                home: Some(HOME),
                dirs: &[],
                want: Want::EmptyState(FALLBACK_CWD),
            },
            // =============================================================
            // The RESTORED candidate. It gets its turn only after the process cwd
            // has been refused, and it faces the SAME `is_usable_project` predicate.
            // =============================================================
            Case {
                // The desktop-launcher shape, which is the case macOS handles too:
                // $HOME is not a project, so the saved project opens.
                desc: "process cwd is home => the RESTORED project opens",
                process_cwd: Some(HOME),
                restored: Some(RESTORED),
                home: Some(HOME),
                dirs: &[HOME, RESTORED],
                want: Want::Folder(RESTORED),
            },
            Case {
                desc: "process cwd is / => the RESTORED project opens",
                process_cwd: Some(ROOT),
                restored: Some(RESTORED),
                home: Some(HOME),
                dirs: &[ROOT, HOME, RESTORED],
                want: Want::Folder(RESTORED),
            },
            Case {
                desc: "no process cwd at all => the RESTORED project opens",
                process_cwd: None,
                restored: Some(RESTORED),
                home: Some(HOME),
                dirs: &[HOME, RESTORED],
                want: Want::Folder(RESTORED),
            },
            Case {
                desc: "process cwd is not a directory => the RESTORED project opens",
                process_cwd: Some(MISSING),
                restored: Some(RESTORED),
                home: Some(HOME),
                dirs: &[HOME, RESTORED],
                want: Want::Folder(RESTORED),
            },
            Case {
                // THE ORDERING RULE. Launching Wolf from a directory is the Linux
                // idiom; silently reopening yesterday's project instead would be
                // surprising, and macOS (which never consults a process cwd) is
                // deliberately NOT mirrored here.
                desc: "a usable process cwd WINS over an equally usable restored cwd",
                process_cwd: Some(PROJECT),
                restored: Some(RESTORED),
                home: Some(HOME),
                dirs: &[PROJECT, RESTORED, HOME],
                want: Want::Folder(PROJECT),
            },
            Case {
                // The saved project was deleted between runs.
                desc: "restored cwd is not a directory => empty state at home",
                process_cwd: Some(HOME),
                restored: Some(MISSING),
                home: Some(HOME),
                dirs: &[HOME],
                want: Want::EmptyState(HOME),
            },
            Case {
                // The SHARED-PREDICATE rule: a restored cwd can never open a project
                // the process cwd would have been refused for.
                desc: "restored cwd == home is refused by the same predicate",
                process_cwd: None,
                restored: Some(HOME),
                home: Some(HOME),
                dirs: &[HOME],
                want: Want::EmptyState(HOME),
            },
            Case {
                // A hand-edited `state.json` naming `/` must NOT be able to point the
                // sidebar's FileSearch at the whole filesystem.
                desc: "restored cwd == / is refused by the same predicate",
                process_cwd: None,
                restored: Some(ROOT),
                home: Some(HOME),
                dirs: &[ROOT, HOME],
                want: Want::EmptyState(HOME),
            },
            Case {
                desc: "restored used with home unknown => still a project",
                process_cwd: None,
                restored: Some(RESTORED),
                home: None,
                dirs: &[RESTORED],
                want: Want::Folder(RESTORED),
            },
        ];

        for case in &cases {
            let process_cwd = case.process_cwd.map(Path::new);
            let restored = case.restored.map(Path::new);
            let home = case.home.map(Path::new);
            let is_dir = |p: &Path| case.dirs.iter().any(|dir| Path::new(dir) == p);
            let got = launch_decision(process_cwd, restored, home, is_dir);
            let want = expected_decision(&case.want);
            assert_eq!(got, want, "case: {}", case.desc);
        }
    }

    /// One `reroot_ready` table row.
    struct RerootCase {
        desc: &'static str,
        pending: Option<&'static str>,
        live: Option<&'static str>,
        expected: bool,
    }

    #[test]
    fn reroot_ready_table() {
        let cases = [
            // --- nothing to settle -------------------------------------------
            RerootCase {
                desc: "no pending re-root => not ready",
                pending: None,
                live: Some(PROJECT),
                expected: false,
            },
            RerootCase {
                desc: "core has no live cwd yet => not ready",
                pending: Some(PROJECT),
                live: None,
                expected: false,
            },
            RerootCase {
                desc: "neither side known => not ready",
                pending: None,
                live: None,
                expected: false,
            },
            // --- the settle ---------------------------------------------------
            RerootCase {
                desc: "pending == live => ready",
                pending: Some(PROJECT),
                live: Some(PROJECT),
                expected: true,
            },
            RerootCase {
                // nvim has not applied the :tcd yet: still the OLD cwd.
                desc: "live is still the OLD cwd => not ready",
                pending: Some(PROJECT),
                live: Some(HOME),
                expected: false,
            },
            // --- what `Path` equality does and does not normalize -------------
            RerootCase {
                // NOTE to coder: `Path: PartialEq` compares COMPONENTS, and a
                // trailing separator contributes no component, so "/projects/wolf/"
                // and "/projects/wolf" ARE equal. This row documents that a plain
                // `==` is enough for the trailing-slash case and must not be
                // hand-rolled into a byte comparison.
                desc: "a trailing separator is not a difference (Path compares components)",
                pending: Some("/projects/wolf/"),
                live: Some(PROJECT),
                expected: true,
            },
            RerootCase {
                // ...but `..` is NOT resolved by `Path` equality, which is exactly
                // why the callers canonicalize before calling.
                desc: "an unnormalized `..` IS a difference (callers must canonicalize)",
                pending: Some("/projects/wolf/../wolf"),
                live: Some(PROJECT),
                expected: false,
            },
        ];

        for case in &cases {
            let pending = case.pending.map(Path::new);
            let live = case.live.map(Path::new);
            let got = reroot_ready(pending, live);
            assert_eq!(got, case.expected, "case: {}", case.desc);
        }
    }
}
