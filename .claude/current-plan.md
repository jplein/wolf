# Plan: Split panes on macOS (wolf-macos)

## Problem statement

The Linux/GNOME front-end (`wolf-gnome`) has a working native split-pane editor
area: a tree of nested `gtk::Paned`s, one `DrawingArea` per Neovim window, driven by
a pure `PaneTree` layout (`wolf-gnome/src/layout.rs`) and `wolf-core`'s
layout-intent channel. Splits (`<C-w>s`/`v`, `:split`/`:vsplit`), close (`<C-w>c`),
focus moves (`<C-w>h/j/k/l`), and divider drag-to-resize all work, plus a bottom
cmdline strip painting the outer grid (grid 1).

The macOS front-end (`wolf-macos`) currently paints only the **active grid** in a
single `GridView`, and the `wolf-ffi` swift-bridge surface exposes almost none of the
pane machinery. We must bring macOS to parity.

`wolf-core` already has everything needed on the Rust side: `drain_intents()`,
`focus_win(win)`, `resize_grid(grid, cols, rows)`, `Snapshot.windows` (grid→WinInfo
with win handle), `Snapshot.active_win`, `Snapshot.grid_for_win(win)`, per-grid
`Grid`s, and the rebound chords/Ex-commands in `runtime/nvim/init.lua`. The work is in
`wolf-ffi` (expose the pane surface) and `wolf-macos` (build the native pane view).

## Decisions already made with the user (load-bearing)

1. **Port the pure `PaneTree` layout to Swift**, mirroring the existing
   KeyMap/Cursor/PickerSeam Rust→Swift ports. Keep it AppKit-free and headlessly
   unit-tested in the `WolfKeyMap` test bundle. Map the recursive `PaneLayout` onto
   AppKit's **n-ary `NSSplitView`** (NSSplitView is natively n-ary, so each
   `PaneLayout::Split` maps to one NSSplitView with N arranged subviews — cleaner than
   GNOME's right-leaning binary `gtk::Paned` chain). The GNOME `layout.rs` stays as-is
   (no move to core); the Swift port is the macOS copy, like the other pure seams.

2. **Include the bottom cmdline strip** (paint outer grid 1's last row) so
   `:split`/`:vsplit`/`:close` Ex-commands stay usable (editor panes stay visible
   instead of the whole view swapping to grid 1). True parity with GNOME's pane_view.

## Project policy constraints

- Real-Neovim integration tests over mocks (the FFI tests drive a real pinned nvim).
- Each source directory has a README that must be kept in sync (docs are the source of
  truth per CLAUDE.md).
- Reformat edited Markdown with `prettier --write`; no hard line breaks within a
  paragraph.
- Non-trivial change → orchestrated (architect → test-writer → coder → reviewer →
  technical-writer).

## Reference material (read these)

- GNOME pure layout: `crates/wolf-gnome/src/layout.rs` (the `PaneTree` to port) and
  its headless tests in the same file.
- GNOME widget glue (the behavior to mirror in AppKit):
  `crates/wolf-gnome/src/pane_view.rs` and its `src/README.md` section.
- Core API: `crates/wolf-core/src/api.rs` (`drain_intents`, `focus_win`,
  `resize_grid`), `crates/wolf-core/src/layout.rs` (`LayoutIntent`, `SplitOrientation`,
  `FocusDir`), `crates/wolf-core/src/render.rs` (`Snapshot.windows`, `WinInfo`,
  `active_win`, `grid_for_win`, per-grid `Grid`).
- Current macOS shell: `crates/wolf-macos/Sources/WolfMacOS/{GridView,MainWindow,
  EditorSplitViewController,AppDelegate,WakeBridge,Cursor,Color,CellMetrics}.swift`.
- Current FFI: `crates/wolf-ffi/src/lib.rs` (`FfiFrame`, `read_frame`, the wake
  trio), `crates/wolf-ffi/README.md`.

## Status: COMPLETE ✅ — two manual-smoke bugs found + fixed (blank panes, missing cmdline)

### Bug 2: command line not visible (FIXED)
After the single-view rewrite, panes + content + editing worked but the command line
didn't show. Diagnosis (via screenshotting the built .app + logging grid contents):
under `ext_multigrid`, nvim 0.12 renders the command line on a DEDICATED MESSAGE grid
(grid 3 here), NOT grid 1 — positioned via `msg_set_pos` (which the core drops). The
cursor moves onto that grid in cmdline mode. The strip only painted grid 1 (always
empty). Confirmed: typing `:set number` → `cursorGrid=3`, grid 3 row 0 = `':set
number'`, `windows=[g2]` (grid 3 is not a window). Fix: `paintCmdlineStrip` now paints
the grid the cursor is on when that grid is neither grid 1 nor a window grid (its
cursor row) — the message/cmdline grid — falling back to grid 1's last row otherwise.
Verified live: `:set number incsearch wrap` renders in the bottom strip with the
cursor. Docs corrected (Sources/README, README, architecture.md cmdline-strip
descriptions) + prettier. Build + WolfKeyMapTests + panes + bundle_layout_test green.
No diagnostics left.

---

## Status: COMPLETE ✅ — manual smoke surfaced a render bug; FIXED by a single-view rewrite

### Manual smoke found: editor panes blank (regression)
The NSSplitView-based pane view did not composite: `draw()` ran with correct frames
+ colors, but NOTHING the nested custom layer-backed views drew reached the screen
(verified by screenshotting a built `.app`; even a red layer bg on `PaneContainerView`
/`splitHost` didn't show). nvim was fully alive (`:e`/`:cd`/splits processed; tree +
statusline updated) — only the editor pane/strip painting was invisible. Root cause:
deeply-nested custom flipped layer-backed views (`PaneContainerView` → `splitHost` →
`NSSplitView` tree → `PaneGridView`s) inside the source-list `NSSplitViewController`
host don't composite, where the old SINGLE `GridView` (direct host child) did.

### Resolution (user-approved): single flat painting view
Rewrote `PaneContainerView` as ONE flat custom `NSView` (the proven `GridView`
pattern) that paints every pane itself: `draw()` computes each pane's rect via
`PaneTree.paneRects()`, clips+translates, paints that pane's grid (cells + glyphs +
cursor), DIMS inactive panes (a front-end-only treatment the user explicitly wanted),
strokes dividers, and paints grid 1's last row in a bottom cmdline strip.
`onRenderWake()` orchestration unchanged in shape; ends with `syncGridSizes()`
(idempotent resize → converges, no feedback loop) + `needsDisplay`. Deleted
`PaneGridView.swift` + `CmdlineStripView.swift`. Trade-off: divider drag-to-resize
deferred (equal-weighted splits; parity with GNOME v1). FFI surface, `PaneTree`, and
all tests UNCHANGED.

