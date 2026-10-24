# Markdown preview stylesheet

This spec owns the vertical arithmetic of the preview stylesheet, [`base.css`](../crates/wolf-core/src/markdown/base.css): the baseline grid, how borders are compensated, the leading of inline code, and the first- and last-block edge rules. A difference between this file and `base.css` is a bug in one of the two. The file-level decisions, including why the values are literal px and which residuals are accepted, are in the [markdown README's decisions](../crates/wolf-core/src/markdown/README.md#decisions).

## No markup

Never write an element name in angle brackets anywhere in an embedded stylesheet, comments included. Every byte ships verbatim into the rendered document's style block, ahead of the real body tag, and three consumers scan that HTML for the first match of a fixed needle: the macOS preview's head and body split, the browser preview's live-reload script injection, and the integration tests. A comment that mentions a tag corrupts all three silently, so name elements in prose ("the code element", "p", "li").

`embedded_stylesheets_carry_no_markup_like_text` in `markdown.rs` enforces this over every embedded sheet and explains the failure modes.

## Baseline grid

The unit is 24px (16px body text times 1.5), declared once as the `--wolf-baseline` token on `body.wolf-markdown` and written B below. Every vertical margin and every declared border-box height is 0 or a whole multiple of B, and the grid is vertical only.

| element | font-size | line-height | margin-top | margin-bottom |
| ------- | --------- | ----------- | ---------- | ------------- |
| body, p | 16px      | 24px (1B)   | p: 0       | p: 24px (1B)  |
| h1      | 32px      | 48px (2B)   | 48px (2B)  | 24px (1B)     |
| h2      | 24px      | 48px (2B)   | 48px (2B)  | 24px (1B)     |
| h3      | 20px      | 24px (1B)   | 48px (2B)  | 0             |
| h4      | 18px      | 24px (1B)   | 24px (1B)  | 0             |
| h5      | 16px      | 24px (1B)   | 24px (1B)  | 0             |
| h6      | 14px      | 24px (1B)   | 24px (1B)  | 0             |

h1 and h2 keep their 2em and 1.5em sizes. The 20/18/16/14 ramp for h3 to h6 bottoms out legibly, with h6 distinguished by weight 600 rather than by size. h2 takes 48px leading because the only other grid-legal value at a 24px font is 24px, a ratio of 1.0, where wrapped lines nearly touch.

Every heading has strictly more space above than below. h3 to h6, with no bottom margin, hug the content they introduce, and collapsing does the section spacing: a paragraph's 24px bottom margin against an h2's 48px top margin collapses to 48, which is 2B.

## Border compensation

A border is always folded into padding or an explicit height, never a margin, so the invariant survives margin collapsing: collapsing keeps the larger of two margins, and the larger of two multiples of B is one.

`hr` gets an explicit 24px border-box height that absorbs the rule, drawn as `border-top`. The theme colors it through `border-top-color`, not `background-color`, because a background would fill the 24px box with a solid bar. The total is 24 (margin-top) + 24 (box) + 0 (margin-bottom) = 48px. Margins alone cannot fix a 1px-high `hr`: one before an h2 collapses to 48 and re-exposes the 1px.

With `border-collapse: collapse`, adjacent table rows share one border, so an N-row table has N+1 horizontal lines. The row pitch is 12 (padding-top) + 24 (line-height) + 11 (padding-bottom) + 1 (shared border) = 48px; 47 is odd, so the padding cannot split evenly, and the 12/11 asymmetry is imperceptible. The whole table is 47N + (N+1) = 48N + 1, the trailing 1px being its own bottom outer border, and a 23px bottom margin brings the total to 48N + 24, exactly 2N+1 baselines.

No compensation is needed for `blockquote`, which has no border and no vertical padding, for `pre`, which has no border, or for inline code, whose vertical padding paints background but does not grow the line box.

## Inline code leading

Inline code costs the grid through font metrics, not padding. The inherited 24px line-height makes the code box 24px tall, but an inline box distributes its leading as half-leading around its own face's metrics: with line-height L, ascent A and descent D, it extends (L + A − D) / 2 above its baseline and (L − A + D) / 2 below. The line box is the union of the boxes present, L + (max(A − D) − min(A − D)) / 2, which exceeds L whenever the body and monospace faces disagree on A − D.

The excess is 2px for a paragraph and for a list and 3px for `pre`, whose UA stylesheet gives it its own monospace family, and because every later block shifts with it, the error accumulates.

`line-height: 0` on the code element removes that union term. Measured against the rendered document in WebKitGTK 2.52.4, border-box heights of 26 for a paragraph carrying inline code, 74 for a list and 123 for `pre` become 24, 72 and 120. Overriding the code element's `font-family` to `inherit` gives 25, 73 and 120 and loses the monospace face, so it is rejected.

The chip's background survives, because an inline box's content area comes from its font metrics rather than from line-height: chip pixels occupy a 20-row band inside each 24px line box. Glyphs do not overflow their line, even with deliberate descenders and tall ascenders. `preview_inline_code_keeps_the_baseline_grid_under_cage` pins both from a real render. The rule changes the document macOS renders too, and every number here is a Linux WebKitGTK reading; macOS is unmeasured.

Do not undo it either way. Giving `pre code`, or the highlight spans inside it, a line-height reintroduces the union term on every line carrying a highlighted token, a per-token error, as `pre_code_declares_no_leading_of_its_own` pins. Deleting the zero because `pre` declares its own 24px deletes it for inline code in paragraphs and list items too, which is what the zero is for.

The declared-value sweep in `markdown_render.rs` cannot see this excess: it exists only in the laid-out box, so a green sweep is not evidence that the grid holds line by line.

## Edge rules

The first block needs margin-top 0 and the last margin-bottom 0, because the body has no padding. An unreset 48px top margin on the opening block either collapses out through the body's top edge, since nothing blocks parent-child collapsing, and displaces the whole document 48px, or, if a leading anchor's anonymous block box blocks the collapse, sits inside the body between that box and the heading.

The displacement is positional only: no stylesheet sets a background on `html`, so the body's background paints the whole canvas. Which way it resolves is engine-dependent, because it turns on whether the anonymous box is self-collapsing, a function of its inherited 24px strut, and both outcomes defeat the flush-to-edge body.

48 is on the grid, so the sweep cannot see this; only the DOM-level edge tests can. The bare `:first-child` and `:last-child` forms are inert, because marker anchors are flushed before the first block's opening tag and, for a source ending in a newline, after the last block's closing tag by `flush_before(usize::MAX)`. So the rules skip the anchors:

- First: `> *:not(.wolf-src):not(*:not(.wolf-src) ~ *)`, an element that is not an anchor and has no non-anchor sibling before it.
- Last: `> *:not(.wolf-src):not(:has(~ *:not(.wolf-src)))`, an element that is not an anchor and has no non-anchor sibling after it.

The naive `> *:last-child` also ships, ahead of the general rule, as a partial fallback that is cheap, Selectors 3, and right for a source that does not end in a newline. The general rule covers every case; neither is worth removing. Why each lives in its own rule, why the run is unbounded, and why mid-block anchors cannot match are in the [markdown README's decisions](../crates/wolf-core/src/markdown/README.md#decisions).
