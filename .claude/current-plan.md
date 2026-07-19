# Plan: Merge feat/terminal-drawer into main

## Status: COMPLETE — merged and verified

**Final verification (cold, `--nocache_test_results`): 57/57 tests pass. `//tools/lint:check` 4/4. `//crates/wolf-macos:Wolf` builds.** `WolfKeyMapTests` = 220 Swift cases, 0 failures, including the new `DrawerFocusSeamTests` (8), `DrawerLayoutTests` (5), `CommandSeamTests` (2).

### Deltas from this plan, discovered during execution

1. **§19's new test could not be written as specified.** `COMMAND_SPECS` is a `const`, not a `static`, so each use site materializes its own promoted array and a pointer-identity check fails on every row. Rewritten as value-equality plus `position()` assertions on `command` and `id`. Reviewer confirmed it genuinely fails on a duplicated or shadowed row — but noted it does NOT catch a straight two-row swap, which remains pinned only by the seven `*_is_at_index_N` guards and the end-to-end drain steps in `//crates/wolf-ffi:commands`.
2. **§14 edit 7 was under-specified and the branch's version was self-inconsistent.** The branch computed `DrawerLayout` only inside the drawer branch and passed raw `paneAreaHeight` to `tree.neighbor` — wrong once edit 3 makes `paneRects()` tile over `dl.editorHeight`. The construction was hoisted above both branches and `dl.editorHeight` passed through. Provably a no-op with no drawer open (`DrawerLayout.init` false branch is an exact passthrough).
3. **§5 understated the auto-merged `toggle_terminal`.** It arrived in a form `rustfmt` rejected and failed `//tools/lint:rustfmt_core`. Merge-introduced, so fixed.
4. **§13 understated the `viewMenuItem` doc change.** Main's doc comment was already stale pre-merge (listed neither Buffer Picker nor Preview nor Edit Source), so the whole list needed rewriting, not a clause appended.
5. **Correction (c) was a non-issue in practice.** The regenerated bindings were byte-identical to the auto-merged state — `mod command_kind` is private and non-bridged, so its constants never cross into the generated header. Now verified rather than assumed.
6. **§18's `crates/wolf-core/tests/README.md` reference was wrong.** That file has no `palette_rank` static-row count and never did; the line the plan pointed at is the `command.rs` unit-test paragraph, which was correctly updated. No work dropped.
7. **`Sources/README.md` file count was wrong on BOTH sides** (HEAD "Twenty-seven", branch "Twenty-one"); actual is 29.
8. **Two stale doc sentences contradicted the new `applyFocus`** (`Sources/README.md:56`, `README.md:89`) — caught by the reviewer as the sole blocker, fixed.
9. **`build.md`'s "47/47" was stale** → corrected to 53/53. The malformed `//crates/wolf-core:...` target pattern turned out to live in this plan file, not `build.md`.

### Known-remaining, non-blocking

- `crates/wolf-ffi/README.md:103` — the `command_kind` table is collapsed onto one line and renders as a paragraph. **Pre-existing on main**, correctly extended with Terminal; left alone to avoid merge churn.
- `crates/wolf-ffi/src/README.md:41-43` — a sentence breaks mid-clause into a spurious list item. Present identically in both parents.
- The §21 GUI-toolkit dep query can never print `clean`: `glibc` in `libc`'s source paths matches the `glib` substring. Pattern needs tightening to stay usable as a gate.
- §19's manual edge cases (split-from-drawer no-op, preview webview reframing, drawer-close focus re-homing) remain compile-and-smoke only — the same coverage posture as the rest of `PaneContainerView`.

---

## Original plan follows


## Problem statement

A merge of `feat/terminal-drawer` into `main` is **already in progress** in the working tree. Do **not** run `git merge --abort`. 23 files are conflicted (`UU` in `git status`).

Merge base is `919ab85`. Since then `main` absorbed a large typography / markdown-preview merge (~28k lines), and `feat/terminal-drawer` developed in parallel (31 commits).

Both sides appended to the **append-only** `COMMAND_SPECS` registry in `crates/wolf-core/src/command.rs`, where slice index == enum discriminant == FFI `command_kind` wire value (an ABI contract, confirmed by the architect at `command.rs:24-36` and `:134-135`, enforced by the `*_is_at_index_N` tests and the exhaustive `FfiCommand::from_command` match at `lib.rs:2336-2362`).

- `main` (HEAD) added `Preview` (18), `Edit` (19); count 20.
- `feat/terminal-drawer` added `Terminal` (18); count 19.

**Resolution: keep both. `Terminal` renumbers to 20 and becomes the LAST row. `Preview` stays 18, `Edit` stays 19. `EXPECTED_COMMAND_COUNT` = 21.**

