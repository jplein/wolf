# Plan: macOS mouse + scroll + synthetic scrollbars (wolf-macos)

## Problem statement

Make the Neovim editor panes on macOS feel native by adding three input/UI features
(all currently deferred on BOTH front-ends):

1. **Full Neovim mouse support** in panes — click positions the cursor, click-drag
   selects (visual mode), via `nvim_input_mouse`. Today only click-to-focus exists.
2. **Scroll wheel / trackpad** scrolling up/down within a pane, forwarded to Neovim.
3. **Synthetic scrollbars** per pane, driven by `win_viewport`
   (topline/botline/curline/linecount), at the PANE's right edge, following the macOS
   "Show scroll bars" setting (`NSScroller.preferredScrollerStyle`: `.legacy` = always;
   `.overlay` = on-scroll then fade; react to
   `NSScroller.preferredScrollerStyleDidChangeNotification`).

## Load-bearing decisions (made with the user)

- **Architect for a future "max column width + centered grid" but DEFER the capping.**
  Introduce a per-pane geometry abstraction with a distinct **GRID RECT** (where the
  Neovim grid is drawn) inside the **PANE RECT** (today grid rect == pane rect). ALL
  coordinate mapping — pixel→cell (mouse), scroll, scrollbar placement, paint offset —
  goes through this geometry so future centering is a clean drop-in. The **SCROLL handler
  maps from the whole PANE RECT** (not the grid rect), so scrolling with the pointer in
  the future side margin still scrolls the pane (explicit requirement).
- **Scrollbar at the PANE's right edge** (out in the right margin once centered).

## Key technical findings (verified in code)

