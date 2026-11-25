# The sidebar reveal

Last verified at commit `d2611095447a69e4ad2f1c35dc0cfee95fc1b80a`.

A reveal brings the current file's row into the sidebar tree: [`tree_view.rs`](../../crates/wolf-gnome/src/README.md#tree_viewrs) expands its ancestors, selects the row at once, and scrolls to it later, from a tick on the list view's frame clock. Calling `ListView::scroll_to` from `reveal` or `refresh` directly brings the bug below back, and so does a rebuild that skips the re-reveal.

## The symptom

A restored tab whose editor pane is active on a file below the fold of a large folder shows the root row, then blank rows in reserved space, then the rows from the scroll anchor down. Scrolling down and back, or collapsing and expanding the root, draws the blank rows. With the preview pane active nothing is revealed, because `init.lua` reports no current file for a preview buffer, and the tree draws correctly.

## The cause, as measured

Under `cage`, with instrumentation in `tree_view.rs` and the fixture of [`smoke_headless/tree_reveal.rs`](../../crates/wolf-gnome/tests/README.md#smoke_headlesstree_revealrs):

- The launch reveal ran in the same refresh as the rebuild that first populated the tree. The list view was mapped and realized but not yet allocated: height 0, and a vertical adjustment with upper 0 and page size 0.
- Inside that `scroll_to`, GTK bound rows 0 to 204, then unbound rows 1 to 82 one by one while binding rows 205 to 285. Only row 0 and rows 83 onward kept a widget.
- Once laid out, the adjustment's value was still 0. The list showed its top, with the root row drawn and the rows below it blank, which is what OCR of the band read.
- A scroll on the first tick after the reveal fails the same way: that tick still precedes the list's first allocation.
- A later version advance, from a watcher rescan, unbound every row and rebound the top of the tree, with no reveal, because the launch reveal had cleared `pending_reveal`. The revealed row went out of view.

## The GTK path

Read in GTK 4.22's list code and not measured: `gtk_list_base_compute_scroll_align` divides a zero cell offset by a zero visible size, so the anchor's alignment is NaN. `gtk_list_base_update_adjustments` then clamps the scroll value to 0, and `gtk_list_base_set_anchor` keeps widgets from two rows above the target downward. The root row keeps its widget through the focus tracker. A `scroll_to` on a row that has no area yet aligns the row to the top of the view rather than scrolling it minimally into view.

## Why a rebuild re-reveals

`remove_all` drops the `SingleSelection`'s selected item, which GTK documents, and returns the list's anchor to row 0, so any rebuild after a reveal loses it. A rebuild with nothing pending therefore reveals the current file again. A refresh without a rebuild reveals only a pending file: a re-reveal there would pull a user's scroll back to the current file on every render wake. The re-reveal never arms a retry, since `reveal` leaves `pending_reveal` untouched when the file does not resolve.

## Why the tick waits a frame

A frame's layout runs after its tick, so on the first frame the tick sees, the rows a reveal just selected have no area. The tick records that frame and scrolls on a later one. Each reveal resets the recorded frame, so a reveal during a pending scroll waits again.

## Why the tick stops while unmapped

A tick callback keeps the frame clock running while its widget is realized, mapped or not, which is read from GTK's source. So the tick withdraws when it finds the list view unmapped, with the scroll still owed, and the `map` handler arms it again. No smoke hides the sidebar between a reveal and its tick, so the withdrawal and the re-arm are unproven, and the measured launch reveal ran while mapped.
