//! Pure fuzzy-match + ranking for the file picker.
//!
//! This module is deliberately IO-free: given a query string and the entries a
//! [`crate::files::FileSearch`] has collected so far, it decides which entries
//! match and in what order the picker should show them. That purity is what lets
//! it be unit-tested headlessly — no filesystem, no Neovim, no GUI — exactly
//! mirroring how `geometry.rs` / `cursor.rs` keep the GNOME front-end's logic
//! testable.
//!
//! ## Contract
//!
//! [`rank_matches`] takes `(query, entries)` and returns a `Vec<Match>` where
//! each [`Match`] carries the `index` into the input `entries` slice (callers
//! map results back by index, never by inspecting `MatchKey` internals).
//!
//! [`rank_matches_top_n`] is the BOUNDED companion the picker FFI uses: it
//! returns EXACTLY `rank_matches(query, entries).into_iter().take(n).collect()`
//! for all inputs, but selects the top `n` with an `O(M log n)` bounded max-heap
//! and clones each surviving `display` only `≤ n` times (never once per entry).
//! The empty / all-whitespace query takes a fast path that emits the MRU entries
//! then the first non-MRU entries in original order WITHOUT a global sort, and
//! the per-entry scoring ([`score_entry`]) is ALLOCATION-FREE (no lowercased
//! `String`, no component/word `Vec`s) — it folds ASCII case in place while
//! walking each candidate.
//!
//! ### Tokenization (fzf/Telescope semantics)
//!
//! The query is split on WHITESPACE into TERMS; each term is ASCII-lowercased
//! and empty terms are dropped. So leading/trailing/repeated whitespace is
//! harmless and never empties the result, and an EMPTY or ALL-WHITESPACE query
//! tokenizes to NO terms.
//!
//! - NO terms (empty / all-whitespace query) => return ALL entries, ordered
//!   **MRU-first**: entries whose `mru_rank` is `Some` come first (ascending
//!   `mru_rank`), then the non-MRU entries in stable original order.
//! - SOME terms => an entry MATCHES iff EVERY term is a case-insensitive
//!   SUBSEQUENCE of `display` (logical AND), each term matched INDEPENDENTLY
//!   over the full display (so terms are ORDER-INDEPENDENT). An entry that
//!   fails any term is DROPPED.
//!
//! ### Scoring (higher = better, summed per term)
//!
//! Two ways to slice the display:
//! - **Components**: `display` split on [`COMPONENT_SEPARATORS`] (`/ \`) — the
//!   path segments (e.g. `crates`, `wolf-gnome`, `README.md`).
//! - **Words**: `display` split on [`WORD_SEPARATORS`] (`/ \ _ - . space`) — a
//!   finer split within components (e.g. `wolf`, `gnome`, `README`, `md`).
//!
//! For each matched term, three bonuses are added (and SUMMED across all terms):
//! - `+`[`COMPONENT_START_BONUS`] (100) if some component `starts_with` the term;
//! - `+`[`WORD_START_BONUS`] (20) if some word `starts_with` the term;
//! - `+`[`CONTIGUOUS_BONUS`] (4) if the lowercased display `contains` the term as
//!   a contiguous substring.
//!
//! The ordering `100 > 20 > 4` makes the single-term tiers strict (a
//! component-start always beats a mere word-start, which always beats a mere
//! contiguous substring, which always beats a scattered subsequence at score 0);
//! summing across terms is a soft preference for more/stronger term hits, with
//! determinism guaranteed by the explicit tie-breaks below.
//!
//! ### Sort key, ascending (lower = better), in priority order:
//!
//! 1. `mru_group`: `0` if `mru_rank.is_some()` else `1`  [MRU FIRST — LOCKED]
//! 2. `mru_rank.unwrap_or(MAX)`
//! 3. `score` wrapped in `Reverse` (so a HIGHER score sorts FIRST)
//! 4. `match_start`: smallest first-matched-char index across the terms
//! 5. `span`: sum over terms of `last_matched - first_matched`
//! 6. `len`: `display.chars().count()`
//! 7. `display` lexicographic

use std::collections::BinaryHeap;

use crate::files::FileEntry;

