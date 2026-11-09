//! Headless tests of the linked-library licence registry, `wolf_core::LIBRARY_LICENSES`,
//! and the block built from it, `wolf_core::LIBRARY_LICENSE_NOTICES`: libghostty-vt and
//! the third-party code its static archive carries, whose notices both About surfaces
//! show.
//!
//! They mirror the font registry's in-source tests in `src/licenses.rs`, for the same
//! reasons: a row whose `include_str!` points at an empty or wrong file still compiles,
//! and two rows with swapped paths ship every text attributed to the wrong library.
//! No Neovim, no filesystem and no runfiles.
//!
//! ## Surface under test
//!
//! - `pub struct LibraryLicense { pub library: &'static str, pub license: &'static str,
//!   pub provenance: &'static str, pub text: &'static str }`
//! - `pub const LIBRARY_LICENSES: &[LibraryLicense]`, seven rows
//! - `pub const LIBRARY_LICENSE_NOTICES: &str`, a preamble, then per row a header block
//!   naming `library — license` between two `====` rules, the text, and a `"\n"`.

use wolf_core::{LibraryLicense, LIBRARY_LICENSES, LIBRARY_LICENSE_NOTICES};

/// How many libraries the binary links code from, stated as a literal:
/// libghostty-vt, Highway, simdutf, translate-c, uucode, the Unicode Character
/// Database uucode compiles in, and Wuffs.
///
/// `LIBRARY_LICENSES.len()` compared against itself would accept zero rows, at which
/// point every "for each row" check below passes over nothing.
const EXPECT_LIBRARY_COUNT: usize = 7;

/// The shortest a real licence text can plausibly be, in bytes.
///
/// The smallest of the seven is Wuffs' MIT text at 1057 bytes, and the largest the
/// Unicode License V3 at 1994. The floor fails an `include_str!` of an empty or
/// truncated file while clearing every real one.
const MIN_LICENCE_BYTES: usize = 500;

/// The distinctive phrase from each library's licence text, re-typed here.
///
/// Five of the seven texts are MIT, so a phrase from the shared boilerplate would be
/// found in whichever text a mis-wired `include_str!` loaded. Each phrase comes from
/// its text's copyright line instead, and each was checked against the pinned tarballs
/// to occur in exactly one of the seven texts and in none of the font texts.
const PHRASES: [(&str, &str); EXPECT_LIBRARY_COUNT] = [
    ("libghostty-vt", "Mitchell Hashimoto, Ghostty contributors"),
    ("Highway", "The Highway Project Authors"),
    ("simdutf", "The simdutf authors"),
    ("translate-c", "Zig contributors"),
    ("uucode", "Jacob Sandlund"),
    ("Unicode Character Database", "Unicode, Inc."),
    ("Wuffs", "The Wuffs Authors"),
];

/// Looks a registered row up by library name.
fn row_for(library: &str) -> &'static LibraryLicense {
    LIBRARY_LICENSES
        .iter()
        .find(|row| row.library == library)
        .unwrap_or_else(|| {
            let registered: Vec<&str> = LIBRARY_LICENSES.iter().map(|row| row.library).collect();
            panic!(
                "no row in LIBRARY_LICENSES names the library {library:?}. Registered libraries \
                 are {registered:?}. Either a linked library lost its licence row, which ships its \
                 code with no attribution, or the row was renamed and this file's phrase table \
                 was not."
            )
        })
}

/// A row's header line in the notices block: the library, an em dash, the licence.
fn header_line(row: &LibraryLicense) -> String {
    format!("{} — {}", row.library, row.license)
}

/// Every linked library has a populated row whose text is a real licence.
#[test]
fn every_linked_library_has_a_non_empty_licence_text() {
    assert_eq!(
        LIBRARY_LICENSES.len(),
        EXPECT_LIBRARY_COUNT,
        "LIBRARY_LICENSES must carry exactly {EXPECT_LIBRARY_COUNT} rows, one per library whose \
         code the static libghostty-vt archive carries. Fewer means a library ships with no \
         licence text beside it; more means a row was added without a phrase in this file."
    );

    for row in LIBRARY_LICENSES {
        let library = row.library;
        assert!(
            !library.is_empty(),
            "a LIBRARY_LICENSES row has an empty `library`, so no reader can tell whose licence \
             it shows."
        );
        assert!(
            !row.license.is_empty(),
            "the {library} row has an empty `license`, so the notices name the library without \
             naming its terms."
        );
        assert!(
            !row.provenance.is_empty(),
            "the {library} row has an empty `provenance`, the record of the tarball, the path in \
             it and the sha256 the text came from; without it a bump cannot be checked."
        );
        assert!(
            row.text.len() > MIN_LICENCE_BYTES,
            "the {library} row's text is {} bytes, not more than the {MIN_LICENCE_BYTES}-byte \
             floor. Every real text here is over 1000 bytes, so its `include_str!` points at an \
             empty, truncated or wrong file.",
            row.text.len()
        );
    }
}

