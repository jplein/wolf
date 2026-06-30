//
//  PaneContainerView.swift
//  The editor area: a SINGLE custom `NSView` that paints every split pane itself.
//
//  The macOS analog of wolf-gnome's `pane_view.rs`, but realized as ONE flat
//  painting view (the proven `GridView` pattern) rather than a tree of nested
//  `NSSplitView`s. It owns the pure `WolfKeyMap.PaneTree` layout, binds each pane to
//  a Neovim window + grid, and on each `draw(_:)` computes every pane's rectangle
//  via `PaneTree.paneRects` and paints that pane's window grid into its rect — so
//  each pane is drawn INDEPENDENTLY of Neovim, leaving room for front-end-only
//  per-pane treatments (e.g. dimming inactive panes). A one-row cmdline strip along
//  the bottom paints the outer grid (grid 1). This view is the editor area's first
//  responder and the single global key sink (keys go to nvim's current window).
//
//  Painting one flat view avoids the fragile compositing of deeply-nested custom
//  layer-backed views; divider drag-to-resize (which `NSSplitView` would give for
//  free) is deferred — splits are equal-weighted (parity with wolf-gnome's v1
//  divider limitation). The pure layout decisions live in the headlessly-tested
//  `WolfKeyMap.PaneTree`; this AppKit glue is compile-verified + manual-smoke only.
//

import AppKit
import CoreGraphics
import CoreText
import WolfFFI
import WolfKeyMap

/// One pane's binding: the Neovim window it follows and the grid it paints
/// (`nil` until resolved via `grid_for_win`).
private struct PaneBinding {
    let win: Int64
    var boundGrid: UInt64?
}

/// The split-panes editor container: a single painting view.
final class PaneContainerView: NSView {
    /// The id of the first (root) pane: the `PaneTree` is constructed with this as
    /// its root leaf, so the first bound pane reuses it (no `split` needed).
    private static let firstPaneId: PaneId = 1
    /// The lowest grid id that is an editor WINDOW grid (grid 1 is the outer/global
    /// grid; window content lives on grids `>= 2`).
    private static let minWindowGrid: UInt64 = 2
    /// The outer/global grid id (cmdline + messages).
    private static let cmdlineGridId: UInt64 = 1
    /// `FfiLayoutIntent.kind()` for a window SPLIT (mirrors `wolf-ffi`).
    private static let intentKindSplit: UInt8 = 0
    /// `FfiLayoutIntent.kind()` for a focus-direction move (mirrors `wolf-ffi`).
    private static let intentKindFocusDir: UInt8 = 1
    /// Scrim opacity laid over an INACTIVE pane (a front-end-only treatment that is
    /// awkward to express inside Neovim itself).
    private static let inactiveDim: CGFloat = 0.28

    private let handle: WolfCoreHandle
    private let metrics: CellMetrics

    /// The pure pane-layout tree (split/remove/neighbour geometry).
    private let tree = PaneTree(root: PaneContainerView.firstPaneId)
    /// The per-pane bindings, keyed by `PaneId`.
    private var panes: [PaneId: PaneBinding] = [:]
    /// The next pane id to allocate (starts at `firstPaneId`).
    private var nextPaneId: PaneId = PaneContainerView.firstPaneId
    /// The currently active pane (cursor target + the one NOT dimmed), if any.
    private var activePane: PaneId?

    /// Invoked when the picker trigger chord (Shift+Ctrl+P) is pressed, BEFORE any
    /// key is translated/forwarded to Neovim. Set by `AppDelegate` to open the
    /// picker.
    var onPickerTrigger: (() -> Void)?

    /// Builds the container bound to a live core `handle` and measured `metrics`.
    /// No panes exist until the first window grid appears on a wake.
    init(handle: WolfCoreHandle, metrics: CellMetrics) {
        self.handle = handle
        self.metrics = metrics
        super.init(frame: .zero)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("PaneContainerView is not constructed from a nib")
    }

