//! Guards that the treesitter languages Wolf vendors are ONE list written in two
//! files: `flake.nix`'s `wolfTreesitterLanguages`, which builds the Linux parser
//! join, and the `srcs` of `parsers_tree` in `bazel/treesitter-parsers/BUILD.bazel`,
//! which builds the parsers the macOS `.app` bundles.
//!
//! Neither build reads the other's list, and no host runs both builds, so a
//! language added on one side only is silent: both platforms build green and one of
//! them highlights a language the other shows as plain text. This file is what
//! notices, on every `cargo test`.
//!
//! ## The two extraction rules
//!
//! Both sides are read by small line scanners rather than a Nix or Starlark parser,
//! exactly as `vendored_pins.rs` reads the pins, so each imposes a stated formatting
//! contract on its file — see [`nix_languages`] and [`bazel_languages`]. A line inside
//! either list that its scanner cannot read is a failure, never a skip: a skipped line
//! is a language nothing compares. When a file and its scanner disagree, reformat the
//! file; do not teach the scanner a second shape.
//!
//! The two files define the set. Nothing here names a language or a count, so adding
//! a language to both sides in one change, as
//! `bazel/treesitter-parsers/README.md#adding-a-language` describes, keeps this green
//! without touching this file.
//!
//! `std` only, for the `Cargo.lock` reason `vendored_pins.rs` records.
//!
//! ## What it does NOT check
//!
//! That either side's parsers build, load or match their queries: that is
//! `//bazel/treesitter-parsers:queries_compile_test` on macOS and the package build on
//! Linux. This file checks only that the two lists name the same languages in the
//! same order.
//!
//! No Bazel wiring, and none is missing: `crates/wolf-gnome` has no `BUILD.bazel`, for
//! the reason `vendored_pins.rs` gives.

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Where each side writes its list down.

/// The workspace file that carries the Linux side.
const FLAKE: &str = "flake.nix";

/// The line that opens the Nix list, as its trimmed form.
const NIX_LIST_OPEN: &str = "wolfTreesitterLanguages = [";

/// The line that closes it, as its trimmed form. The first one after the opener wins.
const NIX_LIST_CLOSE: &str = "];";

/// The Bazel file that carries the macOS side.
const BAZEL_BUILD: &str = "bazel/treesitter-parsers/BUILD.bazel";

/// The line naming the target whose `srcs` are the macOS list, as its trimmed form.
const BAZEL_TARGET_NAME: &str = "name = \"parsers_tree\",";

/// The line that opens that target's `srcs`, as its trimmed form.
const BAZEL_SRCS_OPEN: &str = "srcs = [";

/// The line that closes it, as its trimmed form. The first one after the opener wins.
const BAZEL_SRCS_CLOSE: &str = "],";

/// The line that closes the target's call, as its trimmed form. A `srcs = [` is
/// searched for only before it, so another target's `srcs` is never read instead.
const BAZEL_CALL_CLOSE: &str = ")";

/// The repository prefix every parser label carries, after its `@`.
const BAZEL_REPO_PREFIX: &str = "ts_";

/// The suffix every parser target carries. The language is the target without it.
const BAZEL_TARGET_SUFFIX: &str = "_parser";

// ---------------------------------------------------------------------------
// File helpers.

/// A workspace-relative path, resolved from this crate's own directory exactly as
/// `vendored_pins.rs::workspace_path` does.
fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// Reads a workspace file, or panics naming the path it tried.
fn read_workspace_file(guard: &str, relative: &str) -> String {
    let path = workspace_path(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{guard}: could not read {}: {e}. That path is CARGO_MANIFEST_DIR joined with \
             `../../{relative}`, so an error here means this crate moved relative to the workspace \
             root, or the file was renamed — NOT that the two language lists drifted.",
            path.display()
        )
    })
}

/// Whether `name` is a language name both scanners accept: non-empty, and only
/// lowercase ASCII letters, digits and `_`.
fn is_language_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// What a scanner found inside its list: the languages it read, in file order, and
/// the trimmed lines it could not read.
struct Scanned {
    languages: Vec<String>,
    unreadable: Vec<String>,
}

