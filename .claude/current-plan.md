# Plan: Score-based fuzzy ranking + space-separated terms (rank.rs)

## STATUS: COMPLETE ✅

Score-based ranking + whitespace-tokenized AND matching implemented in rank.rs; 28/28
wolf-core lib tests green (incl. 8 new rank tests + all existing), integration targets +
wolf-ffi build unaffected; reviewer APPROVED; stale `prefix_group`/`word_index` comments
cleaned in rank.rs + smoke_headless.rs; prose docs verified accurate + prettier-clean;
re-confirmed green after the comment cleanup. Nothing committed (standing rule). Net: a
space in the picker query now separates AND-ed search terms (so "wolf-gnome README"
reaches crates/wolf-gnome/README.md and a trailing space no longer empties the list), and
results are scored — MRU first (absolute), then component-start (+100), word-start (+20),
contiguous (+4), summed across terms, with deterministic tie-breaks. Outstanding (host
limitation): the Linux `cargo clippy` gate (no cargo on macOS; Bazel rustc compile clean).


## Problem statement

Two things, combined into one redesign of `crates/wolf-core/src/rank.rs` (the pure,
headlessly-tested fuzzy match + ranking shared by BOTH the macOS and GTK pickers):

### Part 1 — BUG: a space empties the list
`rank_matches()` matches a non-empty query via `subsequence_span(query_lower,
display_lower)`, which walks the ENTIRE query — including any space — as ONE in-order
subsequence over the display path. File paths have no spaces, so any query with a space
(e.g. `"wolf-gnome README"`) matches nothing and the whole list empties.

**Desired (fzf/Telescope semantics):** tokenize the query on whitespace into terms; drop
empty terms (so leading/trailing/multiple/all-whitespace is harmless and never empties
the list); an entry MATCHES iff EVERY term is a case-insensitive subsequence of the
display (logical AND), each term matched INDEPENDENTLY over the full display
(order-independent). So `"wolf-gnome README"` AND `"README wolf-gnome"` both match
`crates/wolf-gnome/README.md`. Empty / all-whitespace query → existing MRU-first
"return all" behavior.

### Part 2 — REDESIGN: go score-based (user directive)
Replace the lexicographic 8-level `MatchKey` with a SCORE-based ranking. User's explicit
priority order (highest-impact first):
1. **MRU presence — weighted VERY highly.** (Was an absolute LOCKED primary group;
   user says "weight very highly" → a dominant weight. ORCHESTRATOR DECISION: make MRU
   dominate decisively — in practice any MRU entry outranks any non-MRU entry — but
   architect chooses whether that's an absolute group or a weight large enough to be
   effectively absolute. Within MRU, more-recent (lower mru_rank) still ranks higher.)
