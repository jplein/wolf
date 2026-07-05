# Plan: Fix high-CPU / main-thread-blocking file-tree projection (large repos)

## Status: COMPLETE ✅ — all layers implemented, reviewed, tested, documented

## Docs: verified accurate by technical-writer (coder's first-pass docs already matched the implementation; no edits needed). READMEs cover TreeIndex/TreeIndexCache, FfiFileSearch cache + version(), and the two WolfKeyMap seams + FileTreeView/WolfWindowController wiring.

## Non-blocking #1 FIXED
Coder reordered `WolfWindowController.start()`: `tree.start()` now runs AFTER the `sidebarVisibilityObservation` KVO (`.initial`) install, so the gate is seeded with real sidebar visibility before the first `refresh()`. A restored-collapsed window now defers its first refresh (no wasted reload while hidden) and catches up once on show. `bazel build //crates/wolf-macos:Wolf` PASS; `WolfKeyMapTests` PASS. Initially-visible + catch-up-on-show behavior preserved (didInitialExpand still fires on first visible refresh).
Coder's suggested extra gate test DECLINED — invariant already covered by existing T5 `testHiddenDefersAndVisibleCatchesUpOnce`.

## Reviewer verdict: APPROVED, no blocking issues. Two non-blocking notes:
1. Launch-time un-gated reload for a restored-hidden folder window: `start()`→`refresh()` runs before the gate's visibility is seeded from `isCollapsed`. Bounded (only MRU seed present; traversal streams later via gated wakes → does NOT reproduce the peg). FIXING — seed gate visibility before start().
2. Degenerate path inputs (leading/double slash) in TreeIndex::build: cannot occur, matches original behavior, not a regression. ACCEPTED as-is, no action.

## Round 1 results
- Coder implemented all three layers; ALL tests pass:
  - `//crates/wolf-core:wolf_core_lib_test` PASS (T1)
  - `//crates/wolf-ffi:tree_search` PASS (T2, real Neovim), `//crates/wolf-ffi:bridge_smoke` PASS
  - `//crates/wolf-macos:WolfKeyMapTests` PASS (T4/T5)
  - `//crates/wolf-macos:Wolf` (full signed app bundle) PASS; clippy clean.
- swift-bridge regenerated via `bazel run //crates/wolf-ffi:gen_bindings`.
- No functional divergence from plan; snapshot fields `view.version`/`view.entries` and `window.sidebarItem`/`self.tree` matched.

## Orchestrator decisions on coder's test recommendations
1. **Add FFI `version()` accessor coverage** to `tree_search.rs` — ACCEPTED (cheap, real-nvim, guards new bridge surface). Routing to test-writer.
2. **Gate+version Swift integration test requiring a refresh() extraction** — DECLINED (out of scope; gate logic already unit-covered by TreeRefreshGateTests; refactor-for-test adds risk for marginal gain).

## Tests written (test-writer)
- `crates/wolf-core/src/tree.rs` — appended T1 (7 tests) to `#[cfg(test)] mod tests`.
- `crates/wolf-ffi/tests/tree_search.rs` — extended with T2 (two-call equality; all existing assertions kept).
- `crates/wolf-macos/Tests/WolfKeyMapTests/TreeChildrenMemoTests.swift` — NEW (T4, 4 tests).
- `crates/wolf-macos/Tests/WolfKeyMapTests/TreeRefreshGateTests.swift` — NEW (T5, 4 tests).
- `crates/wolf-macos/BUILD.bazel` — added the two new Swift test files to `WolfKeyMapTests` srcs (explicit list, not a glob).
- T2 references `wolf_ffi::FfiTreeNode` (crate-root struct) — coder must keep that path valid.

## Problem statement

Opening a large repo (e.g. `~/Git/NixOS/nixpkgs`, ~100k+ files) pegs and blocks the
macOS front-end's main thread, freezing all windows — even before the file picker is
opened, and even with the sidebar hidden.

### Root cause (proven by a live `sample` — 100% of main-thread samples in this stack)

```
nvim render wake
 → WolfWindowController.onRenderWake
  → FileTreeView.onRenderWake
   → refresh() → reloadData()/reveal()
    → NSOutlineView asks for children
     → FileTreeView.children(of:)
      → FFI FfiFileSearch.children_of
       → wolf_core::tree::children_of   (hot loop: node_rel_path / String::as_str)
```

Three compounding cost multipliers:

