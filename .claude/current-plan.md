# Plan: Three CSS tweaks on top of the baseline grid

## Status: COMPLETE (pending manual visual QA)

- 28 tests pass in `//crates/wolf-core:markdown_render` (cold run, `--nocache_test_results`); `wolf_core_lib_test`, clippy, and rustfmt (including the test file) all green.
- Reviewer: **Approved**, arithmetic independently re-derived. Two minor notes raised and both closed.
- Docs: four READMEs updated; prettier confirms conformant.

### Deltas from the plan as written

1. **28 tests, not the projected 27.** `headings_reserve_anchor_scroll_clearance` was added after the plan: §3 made `scroll-margin-top: 48px` load-bearing (the sole thing keeping an anchor-jumped heading off the pane's top edge) and nothing pinned it — the sweep only checks values that *exist*, so outright deletion was invisible.
2. **`heading_bottom_rule_is_gone_from_base_and_theme`'s guard was strengthened** after review. `!decls.is_empty()` was insufficient: `rule_decls` merges the shared `h1..h6` rule and the theme's `color` rule, so the map stays non-empty even if the per-level rule vanished — the exact case the guard claimed to exclude. Now asserts `font-size` from `HEADING_SCALE`.
3. **`markdown/README.md` omitted the third edge rule.** base.css ships the naive `> *:last-child` as a partial fallback alongside the two general selectors; the README documented only two. Pre-existing divergence from the earlier grid work, now closed.
4. **One base.css inline comment outside §7's listed ranges** asserted the heading's margin sits "ON TOP of the body's 48px padding" — made false by `padding: 0`. Corrected.

### Verified traps (both were live)

- **Stale allowlist:** `COMPENSATION_ALLOWLIST` confirmed by reading to hold exactly three entries. A leftover `("h1", "padding-bottom", 23)` would have silently rubber-stamped a future off-grid value.
- **Theme cascade:** zero `border-bottom-color` / `border-left-color` remain in either palette block; `th`/`td` and `hr` colors survive. The coder confirmed the theme deletions alone were invisible to the suite — had the color declarations been left behind, nothing would have failed today.

### Design consequences — implemented as requested, flagged for the user

- **Blockquotes lose their only structural marker.** Sole remaining cue is the theme's dimmer text color plus a 24px indent that is *smaller* than the 32px list indent, against a now-flush body edge.
- **h1/h2 lose 24px of air below** (72px → 48px box) on top of losing the rule, so sections read tighter.
- **Flush-to-edge text** touches the pane's left/right pixel and the first line touches the top; on a wide pane nothing constrains line length. `pre` fares best (its own 24px padding).

### Outstanding

Manual visual QA still not performed — same as the grid work. The tests pin declared values, not rendered geometry.

### Noted, not acted on

- Nothing pins `scroll-margin-top`'s *effect*, only its declared value.
- The whole-document `!html.contains("border-bottom-color")` checks also trip on those literal strings appearing in base.css header-comment prose (the comment ships in the rendered document). Deliberate; a warning comment was added at the `border-bottom-color` site but not the `border-left-color` one.
- `tools/lint` still does not cover `crates/*/tests/`; no `.prettierignore` exists.

## Problem statement

Three requested changes to `crates/wolf-core/src/markdown/base.css`, layered on the completed 24px baseline grid:

1. **Remove the bottom border from h1 and h2.**
2. **Keep blockquote's left indent, remove its `border-left`.**
3. **Remove the padding around the body** (`padding: 48px 40px` on `body.wolf-markdown`).

Not isolated deletions — each collides with a grid, cascade, or margin-collapse invariant.

## Grid invariants still in force

- Unit **24px**, `--wolf-baseline: 24px`. Invariant: every vertical margin is 0 or a multiple of 24, every border-box height a multiple of 24. Closed under margin collapsing.
- `line-height: 24px` is a **length**, not a ratio. Do not reintroduce a ratio.
- Scale: h1 32/48, h2 24/48, h3 20/24, h4 18/24, h5 16/24, h6 14/24. Heading margins h1/h2 48/24, h3 48/0, h4–h6 24/0.
- Literal px, not `calc()`. Horizontal dimensions deliberately not grid-constrained.
- Surviving compensations: `hr` absorbs 1px into a 24px border-box height; cells `12px 16px 11px` for a 48px collapsed row; `table { margin: 0 0 23px }`.
- Do NOT touch `.wolf-src` or `math.css`.

---

# Architect's plan

## 1. Exact replacement values

### h1 / h2 (base.css ~253–269)

Delete BOTH `padding-bottom: 23px;` and `border-bottom: 1px solid;` from each rule. **Do not write `padding-bottom: 0`** — h3–h6 declare no padding either, the UA default is already 0, and an explicit zero is noise the sweep must then parse.

| | h1 | h2 |
| --- | --- | --- |
| content (line-height) | 48 = 2B | 48 = 2B |
| padding-bottom | 0 | 0 |
| border-bottom | 0 | 0 |
| **border-box height** | **48 = 2B ✓** | **48 = 2B ✓** |
| margin-top | 48 = 2B ✓ | 48 = 2B ✓ |
| margin-bottom | 24 = 1B ✓ | 24 = 1B ✓ |
| full footprint | 120 = 5B ✓ | 120 = 5B ✓ |

`margin-bottom: 24px` **stays** — do not zero it to match h3–h6. The type-scale table never mentioned padding, so it needs no edit.

### blockquote (~334–338)

Delete `border-left: 4px solid;` **only**. `margin: 0 0 24px` and `padding: 0 24px` unchanged. Vertical padding is 0 and no border remains on any edge, so border-box height = content height = a sum of on-grid child boxes. ✓

### body.wolf-markdown (~205)

`padding: 48px 40px` → **`padding: 0;`** — keep an explicit declaration rather than deleting the line. Flush-to-edge is a deliberate, contested decision, not an absence of one; an explicit `0` gives the test something positive to pin so nobody silently restores an inset. `px("0")` → `Some(0)`, `0 % 24 == 0`, and `shorthand_block_edges("0")` expands cleanly. Add a one-line comment that flush edges are intentional and any inset belongs to the preview pane chrome.

## 2. Blockquote indent: keep 24px

Leave `padding: 0 24px` exactly as is. The padding **is** the indent; 24px is already the documented "24px blockquote inset" in both base.css's header and markdown/README.md, so 24px causes zero doc churn while 28px would introduce a number matching nothing else in the stylesheet.

## 3. `scroll-margin-top: 48px` — keep the value, change the rationale

The old rationale ("equals the body's top padding") is gone, but the property is now **more** load-bearing: with `padding: 0` there is nothing else keeping an anchor-scrolled heading off the pane's top edge. Without it, clicking an in-document anchor jams the target heading's cap-height flat against the top pixel. 48px = 2B stays on-grid and is swept by `VERTICAL_LONGHANDS`.

New rationale for the docs: "2 baselines of clearance above a heading scrolled into view by an anchor link. The body itself has no padding, so this is the only thing keeping a jumped-to heading off the pane's top edge."

## 4. The `:first-child` reset — still required, new rationale

**Keep both edge rules exactly as spelled.** Walk the behavior with `padding: 0`:

**Does the margin collapse out through the body's top edge?** Parent–child collapsing requires no `border-top`, no `padding-top`, no clearance, no BFC. With `padding: 0` and no border, `body.wolf-markdown` satisfies all of them. So if the opening heading were the body's first in-flow child, its 48px `margin-top` would collapse with the body's zero top margin and be placed **outside** the body's border box.

**Does it escape the body background?** Formally yes — but **there is no visible color gap** and the docs must not claim one. Neither base.css nor the theme sets a background on `html`, so the root is transparent and per CSS background propagation the **body's** background paints the entire canvas. `#1b1b1b` / `#f4f4f4` covers the escaped margin. The symptom is purely positional: the whole document is displaced 48px down. The root element's margins never collapse, so it propagates no further.

**Do the `.wolf-src` anchors block the collapse?** This is the subtlety, and it cuts against intuition. The body's first element child is a zero-size **inline** span, not a block. Inline content at the top of a block container is wrapped in an **anonymous block box**, and that box — not the `<h1>` — is the first in-flow block-level child. Whether the h1's margin reaches the body's top edge then depends on whether that anonymous box is self-collapsing, which depends on the *anonymous container's* inherited strut (24px from the body), not the span's own `line-height: 0`. That is engine-dependent territory.

**It does not matter which way it resolves, and the coder must not try to reason it out.** Both outcomes are wrong without the reset:
- collapse-through → the body box is displaced 48px, heading 48px from the top;
- no collapse-through → the 48px sits inside the body between the anonymous box and the h1, heading again 48px from the top.

**New rationale for base.css EDGE RULES and markdown/README.md.** The old wording ("a block's own margin does NOT collapse through the body's padding, so the first block would sit 48 + 48 = 96px down") is now false in **both its premise and its number**. Replace with: the body has no padding, so an unreset 48px `margin-top` on the opening block either collapses out through the body's top edge (displacing the whole document 48px — the body's background propagates to the canvas, so this reads as position, not a color gap) or, if a leading anchor's anonymous block box blocks the collapse, sits inside the body between them. Either way the opening heading is 48px down and flush-to-edge is defeated. **48px is still a multiple of 24, so the grid sweep still cannot see this bug** — only the DOM-level edge tests can. Mirrored argument covers the last-block rule.

