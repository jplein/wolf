//! Guards that the three vendored assets Wolf ships have ONE pin expressed in two
//! files, not two pins that happen to agree today.
//!
//! `flake.nix` fetches iA Writer Mono, orlando and lighthouse for the Linux
//! package and devShell; `bazel/<asset>/extensions.bzl` fetches the same three
//! tarballs for the macOS `.app`. Each side checks its own sha256 and neither reads
//! the other's, so a ONE-SIDED bump is completely silent: both platforms build
//! green, both tests pass, and Linux ships one revision of a plugin while macOS
//! bundles another. Nothing anywhere fails. This file is the only thing that would
//! notice.
//!
//! ## THIS TEST IS RED UNTIL `flake.nix` GAINS ITS `wolfVendoredPins` BLOCK
//!
//! It is written against the block the packaging unit adds, not against the file as
//! it stands, and that ordering is deliberate: the parser and the block are one
//! contract, and writing the parser after the block would let the block's shape
//! dictate the check instead of the other way round. A failure naming an ABSENT
//! block is therefore the expected state before the flake edit lands, and the fix
//! is to add the block in the shape [`nix_pins`] documents — **never** to loosen the
//! parser until it accepts whatever is there.
//!
//! ## The two extraction rules, stated so nobody invents a third
//!
//! Both sides are read by deliberately small line scanners, not by a Nix or Starlark
//! parser, and both therefore impose a STATED FORMATTING CONTRACT on the file they
//! read — exactly as [`release_profile.rs`](./release_profile.rs)'s manifest scanner
//! does on the workspace `Cargo.toml`. See [`bzl_values`] and [`nix_pins`]; the
//! contracts live on those functions, and `flake.nix` carries the reciprocal comment
//! above its block.
//!
//! `std` only, deliberately: a `regex` or `toml` dev-dependency would move
//! `Cargo.lock`, which is a `crate_universe` input in `MODULE.bazel`, creating a
//! repin obligation that cannot be discharged from a Linux host. That is the same
//! reason `release_profile.rs` records, and it applies with more force here, since
//! this file exists to keep the two platforms in step rather than to add a reason
//! they cannot be.
//!
//! ## What it does NOT check
//!
//! Nothing here fetches anything, so it cannot say that either sha256 is the hash of
//! the tarball at that commit. It says only that the two sides name the same commit
//! and the same hash. A pin that is wrong on both sides in the same way is caught by
//! the fetch, on whichever platform builds first.
//!
//! NO BAZEL WIRING, and none is missing. `crates/wolf-gnome` has no `BUILD.bazel` at
//! all — that absence is deliberate and load-bearing, see
//! [`notes/wolf-gnome-decisions.md`](../../../notes/wolf-gnome-decisions.md) — so there is no `rust_test`
//! target for this file to be added to, and no silent macOS-only gap for an unwired
//! file to fall into. Do not "fix" the missing target.
//!
//! ## What it needs
//!
//! Nothing: no compositor, no nvim, no built binary, no network. It reads four
//! files and runs in microseconds, on every `cargo test`.

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// What is pinned, and where each side writes it down.

/// The Starlark constant each `extensions.bzl` states its commit in.
const BZL_COMMIT_KEY: &str = "_COMMIT";

/// The Starlark constant each `extensions.bzl` states its tarball sha256 in.
const BZL_SHA256_KEY: &str = "_SHA256";

/// The line that opens the Nix-side block, as its trimmed form.
const NIX_BLOCK_OPEN: &str = "wolfVendoredPins = {";

/// The line that closes it, as its trimmed form. The FIRST one after the opener
/// wins, which is what makes a per-asset `};` on its own line a formatting error
/// rather than a silent truncation — see [`nix_pins`].
const NIX_BLOCK_CLOSE: &str = "};";

/// The Nix attribute inside one asset's entry that carries its commit.
const NIX_COMMIT_KEY: &str = "commit";

/// The Nix attribute inside one asset's entry that carries its tarball sha256.
const NIX_SHA256_KEY: &str = "sha256";

/// The workspace file that carries the Nix side of all three pins.
const FLAKE: &str = "flake.nix";