## Corrections to the original premise (found by the architect — read these)

**(a) `crates/wolf-ffi/tests/terminal.rs` needs ZERO edits.** The original brief guessed it asserted on command discriminant 18. It does not — it drives `WolfCoreHandle::toggle_terminal()` directly (deliberately, per its header at lines 26-28). Its only discriminant assertion is `INTENT_KIND_DRAWER_OPEN: u8 = 2` (line 89), which is the **layout-intent** table, unaffected by this merge.

**(b) The merge carries a second, separate ABI addition** — `intent_kind::DRAWER_OPEN = 2` / `LayoutIntent::DrawerOpen { win }`. It **auto-merged cleanly** and is unaffected by the command renumbering. Do not disturb it, nor: the `"drawer_open"` arm in `layout.rs::from_payload` + its two unit tests; `toggle_terminal()` at `lib.rs:480-482`/`1902-1907` and `api.rs:626`; the `FfiLayoutIntent::from_intent` arm at `lib.rs:2298`.

**(c) `generated/wolf-ffi.h` and `generated/wolf-ffi.swift` auto-merged into an UNVERIFIED state.** Not conflicted ≠ correct. They are a textual union of both sides and must still be regenerated (§7).

**(d) The two sides use different Swift indentation.** `main` is **2-space** (swift-format has been run); the branch is **4-space**. Every conflicted Swift file shows this. The merged result must be 2-space throughout, including the four cleanly-added drawer files, or `//tools/lint:swift_format` churns.

---

## 3. `crates/wolf-core/src/command.rs` — the anchor file

Resolve first; everything derives from it.

### Final ordered `COMMAND_SPECS` (21 rows)

| idx | variant | id | ex_command | title | menu |
|----|---------|----|-----------|-------|------|
| 0 | `NewWindow` | `new_window` | `WolfNewWindow` | New Window | File |
| 1 | `CloseWindow` | `close_window` | `WolfCloseWindow` | Close Window | File |
| 2 | `QuitApplication` | `quit_application` | `WolfQuitApplication` | Quit Wolf | App |
| 3 | `HideApplication` | `hide_application` | `WolfHideApplication` | Hide Wolf | App |
| 4 | `MinimizeWindow` | `minimize_window` | `WolfMinimizeWindow` | Minimize | Window |
| 5 | `GoToFile` | `go_to_file` | `WolfGoToFile` | Go to File… | File |
| 6 | `OpenFile` | `open_file` | `WolfOpenFile` | Open File… | File |
| 7 | `OpenFolder` | `open_folder` | `WolfOpenFolder` | Open Folder… | File |
| 8 | `ClosePane` | `close_pane` | `WolfClosePane` | Close | File |
| 9 | `NewBuffer` | `new_buffer` | `WolfNewBuffer` | New | File |
| 10 | `ResetFontSize` | `reset_font_size` | `WolfResetFontSize` | Reset Font Size | View |
| 11 | `IncreaseFontSize` | `increase_font_size` | `WolfIncreaseFontSize` | Increase Font Size | View |
| 12 | `DecreaseFontSize` | `decrease_font_size` | `WolfDecreaseFontSize` | Decrease Font Size | View |
| 13 | `CommandPalette` | `command_palette` | `WolfCommandPalette` | Command Palette… | View |
| 14 | `Copy` | `copy` | `WolfCopy` | Copy | Edit |
| 15 | `Paste` | `paste` | `WolfPaste` | Paste | Edit |
| 16 | `BufferPicker` | `buffer_picker` | `WolfBufferPicker` | Buffer Picker… | View |
| 17 | `NewTab` | `new_tab` | `WolfNewTab` | New Tab | File |
| 18 | `Preview` | `preview` | `WolfPreview` | Preview | View |
| 19 | `Edit` | `edit` | `WolfEdit` | Edit Source | View |
| **20** | **`Terminal`** | **`terminal`** | **`WolfTerminal`** | **Terminal** | **View** |

### Edits

