# Command Palette — Implementation Plan

## Status: COMPLETE

Command Palette implemented, reviewed (APPROVED, no BLOCKER/SHOULD-FIX), documented, and green.
Clippy nit (test-file doc comment) fixed by test-writer and verified cleared. Tests: wolf-core
`command_mru`/`palette_rank`/`wolf_core_lib_test`/`commands` PASS; wolf-ffi all 21 targets (incl.
`command_palette`, `commands`) PASS; wolf-macos `WolfKeyMapTests` PASS; `bazel build //crates/...`
+ `//crates/wolf-macos:Wolf` clean; `cargo clippy -p wolf-core --all-targets` warning-free.
Docs synced across 8 Markdown files (all prettier-clean, proseWrap:never) incl. architecture.md +
both wolf-macos READMEs + Tests README; tech-writer also fixed 5 stale pre-existing doc statements
(the "command picker is deferred" claim, command_kind 0..12 tables, thirteen-vs-fourteen commands).
Regenerated committed swift-bridge glue (FfiCommandPalette/FfiCommandRow). Not committed (per workflow).

## Status: REVIEW APPROVED — NIT + DOCS PENDING (superseded above)

### Reviewer verdict: APPROVED
All plan points conform incl. all 3 locked decisions + the discriminant-13 contract lockstep across
command.rs / wolf-ffi command_kind+from_command / CommandSeam / AppMenu.Tag / CommandDispatcher.
palette.rs assembly+ranking faithful (CommandPalette excluded, Untitled label, MRU-first, take(max),
index-map-back); command_mru.rs a faithful recent_dirs clone, always-on but NOT redraw-wired (only
palette activation records); FFI `ranked` drops the RefCell borrow before rank_palette (no aliasing
panic), empty-backed fallback, FfiCommandRow dynamic-row fields (command_kind 0 / path "") correct,
glue regenerated; Swift menu (standard toggleFullScreen ⌃⌘F, validateMenuItem untouched), controller
palette parallel to picker with [weak self] + teardown drop, three activation actions correct, record()
before onActivate/close while session alive, PaletteSeam dormant/unwired (keyDown only calls picker).
Tests encode intended behavior. ONE NIT only.
- NIT (route to test-writer): `crates/wolf-core/tests/palette_rank.rs:179` clippy::doc_lazy_continuation
  — insert a blank `///` line before line 179. Cosmetic; bazel gate already green.

## Status: IMPLEMENTED + GREEN — REVIEW PENDING (superseded above)