/// Characters that separate "words" within the display: forward/back slash,
/// underscore, hyphen, dot, and space (the finer split inside a component).
const WORD_SEPARATORS: [char; 6] = ['/', '\\', '_', '-', '.', ' '];

/// Characters that separate path COMPONENTS of the display: forward/back slash.
const COMPONENT_SEPARATORS: [char; 2] = ['/', '\\'];

/// Bonus when a term is the start of a path component (the strongest tier).
///
/// `100 > 20 > 4` ([`COMPONENT_START_BONUS`] > [`WORD_START_BONUS`] >
/// [`CONTIGUOUS_BONUS`]) makes the single-term tiers strict, and the bonuses are
/// SUMMED across terms (a soft preference for more/stronger term hits).
const COMPONENT_START_BONUS: u32 = 100;

/// Bonus when a term is the start of a word within a component (the middle tier;
/// see [`COMPONENT_START_BONUS`] for why `100 > 20 > 4`).
const WORD_START_BONUS: u32 = 20;

/// Bonus when a term appears as a contiguous substring of the display (the
/// weakest tier; see [`COMPONENT_START_BONUS`] for why `100 > 20 > 4`).
const CONTIGUOUS_BONUS: u32 = 4;

/// A ranked picker result: the `index` into the input `entries` slice plus the
/// opaque sort [`MatchKey`] that produced its position.
#[derive(Debug, Clone, PartialEq, Eq)]
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
/// the meaning of each level. (Both `PartialOrd` and `Ord` are derived to
/// satisfy `clippy::derive_ord_xor_partial_ord`.)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MatchKey {
    /// `0` if the entry is in the MRU cache, else `1` (MRU first — LOCKED).
    pub mru_group: u8,
    /// The MRU rank (`u32::MAX` when not in the MRU cache).
    pub mru_rank: u32,
    /// The summed match score, wrapped in [`core::cmp::Reverse`] so a HIGHER
    /// score sorts FIRST under the otherwise-ascending derived `Ord`.
    pub score: core::cmp::Reverse<u32>,
    /// Smallest first-matched-char index across the terms (`usize::MAX` if none).
    pub match_start: usize,
    /// Sum over terms of `last_matched_index - first_matched_index`.
    pub span: usize,
    /// `display.chars().count()`.
    pub len: usize,
    /// The `display` string, as the final lexicographic tiebreak.
    pub display: String,
}

/// Ranks `entries` for `query`, returning the matches best-first.
///
/// An EMPTY or ALL-WHITESPACE query returns ALL entries MRU-first (ascending
/// `mru_rank`) then the non-MRU entries in stable original order. Otherwise the
/// query is tokenized into whitespace-delimited TERMS and only entries for which
/// EVERY term is a case-insensitive subsequence of `display` are kept (logical
/// AND, order-independent), ordered by the [`MatchKey`] contract documented on
/// this module.
#[must_use]
pub fn rank_matches(query: &str, entries: &[FileEntry]) -> Vec<Match> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect();

    if terms.is_empty() {
        return rank_empty(entries);
    }

    let mut matches: Vec<Match> = Vec::new();

    for (index, entry) in entries.iter().enumerate() {
        let Some((score, match_start, span)) = score_entry(&terms, &entry.display) else {
            continue;
        };

        let key = MatchKey {
            mru_group: mru_group(entry),
            mru_rank: entry.mru_rank.unwrap_or(u32::MAX),
            score: core::cmp::Reverse(score),
            match_start,
            span,
            len: entry.display.chars().count(),
            display: entry.display.clone(),
        };
        matches.push(Match { index, key });
    }

    matches.sort_by(|a, b| a.key.cmp(&b.key));
    matches
}

