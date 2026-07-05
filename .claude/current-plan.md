# Plan: File-picker filtering performance fix

## Problem statement

Opening a large repo (e.g. ~/Git/NixOS/nixpkgs, ~91,600 files) in the file picker:
the picker opens quickly, but filtering burns a lot of CPU. The UI is NOT the
bottleneck (already capped at `maxRows=200`, `NSTableView` is virtualized). Four
compounding costs were identified:

1. **Un-coalesced streaming wakes.** `files.rs` traversal fires `wake()` every
   `TRAVERSAL_BATCH=256` files (~358 wakes for nixpkgs). `PickerWakeBridge.swift`
   handles each with a bare `DispatchQueue.main.async { refresh() }` — no debounce.
   Each `refresh()` re-ranks the entire accumulated list → ~16M entry-scorings
   during initial load. FIX: coalesce/debounce `refresh()` (~16–33ms) so batches
   collapse instead of one full re-rank per batch.

2. **Full sort + clone in `rank_matches`** (`crates/wolf-core/src/rank.rs`): does a
   full O(M log M) sort of all matches and clones `display` into every `MatchKey`,
   just to `take(200)`. FIX: bounded top-N select (bounded heap / select_nth), only
   clone `display` for survivors.

3. **Per-entry allocation in `score_entry`**: allocates a lowercased String + two
   Vecs (components, words) per entry per keystroke (~275k allocations per scan
   over nixpkgs). FIX: reusable scratch buffers / allocation-free ci matching.

4. **Empty-query path (`rank_empty`)**: sorts all 91,600 entries (cloning every
   display) just to show 200 MRU-first. The order is only `(mru_group, mru_rank,
   index)`. FIX: special-case — emit MRU entries then first `(200 - mru_count)`
   non-MRU in original order, no full sort.

## Constraints

- Do NOT change match semantics / ranking order. All existing tests in
  `crates/wolf-core/tests` and `rank.rs` unit tests must still pass.
- `rank_matches(query, entries) -> Vec<Match>` returns ALL matches best-first
  today; FFI `FfiFileSearch::ranked` (`crates/wolf-ffi/src/lib.rs`) calls
  `rank_matches(...).take(max)`. Architect decides: add a bounded variant
  (e.g. `rank_matches_top_n`) used by the FFI while keeping `rank_matches` intact,
  or thread a limit through. Keep existing behavior/tests green.
- Prefer end-to-end/integration tests starting a real traversal (see
  `crates/wolf-ffi/tests/file_search.rs`); add a large-tree test.
- Update relevant README.md files (rank.rs → `crates/wolf-core/src/README.md`;
  files.rs; wolf-macos picker) per documentation-is-source-of-truth. After editing
  Markdown, run `prettier --write`.

## Status: COMPLETE

Implemented, reviewed (APPROVED, no BLOCKER), documented, prettier-clean, all tests
green. Docs updated across 5 READMEs (wolf-core src/tests, wolf-ffi src/tests,
wolf-macos); prettier --write reports all unchanged (already conform).

### Reviewer (agent a946f24ce9460bd5d): APPROVED, no BLOCKER.
Equivalence invariant + empty fast path + allocation-free score_entry all verified
correct. Only outstanding: docs (already assigned to technical-writer). NIT:
cancelPending() only clears the flag (armed timer still fires, but refresh() is a
no-op when closed) — acceptable, documented.

### Coder results (agent a6a258c843275d8d4)
All 4 fixes implemented per plan. Tests run via bazelisk on macOS (no cargo in env):
- `//crates/wolf-core:wolf_core_lib_test` + `:rank_bounded` → PASSED
- `//crates/wolf-core:file_search` → PASSED (real nvim)
- `//crates/wolf-ffi:file_search` → PASSED, 2 tests incl. new `file_search_bounds_large_tree`
- `//crates/wolf-macos:WolfKeyMapTests` → PASSED (incl. PickerRefreshCoalescerTests)
- lib builds clean.
Deviation: reflowed the wolf-ffi `use` block to stay ≤100 cols (import grew). Could
not run cargo fmt/clippy (no cargo in env); rustc builds warning-clean.
BUILD collision handled correctly: coder added only the LIBRARY src.
OUTSTANDING: docs (plan step 8) NOT done — for technical-writer.

### Tests written (test-writer, agent a036aa539654c4479)
- CREATED `crates/wolf-core/tests/rank_bounded.rs` (7a) + `rank_bounded` rust_test in
  `crates/wolf-core/BUILD.bazel`.
- EDITED `crates/wolf-ffi/tests/file_search.rs` (added `file_search_bounds_large_tree`).
- CREATED `crates/wolf-macos/Tests/WolfKeyMapTests/PickerRefreshCoalescerTests.swift`
  (7d) + added ONLY the TEST src to `WolfKeyMapTests` srcs in
  `crates/wolf-macos/BUILD.bazel`.

