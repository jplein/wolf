# wolf-macos/Sources/WolfKeyMap

The pure seam: Foundation plus the generated swift-bridge glue, with no AppKit. Everything here is a function of values, which is what makes it unit-testable with no window server — see the testing posture in [DECISIONS.md](../../DECISIONS.md).

When you add behavior to this crate, the decision belongs here and the wiring belongs in [`WolfMacOS/`](../WolfMacOS/README.md). A decision left inside a view class is a decision with no test.

Coverage lives in [`Tests/WolfKeyMapTests/`](../../Tests/WolfKeyMapTests/README.md).

## Files

#### `AppearanceSeam.swift`

Purpose: `AppearanceMode`, the user's configured appearance (auto / dark / light) decoded from the FFI discriminant. Foundation only, no AppKit.

**Decisions**

- The enum is `UInt8`-backed and its case order is the contract shared with `wolf-ffi`'s `mod appearance` and `wolf_core::config::Appearance`, so decoding is plain `AppearanceMode(rawValue:)`. Reordering the cases or dropping the raw type breaks the decode silently.

**Gotchas**

- The seam yields `nil` for an unknown discriminant; `AppDelegate.applyResolvedAppearance` is what turns that into the `.auto` fallback.

#### `CellMetricsMath.swift`

Purpose: Pure line-height and baseline math for the grid cell, split out of `CellMetrics` so it can be exercised without a live font. Every input is a `Double`; no AppKit, no CoreText.

**Public API**

- `cellHeight(fontSize:lineHeightPercent:ascent:descent:leading:)` — for a positive `lineHeightPercent` the height is driven purely by the font size and deliberately ignores the passed-in font metrics. Ascent/descent/leading matter only for the degenerate `lineHeightPercent <= 0` fallback to the font's natural line height.
- `baseline(cellHeight:ascent:descent:)` — offset measured from the top of the cell, splitting the slack evenly above and below the glyph box so glyphs stay vertically centered in a taller cell.

**Gotchas**

- `CellMetrics` supplies the line-height percentage from user config, so a config change moves both the cell height and the baseline together.

#### `CmdlineSeam.swift`

Purpose: Pure line-composition and caret-prefix math behind the macOS command-line overlay (`ext_cmdline`), taking scalar inputs only so it stays headlessly testable.

**Public API**

- `composedLine(prompt:firstc:text:)` — the full drawn line. `prompt` is non-empty only for `confirm()` / `input()`-style prompts; a normal `:` or `/` command line passes an empty prompt.
- `caretPrefix(prompt:firstc:text:byteCount:)` — the prefix whose measured width places the caret. The prompt width is included so the caret sits after the prompt, and `byteCount` honors the byte offset Neovim reports rather than a character index.
- `bytePrefix(of:byteCount:)` — the UTF-8 prefix of at most `byteCount` bytes, decoded lossily when the count splits a multi-byte scalar.

**Gotchas**

- `bytePrefix` is public because both `CmdlineView` and `PaneContainerView`'s `pumAnchorX` need it. Sharing one implementation is what keeps the completion popup anchored to the same column the caret is drawn at.

#### `CommandSeam.swift`

Purpose: `CommandKind`, the typed form of a no-parameter, no-return user action, plus the guarded decode of its FFI discriminant. Foundation only, no AppKit or NSMenu.

**Public API**

- `CommandKind` — one case per `wolf_core::Command` variant, each documented in place with the action it performs.
- `init?(ffi: UInt8)` — `nil` for any discriminant outside the known range.

**Decisions**

- Case order is the contract shared with `wolf-ffi`'s `mod command_kind` and `wolf_core::Command`. This is a plain enum rather than a `RawRepresentable` one, so the mapping is written out explicitly in `init?(ffi:)`; a new command needs a case, an arm there, and a matching constant in the Rust module.

**Gotchas**

- The byte arrives either in an `NSMenuItem.tag` (native menu path) or from a drained `FfiCommand.kind()` (nvim path). Both faces funnel through `CommandDispatcher`, so the decode is the single choke point for either route.

#### `CwdTitleSeam.swift`

Purpose: Computes the window/tab title text from a working-directory path, abbreviating the home prefix to `~` per the macOS convention.

**Public API**

- `displayTitle(cwd:home:)` — an empty or all-whitespace `cwd` returns `""`, leaving the caller to substitute an app-name fallback. A `cwd` outside `home` comes back verbatim as a full absolute path.

**Decisions**

- `home` is always injected: no implicit `$HOME`, no `FileManager`. That injection is what makes the seam deterministic and lets it live in the Foundation-only module.
- Abbreviation matches only on a full path-segment boundary, so `/Users/janet/x` is not abbreviated against home `/Users/jane`. One trailing `/` is stripped from `home` first, and an empty `home` disables abbreviation entirely.

**Gotchas**

- `AppDelegate.refreshTabTitles(forGroupOf:)` feeds it each tab's `persistedCwd`, so the title tracks `:cd` rather than the active file.

#### `DividerDragSeam.swift`

Purpose: Pure divider-drag math for the split-panes UI: maps a pointer position along the drag axis to a clamped left-child fraction for the two panes a divider governs.

**Public API**

- `clampedLeftFraction(...)` — clamps so neither side drops below its minimum grid size, counting the per-side pixel overhead (editor inset plus statusbar) that is unavailable to cells. A pair too small to hold both minima, or a non-positive `pairSpan`, falls back to the midpoint.

**Decisions**