- `wolf-core` has `MouseEvent`/`MouseButton`/`MouseAction`, `ScrollEvent`/
  `ScrollDirection`, `InputEvent::Mouse`/`Scroll`, `apply_mouse`/`apply_scroll`; the
  captive `runtime/nvim/init.lua` sets `opt.mouse = "a"`. BUT `nvim_input_mouse`
  HARDCODES `grid = 0` and `MouseEvent`/`ScrollEvent` carry NO grid id. Under our
  independent multigrid pane layout (front-end owns geometry, NOT nvim's `win_pos`),
  mouse/scroll MUST target the pane's WINDOW GRID (`boundGrid` >= 2) with GRID-LOCAL
  row/col → **core change**: add `grid: u64` to `MouseEvent`/`ScrollEvent`, thread it
  through `apply_mouse`/`apply_scroll`/`nvim_input_mouse`. Safe (never wired to a
  front-end).
- `WolfCore::send_input(InputEvent)` is public; `focus_win`/`resize`/`resize_grid` are
  thin convenience wrappers → add `send_mouse`/`send_scroll` in that style.
- Viewport per window: `WinInfo.viewport` (`Viewport { topline, botline, curline,
  linecount }`, render.rs). Scrollbar needs it per grid over the FFI.
- FFI (`wolf-ffi/src/lib.rs`) exposes `send_key`, `read_frame_for_grid`, `resize`,
  `resize_grid`, `focus_win`, `drain_intents`, `windows`, `grid_for_win`, `active_win`.
  ADD: a mouse-send (button/action/mods/grid/row/col) + scroll-send (direction/grid/
  row/col), and viewport accessors on `FfiFrame` (topline/botline/curline/linecount,
  from `snapshot.windows[grid].viewport` in `from_snapshot_grid`). Mirror the existing
  transparent-struct + discriminant-table pattern (like `FfiKeyEvent`); add
  button/action/direction discriminant tables as the single source of truth shared with
  Swift. Regenerate committed `generated/`.
- Swift: `PaneContainerView` is the single flat painting view with `mouseDown`
  (click-to-focus only). Add the per-pane geometry; `mouseDown`/`mouseDragged`/`mouseUp`
  → map pane-local point → (grid, row, col) → send press/drag/release (left; keep
  focus-on-click); `scrollWheel` → map from PANE rect → accumulate `deltaY` into discrete
  line steps → send scroll up/down; draw synthetic scrollbars at each pane's right edge
  from the viewport, honoring `NSScroller.preferredScrollerStyle`. Keyboard stays global.

## Project policy

- Real-Neovim integration tests over mocks: the new FFI surface (mouse/scroll round-trip +
  viewport) covered by a `//crates/wolf-ffi:*` test (send a mouse press → cursor moves;
  scroll → topline changes via viewport accessors). Pure Swift seams (pixel→cell mapping,
  scroll-delta accumulator) headlessly unit-tested in `WolfKeyMapTests`. AppKit event
  wiring + scrollbar drawing = compile + manual-smoke only.
- Update every affected README (wolf-core, wolf-ffi + src, wolf-macos + Sources + Tests,
  architecture.md — move mouse/scroll/scrollbars out of deferred lists) + prettier.
- GNOME crate not buildable here; do NOT touch it. The lead verifies live GUI rendering
  (build + run + screenshot the .app) before sign-off.

## Status: COMPLETE ✅ — all tests green + LIVE-GUI VERIFIED

### Lead live-GUI verification (real Quartz CGEvents, screenshotted the built .app)
- MOUSE CLICK positions the cursor: opened README.md (cursor top-left), real
  `CGEventLeftMouseDown/Up` at a mid-editor point → cursor jumped to the clicked cell. ✅
  (System Events `click at` did NOT work — it's an AXPress, not a HID event; needed a real
  Quartz CGEvent, compiled via `xcrun -sdk macosx swiftc` with the Nix SDK env cleared.)
- SCROLL WHEEL scrolls: opened architecture.md, real `CGEventScrollWheelEvent` (5 line
  events) → content scrolled from top ("# Architecture") down to the wolf-gnome section. ✅
- SYNTHETIC SCROLLBAR renders: overlay-style rounded thumb at the pane's right edge,
  positioned near the top (correct for ~15 lines into a long file), visible during scroll. ✅
- Scrollbar setting is Automatic → overlay (trackpad); drag-to-select covered by the FFI
  mouse_scroll test (DRAG/RELEASE) + code review (mouseDragged wiring) — recommend user
  hands-on drag-select + scroll-direction feel check.
No diag cruft in repo. Nothing committed (standing rule).

## Status (history): DOCS DONE + prettier + all tests green → LEAD LIVE-GUI VERIFICATION

### Docs synced (technical-writer) + prettier (lead)
8 docs updated (wolf-core README + src, wolf-ffi README + src, wolf-macos README + Sources +
Tests, architecture.md): mouse/scroll grid-targeted, send_mouse/send_scroll, FfiMouseEvent/
FfiScrollEvent + 3 discriminant tables, FfiFrame viewport accessors (0-based), :mouse_scroll
target, PaneGeometry/ScrollAccumulator seams, scrollbar style behavior, mouse/scroll out of
macOS deferred list; GNOME untouched. NIT fixed: the two lib.rs `topline` "1-based" doc
comments → 0-based. prettier clean (all already conformant). Final sweep: all 9 targets PASS,
Wolf builds.



### Review result — APPROVED (no BLOCKER/SHOULD-FIX)
Core grid-threading + wrappers, FFI discriminants (verified vs wolf_core enum order + Swift
consts + test consts), FfiMouseEvent/FfiScrollEvent + mappers + FfiFrame viewport, 0-based
scrollbar math underflow-safe (Double before subtraction, offsetFrac=topline/total, no
`-1`), Swift seams Foundation-only + clamped, AppKit mouse/scroll/scrollbar wiring correct
(weak-self fade timer, deinit cleanup, single direction-flip point), tests not weakened,
no wolf-gnome changes. NIT (→ technical-writer): two `topline` "1-based" doc comments in
`crates/wolf-ffi/src/lib.rs` (~line 394 bridge decl comment, ~line 1310 FfiFrame::topline
accessor doc) still contradict the corrected 0-based reality — fix to 0-based.



### Final test state
All pass: wolf_core_lib_test; wolf-ffi {bridge_smoke, file_search, tree_search, panes,
frame_pack, mouse_scroll}; wolf-macos {WolfKeyMapTests, bundle_layout_test}; Wolf builds.
`mouse_scroll` PASSES end-to-end — grid-targeted mouse confirmed (exact cursor_row()==5 &&
cursor_col()==2), topline 0-based baseline, scroll DOWN↑/UP↓. Scrollbar math 0-based +
underflow-safe; render.rs Viewport doc corrected to 0-based.

## Status (history): code fixes done; ONE more test-content fix (invalid Vimscript) then re-run

### Coder fixes done
Scrollbar math now 0-based + underflow-safe (Double before any subtraction; offsetFrac =
topline/total); render.rs Viewport doc corrected to 0-based. All targets PASS except
mouse_scroll, which now fails EARLIER: the test-writer's `:put =map(range(1,80),
'printf("%05d", v:val)')` is INVALID under `:put =<expr>` — nvim strips the literal `"` as
an Ex comment (E115), buffer never fills (linecount stays 1). Coder confirmed the `"` IS
delivered (FFI path fine); the Vimscript is just wrong in this Ex context.
→ ORCHESTRATOR DECISION: test-writer applies the coder's Option A (double-quote-free):
  `:put =map(range(1,80), 'printf(''%05d'', v:val)')` (doubled single-quotes, no `"`; Rust
  string needs no escaping). Yields 5-char lines so the exact (row5,col2) assertion holds.
  Then coder re-runs //crates/wolf-ffi:mouse_scroll.

