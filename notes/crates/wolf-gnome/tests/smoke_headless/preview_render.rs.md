# `crates/wolf-gnome/tests/smoke_headless/preview_render.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Picker over preview geometry

`picker.rs`'s `PICKER_BOX_MARGIN`, `PICKER_BOX_WIDTH` and `PICKER_BOX_HEIGHT` are private, and an integration test is an external crate. Making them `pub` for this file would be circular: a box that moved together with its constants would keep every assertion green. So the box is located from the pixels on both axes, its surface colour is read from the capture, and nothing in `src/` changes for this smoke.

`pickover_fill_reference_band` is four absolute pixel literals inside a region whose extent this file may not import. The smoke asserts the two properties they exist for, uniform with the picker open and absent with it closed, so a band that leaves the box fails loudly instead of reading another widget's colour.

## Picker fill colour

Uniformity says the band sits inside one flat surface rather than straddling an edge, which licenses reading the colour off one pixel. Absence from the same band in the control says the surface is the picker's, not the editor pane's; without it, a run where the box never appeared would read the editor's background as the fill and pass assertion 8.

Three inequalities say what the colour may not be. The first two keep assertion 8's three counts distinguishable, and the third, against `PICKOVER_UNSTYLED_FILL`, is the only reading that tells the overlay chrome apart from the mere presence of an opaque box.

## Picker selected row scan

Assertion 4's `PICKOVER_UNSTYLED_FILL` inequality reads the box's surface, and nothing else reads a row metric, so `padding: 8px 10px` on `list > row` could be deleted with every other assertion green. The box is 600x400 whatever the rows do, so the selection highlight, exactly one row tall, is what is measured.

The scan assumes one row in the box. `PICKOVER_QUERY` is a prefix of `PICKOVER_SOURCE_FILE` and of nothing else in the cwd, so the list narrows to one row above empty fill; with more rows the empty fill the walk starts from moves off the reference band.

The walk goes up from the reference band, which assertion 4 proved is the box's fill, and stops at the first row that is neither fill nor selection. The search entry above the list is neither colour, so the walk ends there or at the box's top edge, and an accent in the tab bar or header can never be counted as a list row.