- The clamp lives here rather than on `PaneTree` because it needs cell size and pixel spans, and `PaneTree` is deliberately pixel-free.
- `leadingOverhead` and `trailingOverhead` are separate parameters so an asymmetric layout can be clamped correctly, even though `PaneContainerView` currently passes one orientation-dependent value for both sides.

**Gotchas**

- `PaneContainerView.mouseDragged` supplies the `minPaneCols` / `minPaneRows` floors.

#### `DividerHitTest.swift`

Purpose: Picks the divider under a point from a pre-computed list of `PaneTree.Divider`s.

**Public API**

- `hit(_:point:tolerance:)` — the nearest divider whose zero-thickness line is within `tolerance` of the point and whose span contains the point, else `nil`. A point past the ends of a divider's line does not hit even when it lies on that line's axis.

**Decisions**

- Divider enumeration stays on `PaneTree` because it needs that type's private `Frame`, while picking lives here so it is pure over the public list and testable headlessly.

**Gotchas**

- `PaneContainerView.mouseDown` runs the hit test before the pane-forward loop and returns on a hit without `focus_win` or `send_mouse`, so grabbing a divider emits no stray nvim mouse event.

#### `DrawerFocusSeam.swift`

Purpose: Picks the editor pane that `<C-w>k` from the terminal drawer should focus. Reuses `Rect` / `PaneId` from `PaneTree.swift` and is `Double`-based, so it stays headlessly testable.

**Public API**

- `bottomMostLeaf(_:)` — greatest `y + h` under a top-left origin, so the leaf lowest on screen. Ties break to the smallest `x`, then the smallest `PaneId`.
- `drawerUpTarget(lastEditorPane:leaves:rects:)` — the most-recently-used editor pane when it is still a live leaf, otherwise `bottomMostLeaf`, which is the pane adjacent to the drawer.

**Decisions**

- The drawer is deliberately excluded from `PaneTree`, which preserves the tree's 1:1 parity with wolf-gnome's `layout.rs`. That exclusion is why drawer-origin focus cannot resolve through `PaneTree.neighbor` and needs this bridge.

**Gotchas**

- `PaneContainerView` updates `lastEditorPane` only when the newly active pane is a real editor leaf, never the drawer, so the MRU target can never point back at the drawer.

#### `DrawerLayout.swift`

Purpose: Pure geometry for the terminal drawer — a full-width strip along the bottom of the pane area, with the editor tree tiling over everything above it. Foundation only (`Double`, reusing `Rect` from `PaneTree.swift`), so it is testable with no view.

**Public API**

- `init(paneAreaHeight:hasDrawer:weight:)` — splits the pane-area height into `editorHeight` and `drawerHeight`. With no drawer the editor takes the whole area and `drawerHeight` is 0.
- `drawerRect(width:) -> Rect?` — the full-width bottom strip, or `nil` when no drawer is present.

**Decisions**

- The drawer is held outside the `PaneTree` so the tree keeps 1:1 parity with wolf-gnome. That is why this split lives in its own seam rather than as another tree node.
- `defaultWeight` is the front-end's copy of the height `_G.wolf_toggle_terminal` gives its `botright` split; the two must agree or the drawer pane and its nvim window disagree about size.
- Both heights clamp at zero, so a pane area shorter than the rounded drawer strip never yields a negative editor height.

**Gotchas**

- The drawer is not a tree leaf, so it has no `PaneTree` divider — its top edge is not drag-resizable — and `PaneTree.neighbor` cannot resolve focus moves out of it. `DrawerFocusSeam` bridges that.

#### `EditSeam.swift`

Purpose: The pure placement decision for the Edit Source command — from a window's pane count and what the other pane holds, it picks the `mode` string handed to core's `edit_source(mode)`. The inverse of `PreviewSeam`; Foundation only, so it needs no live view or Neovim.

**Public API**

- `placementMode(paneCount:activePaneIsPreview:otherEditorPaneCount:) -> String` — `"split_left"` when the preview is the only pane, so the source opens in a new pane to its left; `"reuse_editor_pane"` when there are exactly two panes, the active one is a preview and the other is an editor, so the source binds there and takes focus instead of clobbering the preview; `"active"` otherwise, replacing the preview in the active pane.

**Decisions**

- Both extra parameters are defaulted, so a caller that passes only `paneCount` can reach `"split_left"` or `"active"` but never the reuse case. Reuse is opt-in.
- `activePaneIsPreview: false` declines reuse defensively — Edit is only meaningful fired from a preview pane.

**Gotchas**

- The return values are a string contract with core's `edit_source`, not free-form labels. `PaneContainerView.editPlacementInputs()` derives the three inputs from the live pane layout.

#### `EditorInset.swift`

Purpose: Pure arithmetic for the per-pane text-grid inset. Given one pane's size and its statusbar-strip height, it produces the pane-local rectangle the grid paints into, plus the horizontal inset the statusbar and cmdline text share so they left-align with grid text.

**Public API**

- `padding` — the uniform inset and the single source of truth for it: `PaneContainerView` insets every pane by it, and `MainWindow` grows its initial content size by twice it in each dimension so the initial grid is exactly cols × rows.
- `init(paneWidth:paneHeight:statusbarHeight:padding:)` — derives `gridRect` and `textLeftInset`.

**Decisions**

- Only `gridRect.w` and `.h` clamp to a non-negative floor, so a pane narrower or shorter than the insets never yields negative grid dimensions. The `x` / `y` origins are fixed offsets and deliberately are not clamped.
- `Double` and the `Rect` from `PaneTree.swift` rather than `CGFloat` / `CGRect`, keeping the seam out of CoreGraphics and headlessly testable.

