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
import WebKit
import WolfFFI
import WolfKeyMap

/// One pane's binding: the Neovim window it follows and the grid it paints
/// (`nil` until resolved via `grid_for_win`).
private struct PaneBinding {
    let win: Int64
    var boundGrid: UInt64?
}

/// The scalar inputs `PreviewSeam.placementMode` reads to choose a preview
/// placement, gathered from a `PaneContainerView`'s live pane/preview state.
/// Internal (not `private`) so `WolfWindowController.openPreview()` — in a sibling
/// file of the same module — can feed it through the pure seam.
struct PreviewPlacementInputs {
    let paneCount: Int
    let activePaneIsPreview: Bool
    let otherPreviewPaneCount: Int
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
    /// Gap between the primary (filename) and secondary (path) statusbar lines (px).
    private static let statusbarLineGap: Double = 1
    /// Floor for the secondary (path) statusbar font size (px), so a tiny grid
    /// font still leaves the path legible.
    private static let statusbarMinSecondaryFontSize: Double = 9
    /// The secondary (path) statusbar font size as a fraction of the primary size.
    private static let statusbarSecondaryFontScale: Double = 0.8
    /// The secondary (path) statusbar text alpha when derived from the themed
    /// titlebar foreground (else `secondaryLabelColor` is used directly).
    private static let statusbarSecondaryThemedAlpha: CGFloat = 0.7

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

    /// This view's bound nvim TABPAGE handle (the project/tabpage-per-view bind).
    /// Under the shared-nvim model a wake fires per PROJECT, but this container
    /// must only consume a refocus rebind once the snapshot reflects THIS view's
    /// tabpage — the `handle.active_tabpage() == controllerTabpageHandle` gate.
    private let controllerTabpageHandle: Int64

    /// Set by the window controller (`windowDidBecomeKey`) after it drives its
    /// project's `set_current_tabpage`: the next wake whose snapshot reflects this
    /// view's tabpage re-derives every pane's grid binding (nvim churns grid ids
    /// across a tabpage switch while WINDOW handles stay stable — OD2), then clears
    /// the flag. See `onRenderWake`.
    private var pendingRefocusRebind = false

    /// The pure pane-layout tree (split/remove/neighbour geometry).
    private let tree = PaneTree(root: PaneContainerView.firstPaneId)
    /// The per-pane bindings, keyed by `PaneId`.
    private var panes: [PaneId: PaneBinding] = [:]
    /// The next pane id to allocate (starts at `firstPaneId`).
    private var nextPaneId: PaneId = PaneContainerView.firstPaneId
    /// The currently active pane (cursor target + the one NOT dimmed), if any.
    private var activePane: PaneId?

