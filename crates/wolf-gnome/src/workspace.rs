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
//!   (see [`crate::state_seam::restore_plans`]) gets its turn, which is the macOS
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
//!
//! ## The re-root rule
//!
//! [`reroot_decision`] is the TOTAL decision behind "where is the sidebar rooted,
//! and does this settle move it". Its authority is `rooted` — the directory the
//! sidebar is CURRENTLY rooted at — compared against `live`, the working directory
//! nvim last reported. A front-end request ([`reroot_ready`]'s `pending`) decides only whether
//! that request's flag may be discarded in the same settle; it never decides WHERE
//! to root, so a stuck request (a failed `:tcd`, which core degrades to a warning)
//! blocks nothing and a typed `:cd` moves the sidebar with no request at all.
//!
//! One `chdir` is exactly ONE re-root: while nvim has not applied it `live ==
//! rooted` and the answer is [`RerootAction::Stay`]; the settle that re-roots also
//! updates `rooted`, after which the same inputs answer `Stay` forever.
//!
//! **No project predicate is applied to a re-root.** [`is_usable_project`] guards a
//! launch-time GUESS about a directory the user did not choose; a typed `:cd /` is
//! an explicit act, macOS honours it, and `wolf_core::files::SearchBounds` is the
//! backstop. Do not re-derive the predicate here.

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
    // No project: start in the home directory when it really is one, else at the fallback; a
    // `$HOME` that is not a directory is no better a start than `/`.
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
    // A plain `==`: `Path` equality compares components, so a trailing separator is ignored, but it
    // resolves no `.`, `..` or symlinks, which is why callers canonicalize both sides.
    match (pending, live) {
        (Some(pending), Some(live)) => pending == live,
        _ => false,
    }
}

/// What one settle should do about the sidebar's root.
///
/// Returned by [`reroot_decision`]. The lifetime is the LIVE cwd's: a
/// [`RerootAction::Reroot`] always targets the directory nvim reported, never the
/// one a front-end request asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RerootAction<'a> {
    /// Do nothing at all: nvim has reported no directory, or the sidebar is
    /// already rooted where nvim is and no request has been satisfied.
    Stay,
    /// The sidebar is already rooted where nvim is and a front-end request for
    /// exactly that directory has been satisfied: discard the request, move
    /// nothing. This is what makes one `chdir` exactly one re-root.
    ClearPending,
    /// Re-root the sidebar at `target`.
    Reroot {
        /// Where to root: always the LIVE cwd.
        target: &'a Path,
        /// Whether the front-end request may be discarded in the same settle.
        clear_pending: bool,
    },
}

/// Decides what one settle does about the sidebar's root, from the front-end's
/// `pending` request, nvim's `live` working directory, and the directory the
/// sidebar is currently `rooted` at.
///
/// Three exhaustive rules, in order:
///
/// 1. `live` is `None` — nvim has reported no directory, so there is nowhere to
///    root: [`RerootAction::Stay`].
/// 2. `live == rooted` — the sidebar is already where nvim is:
///    [`RerootAction::ClearPending`] when [`reroot_ready`] holds, else
///    [`RerootAction::Stay`]. This arm is the ANTI-DOUBLE-RE-ROOT rule.
/// 3. otherwise (including `rooted == None`) — [`RerootAction::Reroot`] at `live`,
///    clearing the request iff [`reroot_ready`] holds.
///
/// All three paths canonicalize before calling, so the comparisons are plain
/// `==`; see the module docs.
#[must_use]
pub fn reroot_decision<'a>(
    pending: Option<&Path>,
    live: Option<&'a Path>,
    rooted: Option<&Path>,
) -> RerootAction<'a> {
    // Rule 1: silence is not a move.
    let Some(live) = live else {
        return RerootAction::Stay;
    };
    // The front-end request decides only whether its flag may be discarded — never
    // where to root.
    let satisfied = reroot_ready(pending, Some(live));
    if rooted == Some(live) {
        // Rule 2.
        if satisfied {
            RerootAction::ClearPending
        } else {
            RerootAction::Stay
        }
    } else {
        // Rule 3.
        RerootAction::Reroot {
            target: live,
            clear_pending: satisfied,
        }
    }
}

