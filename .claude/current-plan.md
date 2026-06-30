# Plan: macOS file-tree sidebar

## PHASE 2 (follow-up fixes) — STATUS: COMPLETE ✅

Reviewer APPROVED, zero findings (no BLOCKER/SHOULD-FIX/NIT). Verified: `live_cwd`
is a cheap non-blocking already-canonical cell read (confirmed `apply_cwd`
canonicalizes at redraw.rs:256); `cwd()` glue routes through `live_cwd`, `""` when
absent, borrow scope safe (matches `current_file`); bindings regenerated, no wake
header change; FileTreeView control flow correct — the old `?? ""` nil/root collapse
is GONE, synthetic root kept out of `itemsByRelPath`, `""` sentinel safe,
`onRenderWake` reroot-then-reveal is loop-free, reveal expands root before
ancestors, expand-once gated. Test genuinely exercises `cwd()` vs real nvim with a
hard-failing bounded poll. All five docs accurate, GNOME untouched. Doc-verification
folded into review item 6. Nothing committed (standing rule).

### Implementation result (coder) — ALL TARGETS PASS

`live_cwd` (core) + `cwd()` (FFI bridge + glue, routed through `live_cwd`, regen
bindings, no wake header edit) + `FileTreeView.swift` restructure (synthetic
`rootItem`, `makeRoot`, `reroot(to:)`, `onRenderWake` cwd-check-first,
`refresh` expand-root-once, `reveal` expand-root-before-ancestors, `children(of:)`
explicit nil/TreeItem branch). `cwd()` consumed as `handle.cwd().toString()`
(matches `current_file()`; both emit `-> RustString`). Docs updated + prettier
(already conformant). Verbatim: wolf_core_lib_test PASSED; bridge_smoke PASSED;
file_search PASSED; tree_search PASSED (1.5s real nvim); WolfKeyMapTests PASSED;
bundle_layout_test PASSED; `//crates/wolf-macos:Wolf` builds. No codegen surprises.
GNOME untouched + not buildable on this host.

### Tests written (test-writer)

`crates/wolf-ffi/tests/tree_search.rs` extended: after `wolf_ffi_start`, bounded-poll
`handle.cwd()` (reuse `FLUSH_TIMEOUT`/`POLL_INTERVAL`) until non-empty, then assert
`Path::new(&reported_cwd).canonicalize().unwrap() == canonical_cwd` (reuses the
existing canonical seeded-cwd). Hard-fails (not skip) on timeout. `cwd()` consumed as
a plain owned `String`, exactly like `current_file()`. Coder must add the `cwd()`
bridge method + glue + regen bindings for it to compile.

### Architect corrections/confirmations (override the draft design below)

- `tracked_cwd` is ALREADY canonicalized (`redraw.rs::apply_cwd`), NOT raw. So
  `live_cwd()` returns a canonical path → no extra canonicalization in core/FFI,
  and the synthetic root's `absPath` is a clean prefix of every child's
  `abs_path`. Basename stable.
- Render-wake approach is SOUND: `:cd` always flushes → editor wake →
  `registeredTree?.onRenderWake()` (Phase 1 already wired). Initial cwd: emitted
  at setup with no wake, but startup paints + `wolf_current_file` (which DOES wake)
  fire wakes after the cell is populated. Mitigation already inherent: onRenderWake
  checks cwd unconditionally every wake, so any later wake recovers it. Do NOT make
  `CWD_EVENT` fire a wake (would change core/GNOME/MRU semantics).
- `cwd()` FFI glue routes through `WolfCore::live_cwd` (cwd is NOT in the snapshot,
  so it canNOT read `read_snapshot()` like `current_file`).
- `children(of:)` MUST branch explicitly on `nil` (→ `[rootItem]`) vs a `TreeItem`
  (→ `children_of(relPath)`); the current `?? ""` collapses nil and the synthetic
  root — fix that.
- `reroot` sets `pendingReveal = currentFileAbs` so the current buffer re-reveals
  as the new tree streams. `onRenderWake` checks cwd FIRST (reroot), then
  current_file — so a simultaneous change reveals into the NEW tree.
- ISOLATED to `FileTreeView.swift` + the core/FFI accessor. NO change to
  `WakeBridge.swift`, `AppDelegate.swift`, `MainWindow.swift` (Phase 1 wired them).