## Status (history): IMPLEMENTED; 2 empirical findings → test-writer fix + coder fix pass

### Implementation result (coder) — all targets pass EXCEPT mouse_scroll (test assertions)
Core grid-threading + send_mouse/send_scroll; FFI discriminant mods + FfiMouseEvent/
FfiScrollEvent + send glue + mappers + FfiFrame viewport accessors + from_snapshot_grid
populate; bindings regenerated (wake.h unchanged); :mouse_scroll target added; Swift
PaneGeometry + ScrollAccumulator seams + AppKit mouse/scroll/scrollbars wired. Verbatim:
wolf_core_lib_test, bridge_smoke, file_search, tree_search, panes, frame_pack,
WolfKeyMapTests, bundle_layout_test → PASS; Wolf builds. mouse_scroll FAILS on test
assumptions (below). swift-bridge: FfiMouseEvent/FfiScrollEvent pass by value (intoFfiRepr,
like send_key); viewport accessors plain scalars.

### Empirical findings + ORCHESTRATOR DECISIONS
1. **`topline` is 0-BASED** (confirmed: after `:1`, topline()==0, linecount()>=80 real).
   → TEST FIX: baseline `topline()==0` (keep relative DOWN↑/UP↓ asserts).
   → CODE FIX (coder): scrollbar offset must be 0-based + underflow-safe — the plan's
     `(topline-1)/total` UNDERFLOWS UInt64 at topline==0 (top of every buffer). Use
     `Double(topline)/total`; do all viewport subtractions (botline-topline) in a
     non-underflowing way (convert to Double/Int first).
   → DOC FIX (coder): `crates/wolf-core/src/render.rs` `Viewport::topline` doc says
     "1-based" — change to 0-based (and note for technical-writer to check READMEs).
2. **cursor_col clamps to EOL** because `:put =range(1,80)` lines are 1 char wide.
   → TEST FIX (do NOT weaken to row-only): use WIDE content (e.g. zero-padded/`printf`
     or `repeat` lines ≥ ~6 chars) so clicking grid (row 5, col 2) lands a real column and
     the EXACT `cursor_row()==5 && cursor_col()==2` assertion holds (verifies column
     targeting). Coder confirmed grid-targeting + row mapping work; col is the open item.
3. Scroll sign confirmed correct (DOWN↑topline / UP↓topline at the core level).



### Tests written (test-writer)
- `crates/wolf-ffi/tests/mouse_scroll.rs` — real-nvim `mouse_scroll_click_drag_and_wheel`
  (modeled on panes.rs, reuses char_key/enter_key/send_ex_command/live_windows): put 80
  lines → click(5,2) cursor moves → drag/release(8,4) responsive → scroll DOWN topline↑ →
  scroll UP topline↓. Exact PRESS assert `cursor_row()==5 && cursor_col()==2` (fallback
  row-only commented); DRAG/RELEASE loose (no-panic + soft cursor note). Coder adds the
  `//crates/wolf-ffi:mouse_scroll` BUILD target.
- `crates/wolf-macos/Tests/WolfKeyMapTests/PaneGeometryTests.swift` (6 cases) +
  `ScrollAccumulatorTests.swift` (5 cases) — headless; coder adds the seams + wires srcs.

