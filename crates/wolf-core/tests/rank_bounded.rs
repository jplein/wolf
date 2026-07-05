//! Headless equivalence tests for the BOUNDED top-N file-picker ranking
//! (`wolf_core::rank_matches_top_n`) against the existing unbounded
//! `wolf_core::rank_matches`. Like `rank`'s in-module tests and `palette_rank`,
//! this is IO-free — no filesystem, no Neovim, no GUI — because ranking only
//! reads each `FileEntry`'s `display` + `mru_rank`.
//!
//! ## The one property under test
//!
//! For ALL inputs, the bounded variant must be EXACTLY the unbounded ranking
//! truncated to `n`:
//!
//! ```text
//! rank_matches_top_n(query, &entries, n)
//!     == rank_matches(query, &entries).into_iter().take(n).collect::<Vec<_>>()
//! ```
//!
//! Every test below constructs entry sets + `(query, n)` pairs and asserts that
//! equivalence via `assert_eq!`. Because both paths share the same `score_entry`
//! / tokenization, the property is robust: we never re-derive the expected order
//! by hand — we compare against `rank_matches().take(n)` directly. That is what
//! pins the plan's requirement that the bounded n-boundary tiebreak (the `index`
//! final key) matches the stable-sort `take(n)`.
//!
//! ## Dependency on coder work (plan Fix #2)
//!
//! Neither `rank_matches_top_n` nor the `PartialEq`/`Eq` derive on `Match` exists
//! yet — the coder adds both in `crates/wolf-core/src/rank.rs` (+ re-exports
//! `rank_matches_top_n` from `lib.rs`) and adds the `//crates/wolf-core:rank_bounded`
//! `rust_test` target to `BUILD.bazel`. This file touches NO non-test source. It
//! assumes the plan's exact signature:
//!
//! - `pub fn rank_matches_top_n(query: &str, entries: &[FileEntry], n: usize) -> Vec<Match>`
//! - `Match` additionally derives `PartialEq, Eq` (so `assert_eq!` on `Vec<Match>`
//!   compares structurally: `index` + the full `MatchKey`).

use std::path::PathBuf;

use wolf_core::{rank_matches, rank_matches_top_n, FileEntry, Match};

/// Size of a scoring TIE group (many entries that rank identically), chosen so an
/// `n` can be picked to cut THROUGH the group and exercise the index tiebreak at
/// the bounding boundary.
const TIE_GROUP: usize = 20;

/// Size of the large synthetic in-memory set used to assert exact equivalence at
/// scale (plan 7a: ~5,000–10,000 entries).
const LARGE_SET_SIZE: usize = 8_000;

/// Every Nth entry in the large synthetic set carries an `mru_rank`, so the
/// MRU-first ordering is exercised alongside the textual scoring at scale.
const LARGE_MRU_STRIDE: usize = 500;

/// The picker's real bound (`PICKER_MAX_ROWS`), used as the headline `n` for the
/// large-set equivalence check.
const LARGE_N: usize = 200;

/// Number of distinct pkg directories synthesized in the large set (keeps the
/// `pkgNNN` component small enough to produce many component-start collisions).
const LARGE_PKG_MOD: usize = 64;

/// Builds a [`FileEntry`] from a `display` string (the path is synthesized from
/// it, since ranking only reads `display` + `mru_rank`). Mirrors the `entry`
/// helper in `rank.rs`'s in-module tests.
fn entry(display: &str, mru_rank: Option<u32>) -> FileEntry {
    FileEntry {
        path: PathBuf::from(display),
        display: display.to_string(),
        mru_rank,
    }
}

/// Asserts the central property for one `(query, n)`: the bounded ranking equals
/// the unbounded ranking truncated to `n`.
fn assert_top_n_equivalent(query: &str, entries: &[FileEntry], n: usize) {
    let expected: Vec<Match> = rank_matches(query, entries).into_iter().take(n).collect();
    let bounded = rank_matches_top_n(query, entries, n);
    assert_eq!(
        bounded, expected,
        "rank_matches_top_n({query:?}, .., {n}) must equal rank_matches({query:?}).take({n})"
    );
}

/// Runs [`assert_top_n_equivalent`] for one query across several `n` values.
fn assert_over_ns(query: &str, entries: &[FileEntry], ns: &[usize]) {
    for &n in ns {
        assert_top_n_equivalent(query, entries, n);
    }
}

/// An MRU-MIXED set: some entries carry `mru_rank`, some don't, with a variety of
/// path shapes so both the empty-query (MRU-first) and textual-match paths are
/// meaningfully ordered.
fn mru_mixed_set() -> Vec<FileEntry> {
    vec![
        entry("crates/wolf-core/src/rank.rs", None), // 0
        entry("crates/wolf-core/src/files.rs", Some(2)), // 1: MRU
        entry("crates/wolf-macos/main.swift", None), // 2
        entry("README.md", Some(0)),                 // 3: MRU (most recent)
        entry("docs/manual.md", Some(1)),            // 4: MRU
        entry("src/main.rs", None),                  // 5
        entry("core/wolf-stuff.md", None),           // 6
    ]
}

/// The empty query AND an all-whitespace query both take the MRU-first fast path;
/// bounded must equal unbounded `take(n)` across `n` values (including cutting
/// through the MRU/non-MRU boundary).
#[test]
fn empty_and_whitespace_queries_over_mru_mixed_set() {
    let entries = mru_mixed_set();
    let ns = [0, 1, 2, 3, 4, 7, 100];
    // Empty query and several all-whitespace spellings all tokenize to NO terms.
    for query in ["", "   ", "\t", " \t \n "] {
        assert_over_ns(query, &entries, &ns);
    }
}