### Coder report (round 1)
Implemented all of wolf-core (command.rs CommandPalette variant+row; command_mru.rs; palette.rs;
api.rs field+accessor; lib.rs re-exports), wolf-ffi (command_kind::COMMAND_PALETTE, palette_row_kind,
from_command arm, command_palette() + FfiCommandPalette/FfiCommandRow; regenerated committed glue),
Swift WolfKeyMap (CommandSeam .commandPalette; PaletteSeam NEW + BUILD src add), Swift WolfMacOS
(AppMenu View-menu incl. standard toggleFullScreen: ⌃⌘F; CommandDispatcher; AppDelegate helpers;
CommandPaletteView NEW; WolfWindowController palette+openCommandPalette+activatePaletteRow+teardown).
Also updated 3 READMEs (wolf-core/src, wolf-ffi/src, wolf-macos/Sources) + prettier; did NOT touch
architecture.md (left for tech-writer pass).
Tests: `//crates/wolf-core:{command_mru,palette_rank,wolf_core_lib_test,commands}` PASS;
`//crates/wolf-ffi/...` all 21 (incl. command_palette, commands) PASS; `//crates/wolf-macos:WolfKeyMapTests`
PASS; `bazel build //crates/...` + `//crates/wolf-macos:Wolf` clean; wolf-core/wolf-ffi `--lib` clippy clean.
DEVIATION: `CommandPaletteRow` is module-INTERNAL (not file-private) so `WolfWindowController.activatePaletteRow`
can name it in the onActivate closure — no behavioral effect, documented.
OPEN cosmetic item: `cargo clippy -p wolf-core --all-targets` warns `doc_lazy_continuation` at
`tests/palette_rank.rs:179` (TEST file; coder can't edit). Fix = insert a blank `///` line before that
line. Route to test-writer (batch with any reviewer test findings). Bazel path is clean regardless.

## Status: TESTS WRITTEN — IMPLEMENTATION PENDING (superseded above)

### Test-writer report + API pins the coder MUST honor
Tests created: `crates/wolf-core/tests/command_mru.rs` (headless store), `crates/wolf-core/tests/palette_rank.rs`
(pure `rank_palette` — placed in a SEPARATE integration file, NOT in-module, so it can't collide with
the coder Write-ing `src/palette.rs`), `crates/wolf-ffi/tests/command_palette.rs` (single-#[test]
real-nvim), `crates/wolf-macos/Tests/WolfKeyMapTests/PaletteSeamTests.swift`.
Tests modified: `crates/wolf-core/src/command.rs` (in-module `#[cfg(test)] mod tests` — command.rs
already exists so no collision), `crates/wolf-ffi/tests/commands.rs` (guard 0..=13 + `:WolfCommandPalette`
kind==13), `crates/wolf-macos/Tests/WolfKeyMapTests/CommandSeamTests.swift` (ffi 13 → .commandPalette;
out-of-range → 14/255).
BUILD added: wolf-core `command_mru` + `palette_rank` headless `rust_test` targets; wolf-ffi
`command_palette` real-nvim `rust_test` target; wolf-macos `WolfKeyMapTests` srcs += PaletteSeamTests.swift.
NOTE: test-writer did NOT add `Sources/WolfKeyMap/PaletteSeam.swift` to `wolf_key_map` srcs — that
source registration is the CODER's (plan §6/§7).

**API the tests assume — coder implement to these EXACTLY:**
- `lib.rs` MUST provide TOP-LEVEL re-exports `wolf_core::{CommandMru, COMMAND_MRU_LIMIT, rank_palette,
  PaletteRow, PaletteRowKind, PaletteWindow}` (tests import from crate root, not `wolf_core::palette::…`).
- Derives/visibility: `PaletteRowKind: Debug + PartialEq + Eq`; `PaletteRow: Debug`; `PaletteRow` and
  `PaletteWindow` have PUBLIC fields (struct-literal construction + field reads in tests).
- Open-recent label/key: plain path rendering (`/home/proj` → display `"Open Recent Folder: /home/proj"`,
  mru_key `"open_recent:/home/proj"`). Pin `to_string_lossy()`/`display()` in the impl.
- `FfiCommandRow` on dynamic rows: `path()` returns `""` for non-open-recent rows (None→""); tests never
  read `command_kind()` on non-static rows but keep it 0 there.
- Command MRU starts EMPTY under a fresh `WOLF_STATE_DIR` — ONLY explicit palette `record()` writes to
  `command-mru.sqlite` (nothing auto-records, unlike recent_dirs). The MRU-first round-trip test depends on this.
- XCTest SourceKit "No such module" diagnostics are EXPECTED (Bazel-only modules); ignore.

## Status: ARCHITECTURE COMPLETE — TESTS PENDING (superseded above)

## Problem statement

Introduce a **Command Palette** UI to the macOS front-end (`wolf-macos`), mirroring
the existing file picker overlay.

### Locked decisions (confirmed with user)

1. **All 13 static registry commands** (every current `COMMAND_SPECS` entry) appear in the palette.
2. **Open Recent Folder**: if that folder is already open in another window, RAISE that
   window; otherwise chdir the current window (same effect as File > Open Recent).
3. **Switch to Window**: EXCLUDE the current window; label empty-state windows "Untitled".

### Spec

- **Trigger**: ⌘P, plus a View-menu item "Command palette…". View menu becomes:
  existing font commands → separator → Command palette… → separator → Enter Full Screen.
  (No fullscreen item exists today — add a standard `toggleFullScreen:` item, ⌃⌘F. This
  is a spec-vs-implementation discrepancy to flag per CLAUDE.md.)
- **UI**: same overlay as the file picker (PickerView / PickerRow / PickerRowCellView),
  same fuzzy-match behavior (reuse `wolf_core::rank::rank_matches`, MRU-first).
- **Global command MRU**: new SQLite at `~/Library/Application Support/Wolf/command-mru.sqlite`
  (under `wolf_core` state_dir). Near-clone of `recent_dirs.rs`: lazy creation,
  delete-and-recreate on corruption/wrong schema, monotonic `seq` ordering, capped,
  always-on/disabled fallback. Keyed by an opaque command KEY string (static → registry `id`;
  dynamic → `switch_window:<cwd>`, `open_recent:<path>`).
- **Candidates**: ALL static commands (from `COMMAND_SPECS`) + one "Switch to Window: <title>"
  per OTHER open window + one "Open Recent Folder: <path>" per recent dir (`recent_dirs()`).
  The palette's own invoking command is EXCLUDED from the candidate list.

## Architecture notes / existing machinery to reuse (verify against code)

- `wolf_core::command` (`crates/wolf-core/src/command.rs`): `COMMAND_SPECS`, `Command`,
  `MenuGroup`. Add `Command::CommandPalette` (command_kind 13, id "command_palette", menu View)
  so the menu item wires through the existing tag→CommandKind→CommandDispatcher path. The
  discriminant order is a LOCKED contract shared across: `AppMenu.swift`, `CommandSeam.swift`,
  `CommandDispatcher.swift`, wolf-ffi `command_kind` module, and the Swift `CommandKind` port —
  ALL must gain the case. Exclude `CommandPalette` from the palette's own candidate list.
- `wolf_core::rank::rank_matches` — pure fuzzy ranker over `FileEntry{display, mru_rank}`;
  reuse for command ranking (MRU-first).
- `recent_dirs.rs` — template for the new command MRU store (global under `state_dir`, self-heal).
- FFI (`crates/wolf-ffi/src/lib.rs`): preferred design = an opaque session
  (`FfiCommandPalette` via `handle.command_palette()`) that internally assembles static
  (`COMMAND_SPECS`) + recent-dir rows + the command MRU, accepts the front-end open-window list
  (e.g. `add_window(index,title)` or parallel vecs), and exposes `ranked(query,max)->Vec<FfiCommandRow>`.
  Each row: `display()`, a kind discriminant (static/switch-window/open-recent), `command_kind()` u8,
  `window_index()` u16, `path()` String, `mru_key()` String; plus `record(key)` on invocation.
  Fallback if swift-bridge can't take the window list cleanly: generic ranker free-fn +
  `command_mru_recent()`/`record()`, front-end assembles candidates. Architect to CONFIRM
  swift-bridge feasibility and choose.
- Swift: `PaletteSeam.swift` (pure, WolfKeyMap, headlessly tested) with `isPaletteTrigger`
  (sibling of `PickerSeam.isPickerTrigger`, "o"→"p"); reuse `PickerSeam.nextSelection/restoredIndex`.
  `WolfWindowController.openCommandPalette()` opens the overlay seeded with other windows (from
  AppDelegate coordinator, excluding self) + recent dirs; on Enter acts per row kind
  (static→CommandDispatcher.dispatch; switch-window→raise that controller; open-recent→raise
  existing controller for that cwd if any, else chdir) and records the MRU key; then refocuses
  the editor. AppDelegate gains `otherWindows(excluding:)->[(index,title)]` and
  `controllerForCwd(path)->WolfWindowController?`. ⌘P is a menu key-equivalent (preempts keyDown
  like ⌘N), so the menu item is the primary trigger; PaletteSeam is the pure predicate kept
  consistent with PickerSeam.
- Reuse PickerView vs. a sibling CommandPaletteView: architect chooses (PickerView is coupled to
  FfiFileSearch; either generalize it with a pluggable session+activation, or write a parallel
  view reusing PickerRow/PickerRowCellView). NO async traversal for commands → NO wake bridge;
  candidates are synchronous and fixed at open; `ranked()` re-filters per keystroke.

## Testing (prefer e2e/integration with a real Neovim per CLAUDE.md)

- Rust command MRU store: unit+integration mirroring recent_dirs tests — record/recent ordering,
  cap, corruption self-heal (delete+recreate), disabled store, path under state_dir.
- Rust: palette candidate assembly + ranking + MRU application; exclusion of CommandPalette and
  the current window; dynamic row kinds/payloads.
- wolf-ffi integration test (real core): palette session `ranked()` kinds/payloads, mru
  record/recent round-trip, command_kind discriminant contract (extend `tests/commands.rs`).
- Swift headless: `PaletteSeamTests` (isPaletteTrigger), `CommandSeamTests` gains the
  CommandPalette discriminant case.

## Docs (source of truth per CLAUDE.md)

Update `crates/wolf-core/src/README.md`, `crates/wolf-ffi/src/README.md`,
`crates/wolf-macos/README.md` (+ `Sources/README.md`), `architecture.md`, and new-module doc
comments; reformat edited Markdown with `prettier --write` (no hard wraps in paragraphs).

## Architect plan (authoritative)

### 1. Summary
⌘P / View-menu fuzzy palette listing all static registry commands + one "Switch to Window"
per OTHER open window + one "Open Recent Folder" per recent dir, ranked MRU-first via
`wolf_core::rank::rank_matches`. Activation dispatches a static command / raises a window /
opens-or-raises a recent folder, recording the choice in a new self-healing global SQLite
command MRU (`<state_dir>/command-mru.sqlite`, modeled on `recent_dirs.rs`). Candidate assembly
+ ranking is PURE Rust (`wolf-core`), exposed via a new opaque swift-bridge session
(`FfiCommandPalette`) fed the window list ONE window at a time. Sibling `CommandPaletteView`
reuses `PickerRow`/`PickerRowCellView`; the proven `PickerView` is untouched.

### 2. Resolved design decisions
- **Q1 FFI shape (VERIFIED): no bridge fn takes a `Vec` arg** — Vec support is emitted only for
  opaque RETURN types. So the window list MUST be fed incrementally. Opaque session
  `FfiCommandPalette` from `WolfCoreHandle::command_palette()`; static + recent-dir candidates
  assembled at construction; front-end appends each other window via
  `add_window(index: u16, cwd: String)`. All methods `&self` + interior mutability
  (`RefCell<Vec<…>>`) — matches the existing `&self`-only / RefCell handle precedent
  (lib.rs:980). `ranked(query: String, max: usize) -> Vec<FfiCommandRow>`. Row accessors:
  `display()->String`, `row_kind()->u8` (new `mod palette_row_kind`: STATIC=0, SWITCH_WINDOW=1,
  OPEN_RECENT=2), `command_kind()->u8`, `window_index()->u16`, `path()->String`,
  `mru_key()->String`. Session also has `record(&self, key: String)` and `add_window`.
- **Q2 Command MRU store**: new `crates/wolf-core/src/command_mru.rs`, near-clone of
  `recent_dirs.rs` keyed on String. DB `command-mru.sqlite` under `state_dir()`; table
  `command_mru(key TEXT PRIMARY KEY, seq INTEGER NOT NULL)`; monotonic `MAX(seq)+1`; schema-probe
  `SELECT key, seq FROM command_mru LIMIT 0`; delete-and-recreate self-heal; `parking_lot::Mutex`;
  `disabled()` no-op. `pub const COMMAND_MRU_LIMIT: usize = 100;`. API `open`/`disabled`/
  `record(&self,&str)`/`recent(&self)->Vec<String>`; `#[derive(Clone)]` (Arc-backed).
  WolfCore owns `command_mru: CommandMru`, constructed always-on in `start` like `recent_dirs`
  (api.rs:205-207), NOT wired into the redraw handler (only palette activation records).
  Accessor `command_mru(&self)->CommandMru`. `lib.rs`: `pub mod command_mru;` + re-export.
- **Q3 Candidate assembly + ranking**: new pure `crates/wolf-core/src/palette.rs` with
  `PaletteRowKind{Static,SwitchWindow,OpenRecent}`, `PaletteWindow{index:u16, cwd:String}`,
  `PaletteRow{display, mru_key, kind, command:Option<Command>, window_index:Option<u16>,
  path:Option<PathBuf>}`, and
  `rank_palette(recent_dirs:&[PathBuf], windows:&[PaletteWindow], command_mru_recent:&[String],
  query:&str, max:usize) -> Vec<PaletteRow>`. Assembly (stable order for tie-breaks):
  (1) every `COMMAND_SPECS` row EXCEPT `Command::CommandPalette` → Static row (display=title,
  mru_key=id); (2) each window → display `"Switch to Window: {label}"` (label = "Untitled" when
  cwd empty else cwd), mru_key `switch_window:{cwd}`; (3) each recent dir → display
  `"Open Recent Folder: {path}"`, mru_key `open_recent:{path}`. `mru_rank` = index of `mru_key`
  in `command_mru_recent` else None. Build parallel `Vec<FileEntry>` (path empty, display,
  mru_rank), call `rank_matches` (REUSED UNCHANGED), `.take(max)`, map indices back to cloned
  `PaletteRow`. `lib.rs`: `pub mod palette;` + re-export.
- **Q4 `Command::CommandPalette` (discriminant 13) lockstep sites** (append, never reorder):
  (1) `command.rs` enum variant + `COMMAND_SPECS` row `{id:"command_palette",
  ex_command:"WolfCommandPalette", title:"Command Palette…", menu:View}` + doc-list update;
  (2) ffi `mod command_kind` `COMMAND_PALETTE=13`; (3) ffi `FfiCommand::from_command` arm
  (EXHAUSTIVE match — won't compile until added; intended guard); (4) `CommandSeam.swift`
  `.commandPalette` + `case 13`; (5) `AppMenu.swift` `Tag.commandPalette=13`;
  (6) `CommandDispatcher.swift` `.commandPalette → controller?.openCommandPalette()`.
  Lockstep tests: `wolf-ffi/tests/commands.rs` guard 0..=13; `CommandSeamTests.swift` ffi 13.
- **Q5 View menu** (`AppMenu.viewMenuItem`, AppMenu.swift:131-150): font items → `.separator()`
  → **Command Palette…** via `commandItem(title:"Command Palette…", tag:Tag.commandPalette,
  keyEquivalent:"p")` (plain ⌘P) → `.separator()` → **Enter Full Screen** as a STANDARD item
  (`action:#selector(NSWindow.toggleFullScreen(_:))`, keyEquivalent "f",
  mask `[.control,.command]`, `target:nil` → routes to key NSWindow). Full Screen is NOT a Wolf
  command (no tag/CommandKind); AppKit handles its validation + Enter/Exit title toggle;
  `validateMenuItem` needs NO change (it only inspects `handleCommandMenuItem` items).
  FLAG per CLAUDE.md: fullscreen is net-new (spec-vs-impl deviation) — READMEs must document it.
- **Q6 Dispatch**: `.commandPalette → controller?.openCommandPalette()`. Reachable on the menu
  path (key window's controller) AND the nvim-drained path (`:WolfCommandPalette` auto-registered
  since it is now a `COMMAND_SPECS` row → `drainAndDispatchCommands` passes `controller:self`).
- **Q7 Activation** (overlay owns session + rows; on Enter: `session?.record(row.mruKey)` →
  `onActivate?(row)` → `close()`): STATIC → `CommandKind(ffi:row.commandKind)` then
  `CommandDispatcher.dispatch(kind, window:window, coordinator:coordinator,
  originatingCwd:currentCwd, controller:self)`; SWITCH_WINDOW →
  `coordinator?.raiseWindow(windowIndex:row.windowIndex)`; OPEN_RECENT →
  `if let c = coordinator?.controllerForCwd(row.path) { c.raise() } else { changeDirectory(row.path) }`.
  Chosen row reaches the controller via `onActivate:(CommandPaletteRow)->Void` (same pattern as
  `emptyState.onOpenFolder` / `paneContainer.onPickerTrigger`).
- **Q8 Overlay**: SIBLING `CommandPaletteView.swift` (not a PickerView refactor — PickerView is
  coupled to FfiFileSearch/single-string payload/`open`). Structurally parallel to PickerView
  (search bar + NSTableView + `PickerSeam.nextSelection`/`restoredIndex`), REUSING `PickerRow` +
  `PickerRowCellView` for rendering. Holds `rows:[CommandPaletteRow]` (rich value type: label,
  kind, commandKind, windowIndex, path, mruKey). Placeholder "Run a command…". NO wake bridge
  (candidates fixed at open; `add_window` then `ranked()` re-filters synchronously per keystroke).
  Accepted tradeoff: ~200 lines panel chrome duplicated to leave the proven picker untouched.

### 3. New files
Rust: `crates/wolf-core/src/command_mru.rs`, `crates/wolf-core/src/palette.rs`,
`crates/wolf-core/tests/command_mru.rs` (new BUILD target), `crates/wolf-ffi/tests/command_palette.rs`
(new BUILD target, reuses `tests/common/mod.rs`).
Swift: `crates/wolf-macos/Sources/WolfKeyMap/PaletteSeam.swift` (`isPaletteTrigger` + `enum
PaletteRowKind:UInt8 { staticCommand=0, switchWindow=1, openRecent=2; init?(ffi:) }`; **dormant/
unwired** — kept for consistency, NOT called from keyDown), `crates/wolf-macos/Sources/WolfMacOS/
CommandPaletteView.swift`, `crates/wolf-macos/Tests/WolfKeyMapTests/PaletteSeamTests.swift`.

### 4. Modified files
`command.rs`, `api.rs`, `lib.rs` (wolf-core); `wolf-ffi/src/lib.rs` (command_kind::COMMAND_PALETTE,
from_command arm, `mod palette_row_kind`, `command_palette()` on WolfCoreHandle,
`FfiCommandPalette`/`FfiCommandRow` extern blocks + impls, `use wolf_core::{…}` additions,
hold `windows.borrow()` guard or clone across the `rank_palette` call); `CommandSeam.swift`;
`AppMenu.swift`; `CommandDispatcher.swift`; `WolfWindowController.swift` (`palette:CommandPaletteView?`
built in `start()` parallel to `picker`, `onActivate` wired, `openCommandPalette()` gathers
`coordinator?.otherWindows(excluding:self)`, private `activatePaletteRow(_:)`, close/drop in
`teardown()`); `AppDelegate.swift` (`otherWindows(excluding:)->[(index:Int,cwd:String)]`,
`controllerForCwd(_:)->WolfWindowController?`, `raiseWindow(windowIndex:)`);
`wolf-ffi/tests/commands.rs`; `CommandSeamTests.swift`; BUILD files; generated glue; docs.

### 5. Test plan (prefer real-nvim / headless-pure; no mocks)
- **wolf-core pure `#[cfg(test)]`** (run by existing `wolf_core_lib_test`, no BUILD change):
  `command.rs` — `from_id("command_palette")==Some(CommandPalette)`, ex_command
  `"WolfCommandPalette"`, `COMMAND_SPECS.len()==14`, index 13 is CommandPalette.
  `palette.rs` `rank_palette` — (a) CommandPalette EXCLUDED; (b) row kind/command/window_index/
  path/mru_key/labels correct; (c) empty cwd → `"Switch to Window: Untitled"` +
  `mru_key=="switch_window:"`; (d) MRU-recent static id floats first even vs stronger textual
  match; (e) empty recent + no windows → only static rows; (f) query AND-subsequence filter;
  (g) `max` truncation.
- **wolf-core headless integration — NEW `tests/command_mru.rs` + NEW BUILD target `command_mru`**
  (mirror headless `recent_dirs` target, wolf-core BUILD 142-149; tempdir, no nvim): DB creation
  at `<dir>/command-mru.sqlite`; record/recent newest-first; move-to-top; `COMMAND_MRU_LIMIT` cap
  (prune across re-open); silent self-heal over garbage/wrong-schema DB; `disabled()` → empty.
- **wolf-ffi real-nvim — extend `tests/commands.rs`** (existing `commands` target, no BUILD
  change): guard 0..=13; `:WolfCommandPalette` drains `FfiCommand` `kind()==13`.
- **wolf-ffi real-nvim — NEW `tests/command_palette.rs` + NEW BUILD target `command_palette`**
  (mirror `state`/`recent_dirs` single-`#[test]` hermetic `WOLF_STATE_DIR` targets, wolf-ffi
  BUILD 596-648): `handle.command_palette()`; `ranked("",max)` has static rows (STATIC kind,
  expected command_kinds, excludes CommandPalette); after `add_window(7,"/tmp/x")` a SWITCH_WINDOW
  row `window_index()==7`, `path()==""`; OPEN_RECENT row appears after a `:cd` populates
  recent_dirs; `record(key)` then a fresh `command_palette().ranked("")` shows that row first
  (MRU round-trip through physical `command-mru.sqlite`). MUST be a single `#[test]` per binary
  (mutates process-global `WOLF_STATE_DIR`).
- **Swift headless (`WolfKeyMapTests`)**: NEW `PaletteSeamTests.swift` (mirror PickerSeamTests):
  `isPaletteTrigger` table (Command+"p"/"P"→true; no command→false; wrong letter→false;
  empty→false); `PaletteRowKind(ffi:)` 0/1/2→cases, out-of-range→nil. MODIFY `CommandSeamTests.swift`:
  ffi 13→`.commandPalette`, out-of-range moves to 14/255.
- **AppKit views / menu / dispatch**: compile + manual-smoke only (picker convention).

### 6. BUILD changes (CRITICAL — not all auto-globbed)
- `crates/wolf-core/BUILD.bazel`: lib globs `src/**/*.rs` (line 31) → new src auto-included;
  ADD one headless `rust_test` target `command_mru` (copy `recent_dirs` headless target 142-149).
- `crates/wolf-ffi/BUILD.bazel`: lib srcs = `["src/lib.rs"]` → lib edits need no change; ADD one
  `rust_test` target `command_palette` (`srcs=["tests/command_palette.rs","tests/common/mod.rs"]`,
  `tags=["no-sandbox"]`, data/env for staged nvim + init.lua — copy `recent_dirs` 627-648).
- `crates/wolf-macos/BUILD.bazel`: `wolf_key_map` srcs is an EXPLICIT list (29-42) → ADD
  `"Sources/WolfKeyMap/PaletteSeam.swift"`. `wolf_macos_lib` globs `Sources/WolfMacOS/*.swift`
  (52) → `CommandPaletteView.swift` auto-included. `WolfKeyMapTests` srcs EXPLICIT (167-180) →
  ADD `"Tests/WolfKeyMapTests/PaletteSeamTests.swift"`.
- **Generated glue (committed)**: after editing `mod ffi`, regenerate
  `crates/wolf-ffi/generated/wolf-ffi.swift` (+ `SwiftBridgeCore.swift`) via
  `bazel run //crates/wolf-ffi:gen_bindings` and commit the diff (new FfiCommandPalette/
  FfiCommandRow glue). `generated_swift` + `.h` glob pick up the regenerated files.

### 7. Implementation order (avoids compile breaks)
1–4 together (command.rs enum+row+doc; command_mru.rs + lib re-export + api field/accessor;
palette.rs + lib re-export; wolf-ffi lib.rs incl. from_command arm — the exhaustive match breaks
until this lands — then regenerate glue). 5 WolfKeyMap (CommandSeam + PaletteSeam + BUILD src).
6 WolfMacOS (CommandDispatcher, AppMenu, AppDelegate helpers, CommandPaletteView,
WolfWindowController). 7 Tests (core pure + command_mru; ffi commands ext + command_palette;
Swift PaletteSeamTests + CommandSeamTests; BUILD targets/srcs). 8 Docs + `prettier --write`.

### 8. Dependencies: NONE new (rusqlite/parking_lot already in wolf-core; tempfile already dev-dep).

### 9. Lint notes
`command_mru.rs`: named-local mutex guard (avoid `clippy::significant_drop_in_scrutinee`);
`i64::try_from(...).unwrap_or(...)`; `#[must_use]` on `recent`. `palette.rs`: `#[must_use]` on
`rank_palette`. wolf-ffi: crate `#![allow(clippy::unnecessary_cast)]` + `mod ffi`
`#[allow(clippy::all, missing_docs, unused)]` cover bridge items; new non-bridge impls
(`FfiCommandPalette`/`FfiCommandRow`) MUST carry doc comments; RefCell OK on main-thread handle.
Swift: `PaletteRowKind:UInt8` + `init?(ffi:)` mirrors `StateSeam.ToolbarMode`.

### 10. Risks / edge cases / flags
- swift-bridge Vec-into-Rust unsupported → `add_window` is MANDATORY (not a fallback).
- **Full Screen discrepancy (FLAG):** net-new standard `toggleFullScreen:` ⌃⌘F item, NOT a Wolf
  command — READMEs document it as an intentional AppKit-standard item outside the registry.
- **`isPaletteTrigger` is DORMANT/UNWIRED:** ⌘P menu key-equivalent preempts keyDown (like ⌘N);
  the pure seam is kept for consistency + tested but NOT called from `PaneContainerView.keyDown`.
  (Corrects the brief's implicit "wire it like PickerSeam.") Document so a future reader knows.
- Empty recent-dirs / no other windows → still shows static rows (`rank_palette` handles empties).
- MRU disabled (no state_dir) → `recent()==[]` (registry-order), `record` no-op. No failure.
- Untitled window mru-key collision (multiple empty-cwd windows share `switch_window:`): accepted
  minor MRU quirk; `window_index` (the raise target) is still per-row correct.
- Switch-window target closed between open+activation → `raiseWindow`/`controllerForCwd` nil → no-op.
- Open-recent path deleted → `:cd` fails harmlessly (recent_dirs not pruned; matches menu Open Recent).
- Palette re-entrancy: CommandPalette excluded from its own list → cannot dispatch itself.
- `ranked` borrow: hold the RefCell borrow guard (or clone the window vec) across `rank_palette`.
- `tests/command_palette.rs` must be a single `#[test]` per binary (process-global `WOLF_STATE_DIR`).

### Load-bearing note for coder
`FfiCommand::from_command` (wolf-ffi lib.rs:1619-1636) is an EXHAUSTIVE match — will not compile
until the `Command::CommandPalette` arm is added (intended lockstep guard).
