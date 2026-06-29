# Plan: BUG FIX — split panes break at 3+ panes (right pane lost on a 3rd split)

## Status: COMPLETE ✅ (3-pane bug fixed via n-ary frame model)

### Final state
- ROOT CAUSE was deeper than the divider bug (user-corrected): the binary always-nest PaneTree
  didn't match Neovim. FIXED by rebuilding layout.rs as an N-ARY FRAME model (Frame{Leaf|Split{
  orientation,children,weights}}): same-orientation split FLATTENS into an equal-sized row/column,
  perpendicular NESTS; remove re-equalizes + collapses/merges to canonical. pane_view renders a flat
  N-child frame as a right-leaning gtk::Paned chain with divider positions from weights; install_paned
  _position re-applies on max-position (fixes the original collapse); shrink-guard + 1-cell floor;
  resolve_pending_grids now rebuilds when a pending pane binds. Contained to wolf-gnome; wolf-core
  untouched.
- [P1,P2] vsplit P1 now => flat [P1,P3,P2] equal thirds (P2/B keeps its third, not blanked) —
  matches Neovim. Perpendicular split still nests (necessary).
- Tests: layout pure 50✓ (incl flat-row/4-equal/nest/re-equalize/collapse-merge/neighbor); 9/9 cage
  smokes (added three_panes_keep_all_files_visible); wolf-core untouched✓; clippy clean; denylist CLEAN.
- Reviewed APPROVED (correct + Neovim-faithful). Docs updated (gnome README binary->n-ary,
  architecture.md frame-model note, src/tests READMEs); stale layout.rs bootstrap scaffold removed.
- v1 limitation (documented): divider drags reset to equal on the next window/container resize
  (weights stored for future writeback).

## (history) CODE DONE ✅ — technical-writer (doc drift) then DONE

### Hardening landed: resolve_pending_grids now rebuild_pane_area() when a pane's grid goes None->Some
(only on real change; no loop; borrows scoped). 9/9 cage smokes pass (twice), lib 50✓, wolf-core✓,
clippy clean, denylist CLEAN. Pending-grid race has no deterministic test (grids resolve at split time);
gap noted.

## (history) REVIEWED ✅ (n-ary fix correct & Neovim-faithful) — 1 hardening fix + docs, then DONE

### Reviewer verdict: APPROVED. n-ary frame model correct (flatten same-orientation / nest perpendicular
/ remove re-equalizes + collapse-merge canonical); pane_rects tiles exactly; GTK chain + max-position
re-apply + shrink/floor realize weights without starving panes or fighting drags; 3-pane bug subsumed.
### DECISIONS on findings:
- SHOULD-FIX 1 (TAKE — coder): resolve_pending_grids (pane_view.rs ~274-304) sets p.grid but never
  rebuild_pane_area(); a pane created while its grid is pending (apply_split grid_for_win==None) is
  skipped by build_chain's None-handling and stays INVISIBLE until the next split/close. Latent (grids
  resolve at split time in smokes) but real. FIX: when resolve_pending_grids binds a previously-unbound
  pane (grid went None->Some), call self.rebuild_pane_area() so the now-bound DrawingArea gets parented.
  Verify 9 cage smokes + lib still green (a deterministic race test is impractical — note the gap).
- SHOULD-FIX 2 + NIT (technical-writer): gnome README:32 "binary pane-layout tree" -> n-ary/frame
  model; architecture.md:88-89 add a one-line note that same-orientation splits flatten (frame model).
- NIT (accepted): divider ratio applied vs max_position not full extent — cosmetic, covered by the
  documented idealized-ratio limitation.

## (history) ALL GREEN ✅ (9/9 cage smokes; 3-pane bug FIXED) — reviewer, then technical-writer

### Final: layout pure 50✓, wolf-gnome non-ignored✓, ALL 9 CAGE SMOKES✓ (incl
three_panes_keep_all_files_visible), wolf-core untouched✓, clippy clean both crates, denylist CLEAN.

## (history) N-ARY FIX IMPLEMENTED ✅ (works) — 1 test-typo fix, then reviewer