    // -- Markdown-preview web-view hosting ------------------------------------
    // A preview pane is a SECOND pane content kind (requirement 2): the blank
    // scratch grid paints underneath (occluded, cheap) while a real `WKWebView`
    // subview shows core-rendered HTML over the pane's grid rect. These maps are
    // keyed by the preview WINDOW handle (`FfiPreview.win()`), the stable identity
    // that survives grid-id churn.
    /// The hosted `WKWebView`s, one per preview window.
    private var previewWebViews: [Int64: WKWebView] = [:]
    /// The inactive-dim scrim over each preview web view (a preview pane can't be
    /// dimmed by `paintPane`'s CGContext fill, since the web view is a real subview
    /// painted on top), added only while the pane is not active.
    private var previewDimOverlays: [Int64: NSView] = [:]
    /// The `(sourceBufnr, sourceTick)` each preview window's web view was last loaded
    /// at, so a reload (`render_preview` + `loadHTMLString`) fires when the window's
    /// bound source BUFFER changed (a reused preview pane switching sources, even at
    /// an equal tick) OR its tick advanced (or on first bind) — the OD6
    /// live-on-change gate, plus the reused-preview-pane fix.
    private var lastPreviewLoads: [Int64: PreviewSeam.PreviewLoad] = [:]
    /// The `WKNavigationDelegate` for each preview web view — retained here because a
    /// web view holds its `navigationDelegate` WEAKLY. Each routes a clicked link
    /// (`.linkActivated`) through `PreviewLinkSeam.classify` back into this container
    /// (held weakly by the delegate — no retain cycle).
    private var previewNavDelegates: [Int64: PreviewNavigationDelegate] = [:]
    /// The click-to-focus script-message handler for each preview web view —
    /// retained here because `WKUserContentController` holds its handlers STRONGLY
    /// (so the proxy holds the container only WEAKLY, breaking the retain cycle).
    /// A click anywhere in the preview's `WKWebView` (which swallows AppKit mouse
    /// events, so the container's `mouseDown` never fires there) posts through the
    /// injected `PreviewSeam.clickFocusJS` user script to this proxy, which calls
    /// `focusPreviewPane(win:)`.
    private var previewFocusHandlers: [Int64: PreviewClickFocusHandler] = [:]
    /// The absolute source path currently previewed in each preview window, captured
    /// from `FfiPreview.source_path()` in `reloadDirtyPreviews`. Used as the
    /// `loadHTMLString` `baseURL` (so relative links resolve) and as the same-file
    /// comparison basis in `PreviewLinkSeam.classify`.
    private var previewSourcePaths: [Int64: String] = [:]
    /// A fragment id awaiting the next `didFinish` load for each preview window: set
    /// when `previewFile` repoints a pane at a cross-file `other.md#anchor` link, then
    /// scrolled-to (and cleared) once that file's HTML has loaded.
    private var pendingPreviewFragments: [Int64: String] = [:]
    /// The source basename of each preview pane, keyed by the pane's bound GRID,
    /// rebuilt every wake by `syncPreviewWebViews`. Consulted by `paintStatusbar`
    /// (to show `Preview: <name>`) and `drawScrollbars` (to suppress the synthetic
    /// scrollbar over a web-view pane).
    private var previewNamesByGrid: [UInt64: String] = [:]
    /// The cwd/home-relative source display path of each preview pane, keyed by the
    /// pane's bound GRID, rebuilt every wake by `syncPreviewWebViews` from
    /// `FfiPreview.source_display_path()`. Consulted by `paintStatusbar` to show the
    /// path on the preview pane's secondary statusbar line (mirroring editor panes).
    private var previewDisplayPathsByGrid: [UInt64: String] = [:]
    /// Debounces the BLOCKING preview reload (`render_preview` + `loadHTMLString`)
    /// so a fast-typing burst — which fires many render wakes, each advancing the
    /// source `changedtick` — yields ONE trailing main-thread `render_preview`
    /// round-trip per ~30fps window rather than one per wake (OD6 / OD12). The
    /// per-window load gate (`lastPreviewLoads`, keyed by win → last-loaded
    /// `(sourceBufnr, sourceTick)`) stays the redundant-reload guard (same source
    /// buffer AND tick → still no reload); the coalescer bounds the RATE of the
    /// actual RPC + reload.
    private lazy var previewReloadCoalescer = PickerRefreshCoalescer(
        schedule: { work in
            DispatchQueue.main.asyncAfter(
                deadline: .now() + PickerRefreshCoalescer.refreshInterval, execute: work)
        },
        perform: { [weak self] in self?.reloadDirtyPreviews() })

    /// The grid a press/drag is targeting, set on `mouseDown` and cleared on
    /// `mouseUp`, so a drag keeps targeting the pane the press started in even when
    /// the pointer leaves that pane's rect.
    private var mouseDragGrid: UInt64?

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

    /// Builds the container bound to a live core `handle`, measured `metrics`, and
    /// the view's `tabpageHandle` (the project/tabpage-per-view bind used to gate
    /// the refocus rebind). No panes exist until the first window grid appears on a
    /// wake.
    init(handle: WolfCoreHandle, metrics: CellMetrics, tabpageHandle: Int64) {
        self.handle = handle
        self.metrics = metrics
        self.controllerTabpageHandle = tabpageHandle
        super.init(frame: .zero)
        // Layer-back so a VISIBLE non-key window (two side-by-side windows of the
        // SAME project) keeps its last-painted layer while it is not the current
        // view (the project drives wakes only to `currentView`).
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

    /// The pure vertical layout of the statusbar strip (line tops/heights + total
    /// height), from the current metrics-derived font sizes.
    private var statusbarLayout: StatusbarLayout {
        StatusbarLayout(
            primaryFontSize: statusbarPrimaryFontSize,
            secondaryFontSize: statusbarSecondaryFontSize,
            verticalPadding: Self.statusbarVerticalPadding,
            lineGap: Self.statusbarLineGap)
    }

    /// The height (px) of the per-pane statusbar strip reserved at the top of every
    /// pane; the grid gets the remaining pane height below it.
    private var statusbarHeight: CGFloat { CGFloat(statusbarLayout.height) }

    /// The statusbar strip rectangle at the TOP of pane `rect` (full pane width,
    /// `statusbarHeight` tall).
    private func statusbarRect(ofPane rect: CGRect) -> CGRect {
        CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: statusbarHeight)
    }