/// A single term, a query matching nothing, and edge `n` values over the mixed
/// set.
#[test]
fn single_term_and_no_match_over_mru_mixed_set() {
    let entries = mru_mixed_set();
    let ns = [0, 1, 2, 5, 100];
    // "src" matches several entries as a component/word/contiguous hit.
    assert_over_ns("src", &entries, &ns);
    // "zzz" is a subsequence of no display -> empty result at every n.
    assert_over_ns("zzz", &entries, &ns);
}

/// Multi-term AND matching, and ORDER-INDEPENDENT terms: "wolf core" and
/// "core wolf" both AND-match and must bound-equal unbounded `take(n)`.
#[test]
fn multi_term_and_order_independent_over_mru_mixed_set() {
    let entries = mru_mixed_set();
    let ns = [0, 1, 2, 3, 50];
    assert_over_ns("wolf core", &entries, &ns);
    assert_over_ns("core wolf", &entries, &ns);
}

/// A large TIE group of DISTINCT-but-equally-scoring displays (`aNNN.md`, all the
/// same length so `len` ties and only the lexicographic `display` key separates
/// them). `n` is chosen to cut THROUGH the group, pinning that the bounded
/// n-boundary matches the stable-sort `take(n)`.
#[test]
fn tie_group_straddles_n() {
    // Zero-padded to a fixed width so every display has the same char count: the
    // score, match_start, span, and len keys all tie, leaving `display` (then
    // `index`) as the only separators.
    let entries: Vec<FileEntry> = (0..TIE_GROUP)
        .map(|i| entry(&format!("a{i:03}.md"), None))
        .collect();
    let ns = [0, 1, 5, 7, 13, TIE_GROUP - 1, TIE_GROUP, TIE_GROUP + 5];
    assert_over_ns("a", &entries, &ns);
}

/// The purest test of the `index` final tiebreak: MANY entries with the IDENTICAL
/// display string, so their entire `MatchKey` is equal and ONLY the original
/// `index` orders them. The unbounded stable sort keeps `0..n`; the bounded
/// min-`index` tiebreak must keep exactly the same `0..n`.
#[test]
fn identical_displays_break_ties_by_index() {
    let entries: Vec<FileEntry> = (0..TIE_GROUP).map(|_| entry("a.md", None)).collect();
    let ns = [0, 1, 5, 10, TIE_GROUP - 1, TIE_GROUP, TIE_GROUP + 3];
    assert_over_ns("a", &entries, &ns);
}

/// An MRU-mix where a WEAK-but-MRU entry (scattered match, score 0) leads a
/// STRONG non-MRU entry (component-start, score 124): MRU is the absolute leading
/// key, so the weak MRU entry survives even at `n == 1`. Bounded must reproduce
/// that exactly.
#[test]
fn weak_mru_entry_leads_strong_non_mru() {
    let entries = vec![
        entry("wolf/core.md", None),             // 0: strong non-MRU (124)
        entry("xwxoxlxf.md", Some(0)),           // 1: weak MRU (scattered, 0)
        entry("crates/wolf-core/main.rs", None), // 2: strong non-MRU (124)
    ];
    let ns = [0, 1, 2, 3, 10];
    assert_over_ns("wolf", &entries, &ns);
}

/// Edge `n`: `n == 0` (empty), `n == 1`, and `n` LARGER than the match count all
/// bound-equal unbounded `take(n)`, on both a matching and a non-matching query.
#[test]
fn edge_n_values() {
    let entries = mru_mixed_set();
    // Matching query: fewer than 100 matches, so n=100 exceeds the match count.
    assert_over_ns("main", &entries, &[0, 1, 100]);
    // No-match query: every n (including 0 and beyond) yields an empty vec.
    assert_over_ns("qqqq", &entries, &[0, 1, 100]);
    // Empty query returns ALL entries; n far beyond the set size is a no-op cap.
    assert_over_ns("", &entries, &[0, 1, 1_000]);
}

/// Builds the large synthetic set: `LARGE_SET_SIZE` entries with repeating pkg
/// components (so many component-start collisions), a subset carrying `mru_rank`.
fn large_synthetic_set() -> Vec<FileEntry> {
    (0..LARGE_SET_SIZE)
        .map(|i| {
            let display = format!("pkg{:03}/src/module_{:05}.rs", i % LARGE_PKG_MOD, i);
            let mru = if i % LARGE_MRU_STRIDE == 0 {
                Some((i / LARGE_MRU_STRIDE) as u32)
            } else {
                None
            };
            entry(&display, mru)
        })
        .collect()
}

/// Exact equivalence AT SCALE: a large in-memory set with an MRU subset, checked
/// at the picker's real bound `n = 200` across empty, single-term, multi-term,
/// and no-match queries.
#[test]
fn large_synthetic_set_equivalence_at_scale() {
    let entries = large_synthetic_set();
    for query in ["", "module", "pkg src", "rs", "module_00", "zzzz"] {
        assert_top_n_equivalent(query, &entries, LARGE_N);
    }
}

/// ASCII-only case-fold PARITY: an entry (and a query) carrying a non-ASCII char.
/// Both paths share the same ASCII-only folding in `score_entry`, so whatever
/// they decide, the bounded result must equal the unbounded `take(n)`. We assert
/// only that equivalence — never the exact non-ASCII match outcome.
#[test]
fn non_ascii_char_fold_parity() {
    let entries = vec![
        entry("café/RÉSUMÉ.md", None),  // 0: non-ASCII in display
        entry("cafe/plain.md", None),   // 1: ASCII-only near-neighbor
        entry("notes/CAFÉ.md", Some(0)), // 2: non-ASCII + MRU
    ];
    let ns = [0, 1, 2, 3, 10];
    for query in ["café", "é", "CAFÉ", "cafe"] {
        assert_over_ns(query, &entries, &ns);
    }
}
