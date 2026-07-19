# Plan: Baseline grid for Markdown preview CSS

## Status: COMPLETE (pending manual visual QA)

- 26 tests pass in `//crates/wolf-core:markdown_render`; `wolf_core_lib_test`, clippy, and rustfmt (including the test file) all green.
- Reviewer: **Approved**. First pass returned "Needs revision" on two blocking items, both since fixed.
- Docs: all four stale READMEs rewritten; prettier confirms conformant.

### Implementation deltas from the plan as written

1. **The `:first-child`/`:last-child` edge rules from §2 were inert as specified.** `MarkerInjector` emits `wolf-src` anchor spans as the body's first and last element children, so the naive pseudo-classes matched an anchor, not the first real block — a document opening with `# Title` rendered at 96px from the top instead of 48px. base.css now ships general anchor-skipping forms, each in its own rule so an engine lacking complex `:not()` or `:has()` drops only that rule rather than invalidating a shared comma list:
   - first: `body.wolf-markdown > *:not(.wolf-src):not(*:not(.wolf-src) ~ *)`
   - last: `body.wolf-markdown > *:not(.wolf-src):not(:has(~ *:not(.wolf-src)))`
   - The naive `> *:last-child` survives as a documented partial fallback (correct only when the source lacks a trailing newline). The naive `> *:first-child` was removed.
   Both general forms are unbounded in anchor-run length. An intermediate bounded fix (`.wolf-src:first-child + *`) handled only a one-anchor run and was replaced.

2. **The table residual's trigger is broader than §3.3 stated.** Not "a table followed by a heading" — it is any following block whose `margin-top` exceeds 23px: every heading (h1–h3 at 48px, h4–h6 at 24px) **and `hr`** (24px). A following paragraph, list, `pre`, or blockquote (margin-top 0) is exact. Corrected in base.css, the test message, and the README.

3. **Test count is 26, not the ~8 §6 projected** — 12 pre-existing + 10 grid + 4 edge-rule/anchor tests.

4. **`markdown.rs:145`** carried pre-existing doc drift (`<style>{BASE_CSS}{theme_css}</style>` vs. the five-part `format!`); corrected while the file was open. Likewise `tests/README.md`'s stale `MARKER_COLUMN_STRIDE` privacy claim and the test file's own header repeating it — the const is now `pub`.

### Outstanding — not verifiable by any automated test here (plan §7)

Manual visual QA has NOT been performed. Highest-consequence item: **confirm `hr` renders as a hairline, not a solid 24px bar** (the theme guard test covers the cascade, but not the rendered result). Also: the fixed 24px line-height now inherits into `<math>` where a unitless 1.6 previously scaled, so a tall display equation may clip or overlap — pre-approved escape hatch is `body.wolf-markdown math { line-height: normal }` in **base.css**, never in math.css. Plus optical quality, real margin-collapse adjacencies, and the collapsed-border table arithmetic in WebKit.

### Noted, deliberately not acted on (out of scope)

- `tools/lint` does not cover `crates/*/tests/` — no clippy or rustfmt in CI over the entire tests tree. The test file is currently clean (verified manually), but nothing keeps it that way.
- No `.prettierignore` exists; the "don't prettier `markdown/*.css`" rule is convention only, so `prettier --write .` or format-on-save would reformat `base.css`.

## Problem statement

The Markdown preview stylesheet (`crates/wolf-core/src/markdown/base.css`) uses an em-based vertical rhythm that does not snap to any consistent grid. Rewrite it so every vertical dimension sits on a CSS-only baseline grid.

## Agreed design constraints (from the user)

- **Baseline unit: 24px** (16px / 1.5). Define `--wolf-baseline: 24px` on `body.wolf-markdown`.
- Every vertical dimension must be a whole multiple of the unit. Replace the em-based rhythm.
- h1–h6 all need explicit `font-size` AND `line-height` on grid multiples. h3–h6 currently fall through to UA defaults — fix that discontinuity.
- Borders that add height must be compensated by subtracting the border width from adjacent padding.
- `pre`/code: `line-height` and padding on grid multiples.
- **Accept drift** from images and display MathML. NO JS re-snapping. CSS-only.
- Do NOT touch `.wolf-src` scroll-sync anchor rules.
- Do NOT change colors. `themes/default.css` out of scope **except where a border-width interacts with the grid** (this carve-out is invoked — see `hr`, §4.3).