    /// The GRID rectangle inside pane `rect`: the pane inset by `EditorInset.padding`
    /// on the left/right and bottom, and by `statusbarHeight + padding` on top (so the
    /// grid clears the top statusbar strip and the per-pane padding). All pixel<->cell
    /// mapping and grid painting route through this pure `WolfKeyMap.EditorInset` seam,
    /// so the grid never draws under the strip or into the padding.
    private func gridRect(ofPane rect: CGRect) -> CGRect {
        let inset = EditorInset(
            paneWidth: Double(rect.width), paneHeight: Double(rect.height),
            statusbarHeight: Double(statusbarHeight), padding: EditorInset.padding)
        return CGRect(
            x: rect.minX + inset.gridRect.x, y: rect.minY + inset.gridRect.y,
            width: inset.gridRect.w, height: inset.gridRect.h)
    }

    /// The number of live panes (`PaneTree` leaf count); 1 for a single unsplit
    /// window. Read by the window controller to disable the Close command on the
    /// last split.
    var paneCount: Int { tree.leaves().count }

    /// The scalar inputs `PreviewSeam.placementMode` needs to choose a preview
    /// placement, read from this container's live pane/preview state (no new state):
    /// the live pane count, whether the ACTIVE pane is itself a preview, and how
    /// many OTHER panes are previews. See `previewPlacementInputs()`.
    func previewPlacementInputs() -> PreviewPlacementInputs {
        // The set of window handles currently showing a preview. `previews()`
        // yields borrowed `FfiPreviewRef`s valid only for the loop body, so each
        // `win()` scalar is copied out INSIDE the iteration (mirror the caution in
        // `syncPreviewWebViews`).
        var previewWins = Set<Int64>()
        for preview in handle.previews() {
            previewWins.insert(preview.win())
        }

        let activeWin = activePane.flatMap { panes[$0]?.win }
        let activePaneIsPreview = activeWin.map(previewWins.contains) ?? false
        let otherPreviewPaneCount = panes.values.filter { binding in
            previewWins.contains(binding.win) && binding.win != activeWin
        }.count

        return PreviewPlacementInputs(
            paneCount: tree.leaves().count,
            activePaneIsPreview: activePaneIsPreview,
            otherPreviewPaneCount: otherPreviewPaneCount)
    }

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

    /// Marks that this view just became the current view of its project (its
    /// tabpage was made current). The next wake whose snapshot reflects THIS
    /// view's tabpage consumes a one-shot grid rebind in `onRenderWake`.
    func markRefocusRebind() {
        pendingRefocusRebind = true
    }

    /// Orchestrates a render wake (port of `PaneInner::on_render_wake`): ensure a
    /// first pane exists, apply layout intents, collapse closed panes, resolve
    /// pending grids, refresh the active pane, sync grid sizes, then redraw.
    ///
    /// REFOCUS REBIND (OD2): when this view just became its project's current view
    /// (`markRefocusRebind`), nvim has switched to this view's tabpage — churning
    /// grid ids while keeping WINDOW handles stable. The pane→grid bindings from
    /// before the switch are stale. Consume the rebind ONLY once the snapshot
    /// reflects this tabpage (`active_tabpage()` gate), then re-derive every pane's
    /// grid via `grid_for_win` on the PRESERVED `PaneTree` (no geometry rebuild).
    /// `detectCloses()` is SKIPPED on this pass — the just-vacated tabpage's grids
    /// are gone from the snapshot and would wrongly prune live panes; the next
    /// normal pass reconciles any real closes.
    func onRenderWake() {
        if pendingRefocusRebind {
            guard handle.active_tabpage() == controllerTabpageHandle else { return }
            pendingRefocusRebind = false
            // A view that was materialized but never yet current has no panes; make
            // its first pane bind to the now-current tabpage's window.
            ensureInitialPane()
            rebindStaleGrids()
            updateActivePane()
            syncGridSizes()
            applyTitlebarTheme()
            syncPreviewWebViews()
            needsDisplay = true
            return
        }

        ensureInitialPane()
        applyIntents()
        detectCloses()
        resolvePendingGrids()
        updateActivePane()
        syncGridSizes()
        applyTitlebarTheme()
        syncPreviewWebViews()
        needsDisplay = true
    }

