# wolf-macos/Sources/WolfKeyMap/Terminal

The pure decisions behind a terminal pane: how a cell's style resolves to paint, which rows a new frame repaints, how a session is sized and a pointer mapped, and where keys and input-method text go. The directory compiles into the [`WolfKeyMap`](../README.md) module, not a module of its own.

The views that act on these decisions are in [`WolfMacOS/Terminal/`](../../WolfMacOS/Terminal/README.md), and the reasons that span both are [the terminal-panes decisions](../../../../../notes/wolf-macos-decisions.md#terminal-panes). Coverage lives in [`Tests/WolfKeyMapTests/`](../../../Tests/WolfKeyMapTests/README.md).

## Files

#### `TerminalCellStyle.swift`

Purpose: Resolves one terminal cell's packed style, from [`FfiTerminalCell`](../../../../wolf-ffi/src/README.md#terminal-sessions), into the colours, opacity and `GlyphStyle` a terminal pane paints.

**Public API**

- `resolve(fg:bg:flags:underline:width:selected:defaultFg:defaultBg:) -> TerminalCellPaint` — colours packed `0x00RRGGBB`.
- `cursorColors(cursorColor:defaultFg:defaultBg:)`, `cursorSpan(width:)`, `TerminalCursorShape`.
- `cursorShape(style:focused:)` — the frame's style, an unknown one as a block, while `focused`; else `.hollow`.
- `Flag`, `Underline`, `Width` — the `terminal_flag`, `terminal_underline` and `terminal_cell_width` values.

**Decisions**

- The colours swap when exactly one of inverse and selected holds, so a selected reverse-video cell draws plain.
- Blink is ignored and every underline but curly draws straight; these are present limitations in [wolf-macos-decisions.md](../../../../../notes/wolf-macos-decisions.md#terminal-panes).

#### `TerminalDamage.swift`

Purpose: Chooses the rows a terminal pane repaints for a new frame, from the frame's per-row versions, the version the pane last painted and the cursor.

**Public API**

- `effectivePaintedVersion(previous:current:painted:)` — `nil`, meaning repaint everything, when the `PaintKey` changed.
- `rowsToRepaint(rowVersions:paintedVersion:)`, `cursorRows(previous:current:)` — ascending row indices.

**Decisions**

- `PaintKey` holds everything besides the cells that changes every row's pixels: the size, the default background, the cell metrics and the backing scale.
- The cursor is not in the row versions, so a moved cursor damages its old and new rows separately.

#### `TerminalGeometry.swift`

Purpose: Sizes a terminal session from its pane, and maps a pointer into the cell pixels core expects.

**Public API**

- `size(cols:rows:cellWidth:cellHeight:scale:) -> TerminalSize` — the `resize_terminal` arguments, the cell in backing pixels.
- `pointerPixels(x:y:originX:originY:cellWidth:cellHeight:size:)` — pixels in `size`'s cell pixels, clamped at 0.
- `sizer(showing:active:previous:)` — the pane that sizes a session within one view.

**Decisions**

- Counts clamp to at least 1 and pixel sizes to at least 1, because core ignores a zero size and keeps the previous cell size for a zero cell.
- The pointer is scaled by the sent cell pixels rather than the backing scale, so core's `floor(px / cell_px)` names the cell under the pointer even after rounding.
- `sizer` prefers the active pane, then the last sizer, then the lowest id: the spec's "most recently focused of its panes", within one view.

#### `TerminalInputSeam.swift`

Purpose: Routes a terminal pane's keys, input-method commits and focus changes.

**Public API**

- `routesThroughInputContext(keyCode:ctrl:meta:alt:optionAsAlt:hasMarkedText:)` — whether a key goes through `interpretKeyEvents` rather than straight to `terminal_key`.
- `committedText(_:) -> TextRoute` — `.key`, `.text` or `.none`.
- `focusReports(previous:current:) -> [FocusReport]` — the loss, then the gain.

**Decisions**

- A commit of exactly one printable ASCII scalar is a key, so core's `<C-w>` router sees the chord's second key; any other commit is text, with C0 and DEL removed, as the spec requires for input-method text.
- Marked text keeps every key in the input method, even a Ctrl chord or a named key.