/// Every vendored asset, as `(the name its Nix entry uses, its Bazel extension)`.
///
/// The Nix names are re-typed here as LITERALS rather than read out of the block,
/// which is the whole point of the arity and naming assertions in
/// [`the_nix_side_pins_exactly_the_three_vendored_assets`]: reading the names out of
/// the file and then checking the file against them would hold for any set of names
/// at all, including a set with a fourth asset nobody vendored on the Bazel side.
const VENDORED_ASSETS: [(&str, &str); 3] = [
    ("ia-writer-mono", "bazel/ia-writer-mono/extensions.bzl"),
    ("orlando", "bazel/orlando/extensions.bzl"),
    ("lighthouse", "bazel/lighthouse/extensions.bzl"),
];

/// A git commit's length in hex characters.
const COMMIT_HEX_LEN: usize = 40;

/// A sha256's length in hex characters.
const SHA256_HEX_LEN: usize = 64;

// ---------------------------------------------------------------------------
// File helpers.

/// A workspace-relative path, resolved from this crate's own directory exactly as
/// `release_profile.rs::workspace_manifest_path` does.
///
/// This file lives in `wolf-gnome` for that file's reason: this crate has no
/// `BUILD.bazel` to wire a test into, and neither `flake.nix` nor
/// `bazel/*/extensions.bzl` is a Bazel runfile, so a `wolf-core` home would mean a
/// `rust_test` plus `data` deps on three files outside its package and a Bazel-side
/// check about a Nix-side fact.
fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// Reads a workspace file, or panics naming the path it tried.
fn read_workspace_file(guard: &str, relative: &str) -> (PathBuf, String) {
    let path = workspace_path(relative);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{guard}: could not read {}: {e}. That path is CARGO_MANIFEST_DIR joined with \
             `../../{relative}`, so an error here means this crate moved relative to the workspace \
             root, or the file was renamed — NOT that a pin drifted.",
            path.display()
        )
    });
    (path, text)
}

// ---------------------------------------------------------------------------
// The Bazel side.

/// Every double-quoted value assigned to the Starlark constant `key` in `text`.
///
/// STATED COUPLING with each `extensions.bzl`: a pin is written as `_COMMIT = "…"`
/// on a line of its own, with the constant first. A line qualifies when its
/// `trim_start()` begins with `key` and the remainder is `=` then a double-quoted
/// string, with any amount of whitespace around the `=`.
///
/// Prose mentions of the constants do not match, and that is checked rather than
/// hoped: every reference in the three docstrings and comments is either backticked
/// (`` `_COMMIT` ``) or embedded mid-sentence, so none of them begins a line with the
/// bare identifier followed by `=` and a quote. `_ARCHIVE_ROOT = "orlando-" + _COMMIT`
/// does not match either, because the line begins with a different identifier.
///
/// A Vec rather than an Option, so the caller can tell ZERO matches (the file was
/// reworded and this check has gone vacuous) from MORE THAN ONE (the file grew a
/// second pin and nothing here knows which is authoritative). Those are different
/// defects with different fixes and a single `Option` would report them identically.
fn bzl_values(text: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim_start().strip_prefix(key) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else {
            continue;
        };
        out.push(rest[..end].to_string());
    }
    out
}

/// The single value of `key` in `text`, or a panic naming which of the two failure
/// shapes it saw.
fn bzl_pin(guard: &str, path: &Path, text: &str, key: &str) -> String {
    let found = bzl_values(text, key);
    assert!(
        !found.is_empty(),
        "{guard}: {} states NO `{key} = \"…\"` line that this scanner can read, so the Bazel half \
         of this check is VACUOUS — it would accept any Nix-side value at all, because it can no \
         longer recover the value to compare against. The file was reworded, and there are two \
         shapes that do this: the CONSTANT WAS RENAMED, or the VALUE MOVED onto a continuation \
         line, which a one-line-at-a-time scanner cannot follow. Spacing is NOT one of them — \
         `{key}= \"…\"` with no space before the `=` reads fine, because `bzl_values` trims any \
         amount of whitespace on either side of the `=`, including none. Restore the one-line \
         form under this name; do NOT relax the comparison below to tolerate a missing Bazel \
         value, which is exactly what would let this zero-match state pass forever.",
        path.display()
    );
    assert!(
        found.len() == 1,
        "{guard}: {} states {} separate `{key} = \"…\"` lines ({found:?}). This check cannot know \
         which of them is the authoritative pin, so it refuses rather than comparing against \
         whichever came first. Either the extension now fetches two archives — in which case the \
         Nix side needs a second entry and this file needs a second row in VENDORED_ASSETS — or a \
         stale pin was left behind above the live one.",
        path.display(),
        found.len()
    );
    found[0].clone()
}

