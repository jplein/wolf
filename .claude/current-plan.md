# Plan: macOS file picker (wolf-macos)

## STATUS: COMPLETE ✅

All three layers implemented, all gates green, reviewer APPROVED WITH NITS (none
blocking), docs reconciled + prettier'd. Nothing committed (standing rule).
Remaining: a manual GUI smoke (not headlessly testable) — Shift+Ctrl+P opens the
panel, filtering/Up-Down/Enter/Esc behave; verify the NSSearchField Esc-closes
(nit #1). Two cosmetic/inconsequential nits left as-is.


## Problem statement

Implement the file picker UI in the `wolf-macos` AppKit application, mirroring the
existing `wolf-gnome` file picker.

**Behavior to mirror (from wolf-gnome `src/picker.rs` + README "The file picker"):**
- Trigger: **Shift+Control+P** opens a floating panel. (Deliberately
  non-conflicting — every Neovim key, including `<C-p>`, is preserved.)
- The panel has a text field that filters files in the current working directory,
  respecting `.gitignore` (and excluding `.git/` / `.wolf/`), MRU-first ranking,
  case-insensitive, filename-word strict-prefix preference. Empty query lists all.
- **Up/Down** navigate the results list (focus stays in the text field so typing
  keeps working; selection preserved across streaming traversal batches).
- **Enter** (or clicking a row) opens the selected file via `InputEvent::Open`
  (MRU auto-recorded by core's input pump); then closes.
- **Esc** closes the picker. With zero rows, Enter is a no-op.
- While open, all keys go to the picker and are NOT forwarded to Neovim.

**Constraints (from architecture.md / CLAUDE.md):**
- Ranking stays in **Rust** (`wolf_core::rank_matches`).
- Key translation / nvim notation stays in **Rust**.
- swift-bridge is confined to `wolf-ffi`; generated bindings under
  `crates/wolf-ffi/generated/` are committed and regenerated via
  `bazel run //crates/wolf-ffi:gen_bindings`.
- `wolf-core` must stay GUI-free — no changes expected there (it already has
  `start_file_search` / `FileSearch` / `FileSearchReadGuard` / `rank_matches` /
  `InputEvent::Open`).
- Pure, AppKit-free seams are preferred and unit-tested headlessly (mirror
  `is_picker_trigger` / `next_selection` / `restored_index` from wolf-gnome).
- Tests use real Neovim, not mocks.
- macOS app: AppKit (not SwiftUI), Swift, Tahoe-only, all-Bazel.

## Relevant existing surface (confirmed by orchestrator)

- `wolf-core`: `WolfCore::start_file_search(wake: WakeFn) -> FileSearch`;
  `FileSearch::read() -> FileSearchReadGuard { entries: Vec<FileEntry>, complete,
  version }`; `FileEntry { path: PathBuf, display: String }`; `rank_matches(query,
  &[FileEntry]) -> Vec<Match{index,...}>`; `InputEvent::Open { path }`.
- `wolf-ffi/src/lib.rs`: single `#[swift_bridge::bridge] mod ffi`. Today exposes
  `wolf_ffi_start`, `WolfCoreHandle{read_frame,send_key,resize,shutdown}`,
  `FfiFrame`, `FfiCell`, `FfiColor`, `FfiKeyEvent`. Wake is a SINGLE global
  registered `extern "C"` callback (`wolf_ffi_set_wake`) → Swift `WakeBridge`
  marks the editor view `needsDisplay`.
- `wolf-macos/Sources/WolfKeyMap/KeyMap.swift`: pure `translate(...)` seam (no
  AppKit). `WolfMacOS/{AppDelegate,GridView,MainWindow,CellMetrics,Cursor,Color,
  WakeBridge,main}.swift`. GridView.keyDown forwards every translated key to
  `send_key`.

## Three layers of work

1. **wolf-ffi**: expose a file-search session handle + in-Rust ranking + an
   `open(path)` input + a picker-specific wake; regenerate committed bindings.
2. **wolf-macos pure seam**: a Swift trigger predicate (Shift+Ctrl+P) + list-nav
   helpers (`nextSelection`/`restoredIndex` analogs), headlessly testable (no
   AppKit), mirroring wolf-gnome's pure picker seams + their table tests.
3. **wolf-macos UI**: the AppKit floating picker panel (text field + results
   list) wired into `GridView`/`AppDelegate`, fed by the FFI file search,
   suppressing key-forwarding to nvim while open.

## Orchestrator decisions on architect's flagged items (ALL APPROVED)

1. **Pure seam in the existing `WolfKeyMap` module** — APPROVED. Add
   `PickerSeam.swift` to the existing `wolf_key_map` `swift_library` + test in the
   existing `WolfKeyMapTests` `swift_test`. No new module/targets. Do NOT rename
   the module.
2. **New `//crates/wolf-ffi:file_search` `rust_test`** — APPROVED (isolated from
   `bridge_smoke`, modeled on it: real pinned nvim, `no-sandbox`, staged runfiles).
3. **`FfiFileSearch { search: Option<FileSearch> }`** — APPROVED (keeps
   `start_file_search` infallible across the bridge; `ranked` returns empty when
   `None`, mirroring `read_frame`'s empty-frame fallback).
4. **In-window overlay `NSView`** for the panel — APPROVED (parity with gnome's
   `Overlay`; simpler focus than a child `NSPanel`).

## APPROVED PLAN

### Layer 1 — wolf-ffi (Rust bridge + regenerated bindings)
- In `crates/wolf-ffi/src/lib.rs`, inside `#[swift_bridge::bridge] mod ffi`:
  - New opaque `type FfiFileSearch;` with `fn ranked(&self, query: String, max:
    usize) -> Vec<FfiFileEntry>;`
  - New opaque `type FfiFileEntry;` with `fn display(&self) -> String;` and
    `fn path(&self) -> String;` (OPAQUE because swift-bridge emits `Vec<T>` only
    for opaque T — same reason `FfiCell` is opaque).
  - Extend `extern "Rust" { type WolfCoreHandle; ... }` with
    `fn start_file_search(&self) -> FfiFileSearch;` and `fn open(&self, path:
    String);`.
- Hand-written glue:
  - `pub struct FfiFileSearch { search: Option<wolf_core::FileSearch> }`. `ranked`:
    `let Some(s)=&self.search else { return vec![] };` `let view = s.read();`
    `rank_matches(&query, &view.entries)` `.into_iter().take(max)` → clone
    `view.entries[m.index]` into `FfiFileEntry { path, display }`; keep read-guard
    scope short (drop before return).
  - `pub struct FfiFileEntry { path: PathBuf, display: String }` with
    `display()`/`path()` (path via `to_string_lossy().into_owned()`).
  - `WolfCoreHandle::start_file_search(&self)`: if core present,
    `core.start_file_search(registered_filesearch_wake())` wrapped in
    `FfiFileSearch{ search: Some(..) }`; else `Some`→ `None` wrapper.
  - `WolfCoreHandle::open(&self, path: String)`: `core.send_input(InputEvent::Open
    { path: PathBuf::from(path) })` (MRU recorded by core's input pump).
  - Add `FileSearch`, `rank_matches` to the `use wolf_core::{...}` import.
  - Full `///` docs + `#[must_use]` on accessors (crate denies `missing_docs`
    outside the bridge block).
- Second registered wake (Swift-free, mirrors the editor wake exactly):
  `static FILESEARCH_WAKE_FN: AtomicPtr<()>`; `#[no_mangle] pub extern "C" fn
  wolf_ffi_set_filesearch_wake(wake: Option<WakeCallback>)`;
  `fn registered_filesearch_wake() -> WakeFn` (loads FILESEARCH_WAKE_FN; same
  `# Safety` + transmute shape as `registered_wake`).
- `crates/wolf-ffi/generated/wolf_ffi_wake.h`: add
  `void wolf_ffi_set_filesearch_wake(void (*wake)(void));` (hand-written header;
  covered by existing `glob(["generated/*.h"])`, NO BUILD change for the symbol).
- Regenerate: `bazel run //crates/wolf-ffi:gen_bindings` → rewrites the 4 committed
  `generated/{SwiftBridgeCore,wolf-ffi}.{swift,h}` files; re-commit them. NO
  `CARGO_BAZEL_REPIN` (no new Cargo dep).
- `crates/wolf-ffi/BUILD.bazel`: add a new `rust_test` `file_search` (copy
  `bridge_smoke`'s `data`/`env`/`tags=["no-sandbox"]`/`size`/`deps`, reuse
  `tests/common/mod.rs`).

### Layer 2 — wolf-macos pure seam (headless, no AppKit)
- CREATE `crates/wolf-macos/Sources/WolfKeyMap/PickerSeam.swift` — `public enum
  PickerSeam` namespace (Foundation only):
  - `isPickerTrigger(charactersIgnoringModifiers: String, ctrl: Bool, shift: Bool)
    -> Bool` — `ctrl && shift && firstScalar.lowercased == "p"`; ignores alt/meta.
    Uses `charactersIgnoringModifiers` (layout-independent; with Ctrl held,
    `characters` is the control scalar).
  - `nextSelection(current: Int, delta: Int, nRows: Int) -> Int?` — exact port of
    Rust `next_selection` (clamp(current+delta, 0, nRows-1), nil when nRows<=0).
  - `restoredIndex(resetToTop: Bool, prev: Int?, newLen: Int) -> Int?` — exact port
    of Rust `restored_index`.

### Layer 3 — wolf-macos AppKit UI
- CREATE `Sources/WolfMacOS/PickerView.swift` — `final class PickerView: NSView`
  (layer-backed) added as a top-center overlay subview of the window content view,
  initially hidden. Holds an `NSSearchField` (top) + `NSTableView` in `NSScrollView`
  (single column/selection). Conforms to `NSTableViewDataSource`/`Delegate` +
  `NSControlTextEditingDelegate`. State (main-thread plain props): `search:
  FfiFileSearch?`, `rowEntries: [FfiFileEntry]`, `lastQuery: String?`, the
  `WolfCoreHandle`, ref to the `GridView` for focus restore. Constants mirror gnome:
  `maxRows=200`, width=600, height=400, topMargin=24.
  - `open()` idempotent: `handle.start_file_search()`, show, clear+focus field
    (`window?.makeFirstResponder(searchField)`), `lastQuery=nil`, `refresh()`.
  - `close()` idempotent: `search=nil` (drops→cancels traversal), hide,
    `window?.makeFirstResponder(gridView)`.
  - `refresh()` no-op when closed: read `query`; `resetToTop=(lastQuery != query)`;
    set `lastQuery`; capture `prevSelected` (map `-1`→nil); `search?.ranked(query,
    UInt(maxRows))` → `rowEntries`; `reloadData()`; select via
    `PickerSeam.restoredIndex` + scroll visible.
  - `moveSelection(delta:)` via `PickerSeam.nextSelection` (field keeps focus).
  - `activateSelection()`: empty→true no-op; else `handle.open(entry.path())` +
    `close()`.
  - Delegate `control(_:textView:doCommandBy:)`: `moveUp:`→-1, `moveDown:`→+1,
    `insertNewline:`→activate, `cancelOperation:`→close (all return true); default
    false. `controlTextDidChange`→`refresh()`. Table double-click→`activateSelection`.
- CREATE `Sources/WolfMacOS/PickerWakeBridge.swift` — `enum PickerWakeBridge` with
  `weak static var registeredPicker: PickerView?` and `install()` registering
  `wolf_ffi_set_filesearch_wake({ DispatchQueue.main.async {
  PickerWakeBridge.registeredPicker?.refresh() } })` (stateless `@convention(c)`
  thunk, mirrors `WakeBridge`). Late wake after close is harmless (refresh no-ops).
- MODIFY `Sources/WolfMacOS/GridView.swift` — add `var onPickerTrigger: (() ->
  Void)?`; in `keyDown(_:)` BEFORE translate/send_key, if
  `PickerSeam.isPickerTrigger(...)` call `onPickerTrigger?()` and `return`. Picker
  open ⇒ its field is first responder ⇒ GridView.keyDown stops firing (suppression
  for free; no `isOpen` guard needed). Esc handled by the field delegate.
- MODIFY `Sources/WolfMacOS/AppDelegate.swift` — `PickerWakeBridge.install()`;
  build+hold `picker = PickerView(handle:gridView:metrics:)`, add as overlay
  subview (hidden), `PickerWakeBridge.registeredPicker = picker`,
  `gridView.onPickerTrigger = { [weak picker] in picker?.open() }`. Optionally
  `picker?.close()` before `handle.shutdown()` on terminate.
- MODIFY `crates/wolf-macos/BUILD.bazel` — add `Sources/WolfKeyMap/PickerSeam.swift`
  to `wolf_key_map` srcs; add the new test file to `WolfKeyMapTests` srcs.
  (`wolf_macos_lib` globs `Sources/WolfMacOS/*.swift` → new files auto-included.)

### Tests
- CREATE `crates/wolf-macos/Tests/WolfKeyMapTests/PickerSeamTests.swift` —
  table-driven, mirroring picker.rs `#[cfg(test)]` tables for `isPickerTrigger`
  (incl. Ctrl+Shift+Alt+P→true, wrong-letter→false, missing-modifier→false),
  `nextSelection` (all 12 cases), `restoredIndex` (all cases). `import XCTest` +
  `@testable import WolfKeyMap`. Wired into existing `WolfKeyMapTests`.
- CREATE `crates/wolf-ffi/tests/file_search.rs` — real pinned nvim (stage runfiles
  like `bridge_smoke`, reuse `tests/common/mod.rs`). Seed a temp cwd with a couple
  `.md` files; `wolf_ffi_start` there; `start_file_search()`; poll `ranked("", N)`
  until it streams in the created files; assert displays present, `path()` absolute
  + `display()` relative; `open(path())` issues without panic and core stays
  responsive. HONEST: cannot assert exact opened buffer name (no `current_file`
  accessor on `FfiFrame`, and we are NOT adding one) — assert `ranked` correctness
  strongly, `open` at the round-trip-survives level.
- NOT headlessly testable (state honestly, mirroring the existing increment): the
  AppKit panel rendering, `NSSearchField` first-responder transfer, the
  `doCommandBy` routing, table selection, the dispatched picker wake reload, and the
  `keyDown` trigger path. Manual smoke only.
- Regression gates: `//crates/wolf-ffi:frame_pack`, `:bridge_smoke`,
  `//crates/wolf-macos:WolfKeyMapTests`, `:bundle_layout_test`. wolf-core UNCHANGED
  (GUI denylist clean — new glue pulls only `wolf_core` + `std`).

### Implementation order (FFI green before AppKit, since UI isn't headlessly testable)
1. wolf-ffi bridge + glue + second wake → `bazel build //crates/wolf-ffi:wolf_ffi`.
2. Wake header.
3. `bazel run //crates/wolf-ffi:gen_bindings`; commit the 4 generated files.
4. `tests/file_search.rs` + BUILD target → `bazel test //crates/wolf-ffi:file_search`
   (+ re-run frame_pack, bridge_smoke). GATE before AppKit.
5. `PickerSeam.swift` + `PickerSeamTests.swift` + BUILD → `bazel test
   //crates/wolf-macos:WolfKeyMapTests`. GATE.
6. AppKit: `PickerWakeBridge.swift` → `PickerView.swift` → `GridView.swift` →
   `AppDelegate.swift` → `bazel build //crates/wolf-macos:Wolf` + `bundle_layout_test`.
7. Docs reconciliation + manual smoke.

### Docs to reconcile afterward
- `architecture.md` (wolf-macos phase status: picker no longer deferred; update the
  "Deferred for macOS" list).
- `crates/wolf-ffi/README.md` (FfiFileSearch/FfiFileEntry, start_file_search/open,
  the second wake, the file_search test).
- `crates/wolf-macos/README.md` + `Sources/README.md` (PickerSeam/PickerView/
  PickerWakeBridge, Shift+Ctrl+P, update Known limitations).
- prettier --write all touched Markdown.

## Progress log
- (orchestrator) Wrote problem statement; architect produced the plan above;
  orchestrator APPROVED all 4 flagged items. Next: test-writer.

## Tests written (test-writer)
- `crates/wolf-macos/Tests/WolfKeyMapTests/PickerSeamTests.swift` — table-driven
  XCTest for `PickerSeam.isPickerTrigger` (9 cases), `nextSelection` (11),
  `restoredIndex` (7); `@testable import WolfKeyMap`, matches `KeyMapTests.swift`
  conventions. Coder must add it to the `WolfKeyMapTests` `swift_test` srcs.
- `crates/wolf-ffi/tests/file_search.rs` — real-nvim integration modeled on
  `bridge_smoke.rs` (`mod common;` include style, `nvim_bin_or_skip` loud-skip,
  `tempfile::tempdir` cwd seeded with `alpha.md`/`beta.md`, bounded poll of
  `ranked("",200)`, asserts displays present + path absolute/under-cwd +
  `ranked("alpha")` puts alpha first + `open()` round-trip survives + shutdown).
  Coder must add the `file_search` `rust_test` target to
  `crates/wolf-ffi/BUILD.bazel`.
- Test-writer's API assumptions the coder MUST satisfy exactly: method names +
  arg labels `start_file_search()`, `ranked(query,max)`, `display()`, `path()`,
  `open(path)`; opaque types named `FfiFileSearch`/`FfiFileEntry`; `ranked("")`
  returns top-level files with `display()`=bare filename, `path()`=absolute;
  Swift `PickerSeam` public enum in `WolfKeyMap`, three statics, `Int?` returns,
  external arg labels `charactersIgnoringModifiers:ctrl:shift:`,
  `current:delta:nRows:`, `resetToTop:prev:newLen:`.

## Implementation + verification result (coder) — ALL GATES GREEN

Implemented per plan across all 3 layers. Verbatim results:
1. `bazel build //crates/wolf-ffi:wolf_ffi` — PASS (55 actions).
2. `bazel run //crates/wolf-ffi:gen_bindings` — PASS; `generated/wolf-ffi.{swift,h}`
   changed (now expose start_file_search/open/FfiFileSearch.ranked/FfiFileEntry.
   display/path); SwiftBridgeCore.{swift,h} content-stable (shared runtime, expected);
   hand-edited `wolf_ffi_wake.h`.
3. `bazel test //crates/wolf-ffi:file_search` — PASS (`file_search_ranks_and_opens`).
4. `bazel test //crates/wolf-ffi:frame_pack :bridge_smoke` — PASS (frame_pack 11;
   bridge_smoke RAN, not skipped — a pinned nvim was available — and passed).
5. `bazel test //crates/wolf-macos:WolfKeyMapTests` — PASS (14 tests: 11 KeyMap + 3
   PickerSeam).
6. `bazel build //crates/wolf-macos:Wolf` — PASS (only benign pre-existing
   `@retroactive` warnings).
7. `bazel test //crates/wolf-macos:bundle_layout_test` — PASS.

- wolf-core UNTOUCHED (no git changes, no Cargo/Bazel lock change, no REPIN).
- GUI denylist clean (only `libc`'s `glibc` path matches the "glib" substring — the
  documented false positive; no gtk/gdk/cairo/pango/adwaita/cocoa/objc/wgpu/skia).
- No test changes needed; both test files passed as written.
- HONEST: the AppKit UI (panel render, first-responder transfer, doCommandBy
  routing, table selection, dispatched wake reload, keyDown trigger) is compile-only
  verified (green Wolf build); not headlessly testable — manual smoke required.
- Files created: wolf-ffi glue in `src/lib.rs`; `PickerSeam.swift`,
  `PickerWakeBridge.swift`, `PickerView.swift`; modified `GridView.swift`,
  `AppDelegate.swift`, both BUILD.bazel files, `wolf_ffi_wake.h`, regenerated
  bindings.

## Review result (reviewer) — APPROVED WITH NITS

No blocking defects. Confirmed EXACT gnome parity of all three pure seams
(`isPickerTrigger`/`nextSelection`/`restoredIndex`) and of the open/close/refresh/
activate/move flow. FFI glue correct (ranking in Rust, picker-specific wake genuinely
independent from the editor wake, `open` records MRU via the input pump, `Option`
infallibility). Threading clean, no retain cycles (weak `registeredPicker`,
`[weak picker]`, weak `gridView`). Tests non-tautological. wolf-core untouched.

Nits (non-blocking):
1. **MANUAL-SMOKE ITEM**: the panel uses `NSSearchField`, which has a built-in
   clear-on-Escape. The code routes `cancelOperation:` through `doCommandBy` and
   returns `true`, which SHOULD suppress the clear and close the picker — but confirm
   in manual smoke that the FIRST Esc closes (gnome parity), not just clears.
2. Cosmetic: `ranked` drops the read guard at scope end (functionally fine) rather
   than an explicit early `drop(view)`.
3. Inconsequential: `moveSelection` sees `selectedRow == -1` → 0 vs gnome's default-0
   (+1→1); unreachable because `refresh()` always pre-selects row 0 when rows exist.
4. Docs deferred to the docs phase (below) — code now contradicts stale docs.

## Status
- [x] Architect plan
- [x] Tests written
- [x] Implementation + tests passing
- [x] Review (APPROVED WITH NITS — none blocking)
- [x] Docs (architecture.md, wolf-ffi README + src/tests READMEs, wolf-macos README
      + Sources/Tests READMEs; prettier'd via `~/.nix-profile/bin/prettier`)