## Repo requirements (CLAUDE.md)

- `crates/wolf-core/src/markdown/README.md` must document the grid and its unit.
- `prettier --write` any edited Markdown; no hard line breaks within a paragraph. Do NOT prettier any `.css` under `markdown/`.
- Prefer real integration tests over mocks. Existing tests must pass.
- Verify `crates/wolf-core/BUILD.bazel` `compile_data` globs (build.md:89).

---

# Architect's plan

## 1. The governing invariant (goes in the base.css header comment)

> **Every element's vertical margin is 0 or a whole multiple of 24px, AND every element's border-box height (content + padding + border) is a whole multiple of 24px.**

Closed under margin collapsing: collapsing takes `max(a, b)`, and the max of two multiples of 24 is a multiple of 24. This is why compensation always folds the border into *padding* or an *explicit height* — never into a margin.

Two supporting decisions:

**(a) `body.wolf-markdown` sets `line-height: 24px` — a LENGTH, not a unitless number.** Most load-bearing decision in the rewrite. A unitless `1.5` re-multiplies against each descendant's own `font-size`, so inline `<code>` at `0.875em`, `<small>`, or superscripts would each yield a different line box and break the grid. A length inherits as the *computed* 24px regardless of descendant font-size. Cost: elements needing different leading must say so — only `h1`/`h2` (48px) and `pre` (24px, set anyway).

**(b) `box-sizing: border-box` is already inherited globally** (existing `html`/`*` rules — keep verbatim). This lets `hr` absorb its rule into an explicit `height`. No effect on `h1`/`h2`/`td`, which have no explicit height and must be compensated by hand.

## 2. The type scale (B = 24px)

| Element | font-size | line-height | arithmetic | margin-top | margin-bottom |
| --- | --- | --- | --- | --- | --- |
| body / `p` | 16px | 24px | 24 = 1×24 → 1B | `p`: 0 | `p`: 24px (1B) |
| `h1` | 32px | 48px | 48 = 2×24 → 2B (ratio 1.5) | 48px (2B) | 24px (1B) |
| `h2` | 24px | 48px | 48 = 2×24 → 2B (ratio 2.0) | 48px (2B) | 24px (1B) |
| `h3` | 20px | 24px | 24 = 1×24 → 1B (ratio 1.2) | 48px (2B) | 0 |
| `h4` | 18px | 24px | 24 = 1×24 → 1B (ratio 1.33) | 24px (1B) | 0 |
| `h5` | 16px | 24px | 24 = 1×24 → 1B (ratio 1.5) | 24px (1B) | 0 |
| `h6` | 14px | 24px | 24 = 1×24 → 1B (ratio 1.71) | 24px (1B) | 0 |

**Why these sizes.** `h1: 32px` / `h2: 24px` preserve today's rendered sizes exactly (`2em`/`1.5em` of 16px) so this is not also a visual resize of the two most-seen headings. h3–h6 today are UA defaults — 18.7 / 16 / 13.3 / 10.7px, where h5 equals body text and h6 is illegibly smaller. The `20/18/16/14` ramp is smooth and bottoms out legibly; h6 is distinguished by weight 600, not by being tiny.

**Why h2 is 24px/48px (ratio 2.0).** The only grid-legal alternatives at 24px font are 24px (ratio 1.0 — wrapped lines nearly touch, descenders collide with the border) or 48px. 48px reads as deliberate air around a section rule.

**Why the margin pattern reads correctly.** Every heading has strictly more space above than below (48>24, 48>0, 24>0). h3–h6 with `margin-bottom: 0` *hug* the content they introduce — correct proximity signal. h1/h2 keep 24px below because they carry a rule that needs to breathe. Top-margin step (2B for section-level h1–h3, 1B for h4–h6) maps to hierarchical weight.

**Margin collapsing helps:** `p{margin-bottom:24px}` + `h2{margin-top:48px}` collapses to `max(24,48)=48` — clean 2B section gap with no `+` selector special-casing.