    /// Reconciles the hosted Markdown-preview `WKWebView`s with the core's current
    /// `previews()` snapshot (requirement 2 / OD1 = Design S).
    ///
    /// For every pane bound to a preview WINDOW (a window showing a
    /// `b:wolf_preview_source` scratch buffer), ensure a `WKWebView` framed to the
    /// pane's GRID rect, and reload it from `render_preview(source_bufnr)` whenever
    /// the source's `changedtick` advanced (the OD6 live-on-change gate; a keystroke
    /// burst is already coalesced into one tick-advancing wake). A dim overlay is
    /// applied when the pane is not active. The pane's grid is recorded in
    /// `previewNamesByGrid` so `paintStatusbar` shows `Preview: <name>` and
    /// `drawScrollbars` suppresses the synthetic scrollbar. Web views + overlays for
    /// windows that are no longer previews (or closed) are removed.
    private func syncPreviewWebViews() {
        previewNamesByGrid.removeAll(keepingCapacity: true)
        previewDisplayPathsByGrid.removeAll(keepingCapacity: true)
        var liveWins = Set<Int64>()
        var current: [Int64: PreviewSeam.PreviewLoad] = [:]
        let rects = paneRects()

        // `previews()` yields borrowed `FfiPreviewRef`s valid only for the loop
        // body, so each preview is fully consumed inside its own iteration (the pane
        // is found by its stable window handle; scalars/strings are copied out).
        for preview in handle.previews() {
            let win = preview.win()
            guard
                let entry = rects.first(where: { panes[$0.0]?.win == win }),
                let grid = panes[entry.0]?.boundGrid
            else { continue }
            liveWins.insert(win)
            previewNamesByGrid[grid] = preview.source_name().toString()
            previewDisplayPathsByGrid[grid] = preview.source_display_path().toString()

            // Copy the borrowed ref's scalars out into a `PreviewLoad`; only wins that
            // passed the pane/grid guard land in `current`, preserving the "only
            // previews with a bound pane arm the coalescer" behavior.
            current[win] = PreviewSeam.PreviewLoad(
                sourceBufnr: preview.source_bufnr(), sourceTick: preview.source_tick())

            let gRect = gridRect(ofPane: entry.1)
            let web = ensurePreviewWebView(for: win)
            web.frame = gRect
            web.isHidden = false

            applyPreviewDim(win: win, over: gRect, dimmed: entry.0 != activePane)
        }

        // Only the cheap AppKit layout ran per wake above; the BLOCKING reload is
        // debounced. Requesting when any win's bound source buffer changed (a reused
        // preview pane switching sources, even at an equal tick) or its tick advanced
        // coalesces a keystroke burst into one trailing reload. `lastPreviewLoads` is
        // updated in the coalesced `perform`, not here, so the gate survives the
        // debounce.
        if !PreviewSeam.winsNeedingReload(current: current, loaded: lastPreviewLoads).isEmpty {
            previewReloadCoalescer.request()
        }

        for win in Array(previewWebViews.keys) where !liveWins.contains(win) {
            removePreviewWebView(win)
        }
    }

