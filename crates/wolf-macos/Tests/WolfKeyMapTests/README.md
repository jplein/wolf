# wolf-macos/Tests/WolfKeyMapTests

XCTest coverage of the pure [`WolfKeyMap`](../../Sources/WolfKeyMap/README.md) seam. These run headlessly, with no window server:

```sh
bazel test //crates/wolf-macos:WolfKeyMapTests
```

**Adding a test file means editing `srcs`.** That target's `srcs` is a hand-maintained list, not a glob, and it is the sole source of truth — there is no `cargo`-style fallback that would pick the file up anyway. A `*Tests.swift` file missing from `srcs` never compiles into the test binary and never runs, silently. XCTest does auto-discover the `test…` methods inside a file that is already listed; the gap is only at the file level.

Note that the nearby `swift_format_srcs` glob feeds lint and format only, not test execution, so a new file can look picked up while still being unrun.

## Files

#### `AppearanceSeamTests.swift`

**Covers**

- `AppearanceMode`'s raw values and `init?(rawValue:)`, keeping the Swift enum in lockstep with `mod appearance` in `crates/wolf-ffi/src/lib.rs` and `wolf_core::config::Appearance`. Scalar inputs only — the `NSApp` appearance forcing and `effectiveAppearance` KVO that consume the decode are manual-smoke.

**Gotchas**

- The out-of-range probe is the first unused discriminant. Adding a case without bumping it turns that case's decode into a silently passing nil assertion.

#### `CellMetricsMathTests.swift`

**Covers**

- `CellMetricsMath.cellHeight` and `.baseline`: that a positive line-height percentage drives cell height from the font size alone (the ascent/descent/leading arguments are ignored on that path), that a non-positive percentage falls back to the font's natural line height, and that the baseline splits the leftover space evenly above and below the glyph box.

**Gotchas**

- This is the only headless slice of the cell-metrics work — font registration and CTFont family resolution need a live font and stay manual-smoke, so a new case must be expressible as plain `Double` inputs, with an exact-valued result to assert against the tight `eps`.

#### `CmdlineSeamTests.swift`

**Covers**

- `CmdlineSeam.composedLine`, `.caretPrefix`, and `.bytePrefix`, table-driven over exact `String`s rather than measured text widths, so no CoreText or `FfiCmdline` is involved.
- The prompt-only row exists because of a real bug: a `confirm()` answer line arrives with an empty `firstc` and `text`, and the band must paint the prompt instead of going blank.

**Gotchas**

- `bytePrefix` counts UTF-8 bytes, not characters. The multi-byte rows land on clean boundaries deliberately; a fixture that splits a scalar decodes lossily.

#### `CommandSeamTests.swift`

**Covers**

- `CommandKind(ffi:)` over the whole discriminant range, one assertion per case, keeping the Swift enum in lockstep with `mod command_kind` in `crates/wolf-ffi/src/lib.rs` and the `wolf_core::Command` variant order.

**Gotchas**

- The first out-of-range probe tracks the registry's high-water mark and must be bumped in lockstep whenever a command is appended. Leaving it stale turns the new case's decode into a silently passing nil assertion — the same trap as `AppearanceSeamTests`.

#### `CwdTitleSeamTests.swift`

**Covers**

- `CwdTitleSeam.displayTitle(cwd:home:)`: abbreviation of the home prefix to `~`, a cwd outside home returned verbatim, empty and all-whitespace cwd collapsing to `""`, an empty `home` disabling abbreviation, and one trailing slash on `home` normalizing away.

**Gotchas**

- `home` is always injected, never read from the environment, which is what keeps the cases deterministic.
- The prefix-collision case (`/Users/janet/x` against home `/Users/jane`) is the one that pins segment-boundary matching; a naive substring-prefix implementation passes every other case.

#### `DividerDragSeamTests.swift`

**Covers**

- `DividerDragSeam.clampedLeftFraction(...)`: a mid-span pointer passing through as the raw fraction, each side clamping to its minimum grid size plus that side's pixel overhead (columns for a vertical divider, rows for a horizontal one), and a pair too small to hold both minima collapsing to the midpoint instead of an inverted range.

