# `crates/wolf-core/src/markdown.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Nested excluded blocks

An excluded block (`CodeBlock`, `HtmlBlock`, `Table`) nested in a container needs its interior markers flushed ahead of its opening tag, and `no_inject_depth` alone cannot protect it. A container (`List`, `Item`, `BlockQuote`, `FootnoteDefinition`) defers every `col != 0` marker across its whole range at its own `Start`, including markers that fall inside the nested block. Those markers are already past `next_marker` when the nested `Start` arrives, so `claim_before(.., deferrable = false)` cannot reclaim them.

They would survive to the block's `End`, where step 1 has dropped the depth to 0 and step 2 would drain them inside the code element or just before the table's closing tags, the foster-parenting hazard the exclusion exists to prevent. Draining them before the tag gives a nested excluded block the same flush-before-the-tag placement a top-level one gets, which is what `defers_interior_markers` promises.

The drain is not gated on `no_inject_depth == 0` the way step 2 is, and the asymmetry is intentional. Step 2's gate stops a drain from firing inside a suppressed region, such as `End(TableRow)` within a table; this drain moves markers out of harm's way before the region opens. If an excluded block ever nested inside another, gating it would strand the markers and drain them at the inner `End`, the bug this drain exists to fix. That nesting is unreachable: none of the four regions can contain a `CodeBlock`, `HtmlBlock` or `Table`, and `Start(Image)` is not a block start, so it takes step 5.

## Every event goes through pending

`MarkerInjector::next` buffers each event in `pending`, expanded with whatever markers are due at it, then loops back to drain `pending` in order. Do not optimize this into returning a non-block, non-`Text` event directly.

Step 4 of `place` may drain markers into `pending` and only then have `try_split_text` decline, for instance when the identity predicate fails on an entity reference. The `Text` must still be emitted after the markers just drained ahead of it, and a direct return would emit it first and reverse their document order. `entity_reference_is_not_split` and `escapes_and_entities_survive_inline_anchoring` both take that path. The cost is one extra loop iteration per event.
