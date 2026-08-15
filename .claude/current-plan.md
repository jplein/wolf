# Plan: Remove the active-pane focus ring from wolf-gnome

## Status

COMPLETE. Canonical gate green on the final tree: `cargo build --workspace --all-targets` exit 0,
`cargo test --workspace` exit 0 — 1144 passed, 0 failed, 10 ignored (all pre-existing, each stating
its reason). `cargo clippy --workspace --all-targets -- -D warnings` exit 0, verified real by driver
invocation and by a positive control that failed as expected. `cargo fmt --all -- --check` and
`prettier --check "**/*.md"` clean; `markdown_check.sh` reports no findings on the two edited docs.

Diff-level confirmation that nothing was weakened: exactly one `assert!` removed across the change
(R1's approved one), zero `#[test]` functions added or removed.

Open, reported to the user, NOT fixed here: `DECISIONS.md:161` says "Every widget under the pane area
is destroyed and rebuilt on each layout change", false by the same evidence that corrected `:145` —
the hosts, their `DrawingArea` children and any `WebView` overlay child all survive; only the
`gtk::Paned` tree is rebuilt. Pre-existing, one clause.

**Evidence held (workflow step 7).** The new pixel assertion was falsified against the tree with the
ring present, before any source change:

```
assertion `left == right` failed: … 0 pixels at the LEFT pane's own left edge (x 251..255) and 586
at the RIGHT pane's own right edge (x 1276..1280), rows 67..360, are exactly GNOME's accent #3584e4
  left: 586
 right: 0
```

586 is exactly half the right probe's 1172 px — two of four columns for all 293 rows, which is what
a 2 px outline at `outline-offset: -2px` draws. The precondition stayed green, so the subject was
not vacuous. `the_thumb_lies_wholly_inside_the_right_gutter`, whose assertion set materially changed
under R1, was falsified separately by raising `SCROLLBAR_THICKNESS_PX` to 24 and restored to a
matching sha256. T1–T12 are comments and assertion messages with no behavior to disable, so none is
owed evidence.

**Gate results (real).** `cargo test -p wolf-gnome` exit 0 — 484 passed, 0 failed, 10 ignored
(pre-existing, self-declaring); smoke target 70 passed, 0 failed, 577.57s, unfiltered.
`cargo clippy --workspace --all-targets -- -D warnings` exit 0. `prettier --check` on `DECISIONS.md`
ok. `cargo fmt --all -- --check` RED, whitespace only, inside the test-writer's `pane_edge_probes()`.

**The empirical unknown from §7.2 is settled by measurement.** Both probes read 1172 px of a single
colour `(20, 22, 27)`, and the reference pixel at `(1262, 213)` reads the same. The outermost four
columns are the right pane's own padding, as inferred.

## 0.2 Orchestrator rulings, round 3 (post-review)

The reviewer approved with three routed items. Rulings:

**R6 — the reviewer's severity-1 finding is resolved in the change's favor; `DECISIONS.md:147` stands.**
I read the deleted ablation table directly (`git show HEAD:crates/wolf-gnome/tests/smoke_headless.rs`,
lines 7648-7663):

```
as captured                     4 DIVIDERBR=AVO
right pane cropped out alone    77 DIVIDERBRAVO   (correct)
whole ring masked               B DIVIDERBRAVO    (correct)
ring TOP edge only masked       # DIVIDERBRAVO    (correct)
ring LEFT edge only masked      a DIVIDERBRAVO    (correct)
ring RIGHT edge only masked     4 DIVIDERBR=AVO   (still wrong)
ring BOTTOM edge only masked    4 DIVIDERBR=AVO   (still wrong)
```

with the conclusion "So it is the ring's TOP edge: a long thin high-contrast horizontal line
immediately above a 15-cell heading, which `--psm 11` folds into the same sparse region and scales
from." The `:147` sentence is faithful, geometry included. The reviewer conflated two measurements:
`DIVIDER_BODY_LINE_COUNT`'s ablation concerns the `~` COLUMN, where masking the ring left the misread
in place, while this one concerns the heading OCR, where masking its top edge fixed it. Both are true
of different readings. The technical-writer should carry the decisive line into `:147` so the claim
stays checkable, since the table itself is gone.

**R7 — the precondition gains the left-probe cross-check (reviewer item 2): APPROVED.** The message
names a cause the check cannot detect: if the window fails to reach the output's right edge, the
reference pixel at x 1262 is outside it too, both regions read the compositor's background, they
compare equal, and the subject assertion goes vacuous. `count_exactly(left_probe, editor_bg) > 0`
closes it, because the left probe's x is `OVERLAY_ORIGIN_X_EXPECTED` and sits inside the pane area by
construction. This STRENGTHENS a precondition; the two-assertion shape in §7.2 was the architect's
sketch, not a boundary.

**R8 — the false clause at `smoke_headless.rs:10388` is deleted (reviewer item 3): APPROVED.** It
claims `LINK_SEARCH_INSET_Y_PX` starts the search below the statusbar strip. The search rect is
`y 87..600` and the strip spans `y 47..139` at `SMOKE_FONT_PT`, so rows 87..139 are inside the strip
band. The bullet's conclusion survives on its second reason (grey theme text, which `is_link_blue`'s
blue-over-red margin rejects). Pre-existing defect, adjacent to T3, worth fixing while we are here.