**Gotchas**

- Assertions convert the returned fraction back into pixel extents and compare those against min-cells-plus-overhead, deliberately pinning the observable contract rather than the seam's internal epsilon. Keep new cases in that form.

#### `DividerHitTestTests.swift`

**Covers**

- `DividerHitTest.hit(_:point:tolerance:)`: perpendicular distance to the divider line, rejection of a point that is on the line's axis but past the ends of its span, nearest-wins near a vertical/horizontal intersection, and `nil` for an empty list.

**Gotchas**

- Dividers are hand-built through `PaneTree.Divider`'s memberwise init rather than produced by `PaneTree.dividers(width:height:)`, which pins picking in isolation but means only `orientation` and `lineRect` carry meaning in these fixtures — `pairStart`, `pairSpan`, `before`, `after`, and `fraction` are placeholders. A case that depends on those needs real values.

#### `DrawerFocusSeamTests.swift`

**Covers**

- `DrawerFocusSeam.bottomMostLeaf` (greatest `y + h`, ties breaking to smallest `x` then smallest `PaneId`, `nil` on empty) and `drawerUpTarget` (the MRU editor pane when it is still a live leaf, the bottom-most leaf when it is `nil` or stale).

**Gotchas**

- Rects use a top-left origin, so bottom-most is the largest `y + h`. An AppKit-flipped fixture would invert every expectation here.
- The drawer stays outside `PaneTree` to preserve its parity with wolf-gnome, so there is no tree neighbour to assert against — fixtures are plain `(PaneId, Rect)` lists. The `applyFocus` glue that calls the seam is manual-smoke.

#### `DrawerLayoutTests.swift`

**Covers**

- `DrawerLayout`: the pinned `defaultWeight`, the no-drawer case (editor fills the pane area, `drawerRect` is `nil`), the drawer as a full-width strip sitting directly below the editor, and the invariant that `editorHeight + drawerHeight` tile the pane area exactly.

**Gotchas**

- Two pane-area heights are fixed deliberately — one divisible by three, one not — because the only thing distinguishing them is the rounding in `drawerHeight`. A new case should state which side it exercises.

#### `EditSeamTests.swift`

**Covers**

- `EditSeam.placementMode(paneCount:activePaneIsPreview:otherEditorPaneCount:)`, in particular how narrow the reuse branch is: exactly two panes, the active pane a preview, and exactly one other editor pane. Every other shape, including three or more panes, falls to the active-pane branch; a pane count of one or zero splits left.

**Gotchas**

- The returned strings are the literal `mode` argument passed to `edit_source` in `crates/wolf-ffi/src/lib.rs`, not display labels — changing one here without changing the Rust side breaks the call silently. What those modes actually do to the layout is covered by the real-nvim `//crates/wolf-ffi:edit` test.

#### `EditorInsetTests.swift`

**Covers** — `EditorInset`, the per-pane text-grid inset arithmetic. Pins the padding constant, one exact `gridRect` for a representative pane, the additive invariants tying `gridRect` back to the pane and statusbar dimensions, and the clamp edges.

**Gotchas** — only `gridRect.w` and `gridRect.h` clamp to a non-negative floor; the `x` / `y` origins are fixed offsets and stay unclamped, and one case asserts that explicitly for a pane shorter than its own statusbar, so a "clamp everything" simplification fails here. New cases should reuse the file's `eps` / `approx` / `assertRectEqual` helpers.

#### `FontSizeSeamTests.swift`

**Covers** — `FontSizeSeam`: the `clamped` / `increased` / `decreased` point-size arithmetic behind the View menu font commands, plus the bound and step constants, with each clamp boundary asserted from just inside and from exactly at the bound.

**Gotchas** — the seam is `Double -> Double`, so even the integral expectations are asserted with an accuracy epsilon. Font-size reset is not part of the seam (the controller restores its own stored launch default), so there is deliberately no case for it.

#### `FrameStatsTests.swift`

