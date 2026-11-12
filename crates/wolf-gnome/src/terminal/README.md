# wolf-gnome/src/terminal

The submodules of [`terminal.rs`](../README.md#terminalrs): the pure seams a terminal pane decides by, and the display-free painter it paints through. [Terminal panes](../../../../specs/terminal-panes.md) is authoritative for what a pane does, and the decisions spanning the widget glue are in [`notes/wolf-gnome/terminal-panes.md`](../../../../notes/wolf-gnome/terminal-panes.md).

Four of these port a seam in [`WolfKeyMap`](../../../wolf-macos/Sources/WolfKeyMap/README.md), and each entry states where it diverges. [`terminal_seams.rs`](../../tests/README.md#terminal_seamsrs) pins the seams and [`terminal_paint.rs`](../../tests/README.md#terminal_paintrs) the painter.

## Files

#### `canvas.rs`

Purpose: `TerminalCanvas`, one terminal pane's backing surface: its session's painted cells, repainted row by row as the frame's damage asks, and blitted with the cursor over it on every draw. Display-free Cairo and Pango, like [`render.rs`](../README.md#renderrs); the counterpart of macOS's [`TerminalPaneView`](../../../wolf-macos/Sources/WolfMacOS/Terminal/README.md#terminalpaneviewswift).

**Public API**

- `update(target, frame, metrics, scale) -> Vec<u16>` — repaints the rows the surface lacks and returns them ascending: every row after a paint-key change, none for a frame already painted. The glyphs take `target`'s font options, and `scale` is its device scale.
- `paint(cr, rect, frame, metrics, focused)` — the default background over `rect`, the surface at `rect`'s origin rounded to whole pixels, then the cursor, in the frame's style only while `focused`, all clipped to `rect`.

**Decisions**

- Why the pane paints through a backing surface, and why the cursor is drawn over the blit, is in [Terminal panes](../../../../notes/wolf-gnome/terminal-panes.md).
- A block cursor redraws its glyph without bold or italic, which cannot overflow the cell.
- A row fills every cell's background before any glyph, so a spacer's background cannot cover the right half of a wide glyph. Fills are not antialiased, because adjacent antialiased fills leave a seam.
- The surface is made at the target's device scale and blitted at a whole-pixel origin, so its pixels map 1:1.

#### `damage.rs`

Purpose: which rows of a frame the canvas repaints. Pure; the port of [`TerminalDamage`](../../../wolf-macos/Sources/WolfKeyMap/Terminal/README.md#terminaldamageswift).

**Public API**

- `PaintKey` — everything besides the cells a painted row depends on: the size, the default background, the cell size and the device scale.
- `effective_painted_version(previous, current, painted)` — `None`, meaning repaint everything, when the key changed.
- `rows_to_repaint(row_versions, painted_version)` — the rows newer than `painted_version`, ascending, or every row for `None`.

**Decisions**

- There is no `cursorRows`, unlike macOS, because of where the canvas draws the cursor ([Terminal panes](../../../../notes/wolf-gnome/terminal-panes.md)).

#### `geometry.rs`

Purpose: sizes a session from its pane, and maps a pointer into the cell pixels core expects. Pure; the port of [`TerminalGeometry`](../../../wolf-macos/Sources/WolfKeyMap/Terminal/README.md#terminalgeometryswift).

**Public API**

- `size(cols, rows, cell_w, cell_h) -> TerminalSize` — the `resize_terminal` arguments.
- `pointer_pixels(x, y, rect, cell_w, cell_h, size)` — a pane-local point as pixels from `rect`'s origin in `size`'s cell pixels, clamped at 0, a non-finite result read as 0.
- `sizer(showing, active, previous)` — the pane that sizes a session within one tab.

**Decisions**

- Counts and pixel sizes clamp to at least 1, because core ignores a zero size and keeps the previous cell size for a zero cell.
- `size` takes no backing scale, unlike macOS, so the cell goes in logical pixels: this crate measures its cells at 96 dpi. The pointer is scaled by the same sent cell pixels, so core's `floor(px / cell_px)` still names the cell under it.
- `sizer` prefers the active pane, then the last sizer, then the lowest id: the spec's most recently focused pane, within one tab.

#### `input.rs`

Purpose: where a terminal pane's committed text, focus changes and pointer events go. GDK value types in, `wolf-core` values out; the port of [`TerminalInputSeam`](../../../wolf-macos/Sources/WolfKeyMap/Terminal/README.md#terminalinputseamswift), less its key routing.

**Public API**

- `commit_route(text) -> TerminalCommit` — `Key`, `Text` or `Nothing`.
- `focus_reports(previous, current) -> Vec<FocusReport>` — the loss, then the gain.
- `mouse_button(button)` — GDK's primary, middle and secondary buttons; any other is `None`.
- `pointer_mods(state) -> PointerMods` — Shift, Control and Alt; every other bit is dropped.

**Decisions**

- A commit of exactly one printable ASCII scalar is a modifier-free key, so core's `<C-w>` router sees a chord's second key: GTK's simple context claims plain printables, so without this rule no chord could fire. Other text loses its C0 and DEL scalars.
- No counterpart to `routesThroughInputContext`: every press is offered to the window's input context, whose declining does the bypass, and a declined press reaches `terminal_key` through `gdk_key`.
- Three buttons, where an editor pane forwards the primary alone ([`mouse.rs`](../README.md#mousers)), because a program tracking the mouse reads middle and right clicks.

#### `style.rs`

Purpose: how one cell paints: its colours after inverse, selection and faint, whether it draws a glyph, and the style the glyph painter takes; also the cursor's colours, span and style. Pure; the port of [`TerminalCellStyle`](../../../wolf-macos/Sources/WolfKeyMap/Terminal/README.md#terminalcellstyleswift).

**Public API**

- `resolve(cell, selected, default_fg, default_bg) -> CellPaint`.
- `is_selected(frame, col, row)` — the selection's ends are inclusive.
- `cursor_colors(frame)` — `(fill, glyph)`; `cursor_span(width)` — two cells over a wide lead.
- `cursor_style(style, focused)` — `style` while `focused`, else `BlockHollow`.

**Decisions**

- The colours swap when exactly one of inverse and selected holds, so a selected reverse-video cell draws plain.
- Faint blends the foreground toward the background by `FAINT_ALPHA` through [`statusbar_layout.rs`](../README.md#statusbar_layoutrs)'s `blend`, keeping the paint opaque as the scrollbar thumb's is, where macOS uses an alpha.
- Double, dotted and dashed underlines draw as the straight underline and curly as the undercurl, as on macOS. Blink is ignored, because this crate runs no periodic timer.