### Coder result: n-ary Frame model landed. layout.rs Frame{Leaf|Split{orientation,children,weights}}
+ split (flatten same-orientation / nest perpendicular) + remove/normalize (collapse + same-orientation
merge) + n-ary pane_rects. pane_view build_chain (right-leaning gtk::Paned chain, ratio from weights) +
install_paned_position (re-apply on max-position, no latch) + shrink(false) + 1-cell floor. README
updated. THE 3-PANE BUG IS FIXED: cage OCR reads all three panes (triaone/tribtwo/tricthree) — B no
longer blanked. Tests: layout pure 50✓ (incl vsplit_flattens 1,3,2 equal thirds + merge 1,2,4),
8/9 cage smokes✓, clippy clean, wolf-core untouched✓, denylist CLEAN.

### ORCHESTRATOR DECISION (test typo, my prompt's fault): the 3-pane smoke fails ONLY because
TRI_OCR_C = "trictthree" (doubled t) but heading "# TRICTHREE" lowercases to "tricthree". App renders
all three correctly. FIX (test-writer): smoke_headless.rs:~414 change TRI_OCR_C "trictthree" ->
"tricthree" (and fix the line-410 doc comment to match). Then coder re-runs to confirm 9/9.

## (history) N-ARY REDESIGN PLANNED — test-writer next

# ARCHITECT N-ARY PLAN (condensed)
KEY FINDING: existing layout.rs tests do NOT assert same-orientation nesting (all multi-split tests
are perpendicular V-then-H, which STILL nests identically) -> they pass unchanged; pure work is
ADDITIVE. Contained to wolf-gnome (layout.rs + pane_view.rs); wolf-core untouched.

## Decisions
- KEEP Orientation{Horizontal,Vertical} (matches wolf_core::SplitOrientation + gtk mapping; Vertical=
  side-by-side=:vsplit=divides WIDTH=gtk Horizontal Paned; Horizontal=stacked=:split=divides HEIGHT).
- STORE per-child weights:Vec<f64> (sum=1) — source of truth for divider positions; v1 always
  equalizes on split/remove. Insert new pane AFTER target. Equalize-on-split AND on-remove (nvim
  equalalways). Collapse a 1-child Split into its child; MERGE a surviving same-orientation Split into
  the parent (canonical: no same-orientation nesting persists).
- v1 DRAG limitation: drags are GTK-local, NOT written back to weights -> re-derived (equal) on every
  notify::max-position, so a drag resets to equal on the next window/container resize (or split/close).
  Documented; weights make future writeback easy. (Accepted trade for correctness.)

## Model (layout.rs, pure)
`enum Frame { Leaf(PaneId) | Split{ orientation, children:Vec<Frame>, weights:Vec<f64> } }`
(invariants: Split children>=2; weights.len==children.len, sum 1; no same-orientation Split directly
under a Split). `PaneTree{root:Frame}`. PaneLayout mirror becomes n-ary:
`enum PaneLayout{ Leaf(PaneId) | Split{orientation, children:Vec<PaneLayout>, weights:Vec<f64>} }`.
Keep PaneId/Orientation/Rect; drop PLACEHOLDER_PANE.
- split(target,new,orient): root-leaf => wrap. Else find the Split owning Leaf(target): if frame.
  orientation==orient INSERT Leaf(new) after target + equalize (FLAT); else REPLACE Leaf(target) with
  Split{orient,[target,new],[.5,.5]} (NEST only perpendicular). split_in recursion returns bool.
- remove(pane): guard leaves<=1 no-op; remove child slot+weight; normalize(): bottom-up flatten
  same-orientation children + collapse 1-child Split into child + equalize.
- pane_rects: n-ary cumulative normalized fractions (exact tiling, no zero rects). neighbor UNCHANGED
  (pure geometry over pane_rects). leaves() DFS order. layout()->n-ary PaneLayout.

