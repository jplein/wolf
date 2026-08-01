# Plan: Add three commands to the Command-P palette

## Status: COMPLETE ✓ (uncommitted — no commit requested)

Three commands added to the ⌘P palette: **Toggle sidebar** (23), **Sidebar: Show project** (24), **Search in project** (25) — appended to the `COMMAND_SPECS` registry (append-only, contiguous 0..25), wired through `command_kind` → `CommandKind` → `CommandDispatcher` to `toggleSidebar()` / `showSidebar(panel:.tree)` / `showSidebar(panel:.search)`. Native-only (flavor a) — no nvim/InputEvent; gen_bindings empty diff. Existing View-menu items untouched (no duplicate menu items). Reviewer APPROVED (contract intact across Rust/FFI/Swift, correct dispatch, no UX/title mismatch, animated toggle keeps isCollapsed KVO). Docs synced (9 files, counts 23→26 / 0..22→0..25, AppMenu Tag range correctly left at 0..22), prettier-clean. GREEN under bazelisk (8/8 fresh: wolf_core_lib_test incl. new inline command tests, ffi:commands real-nvim drain, palette_rank, WolfKeyMapTests; rustfmt_core/ffi + clippy(-Dwarnings) + swift_format; app builds+signs). Left uncommitted for review.

## Feature request
Add three commands so they appear in the Command Palette (⌘P) UI:
- **Toggle sidebar** — toggles the sidebar (collapse/expand).
- **Sidebar: Show project** — reveals the sidebar showing the file/project tree.
- **Search in project** — reveals the sidebar showing the search UI (and, per the recent tweak, focuses the search field).

(These are the palette faces of actions that already exist as View-menu items: the standard "Toggle Sidebar" ⌃⌘S item, and the "Project" ⇧⌘E / "Search" ⇧⌘F AppDelegate-selector items. The menu items stay as-is; we add the palette/command faces.)

## Architecture context (authoritative: `command-lifecycle.md`)
- The palette's candidate list = the Rust command registry `COMMAND_SPECS` (`crates/wolf-core/src/command.rs`), EXCLUDING `CommandPalette` (a palette can't dispatch itself). So adding a command to the registry automatically makes it a palette candidate.
- One canonical `snake_case` id → two faces: (A) a Neovim Ex-command `:Wolf<Pascal>` + `require('wolf').<id>()`, and (B) a `command_kind` u8 discriminant carried over FFI. The `Command` enum variant ORDER is the discriminant contract and is APPEND-ONLY (never reorder; append new variants LAST).
- These three are **flavor (a) native-only** commands (checklist §8a): the action lives entirely in AppKit (toggle/show sidebar), nvim is never touched, so NO `InputEvent`, NO `input.rs` arm, NO Lua helper.
- The macOS menu is HARDCODED separately (§4) and does NOT read the registry, so setting a command's `menu: MenuGroup::View` is metadata only — it does NOT auto-create a menu item. We therefore add these commands WITHOUT new AppMenu items (the existing Project/Search/Toggle-Sidebar menu items remain untouched). Architect: verify no test/invariant asserts a menu-group↔AppMenu-item correspondence.

## Design decision (orchestrator)
Add the three as full registry commands (flavor a) to surface them in the palette; wire each `CommandDispatcher` arm to the EXISTING controller action:
- `.toggleSidebar` → a `WolfWindowController.toggleSidebar()` (new; toggles `sidebarItem.isCollapsed`, ideally via the animated `NSSplitViewController.toggleSidebar(_:)` path).
- `.sidebarShowProject` (or chosen name) → `controller.showSidebar(panel: .tree)`.
- `.searchInProject` → `controller.showSidebar(panel: .search)`.
Do NOT convert the existing menu items to the command pipeline (Toggle Sidebar as a standard AppKit item gives the automatic Hide/Show behavior; unifying is not cleanly achievable and would churn the checkmark logic we just built). Minimal, correct, two-faces-coexist.

## Constraints / must-dos (from the §8 checklist)
- Append `Command` variants LAST (append-only discriminant). Add `COMMAND_SPECS` rows (id / ex_command / title / menu). Titles shown in the palette must be exactly: "Toggle sidebar", "Sidebar: Show project", "Search in project".
- Bump `EXPECTED_COMMAND_COUNT`; add index / round-trip / spec-field tests in `command.rs` (mirror `preview_in_browser_*`; demote the prior "last row" guard to a fixed-index guard).
- Add `command_kind` constants + `FfiCommand::from_command` arms (`wolf-ffi/src/lib.rs`).
- Add `CommandKind` cases + `init?(ffi:)` arms (`CommandSeam.swift`) + `CommandSeamTests`.
- Add `CommandDispatcher` arms → the controller actions above.
- Extend `crates/wolf-ffi/tests/commands.rs` contract-guard array (contiguous `0..=N`) + a `:Wolf<Pascal>` drain step per command.
- Update `STATIC_COMMAND_COUNT` in `crates/wolf-core/tests/palette_rank.rs` (+3).
- **Regenerate + commit** swift-bridge bindings (`bazel run //crates/wolf-ffi:gen_bindings`).
- Update affected READMEs + `command-lifecycle.md` (append the three to the discriminant lists) + `prettier --write`.

## Status: ARCHITECTURE COMPLETE — tests + implementation next

## Architect's plan (code-verified)