    /// The coalesced preview-reload pass (the `previewReloadCoalescer`'s `perform`,
    /// run once per debounce window on the main thread). Re-reads the core's
    /// `previews()` into a win → `PreviewLoad` map, then for each window whose bound
    /// source BUFFER changed or whose `sourceTick` advanced past `lastPreviewLoads`
    /// (via `winsNeedingReload`) and whose hosted web view still exists, does the one
    /// blocking `render_preview` round-trip and reloads the web view. The per-window
    /// bufnr-or-tick gate here is the redundant-reload guard; the coalescer bounds
    /// the rate.
    private func reloadDirtyPreviews() {
        var current: [Int64: PreviewSeam.PreviewLoad] = [:]
        for preview in handle.previews() {
            let win = preview.win()
            current[win] = PreviewSeam.PreviewLoad(
                sourceBufnr: preview.source_bufnr(), sourceTick: preview.source_tick())
            // Track the abs source path so a clicked link resolves relative targets
            // (baseURL) and classifies same-vs-cross file (PreviewLinkSeam).
            previewSourcePaths[win] = preview.source_path().toString()
        }
        let needing = PreviewSeam.winsNeedingReload(current: current, loaded: lastPreviewLoads)
        for win in needing {
            // Skip a preview whose web view does not exist yet WITHOUT updating the
            // map, so the next sync re-requests once the web view exists.
            guard let web = previewWebViews[win], let load = current[win] else { continue }
            lastPreviewLoads[win] = load
            let html = handle.render_preview(load.sourceBufnr).toString()
            // A `file://` baseURL from the win's source path lets relative links (and
            // #fragments) resolve against the real file location; unknown → nil
            // (unchanged OD/O4 behavior — no local-subresource loading is added).
            let path = previewSourcePaths[win] ?? ""
            let base = path.isEmpty ? nil : URL(fileURLWithPath: path)
            web.loadHTMLString(html, baseURL: base)
        }
    }

    /// Returns the preview `WKWebView` for window `win`, creating and adding it as a
    /// subview on first use.
    private func ensurePreviewWebView(for win: Int64) -> WKWebView {
        if let existing = previewWebViews[win] { return existing }

        // Build the configuration up front so the click-to-focus user script +
        // message handler are installed before the web view exists. The preview
        // `WKWebView` swallows AppKit mouse events, so the container's `mouseDown`
        // never fires over it; the injected capture-phase `mousedown` listener
        // (`PreviewSeam.clickFocusJS`) posts to the `PreviewClickFocusHandler`
        // registered under `PreviewSeam.clickFocusHandlerName`, which focuses the
        // pane. The controller retains the proxy STRONGLY, so the proxy holds this
        // container WEAKLY (no retain cycle); the proxy is also retained here for
        // explicit removal in `removePreviewWebView`.
        let config = WKWebViewConfiguration()
        let proxy = PreviewClickFocusHandler(win: win, container: self)
        previewFocusHandlers[win] = proxy
        config.userContentController.add(proxy, name: PreviewSeam.clickFocusHandlerName)
        config.userContentController.addUserScript(
            WKUserScript(
                source: PreviewSeam.clickFocusJS(),
                injectionTime: .atDocumentStart,
                forMainFrameOnly: true))

        let web = WKWebView(frame: .zero, configuration: config)
        previewWebViews[win] = web
        // Route clicked links (`.linkActivated`) through PreviewLinkSeam. The web
        // view holds `navigationDelegate` weakly, so retain the delegate here; the
        // delegate holds this container weakly (no cycle).
        let delegate = PreviewNavigationDelegate(win: win, container: self)
        web.navigationDelegate = delegate
        previewNavDelegates[win] = delegate
        addSubview(web)
        return web
    }

    /// Focuses the preview pane bound to window `win` (invoked by that pane's
    /// `PreviewClickFocusHandler` when the user clicks anywhere in the preview web
    /// view, which swallows AppKit clicks so the container's `mouseDown` never
    /// fires there). Mirrors `mouseDown`'s focus path — `handle.focus_win` +
    /// `setActivePane` — WITHOUT synthesizing a mouse event.
    func focusPreviewPane(win: Int64) {
        guard let id = paneForWin(win) else { return }
        handle.focus_win(win)
        setActivePane(id)
        needsDisplay = true
    }

    /// Applies (or hides) the inactive-dim scrim over the preview web view for
    /// window `win`, kept above the web view in the subview order.
    private func applyPreviewDim(win: Int64, over rect: CGRect, dimmed: Bool) {
        guard dimmed else {
            previewDimOverlays[win]?.isHidden = true
            return
        }
        let overlay: NSView
        if let existing = previewDimOverlays[win] {
            overlay = existing
        } else {
            overlay = NSView(frame: rect)
            overlay.wantsLayer = true
            overlay.layer?.backgroundColor = CGColor(gray: 0, alpha: Self.inactiveDim)
            previewDimOverlays[win] = overlay
            addSubview(overlay)
        }
        overlay.frame = rect
        overlay.isHidden = false
        if let web = previewWebViews[win] {
            addSubview(overlay, positioned: .above, relativeTo: web)
        }
    }

