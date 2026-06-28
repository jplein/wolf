//! Pure fuzzy-match + ranking for the file picker.
//!
//! This module is deliberately IO-free: given a query string and the entries a
//! [`crate::files::FileSearch`] has collected so far, it decides which entries
//! match and in what order the picker should show them. That purity is what lets
//! it be unit-tested headlessly — no filesystem, no Neovim, no GUI — exactly
//! mirroring how `geometry.rs` / `cursor.rs` keep the GNOME front-end's logic
//! testable.
//!
//! ## Contract the tests below assert (from the ARCHITECT PLAN, S3)
//!
//! [`rank_matches`] takes `(query, entries)` and returns a `Vec<Match>` where
//! each [`Match`] carries the `index` into the input `entries` slice (tests
//! assert on the ORDER of those indices, never on `MatchKey` internals).
//!
//! - EMPTY query => return ALL entries, ordered **MRU-first**: entries whose
//!   `mru_rank` is `Some` come first (ascending `mru_rank`), then the non-MRU
//!   entries in stable original order.
//! - NON-EMPTY query => case-INSENSITIVE SUBSEQUENCE match on `display` (both
//!   sides ASCII-lowercased); non-matching entries are DROPPED from the result.
//! - A "word" is a maximal run in the FILENAME (the last path component of
//!   `display`) split on any of `/ \ _ - . space`. A query is a "strict prefix
//!   of a word" when some word `starts_with(query)` case-insensitively.
//! - Sort key, ascending (lower = better), in priority order:
//!     1. `mru_group`: `0` if `mru_rank.is_some()` else `1`  [MRU FIRST — LOCKED]
//!     2. `mru_rank.unwrap_or(MAX)`
//!     3. `prefix_group`: `0` if the query is a ci-prefix of some filename word
//!        else `1` (a full-word equality still lands in group `0`, and sorts
//!        ahead of longer prefix matches via the later length tie-break)
//!     4. `word_index` of the earliest anchored word
//!     5. `match_start`: index of the first matched char in `display`
//!     6. `span`: `last_matched - first_matched`
//!     7. `display.len()`
//!     8. `display` lexicographic

// =============================================================================
// NOTE to coder: this file was bootstrapped by the test-writer with ONLY the
// module doc above and the `#[cfg(test)] mod tests` block below. It WILL NOT
// COMPILE until you add the items the tests reference, exactly as `geometry.rs`
// and `cursor.rs` were bootstrapped. Implement the following ABOVE the test
// module (do not change the test module):
//
//   pub struct Match {
//       pub index: usize,   // index into the input `entries` slice (LOAD-BEARING:
//                           //   the tests map results to `.index` and assert order)
//       pub key: MatchKey,  // the opaque sort key (tests never inspect it)
//   }
//
//   pub struct MatchKey { ... }  // whatever fields encode the 8-level sort key.
//       // It must order entries by the contract in the module doc. Deriving
//       // PartialOrd/Ord over fields laid out in priority order (mru_group,
//       // mru_rank, prefix_group, word_index, match_start, span, display.len(),
//       // display) is the natural shape. `MatchKey` may be fully opaque to tests.
//
//   #[must_use]
//   pub fn rank_matches(query: &str, entries: &[FileEntry]) -> Vec<Match>
//       // EMPTY query  => ALL entries, MRU-first then stable original order.
//       // NON-EMPTY    => ci subsequence match on `display`; drop non-matches;
//       //                 sort by the 8-level key above.
//
//   Named consts (lint: mnd) for any magic numbers >= 5 you introduce, and the
//   set of word separators `/ \ _ - . space`. Use `u32::MAX` for the
//   `mru_rank.unwrap_or(MAX)` sentinel. Mark `rank_matches` `#[must_use]`.
//
// IMPORTANT — displays chosen to make a specific key decisive (see each test's
// inline comment). Implement the EXACT word-splitting/prefix semantics the tests
// assume:
//   * "word" comes from the FILENAME (last path component) ONLY — NOT the whole
//     `display`. So in "docs/intro.md" the words are ["intro","md"]; "docs" is a
//     PATH component and contributes NO word. Several tests hinge on this.
//   * "strict prefix of a word" is `word.starts_with(query)` case-insensitively;
//     full-word equality (word == query) is still a prefix (group 0) and wins the
//     later length tie-break against a longer word with the same prefix.
//   * matching is a ci SUBSEQUENCE over the FULL `display` (path included), while
//     prefix_group/word_index are computed over the FILENAME's words.
// =============================================================================