/// Each row's text is the licence of the library it names.
#[test]
fn each_licence_text_carries_its_distinctive_phrase() {
    assert_eq!(
        PHRASES.len(),
        LIBRARY_LICENSES.len(),
        "the phrase table covers {} libraries but LIBRARY_LICENSES carries {}; a row with no \
         phrase is a row nothing here can tell from a mis-wired one.",
        PHRASES.len(),
        LIBRARY_LICENSES.len()
    );

    for (library, phrase) in PHRASES {
        let row = row_for(library);
        assert!(
            row.text.contains(phrase),
            "the {library} row's text does not contain {phrase:?}, from {library}'s own \
             copyright line. The row is populated, so its text is another library's: two rows' \
             `include_str!` paths are swapped, or the file was taken from the wrong place in \
             the tarball."
        );

        let carriers: Vec<&str> = LIBRARY_LICENSES
            .iter()
            .filter(|candidate| candidate.text.contains(phrase))
            .map(|candidate| candidate.library)
            .collect();
        assert_eq!(
            carriers,
            vec![library],
            "{phrase:?} appears in the texts of {carriers:?}, not in {library}'s alone. Either \
             two rows load the same file, or the phrase is boilerplate that cannot detect a \
             swap and must be replaced with one from {library}'s own copyright line."
        );
    }
}

/// The notices block carries every row's header, terms and licence body.
#[test]
fn the_notices_carry_every_linked_library() {
    for row in LIBRARY_LICENSES {
        let header = header_line(row);
        assert!(
            LIBRARY_LICENSE_NOTICES.lines().any(|line| line == header),
            "LIBRARY_LICENSE_NOTICES has no line reading {header:?}. Without it the block \
             carries {}'s licence under no name, or names it with other terms: the registry \
             has the row, so the block was built from a subset of it or its header was not \
             written from the row's `library` and `license`.",
            row.library
        );
    }

    for (library, phrase) in PHRASES {
        assert!(
            LIBRARY_LICENSE_NOTICES.contains(phrase),
            "LIBRARY_LICENSE_NOTICES does not contain {phrase:?}, so {library}'s licence body is \
             absent even if its header is present."
        );
    }
}

/// The rows appear in registry order, and every licence body is followed by an
/// explicit newline.
///
/// `highway-LICENSE-BSD3.txt` and `unicode-LICENSE-V3.txt` end without a newline. A
/// `concat!` that trusts the files would weld `…DAMAGE.` onto the next row's `====` rule,
/// and every other test in this file would still pass. The last row is checked too:
/// the byte after its text must be the explicit `"\n"`, not the end of the block.
#[test]
fn every_licence_body_is_followed_by_a_newline_in_registry_order() {
    let mut cursor = 0usize;
    for (index, row) in LIBRARY_LICENSES.iter().enumerate() {
        let header = format!("\n{}\n", header_line(row));
        let header_at = LIBRARY_LICENSE_NOTICES[cursor..]
            .find(&header)
            .map(|offset| cursor + offset)
            .unwrap_or_else(|| {
                panic!(
                    "LIBRARY_LICENSE_NOTICES has no header line for row {index}, {:?}, after byte \
                     {cursor}. Either the row is missing from the block (see \
                     `the_notices_carry_every_linked_library`) or the block lays the rows out in \
                     an order other than LIBRARY_LICENSES's.",
                    row.library
                )
            });
        let after_header = header_at + header.len();

        let text_at = LIBRARY_LICENSE_NOTICES[after_header..]
            .find(row.text)
            .map(|offset| after_header + offset)
            .unwrap_or_else(|| {
                panic!(
                    "LIBRARY_LICENSE_NOTICES does not contain {}'s licence text verbatim after its \
                     header. The block must embed each text byte for byte; a rewrapped or \
                     re-encoded copy is not the licence the library ships under.",
                    row.library
                )
            });
        let end = text_at + row.text.len();

        let next = LIBRARY_LICENSE_NOTICES[end..].chars().next();
        assert_eq!(
            next,
            Some('\n'),
            "the character after {}'s licence text is {next:?}, not a newline. `None` means the \
             block ends on the text with no explicit \"\\n\"; '=' means the next row's rule is \
             welded onto it, which is what a text without a trailing newline produces when the \
             `concat!` relies on the files. Every row needs an explicit \"\\n\" after its text.",
            row.library
        );
        cursor = end;
    }
}

/// No two rows name the same library.
///
/// A duplicate renders one licence twice and makes `row_for` resolve to the first of
/// the pair, hiding whatever the second contains from the phrase check.
#[test]
fn library_names_are_unique() {
    for (index, row) in LIBRARY_LICENSES.iter().enumerate() {
        let duplicate = LIBRARY_LICENSES[..index]
            .iter()
            .any(|earlier| earlier.library == row.library);
        assert!(
            !duplicate,
            "LIBRARY_LICENSES row {index} names {:?}, which an earlier row already names; the \
             block carries that licence twice and by-name lookups see only the first.",
            row.library
        );
    }
}