### Coder-verify-empirically (from test-writer)
- `:put =range(1,80)` landing 80 lines + `linecount()` reflecting it (`:`-cmd path proven by
  panes.rs; `=range()` chars are the new part).
- nvim_input_mouse grid-targeting + the row/col base: test assumes FFI row/col (0-based
  grid-local) == `cursor_row()/cursor_col()`. Plan §9 empirical gate — resolve here.
- Scroll sign: DOWN↑topline / UP↓topline; if reversed it's a mapper bug, not a test bug.



---

# AUTHORITATIVE ARCHITECT PLAN

## 1. Summary

Add mouse (click positions cursor, drag selects, release ends), scroll wheel/trackpad
up/down, and per-pane synthetic scrollbars (driven by `win_viewport`, at the pane's right
edge, following `NSScroller.preferredScrollerStyle`) to `PaneContainerView`. Thread a real
`grid` id through the core mouse/scroll path (today `nvim_input_mouse` hardcodes `grid=0`),
expose per-grid viewport over the FFI, add a mouse/scroll bridge surface, and add two
headlessly-tested pure Swift seams (`PaneGeometry`, `ScrollAccumulator`). All pixel↔cell
mapping routes through a per-pane **GRID RECT inside PANE RECT** abstraction so a future
max-column-width/centered-grid is a clean drop-in — ARCHITECTED-FOR, NOT IMPLEMENTED (cap
deferred; `gridRect == paneRect` today). Scroll maps from the whole PANE RECT. Scrollbar at
the PANE's right edge; thumb-drag deferred (non-interactive v1). (Go-lint guidance from the
template is N/A — repo is Rust+Swift.)

## 2. Core (`wolf-core`)

### 2.1 `src/input.rs`
- `MouseEvent`: add `pub grid: u64`. `ScrollEvent`: add `pub grid: u64`.
- `apply_mouse`: call `nvim_input_mouse(nvim, button, action, &modifier, ev.grid, ev.row,
  ev.col)`. `apply_scroll`: `nvim_input_mouse(nvim, "wheel", action, "", ev.grid, ev.row,
  ev.col)` (scroll modifiers stay `""`).
- `nvim_input_mouse`: add `grid: u64` param between `modifier` and `row`; replace
  `Value::from(0_u64)` with `Value::from(grid)`. Fix the doc (no longer hardcoded 0).
- Grep confirms `MouseEvent {`/`ScrollEvent {` only constructed in input.rs struct defs.
- UNCERTAINTY: `nvim_input_mouse` grid-targeting under ext_multigrid (grid>=2 + grid-local
  row/col) is intended but never exercised — the FFI test (§6.1) is the empirical gate; if
  it misbehaves, note in test comments (grid-targeting strongly preferred, no global coords
  available under our independent layout).

### 2.2 `src/api.rs`
- Add `pub fn send_mouse(&self, ev: MouseEvent) { self.send_input(InputEvent::Mouse(ev)); }`
  and `pub fn send_scroll(&self, ev: ScrollEvent) { self.send_input(InputEvent::Scroll(ev));
  }` near `focus_win`. Add `MouseEvent, ScrollEvent` to the `use crate::input::{...}` import.

### 2.3 Viewport — already present
`snapshot.windows.get(&grid).map(|w| w.viewport)` yields `Viewport{topline,botline,curline,
linecount}`. No core change for the FFI to read it.

## 3. FFI (`crates/wolf-ffi/src/lib.rs`)

### 3.1 Discriminant modules (single source of truth, beside `mod kind`/`intent_kind`/…)
`mod mouse_button { LEFT=0, RIGHT=1, MIDDLE=2 }`; `mod mouse_action { PRESS=0, RELEASE=1,
DRAG=2 }`; `mod scroll_dir { UP=0, DOWN=1, LEFT=2, RIGHT=3 }` (each `pub const NAME: u8 = N;`
+ `///`). Mirror `wolf_core::MouseButton`/`MouseAction`/`ScrollDirection` variant order.

