//! The `flake.nix` guards: every `export` in `linuxShell.shellHook` is either scrubbed from the
//! packaged smokes or deliberately kept, and the hook still stages the vendored Lua trees the
//! `wolf-ffi` tests need.
//!
//! This binary reads only `flake.nix`: it needs no compositor, no smoke toolchain and no
//! environment, so it runs in seconds. It compiles the packaged smokes' scrub lists from
//! `smoke_headless/scrub_lists.rs` through `#[path]`, so the guard and the packaged smokes in
//! the `smoke_headless` binary read one definition of each list.

#[path = "smoke_headless/scrub_lists.rs"]
mod scrub_lists;

use std::path::{Path, PathBuf};

use scrub_lists::{PACKAGED_SCRUB_ALWAYS, PACKAGED_SCRUB_FONT_STACK, PACKAGED_SCRUB_GTK_RUNTIME};

/// The `shellHook` exports every packaged smoke deliberately KEEPS, because they
/// belong to `cage` rather than to the application under test.
///
/// Removing either would break the compositor itself, not the artifact: they select
/// wlroots' headless backend and stop it looking for input devices.
const KEPT_FOR_THE_COMPOSITOR: [&str; 2] = ["WLR_BACKENDS", "WLR_LIBINPUT_NO_DEVICES"];

// ---------------------------------------------------------------------------
// Guard G1: the three scrub lists track the shellHook.

/// `flake.nix`, relative to this crate's own directory.
const FLAKE_RELATIVE: &str = "../../flake.nix";

/// The line that opens the Linux devShell, as its trimmed form. The DARWIN shell has
/// a `shellHook` of its own (`export DEVELOPER_DIR=…`) which no packaged smoke can
/// inherit on this platform, so the scan is anchored here rather than run over the
/// whole file.
const LINUX_SHELL_MARKER: &str = "linuxShell = pkgs.mkShell {";

/// The line that opens the hook's indented string, as its trimmed form.
const SHELL_HOOK_OPEN: &str = "shellHook = ''";

/// The line that closes it, as its trimmed form.
const SHELL_HOOK_CLOSE: &str = "'';";

/// The prefix a variable export begins with inside the hook.
const EXPORT_PREFIX: &str = "export ";

/// Every `export <NAME>=` inside `linuxShell.shellHook`, plus how many lines began
/// with [`EXPORT_PREFIX`] at all.
///
/// The second number is the vacuity guard: if the two disagree, a line exports
/// something in a shape this scanner cannot read (`export A B`, a name with
/// characters it does not accept), and that variable would leak into every packaged
/// run with nothing reporting it.
///
/// STATED COUPLING with `flake.nix`: one `export NAME=value` per line inside a hook
/// delimited by a trimmed `shellHook = ''` and a trimmed `'';`. Both hold as written,
/// and a change to either shows up as a failed assertion here rather than as a wrong
/// answer somewhere else.
fn linux_shell_hook_exports(text: &str) -> Option<(Vec<String>, usize)> {
    let mut lines = text.lines();
    lines.find(|line| line.trim() == LINUX_SHELL_MARKER)?;
    let mut lines = lines.skip_while(|line| line.trim() != SHELL_HOOK_OPEN);
    lines.next()?;

    let mut names = Vec::new();
    let mut export_lines = 0usize;
    for line in lines {
        let trimmed = line.trim();
        if trimmed == SHELL_HOOK_CLOSE {
            break;
        }
        let Some(rest) = trimmed.strip_prefix(EXPORT_PREFIX) else {
            continue;
        };
        export_lines += 1;
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && rest[name.len()..].starts_with('=') {
            names.push(name);
        }
    }
    Some((names, export_lines))
}

/// [`linux_shell_hook_exports`] against the workspace `flake.nix`, plus the two
/// failure modes that must never be mistaken for "nothing to report": the file could
/// not be READ, and the hook could not be LOCATED. Both panic here, naming which, so
/// every guard below asserts over a hook this scanner really did parse.
///
/// Returns the path it read (for the guards' own messages), the export NAMES, and how
/// many lines began with [`EXPORT_PREFIX`] at all.
///
/// Shared by [`every_shellhook_export_is_scrubbed_or_deliberately_kept`] and
/// [`the_shellhook_still_stages_the_vendored_lua_trees_for_the_ffi_tests`] rather than
/// copied into each: two copies of these two messages would drift apart, and a second
/// scanner would give the guards two different ideas of what the hook says — the
/// exact defect `cage_is_only_spawned_through_the_isolating_helper` exists to
/// prevent one directory over.
fn require_linux_shell_hook_exports(guard: &str) -> (PathBuf, Vec<String>, usize) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FLAKE_RELATIVE);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{guard}: could not read {}: {e}. That path is CARGO_MANIFEST_DIR joined with \
             `{FLAKE_RELATIVE}`, so an error here means this crate moved relative to the workspace \
             root, not that an export changed.",
            path.display()
        )
    });

    let Some((names, export_lines)) = linux_shell_hook_exports(&text) else {
        panic!(
            "{guard}: could not locate `{LINUX_SHELL_MARKER}`'s `{SHELL_HOOK_OPEN}` block in {}. \
             With no block located a guard reading it compares NOTHING, so the packaged smokes' \
             environment would go unchecked and the devShell's staging exports could be gone, with \
             both guards green either way. Either the binding was renamed or the hook was \
             reformatted; fix `flake.nix` or the two markers so they agree again. Do NOT relax the \
             scan to search the whole file: the Darwin shell has a `shellHook` of its own, and \
             folding the two together would classify a variable that cannot exist on this \
             platform.",
            path.display()
        )
    };

    (path, names, export_lines)
}