**Gotchas**

- `gridRect` is pane-local: its origin is the pane's top-left, and the caller offsets it into the pane's view rect.
- `textLeftInset` is the distance from the pane's left edge to the grid, not the statusbar text's origin. That origin also picks up the column-cap centering offset, which this seam knows nothing about; `StatusbarTextSpan` owns that step.

#### `FontSizeSeam.swift`

Purpose: The pure step-and-clamp arithmetic behind the View menu's per-window font-size commands. `Double`-typed (the controller bridges to and from `CGFloat` at its call sites) and Foundation only, so it can be asserted headlessly to a tight accuracy.

**Public API**

- `clamped(_:)`, `increased(_:)`, `decreased(_:)`, plus the `minPointSize` / `maxPointSize` / `step` constants.

**Decisions**

- Reset is not in the seam. It restores the window's stored launch default, which this type cannot know; `WolfWindowController` owns it.
- The seam is arithmetic only — re-measuring `CellMetrics`, reflowing the grid and repainting are AppKit glue in the controller.

#### `FrameStats.swift`

Purpose: The pure half of the dev-only frame-time instrumentation — the bucket set, accumulation, percentiles, report formatting and the `WOLF_FRAME_STATS` env parsing. The clock, report timer and stderr writes live in `WolfMacOS/FrameMetrics.swift`.

**Public API**

- `FrameStatsBucket` — the instrumented regions on the keystroke-to-pixel path; `label` is the report-table name.
- `record(_:nanos:)`, the two counter notes, `reset()` and `summarize(index:elapsedSeconds:)`, plus the statics `format(_:)`, `percentile(_:_:)`, `isEnabled(env:)` and `reportInterval(env:)`.

**Decisions**

- The bucket raw values are storage indices into a dense array. Keep them dense and ascending from zero: that is what keeps the hot path free of dictionary hashing and string allocation.
- No internal locking, and documented main-thread-only. `FrameMetrics` owns the single instance and enforces that with a `#if DEBUG` `dispatchPrecondition` — kept out of this file so tests can drive the type off the main thread.
- `record` is O(1) with no allocation (every ring reserves `sampleCapacity` in `init`); sorting happens only in `summarize`, once per report.
- Percentiles are nearest-rank rather than interpolated, so every number in the table was actually observed. Only the percentile window is capped; `count`, the sum and `max` stay exact, and a bucket past the cap reports `truncated`.

**Gotchas**

- The six `pane.*` buckets are not a partition. `pane.vec` and `pane.glyphs` are nested inside `pane.cells`, so summing all six double-counts.
- `pane.status` is recorded for occluded panes and `pane.read` is not, so their counts differ.
- `wakesPerKeystroke` is an upper bound: mouse, scroll, resize and out-of-band core wakes inflate it.

#### `GlyphStyle.swift`

Purpose: Resolves one cell's packed style-flag bits plus its foreground and special colors into a paint-ready `GlyphStyle` — the single mapping point the renderer and its unit tests share. Foundation only, so it needs no live font.

**Public API**

- `GlyphFace` — the concrete face a glyph is painted with.
- `GlyphStyle` — the resolved face, the underline and strikethrough flags, and the colors each is stroked in. Its memberwise init is public so the renderer can build a plain `.regular` style for the cursor glyph.
- `GlyphStyleResolver.resolve(flags:fg:special:)` — `flags` is `FfiCell.flags()`.

**Decisions**

- The flag-bit constants mirror `mod flag` in `crates/wolf-ffi/src/lib.rs`, which is the source of truth; they have to move in lock-step with it.
- Decorations carry their own colors because the renderer strokes them manually: `CTLineDraw` ignores AppKit decoration-color attributes, so routing them through CoreText attributes instead would silently drop them.
- Undercurl resolves to a straight underline.
- `extension FfiColor: Equatable` lives here because swift-bridge emits no `Equatable` for transparent structs, and `GlyphStyle` needs one to synthesize its own.

#### `HistorySeam.swift`

Purpose: The pure decisions behind the per-pane navigation history — the History menu's rows, and the Back/Forward toolbar items' identifiers, symbols and ordering. Two concerns share one file because they share the identifier constants. Foundation only, so no `NSMenu` or `NSToolbar` is involved.

**Public API**

- `Row.index` is a stack index handed straight back to `history_jump`, not a row number.
- `rows(displays:indices:currentFlags:)` — zips the three parallel arrays from `WolfCoreHandle.history_entries`. Mismatched lengths truncate to the shortest; a negative index is dropped; at most one row keeps `isCurrent`, the first if several arrive; empty input yields `[]` and the caller substitutes `emptyPlaceholderTitle`.
- `itemIdentifiers(toggleSidebar:trackingSeparator:)` — the locked toolbar order after the traffic lights, as raw strings so the seam stays AppKit-free.

**Decisions**

- Core hands the entries over newest-first, and this preserves that order verbatim: it never re-sorts and never renumbers, because renumbering would invalidate the stack indices `history_jump` expects.
- `chevron.backward` / `chevron.forward` rather than `chevron.left` / `.right` because they are RTL-aware, and SF Symbols rather than `NSImage.Name.goBackTemplate` because the toolbar is already SF-Symbol-styled `.iconOnly` unified.

**Gotchas**

- Wolf holds no copy of the history. Core is the single source of truth and the rows are rebuilt from it on every `menuNeedsUpdate`.

#### `KeyComposition.swift`