- No new deps → no `CARGO_BAZEL_REPIN`. `cwd()` is a plain `String` → regen
  bindings but NO `wolf_ffi_wake.h` edit.

### Exact changes (architect, authoritative)

- **`crates/wolf-core/src/api.rs`** — add `#[must_use] pub fn live_cwd(&self) ->
  Option<PathBuf> { self.tracked_cwd.read().clone() }` (after `mru_recent`), doc'd.
- **`crates/wolf-ffi/src/lib.rs`** — bridge decl `fn cwd(&self) -> String;` on
  `WolfCoreHandle` (next to `current_file`); glue `#[must_use] pub fn cwd(&self) ->
  String` borrowing core, `live_cwd().map_or_else(String::new, to_string_lossy)`.
- **`crates/wolf-ffi/generated/`** — `bazel run //crates/wolf-ffi:gen_bindings`,
  commit `wolf-ffi.swift`/`.h`. No wake header edit.
- **`crates/wolf-macos/Sources/WolfMacOS/FileTreeView.swift`** — new props
  `cwdPath: String=""`, `rootItem: TreeItem?`, `didInitialExpand=false`. New
  `makeRoot(_:)` (name = `URL(fileURLWithPath: cwd).lastPathComponent`, relPath
  `""`, isDir true, absPath cwd). `reroot(to:)` (set cwdPath, rebuild rootItem,
  restart `start_tree_search()`, clear `itemsByRelPath`, `didInitialExpand=false`,
  `pendingReveal=currentFileAbs`, refresh). `start()` builds root from
  `handle.cwd()`. `onRenderWake` checks cwd first (reroot on change), then existing
  current_file path. `refresh()` after reloadData: if `!didInitialExpand` && root
  has children → `expandItem(rootItem)`, set flag. `reveal` expands rootItem BEFORE
  ancestors. `children(of:)` branches nil→[root], TreeItem→children_of(relPath);
  keep synthetic root OUT of `itemsByRelPath`.
- **`crates/wolf-ffi/tests/tree_search.rs`** — after start, poll `handle.cwd()`
  until non-empty; assert `Path::new(&cwd).canonicalize().unwrap() ==
  canonical_cwd`. Document `:cd` re-root as manual smoke.
