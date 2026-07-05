# Plan: Remove toolbar + titlebar icon mode plumbing; add View > Toggle Sidebar

## Status: COMPLETE ✅ — implemented, tested green (Bazel), reviewed (approved), docs updated + prettier-clean

### Outcome
- Request 1: `NSToolbar` + `EditorToolbarDelegate` removed from MainWindow.swift; chrome-height math kept AFTER contentViewController assignment (verified). No toolbar shown.
- Request 2: `ToolbarMode` removed end-to-end — wolf-core (enum/field/setter/re-export), wolf-ffi (mod toolbar_mode, bridge fns, FfiWindowState accessors), regenerated bindings (clean 6-line diff), wolf-macos (StateSeam enum, restore, StatePersistence KVO/pushToolbar). Backward-compat: no deny_unknown_fields; new `legacy_toolbar_display_mode_key_is_ignored` test proves old state.json still loads.
- User decision: View > Toggle Sidebar added as FIRST View-menu item (⌃⌘S, standard NSSplitViewController.toggleSidebar via responder chain) + separator below it.
- Tests: all Bazel targets green (wolf-core state_unit + full crate, wolf-ffi state/bridge_smoke/full, wolf-macos WolfKeyMapTests + bundle_layout; all //crates/... 36 targets PASS).
- Reviewer: APPROVED (6/6 areas pass; chrome-math ordering correct; menu item wired correctly; no ⌃⌘S collision).
- Docs: all affected READMEs + architecture.md + build.md + BUILD.bazel comment updated; prettier-clean (proseWrap: never).
- REMAINING: manual GUI smoke (needs window server) — launch app: no toolbar; editor keeps exact cols×rows below titlebar; View menu's first item is Toggle Sidebar w/ separator; ⌃⌘S toggles sidebar; sidebar resizes/restores.
- NOTE: pre-existing `.direnv` vendored snapshot makes `bazel build //...` fail to load (stale copy); real tree `//crates/...` passes fully — unrelated to this change.

## Status (historical): architect plan approved — ready for test-writer + coder

## Problem statement

Two related UI cleanups in the wolf-macos AppKit app, requiring cross-layer removal of the "titlebar icon mode" (toolbar display mode) plumbing.

- **Request 1** — Don't show the toolbar (remove the `NSToolbar` from `MainWindow.swift`; transparent `.fullSizeContentView` titlebar remains).
- **Request 2** — Remove the entire `ToolbarMode` / display-mode-customization feature end to end across wolf-core, wolf-ffi, wolf-macos + docs.

## USER DECISION (locked)

Removing the toolbar removes the ONLY manual sidebar-toggle affordance (the toolbar button; there is no existing View-menu item or keyboard shortcut). The user decided: **ADD a "Toggle Sidebar" item to the View menu — it must be the FIRST item, with a separator immediately below it**, using the standard `#selector(NSSplitViewController.toggleSidebar(_:))` routed through the responder chain, key equivalent **⌃⌘S** (Control-Command-S; no conflict with existing shortcuts). See §B below.

## Constraints
- Documentation is source of truth (CLAUDE.md). Update every affected README. Reformat edited Markdown with `prettier --write`; no hard line breaks mid-paragraph.
- Prefer end-to-end/integration tests that start a real Neovim process. State persistence tests must still pass without the toolbar_mode facet.
- Backward-compat: existing state.json may contain a `"toolbar_display_mode"` key — serde must not choke on the now-unknown key on load. VERIFIED SAFE by construction (no `deny_unknown_fields` on `WindowState` or the private `StateFile`). Lock it with a new backward-compat test (§4.2).
- Build: Bazel on macOS (bazelisk). Swift app targets: WolfMacOS / WolfKeyMap.

---

## A. Verified facts that shape the plan

- **Backward compat safe:** `crates/wolf-core/src/state.rs` uses no `deny_unknown_fields`; a stale `"toolbar_display_mode"` becomes an ignored unknown field. Confirm via grep + new test.
- **Chrome-height math self-adjusts, no change:** `MainWindow.swift` init sets `contentViewController = splitVC`, then computes `chrome = contentView.bounds.height − contentLayoutRect.height` and `setContentSize(...)`. `contentLayoutRect` reads live; with no toolbar it reflects titlebar-only chrome and the editor keeps its exact cols×rows. Keep the measurement AFTER `contentViewController` is set. Only delete the toolbar block that sits between `contentViewController` assignment and the measurement.
- **Editor stays pinned below titlebar:** `EditorSplitViewController` CONTENT host pins editor top to `safeAreaLayoutGuide.topAnchor`; safe-area top inset tracks the titlebar under `.fullSizeContentView`. Doc-comment wording only ("titlebar/toolbar" → "titlebar"); no code change there.
- **`.toggleSidebar`** is a first-responder action of `NSSplitViewController` (the content VC, in the responder chain). Works once invoked from the new View menu item (§B).
- **Generated-binding edits are small contiguous blocks** → hand-removal is deterministic (§5).

---

## 3. Files to modify — Request 2 (ToolbarMode removal), exhaustive

### 3.1 `crates/wolf-core/src/state.rs`
- Module doc (~line 7): drop "optional toolbar display mode" from the per-window field list.
- DELETE `ToolbarMode` enum + doc (~57–74).
- DELETE `toolbar_mode` field + its `#[serde(rename = "toolbar_display_mode", …)]` attr + doc (~98–105).
- DELETE `toolbar_mode: None,` in `Default for WindowState` (~117).
- DELETE `set_toolbar_mode(&self, idx, mode)` + doc (~319–328).

### 3.2 `crates/wolf-core/src/lib.rs`
- Line ~71: remove `ToolbarMode` from `pub use state::{…}`.

### 3.3 `crates/wolf-ffi/src/lib.rs`
- Remove `ToolbarMode` from the `use wolf_core::{…}` import (~51).
- DELETE `mod toolbar_mode` discriminant module + doc (~235–248).
- DELETE bridge decl `fn state_set_toolbar_mode(&self, mode: u8, set: bool);` + comment (~432–437).
- DELETE `FfiWindowState` bridge accessor decls `toolbar_mode()` / `toolbar_mode_set()` + comments (~541–546).
- DELETE `toolbar_mode: u8,` and `toolbar_mode_set: bool,` from `struct FfiWindowState` (~936–937); reword its doc (~922–927).
- DELETE `impl FfiWindowState` accessor methods `toolbar_mode()` / `toolbar_mode_set()` + docs (~977–988).
- In `FfiWindowState::from_window` (~990–1009): DELETE the `let (mode, set) = match w.toolbar_mode {…}` block (~992–998) and the `toolbar_mode: mode,` / `toolbar_mode_set: set,` literal lines (~1006–1007).
- DELETE `pub fn state_set_toolbar_mode(&self, mode, set)` impl + doc (~1444–1469).
- Re-grep the crate `//!` header for stragglers.

### 3.4 `crates/wolf-ffi/generated/wolf-ffi.swift`
- DELETE `state_set_toolbar_mode` wrapper (~339–341) and `toolbar_mode()`/`toolbar_mode_set()` wrappers (~887–893).

### 3.5 `crates/wolf-ffi/generated/wolf-ffi.h`
- DELETE line ~210 (`state_set_toolbar_mode`) and ~235–236 (`toolbar_mode` / `toolbar_mode_set`).

### 3.6 `crates/wolf-macos/Sources/WolfKeyMap/StateSeam.swift`
- DELETE `public enum ToolbarMode: UInt8 {…}` + doc (~22–38). Keep `restoredWindowFrame`, `launchDecision`. Header comment's "NO NSToolbar" stays true.

### 3.7 `crates/wolf-macos/Sources/WolfKeyMap/PaletteSeam.swift`
- ~line 28: rewrite `PaletteRowKind` doc to drop the dangling "sibling of `StateSeam.ToolbarMode`" reference (keep the `CommandKind(ffi:)` sibling wording).

### 3.8 `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift`
- In `restoredWindows(from:)` (~293–313): DELETE the `toolbarMode` decode (~301–302) and the `toolbarMode:` arg in the `RestoredWindowState(...)` init (~309). Update the doc (~295).

### 3.9 `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift`
- `struct RestoredWindowState`: DELETE `let toolbarMode: StateSeam.ToolbarMode?` (~25); doc (~17) "frame/sidebar/toolbar" → "frame/sidebar".
- `start()` restore block (~205–218): DELETE the `if let mode = restored.toolbarMode { window.toolbar?.displayMode = Self.toolbarDisplayMode(mode) }` (~215–217). Keep frame restore. Comment (~202) → "frame".
- `StatePersistence(...)` init (~240–245): DELETE `toolbar: window.toolbar` arg (~244). Comment (~238) → "geometry/sidebar".
- DELETE static `toolbarDisplayMode(_:) -> NSToolbar.DisplayMode` + doc (~590–597).
- Fix docs: `restored` field doc (~47) and `pushSnapshot()` doc (~494–495, "backstop for the best-effort toolbar KVO").

### 3.10 `crates/wolf-macos/Sources/WolfMacOS/StatePersistence.swift`
- Header (~2–11) + class doc (~16–20): now observes geometry + sidebar only; `pushSnapshot()` pushes TWO facets.
- DELETE `private weak var toolbar: NSToolbar?` (~25).
- DELETE `toolbarObservation` var + doc (~30–32).
- DELETE `toolbar: NSToolbar?` init param (~40) and `self.toolbar = toolbar` (~45).
- DELETE `toolbarObservation?.invalidate()` in `deinit` (~55).
- `install()`: DELETE the `toolbarObservation = toolbar?.observe(\.displayMode …)` block (~95–98); update doc (~58–61).
- `pushSnapshot()`: DELETE the `pushToolbar()` call (~108); doc → "geometry + sidebar visibility".
- DELETE `pushToolbar()` method + doc (~126–147).

### 3.11 `crates/wolf-macos/Sources/WolfMacOS/MainWindow.swift` (Request 1)
- Header (~1–10) + class doc (~14–16): drop the "minimal unified `NSToolbar`" clauses; describe just the transparent titlebar.
- DELETE `private let toolbarDelegate: EditorToolbarDelegate` + doc (~21–22).
- Designated `init` (~76–135): update doc (~73–75); DELETE `toolbarDelegate = EditorToolbarDelegate(splitView: splitVC.splitView)` (~84); DELETE the toolbar construction block (~106–116: `NSToolbar` creation, `delegate`, `allowsUserCustomization`, `allowsDisplayModeCustomization`, `toolbarStyle = .unified`, `self.toolbar = toolbar`). KEEP the chrome measurement + `setContentSize` (~118–129) but change comments "titlebar/toolbar" → "titlebar" (also convenience-init comment ~54–56).
- DELETE the entire `private final class EditorToolbarDelegate: NSObject, NSToolbarDelegate {…}` (~163–200) incl. its leading doc.

### 3.12 `crates/wolf-macos/Sources/WolfMacOS/EditorSplitViewController.swift`
- Doc-comment-only: "titlebar/toolbar" → "titlebar" (~19–20, ~80). No code change.

---

## B. Files to modify — View > Toggle Sidebar (user-requested addition)

### B.1 `crates/wolf-macos/Sources/WolfMacOS/AppMenu.swift`
- In `viewMenuItem(target:)` (~138–170), which builds the View menu via sequential `menu.addItem(...)` in visual order: PREPEND two items at the very top, immediately after `let menu = NSMenu(title: "View")` and BEFORE "Reset Font Size":
  1. An `NSMenuItem` titled **"Toggle Sidebar"**, `action: #selector(NSSplitViewController.toggleSidebar(_:))`, `keyEquivalent: "s"` (lowercase), `keyEquivalentModifierMask = [.control, .command]`, `target = nil` (responder-chain routing). NO `.tag`, NOT routed through `commandItem(...)` — it is a standard AppKit item like the existing "Enter Full Screen" (`NSWindow.toggleFullScreen`) item.
  2. `menu.addItem(.separator())`.
  Result: index 0 = Toggle Sidebar, index 1 = separator, then the existing items. `AppKit` is already imported so `NSSplitViewController` is in scope. Follow the "Enter Full Screen" item verbatim as the pattern.
- NOTE: a manually-created item does NOT auto-toggle its title (stays "Toggle Sidebar") — intended.

### B.2 `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift` — NO CHANGE
- `validateMenuItem(_:)` guards on `item.action == #selector(handleCommandMenuItem(_:))`; Toggle Sidebar's action is `toggleSidebar(_:)`, so it falls through to `return true` (identical to Enter Full Screen / Open Recent). Its default tag `0` is never cast to `command_kind` because the action guard short-circuits first. AppKit routes validation to the split VC anyway.

### B.3 No FFI / command-dispatch wiring
- Pure AppKit first-responder action. Do NOT touch CommandSeam / CommandDispatcher / wolf-ffi.

---

## 4. Test changes

### 4.1 `crates/wolf-core/tests/state_unit.rs`
- Module doc (~2): drop `ToolbarMode`. Import (~30): remove `ToolbarMode`.
- `two_window_fixture()` (~114–137): remove the `"toolbar_display_mode": "text_only"` line (~134) and fix the preceding comma. (Fixture is reused by `set_cwd_beyond_len_grows_the_list_with_defaults` ~459.)
- `round_trip_through_store_and_load` (~147): remove `store.set_toolbar_mode(0, Some(ToolbarMode::IconOnly))` (~156) and the `w.toolbar_mode` assertion (~182–185).
- `empty_cwd_round_trips_through_store_and_load` (~205): remove ~216 and assertion ~234–237.
- `set_cwd_alone_writes_zero_geometry_and_omits_toolbar` (~253): RENAME to `set_cwd_alone_writes_zero_geometry`; remove the `toolbar_mode == None` assertion (~273) and the `!text.contains("toolbar_display_mode")` block (~275–280); update banner (~249).
- `rewriting_window_zero_preserves_window_one` (~419): remove `toolbar_mode` assertions (~439, ~449–453).
- `set_cwd_beyond_len_grows_the_list_with_defaults` (~457): remove `gap.toolbar_mode == None` (~505).
- `concurrent_setters_produce_a_clean_file_and_no_tmp_residue` (~524): remove the `let mode = …; store.set_toolbar_mode(0, mode)` block (~543–548).
- KEEP `unknown_extra_fields_still_parse` (~380) as-is.

### 4.2 NEW backward-compat test — `crates/wolf-core/tests/state_unit.rs`
- Add `legacy_toolbar_display_mode_key_is_ignored` (pure unit test, no Neovim): write a `state.json` blob that INCLUDES `"toolbar_display_mode": "icon_only"` on a window entry (plus all required fields + `version: 1`), call `load_state(dir)`, assert `Some(WolfState)` and that the window's other fields (`cwd`, `sidebar_visible`, geometry) survive verbatim. Optionally round-trip: open store, run a setter + flush, read the file back, assert the string no longer contains `toolbar_display_mode`.

### 4.3 `crates/wolf-ffi/tests/state.rs`
- DELETE the toolbar-mode discriminant consts block (~70–84: `TOOLBAR_MODE_*` + banner).
- Remove `handle.state_set_toolbar_mode(TOOLBAR_MODE_ICON_ONLY, true)` (~218).
- Remove the on-disk `"toolbar_display_mode": "icon_only"` assertion (~300–303).
- Remove the `w.toolbar_mode_set()` / `w.toolbar_mode()` restore assertions (~318–326).
- DELETE the "step 8" unrecognized-discriminant no-op block (~336–344) and "step 9" TEXT_ONLY/set=false block (~346–374); renumber remaining step comments.
- Fix this file's `two_window_fixture` copy (remove the legacy key line + fix comma; update fixture doc ~111–113).
- Update the file `//!` header if it enumerates the toolbar facet.

### 4.4 `crates/wolf-macos/Tests/WolfKeyMapTests/StateSeamTests.swift`
- Remove the `ToolbarMode` contract paragraph from the header (~11–17) and the "NOTE to coder" ToolbarMode reference (~33–39).
- DELETE the `// MARK: - ToolbarMode discriminants` section: `ModeCase` struct (~58–62) and `testToolbarModeDiscriminantTable()` (~64–83).
- KEEP `testRestoredWindowFrameTable`, `testLaunchDecisionTable`, and the rest (file stays).

### 4.5 View > Toggle Sidebar — NO new automated test
- No headless coverage of menu construction exists (AppMenu is "compile-verified + manual-smoke only"); the item adds no new pure seam / discriminant / decode logic. Verification is compile + manual smoke.

---

## 5. Binding-regeneration procedure (recommendation)

Generator: `crates/wolf-ffi/src/bin/gen_bindings.rs` via `bazelisk run //crates/wolf-ffi:gen_bindings` (needs `BUILD_WORKSPACE_DIRECTORY`, which `bazel run` sets; rewrites the committed `generated/` tree).

**RECOMMENDATION: hand-remove the six generated lines (§3.4–3.5), then run the generator once and diff to confirm the only changes are those six lines.** The removed symbols are three contiguous self-contained blocks — hand-removal is deterministic and avoids generator reordering noise. If the generator produces extra churn, prefer the hand-edited version and note the discrepancy. Generated files must END UP consistent with the edited bridge module.

---

## 6. Documentation updates (source of truth; `prettier --write` after; no mid-paragraph breaks)

Grep-verified; the coder must re-grep to confirm exact lines:
- `crates/wolf-core/src/README.md` — `ToolbarMode` schema-type bullet (~34); keep the "unknown extra fields tolerated" sentence.
- `crates/wolf-core/README.md` — ~44, 55, 65, 67–69, 102 (toolbar display-mode narrative).
- `crates/wolf-core/tests/README.md` — ~50 (state-test description).
- `crates/wolf-ffi/README.md` — ~42, 69, 73, 75, 251.
- `crates/wolf-ffi/src/README.md` — ~11, 17, 29 (`mod toolbar_mode`, `FfiWindowState` accessors).
- `crates/wolf-ffi/tests/README.md` — ~35 (toolbar-mode discriminant/roundtrip test).
- `crates/wolf-macos/Sources/README.md` — StateSeam.swift entry (~21, drop `ToolbarMode`); PaletteSeam.swift entry (~12, drop sibling ref); AppDelegate.swift entry (~31); WolfWindowController.swift entry (~32, "frame/toolbar" → "frame"); MainWindow.swift entry (~34, remove whole toolbar description; chrome math → "titlebar"); EditorSplitViewController.swift entry (~35, "titlebar/toolbar" → "titlebar"); StatePersistence.swift entry (~36, remove toolbar KVO + `state_set_toolbar_mode`; now two facets); **AppMenu.swift entry (~29): PREPEND `Toggle Sidebar ⌃⌘S` + separator to the View-menu enumeration; note it is a STANDARD AppKit item (not a Wolf command), `nil` target → responder chain → EditorSplitViewController, mask `[.control,.command]`, no `Tag`/`command_kind`, `validateMenuItem` unchanged**; migrate the "`.toggleSidebar` routes up the responder chain" sentence from the toolbar (MainWindow/EditorSplitViewController entries) to the new View menu item.
- `crates/wolf-macos/README.md` — grep directly; update window-chrome / state-restore toolbar narrative; add "Toggle Sidebar ⌃⌘S" as first View-menu item (~line 62 list) and update the View/toolbar paragraph (~102) to say the sidebar is toggled from the View menu (⌃⌘S) not a toolbar button.
- `crates/wolf-macos/Tests/README.md` — ~48 (delete the `testToolbarModeDiscriminantTable` bullet).
- `architecture.md` — ~33, 41, 71, 73 matched "toolbar"; read + remove ToolbarMode/display-mode narrative.
- `build.md` — grep; update or leave per content.
- After edits: re-grep repo for `(?i)toolbar`, `ToolbarMode`, `toolbar_mode`, `toolbar_display_mode` — confirm nothing dangles.

---

## 7. Implementation order (avoid broken intermediate builds)

1. **wolf-core** (§3.1, §3.2) + `state_unit.rs` edits (§4.1) + new backward-compat test (§4.2), same change. Run wolf-core tests.
2. **wolf-ffi bridge** (§3.3) + `tests/state.rs` edits (§4.3), same change.
3. **Regenerate/hand-edit bindings** (§5, §3.4–3.5) — lands WITH step 2.
4. **wolf-macos Swift** (§3.6–3.12, §B) + `StateSeamTests` (§4.4) — lands WITH steps 2–3 in the SAME build (Swift links the regenerated bindings; the `state_set_toolbar_mode` / `toolbar_mode*` calls are gone from both sides simultaneously). Request 1 (MainWindow) and §B (View menu) have no ffi dependency but bundle here.
5. **Docs** (§6) + `prettier --write`.

Atomicity: steps 2–4 are one non-splittable build. Steps 1 and 5 can be separate commits; step 1 precedes step 2.

---

## 8. Lint
- After removing `ToolbarMode` from `use` lists (wolf-ffi ~51, state_unit.rs, state.rs), ensure no now-unused import remains (`-D warnings`). No new `#[allow(...)]` needed. Generated-file edits are inside swift-bridge-owned scope.

---

## 9. Verify commands (coder runs; macOS = bazelisk)
- `bazelisk test //crates/wolf-core:state_unit` (incl. new backward-compat test)
- `bazelisk test //crates/wolf-core/...`
- `bazelisk run //crates/wolf-ffi:gen_bindings` then diff `crates/wolf-ffi/generated/`
- `bazelisk test //crates/wolf-ffi:state`
- `bazelisk test //crates/wolf-ffi/...` and `//crates/wolf-ffi:bridge_smoke`
- `bazelisk build //crates/wolf-macos/...` (atomic check for steps 2–4)
- `bazelisk test //crates/wolf-macos/...` (WolfKeyMapTests incl. trimmed StateSeamTests)
- `bazelisk build //...` and `bazelisk test //...` (whole-repo sanity)
- `prettier --write` all edited Markdown; then re-grep for toolbar stragglers.
- MANUAL SMOKE (needs window server, not automatable): launch app → window shows NO toolbar; editor pane keeps exact cols×rows below the titlebar; View menu's FIRST item is "Toggle Sidebar" with a separator below it; ⌃⌘S toggles the sidebar; sidebar still resizes and restores.