Purpose: The two pure decisions bracketing the Carbon/`UCKeyTranslate` dead-key composer (`KeyComposer`, in the AppKit module) that backs option-as-option. Imports only Foundation and the generated `FfiKeyEvent`, so both are testable with scalar inputs — no `NSEvent`, no Carbon, no window.

**Public API**

- `routesThroughComposer(keyCode:ctrl:meta:optionAsAlt:)` — true only in option-as-option mode with neither ctrl nor meta held and a non-named key. Everything else stays on the `WolfKeyMap.translate` path.
- `charEvents(for:) -> [FfiKeyEvent]` — one modifier-free Char event per Unicode scalar of a composed string.

**Decisions**

- In option-as-option mode every printable-candidate key routes through the composer, not just Option-modified ones, because the committing keystroke of a dead-key pair carries no Option and must still reach the composer to combine.
- Named keys (Return, arrows, F-keys) are excluded via `WolfKeyMap.isNamedKey` — the composer only yields characters.
- C0 and `DEL` scalars are filtered out of `charEvents`: they are not input.

**Gotchas**

- An empty result from `charEvents` is normal, not a failure — a dead key still pending composes to `""`, and the caller consumes the event without sending anything.
- The Char kind discriminant and the control-scalar bounds duplicate `KeyMap.swift`'s privates and must stay in step with the Rust `FfiKeyEvent` kind scheme.

#### `KeyMap.swift`

Purpose: Pure key translation for the macOS front end — the fields an `NSView.keyDown(_:)` pulls off an `NSEvent` become an `FfiKeyEvent` for `WolfCoreHandle.send_key`. It imports only Foundation and the generated FFI struct, so it is testable with no window or event loop.

**Public API**

- `translate(keyCode:characters:charactersIgnoringModifiers:ctrl:shift:alt:meta:) -> FfiKeyEvent?` — `nil` means the press produces no input: a bare modifier key, or an unhandled control character.
- `isNamedKey(keyCode:) -> Bool` — the keys `translate` resolves from `keyCode` alone; `KeyComposition` uses it to keep those keys off the Option-composition path.

**Decisions**

- The Shift contract must match `wolf-core/src/input.rs`. Named keys keep the real `shift` flag so core can emit `<S-Tab>` / `<S-Left>`; printable characters instead carry the already-shifted glyph in `ch` with `shift = false`, because core ignores `shift` for `Key::Char` and would otherwise double-apply it.
- Named keys are recognized from `keyCode` before `characters` is consulted, since a named key's `characters` is usually a control scalar.
- When ctrl/alt/meta are held the glyph comes from `charactersIgnoringModifiers`, lowercased, so Ctrl+w yields `w` rather than U+0017.
- The private `Kind` discriminants are a wire contract with `wolf-ffi`'s `key_event_from_ffi` and the variant order of `wolf_core::input::Key`, not local constants.

**Gotchas**

- On macOS the Backspace key is `kVK_Delete` and forward-delete is `kVK_ForwardDelete` — easy to transpose.

#### `PaletteSeam.swift`

Purpose: The command palette's pure decision seam, the sibling of `PickerSeam`. Foundation only, so it is unit-testable headlessly.

**Public API**

- `PaletteRowKind: UInt8` with a guarded `init?(ffi:)` — decodes `wolf-ffi`'s `mod palette_row_kind`, returning `nil` for an out-of-range discriminant.
- `isPaletteTrigger(charactersIgnoringModifiers:command:) -> Bool` — the Command+P chord, matching the letter case-insensitively. Shift, ctrl and alt are neither required nor disqualifying.

**Decisions**

- `isPaletteTrigger` is dormant and unwired: Command+P is a menu key-equivalent that preempts `keyDown`, so the View-menu item is the real trigger and nothing in `PaneContainerView.keyDown` calls it. It is kept, and pinned by a test, so the palette chord has the same shape as the picker chord — do not delete it as dead code.
- The predicate reads `charactersIgnoringModifiers` because it is layout independent and, unlike when Ctrl is held, Command does not mangle it into a control scalar.

#### `PaneGeometry.swift`

Purpose: The per-pane pixel/cell coordinate seam — a grid rect positioned inside a pane rect, plus the derived grid dimensions and mapping. Every pixel-to-cell decision in `PaneContainerView` (mouse hit-testing, scroll, scrollbar placement, the paint offset) routes through it.

**Public API**

- `init(paneRect:cellWidth:cellHeight:maxCols:)` — `maxCols` caps the grid at that many columns and centers it horizontally in the pane, leaving equal side margins; a narrower pane is unaffected. Passing `nil` disables the cap so the grid fills the pane.
- `cellAt(x:y:) -> (row, col)` — takes a point in `paneRect`'s space and clamps into the grid, so a point in a side margin or past an edge resolves to the nearest edge cell instead of an out-of-range index.

**Decisions**

- `Double` rather than `CGFloat`, and `Rect` reused from `PaneTree.swift`, so the type pulls in no CoreGraphics and stays in the headless `WolfKeyMap` module.

**Gotchas**

- `paintOffsetX` is non-zero exactly when the cap centers the grid. The preview web view's frame, the statusbar text span (via `StatusbarTextSpan`) and the occlusion test all add that same offset, so they must derive from one shared `PaneGeometry` or the pane's contents will disagree about where the left edge is.

#### `PaneLayoutCodec.swift`

Purpose: The pure codec between the live `PaneTree` layout and the shapes that persist or cross the FFI and Lua boundaries.

**Public API**