/// A borrowed sort key mirroring [`MatchKey`]'s field order, plus the original
/// `index` as a final total-order tiebreak.
///
/// It borrows `display` (no clone) so the bounded selection in
/// [`rank_matches_top_n`] can compare candidates without allocating; `display`
/// is only cloned into an owned [`MatchKey`] for the `≤ n` survivors. Adding
/// `index` makes the order TOTAL, so the final `sort_unstable` is deterministic
/// and matches the STABLE sort in [`rank_matches`] (whose `MatchKey` lacks
/// `index`, making the original insertion order the implicit final tiebreak).
/// (Both `PartialOrd` and `Ord` are derived to satisfy
/// `clippy::derive_ord_xor_partial_ord`.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct KeyRef<'a> {
    mru_group: u8,
    mru_rank: u32,
    score: core::cmp::Reverse<u32>,
    match_start: usize,
    span: usize,
    len: usize,
    display: &'a str,
    index: usize,
}

/// Ranks `entries` for `query`, returning at most the top `n` matches best-first
/// — EXACTLY `rank_matches(query, entries).into_iter().take(n).collect()`.
///
/// This is the bounded companion to [`rank_matches`] used by the picker FFI. It
/// selects the top `n` with an `O(M log n)` bounded max-heap (keyed by the
/// borrowed [`KeyRef`], so no `display` is cloned during selection) and clones a
/// `display` into an owned [`MatchKey`] only for the `≤ n` survivors. An EMPTY /
/// all-whitespace query delegates to the MRU-first fast path (`rank_empty_top_n`)
/// which avoids a global sort; `n == 0` returns an empty `Vec` immediately.
#[must_use]
pub fn rank_matches_top_n(query: &str, entries: &[FileEntry], n: usize) -> Vec<Match> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect();

    if terms.is_empty() {
        return rank_empty_top_n(entries, n);
    }
    if n == 0 {
        return Vec::new();
    }

    // Max-heap capped at `n`: `peek()` is the current WORST kept candidate, so a
    // new candidate need only be compared against it to decide eviction.
    let mut heap: BinaryHeap<KeyRef> = BinaryHeap::with_capacity(n);

    for (index, entry) in entries.iter().enumerate() {
        let Some((score, match_start, span)) = score_entry(&terms, &entry.display) else {
            continue;
        };

        let cand = KeyRef {
            mru_group: mru_group(entry),
            mru_rank: entry.mru_rank.unwrap_or(u32::MAX),
            score: core::cmp::Reverse(score),
            match_start,
            span,
            len: entry.display.chars().count(),
            display: &entry.display,
            index,
        };

        if heap.len() < n {
            heap.push(cand);
        } else if cand < *heap.peek().unwrap() {
            heap.pop();
            heap.push(cand);
        }
    }

    let mut keys = heap.into_vec();
    keys.sort_unstable();

    keys.into_iter()
        .map(|k| Match {
            index: k.index,
            key: MatchKey {
                mru_group: k.mru_group,
                mru_rank: k.mru_rank,
                score: k.score,
                match_start: k.match_start,
                span: k.span,
                len: k.len,
                display: entries[k.index].display.clone(),
            },
        })
        .collect()
}

/// EMPTY / all-whitespace query path: every entry, ordered by
/// `(mru_group, mru_rank, index)` so MRU entries come first (ascending rank) and
/// non-MRU entries keep their stable original order.
fn rank_empty(entries: &[FileEntry]) -> Vec<Match> {
    let mut matches: Vec<Match> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| Match {
            index,
            key: MatchKey {
                mru_group: mru_group(entry),
                mru_rank: entry.mru_rank.unwrap_or(u32::MAX),
                score: core::cmp::Reverse(0),
                match_start: usize::MAX,
                span: 0,
                len: entry.display.chars().count(),
                display: entry.display.clone(),
            },
        })
        .collect();

    matches.sort_by(|a, b| {
        (a.key.mru_group, a.key.mru_rank, a.index).cmp(&(b.key.mru_group, b.key.mru_rank, b.index))
    });
    matches
}