/// Enforces mechanically that every variable `flake.nix`'s `linuxShell.shellHook`
/// exports is either SCRUBBED from the packaged smokes or on a short, explicitly
/// named keep-list.
///
/// ## What an unclassified export costs
///
/// Every packaged smoke's claim is that the STORE artifact supplies its own runtime.
/// A `shellHook` export that reaches the app under `cage` quietly converts that into
/// an inherited-environment claim: the smoke stays green, the wrapper could be
/// carrying nothing at all, and the failure is invisible because a green run looks
/// identical either way. That is the same distance-between-fixture-and-assertion
/// problem the fixture-name guards exist for, and it is why this is a guard
/// rather than a comment on `packaged_cage_command`.
///
/// ## What a failure means, and the decision it forces
///
/// A new name here is not a bug in this test. It is a variable somebody added to the
/// devShell, and it has to be classified deliberately: does the packaged artifact
/// have to supply it for itself (add it to [`PACKAGED_SCRUB_ALWAYS`], or to
/// [`PACKAGED_SCRUB_GTK_RUNTIME`] / [`PACKAGED_SCRUB_FONT_STACK`] if a smoke with an
/// unwrapped control leg still needs it), or does it belong to `cage` rather than to
/// the application (add it to [`KEPT_FOR_THE_COMPOSITOR`], with the reason)? Do not
/// widen the keep-list to silence a failure; a variable kept without a reason is a
/// variable the packaged smokes are quietly reading.
///
/// ## Falsifier
///
/// Add `export WOLF_ANYTHING=1` to `linuxShell.shellHook`; assertion 3 fires and
/// names it.
///
/// ## What it needs
///
/// NO environment: no compositor, no `require_smoke_env`, no cage slot, no
/// subprocess and no built package. It reads one file, and it is NOT `#[ignore]`d —
/// it runs in the ordinary suite, which is the point, since the leak it guards
/// against arrives with a `flake.nix` edit rather than with a packaged run.
#[test]
fn every_shellhook_export_is_scrubbed_or_deliberately_kept() {
    const GUARD: &str = "every_shellhook_export_is_scrubbed_or_deliberately_kept";

    let (path, names, export_lines) = require_linux_shell_hook_exports(GUARD);

    // --- Assertion 1 (non-vacuity): the hook exports something ----------------
    assert!(
        !names.is_empty(),
        "{GUARD}: no `{EXPORT_PREFIX}NAME=` line was found inside {}'s `{LINUX_SHELL_MARKER}` \
         hook. The hook exports WOLF_NVIM_BIN among others, so zero matches means this scanner no \
         longer reads it — and a guard that recognizes no exports accepts every leak there is.",
        path.display()
    );

    // --- Assertion 2 (non-vacuity): every export line was readable ------------
    assert!(
        names.len() == export_lines,
        "{GUARD}: {} of {export_lines} `{EXPORT_PREFIX}` line(s) in {}'s hook yielded a variable \
         name this scanner could read; the others use a shape it does not understand (an `export \
         A B` form, or a name carrying characters outside `[A-Za-z0-9_]`). Each unread line is a \
         variable that is NOT classified below and would leak into every packaged run unnoticed. \
         Reformat the export, or teach the scanner the shape and state the new contract in \
         `linux_shell_hook_exports`.",
        names.len(),
        path.display()
    );

    // --- Assertion 3: every export is classified ------------------------------
    let unclassified: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| {
            !PACKAGED_SCRUB_ALWAYS.contains(name)
                && !PACKAGED_SCRUB_GTK_RUNTIME.contains(name)
                && !PACKAGED_SCRUB_FONT_STACK.contains(name)
                && !KEPT_FOR_THE_COMPOSITOR.contains(name)
        })
        .collect();
    assert!(
        unclassified.is_empty(),
        "{GUARD}: {}'s `{LINUX_SHELL_MARKER}` hook exports {unclassified:?}, which appear in NONE \
         of the three packaged scrub lists ({PACKAGED_SCRUB_ALWAYS:?}, \
         {PACKAGED_SCRUB_GTK_RUNTIME:?} and {PACKAGED_SCRUB_FONT_STACK:?}) NOR in the compositor \
         keep-list ({KEPT_FOR_THE_COMPOSITOR:?}). Every packaged smoke inherits this process's \
         environment except for what `packaged_cage_command` removes, so an unclassified export \
         reaches the STORE binary under `cage` — and every claim those smokes make about the \
         artifact supplying its own runtime quietly becomes a claim about the devShell supplying \
         it instead, with a green run either way. This is a DECISION, not a bug in this test: put \
         each name in PACKAGED_SCRUB_ALWAYS if the artifact must supply it, in \
         PACKAGED_SCRUB_GTK_RUNTIME if the A/B smoke's unwrapped control leg still needs it, in \
         PACKAGED_SCRUB_FONT_STACK if it pins the font stack every derived rectangle is measured \
         against, or in KEPT_FOR_THE_COMPOSITOR — with the reason — if it belongs to `cage` \
         rather than to the application. Do not widen the keep-list to make this pass.",
        path.display()
    );
}