Do NOT simplify any edge rule to bare `:first-child`/`:last-child`; anchor-skipping is unchanged in purpose.

## 5. Theme audit (themes/default.css) — complete

**Dark block:**
- **Lines 36–39** — the whole rule `h1, h2 { border-bottom-color: #3a3a3a }`. Delete the **entire rule**; it has no other declaration and would be left empty.
- **Line 56** — `border-left-color: #3a3a3a` inside the blockquote rule (54–57). Delete **this declaration only**; the rule keeps `color: #a8a8a8`.

**`@media (prefers-color-scheme: light)` block:**
- **Lines 87–90** — the whole rule `h1, h2 { border-bottom-color: #d0d0d0 }`. Delete entirely.
- **Line 107** — `border-left-color: #d0d0d0` inside the light blockquote rule (105–108). Declaration only; keeps `color: #656565`.

**Confirmed unaffected — do not touch:** `th, td { border-color }` (59–62 / 110–113, load-bearing for the 2B row pitch); `hr { border-top-color }` (68–70 / 119–121, asserted by an existing test); `code`/`pre` (background/color only); the `h1…h6 { color }` rules (27–34 / 78–85) — h1/h2 must keep matching a non-empty theme rule. Header comment (1–19) discusses only `hr`; **no header edit needed**, and "only sets color/background/border-color" stays true.