**Covers** — `FrameStats`, the frame-time aggregation seam: bucket raw values and labels, env parsing for the enable and interval variables, the truncating integer mean, nearest-rank percentiles, the ring-bounded percentile window against exact unbounded count / sum / max, the wakes-per-keystroke ratio, `reset`, the saturating `delta`, and a byte-exact `format` table.

**Gotchas** — `format` is pinned to a byte-exact expected string, so any change to a label, a column width, or the row ordering means updating that fixture; labels must also stay unique and fit the label column, since the ordering test scans forward and a duplicate would pass silently. The `pane.*` buckets are not a partition — summing them double-counts — and that nesting is a call-site property checked by manual capture, not here. `testSummarizeOfUntouchedStatsIsEmpty` stands in for `FrameMetrics`'s private idle-suppression flag, which is otherwise manual-smoke only.

#### `GlyphStyleTests.swift`

**Covers** — `GlyphStyleResolver.resolve(flags:fg:special:)`: face selection from the bold and italic bits, underline from either the underline or the undercurl bit, strikethrough drawn in `fg`, and the underline color falling back to `fg` when `special` is unset.

**Gotchas** — the style-flag bit positions are copied here from `mod flag` in `crates/wolf-ffi/src/lib.rs`, which is the source of truth; a new bit has to be added in both places. Undercurl resolves to a straight underline, so the undercurl-only case asserts `underline == true` rather than a distinct kind. `FfiColor` is compared component-wise, since it is not assumed `Equatable`.

#### `HistorySeamTests.swift`

**Covers** — `HistorySeam.rows(displays:indices:currentFlags:)` zipping the three parallel FFI arrays: order preserved verbatim, the shortest array truncating, negative indices dropped, and at most one `isCurrent` with the first winning. Also pins `itemIdentifiers`' locked toolbar order and the identifier / symbol constants.

**Gotchas** — indices are stack slots handed straight back to `history_jump`, so they are never renumbered and index 0 is a real slot rather than a sentinel. The negative-index rule and the one-checkmark rule are asserted independently and deliberately leave their interaction unpinned; do not add a case that fixes it.

#### `KeyCompositionTests.swift`

**Covers** — `KeyComposition.routesThroughComposer(keyCode:ctrl:meta:optionAsAlt:)`, true only for a plain non-named keyCode with no ctrl or meta while option-as-option is active, and `charEvents(for:)`, which emits one modifier-free Char event per Unicode scalar with C0 / DEL filtered. Cross-checks `WolfKeyMap.isNamedKey(keyCode:)` on the named-key cases.

**Gotchas** — only the routing decision is testable here; the Carbon `UCKeyTranslate` composition itself (`KeyComposer`, in the WolfMacOS module) is not, and the composed-scalar delivery to Neovim is covered by the real-nvim `//crates/wolf-ffi:unicode_key` test instead.

#### `KeyMapTests.swift`

**Covers** — `WolfKeyMap.translate(...)` driven by the scalar fields an `NSView.keyDown(_:)` pulls off an `NSEvent`. Pins the Shift contract (a `Char` carries the already-shifted glyph with `shift == false`; only named keys keep a true `shift`), that ctrl / alt / meta combos take the lowercased glyph from `charactersIgnoringModifiers`, and that bare modifiers and unmodified control characters yield nil.

**Gotchas** — the `Kind` discriminants and `kVK_*` codes are local copies that must stay in lockstep with the Rust `FfiKeyEvent` and `wolf-core/src/input.rs`; note that Backspace is `kVK_Delete` and forward-delete is `kVK_ForwardDelete`, which are easy to transpose. `assertKey` compares field by field because the generated `FfiKeyEvent` is not `Equatable`.

#### `PaletteSeamTests.swift`

**Covers** — `PaletteSeam.isPaletteTrigger(charactersIgnoringModifiers:command:)`, the case-insensitive Command+P chord, and `PaletteSeam.PaletteRowKind(ffi:)`, which decodes the FFI row-kind discriminant and must stay in lockstep with wolf-ffi's `mod palette_row_kind`.

