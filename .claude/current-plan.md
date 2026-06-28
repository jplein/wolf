# Plan: wolf-gnome mode-aware cursor (fix invisible cursor)

## Status: COMPLETE ✅ (mode-aware cursor shipped)

### Final state
- Cursor is now OPAQUE reverse-video, mode-aware via ModeInfo.shape: BLOCK in normal
  (fills cell, redraws glyph inverted so it stays readable), VERTICAL BAR in insert,
  HORIZONTAL bar where nvim reports it. Reliably visible (inversion of underlying cell;
  honor mode hl only when visibly distinct). New pure module cursor.rs (cursor_rect +
  cursor_colors), unit-tested headlessly.
- Tests: wolf-gnome lib 14✓ (cursor geometry + colors incl 3 S1 guard cases), Tier-1
  intact, smoke (--ignored) ✓ under cage, wolf-core 15✓, clippy clean, denylist CLEAN.
- Docs: architecture.md + crate/src/tests READMEs + in-code comments all updated.
- New file: crates/wolf-gnome/src/cursor.rs. Removed CURSOR_ALPHA/FALLBACK_CURSOR/cursor_color.

## (history) S1 HARDENED, ALL GREEN — technical-writer updating docs (final step)

### S1 fix landed: cursor_colors now honors the mode hl ONLY when visible (bg != cell_bg AND
bg != fg), else inverts. N1 redundant .or removed. Rgb derives PartialEq/Eq (no core change).
Tests: wolf-gnome lib 14✓ (incl 3 new guard cases), Tier-1 intact, wolf-core 15✓, clippy clean,
denylist CLEAN. No deviations.

## (history) REVIEWER APPROVED w/ S1 — hardening honor path (test-writer then coder)

### Reviewer verdict: APPROVED w/ minor notes. Shape work complete+correct; edges/scope/lint clean.
### ORCHESTRATOR DECISION on S1 (substantive, correctness-first):
S1: cursor_colors' "honor mode attr_id" gate checks the DEFAULTS-MERGED resolved style, so a
half-defined Cursor hl (e.g. only dark bg; fg filled from defaults) passes "both Some" and can
re-introduce dark-on-dark invisibility; the inversion safety net is dead whenever defaults set.
Benign for bare --clean (our target) but the "always visible" guarantee isn't enforced.
FIX (self-contained in cursor.rs, NO core change): honor the mode cursor hl ONLY when it yields a
VISIBLE cursor — i.e. resolved fg AND bg both Some AND honored fill (bg) != underlying cell
background AND bg != fg. Otherwise fall through to INVERSION (fill=cell_fg, glyph=cell_bg).
Define underlying cell background = (cell present: resolve(cell.hl_id).bg or defaults.bg or
FALLBACK_BG; cell None: defaults.bg or FALLBACK_BG).
N1: remove the redundant `.or(defaults.fg/bg)` in the inversion branch (resolve already merged
defaults) — cosmetic cleanup, do alongside.
- Existing honor test (distinct colors, no defaults) STILL passes (fill != bg, fg != bg).
- Order: test-writer adds the guard test case(s), then coder implements + runs.
- Docs still pending (technical-writer): architecture.md:65, README.md:58, src/README.md:38-39,54-55
  (drop CURSOR_ALPHA/FALLBACK_CURSOR, add cursor.rs section). In-code app.rs/lib.rs docs already done.

## (history) CODER DONE (all green, no deviations) — proceeding to reviewer

### Coder result
- render.rs: FALLBACK_FG/BG + draw_glyph -> pub(crate) (verified white/black per D-CUR3).
- cursor.rs: cursor_rect + cursor_colors + consts + pct_fraction helper (pure, no GTK).
- lib.rs: pub mod cursor. app.rs: draw_cursor rewritten (mode-aware, opaque inversion,
  block redraws glyph inverted via render::draw_glyph; OOR cell -> no panic); removed
  cursor_color/CURSOR_ALPHA/FALLBACK_CURSOR + unused Rgb import. In-code docs updated.
