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
    /// Maximum Neovim grid width, in cells. A pane wider than this many columns caps
    /// its grid at `maxGridCols` and CENTERS it horizontally, leaving side margins
    /// (painted in the grid's background); a narrower pane is unaffected. All pixel↔
    /// cell mapping routes through `PaneGeometry`, which owns the cap + centering.
    private static let maxGridCols = 90

    // -- Mouse / scroll FFI discriminants (mirror `wolf-ffi`'s `mod mouse_button`
    //    / `mod mouse_action` / `mod scroll_dir`) ------------------------------
    /// `mouse_button::LEFT`.
    private static let mouseButtonLeft: UInt8 = 0
    /// `mouse_action::PRESS`.
    private static let mouseActionPress: UInt8 = 0
    /// `mouse_action::RELEASE`.
    private static let mouseActionRelease: UInt8 = 1
    /// `mouse_action::DRAG`.
    private static let mouseActionDrag: UInt8 = 2
    /// `scroll_dir::UP` (decreases the viewport topline).
    private static let scrollDirUp: UInt8 = 0
    /// `scroll_dir::DOWN` (increases the viewport topline).
    private static let scrollDirDown: UInt8 = 1

    // -- Per-pane statusbar tuning --------------------------------------------
    /// Vertical padding above the first and below the last statusbar line (px).
    private static let statusbarVerticalPadding: Double = 3
    /// Floor for the secondary (path) statusbar font size (px), so a tiny grid
    /// font still leaves the path legible.
    private static let statusbarMinSecondaryFontSize: Double = 9
    /// The secondary (path) statusbar font size as a fraction of the primary size.
    private static let statusbarSecondaryFontScale: Double = 0.8
    /// The secondary (path) statusbar text alpha when derived from the themed
    /// titlebar foreground (else `secondaryLabelColor` is used directly).
    private static let statusbarSecondaryThemedAlpha: CGFloat = 0.7

    // -- Per-pane tab-strip tuning --------------------------------------------
    /// Fixed per-tab width (px); the last tab clamps to the remaining strip width.
    private static let tabWidth: Double = 180
    /// The close (X) glyph hit-square side (px), flush to each tab's trailing edge.
    private static let tabCloseSize: Double = 12
    /// Left inset of a tab's label text from the tab's leading edge (px).
    private static let tabTextInset: CGFloat = 8
    /// Extra right inset of a tab's label from the close glyph so text never runs
    /// under the X (px).
    private static let tabLabelRightInset: CGFloat = 4
    /// Fill alpha for the ACTIVE tab (a subtle highlight over the strip bg).
    private static let tabActiveFillAlpha: CGFloat = 0.16
    /// Fill alpha for a HOVERED (non-active) tab.
    private static let tabHoverFillAlpha: CGFloat = 0.10
    /// Text alpha for an INACTIVE (non-active) tab label.
    private static let tabInactiveTextAlpha: CGFloat = 0.6

    // -- Synthetic scrollbar appearance / fade tuning -------------------------
    /// Scrollbar track/thumb thickness (px).
    private static let scrollbarThickness: CGFloat = 8
    /// Inset of the scrollbar from the pane's right/top/bottom edges (px).
    private static let scrollbarInset: CGFloat = 2
    /// Minimum thumb length (px) so a tiny viewport fraction stays grabbable-looking.
    private static let scrollbarMinThumb: CGFloat = 24
    /// Track opacity (`.legacy` only).
    private static let scrollbarTrackAlpha: CGFloat = 0.12
    /// Thumb opacity at full visibility.
    private static let scrollbarThumbAlpha: CGFloat = 0.45
    /// How long an `.overlay` scrollbar stays fully visible after a scroll (s).
    private static let scrollbarOverlayFadeDelay: TimeInterval = 0.9
    /// How long an `.overlay` scrollbar takes to fade out after the delay (s).
    private static let scrollbarOverlayFadeDuration: TimeInterval = 0.4
    /// Repaint cadence while an `.overlay` scrollbar is fading (s).
    private static let scrollbarOverlayFadeTick: TimeInterval = 1.0 / 30.0

    private let handle: WolfCoreHandle
    /// The cell geometry driving all pixel↔cell mapping and painting. A `var` so a
    /// per-window font-size change can re-measure and reflow via `applyMetrics`.
    private var metrics: CellMetrics

    /// The pure pane-layout tree (split/remove/neighbour geometry).
    private let tree = PaneTree(root: PaneContainerView.firstPaneId)
    /// The per-pane bindings, keyed by `PaneId`.
    private var panes: [PaneId: PaneBinding] = [:]
    /// The next pane id to allocate (starts at `firstPaneId`).
    private var nextPaneId: PaneId = PaneContainerView.firstPaneId
    /// The currently active pane (cursor target + the one NOT dimmed), if any.
    private var activePane: PaneId?

    /// The grid a press/drag is targeting, set on `mouseDown` and cleared on
    /// `mouseUp`, so a drag keeps targeting the pane the press started in even when
    /// the pointer leaves that pane's rect.
    private var mouseDragGrid: UInt64?

    /// The tab currently under the pointer (its pane + tab index), or `nil` when
    /// no tab is hovered. Drives the hover fill + the X-on-hover treatment (the X
    /// is painted only on the hovered tab). Updated by `mouseMoved`/`mouseExited`.
    private var hoveredTab: (pane: PaneId, index: Int)?

    /// Accumulates fractional scroll deltas into discrete whole-cell line steps.
    private var scrollAccumulator = ScrollAccumulator()
    /// The grid the last scroll targeted; a change resets `scrollAccumulator`.
    private var lastScrollGrid: UInt64?

    /// The live "Show scroll bars" system setting (`.legacy` = always visible;
    /// `.overlay` = on-scroll then fade). Updated via the system notification.
    private var scrollerStyle: NSScroller.Style = NSScroller.preferredScrollerStyle
    /// When the most recent scroll happened, driving the `.overlay` fade.
    private var lastScrollTime: Date = .distantPast
    /// The repeating repaint timer that animates the `.overlay` fade; nil when idle.
    private var scrollbarFadeTimer: Timer?

    /// Invoked when the picker trigger chord (Command+O) is pressed, BEFORE any
    /// key is translated/forwarded to Neovim. Set by `AppDelegate` to open the
    /// picker.
    var onPickerTrigger: (() -> Void)?

    /// The last `WolfTitlebar` background applied to the window, so a render
    /// wake only touches AppKit when the theme actually changed (`nil` until the
    /// first wake reads it).
    private var lastTitlebarTheme: (rgb: UInt32, set: Bool)?

    /// Builds the container bound to a live core `handle` and measured `metrics`.
    /// No panes exist until the first window grid appears on a wake.
    init(handle: WolfCoreHandle, metrics: CellMetrics) {
        self.handle = handle
        self.metrics = metrics
        super.init(frame: .zero)
        wantsLayer = true
        // Follow the live "Show scroll bars" setting so the synthetic scrollbars
        // switch between always-on (`.legacy`) and fade-on-scroll (`.overlay`)
        // without a relaunch.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(scrollerStyleChanged),
            name: NSScroller.preferredScrollerStyleDidChangeNotification,
            object: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("PaneContainerView is not constructed from a nib")
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
        scrollbarFadeTimer?.invalidate()
    }

    /// Top-left origin so grid row/col map directly to (x, y) in points.
    override var isFlipped: Bool { true }

    /// The container is the single key sink for the editor area.
    override var acceptsFirstResponder: Bool { true }

    // MARK: - Geometry

    /// One-row cmdline-strip height (the bottom of the view).
    private var stripHeight: CGFloat { ceil(metrics.cellHeight) }

    /// The height of the pane area (the whole view minus the bottom cmdline strip).
    private var paneAreaHeight: CGFloat { max(0, bounds.height - stripHeight) }

    /// Maps a pixel size to a grid `(cols, rows)`, each at least 1.
    private func gridCells(width: CGFloat, height: CGFloat) -> (cols: Int, rows: Int) {
        (max(1, Int(width / metrics.cellWidth)), max(1, Int(height / metrics.cellHeight)))
    }

    /// The current pane rectangles from the pure layout, in this view's (flipped)
    /// coordinates, spanning the FULL bounds (origin `(0, 0)`): the pane area occupies
    /// the top `paneAreaHeight`, the cmdline strip the bottom `stripHeight`. The
    /// per-pane 20pt inset is NOT applied here — it is applied around each pane's grid
    /// in `gridRect(ofPane:)`, so these rects are the full pane rectangles.
    private func paneRects() -> [(PaneId, CGRect)] {
        tree.paneRects(width: Double(bounds.width), height: Double(paneAreaHeight))
            .map {
                ($0.0, CGRect(x: $0.1.x, y: $0.1.y, width: $0.1.w, height: $0.1.h))
            }
    }

    /// The per-pane GRID-RECT-inside-PANE-RECT geometry for `rect` (the pure seam
    /// that owns all pixel<->cell mapping). The grid width is capped at
    /// `maxGridCols` and CENTERED when the pane is wider than that; a narrower pane
    /// keeps `gridRect == rect`.
    private func geometry(for rect: CGRect) -> PaneGeometry {
        PaneGeometry(
            paneRect: Rect(
                x: Double(rect.minX), y: Double(rect.minY),
                w: Double(rect.width), h: Double(rect.height)),
            cellWidth: Double(metrics.cellWidth),
            cellHeight: Double(metrics.cellHeight),
            maxCols: Self.maxGridCols)
    }

    /// The pane rectangle currently bound to `grid`, if any (used to re-resolve a
    /// drag/scroll target by grid after the press).
    private func paneRect(forGrid grid: UInt64) -> CGRect? {
        for (id, rect) in paneRects() where panes[id]?.boundGrid == grid {
            return rect
        }
        return nil
    }

    /// The primary (filename) statusbar font size — the grid's base point size.
    private var statusbarPrimaryFontSize: Double { Double(CTFontGetSize(metrics.font)) }

    /// The secondary (path) statusbar font size — a fraction of the primary size,
    /// floored so it stays legible.
    private var statusbarSecondaryFontSize: Double {
        max(Self.statusbarMinSecondaryFontSize, statusbarPrimaryFontSize * Self.statusbarSecondaryFontScale)
    }

    /// The tab-row height (px): one primary-font line plus padding above and below.
    private var tabRowHeight: Double {
        (statusbarPrimaryFontSize * 1.25).rounded(.up) + 2 * Self.statusbarVerticalPadding
    }

    /// The secondary (active tab's path) line height (px): one secondary-font line
    /// plus padding beneath it.
    private var tabSecondaryLineHeight: Double {
        (statusbarSecondaryFontSize * 1.25).rounded(.up) + Self.statusbarVerticalPadding
    }

    /// The pure tab-strip geometry for a `width`-wide strip holding `tabCount`
    /// tabs (tab rects, per-tab close rects, secondary line, total height).
    private func tabStrip(width: Double, tabCount: Int) -> TabStripLayout {
        TabStripLayout(
            stripWidth: width, tabCount: tabCount, tabWidth: Self.tabWidth,
            tabHeight: tabRowHeight, closeSize: Self.tabCloseSize,
            secondaryLineHeight: tabSecondaryLineHeight)
    }

    /// The height (px) of the per-pane tab strip reserved at the top of every
    /// pane (tab row + secondary path line); the grid gets the remaining pane
    /// height below it. Independent of tab count / width.
    private var tabStripHeight: CGFloat { CGFloat(tabRowHeight + tabSecondaryLineHeight) }

    /// The tab-strip rectangle at the TOP of pane `rect` (full pane width,
    /// `tabStripHeight` tall).
    private func tabStripRect(ofPane rect: CGRect) -> CGRect {
        CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: tabStripHeight)
    }

    /// The GRID rectangle inside pane `rect`: the pane inset by `EditorInset.padding`
    /// on the left/right and bottom, and by `statusbarHeight + padding` on top (so the
    /// grid clears the top statusbar strip and the per-pane padding). All pixel<->cell
    /// mapping and grid painting route through this pure `WolfKeyMap.EditorInset` seam,
    /// so the grid never draws under the strip or into the padding.
    private func gridRect(ofPane rect: CGRect) -> CGRect {
        let inset = EditorInset(
            paneWidth: Double(rect.width), paneHeight: Double(rect.height),
            statusbarHeight: Double(tabStripHeight), padding: EditorInset.padding)
        return CGRect(
            x: rect.minX + inset.gridRect.x, y: rect.minY + inset.gridRect.y,
            width: inset.gridRect.w, height: inset.gridRect.h)
    }

    /// The number of live panes (`PaneTree` leaf count); 1 for a single unsplit
    /// window. Read by the window controller to disable the Close command on the
    /// last split.
    var paneCount: Int { tree.leaves().count }

    /// Re-measures against `newMetrics` (a per-window font-size change): swaps the
    /// cell geometry, re-syncs every grid's cell size to the new metrics, and
    /// repaints. The OS window frame is deliberately UNCHANGED — the grid reflows
    /// its cols/rows to fit.
    func applyMetrics(_ newMetrics: CellMetrics) {
        metrics = newMetrics
        syncGridSizes()
        needsDisplay = true
    }

    // MARK: - Input

    override func keyDown(with event: NSEvent) {
        let flags = event.modifierFlags

        // Picker trigger (Command+O) is intercepted BEFORE translation so it is
        // never forwarded to Neovim. While the picker is open its search field is
        // first responder, so this stops firing — suppression is automatic.
        if PickerSeam.isPickerTrigger(
            charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "",
            command: flags.contains(.command)
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

    /// Click-to-focus AND position the cursor: focus the window of the pane under
    /// the click, then send a LEFT/PRESS at the clicked grid cell (under nvim's
    /// `mouse=a` this positions the cursor; a following drag extends a selection).
    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        for (id, rect) in paneRects() where rect.contains(point) {
            guard let binding = panes[id] else { return }

            // Tab-strip clicks: a click in the top strip hit-tests the tabs. An X
            // closes that tab (win-explicit); a tab body focuses the pane and
            // switches it to that buffer. Neither forwards a grid mouse press.
            if let grid = binding.boundGrid,
                let hit = tabStripHit(grid: grid, paneRect: rect, point: point)
            {
                switch hit.hit {
                case .close(let i):
                    if let bufnr = hit.bufnr(at: i) {
                        handle.close_tab(binding.win, bufnr)
                    }
                case .tab(let i):
                    if let bufnr = hit.bufnr(at: i) {
                        handle.focus_win(binding.win)
                        setActivePane(id)
                        handle.switch_tab(binding.win, bufnr)
                    }
                }
                needsDisplay = true
                return
            }

            handle.focus_win(binding.win)
            setActivePane(id)
            needsDisplay = true
            if let grid = binding.boundGrid {
                let cell = geometry(for: gridRect(ofPane: rect)).cellAt(
                    x: Double(point.x), y: Double(point.y))
                handle.send_mouse(
                    mouseEvent(Self.mouseActionPress, grid: grid, cell: cell, event: event))
                mouseDragGrid = grid
            }
            return
        }
    }

    /// The result of a tab-strip hit test: the hit target plus the tab list it was
    /// resolved against (so the caller can read the hit tab's bufnr).
    private struct TabStripHit {
        let hit: TabStripLayout.Hit
        let tabs: RustVec<FfiTab>
        /// The buffer handle of the tab at index `i`, or `nil` if out of range.
        func bufnr(at i: Int) -> Int64? {
            guard i >= 0, i < Int(tabs.len()), let tab = tabs.get(index: UInt(i)) else {
                return nil
            }
            return tab.bufnr()
        }
    }

    /// Hit-tests the tab strip at the top of pane `paneRect` (bound to `grid`) for
    /// `point` (this view's coordinates), returning the tab/close hit or `nil` when
    /// the point is not on a tab. Only points inside the strip rect are considered.
    private func tabStripHit(grid: UInt64, paneRect: CGRect, point: CGPoint) -> TabStripHit? {
        let strip = tabStripRect(ofPane: paneRect)
        guard strip.contains(point) else { return nil }
        let tabs = handle.pane_tabs_for_grid(grid)
        let layout = tabStrip(width: Double(strip.width), tabCount: Int(tabs.len()))
        guard
            let hit = layout.hit(
                x: Double(point.x - strip.minX), y: Double(point.y - strip.minY))
        else { return nil }
        return TabStripHit(hit: hit, tabs: tabs)
    }

    /// Installs a single tracking area over the whole view so `mouseMoved` /
    /// `mouseExited` fire for the tab-strip hover treatment (X-on-hover).
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas {
            removeTrackingArea(area)
        }
        addTrackingArea(
            NSTrackingArea(
                rect: bounds,
                options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                owner: self, userInfo: nil))
    }

    /// Tracks the tab under the pointer so the tab strip can fill the hovered tab
    /// and draw its close X. Repaints only when the hovered tab actually changes.
    override func mouseMoved(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let next = hoveredTabAt(point)
        if hoveredTab?.pane != next?.pane || hoveredTab?.index != next?.index {
            hoveredTab = next
            needsDisplay = true
        }
    }

    /// Clears the hovered tab when the pointer leaves the view.
    override func mouseExited(with event: NSEvent) {
        if hoveredTab != nil {
            hoveredTab = nil
            needsDisplay = true
        }
    }

    /// The tab (pane id + tab index) under `point`, or `nil` when the point is not
    /// on a tab in any pane's strip.
    private func hoveredTabAt(_ point: CGPoint) -> (pane: PaneId, index: Int)? {
        for (id, rect) in paneRects() where rect.contains(point) {
            guard let grid = panes[id]?.boundGrid,
                let hit = tabStripHit(grid: grid, paneRect: rect, point: point)
            else { return nil }
            switch hit.hit {
            case .tab(let i), .close(let i):
                return (id, i)
            }
        }
        return nil
    }

    /// Extends the press into a drag (visual selection under `mouse=a`), keeping the
    /// target on the pane the press started in even when the pointer leaves it.
    override func mouseDragged(with event: NSEvent) {
        guard let grid = mouseDragGrid, let rect = paneRect(forGrid: grid) else { return }
        let point = convert(event.locationInWindow, from: nil)
        let cell = geometry(for: gridRect(ofPane: rect)).cellAt(x: Double(point.x), y: Double(point.y))
        handle.send_mouse(mouseEvent(Self.mouseActionDrag, grid: grid, cell: cell, event: event))
    }

    /// Ends the press/drag (RELEASE), then clears the drag target.
    override func mouseUp(with event: NSEvent) {
        guard let grid = mouseDragGrid else { return }
        if let rect = paneRect(forGrid: grid) {
            let point = convert(event.locationInWindow, from: nil)
            let cell = geometry(for: gridRect(ofPane: rect)).cellAt(
                x: Double(point.x), y: Double(point.y))
            handle.send_mouse(
                mouseEvent(Self.mouseActionRelease, grid: grid, cell: cell, event: event))
        }
        mouseDragGrid = nil
    }

    /// Forwards trackpad / wheel scrolling to the pane under the pointer.
    ///
    /// Hit-tests from the whole PANE rect (so scrolling in a future side margin
    /// still scrolls the pane), accumulates the fractional delta into whole-cell
    /// line steps, and emits that many discrete scroll events to Neovim.
    override func scrollWheel(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        var hit: (grid: UInt64, rect: CGRect)?
        for (id, rect) in paneRects() where rect.contains(point) {
            if let grid = panes[id]?.boundGrid {
                hit = (grid, rect)
            }
            break
        }
        guard let (grid, rect) = hit else { return }

        if grid != lastScrollGrid {
            scrollAccumulator.reset()
            lastScrollGrid = grid
        }

        let cellH = Double(metrics.cellHeight)
        // Precise (trackpad) deltas are already in points; coarse (mouse-wheel)
        // deltas are in lines, so scale them by the cell height to share the
        // whole-cell accumulator.
        let dy = event.hasPreciseScrollingDeltas
            ? Double(event.scrollingDeltaY)
            : Double(event.scrollingDeltaY) * cellH
        let steps = scrollAccumulator.consume(deltaY: dy, cellHeight: cellH)
        guard steps != 0 else { return }

        // DIRECTION — the SINGLE flip point. `scrollingDeltaY` ALREADY reflects the
        // user's "natural scroll" preference (the system flips its sign for us, just
        // like `NSScrollView` sees), so we use its sign directly. Do NOT additionally
        // consult `isDirectionInvertedFromDevice` — that double-corrects and inverts
        // scrolling for natural-scroll users. A positive delta shows earlier content,
        // i.e. the viewport moves UP. If scrolling ever feels inverted, flip this line.
        let direction = steps > 0 ? Self.scrollDirUp : Self.scrollDirDown

        let cell = geometry(for: gridRect(ofPane: rect)).cellAt(x: Double(point.x), y: Double(point.y))
        for _ in 0..<abs(steps) {
            handle.send_scroll(
                FfiScrollEvent(
                    direction: direction, grid: grid,
                    row: UInt32(cell.row), col: UInt32(cell.col)))
        }
        scrollbarActivity()
    }

    /// Builds a LEFT-button `FfiMouseEvent` for `action` at grid cell `cell`,
    /// carrying the event's ctrl/shift/alt modifiers (no meta — nvim has no meta
    /// mouse modifier).
    private func mouseEvent(
        _ action: UInt8, grid: UInt64, cell: (row: Int, col: Int), event: NSEvent
    ) -> FfiMouseEvent {
        let flags = event.modifierFlags
        return FfiMouseEvent(
            button: Self.mouseButtonLeft,
            action: action,
            grid: grid,
            row: UInt32(cell.row),
            col: UInt32(cell.col),
            ctrl: flags.contains(.control),
            shift: flags.contains(.shift),
            alt: flags.contains(.option))
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

        // Grid 1 (the outer/global grid) covers the whole view (cmdline + messages);
        // the per-pane padding is applied around each pane's grid, not here.
        let global = gridCells(width: bounds.width, height: bounds.height)
        let outer = handle.read_frame_for_grid(Self.cmdlineGridId)
        if Int(outer.cols()) != global.cols || Int(outer.rows()) != global.rows {
            handle.resize(
                UInt16(truncatingIfNeeded: global.cols),
                UInt16(truncatingIfNeeded: global.rows))
        }

        for (id, rect) in paneRects() {
            guard let grid = panes[id]?.boundGrid else { continue }
            // Size from the pane's GRID RECT (the pane minus the top statusbar
            // strip) via `PaneGeometry`, so nvim sees the REDUCED rows the grid
            // actually paints into.
            let geo = geometry(for: gridRect(ofPane: rect))
            let frame = handle.read_frame_for_grid(grid)
            if Int(frame.cols()) != geo.cols || Int(frame.rows()) != geo.rows {
                handle.resize_grid(
                    grid,
                    UInt16(truncatingIfNeeded: geo.cols),
                    UInt16(truncatingIfNeeded: geo.rows))
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
        applyTitlebarTheme()
        needsDisplay = true
    }

    /// Applies the `WolfTitlebar` theme background to the window when it
    /// changed since the last wake (cached in `lastTitlebarTheme`, so there is
    /// no per-wake AppKit churn). The cursor needs no equivalent here: its
    /// themed colors arrive pre-resolved via `cursor_fill()`/`cursor_glyph()`.
    private func applyTitlebarTheme() {
        guard let win = window as? MainWindow else { return }
        let bg = handle.theme_titlebar_bg()
        let current = (rgb: bg.rgb, set: bg.set)
        if let last = lastTitlebarTheme, last == current { return }
        lastTitlebarTheme = current
        win.applyTitlebarBackground(rgb: current.rgb, set: current.set)
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
        // Neighbour geometry must match `paneRects()`, which lays out over the full
        // view width and the pane area above the cmdline strip.
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

        // Render glyphs with plain grayscale antialiasing and NO font smoothing
        // (stem darkening). macOS font smoothing thickens light-on-dark text, so a
        // regular weight reads as bold; disabling it keeps the weight accurate and
        // matches iA Writer / VS Code / Ghostty on dark backgrounds.
        ctx.setShouldAntialias(true)
        ctx.setShouldSmoothFonts(false)

        // Editor background (the per-pane padding regions around each grid + any
        // not-yet-tiled gaps). Use the live grid default background (grid 1) so the
        // padding matches the text area exactly, rather than the static fallback
        // (which reads darker under a themed background).
        let editorBg = WolfColor.cgColor(
            handle.read_frame_for_grid(Self.cmdlineGridId).default_bg(),
            fallback: WolfColor.fallbackBg)
        ctx.setFillColor(editorBg)
        ctx.fill(bounds)

        let rects = paneRects()
        for (id, rect) in rects {
            guard let grid = panes[id]?.boundGrid else { continue }
            paintPane(pane: id, grid: grid, paneRect: rect, dimmed: id != activePane, ctx: ctx)
        }
        drawDividers(rects, ctx: ctx)
        paintCmdlineStrip(ctx)
        drawScrollbars(rects, ctx: ctx)
    }

    /// Paints one whole pane: the top tab strip, then the window grid below it,
    /// then (for an inactive pane) an inactive-dim scrim over the WHOLE pane (tab
    /// strip + grid + any side margins), so the strip dims with its pane.
    private func paintPane(pane: PaneId, grid: UInt64, paneRect: CGRect, dimmed: Bool, ctx: CGContext) {
        ctx.saveGState()
        ctx.clip(to: paneRect)

        paintTabStrip(
            pane: pane, into: tabStripRect(ofPane: paneRect), grid: grid, paneRect: paneRect,
            ctx: ctx)

        let frame = handle.read_frame_for_grid(grid)
        paintGrid(frame, into: gridRect(ofPane: paneRect), ctx: ctx)

        if dimmed {
            ctx.setFillColor(CGColor(gray: 0, alpha: Self.inactiveDim))
            ctx.fill(paneRect)
        }
        ctx.restoreGState()
    }

    /// Paints one window grid into the GRID rect `rect` (the pane minus its top
    /// statusbar strip).
    ///
    /// When the grid is capped narrower than the rect (`maxGridCols`) it is CENTERED
    /// via the geometry's paint offset; the whole grid rect is first filled with the
    /// grid's default background so the side margins blend with the grid. The
    /// inactive-dim scrim is applied by `paintPane` over the whole pane, not here.
    private func paintGrid(_ frame: FfiFrame, into rect: CGRect, ctx: CGContext) {
        let geo = geometry(for: rect)
        ctx.saveGState()
        ctx.clip(to: rect)

        // Fill the whole grid rect (including the side margins around a centered
        // grid) with the grid's default background, so the margins are seamless.
        ctx.setFillColor(WolfColor.cgColor(frame.default_bg(), fallback: WolfColor.fallbackBg))
        ctx.fill(rect)

        // Paint the (possibly centered) grid at its paint offset.
        ctx.saveGState()
        ctx.translateBy(
            x: rect.minX + CGFloat(geo.paintOffsetX),
            y: rect.minY + CGFloat(geo.paintOffsetY))
        paintCells(frame, ctx: ctx)
        if frame.cursor_visible() {
            drawCursor(ctx, frame: frame, cells: frame.cells(), cols: Int(frame.cols()))
        }
        ctx.restoreGState()
        ctx.restoreGState()
    }

    /// Paints the per-pane tab strip: a titlebar-matched background, one tab per
    /// buffer the pane has shown (active highlighted, inactive dimmed, the hovered
    /// tab given a fill and its close X), and — beneath the tab row — the active
    /// tab's display path as a secondary line.
    ///
    /// The background matches the macOS titlebar: the window's own
    /// `backgroundColor` (kept in sync with the `WolfTitlebar` theme by
    /// `MainWindow.applyTitlebarBackground`), falling back to the themed titlebar
    /// bg / editor fallback when there is no window. Tab geometry comes from the
    /// pure `WolfKeyMap.TabStripLayout`; the secondary path comes from
    /// `pane_status_for_grid`. The X is drawn only on the hovered tab.
    private func paintTabStrip(
        pane: PaneId, into strip: CGRect, grid: UInt64, paneRect: CGRect, ctx: CGContext
    ) {
        ctx.saveGState()
        ctx.clip(to: strip)

        // Background: the window's (themed) titlebar background when available,
        // else the themed titlebar bg, else the editor fallback.
        let bgColor: CGColor
        if let mainWindow = window as? MainWindow {
            bgColor = mainWindow.backgroundColor.cgColor
        } else {
            bgColor = WolfColor.cgColor(handle.theme_titlebar_bg(), fallback: WolfColor.fallbackBg)
        }
        ctx.setFillColor(bgColor)
        ctx.fill(strip)

        // Colors: the themed titlebar fg when set (dimmed for inactive tabs / the
        // secondary line), else AppKit's label / secondary-label system colors.
        let titlebarFg = handle.theme_titlebar_fg()
        let baseColor: NSColor
        let secondaryColor: NSColor
        if titlebarFg.set {
            let themed = NSColor(cgColor: WolfColor.cgColor(packed: titlebarFg.rgb)) ?? .labelColor
            baseColor = themed
            secondaryColor = themed.withAlphaComponent(Self.statusbarSecondaryThemedAlpha)
        } else {
            baseColor = .labelColor
            secondaryColor = .secondaryLabelColor
        }

        let tabs = handle.pane_tabs_for_grid(grid)
        let count = Int(tabs.len())
        let layout = tabStrip(width: Double(strip.width), tabCount: count)
        let tabFont = NSFont.systemFont(ofSize: CGFloat(statusbarPrimaryFontSize))

        var activePath = ""
        for i in 0..<count {
            guard let tab = tabs.get(index: UInt(i)), i < layout.tabRects.count else { continue }
            let tabRect = paneLocalRect(layout.tabRects[i], in: strip)
            guard tabRect.width > 0 else { continue }
            let isActive = tab.active()
            let isHovered = hoveredTab?.pane == pane && hoveredTab?.index == i

            // Active highlight / hover fill.
            if isActive {
                ctx.setFillColor(CGColor(gray: 1, alpha: Self.tabActiveFillAlpha))
                ctx.fill(tabRect)
            } else if isHovered {
                ctx.setFillColor(CGColor(gray: 1, alpha: Self.tabHoverFillAlpha))
                ctx.fill(tabRect)
            }

            // Label text (inactive tabs dimmed), tail-truncated, clearing the X.
            let label = tab.display().toString() + (tab.modified() ? " •" : "")
            let color = isActive ? baseColor : baseColor.withAlphaComponent(Self.tabInactiveTextAlpha)
            let textLeft = tabRect.minX + Self.tabTextInset
            let textRight = tabRect.maxX - CGFloat(Self.tabCloseSize) - Self.tabLabelRightInset
            let textRect = CGRect(
                x: textLeft, y: tabRect.minY,
                width: max(0, textRight - textLeft), height: tabRect.height)
            drawStatusbarText(label, in: textRect, font: tabFont, color: color)

            // The close X is drawn ONLY on the hovered tab.
            if isHovered, i < layout.closeRects.count {
                drawCloseGlyph(paneLocalRect(layout.closeRects[i], in: strip), color: color, ctx: ctx)
            }

            if isActive {
                activePath = handle.pane_status_for_grid(grid).display_path().toString()
            }
        }

        // Secondary line: the active tab's display path, aligned with the tab text.
        if !activePath.isEmpty {
            let secRect = paneLocalRect(layout.secondaryLineRect, in: strip)
            let textRect = CGRect(
                x: secRect.minX + Self.tabTextInset, y: secRect.minY,
                width: max(0, secRect.width - Self.tabTextInset), height: secRect.height)
            drawStatusbarText(
                activePath, in: textRect,
                font: .systemFont(ofSize: CGFloat(statusbarSecondaryFontSize)), color: secondaryColor)
        }

        ctx.restoreGState()
    }

    /// Maps a strip-local `WolfKeyMap.Rect` into this view's coordinates by
    /// offsetting it to the strip's origin.
    private func paneLocalRect(_ r: Rect, in strip: CGRect) -> CGRect {
        CGRect(x: strip.minX + CGFloat(r.x), y: strip.minY + CGFloat(r.y), width: CGFloat(r.w), height: CGFloat(r.h))
    }

    /// Draws a small close (X) glyph centered in `rect` in `color`.
    private func drawCloseGlyph(_ rect: CGRect, color: NSColor, ctx: CGContext) {
        let inset: CGFloat = 3
        let x0 = rect.minX + inset
        let x1 = rect.maxX - inset
        let y0 = rect.minY + inset
        let y1 = rect.maxY - inset
        ctx.saveGState()
        ctx.setStrokeColor(color.cgColor)
        ctx.setLineWidth(1.5)
        ctx.setLineCap(.round)
        ctx.move(to: CGPoint(x: x0, y: y0))
        ctx.addLine(to: CGPoint(x: x1, y: y1))
        ctx.move(to: CGPoint(x: x1, y: y0))
        ctx.addLine(to: CGPoint(x: x0, y: y1))
        ctx.strokePath()
        ctx.restoreGState()
    }

    /// Draws one line of statusbar `text` clipped to `rect`, tail-truncated in
    /// `font`/`color` (the view is flipped, so the top-left NSString draw is
    /// upright).
    private func drawStatusbarText(_ text: String, in rect: CGRect, font: NSFont, color: NSColor) {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byTruncatingTail
        let attrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: color,
            .paragraphStyle: paragraph,
        ]
        (text as NSString).draw(in: rect, withAttributes: attrs)
    }

    /// Paints the command line / messages into the bottom one-row strip.
    ///
    /// Under `ext_multigrid` Neovim renders the command line and messages onto a
    /// dedicated MESSAGE grid (NOT grid 1), positioned at the bottom of the screen
    /// via `msg_set_pos` (which the core drops). When the command line is open the
    /// cursor moves onto that grid, so the message grid is identified as "the grid
    /// the cursor is on that is neither grid 1 nor a window/pane grid"; its cursor
    /// row (where the `:`/`/` line lives) is painted into the strip. Otherwise the
    /// strip paints grid 1's LAST row (persistent messages), as before.
    private func paintCmdlineStrip(_ ctx: CGContext) {
        // The cmdline strip spans the full view width along the bottom (below the
        // pane area); its text is horizontally indented by the per-pane padding in
        // `paintStripRow` so it aligns with the grid text.
        let stripRect = CGRect(
            x: 0, y: paneAreaHeight, width: bounds.width, height: stripHeight)
        let cursorGrid = handle.read_frame().cursor_grid()
        let windowGrids = Set(handle.windows().map { $0.grid() })

        if cursorGrid != Self.cmdlineGridId, !windowGrids.contains(cursorGrid) {
            // Command-line / message grid: paint the cursor's row (the active line).
            let frame = handle.read_frame_for_grid(cursorGrid)
            guard frame.rows() > 0 else { return }
            paintStripRow(frame, row: Int(frame.cursor_row()), stripRect: stripRect, ctx: ctx)
        } else {
            // Grid 1's LAST row (persistent messages while not in cmdline mode).
            let frame = handle.read_frame_for_grid(Self.cmdlineGridId)
            let rows = Int(frame.rows())
            guard rows > 0 else { return }
            paintStripRow(frame, row: rows - 1, stripRect: stripRect, ctx: ctx)
        }
    }

    /// Paints a single `row` of `frame` into `stripRect` (translated so that row
    /// lands at the strip's top; the rest paints above and is clipped away), plus
    /// the cursor when it lives on this grid.
    private func paintStripRow(
        _ frame: FfiFrame, row: Int, stripRect: CGRect, ctx: CGContext
    ) {
        ctx.saveGState()
        ctx.clip(to: stripRect)
        // Translate by the per-pane padding (the strip spans the full width from
        // x = 0), so the cmdline row + cursor start at x = padding and align with the
        // grid text.
        ctx.translateBy(
            x: CGFloat(EditorInset.padding),
            y: stripRect.minY - CGFloat(row) * metrics.cellHeight)
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
                    let style = GlyphStyleResolver.resolve(
                        flags: cell.flags(), fg: cell.fg(), special: cell.special())
                    drawGlyph(ctx, text: text, x: x, y: y, fg: fg, style: style)
                }
            }
        }
    }

    /// Strokes a thin separator between adjacent panes (along each pane rect's
    /// interior right/bottom edge).
    private func drawDividers(_ rects: [(PaneId, CGRect)], ctx: CGContext) {
        // Compare against the pane-area edges (the full view width, and the pane area
        // above the cmdline strip), so the outermost panes' edges aren't mistaken for
        // interior dividers.
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

    // MARK: - Synthetic scrollbars

    /// Draws a synthetic, non-interactive scrollbar at each pane's right edge,
    /// driven by that grid's `win_viewport` (topline/botline/linecount).
    ///
    /// Custom-drawn (not `NSScroller` subviews) so it composites cleanly into the
    /// single flat painting view and sits at the PANE's right edge (out in the future
    /// side margin once the grid is centered). It follows the system "Show scroll
    /// bars" setting: `.legacy` is always visible (with a faint track); `.overlay`
    /// fades in on scroll and out after a delay. Thumb-drag is deferred (v1 is
    /// position-only).
    private func drawScrollbars(_ rects: [(PaneId, CGRect)], ctx: CGContext) {
        let alpha = currentScrollbarAlpha()
        guard alpha > 0 else { return }

        for (id, rect) in rects {
            guard let grid = panes[id]?.boundGrid else { continue }
            let frame = handle.read_frame_for_grid(grid)
            let linecount = frame.linecount()
            guard linecount > 0 else { continue }

            // `win_viewport` reports `topline`/`botline` as 0-based buffer line
            // indices (botline is one-past-last-visible). Convert the UInt64
            // viewport values to Double BEFORE any subtraction so a
            // `botline < topline` or `topline == 0` case can never wrap a UInt64.
            // Skip when nothing is visible or the whole buffer fits (no
            // scrollbar needed).
            let total = Double(linecount)
            let topline = Double(frame.topline())
            let botline = Double(frame.botline())
            let visible = max(0, botline - topline)
            guard visible > 0, visible < total else { continue }

            // The scrollbar track spans the GRID rect (the pane minus its top
            // statusbar strip), so the thumb never rides up over the strip.
            let gRect = gridRect(ofPane: rect)
            let inset = Self.scrollbarInset
            let thickness = Self.scrollbarThickness
            let track = CGRect(
                x: gRect.maxX - thickness - inset,
                y: gRect.minY + inset,
                width: thickness,
                height: gRect.height - 2 * inset)
            guard track.height > 0 else { continue }

            // 0-based: the thumb's top sits at `topline / total` of the track (no
            // `- 1`), which is 0 at the top of every buffer.
            let frac = min(1.0, visible / total)
            let thumbH = max(Self.scrollbarMinThumb, track.height * CGFloat(frac))
            let offsetFrac = topline / total
            let maxThumbY = track.maxY - thumbH
            let rawThumbY = track.minY + CGFloat(offsetFrac) * track.height
            let thumbY = min(max(rawThumbY, track.minY), max(track.minY, maxThumbY))

            if scrollerStyle == .legacy {
                ctx.setFillColor(CGColor(gray: 0.5, alpha: Self.scrollbarTrackAlpha * alpha))
                fillRounded(track, radius: thickness / 2, ctx: ctx)
            }
            let thumb = CGRect(x: track.minX, y: thumbY, width: thickness, height: thumbH)
            ctx.setFillColor(CGColor(gray: 0.5, alpha: Self.scrollbarThumbAlpha * alpha))
            fillRounded(thumb, radius: thickness / 2, ctx: ctx)
        }
    }

    /// Fills `rect` as a rounded rectangle of the given corner `radius`.
    private func fillRounded(_ rect: CGRect, radius: CGFloat, ctx: CGContext) {
        let path = CGPath(
            roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
        ctx.addPath(path)
        ctx.fillPath()
    }

    /// The current scrollbar opacity: 1 always for `.legacy`; for `.overlay`, 1
    /// while within the fade delay after the last scroll, then ramping to 0 over the
    /// fade duration.
    private func currentScrollbarAlpha() -> CGFloat {
        guard scrollerStyle == .overlay else { return 1.0 }
        let elapsed = Date().timeIntervalSince(lastScrollTime)
        if elapsed < Self.scrollbarOverlayFadeDelay {
            return 1.0
        }
        let fade = (elapsed - Self.scrollbarOverlayFadeDelay) / Self.scrollbarOverlayFadeDuration
        return CGFloat(max(0.0, 1.0 - fade))
    }

    /// Marks scroll activity (so the `.overlay` scrollbar shows) and arms the fade
    /// tick timer; a no-op repaint trigger for `.legacy` (always visible).
    private func scrollbarActivity() {
        lastScrollTime = Date()
        needsDisplay = true
        guard scrollerStyle == .overlay, scrollbarFadeTimer == nil else { return }
        scrollbarFadeTimer = Timer.scheduledTimer(
            withTimeInterval: Self.scrollbarOverlayFadeTick, repeats: true
        ) { [weak self] _ in
            self?.scrollbarFadeTick()
        }
    }

    /// One `.overlay` fade tick: repaint, and stop the timer once fully faded.
    private func scrollbarFadeTick() {
        needsDisplay = true
        if currentScrollbarAlpha() <= 0 {
            scrollbarFadeTimer?.invalidate()
            scrollbarFadeTimer = nil
        }
    }

    /// Reacts to a live "Show scroll bars" system setting change.
    @objc private func scrollerStyleChanged() {
        scrollerStyle = NSScroller.preferredScrollerStyle
        needsDisplay = true
    }

    /// A plain regular glyph style (no face variant, no decorations) used for the
    /// glyph redrawn under a block cursor — avoids a bold face overflowing the
    /// reverse-video cell.
    private static let cursorGlyphStyle = GlyphStyle(
        face: .regular, underline: false, underlineColor: FfiColor(rgb: 0, set: false),
        strikethrough: false, strikethroughColor: FfiColor(rgb: 0, set: false))

    /// Underline stroke offset below the baseline, in points.
    private static let underlineOffset: CGFloat = 2
    /// Underline / strikethrough stroke thickness, in points.
    private static let decorationThickness: CGFloat = 1
    /// Strikethrough height above the baseline as a fraction of the cell height
    /// (roughly the x-height midline).
    private static let strikethroughRatio: CGFloat = 0.3

    /// Draws one cell's glyph in `fg` using CoreText at the cell baseline, in the
    /// face `style.face` selects, and strokes any underline / strikethrough
    /// decorations MANUALLY (in `style`'s resolved colors) — `CTLineDraw` paints
    /// decorations only in the foreground color and ignores AppKit decoration-color
    /// attributes, so the special (underline) color must be stroked by hand.
    private func drawGlyph(
        _ ctx: CGContext, text: String, x: CGFloat, y: CGFloat, fg: CGColor,
        style: GlyphStyle
    ) {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: metrics.font(for: style.face),
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

        drawDecorations(ctx, x: x, y: y, style: style)
    }

    /// Strokes the underline / strikethrough for one cell as horizontal lines in
    /// the resolved decoration colors (drawn separately from the glyph because
    /// CoreText ignores decoration-color attributes).
    private func drawDecorations(
        _ ctx: CGContext, x: CGFloat, y: CGFloat, style: GlyphStyle
    ) {
        guard style.underline || style.strikethrough else { return }
        let width = metrics.cellWidth
        ctx.saveGState()
        ctx.setLineWidth(Self.decorationThickness)
        if style.underline {
            let uy = y + metrics.baseline + Self.underlineOffset
            ctx.setStrokeColor(
                WolfColor.cgColor(style.underlineColor, fallback: WolfColor.fallbackFg))
            ctx.move(to: CGPoint(x: x, y: uy))
            ctx.addLine(to: CGPoint(x: x + width, y: uy))
            ctx.strokePath()
        }
        if style.strikethrough {
            let sy = y + metrics.cellHeight * Self.strikethroughRatio
            ctx.setStrokeColor(
                WolfColor.cgColor(style.strikethroughColor, fallback: WolfColor.fallbackFg))
            ctx.move(to: CGPoint(x: x, y: sy))
            ctx.addLine(to: CGPoint(x: x + width, y: sy))
            ctx.strokePath()
        }
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
                    fg: glyph,
                    style: Self.cursorGlyphStyle)
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