**Edge rules (new, required):**
- `body.wolf-markdown > *:first-child { margin-top: 0 }` — else a doc opening with `# Title` gets 48px padding PLUS 48px margin (margins don't collapse through padding) = 96px gap.
- `body.wolf-markdown > *:last-child { margin-bottom: 0 }`.

## 3. Border compensation

### 3.1 h1 / h2 bottom border

Target: the padding+rule band occupies exactly 1B. `24 − 1 = 23px` padding.

| | before | after |
| --- | --- | --- |
| h1 content | 1.25×32 = 40px | 48px |
| h1 padding-bottom | 0.3em = 9.6px | **23px** |
| border-bottom | 1px | 1px |
| **h1 total** | 50.6px (2.1B) | 48+23+1 = **72px = 3B** ✓ |
| h2 content | 1.25×24 = 30px | 48px |
| h2 padding-bottom | 0.3em = 7.2px | **23px** |
| **h2 total** | 38.2px (1.6B) | 48+23+1 = **72px = 3B** ✓ |

Both reach 72px from different font sizes — intentional; a section rule occupies a consistent 3B band regardless of level.

### 3.2 `hr` — the one change that reaches into themes/default.css

Current: `height:1px; border:0; margin:1.5em 0`, rule painted by the theme's `background-color`. Total = 24+1+24 = **49px**, uncompensated, and margins alone can't fix it (an `hr` before an `h2` collapses to 48 and re-exposes the 1px).

Fix: explicit **24px height, rule drawn as `border-top`**. With inherited `border-box` + explicit height, the 1px is absorbed structurally.

| | before | after |
| --- | --- | --- |
| height | 1px | **24px** (border-box) |
| rule drawn by | `background-color` (theme) | **`border-top: 1px solid`** (color from theme) |
| background | painted bar | `transparent` (base.css) |
| margin | `24px 0` | **`24px 0 0`** |
| **total** | 24+1+24 = 49px ✗ | 24 + 24 + 0 = **48px = 2B** ✓ |

The rule sits at the top edge of the 24px box — one baseline below the preceding block, one baseline of clear space beneath.

**This requires the paired theme edit.** `themes/default.css` must change `hr { background-color: <c> }` → `border-top-color: <c>` in BOTH the dark block and the `@media (prefers-color-scheme: light)` block, preserving `#3a3a3a` and `#d0d0d0` verbatim. It is a color-property *move*, not a color change. Emitted after base.css, so a leftover `background-color` wins the cascade and paints a solid 24px bar.

### 3.3 Table th / td

`border-collapse: collapse` → adjacent rows share one border; an N-row table has N+1 horizontal lines. Target 2B = 48px per row.

| | before | after |
| --- | --- | --- |
| line-height | 25.6px | 24px |
| padding-top | 0.4em = 6.4px | **12px** |
| padding-bottom | 0.4em = 6.4px | **11px** |
| content+padding | 38.4px | 12+24+11 = **47px** |
| shared border | 1px | 1px |
| **row pitch** | ~39.4px (1.6B) | **48px = 2B** ✓ |

Whole table = `47N + (N+1)` = **48N + 1**. The trailing 1px is the table's own bottom outer border with no row to share with; absorb it in `table { margin: 0 0 23px }` → `48N + 24` = exactly `2N+1` baselines ✓. The 12/11 asymmetry is imperceptible.

**Documented residual:** a table immediately followed by a heading collapses 23px against 48px → 48, re-exposing the 1px. Constant, non-accumulating. Note in the header and README.

### 3.4 No compensation needed

- `blockquote`: `border-left` is horizontal. Vertical padding 0, so height is entirely its children's.
- `pre`: no border.
- inline `code`: an inline box's vertical padding paints background but does not grow the line box. Layout-neutral.

## 4. Decisions

### 4.1 Literal px, NOT `calc(var(--wolf-baseline) * n)`

Declare `--wolf-baseline: 24px` once on `body.wolf-markdown`; write literal px everywhere else. Because:

1. **Auditability.** The sweep test (§6.9) asserts values are multiples of 24. It can do that against `48px`. Against `calc(var(--wolf-baseline) * 2)` it could only string-match syntax — proving nothing about the number while being more brittle. Literals make the grid machine-checkable; `calc()` makes it un-checkable.
2. **Compensated values aren't clean multiples anyway** — `23px`/`12px`/`11px` would become `calc(var(--wolf-baseline) - 1px)`, mixing idioms and making the compensation harder to see.
3. **Verbosity/bytes.** 32 chars vs 4, ×~25 declarations, in a file embedded verbatim into every re-render.
4. **Buys no configurability today.** One `PreviewTheme` variant, no user config surface. Nothing can override it.

Mitigation: a **required header comment tabulating the full scale + compensation arithmetic**, plus the `--wolf-baseline` declaration as the documented token. Do NOT add per-declaration `/* 2B */` comments — the header table is the single source of truth.

### 4.2 `pre` line-height: **24px**

Reject 12px outright: at a 14px code font, 12px leading is less than the font size — lines would overlap. 12px is the only tighter grid-legal value, so there is no viable tighter option.

`pre { font-size: 14px; line-height: 24px; padding: 24px; margin: 0 0 24px }`, `pre code { font-size: inherit }`. Total = `24 + 24n + 24` = `24(n+2)` — always a multiple, **zero compensation** (no border). Code lines share the body baseline exactly.

**Bug to fix while here:** `code` sets `font-size: 0.9em` and `pre code` *also* sets `0.9em`, compounding to `0.81em` inside `<pre>`. Set size once on `pre`, use `inherit` on `pre code`. Keep inline `code { font-size: 0.875em }` relative so inline code in an h1 scales — harmless, since line-height is an inherited length.

### 4.3 Body padding `2rem 2.5rem` → **`48px 40px`** (must change)

`2rem` = 32px = 1.33×24 — off-grid, and it shifts the whole document before the first block. `48px` = 2B (24px would feel cramped for a preview pane). Horizontal `40px` is `2.5rem` unchanged — **horizontal dimensions are deliberately NOT grid-constrained**; say so in the header so nobody "fixes" it later.

`rem`→`px` also removes a real hazard: `rem` resolves against a root font-size this document never sets, so a WebKit minimum-font-size preference or OS text scaling could silently detune the grid. Every value in the rewritten file should be px.

`scroll-margin-top` on h1–h6: `2rem` → **`48px`**, keeping it equal to the body top padding (its original intent) and on-grid. Two READMEs state `2rem` and must be updated.

## 5. File-by-file changes

### 5.1 `base.css` — rewrite

**Header comment:** replace the 2-item "It owns two things" block with a 3-item version adding the 24px baseline grid: the unit and `--wolf-baseline` token; that `line-height` is a LENGTH not a ratio, and why; the invariant from §1; the scale table from §2; the compensation arithmetic from §3 with both documented residuals (table-then-heading 1px; images/display MathML); that horizontal dimensions are NOT grid-constrained; the literal-px-over-calc rationale.

**Rules** (keep `.wolf-src`, `html`, `*`/`::before`/`::after` **byte-for-byte unchanged**):

- `body.wolf-markdown` — `margin:0; padding:48px 40px; --wolf-baseline:24px; font-size:16px; line-height:24px`; font stack unchanged; `word-wrap:break-word`.
- `body.wolf-markdown > *:first-child { margin-top: 0 }` (new)
- `body.wolf-markdown > *:last-child { margin-bottom: 0 }` (new)
- `h1..h6` shared — `font-weight:600; scroll-margin-top:48px`. **Remove** margin-top/margin-bottom/line-height from the shared rule.
- Six per-level rules with §2 values. h1/h2 additionally `padding-bottom:23px; border-bottom:1px solid`.
- `p { margin: 0 0 24px }`
- `a` / `a:hover` — unchanged
- `code` — font stack unchanged; `font-size:0.875em; padding:2px 4px; border-radius:6px`
- `pre` — `font-size:14px; line-height:24px; padding:24px; margin:0 0 24px; overflow:auto; border-radius:6px`
- `pre code` — `padding:0; font-size:inherit; background:transparent`
- `blockquote` — `margin:0 0 24px; padding:0 24px; border-left:4px solid`
- `blockquote > :last-child { margin-bottom: 0 }` (new — else the quote is 24px taller than its content, on-grid but lopsided against the 0 top padding)
- `table { border-collapse:collapse; margin:0 0 23px }` — **comment the 23px inline**; it looks like a typo
- `th, td { padding:12px 16px 11px; border:1px solid }`
- `img { max-width:100%; height:auto }`
- `hr { height:24px; margin:24px 0 0; border:0; border-top:1px solid; background:transparent }`
- `ul, ol { margin:0 0 24px; padding-left:32px }`
- `li > ul, li > ol { margin-bottom: 0 }` (new — else a nested list injects a phantom 24px inside its parent item)

### 5.2 `themes/default.css` — two paired edits (only in-scope theme change)

In **both** the dark block and the `@media (prefers-color-scheme: light)` block: `hr { background-color: <c> }` → `border-top-color: <c>`, values preserved exactly (`#3a3a3a`, `#d0d0d0`). Nothing else.

Update the header comment: "only sets color/background/border-color" stays true (`border-top-color` is a border-color property), but add a sentence noting `hr` paints via `border-top-color` because base.css gives it an explicit 24px grid height that a background would fill.

**Must land in the same commit as the base.css `hr` change.**

### 5.3 `markdown/README.md`

Rewrite the `## base.css` "Baseline layout" bullet into a proper subsection stating: the 24px unit and `--wolf-baseline`; that line-height is a length not a ratio, and why; the invariant; the full scale table from §2 (deliberately duplicated so the doc reads without opening the CSS); the compensation arithmetic including both residuals; that horizontal dimensions are not grid-constrained; the literal-px rationale. Update `scroll-margin-top: 2rem` → `48px`.

Add to `## themes/default.css`: one sentence that `hr` is colored via `border-top-color` because base.css gives it an explicit grid height.

### 5.4 `markdown.rs` — doc comments only, no code

`BASE_CSS` const doc (~77-80) says "the `.wolf-src` marker rule plus baseline layout". "Baseline layout" currently means "starting/default layout" and now **collides with the typographic sense** — resolve the ambiguity at every occurrence: "…plus the 24px-baseline-grid structural layout (see `markdown/base.css` and `markdown/README.md`)". Verify no other doc comment mentions `1.6`, `em`, or `2rem`.

### 5.5 `crates/wolf-core/README.md` — lines 126, 133

L126 "baseline layout" → name the 24px baseline grid. L133 `scroll-margin-top: 2rem` → `48px` (2 baselines, matching the body top padding).

### 5.6 `crates/wolf-core/src/README.md` — line 111

Same two edits.

### 5.7 `crates/wolf-core/tests/README.md` — line 117

Says "ten tests"; it is actually 12 (count drifted). Correct to the new total and describe the baseline-grid assertions.

### 5.8 `BUILD.bazel` — **verify only, no edit**

`compile_data = glob(["src/markdown/**/*.css", ...])` already covers both edited files; no CSS file added/renamed/removed. The `markdown_render` target picks up new `#[test]` fns automatically.

**No new files. No new dependencies.**

## 6. Test plan

All in existing `crates/wolf-core/tests/markdown_render.rs`, following the established `default_theme_carries_dark_and_light_palettes` pattern of asserting on embedded CSS. No new file, no new Bazel target.

### 6.1 Helpers (private to the test file)

`base.css` is `include_str!`'d verbatim, so the document contains it byte-for-byte and a crude parser is reliable:
- `fn style_block(html: &str) -> &str` — slices between `<style>` and `</style>`
- `fn rule_decls<'a>(css: &'a str, selector: &str) -> HashMap<String, String>` — finds the rule whose selector list contains `selector` as a whole comma-separated entry; splits body on `;`, each declaration on the first `:`, trimming/collapsing whitespace
- `fn px(value: &str) -> Option<i32>` — parses `Npx`. **Return i32, not f64** (see §8 lints)

Doc-comment the helpers: this parser is valid only because the CSS is hand-written, flat, and non-nested. The only `@media` lives in `themes/default.css` — the `hr` theme assertions (§6.6) must use plain substring checks, not `rule_decls`.

### 6.2 `baseline_unit_is_declared`
Document contains `--wolf-baseline: 24px`.

### 6.3 `body_line_height_is_a_length_not_a_ratio`
`body.wolf-markdown` has `font-size == "16px"`, `line-height == "24px"`, `padding == "48px 40px"`. Explicitly assert line-height **ends in `px`**, with a failure message explaining why a unitless ratio breaks inherited grid alignment — the invariant most likely to be silently regressed.

### 6.4 `headings_declare_explicit_font_size_and_line_height`
Table-driven over `[(h1,32,48),(h2,24,48),(h3,20,24),(h4,18,24),(h5,16,24),(h6,14,24)]`. Each per-level rule declares **both** properties at exact px, and line-height % 24 == 0. Pins the scale and the h3–h6 UA-default fix.

### 6.5 `heading_border_is_compensated_into_padding`
h1 and h2: `padding-bottom == "23px"`, `border-bottom` contains `1px`, assert `23 + 1 == 24`. Failure message shows the arithmetic.

### 6.6 `hr_occupies_whole_baselines_and_is_painted_by_its_border`
- base.css `hr`: `height == "24px"`, `border-top` contains `1px`, `margin == "24px 0 0"`, `background` is `transparent`.
- **Theme guard (the important half):** document contains `border-top-color: #3a3a3a` AND `border-top-color: #d0d0d0`. Catches the failure where base.css is updated but the theme still paints a 24px solid bar. Failure message must name that mode.

### 6.7 `table_cells_land_two_baselines_per_row`
`th, td`: `padding == "12px 16px 11px"`, `border` contains `1px`; assert `12+24+11+1 == 48`. Plus `table` `margin == "0 0 23px"` with a message citing the `48N+1` collapsed-border arithmetic — the value looks like a typo and needs a self-explaining test.

### 6.8 `pre_sits_on_the_baseline_grid`
`pre`: `line-height == "24px"`, `padding == "24px"`, `margin == "0 0 24px"`. Plus `pre code` has `font-size: inherit` — regression guard for the 0.81em compounding bug.

### 6.9 `every_vertical_dimension_is_on_the_baseline_grid` — the sweep test

**The centerpiece.** Over a fixed list of styled selectors (`body.wolf-markdown`, `p`, `h1`…`h6`, `pre`, `blockquote`, `table`, `th, td`, `hr`, `ul, ol`, `li > ul, li > ol`, the `:first-child`/`:last-child` rules), for every vertical property (`margin-top/bottom`, `padding-top/bottom`, `line-height`, `height`, `scroll-margin-top`, plus top/bottom components of shorthand `margin`/`padding`), assert the px value is 0 or `v % 24 == 0` — **except** an explicit documented allowlist:

```
(h1, padding-bottom, 23), (h2, padding-bottom, 23),
(th/td, padding-top, 12), (th/td, padding-bottom, 11),
(table, margin-bottom, 23)
```

Each entry carries a comment naming the border it compensates. Value: **any new off-grid vertical value anywhere in the file fails the build**, and any new compensated value forces the author to state which border it compensates. A genuine grid audit, not a substring smoke test.

### 6.10 Regression
All 12 existing tests pass unchanged. `default_theme_carries_dark_and_light_palettes` survives the `hr` edit: it asserts `#1b1b1b`, `#f4f4f4`, `#0969da`, `#111111`, `prefers-color-scheme: light`, `color-scheme: light dark` — none live in the `hr` rule, and `#3a3a3a`/`#d0d0d0` remain present elsewhere (h1/h2 border-bottom-color, blockquote, cells). **Do not modify this test.**

## 7. NOT verifiable by automated test — rests on human visual review

1. **Whether text baselines actually land on the grid in WebKit.** The tests prove *declared* values, not rendered geometry: half-leading distribution, Inter's ascent/descent metrics, subpixel rounding at fractional device scale, WebKit line-box rounding. A 24px line box does not guarantee the baseline within it sits where a 24px overlay expects.
2. **Whether the collapsed-border table arithmetic holds in practice** (`48N+1` depends on WebKit's collapsed-border resolution). Verify with a real 3+ row table.
3. **Optical quality** — whether 48px above an h1 reads sensibly, whether h2 at 24/48 looks airy vs broken, whether h3–h6 hug correctly, whether 24px leading on 14px code reads well. Pure judgement.
4. **Margin-collapse across every real adjacency** (list→heading, blockquote→hr, table→heading, nested list tails).
5. **The accepted drift** from images and display MathML — unmeasurable here by construction.
6. **Inter fallback** — if the inlined face fails to load, fallback metrics change the visual result while every declaration stays identical.

**Recommended manual QA:** render a scratch doc exercising h1–h6, two paragraphs between each, `hr`, a 3-row table, a fenced code block, a blockquote with two paragraphs, a nested list, an image, and inline + display math; overlay a 24px repeating-linear-gradient via the web inspector. Confirm specifically: **(a) the `hr` is a hairline, not a 24px bar** — highest-consequence failure mode; (b) the first heading is not double-spaced from the top; (c) code blocks don't shift the grid below them.

## 8. Lint notes

- **Parse to integer px.** `px()` returns `Option<i32>`, not `f64` — avoids `clippy::float_cmp` on `v % 24 == 0`, and a fractional px anywhere in the file is *itself* a grid violation that should fail loudly rather than round silently.
- Prefer deleting an unused helper over `#[allow(dead_code)]`. No `#[allow(...)]` should be needed; if one seems necessary the helper is doing too much.
- Keep table-driven fixtures as slice-of-tuple constants so rustfmt formats them predictably.
- No CSS linter runs here and `markdown/*.css` is excluded from prettier — add no stylelint-style comments; nothing reads them.

## 9. Implementation order

1. `base.css` header comment (the spec the file implements — write it first to prevent drift)
2. `base.css` body + edge rules
3. `base.css` headings (shared rule trimmed, then six per-level rules with compensation)
4. `base.css` flow content (`p`, `code`, `pre`, `pre code`, `blockquote`, lists, `img`)
5. **`base.css` `hr` AND `themes/default.css` `hr` together, in one step** — separating them leaves the preview visibly broken
6. `base.css` table
7. Test helpers, then targeted tests, then the sweep test last (depends on the selector list being settled)
8. Run `//crates/wolf-core:markdown_render` and `//crates/wolf-core:wolf_core_lib_test` — green before any docs
9. `markdown.rs` doc comments
10. README updates, then `prettier --write` each edited `.md`. **Never** prettier a `.css` under `markdown/`
11. Verify `BUILD.bazel` compile_data (read-only)
12. Manual visual review per §7

## 10. Risks

- **`themes/default.css` `hr` (HIGH, mitigated).** base.css is emitted *before* the theme, so a leftover `background-color` wins the cascade and paints the 24px `hr` box as a solid bar. Highest-consequence regression. Mitigated by the paired edit (step 5), the theme guard test (§6.6), and QA item (a).
- **`themes/default.css` everything else (LOW).** Sets only color/background-color/border-*-color/color-scheme — none affect layout. h1/h2 and cell borders still work because base.css declares `1px solid`; the theme only supplies color. No theme selector loses its structural counterpart.
- **`math.css` (MEDIUM — DO NOT EDIT).** Emitted *after* base.css, so it wins on conflicting properties. Its em-relative vertical paddings (L108, L150) and fractional `border-top: 0.06em` (L139/143) are inside `<math>` — exactly the accepted display-MathML drift. Leave them. **The real risk:** body now inherits a *fixed* 24px line-height into `<math>`, so a tall display equation may overflow its 24px line box and clip or overlap, where the old unitless 1.6 at least scaled with math font-size. Watch in QA. **Pre-approved escape hatch if it bites: a single rule in `base.css` — NOT math.css — `body.wolf-markdown math { line-height: normal }`**, reintroducing already-accepted math drift without touching the vendored file. Do not improvise inside math.css.
- **`.wolf-src` anchors (LOW).** Its `font-size: 0` and `line-height: 0` must survive verbatim. The length-based body line-height *strengthens* this, **but only because `.wolf-src` sets `line-height: 0` explicitly** — if that declaration were ever dropped, markers would inherit a hard 24px and inject visible blank lines throughout. Do not touch the rule; keep the existing header warning. Existing marker tests cover placement, not the zero-height property.
- **Margin-collapse assumptions (MEDIUM).** The design leans on adjacent-sibling collapsing taking the max. Collapsing is suppressed by non-`visible` `overflow`, floats, and flex/grid containers. `pre` sets `overflow: auto`, establishing a BFC — so `pre`'s own margins don't collapse with its children's, which is fine and desirable. No other rule introduces a BFC. Confirm in QA that a `pre` between two paragraphs advances the grid correctly.
- **`rem`→`px` (LOW; this change removes risk).** Today's rem values resolve against an unset root font-size, a dependency on webview text-scaling that could detune the grid invisibly.
- **README drift (LOW but certain).** Four Markdown files repeat base.css facts (`scroll-margin-top: 2rem` in two, "baseline layout" in three). Per CLAUDE.md a doc/implementation mismatch is a bug — §5.5/§5.6 are not optional polish.

---

## Workflow log

- [x] Problem statement written
- [x] Architect plan
- [x] Tests written
- [x] Implementation + tests passing (26/26)
- [x] Review (approved, second pass)
- [x] Documentation
- [ ] Manual visual QA — see "Outstanding" above