**Gotchas** — `isPaletteTrigger` is dormant: Command+P is a menu key-equivalent that preempts `keyDown`, so nothing in the app calls it. The predicate and its test exist for symmetry with `PickerSeam`'s Command+O, and the wrong-letter case is there specifically to catch confusing the two chords.

#### `PaneGeometryTests.swift`

**Covers** — `PaneGeometry`: `cols` / `rows` derived from the cell size with a one-cell floor, `cellAt` mapping and clamping (including subtracting a non-zero pane origin), and the `maxCols` cap, which narrows and horizontally centers `gridRect` while leaving rows untouched.

**Gotchas** — the fixture cell is deliberately non-square so column math and row math cannot be mistaken for each other, and the `maxCols` fixture is a copy of `PaneContainerView`'s hardcoded cap that has to be kept in step with it. `cellAt` clamps rather than returning nil, so margin and out-of-bounds points land on the nearest edge cell.

#### `PaneLayoutCodecTests.swift`

**Covers** — the `PaneLayoutCodec` / `SavedPaneNode` bridge: `encode` (leaf ids to buffer paths, an unmapped leaf to the empty path, preview marking), the `postOrderTokens` / `decodeTokens` round-trip over leaf, two-child, n-ary and perpendicular-nested trees, `isEmptyLeaf`, `PaneTree.rebuilt(from:firstLeafId:)`, and the `specJSON` object shape.

**Gotchas** — the `specJSON` cases parse with `JSONSerialization` and assert only the object graph, so key ordering and float spelling stay unpinned on purpose; the key names themselves must match `runtime/nvim/init.lua`'s `wolf_rebuild_layout`. A preview leaf's `path` is its source Markdown file, and `isEmptyLeaf` is false for an empty-path preview leaf because preview-ness has to survive persistence.

#### `PaneOcclusionTests.swift`

**Covers**

`PaneOcclusion.isFullyOccluded(gridRect:by:)` — the eight conditions that must all hold before the painter may skip a pane's grid draw and its `read_frame_for_grid` FFI call because a preview web view covers it. Fixtures are built from named geometry constants (a non-square rect at a non-zero origin, plus `overhang` / `shortfall` / `hairGap`) so an `x`/`y` or `w`/`h` confusion fails; new cases should reuse those rather than introduce literals.

**Gotchas**

- Containment tolerance is exactly `0.0`, and `testOccluderShortByAHairOnMaxXEdge` exists to pin it. `syncPreviewWebViews` assigns the web view frame from the same `gridRect(ofPane:)` expression the painter receives, so the rects are bit-identical when in sync; an epsilon would leave the grid unpainted mid-live-resize instead of repainting the moment geometry drifts.
- Every flag case uses a frame that fully contains the grid rect, so a `false` result can only be attributed to the flag under test. The control assertion at the end of `testContainingOccluderWithASingleDisqualifyingFlagPaints` is what proves that.

#### `PaneTreeTests.swift`

**Covers**

`PaneTree` topology and geometry: construction, split (a same-orientation split inserts a flat sibling after the target; a perpendicular one nests), remove with collapse and same-orientation merge, `neighbor` in all four directions, stable DFS ordering of `leaves()` and `paneRects()`, and the `Orientation(ffi:)` / `FocusDir(ffi:)` discriminant table shared with `wolf-ffi`. Weight bookkeeping and the divider surface belong in `PaneTreeWeightsTests.swift`, not here.

**Gotchas**

- The suite is a 1:1 port of the tests in `crates/wolf-gnome/src/layout.rs` — same pane ids, same 100x40 bounds, same helper names. Keep new cases in that vocabulary so the two suites stay diffable.
- A split re-equalizes the row, so unequal weights cannot be produced by splitting. A case needing an unequal precondition builds it with a `setPairFraction` drag, as `testRemoveProportionallyRenormalizesRow` does.

#### `PaneTreeWeightsTests.swift`

**Covers**