// --- Guard G2: the shellHook still stages the vendored Lua trees the wolf-ffi tests need. ---

/// The `shellHook` exports that decide whether three `wolf-ffi` real-Neovim tests
/// ASSERT on Linux or print a loud SKIP and pass, paired with what each one's absence
/// costs — the sentence a failure has to hand the reader.
///
/// Measured: `common::orlando_dir_or_none` and `common::lighthouse_dir_or_none` have
/// exactly four call sites in this repository, all in the files named here, so this
/// table is the whole blast radius of the two exports rather than a sample of it.
///
/// The two names are deliberately NOT re-derived from [`PACKAGED_SCRUB_ALWAYS`],
/// where they also appear: that list says the packaged smokes must not INHERIT them,
/// which is a different claim from the devShell having to SET them, and reading one
/// out of the other would make deleting both lines a self-consistent no-op.
const VENDORED_TREE_EXPORTS: [(&str, &str); 2] = [
    (
        "WOLF_TEST_ORLANDO_DIR",
        "crates/wolf-ffi/tests/orlando_load.rs and crates/wolf-ffi/tests/orlando_bg_stress.rs \
         BOTH return a no-op pass without it — each opens by taking \
         `common::orlando_dir_or_none()`, printing `SKIP: … WOLF_TEST_ORLANDO_DIR not staged … \
         No-op pass.` and returning — so the vendored orlando plugin loses its only Linux \
         evidence that `runtime/nvim/init.lua`'s sibling-`orlando/` branch loads anything",
    ),
    (
        "WOLF_TEST_LIGHTHOUSE_DIR",
        "crates/wolf-ffi/tests/lighthouse_load.rs returns a no-op pass without it, by the same \
         route through `common::lighthouse_dir_or_none()`, so the vendored lighthouse \
         colorscheme loses its only Linux evidence that `runtime/nvim/init.lua` ACTIVATES it \
         rather than merely putting it on the runtimepath; and \
         crates/wolf-ffi/tests/orlando_bg_stress.rs still runs, but stages no colorscheme at \
         all, so its background-uniformity churn is measured against Neovim's defaults instead",
    ),
];

