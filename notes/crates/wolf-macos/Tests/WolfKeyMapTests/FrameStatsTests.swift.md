# `crates/wolf-macos/Tests/WolfKeyMapTests/FrameStatsTests.swift`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Label rules

`label` is a hand-written `switch`, so a copy-paste duplicate compiles and emits two identical report rows that no other test rejects. It would also fool `testFormattedBucketRowsAreInRawValueOrder`: that scan is forward-only, so the duplicated label re-matches the first row's text and the ordering assertion passes on an ambiguous report. `testBucketLabelsAreUniqueAndFitTheReportColumn` is the guard for that hole.

Uniqueness is the strongest check available. A rule that no label is a substring of another is impossible, because `draw` is legitimately a substring of `draw.pane` and `draw.prev`; the forward-only scan tolerates that only because `draw` has the lower raw value and is matched first. No `pane.*` label is a substring of another: `pane.cells` and `pane.cursor` only share a prefix.

`format` left-aligns the label in a width-14 field, so a label of 14 or more characters runs straight into the right-aligned count column and breaks the byte-exact table. That is why `maxLabelWidth` is 13.