/// Bounded EMPTY / all-whitespace query path: the same MRU-first order as
/// [`rank_empty`] truncated to `n`, but WITHOUT sorting the whole set.
///
/// The order is only `(mru_group, mru_rank, index)`, so we partition in index
/// order into `mru` (has an `mru_rank`) and `nonmru` (already in index order),
/// stable-sort the small `mru` set by `mru_rank`, then take the first `n` of
/// `mru ++ nonmru`. The MRU set is bounded by `MRU_LIMIT`, so this is `O(M)` plus
/// a tiny sort, with at most `n` `display` clones.
fn rank_empty_top_n(entries: &[FileEntry], n: usize) -> Vec<Match> {
    if n == 0 {
        return Vec::new();
    }

    let mut mru: Vec<usize> = Vec::new();
    let mut nonmru: Vec<usize> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if entry.mru_rank.is_some() {
            mru.push(index);
        } else {
            nonmru.push(index);
        }
    }
    // Stable sort keeps the index order among any equal `mru_rank`s, matching
    // `rank_empty`'s `(mru_rank, index)` ordering for the MRU group.
    mru.sort_by_key(|&i| entries[i].mru_rank.unwrap_or(u32::MAX));

    mru.into_iter()
        .chain(nonmru)
        .take(n)
        .map(|index| {
            let entry = &entries[index];
            Match {
                index,
                key: MatchKey {
                    mru_group: mru_group(entry),
                    mru_rank: entry.mru_rank.unwrap_or(u32::MAX),
                    score: core::cmp::Reverse(0),
                    match_start: usize::MAX,
                    span: 0,
                    len: entry.display.chars().count(),
                    display: entry.display.clone(),
                },
            }
        })
        .collect()
}

/// `0` if the entry is in the MRU cache, else `1`.
fn mru_group(entry: &FileEntry) -> u8 {
    u8::from(entry.mru_rank.is_none())
}