/// Enforces that `flake.nix`'s `linuxShell.shellHook` still exports both variables
/// that make the three `wolf-ffi` vendored-tree tests assert on Linux.
///
/// ## What deleting either export costs, and why nothing else would say so
///
/// Those three tests are written to SKIP rather than fail when their tree is not
/// staged, because on the Bazel/macOS path the trees arrive as `rust_test` runfiles
/// and on the plain-cargo path they do not exist at all. That policy is right and is
/// not what this guard argues with. What it means is that deleting an export line
/// reddens NOTHING: the tests that lose their tree return early, print a SKIP banner
/// into output nobody reads on a green run, and pass. Deleting the orlando line does
/// that to two of them and deleting the lighthouse line to the third, so with both
/// gone three real-Neovim tests are no-op passes at once and the two vendored Lua
/// trees have no Linux load evidence left — precisely the state the derivations and
/// these exports were added to end. The Linux suite stays green throughout, proving
/// strictly less than it did the day before.
///
/// ## Why it lives here rather than in `crates/wolf-ffi/tests`
///
/// It reads `flake.nix` through the same scanner
/// [`every_shellhook_export_is_scrubbed_or_deliberately_kept`] uses, anchored on the
/// same `linuxShell` marker, so the guards cannot form different opinions about
/// what the hook says. A copy in `wolf-ffi` would need a second scanner, plus a
/// `rust_test` target with a `data` dependency on a file outside that package which is
/// not a Bazel runfile — the trade `crates/wolf-gnome/tests/vendored_pins.rs` and
/// `release_profile.rs` both record. The subject is a `wolf-ffi` gap; the instrument
/// is here.
///
/// ## What else could satisfy this assertion, and why nothing does
///
/// A bare "does `flake.nix` contain this string" check would pass on the prose above
/// the exports, which names both variables, and on any later `--set-default` in the
/// packaged derivation. [`linux_shell_hook_exports`] cannot see either: it opens at
/// [`LINUX_SHELL_MARKER`], reads only between the hook's `''` delimiters, and takes a
/// name only from a line whose trimmed form begins `export NAME=`. So a match here is
/// a live devShell export and nothing else — which is the whole reason this guard
/// borrows that scanner instead of grepping.
///
/// ## What it does NOT check
///
/// The export's NAME, not its value. An export left in place but pointed at an empty
/// string, or at a path that does not exist, returns all three tests to the SKIP
/// branch with this guard still green, because `wolf_test_support::resolve_test_runfile`
/// treats both as "not staged". Nothing in either suite would notice that; it is recorded here
/// rather than papered over, and a value shape is not asserted because the value in
/// the file is a Nix interpolation (`"${wolfOrlando}"`) whose resolution this scanner
/// cannot see.
///
/// ## Falsifier
///
/// Delete the `export WOLF_TEST_LIGHTHOUSE_DIR="${wolfLighthouse}"` line from
/// `linuxShell.shellHook`: assertion 2 fires naming that variable, the two test files
/// it disarms and what each one stops proving. Delete both lines and BOTH are
/// reported in one run, rather than the first alone — the shape
/// `crates/wolf-gnome/tests/README.md` records a loop-with-an-assertion getting wrong.
///
/// ## What it needs
///
/// NO environment: no compositor, no `require_smoke_env`, no cage slot, no
/// subprocess and no built package. It reads one file, and it is NOT `#[ignore]`d —
/// the deletion it guards against arrives with a `flake.nix` edit, so it has to run in
/// the ordinary suite.
#[test]
fn the_shellhook_still_stages_the_vendored_lua_trees_for_the_ffi_tests() {
    const GUARD: &str = "the_shellhook_still_stages_the_vendored_lua_trees_for_the_ffi_tests";

    let (path, names, _export_lines) = require_linux_shell_hook_exports(GUARD);

    // Assertion 1 (non-vacuity): a scanner that stopped reading the hook would report both names as
    // deleted and send the reader to restore lines that are there.
    assert!(
        !names.is_empty(),
        "{GUARD}: no `{EXPORT_PREFIX}NAME=` line was found inside {}'s `{LINUX_SHELL_MARKER}` \
         hook, so this guard cannot tell an export that was DELETED from one it can no longer \
         READ. Fix the scanner or the hook's formatting before believing assertion 2 below; \
         `every_shellhook_export_is_scrubbed_or_deliberately_kept` reports the same finding from \
         the other side.",
        path.display()
    );

    // --- Assertion 2: both vendored-tree exports are still there --------------
    let missing: Vec<(&str, &str)> = VENDORED_TREE_EXPORTS
        .into_iter()
        .filter(|(name, _)| !names.iter().any(|found| found.as_str() == *name))
        .collect();
    let missing_names: Vec<&str> = missing.iter().map(|(name, _)| *name).collect();
    let cost: String = missing
        .iter()
        .map(|(name, consequence)| format!("\n  * {name}: {consequence}.\n"))
        .collect();
    assert!(
        missing.is_empty(),
        "{GUARD}: {}'s `{LINUX_SHELL_MARKER}` hook no longer exports {missing_names:?}:\n{cost}\n\
         DELETING ONE OF THESE LINES REDDENS NOTHING ELSE, which is the whole reason this guard \
         exists. The three tests are `crates/wolf-ffi/tests/orlando_load.rs`, \
         `crates/wolf-ffi/tests/lighthouse_load.rs` and \
         `crates/wolf-ffi/tests/orlando_bg_stress.rs`; every one of them is written to SKIP rather \
         than fail when its tree is not staged, so they would go on PASSING — as loud no-ops whose \
         banner nobody reads on a green run — while the vendored orlando and lighthouse trees lose \
         their only Linux proof that the captive `runtime/nvim/init.lua` loads either one. Restore \
         the export. If the derivation behind it was genuinely removed, that is a decision to take \
         deliberately and to record in `crates/wolf-ffi/tests/README.md` as coverage given up, not \
         a reason to delete this line from the table above.",
        path.display()
    );
}