### Verified working (live, via screenshots of the built .app)
Single pane (cursor + tildes); `:e` loads real content; `:vsplit` → two equal
side-by-side panes, inactive dimmed, divider; `<C-w>s` → stacked panes (active bright
/ inactive dimmed); file-tree reveal. All 8 Bazel targets pass; `Wolf` builds. Docs
re-synced to the single-view model (architecture.md, wolf-macos README/Sources README/
Tests README, PaneTree.swift comments) + prettier. No code diagnostics left.

---

## (superseded) Earlier status: COMPLETE ✅ (manual smoke pending — needs a window server)

### Docs updated (technical-writer) + prettier (orchestrator)
6 files synced to the implemented code: `architecture.md` (macOS phase status = native
n-ary NSSplitView panes + cmdline strip; FFI surface + discriminant tables in the contract;
shared v1 limitations carried to macOS; panes/cmdline out of macOS Deferred), `wolf-ffi/
README.md` + `src/README.md` (new bridge surface, 3 discriminant tables, per-grid FfiFrame,
:panes target), `wolf-macos/README.md` + `Sources/README.md` (PaneTree/PaneGridView/
PaneContainerView/CmdlineStripView; GridView removed; WakeBridge registeredPaneView +
onRenderWake; Known-limitations rewrite + manual-smoke checklist), `wolf-macos/Tests/
README.md` (PaneTreeTests). `prettier --write` run on all six (only wolf-ffi/README.md
needed reformatting). core/GNOME docs untouched.

### Final checklist
- [x] Architect plan
- [x] Tests written (PaneTreeTests.swift + panes.rs)
- [x] Implementation + all 8 Bazel targets pass; Wolf builds
- [x] Review (APPROVED, no findings routed)
- [x] Docs updated + prettier
- [ ] Manual smoke on a real macOS host (NSSplitView panes, focus moves, divider drag,
      close, cmdline strip, picker/tree refocus) — needs a window server; not automatable.

### Standing rule: nothing committed.


### Review result (reviewer) — APPROVED with doc-sync as the only outstanding item
Full plan conformance across FFI / Swift PaneTree port / AppKit glue / tests / BUILD.
Discriminant contracts agree across all four sources (Rust modules, Swift init?(ffi:),
panes.rs, PaneTreeTests). §3.5 per-grid cursor refactor correct + preserves frame_pack
(absent-grid !cursor_visible asserted). No retain cycles ([weak self]/[weak picker], weak
statics). Scope clean: no wolf-core, no wolf-gnome change; GridView.swift fully removed from
code (only READMEs still mention it). Two plan-sanctioned adaptations recorded as correct:
detectCloses via `windows()` live-grid set (not snap.grid().is_none()); updateActivePane
reads cursor grid via `read_frame().cursor_grid()`. NO findings routed back to coder.
Outstanding (technical-writer): architecture.md + wolf-ffi READMEs + wolf-macos
README/Sources README/Tests README per plan §8 (README.md ~39/~46 still name GridView +
old registeredView/needsDisplay; Sources/README ~29 same + missing the 4 new Swift files).