    /// Tears down the hosted web view + dim overlay + last-load entry for window
    /// `win` (it is no longer a preview, or its pane closed).
    private func removePreviewWebView(_ win: Int64) {
        // Remove the click-to-focus script-message handler on the STILL-LIVE web
        // view's content controller before dropping it, so the controller releases
        // its strong ref to the proxy (and the injected script is torn down with the
        // web view).
        previewWebViews[win]?.configuration.userContentController
            .removeScriptMessageHandler(forName: PreviewSeam.clickFocusHandlerName)
        previewWebViews[win]?.removeFromSuperview()
        previewWebViews[win] = nil
        previewDimOverlays[win]?.removeFromSuperview()
        previewDimOverlays[win] = nil
        lastPreviewLoads[win] = nil
        previewNavDelegates[win] = nil
        previewFocusHandlers[win] = nil
        previewSourcePaths[win] = nil
        pendingPreviewFragments[win] = nil
    }

    // -- Preview link handling (called by PreviewNavigationDelegate) ----------

    /// The absolute source path currently previewed in `win`, for
    /// `PreviewLinkSeam.classify`'s same-vs-cross-file comparison. Falls back to a
    /// live `handle.previews()` scan when the cache has no entry yet.
    func previewSourcePath(forWin win: Int64) -> String {
        if let path = previewSourcePaths[win] { return path }
        for preview in handle.previews() where preview.win() == win {
            return preview.source_path().toString()
        }
        return ""
    }

    /// Scrolls the preview web view for `win` to the `#fragment` anchor (the
    /// same-file `[x](#anchor)` case, and the post-load scroll of a cross-file
    /// `other.md#anchor`). Runs the pure `PreviewLinkSeam.scrollIntoViewJS`.
    func scrollPreview(win: Int64, toFragment fragment: String) {
        previewWebViews[win]?.evaluateJavaScript(
            PreviewLinkSeam.scrollIntoViewJS(fragmentId: fragment), completionHandler: nil)
    }

    /// Repoints the preview `win` at the linked file `path` (a clicked `file://`
    /// link). Asks core to `preview_file`; on success (`src > 0`) stashes any
    /// `fragment` to scroll after the reload and requests a coalesced reload. On `0`
    /// applies O3: an existing-but-non-previewable file is handed to the OS default
    /// app (`NSWorkspace.open`); a missing file is a no-op.
    func previewFile(win: Int64, path: String, fragment: String?) {
        let src = handle.preview_file(win, path)
        if src == 0 {
            // O3: not previewable. If the file exists, let the OS default app open
            // it; if it is missing, do nothing (keep the preview view-only).
            if FileManager.default.fileExists(atPath: path) {
                NSWorkspace.shared.open(URL(fileURLWithPath: path))
            }
            return
        }
        if let fragment {
            pendingPreviewFragments[win] = fragment
        }
        previewReloadCoalescer.request()
    }

    /// A preview web view finished loading: if a fragment was queued for `win` (a
    /// cross-file `other.md#anchor` navigation), scroll to it now and clear it.
    func previewDidFinishLoad(win: Int64) {
        guard let fragment = pendingPreviewFragments.removeValue(forKey: win) else { return }
        previewWebViews[win]?.evaluateJavaScript(
            PreviewLinkSeam.scrollIntoViewJS(fragmentId: fragment), completionHandler: nil)
    }