// ---------------------------------------------------------------------------
// The Nix side.

/// One asset's pin as `flake.nix` states it.
#[derive(Debug)]
struct NixPin {
    /// The attribute name, which must match a [`VENDORED_ASSETS`] row.
    name: String,
    /// The value of `commit`.
    commit: String,
    /// The value of `sha256`.
    sha256: String,
}

/// Consumes `<key> = "<value>";` from the head of `rest`, returning the value and
/// what follows it.
fn quoted_attr<'a>(rest: &'a str, key: &str) -> Option<(String, &'a str)> {
    let rest = rest.trim_start().strip_prefix(key)?;
    let rest = rest.trim_start().strip_prefix('=')?;
    let rest = rest.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    let value = rest[..end].to_string();
    let after = rest[end + 1..].trim_start().strip_prefix(';')?;
    Some((value, after))
}

/// Parses one line of the `wolfVendoredPins` block, or `None` when it is not one.
///
/// The shape, which is the formatting contract `flake.nix`'s own comment states:
///
/// ```text
/// <name> = { commit = "<40 hex>"; sha256 = "<64 hex>"; };
/// ```
///
/// One attribute set per line, `commit` before `sha256`, both values double-quoted,
/// the closing `};` on the SAME line. The order is fixed rather than free because
/// this is a scanner and not a parser, and a free order would mean either two passes
/// or a state machine for no gain.
fn parse_pin_line(line: &str) -> Option<NixPin> {
    let (name, rest) = line.trim().split_once('=')?;
    let name = name.trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return None;
    }
    let rest = rest.trim_start().strip_prefix('{')?;
    let (commit, rest) = quoted_attr(rest, NIX_COMMIT_KEY)?;
    let (sha256, rest) = quoted_attr(rest, NIX_SHA256_KEY)?;
    let rest = rest.trim_start().strip_prefix('}')?;
    let rest = rest.trim();
    if !(rest.is_empty() || rest == ";") {
        return None;
    }
    Some(NixPin {
        name: name.to_string(),
        commit,
        sha256,
    })
}

/// Every pin in `flake.nix`'s `wolfVendoredPins` block, plus the lines inside the
/// block this scanner could NOT read.
///
/// STATED COUPLING with `flake.nix`, in two parts. The BLOCK is delimited by a line
/// whose trimmed form is exactly `wolfVendoredPins = {` and the first line after it
/// whose trimmed form is exactly `};` — so an entry that spread its own attribute set
/// across several lines would end the block early rather than being read. Each LINE
/// between them carries one whole asset, in the shape [`parse_pin_line`] documents.
///
/// `None` for the whole result means the opening line is absent. That is reported
/// separately from an empty block, because "the packaging unit has not landed yet"
/// and "the block was emptied" are different findings.
fn nix_pins(text: &str) -> Option<(Vec<NixPin>, Vec<String>)> {
    let mut lines = text.lines();
    lines.find(|line| line.trim() == NIX_BLOCK_OPEN)?;

    let mut pins = Vec::new();
    let mut unreadable = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed == NIX_BLOCK_CLOSE {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match parse_pin_line(line) {
            Some(pin) => pins.push(pin),
            None => unreadable.push(trimmed.to_string()),
        }
    }
    Some((pins, unreadable))
}

/// Reads the block, or panics with the finding that applies.
fn require_nix_pins(guard: &str, path: &Path, text: &str) -> Vec<NixPin> {
    let Some((pins, unreadable)) = nix_pins(text) else {
        panic!(
            "{guard}: {} contains no line whose trimmed form is exactly `{NIX_BLOCK_OPEN}`, so the \
             Nix side of all three pins could not be located and this check is comparing nothing. \
             Two states look like this and both are actionable. BEFORE the packaging unit lands, \
             the block simply does not exist yet and this failure is expected — add it, in the \
             shape `parse_pin_line`'s doc comment states. AFTER it lands, the binding was renamed \
             or reformatted, in which case fix `flake.nix` or this constant so the two agree \
             again. Do NOT widen the scanner to search for something looser: a matcher that finds \
             a block wherever one might be is a matcher that reports a stale one.",
            path.display()
        )
    };
    assert!(
        unreadable.is_empty(),
        "{guard}: {} has {} line(s) inside its `{NIX_BLOCK_OPEN}` block that this scanner cannot \
         read: {unreadable:?}. Every such line is a pin that is NOT being compared against Bazel, \
         so it would drift in silence — which is the entire failure this file exists to catch. \
         The required shape is one asset per line, `<name> = {{ commit = \"…\"; sha256 = \"…\"; \
         }};`, `commit` before `sha256`, both values double-quoted, the closing brace on the same \
         line. Reformat the block; do not teach the scanner a second shape, because a scanner with \
         two shapes has no contract to state in `flake.nix`'s comment.",
        path.display(),
        unreadable.len()
    );
    pins
}