- **Enum doc (conflict ~30-35).** Take HEAD's text, extend the trailing list to `` `BufferPicker`(16), `NewTab`(17), `Preview`(18), `Edit`(19), `Terminal`(20). Adding a command is additive `` + HEAD's existing continuation.
- **Enum body (conflict ~79-89).** Keep HEAD's `Preview` then `Edit`, then append the branch's `Terminal` variant with its doc comment.
- **`COMMAND_SPECS` (conflict ~267-286).** Keep HEAD's `Preview` and `Edit` rows verbatim, then append a full `CommandSpec` row for `Terminal`. **The conflict is badly aligned** — the branch's `menu: MenuGroup::View,\n},` tail is *shared* text, so construct the two closing rows carefully rather than mechanically deleting one side.
- **`EXPECTED_COMMAND_COUNT` (~316-327)** → `21`, doc updated to name Preview (18), Edit (19), Terminal (20).
- **Index consts (~349-370).** `NEW_TAB_INDEX = 17` (keep HEAD's doc). `PREVIEW_INDEX = 18` unchanged. `EDIT_INDEX = 19` — change doc from "LAST row" to "no longer the last row after `Terminal` is appended at 20". Add `TERMINAL_INDEX: usize = 20` with the "appended discriminant 20 and LAST row" doc.
- **`command_specs_has_expected_len` (~393-412).** Doc + message string → "…Copy, Paste, BufferPicker, NewTab, Preview, Edit, and Terminal … exactly 21 rows".
- **`new_tab_is_at_index_17` doc (~545-551).** Take HEAD's wording; still accurate.
- **Final test block (~561-664).** Keep HEAD's `preview_id_round_trips`, `preview_spec_fields`, `preview_is_at_index_18`, `edit_id_round_trips`, `edit_spec_fields` verbatim. Then:
  - **Rename `edit_is_the_last_row` → `edit_is_at_index_19`.** Drop its `EDIT_INDEX == COMMAND_SPECS.len() - 1` assertion entirely — Edit is no longer final. Keep only the row-identity assertion. Update the doc to the "stays at fixed index 19" pattern.
  - Append the branch's `terminal_id_round_trips`, `terminal_spec_fields`, `terminal_is_the_last_row` — with `TERMINAL_INDEX` = 20. The "is FINAL" assertion moves here and is the only one of its kind.

---

## 4. `crates/wolf-core/tests/palette_rank.rs`

**`STATIC_COMMAND_COUNT = 20`.** Derivation verified against code, not assumed: `rank_palette` (`crates/wolf-core/src/palette.rs:89-103`) iterates `for spec in COMMAND_SPECS` with exactly one `continue`, guarded by `if spec.command == Command::CommandPalette`. No other filter, no `MenuGroup` gating, no "is markdown active" check, no dedup. Sections (2)/(3) append only window/recent-dir rows, empty in the `rank_palette(&[], &[], &[], "", …)` calls these assertions use. So static rows == `COMMAND_SPECS.len() - 1` == 21 - 1 == **20** — correct *because* exactly one spec self-excludes, not by arithmetic coincidence.

Two count-only consumers, no other edits: `command_palette_is_excluded_and_static_rows_present` (~line 100), `empty_inputs_yield_only_static_rows` (~line 238).

Resolve the conflict (~35-47) with one merged doc comment: 21 registered commands (appended at 14-20) yield 20 static rows; Preview, Edit and Terminal are all NEW palette static rows (OD4 — the palette does not gate them on active buffer/pane, unlike the menu items).

`query_filters_to_and_subsequence_matches` still holds (`rows.len() == 3` for `"font size"`; none of "Preview"/"Edit Source"/"Terminal" contains both tokens).

---

## 5. `crates/wolf-core/src/input.rs`

Pure adjacent-append; keep both, HEAD first.

- **Conflict ~245-280.** Keep HEAD's `SetBackground { dark: bool }` variant *and* HEAD's `impl InputEvent { … is_toast_dismissing … }` block. Then insert the branch's `ToggleTerminal` variant **inside the enum, after `SetBackground`, before the enum's closing brace**. ⚠️ The conflict markers are misleadingly placed: the branch side puts `ToggleTerminal` *after* HEAD's `impl` block, which would be a syntax error. Be deliberate about brace placement.
- **Conflict ~420-431 (`apply_input` match).** Keep both arms: HEAD's `SetBackground` block, then `InputEvent::ToggleTerminal => toggle_terminal(nvim).await,`.
- The free `async fn toggle_terminal(nvim: &Nvim)` (~531-546) auto-merged; leave it.

---

## 6. `crates/wolf-ffi/src/lib.rs`

- **`mod command_kind` (~197-205).** Keep `PREVIEW = 18`, `EDIT = 19`, append `TERMINAL: u8 = 20` with doc.
- **Bridge doc (~783-787)** and **`FfiCommand::kind` doc (~2323-2327)** → range ends at `` `TERMINAL` ``.
- **`FfiCommand::from_command` match (~2356-2361).** Keep Preview/Edit arms, append `Command::Terminal => command_kind::TERMINAL,`. Exhaustive, no `_` fallback → omitting it is a compile error (useful early signal).

Do **not** touch `mod intent_kind`, the `toggle_terminal` bridge decl/impl, or the `LayoutIntent::DrawerOpen` arm.

---

## 7. Regenerating the FFI bindings

After `lib.rs` resolves and `//crates/wolf-ffi:wolf_ffi` builds — **do not hand-edit** `generated/wolf-ffi.h` / `wolf-ffi.swift`:

```sh
bazel run //crates/wolf-ffi:gen_bindings
```

Documented at `crates/wolf-ffi/README.md:211` and `build.md:157` ("Regenerating the swift-bridge bindings"). It is a `rust_binary` (`crates/wolf-ffi/BUILD.bazel:81`) over `src/bin/gen_bindings.rs`, writing in-tree via `BUILD_WORKSPACE_DIRECTORY` — only `bazel run` provides that. Then `git add crates/wolf-ffi/generated/`.

`SwiftBridgeCore.swift`/`.h`, `wolf_ffi_wake.h`, `module.modulemap` are hand-written/runtime-shared and not rewritten. `wolf_ffi_wake.h` should stay byte-identical.

---

## 8. `crates/wolf-ffi/tests/commands.rs`

- **Header doc (~26-38)** → `… NEW_TAB = 17, PREVIEW = 18, EDIT = 19, TERMINAL = 20 … guard array asserts contiguous 0..=20.`
- **Local consts (~114-123).** Keep `CMD_KIND_PREVIEW = 18`, `CMD_KIND_EDIT = 19`; append `CMD_KIND_TERMINAL: u8 = 20`.
- **Guard comment (~190-196)** → "contiguous 0..=20 … Referencing all twenty-one here".
- **Guard array (~218-229).** Add `CMD_KIND_TERMINAL,`; expected literal → `[0, …, 20]`; message → "contiguous 0..=20". (`CMD_KIND_TERMINAL` must appear here or it trips `dead_code` — that is the array's stated purpose, see its comment at ~line 197.)
- **Drain steps (~329-355).** Keep HEAD's `3i` (`:WolfPreview`) and `3j` (`:WolfEdit`) verbatim, append the branch's block **renumbered `3k`** with its comment corrected from "discriminant 18" to **"discriminant 20"**.

---

## 9-13. Swift front-end

**`CommandSeam.swift`** — take HEAD's whole body (2-space), discard the branch's 4-space block. Add `case terminal` after `case edit`; add `case 20: self = .terminal` in `init?(ffi:)`; update the header doc to `` `preview == 18`, `edit == 19`, `terminal == 20` `` and the `init?(ffi:)` doc range to `0` → `.newWindow` .. `20` → `.terminal`.

**`CommandSeamTests.swift`** — HEAD's 2-space bodies for both conflicts.
- `testCommandKindFfiDiscriminants`: add `XCTAssertEqual(CommandKind(ffi: 20), .terminal, "ffi 20 -> .terminal")`.
- `testCommandKindOutOfRangeIsNil`: change `CommandKind(ffi: 20)` → **`CommandKind(ffi: 21)`**. Main currently asserts 20 is out of range, which becomes false. Easy to miss; fails loudly if missed.
- Header comment → `… preview = 18, edit = 19, terminal = 20 (the range 0..20).`

**`CommandDispatcher.swift`** — take HEAD's `dispatch` body in full; add after `case .edit: controller?.editSource()`:
```
case .terminal:
  controller?.toggleTerminal()
```
Exhaustive switch, no `default:` → missing case is a compile error.

**`WolfWindowController.swift`** — single conflict (~611-1024). **Take HEAD wholesale** (611-797); the branch side is the pre-rewrite body and contributes exactly one thing. Insert after main's `newBuffer()` (ends ~550), before the `// MARK: - Edit commands` divider:
```
/// Toggles this window's per-project terminal drawer (`:WolfTerminal`).
func toggleTerminal() {
  handle?.toggle_terminal()
}
```
`handle` is the same optional `WolfCoreHandle?` that `closePane()`/`newBuffer()` use (~543-550) — no new plumbing. `openPreview()` (~688) and `editSource()` (~710) are main's and stay.

**`AppMenu.swift`** — take HEAD for both conflicts (HEAD already has the `modifiers:` parameter on `commandItem`, which the Terminal item needs — no signature work).
- **Conflict ~18-289 (`Tag` enum).** Keep HEAD's tags through `static let edit = 19`, add `static let terminal = 20`. **Must be 20, not the branch's 18** — tags carry the FFI discriminant into `handleCommandMenuItem`.
- **Conflict ~335-597.** Keep HEAD's `commandItem` and everything after. In `viewMenuItem` (~170), insert **after** the "Edit Source" item (~212-214) and **before** `menu.addItem(.separator())` (~215):
```
menu.addItem(
  commandItem(
    title: "Terminal", tag: Tag.terminal, keyEquivalent: "`",
    target: target, modifiers: .control))