- `SavedPaneNode` — a `Hashable` mirror of the split tree keyed by per-leaf buffer path. `leafCount` is how many nvim window handles the rebuild path zips onto the leaves. `isEmptyLeaf` is the suppress-from-persistence predicate for a lone pane: true only for an unnamed, non-preview leaf, so a lone pane bound to a file or a preview still persists.
- `encode(_:paths:previews:)` — maps leaf ids to buffer paths; a leaf missing from `paths` becomes `""`.
- `postOrderTokens(_:)` and its inverse `decodeTokens(_:)` — the post-order stream the FFI push-builder replays; `decodeTokens` returns `nil` on a malformed stream.
- `specJSON(_:)` — the JSON string that `runtime/nvim/init.lua`'s `wolf_rebuild_layout` decodes.

**Decisions**

- A `preview` leaf stores the source Markdown file's absolute path, not the scratch buffer's, because restore reopens it through `wolf_preview_file` rather than `:edit`.

**Gotchas**

- The JSON key names and the `"v"` / `"h"` orientation tags are a contract with the Lua helper, and the token stream is a contract with `wolf-ffi`. Neither can be changed on one side alone.

#### `PaneOcclusion.swift`

Purpose: The pure arithmetic behind the preview-web-view occlusion skip — may a pane's text grid paint be skipped because a `WKWebView` fully covers it? `PaneContainerView` samples the live web view into an `OccluderGeometry` at paint time and asks.

**Public API**

- `OccluderGeometry` — the headless sample: the frame in the grid rect's coordinate space, plus `isHidden` (fed from `isHiddenOrHasHiddenAncestor`), `isAttached`, `alpha` and `hasLoadedContent`. Public memberwise init, so tests build fixtures with no AppKit view.
- `isFullyOccluded(gridRect:by:) -> Bool` — true only when every disqualifier is clear and the occluder contains the grid rect on all four edges. Partial coverage is always false; the seam answers only "fully occluded", keeping the caller's choice binary.

**Decisions**

- The containment tolerance is exactly 0.0, and no epsilon may be introduced. What makes skipping safe is that the web view physically covers the region whose paint was skipped — not that the two frames are in sync. An epsilon, inset, transform, clipping ancestor or differing coordinate space would let a slightly-too-small web view suppress the paint underneath it and leave a garbage region on screen.
- `hasLoadedContent` is required because a web view that exists but has loaded nothing is blank white.
- A degenerate `gridRect` reports not occluded: there is no paint work to save, and true would assert coverage of a rectangle that does not exist.

**Gotchas**

- Only the grid may be skipped. The statusbar strip sits outside `gridRect`, is never covered, and always paints.

#### `PaneTree.swift`

Purpose: The n-ary pane-layout tree behind the macOS split-panes UI, matching Neovim's frame model — a port of `crates/wolf-gnome/src/layout.rs`. `PaneContainerView` paints each pane from `paneRects()` / `layout()` into one flat view, and drags interior dividers with `dividers()` / `setPairFraction(...)`.

**Public API**

- `PaneTree` is a reference type mutated in place; it is never empty. `rebuilt(from:firstLeafId:)` reconstructs a tree from a `SavedPaneNode` and returns leaf ids in the same DFS order as `leaves()`, so the caller can zip them to restored window handles.
- `Divider.lineRect` is zero-thickness — the hit tolerance supplies the clickable width. `pairStart` / `pairSpan` cover only the two children that divider governs, which are the same two `setPairFraction` reweights.
- `Orientation` and `FocusDir` carry `init?(ffi:)` decoders for the `wolf-ffi` discriminant tables. `Rect`, `PaneId` and `PaneLayout` are reused by the other geometry seams in this module.

**Decisions**

- The topology contract stays 1:1 with `layout.rs`: a same-orientation split inserts a flat sibling after the target, a perpendicular split nests, remove never empties the tree, and `neighbor` requires strictly positive cross-axis overlap.
- Weight policy diverges from the gnome port on remove alone: survivors renormalize proportionally rather than re-equalizing, so a user-dragged layout survives an adjacent pane closing.
- Foundation only, `Double` never `CGFloat`, so the tree stays in the headless module.

**Gotchas**

- `setPairFraction` silently no-ops on a path or boundary that does not resolve, so re-read `dividers()` after any mutation rather than caching one.

#### `PickerPanelSeam.swift`

Purpose: The pure positioning arithmetic for a floating picker panel over its owning window's content.

**Public API**

- `frame(parentContentWidth:parentContentHeight:panelWidth:panelHeight:topMargin:) -> Rect` — centered horizontally, pinned `topMargin` below the content top, with the sizes clamped so a panel larger than the content never overflows.

**Decisions**

- Following the `EditorInset` convention, only the sizes clamp: `x` is derived by centering and `y` is a fixed offset that is deliberately left unclamped.

**Gotchas**

- The result is in parent content space with a top-left origin. The AppKit glue (`PickerPanelHost`) converts it into a bottom-left screen frame for the child `NSPanel`.

#### `PickerRefreshCoalescer.swift`

Purpose: Collapses a burst of `request()` calls into a single trailing `perform()`. Callers stream a large traversal that wakes once per batch, and each bare refresh would re-rank or rebuild the whole accumulated snapshot.

**Public API**

- `init(schedule:perform:)` — `schedule` arms the trailing fire (in production a `DispatchQueue.main.asyncAfter`) and `perform` is the work. Both are injected so the seam runs without a real run loop and tests can drive the timer edge by hand.
- `request()` — coalesces into the armed fire if one is pending.
- `cancelPending()` — clears the armed flag so the next `request()` re-arms. The already-armed closure still runs, so `perform` must tolerate firing after the caller has closed.