## GTK render (pane_view.rs build_layout_widget rewrite)
Leaf unchanged (reuse area by PaneId). Split => render N children as a RIGHT-LEANING CHAIN of binary
gtk::Paned via build_chain: paned start=child0, end=chain(rest); divider ratio = weights[0]/sum(rest+
first) so equal-thirds => outer 1/3, inner 1/2 (= equal widths). None-handling: skip unbound panes
(start None=>end, end None=>start). set_hexpand/vexpand; set_shrink_start_child(false)+set_shrink_end_
child(false); install_paned_position. install_paned_position: DROP one-shot latch, re-apply on EVERY
notify::max-position (max>0 => set_position(ratio_position(max,ratio))). Optional: 1-cell min on leaf
DrawingAreas (make_pane_area set_content_width/height) as a floor under shrink=false. Glue otherwise
UNCHANGED (apply_split/detect_closes/apply_focus/resolve_pending_grids/rebuild_pane_area signatures
stable; only build_layout_widget body + install_paned_position change).

## Tests
PURE layout.rs (ADD; existing pass unchanged): vsplit_flattens_into_row ([P1,P2] vsplit P1 ->
[P1,P3,P2] equal thirds, none zero); hsplit_of_row_member_nests (V then H -> Row[Col[P1,P3],P2]);
vsplit_twice_stays_flat_four_equal; remove_re_equalizes_row ([P1,P3,P2] remove P3 -> [P1,P2] 50/50);
remove_collapses_single_child; remove_collapses_and_merges_same_orientation (-> flat equal thirds);
neighbor_flat_row_of_three (P1.R=P3,P3.R=P2,P2.R=None,P3.L=P1); keep remove_last_is_noop. Update the
module header doc-comment to the frame model. CAGE smoke (smoke_headless.rs): ADD 9th #[ignore]
CAGE_LOCK three_panes_keep_all_files_visible_under_cage (TRIAONE/TRIBTWO/TRICTHREE; open A; <C-w>v+:e
B; <C-w>h; <C-w>v+:e C -> flat [A,C,B]; o+Esc park; grim; OCR all three). Keep 8 existing smokes.

## Lint/scope: no new deps; denylist unchanged. #[allow(cast_precision_loss, reason)] on 1.0/n as f64;
keep existing casts' allows. Contained to wolf-gnome; wolf-core untouched. nix develop gate.
Impl order: layout.rs model+tests -> pane_view build_layout_widget+install_paned_position -> README ->
test-writer cage smoke. README known-limitations: idealized/equal ratios + drags reset on resize (v1).

## (history) SCOPE CORRECTED by user — redesign to N-ARY FRAME MODEL (re-architecting)

## ★ USER CORRECTION (supersedes the band-aid divider fix): match Neovim's FRAME MODEL.
The binary always-nest PaneTree is WRONG. Neovim: a frame is leaf | ROW(children side-by-side) |
COLUMN(children stacked). RULE:
- Split SAME orientation as the containing frame => insert a FLAT sibling (equalized). [P1,P2] row,
  vsplit P1 => flat [P1,P3,P2] ~33/33/33. NO nesting.
- Split PERPENDICULAR => wrap target leaf in a new sub-frame. [P1,P2] row, hsplit P1 =>
  [ column[P1,P3], P2 ]. Nest ONLY when necessary.