- **Layer 1 (Swift):** `FileTreeView.children(of:)` (FileTreeView.swift:267) is recomputed
  once PER CHILD — AppKit calls `numberOfChildrenOfItem` then `child:index:` per child
  (FileTreeView.swift:221-227), each re-invoking `children_of(dir)` from scratch (no memo)
  → ~O(K^2 * N) per directory.
- **Layer 2 (Rust):** `wolf_core::tree::children_of` (tree.rs:56) scans the ENTIRE flat
  FileEntry list per directory AND uses a linear `nodes.iter().any()` dedup (tree.rs:103)
  → O(N*M) per call.
- **Layer 3 (redundant work):** Full reload/reveal on every render wake; and the outline
  work runs even when the sidebar is collapsed/hidden (why hiding the sidebar did NOT help).

## Requested fix — all three layers

1. **Swift:** Memoize `children(of:)` per `relPath`, invalidated on new snapshot / before
   reloadData, so `children_of` is called at most once per directory per reload. Preserve
   stable-identity behavior (`itemsByRelPath` reuse) and synthetic-root handling.
2. **Rust:** Make `children_of` cheap — build a per-snapshot index
   (`HashMap<dir_rel_path, Vec<TreeNode>>`) so per-directory lookup is O(children); at
   minimum replace linear dedup with a `HashSet`. Keep public API/semantics (dirs-first,
   case-insensitive name sort with rel_path tie-break, dir dedup, files as leaves), keep it
   toolkit-free/pure. Extend tests; mind the FFI boundary (`FfiFileSearch::children_of`).
3. **Redundant work:** (a) Avoid redundant full reloads when nothing structural changed.
   (b) Skip outline/children work while the sidebar is collapsed/hidden; re-run a deferred
   refresh/reveal when it becomes visible again.

## Constraints

- Documentation is source of truth (CLAUDE.md). Update READMEs to match new behavior.
- Prefer real-Neovim integration tests over mocks.
- Bazel build system. Align with prior commit b28c594 (PickerRefreshCoalescer, rank bounding).
- Acceptance signal: opening nixpkgs no longer pegs/blocks the main thread; projection no
  longer O(N) per AppKit child callback.

## Key files

- `crates/wolf-core/src/tree.rs`
- `crates/wolf-core/src/README.md`
- `crates/wolf-ffi/src/lib.rs`
- `crates/wolf-macos/Sources/WolfMacOS/FileTreeView.swift`
- `crates/wolf-macos/Sources/WolfMacOS/EditorSplitViewController.swift`
- `crates/wolf-macos/README.md`

## Orchestrator decisions on architect's flagged risks

1. **Keep the free `children_of` re-implemented via `TreeIndex` (one source of truth).** ACCEPTED — hot macOS path uses the cached `TreeIndexCache`; wolf-gnome can adopt the cache later if it shows the same storm.
2. **`version()` returns 0 for absent session.** ACCEPTED — live `version` starts at 1, so the first refresh always reloads. Safe.
3. **Two KVO observers on `sidebarItem.isCollapsed`** (existing persistence one + new tree relay). ACCEPTED — keeps `StatePersistence` focused on disk state.
4. **`rel_path_for` left O(N).** ACCEPTED — called once per reveal (rare), out of the hot path. Out of scope.
5. **Reveal while version unchanged still runs (skips reloadData).** ACCEPTED — outline structure unchanged; only selection/scroll move; children served from the still-valid memo.

## Architect's plan

### Overview
Make per-directory child projection cheap at all three layers:
- **Layer 2 (Rust core):** single per-snapshot `TreeIndex` (`HashMap<dir, Vec<TreeNode>>`) + version-keyed `TreeIndexCache`, replacing the full-flat-list scan + linear dedup per call. Free `children_of` re-implemented via the index (semantics preserved). FFI `FfiFileSearch::children_of` routes through the cached index; new `FfiFileSearch::version()` accessor.
- **Layer 1 (Swift):** `TreeChildrenMemo` seam caches `[TreeItem]` per dir so AppKit's numberOfChildren/child:index callback storm hits memory, not the FFI. Invalidated on refresh-reload and reroot. `itemsByRelPath` still provides stable identity.
- **Layer 3 (redundant work):** `TreeRefreshGate` seam — visibility gate (defer all work while sidebar collapsed; one catch-up refresh on reappear) + version gate (reloadData only when snapshot version advanced). Sidebar visibility observed via KVO on `sidebarItem.isCollapsed` in WolfWindowController, relayed to `FileTreeView.setSidebarVisible(_:)`.