/// Walks the already-lowercased `term` through `display` as an in-order
/// case-insensitive subsequence, folding ASCII case of each `display` char in
/// place (never allocating a lowercased copy). Returns the `(first, last)`
/// `display` char indices of the first and last matched chars, or `None` if not
/// all `term` chars are consumed.
fn subsequence_span_ci(term: &str, display: &str) -> Option<(usize, usize)> {
    let mut chars = term.chars();
    let mut needle = chars.next();
    let mut first: Option<usize> = None;
    let mut last = 0usize;

    for (idx, hc) in display.chars().enumerate() {
        match needle {
            Some(qc) if qc == hc.to_ascii_lowercase() => {
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

/// Whether `hay` starts with the already-lowercased `needle`, comparing ASCII
/// case-folded chars with no allocation (parity with `hay_lower.starts_with`).
fn ci_starts_with(hay: &str, needle: &str) -> bool {
    let mut hc = hay.chars();
    for nc in needle.chars() {
        match hc.next() {
            Some(h) if h.to_ascii_lowercase() == nc => {}
            _ => return false,
        }
    }
    true
}

/// Whether `hay` contains the already-lowercased `needle` as a contiguous
/// substring, comparing ASCII case-folded chars with no allocation (parity with
/// `hay_lower.contains`).
fn ci_contains(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    hay.char_indices()
        .any(|(i, _)| ci_starts_with(&hay[i..], needle))
}

/// Scores `display` against the already-lowercased `terms` (logical AND).
///
/// Returns `None` as soon as any term is not a ci subsequence of `display`
/// (the AND drop). Otherwise returns `(score, match_start, span)` where `score`
/// SUMS the per-term component/word/contiguity bonuses, `match_start` is the
/// smallest first-matched index across the terms, and `span` is the sum of each
/// term's matched span.
///
/// This is ALLOCATION-FREE: case is folded ASCII-only in place (never Unicode)
/// and components/words are compared as split iterators, so no lowercased
/// `String` and no component/word `Vec`s are built per entry.
fn score_entry(terms: &[String], display: &str) -> Option<(u32, usize, usize)> {
    let mut score = 0u32;
    let mut match_start = usize::MAX;
    let mut span = 0usize;

    for term in terms {
        let (first, last) = subsequence_span_ci(term, display)?;
        match_start = match_start.min(first);
        span += last - first;

        if display
            .split(COMPONENT_SEPARATORS)
            .filter(|c| !c.is_empty())
            .any(|c| ci_starts_with(c, term))
        {
            score += COMPONENT_START_BONUS;
        }
        if display
            .split(WORD_SEPARATORS)
            .filter(|w| !w.is_empty())
            .any(|w| ci_starts_with(w, term))
        {
            score += WORD_START_BONUS;
        }
        if ci_contains(display, term) {
            score += CONTIGUOUS_BONUS;
        }
    }

    Some((score, match_start, span))
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
            entry("b.md", None),    // 0: non-MRU
            entry("c.md", Some(1)), // 1: MRU rank 1
            entry("a.md", Some(0)), // 2: MRU rank 0 (most recent)
            entry("d.md", None),    // 3: non-MRU
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
        assert!(
            position_of(&got, 0).is_some(),
            "README.md should match 'readme'"
        );
        assert!(
            position_of(&got, 1).is_some(),
            "Readme.md should match 'readme'"
        );

        // An uppercase query matches a lowercase display as a subsequence.
        let entries = [entry("readme.md", None)];
        let got = rank_matches("RDM", &entries);
        assert_eq!(
            indices(&got),
            vec![0],
            "'RDM' should ci-subsequence-match readme.md"
        );
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

    /// MRU is the ABSOLUTE leading key (LOCKED): an MRU entry outranks a non-MRU
    /// entry even at an equal raw score. Query "ma":
    ///   idx0 "main.rs" (non-MRU): component "main.rs" starts with "ma" (+100),
    ///        word "main" starts with "ma" (+20), contiguous (+4) = 124.
    ///   idx1 "docs/manual.md" (MRU rank 0): component "manual.md" starts with
    ///        "ma" (+100), word "manual" starts with "ma" (+20), contiguous (+4)
    ///        = 124.
    /// Scores tie at 124, but `mru_group` (key 1) sorts the MRU entry ahead
    /// regardless of score, so idx1 ranks first.
    #[test]
    fn mru_outranks_better_non_mru_match() {
        let entries = [
            entry("main.rs", None),           // 0: non-MRU, "ma" -> 124
            entry("docs/manual.md", Some(0)), // 1: MRU rank 0, "ma" -> 124
        ];

        let got = rank_matches("ma", &entries);

        // MRU entry first despite "main.rs" being an equally-strong prefix match.
        assert_eq!(indices(&got), vec![1, 0]);
    }

    /// Among NON-MRU entries, a term that starts a path component/word beats a
    /// merely scattered subsequence: a higher `score` (key 3) decides. Query
    /// "doc" (the assertion compares only idx2 vs idx1):
    #[test]
    fn strict_prefix_beats_scattered_subsequence() {
        let entries = [
            // 0: component start +100, word start +20, contiguous +4 = 124 (not asserted).
            entry("docs/intro.md", None),
            // 1: "doc" only appears inside "mydocument", so contiguous +4 only = 4.
            entry("mydocument.md", None),
            // 2: component start +100, word start +20, contiguous +4 = 124.
            entry("doc_notes.md", None),
        ];

        let got = rank_matches("doc", &entries);

        // The component/word-start entry (idx2, 124) ranks ahead of the merely
        // contiguous/scattered one (idx1, 4) on the higher `score`.
        let p_prefix = position_of(&got, 2).expect("doc_notes.md should match 'doc'");
        let p_scatter = position_of(&got, 1).expect("mydocument.md should match 'doc'");
        assert!(
            p_prefix < p_scatter,
            "doc_notes.md (strict prefix) should rank above mydocument.md (scattered): {got:?}",
            got = indices(&got),
        );
    }

    /// The WORD_START_BONUS rewards starting a word (split on the separator set),
    /// which decides via the higher `score` here. Query "in":
    ///   idx0 "main_intro.md": words ["main","intro","md"]; word "intro" starts
    ///        with "in" (+20), contiguous (ma-IN-...) (+4) = 24.
    ///   idx1 "browsing.md": words ["browsing","md"]; no word starts with "in",
    ///        only contiguous (brows-IN-g) (+4) = 4.
    /// 24 > 4, so the word-start match (idx0) ranks first.
    #[test]
    fn word_splitting_on_separators() {
        let entries = [
            // 0: word "intro" starts with "in" (+20) + contiguous (+4) = 24.
            entry("main_intro.md", None),
            // 1: no word starts with "in", only contiguous (+4) = 4.
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

    /// Equal-score matches fall through to the length tie-break. Query "main":
    ///   idx0 "main.rs": component "main.rs" starts with "main" (+100), word
    ///        "main" starts with "main" (+20), contiguous (+4) = 124; len 7.
    ///   idx1 "maintenance.md": component "maintenance.md" starts with "main"
    ///        (+100), word "maintenance" starts with "main" (+20), contiguous
    ///        (+4) = 124; len 15.
    /// Scores tie at 124, match_start (0) and span tie, so `len` (key 6) decides:
    /// 7 < 15, so main.rs ranks first.
    #[test]
    fn full_word_match_ranks_top_of_prefix_group() {
        let entries = [
            entry("main.rs", None),        // 0: 124, len 7
            entry("maintenance.md", None), // 1: 124, len 15
        ];

        let got = rank_matches("main", &entries);

        assert_eq!(
            indices(&got),
            vec![0, 1],
            "main.rs should rank before maintenance.md"
        );
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

        // All four score 124 with match_start 0 and span 0, so only `len` (key 6) then
        // `display` (key 7) decide: "a.md" < "ab.md" < "ax.md" < "ay.md".
        let got = rank_matches("a", &entries);
        assert_eq!(indices(&got), vec![1, 0, 3, 2]);
    }

    /// `match_start` (key 4) is DECISIVE: two NON-MRU entries that tie on score
    /// and on every LATER key (span, len, lexicographic-relevant fields) except
    /// `match_start` must order by the smaller `match_start` (earlier first
    /// matched char) first.
    ///
    /// Query "xz" over the FULL lowercased display, subsequence walk per entry:
    ///   "xazbcd": x@0, (a@1 skip), z@2  -> first=0, last=2, match_start=0, span=2
    ///   "bxazcd": (b@0 skip), x@1, (a@2 skip), z@3 -> first=1, last=3,
    ///             match_start=1, span=2
    /// Neither display has a component or word starting with "xz", and neither
    /// CONTAINS "xz" as a contiguous substring, so BOTH score 0 (key 3 tied);
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

    /// Under the SCORE-based model `score` (not `span`) is DECISIVE here, though
    /// the asserted order `[1, 0]` is unchanged. Query "xz" (one term):
    ///   "bxzcd": "xz" is a CONTIGUOUS substring (b-XZ-cd) -> +CONTIGUOUS_BONUS
    ///            (4); no component/word starts with "xz" -> score = 4.
    ///   "bxaaz": "xz" matches only as a scattered subsequence (x@1 .. z@4), NOT
    ///            contiguous, and no component/word starts with "xz" -> score = 0.
    /// Both are non-MRU (mru_group/mru_rank tied), so the higher score (4 > 0)
    /// puts "bxzcd" first; match_start/span/len never get consulted.
    ///
    /// The worse entry (score 0) is placed at input index 0 so a no-op sort would
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

    /// Space-separated terms are AND-matched: EVERY whitespace-delimited term
    /// must be a ci subsequence of the display, each matched INDEPENDENTLY over
    /// the full display. Query "wolf-gnome README" splits into terms
    /// ["wolf-gnome", "readme"].
    ///   idx0 "crates/wolf-gnome/README.md": contains literal "wolf-gnome" AND
    ///        "readme" -> both terms match -> KEPT.
    ///   idx1 "crates/wolf-macos/main.rs": "wolf-gnome" needs a 'g' after "wolf",
    ///        but "macos/main.rs" has none, so the "-gnome" tail (and "readme")
    ///        is not a subsequence -> term fails the AND -> DROPPED.
    #[test]
    fn space_separated_terms_match_and() {
        let entries = [
            entry("crates/wolf-gnome/README.md", None), // 0: matches both terms
            entry("crates/wolf-macos/main.rs", None),   // 1: lacks "wolf-gnome"/"readme"
        ];

        let got = rank_matches("wolf-gnome README", &entries);

        assert!(
            position_of(&got, 0).is_some(),
            "README.md matches both 'wolf-gnome' and 'README': {got:?}",
            got = indices(&got),
        );
        assert!(
            position_of(&got, 1).is_none(),
            "main.rs lacks a 'readme' subsequence -> dropped by AND: {got:?}",
            got = indices(&got),
        );
    }

    /// Terms are ORDER-INDEPENDENT: each is matched separately over the full
    /// display, so "README wolf-gnome" behaves exactly like "wolf-gnome README"
    /// (idx0 kept, idx1 dropped).
    #[test]
    fn terms_are_order_independent() {
        let entries = [
            entry("crates/wolf-gnome/README.md", None), // 0: matches both terms
            entry("crates/wolf-macos/main.rs", None),   // 1: lacks "wolf-gnome"/"readme"
        ];

        let got = rank_matches("README wolf-gnome", &entries);

        assert!(
            position_of(&got, 0).is_some(),
            "term order must not matter; README.md still matches: {got:?}",
            got = indices(&got),
        );
        assert!(
            position_of(&got, 1).is_none(),
            "main.rs still dropped regardless of term order: {got:?}",
            got = indices(&got),
        );
    }

    /// Whitespace is HARMLESS: leading/trailing/internal-extra spaces are dropped
    /// when tokenizing, and an ALL-whitespace query is equivalent to the empty
    /// query (MRU-first "return all").
    #[test]
    fn whitespace_is_harmless() {
        // (a) Padded/extra-spaced query still tokenizes to ["wolf-gnome","readme"]
        //     and matches the README entry.
        let entries = [
            entry("crates/wolf-gnome/README.md", None), // 0: matches both terms
            entry("crates/wolf-macos/main.rs", None),   // 1: lacks "wolf-gnome"/"readme"
        ];
        let got = rank_matches("  wolf-gnome   readme  ", &entries);
        assert!(
            position_of(&got, 0).is_some(),
            "surrounding/extra whitespace must not break the match: {got:?}",
            got = indices(&got),
        );

        // (b) All-whitespace query == empty query: both return ALL entries
        //     MRU-first. Use an MRU-mixed set so the ordering is meaningful.
        let set = [
            entry("b.md", None),    // 0: non-MRU
            entry("c.md", Some(1)), // 1: MRU rank 1
            entry("a.md", Some(0)), // 2: MRU rank 0 (most recent)
            entry("d.md", None),    // 3: non-MRU
        ];
        assert_eq!(
            indices(&rank_matches("   ", &set)),
            indices(&rank_matches("", &set)),
            "all-whitespace query must behave exactly like the empty query",
        );
    }

    /// MRU is an ABSOLUTE leading group: an MRU entry outranks a non-MRU entry
    /// EVEN WHEN the non-MRU entry has a far stronger raw match score.
    ///   idx0 "wolf/core.md" (non-MRU): component "wolf" starts with "wolf"
    ///        -> 100 (component) + 20 (word "wolf") + 4 (contiguous) = 124.
    ///   idx1 "xwxoxlxf.md" (MRU rank 0): "wolf" matches only scattered
    ///        (w@1,o@3,l@5,f@7); no component/word starts with "wolf" and it is
    ///        not contiguous -> score 0.
    /// Despite 0 << 124, the MRU group (mru_group 0) sorts ahead of the non-MRU
    /// group (mru_group 1), so idx1 ranks first.
    #[test]
    fn mru_dominates_stronger_non_mru() {
        let entries = [
            entry("wolf/core.md", None), // 0: non-MRU, perfect component start (124)
            entry("xwxoxlxf.md", Some(0)), // 1: MRU, weak scattered match (0)
        ];

        let got = rank_matches("wolf", &entries);

        assert_eq!(
            indices(&got),
            vec![1, 0],
            "MRU entry must rank first despite the stronger non-MRU score: {got:?}",
            got = indices(&got),
        );
    }

    /// A subsequence that starts a PATH COMPONENT beats a merely scattered one.
    /// Query "wolf" (both entries non-MRU, so MRU keys tie):
    ///   idx0 "wxoxlxf.md": "wolf" matches scattered (w@0,o@2,l@4,f@6); no
    ///        component/word starts with "wolf", not contiguous -> score 0.
    ///   idx1 "crates/wolf-core/main.rs": component "wolf-core" starts with "wolf"
    ///        -> 100 + 20 (word "wolf") + 4 (contiguous) = 124.
    /// 124 > 0, so idx1 ranks ahead of idx0 (placed at input index 0).
    #[test]
    fn component_start_beats_scattered() {
        let entries = [
            entry("wxoxlxf.md", None),               // 0: scattered (0)
            entry("crates/wolf-core/main.rs", None), // 1: component start (124)
        ];

        let got = rank_matches("wolf", &entries);

        let p_component = position_of(&got, 1).expect("wolf-core matches 'wolf'");
        let p_scatter = position_of(&got, 0).expect("wxoxlxf matches 'wolf'");
        assert!(
            p_component < p_scatter,
            "component-start match must rank above scattered: {got:?}",
            got = indices(&got),
        );
    }

    /// Among matches that start NEITHER a component NOR a word, a CONTIGUOUS run
    /// beats a scattered subsequence. Query "abc" (both non-MRU):
    ///   idx0 "azbzc.md": "abc" matches scattered (a@0,b@2,c@4); "abc" is not a
    ///        component/word prefix and not a contiguous substring -> score 0.
    ///   idx1 "zzabczz.md": "abc" appears as the contiguous substring "abc"
    ///        (zz-ABC-zz); still not a component/word prefix -> only +4
    ///        (contiguous) = 4.
    /// 4 > 0, so contiguity is the sole deciding factor; idx1 ranks ahead of idx0
    /// (placed at input index 0).
    #[test]
    fn contiguity_beats_scattered_among_non_component() {
        let entries = [
            entry("azbzc.md", None),   // 0: scattered, no contiguity (0)
            entry("zzabczz.md", None), // 1: contiguous "abc" (4)
        ];

        let got = rank_matches("abc", &entries);

        let p_contig = position_of(&got, 1).expect("zzabczz matches 'abc'");
        let p_scatter = position_of(&got, 0).expect("azbzc matches 'abc'");
        assert!(
            p_contig < p_scatter,
            "contiguous match must rank above scattered: {got:?}",
            got = indices(&got),
        );
    }

    /// Per-term sub-scores are SUMMED, so more component-start hits score higher.
    /// Query "wolf core" -> terms ["wolf","core"] (both entries non-MRU):
    ///   idx0 "crates/wolf-core/main.rs":
    ///        "wolf" -> component "wolf-core" (100) + word "wolf" (20) + contig (4) = 124
    ///        "core" -> word "core" (20) + contig (4) = 24  (no component starts "core")
    ///        total = 148.
    ///   idx1 "core/wolf-stuff.md":
    ///        "wolf" -> component "wolf-stuff.md" (100) + word "wolf" (20) + contig (4) = 124
    ///        "core" -> component "core" (100) + word "core" (20) + contig (4) = 124
    ///        total = 248.
    /// 248 > 148, so idx1 ranks first. idx0 (lower score) is placed at input
    /// index 0 so a no-op sort would fail.
    #[test]
    fn multi_term_more_component_hits_score_higher() {
        let entries = [
            entry("crates/wolf-core/main.rs", None), // 0: 124 + 24 = 148
            entry("core/wolf-stuff.md", None),       // 1: 124 + 124 = 248
        ];

        let got = rank_matches("wolf core", &entries);

        assert_eq!(
            indices(&got),
            vec![1, 0],
            "more component-start hits (248) must outrank fewer (148): {got:?}",
            got = indices(&got),
        );
    }

    /// AND requirement: if ANY term has no subsequence match the whole entry is
    /// dropped. Query "wolf zzz" -> terms ["wolf","zzz"]:
    ///   idx0 "crates/wolf-gnome/README.md": "wolf" matches but "zzz" has no 'z'
    ///        to match -> dropped.
    ///   idx1 "main.rs": neither "wolf" nor "zzz" is a subsequence -> dropped.
    /// No entry survives -> the result is empty.
    #[test]
    fn all_terms_required_dropped_when_one_missing() {
        let entries = [
            entry("crates/wolf-gnome/README.md", None), // 0: lacks "zzz"
            entry("main.rs", None),                     // 1: lacks both terms
        ];

        let got = rank_matches("wolf zzz", &entries);

        assert!(
            got.is_empty(),
            "no entry has a 'zzz' subsequence, so AND drops everything: {got:?}",
            got = indices(&got),
        );
    }
}