### Implementation result (coder) — ALL TARGETS PASS, Wolf builds
FFI: discriminant modules (verified vs `wolf_core` variant order), opaque `FfiLayoutIntent`
/`FfiWinInfo`, `WolfCoreHandle` {read_frame_for_grid, resize_grid, drain_intents, windows,
grid_for_win, active_win, focus_win}, `FfiFrame` per-grid refactor (grid_id+cursor_grid,
from_snapshot_grid, §3.5 cursor_colors-by-grid, absent→empty). Bindings regenerated (no
wake-header/SwiftBridgeCore change). `:panes` BUILD target added. Swift `PaneTree.swift`
(Foundation-only, init?(ffi:)). New `PaneGridView`/`CmdlineStripView`/`PaneContainerView`;
shell rewired (WakeBridge registeredPaneView→onRenderWake then tree; MainWindow editor
cols×rows + strip extra, 80-col preserved); `GridView.swift` DELETED.
Verbatim: wolf_core_lib_test, bridge_smoke, file_search, frame_pack, panes, tree_search,
WolfKeyMapTests, bundle_layout_test → ALL PASS (8/8). `//crates/wolf-macos:Wolf` builds.
Test uncertainties resolved empirically WITHOUT weakening assertions: `!hidden() &&
grid()>=2` gave 1/2/1 windows; `resize_grid` settled exactly to `cols()==40`. swift-bridge:
`Vec<FfiLayoutIntent>`/`Vec<FfiWinInfo>` codegen clean (scalar accessors, no RustString).

### Orchestrator decision on coder's optional test recommendations
Coder suggested (optional) a synthetic two-grid `frame_pack` unit test + direct
`grid_id()`/`cursor_grid()` accessor coverage. DECLINED as a separate test-writer pass: the
cross-grid path + accessors are exercised end-to-end by the real-nvim `:panes` test (two
windows, grid 1, per-grid frame reads), which per project policy (e2e over mocks) is
preferable to a synthetic snapshot. Coverage judged adequate.



### Tests written (test-writer)
- `crates/wolf-macos/Tests/WolfKeyMapTests/PaneTreeTests.swift` — full 1:1 port of the
  `layout.rs` suite + an FFI-discriminant-contract test (assumes `Orientation.init?(ffi:)`
  / `FocusDir.init?(ffi:)`). `@testable import WolfKeyMap`, XCTest style matching
  KeyMapTests/PickerSeamTests. Coder must add `PaneTree.swift` to `wolf_key_map` srcs and
  the test file to `WolfKeyMapTests` srcs.
- `crates/wolf-ffi/tests/panes.rs` — real-pinned-nvim integration test (one test
  `panes_split_resize_focus_close`) covering attach → baseline 1 window → `:vsplit` → SPLIT
  intent (VERTICAL, from==original, new distinct win) → 2 windows → `resize_grid` to exact
  `cols()==40` (fallback `0<cols<=40` noted in-code) → `focus_win` → `:close` → 1 window,
  `grid_for_win(closed)==0` → grid 1 `rows()>0` → shutdown. Sends `:vsplit<CR>`/`:close<CR>`
  as `FfiKeyEvent` sequences. Coder must add the `//crates/wolf-ffi:panes` BUILD target
  (mirror `tree_search`).

### Test-writer uncertainties for the coder to resolve
- `windows()` liveness filter: the Rust test filters `!hidden() && grid() >= 2` to count
  live editor windows. If `hidden()` is set on transiently-hidden-but-live windows the
  baseline/post-split counts could need dropping `!hidden()` (count on `grid() >= 2` only).
  Resolve empirically when running `:panes`.
- `resize_grid` exact `cols()==40` asserted; in-code fallback comment allows `0<cols<=40`
  if flaky headless.



---

# AUTHORITATIVE ARCHITECT PLAN

## 1. Summary

Bring the GNOME split-panes experience to macOS/AppKit. The pure pane-layout tree
(`crates/wolf-gnome/src/layout.rs::PaneTree`) is ported faithfully to AppKit-free Swift
in the `WolfKeyMap` module (Foundation only, headlessly unit-tested), like the existing
`KeyMap.swift` / `Cursor.swift` / `PickerSeam.swift` ports. The `wolf-ffi` bridge is
widened with per-grid frame reads, drained layout intents, window enumeration,
`grid_for_win` / `active_win`, `focus_win`, `resize_grid`. A new AppKit pane container
builds nested **n-ary `NSSplitView`s** from `PaneTree.layout()` (one NSSplitView per
`PaneLayout.split`, N arranged subviews), reusing one `PaneGridView` per pane, plus a
one-row cmdline strip painting outer grid 1's last row. Render-wake orchestration mirrors
`pane_view.rs::on_render_wake` one-for-one. GNOME `layout.rs` + glue stay unchanged; core
already exposes the full surface, so **no `wolf-core` change is required**.

(Note: the architect observed there is NO Go in this repo, so any Go-lint guidance is
inapplicable; the real bars are strict clippy — doc comments, `#[must_use]`, discriminant
constant modules — and the established swift-bridge opaque-type pattern.)

## 2. Files to create

### A. Swift pure layout port (Foundation-only, in `WolfKeyMap`)

`crates/wolf-macos/Sources/WolfKeyMap/PaneTree.swift` — faithful port of
`crates/wolf-gnome/src/layout.rs`, `import Foundation` ONLY (use `Double`, not `CGFloat`,
to avoid CoreGraphics). Public API:

- `public typealias PaneId = UInt64`
- `public enum Orientation { case horizontal, vertical }` — discriminants align with the
  FFI table: `horizontal == 0`, `vertical == 1`. `public init?(ffi: UInt8)`.