### Canonical values (use EXACTLY)
| disc | Command variant | id | ex_command | title (palette) | command_kind const | Swift case |
|--|--|--|--|--|--|--|
| 23 | `ToggleSidebar` | `toggle_sidebar` | `WolfToggleSidebar` | `Toggle sidebar` | `TOGGLE_SIDEBAR` | `.toggleSidebar` |
| 24 | `SidebarShowProject` | `sidebar_show_project` | `WolfSidebarShowProject` | `Sidebar: Show project` | `SIDEBAR_SHOW_PROJECT` | `.sidebarShowProject` |
| 25 | `SearchInProject` | `search_in_project` | `WolfSearchInProject` | `Search in project` | `SEARCH_IN_PROJECT` | `.searchInProject` |
All `menu: MenuGroup::View`. Confirmed: current last variant `FullWidth`=22, `EXPECTED_COMMAND_COUNT`=23 → 26. Titles are EXACT sentence-case palette display strings.

### Production edits (coder)
1. `crates/wolf-core/src/command.rs`: append 3 `Command` variants LAST (after `FullWidth`, ~L90) + 3 `COMMAND_SPECS` rows (after L301) + update enum doc list (L22-35).
2. `crates/wolf-ffi/src/lib.rs`: `mod command_kind` (~L161-209) append 3 consts 23/24/25; `FfiCommand::from_command` (~L2961-2988, EXHAUSTIVE match — compile error until done) append 3 arms. **No new extern/bridge-struct → bindings expected UNCHANGED.**
3. `crates/wolf-macos/Sources/WolfKeyMap/CommandSeam.swift`: enum (L26-73) append 3 cases; `init?(ffi:)` (L78-105) append `case 23/24/25`; update doc.
4. `crates/wolf-macos/Sources/WolfMacOS/CommandDispatcher.swift`: exhaustive `switch kind` — add `.toggleSidebar → controller?.toggleSidebar()`, `.sidebarShowProject → controller?.showSidebar(panel:.tree)`, `.searchInProject → controller?.showSidebar(panel:.search)`.
5. `WolfWindowController.swift`: add `func toggleSidebar()` (prefer animated: add `MainWindow.toggleSidebar()` calling `splitViewController.toggleSidebar(nil)`, and `window?.toggleSidebar()`; fallback `sidebarItem.animator().isCollapsed = !isCollapsed` — keeps isCollapsed KVO for StatePersistence). `showSidebar(panel:)` already exists (L707, focuses search field on .search).

### Inline `#[cfg(test)]` in command.rs (coder — inline)
- `EXPECTED_COMMAND_COUNT` 23→26 (L336) + msg. Demote `full_width_is_the_last_row`→`full_width_is_at_index_22` (drop len-1 assert). Add index consts 23/24/25. Add per-command `*_id_round_trips` + `*_spec_fields` (pin id/ex_command/title/menu==View) + `*_is_at_index_NN`. New final-row guard `search_in_project_is_the_last_row` (index 25 AND == len-1). Whole-table invariant test needs no change.

### Palette mechanism (CONFIRMED)
`palette_rank::rank_palette` iterates all `COMMAND_SPECS`, only `continue` on `CommandPalette` — no menu-group filter, no per-command flag, no dedup. Appending a row ⇒ automatic palette candidate. Swift `CommandPaletteView`/`activatePaletteRow` decode `.staticCommand` rows via `CommandKind(ffi:)` → same `CommandDispatcher`. No AppMenu duplication (menu is hardcoded, ignores registry; menu metadata is informational; no MenuGroup↔menu-item test exists; `AppMenu.Tag` needs NO new values).

## Test plan (external — test-writer; NO new files, all targets already wired)
- `crates/wolf-ffi/tests/commands.rs`: add `CMD_KIND_TOGGLE_SIDEBAR=23`/`CMD_KIND_SIDEBAR_SHOW_PROJECT=24`/`CMD_KIND_SEARCH_IN_PROJECT=25`; extend the contiguous contract-guard array + its `0..25` literal + "contiguous 0..=25" msg; add a `:WolfToggleSidebar`/`:WolfSidebarShowProject`/`:WolfSearchInProject` send+drain (poll_for_command_kind) step each (after step 3m); update module-doc discriminant list. (Adding the consts to the guard array also silences dead-code lint.)
- `crates/wolf-macos/Tests/WolfKeyMapTests/CommandSeamTests.swift`: add decode asserts `CommandKind(ffi:23)==.toggleSidebar`/`24==.sidebarShowProject`/`25==.searchInProject`; move the out-of-range nil probe from 23→**26**; update header doc range 0..25.
- `crates/wolf-core/tests/palette_rank.rs`: `STATIC_COMMAND_COUNT` 22→**25** (+doc). Membership asserts iterate generically — no per-row change needed.
- Other `wolf-ffi/tests/{preview,edit,full_width,command_palette}.rs` re-declare only their own consts (no total-count pin) → no change. Only `commands.rs` has the full-array guard.

### Docs (technical-writer)
`command-lifecycle.md` §1/§4 lists; `command.rs` doc (coder); `crates/wolf-core/src/README.md` (Command list + "23 rows"→26); `crates/wolf-ffi/src/README.md` + `crates/wolf-ffi/README.md` (command_kind list); `crates/wolf-macos/Sources/README.md` + `README.md` (CommandKind list); `crates/wolf-macos/Tests/README.md` (range 0..25, probe 26); `architecture.md` §10 (twenty-three→twenty-six, 0..22→0..25). prettier.

### Bindings (coder)
`bazel run //crates/wolf-ffi:gen_bindings` — expect EMPTY diff (native-only). Commit if non-empty.

## Progress log
(updated as agents report)