### 3.2 Transparent structs (inside `mod ffi`, like `FfiKeyEvent`)
`#[swift_bridge(swift_repr="struct")] struct FfiMouseEvent { button: u8, action: u8, grid:
u64, row: u32, col: u32, ctrl: bool, shift: bool, alt: bool }`; `struct FfiScrollEvent {
direction: u8, grid: u64, row: u32, col: u32 }`. Add to `WolfCoreHandle` extern block:
`fn send_mouse(&self, ev: FfiMouseEvent);`, `fn send_scroll(&self, ev: FfiScrollEvent);`.
Add to `FfiFrame` extern block: `fn topline/botline/curline/linecount(&self) -> u64;`.
Re-export: `pub use ffi::{FfiColor, FfiKeyEvent, FfiMouseEvent, FfiScrollEvent};`.

### 3.3 `FfiFrame` viewport fields
Add `topline/botline/curline/linecount: u64` fields + `#[must_use]` accessors;
`empty()` sets all 0; in `from_snapshot_grid` read `let vp = snapshot.windows.get(&grid_id)
.map(|w| w.viewport).unwrap_or_default();` and populate. `from_snapshot` delegates →
`read_frame` unchanged; `frame_pack` (no windows) reads 0 (optional new assert
`topline()==0`).

### 3.4 Glue + mappers (full docs, `#[must_use]` on accessors)
`send_mouse`/`send_scroll` impls: map via `mouse_event_from_ffi`/`scroll_event_from_ffi`
(return `None` on bad discriminant), then `if let Some(core)=self.core.borrow().as_ref()
{ core.send_mouse(m) }`. Mappers per architect spec (match discriminant consts → core enum,
`_ => return None`; build `MouseEvent`/`ScrollEvent` with `grid/row/col` + mods). Add
`use wolf_core::{MouseButton, MouseAction, MouseEvent, ScrollDirection, ScrollEvent}`.

### 3.5 Regenerate
`bazel run //crates/wolf-ffi:gen_bindings`; commit `generated/wolf-ffi.swift`/`.h`.
`wolf_ffi_wake.h` MUST stay unchanged (no new wake). swift-bridge transparent-struct-arg is
proven by `send_key`; fallback = scalar args (not expected).

## 4. Swift pure seams (`WolfKeyMap`, Foundation-only, `Double` not `CGFloat`)

### 4.1 `Sources/WolfKeyMap/PaneGeometry.swift` (new)
`public struct PaneGeometry` with `paneRect, gridRect: Rect` (reuse `Rect` from
PaneTree.swift), `cellWidth/cellHeight: Double`, `cols/rows: Int`. `init(paneRect:
cellWidth: cellHeight: maxCols: Int? = nil)` — `maxCols==nil` ALWAYS today → `gridRect ==
paneRect`; future cap centers a narrower gridRect (guarded, untested, pass `nil`
everywhere). `cols = max(1, floor(gridRect.w/cellWidth))`, `rows` likewise. `paintOffsetX/Y
= gridRect.{x,y} - paneRect.{x,y}` (0 today). `cellAt(x:y:) -> (row, col)` CLAMPED into
`[0,cols-1]×[0,rows-1]` (margin/edge → nearest edge cell): `col = clamp(floor((x-gridRect.x)
/cellWidth), 0, cols-1)`, `row` likewise.

### 4.2 `Sources/WolfKeyMap/ScrollAccumulator.swift` (new)
`public struct ScrollAccumulator` with private signed `remainder: Double`. `mutating func
consume(deltaY: Double, cellHeight: Double) -> Int` (guard cellHeight>0 → 0; `remainder +=
deltaY`; `while remainder >= cellHeight { steps+=1; remainder-=cellHeight }`; `while
remainder <= -cellHeight { steps-=1; remainder+=cellHeight }`; return signed steps).
`mutating func reset()`. Direction-agnostic (signed); AppKit maps sign → direction.

### 4.3 Headless tests (wire into `WolfKeyMapTests` srcs; seams into `wolf_key_map` srcs)
`PaneGeometryTests.swift`: gridRect==paneRect today + paintOffset 0; cols/rows from cell
size (800×480 @8×16 → 100×30; sub-cell → 1×1); cellAt interior; cellAt clamps left/top
margin → (0,0); clamps right/bottom edge → (rows-1,cols-1); clamps with non-zero pane
origin. `ScrollAccumulatorTests.swift`: whole cell → ±1; sub-cell accumulates (0 then 1);
remainder carries across calls; sign change reverses (per documented signed-remainder
values: +1.5h→1 rem+0.5h, then -2h→ rem-1.5h→ -1); zero cellHeight → 0.