**Hex safety:** `#3a3a3a` and `#d0d0d0` each remain in use (th/td, hr), so the hr test's substring assertions still pass. `default_theme_carries_dark_and_light_palettes` asserts only `#1b1b1b`, `#f4f4f4`, `#0969da`, `#111111`, `color-scheme: light dark` — all untouched.

## 6. Test changes (markdown_render.rs)

### 6.1 Value change
`body_line_height_is_a_length_not_a_ratio` (~743–748): expected padding `Some("48px 40px")` → `Some("0")`. Rewrite the message: the body is deliberately flush to the pane edges; `0` is trivially on-grid; the assertion exists so a "comfortable" padding is not silently reintroduced. line-height / font-size / length-not-ratio assertions unchanged.

### 6.2 DELETE outright
`heading_border_is_compensated_into_padding` (~801–845). Its premise disappears entirely. Do not adapt it into a negative assertion in place — write the two focused replacements below.

### 6.3 ADD two
Both follow the `hr_occupies_whole_baselines_and_is_painted_by_its_border` pattern: base.css structure via `rule_decls`, theme half via plain substring checks against the whole document (the light palette is inside the one `@media`, which the crude parser deliberately cannot address).

- **`heading_bottom_rule_is_gone_from_base_and_theme`** — for h1 and h2, the merged `rule_decls` map contains neither `border-bottom` nor `padding-bottom`; and the document contains no `border-bottom-color` substring anywhere (covers **both** palette blocks in one check — exactly the dead-theme-declaration hazard the `hr` bug taught). Message: reintroducing a bottom rule requires re-adding the 23px compensation *and* an allowlist entry, since a bare 1px border on a 48px content box makes the heading 49px tall.
- **`blockquote_keeps_its_indent_without_a_border`** — blockquote's merged decls have `padding == "0 24px"` and `margin == "0 0 24px"` and no `border-left` key; document contains no `border-left-color` substring. Message: the 24px padding **is** the indent (the only thing holding the quote in from the flush body edge), and its vertical `0` is why blockquote needs no border compensation.

