# `crates/wolf-macos/Sources/WolfKeyMap/PaneOcclusion.swift`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Why the tolerance is zero

Skipping a pane's grid paint is safe because the web view physically covers the region whose paint was skipped, not because the web view's frame and the grid rect are in sync. In the common case `syncPreviewWebViews` assigns the web view's frame from the same `gridRect(ofPane:)` expression the painter uses, so the two are bit-identical; mid-live-resize, just after a split or just before a close they disagree, and the containment test decides on the geometry actually sampled.

If the pane grew and `draw` runs before `syncPreviewWebViews`, the stale smaller web frame no longer contains the larger grid rect, containment fails, and the grid paints. If the pane shrank, the stale larger web frame still contains the smaller grid rect and the paint is skipped, which is visually safe only because the oversized web view covers at least the grid rect at composite time.

So painting resumes immediately after a geometry change structurally, through the coverage invariant rather than bookkeeping. Anything that lets the web view stop covering the skipped region breaks correctness even while the frames look in sync: an epsilon tolerance, an inset, a transform, a clipping ancestor or a different coordinate space. An epsilon in particular would let a slightly too small web view suppress the paint underneath it and leave a garbage region on screen.