## 5. AppKit wiring (`PaneContainerView.swift`) — compile + manual-smoke

### 5.1 Geometry lookup + refactor (no behavior change today)
Helper `geometry(for rect: CGRect) -> PaneGeometry` (maxCols: nil). `paintGrid`: translate
by `rect.minX + geo.paintOffsetX, rect.minY + geo.paintOffsetY` (clip stays the pane rect).
`syncGridSizes`: per-pane target from `geo.cols/geo.rows` (today identical); grid-1/global
path stays on `gridCells(bounds)`.

### 5.2 Mouse press/drag/release
State `mouseDragGrid: UInt64?`. Mods from `event.modifierFlags` (ctrl/shift/alt; no meta).
`mouseDown`: hit-test pane → focus_win+setActive+needsDisplay (keep click-to-focus); `(row,
col)=geo.cellAt(point)`; `send_mouse(FfiMouseEvent(button:LEFT, action:PRESS, grid, row,
col, mods))`; `mouseDragGrid=grid`. `mouseDragged`: if set, look up that grid's pane rect,
map (clamped) → `action:DRAG`. `mouseUp`: if set, map → `action:RELEASE`; clear
`mouseDragGrid`. (nvim `mouse=a` makes press→drag→release = cursor + visual selection.)

### 5.3 Scroll
State `scrollAccumulator = ScrollAccumulator()`, `lastScrollGrid: UInt64?`. `scrollWheel`:
hit-test from the PANE rect (margin-scroll-ready); if grid != lastScrollGrid → reset accum +
update. `(row,col)=geo.cellAt(...)`. `let dy = event.hasPreciseScrollingDeltas ?
Double(event.scrollingDeltaY) : Double(event.scrollingDeltaY)*cellHeight`. `steps =
accum.consume(dy, cellHeight)`. DIRECTION (verify empirically, single commented flip
point): natural scroll positive deltaY → viewport UP; `steps>0→UP, steps<0→DOWN`; honor
`event.isDirectionInvertedFromDevice`. Emit `abs(steps)` `FfiScrollEvent(direction, grid,
row, col)`. Trigger `scrollbarActivity()`.

### 5.4 Synthetic scrollbars (custom-drawn in `draw(_:)`, NOT NSScroller subviews)
Constants: `scrollbarThickness=8, scrollbarInset=2, scrollbarMinThumb=24, trackAlpha=0.12,
thumbAlpha=0.45, overlayFadeDelay=0.9, overlayFadeDuration=0.4, overlayFadeTick=1/30`.
State: `scrollerStyle = NSScroller.preferredScrollerStyle`, `lastScrollTime`,
`scrollbarFadeTimer`. Per pane: track at right edge `CGRect(x: rect.maxX-thickness-inset,
y: rect.minY+inset, w: thickness, h: rect.height-2inset)`; read `topline/botline/linecount`;
if `linecount==0` or whole buffer visible → draw nothing; `frac=visible/total` (clamp),
`thumbH=max(minThumb, track.h*frac)`, `offsetFrac=(topline-1)/total`, `thumbY` clamped;
rounded thumb (radius thickness/2) at `thumbAlpha*currentAlpha`; `.legacy` also faint track.
Fade: `.legacy` → alpha 1 always; `.overlay` → 1 while `now-lastScrollTime<fadeDelay` then
ramp to 0 over fadeDuration. `scrollbarActivity()` sets lastScrollTime + arms repeating tick
timer (needsDisplay; invalidate at alpha 0) for `.overlay`. Observe
`NSScroller.preferredScrollerStyleDidChangeNotification` → update style + needsDisplay;
remove in deinit. Draw after dividers/cmdline strip. Thumb-drag DEFERRED (non-interactive).

### 5.5 Swift FFI discriminant consts (mirror Rust, `private static let`)
`mouseButtonLeft=0, mouseActionPress=0/Release=1/Drag=2, scrollDirUp=0/Down=1`.

### 5.6 Keyboard stays global (`keyDown` unchanged).

## 6. Tests