use crate::files::FileEntry;

/// Characters that separate "words" inside a filename: forward/back slash,
/// underscore, hyphen, dot, and space.
const WORD_SEPARATORS: [char; 6] = ['/', '\\', '_', '-', '.', ' '];

/// A ranked picker result: the `index` into the input `entries` slice plus the
/// opaque sort [`MatchKey`] that produced its position.
#[derive(Debug, Clone)]
pub struct Match {
    /// Index into the `entries` slice passed to [`rank_matches`].
    pub index: usize,
    /// The sort key (lower = better) this entry ranked by.
    pub key: MatchKey,
}

/// The lexicographic sort key for a matched entry.
///
/// Fields are declared in priority order so the derived `Ord` compares them
/// lexicographically — every field is "lower = better". See the module doc for
/// the meaning of each level.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MatchKey {
    /// `0` if the entry is in the MRU cache, else `1` (MRU first — LOCKED).
    pub mru_group: u8,
    /// The MRU rank (`u32::MAX` when not in the MRU cache).
    pub mru_rank: u32,
    /// `0` if the query is a ci-prefix of some filename word, else `1`.
    pub prefix_group: u8,
    /// Index of the earliest filename word the query prefixes (`usize::MAX` if
    /// none).
    pub word_index: usize,
    /// Index of the first matched char in the lowercased `display`.
    pub match_start: usize,
    /// `last_matched_index - first_matched_index`.
    pub span: usize,
    /// `display.chars().count()`.
    pub len: usize,
    /// The `display` string, as the final lexicographic tiebreak.
    pub display: String,
}

/// Ranks `entries` for `query`, returning the matches best-first.
///
/// An EMPTY query returns ALL entries MRU-first (ascending `mru_rank`) then the
/// non-MRU entries in stable original order. A NON-EMPTY query keeps only the
/// entries whose `display` contains `query` as a case-insensitive subsequence,
/// ordered by the [`MatchKey`] contract documented on this module.
#[must_use]
pub fn rank_matches(query: &str, entries: &[FileEntry]) -> Vec<Match> {
    if query.is_empty() {
        return rank_empty(entries);
    }

    let query_lower = query.to_ascii_lowercase();
    let mut matches: Vec<Match> = Vec::new();

    for (index, entry) in entries.iter().enumerate() {
        let display_lower = entry.display.to_ascii_lowercase();
        let Some((first, last)) = subsequence_span(&query_lower, &display_lower) else {
            continue;
        };

        let (prefix_group, word_index) = filename_prefix(&entry.display, &query_lower);

        let key = MatchKey {
            mru_group: mru_group(entry),
            mru_rank: entry.mru_rank.unwrap_or(u32::MAX),
            prefix_group,
            word_index,
            match_start: first,
            span: last - first,
            len: entry.display.chars().count(),
            display: entry.display.clone(),
        };
        matches.push(Match { index, key });
    }

    matches.sort_by(|a, b| a.key.cmp(&b.key));
    matches
}

/// EMPTY-query path: every entry, ordered by `(mru_group, mru_rank, index)` so
/// MRU entries come first (ascending rank) and non-MRU entries keep their stable
/// original order.
fn rank_empty(entries: &[FileEntry]) -> Vec<Match> {
    let mut matches: Vec<Match> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| Match {
            index,
            key: MatchKey {
                mru_group: mru_group(entry),
                mru_rank: entry.mru_rank.unwrap_or(u32::MAX),
                prefix_group: 1,
                word_index: usize::MAX,
                match_start: usize::MAX,
                span: 0,
                len: entry.display.chars().count(),
                display: entry.display.clone(),
            },
        })
        .collect();

    matches.sort_by(|a, b| {
        (a.key.mru_group, a.key.mru_rank, a.index).cmp(&(
            b.key.mru_group,
            b.key.mru_rank,
            b.index,
        ))
    });
    matches
}