### ⚠ BUILD collision to honor (coder):
The test-writer ALREADY added `"Tests/WolfKeyMapTests/PickerRefreshCoalescerTests.swift"`
to the `WolfKeyMapTests` swift_test `srcs`. The coder must add the LIBRARY src line
`"Sources/WolfKeyMap/PickerRefreshCoalescer.swift"` to the `WolfKeyMap` swift_library
`srcs`, but MUST NOT re-add the test src line (avoid duplicate-label error).

---

# Implementation Plan (from architect)

## Summary

Four fixes, picker-only. Keep `rank_matches` public API + all existing tests 100%
intact. Out of scope: `palette.rs`/`rank_palette` (tiny candidate set), `files.rs`
(TRAVERSAL_BATCH stays 256), sidebar tree wake.

Key files:
- `crates/wolf-core/src/rank.rs` — fixes #2, #3, #4
- `crates/wolf-core/src/lib.rs` — re-export `rank_matches_top_n`
- `crates/wolf-ffi/src/lib.rs` — `FfiFileSearch::ranked` switch (~line 1930)
- `crates/wolf-macos/Sources/WolfKeyMap/PickerRefreshCoalescer.swift` (NEW) +
  `PickerView.swift` + `PickerWakeBridge.swift` — fix #1

## Fix #2 — Bounded top-N ranking

Add `pub fn rank_matches_top_n(query: &str, entries: &[FileEntry], n: usize) -> Vec<Match>`.
- Must return EXACTLY `rank_matches(query, entries).into_iter().take(n).collect()`
  for ALL inputs. `rank_matches`/`rank_empty` unchanged.
- Effective order is `(MatchKey, index)` ascending (rank_matches uses a STABLE
  sort and MatchKey lacks index, so index is the implicit final tiebreak). Bounded
  path MUST use `index` as final tiebreak.
- Algorithm: private borrowed key `struct KeyRef<'a>` mirroring MatchKey field order
  (mru_group, mru_rank, score:Reverse<u32>, match_start, span, len, display:&'a str,
  index:usize). Derive PartialOrd+Ord+PartialEq+Eq+Copy+Clone+Debug. index makes it a
  TOTAL order → final `sort_unstable` is deterministic.
  - `BinaryHeap<KeyRef>` max-heap cap n (peek = current WORST). For each matching
    entry: if len<n push; else if cand < peek, pop+push. Drain → `into_vec()` →
    `sort_unstable()` ascending → map to owned `Match` cloning `display` ONLY for
    the ≤n survivors.
  - n==0 → early `Vec::new()`. terms.is_empty() → delegate to empty fast path.
  - O(M log n), ≤n display clones.
- Add `PartialEq, Eq` to `Match`'s derive (additive) so tests can `assert_eq!`.

## Fix #3 — Allocation-free `score_entry`

No lowercased String, no component/word Vecs, no scratch buffers. ASCII-only folding
preserved (never Unicode). Preserve bonuses 100/20/4, separators, match_start/span.
- `subsequence_span_ci(term, display)`: walk `display.chars().enumerate()`, compare
  `qc == hc.to_ascii_lowercase()` (term already lowercased). Same (first,last) char
  indices, None if term not consumed.
- `ci_starts_with(hay, needle)` and `ci_contains(hay, needle)`: ASCII-folded char
  comparison, no alloc. Parity with old `component.starts_with`/`display_lower.contains`.
- Body: for each term, subsequence_span_ci (AND-drop on None); component/word bonuses
  via `display.split(SEP).filter(non-empty).any(|c| ci_starts_with(c, term))`;
  contiguous via `ci_contains`. Both rank_matches and rank_matches_top_n call this
  same refactored score_entry.
- Regression guard = EXISTING in-module rank.rs tests (do NOT modify them).

## Fix #4 — Empty-query fast path

Private `fn rank_empty_top_n(entries, n) -> Vec<Match>`, called when terms empty.
No global sort. Partition in index order into `mru` (mru_rank.is_some()) and `nonmru`
(index order already). Stable-sort `mru` by `mru_rank` asc. Concat mru++nonmru,
take(n), build Match with same MatchKey rank_empty would (score Reverse(0),
match_start MAX, span 0, len chars().count(), display clone). MRU set is small
(MRU_LIMIT=50). ≤n clones.

## Fix #1 — Swift debounce/coalescing