- `public enum FocusDir { case left, right, up, down }` — `0→left,1→right,2→up,3→down`.
  `public init?(ffi: UInt8)`.
- `public struct Rect: Equatable { public let x, y, w, h: Double }`
- `public indirect enum PaneLayout: Equatable { case leaf(PaneId); case split(orientation:
  Orientation, children: [PaneLayout], weights: [Double]) }`
- private `indirect enum Frame { case leaf(PaneId); case split(orientation, children:
  [Frame], weights: [Double]) }`
- `public final class PaneTree` (reference type — mutated in place), `private var root:
  Frame`, methods: `init(root:)`, `split(_:_:_:)`, `remove(_:)`, `leaves() -> [PaneId]`,
  `paneRects(width:height:) -> [(PaneId, Rect)]`, `neighbor(_:_:width:height:) -> PaneId?`,
  `layout() -> PaneLayout`.
- Free helpers ported exactly: `splitIn`, `removeIn`, `normalize` (flatten same-orientation
  child scaling grandchild weights by parent weight → collapse 1-child → re-equalize),
  `collectLeaves`, `collectRects` (Vertical divides WIDTH, Horizontal divides HEIGHT),
  `directionalDistance`, `xOverlap`, `yOverlap`, `equalize`. Constants `defaultRatio = 0.5`,
  `neighborEps = 1e-9`.
- Pinned semantics: same-orientation split inserts AFTER target then re-equalizes (DFS
  `[P1,P3,P2]`); perpendicular nests `[target,newPane]`; remove never empties; neighbor
  uses STRICTLY positive cross-axis overlap; `leaves()`/`paneRects()` stable DFS order.

### B. AppKit pane glue (in `WolfMacOS`)

`crates/wolf-macos/Sources/WolfMacOS/PaneGridView.swift` — `final class PaneGridView:
NSView`. Refactor of `GridView.draw/drawGlyph/drawCursor` for ONE specified grid. Stored:
`handle`, `metrics`, `let win: Int64`, `var boundGrid: UInt64?`, `var isActivePane: Bool`,
`var onClickFocus: ((Int64)->Void)?`, `var onResize: ((PaneGridView)->Void)?`. `isFlipped
== true`; `acceptsFirstResponder == false`. `draw(_:)`: `read_frame_for_grid(boundGrid ??
active)`, paint cells + glyphs, draw cursor only when `frame.cursor_visible()`; when
`isActivePane` stroke a 2-pt accent focus ring inset 2 pt (inside bounds, no layout cost).
`mouseDown`: `onClickFocus?(win)`. Resize override → `onResize?(self)` → map bounds→cells
→ `resize_grid(boundGrid, cols, rows)` + global `resize`. One-cell min content size so a
split subview can't starve to zero.

`crates/wolf-macos/Sources/WolfMacOS/CmdlineStripView.swift` — `final class
CmdlineStripView: NSView` (flipped), paints outer grid 1's LAST row + cursor when
`cursor.grid == 1`. `draw`: `read_frame_for_grid(1)`; if `rows()==0` return; translate up
by `(rows-1)*cellHeight` so only the last row lands in the one-row strip; draw cursor iff
`frame.cursor_visible()`. Fixed height = `ceil(cellHeight)`.

`crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift` — `final class
PaneContainerView: NSView`, analog of GNOME `PaneInner`+`PaneHandle`; the editor area's
first responder + global key sink. State: `handle`, `metrics`, `tree = PaneTree(root:
firstPaneId)`, `panes: [PaneId: PaneGridView]`, `nextPaneId`, `activePane: PaneId?`,
`splitHost: NSView` (hosts the NSSplitView tree above the strip), `cmdlineStrip`,
`onPickerTrigger: (()->Void)?`. Constants `firstPaneId=1`, `minWindowGrid=2`,
`cmdlineGridId=1`. Layout: splitHost pinned leading/trailing/top; cmdlineStrip pinned
leading/trailing/bottom fixed height; splitHost.bottom == cmdlineStrip.top. Input (moved
verbatim from `GridView.keyDown`): `acceptsFirstResponder==true`; `keyDown` checks
`PickerSeam.isPickerTrigger` → `onPickerTrigger?()` else `WolfKeyMap.translate` →
`handle.send_key`. Keys global to nvim's current window.

`onRenderWake()` — one-for-one port of `PaneInner::on_render_wake`:
1. `ensureInitialPane()` — when `panes` empty, pick initial window via Swift port of
   `pick_initial_window` (prefer `active_win()` if a real window grid ≥2 & not hidden,
   else lowest visible window grid from `windows()`); create first pane reusing
   `firstPaneId` (tree root), bind grid, `rebuildSplitTree()`, set active.
2. `applyIntents()` — `for intent in handle.drain_intents()`: `kind==SPLIT` →
   `applySplit(orientation:from:win:)` (skip if pane already exists for `win`; target =
   paneForWin(from) else active; `tree.split(target,newId,orientation)`; create pane bound
   to `win`, grid via `grid_for_win(win)` (0→pending nil); rebuild; active=newId).
   `kind==FOCUS_DIR` → `applyFocus(dir:from:)` (origin = paneForWin(from) else active;
   `tree.neighbor(origin,dir,width,height)` over splitHost.bounds; `handle.focus_win(
   targetWin)`).