/// `0` if the entry is in the MRU cache, else `1`.
fn mru_group(entry: &FileEntry) -> u8 {
    u8::from(entry.mru_rank.is_none())
}

/// Walks `query` through `haystack` as an in-order subsequence (both already
/// lowercased). Returns the `(first, last)` haystack char indices of the first
/// and last matched chars, or `None` if not all query chars are consumed.
fn subsequence_span(query: &str, haystack: &str) -> Option<(usize, usize)> {
    let mut chars = query.chars();
    let mut needle = chars.next();
    let mut first: Option<usize> = None;
    let mut last = 0usize;

    for (idx, hc) in haystack.chars().enumerate() {
        match needle {
            Some(qc) if qc == hc => {
                if first.is_none() {
                    first = Some(idx);
                }
                last = idx;
                needle = chars.next();
            }
            _ => {}
        }
    }

    if needle.is_some() {
        return None;
    }
    first.map(|f| (f, last))
}

/// Computes `(prefix_group, word_index)` over the FILENAME words of `display`
/// (the last path component, split on [`WORD_SEPARATORS`]). `query_lower` is the
/// already-lowercased query. `prefix_group` is `0` when some word ci-prefixes
/// the query, and `word_index` is the earliest such word's index.
fn filename_prefix(display: &str, query_lower: &str) -> (u8, usize) {
    let filename = display
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(display);

    for (i, word) in filename
        .split(WORD_SEPARATORS)
        .filter(|w| !w.is_empty())
        .enumerate()
    {
        if word.to_ascii_lowercase().starts_with(query_lower) {
            return (0, i);
        }
    }
    (1, usize::MAX)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::FileEntry;

    /// Builds a [`FileEntry`] from a `display` string (the path is synthesized
    /// from it, since ranking only reads `display` + `mru_rank`).
    fn entry(display: &str, mru_rank: Option<u32>) -> FileEntry {
        FileEntry {
            path: PathBuf::from(display),
            display: display.to_string(),
            mru_rank,
        }
    }

    /// Maps the ranked results to their original-input indices (the only part of
    /// a [`Match`] the tests assert on).
    fn indices(matches: &[Match]) -> Vec<usize> {
        matches.iter().map(|m| m.index).collect()
    }

    /// Position of original index `idx` within the ranked result, or `None` if it
    /// was dropped (non-match).
    fn position_of(matches: &[Match], idx: usize) -> Option<usize> {
        matches.iter().position(|m| m.index == idx)
    }

    /// EMPTY query returns ALL entries, MRU-first: `mru_rank` `Some` first
    /// (ascending rank), then non-MRU entries in stable original order.
    #[test]
    fn empty_query_returns_all_mru_first() {
        let entries = [
            entry("b.md", None),     // 0: non-MRU
            entry("c.md", Some(1)),  // 1: MRU rank 1
            entry("a.md", Some(0)),  // 2: MRU rank 0 (most recent)
            entry("d.md", None),     // 3: non-MRU
        ];

        let got = rank_matches("", &entries);

        // All four returned; MRU rank0 (idx 2), MRU rank1 (idx 1), then the
        // non-MRU entries in their original order (idx 0, idx 3).
        assert_eq!(indices(&got), vec![2, 1, 0, 3]);
    }

    /// NON-EMPTY query matching is case-INSENSITIVE on both sides.
    #[test]
    fn match_is_case_insensitive() {
        // "readme" matches both differently-cased displays.
        let entries = [entry("README.md", None), entry("Readme.md", None)];
        let got = rank_matches("readme", &entries);
        assert!(position_of(&got, 0).is_some(), "README.md should match 'readme'");
        assert!(position_of(&got, 1).is_some(), "Readme.md should match 'readme'");

        // An uppercase query matches a lowercase display as a subsequence.
        let entries = [entry("readme.md", None)];
        let got = rank_matches("RDM", &entries);
        assert_eq!(indices(&got), vec![0], "'RDM' should ci-subsequence-match readme.md");
    }

    /// A subsequence (non-contiguous) query matches; an entry with no subsequence
    /// match is DROPPED from the result.
    #[test]
    fn subsequence_match_and_nonmatch_excluded() {
        // "rdm" is a subsequence of "readme.md" (r..d..m).
        let entries = [entry("readme.md", None)];
        let got = rank_matches("rdm", &entries);
        assert_eq!(indices(&got), vec![0]);

        // "xyz" is a subsequence of neither display -> empty result.
        let entries = [entry("readme.md", None), entry("main.rs", None)];
        let got = rank_matches("xyz", &entries);
        assert!(got.is_empty(), "no entry contains 'xyz' as a subsequence");
    }

    /// MRU is the PRIMARY key (LOCKED): an MRU entry outranks a stronger non-MRU
    /// prefix match. Both displays match "ma" as a subsequence AND have a filename
    /// word that is a strict prefix of "ma" ("main", "manual"), so prefix_group
    /// ties at 0 and MRU breaks the tie in MRU's favor.
    #[test]
    fn mru_outranks_better_non_mru_match() {
        let entries = [
            entry("main.rs", None),          // 0: non-MRU, word "main" prefixes "ma"
            entry("docs/manual.md", Some(0)),// 1: MRU rank 0, word "manual" prefixes "ma"
        ];

        let got = rank_matches("ma", &entries);

        // MRU entry first despite "main.rs" being an equally-strong prefix match.
        assert_eq!(indices(&got), vec![1, 0]);
    }

    /// Among NON-MRU entries, a strict prefix of a FILENAME word beats a merely
    /// scattered subsequence match (prefix_group is the deciding key here).
    #[test]
    fn strict_prefix_beats_scattered_subsequence() {
        let entries = [
            // 0: filename "intro.md" -> words ["intro","md"]; "doc" prefixes NO
            //    word ("docs" is a PATH component, not a word). "doc" still matches
            //    as a subsequence over the full display -> prefix_group 1.
            entry("docs/intro.md", None),
            // 1: filename "mydocument.md" -> word "mydocument" starts with "my",
            //    NOT "doc"; "doc" appears scattered -> prefix_group 1.
            entry("mydocument.md", None),
            // 2: filename "doc_notes.md" -> words ["doc","notes","md"]; word "doc"
            //    IS a strict prefix of "doc" -> prefix_group 0.
            entry("doc_notes.md", None),
        ];

        let got = rank_matches("doc", &entries);

        // The strict-prefix entry ranks ahead of the scattered subsequence ones.
        let p_prefix = position_of(&got, 2).expect("doc_notes.md should match 'doc'");
        let p_scatter = position_of(&got, 1).expect("mydocument.md should match 'doc'");
        assert!(
            p_prefix < p_scatter,
            "doc_notes.md (strict prefix) should rank above mydocument.md (scattered): {got:?}",
            got = indices(&got),
        );
    }

    /// Words are split on the separator set, so "in" is a strict prefix of the
    /// word "intro" inside "main_intro.md" (prefix_group 0), but only a scattered
    /// subsequence inside "browsing.md" (no word starts with "in"; prefix_group 1).
    #[test]
    fn word_splitting_on_separators() {
        let entries = [
            // 0: filename "main_intro.md" -> words ["main","intro","md"]; "in"
            //    prefixes "intro" -> prefix_group 0.
            entry("main_intro.md", None),
            // 1: filename "browsing.md" -> words ["browsing","md"]; "in" prefixes
            //    no word, only scattered (brows-IN-g) -> prefix_group 1.
            entry("browsing.md", None),
        ];

        let got = rank_matches("in", &entries);

        let p_word = position_of(&got, 0).expect("main_intro.md should match 'in'");
        let p_scatter = position_of(&got, 1).expect("browsing.md should match 'in'");
        assert!(
            p_word < p_scatter,
            "word-prefix match should rank above scattered match: {got:?}",
            got = indices(&got),
        );
    }

    /// A full-word match ranks at/near the top of the prefix group: "main" equals
    /// the word "main" in "main.rs" and merely prefixes "maintenance" in
    /// "maintenance.md". Both are prefix_group 0; the shorter display wins the
    /// length tie-break, so main.rs ranks first.
    #[test]
    fn full_word_match_ranks_top_of_prefix_group() {
        let entries = [
            entry("main.rs", None),         // 0: word "main" == query
            entry("maintenance.md", None),  // 1: word "maintenance" starts with "main"
        ];

        let got = rank_matches("main", &entries);

        assert_eq!(indices(&got), vec![0, 1], "main.rs should rank before maintenance.md");
    }

    /// Deterministic tie-breaks: entries otherwise equal under keys 1-6 are
    /// ordered by display length, then lexicographically.
    #[test]
    fn deterministic_tie_breaks() {
        let entries = [
            entry("ab.md", None), // 0: len 5
            entry("a.md", None),  // 1: len 4  (shortest -> first)
            entry("ay.md", None), // 2: len 5, lexicographically after "ax.md"
            entry("ax.md", None), // 3: len 5, lexicographically before "ay.md"
        ];

        // All four match "a" (subsequence) and each filename word starts with "a"
        // (prefix_group 0, match_start 0, span 0), so only length then lexicographic
        // order decide: "a.md" < "ab.md" < "ax.md" < "ay.md".
        let got = rank_matches("a", &entries);
        assert_eq!(indices(&got), vec![1, 0, 3, 2]);
    }

    /// `match_start` (key 5) is DECISIVE: two NON-MRU entries tied on keys 1-4
    /// and on every LATER key (span, len, lexicographic-relevant fields) except
    /// `match_start` must order by the smaller `match_start` (earlier first
    /// matched char) first.
    ///
    /// Query "xz" over the FULL lowercased display, subsequence walk per entry:
    ///   "xazbcd": x@0, (a@1 skip), z@2  -> first=0, last=2, match_start=0, span=2
    ///   "bxazcd": (b@0 skip), x@1, (a@2 skip), z@3 -> first=1, last=3,
    ///             match_start=1, span=2
    /// Both: len=6; single filename word ("xazbcd"/"bxazcd") that does NOT
    /// start_with "xz", so prefix_group=1, word_index=usize::MAX (keys 3,4 tied);
    /// both non-MRU (keys 1,2 tied); span=2 and len=6 are EQUAL, so `match_start`
    /// (0 vs 1) is the ONLY differing key and must decide.
    ///
    /// The worse entry (match_start=1) is placed at input index 0 so a no-op
    /// sort would produce [0, 1] and fail this assertion.
    #[test]
    fn match_start_is_decisive() {
        let entries = [
            entry("bxazcd", None), // 0: match_start=1, span=2, len=6
            entry("xazbcd", None), // 1: match_start=0, span=2, len=6
        ];

        let got = rank_matches("xz", &entries);

        assert_eq!(
            indices(&got),
            vec![1, 0],
            "smaller match_start (xazbcd) must rank first: {got:?}",
            got = indices(&got),
        );
    }

    /// `span` (key 6) is DECISIVE: two NON-MRU entries tied on keys 1-5 (same
    /// prefix_group, word_index, AND match_start) and on len must order by the
    /// smaller `span` (more contiguous match) first.
    ///
    /// Query "xz" over the FULL lowercased display, subsequence walk per entry:
    ///   "bxzcd": (b@0 skip), x@1, z@2 -> first=1, last=2, match_start=1, span=1
    ///   "bxaaz": (b@0 skip), x@1, (a@2,a@3 skip), z@4 -> first=1, last=4,
    ///            match_start=1, span=3
    /// Both: match_start=1 (key 5 tied), len=5; single filename word
    /// ("bxzcd"/"bxaaz") that does NOT start_with "xz", so prefix_group=1,
    /// word_index=usize::MAX (keys 3,4 tied); both non-MRU (keys 1,2 tied). With
    /// match_start and len equal, `span` (1 vs 3) is the ONLY differing key and
    /// must decide.
    ///
    /// The worse entry (span=3) is placed at input index 0 so a no-op sort would
    /// produce [0, 1] and fail this assertion.
    #[test]
    fn span_is_decisive() {
        let entries = [
            entry("bxaaz", None), // 0: match_start=1, span=3, len=5
            entry("bxzcd", None), // 1: match_start=1, span=1, len=5
        ];

        let got = rank_matches("xz", &entries);

        assert_eq!(
            indices(&got),
            vec![1, 0],
            "smaller span (bxzcd, more contiguous) must rank first: {got:?}",
            got = indices(&got),
        );
    }
}