The macOS-only weight layer on top of `PaneTree`'s ported topology: `setPairFraction(path:boundary:leftFraction:)` (moves weight between only the two adjacent children, clamps at both extremes so neither pane collapses, leaves a perpendicular nesting alone, no-ops on an invalid path or boundary) and `dividers(width:height:)` (one entry per interior boundary, with orientation, `lineRect`, `pairStart` / `pairSpan`, `before` / `after` and `fraction`).

It also pins the two policies that pull in opposite directions: a same-orientation split re-equalizes the whole row, while a remove renormalizes survivors proportionally so dragged relative sizes survive.

**Gotchas**

- A divider's `lineRect` is zero-thickness on its own axis (zero-width for a vertical divider, zero-height for a horizontal one); the pair extent is carried by `pairStart` / `pairSpan`, so assert on those.
- Since splits re-equalize, any unequal starting weights in a new case must come from a `setPairFraction` call.

#### `PickerPanelSeamTests.swift`

**Covers**

`PickerPanelSeam.frame(...)`, the arithmetic that places the floating picker top-center over its parent window's content, expressed in content space with a top-left origin. Pins that only the sizes clamp — `w` to the content width, `h` to the content remaining below the margin — while `x` centers with a floor at zero and `y` is the fixed `topMargin` echoed straight through, along with the exact-fit boundary and the never-negative sweep.

**Gotchas**

- `y` stays at `topMargin` even when the content is shorter than the margin; only `h` collapses to zero in that case. A case that "fixes" this by clamping `y` would break the shared convention with `EditorInset`.

#### `PickerRefreshCoalescerTests.swift`

**Covers**

`PickerRefreshCoalescer`: a burst of `request()` calls arms exactly one trailing fire, the fire clears the scheduled flag so a later wake reschedules and runs again (the final complete snapshot is never dropped), and `refreshInterval` stays inside the ~30–60fps band.

**Gotchas**

- No timer or run loop is involved. The injected `schedule` closure captures the pending work instead of arming anything, so a new case drives the "timer fired" edge by invoking the captured closure by hand.

#### `PickerSeamTests.swift`

**Covers**

The three pure file-picker seams, each as a table of cases: `isPickerTrigger` (the chord predicate), `nextSelection` (clamped list navigation with no wrap, `nil` on an empty list), and `restoredIndex` (selection restore across streaming result batches). Add a row to the relevant table rather than a new test method.

**Gotchas**

- `isPickerTrigger` matches Command+O, the platform convention for "open". Both front-ends agree on the letter; they differ only in modifier, since wolf-gnome must use Ctrl+Shift. A case asserts that Command+"p" is false, pinning the letter against drift.
- `nextSelection` and `restoredIndex` are exact ports of `crates/wolf-gnome/src/picker.rs` and are meant to stay case-for-case in step with it.

#### `PreviewLinkSeamTests.swift`

**Covers**

`PreviewLinkSeam.classify(url:sourcePath:)` across every branch — a non-`file` scheme goes to the browser, a same-file fragment scrolls, and a cross-file link previews with or without its fragment — plus the detail that a same-file link with no fragment previews the file rather than scrolling. Path comparison runs through `standardizedFileURL`, so `/a/./x.md` and `/a/x.md` are the same file. It also pins that `scrollIntoViewJS(fragmentId:)` JSON-escapes the id before interpolating it.

#### `PreviewScrollSeamTests.swift`

**Covers**

- `PreviewScrollSeam`'s anchor arithmetic (`anchorColumn`, `anchorIds` and their dedup at column zero), the edge-threshold band including its inclusive boundaries and the short-viewport cap, `winsNeedingScroll`, `splitDocument`'s first-open / last-close scan and the shapes it refuses, and the ordered rules of `captureDecision`, `updateStrategy` and `loadCommitOutcome`.
- The synthesized JavaScript: that each tuning constant's rendered value appears in the generated source, and that `inPlaceUpdateJS`'s free identifiers are exactly `inPlaceUpdateArgumentNames`.

**Gotchas**

