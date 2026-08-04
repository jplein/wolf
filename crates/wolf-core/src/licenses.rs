//! The compiled-in font licence registry: the four bundled fonts' verbatim licence
//! texts, plus the one plain-text block every front-end and every rendered preview
//! document carries.

#[cfg(test)]
mod tests {
    use super::*;

    /// How many fonts this crate bundles, stated as a literal.
    ///
    /// `FONT_LICENSES.len()` compared against itself would accept any number,
    /// including zero — at which point every "for each row" check below passes
    /// vacuously while the shipped binary carries no attribution at all.
    const EXPECT_FONT_COUNT: usize = 4;

    /// The shortest a real licence text can plausibly be, in bytes.
    ///
    /// The smallest of the four is iA Writer Mono S's at 1417 bytes, so this floor
    /// clears it by a wide margin while still failing an `include_str!` pointed at
    /// an empty or truncated file — which otherwise produces a row that satisfies
    /// every other check here.
    const MIN_LICENCE_BYTES: usize = 500;

    /// The distinctive phrase from each font's licence, RE-TYPED here as local
    /// literals.
    ///
    /// Each is taken from its file's copyright line rather than from the shared
    /// OFL boilerplate. Three of the four texts are the same OFL, so a phrase
    /// drawn from the boilerplate would be found in whichever text a mis-wired
    /// `include_str!` happened to load and this table would prove nothing. Each
    /// phrase was verified to occur in exactly one of the four texts and zero times
    /// in the other three, which is what
    /// [`each_licence_text_carries_its_distinctive_phrase`] re-checks from both
    /// sides.
    ///
    /// The iA Writer Mono S phrase deliberately omits the copyright glyph and the
    /// quoted reserved font name: a literal that has to match bytes exactly is
    /// safer with no non-ASCII character and no escaped quote in it. The Latin
    /// Modern Math phrase is shaped unlike the other three because the GUST file
    /// carries no copyright line at all — line 2 is the only line naming the rights
    /// holder.
    const PHRASES: [(&str, &str); EXPECT_FONT_COUNT] = [
        ("Inter", "The Inter Project Authors"),
        ("Latin Modern Math", "GUST is the Polish TeX Users Group"),
        ("JetBrains Mono", "The JetBrains Mono Project Authors"),
        ("iA Writer Mono S", "Information Architects Inc."),
    ];

