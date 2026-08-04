//! The About dialog: the pure text seam that carries the bundled fonts' licence
//! notices, plus the transient `gtk::Window` that presents it.

#[cfg(test)]
mod tests {
    use super::*;

    /// The distinctive phrase from Inter's licence, RE-TYPED here as a local
    /// literal.
    ///
    /// None of the four phrases below may be imported from
    /// `wolf_core::licenses` — importing them would make this file assert that a
    /// constant equals itself, and it would keep passing while the dialog carried
    /// no licence text at all. Each is taken from its file's copyright line rather
    /// than from the shared OFL boilerplate, which three of the four texts have in
    /// common and which therefore could not tell a mis-wired `include_str!` apart
    /// from a correct one.
    const INTER_PHRASE: &str = "The Inter Project Authors";
    /// The distinctive phrase from Latin Modern Math's licence. Shaped unlike the
    /// other three because the GUST file carries no copyright line at all — line 2
    /// is the only line naming the rights holder.
    const LATIN_MODERN_PHRASE: &str = "GUST is the Polish TeX Users Group";
    /// The distinctive phrase from JetBrains Mono's licence.
    const JETBRAINS_PHRASE: &str = "The JetBrains Mono Project Authors";
    /// The distinctive phrase from iA Writer Mono S's licence. It deliberately
    /// omits the `(c)` glyph and the quoted reserved font name: a literal that has
    /// to match bytes exactly is safer with no non-ASCII character and no escaped
    /// quote in it.
    const IA_WRITER_PHRASE: &str = "Information Architects Inc.";

    /// Every phrase, with the font it belongs to, for a failure message that names
    /// which licence went missing.
    const PHRASES: [(&str, &str); 4] = [
        ("Inter", INTER_PHRASE),
        ("Latin Modern Math", LATIN_MODERN_PHRASE),
        ("JetBrains Mono", JETBRAINS_PHRASE),
        ("iA Writer Mono S", IA_WRITER_PHRASE),
    ];

    /// The application name the heading must carry.
    const APP_NAME: &str = "Wolf";

    /// The About text carries all four bundled fonts' licences.
    ///
    /// [`about_text`] takes NO arguments and reads
    /// `wolf_core::licenses::FONT_LICENSE_NOTICES` itself, so this asserts
    /// reachability FROM THE COMPILED ARTIFACT — the same bytes a user reads in the
    /// dialog — rather than from a file on disk. A version taking the notices as a
    /// parameter would let this test pass them in, which would prove nothing about
    /// what ships.
    ///
    /// This is the one surface on this front-end that discharges the bundled
    /// fonts' attribution obligation, so a missing phrase is a licence-compliance
    /// defect and not a cosmetic one.
    #[test]
    fn the_about_text_carries_every_bundled_font_licence() {
        let text = about_text();
        for (font, phrase) in PHRASES {
            assert!(
                text.contains(phrase),
                "the About text does not contain {phrase:?}, the distinctive phrase from {font}'s \
                 licence. Two things produce this and they need different fixes: \
                 `FONT_LICENSE_NOTICES` was dropped from `about_text`'s composition (then all four \
                 phrases are missing), or {font}'s row was dropped from `FONT_LICENSES` or its \
                 `include_str!` re-pointed (then only this one is). Either way the shipped dialog \
                 fails to carry the attribution the bundled font requires. Text was:\n{text}"
            );
        }
    }

    /// The About text names the application and its version.
    ///
    /// Two premises, both stated because the assertions rest on them: the heading
    /// is the first line of [`about_text`], and the version comes from
    /// `env!("CARGO_PKG_VERSION")` — this crate's own version, which is why the
    /// test can state the expected string at all.
    ///
    /// The version is additionally required to appear BEFORE the first licence
    /// phrase. Without that, a bare `contains` would be satisfied by a version-like
    /// string occurring inside one of the ~14 KB of licence bodies, and deleting
    /// the heading outright would not redden this test.
    #[test]
    fn the_about_text_names_the_application_and_its_version() {
        let text = about_text();
        let version = env!("CARGO_PKG_VERSION");
        assert!(
            !version.is_empty(),
            "CARGO_PKG_VERSION is empty, so the version half of this test would hold vacuously — \
             every string contains the empty string. Check `crates/wolf-gnome/Cargo.toml`."
        );

        let first_line = text.lines().find(|line| !line.trim().is_empty());
        let heading = first_line.unwrap_or_else(|| {
            panic!("`about_text()` produced no non-blank line at all, so the dialog opens empty.")
        });
        assert!(
            heading.contains(APP_NAME),
            "the first non-blank line of the About text is {heading:?}, which does not name \
             {APP_NAME:?}. The dialog opens on this line, so a user meeting it learns which \
             application they are looking at from it or from nothing."
        );

        let version_at = text.find(version).unwrap_or_else(|| {
            panic!(
                "the About text does not contain the version {version:?} anywhere, so the dialog \
                 cannot say which build a bug report is against. Text was:\n{text}"
            )
        });
        let first_phrase_at = PHRASES
            .iter()
            .filter_map(|(_, phrase)| text.find(phrase))
            .min()
            .unwrap_or_else(|| {
                panic!(
                    "no licence phrase appears in the About text, so the ordering half of this \
                     test has nothing to measure against. Read \
                     `the_about_text_carries_every_bundled_font_licence` first — it fails for the \
                     same reason and says more about it."
                )
            });
        assert!(
            version_at < first_phrase_at,
            "the version {version:?} first appears at byte {version_at}, AFTER the first licence \
             phrase at byte {first_phrase_at}. That means it was found inside a licence body \
             rather than in the heading, so the heading has been deleted and this assertion was \
             about to pass on a coincidence."
        );
    }
}