- Scalar and string inputs only — no AppKit, no `WKWebView`, no Neovim. The on-screen scrolling and the `PaneContainerView` wiring are compile-and-manual-smoke only.
- `markerColumnStride` is asserted equal to the literal `100` that `wolf_core::markdown::MARKER_COLUMN_STRIDE` also carries. Neither side reads the other, so this catches Swift-side drift only; a Rust-side change fails nothing here.

#### `PreviewSeamTests.swift`

**Covers**

Two unrelated `PreviewSeam` decisions plus the click-to-focus surface. `placementMode(...)` is pinned across its three modes, including the two-pane branch that reuses an existing preview pane and the defensive cases that fall back to the active pane. `winsNeedingReload(current:loaded:)` is the per-preview-window reload gate, covered for first bind, source change, tick advance, an unchanged no-op, independent windows, and windows dropped from `current`.

**Gotchas**

- The gate keys on `(sourceBufnr, sourceTick)` rather than the tick alone because two freshly opened files can share a tick; `testSourceBufnrChangedSameTickNeedsReload` is the reused-preview-pane regression that forced it.
- Clearing `loaded` to `[:]` is the mechanism the appearance re-pull relies on to force every preview to re-render, so `testEmptyLoadedReloadsEveryCurrentWin` is load-bearing.
- The result order is dictionary iteration order and is not part of the contract — compare through `Set<Int64>`, never array equality.

#### `RestoreSeamTests.swift`

**Covers**

`RestoreSeam.plan(windows:home:isDirectory:)`, the group-by-cwd project planner: tabs sharing a real directory merge into one project even across saved windows, different directories split, each empty-state or since-deleted tab becomes its own single-tab project launched at `home`, and every tab keeps its `(groupIndex, positionInGroup, cwd, sidebarVisible)` placement plus its `layout` passthrough. Layout rides through verbatim and never influences grouping.

**Gotchas**

- `isDirectory` is injected and the tests pass a set lookup; that is what keeps the seam free of `FileManager`. A new case must list its real directories in that set, including `home`.
- Project order is not part of the contract. Assert through the `projectIndex` helper or an order-independent `Set<PlannedTab>`.
- A since-deleted directory plans at `home` but its `PlannedTab.cwd` retains the original saved string — the grouping key and the retained field differ on purpose.

#### `ScrollAccumulatorTests.swift`

**Covers**

`ScrollAccumulator.consume(deltaY:cellHeight:)`: sub-cell deltas accumulate rather than round away, the signed remainder carries between calls, a direction change crosses back through the remainder to yield a negative step, and a non-positive `cellHeight` short-circuits to zero instead of dividing by zero or looping forever.

#### `SearchBudgetSeamTests.swift`

**Covers**: `SearchBudgetSeam.charBudget` — the floor-then-multiply arithmetic over exact and fractional divisions, the `lines` factor at values other than its default of 2, and the degenerate guard where a zero or negative `charAdvance` still yields a budget of at least `lines` instead of 0 or a divide-by-zero. New rows go in the private `BudgetCase` table with the expected count worked out longhand.

#### `SearchHighlightSeamTests.swift`

**Covers**: `SearchHighlightSeam.spans` — that the result partitions the whole string into ordered, contiguous, non-overlapping, gap-filling spans with `bold` set exactly on the match ranges, and that Rust byte offsets land on the right UTF-16 offsets (an accented BMP scalar is one unit, an emoji is a surrogate pair worth two).

**Gotchas**: `assertPartition` checks the partition property from a text plus a set of expected-bold UTF-16 indices, which is all a new case needs. It deliberately does not pin the span _count_, so an implementation that merges adjacent bold ranges and one that keeps them separate both pass; only the full-cover and empty-range cases assert a count.

#### `SearchToggleSeamTests.swift`

**Covers**: the three seeds for the sidebar search panel's toggle buttons — `initialCaseSensitive` is the inverse of nvim's `:ignorecase`, `defaultUseRegex` is on, and `defaultAllFiles` is off (Markdown-only).

#### `ShortcutSeamTests.swift`