Net count: **26 → 27**.

### 6.4 COMPENSATION_ALLOWLIST — remove two entries
Delete `("h1", "padding-bottom", 23)` and `("h2", "padding-bottom", 23)` **together with their explanatory comments** (~519–522). Keep the three table/cell entries.

**Not bookkeeping.** The sweep checks `s == selector && p == prop && v == parsed`, so a leftover h1 entry would silently wave through any future `h1 { padding-bottom: 23px }` — with no border to compensate, a plain 1px-per-heading grid violation and exactly the drift the sweep exists to catch. The allowlist's design is "an entry forces its author to name the border it pays for"; an entry naming a border that no longer exists inverts that into a rubber stamp.

### 6.5 GRID_SELECTORS — no change
Verify every entry still resolves non-empty: `body.wolf-markdown` (still has `--wolf-baseline`, margin, `padding: 0`, fonts, line-height, word-wrap + theme colors); `h1`/`h2` (font-size, line-height, margins, shared scroll-margin-top/font-weight, theme color); `blockquote` (margin, padding, theme color). **Add and remove nothing.**

### 6.6 BODY_PADDING_TOP — delete the constant
Delete `const BODY_PADDING_TOP: i32 = 48;` (~1229) and its doc comment. Redefining it to `0` leaves a constant named for a padding that no longer exists, and both uses are in failure messages being rewritten anyway.

- `first_child_margin_reset_reaches_the_opening_heading` message (~1800–1824): rewrite the "Why" paragraph per §4. Keep the closing observation, corrected: 48px is still a multiple of 24, so the sweep passes and only this test sees the bug. Preserve the selector guidance and the "do NOT touch `.wolf-src`" warning verbatim.
- `first_child_margin_reset_survives_a_multi_anchor_leading_run` message (~1875–1894): same substitution; "the SAME `{BODY_PADDING_TOP}` + 48 = 96px gap" → "the SAME 48px displacement". `MIN_LEADING_ANCHORS` sanity check and narrow-selector explanation unchanged.

### 6.7 Comment/message text now false
- `BORDER` const doc (~427–429): "(h1/h2 `border-bottom`, th/td `border`)" → cite only th/td (and hr). Const stays; still used.
- Edge-reset section banner (~1188–1224): contains "96px" and "margins do not collapse through padding" — rewrite both per §4. The anchor displacement explanation, selector shapes, and "pins the OUTCOME" framing are unchanged.
- Sweep comment (~1108): "except the five documented compensations" → three.
- `last_child_margin_reset_reaches_the_closing_block` (~1936–1938): "below the body's own bottom padding" → "below the last line of content, at the pane's bottom edge".
- `blockquote_last_child_margin_is_zeroed` (~1015–1048): **no change** — its reasoning is about vertical padding only and stays true.

## 7. Docs

### base.css header (authoritative spec)
- ~66–67: "descenders collide with the rule beneath" now false. Keep the surviving half of the h2 justification (at ratio 1.0 wrapped lines nearly touch), drop the rule clause.
- ~74–84 BORDER COMPENSATION: delete the entire h1/h2 paragraph and its 72px = 3B arithmetic. Keep `hr` and `table th/td` verbatim.
- ~104–106: blockquote no longer has a border at all, so the "horizontal-only" phrasing is stale. `pre`/inline `code` clauses stand.
- ~108–118 EDGE RULES opening: replace the "does NOT collapse through the body's padding … 96px" argument with §4. Everything from "The obvious spelling … is INERT here" onward is **unchanged**.
- ~155–159 HORIZONTAL DIMENSIONS: drop "the body's 40px side padding"; keep 32px list indent, 16px cell padding, 24px blockquote inset. Add that the body is flush by design.
- ~167 "the compensated values 23/12/11" still accurate (table 23, cells 12/11). No change.
- ~25–47 (unit, invariant, length-not-ratio) and the scale table ~51–58: **unchanged**.