- Tests: cursor 9✓, wolf-gnome lib 11✓, gdk_key 1✓, paint_golden 3✓, roundtrip_nvim 1✓;
  wolf-core 15✓; clippy clean (no new #[allow]); denylist CLEAN; smoke (--ignored) ✓ under cage.
- No deviations. Optional deferred: offscreen structural cursor-pixel test (architect already deferred).

### Test-writer output
- crates/wolf-gnome/src/cursor.rs: module doc + NOTE-to-coder placeholder + #[cfg(test)] mod tests
  (cursor_rect_table + named_constants_match_contract; cursor_colors hand-built-snapshot tests).
- Won't compile until coder adds cursor_rect/cursor_colors/MIN_BAR_PX/DEFAULT_CELL_PCT/PCT_DENOM
  ABOVE the tests, and adds `pub mod cursor;` to lib.rs.

### ORCHESTRATOR DECISIONS
- D-CUR1: cursor_colors must use highlights.resolve(attr_id) (resolved style), per plan.
- D-CUR2 (unspecified edge flagged by test-writer): if grid.cell(cursor.row,cursor.col) is None
  (cursor out of grid bounds), cursor_colors must NOT panic — fall through to
  defaults.fg/bg then FALLBACK_FG/BG. draw_cursor likewise: skip the glyph redraw if cell is None.
- D-CUR3: promoted render.rs FALLBACK_FG must equal white(255,255,255) and FALLBACK_BG black(0,0,0)
  (tests assert literals). Coder: verify render.rs's existing FALLBACK consts ARE these values;
  if they differ, STOP and report (do not silently change either).

### NOTE coder must honor
- Tests assert fallback literals white/black rather than importing the consts; ensure the
  pub(crate) FALLBACK_FG/BG equal those. Tests use highlights.resolve path.

---

# ARCHITECT PLAN (condensed)

## Root cause
50%-alpha overlay + bg-first color pick from the bare-nvim Cursor hl => dark-on-dark,
invisible. Fix must NOT depend on Cursor hl being well-defined.

## Color strategy: reverse-video INVERSION (opaque)
`cursor_colors(snap) -> (fill: Rgb, glyph: Rgb)`:
1. mode = mode_infos.get(cursor.mode_idx).
2. If mode.attr_id resolves to BOTH fg+bg Some => honor it: fill=bg, glyph=fg.
3. Else INVERT underlying cell: resolve grid.cell(row,col).hl_id; fill = cell_fg
   (style.fg or defaults.fg or FALLBACK_FG/white); glyph = cell_bg (style.bg or
   defaults.bg or FALLBACK_BG/black).
Promote render.rs FALLBACK_FG/FALLBACK_BG to pub(crate) and reuse (no drift). REMOVE
app.rs CURSOR_ALPHA + FALLBACK_CURSOR. Keep RGB_MAX; use opaque set_source_rgb.

## Shape geometry: PURE helper in NEW module crates/wolf-gnome/src/cursor.rs
`cursor_rect(shape, cell_x, cell_y, cell_w, cell_h, cell_percentage: Option<u32>)
 -> (x,y,w,h)`:
- Block => full cell (ignores pct).
- Vertical => left bar: w = (cell_w*pct).max(MIN_BAR_PX).min(cell_w), full height.
- Horizontal => bottom bar: h=(cell_h*pct).max(MIN_BAR_PX).min(cell_h), full width,
  y=cell_y+cell_h-h.
Consts: MIN_BAR_PX=2.0, DEFAULT_CELL_PCT=0.25 (Neovim convention when None),
PCT_DENOM=100.0. pct = cell_percentage.map(f64::from(p)/100).unwrap_or(0.25).clamp(0,1)
(cell_percentage is a PERCENT 1-100). f64::from(p) (no cast lint). Pure, no GTK.

## draw_cursor rewrite (app.rs, GTK glue only)
shape = mode_infos.get(mode_idx).map(.shape).unwrap_or(Block); pct from same.
(fill,glyph)=cursor_colors(snap); cell_x=col*cell_w, cell_y=row*cell_h;
(x,y,w,h)=cursor_rect(...); opaque fill rect. THEN only for Block (covers glyph):
if grid.cell(row,col) has non-empty text, render::draw_glyph(cr, metrics, cell_x,
cell_y, &text, glyph, &resolve(hl_id)) to redraw glyph inverted. Vertical/Horizontal:
no redraw (thin bar leaves glyph visible). Empty cell: rect only. Promote render.rs
draw_glyph to pub(crate) (no duplication, exact alignment). wire_draw order unchanged
(paint then draw_cursor).

## Tests (test-writer)
A. PURE unit tests in cursor.rs #[cfg(test)] (mirror geometry.rs):
   cursor_rect: Block=full cell (any pct); Vertical Some(25) on cell_w=20 -> w=5.0,
   x=cell_x, h=cell_h; Vertical None -> 0.25; tiny pct -> floored to MIN_BAR_PX;
   Some(150) -> clamp 1.0 -> w=cell_w; Horizontal Some(25) -> h=0.25*cell_h,
   y=cell_y+cell_h-h, full width; min-height + None analogous. Non-square dims + eps.
   cursor_colors (hand-built Snapshots): attr_id both set -> (bg,fg) honor; bg-only/
   fg-only/none -> inversion (cell_fg,cell_bg); mode_idx OOR -> inversion no panic;
   empty mode_infos -> inversion no panic; unset cell colors -> FALLBACK_FG/BG.
B. Smoke: DO NOT couple cursor to OCR (fragile). Keep "hello" assertion; add a NOTE
   that cursor geometry/color is covered by cursor unit tests + visual confirmation.
   (Optional deferred: offscreen ImageSurface cursor structural test — defer unless
   reviewer wants it.)
C. Green gate: wolf-core 15, Tier-1 (gdk_key/paint_golden/roundtrip_nvim), geometry,
   new cursor tests, clippy, GUI denylist (no new deps).

## Docs (technical-writer later)
app.rs module doc ~L14, draw_cursor/(removed)cursor_color comments; src/README.md
(drop CURSOR_ALPHA/FALLBACK_CURSOR + "semi-transparent block" bullet, add ## cursor.rs);
README.md "Draw." bullet L58-60; lib.rs module doc add cursor; architecture.md L65
"semi-transparent block cursor" -> "mode-aware cursor: block normal / bar insert,
driven by ModeInfo.shape".

## Lint/risks
No new deps (denylist unaffected). cell_percentage is percent /100. usize->f64 keep
existing `as f64` form (no new #[allow] expected; add narrow one w/ reason if clippy
flags). OOR mode_idx + empty mode_infos handled via .get + Block/inversion fallback.
draw_glyph reuse => exact alignment. Wide-trailing: cell.text.is_empty() guard.
Remove dead CURSOR_ALPHA/FALLBACK_CURSOR (else unused warn).

## Impl order
1. render.rs: FALLBACK_FG/BG + draw_glyph -> pub(crate). 2. new cursor.rs
(cursor_rect+cursor_colors+consts+tests). 3. lib.rs `pub mod cursor;`. 4. app.rs
rewrite draw_cursor, remove cursor_color/CURSOR_ALPHA/FALLBACK_CURSOR. 5. docs.

---


## Problem statement
The character grid renders correctly, but the cursor is INVISIBLE, and it should be
mode-aware: a BLOCK cursor in normal mode, a VERTICAL BAR in insert mode.

### Two problems
1. **Invisible cursor.** `crates/wolf-gnome/src/app.rs` `draw_cursor` (lines ~207-219)
   fills the cursor cell with `set_source_rgba(color, CURSOR_ALPHA=0.5)`. `cursor_color`
   (lines ~222-239) resolves from `mode_infos[cursor.mode_idx].attr_id` -> resolve ->
   bg, else fg, else `defaults.fg`, else white. Likely failure: in the bare `--clean`
   nvim the Cursor highlight resolves to a bg close to the editor background (dark on
   dark), and at 50% alpha it disappears. The whole approach (semi-transparent +
   bg-first color pick) is fragile.
2. **No shape.** It always draws a full-cell block; it ignores `ModeInfo.shape`.

### Required behavior
- BLOCK cursor in normal mode, VERTICAL BAR in insert mode — driven by
  `mode_infos[cursor.mode_idx].shape` (NOT by mode name), so it generalizes:
  - `CursorShape::Block`  => fill the whole cell.
  - `CursorShape::Vertical` => thin vertical bar at the LEFT of the cell
    (width from `cell_percentage` if set, else a small fixed fraction; min ~1-2px).
  - `CursorShape::Horizontal` => thin bar at the BOTTOM of the cell
    (height from `cell_percentage` else small fraction; min ~1-2px).
- RELIABLY VISIBLE over both background and glyph. Conventional terminal approach is
  to INVERT: a block cursor fills the cell with a contrasting/opaque color and the
  glyph reads against it (e.g. fill with the cell's fg and redraw the glyph in the
  cell's bg), rather than a translucent overlay. Investigate what looks right; opaque
  + contrasting is the goal. A bar cursor is a thin opaque line in a visible color.

### Available render-state (wolf-core/src/render.rs — do NOT modify core)
- `Snapshot.cursor: Cursor { row, col, mode_idx: usize }`
- `Snapshot.mode_infos: Vec<ModeInfo>`; `ModeInfo { name: String, shape: CursorShape,
  cell_percentage: Option<u32>, attr_id: Option<u64> }`
- `enum CursorShape { Block, Horizontal, Vertical }`
- `Snapshot.highlights: HighlightTable` with `resolve(hl_id) -> ResolvedStyle { fg, bg,
  ... }`; `defaults: DefaultColors { fg, bg, special }`; `Rgb { r, g, b }`.
- The cell under the cursor: `snap.grid.cell(row, col)` -> `Cell { text, hl_id }`;
  resolve its style for inversion colors.

### Scope
- ONLY cursor rendering in `crates/wolf-gnome/src/app.rs`. A small PURE helper
  (e.g. cursor rect geometry from shape + cell box + cell_percentage) is encouraged
  so it can be unit-tested without GTK (mirror the geometry.rs pattern).
- Verify: build + clippy clean (via `nix develop --command`); confirm/extend the
  headless cage smoke test so it still renders (ideally assert the cursor region
  differs from an empty cell, or at least the app renders + types).
- Update READMEs / architecture.md cursor wording (they currently say "block cursor"
  / "semi-transparent block cursor" — should describe mode-aware block/bar).
- Do NOT regress wolf-core (15 tests) or the GUI denylist. Keep wolf-gnome Tier-1 +
  geometry tests green.

### Existing constants/helpers in app.rs (relevant)
CURSOR_ALPHA=0.5 (likely removed/changed), RGB_MAX=255.0, FALLBACK_CURSOR=white,
draw_cursor, cursor_color, wire_draw (calls paint then draw_cursor).

## Progress log
- (orchestrator) Read app.rs cursor code + render.rs CursorShape/ModeInfo. Grounded the
  problem statement. Next: invoke architect.