    /// Top-left origin so grid row/col map directly to (x, y) in points.
    override var isFlipped: Bool { true }

    /// The container is the single key sink for the editor area.
    override var acceptsFirstResponder: Bool { true }

    // MARK: - Geometry

    /// One-row cmdline-strip height (the bottom of the view).
    private var stripHeight: CGFloat { ceil(metrics.cellHeight) }

    /// The pane area (everything above the cmdline strip).
    private var paneAreaHeight: CGFloat { max(0, bounds.height - stripHeight) }

    /// Maps a pixel size to a grid `(cols, rows)`, each at least 1.
    private func gridCells(width: CGFloat, height: CGFloat) -> (cols: Int, rows: Int) {
        (max(1, Int(width / metrics.cellWidth)), max(1, Int(height / metrics.cellHeight)))
    }

    /// The current pane rectangles from the pure layout, in this view's (flipped)
    /// coordinates: the pane area occupies the top `paneAreaHeight`, the strip the
    /// bottom `stripHeight`.
    private func paneRects() -> [(PaneId, CGRect)] {
        tree.paneRects(width: Double(bounds.width), height: Double(paneAreaHeight))
            .map { ($0.0, CGRect(x: $0.1.x, y: $0.1.y, width: $0.1.w, height: $0.1.h)) }
    }

    // MARK: - Input

    override func keyDown(with event: NSEvent) {
        let flags = event.modifierFlags

        // Picker trigger (Shift+Ctrl+P) is intercepted BEFORE translation so it is
        // never forwarded to Neovim. While the picker is open its search field is
        // first responder, so this stops firing — suppression is automatic.
        if PickerSeam.isPickerTrigger(
            charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "",
            ctrl: flags.contains(.control),
            shift: flags.contains(.shift)
        ) {
            onPickerTrigger?()
            return
        }

        guard
            let ev = WolfKeyMap.translate(
                keyCode: event.keyCode,
                characters: event.characters ?? "",
                charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "",
                ctrl: flags.contains(.control),
                shift: flags.contains(.shift),
                alt: flags.contains(.option),
                meta: flags.contains(.command)
            )
        else { return }
        handle.send_key(ev)
    }

