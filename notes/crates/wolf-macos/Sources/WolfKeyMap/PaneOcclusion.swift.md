# `crates/wolf-macos/Sources/WolfKeyMap/PaneOcclusion.swift`

Last verified at commit `e88be5f2168178fb6190932325fdce0156f7d2d3`.

## Why the tolerance is zero

Skipping a pane's grid paint is safe because the web view physically covers the region whose paint was skipped, not because the web view's frame and the grid rect are in sync. In the common case `syncPreviewWebViews` frames the web view to the pane below its strip, which contains the capped grid rect the painter tests with at least the padding less the divider band to spare; mid-live-resize, just after a split or just before a close the two are out of step, and the containment test decides on the geometry actually sampled.

If the pane grew past that spare and `draw` runs before `syncPreviewWebViews`, the stale smaller web frame no longer contains the larger grid rect, containment fails, and the grid paints. Wherever the stale frame still contains the grid rect, as after a shrink, the paint is skipped, which is visually safe only because that web view covers at least the grid rect at composite time.

So painting resumes immediately after a geometry change structurally, through the coverage invariant rather than bookkeeping. Anything that lets the web view stop covering the skipped region breaks correctness even while the frames look in sync: an epsilon tolerance, an inset, a transform, a clipping ancestor or a different coordinate space. An epsilon in particular would let a slightly too small web view suppress the paint underneath it and leave a garbage region on screen.
