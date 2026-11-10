# wolf-macos/Sources/WolfMacOS/Terminal

The view that paints a terminal pane, and the glyph painter it shares with the Neovim grid. The directory compiles into the [`WolfMacOS`](../README.md) module, not a module of its own.

The pure decisions these views act on are in [`WolfKeyMap/Terminal/`](../../WolfKeyMap/Terminal/README.md), and the reasons that span both are [the terminal-panes decisions](../../../../../notes/wolf-macos-decisions.md#terminal-panes). What `TerminalPaneView` paints is covered offscreen in [`WolfMacOSTests`](../../../Tests/WolfMacOSTests/README.md).

## Files

#### `GlyphPainter.swift`

Purpose: Draws one cell's glyph and its underline and strikethrough with CoreText into a flipped context, for both the Neovim grid and terminal panes.

**Public API**

- `drawGlyph(_:text:x:y:fg:style:)` — at the cell geometry of `metrics`.
- `plainStyle` — the style of the glyph redrawn under a block cursor, so a bold face cannot overflow the cell.

**Decisions**

- Decorations are stroked by hand in `style`'s colours: `CTLineDraw` draws them only in the foreground colour and has no wavy underline.

#### `TerminalPaneView.swift`

Purpose: A subview of `PaneContainerView` over one terminal pane's content area, painting its session's frame with CoreText.

**Public API**

- `update(_:scale:)` — stores a frame and invalidates only the rows [`TerminalDamage`](../../WolfKeyMap/Terminal/README.md#terminaldamageswift) picks, so an unchanged frame invalidates nothing.
- `applyMetrics(_:)` — repaints every row.
- `markedText` — the input method's uncommitted text, painted underlined at the cursor in its place. `cursorRect` is the cursor cell, for the candidate window.

**Decisions**

- It is a subview rather than painted by the container because every wake dirties the whole container, and a terminal publish shares the wake without bumping `Snapshot.seq`, so the container cannot tell a terminal-only wake apart. The subview keeps its unchanged rows and repaints only damaged ones.
- `hitTest(_:)` returns `nil`, so every event reaches the container, which routes it to the session.
- It sets `clipsToBounds = true`. The macOS 14 and later SDK default is `false`, and `draw(_:)` fills its dirty rect, which can then reach past the bounds and cover every Neovim pane the container has painted.
- A row fills every cell's background before any glyph, so a spacer's background cannot overpaint the right half of a wide glyph.

**Gotchas**

- A frame larger than the view is clipped and a smaller one padded with the default background, which is the spec's "clipped or padded" while a resize is in flight.