New `crates/wolf-macos/Sources/WolfKeyMap/PickerRefreshCoalescer.swift` (Foundation
only, headless-testable seam):
- `public static let refreshInterval: TimeInterval = 0.033` (~30fps).
- init(schedule: (@escaping ()->Void)->Void, perform: ()->Void). `var scheduled=false`.
- `request()`: if scheduled return (coalesce); else scheduled=true, schedule
  { scheduled=false; perform() }. Optional `cancelPending()`.
- Guarantees: burst collapses to one trailing fire; fire clears flag so a later wake
  reschedules (final/complete wake never dropped); fire reads latest snapshot.
- `PickerView.swift`: add lazy `refreshCoalescer` (schedule via
  `DispatchQueue.main.asyncAfter`); add `func scheduleRefresh() { refreshCoalescer.request() }`.
  Keystrokes (`controlTextDidChange`) + `open()` keep calling `refresh()` DIRECTLY
  (immediate). `close()` may `cancelPending()`.
- `PickerWakeBridge.swift`: change thunk to `picker?.scheduleRefresh()` (still inside
  DispatchQueue.main.async).

## FFI change

`crates/wolf-ffi/src/lib.rs`: import `rank_matches_top_n` (replace `rank_matches`,
keep `rank_palette`); `FfiFileSearch::ranked` uses
`rank_matches_top_n(&query, &view.entries, max).into_iter()` (drop redundant `.take`).
Update doc comment reference.

## Tests

- **7a (NEW `crates/wolf-core/tests/rank_bounded.rs`, headless, own BUILD target
  mirroring palette_rank):** central property `rank_matches_top_n(q,&e,n) ==
  rank_matches(q,&e).into_iter().take(n).collect()` via `assert_eq!`. Cases: empty +
  all-whitespace query (MRU-mixed set); single term; multi-term AND; order-independent;
  no-match; tie group straddling n (many "a*.md" for "a"); MRU weak-beats-strong;
  n>match count; n==0; n==1; large synthetic 5–10k in-memory set n=200; one non-ASCII
  display+query char (ASCII-fold parity).
- **7b (fix #3 guard):** EXISTING in-module rank.rs tests stay green; no new dupes there.
- **7c (extend `crates/wolf-ffi/tests/file_search.rs`, real nvim+fs):** temp cwd with
  ~5,000 files (~500 sharing `alphaNNNN.md` + one `alpha.md`); poll ranked("",200)
  until streamed; assert ranked("alpha",200) returns exactly 200 rows, `alpha.md`
  first, every display contains a-l-p-h-a subsequence. Follow SKIP-when-unset +
  poll pattern; tempfile::tempdir(). Lower to ~2,000 if slow.
- **7d (NEW `crates/wolf-macos/Tests/WolfKeyMapTests/PickerRefreshCoalescerTests.swift`,
  XCTest, follow PickerSeamTests):** inject capturing `schedule` + counting `perform`.
  Cases: N rapid request()→one fire→perform once; after fire, request() reschedules+
  fires again; single request fires once; refreshInterval in 16–33ms band.
- No mocks for traversal (real temp dirs / in-memory data).

## BUILD wiring

- `crates/wolf-core/BUILD.bazel`: add `rust_test` `rank_bounded` copying headless
  `palette_rank` target (no data/env/nvim).
- `crates/wolf-macos/BUILD.bazel`: WolfKeyMap lib srcs += PickerRefreshCoalescer.swift;
  WolfKeyMapTests srcs += PickerRefreshCoalescerTests.swift (explicit lists,
  alphabetical). WolfMacOS is globbed (no change). file_search target already lists
  the file.

## Docs (prettier --write after each; no hard breaks mid-paragraph)

- `crates/wolf-core/src/README.md`: rank.rs section — rank_matches_top_n, empty-query
  fast path, allocation-free ci scoring; lib.rs re-export list += rank_matches_top_n.
- `crates/wolf-core/tests/README.md`: new rank_bounded bullet; extend file_search
  picker bullet; update aggregate counts.
- `crates/wolf-macos/README.md`: PickerRefreshCoalescer seam; PickerWakeBridge now
  scheduleRefresh (coalesced) while keystrokes immediate.

## Implementation order

1. rank.rs allocation-free score_entry (#3). 2. rank.rs Match Eq + KeyRef +
rank_matches_top_n + rank_empty_top_n (#2,#4). 3. lib.rs re-export. 4. wolf-ffi
ranked switch. 5. Swift coalescer + wiring. 6. BUILD updates. 7. tests. 8. docs+prettier.

## Lint notes

KeyRef: derive both PartialOrd+Ord (clippy::derive_ord_xor_partial_ord) + Copy.
rank_matches_top_n `#[must_use]`. Avoid clippy::comparison_chain in heap loop
(keep len<n / peek branches distinct). No new #[allow].

Architect agent id: a13ced4d31d81e21a