    /// Click-to-focus: focus the window of the pane under the click.
    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        for (id, rect) in paneRects() where rect.contains(point) {
            if let win = panes[id]?.win {
                handle.focus_win(win)
                setActivePane(id)
                needsDisplay = true
            }
            return
        }
    }

    // MARK: - Resize

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        syncGridSizes()
        needsDisplay = true
    }

    override func viewDidEndLiveResize() {
        super.viewDidEndLiveResize()
        syncGridSizes()
        needsDisplay = true
    }

    /// Resizes the outer grid (grid 1) to the whole view and each bound pane's grid
    /// to its current rectangle — but only when a size actually differs from what
    /// Neovim currently has, so this converges (no resize feedback loop). The
    /// authoritative sizes still come back via core's `grid_resize`.
    private func syncGridSizes() {
        guard bounds.width > 0, bounds.height > 0 else { return }

        let global = gridCells(width: bounds.width, height: bounds.height)
        let outer = handle.read_frame_for_grid(Self.cmdlineGridId)
        if Int(outer.cols()) != global.cols || Int(outer.rows()) != global.rows {
            handle.resize(
                UInt16(truncatingIfNeeded: global.cols),
                UInt16(truncatingIfNeeded: global.rows))
        }

        for (id, rect) in paneRects() {
            guard let grid = panes[id]?.boundGrid else { continue }
            let target = gridCells(width: rect.width, height: rect.height)
            let frame = handle.read_frame_for_grid(grid)
            if Int(frame.cols()) != target.cols || Int(frame.rows()) != target.rows {
                handle.resize_grid(
                    grid,
                    UInt16(truncatingIfNeeded: target.cols),
                    UInt16(truncatingIfNeeded: target.rows))
            }
        }
    }

    // MARK: - Render-wake orchestration

    /// Orchestrates a render wake (port of `PaneInner::on_render_wake`): ensure a
    /// first pane exists, apply layout intents, collapse closed panes, resolve
    /// pending grids, refresh the active pane, sync grid sizes, then redraw.
    func onRenderWake() {
        ensureInitialPane()
        applyIntents()
        detectCloses()
        resolvePendingGrids()
        updateActivePane()
        syncGridSizes()
        needsDisplay = true
    }

    /// Creates the first pane (reusing `firstPaneId`, the tree root) once a window
    /// grid (`>= 2`) appears. No-op once any pane exists.
    private func ensureInitialPane() {
        guard panes.isEmpty else { return }
        guard let (win, grid) = pickInitialWindow() else { return }
        let id = allocPaneId()
        panes[id] = PaneBinding(win: win, boundGrid: grid)
        setActivePane(id)
    }

    /// Drains and applies all pending layout intents (splits + focus moves).
    private func applyIntents() {
        for intent in handle.drain_intents() {
            switch intent.kind() {
            case Self.intentKindSplit:
                guard let orientation = Orientation(ffi: intent.orientation()) else { continue }
                applySplit(orientation: orientation, from: intent.from(), win: intent.win())
            case Self.intentKindFocusDir:
                guard let dir = FocusDir(ffi: intent.dir()) else { continue }
                applyFocus(dir: dir, from: intent.from())
            default:
                continue
            }
        }
    }

    /// Applies a split intent: split the `from` pane (fallback: active pane), then
    /// bind a new pane to `win` (grid resolved now or pending). Idempotent if a pane
    /// already exists for `win`.
    private func applySplit(orientation: Orientation, from: Int64, win: Int64) {
        if paneForWin(win) != nil { return }
        guard let target = paneForWin(from) ?? activePane else { return }
        let newId = allocPaneId()
        tree.split(target, newId, orientation)
        let grid = handle.grid_for_win(win)
        panes[newId] = PaneBinding(win: win, boundGrid: grid == 0 ? nil : grid)
        setActivePane(newId)
    }

    /// Applies a focus-direction intent: resolve the neighbour of the `from` pane
    /// (fallback: active pane) in `dir`, then `focus_win` its window.
    private func applyFocus(dir: FocusDir, from: Int64) {
        guard let origin = paneForWin(from) ?? activePane else { return }
        let target = tree.neighbor(
            origin, dir, width: Double(bounds.width), height: Double(paneAreaHeight))
        guard let target, let win = panes[target]?.win else { return }
        handle.focus_win(win)
    }

    /// Detects panes whose bound grid has vanished (a window close) and collapses
    /// them from the tree, never emptying the tree.
    private func detectCloses() {
        let liveGrids = Set(handle.windows().map { $0.grid() })
        let closed = panes.compactMap { id, binding -> PaneId? in
            if let grid = binding.boundGrid, !liveGrids.contains(grid) { return id }
            return nil
        }
        if closed.isEmpty { return }

        for id in closed {
            // Never empty the tree (nvim guards last-window close, but be safe).
            if tree.leaves().count <= 1 { break }
            tree.remove(id)
            panes.removeValue(forKey: id)
        }
        if let active = activePane, panes[active] == nil {
            setActivePane(tree.leaves().first)
        }
    }

    /// Resolves any pane whose grid is still pending via `grid_for_win`.
    private func resolvePendingGrids() {
        for (id, binding) in panes where binding.boundGrid == nil {
            let grid = handle.grid_for_win(binding.win)
            if grid != 0 {
                panes[id]?.boundGrid = grid
            }
        }
    }

    /// Refreshes the active pane from `active_win` (fallback: the pane owning the
    /// cursor's grid).
    private func updateActivePane() {
        var target: PaneId?
        let active = handle.active_win()
        if active != 0 {
            target = paneForWin(active)
        }
        if target == nil {
            let cursorGrid = handle.read_frame().cursor_grid()
            target = panes.first(where: { $0.value.boundGrid == cursorGrid })?.key
        }
        if let id = target, activePane != id {
            setActivePane(id)
        }
    }

    // MARK: - Painting

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }

        // Editor background (covers any not-yet-tiled gaps).
        ctx.setFillColor(WolfColor.fallbackBg)
        ctx.fill(bounds)

        let rects = paneRects()
        for (id, rect) in rects {
            guard let grid = panes[id]?.boundGrid else { continue }
            let frame = handle.read_frame_for_grid(grid)
            paintGrid(frame, into: rect, dimmed: id != activePane, ctx: ctx)
        }
        drawDividers(rects, ctx: ctx)
        paintCmdlineStrip(ctx)
    }

    /// Paints one window grid into `rect`, optionally dimming it (an inactive pane).
    private func paintGrid(_ frame: FfiFrame, into rect: CGRect, dimmed: Bool, ctx: CGContext) {
        ctx.saveGState()
        ctx.clip(to: rect)
        ctx.translateBy(x: rect.minX, y: rect.minY)
        paintCells(frame, ctx: ctx)
        if frame.cursor_visible() {
            drawCursor(ctx, frame: frame, cells: frame.cells(), cols: Int(frame.cols()))
        }
        if dimmed {
            ctx.setFillColor(CGColor(gray: 0, alpha: Self.inactiveDim))
            ctx.fill(CGRect(x: 0, y: 0, width: rect.width, height: rect.height))
        }
        ctx.restoreGState()
    }

    /// Paints the outer grid (grid 1)'s LAST row into the bottom cmdline strip.
    private func paintCmdlineStrip(_ ctx: CGContext) {
        let frame = handle.read_frame_for_grid(Self.cmdlineGridId)
        let rows = Int(frame.rows())
        guard rows > 0 else { return }

        let stripRect = CGRect(x: 0, y: paneAreaHeight, width: bounds.width, height: stripHeight)
        ctx.saveGState()
        ctx.clip(to: stripRect)
        // Place grid 1's LAST row at the strip's top; the rest paints above and is
        // clipped away.
        ctx.translateBy(x: 0, y: stripRect.minY - CGFloat(rows - 1) * metrics.cellHeight)
        paintCells(frame, ctx: ctx)
        if frame.cursor_visible() {
            drawCursor(ctx, frame: frame, cells: frame.cells(), cols: Int(frame.cols()))
        }
        ctx.restoreGState()
    }

    /// Paints all cells of `frame` at grid-local coordinates (the caller has already
    /// translated/clipped the context to the target rect).
    private func paintCells(_ frame: FfiFrame, ctx: CGContext) {
        let cols = Int(frame.cols())
        let rows = Int(frame.rows())
        let cells = frame.cells()
        let cellW = metrics.cellWidth
        let cellH = metrics.cellHeight

        for row in 0..<rows {
            for col in 0..<cols {
                let index = row * cols + col
                guard index < cells.len() else { continue }
                let cell = cells[index]
                let x = CGFloat(col) * cellW
                let y = CGFloat(row) * cellH

                let bg = WolfColor.cgColor(cell.bg(), fallback: WolfColor.fallbackBg)
                ctx.setFillColor(bg)
                ctx.fill(CGRect(x: x, y: y, width: cellW, height: cellH))

                let text = cell.text().toString()
                if !text.isEmpty {
                    let fg = WolfColor.cgColor(cell.fg(), fallback: WolfColor.fallbackFg)
                    drawGlyph(ctx, text: text, x: x, y: y, fg: fg)
                }
            }
        }
    }

    /// Strokes a thin separator between adjacent panes (along each pane rect's
    /// interior right/bottom edge).
    private func drawDividers(_ rects: [(PaneId, CGRect)], ctx: CGContext) {
        let areaW = bounds.width
        let areaH = paneAreaHeight
        ctx.setStrokeColor(NSColor.separatorColor.cgColor)
        ctx.setLineWidth(1)
        for (_, rect) in rects {
            if rect.maxX < areaW - 0.5 {
                ctx.stroke(CGRect(x: rect.maxX - 0.5, y: rect.minY, width: 0, height: rect.height))
            }
            if rect.maxY < areaH - 0.5 {
                ctx.stroke(CGRect(x: rect.minX, y: rect.maxY - 0.5, width: rect.width, height: 0))
            }
        }
    }

    /// Draws one cell's glyph in `fg` using CoreText at the cell baseline.
    private func drawGlyph(_ ctx: CGContext, text: String, x: CGFloat, y: CGFloat, fg: CGColor) {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: metrics.font,
            .foregroundColor: fg,
        ]
        let line = CTLineCreateWithAttributedString(
            NSAttributedString(string: text, attributes: attrs))
        ctx.saveGState()
        // The view is flipped; flip the text matrix so glyphs are upright.
        ctx.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
        ctx.textPosition = CGPoint(x: x, y: y + metrics.baseline)
        CTLineDraw(line, ctx)
        ctx.restoreGState()
    }

    /// Overlays the mode-aware cursor (reverse-video colors resolved in Rust) at
    /// grid-local coordinates.
    private func drawCursor(
        _ ctx: CGContext, frame: FfiFrame, cells: RustVec<FfiCell>, cols: Int
    ) {
        let row = Int(frame.cursor_row())
        let col = Int(frame.cursor_col())
        let shape = CursorShape(rawValue: frame.cursor_shape()) ?? .block
        let rect = Cursor.rect(
            shape: shape,
            cellX: CGFloat(col) * metrics.cellWidth,
            cellY: CGFloat(row) * metrics.cellHeight,
            cellW: metrics.cellWidth,
            cellH: metrics.cellHeight,
            cellPercentage: frame.cursor_cell_percentage()
        )

        let fill = WolfColor.cgColor(frame.cursor_fill(), fallback: WolfColor.fallbackFg)
        ctx.setFillColor(fill)
        ctx.fill(rect)

        // Redraw the glyph under a block cursor in the readable (glyph) color.
        let index = row * cols + col
        if shape == .block, index >= 0, index < cells.len() {
            let text = cells[index].text().toString()
            if !text.isEmpty {
                let glyph = WolfColor.cgColor(frame.cursor_glyph(), fallback: WolfColor.fallbackBg)
                drawGlyph(
                    ctx, text: text,
                    x: CGFloat(col) * metrics.cellWidth,
                    y: CGFloat(row) * metrics.cellHeight,
                    fg: glyph)
            }
        }
    }

    // MARK: - Helpers

    /// Allocates the next pane id.
    private func allocPaneId() -> PaneId {
        let id = nextPaneId
        nextPaneId += 1
        return id
    }

    /// Returns the pane bound to window handle `win`, if any.
    private func paneForWin(_ win: Int64) -> PaneId? {
        panes.first(where: { $0.value.win == win })?.key
    }

    /// Sets the active pane (the one NOT dimmed) and requests a redraw.
    private func setActivePane(_ pane: PaneId?) {
        if activePane != pane {
            activePane = pane
            needsDisplay = true
        }
    }

    /// Picks the window to bind the first pane to: the active window if it is a
    /// real, visible window grid (`>= 2`), else the lowest visible window grid.
    private func pickInitialWindow() -> (win: Int64, grid: UInt64)? {
        let windows = handle.windows()
        let active = handle.active_win()
        if active != 0 {
            for w in windows
            where w.win() == active && w.grid() >= Self.minWindowGrid && !w.hidden() {
                return (active, w.grid())
            }
        }
        var best: (win: Int64, grid: UInt64)?
        for w in windows where w.grid() >= Self.minWindowGrid && !w.hidden() {
            if best == nil || w.grid() < best!.grid {
                best = (w.win(), w.grid())
            }
        }
        return best
    }
}