```
- Update `viewMenuItem`'s doc (~159-162) to the full order, including **Terminal ⌃`**.
- Terminal is **not** gated in `AppDelegate.validateMenuItem` (unlike Preview/Edit Source) — always enabled, matching the branch.

---

## 14. `PaneContainerView.swift` — the hard one

Four conflicts. Two are **spurious mega-hunks**: at ~519-1301 HEAD's side is 38 lines of current code while the branch's is 744 lines of the *deleted* pre-rewrite body (grid-drawn `paintCmdlineStrip`/`paintStripRow`, removed by main under OD9 when the cmdline became an overlay `NSView`); likewise ~2384-2412 is HEAD's `drawScrollbars` vs the branch's old `setActivePane`.

**Strategy: resolve all four conflicts by taking HEAD wholesale, confirm `bazel build //crates/wolf-macos:wolf_macos_lib` succeeds, THEN apply the nine edits below and rebuild.** Do not do both in one pass — the point is separating "resolve the conflict" from "re-apply the feature." Do not merge line-by-line inside the big hunks.

All edits 2-space. Line numbers are main's current (HEAD-side) positions.

1. **Constant** — after `intentKindFocusDir` (~71) add `private static let intentKindDrawerOpen: UInt8 = 2` with doc. **Do NOT bring over the branch's `inactiveDim: CGFloat = 0.28`** — main deleted whole-pane dimming, carrying active/inactive in the statusbar (`statusbarInactiveAlpha`, ~136). It would be dead code.
2. **State** — after `activePane` (~183) add `drawerPaneId: PaneId?` and `lastEditorPane: PaneId?` with the branch's doc comments.
3. **`paneRects()`** (~398-403) — rewrite per the branch. Construct `let dl = DrawerLayout(paneAreaHeight: Double(paneAreaHeight), hasDrawer: drawerPaneId != nil, weight: DrawerLayout.defaultWeight)`, tile the tree over `dl.editorHeight` into `var rects`, append the drawer rect from `dl.drawerRect(width: Double(bounds.width))` when `drawerPaneId` is set. Extend the doc with the branch's "When the terminal drawer is open…" paragraph. ⚠️ **Main's `paneAreaHeight` is `bounds.height`** (full view height under OD9, ~385), not the branch's `bounds.height - stripHeight`. Do not carry the branch's expression over.
4. **`applyIntents()`** (~1945-1958) — add before `default:`: `case Self.intentKindDrawerOpen: applyDrawerOpen(win: intent.win())`.
5. **`applySplit`** (~1963-1971) — after the `guard let target = …` line add `guard target != drawerPaneId else { return }` with the FD2 comment (a split fired from within the drawer is dropped front-end-side).
6. **New `applyDrawerOpen(win:)`** — insert between `applySplit` and `applyFocus`, from the branch (~989-1000), reindented. Allocs a pane id, sets `drawerPaneId`, binds `PaneBinding(win:boundGrid:)`, calls `setActivePane`. `allocPaneId()` (~2624) and `PaneBinding` are unchanged — drops in as-is.
7. **`applyFocus`** (~1975-1983) — replace with the branch's drawer-aware version (~1013-1039), reindented, keeping main's `paneAreaHeight`. If `origin == drawerPaneId`, handle only `.up` via `DrawerFocusSeam.drawerUpTarget(lastEditorPane:leaves:rects:)` over `tree.paneRects(width:height: dl.editorHeight)` + `focus_win`, then return; otherwise resolve `tree.neighbor(origin, dir, width:, height:)` and, on a nil neighbour with `dir == .down` and an open drawer, focus the drawer. ⚠️ **Geometry consistency matters:** the neighbour rects must be computed over `dl.editorHeight`, the same basis `paneRects()` now uses, or focus moves disagree with what is painted.
8. **`detectCloses()`** (~1987-2004) — insert the branch's drawer-removal block (~1051-1059) between the `if closed.isEmpty { return }` guard and the `for id in closed` loop; change the loop header to `for id in closed where id != closedDrawer`. The drawer is removed from `panes` and `drawerPaneId` cleared **without** `tree.remove` and **without** the last-leaf guard — it was never a tree leaf. Main's trailing `if let active = activePane, panes[active] == nil { setActivePane(tree.leaves().first) }` stays and correctly re-homes focus.
9. **`setActivePane`** (~2636-2641) — prepend `if let pane, pane != drawerPaneId, tree.leaves().contains(pane) { lastEditorPane = pane }`. Keep main's `if activePane != pane { … }` body and main's doc phrasing ("the one with the bold statusbar filename" — **not** the branch's "the one NOT dimmed", which describes deleted behavior), extended with the branch's `lastEditorPane` paragraph.

**Interaction notes:** main's `syncGridSizes()` (~1471) and `syncPreviewWebViews()` (~1668) both call `paneRects()`, so after (3) the drawer participates in grid sizing and preview-webview framing automatically — no edits there. `drawScrollbars`/`drawDividers` iterate the same rects, so the drawer gets a scrollbar and separators for free; cosmetic follow-up, not a merge blocker. The drawer is never in `previewNamesByGrid`, so it paints as an ordinary grid pane with a statusbar — acceptable.

---

## 15. `crates/wolf-macos/BUILD.bazel`

Two conflicts, both **unions in alphabetical order**:
- `wolf_key_map.srcs` (~54-59): `CommandSeam.swift`, `DrawerFocusSeam.swift`, `DrawerLayout.swift`, `EditSeam.swift`, `EditorInset.swift`.
- `WolfKeyMapTests.srcs` (~242-247): `CommandSeamTests.swift`, `DrawerFocusSeamTests.swift`, `DrawerLayoutTests.swift`, `EditSeamTests.swift`, `EditorInsetTests.swift`.

`wolf_macos_lib` and `swift_format_srcs` use `glob()` — no change.

---

## 16. New seam files — verified compile-clean against main

Architect checked both against main's current `PaneTree.swift`:
- `DrawerLayout.swift` uses only `Rect(x:y:w:h:)` — present at `PaneTree.swift:97`, public `let x/y/w/h: Double`. ✅
- `DrawerFocusSeam.swift` uses `PaneId` (`public typealias PaneId = UInt64`, `PaneTree.swift:34`, so the `id < current.id` comparison compiles) and `Rect`. Callers pass `tree.leaves()` (`:184` → `[PaneId]`) and `tree.paneRects(width:height:)` (`:193` → `[(PaneId, Rect)]`) — signatures unchanged. ✅

**Only adaptation: reindent both + their two test files from 4-space to 2-space.** Not conflicted, so this is a deliberate follow-up edit.

---

## 17. Lua: `runtime/nvim/init.lua` — NO EDITS

Not conflicted; auto-merged. Architect verified the branch's additions survived and their dependencies still exist on main:
- `emit_drawer(win)` (~389-394) is defined inside `wolf_setup_layout`, closing over its local `chan`, exported as `_G.wolf_emit_drawer` (~402) beside main's intact `_G.wolf_do_split`/`wolf_do_close`/`wolf_do_focus` (~399-401) — so the enclosing function and its `chan` binding survived main's rewrite.
- `_G.wolf_toggle_terminal()` (~547-591) is module-level, depends only on stock nvim API, and guards its emit with `if _G.wolf_emit_drawer then` (~587).
- The `t`-mode `<C-w>` maps (~429-432) and the `WolfTerminalInsert` augroup (~439+) are self-contained.

**Verify at test time, do not edit:** main's `wolf_buffers`/`wolf_current_file` trackers skip `b:wolf_preview_source` buffers. Terminal buffers carry `b:wolf_terminal`, not that flag, so `term://…` names flow through the path-based trackers. That is the branch's intent (OD7 relies on terminal buffers being `buflisted` and appearing in the picker). **If `//crates/wolf-ffi:terminal` fails on a `display()` assertion, look here first.**

---

## 18. Documentation (docs are the source of truth — must describe all three commands)

**`architecture.md`**
- **Conflict ~160-166.** Take HEAD's registry bullet; "now twenty commands (`command_kind` discriminants `0..19`)" → **"twenty-one commands (`0..20`)"**; after HEAD's Edit Source (`19`) clause append the Terminal clause renumbered to `20` (id `terminal`, `:WolfTerminal`, title "Terminal", `` ⌃` ``) describing the per-tabpage bottom `:terminal` drawer over a single shared per-project buffer (`wolf_toggle_terminal` → `WolfCoreHandle::toggle_terminal` → `InputEvent::ToggleTerminal`; buffer/job persist across toggles VS-Code-style, recreated if the shell exited). **Keep the branch's trailing** `See [Command lifecycle](command-lifecycle.md) …` line — that file is new from the branch and `README.md:18` already links it.
- **Conflict ~214-232.** Merged order: (1) the branch's "The terminal drawer rides this same channel" paragraph (~225) verbatim — documents `DrawerOpen`, the outside-the-`PaneTree` decision, the `win_close` hide asymmetry, the `<C-w>j`/`<C-w>k` bridge; nothing in main contradicts it and `architecture.md:212` already names `DrawerOpen { win }`. (2) HEAD's "macOS consumes this same channel over the FFI" paragraph (~215), splicing in the branch's "**a DRAWER_OPEN reuses `win`**" after "a FOCUS_DIR carries `dir` + `from`"; keep HEAD's cmdline/pum/toast-accessor sentence and **discard the branch's stale "the cmdline strip reads the message/command-line grid" sentence** (that mechanism is gone). (3) HEAD's `### Output: commands` heading, static list extended to `… / Preview / Edit Source / Terminal (0..20)`, plus the Terminal (`20`) clause noting it is an editor-affecting command unlike the front-end-only overlays. (4) HEAD's `### Output: preview` section (~221-223) verbatim.
- **Outside any conflict:** the `### wolf-ffi` paragraph (~43) hardcodes the table ending `… NEW_TAB=17 / PREVIEW=18 / EDIT=19`. Append `/ TERMINAL=20`. Grep for `EDIT=19`.

**`crates/wolf-core/src/README.md`** (~97-103, ~132-137, ~346-350) — 21 commands with Terminal at 20; `InputEvent` list carries both `SetBackground` and `ToggleTerminal`; palette section states 20 static rows.

**`crates/wolf-core/tests/README.md`** (~101-105) — `palette_rank` static-row count → 20, as `COMMAND_SPECS.len() - 1` over 21 commands.

**`crates/wolf-ffi/README.md`** (~233-237, ~276-280) — `command_kind` table gains `TERMINAL=20`; keep both the preview-surface and `toggle_terminal` descriptions.

**`crates/wolf-ffi/src/README.md`** (~11-15, ~41-45) — discriminant summary gains `TERMINAL=20`; `intent_kind` summary gains `DRAWER_OPEN=2` (branch side, keep); handle-method list keeps `render_preview`/`edit_source`/`is_preview_active` **and** `toggle_terminal`.

**`crates/wolf-ffi/tests/README.md`** (~49-79) — keep HEAD's `preview`/`edit` entries **and** add the branch's `terminal.rs`. Cross-check against the actual `rust_test` targets in `crates/wolf-ffi/BUILD.bazel`: `commands` (~241), `terminal` (~385), `preview` (~879) all exist and all must be documented.

**`crates/wolf-macos/README.md`** (~45-55, ~81-93), **`Sources/README.md`** (4 conflicts), **`Tests/README.md`** (~3-7, ~73-93) — union, HEAD first. `Sources/README.md` documents the two new `WolfKeyMap` seams beside main's `EditSeam`/`PreviewSeam`/`PreviewLinkSeam`, and `PaneContainerView`'s drawer handling beside its preview handling. `Tests/README.md` lists `DrawerFocusSeamTests`/`DrawerLayoutTests`. Keep seam lists **alphabetized** to match the BUILD file.

**`runtime/nvim/README.md`** (~146-194) — largest doc conflict (HEAD 33 lines vs branch 15). Keep HEAD's block (preview scratch-buffer helpers, `wolf_preview`, `wolf_markdown_dirty`) and append the branch's documenting `wolf_toggle_terminal`, `wolf_emit_drawer`, the `t`-mode `<C-w>` maps and their shell word-erase tradeoff, and the `WolfTerminalInsert` re-entry autocmd.

**After every Markdown edit:** `prettier --write` on the touched file; no hard line breaks inside a paragraph (per `CLAUDE.md`).

---

## 19. Test plan

### Must change
| File | Change |
|---|---|
| `crates/wolf-core/src/command.rs` (`mod tests`) | count 21; `TERMINAL_INDEX` 20; `edit_is_the_last_row` → `edit_is_at_index_19`, finality assertion removed |
| `crates/wolf-core/tests/palette_rank.rs` | `STATIC_COMMAND_COUNT` 20 |
| `crates/wolf-ffi/tests/commands.rs` | `CMD_KIND_TERMINAL` 20; guard array `0..=20`; new `3k` `:WolfTerminal` drain step |
| `crates/wolf-macos/Tests/WolfKeyMapTests/CommandSeamTests.swift` | `ffi: 20 -> .terminal`; out-of-range probe 20 → **21** |

### Must NOT change
- `crates/wolf-ffi/tests/terminal.rs` — asserts `DRAWER_OPEN = 2` only (see correction (a)).
- `DrawerFocusSeamTests.swift`, `DrawerLayoutTests.swift` — pure geometry; **reindent only**.

### New test warranted (one, cheap and high-value)
In `command.rs`'s `mod tests`, add **`command_specs_index_matches_variant_order`**: iterate `COMMAND_SPECS.iter().enumerate()` and assert `COMMAND_SPECS[i].command.spec()` resolves to the same row — i.e. `spec()`'s linear scan and the slice index agree for **every** row, not just the seven hand-picked ones. This merge is exactly the scenario the per-command guards were designed to catch, and it is the third command append; a whole-table invariant is cheaper than a 22nd hand-written guard next time.

Otherwise coverage is adequate: `//crates/wolf-ffi:commands` proves every discriminant round-trips through a real nvim, `:terminal` proves the drawer lifecycle, `:preview` the preview surface, `palette_rank` all 20 static rows.

### Manual edge cases (AppKit glue is compile+smoke only)
- Open the drawer, then `:vsplit` from inside it → must be a no-op (edit 5).
- Preview pane + drawer together → the preview `WKWebView` frame must shrink to the reduced `dl.editorHeight` on the next wake.
- `<C-w>k` out of the drawer with `lastEditorPane` pointing at a since-closed pane → must fall back to `bottomMostLeaf`.
- Close the drawer while it is active → `detectCloses` re-homes focus via `tree.leaves().first`.

---

## 20. Implementation order (each step yields a compile signal)

1. `crates/wolf-core/src/command.rs` — the anchor.
2. `crates/wolf-core/src/input.rs` — watch the enum/`impl` brace placement.
3. `bazel test //crates/wolf-core/...`
4. `crates/wolf-core/tests/palette_rank.rs` → 20.
5. `crates/wolf-ffi/src/lib.rs` — exhaustive `from_command` match is the compile gate.
6. `bazel run //crates/wolf-ffi:gen_bindings`, then `git add crates/wolf-ffi/generated/`.
7. `crates/wolf-ffi/tests/commands.rs`.
8. `bazel test //crates/wolf-ffi:commands //crates/wolf-ffi:terminal //crates/wolf-ffi:preview` — Rust fully green before Swift.
9. `crates/wolf-macos/BUILD.bazel` — **before** the Swift files, so the new seams are in the module when the compiler runs.
10. `CommandSeam.swift` → `CommandSeamTests.swift`.
11. `bazel test //crates/wolf-macos:WolfKeyMapTests`
12. `CommandDispatcher.swift` → `WolfWindowController.swift` (supplies `toggleTerminal()`) → `AppMenu.swift`.
13. `PaneContainerView.swift` — last and largest; HEAD-wholesale + build, **then** the nine edits + rebuild.
14. `bazel build //crates/wolf-macos:Wolf`
15. Reindent the four drawer files to 2-space.
16. Docs + `prettier --write`.
17. Full verification (§21).
18. `git add` everything; `git commit` **only after** §21 passes.

---

## 21. Build / test commands (from `build.md`)

```sh
bazel test //crates/wolf-core/...                 # unit + real-nvim integration
bazel test //crates/wolf-ffi/...                  # command channel, drawer, preview
bazel test //crates/wolf-macos:WolfKeyMapTests    # pure Swift seams
bazel build //crates/wolf-macos:Wolf
bazel test  //crates/wolf-macos:bundle_layout_test
bazel test //crates/...                           # full default sweep
bazel test //tools/lint:check                     # clippy + rustfmt + swift-format (manual-tagged)
bazel query 'deps(//crates/wolf-core:wolf_core)' | grep -iE \
  'gtk|gdk|glib|cairo|pango|adwaita|objc|cocoa|core-graphics|winit|wgpu|skia|softbuffer' \
  || echo clean                                   # no GUI toolkit in core
```

Notes: lint targets are **advisory** (`build.md:348`) — a pass only means the linter ran; pre-existing findings are not merge blockers, but anything **this merge introduces** must be fixed. Tests run in the **default** configuration, never `-c opt` (`@testable import` needs `-enable-testing`, omitted by rules_swift in opt). `//crates/wolf-macos:WolfRelease` is `manual` and irrelevant. No `CARGO_BAZEL_REPIN` needed (`build.md:87`) — no new Cargo deps.

## 22. Lint considerations

No new `#[allow(...)]` or `// swift-format-ignore` should be needed; following the existing named-constant pattern for `TERMINAL_INDEX = 20` keeps the magic-number lint quiet. **swift-format is the real risk** — every Swift file touched (five conflicted + four cleanly-added drawer files) must end up 2-space. `bazel run //tools/lint:swift_format.fix` is available; note check and fix enforce different things (`build.md:364`).

## Out of scope

Untracked scratch files (`test-1-no-preview` … `test-7-with-opt-param`, `preview-*-decisions.md`) are unrelated. Leave them.