    /// Looks a registered row up by font name.
    fn row_for(font: &str) -> &'static FontLicense {
        FONT_LICENSES
            .iter()
            .find(|row| row.font == font)
            .unwrap_or_else(|| {
                let registered: Vec<&str> = FONT_LICENSES.iter().map(|row| row.font).collect();
                panic!(
                    "no row in FONT_LICENSES names the font {font:?}. Registered fonts are \
                     {registered:?}. Either a bundled font lost its licence row — which is the \
                     compliance defect this module exists to prevent — or the row was renamed and \
                     the local phrase table above was not."
                )
            })
    }

    /// Every bundled font has a non-empty licence text, and there are four of them.
    ///
    /// The length floor is what catches the silent failure mode: an `include_str!`
    /// pointed at the wrong path, or at a file that was truncated to nothing by a
    /// bad extraction, still compiles and still yields a row whose every field is
    /// present.
    #[test]
    fn every_bundled_font_has_a_non_empty_licence_text() {
        assert_eq!(
            FONT_LICENSES.len(),
            EXPECT_FONT_COUNT,
            "FONT_LICENSES must carry exactly {EXPECT_FONT_COUNT} rows — one per bundled font \
             (Inter and Latin Modern Math ship inside every rendered preview document; JetBrains \
             Mono and iA Writer Mono S ship as editor faces). Fewer means a font is distributed \
             with no licence text compiled in beside it; more means a row was added without a \
             phrase in this file's table."
        );

        for row in FONT_LICENSES {
            let font = row.font;
            assert!(
                !font.is_empty(),
                "a FONT_LICENSES row has an empty `font` name, so nothing downstream can say which \
                 licence it is showing."
            );
            assert!(
                !row.license.is_empty(),
                "the {font} row has an empty `license`, so the notices block would name the font \
                 without naming the terms it is licensed under."
            );
            assert!(
                !row.provenance.is_empty(),
                "the {font} row has an empty `provenance`. That field is where the archive, the \
                 in-archive path and the pinned sha256 the text came from are recorded; without it \
                 a later bump cannot be checked against anything."
            );
            assert!(
                row.text.len() > MIN_LICENCE_BYTES,
                "the {font} row's licence text is {} bytes, which is not more than the \
                 {MIN_LICENCE_BYTES}-byte floor. A real licence is thousands of bytes, so this \
                 means the `include_str!` points at an empty, truncated or wrong file — a row that \
                 looks complete and discharges nothing.",
                row.text.len()
            );
        }
    }

    /// Each row's text really is the licence its `font` names.
    ///
    /// The failure this catches is silent and entirely plausible: two rows'
    /// `include_str!` paths swapped. Every field is still non-empty, the notices
    /// block still contains all four texts, and the document still ships four
    /// licences — attributed to the wrong fonts.
    #[test]
    fn each_licence_text_carries_its_distinctive_phrase() {
        assert_eq!(
            PHRASES.len(),
            FONT_LICENSES.len(),
            "the local phrase table covers {} fonts but FONT_LICENSES carries {}. A registered row \
             with no phrase is a row nothing in this file discriminates, so a new font must be \
             given a phrase here at the same time.",
            PHRASES.len(),
            FONT_LICENSES.len()
        );

        for (font, phrase) in PHRASES {
            let row = row_for(font);
            assert!(
                row.text.contains(phrase),
                "the {font} row's licence text does not contain {phrase:?}, the phrase from that \
                 font's own copyright line. The row is populated (see \
                 `every_bundled_font_has_a_non_empty_licence_text`), so this says the text belongs \
                 to a DIFFERENT font — two rows' `include_str!` paths swapped, which ships four \
                 correct licences attributed to the wrong four fonts."
            );

            // The other half: a phrase drawn from the shared OFL boilerplate would
            // be found in three of the four texts and could not tell a swap apart
            // from a correct wiring at all.
            let carriers: Vec<&str> = FONT_LICENSES
                .iter()
                .filter(|candidate| candidate.text.contains(phrase))
                .map(|candidate| candidate.font)
                .collect();
            assert_eq!(
                carriers,
                vec![font],
                "{phrase:?} appears in the licence texts of {carriers:?} rather than in {font}'s \
                 alone. A phrase shared by more than one text is boilerplate, and a boilerplate \
                 phrase makes the assertion above unable to detect the swap it exists for — \
                 replace it with a line from that font's own copyright notice."
            );
        }
    }

    /// The rendered notices block carries every registered font, its terms, and its
    /// licence body.
    ///
    /// `FONT_LICENSE_NOTICES` is what actually reaches a user — through every
    /// preview document's head and through `wolf-gnome`'s About dialog — so a
    /// registry that is complete while the block built from it is not discharges
    /// nothing.
    #[test]
    fn the_notices_carry_every_registered_font() {
        for row in FONT_LICENSES {
            let font = row.font;
            assert!(
                FONT_LICENSE_NOTICES.contains(font),
                "FONT_LICENSE_NOTICES does not name {font:?}. The registry has the row, so the \
                 block was built from a subset of FONT_LICENSES — the font ships with no visible \
                 attribution anywhere a user can reach."
            );
            assert!(
                FONT_LICENSE_NOTICES.contains(row.license),
                "FONT_LICENSE_NOTICES names {font:?} but not its licence {:?}, so a reader learns \
                 the font is bundled without learning under what terms.",
                row.license
            );
        }

        for (font, phrase) in PHRASES {
            assert!(
                FONT_LICENSE_NOTICES.contains(phrase),
                "FONT_LICENSE_NOTICES does not contain {phrase:?}, so {font}'s licence BODY is \
                 absent even if its name is present. A block that lists the four fonts and \
                 includes three of the four texts is the shape this catches."
            );
        }
    }

    /// Every licence body is separated from the next row by a newline.
    ///
    /// The trap, measured: `ia-writer-mono-LICENSE.txt` has NO trailing newline. A
    /// `concat!` that abuts the texts and relies on the files to end in one
    /// therefore welds `…SOFTWARE.` straight onto the next row's header, producing
    /// a line that reads as neither. Every other test in this file passes through
    /// that defect untouched — the phrases are all still present, the fonts are all
    /// still named, and the block still contains all four texts.
    ///
    /// Both halves are needed. The ordering half proves the rows appear in registry
    /// order (so the assertion below is looking at the boundary it thinks it is),
    /// and the separation half proves the boundary carries an EXPLICIT separator
    /// rather than one borrowed from whichever files happen to end in a newline
    /// today.
    #[test]
    fn every_licence_body_is_separated_from_the_next_row() {
        assert!(
            FONT_LICENSES.len() > 1,
            "FONT_LICENSES carries fewer than two rows, so there is no adjacent pair and this \
             whole test is vacuous. See `every_bundled_font_has_a_non_empty_licence_text`."
        );

        // Ordering: each row's name appears after the previous row's, so the block
        // is the registry laid out in order rather than an arbitrary interleaving.
        let mut cursor = 0usize;
        let mut previous: Option<(&str, usize)> = None;
        for row in FONT_LICENSES {
            let at = FONT_LICENSE_NOTICES[cursor..]
                .find(row.font)
                .map(|offset| cursor + offset)
                .unwrap_or_else(|| {
                    panic!(
                        "FONT_LICENSE_NOTICES has no occurrence of {:?} after byte {cursor}. Either \
                         the row is missing from the block (see \
                         `the_notices_carry_every_registered_font`) or the rows are emitted in a \
                         different order from FONT_LICENSES, which makes the separation check \
                         below inspect the wrong boundary.",
                        row.font
                    )
                });
            if let Some((earlier_font, earlier_at)) = previous {
                assert!(
                    at > earlier_at,
                    "{:?} is rendered at byte {at}, not after {earlier_font:?} at byte \
                     {earlier_at}. The block must lay the rows out in FONT_LICENSES order.",
                    row.font
                );
            }
            previous = Some((row.font, at));
            cursor = at + row.font.len();
        }

        // Separation: whatever follows a row's licence body starts on its own line.
        for pair in FONT_LICENSES.windows(2) {
            let earlier = &pair[0];
            let later = &pair[1];
            let end = FONT_LICENSE_NOTICES
                .find(earlier.text)
                .map(|start| start + earlier.text.len())
                .unwrap_or_else(|| {
                    panic!(
                        "FONT_LICENSE_NOTICES does not contain {}'s licence text verbatim, so the \
                         boundary between it and {} cannot be located. The block must embed each \
                         text byte-for-byte; rewrapping or normalising line endings changes a \
                         third party's licence, which is not this crate's to change.",
                        earlier.font, later.font
                    )
                });
            let next = FONT_LICENSE_NOTICES[end..].chars().next();
            assert_eq!(
                next,
                Some('\n'),
                "the byte after {}'s licence text is {next:?}, not a newline, so {}'s header is \
                 welded onto the end of it. This is the `ia-writer-mono-LICENSE.txt` trap: that \
                 file carries NO trailing newline, so a `concat!` that abuts the rows produces \
                 `…SOFTWARE.<next header>` on one line. Put an EXPLICIT \"\\n\" separator between \
                 every pair of rows rather than relying on the files — three of the four do end in \
                 a newline, which is exactly what makes the fourth's omission invisible.",
                earlier.font,
                later.font
            );
        }
    }

    /// The notices cannot terminate, open, or escape the HTML comment they are
    /// emitted inside.
    ///
    /// `render_html_with_highlights` wraps this block in `<!-- … -->` in every
    /// preview document's head (D7), so a licence text containing a comment
    /// delimiter would close the comment early and dump thousands of lines of
    /// legal text into the rendered page — or, worse, leave the rest of the head
    /// inside a comment that never closes. This is the guard that keeps that
    /// placement safe across a future licence bump, which is when a new text
    /// arrives that nobody read.
    #[test]
    fn the_notices_cannot_terminate_an_html_comment() {
        for needle in ["-->", "--!>", "<!--"] {
            assert!(
                !FONT_LICENSE_NOTICES.contains(needle),
                "FONT_LICENSE_NOTICES contains {needle:?}. Emitted inside the preview document's \
                 `<!-- … -->` head comment, that either closes the comment early — spilling the \
                 licence bodies into the rendered page as visible text — or opens a nested one the \
                 parser never closes, taking the rest of the `<head>` with it. A newly bundled \
                 font's licence is the likely source; wrap or re-encode it rather than moving the \
                 notices into the `<body>`, where `base.css`'s first/last-block margin resets \
                 would break."
            );
        }
        for prefix in [">", "->"] {
            assert!(
                !FONT_LICENSE_NOTICES.starts_with(prefix),
                "FONT_LICENSE_NOTICES starts with {prefix:?}. `<!--` followed by `>` or `->` \
                 terminates the comment immediately in the HTML parsing spec, so the whole block \
                 would render as page text. The leading newline the head interpolation inserts is \
                 what normally prevents this; both sides are asserted so neither can be removed \
                 alone."
            );
        }
        assert!(
            !FONT_LICENSE_NOTICES.ends_with("<!-"),
            "FONT_LICENSE_NOTICES ends with `<!-`, which the closing `-->` completes into a nested \
             comment opener, so the comment does not end where the document says it does."
        );
    }

    /// No two rows name the same font.
    ///
    /// A duplicated row renders the same licence twice and, worse, makes
    /// [`row_for`] resolve every lookup to the first of the pair — so a genuinely
    /// mis-wired second row would be invisible to
    /// [`each_licence_text_carries_its_distinctive_phrase`].
    #[test]
    fn font_names_are_unique() {
        for (index, row) in FONT_LICENSES.iter().enumerate() {
            let duplicate = FONT_LICENSES[..index]
                .iter()
                .any(|earlier| earlier.font == row.font);
            assert!(
                !duplicate,
                "FONT_LICENSES row {index} names {:?}, which an earlier row already names. The \
                 notices block would carry that licence twice, and every by-name lookup — this \
                 file's included — would resolve to the first of the two, hiding whatever the \
                 second actually contains.",
                row.font
            );
        }
    }
}