## 0.1 Orchestrator rulings, round 2

**R3 — `cargo fmt -p wolf-gnome` is the coder's to run, despite landing in a test file.** Applying a
formatter is mechanical, not authoring, and a whole agent round for whitespace is waste. The coder
runs it and re-verifies; it must change no token.

**R4 — record the settled measurement (coder's recommendation 1): APPROVED.** §7.2 asked for exactly
this. The test-writer rewrites `pane_edge_probes()`'s "inference until the first green run"
paragraph as the measured fact it now is, citing the counts above.

**R5 — the two uncovered guards (coder's recommendations 3 and 4) are NOT built in this pass.** The
coder measured that both fail to fire: deleting the `record_active` block from `set_active_pane`
leaves the whole suite green (`drawer_up_target` falls back to `bottom_most_leaf`, which returns the
same pane in every one-editor-pane scenario), and inverting the active-pane predicate at
`pane_view.rs:2428` also leaves it green (`tests/README.md:240` already records that OCR reads
neither a font weight nor a dim). Both are pre-existing gaps this change did not create. The second
becomes more consequential — the strip's text is the only active-pane signal and nothing observes
it — but building new smoke coverage is beyond "remove the border", and scaling scope up is the
user's call, not mine. Surface both to the user as findings. Do NOT add a replacement indicator, and
do not weaken anything to compensate.

**Recommendation 5** (the `Six of the sixty read RAW PIXELS` count in `smoke_headless.rs`'s module
doc, and whether `tests/README.md` carries a matching one) goes to the technical-writer.

## Problem statement

`crates/wolf-gnome/src/pane_view.rs` installs a display-global CSS provider carrying
`.wolf-active-pane { outline: 2px solid #3584e4; outline-offset: -2px; }` and toggles that class
across every pane on each focus change, drawing a bright blue ring around the active pane. Delete
the rule, the provider install, and the class toggle. The per-pane statusbar strip's text — bold at
full strength in the active pane, both lines scaled by `STATUSBAR_INACTIVE_ALPHA` elsewhere — then
becomes the ONLY active-pane signal, which is what macOS already does. That is the intended
outcome. Do NOT add a replacement indicator.

## 0. Orchestrator rulings

**R1 — the deleted `scrollbar.rs` const assertion (§4.2c) is approved.** The `SCROLLBAR_MARGIN_PX >= 4.0`
check's entire subject was the four pixels the outline covered. It could not fail before or after
(`SCROLLBAR_MARGIN_PX` is 6.0), it never guarded a false positive, and re-justifying it would invent
a rule that never existed. This is the one deletion approved; no other assertion may go. The
thumb-inside-the-gutter property must remain carried by the surviving const relation plus the
per-width loop.

**R2 — `DECISIONS.md:111` (§4.3b) is a REWRITE OF EXISTING TEXT, not new prose.** The architect
proposed wording without quoting what is there. Read `:109`-`:111` first and rewrite what those
paragraphs actually say. If the existing text makes a claim the architect's wording drops or
invents (e.g. "the class reaches no pixel", or the `tests/README.md` citation), keep the file
truthful over the proposed sentence. The reviewer will check this one specifically.

## 1. What must NOT change

Hard boundary for every agent:

- **`active_pane` state stays.** `PaneInner::active_pane: Cell<Option<PaneId>>` (`pane_view.rs:555`)
  is read by `draw_pane` at `:2444`
  (`statusbar_layout::statusbar_text(fg, bg, self.active_pane.get() == Some(pane_id))`), by
  `apply_split` `:1120`, `apply_focus` `:1164`, `detect_closes` `:1248`, `reset_after_restart` `:3332`.
  It is routing and cursor-target state, not the ring.
- **The MRU bookkeeping in `set_active_pane` stays** — `self.active_pane.set(pane)` plus the
  `tree.borrow().leaves().contains(&pane)` / `DrawerState::record_active` block at `:1444-1450`.
  Deleting it silently breaks `<C-w>k` out of the terminal drawer.
- **Every call site of `set_active_pane` stays** (`:657`, `:776`, `:1104`, `:1144`, `:1250`, `:1320`,
  `:2596`), as does `update_active_pane` and its position in `on_render_wake`.
- **The statusbar strip's active/inactive treatment stays exactly as it is.** No replacement
  indicator of any kind — no border, no tint, no scrim, no gutter mark.
- **No assertion is deleted** beyond the single one approved in R1. The two cluster ceilings in the
  preview-link smoke (`LINK_CLUSTER_MAX_W_PX`, `LINK_CLUSTER_MAX_H_PX`) keep their values, and the
  `<C-w>h` step in `DIVIDER_RELAUNCH_SCENARIO` stays in the scenario.
- `tree_view.rs`'s `CURRENT_FILE_CSS` provider and its `install_css()` are untouched.

## 2. Files to create

None.

## 3. Code — `crates/wolf-gnome/src/pane_view.rs` (coder)

Edit in this order so the tree never fails to compile mid-edit:

**a. `:1451-1458` — delete the class-toggle loop.** Remove the trailing block of `set_active_pane`
(`let panes = self.panes.borrow();` and the `for (pid, p) in panes.iter()` add/remove loop). The
function then ends after the `if let Some(pane) = pane { … }` drawer block and takes no `panes`
borrow at all.

**b. `:1435-1436` — `set_active_pane`'s summary.** First line becomes: "Sets the active pane and
records it as the drawer bridge's most-recently-used EDITOR pane." The paragraph below it ("The MRU
is recorded HERE and nowhere else…") is unchanged.

**c. `:3525` — delete `install_pane_css();`** from the top of `build`, and the blank line after it.

**d. `:4152-4170` — delete `install_pane_css` and its whole doc comment.** `gdk` stays imported-by-use
elsewhere (`:2288`, `:2804`, `:3203`, `:3225`); `gtk::CssProvider` and
`gtk::style_context_add_provider_for_display` were fully qualified, so no import to clean up.

**e. `:218-225` — delete `ACTIVE_PANE_CSS` and `ACTIVE_PANE_CSS_DATA`** with their doc comments.

**f. `:49` — module doc.** "…resolves pending pane grids, and updates the active-pane focus ring from
`active_win`" → "…and reconciles the active pane from `active_win`."

**g. `:554` — the `active_pane` field doc.** "The currently active pane (focus ring + cursor target),
if any." → "The currently active pane — the cursor's target and the pane whose statusbar strip draws
at full strength — if any."

**h. `:1298-1299` — `update_active_pane`'s doc.** Drop the trailing clause: "Refreshes the active pane
from `active_win` (fallback: the pane owning the cursor's grid)."

After these edits `grep -n 'ACTIVE_PANE\|wolf-active-pane\|install_pane_css\|focus ring\|focus-ring'`
over `src/` must return nothing.

## 4. Code — `crates/wolf-gnome/src/scrollbar.rs`

### Module docs (coder)

**a. `:21-25`** — delete the `ACTIVE_PANE_CSS` premise; the sentence stands on the surviving reason:

> And a pane leaf must stay a bare `gtk::DrawingArea`: a widget child would need a `gtk::Overlay` or
> `gtk::Fixed` around every leaf, rebuilt and re-parented on every layout change.

**b. `:36-37`** — delete the last sentence of the padding-gutter paragraph ("The margin also clears
the two-pixel active-pane outline, which `outline-offset: -2px` draws over the pane's last four
pixels."). The paragraph ends at "…so it never covers a glyph at any pane width, capped or not."

### `#[cfg(test)] mod tests` (test-writer)

**c. `:632-639`** — delete the second assertion in `the_thumb_lies_wholly_inside_the_right_gutter`,
the `SCROLLBAR_MARGIN_PX >= 4.0` check whose message names `ACTIVE_PANE_CSS_DATA`. Approved per R1.

**d. `:606-618`** — the `#[allow(clippy::assertions_on_constants, reason = …)]` attribute **STAYS**
(the surviving `SCROLLBAR_THICKNESS_PX + SCROLLBAR_MARGIN_PX <= EDITOR_PADDING_PX` is still
const-folded and `-D warnings` is unconditional). Only its `reason` text changes — it currently names
two relations and the ring:

> this test opens with a relation between production constants — SCROLLBAR_THICKNESS_PX plus
> SCROLLBAR_MARGIN_PX fitting inside EDITOR_PADDING_PX — which clippy const-folds to a literal and
> reports as vacuous. Being true at compile time is the point: it is a fact about the constants whose
> whole value is the message it carries the day SCROLLBAR_THICKNESS_PX is widened past the gutter it
> hides in, and it has no runtime input to depend on. Scoped to this ONE function rather than the
> module so a genuinely vacuous constant assertion added anywhere else here is still reported.

**e. `:588-589`** — "The first two assertions are relations between constants and are what make…" →
"The first assertion is a relation between constants and is what makes…". The `FALSIFYING EDIT`
paragraph at `:600-604` is unchanged and still correct (raising `SCROLLBAR_THICKNESS_PX` to 24 still
fires the surviving relation first with the measured message).

## 5. Docs — `crates/wolf-gnome/DECISIONS.md` (coder)

Rewrite three paragraphs in place, then `prettier --write` (`.prettierrc` sets `proseWrap: never`, so
each stays one long line). Present tense only — no "no longer", "previously", "used to".

**a. `:109` →**

> **A display-global CSS provider is deliberately not hoisted up here, and the hoist is a named
> follow-up.** Each build adds one to the default display for
> [`tree_view.rs`](src/README.md#tree_viewrs)'s current-file bold, one per window, so every extra
> window adds another identical provider, inert since the rule resolves the same either way.

**b. `:111` —** see R2. Read the existing text and rewrite it faithfully; do not paste invented
wording.

**c. `:147` →**

> A pane is painted identically whether it is active or not: no scrim, no tint, no border, nothing
> conditional in the grid paint. **The strip's text is the only active-pane signal** — bold at full
> strength in the active pane, both lines scaled by `STATUSBAR_INACTIVE_ALPHA` in the others — and
> macOS draws the same. An edge treatment is the thing to resist: it lands in the padding gutter
> [`scrollbar.rs`](src/README.md#scrollbarrs)'s thumb hides in, and a long thin high-contrast line
> above a narrow pane's text is measured to corrupt `tesseract --psm 11`'s scale estimate for the
> whole pane.

That last clause rehouses the divider smoke's ablation finding: a durable, cross-cutting cost of
re-adding chrome at a pane's edge, which is `DECISIONS.md`'s job. No other document restates it.

## 6. Docs — no change (verified by the architect)

- `crates/wolf-gnome/src/README.md` — the `pane_view.rs` (`:544-562`) and `scrollbar.rs` (`:719-739`)
  entries contain no ring, outline or CSS mention. Do NOT add one: the ring's absence is a
  `DECISIONS.md` fact, and duplicating it here violates one-file-one-owner.
- `crates/wolf-gnome/tests/README.md` — the per-smoke counts at `:121` are unaffected; the new
  assertion lands in a smoke that already OCRs, so the buckets and the total of sixty hold.

## 7. Tests — `crates/wolf-gnome/tests/smoke_headless.rs` (test-writer)

### 7.1 Prose and message sites — thirteen, each with its decision

| # | Site | Decision |
|---|---|---|
| T1 | `:3180` `FOCUS_SCENARIO` shell comment | Reword: "# Let the focus change land (intent drained on wake, routing and the active pane updated)." No step is dead; the sleep still budgets the wake. |
| T2 | `:10350-10362` `LINK_SEARCH_INSET_X_PX` doc | **Constant stays at 10 and must not be reduced.** Its surviving job is keeping the search off both pane boundaries and out of the neighbouring editor pane. Replacement paragraph below the table. Closing sentence of the `BAND_X_INSET_PX` paragraph: "Ten pixels clears a 2 px outline five times over and still leaves the whole link inside" → "Ten pixels clears the boundary and still leaves the whole link inside." **No setup is dead** — the inset is still applied in `link_search_rect` at `:23747`/`:23751`. |
| T3 | `:10364-10373` `LINK_SEARCH_INSET_Y_PX` doc | "The same ring clearance as…" → "The same boundary clearance as…"; "…puts the pane's top outline well outside the rectangle" → "…keeps the pane's own top boundary well outside the rectangle." Do NOT claim it clears the statusbar strip: the derived rect is `y 87..600` and the strip at `SMOKE_FONT_PT` reaches lower than 87. The strip's grey text is already covered by `LINK_CLICK_X`'s note that `is_link_blue`'s blue-over-red margin rejects it. |
| T4 | `:10420-10427` `LINK_CLUSTER_MAX_W_PX` doc | **This is the "only meaningful because of a false positive" case — the ceiling stays at 400.** One clause: "…while a focus ring or any other pane-spanning blue would produce…" → "…while any pane-spanning blue would produce something near the search rectangle's own ~484 px." |
| T5 | `:17604-17609` smoke doc | Replace the ring parenthetical with the general form: "…so a cluster swollen to the whole pane — which any pane-spanning blue-dominant mark would produce, chrome drawn at the pane's own edge being the obvious way to get one — would contain the click constant whatever it was." Rest unchanged. |
| T6 | `:17862-17881` assertion 3's ceiling MESSAGE | Assertion untouched. Two sentences of the message: "the first suspect is `pane_view`'s `ACTIVE_PANE_CSS` focus ring, a 2 px `#3584e4` outline that satisfies the same blue-dominant predicate and would swell the box to the whole pane" → "the suspect is a pane-spanning mark satisfying the same blue-dominant predicate — chrome drawn at the pane's own edge, or a second styled element in the document." KEEP "The containment assertion below would then hold for ANY click point, so this one has to fire first" and KEEP "do NOT raise these ceilings". |
| T7 | `:23734-23736` `link_search_rect` doc | "see [`LINK_SEARCH_INSET_X_PX`] for the ring the horizontal inset clears" → "…for what the horizontal inset clears". |
| T8 | `:25856-25876` `CODEGRID_CHIP_INSET_X_PX` doc | Inset stays at 10. Replace `:25865-25869` with: "Ten pixels is [`LINK_SEARCH_INSET_X_PX`]'s value and clears the same thing: both pane boundaries. What the inset buys here is that the band cannot reach the OTHER pane, whose background is the editor's and is a colour this assertion takes no position on." |
| T9 | `:7646-7681` `DIVIDER_RELAUNCH_SCENARIO`'s closing `<C-w>h` | **The step is NOT dead — keep both `wtype` lines and the `sleep`.** It still moves the active-pane statusbar emphasis (bold, full strength, drawn at the top of the pane directly above the heading) and Neovim's block cursor off the narrow pane the OCR reads. Delete the six-row ring-ablation table at `:7653-7659` and the paragraph at `:7661-7665` (they ablate something the build does not render). Keep the "`<C-w>h` is a pure FocusDir intent … resizes nothing … no-op either way" paragraph verbatim. Keep the "if the OCR assertion fails on DIVIDER_OCR_LEFT instead, the next lever is dvleft.md's body" paragraph with "the ring" → "that treatment". Proposed opener: "# The narrow right pane is what the OCR assertion reads, and `--psm 11` scales a sparse region from everything in it, so that pane is given as little non-text ink as possible: this moves the active-pane treatment — the strip's bold, full-strength filename — and Neovim's block cursor into the WIDE pane. `crates/wolf-gnome/DECISIONS.md` records why nothing else is drawn at a pane's edge, and what one would cost this reading." |
| T10 | `:7184-7226` `DIVIDER_BODY_LINE_COUNT` doc | Constant stays at 24. Four edits. `:7192-7197`: "TWO such neighbours were measured… and the active-pane FOCUS RING, which…" → one neighbour, the `~` column, which this constant answers; drop the "A reader wants the ring first" sentence. `:7203-7206`: "Four controlled ablations … masking out the block cursor, masking out the focus ring and padding the image all left the misread in place" → "Three controlled ablations … masking out the block cursor and padding the image both left the misread in place". `:7215-7218`: drop "for both hazards" and "leave the ring exactly where it is". `:7220`: "With BOTH of those answered, a third cause remained" → "With that answered, a second cause remained". Also drop the banned "now" at `:7213`. |
| T11 | `:28372-28391` "Reading a narrow column" | `:28376`: "All three findings … an end-of-buffer marker column, a focus ring's edge, and a misclassified glyph" → "Both findings … an end-of-buffer marker column and a misclassified glyph". `:28380-28382`: "three separate things … Two are answered by FIXTURE and one by the READER" → "two separate things … One is answered by FIXTURE and one by the READER". Delete the FOCUS RING table row at `:28388`. `:28391`: "The third is why this smoke…" → "The second is why this smoke…". |
| T12 | `:28779-28785` assertion 3's failure message | Assertion untouched. "Two further causes are ANSWERED BY FIXTURE …" → one cause, the `~` column, same parenthetical; "Do NOT lower DIVIDER_RELAUNCH_DRAG_PX for either: it moves neither" → "…for it: it moves the column nowhere, and it narrows the weight gap assertion 1 rests on." Optionally append one clause noting the closing `<C-w>h` keeps the strip emphasis and the block cursor in the WIDE pane. |

T2's replacement paragraph:

> It keeps the search off both pane boundaries: the `gtk::Paned` handle and the editor pane beyond it
> on the left, the window edge on the right. Nothing there is part of the rendered document, and a
> pane-spanning mark inside the rectangle would swell the located cluster to the whole pane and make
> the containment assertion below pass for any click point at all — which is what
> [`LINK_CLUSTER_MAX_W_PX`] fires on.

### 7.2 The new assertion — warranted, and not theater

`pane_view.rs`'s claim that "no smoke observes this outline" was true of the suite as written, not a
statement about what the toolchain can do. `smoke_headless.rs` already carries `decode_capture`
(`:23906`), `Capture::pixel` (`:23573`), `Capture::count_exactly(rect, colour)` (`:23579`) and
`SampleRect` (`:23541`); eight smokes assert on raw pixels with them. The ring is a solid,
axis-aligned, opaque `#3584e4` band along a pane's edge — `count_exactly` reads it directly.

A `#[cfg(test)]` unit assertion is **not viable and must not be attempted**: `install_pane_css` needed
a `gdk::Display`, `pane_view.rs`'s test module is pure-string only, and the crate's `cargo test` runs
with no display. A source-text grep test would be theater. Write neither.

**Where.** Inside the existing `vertical_split_shows_two_panes_under_cage` (`:16051-16151`) — no new
`cage` launch, and its scenario already produces the right page: two panes, the RIGHT one active by
construction (`<C-w>v` focuses the new pane, `:e file B` lands there, then `grim`).

**New items**, placed with the other pixel helpers near `:23540-23600`:

- `PANE_EDGE_PROBE_W_PX: f64 = 4.0` — probe depth from a pane's own edge; documented as spanning both
  possible placements of a 2 px outline offset inward by 2 px, and as sitting well left of the
  scrollbar thumb, whose left edge is `SCROLLBAR_THICKNESS_PX + SCROLLBAR_MARGIN_PX` = 14 px in.
- `PANE_EDGE_PROBE_TOP_INSET_PX: f64 = EDITOR_PADDING_PX` — start one padding gutter below the pane
  area's top edge so the probe cannot read the corner where two edges meet.
- `PANE_EDGE_GUTTER_REF_INSET_PX: f64 = 18.0` — where the reference background pixel is read: inside
  the 20 px `EDITOR_PADDING_PX` gutter `draw_pane` fills with the editor background, and left of the
  thumb's 14 px.
- `ADWAITA_ACCENT_RGB: Rgb8 = (0x35, 0x84, 0xe4)` — GNOME's accent, and a colour this front-end takes
  into no pane, since it takes no colour from the GTK theme into the pane area at all.
- `fn pane_edge_probes() -> (SampleRect, SampleRect)` — right probe `x ∈ [output_w - 4, output_w)`,
  left probe `x ∈ [origin_x, origin_x + 4)`, both `y ∈ [OVERLAY_ORIGIN_Y_EXPECTED + 20, output_h / 2)`.
  Derive every number through `exact_f64` and `pixel_index`, never an `as` cast (the file forbids
  them). Two probes so the assertion holds whichever pane is active, without needing the divider's
  position.

**In the test**, after the existing OCR assertions:

1. **Precondition (non-vacuity).** `let bg = capture.pixel(ref_x, ref_y);` then assert
   `capture.count_exactly(right_probe, bg) > 0`, with a message saying the probe read no pixel of the
   editor's own background, so it is not looking at the pane's padding at all — the window does not
   reach the output's right edge, or the pane origin moved; re-derive from `CAGE_OUTPUT_*_EXPECTED` /
   `OVERLAY_ORIGIN_Y_EXPECTED` rather than moving the probe.
2. **Subject.** Assert
   `count_exactly(right_probe, ADWAITA_ACCENT_RGB) + count_exactly(left_probe, ADWAITA_ACCENT_RGB) == 0`,
   with a message stating the product rule: a pane carries no edge treatment, the strip's text is the
   only active-pane signal, `DECISIONS.md` records why, and a non-zero reading means themed chrome is
   being drawn inside the pane area. Keep the PNG at its path.

**Empirical unknown to confirm on the first green run:** that the window's outermost 4 columns at
`y ∈ [67, 360)` really are the right pane's padding, i.e. precondition 1 passes. If it does not, move
the probe 2 px inward or drop `PANE_EDGE_PROBE_W_PX` to 2 — never delete the assertion, never widen it
to a tolerance. If the probe is fully uniform, record that as a MEASURED count in the doc comment per
this file's idiom; do not upgrade precondition 1 into a full-uniformity assertion without the
measurement.

**Wiring:** none needed. `crates/wolf-gnome` has no `BUILD.bazel` (`tests/README.md:3` — Cargo-only),
and this is an assertion added to an existing `#[test]` in an existing file, so `cargo test` already
runs it. No new test file, no new target.

### 7.3 Required red run (step 7 of the workflow)

The coder must capture the new assertion FAILING with the ring present, and paste the output. Cheapest
route: run the new assertion against the pre-change tree (stash the `pane_view.rs` deletions).
Expected red: a few hundred accent pixels in the right probe (the ring's right edge is a full-height
2 px line), with precondition 1 green.

### 7.4 Behaviours and their smallest falsifying edits

| Behaviour | Test | Smallest falsifying edit |
|---|---|---|
| No accent-coloured chrome at a pane's edge | `vertical_split_shows_two_panes_under_cage`, new assertion | Re-add the CSS rule, the provider install and the class toggle |
| The probe is aimed at the pane's own padding | same, new precondition | `PANE_EDGE_PROBE_W_PX` walking off the window, or `OVERLAY_ORIGIN_Y_EXPECTED` |
| The active pane still drives the strip's bold | `statusbar_layout::tests` row 1 (`:581`) and the statusbar smoke | Invert `self.active_pane.get() == Some(pane_id)` at `pane_view.rs:2444` |
| `set_active_pane` still records the drawer MRU | terminal-drawer smoke legs d/e (`<C-w>k` out, `<C-w>j` in) | Delete the `record_active` block at `:1446-1450` — the guard against over-deleting in §3a |
| The thumb still hides wholly inside the right gutter | `the_thumb_lies_wholly_inside_the_right_gutter` | Raise `SCROLLBAR_THICKNESS_PX` to 24 |
| A swollen blue cluster still cannot vacuously satisfy the link click point | preview-link assertion 3 | Raise `LINK_CLUSTER_MAX_W_PX` past the search rect's ~484 px |

## 8. Implementation order

1. `pane_view.rs`: class-toggle loop (`:1451-1458`) → `install_pane_css();` call (`:3525`) →
   `fn install_pane_css` (`:4152-4170`) → the two consts (`:218-225`). Never leaves a dangling ref.
2. `pane_view.rs`: the four rustdoc corrections. Confirm the §3 grep comes back empty.
3. `scrollbar.rs`: module doc `:21-25` and `:36-37`; then the test's doc, its clippy `reason`, and the
   deleted second assertion. Compile.
4. `smoke_headless.rs`: the twelve prose/message sites T1–T12. Compile with `--all-targets`.
5. `smoke_headless.rs`: the new constants, `pane_edge_probes()`, and the two assertions.
6. `DECISIONS.md`: the three paragraphs; then `prettier --write crates/wolf-gnome/DECISIONS.md`.
7. Red-run demonstration for the new assertion, then the full gate.

## 9. Dependencies

None. No new crate, dev-dependency or devShell tool. `Capture`, `SampleRect`, `count_exactly`,
`decode_capture`, `pixel_index` and `exact_f64` all already exist in `smoke_headless.rs`.

## 10. Lint

- **KEEP** `#[allow(clippy::assertions_on_constants, reason = …)]` on
  `the_thumb_lies_wholly_inside_the_right_gutter`; only the `reason` text changes. Removing it fails
  `-D warnings`.
- No new `#[allow]` in `pane_view.rs`. `set_active_pane` shrinks to two statements and takes no
  `panes` borrow; nothing becomes unused.
- New test code: `exact_f64` / `pixel_index` for every conversion — `clippy::cast_precision_loss` and
  the file's own convention both rule out bare `as`.
- `cargo clippy --workspace --all-targets` lints `tests/*.rs`, so `pane_edge_probes()` must be used or
  `dead_code` fires. Both probes are consumed by the one assertion.
- After editing `DECISIONS.md`: `prettier --write`, then optionally
  `./tools/lint/markdown_check.sh crates/wolf-gnome/DECISIONS.md` (advisory). The rewritten `:147`
  must avoid "no longer", "previously", "used to".

## 11. Verification

All from the repository root inside the devShell (`build.md:25` — the `shellHook` substitutes store
paths into a repo-relative fontconfig template, so a run started elsewhere loses the font pinning
every golden render depends on).

```sh
nix develop --command cargo fmt --all -- --check
nix develop --command prettier --check "**/*.md"
nix develop --command cargo clippy --workspace --all-targets -- -D warnings
nix develop --command cargo test -p wolf-gnome
```

While iterating on one smoke:

```sh
nix develop --command cargo test -p wolf-gnome --test smoke_headless -- vertical_split_shows_two_panes_under_cage --nocapture
nix develop --command cargo test -p wolf-gnome --test smoke_headless -- dragged_divider_weights_survive_relaunch_under_cage --nocapture
nix develop --command cargo test -p wolf-gnome --test smoke_headless -- preview_link_click_repoints_the_pane_under_cage --nocapture
```

**Wall clock.** `cargo test -p wolf-gnome` is dominated by the GTK smokes: sixty of them each launch
the real binary in a real window under `cage`, serialized by a process-wide `CAGE_LOCK` because two
compositors contend for one Wayland socket, several running two full app launches plus multi-second
settles. Expect tens of minutes and no parallel speedup. `require_smoke_env` panics rather than
skipping if `cage`, `wtype`, `grim`, `tesseract` or `wolf-pointer-inject` is off `PATH`, or if
`WOLF_NVIM_BIN` / `WOLF_RG_BIN` are unset — that is a stale devShell, not a product failure; re-enter
`nix develop`.

**Report real results.** The three smokes above are the only ones whose pixels this change can move,
and only in one direction: it removes `#3584e4` ink and reveals the editor background beneath it.
Every colour-fraction band in the suite is inset at least 10 px from a pane boundary
(`LINK_SEARCH_INSET_X_PX`, `CODEGRID_CHIP_INSET_X_PX`) or 80 px (`BAND_X_INSET_PX`), so none sampled
the ring; the two readings that did include a pane edge — the divider relaunch's full-page OCR and the
drawer band — lose a high-contrast line, which can only help `--psm 11`. A red run in any of them is a
finding to diagnose from the retained PNG, never a constant to tune.