    /// Nulls EVERY pane's bound grid, then re-resolves each via `grid_for_win` on
    /// its (stable) window handle. The `PaneTree` topology is untouched, so a
    /// multi-pane layout restores by rebinding grids alone (OD2).
    private func rebindStaleGrids() {
        for id in panes.keys {
            panes[id]?.boundGrid = nil
        }
        resolvePendingGrids()
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
            paintPane(grid: grid, paneRect: rect, dimmed: id != activePane, ctx: ctx)
        }
        drawDividers(rects, ctx: ctx)
        paintCmdlineStrip(ctx)
        drawScrollbars(rects, ctx: ctx)
    }

    /// Paints one whole pane: the top statusbar strip, then the window grid below
    /// it, then (for an inactive pane) an inactive-dim scrim over the WHOLE pane
    /// (statusbar + grid + any side margins), so the strip dims with its pane.
    private func paintPane(grid: UInt64, paneRect: CGRect, dimmed: Bool, ctx: CGContext) {
        ctx.saveGState()
        ctx.clip(to: paneRect)

        paintStatusbar(into: statusbarRect(ofPane: paneRect), grid: grid, paneRect: paneRect, ctx: ctx)

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

    /// Paints the per-pane statusbar strip: a titlebar-matched background, the
    /// pane's base filename (primary line), and its display path (secondary line).
    ///
    /// The background matches the macOS titlebar: the window's own
    /// `backgroundColor` (kept in sync with the `WolfTitlebar` theme by
    /// `MainWindow.applyTitlebarBackground`), falling back to the themed titlebar
    /// bg / editor fallback when there is no window. Text is left-aligned with the
    /// grid's text left edge (respecting `PaneGeometry.paintOffsetX`) and truncated
    /// with a tail ellipsis. An unnamed buffer shows `[No Name]` with no path line.
    private func paintStatusbar(into strip: CGRect, grid: UInt64, paneRect: CGRect, ctx: CGContext) {
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

        // Text left- and right-aligns with the grid's text edges: base it on the
        // INSET grid rect (not the full pane rect), so the statusbar text sits inside
        // the per-pane padding, flush with the grid text.
        let gRect = gridRect(ofPane: paneRect)
        let geo = geometry(for: gRect)
        let textLeft = gRect.minX + CGFloat(geo.paintOffsetX)
        let textWidth = max(0, gRect.maxX - textLeft)

        let layout = statusbarLayout
        // A preview pane's scratch buffer is SKIPPED by the path-based trackers, so
        // its pane_status is "[No Name]"; override it with `Preview: <basename>`
        // (from the core's previews() snapshot) and the source's cwd/home-relative
        // display path on the secondary line.
        let filename: String
        let displayPath: String
        if let name = previewNamesByGrid[grid] {
            filename = "Preview: \(name)"
            // Show the source's cwd/home-relative path on the secondary line,
            // mirroring editor panes (empty → no path line via the guard below).
            displayPath = previewDisplayPathsByGrid[grid] ?? ""
        } else {
            let status = handle.pane_status_for_grid(grid)
            filename = status.filename().toString()
            displayPath = status.display_path().toString()
        }

        // Colors: the themed titlebar fg when set (secondary derived at a lower
        // alpha), else AppKit's label / secondary-label system colors.
        let titlebarFg = handle.theme_titlebar_fg()
        let primaryColor: NSColor
        let secondaryColor: NSColor
        if titlebarFg.set {
            let themed = NSColor(cgColor: WolfColor.cgColor(packed: titlebarFg.rgb)) ?? .labelColor
            primaryColor = themed
            secondaryColor = themed.withAlphaComponent(Self.statusbarSecondaryThemedAlpha)
        } else {
            primaryColor = .labelColor
            secondaryColor = .secondaryLabelColor
        }

        let primaryRect = CGRect(
            x: textLeft, y: strip.minY + CGFloat(layout.primaryLineTop),
            width: textWidth, height: CGFloat(layout.primaryLineHeight))
        drawStatusbarText(
            filename, in: primaryRect,
            font: .systemFont(ofSize: CGFloat(statusbarPrimaryFontSize)), color: primaryColor)

        if !displayPath.isEmpty {
            let secondaryRect = CGRect(
                x: textLeft, y: strip.minY + CGFloat(layout.secondaryLineTop),
                width: textWidth, height: CGFloat(layout.secondaryLineHeight))
            drawStatusbarText(
                displayPath, in: secondaryRect,
                font: .systemFont(ofSize: CGFloat(statusbarSecondaryFontSize)), color: secondaryColor)
        }

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
            // A preview pane hosts a WKWebView (with its own scrolling), so suppress
            // the synthetic scrollbar over it.
            if previewNamesByGrid[grid] != nil { continue }
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