/// Whether `value` is exactly `len` lowercase hex characters.
fn is_lowercase_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

// ---------------------------------------------------------------------------
// Test 1: the Nix block's arity, names and value shapes.

/// Pins that `flake.nix` states exactly the three assets this file knows how to
/// compare, under exactly the names [`VENDORED_ASSETS`] uses.
///
/// ## Why arity is a claim rather than a formality
///
/// A FOURTH entry is not a harmless addition. Every entry in that block is an asset
/// the Linux package ships, and an entry with no `VENDORED_ASSETS` row is an asset
/// whose Bazel counterpart nothing checks — so the two platforms would be free to
/// diverge on it from the day it was added, which is the drift this file exists for.
/// A MISSING entry is the same defect from the other end: the loop below would have
/// nothing to compare and would pass while one of the three went unguarded.
///
/// ## Falsifiers
///
/// Add a fourth entry to `wolfVendoredPins` — assertion 2 fires and names it. Rename
/// one entry — assertion 3 fires and shows both sets. Truncate a sha256 by a
/// character — assertion 4 fires, rather than the mismatch loop reporting it as a
/// drift it is not.
#[test]
fn the_nix_side_pins_exactly_the_three_vendored_assets() {
    const GUARD: &str = "the_nix_side_pins_exactly_the_three_vendored_assets";

    let (path, text) = read_workspace_file(GUARD, FLAKE);
    let pins = require_nix_pins(GUARD, &path, &text);

    // --- Assertion 1 (non-vacuity): the block is not empty --------------------
    assert!(
        !pins.is_empty(),
        "{GUARD}: {}'s `{NIX_BLOCK_OPEN}` block holds no entries at all. An empty block makes \
         every comparison below vacuous — there is nothing to compare — and it is a DIFFERENT \
         defect from an absent block, which `require_nix_pins` reports separately: this one means \
         the binding survived and its contents were removed.",
        path.display()
    );

    // --- Assertion 2: exactly three --------------------------------------------
    let names: Vec<&str> = pins.iter().map(|p| p.name.as_str()).collect();
    assert!(
        pins.len() == VENDORED_ASSETS.len(),
        "{GUARD}: {}'s `{NIX_BLOCK_OPEN}` block holds {} entries ({names:?}), expected exactly {} \
         ({:?}). An EXTRA entry is an asset the Linux package pins with no Bazel counterpart in \
         this file's table, so nothing compares it and macOS is free to carry a different revision \
         from the day it was added. A MISSING one is the same hole from the other side. Add the \
         asset's Bazel extension to VENDORED_ASSETS, or remove the entry.",
        path.display(),
        pins.len(),
        VENDORED_ASSETS.len(),
        VENDORED_ASSETS.map(|(name, _)| name)
    );

    // --- Assertion 3: the three names, exactly ---------------------------------
    for (expected, bzl) in VENDORED_ASSETS {
        assert!(
            pins.iter().any(|p| p.name == expected),
            "{GUARD}: {}'s `{NIX_BLOCK_OPEN}` block has no entry named {expected:?}; it names \
             {names:?}. That asset's Bazel pin lives in `{bzl}` and this file compares the two by \
             NAME, so a renamed entry silently stops being compared while the block's arity still \
             looks right.",
            path.display()
        );
    }

    // --- Assertion 4: the value shapes -----------------------------------------
    // Checked here, not in the mismatch loop, so a malformed hex string is not reported as drift.
    for pin in &pins {
        assert!(
            is_lowercase_hex(&pin.commit, COMMIT_HEX_LEN),
            "{GUARD}: {}'s {:?} entry states commit {:?}, which is not {COMMIT_HEX_LEN} lowercase \
             hex characters. `fetchurl` would fail on a bad commit, so this is caught eventually — \
             but it is caught HERE, before the mismatch loop reports it as a drift against Bazel \
             that it is not.",
            path.display(),
            pin.name,
            pin.commit
        );
        assert!(
            is_lowercase_hex(&pin.sha256, SHA256_HEX_LEN),
            "{GUARD}: {}'s {:?} entry states sha256 {:?}, which is not {SHA256_HEX_LEN} lowercase \
             hex characters. Both sides write the hash as bare hex (not SRI, not base32), which is \
             what makes the string comparison below meaningful; a different encoding on one side \
             would read as a drift on every asset at once.",
            path.display(),
            pin.name,
            pin.sha256
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: the Bazel side is readable at all.

/// Pins that every `extensions.bzl` states exactly one commit and exactly one
/// sha256 that [`bzl_values`] can read.
///
/// This is the non-vacuity half, and it is a test of its own rather than a line
/// inside the comparison because its failure means something different: not that the
/// two platforms disagree, but that this file can no longer see one of them. A
/// comparison that silently treated an unreadable Bazel pin as "no finding" would
/// accept any Nix value at all — the shape
/// `smoke_headless.rs::cage_is_only_spawned_through_the_isolating_helper` records as
/// a guard that stopped guarding.
///
/// ## Falsifier
///
/// RENAME the constant: `_COMMIT` → `_REV` in `bazel/lighthouse/extensions.bzl`, at
/// its definition and both of its uses (`_ARCHIVE_ROOT` and the `urls` format), which
/// leaves the file valid Starlark. The zero-match arm fires naming that file and the
/// word VACUOUS, rather than the comparison passing. Moving the value onto a
/// continuation line does the same, since this scanner reads one line at a time.
///
/// ## A recipe that does NOT falsify it, recorded so nobody spends the run twice
///
/// Rewording `_COMMIT = "` to `_COMMIT= "` — no space before the `=` — leaves this
/// test GREEN, and that is correct behaviour rather than a hole: [`bzl_values`] trims
/// any amount of whitespace on either side of the `=`, including none, which is what
/// its own stated contract says. A pin is no less readable for a missing space, and
/// tightening the scanner to reject one would only turn a formatting change into a
/// failure that names the wrong thing.
#[test]
fn every_bazel_extension_states_exactly_one_commit_and_one_sha256() {
    const GUARD: &str = "every_bazel_extension_states_exactly_one_commit_and_one_sha256";

    for (asset, relative) in VENDORED_ASSETS {
        let (path, text) = read_workspace_file(GUARD, relative);
        let commit = bzl_pin(GUARD, &path, &text, BZL_COMMIT_KEY);
        let sha256 = bzl_pin(GUARD, &path, &text, BZL_SHA256_KEY);

        assert!(
            is_lowercase_hex(&commit, COMMIT_HEX_LEN),
            "{GUARD}: {relative} states {BZL_COMMIT_KEY} = {commit:?} for {asset:?}, which is not \
             {COMMIT_HEX_LEN} lowercase hex characters. The scanner read SOMETHING, so this is a \
             malformed pin rather than an unreadable file."
        );
        assert!(
            is_lowercase_hex(&sha256, SHA256_HEX_LEN),
            "{GUARD}: {relative} states {BZL_SHA256_KEY} = {sha256:?} for {asset:?}, which is not \
             {SHA256_HEX_LEN} lowercase hex characters. Both sides write the hash as bare hex, \
             which is what makes the comparison in \
             `the_nix_and_bazel_pins_name_the_same_commit_and_sha256_for_every_vendored_asset` a \
             string equality at all."
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: the two sides agree, asset by asset.

/// One disagreement between the two files, kept so a run reports ALL of them.
struct Mismatch {
    /// Which vendored asset.
    asset: &'static str,
    /// `commit` or `sha256`.
    field: &'static str,
    /// What `flake.nix` says.
    nix: String,
    /// What the asset's `extensions.bzl` says.
    bazel: String,
    /// The Bazel file the value came from, workspace-relative.
    bzl_file: &'static str,
}

/// The claim: for each of the three vendored assets, `flake.nix` and
/// `bazel/<asset>/extensions.bzl` name the SAME commit and the SAME sha256.
///
/// ## Why the mismatches are collected rather than asserted in a loop
///
/// `crates/wolf-gnome/tests/README.md` records an observed case where a revert broke
/// two rows of a `for`-loop-with-an-assertion and produced exactly one reported
/// failure, so the second row's breakage was invisible until the first was fixed. A
/// one-sided bump of two assets at once is the ordinary shape here — someone bumps
/// orlando and lighthouse together on macOS — so reporting one of the two would send
/// the reader round the loop twice.
///
/// ## Falsifiers
///
/// Change one hex digit of `wolfVendoredPins.orlando.commit`; the report names
/// orlando's commit and both files. Change one digit of
/// `wolfVendoredPins.lighthouse.sha256`; it names lighthouse's sha256. Change
/// `ia-writer-mono`'s commit; it names THAT asset, which is what proves all three are
/// compared rather than the first. Change two assets at once; TWO mismatches are
/// reported, not one.
#[test]
fn the_nix_and_bazel_pins_name_the_same_commit_and_sha256_for_every_vendored_asset() {
    const GUARD: &str =
        "the_nix_and_bazel_pins_name_the_same_commit_and_sha256_for_every_vendored_asset";

    let (flake_path, flake_text) = read_workspace_file(GUARD, FLAKE);
    let pins = require_nix_pins(GUARD, &flake_path, &flake_text);

    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut compared = 0usize;

    for (asset, relative) in VENDORED_ASSETS {
        // A missing entry is `the_nix_side_pins_exactly_the_three_vendored_assets`'s finding;
        // skipping keeps the two failures distinct.
        let Some(pin) = pins.iter().find(|p| p.name == asset) else {
            continue;
        };

        let (bzl_path, bzl_text) = read_workspace_file(GUARD, relative);
        let bzl_commit = bzl_pin(GUARD, &bzl_path, &bzl_text, BZL_COMMIT_KEY);
        let bzl_sha256 = bzl_pin(GUARD, &bzl_path, &bzl_text, BZL_SHA256_KEY);
        compared += 1;

        if pin.commit != bzl_commit {
            mismatches.push(Mismatch {
                asset,
                field: NIX_COMMIT_KEY,
                nix: pin.commit.clone(),
                bazel: bzl_commit,
                bzl_file: relative,
            });
        }
        if pin.sha256 != bzl_sha256 {
            mismatches.push(Mismatch {
                asset,
                field: NIX_SHA256_KEY,
                nix: pin.sha256.clone(),
                bazel: bzl_sha256,
                bzl_file: relative,
            });
        }
    }

    // --- Assertion 1 (non-vacuity): all three were really compared ------------
    // Without it, a block whose entries were all renamed compares nothing and passes.
    assert!(
        compared == VENDORED_ASSETS.len(),
        "{GUARD}: only {compared} of {} vendored assets were compared at all, so this check is \
         partly vacuous — the assets it skipped could carry any Bazel revision and nothing here \
         would report it. An asset is skipped when {}'s block has no entry under the name this \
         file expects; `the_nix_side_pins_exactly_the_three_vendored_assets` names which.",
        VENDORED_ASSETS.len(),
        flake_path.display()
    );

    // --- Assertion 2: the two sides agree, on every asset and both fields -----
    assert!(
        mismatches.is_empty(),
        "{GUARD}: {} vendored pin(s) differ between the Nix side and the Bazel side:{}\n\
         \n\
         A ONE-SIDED BUMP IS SILENT, which is why this test exists. Each side fetches its own \
         tarball and verifies it against its OWN sha256, and neither side ever reads the other's — \
         so `nix build` succeeds, `bazel build` succeeds, every other test on both platforms stays \
         green, and Linux ships one revision of a vendored plugin while the macOS `.app` bundles \
         another. Nothing else in this repository would notice.\n\
         \n\
         The fix is to BUMP BOTH TOGETHER — set the same commit and the same sha256 in {} and in \
         the named `bazel/<asset>/extensions.bzl`, then regenerate `MODULE.bazel.lock`. Do NOT \
         'fix' this by editing the expected value in this test: there is no expected value here, \
         only the two files' own words.",
        mismatches.len(),
        mismatches
            .iter()
            .map(|m| format!(
                "\n  * {} {}:\n      {} says {:?}\n      {} says {:?}",
                m.asset, m.field, FLAKE, m.nix, m.bzl_file, m.bazel
            ))
            .collect::<Vec<_>>()
            .join(""),
        flake_path.display()
    );
}