**Decisions**

- The fire clears the scheduled flag before calling `perform`, so a wake arriving during the work reschedules and the final, complete snapshot is never dropped.

**Gotchas**

- Despite the name the seam is generic and shared: the file picker, the sidebar file tree, the sidebar search panel and the preview reload path all own one.

#### `PickerSeam.swift`

Purpose: The pure decision functions behind the macOS file picker — trigger detection and list-selection arithmetic. A port of wolf-gnome's `crates/wolf-gnome/src/picker.rs` seams (`is_picker_trigger`, `next_selection`, `restored_index`), whose semantics it keeps identical.

**Public API**

- `isPickerTrigger(charactersIgnoringModifiers:command:) -> Bool` — Command+O. The letter matches case-insensitively, and Shift, Ctrl and Alt are neither required nor disqualifying.
- `nextSelection(current:delta:nRows:) -> Int?` — clamps into range with no wrap, so an out-of-range or negative `current` is absorbed by the same clamp. `nil` for an empty list.
- `restoredIndex(resetToTop:prev:newLen:) -> Int?` — which row to select after a refresh rebuild, keeping `prev` unless the caller asks to reset to the top.

**Decisions**

- The chord is Command+O rather than the GNOME original's Ctrl+Shift+P: Command+O is the idiomatic macOS open chord and, unlike a Ctrl chord, is not a Neovim keybinding, so intercepting it costs Neovim no key.
- The predicate reads `charactersIgnoringModifiers` because it is layout independent and, unlike when Ctrl is held, Command does not mangle it into a control scalar — it still carries a plain `"o"`.

**Gotchas**

- `nextSelection` and `restoredIndex` also drive the command palette and the buffer picker, so a change here is not confined to the file picker.

#### `PreviewLinkSeam.swift`

Purpose: Classifies a link clicked in the Markdown preview web view into one of three actions, and synthesizes the JavaScript that scrolls a fragment into view.

**Public API**

- `Action` — `.openInBrowser(URL)`, `.scrollToFragment(String)`, `.previewFile(path:fragment:)`, whose path is absolute.
- `classify(url:sourcePath:)` — `sourcePath` is the absolute path of the file the pane currently shows. Both sides compare through `standardizedFileURL.path`, so `/a/./x.md` matches `/a/x.md`. An empty fragment counts as no fragment, and a URL with no scheme is defensively previewed when it names a path.
- `scrollIntoViewJS(fragmentId:) -> String`.

**Decisions**

- A same-file `#anchor` becomes a JavaScript scroll rather than a native navigation, so the web view never navigates itself and only ever shows core-rendered HTML.
- `fragmentId` runs through a hand-written JSON escaper instead of `JSONEncoder`, which keeps the file within Foundation. It escapes `<` and `/` on top of the usual set, so an embedded `</script>` cannot break out of the JavaScript string literal.

**Gotchas**

- Every link-routing branch belongs here. `PreviewNavigationDelegate` is deliberately thin glue that only calls `NSWorkspace.open` and `evaluateJavaScript`; behavior added there is untestable.

#### `PreviewSeam.swift`

Purpose: Where a Preview command opens its pane, which preview windows need their web view reloaded, and the click-to-focus script a preview pane injects.

**Public API**

- `placementMode(paneCount:activePaneIsPreview:otherPreviewPaneCount:) -> String` — the `mode` string handed to core's `wolf_open_preview`: `"split_right"`, `"reuse_preview_pane"`, or `"active"`.
- `PreviewLoad` — one preview window's `(sourceBufnr, sourceTick)` reload key.
- `winsNeedingReload(current:loaded:) -> [Int64]` — order is unspecified, and a window present in `loaded` but absent from `current`, a dropped preview, is never returned.
- `clickFocusHandlerName` and `clickFocusJS(handlerName:)`.

**Decisions**

- The reload gate keys on the source buffer number as well as the tick. Two freshly opened files can share a `changedtick`, so a tick-only gate leaves a reused preview pane rendering the file it was bound to before.
- `PreviewLoad` is a struct rather than a tuple so `Optional` inequality compiles, which is what lets the gate be one dictionary comparison.
- `clickFocusJS` registers `mousedown` in the capture phase. The hosting `WKWebView` swallows AppKit mouse events, so the container's `mouseDown` never fires over a preview pane, and capture stops an inner element that halts propagation from suppressing the focus notification.
- Pane reuse is declined when the active pane is itself a preview, so a preview is never opened from inside a preview.

#### `RestoreSeam.swift`

Purpose: The pure restore planner. It rebuilds projects from the saved (window, tab, working directory) tree, because project identity is not persisted — see [DECISIONS.md](../../DECISIONS.md) for that model.

**Public API**

- `plan(windows:home:isDirectory:) -> RestorePlan` — `isDirectory` is the seam's only injected impurity (the app passes a `FileManager` check, tests pass a set lookup). Projects come back in first-encounter order.
- `SavedTab` and `SavedWindow` are the inputs; `PlannedTab`, `ProjectPlan` and `RestorePlan` the outputs. A `PlannedTab` retains its `groupIndex` and `positionInGroup` so the window-assembly phase can drop it back into the right OS window and tab group.

**Decisions**

