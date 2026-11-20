//! Guards that every vendored asset Wolf ships has ONE pin expressed in two files,
//! not two pins that happen to agree today.
//!
//! `flake.nix` fetches each asset in [`VENDORED_ASSETS`] for the Linux package and
//! devShell; `bazel/<asset>/extensions.bzl` fetches the same archive for the macOS
//! `.app`. Each side checks its own sha256 and neither reads the other's, so a
//! ONE-SIDED bump is completely silent: both platforms build green, both tests pass,
//! and Linux ships one revision of an asset while macOS bundles another. Nothing
//! anywhere fails. This file is the only thing that would notice.
//!
//! An asset is pinned either to a git commit or to a release version, and its row in
//! [`VENDORED_ASSETS`] says which; [`RevisionKind`] holds the key and the value shape
//! each kind uses on both sides.
//!
//! ## The parser defines the contract
//!
//! The parser and the `wolfVendoredPins` block in `flake.nix` are one contract, and
//! the parser is its definition: the block takes the shape [`nix_pins`] documents,
//! so the check is not dictated by whatever shape the block happens to have. When
//! the two disagree, the fix is to restore that shape — **never** to loosen the
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
//! the archive at that revision. It says only that the two sides name the same
//! revision and the same hash. A pin that is wrong on both sides in the same way is
//! caught by the fetch, on whichever platform builds first.
//!
//! NO BAZEL WIRING, and none is missing. `crates/wolf-gnome` has no `BUILD.bazel` at
//! all — that absence is deliberate and load-bearing, see
//! [`notes/wolf-gnome-decisions.md`](../../../notes/wolf-gnome-decisions.md) — so there is no `rust_test`
//! target for this file to be added to, and no silent macOS-only gap for an unwired
//! file to fall into. Do not "fix" the missing target.
//!
//! ## What it needs
//!
//! Nothing: no compositor, no nvim, no built binary, no network. It reads `flake.nix`
//! and each asset's extension, and runs in microseconds, on every `cargo test`.

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// What is pinned, and where each side writes it down.

/// The Starlark constant a commit-pinned `extensions.bzl` states its commit in.
const BZL_COMMIT_KEY: &str = "_COMMIT";

/// The Starlark constant a version-pinned `extensions.bzl` states its release in.
const BZL_VERSION_KEY: &str = "_VERSION";

/// The Starlark constant each `extensions.bzl` states its archive sha256 in.
const BZL_SHA256_KEY: &str = "_SHA256";

/// The line that opens the Nix-side block, as its trimmed form.
const NIX_BLOCK_OPEN: &str = "wolfVendoredPins = {";

/// The line that closes it, as its trimmed form. The FIRST one after the opener
/// wins, which is what makes a per-asset `};` on its own line a formatting error
/// rather than a silent truncation — see [`nix_pins`].
const NIX_BLOCK_CLOSE: &str = "};";

/// The Nix attribute inside a commit-pinned asset's entry that carries its commit.
const NIX_COMMIT_KEY: &str = "commit";

/// The Nix attribute inside a version-pinned asset's entry that carries its release.
const NIX_VERSION_KEY: &str = "version";

/// The Nix attribute inside one asset's entry that carries its archive sha256.
const NIX_SHA256_KEY: &str = "sha256";

/// The workspace file that carries the Nix side of every pin.
const FLAKE: &str = "flake.nix";

/// How an asset is pinned. The row in [`VENDORED_ASSETS`] decides, and both sides
/// must state the revision under the key that kind names.
///
/// Neither kind is a fallback for the other: an asset whose row says `Commit` and
/// whose Nix entry says `version` is a defect, reported as a kind mismatch, because
/// accepting either key for any asset would let a commit pin turn into a version
/// string with nothing noticing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RevisionKind {
    /// A git commit: 40 lowercase hex characters.
    Commit,
    /// A release version: dot-separated runs of ASCII digits, such as `3.5.1`.
    Version,
}

impl RevisionKind {
    /// The Nix attribute this kind's revision is stated under.
    fn nix_key(self) -> &'static str {
        match self {
            RevisionKind::Commit => NIX_COMMIT_KEY,
            RevisionKind::Version => NIX_VERSION_KEY,
        }
    }

    /// The Starlark constant this kind's revision is stated in.
    fn bzl_key(self) -> &'static str {
        match self {
            RevisionKind::Commit => BZL_COMMIT_KEY,
            RevisionKind::Version => BZL_VERSION_KEY,
        }
    }

    /// The value shape this kind requires, for failure messages.
    fn shape(self) -> &'static str {
        match self {
            RevisionKind::Commit => "40 lowercase hex characters",
            RevisionKind::Version => "dot-separated runs of ASCII digits, such as `3.5.1`",
        }
    }

    /// Whether `value` has this kind's shape.
    fn is_well_formed(self, value: &str) -> bool {
        match self {
            RevisionKind::Commit => is_lowercase_hex(value, COMMIT_HEX_LEN),
            RevisionKind::Version => value
                .split('.')
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())),
        }
    }
}

