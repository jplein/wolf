//! Headless coverage that the GNOME About dialog's text, `wolf_gnome::about::about_text()`,
//! carries the licences of libghostty-vt and the code its static archive carries, beside
//! the font licences its in-source tests cover.
//!
//! `about_text()` takes no arguments and reads the notices itself, so this reads the
//! same bytes a user reads in the dialog. The phrases are re-typed here rather than
//! taken from `wolf_core`: imported, they would keep passing while the dialog carried
//! no library licence at all.
//!
//! Cargo only, and nothing is missing: `crates/wolf-gnome` has no Bazel target by
//! design (`notes/wolf-gnome-decisions.md`). No display, no Neovim and no compositor.

use wolf_gnome::about::about_text;

/// The distinctive phrase from each linked library's licence, taken from its copyright
/// line, and checked against the pinned tarballs to occur in that text alone and in
/// none of the font texts the dialog also carries.
const PHRASES: [(&str, &str); 7] = [
    ("libghostty-vt", "Mitchell Hashimoto, Ghostty contributors"),
    ("Highway", "The Highway Project Authors"),
    ("simdutf", "The simdutf authors"),
    ("translate-c", "Zig contributors"),
    ("uucode", "Jacob Sandlund"),
    ("Unicode Character Database", "Unicode, Inc."),
    ("Wuffs", "The Wuffs Authors"),
];

/// The About text carries every linked library's licence, verbatim.
#[test]
fn the_about_text_carries_every_linked_library_licence() {
    let text = about_text();
    let missing: Vec<&str> = PHRASES
        .iter()
        .filter(|(_, phrase)| !text.contains(phrase))
        .map(|(library, _)| *library)
        .collect();
    assert!(
        missing.is_empty(),
        "the About text lacks the licence phrase of {missing:?}. All seven missing means \
         `about_text` does not include `wolf_core::LIBRARY_LICENSE_NOTICES`; some of them means \
         those rows are missing from the block or load another file. Either way the dialog \
         ships linked code without its attribution. The text is {} bytes.",
        text.len()
    );

    assert!(
        text.contains(wolf_core::LIBRARY_LICENSE_NOTICES),
        "the About text carries every library phrase but not LIBRARY_LICENSE_NOTICES verbatim, \
         so the block was altered, re-wrapped or truncated on its way into the dialog."
    );
}