### 6.1 `crates/wolf-ffi/tests/mouse_scroll.rs` (real nvim, mirror panes.rs)
`mouse_scroll_click_drag_and_wheel`: (1) start, poll `read_frame().cols()>0`; (2) poll live
window grid (>=2, not hidden); (3) `:put =range(1,80)` → poll `linecount()>=80`, `:1`/gg →
`topline()==1`; (4) `send_mouse(LEFT,PRESS,grid,row5,col2)` → poll `cursor_row()==5 &&
cursor_col()==2` (fallback: row only, documented); (5) DRAG (8,4) + RELEASE (8,4) → assert
responsive + loosely `cursor_row()==8`; (6) `t0=topline()`, send ~5 `DOWN` → poll
`topline()>t0`; (7) `t1=topline()`, send `UP` → poll `topline()<t1`; (8) shutdown. Local
discriminant consts mirroring the Rust mods. `frame_pack` still passes (optional
`topline()==0` assert).

### 6.2 BUILD: add `mouse_scroll` rust_test to `crates/wolf-ffi/BUILD.bazel` mirroring `panes`
(tags no-sandbox, size small, data nvim_tree+init.lua, env, deps `[":wolf_ffi"]+
all_crate_deps(normal_dev=True)`, srcs common/mod.rs+mouse_scroll.rs, crate_root).

### 6.3 Swift headless: PaneGeometryTests + ScrollAccumulatorTests (§4.3) into WolfKeyMapTests.

### 6.4 AppKit + scrollbars = compile + manual-smoke. Lead checklist: click positions
cursor; drag selects (clamps at edges); release clean; scroll up/down (direction per system
natural-scroll, flip if reversed); smooth trackpad precision; per-pane scrollbar reflects
position/size; "Show scroll bars: Always" → always visible; "When scrolling" → fade; live
setting change updates without relaunch; multi-pane targets correct grid + dimming intact.

### 6.5 Targets that must pass: `wolf_core_lib_test`; `wolf-ffi:{bridge_smoke,file_search,
tree_search,panes,frame_pack,mouse_scroll}`; `wolf-macos:{WolfKeyMapTests,bundle_layout_test,
Wolf}`.

## 7. Docs (prettier after)
`wolf-core/src/README.md` (mouse/scroll carry grid; nvim_input_mouse grid-targeted;
send_mouse/send_scroll). `wolf-ffi/README.md` + `src/README.md` (send_mouse/send_scroll,
FfiMouseEvent/FfiScrollEvent + discriminant tables, FfiFrame viewport accessors,
:mouse_scroll target, mappers, from_snapshot_grid viewport). `wolf-macos/README.md` +
`Sources/README.md` (mouse/scroll/scrollbars; PaneGeometry GRID-RECT-inside-PANE-RECT
readying max-col-width deferred; ScrollAccumulator; scrollbar style behavior).
`wolf-macos/Tests/README.md` (the two new test files). `architecture.md` (remove mouse/scroll
from macOS Deferred → realized; grid-targeted mouse + synthesized scrollbar per decision 5;
per-pane PaneGeometry readies future max-col-width, deferred; DO NOT change GNOME claims).

## 8. Deps/lint
No new packages/crates/allowlists. Go-lint config N/A (no Go). Rust: `///` + `#[must_use]`;
magic numbers in the discriminant mods; new bridge items inside `mod ffi`'s scoped allow;
glue at normal bar, no unsafe. Swift: `static let` constants; seams Foundation-only/`Double`.

## 9. Order
core input.rs → core api.rs → ffi lib.rs → regen bindings → ffi test + run (resolve grid +
scroll sign) → Swift seams + tests → AppKit wiring + scrollbars → build Wolf → docs+prettier
→ lead live-GUI verification.

## 10. Status checklist
- [ ] core grid field + threading; send_mouse/send_scroll
- [ ] ffi discriminant mods + structs + send glue + mappers + re-exports
- [ ] ffi FfiFrame viewport fields/accessors/from_snapshot_grid/empty
- [ ] regen + commit generated (wake.h unchanged)
- [ ] ffi mouse_scroll test + target green (grid + sign resolved)
- [ ] Swift PaneGeometry + ScrollAccumulator + headless tests green
- [ ] AppKit geometry refactor + mouse + scroll + scrollbars
- [ ] Wolf builds; bundle_layout_test green
- [ ] docs + prettier
- [ ] lead live-GUI verification