**Covers**: `ShortcutSeam.isNewWindowTrigger` as a table — Command plus `n` or `N` is a trigger (Command+Shift delivers the uppercase glyph in `charactersIgnoringModifiers`, so the match is case-insensitive), while a missing Command, the wrong letter, and an empty string are not.

**Gotchas**: nothing in the live app calls this seam — New Window is a ⇧⌘N menu item, and menu key equivalents preempt `keyDown` — so this test is the only thing exercising it. The wrong-letter row uses `o` on purpose, guarding against collision with the Command+O picker chord.

#### `SidebarMenuSeamTests.swift`

**Covers**: `SidebarMenuSeam.isChecked` across the cross product of `sidebarVisible` and `current` (`.tree` / `.search` / `nil`) against both menu items: only the active panel's entry is checked, a hidden sidebar checks nothing whatever `current` says, and a `nil` current checks nothing.

#### `StateSeamTests.swift`

**Covers**: two tables. `restoredWindowFrame` accepts a frame intersecting any screen's visible frame (second display, partly past an edge) and returns nil for non-positive width or height (the unset sentinel), a fully offscreen frame, and an empty screen list. `launchDecision` returns `.folder` only for a restored path the injected `isDirectory` closure accepts, and `.emptyState(nvimCwd: home)` for empty, deleted, plain-file, and missing paths.

**Gotchas**: `isDirectory` is a set lookup, so add a path to the row's `directories` set to make it a directory. Every `launchDecision` row also asserts the result is not the process working directory — a regression guard against polluting the recent-dirs MRU — so keep that when adding rows.

#### `StatusbarLayoutTests.swift`

**Covers**: `StatusbarLayout`'s vertical arithmetic. One test pins exact values for representative inputs at the 1.25 line factor; the rest assert structural invariants (positive heights, `primaryLineTop == verticalPadding`, the additive `secondaryLineTop`, a height tall enough for both lines) plus the zero-padding and 1pt-font edges.

**Gotchas**: `assertInvariants` is deliberately factor-independent, so a change to the line factor should fail only the exact-value assertions and leave the structural ones meaningful. Derive new expectations from `expectedLineHeight` rather than fresh literals.

#### `StatusbarTextSpanTests.swift`

**Covers**: `StatusbarTextSpan`'s horizontal arithmetic for both `PaneContentKind`s — left at `gridLeft` plus the clamped `paintOffsetX`, width the rest of the grid — across narrow, wide and exactly-at-the-cap panes, an absolute `gridLeft` for a right-hand split, and the degenerate inputs (zero or negative grid width, offset past the right edge, negative offset), with a universal invariant that the width stays non-negative and the span stays inside the grid.

**Gotchas**: both kinds produce identical spans and a test asserts that directly; the enum exists for the call sites, not for divergent math. A sweep across the cap threshold checks the editor branch against a longhand copy of the formula, so dropping `paintOffsetX` there — which would un-center every editor pane wider than the cap — fails loudly. Derive offsets for new cases from `expectedPaintOffsetX` rather than inventing numbers.

#### `TreeChildrenMemoTests.swift`

**Covers**: `TreeChildrenMemo` — repeated lookups of one directory project exactly once, distinct directories project once each, `invalidate()` forces a recompute, and memoized output equals calling the projection directly.

**Gotchas**: the injected `project` closure bumps a counter and the assertions are on the exact count. That count is the point of the seam (`NSOutlineView` asks for one directory's children in a per-child storm), so a new case should assert a projection count, not just the returned array.

#### `TreeRefreshGateTests.swift`

**Covers**: both gates. Visibility — `requestRefresh` defers while hidden, the first `setVisible(true)` with deferred work catches up once and a second returns false, and a request while already visible proceeds. Version — `shouldReload` fires only when the version differs from the last one recorded, and `resetReloadTracking` clears that memory.

**Gotchas**: one test pins the interaction of the two: requests made while hidden must not advance reload tracking, so the version still reloads after the catch-up. That is what stops a hidden sidebar from silently consuming a snapshot it never rendered.