/// Reads the lines of `lines` up to the first whose trimmed form is `close`, skipping
/// blank and `#` lines and handing every other trimmed line to `parse`.
fn scan_list<'a>(
    lines: impl Iterator<Item = &'a str>,
    close: &str,
    parse: impl Fn(&str) -> Option<String>,
) -> Scanned {
    let mut scanned = Scanned {
        languages: Vec::new(),
        unreadable: Vec::new(),
    };
    for line in lines {
        let trimmed = line.trim();
        if trimmed == close {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match parse(trimmed) {
            Some(language) => scanned.languages.push(language),
            None => scanned.unreadable.push(trimmed.to_string()),
        }
    }
    scanned
}

// ---------------------------------------------------------------------------
// The Nix side.

/// One line of the Nix list: `"<lang>"` and nothing else.
fn parse_nix_line(trimmed: &str) -> Option<String> {
    let name = trimmed.strip_prefix('"')?.strip_suffix('"')?;
    is_language_name(name).then(|| name.to_string())
}

/// The languages in `flake.nix`'s `wolfTreesitterLanguages`, plus the lines inside
/// the list this scanner could NOT read.
///
/// STATED COUPLING with `flake.nix`: the list opens on a line whose trimmed form is
/// exactly `wolfTreesitterLanguages = [` and ends at the first later line whose
/// trimmed form is exactly `];`. Blank lines and `#` lines inside it are skipped.
/// Every other line is one double-quoted language name, alone on its line:
///
/// ```text
/// "<lang>"
/// ```
///
/// `None` means the opening line is absent, which is reported apart from an empty
/// list: the first means the binding was renamed or reformatted, the second that its
/// contents were removed.
fn nix_languages(text: &str) -> Option<Scanned> {
    let mut lines = text.lines();
    lines.find(|line| line.trim() == NIX_LIST_OPEN)?;
    Some(scan_list(lines, NIX_LIST_CLOSE, parse_nix_line))
}

// ---------------------------------------------------------------------------
// The Bazel side.

/// One line of the Bazel `srcs`: `"@ts_<repo>//:<lang>_parser",` and nothing else.
/// The repository is ignored, since one repository can build two languages.
fn parse_bazel_line(trimmed: &str) -> Option<String> {
    let label = trimmed.strip_prefix('"')?.strip_suffix("\",")?;
    let (repo, target) = label.strip_prefix('@')?.split_once("//:")?;
    if !is_language_name(repo.strip_prefix(BAZEL_REPO_PREFIX)?) {
        return None;
    }
    let name = target.strip_suffix(BAZEL_TARGET_SUFFIX)?;
    is_language_name(name).then(|| name.to_string())
}

/// The languages in `parsers_tree`'s `srcs` in `bazel/treesitter-parsers/BUILD.bazel`,
/// plus the lines inside that list this scanner could NOT read.
///
/// STATED COUPLING with that file: the target is found by a line whose trimmed form
/// is exactly `name = "parsers_tree",`. Its `srcs` opens on the first later line,
/// before the call's closing `)`, whose trimmed form is exactly `srcs = [`, and ends
/// at the first later line whose trimmed form is exactly `],`. Blank lines and `#`
/// lines inside it are skipped. Every other line is one label, alone on its line:
///
/// ```text
/// "@ts_<repo>//:<lang>_parser",
/// ```
///
/// The language is the target name without `_parser`, so `@ts_typescript//:tsx_parser`
/// gives `tsx`.
///
/// `None` means either anchor is absent: the target was renamed, or its `srcs` is no
/// longer a list literal on lines of its own.
fn bazel_languages(text: &str) -> Option<Scanned> {
    let mut lines = text.lines();
    lines.find(|line| line.trim() == BAZEL_TARGET_NAME)?;
    loop {
        let trimmed = lines.next()?.trim();
        if trimmed == BAZEL_SRCS_OPEN {
            break;
        }
        if trimmed == BAZEL_CALL_CLOSE {
            return None;
        }
    }
    Some(scan_list(lines, BAZEL_SRCS_CLOSE, parse_bazel_line))
}

// ---------------------------------------------------------------------------
// Both sides, read or refused.

/// One side's languages, in file order, or a panic naming why the side could not be
/// read.
///
/// The three refusals keep the check from going vacuous: a list that could not be
/// found, or holds a line nothing reads, or is empty, would compare as a subset of
/// anything.
fn require_languages(guard: &str, relative: &str) -> Vec<String> {
    let text = read_workspace_file(guard, relative);
    let (scanned, shape) = if relative == FLAKE {
        (
            nix_languages(&text),
            format!(
                "a line whose trimmed form is exactly `{NIX_LIST_OPEN}`, closed by the first \
                 later `{NIX_LIST_CLOSE}`, holding one `\"<lang>\"` per line"
            ),
        )
    } else {
        (
            bazel_languages(&text),
            format!(
                "a `{BAZEL_TARGET_NAME}` line followed, before its call's `{BAZEL_CALL_CLOSE}`, \
                 by `{BAZEL_SRCS_OPEN}`, closed by the first later `{BAZEL_SRCS_CLOSE}`, holding \
                 one `\"@{BAZEL_REPO_PREFIX}<repo>//:<lang>{BAZEL_TARGET_SUFFIX}\",` per line"
            ),
        )
    };

    let Some(scanned) = scanned else {
        panic!(
            "{guard}: {relative} has no language list this scanner can locate, so its side of \
             the check is VACUOUS — it compares nothing. The scanner expects {shape}. The list \
             was renamed or reformatted: restore that shape, or update this file's constants if \
             the rename is deliberate. Do NOT widen the scanner to search for something looser."
        )
    };
    assert!(
        scanned.unreadable.is_empty(),
        "{guard}: {relative} has {} line(s) inside its language list that this scanner cannot \
         read: {:?}. Each is a language NOT compared against the other platform, so it could \
         drift in silence. The scanner expects {shape}. Reformat the list; do not teach the \
         scanner a second shape.",
        scanned.unreadable.len(),
        scanned.unreadable
    );
    assert!(
        !scanned.languages.is_empty(),
        "{guard}: {relative}'s language list was found and holds no languages at all, so its \
         side of the check is VACUOUS. Either its contents were removed, or every entry sits on \
         a `#` line this scanner skips."
    );
    scanned.languages
}

// ---------------------------------------------------------------------------
// Test 1: each side, on its own.

/// Pins that each side's list is readable, non-empty and strictly ascending in `str`
/// order, which also rules out a language listed twice.
///
/// Order is a claim of its own, apart from membership, because it is what keeps the
/// two lists comparable by eye and lets a review of one side's diff find the other
/// side's line. A strictly ascending list also cannot hide a duplicate, which a set
/// comparison would pass.
///
/// ## Falsifier
///
/// Swap the `"java"` and `"javascript"` lines in `flake.nix`: this fails naming
/// `flake.nix` and that pair, while
/// [`the_flake_and_bazel_name_the_same_treesitter_languages`] stays green.
#[test]
fn each_side_lists_its_treesitter_languages_readably_and_in_alphabetical_order() {
    const GUARD: &str =
        "each_side_lists_its_treesitter_languages_readably_and_in_alphabetical_order";

    for relative in [FLAKE, BAZEL_BUILD] {
        let languages = require_languages(GUARD, relative);
        if let Some(pair) = languages.windows(2).find(|pair| pair[0] >= pair[1]) {
            let finding = if pair[0] == pair[1] {
                "lists the same language twice"
            } else {
                "is out of alphabetical order"
            };
            panic!(
                "{GUARD}: {relative}'s language list {finding}: {:?} comes directly before {:?}. \
                 Both lists must be strictly ascending, one entry per line, so the two files stay \
                 line-for-line comparable. Move or remove the entry in {relative}; the full list \
                 reads {languages:?}.",
                pair[0], pair[1]
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Test 2: the two sides agree.

/// The claim: `flake.nix` and `bazel/treesitter-parsers/BUILD.bazel` name the same
/// treesitter languages. With [`each_side_lists_its_treesitter_languages_readably_and_in_alphabetical_order`],
/// that makes them the same list in the same order.
///
/// Both directions are reported in one assertion, because a one-sided edit on each
/// platform at once is an ordinary shape — a language added on macOS while another is
/// dropped on Linux — and reporting one direction would send the reader round twice.
///
/// ## Falsifiers
///
/// Delete the `"go"` line from `flake.nix`'s list: this fails with `go` reported as
/// only in the Bazel file. Delete the `"@ts_rust//:rust_parser",` line from the Bazel
/// `srcs`: this fails with `rust` reported as only in `flake.nix`.
#[test]
fn the_flake_and_bazel_name_the_same_treesitter_languages() {
    const GUARD: &str = "the_flake_and_bazel_name_the_same_treesitter_languages";

    let nix = require_languages(GUARD, FLAKE);
    let bazel = require_languages(GUARD, BAZEL_BUILD);

    let only_nix: Vec<&String> = nix.iter().filter(|lang| !bazel.contains(lang)).collect();
    let only_bazel: Vec<&String> = bazel.iter().filter(|lang| !nix.contains(lang)).collect();

    assert!(
        only_nix.is_empty() && only_bazel.is_empty(),
        "{GUARD}: the two platforms vendor different treesitter languages.\n  \
         only in {FLAKE} (Linux): {only_nix:?}\n  \
         only in {BAZEL_BUILD} (macOS): {only_bazel:?}\n\
         \n\
         Neither build reads the other's list, so this one-sided edit builds green on both \
         platforms and one of them ships a language the other does not highlight. The fix is to \
         add or remove each language on BOTH sides in one change, as \
         bazel/treesitter-parsers/README.md's \"Adding a language\" describes. Do NOT edit this \
         test: it names no language of its own, only the two files' own words."
    );
}