So the real fix = rebuild PaneTree as an N-ARY frame tree (orientation + Vec<child>): split flattens
into parent when orientation matches, nests when perpendicular; a frame divides extent EQUALLY among
children by default (nvim equalize-on-split), draggable later; remove re-equalizes remaining siblings
and collapses a 1-child frame into its parent. This SUBSUMES the original "B disappears" bug
(positions computed from the frame's equal pane_rects, not blind 0.5; + set_shrink_*_child(false)).
Contained to wolf-gnome (layout.rs + pane_view.rs); wolf-core is layout-agnostic.

## (kept) Prior architect diagnosis remains valid as a SUB-issue: install_paned_position one-shot
latch mis-sized nested Paneds (collapsed the right child) — fold the correct-position + shrink-guard
into the n-ary rendering. H2 (stale grid) ruled out.

## (history) ROOT CAUSE FOUND (H1) — test-writer next

---

# ARCHITECT DIAGNOSIS + FIX PLAN

## ROOT CAUSE = H1 (nested gtk::Paned divider sizing). H2 RULED OUT.
- Pure layout INNOCENT: PaneTree.split([L,P2] + split L) -> Split V{ Split V{P1,P3}, Leaf(P2) };
  leaves DFS [P1,P3,P2]; pane_rects tiles with no zero-width rect. build_layout_widget maps all 3
  leaves to their reused DrawingAreas; P2 (B) is NEVER dropped from panes/tree.
- BUG: `install_paned_position` (pane_view.rs ~684) is a ONE-SHOT absolute-pixel setter: it latches
  `done` on the FIRST positive notify::max-position and never re-applies. Fine for 2 panes (one
  Paned) — why existing smokes pass. For the NESTED 3-pane case (outer Paned[ inner[P1,P3], P2 ]),
  the outer's first positive max-position arrives during an INTERMEDIATE measure/allocate pass (inner
  reporting large natural width), so max/2 is wrong AND never re-applied => outer divider ends near
  max, start child (inner) takes ~all width, end child P2 (B) starved to ~0 px => blank/gray, B
  "vanishes". DrawingArea/grid/tree entry all still exist; only allocation is ~0.
- H2 ruled out by reading redraw.rs/render.rs: a LEFT :vsplit emits no win_close/grid_destroy for
  B's grid; B's grid persists in snapshot.grids; P2.grid already Some(B_grid) so draw_pane resolves
  it (no active_grid fallback); detect_closes only fires when snap.grid(g) is None (not the case);
  resolve_pending_grids skips already-resolved P2. So B's area would be full-size painting A/empty if
  H2 — but symptom is GRAY/blank => H1.
- DISAMBIGUATION (coder confirm BEFORE fixing): temp-log P2 DrawingArea width + outer Paned
  position()/max_position() after the 3rd split. width≈0 & position≈max => H1 (apply fix). full-size
  but wrong content => H2 (investigate further — not expected).