2. **A subsequence that matches the BEGINNING of a path component.** Path components are
   the `/`-delimited segments of `display` (e.g. `crates`, `wolf-gnome`, `README.md`). A
   term matching the start of a component (e.g. `wolf-gnome`, `README`) scores higher.
   (Architect: decide whether to ALSO give a lesser bonus for matching the start of a
   "word" within a component — the existing finer split on `_ - . space` — vs. only the
   component start. The user said "path component"; keep the headline on component-start,
   a word-start sub-bonus is optional/architect's call.)
3. **A subsequence matching CONSECUTIVE characters** (contiguity) scores higher than a
   scattered subsequence.
4. Deterministic tie-breaks remain at the bottom (shorter `display`, then lexicographic).

## Design constraints / decisions to resolve (architect)
- **Scoring model:** propose a concrete, deterministic score (e.g. higher = better, or
  keep "lower = better" — your call, but be explicit and consistent). Define how the
  three weighted factors + tie-breaks compose into a total order. Pick concrete weights
  such that the priority order above always holds for realistic inputs, and JUSTIFY them
  (named consts, documented). Determinism is mandatory (stable sort + explicit
  tie-breaks; no NaN/float hazards — prefer integer scoring if floats risk ties/NaN).
- **Multi-term composition:** all terms must match (AND) to be included; the entry's
  score is composed from the per-term sub-scores (e.g. SUM), and the component-start /
  contiguity factors are computed per term then aggregated (architect: define precisely
  — e.g. component-start bonus if ANY/BEST term hits a component start; contiguity from
  the best term; match_start/span union if still needed for tie-breaks).
- **Single-term + empty-query parity:** the EMPTY (and now all-whitespace) query keeps
  MRU-first "return all". For a single term, the ordering the existing tests assert
  should still hold WHERE the new model agrees; where the score model legitimately
  changes an ordering, the architect must CALL IT OUT explicitly and the test-writer
  updates that specific test with a documented rationale (do NOT silently break tests).
- **Pure / no IO / no GUI / no FFI change.** `rank_matches(query, &[FileEntry]) ->
  Vec<Match>` signature and the `Match.index` contract stay (front-ends + the FFI
  `ranked` are unchanged). `MatchKey` internals MAY change (tests assert on `.index`
  order, never on key internals — confirm).
- **Word/component semantics:** today "word" = filename (last component) split on
  `/ \ _ - . space`; matching is a ci subsequence over the FULL display. The new
  "component start" factor uses the `/`-delimited components of the full display.
  Reconcile/clarify these in the module doc.

## Tests (headless unit tests in rank.rs itself; real matching, no mocks needed — pure)
- Space-separated AND: `"wolf-gnome README"` → `crates/wolf-gnome/README.md` matches.
- Order-independence: `"README wolf-gnome"` also matches it.
- Whitespace harmless: leading/trailing/internal-extra spaces never empty the list;
  all-whitespace query == empty query (MRU-first all).
- MRU dominates: an MRU entry outranks a stronger non-MRU match (preserve the LOCKED-MRU
  intent under the new weights).
- Component-start beats scattered: a term hitting a component start ranks above a merely
  scattered subsequence.
- Contiguity beats scattered: among non-component-start matches, a contiguous run ranks
  above a gappy one.
- Single-term parity: existing single-term orderings hold (or are explicitly updated).
- Keep existing rank.rs tests green; adjust ONLY where the documented contract genuinely
  changes, with explicit rationale.

## Docs to reconcile
- `crates/wolf-core/src/rank.rs` module doc — rewrite the contract (tokenized terms,
  score-based weights + ordering, component-start + contiguity factors, MRU dominance).
- `crates/wolf-gnome/README.md` "The file picker", `crates/wolf-macos/README.md`,
  `architecture.md` "Output: file-search session" — wherever the single-subsequence /
  "strict prefix of a filename word" rule is described, update to the new model.
- prettier --write touched Markdown.

## Relevant code
- `crates/wolf-core/src/rank.rs` — `rank_matches`, `subsequence_span`, `filename_prefix`,
  `MatchKey` (8 fields), `rank_empty`, `WORD_SEPARATORS`, and the `#[cfg(test)]` table.
- `crates/wolf-core/src/files.rs` — `FileEntry { path, display, mru_rank }` (the input).
- Consumers (unchanged): `crates/wolf-ffi/src/lib.rs` `FfiFileSearch::ranked`;
  `crates/wolf-gnome/src/picker.rs`; `crates/wolf-macos` via FFI.

## Orchestrator approval (APPROVED — no blocking items)

1. **MRU as an absolute leading group** (`mru_group` then `mru_rank`), not a weight —
   APPROVED. Preserves today's LOCKED-MRU intent exactly; "MRU dominates" is then
   trivially true + testable.
2. **All 8 existing tests stay GREEN** (architect verified each verdict under the new
   model) — no assertion changes. Only fix the now-inaccurate explanatory COMMENT in
   `span_is_decisive` (assertion unchanged). The `WORD_START_BONUS` sub-bonus is included
   specifically so `word_splitting_on_separators` stays green — APPROVED.
3. Integer scoring (no floats); `Reverse<u32>` score field keeps the derived `Ord`.

## APPROVED PLAN

### `crates/wolf-core/src/rank.rs`
- **Consts:** keep `WORD_SEPARATORS=[/ \ _ - . space]`; add `COMPONENT_SEPARATORS=[/ \]`,
  `COMPONENT_START_BONUS=100`, `WORD_START_BONUS=20`, `CONTIGUOUS_BONUS=4` (documented:
  100>20>4 makes single-term tiers strict; summed across terms → soft preference for
  unbounded term counts, determinism guaranteed by tie-breaks).
- **`Match`** unchanged (`{ index, key }`). **`MatchKey`** new fields (derive
  `Debug,Clone,PartialEq,Eq,PartialOrd,Ord`), in priority order: `mru_group: u8`,
  `mru_rank: u32`, `score: core::cmp::Reverse<u32>` (higher score sorts first),
  `match_start: usize`, `span: usize`, `len: usize`, `display: String`.
- **`rank_matches`:** `let terms = query.split_whitespace().map(to_ascii_lowercase)
  .collect()` (drops empty/leading/trailing/extra/all-whitespace). If `terms.is_empty()`
  → `rank_empty(entries)`. Else per entry call `score_entry(&terms, &entry.display)`; drop
  on `None`; build MatchKey with `Reverse(score)`. Keep `#[must_use]`, stable sort by key.
- **`score_entry(terms: &[String], display: &str) -> Option<(u32, usize, usize)>`**
  (score, match_start, span): lowercase display; `components = split(COMPONENT_SEPARATORS)
  .filter(!empty)`; `words = split(WORD_SEPARATORS).filter(!empty)`. For each term:
  `subsequence_span(term, &display_lower)?` (the AND drop); accumulate
  `match_start = min(first)`, `span += last-first`; `+100` if any component
  `starts_with(term)`, `+20` if any word `starts_with(term)`, `+4` if
  `display_lower.contains(term)`. Init `match_start=usize::MAX, score=0, span=0`.
- Keep `subsequence_span`, `mru_group`. Update `rank_empty`'s `MatchKey` literal to the
  new fields (`score: Reverse(0)`, `match_start: usize::MAX`, `span:0`, len, display;
  same `(mru_group, mru_rank, index)` sort). DELETE `filename_prefix` (superseded by the
  full-display component/word logic in `score_entry`).
- Rewrite the module-doc contract (architect supplied the new text: tokenized AND terms;
  empty/all-whitespace → MRU-first all; component/word/contiguity bonuses summed; the
  7-level sort order). Remove the stale "NOTE to coder" block.

### Per-existing-test verdict — ALL GREEN (architect worked each through)
empty_query (rank_empty), case-insensitive, subsequence/nonmatch, mru_outranks (absolute
group), strict_prefix_beats_scattered (124 vs 4), word_splitting (24 vs 4 — needs
WORD_START_BONUS), full_word (124=124, len tiebreak), deterministic_tie_breaks (all 124,
len+lex), match_start_is_decisive (score 0 tie, match_start), span_is_decisive (assertion
[1,0] holds — now via CONTIGUOUS_BONUS not span; FIX the stale comment only).

### New tests (test-writer), mapped to the problem statement
1. `space_separated_terms_match_and` — "wolf-gnome README" → crates/wolf-gnome/README.md
   matches; the macos/main.rs entry dropped (no "readme").
2. `terms_are_order_independent` — "README wolf-gnome" also matches it.
3. `whitespace_is_harmless` — "  wolf-gnome   readme  " still matches; `rank_matches("   ")`
   == `rank_matches("")` (all-whitespace == empty, MRU-first all).
4. `mru_dominates_stronger_non_mru` — weak-scattered MRU entry outranks a perfect
   component-start non-MRU entry.
5. `component_start_beats_scattered` — "wolf": component-start (124) > scattered (0);
   scattered at input idx 0.
6. `contiguity_beats_scattered_among_non_component` — "abc": contiguous (4) > scattered
   (0); scattered at idx 0.
7. `multi_term_more_component_hits_score_higher` — "wolf core": both-component-start
   (248) > one-component+one-word (148); demonstrates SUM aggregation.
8. `all_terms_required_dropped_when_one_missing` — "wolf zzz" → empty (AND requirement).
Append to the existing `#[cfg(test)] mod tests`; reuse `entry()/indices()/position_of()`;
do NOT modify the 8 originals (except the `span_is_decisive` stale comment, which is the
CODER's edit, not the test-writer's — see below).

### Docs to reconcile
`crates/wolf-core/src/README.md` (## rank.rs — full rewrite of the contract);
`crates/wolf-gnome/README.md` ("The file picker" matching sentence);
`architecture.md` "Output: file-search session" (optional one clause: query tokenized,
terms AND-matched); `crates/wolf-ffi/src/lib.rs` `ranked` doc (optional one clause).
`crates/wolf-macos/README.md` NEEDS NO CHANGE (its PickerSeam section documents only
trigger/nav, not ranking). prettier --write touched Markdown.

### Pure / no FFI change / GUI-free
`rank_matches(query, &[FileEntry]) -> Vec<Match>` + `Match.index` unchanged; `MatchKey`
type name stays public (lib.rs:45 re-export) but its FIELDS may change freely (no external
reader — gnome picker + ffi use only `rank_matches` + `m.index`). No new crate. clippy:
keep both PartialOrd+Ord derives; `Reverse<u32>` keeps derived Ord correct.

### Verification (coder)
This is wolf-core pure logic → runs on the **macOS bazel host** as a rust_test. The
existing `//crates/wolf-core:file_search` / `:mru_open_records` targets don't cover
rank.rs unit tests (those are `#[cfg(test)] mod tests` IN rank.rs, i.e. wolf-core's
LIBRARY unit tests, not an integration test binary). The coder must run wolf-core's
LIB unit tests — wire/identify a bazel target for `cargo test -p wolf-core --lib` (a
rust_test on the wolf_core crate's own tests) OR run via cargo on Linux. Orchestrator
will direct the coder to find the right mechanism (likely a `rust_test` with `crate =
":wolf_core"`); these are pure (no nvim) so they need no runfiles/no-sandbox.

## Tests written (test-writer) — VERIFIED by orchestrator (agent dropped on a
## connection error during reporting, but edits landed; I inspected the file directly)

Appended to `crates/wolf-core/src/rank.rs` `#[cfg(test)] mod tests` (8 new tests; existing
8 untouched except the `span_is_decisive` COMMENT, correctly updated to explain score
(+4 contiguous) decides, assertion `[1,0]` unchanged):
1. `space_separated_terms_match_and` — "wolf-gnome README" → idx0 kept, idx1 dropped.
2. `terms_are_order_independent` — "README wolf-gnome" same result.
3. `whitespace_is_harmless` — padded query matches; `rank_matches("   ")` == `rank_matches("")`.
4. `mru_dominates_stronger_non_mru` — MRU(0) outranks non-MRU(124) → [1,0].
5. `component_start_beats_scattered` — relative `position_of`; 124 vs 0.
6. `contiguity_beats_scattered_among_non_component` — relative `position_of`; 4 vs 0.
7. `multi_term_more_component_hits_score_higher` — "wolf core": 248 vs 148 → [1,0].
8. `all_terms_required_dropped_when_one_missing` — "wolf zzz" → empty.
Orchestrator re-verified the score arithmetic in each comment against the bonus consts
(100/20/4) and the AND/subsequence rule — all correct. Tests pass once the coder lands
the scoring.

## Implementation result (coder) — ALL TESTS GREEN

Implemented per plan in `rank.rs`: new module doc; consts (COMPONENT_SEPARATORS +
100/20/4 bonuses); `MatchKey` reshaped (`mru_group, mru_rank, Reverse(score),
match_start, span, len, display`, all 6 derives kept); `rank_matches` tokenizes via
`split_whitespace` → `rank_empty` on zero terms else `score_entry` per entry;
`score_entry` (AND-drop via `?`, summed component/word/contiguous bonuses); `rank_empty`
literal updated; `filename_prefix` deleted. Docs reconciled + prettier'd
(`crates/wolf-core/src/README.md`, `crates/wolf-gnome/README.md`, `architecture.md`,
`wolf-ffi` `ranked` doc).

NEW bazel target `//crates/wolf-core:wolf_core_lib_test` (`rust_test` with
`crate = ":wolf_core"`) — runs wolf-core's LIBRARY unit tests (rank + layout), which the
integration targets (`file_search`/`mru_open_records`, over `tests/*.rs`) do NOT cover.
Reusable infra; previously wolf-core src-level unit tests never ran on the macOS host.

Verbatim: `bazel test //crates/wolf-core:wolf_core_lib_test` → **28 passed, 0 failed**
(18 `rank::tests` = 10 existing + 8 new, all named + passing; + 10 `layout::tests`).
`bazel test //crates/wolf-core:file_search //crates/wolf-core:mru_open_records` → both
PASSED (no regression). `bazel build //crates/wolf-ffi:wolf_ffi` → PASS.

Confirmed: all 8 new + all existing rank tests pass; wolf-core GUI-free (no new dep —
`core::cmp::Reverse` is std); `rank_matches`/`Match.index`/`MatchKey` type-name unchanged
(only MatchKey FIELDS changed; no external reader); FFI unaffected. clippy not runnable on
macOS (no cargo) — Bazel rustc compile clean, both PartialOrd+Ord derives kept; Linux
clippy gate should still confirm. Optional (non-blocking) gap noted: no multi-term
match_start tiebreak test.

## Review result (reviewer) — APPROVED WITH NITS (comment-only)

Independently re-derived all 10 existing rank tests + the 4 order-sensitive new tests
under the new model — all asserted `indices` hold; 28/28 claim sound. Confirmed: headline
bug fixed (`split_whitespace` drops empties; "wolf-gnome README" matches; trailing space →
rank_empty not empty-list); scoring matches contract; determinism (integer `Reverse<u32>`,
stable sort, no float/NaN); edge cases sound (score-0 subsequence still matches, leading-`/`
empty component filtered, `match_start` MAX-init + `.min`, `span` no underflow since
`last>=first`); public surface + GUI-free unchanged (ffi/gnome read only `rank_matches`+
`m.index`); prose docs (rank.rs module doc, src/README, gnome README, architecture.md, ffi
`ranked` doc) ACCURATE.

NITS (all comment-only doc-drift; route to test-writer):
1. `crates/wolf-core/src/rank.rs` — 6 UNCHANGED existing tests have comments referencing
   the DELETED `prefix_group`/`word_index` keys (lines ~346,362,368-374,391-400,417,443,459).
   Assertions correct; explanations describe an abandoned model. Update comments to the
   score model (do NOT touch assertions/code).
2. `crates/wolf-gnome/tests/smoke_headless.rs` lines ~89-90 — comment justifies row order
   via `prefix_group`/`word_index` tie; under the new model both score 124 and the
   `display` lexicographic tiebreak puts pick1.md first (assertion STILL HOLDS). Fix the
   comment only.
3. (Optional, agreed non-blocking) no dedicated multi-term match_start/span tiebreak test —
   skip.

## Orchestrator decision
Route nits 1 & 2 (stale-comment cleanup) to the TEST-WRITER — comment-only, no assertion/
code changes, satisfies CLAUDE.md's docs-as-source-of-truth. Skip nit 3 (optional). The
prose-docs technical-writer pass is a confirmed no-op (reviewer verified them accurate), so
it's folded into this cleanup.

## Status: AWAITING TEST-WRITER (comment cleanup)
