# Plan: Merge `main` into `feat/search-ui` + verify build/tests/linters

## Status: MERGE COMPLETE ✓ (commit `3946abb`)

Merged `main` into `feat/search-ui`. Conflicts (2 macOS READMEs + plan file) resolved keeping both sides. Auto-merged Swift verified semantically correct (window toolbar AND in-sidebar search header both survive; no compile fixes needed). `WindowRegistry` "three→four wake bridges" comment fixed. Reviewer APPROVED merge integrity (both features present, refactor complete, no residue). All green under bazelisk: app builds+signs; 8/8 tests (WolfKeyMapTests, wolf_core_lib_test, search, search_projection, search_no_rg, ffi:search, bundle_layout_test, rg_version_test); 4/4 linters (rustfmt_core/ffi, clippy -Dwarnings, swift_format). Not pushed (branch ahead of origin by 3).

The Search UI feature is COMPLETE (see git history `98e62cd "Search UI"`). Now merging `main` (which advanced with `96d2c97 "Toolbar"` + `ebc7300 "Move sidebar to the left"`) into this branch.

## Merge situation
- Merge base: `cceca62` (Move MRU cache to state dir).
- `main` since base (2 commits): added a window-level unified `NSToolbar` (system `.toggleSidebar` button + `.sidebarTrackingSeparator`) in `MainWindow.swift`, moved the sidebar to the left, refactored `TabTitleSeam`→`CwdTitleSeam` (+ its tests), and touched `AppDelegate.swift`, `WolfWindowController.swift`, macOS `BUILD.bazel`/READMEs, `architecture.md`, `RestoreSeam.swift`.
- Our branch since base: the whole Search UI (wolf-core `search.rs`/`search_tree.rs`, FFI `FfiSearch*` + 4th wake channel, Swift `SearchView`/`SidebarContainerView`/`SearchWakeBridge` + 3 WolfKeyMap seams, `bazel/rg` bundling, threading through `MainWindow`/`WolfWindowController`/`EditorSplitViewController`/`AppDelegate`/`Project`).

## Conflicts (from `git merge main`)
- **Textual conflicts (3 files):** `.claude/current-plan.md` (this file — resolved by rewrite), `crates/wolf-macos/README.md` (2 hunks), `crates/wolf-macos/Sources/README.md` (1 hunk). All are documentation where BOTH sides APPENDED new seam/file docs to the same region → resolution = keep BOTH sides' additions (main's CwdTitleSeam + toolbar prose AND our Search seams / SearchView / SidebarContainerView / SearchWakeBridge prose; combine the AppKit-free file counts and imported-generated-types lists coherently).
- **Auto-merged textually but SEMANTIC-RISK (must verify by building):** `MainWindow.swift`, `WolfWindowController.swift`, `AppDelegate.swift` — main's toolbar/sidebar-left restructuring vs our `SidebarContainerView` threading + `SearchWakeBridge.install()`. A clean textual merge can still be broken. The window toolbar (top-of-window toggle-sidebar) and our in-sidebar header (folder/magnifier toggles + SearchView) are ORTHOGONAL and must BOTH survive: the sidebar must still host `SidebarContainerView` (tree↔search), and the window must still install main's unified toolbar. Verify `MainWindow`'s init still receives our `SidebarContainerView` and that main's `splitViewController`/toolbar-delegate code is intact.
- **Clean adds/deletes (no conflict):** `CwdTitleSeam.swift`(+test) added, `TabTitleSeam.swift`(+test) deleted, `architecture.md`, `RestoreSeam.swift`, macOS `BUILD.bazel`, `Tests/README.md` — accept as-is but confirm BUILD.bazel `WolfKeyMapTests` srcs now lists BOTH `CwdTitleSeamTests` (main) and our three `Search*SeamTests`, and NOT the deleted `TabTitleSeamTests`.

## Resolution + verification plan
1. **coder**: resolve the 2 README conflicts (keep both sides' additions, coherent counts/lists), verify the auto-merged Swift builds and is semantically correct (fix any breakage from toolbar/sidebar-left × search-UI interaction), confirm `BUILD.bazel` test-srcs are consistent post-refactor (CwdTitleSeamTests in, TabTitleSeamTests out, Search*Tests in). Then run FULL verification with bazelisk and report exact results:
   - Build: `//crates/wolf-macos:Wolf` (app compiles + signs).
   - Tests: macOS `WolfKeyMapTests`; wolf-core `wolf_core_lib_test`, `search`, `search_projection`, `search_no_rg`; wolf-ffi `search`; `bundle_layout_test`; `bazel/rg:rg_version_test`. (Plus any main-added targets, e.g. `CwdTitleSeamTests` runs within WolfKeyMapTests.)
   - Linters: rustfmt_core, rustfmt_ffi, clippy (`-Dwarnings`), swift_format.
   - Use `--nocache_test_results` to prove fresh runs.
2. **reviewer**: verify the merge preserves BOTH main's intent (sidebar-left, unified toolbar, CwdTitleSeam) AND the Search UI, and that nothing was dropped/clobbered by the auto-merge.
3. Commit the merge only after green (the merge is mid-flight; `git commit` completes it).

## Progress log
(updated as agents report)