### Implementation order
1. `wolf-core/src/tree.rs`: add `node_cmp`, `TreeIndex` (`build`, `children_of`), `TreeIndexCache` (`index_for(version, entries)`, `build_count`); re-implement free `children_of` via `TreeIndex`.
2. `wolf-core/src/lib.rs`: re-export `TreeIndex`, `TreeIndexCache`; update doc comment.
3. wolf-core unit tests for `TreeIndex` / `TreeIndexCache` (T1).
4. `wolf-ffi/src/lib.rs`: add `tree_cache: RefCell<TreeIndexCache>` field + set it in BOTH constructors (`start_file_search`, `start_tree_search`); route `children_of` through the cache (`view = search.read()`, `cache.index_for(view.version, &view.entries)`); add `version()` method + bridge declaration.
5. Regenerate committed swift-bridge bindings (`bazel run //crates/wolf-ffi:gen_bindings`); commit `generated/wolf-ffi.swift` + headers.
6. New WolfKeyMap seams `TreeChildrenMemo.swift`, `TreeRefreshGate.swift`; add both to `wolf_key_map` srcs in `crates/wolf-macos/BUILD.bazel`.
7. WolfKeyMap seam tests (T4/T5); add to `WolfKeyMapTests` srcs.
8. `FileTreeView.swift`: memo wiring (`projectChildren`, rewritten `children(of:)`, weak-self memo), gate wiring (rewritten `refresh()`, new `setSidebarVisible`, `reroot` invalidation + `resetReloadTracking`).
9. `WolfWindowController.swift`: sidebar visibility KVO relay (`sidebarVisibilityObservation`, `.observe(\.isCollapsed, options: [.initial, .new])`), teardown invalidation.
10. Docs (READMEs) + prettier.

### Key semantics to PRESERVE
- Projection: dirs-first, case-insensitive name sort, `rel_path` tie-break, dir dedup (one dir node per many files), files as leaves, `dir==""` == cwd root.
- Swift stable identity: memo holds the SAME `TreeItem` instances; `itemsByRelPath` reuse intact; synthetic root (`nil` → rootItem, not in `itemsByRelPath`, not memoized).
- `TreeChildrenMemo.project` closure must capture `self` weakly (FileTreeView owns memo owns closure → retain cycle otherwise).

### Detailed Rust surface
- `TreeIndex { children: HashMap<String, Vec<TreeNode>> }`; `build(entries: &[FileEntry]) -> TreeIndex` (single pass, HashSet dedup per parent, sort each vec via `node_cmp`); `children_of(&self, dir: &str) -> &[TreeNode]` (empty slice for unknown dir).
- `TreeIndexCache { cached: Option<(u64, TreeIndex)>, build_count: u64 }`; `new()`/`Default`; `index_for(&mut self, version: u64, entries) -> &TreeIndex` (rebuild + bump build_count only when version differs); `build_count(&self) -> u64`.
- Free `children_of(entries, dir) -> Vec<TreeNode>` = `TreeIndex::build(entries).children_of(dir).to_vec()`. Existing in-module tests STAY.
- `#[must_use]` on `build_count`, `TreeIndex::children_of`, `TreeIndex::build`, `version()`.

### Detailed FFI surface
- `FfiFileSearch` gains `tree_cache: RefCell<TreeIndexCache>` (RefCell already used in the file). Both constructors init `TreeIndexCache::new()`.
- `children_of(&self, dir: String) -> Vec<FfiTreeNode>` unchanged signature, body routes through cache.
- New `version(&self) -> u64` in the `extern "Rust"` bridge block; impl `self.search.as_ref().map_or(0, |s| s.read().version)`.

### Detailed Swift surface
- `TreeChildrenMemo<Item>` (Foundation-only): `init(project:)`, `children(ofDir:) -> [Item]` (compute-once-per-dir), `invalidate()`.
- `TreeRefreshGate` (Foundation-only): `init(isVisible:)`, `requestRefresh() -> Bool` (false+defer when hidden), `setVisible(_:) -> Bool` (true iff hidden→visible with deferred work), `shouldReload(version:) -> Bool` (true iff version != lastReloaded, records it), `resetReloadTracking()`.
- `FileTreeView`: `projectChildren(dir:)` extracted from old `children(of:)` loop; `children(of:)` → `nil` returns rootItem, else `childrenMemo.children(ofDir: relPath)`; `refresh()` gated (requestRefresh → shouldReload(version) → invalidate+reloadData → pending reveal); `setSidebarVisible(_:)` runs catch-up refresh; `reroot` resets tracking + invalidates memo.
- `WolfWindowController`: KVO on `window.sidebarItem.isCollapsed` with `.initial`, relays `!isCollapsed` to `tree?.setSidebarVisible(...)`; invalidated in teardown.