- **Docs**: `architecture.md` (cwd over FFI; macOS tree = cwd root expanded +
  follows live cwd, divergent from GNOME — replace the line ~65 "top level expanded
  by default"), `crates/wolf-ffi/README.md` (`cwd()`),
  `crates/wolf-core/src/README.md` (`live_cwd`), `crates/wolf-macos/README.md` +
  `Sources/README.md` (synthetic root, expand-on-init, live-cwd re-root, manual
  smoke). prettier --write.

### Implementation order

core `live_cwd` → ffi `cwd` + glue → regen bindings → extend tree_search test +
run → restructure `FileTreeView.swift` + build Wolf → docs + prettier.

### ORIGINAL draft Phase 2 design (superseded where it conflicts with the above)

User feedback after Phase 1 landed:

1. **Tree must follow Neovim's live cwd, like the picker.** Root cause: the picker
   calls `start_file_search()` on every open, which re-queries nvim's live
   `getcwd()` via `WolfCore::resolve_search_root()`. The tree calls
   `start_tree_search()` ONCE at launch (`tree.start()` in `AppDelegate`), so it
   roots at whatever getcwd returned at that early instant (before nvim settles
   its dir per the "Wolf follows embedded Neovim current directory" feature) and
   never re-roots on later `:cd`/`DirChanged`. Fix: expose nvim's live cwd over the
   FFI; on each editor render wake (where `:cd`/`DirChanged` already lands a redraw
   wake), re-root the tree's `FileSearch` when the cwd changed — mirroring how
   `current_file` is already handled in `FileTreeView.onRenderWake`.

2. **Show the cwd itself as a single root node, expanded on init** (VS Code /
   Snacks explorer model). Example with cwd = repo root `wolf`:

   ```
   v wolf
       BUILD.bazel
       CLAUDE.md
       ...
     > crates
       flake.lock
   ```

   The cwd (`wolf`) is a single top-level node, EXPANDED by default; its immediate
   children (`children_of("")`) are visible; their subdirectories (`crates`) stay
   COLLAPSED. This differs from the current macOS tree (children_of("") shown at
   the root with no folder node) and from GNOME — **scope this change to macOS
   only; do NOT change the GNOME tree.**

Both fixes share one new primitive: a live-cwd accessor over the FFI.

### Phase 2 design (to be formalized by architect)

- **Core (`crates/wolf-core/src/api.rs`):** add `pub fn live_cwd(&self) ->
  Option<PathBuf>` returning `self.tracked_cwd.read().clone()` (the cell
  `mru_recent` already reads). `#[must_use]`.
- **FFI (`crates/wolf-ffi/src/lib.rs`):** add bridge `fn cwd(&self) -> String;` on
  `WolfCoreHandle` (absolute path; `""` when unknown/shut down). Hand glue +
  `#[must_use]` + doc. Regenerate `generated/` (plain String method — no new wake,
  no `wolf_ffi_wake.h` edit). No Cargo dep change.
- **Swift (`FileTreeView.swift`):** introduce a synthetic root `TreeItem`
  (relPath `""`, isDir true, name = basename of cwd, absPath = cwd). Data source:
  `children(of: nil)` → `[rootItem]` (empty until cwd known); `children(of:
  rootItem)` (relPath `""`) → `children_of("")`; deeper dirs unchanged. Reveal
  must expand `rootItem` BEFORE the ancestors. `start()` + a new `reroot(to:)`
  helper: set cwdPath, rebuild rootItem, drop+restart `start_tree_search()`, clear
  `itemsByRelPath`, reset a `didInitialExpand` flag, `refresh()`. `onRenderWake`
  reads `handle.cwd()`; on change → `reroot`. `refresh()` after reloadData:
  if `!didInitialExpand` and rootItem has children → `expandItem(rootItem)`, set
  flag. Note relPath `""` is a safe sentinel (no real node has an empty rel_path).
- **Tests:** extend `crates/wolf-ffi/tests/tree_search.rs` (or a sibling) to poll
  `handle.cwd()` until it equals the seeded cwd (canonicalized, real nvim); the
  NSOutlineView synthetic-root/expand glue is not headlessly testable (document as
  manual-smoke, like the picker).
- **Docs:** `architecture.md` (cwd exposed over FFI; macOS tree follows live cwd +
  shows cwd root expanded), `crates/wolf-ffi/README.md` (`cwd()`),
  `crates/wolf-macos/README.md` + `Sources/README.md` (synthetic root,
  expand-on-init, live-cwd re-root), `crates/wolf-core/src/README.md`
  (`live_cwd`). prettier --write.

### Phase 2 checklist

- [ ] Architect plan
- [ ] Tests written
- [ ] Implementation + tests passing
- [ ] Review
- [ ] Docs updated

---

# Phase 1 (COMPLETE ✅)

Docs verified in sync by technical-writer (no edits needed — all five READMEs +

Docs verified in sync by technical-writer (no edits needed — all five READMEs +
architecture.md already accurate, no overstatement of Swift UI test coverage).
All planned macOS Bazel targets pass; `//crates/wolf-macos:Wolf` builds. Only
outstanding caveat: the GNOME `tree.rs` re-export is statically verified clean by
the reviewer but NOT compiled on this macOS host (needs a Linux/Cargo build to
confirm). Nothing committed (standing rule).

## Status checklist

- [x] Architect plan
- [x] Tests written
- [x] Implementation + tests passing
- [x] Review (APPROVED)
- [x] Docs updated (verified in sync)


## Review result (reviewer) — APPROVED

All 7 focus areas PASS, no BLOCKER/SHOULD-FIX. Verified: core projection moved
verbatim (`crate::FileEntry`, `#[must_use]` kept, dirs-first/dedup/exact-match
intact); GNOME re-export statically clean (enumerated every `tree::` consumer in
`tree_view.rs`/`lib.rs` — all covered by the glob; no duplicate tests); FFI glue
correct (empty-string fallbacks, exact-match `rel_path_for`, faithful wake-trio
clone, no lock held across a core call); generated bindings + hand wake header
correct; Swift app matches plan + picker patterns (NSSplitView, stable TreeItem
identity, first-responder return, pending_reveal, weak statics, picker overlay
still on GridView, no retain cycles); `tree_search` target a faithful real-nvim
clone; docs accurate (not aspirational — NSOutlineView glue scoped as
compile/manual-smoke only). Two informational NITs, no action. Nothing routed.

## Implementation result (coder) — ALL TARGETS PASS

Implemented per plan in order. Verbatim Bazel results:
`//crates/wolf-core:wolf_core_lib_test` PASSED; `//crates/wolf-ffi:bridge_smoke`
PASSED; `//crates/wolf-ffi:file_search` PASSED; `//crates/wolf-ffi:tree_search`
PASSED (1.4s — real nvim, not a skip); `//crates/wolf-macos:WolfKeyMapTests`
PASSED; `//crates/wolf-macos:bundle_layout_test` PASSED; `//crates/wolf-macos:Wolf`
builds; staticlib links (all new C symbols resolve). No recommended test changes.

swift-bridge: the free fn `wolf_ffi_ancestors_to_reveal -> Vec<String>` generated
cleanly (Decision-2 Swift fallback NOT needed). Iterating `RustVec<RustString>`
yields `RustStringRef` (no `.toString()`) → coder used `.as_str().toString()`;
`children_of` `FfiTreeNodeRef` accessors return owned `RustString` (`.toString()`).
`FileTreeView`/`TreeWakeBridge` need `import WolfKeyMap`.

UNVERIFIED: the GNOME `tree.rs` → `pub use wolf_core::tree::*;` re-export is not
buildable on this macOS host (no Linux/Cargo/GTK). Needs a Linux build to confirm.

## Tests written (test-writer)

- `crates/wolf-core/src/tree.rs` — created with TESTS ONLY + an `// IMPL PENDING
  (coder)` banner listing the exact public signatures to add above the
  `#[cfg(test)] mod tests` block. Ported the GNOME suite verbatim (`crate::FileEntry`,
  `use super::*;`). File will not compile until the coder adds the impl above the
  tests. GNOME copy at `crates/wolf-gnome/src/tree.rs` left untouched (coder removes
  it when wiring the re-export).
- `crates/wolf-ffi/tests/tree_search.rs` — created; real-pinned-nvim integration
  test modeled on `file_search.rs`. Coder must add the `tree_search` rust_test
  target to `crates/wolf-ffi/BUILD.bazel`.

Coder must honor the test-writer's surface assumptions: `FfiTreeNode` accessors
`name()/rel_path()/abs_path()/is_dir()`; `rel_path_for(abs)` EXACT path match
returning `""` on miss; `children_of` lists dotfiles, excludes gitignored;
`current_file()` may return `/private/var` canonical form (test canonicalizes both
sides); dir `name()` is the bare segment.

## Problem statement

Add a file-tree sidebar to the left side of the wolf-macos (AppKit) window,
matching the existing wolf-gnome file-tree sidebar and the Linux UI. Lists files
from Neovim's cwd, filtered by `.gitignore` but including dotfiles (already
provided by wolf-core's `FileSearch`). Single click opens a file; top level
expanded by default; current file highlighted + revealed after picker- and
Neovim-driven (`:e`) navigation; fed by the same streaming `FileSearch` + wake
pattern as the picker.

Reference: `crates/wolf-gnome/src/tree.rs` (pure projection) +
`crates/wolf-gnome/src/tree_view.rs` (GTK glue).

## Resolved design decisions

1. **Pure projection moves into `wolf-core`** (new `crates/wolf-core/src/tree.rs`):
   `TreeNode`, `children_of`, `ancestors_to_reveal`, `node_rel_path`,
   `rel_path_for` + their unit tests, retargeted to `crate::FileEntry`.
   `wolf-gnome/src/tree.rs` becomes `pub use wolf_core::tree::*;` (keeps
   `tree_view.rs`/`lib.rs` paths working; tests move to core). Single source of
   truth, FFI-expressible — matches architecture decisions 1 & 9.
2. **Tree projection over swift-bridge:** opaque `FfiTreeNode`
   (`name()`/`rel_path()`/`abs_path()`/`is_dir()`); methods on `FfiFileSearch`:
   `children_of(dir: String) -> Vec<FfiTreeNode>` and
   `rel_path_for(abs: String) -> String`; free fn
   `wolf_ffi_ancestors_to_reveal(rel_path: String) -> Vec<String>`. Coder must
   confirm swift-bridge 0.1.59 emits `Vec<String>` for a free fn; fallback is a
   pure Swift `ancestors` helper (PickerSeam-style) but `children_of`/
   `rel_path_for` MUST stay FFI calls.
3. **`current_file` over FFI:** `WolfCoreHandle::current_file() -> String` (empty
   for `None`), read on each editor wake. Reveal driven from `WakeBridge`: after
   `needsDisplay`, same main-thread block calls `registeredTree?.onRenderWake()`,
   which reads `current_file()`, compares to `lastCurrent`, reveals on change.
4. **AppKit:** horizontal `NSSplitView` (sidebar left = `FileTreeView`, editor
   right = `GridView`; picker overlay stays a `GridView` subview). Sidebar =
   `NSOutlineView` in `NSScrollView`, data driven by FFI projection (top level
   = `children_of("")` visible by default, dirs collapsed). Stable item identity
   via Swift `final class TreeItem` cached by `relPath`
   (`itemsByRelPath`). Single click: File → `handle.open(absPath)` +
   `makeFirstResponder(gridView)`; Dir → toggle expand. Tree gets its OWN
   long-lived `FfiFileSearch` via `start_tree_search()` and a THIRD wake
   `wolf_ffi_set_tree_wake` (clone of `wolf_ffi_set_filesearch_wake`); Swift
   `TreeWakeBridge` registers a dispatch-to-main `refresh()`. `pendingReveal`
   holds a current-file that arrived before its file streamed in.
5. **Bazel/sizing:** `wolf_macos_lib` globs `*.swift` (no BUILD edit for new Swift
   files). Regenerate `crates/wolf-ffi/generated/` via
   `bazel run //crates/wolf-ffi:gen_bindings`; hand-add
   `wolf_ffi_set_tree_wake` to `generated/wolf_ffi_wake.h`. No Cargo dep change →
   no `CARGO_BAZEL_REPIN`. Add `tree_search` rust_test to wolf-ffi BUILD. Widen
   window content width by `treePaneWidth + dividerThickness` so editor keeps its
   80-col width (`GridView.gridSize` derives from its own bounds → OCR
   undisturbed).

## Files to CREATE

- `crates/wolf-core/src/tree.rs` — pure projection moved from GNOME (impl +
  `#[cfg(test)] mod tests` ported verbatim, `crate::FileEntry`).
- `crates/wolf-macos/Sources/WolfMacOS/FileTreeView.swift` — `NSView` +
  `NSOutlineViewDataSource/Delegate`; `start()`, `refresh()`, `onRenderWake()`,
  `reveal(abs:)`, `activate(item:)`, `close()`; `final class TreeItem`. Reveal:
  expand ancestors top-down (lazy-materialize cached `TreeItem`s) BEFORE
  `row(forItem:)`; leave `pendingReveal` set when `rel_path_for` returns `""`.
- `crates/wolf-macos/Sources/WolfMacOS/TreeWakeBridge.swift` — clone of
  `PickerWakeBridge`; `weak static var registeredTree`, `install()` registers
  `wolf_ffi_set_tree_wake({ DispatchQueue.main.async { ...refresh() } })`.
- `crates/wolf-ffi/tests/tree_search.rs` — real-nvim integration test (see test
  plan).

## Files to MODIFY

- `crates/wolf-core/src/lib.rs` — `pub mod tree;` + re-exports; module doc.
- `crates/wolf-gnome/src/tree.rs` — replace impl+tests with `pub use
  wolf_core::tree::*;`.
- `crates/wolf-ffi/src/lib.rs` — bridge decls (`current_file`,
  `start_tree_search`, `FfiFileSearch::{children_of, rel_path_for}`, opaque
  `FfiTreeNode` + 4 accessors, free `wolf_ffi_ancestors_to_reveal`) + hand glue +
  `TREE_WAKE_FN`/`wolf_ffi_set_tree_wake`/`registered_tree_wake` trio (clones of
  the FILESEARCH wake trio). Doc comments + `#[must_use]`; `# Safety` on the
  extern "C" wake.
- `crates/wolf-ffi/generated/` — regenerate `wolf-ffi.swift`/`.h`; hand-add
  `wolf_ffi_set_tree_wake` to `wolf_ffi_wake.h`. Commit.
- `crates/wolf-ffi/BUILD.bazel` — `tree_search` rust_test (clone of
  `file_search`).
- `crates/wolf-macos/Sources/WolfMacOS/WakeBridge.swift` — `weak static var
  registeredTree`; call `onRenderWake()` after `needsDisplay`.
- `crates/wolf-macos/Sources/WolfMacOS/MainWindow.swift` — `NSSplitView`
  container; take the `FileTreeView`; widen width by
  `treePaneWidth + dividerThickness`; keep `makeFirstResponder(gridView)`.
- `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift` —
  `TreeWakeBridge.install()`, build `FileTreeView`, set both `registeredTree`s,
  `tree.start()`, pass tree to `MainWindow`, hold it; `tree?.close()` on
  terminate before picker/shutdown.

## Docs to update (prettier --write after)

- `architecture.md` — macOS phase status (sidebar realized; drop from deferred);
  contract (`current_file` over FFI); file-search session (tree projection +
  `wolf_ffi_set_tree_wake`).
- `crates/wolf-ffi/README.md` — new surface + wake + `:tree_search` target.
- `crates/wolf-macos/README.md` + `Sources/README.md` — new files, `NSSplitView`,
  known-limitations (drop "no file-tree sidebar"; NSOutlineView glue
  compile/manual-smoke only like the picker).
- `crates/wolf-core/src/README.md` — `tree.rs` section + lib.rs re-export list.
- `crates/wolf-gnome/src/README.md` — note `tree.rs` re-exports `wolf_core::tree`.

## Test plan

- **Core unit tests** (headless, in `tree.rs`): port the GNOME suite verbatim
  (`children_of_*`, `ancestors_to_reveal_cases`, `node_rel_path_both_variants`,
  `rel_path_for_*`, `empty_entries_yield_empty`). Run under existing
  `//crates/wolf-core:wolf_core_lib_test` (glob picks up the file). REMOVE the
  GNOME copy's tests so they don't double-run / fail against the re-export.
- **FFI integration** `crates/wolf-ffi/tests/tree_search.rs` →
  `//crates/wolf-ffi:tree_search` (real pinned nvim, modeled on `file_search.rs`,
  reuse `tests/common/mod.rs`, SKIP-when-unset, `tags` no-sandbox). Seed temp cwd:
  nested dirs (`src/main.rs`, `src/lib.rs`, `docs/intro.md`), `README.md`, a
  **dotfile** `.config.md`, a `.gitignore` + the **gitignored** file. Assert:
  `children_of("")` dirs-before-files case-insensitive, dotfile present,
  gitignored absent; `children_of("src")` two leaf files, `is_dir()==false`,
  `rel_path()`=`src/...`, `abs_path()` absolute under cwd; `rel_path_for(abs)` =
  display, non-existent → `""`; `wolf_ffi_ancestors_to_reveal("src/main.rs")` ==
  `["src"]`, `"README.md"` == `[]`; `open(abs)` then poll `current_file()` until
  it equals opened path (canonicalized, robust to `/var`→`/private/var`); core
  stays responsive (`read_frame().cols() > 0`); `shutdown()`.
- **Swift:** NSOutlineView glue not headlessly testable (no window server) —
  document like the picker. No new Swift seam unless the Decision-2 `Vec<String>`
  fallback is taken (then a PickerSeam-style `ancestors` helper +
  `WolfKeyMapTests` case).
- **Targets that must pass:** `//crates/wolf-core:wolf_core_lib_test`,
  `//crates/wolf-ffi:tree_search` (new), `//crates/wolf-ffi:bridge_smoke`,
  `//crates/wolf-ffi:file_search`, `//crates/wolf-macos:WolfKeyMapTests`,
  `//crates/wolf-macos:bundle_layout_test`; `//crates/wolf-macos:Wolf` still
  builds.

## Implementation order (avoid compile breakage)

1. Core `tree.rs` + lib.rs re-exports (→ `wolf_core_lib_test` green).
2. GNOME `tree.rs` → re-export.
3. FFI Rust glue (FfiTreeNode, methods, current_file, start_tree_search, wake
   trio, ancestors free fn) + bridge decls.
4. `bazel run //crates/wolf-ffi:gen_bindings`; hand-edit `wolf_ffi_wake.h`; commit
   generated.
5. FFI test + BUILD target; run it.
6. Swift: TreeWakeBridge → FileTreeView → WakeBridge/MainWindow/AppDelegate.
   Build `//crates/wolf-macos:Wolf`.
7. Docs + prettier.

## Notes

- No new external deps → no `CARGO_BAZEL_REPIN`.
- Hand-written FFI glue: doc comments, `#[must_use]`, `# Safety` on the extern
  "C" wake; reuse existing `// SAFETY:` transmute pattern.

## Status checklist

- [x] Architect plan
- [ ] Tests written
- [ ] Implementation + tests passing
- [ ] Review
- [ ] Docs updated