- An empty-state tab (`cwd == ""`) and a tab whose saved directory has since been deleted each get a single-tab project of their own rather than sharing one, matching "a new window is a new project". Their `launchCwd` is `home`, while `PlannedTab.cwd` still carries the original saved string.
- The seam does not canonicalize. The caller persists an already-canonical working directory via `cwd_for_tabpage`, so grouping is on the string exactly as saved.
- `layout` is a passthrough: it rides from `SavedTab` into the matching `PlannedTab` untouched and never affects grouping, which is by working directory alone.

#### `ScrollAccumulator.swift`

Purpose: Accumulates the sub-cell `deltaY` values that trackpads and high-resolution wheels deliver into discrete whole-cell line steps, so the caller can forward a whole number of line-scroll events to Neovim.

**Public API**

- `consume(deltaY:cellHeight:) -> Int` — the signed count of cell boundaries newly crossed. A non-positive `cellHeight` returns 0.
- `reset()` — clears the carry; the caller invokes it when the scroll target pane changes.

**Decisions**

- The remainder is signed and carries between calls, so a run of sub-cell nudges crosses a boundary exactly once and a direction reversal is symmetric.
- The seam is direction-agnostic: positive means the `+deltaY` direction and nothing more. Direction belongs to the single flip point in the AppKit caller, where `scrollingDeltaY` already reflects the system natural-scroll setting. A second flip here would double-invert it.

#### `SearchBudgetSeam.swift`

Purpose: Converts a sidebar pixel width and a monospace per-character advance into an integer character budget for a matched search line wrapped across up to `lines` rows.

**Public API**

- `charBudget(sidebarWidth:charAdvance:lines:) -> Int` — an unmeasurable font (`charAdvance <= 0`) floors the per-row count to 1 rather than dividing by zero.

**Decisions**

- The budget is a character count rather than a pixel width because it is consumed by wolf-core's `context_window`, which slices on character boundaries and so never splits a multibyte scalar.

#### `SearchHighlightSeam.swift`

Purpose: Maps wolf-core's byte-offset bold match ranges onto the UTF-16 spans that `NSAttributedString` and `NSRange` need.

**Public API**

- `SearchSpan` — one run of the display line with a uniform `bold` flag, measured in UTF-16 units.
- `spans(text:boldByteRanges:) -> [SearchSpan]` — `boldByteRanges` are byte `[start, end)` offsets into `text`'s UTF-8, the shape core returns. The result is ordered, contiguous and gap-filling, covering every UTF-16 unit with exactly one span, so a caller can style the whole line by walking the array and needs no fallback for an uncovered remainder. Empty ranges yield a single non-bold span, and an empty string yields none.

**Decisions**

- The seam exists because the two sides index differently: Rust returns byte offsets while AppKit's text ranges are UTF-16, and a scalar outside the basic multilingual plane occupies two UTF-16 units, so the offsets cannot be reused as-is.

#### `SearchToggleSeam.swift`

Purpose: The initial states for the sidebar search panel's three toggle buttons.

**Public API**

- `initialCaseSensitive(ignorecase:) -> Bool` — the inverse of Neovim's `:ignorecase`.
- `defaultUseRegex` — regex search is on by default. `defaultAllFiles` — off by default, so search covers Markdown only.

**Decisions**

- Case sensitivity is seeded from Neovim once, when the panel is first shown; toggling the button never writes back, so the search panel cannot change the editor's `:ignorecase`.

#### `ShortcutSeam.swift`

Purpose: The pure decision seam for the new-window chord, the sibling of `PickerSeam`. Foundation only, so it is unit-testable headlessly.

**Public API**

- `isNewWindowTrigger(charactersIgnoringModifiers:command:) -> Bool` — the Command+N chord, matching the letter case-insensitively. Shift, ctrl and alt are neither required nor disqualifying.

**Decisions**

- It reads `charactersIgnoringModifiers` because that is layout independent and, unlike when Ctrl is held, Command does not mangle it into a control scalar.

**Gotchas**

- Dormant and unwired: menu key equivalents preempt `keyDown`, and both chords are menu items (File > New is ⌘N, File > New Window is ⇧⌘N), so nothing in the app calls this. It is kept, and pinned by `ShortcutSeamTests`, so the window chord has the same shape as the picker chord — do not delete it as dead code.
- Because shift is not consulted, the predicate is true for ⇧⌘N as well as ⌘N. Wiring it into a `keyDown` path as-is would not distinguish New from New Window.

#### `SidebarMenuSeam.swift`

Purpose: The pure decision behind the View menu's Project and Search checkmarks. Foundation only, so it is testable without an `NSMenuItem` or a window.

**Public API**

- `SidebarPanel` — names one sidebar panel, the file tree or the content search. Also used by `WolfWindowController` to name the live panel.
- `isChecked(sidebarVisible:current:item:) -> Bool` — at most one entry is checked; a hidden sidebar or a nil `current` checks nothing.

**Gotchas**

- Checked and enabled are separate concerns. `AppDelegate.validateMenuItem` uses this seam only for the checkmark and always leaves both items enabled, since choosing either while the sidebar is hidden is what reveals it.

#### `StateSeam.swift`

Purpose: The pure decision seams for the macOS state-restore path — validating a saved window frame, and deciding whether a window opens a folder or the empty state. Foundation and CoreGraphics only; the AppKit glue feeds it real window frames, `NSScreen` visible frames and a `FileManager`-backed directory check.

**Public API**