3. `detectCloses()` — live grids from `windows()`; pane closed iff `boundGrid != nil &&
   !liveGrids.contains(boundGrid)`; `tree.remove(id)` + drop pane (never below 1 leaf);
   rebuild; if active closed, active = first leaf.
4. `resolvePendingGrids()` — for each pane with `boundGrid==nil`, `grid_for_win(win)`; when
   non-zero set boundGrid, `resize_grid` to current bounds, rebuild once if anything bound.
5. `updateActivePane()` — target = pane whose `win==active_win()`, else pane whose
   `boundGrid == cursor grid` (from `read_frame_for_grid(...).cursor_grid()`); update focus
   ring if changed.
6. `redrawAll()` — `setNeedsDisplay` on every pane + strip.

`rebuildSplitTree()` — analog of `rebuild_pane_area`, but N-ARY: walk `tree.layout()`; for
`.leaf(id)` return reused `panes[id]` (skip if unbound/pending, like GNOME's None);
for `.split(orientation, children, weights)` create ONE `NSSplitView`, `isVertical =
(orientation == .vertical)` (Vertical = side-by-side = isVertical true), add each built
child as arranged subview in order, set divider positions from cumulative `weights` after
layout. Detach reused pane views from previous parent before re-inserting. Replace
splitHost's single child with the freshly built root. Helpers `allocPaneId`, `paneForWin`.

### C. FFI Rust glue — new opaque types

In `crates/wolf-ffi/src/lib.rs` (follow the `FfiTreeNode` opaque pattern so `Vec<T>`
codegen works):
- `pub struct FfiLayoutIntent` — opaque; accessors `kind()->u8`, `orientation()->u8`,
  `dir()->u8`, `from()->i64`, `win()->i64`.
- `pub struct FfiWinInfo` — opaque; accessors `grid()->u64`, `win()->i64`,
  `hidden()->bool`.

### D. Tests

`crates/wolf-macos/Tests/WolfKeyMapTests/PaneTreeTests.swift` — port the full `layout.rs`
suite (see §4).
`crates/wolf-ffi/tests/panes.rs` — real-pinned-nvim integration test (see §4), reusing
`tests/common/mod.rs`.

## 3. FFI surface expansion (`crates/wolf-ffi/src/lib.rs` + regenerated `generated/`)

### 3.1 Discriminant modules (single source of truth, near `mod kind`)

```
mod intent_kind { pub const SPLIT: u8 = 0; pub const FOCUS_DIR: u8 = 1; }
mod orientation { pub const HORIZONTAL: u8 = 0; pub const VERTICAL: u8 = 1; }  // SplitOrientation order
mod focus_dir  { pub const LEFT: u8=0; pub const RIGHT: u8=1; pub const UP: u8=2; pub const DOWN: u8=3; } // FocusDir order
```
Must match Swift `Orientation.init?(ffi:)` / `FocusDir.init?(ffi:)` + `WolfKeyMapTests`.

### 3.2 `FfiLayoutIntent` (opaque, Vectorizable)

swift-bridge can't bridge a payload enum and omits `Vec` for transparent structs → model
as OPAQUE with scalar accessors, return `Vec<FfiLayoutIntent>` (like `FfiTreeNode`).
`pub struct FfiLayoutIntent { kind: u8, orientation: u8, dir: u8, from: i64, win: i64 }`,
`#[must_use]` accessors. `from_intent(LayoutIntent)->Self`:
- `Split{orientation,from,win}` → kind=SPLIT, orientation=HORIZONTAL|VERTICAL, dir=0,
  from, win.
- `FocusDir{dir,from}` → kind=FOCUS_DIR, dir=LEFT|RIGHT|UP|DOWN, from, orientation=0,
  win=0.
Bridge `extern "Rust"` block declares the type + the 5 accessors.
`WolfCoreHandle::drain_intents(&self) -> Vec<FfiLayoutIntent>` maps
`core.drain_intents()` through `from_intent`; empty Vec when core gone.

### 3.3 `FfiWinInfo` (opaque, Vectorizable)

`pub struct FfiWinInfo { grid: u64, win: i64, hidden: bool }`, accessors `grid/win/hidden`.
`WolfCoreHandle::windows(&self) -> Vec<FfiWinInfo>` maps `snapshot.windows.values()` —
return ALL (let Swift filter, mirroring GNOME reading `snap.windows`).

### 3.4 `grid_for_win` / `active_win` / `focus_win` / `resize_grid` on `WolfCoreHandle`

- `grid_for_win(&self, win: i64) -> u64` = `snapshot.grid_for_win(win).unwrap_or(0)`.
- `active_win(&self) -> i64` = `snapshot.active_win.unwrap_or(0)`.
- `focus_win(&self, win: i64)` = `core.focus_win(win)`.
- `resize_grid(&self, grid: u64, cols: u16, rows: u16)` = `core.resize_grid(...)`.
Add to the existing `WolfCoreHandle` bridge block; `#[must_use]` on the two getters.

### 3.5 Per-grid frame read + cursor-grid reporting

- `WolfCoreHandle::read_frame_for_grid(&self, grid: u64) -> FfiFrame` →
  `FfiFrame::from_snapshot_grid(&snapshot, grid)`.
- Refactor `FfiFrame::from_snapshot(snap) == from_snapshot_grid(snap,
  snap.active_grid_id())`. `from_snapshot_grid`: resolve grid; if absent return
  `FfiFrame::empty()` (Swift treats `cols()==0`/`rows()==0` as "grid gone"); pack that
  grid's cells row-major; set `cursor_visible = (snapshot.cursor.grid == grid_id)`
  (per-grid); cursor reverse-video colors computed against the REQUESTED grid's cell under
  the cursor (generalize `cursor_colors` to take the grid; irrelevant when not visible);
  set new field `grid_id: u64`.
- Add `FfiFrame::grid_id(&self)->u64` + `cursor_grid(&self)->u64` (= `snapshot.cursor.grid`
  at pack time) accessors + struct fields; add to the `FfiFrame` bridge block.
- SUBTLETY: the current `cursor_colors` reads `snap.active_grid()` — it MUST be generalized
  to read the REQUESTED grid's cell under the cursor, else a pane's cursor colors compute
  against the wrong grid.

### 3.6 swift-bridge uncertainty + fallback

Opaque+`Vec` is proven here (`FfiTreeNode`/`FfiFileEntry`/`FfiCell`), so
`Vec<FfiLayoutIntent>`/`Vec<FfiWinInfo>` generate cleanly. Do NOT use a transparent struct
in a `Vec`. If a `u64`/`i64` opaque accessor mis-generates, fall back to `u32` hi/lo or
free fns — not expected. `read_frame_for_grid` returning owned opaque `FfiFrame` matches
`read_frame`.

### 3.7 Wake header — NO new wake

The pane container is driven by the EXISTING editor wake (`wolf_ffi_set_wake`);
`generated/wolf_ffi_wake.h` does NOT change. Only `WakeBridge.swift` gains a call to the
pane view's `onRenderWake()`.

### 3.8 Regenerate + commit

After editing `src/lib.rs`: `bazel run //crates/wolf-ffi:gen_bindings`; commit
`generated/SwiftBridgeCore.swift`, `generated/wolf-ffi.swift`, `generated/wolf-ffi.h` (and
`SwiftBridgeCore.h` if changed). Hand-written `module.modulemap` / `wolf_ffi_wake.h` are
not overwritten.

## 4. Test plan

### 4.1 Swift `PaneTreeTests.swift` (headless, in `WolfKeyMapTests`)

Port every `layout.rs` test 1:1 (XCTest): ids `P1=1…P4=4`, bounds `W=100,H=40`, same
half/third/quarter consts + `approx`/`assertRect`/`rectsOverlap`/`rectFor` helpers.
Cases: construction (`new_is_single_leaf`); split
(`split_vertical_two_leaves`,`split_then_pane_rects_side_by_side`,`split_horizontal_stacked`,
`nested_split_rects`); n-ary flatten (`vsplit_flattens_into_row`,
`vsplit_twice_stays_flat_four_equal`,`hsplit_of_row_member_nests`); remove/collapse/merge
(`remove_collapses_sibling`,`remove_collapses_sibling_subtree`,`remove_last_is_noop`,
`remove_re_equalizes_row`,`remove_collapses_single_child`,
`remove_collapses_and_merges_same_orientation`); neighbor (`neighbor_basic_vertical`,
`neighbor_basic_horizontal`,`neighbor_nested`,`neighbor_unknown_pane_is_none`,
`neighbor_flat_row_of_three`); determinism (`leaves_and_rects_order_is_stable`). Plus a
small test that `Orientation.init?(ffi:)`/`FocusDir.init?(ffi:)` map `0/1` & `0/1/2/3` to
the right cases (guards the discriminant contract with wolf-ffi).

### 4.2 Rust `crates/wolf-ffi/tests/panes.rs` (real pinned nvim)

Modeled on `tree_search.rs`/`file_search.rs`: `mod common;`, loud SKIP via
`common::nvim_bin_or_skip()`, `init_lua_path()`, fresh temp cwd, `wolf_ffi_start(init_lua,
cwd, 80, 24)`, `FLUSH_TIMEOUT=10s`, `POLL_INTERVAL=50ms`. Sequence:
1. Poll `read_frame()` until `cols()>0`.
2. Baseline: `windows()` has exactly one `FfiWinInfo` with `grid()>=2`; record `win`.
   `active_win()` nonzero == that win. `grid_for_win(win)` == that grid.
   `read_frame_for_grid(grid).cols()>0`.
3. Trigger a vsplit. PREFER `:vsplit<CR>` characters via `send_key` for determinism (the
   Ex-command path also emits the Split intent); `<C-w>v` is also acceptable. Poll
   `drain_intents()` until a `kind()==SPLIT` intent appears; assert `orientation()==VERTICAL
   (1)`, `from()==originalWin`, `win()` new nonzero distinct from `from()`.
4. Poll `windows()` until TWO window grids; new `win` present, `grid_for_win(newWin)`
   nonzero & distinct; `read_frame_for_grid(newGrid).cols()>0`.
5. `resize_grid(newGrid,40,20)`; poll `read_frame_for_grid(newGrid)` until `cols()==40`
   (nvim authoritative). Prefer exact; if flaky headless, assert `cols()>0 && cols()<=40`.
6. `focus_win(originalWin)`; poll `active_win()` until == originalWin.
7. Close from the focused window (`:close<CR>` or `<C-w>c`); poll `windows()` until ONE
   grid again and `grid_for_win(closedWin)==0` (close observed via vanish; no close intent).
8. `read_frame_for_grid(1).rows()>0` (outer grid present for the strip).
9. `handle.shutdown()`, drop tmp.
The test links the rlib, so `wolf_ffi::FfiKeyEvent` is nameable for building key events.

### 4.3 AppKit glue — not headlessly testable (compile + manual smoke)

Eyeball checklist (add to `crates/wolf-macos/README.md`): launch = one editor pane + thin
bottom strip, typing reaches nvim, editor holds first responder; `:vsplit`/`<C-w>v` = two
EQUAL side-by-side panes both live (pre-existing pane NOT blanked — the n-ary regression),
third vsplit = three equal columns; `:split`/`<C-w>s` = stacked; `<C-w>h/j/k/l` = focus
ring + cursor move to geometric neighbour; click a pane = active + cursor + typing there;
drag divider resizes, content reflows, window-resize re-equalizes (v1 parity); `:close`/
`<C-w>c` = pane disappears, sibling reclaims, survivors stay live; cmdline mode (`:`,`/`)
shows in the BOTTOM STRIP with editor panes STILL VISIBLE (no whole-view swap), press-ENTER/
messages truncated to one row (v1 limitation); picker (Shift+Ctrl+P) still overlays the
editor + refocuses on close; file-tree opens into the active pane + refocuses.

### 4.4 Bazel targets that must pass (macOS host)

`//crates/wolf-core:wolf_core_lib_test`; `//crates/wolf-ffi:{bridge_smoke,file_search,
tree_search,panes(new),frame_pack}`; `//crates/wolf-macos:{WolfKeyMapTests,
bundle_layout_test,Wolf}` (Wolf must still build).

## 5. Files to modify

### Rust / FFI
`crates/wolf-ffi/src/lib.rs` — discriminant modules; `FfiLayoutIntent`+`FfiWinInfo` opaque
types + bridge blocks; `WolfCoreHandle` methods `drain_intents`,`windows`,`grid_for_win`,
`active_win`,`focus_win`,`resize_grid`,`read_frame_for_grid`; `FfiFrame` gains
`grid_id`+`cursor_grid` + `from_snapshot_grid` (with `from_snapshot` delegating). Update
crate-doc surface list. Import `LayoutIntent`,`SplitOrientation`,`FocusDir` from
`wolf_core`.
`crates/wolf-ffi/generated/*` — regenerated, commit.

### Swift / AppKit
`GridView.swift` — REMOVE (paint→`PaneGridView`; keyDown+resize→`PaneContainerView`).
`wolf_macos_lib` srcs are GLOBBED → no BUILD change for add/remove of WolfMacOS files.
`EditorSplitViewController.swift` — trailing content item hosts `PaneContainerView`;
`init(treeView:paneContainer:)` (or editor root NSView); `HostViewController(...
pinTopToSafeArea:true)` unchanged.
`MainWindow.swift` — convenience+designated init take the `PaneContainerView`. SIZING:
keep editor pane exactly cols×rows cells: `editorHeight = rows*cellHeight +
cmdlineStripHeight` (strip is extra), preserve 80-col width. `initialFirstResponder =
paneContainer`.
`AppDelegate.swift` — build `PaneContainerView(handle:metrics:)`; register with
`WakeBridge.registeredPaneView`; pass pane container to FileTreeView/PickerView/MainWindow/
EditorSplitViewController; add picker as subview of the pane container editor area + set
`paneContainer.onPickerTrigger`; `makeFirstResponder(paneContainer)`.
`WakeBridge.swift` — `weak static var registeredView: GridView?` →
`registeredPaneView: PaneContainerView?`; thunk calls `registeredPaneView?.onRenderWake()`
THEN `registeredTree?.onRenderWake()`.
`PickerView.swift` — `gridView` ref → `PaneContainerView` (only for makeFirstResponder on
close). Mechanical.
`FileTreeView.swift` — same mechanical `gridView` → `PaneContainerView` rename (refocus
editor after open).

### Bazel
`crates/wolf-macos/BUILD.bazel` — `wolf_key_map` srcs += `Sources/WolfKeyMap/PaneTree.swift`;
`WolfKeyMapTests` srcs += `Tests/WolfKeyMapTests/PaneTreeTests.swift`. `wolf_macos_lib`
globbed (no change).
`crates/wolf-ffi/BUILD.bazel` — add `panes` rust_test mirroring `tree_search` exactly
(`tags=["no-sandbox"]`, `size="small"`, `data`=nvim_tree+init.lua, `env`=
WOLF_TEST_INIT_LUA/WOLF_TEST_NVIM_DIR, `deps`=`[":wolf_ffi"]+all_crate_deps(normal_dev=
True)`, `srcs`=`["tests/common/mod.rs","tests/panes.rs"]`, `crate_root="tests/panes.rs"`).

## 6. Implementation order (no compile breakage)

1. FFI Rust glue (§3) in `wolf-ffi/src/lib.rs`; build `:wolf_ffi` + `:frame_pack`.
2. `bazel run //crates/wolf-ffi:gen_bindings`; commit `generated/`.
3. `tests/panes.rs` + `panes` BUILD target; run `bazel test //crates/wolf-ffi:panes` (+
   re-run bridge_smoke/file_search/tree_search/frame_pack).
4. `WolfKeyMap/PaneTree.swift` + `PaneTreeTests.swift` + BUILD wiring;
   `bazel test //crates/wolf-macos:WolfKeyMapTests`.
5. `PaneGridView.swift` + `CmdlineStripView.swift` (keep `GridView.swift` for now).
6. `PaneContainerView.swift`.
7. Rewire `EditorSplitViewController`/`MainWindow`/`AppDelegate`/`WakeBridge`/`PickerView`/
   `FileTreeView`; DELETE `GridView.swift` last. Build `:wolf_macos_lib`.
8. `bazel build //crates/wolf-macos:Wolf`; `bundle_layout_test`.
9. Docs + prettier.
10. Manual smoke (§4.3).

## 7. Dependencies & lint

No new deps (no Cargo repin, no new Swift package). Strict clippy: doc comment +
`#[must_use]` on every new value-returning non-`&mut` public item (all FfiLayoutIntent/
FfiWinInfo accessors, `grid_for_win`/`active_win`/`windows`/`drain_intents`/
`read_frame_for_grid`, `FfiFrame::grid_id`/`cursor_grid`); `focus_win`/`resize_grid` return
`()` (no must_use). Keep discriminants in `mod intent_kind`/`orientation`/`focus_dir`. The
`#[swift_bridge::bridge] mod ffi` carries `allow(clippy::all, missing_docs, unused)` so
generated shims stay under the bar; hand impls at normal bar. No new `unsafe` (no new wake)
→ no new `# Safety`. Swift `PaneTree.swift` imports ONLY Foundation (no AppKit/CoreGraphics)
so it stays in the headless `wolf_key_map` test bundle; `Double` not `CGFloat`.

## 8. Docs to update (then prettier --write)

- `architecture.md` — macOS Phase status (~59–67): native panes (n-ary NSSplitView from
  Swift `PaneTree` port) + bottom cmdline strip realized; REMOVE the "paints only active
  grid / no cmdline strip / deferred" paragraph; move panes+cmdline out of macOS deferred
  list. In the core↔front-end contract / wolf-ffi section note the added FFI surface
  (`read_frame_for_grid`, `drain_intents`, `windows`, `grid_for_win`, `active_win`,
  `focus_win`, `resize_grid`, `FfiLayoutIntent`, `FfiWinInfo`, `FfiFrame.grid_id`/
  `cursor_grid`). Carry the shared "Split panes — known v1 limitations" (~197–204) to
  macOS. Note macOS maps each `PaneLayout.split` to ONE n-ary NSSplitView (vs GNOME's
  binary `gtk::Paned` chain).
- `crates/wolf-ffi/README.md` + `crates/wolf-ffi/src/README.md` — new bridge surface, the
  discriminant tables (single source of truth shared with Swift PaneTree), per-grid
  `read_frame_for_grid` + `FfiFrame.grid_id`/`cursor_grid`/per-grid `cursor_visible`, the
  new `:panes` test target. Note `FfiLayoutIntent`/`FfiWinInfo` opaque (Vectorizable) like
  `FfiTreeNode`.
- `crates/wolf-macos/README.md` + `crates/wolf-macos/Sources/README.md` — new Swift files
  (`WolfKeyMap/PaneTree.swift`; `WolfMacOS/PaneGridView.swift`,`PaneContainerView.swift`,
  `CmdlineStripView.swift`), removal of `GridView.swift`, `WakeBridge.registeredPaneView`
  rename + pane `onRenderWake()`, `EditorSplitViewController`/`MainWindow` taking the pane
  container, updated Known limitations (native panes + cmdline strip landed; carry shared
  v1 limitations; add the manual-smoke checklist). macOS at parity with GNOME (n-ary
  NSSplitView).
- `crates/wolf-macos/Tests/README.md` — mention `PaneTreeTests.swift`.
- `crates/wolf-core/src/README.md` — NO change. GNOME docs — NO change.

## 9. Status checklist (coder)

- [ ] FFI glue + per-grid refactor; build green; `frame_pack` passes.
- [ ] regen bindings; commit `generated/`.
- [ ] `tests/panes.rs` + `:panes` target; FFI tests green.
- [ ] `PaneTree.swift` + `PaneTreeTests.swift`; `WolfKeyMapTests` green.
- [ ] `PaneGridView`/`CmdlineStripView`/`PaneContainerView`.
- [ ] rewire shell; delete `GridView.swift`.
- [ ] `Wolf` builds; `bundle_layout_test` green.
- [ ] docs + prettier.
- [ ] manual smoke.
