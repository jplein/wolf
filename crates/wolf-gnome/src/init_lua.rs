//! Where the captive `runtime/nvim/init.lua` lives, as a function of values.
//!
//! A pure seam in the shape of [`crate::config_seam`]: GTK-free, environment-free
//! and filesystem-free. The environment read and the `current_exe()` read belong to
//! `app.rs`; the RULE belongs here, and the existence probe is injected as a
//! predicate exactly as [`crate::palette::open_recent_candidates`] injects its
//! `is_dir`. That is what makes all three branches decidable with no temp
//! directory, no process environment and no filesystem race — which a parallel test
//! suite cannot safely arrange in any case.
//!
//! ## The three branches, in order, total, never panicking
//!
//! 1. The `WOLF_INIT_LUA` override ([`INIT_LUA_ENV`]), when it is `Some` and not
//!    blank.
//! 2. The installed tree beside the executable, `<exe dir>` joined with
//!    [`INSTALLED_INIT_LUA_RELATIVE`], when the injected predicate says it is there.
//! 3. The build-tree fallback the caller supplies, unchanged.
//!
//! ## The asymmetry between branches 1 and 2 is the point
//!
//! Branch 1 is returned WITHOUT asking whether the path exists; branch 2 is
//! returned only if it does. That is deliberate on both sides.
//!
//! An override the user typed is an INSTRUCTION, not a candidate. Existence-checking
//! it would make a typo fall silently through to the installed file, so the
//! application would look perfectly healthy while ignoring the override entirely.
//! What replaces that is loud: Neovim launched with a `-u` naming a missing file
//! never attaches, `WolfCore`'s start fails, and the app prints `failed to start
//! core` and opens no window. Loud over silent is this repository's rule, and macOS
//! does not check either (`Project.swift::initLuaPath()`), so it is parity as well.
//!
//! Branch 2 IS a guess the binary makes about its own surroundings, and it must fall
//! through in a `cargo run` or `cargo test` build where no such tree exists — every
//! compositor smoke depends on that fall-through reaching branch 3.
//!
//! ## The joined path is deliberately NOT normalized
//!
//! `<store>/bin/../share/wolf/nvim/init.lua` is handed to Neovim as `-u` verbatim.
//! `debug.getinfo(1, "S").source` reports exactly that string back, and
//! `fnamemodify(…, ":p:h")` yields `<store>/bin/../share/wolf/nvim`, whose
//! `/orlando` and `/lighthouse` children resolve correctly through the kernel. So
//! normalizing would be harmless and is not required; do not add a fallible
//! `canonicalize` to a startup path for it.

use std::path::{Path, PathBuf};

/// The development override's variable name, spelled once so `app.rs`, the docs and
/// the packaged-artifact tests cannot disagree about it.
pub const INIT_LUA_ENV: &str = "WOLF_INIT_LUA";

/// Where an installed Wolf keeps its captive `init.lua`, relative to the directory
/// holding the executable.
///
/// This constant is the CONTRACT between the Rust resolver and the Nix install
/// layout. `crates/wolf-gnome/tests/packaged_layout.rs` joins `<out>/bin` with it
/// rather than retyping the path, so the store tree and the branch that looks for it
/// cannot drift apart silently.
pub const INSTALLED_INIT_LUA_RELATIVE: &str = "../share/wolf/nvim/init.lua";