- `restoredWindowFrame(x:y:width:height:screenVisibleFrames:) -> CGRect?` — `nil` means discard the saved geometry and fall back to the default centered placement. It rejects two cases: a non-positive width or height, which is the "geometry never reported" sentinel; and a frame intersecting no screen's visible frame, which would restore the window fully offscreen after a display is disconnected.
- `launchDecision(restored:home:isDirectory:) -> LaunchDecision` — `.folder(cwd:)` only when `restored` is non-empty and still names a directory, since it may have been deleted or replaced by a plain file since the last run; otherwise `.emptyState(nvimCwd: home)`.

**Decisions**

- `launchDecision` never returns the process working directory. Nvim always needs a cwd, so the empty state launches it at `home`; that is a passive cwd the caller deliberately does not persist or record in the recent-dirs MRU, and a process-cwd fallback would pollute it.
- `isDirectory` is injected rather than reaching for `FileManager`, which is what keeps the seam pure; tests pass a set lookup.

#### `StatusbarLayout.swift`

Purpose: The vertical layout math for a pane's statusbar strip — its total height plus the top offset and height of the primary (filename) and secondary (path) lines. Foundation only: no live font and no `NSString` drawing, so it is unit-testable headlessly.

**Public API**

- `init(primaryFontSize:secondaryFontSize:verticalPadding:lineGap:)` — `verticalPadding` applies both above the primary line and below the secondary; `lineGap` sits between the two. Each line's height is its font size scaled by a fixed private `lineFactor` and rounded up.

**Gotchas**

- This owns only the vertical math; the text's horizontal span is `StatusbarTextSpan`. The two take disjoint inputs — font sizes here, rect scalars and pane kind there — and neither should absorb the other.
- Only this arithmetic is tested. The strip's background fill, string drawing and truncation are AppKit glue in `PaneContainerView.paintStatusbar`, verified by manual smoke only.

#### `StatusbarTextSpan.swift`

Purpose: The horizontal sibling of `StatusbarLayout` — the absolute left edge and width of a pane's statusbar text, so the text aligns with whatever that pane paints. Foundation only (`Double`, no CoreGraphics), so it is unit-testable headlessly.

**Public API**

- `PaneContentKind` — which kind of content the pane paints, `.editor` or `.preview`.
- `init(gridLeft:gridWidth:paintOffsetX:kind:)` — `gridLeft` and `gridWidth` are the pane's absolute grid rect from `PaneContainerView.gridRect(ofPane:)`; `paintOffsetX` is `PaneGeometry`'s grid-centering inset. `left` is absolute, and `width` never goes negative.

**Decisions**

- Both kinds compute the same span. A preview's `WKWebView` is framed to the same capped and centered grid rect as an editor's text grid, so a preview statusbar indents by `paintOffsetX` exactly like an editor's; in full-width mode that inset is 0 and both land flush at `gridLeft`. The parameter is retained because it is the seam where the two would split again if preview framing ever diverged.
- The clamp of `paintOffsetX` into `[0, gridWidth]` is defensive — `PaneGeometry` already floors it at 0 — and is what guarantees `width` stays non-negative and `left` never passes the grid's right edge.

**Gotchas**

- The preview web view's frame, this span, and the draw loop's occlusion test all derive from the same `geometry(_:fullWidth:)` applied to `gridRect(ofPane:)`, and must change together.

#### `TreeChildrenMemo.swift`

Purpose: A per-directory memo for the sidebar file tree's child projection, so the injected `project` runs at most once per directory per invalidation window. Generic over `Item` and Foundation only, so it is testable without AppKit or a live search.

**Public API**

- `init(project:)` — the injected projection from a directory rel-path to its immediate children.
- `children(ofDir:)` and `invalidate()` — `invalidate` drops every cached array, keeping the dictionary's capacity.

**Decisions**

- It exists because `NSOutlineView` asks for one directory's children in a storm — `numberOfChildrenOfItem`, then `child:index:` once per child — and each callback would otherwise re-run the FFI `children_of(dir)`, an O(N) scan of the flat file list.

**Gotchas**

- `FileTreeView` projects the same `TreeItem` instances it hands out from `itemsByRelPath`, so memoizing does not disturb the outline's stable item identity.
- `FileTreeView` invalidates only inside the `TreeRefreshGate.shouldReload` branch and on reroot. A version-unchanged refresh — an expand, a collapse, a cursor move — keeps serving from the still-valid memo, which is the point; invalidating more eagerly reinstates the storm.

#### `TreeRefreshGate.swift`

Purpose: A refresh gate for a sidebar panel on two axes, visibility and file-search snapshot version. Foundation only, so it is testable without AppKit; `FileTreeView` and `SearchView` each own one, relaying the sidebar's real `isCollapsed` KVO into `setVisible` and `search.version()` into `shouldReload`.

**Public API**

- `requestRefresh() -> Bool` — true while visible. While hidden it records that a catch-up is owed and returns false, and the caller does nothing.
- `setVisible(_:) -> Bool` — true only on a hidden-to-visible transition that has deferred work, so exactly one catch-up refresh runs on reappear.
- `shouldReload(version:) -> Bool` — true only when `version` differs from the last reloaded one, which it then records. The initial last-reloaded version is nil, so the first call at any version, including 0, reloads.
- `resetReloadTracking()` — forgets that version so the next `shouldReload` reloads unconditionally. Called on reroot, where the outline is rebuilt wholesale.

**Decisions**

- Both gates guard the same expense, the outline reload and reveal. Without the visibility gate that work runs while the sidebar is collapsed, so hiding the sidebar does not relieve a freeze; without the version gate a full `reloadData()` runs on every render wake even when nothing structural changed.