/// The text the sidebar's synthetic root row draws for `cwd`: its final component,
/// falling back to the whole path when there is none.
///
/// Total. `/` yields `/` and `""` yields `""`; an empty label means the caller
/// installs no root row, which production never reaches because `live_cwd` is
/// always absolute.
#[must_use]
pub fn root_label(cwd: &Path) -> String {
    cwd.file_name().map_or_else(
        || cwd.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
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

    // --- Paths for the launch-decision table: all absolute and distinct, so `==` is unambiguous.
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
            // === `restored: None` rows: the process cwd alone decides, as on every cage smoke's first launch.
            // --- The one Folder case, and its near misses ---
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
            // === The restored candidate: tried only once the process cwd is refused, by the same predicate.
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
                // The ordering rule: launching from a directory is the Linux idiom, and silently reopening
                // yesterday's project would surprise. macOS, which never reads a process cwd, is not mirrored.
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
                // `Path` equality compares components and a trailing separator adds none, so a plain `==`
                // covers this; do not hand-roll a byte comparison.
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

    // === reroot_decision ===
    // Three distinct absolute directories, named for the role each plays in a settle.

    /// Where the sidebar is already rooted in most rows.
    const DIR_A: &str = "/a";

    /// The directory nvim has moved to — the LIVE cwd in the re-rooting rows.
    const DIR_B: &str = "/b";

    /// A THIRD directory, distinct from both, so one row can tell "re-root at the
    /// LIVE cwd" apart from "re-root at the PENDING one" — a rule that re-rooted
    /// at `pending` would send the sidebar somewhere nvim is not.
    const DIR_C: &str = "/c";

    /// One `reroot_decision` table row.
    struct DecisionCase {
        desc: &'static str,
        pending: Option<&'static str>,
        live: Option<&'static str>,
        rooted: Option<&'static str>,
        expected: RerootAction<'static>,
    }

    /// The whole re-root rule as a table.
    ///
    /// Two distinguishable failure classes, and the message names the row so a
    /// failure identifies which:
    ///
    /// - A `Stay` where a `Reroot` was due means the sidebar never follows the
    ///   working directory nvim reports, so a typed `:cd` leaves it on the old
    ///   project — the behaviour this rule exists to replace.
    /// - A `Reroot` where a `Stay` or `ClearPending` was due means the sidebar
    ///   re-roots at a directory it is ALREADY rooted at, which drops the running
    ///   traversal and restarts it on every settle.
    ///
    /// Reverts that must redden it: (i) making the `live != rooted` rule return
    /// `Stay` when `pending.is_none()` fails rows 3 and 5; (ii) making the
    /// `live == rooted` rule return `Reroot` unconditionally fails rows 4 and 7;
    /// (iii) deleting the `ClearPending` arm fails row 9.
    #[test]
    fn reroot_decision_table() {
        let cases = [
            // --- nvim has reported no directory: there is nowhere to root ------
            DecisionCase {
                desc: "no live cwd and nothing rooted yet => nothing to do",
                pending: None,
                live: None,
                rooted: None,
                expected: RerootAction::Stay,
            },
            DecisionCase {
                desc: "no live cwd leaves an already-rooted sidebar alone; silence is not a move",
                pending: None,
                live: None,
                rooted: Some(DIR_A),
                expected: RerootAction::Stay,
            },
            // --- the live cwd differs from where the sidebar is rooted ---------
            DecisionCase {
                desc: "the first report with nothing rooted yet roots the sidebar",
                pending: None,
                live: Some(DIR_A),
                rooted: None,
                expected: RerootAction::Reroot {
                    target: Path::new(DIR_A),
                    clear_pending: false,
                },
            },
            DecisionCase {
                desc: "already rooted where nvim is, with no pending request => stay put",
                pending: None,
                live: Some(DIR_A),
                rooted: Some(DIR_A),
                expected: RerootAction::Stay,
            },
            DecisionCase {
                // THE TYPED `:cd`. No front-end request exists, so nothing but the
                // live cwd can drive this.
                desc: "a typed :cd moves the live cwd away from the root => re-root, nothing to \
                       clear",
                pending: None,
                live: Some(DIR_B),
                rooted: Some(DIR_A),
                expected: RerootAction::Reroot {
                    target: Path::new(DIR_B),
                    clear_pending: false,
                },
            },
            DecisionCase {
                // The Open Folder / Open Recent path, settling: nvim has applied
                // the request, so the SAME settle re-roots and clears the flag.
                desc: "a settled front-end request re-roots ONCE and clears its pending flag",
                pending: Some(DIR_B),
                live: Some(DIR_B),
                rooted: Some(DIR_A),
                expected: RerootAction::Reroot {
                    target: Path::new(DIR_B),
                    clear_pending: true,
                },
            },
            DecisionCase {
                // Anti-premature re-root: the request is out but nvim is still in the old directory, where the
                // sidebar already is; re-rooting now would re-root at the old cwd.
                desc: "a pending request nvim has not applied yet => stay at the old root",
                pending: Some(DIR_B),
                live: Some(DIR_A),
                rooted: Some(DIR_A),
                expected: RerootAction::Stay,
            },
            DecisionCase {
                // A stuck pending request must not decide where to root; the live cwd does. Rooting at
                // `pending` would show /b while nvim sits in /c.
                desc: "the target is the LIVE cwd, never the pending one, and the flag stays set",
                pending: Some(DIR_B),
                live: Some(DIR_C),
                rooted: Some(DIR_A),
                expected: RerootAction::Reroot {
                    target: Path::new(DIR_C),
                    clear_pending: false,
                },
            },
            DecisionCase {
                // The second settle after the row-6 re-root: `rooted` has caught up, so only the flag is
                // discarded. This makes one chdir exactly one re-root.
                desc: "a satisfied request against the directory already rooted just clears the \
                       flag",
                pending: Some(DIR_A),
                live: Some(DIR_A),
                rooted: Some(DIR_A),
                expected: RerootAction::ClearPending,
            },
            DecisionCase {
                desc: "a pending request with no live cwd at all cannot move anything",
                pending: Some(DIR_B),
                live: None,
                rooted: Some(DIR_A),
                expected: RerootAction::Stay,
            },
        ];

        for (index, case) in cases.iter().enumerate() {
            let pending = case.pending.map(Path::new);
            let live = case.live.map(Path::new);
            let rooted = case.rooted.map(Path::new);
            let got = reroot_decision(pending, live, rooted);
            assert_eq!(
                got,
                case.expected,
                "row {}: reroot_decision(pending = {:?}, live = {:?}, rooted = {:?}) must be \
                 {:?} but was {:?} — {}",
                index + 1,
                case.pending,
                case.live,
                case.rooted,
                case.expected,
                got,
                case.desc
            );
        }
    }

    // === root_label ===

    /// One `root_label` table row.
    struct LabelCase {
        desc: &'static str,
        cwd: &'static str,
        expected: &'static str,
    }

    /// The synthetic root row's displayed name, for every shape a cwd can take.
    ///
    /// Rows 1-3 are the discriminating ones: returning the whole path instead of
    /// the final component fails all three, while rows 4 and 5 stay green because
    /// there is no final component to take. That asymmetry is deliberate — the
    /// last two rows pin the TOTALITY of the fallback, not the basename rule.
    ///
    /// Revert that must redden it: returning `cwd.to_string_lossy()`
    /// unconditionally (dropping the `file_name()` half) fails rows 1, 2 and 3.
    #[test]
    fn root_label_table() {
        let cases = [
            LabelCase {
                desc: "a nested project directory is labelled by its final component alone",
                cwd: "/home/jane/proj",
                expected: "proj",
            },
            LabelCase {
                desc: "a top-level project directory is labelled the same way",
                cwd: "/proj",
                expected: "proj",
            },
            LabelCase {
                // `Path` compares and splits by COMPONENTS, and a trailing
                // separator contributes none, so no manual trimming is needed.
                desc: "a trailing separator contributes no component",
                cwd: "/a/b/",
                expected: "b",
            },
            LabelCase {
                // No final component, so the fallback answers with the whole path.
                desc: "the filesystem root has no basename, so the label is the path itself",
                cwd: "/",
                expected: "/",
            },
            LabelCase {
                // Also no final component: an empty label means the caller installs no root row; production
                // never reaches it since `live_cwd` is always absolute.
                desc: "the empty path yields an empty label rather than panicking",
                cwd: "",
                expected: "",
            },
        ];

        for (index, case) in cases.iter().enumerate() {
            let got = root_label(Path::new(case.cwd));
            assert_eq!(
                got,
                case.expected,
                "row {}: root_label({:?}) must be {:?} but was {:?} — {}. The label is the only \
                 text the synthetic root row draws, so a whole path here would put the user's \
                 home directory in the sidebar's first row.",
                index + 1,
                case.cwd,
                case.expected,
                got,
                case.desc
            );
        }
    }
}