## MINIMAL FIX (H1), in crates/wolf-gnome/src/pane_view.rs:
1. LOAD-BEARING: rework `install_paned_position` (~684) — DROP the one-shot `done` latch; re-apply the
   proportional position on EVERY notify::max-position: `let max=p.max_position(); if max>0 {
   p.set_position(ratio_position(max,ratio)); }`. Converges across the nested multi-pass allocation +
   re-proportions on window resize. Does NOT fight user drags (a drag changes position, not
   max-position, so the handler isn't retriggered); no infinite loop (set_position doesn't change the
   Paned's own size/max-position).
2. Belt-and-suspenders (prevent any collapse): in build_layout_widget (~665-673) after creating each
   gtk::Paned: `paned.set_shrink_start_child(false); paned.set_shrink_end_child(false);` (leave
   set_resize_* default true). Optionally give each leaf DrawingArea a 1-cell min in make_pane_area
   (~375): set_content_width/height to ~1 cell px from metrics, so shrink=false has a non-zero floor.
   #1 is the real fix; #2 is cheap insurance.
gtk4-rs notes: set_shrink_start_child/set_shrink_end_child + set_content_width/height already in the
deps; don't call set_position at build time (size unknown) — only in the notify::max-position handler.

## TESTS
- Cage smoke (test-writer): 9th opt-in #[ignore] CAGE_LOCK-serialized smoke, mirror
  vertical_split_shows_two_panes (~1376): 3 files (# TRIAONE/# TRIBTWO/# TRICTHREE); open A; <C-w>v +
  :e B (A|B); <C-w>h focus left; <C-w>v + :e C in new sub-pane; park cursor o+Esc; grim; OCR; assert
  ALL THREE (triaone,tribtwo,trictthree) visible simultaneously. Fails today (B gray), passes after.
- Pure layout test (optional, additive — proves pure layer innocent): in layout.rs near
  nested_split_rects, split(P1,P2,V) then split(P1,P3,V); pane_rects(W,H) => P1[0,0,W/4,H],
  P3[W/4,0,W/4,H], P2[W/2,0,W/2,H], 3 non-overlapping, none zero-width.

## NO-REGRESSION: 2-pane (single Paned re-applying 0.5 on max-position == same centered divider);
focus/picker/tree/cursor smokes untouched; app.rs tree-sidebar paned not touched; re-apply only on
resize (not drag) so no drag regression. nix develop gate; denylist clean (no new deps).

---

## Problem statement
Split panes work for TWO panes but break at THREE. Repro (user):
- Left pane = file A, right pane = file B (two panes, working).
- In the LEFT pane, split vertically.
- BROKEN RESULT: left pane splits into two halves BOTH editing A (this part is EXPECTED — nvim
  :vsplit duplicates buffer A into the new window), AND the original RIGHT pane (B) becomes
  "grayed out" / blank and file B is no longer visible.
- EXPECTED: 3 panes — two left halves (A, A) + the original right pane STILL showing B.
The bug = creating a 3rd pane corrupts/hides the pre-existing right pane.

Lean orchestration (bug fix, not a feature): architect root-cause + minimal fix plan; test-writer
adds a 3-pane regression test; coder DIAGNOSES empirically then fixes + verifies; reviewer;
tech-writer only if docs change. nix develop gate; keep the 8 existing cage smokes + all unit tests
green; denylist clean.

## Orchestrator investigation (grounding — verify, don't assume)
- pane_view.rs: apply_split(~177), ensure_initial_pane(~125), alloc_pane_id(~345, monotonic),
  draw_pane(~414 — falls back to snap.active_grid() when pane.grid_id doesn't resolve),
  rebuild_pane_area(~509), build_layout_widget(~647 nested gtk::Paned), divider/position setting.
  layout.rs PaneTree split/pane_rects/PaneLayout. core render.rs grid_for_win + windows/grids;
  redraw.rs win_pos.
- H1 (nested Paned sizing): rebuild builds outer Paned[inner[L,new], R]; the once-set divider
  position likely mis-sizes the nested case, collapsing the right child to ~0 width => blank/gray,
  B not visible.
- H2 (stale/wrong grid binding): after :vsplit nvim re-lays-out / re-emits win_pos; the right pane's
  bound grid may go stale; draw_pane then falls back to active_grid() (= the new A grid) => paints
  A/empty not B. Inspect snapshot.windows/grids + grid_for_win for the right window after a 3-pane
  split (empirically — temp tracing under cage or a core test).
- "grayed out" => either H1 (collapsed child) or H2 (empty/fallback grid). Coder must DIAGNOSE which
  (or both/other) before fixing — fix the root cause, not the symptom.

## Required regression test (test-writer)
- Cage smoke (opt-in, CAGE_LOCK-serialized, OCR-robust, like existing split smokes in
  crates/wolf-gnome/tests/smoke_headless.rs): THREE distinct md files w/ OCR-friendly headings
  (tri_a.md=># TRIAONE, tri_b.md=># TRIBTWO, tri_c.md=># TRICTHREE). Open A; vsplit + :e B in new
  (right) pane (A|B); focus LEFT (<C-w>h); vsplit left + :e C in the new sub-pane; park cursor off
  text (o+Esc); grim; OCR; assert ALL THREE (triaone, tribtwo, trictthree) visible simultaneously
  (fails today — B disappears). Keep the 8 existing smokes intact.
- If root cause is in pure layout: add a PURE layout.rs unit test for the 3-leaf nested case
  (pane_rects for [[L,new],R] returns 3 non-overlapping rects tiling the area, none zero-sized).

## Constraints
nix develop gate; denylist clean; prefer real-nvim/real-fs; do NOT weaken existing tests; docs are
source of truth (update known-limitations only if behavior docs change).

## Progress log
- (orchestrator) Plan reset for the 3-pane bug. Investigated pane_view.rs (apply_split/draw_pane/
  rebuild_pane_area) — 2 hypotheses (H1 nested-Paned sizing, H2 stale grid binding). Next: architect.
