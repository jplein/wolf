# `crates/wolf-core/tests/markdown_render.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Stylesheet emission order

base.css, math.css and the theme share one `<style>` block at equal specificity, so on any conflicting property the last one wins. math.css is vendored and must not be edited, so it overrides base.css on anything it touches. base.css sets `background: transparent` on the 24px `hr` box and the theme, emitted after it, could repaint that box, which is why the theme colours the `hr` through `border-top-color` and never `background-color`. Reordered, the grid's compensation rules lose cascades silently, with no other test noticing.

The markers are stable and each unique to one file. `--wolf-baseline` is base.css's grid token, asserted by `baseline_unit_is_declared`; `menv-cases` is a vendored pulldown-latex class in math.css that this repo does not author; `color-scheme: light dark` is the theme declaration `default_theme_carries_dark_and_light_palettes` pins. None can be edited away without failing another test first.

## Edge resets skip leading anchors

The text audits cannot catch a declaration that exists, is spelled correctly and still applies to the wrong element. `MarkerInjector` flushes its column-zero `<span class="wolf-src">` anchors as raw HTML before each block's opening tag, and `wl-1-0` sits at byte offset 0, so the body's first child is an anchor rather than the first block.

A naive `body.wolf-markdown > *:first-child { margin-top: 0 }` matches that span and leaves the opening `<h1>` with its full 48px margin-top. The body has no padding, so whether that margin collapses through the body's top edge or sits inside it, the heading lands 48px below the top edge.

`base.css` spells the opening-block rule `body.wolf-markdown > *:not(.wolf-src):not(*:not(.wolf-src) ~ *)`: not an anchor, and no non-anchor sibling precedes it. That skips a leading anchor run of any length. `flush_before(usize::MAX)` emits trailing anchors after the last block, so a naive `:last-child` matches an anchor too. 48px is two baselines, so the grid sweep cannot see either failure.

These tests pin the outcome by parsing the emitted body into its child list and resolving the cascade against it. They are agnostic about how the outcome is achieved: any selector shape that skips the anchors passes, and so would a renderer that stopped emitting a leading anchor.

## Inline column anchors

A block's interior `col != 0` markers are deferred past its opening tag and placed at their real position in the rendered inline flow. Flushed before the opening tag with the column-zero markers, every one of them would measure to the paragraph's top, and cursor-follow while typing at the end of a long paragraph would reveal its start.

Column-zero markers keep their block-top placement, because the front-end's tier-4 sweep filters on `[data-col="0"]` and depends on their document order. Placement is suppressed inside code blocks, HTML blocks, tables and image alt text. These tests use the public surface only.

## Nested excluded blocks

The top-level code-block and table tests cannot see this failure, because nothing is deferred at the top level. A container (`List`, `Item`, `BlockQuote`, `FootnoteDefinition`) defers every `col != 0` marker across its whole range at its own `Start`, including markers that fall inside a `CodeBlock`, `HtmlBlock` or `Table` nested in it. By the nested block's `Start` those markers are already past the marker cursor, so the block's non-deferrable claim cannot reclaim them. They survive to its `End`, where the suppression depth is back to 0, and drain inside `<pre><code>` or `<table>`.

The markers of a nested excluded block must instead be flushed ahead of that block's opening tag, as a top-level block's are.
