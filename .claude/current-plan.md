# Plan: Merge `origin/main` into `feat/nav-buttons`

## Problem statement

`feat/nav-buttons` (per-pane navigation back/forward: `nav_history` core module, history FFI, `HistorySeam`, History menu + toolbar buttons) diverged from `main` at `ebc7300`. `main` has since gained the Search UI feature set (`search`/`search_tree` core modules, ripgrep vendoring under `bazel/rg`, search FFI, `SearchView`, `SidebarContainerView`, and the `SidebarMenuSeam` / `SearchToggleSeam` / `SearchHighlightSeam` / `SearchBudgetSeam` seams).

Merge `origin/main` into `feat/nav-buttons`, resolve every conflict, and verify that the app builds, linters pass, and the full test suite passes.

Note: this file previously held the nav-buttons feature plan (HEAD side) and the search-focus plan (origin/main side). Both features are complete and committed on their respective branches; this document supersedes both and resolves that conflict.

## Merge state

Merge is in progress in the worktree (`git merge --no-ff origin/main`, conflict style `diff3` — each hunk shows HEAD / ancestor `ebc7300` / `origin/main`). Conflicted files:

| File | Hunks | Nature |
| --- | --- | --- |
| `.claude/current-plan.md` | 13 | Orchestrator-owned; RESOLVED by this document |
| `crates/wolf-core/src/README.md` | 3 | Docs |
| `crates/wolf-ffi/README.md` | 1 | Docs |
| `crates/wolf-ffi/src/README.md` | 2 | Docs |
| `crates/wolf-macos/README.md` | 3 | Docs |
| `crates/wolf-macos/Sources/README.md` | 4 | Docs |
| `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift` | 1 | Code |

## Architect's headline finding

**Every conflicted hunk is a genuine "keep both" hunk. There is no either/or choice anywhere, and there are NO cross-feature collisions** — not in FFI discriminants, not in keyboard shortcuts, not in BUILD `srcs`. The one code conflict (`validateMenuItem`) is two disjoint selector branches that need concatenating.

## No test-writer pass

Decision (orchestrator): this is a merge of two already-tested, already-reviewed features. Both sides' test suites exist and are wired. No new behavior is being introduced, so no new tests are warranted. The correctness bar is "both suites pass together." If the coder finds a genuine coverage gap created by the *interaction* of the two features, it reports it and a test-writer pass is added.

---

## 1. Conflicted files — hunk-by-hunk resolution

### 1.1 `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift` (~lines 693–730) — the only code conflict

**Keep both, HEAD's block first, then main's block, then the existing `guard`.** The two blocks dispatch on disjoint selectors (`historyBackMenuItem` / `historyForwardMenuItem` / `historyJumpMenuItem` vs `showProjectTreeMenuItem` / `showSearchMenuItem`), and HEAD's `switch` already has `default: break`, so it falls through cleanly into main's `if`. Both must precede `guard item.action == #selector(handleCommandMenuItem(_:))`.

Resolved body of `validateMenuItem(_:)`, in order:

1. HEAD's comment block + `switch item.action { case #selector(historyBackMenuItem(_:)): … case #selector(historyForwardMenuItem(_:)): … case #selector(historyJumpMenuItem(_:)): return true; default: break }` — **verbatim from the HEAD side**.
2. Then main's `if item.action == #selector(showProjectTreeMenuItem(_:)) || item.action == #selector(showSearchMenuItem(_:)) { … return true }` — **verbatim from the origin/main side**.
3. Then the pre-existing `guard item.action == #selector(handleCommandMenuItem(_:)) else { return true }` and the `switch kind` below it, unchanged.

Deliberate asymmetry to PRESERVE, not "fix": HEAD's history branches resolve the controller with `controllers.first { $0.owns(effectiveKeyWindow) } ?? controllers.first`; main's sidebar branch uses `controllers.first { $0.owns(effectiveKeyWindow) }` with no fallback. Both are documented as intentional in their respective READMEs. Leave them as each side wrote them.

**Additional edit required in the same file (outside the conflict markers):** the doc comment on `validateMenuItem` (~lines 686–691) is still the *base* text ("Menu validation: disables the Close (`.closePane`) command…"). Per CLAUDE.md docs-are-truth, extend it to mention (a) the three front-end-only History branches evaluated before the `command_kind` guard, and (b) the sidebar Project/Search branch that sets a checkmark from `SidebarMenuSeam` and always returns `true`.

### 1.2 `crates/wolf-core/src/README.md` — 3 hunks

**Hunk 1 — "Dependency order".** Start from the **origin/main** sentence, then:

- Insert `nav_history` into the chain between `input` and `redraw`, giving: `render` → `layout` → `state` → `input` → `nav_history` → `redraw` → `nvim` → `files` → `search` → `mru` → `rank` → `tree` → `search_tree` → `api`.
- Inside the parenthetical, immediately after ``(`layout` is the pure intent types parsed by `redraw`;`` insert HEAD's clause verbatim: ``` `nav_history` is the pure per-pane back/forward navigation model — it depends only on `render::WinBuffer`, and `redraw` FOLDS a derived per-window target into it, so it sits after `render` and before `redraw`; ```
- The rest of main's parenthetical is byte-identical to the ancestor; keep it.

**Hunk 2 — `lib.rs` module + re-export list.** Start from **origin/main**, then:

- Add `nav_history` to the declared-modules list, alphabetically between `mru` and `nvim`.
- After the ``` `MruStore`, `MRU_LIMIT` (from `mru`); ``` clause, insert HEAD's clause verbatim: ``` `derive_target`, `disambiguate`, `MenuRow`, `NavHistory`, `NavMove`, `NavTarget`, `PaneHistory`, `MAX_ENTRIES`, `MAX_MENU_ROWS` (from `nav_history`); ```
- Keep main's ``` `SearchOptions`, `SearchMatch`, `SearchSession`, `SearchReadGuard` (from `search`); ``` and ``` `SearchFileGroup`, `ContextWindow`, `project`, `file_label`, `context_window` (from `search_tree`); ``` clauses.
- Change the trailing "(including `PreviewInfo`)" to HEAD's "(including `PreviewInfo` and `WinBuffer`)".

This matches the real `crates/wolf-core/src/lib.rs` (verified: `pub mod nav_history;` at 64, `pub mod search;`/`pub mod search_tree;` at 72–73, re-export blocks at 96–99 / 110–111, `WinBuffer` in the `render` re-export at 108).

**Hunk 3 — `api.rs`.** The resolved block is three lines: merged paragraph, blank line, HEAD's nav-history paragraph.

- Line 1: start from **origin/main's** paragraph (it carries `InputEvent::OpenAt`, `start_search`, `open_at`, `ignorecase`), then splice HEAD's ownership clause into the "`WolfCore` owns …" list — after "…unlike the `StateStore` it is NOT attached/opt-in), " and before "and the configured `cwd`" insert verbatim: ``` the sibling `NavHistoryCell` (created in `start` next to `intents` / `commands` and cloned into the redraw handler, which folds each pane's derived navigation target into it), ```
- Line 2: blank.
- Line 3: HEAD's entire `**Per-pane navigation history (back / forward).**` paragraph, **verbatim**. It then sits directly before the existing `**Crash & stall recovery (liveness + restart).**` paragraph.

### 1.3 `crates/wolf-ffi/README.md` — 1 hunk

**"RPC timeout" paragraph.** Start from **origin/main** (it adds `ignorecase` to the bounded-method list and the `start_file_search`/`start_search` exemption sentence), then splice HEAD's addition into the same list: change

`…, `open_in_browser`, and `rebuild_layout`) —`

to

`…, `open_in_browser`, `rebuild_layout`, and the three navigation-history movers `history_back` / `history_forward` / `history_jump`) —`

Everything else on main's side (including the trailing "(`start_file_search` and `start_search` are NOT in this list…)") is kept verbatim. The `history_*` bullets and the search bullets are already outside the conflict and both survived — no action there.

### 1.4 `crates/wolf-ffi/src/README.md` — 2 hunks

**Hunk 1 — "The bridge module (`mod ffi`)".** Start from **origin/main** (it has `FfiSearch`, `FfiSearchFile`, `FfiSearchMatch` after `FfiTreeNode`), then insert `FfiHistoryEntry` between `FfiPreview` and `FfiOpenFiles` — HEAD's exact position.

**Hunk 2 — `WolfCoreHandle` method prose.** Start from **origin/main** (its `start_search` / `ignorecase` / `open_at` clauses appear early, before `open`). Then splice HEAD's history clause at HEAD's insertion point: immediately after the `previews` clause "…an empty `Vec` when the core is gone), " and before "`recent_dirs` (the GLOBAL recent-directories MRU…", insert HEAD's six-method navigation-history clause verbatim (`history_can_back` / `history_can_forward` / `history_back` / `history_forward` / `history_jump` / `history_entries`). The two insertions do not overlap.

### 1.5 `crates/wolf-macos/Sources/README.md` — 4 hunks

**Hunk 1 — WolfKeyMap file count.**

- The count is **"Thirty-eight"**, not 34 and not 37. Verified against `crates/wolf-macos/Sources/WolfKeyMap/*.swift` (38 files) and the `wolf_key_map` `srcs` list in `crates/wolf-macos/BUILD.bazel` (38 entries): 33 ancestor + `HistorySeam.swift` (HEAD) + `SearchBudgetSeam.swift` / `SearchHighlightSeam.swift` / `SearchToggleSeam.swift` / `SidebarMenuSeam.swift` (main).
- Type list: start from **origin/main** (has `FfiSearch` / `FfiSearchFile` / `FfiSearchMatch` after `FfiTreeNode`), insert `FfiHistoryEntry` after `FfiPreview`.

**Hunk 2 — `AppMenu.swift`, `CommandDispatcher.swift`, `AppDelegate.swift` bullets.** Resolved block, in this order:

*(a) `AppMenu.swift` — split into two paragraphs, mirroring HEAD's structure.*

- Paragraph 1 = **main's** bullet text up through the menu-bar enumeration, with these HEAD edits applied:
  - `InstalledMenus { window; openRecent }` **pair** → `InstalledMenus { window; openRecent; history }` **TRIO**, and "so the `AppDelegate` can rebuild both dynamic tails" → "…rebuild all three dynamic tails".
  - Keep main's View-menu enumeration (`**Toggle Sidebar** ⌃⌘S / **Project** ⇧⌘E / **Search** ⇧⌘F / separator / …`).
  - Insert HEAD's `**History** (Back `⌘[` / Forward `⌘]` / separator / the active pane's recently-visited rows), ` between the View enumeration and `**Window**`.
  - Append HEAD's sentence after the Window enumeration: "**History sits BETWEEN View and Window** — the conventional place for a navigation menu — so the bar is now SEVEN menus."
  - Truncate paragraph 1 there (HEAD moved the "The item titles mirror `wolf_core::COMMAND_SPECS`…" tail into paragraph 2).
- Blank line.
- Paragraph 2 (indented two spaces) = HEAD's "The **History menu** is built by the private `historyMenuItem(target:)`…" text verbatim, followed by the original tail ("The item titles mirror `wolf_core::COMMAND_SPECS` … Compile-verified + manual-smoke only."), into which main's sidebar-items sentence is spliced **immediately after** the "**Toggle Sidebar is likewise a STANDARD AppKit item…falls through to `true`)**." sentence and **before** "Every Wolf command item carries its discriminant in `NSMenuItem.tag`…". That spliced sentence is main's, verbatim, beginning "Directly below Toggle Sidebar (and still ABOVE the same separator) `viewMenuItem(...)` adds two more NON-command, AppDelegate-targeted view-state items: **Project** … Neither carries a `Tag`/`command_kind`."

*(b) `CommandDispatcher.swift` — take **origin/main verbatim**.* HEAD's side is byte-identical to the ancestor here (it adds nothing), while main adds `.toggleSidebar` / `.sidebarShowProject` / `.searchInProject`. Keep-one is correct and safe in this sub-hunk.

*(c) `AppDelegate.swift` — start from **origin/main**, apply four HEAD splices:*

1. Keep main's "install the four wake bridges (`WakeBridge` / `PickerWakeBridge` / `TreeWakeBridge` / `SearchWakeBridge`)" — verified against `AppDelegate.swift:91–94`.
2. In the `AppMenu.install(target: self)` stash clause, append HEAD's ", and the History `NSMenu` into `historyMenu` with `historyMenu?.delegate = self` for the same lazy rebuild".
3. Replace main's "The **dynamic Open Recent submenu** is rebuilt lazily via the `NSMenuDelegate` conformance: `menuNeedsUpdate(_:)` (guarded on `menu === openRecentMenu`) … with no panel)." with HEAD's full replacement: "The **dynamic Open Recent and History submenus** are BOTH rebuilt lazily … `historyJumpMenuItem(_:)` → `historyJump(index: sender.tag)` (…which core rejects as a no-op)." Keep main's separate "Two more `@objc` view-state selectors back the View menu's sidebar items: `showProjectTreeMenuItem(_:)` and `showSearchMenuItem(_:)` …" sentence.
4. `validateMenuItem` sentence: use **HEAD's** rewritten opening ("`validateMenuItem(_:)` gains three explicit branches BEFORE its `handleCommandMenuItem` guard … Open Recent items are still untouched by it — …"), and **also keep main's** appended sidebar-branch sentence, placed after the `.fullWidth` "FOURTH-plus case" sentence. Reword main's "handled FIRST (before the `handleCommandMenuItem` tag guard…)" to "handled alongside the History branches, before the `handleCommandMenuItem` tag guard…" so the prose matches the resolved code order in §1.1.

**Hunk 3 — five bullets.** Resolved order and per-bullet source:

1. `WolfWindowController.swift` — start from **origin/main** (`SidebarContainerView`, `sidebarPanel`, `showSidebar(panel:)`, `toggleSidebar()`), then splice three HEAD fragments verbatim at HEAD's positions:
   - into the `build()` sentence, after "installs a `StatePersistence` (whose `onChange` calls `coordinator?.scheduleStateSnapshot()`),": "assigns `window.historyNav = self` right after the window is constructed (the toolbar's Back / Forward buttons delegate here EXPLICITLY — …not in the responder chain…),";
   - into `onRenderWake()`, between `paneContainer.onRenderWake()` and `tree.onRenderWake()`: "then IMMEDIATELY `window?.refreshHistoryToolbar(canBack: historyCanBack, canForward: historyCanForward)` (…must run AFTER the pane container reconciles `activePane`…),";
   - the whole "It conforms to **`HistoryNavigating`** (declared in `MainWindow.swift`) and owns the **per-pane navigation-history** surface: `historyWin` … `historyRows()` … Wolf keeps NO copy of the history." sentence, placed after the `raise()` sentence and before "It exposes the **crash/stall** methods…".

   Verified against the real file: `historyNav = self` at 244, `refreshHistoryToolbar` at 377, `sidebarContainer`/`SearchView` at 116–118 / 229–235, `sidebarPanel`/`showSidebar`/`toggleSidebar` at 704–726, `historyWin`…`historyRows` at 794–852.
2. `Project.swift` — **origin/main verbatim** (adds `onSearchWake()`; HEAD's side is the ancestor). Verified: `Project.swift:288`.
3. `WindowRegistry.swift` — **origin/main verbatim** ("the four `*WakeBridge` thunks").
4. `MainWindow.swift` — **HEAD verbatim**, all three lines (bullet, blank, the indented "The two navigation items are built by a private `navigationItem(...)`…" paragraph). Main's side is the ancestor here. Verified against `MainWindow.swift:22 / 47 / 216–328`.
5. `EditorSplitViewController.swift` — **origin/main verbatim** (wraps `SidebarContainerView`; HEAD's side is the ancestor).

**Hunk 4 — `PaneContainerView.swift` + two new bullets.** Resolved order:

1. main's `- **`SidebarContainerView.swift`** — …` bullet, verbatim.
2. main's `- **`SearchView.swift`** — …` bullet, verbatim.
3. `PaneContainerView.swift`: **HEAD's** version (it adds the "It exposes an internal `activeWin: Int64` …" sentence just before "It ALSO exposes `editPlacementInputs()`"). Before finalizing, diff main's side of this bullet against the ancestor; if main touched it too, instead take main's text and splice HEAD's one `activeWin` sentence into it at the same position.

### 1.6 `crates/wolf-macos/README.md` — 3 hunks

**Hunk 1.** Resolved block:

1. main's "The module also holds the three pure sidebar CONTENT-search seams …" intro paragraph + the three bullets (`SearchToggleSeam.swift`, `SearchBudgetSeam.swift`, `SearchHighlightSeam.swift`), verbatim.
2. main's "The module also holds `SidebarMenuSeam.swift` — …" paragraph, verbatim.
3. **One** `wolf_key_map` sentence combining both sides:
   - srcs list = ancestor list + `HistorySeam.swift` inserted after `GlyphStyle.swift` (HEAD's position) + main's four appended at the end (`SearchToggleSeam.swift`, `SearchBudgetSeam.swift`, `SearchHighlightSeam.swift`, `SidebarMenuSeam.swift`). Cross-check the final list against `crates/wolf-macos/BUILD.bazel` lines 51–88 — it must name all 38.
   - generated-glue list = ancestor + main's "the `FfiSearch` / `FfiSearchFile` / `FfiSearchMatch` content-search types" (after `FfiFileEntry`) + HEAD's "the `FfiHistoryEntry` navigation-history row type" (after `FfiPreview`).
   - Append HEAD's trailing parenthetical: "(The `srcs` list is hand-maintained, not a glob — see CLAUDE.md.)"

**Hunk 2.** Same per-bullet strategy as §1.5 hunks 2+3, applied to this file's bullets. Resolved order: `AppMenu.swift` (merged: main's ⇧⌘E/⇧⌘F View enumeration + HEAD's History menu entry and "seven menus" sentence) → blank → HEAD's indented History-menu paragraph (which carries the "Every Wolf command item carries its FFI `command_kind` u8…" tail; splice main's Project/Search view-state-items sentence into that tail, right after the Toggle Sidebar sentence) → blank → `CommandDispatcher.swift` (**main**) → `AppDelegate.swift` (merge HEAD + main, same four splices as §1.5) → `WolfWindowController.swift` (merge HEAD + main, same three splices) → `Project.swift` (**main**) → `WindowRegistry.swift` (**main**) → `MainWindow.swift` (**HEAD**) → `EditorSplitViewController.swift` (**main**) → main's three new bullets `SidebarContainerView.swift`, `SearchView.swift`, `SearchWakeBridge.swift`, verbatim.

**Hunk 3 — the long "Multiple windows now supported" paragraph.** Take **HEAD verbatim** (it carries the entire "**Per-pane back/forward navigation HAS landed**" block and the toolbar ordering), then splice main's single addition into the `AppMenu` shortcut inventory, immediately after the `⌃⌘S` **Toggle Sidebar** clause and before `⌘P` **Command Palette…**, copied verbatim from the origin/main side:

`` `⇧⌘E` **Project** and `⇧⌘F` **Search** (AppDelegate-targeted view-state items that reveal the sidebar and switch it to that panel — Search also focuses the search text field — each checkmarked when its panel is the visible one via the pure `SidebarMenuSeam`), ``

The architect searched this paragraph for any other main-side addition ("HAS landed", "ripgrep", "content search", "search panel", "four wake") and found none — this is main's only edit here.

---

## 2. Auto-merged files — semantic verdicts (architect-verified)

| File | Verdict |
| --- | --- |
| `crates/wolf-core/src/lib.rs` | **Coherent.** `pub mod nav_history;` (64) + `pub mod search;`/`search_tree` (72–73); all three re-export blocks (96–99, 110–111); `WinBuffer` in the `render` re-export (108). |
| `crates/wolf-core/src/api.rs` | **Coherent.** `NavHistoryCell` imported (35), field (155), constructed in `start` (251); six `history_*` methods + private `apply_nav` (733–824); `open_at` (468), `ignorecase` (481), `start_search` (1289–1323). No overlap. |
| `crates/wolf-core/src/nvim.rs` | **Coherent.** `ignorecase` (259–275) and `history_open` (575) / `history_set_buf` (613) all present; `preview_file` (533) untouched. |
| `crates/wolf-core/src/command.rs` | **Coherent; no discriminant collision.** HEAD added NO `Command` variants (History is deliberately front-end-only). Main appended `ToggleSidebar` = 23, `SidebarShowProject` = 24, `SearchInProject` = 25 with matching append-index guard tests. Registry is 26 rows. |
| `crates/wolf-ffi/src/lib.rs` | **Coherent; no numeric collision.** `mod command_kind` runs 0..25 with no duplicates (163–216). Four distinct wake channels — `wolf_ffi_set_wake` / `set_filesearch_wake` / `set_tree_wake` / `set_search_wake` (1232/1272/1314/1356) with separate `AtomicPtr` statics; history added none. `FfiCommand::from_command` maps all 26 variants 1:1 (3084–3109). |
| `crates/wolf-ffi/generated/*` | **Consistent, but REGENERATE to prove it** — see §3.1. Declaration order matches `mod ffi`; all of `start_search` / `ignorecase` / `open_at` / `history_*` / `FfiSearch*` / `FfiHistoryEntry` / `wolf_ffi_set_search_wake` present. |
| `crates/wolf-core/BUILD.bazel` | **Coherent.** HEAD's `nav_history` (368) + `nav_history_live` (491) and main's `search` (97) / `search_no_rg` (134) / `search_projection` (166) all survived with correct `no-sandbox` + staged runfiles + `WOLF_RG_BIN`. Every `tests/*.rs` maps to a target. |
| `crates/wolf-ffi/BUILD.bazel` | **Coherent.** `history` (1039) and `search` (188, with `//bazel/rg:rg_tree` + `WOLF_RG_BIN`) both present; all 35 files under `crates/wolf-ffi/tests/` wired. |
| `crates/wolf-macos/BUILD.bazel` | **Coherent — highest-risk file, and it is clean.** `wolf_key_map` `srcs` names all 38 sources incl. `HistorySeam.swift` (65) AND the four Search/Sidebar seams (79–83). `WolfKeyMapTests` `srcs` names `HistorySeamTests.swift` (260) AND `SearchBudgetSeamTests` / `SearchHighlightSeamTests` / `SearchToggleSeamTests` / `SidebarMenuSeamTests` (275–279). `//bazel/rg:rg_tree` in app `resources` (188). |
| `AppMenu.swift` | **Coherent; no shortcut collision.** History menu (73–74, 92–158) with ⌘[ / ⌘] (107–118); sidebar items ⇧⌘E / ⇧⌘F (279–291). Full audit: ⌘E Buffer Picker vs ⇧⌘E Project; ⌃⌘F Enter Full Screen vs ⇧⌘F Search; ⌘[ / ⌘] unused elsewhere. |
| `MainWindow.swift` | **Coherent.** `HistoryNavigating` (22), `historyNav` (47), `refreshHistoryToolbar` (218), `historyBack/ForwardToolbarAction` (231/236), `orderedItemIdentifiers` via `HistorySeam.itemIdentifiers` (258–262), Back/Forward items with `autovalidates = false` (287–328) — and the initializer now takes `sidebar: SidebarContainerView` (82). Toolbar and sidebar coexist. |
| `WolfWindowController.swift` | **Coherent.** Owns `search: SearchView?` (116) + `sidebarContainer` (118), builds/passes `SidebarContainerView` into `MainWindow` (229–235), assigns `window.historyNav = self` (244), calls `refreshHistoryToolbar` in `onRenderWake` (377), exposes sidebar surface (704–726) AND history surface (794–852); `teardown` closes both search session and container (966–968). |
| `CommandSeam.swift` | **Coherent.** `CommandKind` covers 23/24/25 (76–112), matching `command_kind` in the FFI. |
| `architecture.md` | **Coherent.** Nav-history bullet intact (36); ripgrep/content-search prose survives (45, 162, 225). |
| `runtime/nvim/init.lua` | **Clean, as predicted.** HEAD's additions all present (`wolf_buffers_suppressed` 327–338, 5-element `wolf_buffers` tuple with `is_drawer` 349–431, `wolf_history_set_buf` 1500, `wolf_history_open` 1545). Main touched nothing here — its `open_at` lands in `crates/wolf-core/src/input.rs` with no Lua helper. |

---

## 3. Cross-cutting integration work

**3.1 Regenerate the swift-bridge bindings (required).** `crates/wolf-ffi/generated/*` is machine output that git merged textually. Order matches `mod ffi`, so it is likely already byte-correct — but prove it, do not assume:

```
bazelisk run //crates/wolf-ffi:gen_bindings
git diff --stat -- crates/wolf-ffi/generated
```

An empty diff confirms the merge; a non-empty diff is the authoritative fix and must be kept.

**3.2 FFI discriminants — no renumbering needed.** HEAD introduced zero new `command_kind` / `intent_kind` / event-kind values. Main's three new command kinds (23/24/25) are strictly appended and guarded by the `*_INDEX` tests in `command.rs`.

**3.3 BUILD `srcs` — no additions needed.** All five new Swift test files and all five new Swift sources from both sides are already enumerated. All new Rust test files from both sides have `rust_test` targets. Confirm with the negative check in §4 step 4.

**3.4 Keyboard shortcuts — no reconciliation needed.** ⌘[ / ⌘] / ⇧⌘E / ⇧⌘F are all collision-free.

**3.5 Prose coherence follow-ups** (small, but CLAUDE.md makes docs load-bearing):

- The `validateMenuItem` doc comment in `AppDelegate.swift` (§1.1).
- Both AppDelegate README bullets must describe the resolved branch ORDER consistently: History branches first, then the sidebar branch, then the `handleCommandMenuItem` guard. Do not leave main's "handled FIRST" phrasing next to HEAD's "gains three explicit branches BEFORE its guard" — they contradict.
- The WolfKeyMap file count must read **Thirty-eight** (§1.5 hunk 1).

**3.6 Markdown reformat.** Per CLAUDE.md, after editing run `prettier --write` on all five resolved Markdown files, and do NOT introduce hard line breaks inside the merged paragraphs (they are deliberately single-line).

---

## 4. Verification commands

Run in order, from the worktree root. macOS toolchain is `bazelisk`, NOT `cargo`.

**Step 0 — no markers left behind:**

```
rg -n '^(<<<<<<<|\|\|\|\|\|\|\||=======|>>>>>>>)' .
```

Must return nothing.

**Step 1 — bindings are authoritative:**

```
bazelisk run //crates/wolf-ffi:gen_bindings
git diff --stat -- crates/wolf-ffi/generated
```

**Step 2 — build both front-end halves:**

```
bazelisk build //crates/wolf-core:wolf_core //crates/wolf-ffi:wolf_ffi //crates/wolf-macos:Wolf
```

**Step 3 — targeted tests for the two features (fastest signal on a bad merge):**

```
bazelisk test //crates/wolf-core:nav_history //crates/wolf-core:nav_history_live //crates/wolf-ffi:history
bazelisk test //crates/wolf-core:search //crates/wolf-core:search_no_rg //crates/wolf-core:search_projection //crates/wolf-ffi:search
bazelisk test //crates/wolf-macos:WolfKeyMapTests
bazelisk test //crates/wolf-core:commands //crates/wolf-ffi:commands
bazelisk test //crates/wolf-core:wolf_core_lib_test
```

**Step 4 — the negative check CLAUDE.md demands (nothing silently unwired):**

```
bazelisk test //crates/wolf-macos:WolfKeyMapTests --test_output=all
```

Grep the output for `HistorySeamTests`, `SidebarMenuSeamTests`, `SearchToggleSeamTests`, `SearchHighlightSeamTests`, `SearchBudgetSeamTests`.

**Step 5 — full suite (authoritative):**

```
bazelisk test //...
```

**Step 6 — lint (rustfmt is required, not advisory):**

```
bazelisk test //tools/lint:check
```

(covers `clippy_core`, `clippy_ffi`, `rustfmt_core`, `rustfmt_ffi`, `swift_format`; `bazelisk run //tools/lint:fix` auto-fixes.)

**Step 7 — Markdown:**

```
prettier --write crates/wolf-core/src/README.md crates/wolf-ffi/README.md crates/wolf-ffi/src/README.md crates/wolf-macos/README.md crates/wolf-macos/Sources/README.md
```

**Step 8 — manual smoke** (AppKit glue on both sides is compile-verified + manual-smoke only by design). Deferred to the user; not a blocker for this merge.

---

## 5. Coder results

All six files resolved; merge staged in the working tree, NOT committed.

- `gen_bindings` produced an **empty** diff — the textually-merged bindings are byte-correct.
- Build: `wolf_core` + `wolf_ffi` + `Wolf` succeeded (only pre-existing swift-bridge `@retroactive` warnings).
- `bazelisk test //...` → **91/91 PASSED**; `//tools/lint:check` → 4/4 PASSED; final combined re-run **95/95 pass**.
- `prettier --write` on all five resolved READMEs reported them unchanged.
- Step-4 negative check CONFIRMED from `bazel-testlogs/.../WolfKeyMapTests/test.log` (301 test cases, 0 failures): `HistorySeamTests` 10, `SidebarMenuSeamTests` 4, `SearchToggleSeamTests` 3, `SearchHighlightSeamTests` 8, `SearchBudgetSeamTests` 3 — all five genuinely wired and executed.

### Two deviations from the literal plan (both correct, verified against file contents)

1. §1.6's `AppDelegate.swift` bullet in `crates/wolf-macos/README.md`: HEAD's side is **byte-identical to the ancestor** (HEAD only edited the `Sources/README.md` copy), so keep-one (main) is the only correct resolution — not a four-way splice. Per §3.5, main's contradictory "handled FIRST" phrasing was reworded to "handled alongside the History branches, before the `handleCommandMenuItem` tag guard…".
2. §1.5 hunk 4's `PaneContainerView.swift` bullet: main's side confirmed byte-identical to the ancestor, so HEAD's version was taken as written.

### One flake, not a real failure

`//crates/wolf-core:file_search` FAILED once (truncated log, process died before flushing). Re-run with `--nocache_test_results`: all 11 subtests pass; `--runs_per_test=5` → 5/5. Touches neither nav-history nor search; it `git init`s into a system temp dir, the likely source. Pre-existing, not merge-related.

### Open item for the technical writer

Pre-existing doc drift on BOTH branches: the `wolf_key_map` `srcs` sentence in `crates/wolf-macos/README.md` names 27 files after the merge; `BUILD.bazel` has 38. Missing from the prose on both sides: `CmdlineSeam`, `DividerHitTest`, `DividerDragSeam`, `PaneLayoutCodec`, `StatusbarTextSpan`, `AppearanceSeam`, `RestoreSeam`, `DrawerLayout`, `DrawerFocusSeam`, `PaneOcclusion`, `FrameStats`. (`crates/wolf-macos/Sources/README.md` IS correct — "Thirty-eight", all 38 bulleted.)

## 6. Reviewer verdict

**Code side: fully correct and complete.** No conflict markers or merge debris. The `validateMenuItem` resolution verified correct (not merely plausible): `default: break` lets non-history selectors fall through, nothing is swallowed, the deliberate `?? controllers.first` asymmetry is preserved, the doc comment matches. Both features' surfaces survive intact in every auto-merged file. All BUILD `srcs` complete: 39/39 Swift test files wired, 44/44 `wolf-core` + 35/35 `wolf-ffi` Rust test files have targets. No discriminant or shortcut collisions. Deviation 2 (`PaneContainerView`) independently corroborated.

**Blocker (doc-only), plus non-blocking doc drift — all routed to the technical writer:**

- **B-1 / B-2 — `crates/wolf-macos/README.md:100`, the `AppDelegate.swift` bullet.** The coder's deviation-1 keep-one left this bullet asserting `validateMenuItem(_:)` "is UNCHANGED", which contradicts `AppDelegate.swift:705–739`, contradicts line 97 of the same file, and contradicts `Sources/README.md:58`; the reword also left a dangling "alongside the History branches" reference to History branches this bullet never describes. Plan §1.6 hunk-2 splices 2 and 3 were never applied either: the `AppMenu.install` stash clause omits `historyMenu` + `historyMenu?.delegate = self` (`AppDelegate.swift:106–107`), and the `menuNeedsUpdate` prose still says "guarded on `menu === openRecentMenu`" when the code now dispatches on menu identity with a second `menu === historyMenu` branch (634–644); the three `@objc` history actions (647–667) are undocumented here.
- **N-1** — same file, line 90: the `wolf_key_map` `srcs` sentence names 27 files; `BUILD.bazel:51–88` has 38. Missing: `CmdlineSeam`, `DividerHitTest`, `DividerDragSeam`, `PaneLayoutCodec`, `StatusbarTextSpan`, `AppearanceSeam`, `RestoreSeam`, `DrawerLayout`, `DrawerFocusSeam`, `PaneOcclusion`, `FrameStats`. Pre-existing on both branches.
- **N-2** — same file, line 101 (`WolfWindowController.swift`): missing plan §1.6 hunk-2 splice 1, the `window.historyNav = self` sentence.
- **N-3** — `crates/wolf-macos/README.md:101` and `Sources/README.md:60`: `teardown()` prose omits `search?.close()` / `sidebarContainer = nil` (`WolfWindowController.swift:966–968`). Pre-existing on main's side.
- **N-4** — `architecture.md:45`: `command_kind` table ends at `TERMINAL=20`; `command.rs` runs to 25. Pre-existing; line 162 of the same file already has the correct 26-command prose.

**Test coverage:** no gap attributable to this merge; the no-test-writer decision holds.

## Status

- [x] Scout conflicts
- [x] Architect: conflict-resolution plan
- [x] Coder: resolve + build + test — green
- [x] Reviewer: verify — code correct, doc blocker raised
- [x] Technical writer: fixed B-1/B-2 + N-1..N-4
- [x] Coder (2nd pass): prettier + re-verify lint/tests + stage — green

## 8. Final verification (coder, 2nd pass)

- **Prettier: no-op.** All three edited Markdown files byte-identical before/after (md5-verified, not just prettier's own "unchanged" report). Repo config is `.prettierrc` → `{"proseWrap": "never"}`. No hard line breaks introduced: every edit is a strict 1:1 line replacement (`git diff --numstat` 3/3, 1/1, 1/1) and each replaced line remains a single long line (9466 / 1841 / 14872 / 16710 / 16732 chars).
- **`wolf_key_map` `srcs` drift closed and independently verified**: the README sentence now names 38 files and the set matches `BUILD.bazel` exactly — zero in-build-not-in-doc, zero in-doc-not-in-build.
- **Lint: 4/4 PASS** (`clippy`, `rustfmt_core`, `rustfmt_ffi`, `swift_format`), forced with `--nocache_test_results` so `swift_format` genuinely re-checked the `AppDelegate.swift` comment edit.
- **Full suite: `bazelisk test //...` → 91/91 pass, 0 failures.** `//crates/wolf-macos:bundle_layout_test` genuinely re-ran (bundle invalidated by the Swift edit); the rest were cache hits, the correct signal for doc-only changes. `bazelisk build //crates/wolf-macos:Wolf` succeeds. The earlier `//crates/wolf-core:file_search` flake did not recur.
- **Comment fix applied** at `AppDelegate.swift:96–101`: now names all THREE dynamic submenus (Window / Open Recent / History) and draws the Window-rebuilt-explicitly vs Open-Recent-and-History-rebuilt-lazily-via-identity-dispatch distinction, matching `InstalledMenus { window; openRecent; history }` and the updated READMEs.
- **Staged**: 0 unresolved conflict paths, no unstaged changes, 59 files changed / 5350 insertions / 785 deletions.

## 7. Technical-writer pass

- `crates/wolf-macos/README.md` — rewrote the `AppDelegate.swift` bullet (removed the false "UNCHANGED" claim and the dangling reference; added the `historyMenu` stash + delegate, the `menuNeedsUpdate` identity dispatch with the `menu === historyMenu` branch feeding `AppMenu.rebuildHistoryMenu`, the three `@objc` history actions, and the real `validateMenuItem` branch order). Also completed the `wolf_key_map` `srcs` sentence to all **38** entries (count verified against `BUILD.bazel:51–88`), added the `window.historyNav = self` sentence to the `WolfWindowController.swift` bullet, and added `search?.close()` / `sidebarContainer` to the `teardown()` prose. Additionally fixed two pre-existing omissions the reviewer had not flagged: the gate list was missing `.copy` (visual-mode-only) and `.fullWidth` (always enabled, checkmark only).
- `crates/wolf-macos/Sources/README.md` — `teardown()` clause now includes `search?.close()` and `sidebarContainer = nil` in code order.
- `architecture.md` — `command_kind` table extended from `TERMINAL=20` through `PREVIEW_IN_BROWSER=21` / `FULL_WIDTH=22` / `TOGGLE_SIDEBAR=23` / `SIDEBAR_SHOW_PROJECT=24` / `SEARCH_IN_PROJECT=25`, verified against `command.rs:30–32` and `wolf-ffi/src/lib.rs:201–216`. Now consistent with line 162's 26-command prose.

The writer had no shell, so `prettier` and `git add` were deferred to the coder's second pass, along with one comment-only code fix: `AppDelegate.swift:96–99`, whose comment above `AppMenu.install(target: self)` omits the History submenu.