### Tests to write
**T1 — `wolf-core/src/tree.rs` in-module (rides `//crates/wolf-core:wolf_core_lib_test`, no BUILD change):**
- `tree_index_equivalence_all_cases` — index vs hand-built expected for `""`, `"src"`, `"docs"`, deep `a`, `a/b`.
- `tree_index_dedupes_dirs` — one `dir("a")` at root, three files under `a` sorted.
- `tree_index_dirs_first_case_insensitive_tiebreak` — mirrors `dirs_sort_before_files_case_insensitive`.
- `tree_index_deep_surfaces_intermediate_dir` — mirrors deep case.
- `tree_index_root_and_unknown_dir_empty` — empty entries → empty slices.
- `tree_index_large_shape` — thousands of synthetic entries; build once; assert per-dir counts + dirs-first on a sampled dir. Correctness only, NO wall-clock.
- `tree_index_cache_builds_once_per_version` — `index_for(1,..)` twice → build_count==1; `index_for(2,..)` → build_count==2; results correct each time. (Headless proof of once-per-snapshot.)

**T2 — `wolf-ffi/tests/tree_search.rs` (existing `tree_search` target, real hermetic Neovim):**
- Add: two consecutive `tree.children_of("src")` calls (no snapshot change) return equal name/rel_path/is_dir vectors.
- Keep all existing assertions (dirs-first, dotfiles, gitignored excluded, `rel_path_for`, ancestors, open round-trip).
- Do NOT try to observe build count across the bridge (proven headlessly in T1).

**T4 — `crates/wolf-macos/Tests/WolfKeyMapTests/TreeChildrenMemoTests.swift` (add to `WolfKeyMapTests` srcs):**
- `testRepeatedLookupsForOneDirProjectOnce` — inject counting `project`; N lookups of one dir → projectCount==1, arrays equal.
- `testDistinctDirsProjectPerDir` — two dirs → projectCount==2.
- `testInvalidateForcesRecompute` — lookup, invalidate, lookup same → projectCount==2, results equal.
- `testResultsMatchDirectProjection` — memo output equals direct `project` call.

**T5 — `crates/wolf-macos/Tests/WolfKeyMapTests/TreeRefreshGateTests.swift` (add to `WolfKeyMapTests` srcs):**
- `testHiddenDefersAndVisibleCatchesUpOnce` — hidden: requestRefresh false repeatedly; setVisible(true) true once; second setVisible(true) false.
- `testVisibleRequestProceeds` — visible: requestRefresh true; setVisible(true) unchanged false.
- `testShouldReloadGatesOnVersion` — shouldReload(1) true, (1) false, (2) true; after resetReloadTracking, (2) true again.
- `testHiddenNeverReloadsUntilVisible` — hidden never advances reload tracking; after catch-up, requestRefresh+shouldReload reloads.

### Docs to update
- `crates/wolf-core/src/README.md` — `## tree.rs`: document `TreeIndex` + `TreeIndexCache`; update `children_of` bullet (now via index, O(children) lookup, HashSet dedup, no O(N) per-call scan). `## lib.rs` re-export list: add `TreeIndex`, `TreeIndexCache`.
- `crates/wolf-ffi/` docs (module `//!` and/or README): `FfiFileSearch` caches `TreeIndexCache`; `children_of` served from per-snapshot index; document `version()`.
- `crates/wolf-macos/README.md` — `FileTreeView.swift` entry: per-dir memo + visibility/version gate; add entries for the two new WolfKeyMap seams.
- Reformat every edited Markdown with `prettier --write`; no hard line breaks in paragraphs.

### Bazel / swift-bridge wiring the coder must not forget
- `fn version(&self)` REQUIRES `bazel run //crates/wolf-ffi:gen_bindings` + commit regenerated `generated/wolf-ffi.swift` (+ headers). `children_of` change is Rust-internal, no codegen.
- `crates/wolf-macos/BUILD.bazel`: add `TreeChildrenMemo.swift`, `TreeRefreshGate.swift` to `wolf_key_map` srcs; add the two test files to `WolfKeyMapTests` srcs.
- No new wolf-core/wolf-ffi BUILD targets. Only `std::collections::{HashMap, HashSet}`.

### Lint notes
- `TreeIndexCache::index_for`: do stale-check + insert as separate statements before returning `&self.cached...` (avoid borrow friction / `significant_drop_in_scrutinee`).
- `TreeChildrenMemo.project` closure `[weak self]` (retain-cycle avoidance).
