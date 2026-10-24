# `crates/wolf-gnome/src/scrollbar.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## The clamp row

Row 13 of the thumb table is the only row on which the upper `y` clamp binds. The floor lifts `h` above the proportional height — `max(608 * 11/100000, 24) = 24` — so the proportional position leaves no room for it: `raw = 52 + (99989/100000) * 608 = 659.93312`, while the lowest top edge a 24 px thumb may have is `52 + 608 - 24 = 636`. Without the clamp the thumb hangs 23.93 px below the pane's grid, which is what a 100k-line file scrolled to its end shows.

Row 2 is not a substitute. It lands exactly on the boundary — `626.56 + 33.44` is precisely 660, the grid's own lower edge — so the clamp returns its argument untouched and deleting the clamp outright leaves row 2 green. Every other `Some` row's raw `y` sits far below the bound: rows 1, 7 and 12 at the track's top, row 8 in a track no taller than its own thumb.