/// Every vendored asset, as `(the name its Nix entry uses, its Bazel extension, how
/// it is pinned)`.
///
/// The Nix names are re-typed here as LITERALS rather than read out of the block,
/// which is the whole point of the arity and naming assertions in
/// [`the_nix_side_pins_exactly_the_vendored_assets`]: reading the names out of the
/// file and then checking the file against them would hold for any set of names at
/// all, including one with an asset nobody vendored on the Bazel side. The kind is a
/// literal for the same reason.
const VENDORED_ASSETS: [(&str, &str, RevisionKind); 4] = [
    (
        "ia-writer-mono",
        "bazel/ia-writer-mono/extensions.bzl",
        RevisionKind::Commit,
    ),
    (
        "orlando",
        "bazel/orlando/extensions.bzl",
        RevisionKind::Commit,
    ),
    (
        "lighthouse",
        "bazel/lighthouse/extensions.bzl",
        RevisionKind::Commit,
    ),
    (
        "nerd-fonts-symbols",
        "bazel/nerd-fonts-symbols/extensions.bzl",
        RevisionKind::Version,
    ),
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
/// `rust_test` plus `data` deps on files outside its package and a Bazel-side check
/// about a Nix-side fact.
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
             root, or the file was renamed or not yet written — NOT that a pin drifted.",
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
/// or `_VERSION = "…"`, and `_SHA256 = "…"`, each on a line of its own with the
/// constant first. A line qualifies when its `trim_start()` begins with `key` and the
/// remainder is `=` then a double-quoted string, with any amount of whitespace around
/// the `=`.
///
/// Prose mentions of the constants do not match: a reference in a docstring or
/// comment is backticked (`` `_COMMIT` ``) or embedded mid-sentence, so it does not
/// begin a line with the bare identifier followed by `=` and a quote.
/// `_ARCHIVE_ROOT = "orlando-" + _COMMIT` and `_URL = "…".format(v = _VERSION)` do not
/// match either, because each line begins with a different identifier.
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
    /// Which key the entry stated its revision under.
    kind: RevisionKind,
    /// The value of `commit` or `version`.
    revision: String,
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
/// <name> = { version = "<release>"; sha256 = "<64 hex>"; };
/// ```
///
/// One attribute set per line, the revision (`commit` or `version`) before `sha256`,
/// both values double-quoted, the closing `};` on the SAME line. Any other first key
/// leaves the line unreadable. The order is fixed rather than free because this is a
/// scanner and not a parser, and a free order would mean either two passes or a state
/// machine for no gain.
///
/// Accepting either revision key is not a loosening: the line records which key it
/// used, and [`the_nix_side_pins_exactly_the_vendored_assets`] requires it to be the
/// one the asset's row names.
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
    let (kind, revision, rest) = [RevisionKind::Commit, RevisionKind::Version]
        .into_iter()
        .find_map(|kind| {
            quoted_attr(rest, kind.nix_key()).map(|(value, after)| (kind, value, after))
        })?;
    let (sha256, rest) = quoted_attr(rest, NIX_SHA256_KEY)?;
    let rest = rest.trim_start().strip_prefix('}')?;
    let rest = rest.trim();
    if !(rest.is_empty() || rest == ";") {
        return None;
    }
    Some(NixPin {
        name: name.to_string(),
        kind,
        revision,
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
/// separately from an empty block, because "the block is absent" and "the block
/// was emptied" are different findings.
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
             Nix side of every pin could not be located and this check is comparing nothing. The \
             binding was renamed or reformatted: fix `flake.nix` or this constant so the two agree \
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
         }};` or the same with `version` in place of `commit`: the revision key first, both \
         values double-quoted, the closing brace on the same line. Reformat the block; do not \
         teach the scanner another shape, because a scanner with open-ended shapes has no \
         contract to state in `flake.nix`'s comment.",
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
// Test 1: the Nix block's arity, names, revision kinds and value shapes.

/// Pins that `flake.nix` states exactly the assets this file knows how to compare,
/// under exactly the names [`VENDORED_ASSETS`] uses, each under the revision key its
/// row names.
///
/// ## Why arity is a claim rather than a formality
///
/// An EXTRA entry is not a harmless addition. Every entry in that block is an asset
/// the Linux package ships, and an entry with no `VENDORED_ASSETS` row is an asset
/// whose Bazel counterpart nothing checks — so the two platforms would be free to
/// diverge on it from the day it was added, which is the drift this file exists for.
/// A MISSING entry is the same defect from the other end: the loop below would have
/// nothing to compare and would pass while that asset went unguarded.
///
/// ## Falsifiers
///
/// The Nix side cannot take a value edit that breaks its own fetch, because
/// `nix develop` then fails before this test runs. So each recipe keeps the flake
/// evaluating. Rename the `nerd-fonts-symbols` entry to `nerd-fonts`, and its
/// reference in `wolfNerdFontsSymbols`; assertion 3 fires and shows both sets. Change
/// that entry's key `version` to `commit`, and `pin.version` to `pin.commit`;
/// assertion 4 fires naming the kind mismatch, rather than assertion 5 reporting
/// `3.5.1` as a malformed commit.
#[test]
fn the_nix_side_pins_exactly_the_vendored_assets() {
    const GUARD: &str = "the_nix_side_pins_exactly_the_vendored_assets";

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

    // --- Assertion 2: exactly the table's arity --------------------------------
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
        VENDORED_ASSETS.map(|(name, _, _)| name)
    );

    // --- Assertion 3: the table's names, exactly -------------------------------
    for (expected, bzl, _) in VENDORED_ASSETS {
        assert!(
            pins.iter().any(|p| p.name == expected),
            "{GUARD}: {}'s `{NIX_BLOCK_OPEN}` block has no entry named {expected:?}; it names \
             {names:?}. That asset's Bazel pin lives in `{bzl}` and this file compares the two by \
             NAME, so a renamed entry silently stops being compared while the block's arity still \
             looks right.",
            path.display()
        );
    }

    for pin in &pins {
        let Some(&(_, bzl, kind)) = VENDORED_ASSETS
            .iter()
            .find(|(name, _, _)| *name == pin.name)
        else {
            panic!(
                "{GUARD}: {}'s entry {:?} has no VENDORED_ASSETS row, though assertions 2 and 3 \
                 passed, which together leave every entry exactly one row. The assertions above \
                 were weakened; restore them rather than skipping this entry.",
                path.display(),
                pin.name
            )
        };

        // --- Assertion 4: the revision key is the one the row names -----------
        // Before the shape check, so a kind mismatch is not reported as a malformed value.
        assert!(
            pin.kind == kind,
            "{GUARD}: {}'s {:?} entry states its revision under `{}`, but VENDORED_ASSETS pins that \
             asset by `{}`, which `{bzl}` states as `{}`. The row decides the kind, so the two \
             sides compare like with like; a commit-pinned asset written with `version`, or the \
             reverse, is a different pin from the one Bazel fetches even when the strings agree. \
             Restore the row's key in `flake.nix`, or change the row and the extension together.",
            path.display(),
            pin.name,
            pin.kind.nix_key(),
            kind.nix_key(),
            kind.bzl_key()
        );

        // --- Assertion 5: the value shapes -------------------------------------
        // Checked here, not in the mismatch loop, so a malformed value is not reported as drift.
        assert!(
            kind.is_well_formed(&pin.revision),
            "{GUARD}: {}'s {:?} entry states {} {:?}, which is not {}. The fetch would fail on a \
             bad revision, so this is caught eventually — but it is caught HERE, before the \
             mismatch loop reports it as a drift against Bazel that it is not.",
            path.display(),
            pin.name,
            kind.nix_key(),
            pin.revision,
            kind.shape()
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

/// Pins that every `extensions.bzl` states exactly one revision, under the constant
/// its row's kind names, and exactly one sha256 that [`bzl_values`] can read.
///
/// This is the non-vacuity half, and it is a test of its own rather than a line
/// inside the comparison because its failure means something different: not that the
/// two platforms disagree, but that this file can no longer see one of them. A
/// comparison that silently treated an unreadable Bazel pin as "no finding" would
/// accept any Nix value at all — the shape
/// `smoke_headless/main.rs`'s `cage_is_only_spawned_through_the_isolating_helper` records as
/// a guard that stopped guarding.
///
/// ## Falsifier
///
/// RENAME the constant: `_VERSION` → `_REL` in `bazel/nerd-fonts-symbols/extensions.bzl`,
/// at its definition and its use in `_URL`, which leaves the file valid Starlark. The
/// zero-match arm fires naming that file and the word VACUOUS, rather than the
/// comparison passing. Moving the value onto a continuation line does the same, since
/// this scanner reads one line at a time.
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
fn every_bazel_extension_states_exactly_one_revision_and_one_sha256() {
    const GUARD: &str = "every_bazel_extension_states_exactly_one_revision_and_one_sha256";

    for (asset, relative, kind) in VENDORED_ASSETS {
        let (path, text) = read_workspace_file(GUARD, relative);
        let revision = bzl_pin(GUARD, &path, &text, kind.bzl_key());
        let sha256 = bzl_pin(GUARD, &path, &text, BZL_SHA256_KEY);

        assert!(
            kind.is_well_formed(&revision),
            "{GUARD}: {relative} states {} = {revision:?} for {asset:?}, which is not {}. The \
             scanner read SOMETHING, so this is a malformed pin rather than an unreadable file.",
            kind.bzl_key(),
            kind.shape()
        );
        assert!(
            is_lowercase_hex(&sha256, SHA256_HEX_LEN),
            "{GUARD}: {relative} states {BZL_SHA256_KEY} = {sha256:?} for {asset:?}, which is not \
             {SHA256_HEX_LEN} lowercase hex characters. Both sides write the hash as bare hex, \
             which is what makes the comparison in \
             `the_nix_and_bazel_pins_name_the_same_revision_and_sha256_for_every_vendored_asset` \
             a string equality at all."
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: the two sides agree, asset by asset.

/// One disagreement between the two files, kept so a run reports ALL of them.
struct Mismatch {
    /// Which vendored asset.
    asset: &'static str,
    /// `commit`, `version` or `sha256`.
    field: &'static str,
    /// What `flake.nix` says.
    nix: String,
    /// What the asset's `extensions.bzl` says.
    bazel: String,
    /// The Bazel file the value came from, workspace-relative.
    bzl_file: &'static str,
}

/// The claim: for every vendored asset, `flake.nix` and `bazel/<asset>/extensions.bzl`
/// name the SAME revision and the SAME sha256.
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
/// Each edits the Bazel side, because a Nix-side value edit breaks the flake's own
/// fetch and `nix develop` with it. Change one hex digit of `_SHA256` in
/// `bazel/nerd-fonts-symbols/extensions.bzl`; the report names nerd-fonts-symbols'
/// sha256 and both files. Set `_VERSION = "3.5.0"` there; it names
/// nerd-fonts-symbols' version. Change a digit of `_SHA256` in two extensions at
/// once; TWO mismatches are reported, not one.
#[test]
fn the_nix_and_bazel_pins_name_the_same_revision_and_sha256_for_every_vendored_asset() {
    const GUARD: &str =
        "the_nix_and_bazel_pins_name_the_same_revision_and_sha256_for_every_vendored_asset";

    let (flake_path, flake_text) = read_workspace_file(GUARD, FLAKE);
    let pins = require_nix_pins(GUARD, &flake_path, &flake_text);

    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut compared = 0usize;

    for (asset, relative, kind) in VENDORED_ASSETS {
        // A missing entry is `the_nix_side_pins_exactly_the_vendored_assets`'s finding;
        // skipping keeps the two failures distinct.
        let Some(pin) = pins.iter().find(|p| p.name == asset) else {
            continue;
        };

        let (bzl_path, bzl_text) = read_workspace_file(GUARD, relative);
        let bzl_revision = bzl_pin(GUARD, &bzl_path, &bzl_text, kind.bzl_key());
        let bzl_sha256 = bzl_pin(GUARD, &bzl_path, &bzl_text, BZL_SHA256_KEY);
        compared += 1;

        if pin.revision != bzl_revision {
            mismatches.push(Mismatch {
                asset,
                field: kind.nix_key(),
                nix: pin.revision.clone(),
                bazel: bzl_revision,
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

    // --- Assertion 1 (non-vacuity): every asset was really compared -----------
    // Without it, a block whose entries were all renamed compares nothing and passes.
    assert!(
        compared == VENDORED_ASSETS.len(),
        "{GUARD}: only {compared} of {} vendored assets were compared at all, so this check is \
         partly vacuous — the assets it skipped could carry any Bazel revision and nothing here \
         would report it. An asset is skipped when {}'s block has no entry under the name this \
         file expects; `the_nix_side_pins_exactly_the_vendored_assets` names which.",
        VENDORED_ASSETS.len(),
        flake_path.display()
    );

    // --- Assertion 2: the two sides agree, on every asset and both fields -----
    assert!(
        mismatches.is_empty(),
        "{GUARD}: {} vendored pin(s) differ between the Nix side and the Bazel side:{}\n\
         \n\
         A ONE-SIDED BUMP IS SILENT, which is why this test exists. Each side fetches its own \
         archive and verifies it against its OWN sha256, and neither side ever reads the other's — \
         so `nix build` succeeds, `bazel build` succeeds, every other test on both platforms stays \
         green, and Linux ships one revision of a vendored asset while the macOS `.app` bundles \
         another. Nothing else in this repository would notice.\n\
         \n\
         The fix is to BUMP BOTH TOGETHER — set the same revision and the same sha256 in {} and \
         in the named `bazel/<asset>/extensions.bzl`, then regenerate `MODULE.bazel.lock`. Do NOT \
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