### markdown/README.md
L10 scroll-margin-top rationale → §3. L33 descenders clause. L35 delete the h1/h2 compensation sentence, keep hr/cells/table, rewrite the blockquote "horizontal-only" clause. L37 edge-rules opening → §4. L41 remove "The body's 40px side padding", note flush body. L39/43/47/49 unchanged.

### crates/wolf-core/README.md
L133 "(2 baselines, matching the body's top padding)" → "(2 baselines of clearance; the body itself has no padding, so this is the only thing keeping a jumped-to heading off the top edge)".

### crates/wolf-core/src/README.md
L111 same phrase, same replacement.

### crates/wolf-core/tests/README.md
L117 "26 tests" → 27. L119 four corrections: (a) padding `48px 40px` → `0`; (b) delete the `heading_border_is_compensated_into_padding` sentence, describe the two replacements; (c) "five compensated values" → three (cells 12/11, table 23); (d) the "96px instead of 48px" edge-rule framing → 48px-instead-of-flush.

`prettier --write` every edited `.md`. **Never** prettier `.css` under `src/markdown/`.

## 8. Implementation order

1. **themes/default.css** — the four deletions. Alone this is visually a no-op (borders still exist, uncolored → fall back to `currentColor`); no test asserts those colors.
2. **base.css declarations** — the four edits. Suite now fails in exactly the expected places. Note the allowlist entries go *inert* rather than failing — eyeball that the sweep is green for the right reason.
3. **markdown_render.rs** — §6 in full. Suite green.
4. **base.css header comment** — §7.
5. **The four Markdown docs**, then `prettier --write`.

## 9. Dependencies / lint

None. No new files, no BUILD.bazel change (`compile_data` already globs `src/markdown/**/*.css`). No new Rust items beyond two `#[test]` fns matching existing shapes. `BODY_PADDING_TOP`'s deletion removes the constant with both uses, so no `dead_code` warning. `BORDER` remains used. No `#[allow(...)]` needed.

## 10. Risks

1. **Flush-to-edge readability — largest, and intentional.** Body text, tables, and headings touch the left/right pixel of the pane; the first line touches the top. On a wide pane it also removes the last constraint on line length. `pre` fares best (its own 24px padding). User explicitly asked; implement. Note it in the CSS so the next reader knows it is a decision, not an omission.
2. **Blockquotes lose their only structural marker.** After the border deletion the sole cue is the theme's dimmer text color plus a 24px indent *smaller* than the 32px list indent — a quote reads as a slightly-under-indented grey paragraph, and against a flush body edge the indent alone carries all the meaning. Real legibility regression. **Flagging only; no action without the user's word.**
3. **Headings lose their section rule.** h1/h2 become distinguishable from h3 only by size and leading. The 72px → 48px shrink also removes 24px of air *below* every h1/h2, so sections read noticeably tighter even though `margin-bottom` is untouched.
4. **The stale-allowlist trap (§6.4)** — highest-consequence way to get this wrong; fails silently and permanently.
5. **The theme cascade trap (§5)** — leaving the color declarations is harmless *today* (a color on a non-existent border paints nothing), which is what makes it dangerous: a loaded gun for whoever next adds a border. Same failure mode as the `hr` bug. The two new substring checks are what make it catchable.
6. **Crude parser `@media` truncation.** `stylesheet_rules` truncates at the first `@media`; deleting a whole rule from the light block does not change brace balance in a way the helpers observe. No parser change needed. **Do not add any `@media` to base.css** — that would break every base.css assertion at once.
7. **math.css untouched and unaffected** — styles `<math>` internals only, sets no body padding or h1/h2/blockquote borders. Vendored; do not edit.
8. **`.wolf-src` untouched.** Guarded by `scroll_sync_anchors_stay_zero_sized`. Per §4, if collapse behavior looks surprising in the browser, the fix is still never on the anchor side.
9. **`table { margin: 0 0 23px }` residual unchanged** — re-exposes 1px whenever the next block's `margin-top` exceeds 23px. This change touches no `margin-top`, so the documented residual stays correct. No edit.

---

## Workflow log

- [x] Problem statement written
- [x] Architect plan
- [x] Tests updated
- [x] Implementation + tests passing (28/28, cold run)
- [x] Review (approved; two notes closed)
- [x] Documentation
- [ ] Manual visual QA — see "Outstanding" above