/// Resolves the captive `init.lua` from the override, the executable's own
/// directory, and the build-tree fallback, in that order.
///
/// `env_override` is `std::env::var(INIT_LUA_ENV).ok()` as the caller read it —
/// trimmed here rather than there, so an exported-but-empty variable behaves like an
/// absent one. `exe_dir` is `std::env::current_exe().ok().and_then(|p|
/// p.parent().map(Path::to_path_buf))`. `exists` is `Path::exists` in production.
///
/// Total: every combination of inputs returns a path, and nothing here panics.
#[must_use]
pub fn resolve_init_lua(
    env_override: Option<&str>,
    exe_dir: Option<&Path>,
    manifest_fallback: &Path,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    // Branch 1: the typed override, returned WITHOUT an existence check. The
    // asymmetry with branch 2 is deliberate — see the module doc comment.
    if let Some(raw) = env_override {
        if !raw.trim().is_empty() {
            return PathBuf::from(raw);
        }
    }

    // Branch 2: the installed tree beside the executable, returned verbatim with its `bin/..` step;
    // nothing here canonicalizes, and nothing needs to.
    if let Some(dir) = exe_dir {
        let candidate = dir.join(INSTALLED_INIT_LUA_RELATIVE);
        if exists(&candidate) {
            return candidate;
        }
    }

    // Branch 3: the build-tree fallback, unchanged. This is what every `cargo run`
    // and every compositor smoke resolves to.
    manifest_fallback.to_path_buf()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// [`INSTALLED_INIT_LUA_RELATIVE`] re-typed here as a LITERAL, per this crate's
    /// re-declare-do-not-import discipline (the `config_seam` tests'
    /// `EXPECT_FALLBACK_FONT_PT` precedent).
    ///
    /// Joining `exe_dir` with the constant and then comparing against a join of the
    /// same constant would hold for whatever value it took, including one that names
    /// a directory the Nix `postInstall` never writes. Stating the relative path
    /// independently means a change to the constant fails HERE — where the message
    /// can say that `packaged_layout.rs` joins on it too — rather than only in an
    /// `#[ignore]`d test nobody ran.
    const EXPECT_INSTALLED_RELATIVE: &str = "../share/wolf/nvim/init.lua";

    /// The override arm's fixture. Deliberately unlike [`EXE_DIR`] and
    /// [`MANIFEST_FALLBACK`] in every path component except the filename, so a
    /// failure names WHICH branch answered rather than merely that the answer was
    /// wrong.
    const ENV_OVERRIDE: &str = "/opt/typed-by-a-human/init.lua";

    /// The installed arm's fixture: the directory an installed executable sits in.
    const EXE_DIR: &str = "/nix/store/wolf-gnome-0.1.0/bin";

    /// The fallback arm's fixture: what `app.rs` builds from `CARGO_MANIFEST_DIR`.
    const MANIFEST_FALLBACK: &str = "/home/dev/checkout/runtime/nvim/init.lua";

    /// The exe-relative candidate as the resolver must form it — UN-normalized, with
    /// the `bin/..` step still in it. Built from the re-typed literal above rather
    /// than from the constant, for the reason stated there.
    fn expected_installed() -> PathBuf {
        Path::new(EXE_DIR).join(EXPECT_INSTALLED_RELATIVE)
    }

    /// A predicate that answers `true` for every path and records nothing. The
    /// "an installed tree is present" arm.
    fn everything_exists(_: &Path) -> bool {
        true
    }

    /// A predicate that answers `false` for every path. The "this is a checkout
    /// build, there is no installed tree" arm, which every compositor smoke runs in.
    fn nothing_exists(_: &Path) -> bool {
        false
    }

    // === Branch 1: the WOLF_INIT_LUA override ===

    /// A non-empty override beats an installed tree that really is there.
    ///
    /// ## Falsifier
    ///
    /// Delete the `env_override` branch: the resolver answers the installed path
    /// instead, and the packaged binary silently ignores a variable a developer
    /// exported to point it at a work-in-progress `init.lua`.
    #[test]
    fn a_typed_override_wins_over_an_installed_tree() {
        let got = resolve_init_lua(
            Some(ENV_OVERRIDE),
            Some(Path::new(EXE_DIR)),
            Path::new(MANIFEST_FALLBACK),
            everything_exists,
        );
        assert_eq!(
            got,
            PathBuf::from(ENV_OVERRIDE),
            "a non-blank {INIT_LUA_ENV} must win outright, even with an installed tree present. \
             Three outcomes are distinguishable and two are defects: {ENV_OVERRIDE:?} is correct; \
             {:?} means the override branch was deleted or moved BELOW the exe-relative one, so \
             the packaged binary ignores the variable entirely; {MANIFEST_FALLBACK:?} means both \
             of the first two branches were skipped.",
            expected_installed()
        );
    }

    /// A non-empty override is returned WITHOUT being checked for existence.
    ///
    /// This is the case that pins the deliberate asymmetry between branches 1 and 2,
    /// and it is asserted with the predicate answering `false` for everything — so a
    /// resolver that existence-checked the override would have nowhere to go but the
    /// fallback, and the difference is visible.
    ///
    /// ## Falsifier
    ///
    /// Add an `exists` check to the `env_override` branch. A mistyped override then
    /// falls through in silence and the app runs on the wrong `init.lua` while
    /// looking perfectly healthy; what this test keeps is the loud failure instead —
    /// Neovim never attaches and the app prints `failed to start core`.
    #[test]
    fn a_typed_override_is_returned_without_checking_that_it_exists() {
        let got = resolve_init_lua(
            Some(ENV_OVERRIDE),
            Some(Path::new(EXE_DIR)),
            Path::new(MANIFEST_FALLBACK),
            nothing_exists,
        );
        assert_eq!(
            got,
            PathBuf::from(ENV_OVERRIDE),
            "a non-blank {INIT_LUA_ENV} must be returned even though the predicate answers false \
             for every path: an override the user typed is an instruction, not a candidate. \
             Getting {MANIFEST_FALLBACK:?} means the override is now existence-checked, which \
             turns a typo into a SILENT fall-through to a different init.lua — the failure mode \
             this asymmetry exists to prevent. Do not 'fix' this by making the check tolerant; \
             delete the check."
        );
    }

    /// An absent, empty or all-whitespace override is not an override.
    ///
    /// All three reach this function in practice: `std::env::var` gives `None` for an
    /// unset variable and `Some("")` for `export WOLF_INIT_LUA=`, and a value pasted
    /// out of a terminal can be whitespace alone.
    ///
    /// ## Falsifier
    ///
    /// Drop the `.trim().is_empty()` test. `PathBuf::from("")` then reaches Neovim as
    /// `-u ''`, which is not the installed file and not the checkout's either.
    #[test]
    fn a_blank_override_is_ignored_and_the_next_branch_runs() {
        let blanks: [(&str, Option<&str>); 4] = [
            ("unset", None),
            ("empty", Some("")),
            ("one space", Some(" ")),
            ("tabs and newlines", Some("\t\n  ")),
        ];
        for (what, value) in blanks {
            let got = resolve_init_lua(
                value,
                Some(Path::new(EXE_DIR)),
                Path::new(MANIFEST_FALLBACK),
                everything_exists,
            );
            assert_eq!(
                got,
                expected_installed(),
                "the {what} override must be ignored so the exe-relative branch runs, but this \
                 answered {got:?}. An empty path here is worse than either real answer: it names \
                 no file at all, so Neovim is launched with `-u ''`, never attaches, and the \
                 window never opens."
            );
        }
    }

    // === Branch 2: the installed tree beside the executable ===

    /// With an executable directory and a predicate that finds the file, the answer
    /// is `<exe dir>` joined with [`INSTALLED_INIT_LUA_RELATIVE`].
    ///
    /// The expected value is built from a re-typed literal, so this also pins the
    /// constant's VALUE — which is the half `packaged_layout.rs` joins on to find the
    /// file the Nix `postInstall` wrote.
    ///
    /// ## Falsifier
    ///
    /// Change [`INSTALLED_INIT_LUA_RELATIVE`]. Assertion 1 fires on the constant
    /// itself and names the install layout it has to agree with.
    #[test]
    fn an_executable_relative_tree_that_exists_is_resolved_from_the_installed_path() {
        // --- Assertion 1: the constant's value, stated independently ----------
        assert_eq!(
            INSTALLED_INIT_LUA_RELATIVE, EXPECT_INSTALLED_RELATIVE,
            "INSTALLED_INIT_LUA_RELATIVE is {INSTALLED_INIT_LUA_RELATIVE:?}, but the Nix package's \
             `postInstall` writes the captive init.lua at `$out/share/wolf/nvim/init.lua` and the \
             wrapped binary sits in `$out/bin`, so {EXPECT_INSTALLED_RELATIVE:?} is the join that \
             reaches it. Changing one side alone is silent at runtime: the branch simply never \
             matches and the packaged app falls through to a build-tree path that does not exist \
             in the store, so Neovim never attaches. Change flake.nix's install layout and this \
             constant together."
        );

        // --- Assertion 2: the resolver really forms that join -----------------
        let got = resolve_init_lua(
            None,
            Some(Path::new(EXE_DIR)),
            Path::new(MANIFEST_FALLBACK),
            everything_exists,
        );
        assert_eq!(
            got,
            expected_installed(),
            "with an executable directory and a predicate that finds the file, the resolver must \
             answer {:?}. Getting {MANIFEST_FALLBACK:?} means branch 2 never returns at all, so a \
             packaged binary is launched against a checkout path that does not exist in the store.",
            expected_installed()
        );
    }

    /// A checkout build has no such tree, and must fall through rather than return a
    /// path to nothing.
    ///
    /// This is the case that keeps `cargo run` and every compositor smoke working:
    /// the test binary's own directory is `target/debug/deps`, and
    /// `target/debug/deps/../share/wolf/nvim/init.lua` does not exist.
    ///
    /// ## Falsifier
    ///
    /// Delete the `exists` guard on branch 2. Every smoke in
    /// `crates/wolf-gnome/tests/smoke_headless.rs` then launches Neovim with a `-u`
    /// naming a file inside `target/`, none of them attaches, and the whole suite
    /// goes red at once with no failure naming this branch.
    #[test]
    fn an_executable_relative_tree_that_is_absent_falls_through_to_the_manifest() {
        let got = resolve_init_lua(
            None,
            Some(Path::new(EXE_DIR)),
            Path::new(MANIFEST_FALLBACK),
            nothing_exists,
        );
        assert_eq!(
            got,
            PathBuf::from(MANIFEST_FALLBACK),
            "with the predicate answering false, branch 2's guess must fall through to the \
             build-tree fallback. Getting {:?} means the guard was dropped and branch 2 now \
             returns unconditionally — which is a path that exists only in a Nix store output, so \
             every checkout run hands Neovim a `-u` naming nothing.",
            expected_installed()
        );
    }

    /// No executable directory at all is the same fall-through, and must not panic.
    ///
    /// `std::env::current_exe()` is fallible and its result may have no parent, so
    /// `None` is reachable rather than hypothetical.
    ///
    /// ## Falsifier
    ///
    /// Make branch 2 unwrap the `Option`. The resolver then panics on the startup
    /// path for an input the standard library documents as possible.
    #[test]
    fn no_executable_directory_falls_through_to_the_manifest() {
        let got = resolve_init_lua(None, None, Path::new(MANIFEST_FALLBACK), everything_exists);
        assert_eq!(
            got,
            PathBuf::from(MANIFEST_FALLBACK),
            "with no executable directory the resolver must answer the build-tree fallback and \
             must not panic. A panic here is `current_exe()`'s documented failure turned into a \
             crash before the window exists; any other path means branch 2 invented a directory \
             it was not given."
        );
    }

    /// The candidate handed to the predicate is the raw join — the `bin/..` step
    /// still in it — and not a normalized or canonicalized form of it.
    ///
    /// The predicate records what it saw, so this asserts on the exact path rather
    /// than on the resolver's return value: a resolver that canonicalized only for
    /// the probe and returned the raw join would satisfy every other test here.
    ///
    /// ## Falsifier
    ///
    /// Insert a `canonicalize` (or any `..`-collapsing normalization) before the
    /// probe. Assertion 2 fires. What that would cost is not cosmetic: on the
    /// packaged path `canonicalize` is fallible and is being called on a startup
    /// path, and the value Neovim is handed is what `debug.getinfo(1, "S").source`
    /// reports back to the captive `init.lua` when it self-locates its sibling
    /// `orlando/` and `lighthouse/` trees.
    #[test]
    fn the_executable_relative_candidate_reaches_the_predicate_unnormalized() {
        let seen: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());

        let got = resolve_init_lua(
            None,
            Some(Path::new(EXE_DIR)),
            Path::new(MANIFEST_FALLBACK),
            |candidate: &Path| {
                seen.borrow_mut().push(candidate.to_path_buf());
                true
            },
        );

        let seen = seen.into_inner();

        // --- Assertion 1 (non-vacuity): the predicate was called, or a resolver that never probed
        // would leave `seen` empty and assertion 2 would pass over nothing.
        assert_eq!(
            seen.len(),
            1,
            "the resolver must probe the exe-relative candidate exactly once; it probed \
             {} time(s) with {seen:?}. ZERO means branch 2 returns without asking, so this test \
             is checking nothing and `an_executable_relative_tree_that_is_absent_falls_through_to_\
             the_manifest` is the one to read next. MORE THAN ONE means the branch probes \
             several candidates, which is a rule this seam does not have and which \
             `packaged_layout.rs` could not join against.",
            seen.len()
        );

        // --- Assertion 2: the probed path is the raw join ----------------------
        let raw = expected_installed();
        let normalized = Path::new("/nix/store/wolf-gnome-0.1.0/share/wolf/nvim/init.lua");
        assert_eq!(
            seen[0], raw,
            "the predicate saw {:?}; it must see the raw join {raw:?}, with the `bin/..` step \
             intact. Seeing {normalized:?} instead means a `canonicalize` or a `..`-collapsing \
             normalization was inserted — which adds a fallible filesystem call to the startup \
             path and changes the string Neovim reports back through \
             `debug.getinfo(1, \"S\").source`, which is how the captive init.lua finds its sibling \
             orlando/ and lighthouse/ trees.",
            seen[0]
        );
        assert_ne!(
            seen[0], normalized,
            "the predicate saw the NORMALIZED path {normalized:?} rather than the raw join. \
             Stated as its own assertion so the failure names the collapse explicitly: nothing in \
             this seam may resolve `..`, and nothing needs to — the kernel resolves it when \
             Neovim opens the file."
        );

        assert_eq!(
            got, raw,
            "having probed the raw join, the resolver must also RETURN it. Returning a different \
             path than the one it checked means the probe and the answer have come apart."
        );
    }

    // === The order of the three branches ===

    /// With every input available at once, the branches answer in order: the
    /// override, then the installed tree, then the fallback.
    ///
    /// The three assertions are a chain — each removes the winner of the one before
    /// — so the ordering is asserted rather than each branch being asserted in
    /// isolation, which is what a swapped pair would survive.
    ///
    /// ## Falsifier
    ///
    /// Swap any two branches. Swapping 1 and 2 fires assertion 1; swapping 2 and 3
    /// fires assertion 2.
    #[test]
    fn the_three_branches_are_tried_in_order_with_all_three_available() {
        let exe_dir = Path::new(EXE_DIR);
        let manifest = Path::new(MANIFEST_FALLBACK);
        let installed = expected_installed();

        // --- Assertion 1: the override beats both of the others ---------------
        assert_eq!(
            resolve_init_lua(
                Some(ENV_OVERRIDE),
                Some(exe_dir),
                manifest,
                everything_exists
            ),
            PathBuf::from(ENV_OVERRIDE),
            "with all three inputs available the override must win. {installed:?} means branches \
             1 and 2 are swapped, so the packaged binary can never be pointed at a different \
             init.lua; {MANIFEST_FALLBACK:?} means the override was moved below both."
        );

        // --- Assertion 2: with no override, the installed tree beats the fallback
        assert_eq!(
            resolve_init_lua(None, Some(exe_dir), manifest, everything_exists),
            installed,
            "with the override gone and the installed tree present, the installed tree must win. \
             Getting {MANIFEST_FALLBACK:?} means branches 2 and 3 are swapped, which makes the \
             installed branch dead code: the fallback always answers first, so a packaged binary \
             would run against a checkout path that does not exist in the store."
        );

        // --- Assertion 3: with neither, the fallback answers ------------------
        assert_eq!(
            resolve_init_lua(None, None, manifest, everything_exists),
            PathBuf::from(MANIFEST_FALLBACK),
            "with no override and no executable directory, the fallback must answer. Anything \
             else is a path the caller never supplied."
        );
    }
}
