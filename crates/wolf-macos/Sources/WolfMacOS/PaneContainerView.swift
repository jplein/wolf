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
//  per-pane treatments (e.g. the active pane's bold statusbar filename). A one-row cmdline strip along
//  the bottom paints the outer grid (grid 1). This view is the editor area's first
//  responder and the single global key sink (keys go to nvim's current window).
//
//  Painting one flat view avoids the fragile compositing of deeply-nested custom
//  layer-backed views. Interior dividers ARE draggable: `mouseDown` hit-tests the
//  pure `PaneTree.dividers()` list (via `DividerHitTest`) BEFORE forwarding to
//  nvim; a drag reweights the two adjacent panes through
//  `DividerDragSeam.clampedLeftFraction` + `PaneTree.setPairFraction` (clamped so
//  no pane drops below 3 rows / 10 columns) and converges the grids via
//  `syncGridSizes`; `resetCursorRects` shows the resize cursor on hover (and is
//  invalidated on every layout mutation). The whole layout (topology + weights +
//  per-leaf buffer path) persists via `layoutSnapshot()` and is rebuilt on launch
//  via `requestLayoutRebuild(_:)`. The pure layout decisions live in the
//  headlessly-tested `WolfKeyMap.PaneTree`; this AppKit glue is compile-verified +
//  manual-smoke only.
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

/// The scalar inputs `EditSeam.placementMode` reads to choose a source-editor
/// placement for the Edit command (the inverse of `PreviewPlacementInputs`),
/// gathered from a `PaneContainerView`'s live pane/preview state. Internal (not
/// `private`) so `WolfWindowController.editSource()` — in a sibling file of the
/// same module — can feed it through the pure seam.
struct EditPlacementInputs {
  let paneCount: Int
  let activePaneIsPreview: Bool
  let otherEditorPaneCount: Int
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
  /// `FfiLayoutIntent.kind()` for a terminal-DRAWER open (mirrors `wolf-ffi`).
  private static let intentKindDrawerOpen: UInt8 = 2
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

  // -- Divider drag-to-resize tuning ----------------------------------------
  /// The clickable half-width around a divider line, in points: a `mouseDown`
  /// within this distance of a divider grabs it instead of forwarding to nvim, and
  /// the resize-cursor rect is this wide on each side of the line.
  private static let dividerHitTolerance: CGFloat = 4
  /// The minimum columns a pane may shrink to when dragging a vertical divider.
  private static let minPaneCols = 10
  /// The minimum rows a pane may shrink to when dragging a horizontal divider.
  private static let minPaneRows = 3

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
  /// The statusbar text alpha applied to an INACTIVE pane's filename + path (a
  /// front-end-only treatment that is awkward to express inside Neovim itself):
  /// the active pane's filename is bold at full strength, an inactive pane's is
  /// regular-weight and dimmed. Multiplies the themed alpha for the secondary
  /// line; for the non-themed case the inactive pane falls back to the
  /// system `secondaryLabelColor` / `tertiaryLabelColor` instead.
  private static let statusbarInactiveAlpha: CGFloat = 0.5

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
  /// A `var` because a crash-restart rebinds this view to a FRESH tabpage (see
  /// `rebind(tabpageHandle:)`).
  private var controllerTabpageHandle: Int64

  /// The crash/stall overlay ("Neovim crashed" / "Neovim stopped responding" +
  /// "Restart Neovim"). A subview of this container (mirroring the empty-state
  /// overlay on the controller), shown via `showNvimOverlay(_:)` when the project
  /// reads a non-`Alive` liveness and hidden via `hideNvimOverlay()`.
  private let nvimOverlay = NvimStatusOverlayView()

  /// The transient post-restart banner ("Neovim was restarted"). A SMALL top-center
  /// banner over the WORKING editor, flashed via `flashRestartNotice()` from a
  /// successful restart and auto-dismissed on its own timer — INDEPENDENT of the
  /// liveness-driven `nvimOverlay` (so `hideNvimOverlay()` never clears it).
  private let restartBanner = NvimRestartedBannerView()

  /// Invoked when the overlay's "Restart Neovim" button is clicked. The controller
  /// wires this to `project.restartNeovim()`.
  var onRestart: (() -> Void)? {
    didSet { nvimOverlay.onRestart = onRestart }
  }

  /// Set by the window controller (`windowDidBecomeKey`) after it drives its
  /// project's `set_current_tabpage`: the next wake whose snapshot reflects this
  /// view's tabpage re-derives every pane's grid binding (nvim churns grid ids
  /// across a tabpage switch while WINDOW handles stay stable — OD2), then clears
  /// the flag. See `onRenderWake`.
  private var pendingRefocusRebind = false

  /// The pure pane-layout tree (split/remove/neighbour/divider geometry). A `var`
  /// so `requestLayoutRebuild` can replace it wholesale on launch restore via
  /// `replaceTree(_:)`.
  private var tree = PaneTree(root: PaneContainerView.firstPaneId)

  /// The interior divider a `mouseDown` grabbed, held across the drag so
  /// `mouseDragged` reweights the two adjacent panes; cleared on `mouseUp`.
  private var activeDivider: PaneTree.Divider?

  /// A saved pane-split layout awaiting rebuild once this view's first pane binds
  /// (consumed once in `onRenderWake`, gated like `pendingRefocusRebind`).
  private var pendingLayoutRebuild: SavedPaneNode?

  /// Invoked whenever the pane layout changes in a way that must be persisted (a
  /// divider drag ends, or a restore rebuild completes). Set by the window
  /// controller to `coordinator.scheduleStateSnapshot()`.
  var onLayoutChanged: (() -> Void)?
  /// The per-pane bindings, keyed by `PaneId`.
  private var panes: [PaneId: PaneBinding] = [:]
  /// The next pane id to allocate (starts at `firstPaneId`).
  private var nextPaneId: PaneId = PaneContainerView.firstPaneId
  /// The currently active pane (cursor target + the one whose statusbar filename
  /// is bold at full strength), if any.
  private var activePane: PaneId?
  /// The terminal-drawer pane, if open. Held in `panes` (so it paints /
  /// hit-tests / sizes via `paneRects()`) but deliberately NOT in `tree` — it is a
  /// full-width, root-bottom pane rendered OUTSIDE the pure `PaneTree` (preserving
  /// its 1:1 parity with wolf-gnome). Bound on a `DrawerOpen` intent; cleared when
  /// its window closes.
  private var drawerPaneId: PaneId?
  /// The most-recently-active EDITOR pane (a live tree leaf, never the drawer).
  /// The `<C-w>k` (drawer -> up) bridge targets this when it is still a leaf; it is
  /// updated only in `setActivePane`.
  private var lastEditorPane: PaneId?

  // -- Markdown-preview web-view hosting ------------------------------------
  // A preview pane is a SECOND pane content kind (requirement 2): a real
  // `WKWebView` subview shows core-rendered HTML over the pane's grid rect,
  // backed by a blank nvim scratch grid. That scratch grid is NOT painted while
  // the web view fully, opaquely covers it — see `PaneOcclusion` and the
  // `paintsGrid` parameter of `paintPane`.
  //
  // This used to read "the blank scratch grid paints underneath (occluded,
  // cheap)". It was never cheap, and that wrong assumption is what hid the cost
  // for so long: nvim's blank cells hold a literal `" "`, not `""`, so every
  // "blank" cell still ran a full `CTLineCreateWithAttributedString` +
  // `CTLineDraw` (plus a per-cell Rust `String` clone across the FFI). Measured
  // with `WOLF_FRAME_STATS=1`, the blank preview grid (`draw.prev`) cost 22 ms
  // per frame — essentially identical to the text-full editor grid
  // (`draw.pane`, also 22 ms) — together nearly all of the ~46 ms `draw`.
  //
  // These maps are keyed by the preview WINDOW handle (`FfiPreview.win()`), the
  // stable identity that survives grid-id churn.
  /// The hosted `WKWebView`s, one per preview window.
  private var previewWebViews: [Int64: WKWebView] = [:]
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
  /// The preview windows whose web view has EVER finished loading a document —
  /// the `hasLoadedContent` input to `PaneOcclusion.isFullyOccluded`. A web view
  /// that exists but has loaded nothing is blank/white, so its pane's grid must
  /// still paint underneath.
  ///
  /// Set on the first `didFinish` for a window and deliberately NOT cleared on a
  /// subsequent reload: WebKit keeps displaying the PREVIOUS document while the
  /// next one loads, so the pane stays opaquely covered throughout. Clearing it
  /// per reload would resume painting the scratch grid under an
  /// already-opaque web view on every keystroke — pure paint/unpaint flicker.
  /// It IS cleared in `removePreviewWebView`, which is what makes a closed
  /// preview restore painting immediately.
  private var previewLoadedWins: Set<Int64> = []
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
  /// The SOURCE buffer's full-width flag of each preview pane, keyed by the pane's
  /// bound GRID, rebuilt every wake by `syncPreviewWebViews` from
  /// `FfiPreview.source_full_width()`. Consulted by `fullWidth(forGrid:)` so the
  /// preview's webview frame, occlusion test, and statusbar span all resolve the same
  /// capped-vs-full geometry.
  private var previewFullWidthByGrid: [UInt64: Bool] = [:]
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

  // -- Native command-line / completion / toast overlays (OD9) --------------
  //    Dedicated `NSView` subviews pinned to the bottom of the editor area and
  //    shown only while active, rather than a permanently-reserved band, so the
  //    editor grid keeps its full cols×rows when nothing is showing
  //    (`cmdheight=0` spirit). They replace the old grid-painted cmdline strip.
  /// The native command line (`ext_cmdline`).
  private let cmdlineView: CmdlineView
  /// The completion popup (`ext_popupmenu`), rendered above the command line.
  private let completionPopupView: CompletionPopupView
  /// The message toast (`ext_messages`), pinned bottom-left.
  private let toastView: ToastView

  /// Builds the container bound to a live core `handle`, measured `metrics`, and
  /// the view's `tabpageHandle` (the project/tabpage-per-view bind used to gate
  /// the refocus rebind). No panes exist until the first window grid appears on a
  /// wake.
  init(handle: WolfCoreHandle, metrics: CellMetrics, tabpageHandle: Int64) {
    self.handle = handle
    self.metrics = metrics
    self.controllerTabpageHandle = tabpageHandle
    self.cmdlineView = CmdlineView(metrics: metrics)
    self.completionPopupView = CompletionPopupView(metrics: metrics)
    self.toastView = ToastView(metrics: metrics)
    super.init(frame: .zero)
    // Layer-back so a VISIBLE non-key window (two side-by-side windows of the
    // SAME project) keeps its last-painted layer while it is not the current
    // view (the project drives wakes only to `currentView`).
    wantsLayer = true
    // The bottom overlays composite ON TOP of the flat pane painting; the
    // completion popup is added LAST so it sits above the command line.
    addSubview(toastView)
    addSubview(cmdlineView)
    addSubview(completionPopupView)
    // The crash/stall overlay composites ON TOP of everything (added last, starts
    // hidden); it is revealed only when the project reads a non-Alive liveness.
    addSubview(nvimOverlay)
    // The transient restart banner is added ONCE here, above the panes. It is NOT
    // re-added per flash (that would duplicate its `viewDidMoveToSuperview`
    // constraints); `flash()` only toggles `isHidden`. This is NOT a permanent
    // topmost invariant: a later `showNvimOverlay` does removeFromSuperview +
    // addSubview on `nvimOverlay`, intentionally re-raising the full-bleed overlay
    // ABOVE the banner — a fresh crash's overlay should cover any stale banner, and
    // the banner only ever shows while `nvimOverlay` is hidden.
    addSubview(restartBanner)
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

  /// The height of the pane area. Under OD9 the native command line + toast are
  /// OVERLAY subviews shown only while active (not a permanently-reserved band),
  /// so the pane area — and hence each pane grid — occupies the FULL view height
  /// (matching Neovim's `cmdheight=0`). The overlays composite over the bottom of
  /// this area when they appear.
  private var paneAreaHeight: CGFloat { bounds.height }

  /// Maps a pixel size to a grid `(cols, rows)`, each at least 1.
  private func gridCells(width: CGFloat, height: CGFloat) -> (cols: Int, rows: Int) {
    (max(1, Int(width / metrics.cellWidth)), max(1, Int(height / metrics.cellHeight)))
  }

  /// The current pane rectangles from the pure layout, in this view's (flipped)
  /// coordinates, spanning the FULL bounds (origin `(0, 0)`): the panes tile the
  /// whole `paneAreaHeight` (the full view height under OD9; the command line /
  /// toast are bottom overlays that composite over it). The per-pane 20pt inset is
  /// NOT applied here — it is applied around each pane's grid in `gridRect(ofPane:)`,
  /// so these rects are the full pane rectangles.
  ///
  /// When the terminal drawer is open the editor tree tiles over the reduced
  /// `DrawerLayout.editorHeight` and the full-width drawer strip is appended below
  /// it (`DrawerLayout.drawerRect`). The drawer pane lives OUTSIDE `tree`, so it is
  /// appended here rather than produced by `tree.paneRects`.
  private func paneRects() -> [(PaneId, CGRect)] {
    let dl = DrawerLayout(
      paneAreaHeight: Double(paneAreaHeight),
      hasDrawer: drawerPaneId != nil,
      weight: DrawerLayout.defaultWeight)
    var rects = tree.paneRects(width: Double(bounds.width), height: dl.editorHeight)
      .map {
        ($0.0, CGRect(x: $0.1.x, y: $0.1.y, width: $0.1.w, height: $0.1.h))
      }
    if let drawer = drawerPaneId, let r = dl.drawerRect(width: Double(bounds.width)) {
      rects.append((drawer, CGRect(x: r.x, y: r.y, width: r.w, height: r.h)))
    }
    return rects
  }

  /// The editor tree's interior dividers, in this view's (flipped) coordinates,
  /// over the SAME `DrawerLayout.editorHeight` as `paneRects()`. The drawer lives
  /// outside `tree`, so it has no divider here.
  private func editorDividers() -> [PaneTree.Divider] {
    let dl = DrawerLayout(
      paneAreaHeight: Double(paneAreaHeight),
      hasDrawer: drawerPaneId != nil,
      weight: DrawerLayout.defaultWeight)
    return tree.dividers(width: Double(bounds.width), height: dl.editorHeight)
  }

  /// The per-pane GRID-RECT-inside-PANE-RECT geometry for `rect` (the pure seam
  /// that owns all pixel<->cell mapping). When `fullWidth` is `false` (the ~90-char
  /// default) the grid width is capped at `maxGridCols` and CENTERED when the pane is
  /// wider than that; when `fullWidth` is `true` the cap is lifted (`maxCols: nil`)
  /// so the grid fills the whole pane rect (`gridRect == rect`, `paintOffsetX == 0`).
  /// A pane narrower than the cap keeps `gridRect == rect` regardless.
  private func geometry(for rect: CGRect, fullWidth: Bool) -> PaneGeometry {
    PaneGeometry(
      paneRect: Rect(
        x: Double(rect.minX), y: Double(rect.minY),
        w: Double(rect.width), h: Double(rect.height)),
      cellWidth: Double(metrics.cellWidth),
      cellHeight: Double(metrics.cellHeight),
      maxCols: fullWidth ? nil : Self.maxGridCols)
  }

  /// Whether window `win` is in FULL-WIDTH mode (core's per-window `full_width`
  /// map). `false` when `win` is absent from the map or the core is gone.
  private func fullWidth(forWin win: Int64) -> Bool {
    handle.full_width_for_win(win)
  }

  /// Whether the pane bound to `grid` is in FULL-WIDTH mode. A PREVIEW pane follows
  /// its SOURCE buffer's flag (cached per wake in `previewFullWidthByGrid` from
  /// `FfiPreview.source_full_width()`), so its webview frame, occlusion test, and
  /// statusbar span all agree; an EDITOR pane follows the per-window `full_width` map
  /// via its bound window handle. `false` when the grid has no bound pane.
  private func fullWidth(forGrid grid: UInt64) -> Bool {
    if let cached = previewFullWidthByGrid[grid] { return cached }
    for binding in panes.values where binding.boundGrid == grid {
      return handle.full_width_for_win(binding.win)
    }
    return false
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
    max(
      Self.statusbarMinSecondaryFontSize,
      statusbarPrimaryFontSize * Self.statusbarSecondaryFontScale)
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

  /// The scalar inputs `EditSeam.placementMode` needs to choose a source-editor
  /// placement for the Edit command, read from this container's live pane/preview
  /// state (no new state): the live pane count, whether the ACTIVE pane is itself
  /// a preview, and how many OTHER panes are editors (non-preview). The inverse of
  /// `previewPlacementInputs()`.
  func editPlacementInputs() -> EditPlacementInputs {
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
    let otherEditorPaneCount = panes.values.filter { binding in
      !previewWins.contains(binding.win) && binding.win != activeWin
    }.count

    return EditPlacementInputs(
      paneCount: tree.leaves().count,
      activePaneIsPreview: activePaneIsPreview,
      otherEditorPaneCount: otherEditorPaneCount)
  }

  /// Re-measures against `newMetrics` (a per-window font-size change): swaps the
  /// cell geometry, re-syncs every grid's cell size to the new metrics, and
  /// repaints. The OS window frame is deliberately UNCHANGED — the grid reflows
  /// its cols/rows to fit.
  func applyMetrics(_ newMetrics: CellMetrics) {
    metrics = newMetrics
    cmdlineView.applyMetrics(newMetrics)
    completionPopupView.applyMetrics(newMetrics)
    toastView.applyMetrics(newMetrics)
    syncGridSizes()
    refreshOverlays()
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
    // Counted here, after the picker-trigger return and the translate guard, so
    // it reflects only keys actually delivered to Neovim.
    FrameMetrics.noteKeystroke()
    handle.send_key(ev)
  }

  /// Click-to-focus AND position the cursor: focus the window of the pane under
  /// the click, then send a LEFT/PRESS at the clicked grid cell (under nvim's
  /// `mouse=a` this positions the cursor; a following drag extends a selection).
  override func mouseDown(with event: NSEvent) {
    let point = convert(event.locationInWindow, from: nil)

    // A press near an interior divider grabs it to resize, and is NOT forwarded to
    // nvim as a cursor/selection event. Hit-tested BEFORE the pane-forward loop.
    if let divider = DividerHitTest.hit(
      editorDividers(),
      point: (x: Double(point.x), y: Double(point.y)),
      tolerance: Double(Self.dividerHitTolerance))
    {
      activeDivider = divider
      return
    }

    for (id, rect) in paneRects() where rect.contains(point) {
      guard let binding = panes[id] else { return }
      handle.focus_win(binding.win)
      setActivePane(id)
      needsDisplay = true
      if let grid = binding.boundGrid {
        let cell = geometry(for: gridRect(ofPane: rect), fullWidth: fullWidth(forGrid: grid))
          .cellAt(x: Double(point.x), y: Double(point.y))
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
    let point = convert(event.locationInWindow, from: nil)

    // A divider drag reweights the two adjacent panes (clamped so neither drops
    // below the min grid size) and converges the grids; it is NOT a nvim drag.
    if let divider = activeDivider {
      let pointerPos =
        divider.orientation == .vertical ? Double(point.x) : Double(point.y)
      let overhead: Double =
        divider.orientation == .vertical
        ? 2 * EditorInset.padding
        : Double(statusbarHeight) + 2 * EditorInset.padding
      let fraction = DividerDragSeam.clampedLeftFraction(
        pairStart: divider.pairStart,
        pairSpan: divider.pairSpan,
        pointerPos: pointerPos,
        orientation: divider.orientation,
        cellWidth: Double(metrics.cellWidth),
        cellHeight: Double(metrics.cellHeight),
        minCols: Self.minPaneCols,
        minRows: Self.minPaneRows,
        leadingOverhead: overhead,
        trailingOverhead: overhead)
      tree.setPairFraction(
        path: divider.path, boundary: divider.boundary, leftFraction: fraction)
      syncGridSizes()
      window?.invalidateCursorRects(for: self)
      needsDisplay = true
      return
    }

    guard let grid = mouseDragGrid, let rect = paneRect(forGrid: grid) else { return }
    let cell = geometry(for: gridRect(ofPane: rect), fullWidth: fullWidth(forGrid: grid))
      .cellAt(x: Double(point.x), y: Double(point.y))
    handle.send_mouse(mouseEvent(Self.mouseActionDrag, grid: grid, cell: cell, event: event))
  }

  /// Ends the press/drag (RELEASE), then clears the drag target.
  override func mouseUp(with event: NSEvent) {
    // Finish a divider drag: clear it and persist the resized layout.
    if activeDivider != nil {
      activeDivider = nil
      onLayoutChanged?()
      return
    }

    guard let grid = mouseDragGrid else { return }
    if let rect = paneRect(forGrid: grid) {
      let point = convert(event.locationInWindow, from: nil)
      let cell = geometry(for: gridRect(ofPane: rect), fullWidth: fullWidth(forGrid: grid))
        .cellAt(x: Double(point.x), y: Double(point.y))
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

    // Scroll-over-toast exemption (OD8): a wheel gesture WHILE HOVERING the
    // visible toast scrolls the TOAST and is NOT forwarded to Neovim — so no
    // `Scroll` input reaches core and the toast is not dismissed. Every other
    // scroll (over the editor) falls through to the pane forwarding below, which
    // DOES send a `Scroll` and therefore dismisses the toast.
    if !toastView.isHidden, toastView.frame.contains(point) {
      let cellH = CGFloat(metrics.cellHeight)
      let dy =
        event.hasPreciseScrollingDeltas
        ? CGFloat(event.scrollingDeltaY)
        : CGFloat(event.scrollingDeltaY) * cellH
      toastView.scrollBy(deltaY: dy)
      return
    }

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
    let dy =
      event.hasPreciseScrollingDeltas
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

    let cell = geometry(for: gridRect(ofPane: rect), fullWidth: fullWidth(forGrid: grid))
      .cellAt(x: Double(point.x), y: Double(point.y))
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
    refreshOverlays()
    needsDisplay = true
  }

  override func viewDidEndLiveResize() {
    super.viewDidEndLiveResize()
    syncGridSizes()
    refreshOverlays()
    needsDisplay = true
  }

  /// Resizes the outer grid (grid 1) to the whole view and each bound pane's grid
  /// to its current rectangle — but only when a size actually differs from what
  /// Neovim currently has, so this converges (no resize feedback loop). The
  /// authoritative sizes still come back via core's `grid_resize`.
  private func syncGridSizes() {
    guard bounds.width > 0, bounds.height > 0 else { return }

    // Grid 1 (the outer/global grid) covers the whole view. Under `ext_messages`
    // it empties (`cmdheight=0`) and the command line / messages render via the
    // native overlays, but keeping it view-sized is harmless and preserves its
    // default-bg used for the editor padding fill.
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
      // actually paints into. Full-width panes lift the 90-column cap.
      let geo = geometry(for: gridRect(ofPane: rect), fullWidth: fullWidth(forGrid: grid))
      let frame = handle.read_frame_for_grid(grid)
      if Int(frame.cols()) != geo.cols || Int(frame.rows()) != geo.rows {
        handle.resize_grid(
          grid,
          UInt16(truncatingIfNeeded: geo.cols),
          UInt16(truncatingIfNeeded: geo.rows))
      }
    }

    // The divider lines moved with the panes, so the hover resize-cursor rects are
    // now stale — rebuild them on the next tracking pass.
    window?.invalidateCursorRects(for: self)
  }

  /// Registers a resize-cursor rect around each interior divider line so hovering
  /// a divider shows `.resizeLeftRight` (vertical divider) / `.resizeUpDown`
  /// (horizontal divider). Invalidated on every layout mutation (via
  /// `invalidateCursorRects(for:)` in `syncGridSizes`).
  override func resetCursorRects() {
    let tol = Self.dividerHitTolerance
    for divider in editorDividers() {
      let line = divider.lineRect
      let rect: CGRect
      let cursor: NSCursor
      switch divider.orientation {
      case .vertical:
        rect = CGRect(
          x: CGFloat(line.x) - tol, y: CGFloat(line.y),
          width: tol * 2, height: CGFloat(line.h))
        cursor = .resizeLeftRight
      case .horizontal:
        rect = CGRect(
          x: CGFloat(line.x), y: CGFloat(line.y) - tol,
          width: CGFloat(line.w), height: tol * 2)
        cursor = .resizeUpDown
      }
      addCursorRect(rect, cursor: cursor)
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
    let frameStart = FrameMetrics.begin()
    defer { FrameMetrics.end(.containerWake, frameStart) }

    // One-shot launch restore of a multi-pane layout: once this view's tabpage is
    // current AND its first pane has bound, realize the saved split tree in nvim
    // and rebuild the pane bindings. Gated like `pendingRefocusRebind`.
    if let saved = pendingLayoutRebuild,
      handle.active_tabpage() == controllerTabpageHandle
    {
      ensureInitialPane()
      if panes.isEmpty { return }  // no window grid yet; retry on the next wake.
      pendingLayoutRebuild = nil
      rebuildLayout(from: saved)
      applyTitlebarTheme()
      syncPreviewWebViews()
      refreshOverlays()
      return
    }

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
      refreshOverlays()
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
    refreshOverlays()
    needsDisplay = true
  }

  /// Shows the crash/stall overlay for `mode` ("Neovim crashed" / "Neovim stopped
  /// responding"). Kept ON TOP of the panes so a frozen last frame is covered.
  func showNvimOverlay(_ mode: NvimStatusMode) {
    nvimOverlay.removeFromSuperview()
    addSubview(nvimOverlay)
    nvimOverlay.show(mode: mode)
  }

  /// Hides the crash/stall overlay (liveness returned to `Alive`).
  func hideNvimOverlay() {
    nvimOverlay.hide()
  }

  /// Flashes the transient "Neovim was restarted" banner (auto-dismisses on its own
  /// timer). Independent of the liveness overlay — kept separate from
  /// `hideNvimOverlay()` so the Alive render wake never clears it.
  func flashRestartNotice() {
    restartBanner.flash()
  }

  /// Rebinds this container to a FRESH tabpage after a crash-restart, so the
  /// refocus gate (`active_tabpage() == controllerTabpageHandle`) matches the new
  /// nvim's tabpage. The next render wake repaints the rebound panes.
  func rebind(tabpageHandle: Int64) {
    controllerTabpageHandle = tabpageHandle
    pendingRefocusRebind = true
  }

  /// Refreshes the three bottom overlays from the live snapshot and lays them
  /// out (OD9 — overlay when active). Reads `handle.cmdline()` / `handle.pum()` /
  /// `handle.toast()` (the toast's chunk colors are already resolved in Rust),
  /// updates each subview, then positions the visible ones along the bottom of
  /// the editor area: the command line spans the bottom row, the completion popup
  /// sits directly above it anchored at the cmdline byte column, and the toast is
  /// pinned bottom-left (above the command line when it is active).
  private func refreshOverlays() {
    let outer = handle.read_frame_for_grid(Self.cmdlineGridId)
    let defaultFg = WolfColor.cgColor(outer.default_fg(), fallback: WolfColor.fallbackFg)
    let defaultBg = WolfColor.cgColor(outer.default_bg(), fallback: WolfColor.fallbackBg)

    let cmdline = handle.cmdline()
    let pum = handle.pum()
    let cmdlineActive = cmdlineView.update(cmdline, defaultFg: defaultFg, defaultBg: defaultBg)
    let pumVisible = completionPopupView.update(pum)
    let toastVisible = toastView.update(handle.toast(), defaultBg: defaultBg)

    guard bounds.width > 0, bounds.height > 0 else { return }

    // Command line: the bottom row, full width.
    let cmdlineHeight = cmdlineView.preferredHeight
    if cmdlineActive {
      cmdlineView.frame = CGRect(
        x: 0, y: bounds.height - cmdlineHeight, width: bounds.width, height: cmdlineHeight)
    }
    let cmdlineTop = cmdlineActive ? bounds.height - cmdlineHeight : bounds.height

    // Completion popup: directly ABOVE the command line, anchored at the byte
    // column when it is cmdline-anchored (`anchor_grid == -1`).
    if pumVisible {
      let popupHeight = completionPopupView.preferredHeight
      let popupWidth = min(completionPopupView.preferredWidth, bounds.width)
      // Shift the box left by its own text inset so the item text column lands
      // on the completed word's glyph in the command line, not the box border.
      let anchorX = pumAnchorX(pum: pum, cmdline: cmdline) - completionPopupView.leadingTextInset
      let x = max(0, min(anchorX, max(0, bounds.width - popupWidth)))
      completionPopupView.frame = CGRect(
        x: x, y: max(0, cmdlineTop - popupHeight), width: popupWidth, height: popupHeight)
    }

    // Toast: bottom-left, above the command line when it is active.
    if toastVisible {
      let toastHeight = toastView.preferredHeight
      let toastWidth = min(toastView.preferredWidth, bounds.width)
      toastView.frame = CGRect(
        x: 0, y: max(0, cmdlineTop - toastHeight), width: toastWidth, height: toastHeight)
    }
  }

  /// The x origin (points) for the completion popup: for a cmdline-anchored popup
  /// (`anchor_grid == -1`) it is the per-pane padding plus the rendered width of
  /// the `firstc` + the cmdline text up to the popup's byte column; otherwise it
  /// falls back to the padding (a window-grid-anchored popup is placed above the
  /// command line at the left edge in v1).
  ///
  /// The BARE `EditorInset.padding` is correct here and must NOT acquire a
  /// `PaneGeometry.paintOffsetX` term by analogy with `paintStatusbar`: the popup is
  /// positioned in the container view's own `bounds` against the CMDLINE — a
  /// bottom-spanning overlay across the whole view, not inside any pane — so there
  /// is no pane rect, grid rect or centering offset in scope. It already agrees with
  /// `CmdlineView`, which uses the same bare padding; adding an offset would BREAK
  /// that agreement rather than fix one.
  private func pumAnchorX(pum: FfiPum, cmdline: FfiCmdline) -> CGFloat {
    let padding = CGFloat(EditorInset.padding)
    guard pum.anchor_grid() == -1 else { return padding }
    let col = Int(pum.anchor_col())
    let prefix =
      cmdline.firstc().toString()
      + CmdlineSeam.bytePrefix(of: cmdline.text().toString(), byteCount: col)
    let attrs: [NSAttributedString.Key: Any] = [.font: overlayFont(metrics: metrics)]
    return padding + (prefix as NSString).size(withAttributes: attrs).width
  }

  /// Reconciles the hosted Markdown-preview `WKWebView`s with the core's current
  /// `previews()` snapshot (requirement 2 / OD1 = Design S).
  ///
  /// For every pane bound to a preview WINDOW (a window showing a
  /// `b:wolf_preview_source` scratch buffer), ensure a `WKWebView` framed to the
  /// pane's GRID rect, and reload it from `render_preview(source_bufnr)` whenever
  /// the source's `changedtick` advanced (the OD6 live-on-change gate; a keystroke
  /// burst is already coalesced into one tick-advancing wake). The pane's grid is
  /// recorded in `previewNamesByGrid` so `paintStatusbar` shows `Preview: <name>`
  /// and `drawScrollbars` suppresses the synthetic scrollbar. Web views for
  /// windows that are no longer previews (or closed) are removed.
  ///
  /// `web.frame` uses the capped+CENTERED `geometry(…, fullWidth: source_full_width())
  /// .gridRect` (the ~90-char default, or the full inset pane when the source is
  /// full-width), and this is what `paintStatusbar`'s `.preview` span AND the draw
  /// loop's occlusion test mirror: all three derive from the SAME
  /// `geometry(…, fullWidth:)` off `gridRect(ofPane:)`, so they must change together.
  ///
  /// ATOMICITY, LOAD-BEARING: this one function both rebuilds `previewNamesByGrid`
  /// (cleared and repopulated below) and sets the webview frames, in the SAME pass,
  /// and it runs BEFORE `needsDisplay = true` at the wake site — AppKit's `draw(_:)`
  /// runs later in the run loop, never synchronously inside the wake. So every
  /// painted frame sees a map and a webview SET written together: a pane can never
  /// be drawn with `.preview` alignment while it has no webview. Do NOT move the map
  /// rebuild out of this function, do NOT reorder the call after
  /// `needsDisplay = true`, and do NOT convert the `removeAll`-then-rebuild pattern
  /// to an incremental update.
  ///
  /// The guarantee covers the map and the webview set, NOT frame freshness. Repaints
  /// that do not run through a wake — `setFrameSize` and `applyMetrics`, which both
  /// set `needsDisplay = true` without calling this function — recompute the
  /// statusbar span from LIVE geometry on every `draw(_:)` while `web.frame` still
  /// holds the previous `gridRect`. So the statusbar is never stale; only the
  /// webview frame lags. `applyMetrics` is the sharper case: a font-size change
  /// moves both `cellWidth` (hence `cap` and `paintOffsetX`) and `statusbarHeight`
  /// (hence the grid rect's y/h) immediately while the webview stays put. Both
  /// converge on the next wake via `syncGridSizes` → `handle.resize`. This lag is
  /// pre-existing and orthogonal to the alignment fix above.
  private func syncPreviewWebViews() {
    previewNamesByGrid.removeAll(keepingCapacity: true)
    previewDisplayPathsByGrid.removeAll(keepingCapacity: true)
    previewFullWidthByGrid.removeAll(keepingCapacity: true)
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
      let sourceFullWidth = preview.source_full_width()
      previewFullWidthByGrid[grid] = sourceFullWidth

      // Copy the borrowed ref's scalars out into a `PreviewLoad`; only wins that
      // passed the pane/grid guard land in `current`, preserving the "only
      // previews with a bound pane arm the coalescer" behavior.
      current[win] = PreviewSeam.PreviewLoad(
        sourceBufnr: preview.source_bufnr(), sourceTick: preview.source_tick())

      // Frame the web view to the capped+CENTERED grid rect when the source is in
      // the ~90-char default, or the full inset pane when the source is full-width.
      // The occlusion test (draw loop) and the preview statusbar span derive from
      // this SAME `geometry(…, fullWidth:)` — they MUST change together.
      let gRect = geometry(for: gridRect(ofPane: entry.1), fullWidth: sourceFullWidth).gridRect
      let web = ensurePreviewWebView(for: win)
      web.frame = CGRect(
        x: gRect.x, y: gRect.y, width: gRect.w, height: gRect.h)
      web.isHidden = false
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

  /// Forces every bound preview to re-render when the effective appearance flips
  /// (dark↔light in `auto` mode). A code block's syntax colors come from nvim's
  /// colorscheme, which re-sources on the appearance change, but the per-window
  /// reload gate (`lastPreviewLoads`, keyed by `(sourceBufnr, sourceTick)`) does
  /// NOT change on an appearance flip — so without invalidating it the stale
  /// colors would linger. Clearing `lastPreviewLoads` makes
  /// `PreviewSeam.winsNeedingReload(current:, loaded:[:])` return every bound
  /// preview window (first-bind semantics), and the coalescer's debounce lets the
  /// synchronous `set_background` RPC (from the AppDelegate appearance KVO) reach
  /// nvim first, so the coalesced `render_preview` picks up the re-sourced colors.
  /// A forced dark/light appearance does not change the effective appearance on an
  /// OS toggle, so this correctly no-ops there.
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    lastPreviewLoads.removeAll()
    previewReloadCoalescer.request()
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
    let reloadStart = FrameMetrics.begin()
    defer { FrameMetrics.end(.previewReload, reloadStart) }

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
      // One sample per preview (explicit start/stop, not `defer`). Deliberately
      // includes the `.toString()` bridge copy — part of the FFI round-trip cost.
      let renderStart = FrameMetrics.begin()
      let html = handle.render_preview(load.sourceBufnr).toString()
      FrameMetrics.end(.previewRender, renderStart)
      // A `file://` baseURL from the win's source path lets relative links (and
      // #fragments) resolve against the real file location; unknown → nil
      // (unchanged OD/O4 behavior — no local-subresource loading is added).
      let path = previewSourcePaths[win] ?? ""
      let base = path.isEmpty ? nil : URL(fileURLWithPath: path)
      // LIMITATION: this measures ONLY the synchronous `loadHTMLString` call.
      // The WebKit parse / layout / paint it schedules happens asynchronously,
      // off this call stack, and is NOT included in the `prev.html` bucket.
      let htmlStart = FrameMetrics.begin()
      web.loadHTMLString(html, baseURL: base)
      FrameMetrics.end(.previewLoadHTML, htmlStart)
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

  /// Tears down the hosted web view + last-load entry for window
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
    lastPreviewLoads[win] = nil
    previewNavDelegates[win] = nil
    previewFocusHandlers[win] = nil
    previewSourcePaths[win] = nil
    pendingPreviewFragments[win] = nil
    // Dropping this is what restores grid painting IMMEDIATELY when a preview
    // closes: with no web view and no loaded-content flag, `previewOccluder`
    // returns nil and `isFullyOccluded` is `false` on the very next frame.
    previewLoadedWins.remove(win)
  }

  /// Samples the live preview web view for `win` into the headless
  /// `OccluderGeometry` the pure `PaneOcclusion` seam consumes, or `nil` when no
  /// web view exists for that window yet (the documented "web view does not exist
  /// yet" path — that pane's grid must paint).
  private func previewOccluder(forWin win: Int64) -> OccluderGeometry? {
    guard let web = previewWebViews[win] else { return nil }
    return OccluderGeometry(
      frame: rect(from: web.frame),
      // `isHiddenOrHasHiddenAncestor`, NOT the bare `isHidden`: a visible web
      // view inside a hidden ancestor covers nothing.
      isHidden: web.isHiddenOrHasHiddenAncestor,
      isAttached: web.superview === self && web.window != nil,
      alpha: Double(web.alphaValue),
      hasLoadedContent: previewLoadedWins.contains(win))
  }

  /// Converts a `CGRect` into the pure seam's `WolfKeyMap.Rect` (the inverse of
  /// the mapping `paneRects()` does inline).
  private func rect(from cgRect: CGRect) -> Rect {
    Rect(
      x: Double(cgRect.origin.x), y: Double(cgRect.origin.y),
      w: Double(cgRect.width), h: Double(cgRect.height))
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

  /// A preview web view finished loading: mark the window as having loaded
  /// content (so its pane's grid may be skipped as occluded), then, if a fragment
  /// was queued for `win` (a cross-file `other.md#anchor` navigation), scroll to
  /// it now and clear it.
  func previewDidFinishLoad(win: Int64) {
    // FIRST — the fragment guard below returns early in the common (no pending
    // fragment) case, which is every ordinary reload.
    previewLoadedWins.insert(win)

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

  // MARK: - Layout persistence (save + restore)

  /// The tab's persistable pane-split layout (topology + per-child weights +
  /// per-leaf buffer path). A lone pane persists its single leaf UNLESS it is an
  /// empty, non-preview buffer — in that empty-state case this returns `nil` so
  /// the tab stays layout-less. The drawer is excluded — it lives outside `tree`,
  /// so it never appears in `tree.layout()`.
  func layoutSnapshot() -> SavedPaneNode? {
    let (paths, previewIds) = leafPaths()
    let saved = PaneLayoutCodec.encode(tree.layout(), paths: paths, previews: previewIds)
    return saved.isEmptyLeaf ? nil : saved
  }

  /// Maps each editor leaf id to the ABSOLUTE path to persist plus the set of leaf
  /// ids that are Markdown PREVIEWS. A preview leaf (its bound window is in the
  /// `handle.previews()` snapshot, keyed by window like `syncPreviewWebViews`) maps
  /// to its SOURCE Markdown file's abs path so restore can recreate the preview via
  /// `wolf_preview_file`; a plain editor leaf maps to its bound buffer's abs path
  /// (via the bound grid → `pane_status_for_grid().abs_path()`). An unresolved /
  /// unnamed leaf maps to `""`.
  private func leafPaths() -> (paths: [PaneId: String], previewIds: Set<PaneId>) {
    // `previews()` yields borrowed refs valid only for this loop; snapshot the
    // preview-window → source-path map up front.
    var previewByWin: [Int64: String] = [:]
    for preview in handle.previews() {
      previewByWin[preview.win()] = preview.source_path().toString()
    }

    var paths: [PaneId: String] = [:]
    var previewIds: Set<PaneId> = []
    for id in tree.leaves() {
      if let win = panes[id]?.win, let source = previewByWin[win] {
        paths[id] = source
        previewIds.insert(id)
      } else if let grid = panes[id]?.boundGrid {
        paths[id] = handle.pane_status_for_grid(grid).abs_path().toString()
      }
    }
    return (paths, previewIds)
  }

  /// Requests that this view's pane layout be rebuilt from `saved` once its first
  /// pane binds and its tabpage is current (consumed once in `onRenderWake`).
  func requestLayoutRebuild(_ saved: SavedPaneNode) {
    pendingLayoutRebuild = saved
  }

  /// Replaces the pure layout tree wholesale (launch restore only).
  private func replaceTree(_ newTree: PaneTree) {
    tree = newTree
  }

  /// Realizes `saved` in nvim (its window topology) and rebuilds the pane bindings
  /// from the returned DFS-ordered handles, taking the tree's weights from
  /// `PaneTree.rebuilt`. On a handle-count mismatch it logs and falls back to the
  /// single already-bound pane (leaving `tree` untouched).
  private func rebuildLayout(from saved: SavedPaneNode) {
    let handles = Array(handle.rebuild_layout(PaneLayoutCodec.specJSON(saved)))
    guard handles.count == saved.leafCount else {
      NSLog(
        "wolf: layout rebuild handle count \(handles.count) != leaf count "
          + "\(saved.leafCount); falling back to a single pane")
      return
    }
    let (newTree, leafIds) = PaneTree.rebuilt(from: saved, firstLeafId: nextPaneId)
    replaceTree(newTree)
    nextPaneId += PaneId(leafIds.count)
    var rebound: [PaneId: PaneBinding] = [:]
    for (index, id) in leafIds.enumerated() {
      rebound[id] = PaneBinding(win: handles[index], boundGrid: nil)
    }
    panes = rebound
    drawerPaneId = nil
    setActivePane(leafIds.first)
    resolvePendingGrids()
    syncGridSizes()
    window?.invalidateCursorRects(for: self)
    needsDisplay = true
    onLayoutChanged?()
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
      case Self.intentKindDrawerOpen:
        applyDrawerOpen(win: intent.win())
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
    // A split fired from within the drawer is dropped front-end-side (FD2): the
    // drawer is not a tree leaf, so it cannot host an editor split.
    guard target != drawerPaneId else { return }
    let newId = allocPaneId()
    tree.split(target, newId, orientation)
    let grid = handle.grid_for_win(win)
    panes[newId] = PaneBinding(win: win, boundGrid: grid == 0 ? nil : grid)
    setActivePane(newId)
  }

  /// Applies a terminal-drawer open intent: bind a new pane to the drawer `win`
  /// and record it in `drawerPaneId` (NOT in `tree` — the drawer is rendered as a
  /// full-width, root-bottom pane outside the pure tree). Idempotent: a no-op once
  /// a drawer is bound or a pane already follows `win`.
  private func applyDrawerOpen(win: Int64) {
    if drawerPaneId != nil || paneForWin(win) != nil { return }
    let id = allocPaneId()
    drawerPaneId = id
    let grid = handle.grid_for_win(win)
    panes[id] = PaneBinding(win: win, boundGrid: grid == 0 ? nil : grid)
    setActivePane(id)
  }

  /// Applies a focus-direction intent, bridging editor <-> drawer since the drawer
  /// lives outside `tree`.
  ///
  /// - From the DRAWER: `.up` bridges to an editor pane via
  ///   `DrawerFocusSeam.drawerUpTarget` (the MRU editor leaf, else the bottom-most
  ///   leaf); the other directions are no-ops (the drawer is full-width).
  /// - From an EDITOR pane: resolve the `tree.neighbor`; on a nil neighbour going
  ///   `.down` into an open drawer, focus the drawer (bottom-edge bridge).
  ///
  /// Only `focus_win` is called — `updateActivePane` reconciles `activePane` on the
  /// next wake. The neighbour geometry MUST match `paneRects()`, i.e. it resolves
  /// over `DrawerLayout.editorHeight`, not the raw pane-area height.
  private func applyFocus(dir: FocusDir, from: Int64) {
    guard let origin = paneForWin(from) ?? activePane else { return }
    let width = Double(bounds.width)
    let dl = DrawerLayout(
      paneAreaHeight: Double(paneAreaHeight),
      hasDrawer: drawerPaneId != nil,
      weight: DrawerLayout.defaultWeight)

    if origin == drawerPaneId {
      guard dir == .up else { return }
      let rects = tree.paneRects(width: width, height: dl.editorHeight)
      guard
        let target = DrawerFocusSeam.drawerUpTarget(
          lastEditorPane: lastEditorPane, leaves: tree.leaves(), rects: rects),
        let win = panes[target]?.win
      else { return }
      handle.focus_win(win)
      return
    }

    if let target = tree.neighbor(origin, dir, width: width, height: dl.editorHeight),
      let win = panes[target]?.win
    {
      handle.focus_win(win)
    } else if dir == .down, let drawer = drawerPaneId, let win = panes[drawer]?.win {
      handle.focus_win(win)
    }
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

    // The drawer lives OUTSIDE the tree: when its grid dies (a `win_close` hide)
    // remove it from `panes` and clear `drawerPaneId` WITHOUT the last-leaf guard
    // or `tree.remove` (it is not a tree leaf).
    var closedDrawer: PaneId?
    if let drawer = drawerPaneId, closed.contains(drawer) {
      closedDrawer = drawer
      panes.removeValue(forKey: drawer)
      drawerPaneId = nil
    }

    for id in closed where id != closedDrawer {
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
    // Placed BEFORE the context guard so a no-context early return is still
    // counted; a near-zero sample is itself informative.
    let frameStart = FrameMetrics.begin()
    defer { FrameMetrics.end(.draw, frameStart) }

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
    for (id, paneRect) in rects {
      guard let binding = panes[id], let grid = binding.boundGrid else { continue }
      // Skip the grid paint (and its FFI frame read) when an opaque, loaded
      // preview web view fully covers this pane's grid rect — those pixels are
      // never visible. The containment test has tolerance 0.0 against the SAME
      // capped+centered `geometry(…, fullWidth:).gridRect` expression
      // `syncPreviewWebViews` assigns to the web view, so any out-of-sync geometry
      // (live resize, split, pre-close) fails containment and the grid paints. The
      // statusbar strip is OUTSIDE `gridRect` and always paints.
      let occluded = PaneOcclusion.isFullyOccluded(
        gridRect: geometry(
          for: gridRect(ofPane: paneRect), fullWidth: fullWidth(forGrid: grid)
        ).gridRect,
        by: previewOccluder(forWin: binding.win))
      // Explicit begin/end rather than `defer` so the sample covers exactly the
      // `paintPane` call and nothing else in the loop body (mirrors the
      // `previewRender` call site).
      let paneStart = FrameMetrics.begin()
      paintPane(
        grid: grid, paneRect: paneRect, active: id == activePane, paintsGrid: !occluded,
        ctx: ctx)
      FrameMetrics.end(
        previewNamesByGrid[grid] != nil ? .drawPanePreview : .drawPaneEditor, paneStart)
    }
    drawDividers(rects, ctx: ctx)
    // The command line / completion popup / toast are dedicated overlay
    // subviews (they paint themselves on top of this flat view); nothing is
    // painted for them here.
    drawScrollbars(rects, ctx: ctx)
  }

  /// Paints one whole pane: the top statusbar strip, then the window grid below
  /// it. The pane body is painted the SAME whether active or not — the only
  /// active/inactive treatment is in the statusbar text (bold + full-strength
  /// filename on the active pane, regular-weight + dimmed on an inactive one).
  ///
  /// `paintsGrid == false` means the grid rect is fully occluded by an opaque
  /// preview web view: the statusbar still paints (it sits OUTSIDE the grid rect
  /// and is never covered), but the grid is skipped entirely.
  ///
  /// Sub-buckets recorded here: `pane.status` (the statusbar strip, recorded even
  /// for an occluded pane) and `pane.read` (the `read_frame_for_grid` FFI call,
  /// deliberately BELOW the `paintsGrid` guard so an occluded pane records none).
  private func paintPane(
    grid: UInt64, paneRect: CGRect, active: Bool, paintsGrid: Bool, ctx: CGContext
  ) {
    ctx.saveGState()
    ctx.clip(to: paneRect)

    let statusbarStart = FrameMetrics.begin()
    paintStatusbar(
      into: statusbarRect(ofPane: paneRect), grid: grid, paneRect: paneRect,
      active: active, ctx: ctx)
    FrameMetrics.end(.paneStatusbar, statusbarStart)

    // Return BEFORE `read_frame_for_grid`: the FFI frame read (a whole cell
    // vector across the bridge, one Rust String clone per cell) is a material
    // part of the saving, so it must NOT be hoisted above this branch.
    //
    // This also skips `drawCursor`, which is only reachable from `paintGrid`.
    // That is CORRECT and not an oversight: an occluded pane is a preview pane,
    // and its cursor is already invisible under the opaque web view.
    guard paintsGrid else {
      ctx.restoreGState()
      return
    }

    // `pane.read` sits BELOW the `paintsGrid` guard on purpose: an occluded pane
    // must record NO sample, which is what preserves the occlusion semantics and
    // makes `count(pane.read) == count(draw.pane)` a meaningful check.
    let frameReadStart = FrameMetrics.begin()
    let frame = handle.read_frame_for_grid(grid)
    FrameMetrics.end(.paneFrameRead, frameReadStart)

    paintGrid(
      frame, into: gridRect(ofPane: paneRect), fullWidth: fullWidth(forGrid: grid), ctx: ctx)

    ctx.restoreGState()
  }

  /// Paints one window grid into the GRID rect `rect` (the pane minus its top
  /// statusbar strip).
  ///
  /// When the grid is capped narrower than the rect (`maxGridCols`, the ~90-char
  /// default) it is CENTERED via the geometry's paint offset; a `fullWidth` pane
  /// lifts the cap so the grid fills the rect. The whole grid rect is first filled
  /// with the grid's default background so the side margins blend with the grid.
  private func paintGrid(_ frame: FfiFrame, into rect: CGRect, fullWidth: Bool, ctx: CGContext) {
    let geo = geometry(for: rect, fullWidth: fullWidth)
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
    // `pane.cells` is timed at the CALL SITE, not with a `defer` inside
    // `paintCells`: `defer` ordering relative to the ARC release of a function's
    // locals is UNSPECIFIED in Swift, and this bucket exists precisely to capture
    // the `RustVec<FfiCell>` teardown that happens at that release.
    let cellsStart = FrameMetrics.begin()
    paintCells(frame, ctx: ctx)
    FrameMetrics.end(.paneCells, cellsStart)

    if frame.cursor_visible() {
      // MOST ERROR-PRONE PLACEMENT IN THIS INSTRUMENTATION: `begin()` MUST precede
      // the whole `drawCursor(...)` statement INCLUDING its arguments, because the
      // `frame.cells()` argument is a full deep clone of the cell vector, evaluated
      // here at the call site. A `defer` inside `drawCursor` would start the clock
      // AFTER that clone, report near-zero, and silently hide it.
      let cursorStart = FrameMetrics.begin()
      drawCursor(ctx, frame: frame, cells: frame.cells(), cols: Int(frame.cols()))
      FrameMetrics.end(.paneCursor, cursorStart)
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
  /// bg / editor fallback when there is no window.
  ///
  /// The text span comes from `WolfKeyMap.StatusbarTextSpan`, which aligns the text
  /// with the pane's CONTENT — and the two kinds of content start at different x
  /// origins:
  ///
  /// - EDITOR panes: the grid is painted at `gRect.minX + PaneGeometry.paintOffsetX`
  ///   (the 90-column centering inset), so the text is indented by the same offset
  ///   and its right edge stays at `gRect.maxX`. This is bit-identical to the
  ///   pre-existing behavior.
  /// - PREVIEW panes: `syncPreviewWebViews` now frames the `WKWebView` to the SAME
  ///   capped+CENTERED `geometry(…, fullWidth:).gridRect`, so a preview's statusbar is
  ///   indented by `paintOffsetX` exactly like an editor's (`StatusbarTextSpan`'s two
  ///   kinds are now identical). In full-width mode `paintOffsetX == 0`, so the text
  ///   lands flush at `gRect.minX`. The webview frame, this span, and the draw loop's
  ///   occlusion test all derive from the same `geometry(…, fullWidth:)` and must
  ///   change together.
  ///
  /// The kind comes from `previewNamesByGrid` (looked up ONCE, and reused for the
  /// filename/path override below) — the same predicate `drawScrollbars` uses.
  /// Text is truncated with a tail ellipsis; an unnamed buffer shows `[No Name]`
  /// with no path line.
  private func paintStatusbar(
    into strip: CGRect, grid: UInt64, paneRect: CGRect, active: Bool, ctx: CGContext
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

    // ONE preview lookup for the whole function: it decides both the text span's
    // pane kind (below) and the filename/path source (further down), so "is this a
    // preview" has a single source of truth per paint.
    let previewName = previewNamesByGrid[grid]

    // The text span is based on the INSET grid rect (not the full pane rect), so
    // the statusbar text sits inside the per-pane padding, flush with the pane's
    // CONTENT — which differs by kind: an editor's grid is painted at
    // `paintOffsetX`, a preview's webview is framed to the uncentered `gRect`.
    // `StatusbarTextSpan` owns that arithmetic.
    let gRect = gridRect(ofPane: paneRect)
    let geo = geometry(for: gRect, fullWidth: fullWidth(forGrid: grid))
    let span = StatusbarTextSpan(
      gridLeft: Double(gRect.minX), gridWidth: Double(gRect.width),
      paintOffsetX: geo.paintOffsetX, kind: previewName != nil ? .preview : .editor)
    let textLeft = CGFloat(span.left)
    let textWidth = CGFloat(span.width)

    let layout = statusbarLayout
    // A preview pane's scratch buffer is SKIPPED by the path-based trackers, so
    // its pane_status is "[No Name]"; override it with `Preview: <basename>`
    // (from the core's previews() snapshot) and the source's cwd/home-relative
    // display path on the secondary line.
    let filename: String
    let displayPath: String
    if let name = previewName {
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
    // alpha), else AppKit's label / secondary-label system colors. An INACTIVE
    // pane dims BOTH lines — the themed alphas are scaled by
    // `statusbarInactiveAlpha`, the non-themed case steps down one system
    // label tier (label → secondary, secondary → tertiary).
    let titlebarFg = handle.theme_titlebar_fg()
    let primaryColor: NSColor
    let secondaryColor: NSColor
    if titlebarFg.set {
      let themed = NSColor(cgColor: WolfColor.cgColor(packed: titlebarFg.rgb)) ?? .labelColor
      let primaryAlpha: CGFloat = active ? 1 : Self.statusbarInactiveAlpha
      let secondaryAlpha =
        Self.statusbarSecondaryThemedAlpha * (active ? 1 : Self.statusbarInactiveAlpha)
      primaryColor = themed.withAlphaComponent(primaryAlpha)
      secondaryColor = themed.withAlphaComponent(secondaryAlpha)
    } else {
      primaryColor = active ? .labelColor : .secondaryLabelColor
      secondaryColor = active ? .secondaryLabelColor : .tertiaryLabelColor
    }

    // The active pane's filename is BOLD; an inactive pane's is regular weight.
    let primaryFont: NSFont =
      active
      ? .boldSystemFont(ofSize: CGFloat(statusbarPrimaryFontSize))
      : .systemFont(ofSize: CGFloat(statusbarPrimaryFontSize))

    let primaryRect = CGRect(
      x: textLeft, y: strip.minY + CGFloat(layout.primaryLineTop),
      width: textWidth, height: CGFloat(layout.primaryLineHeight))
    drawStatusbarText(filename, in: primaryRect, font: primaryFont, color: primaryColor)

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

  /// Paints all cells of `frame` at grid-local coordinates (the caller has already
  /// translated/clipped the context to the target rect).
  ///
  /// Records two sub-buckets: `pane.vec` (the `frame.cells()` deep clone alone)
  /// and `pane.glyphs` (the double loop alone). Both are nested INSIDE the
  /// caller's `pane.cells` bucket and do not account for all of it — the
  /// difference is the `RustVec<FfiCell>` teardown at ARC release.
  private func paintCells(_ frame: FfiFrame, ctx: CGContext) {
    let cols = Int(frame.cols())
    let rows = Int(frame.rows())
    let cellsVecStart = FrameMetrics.begin()
    let cells = frame.cells()
    FrameMetrics.end(.paneCellsVec, cellsVecStart)
    let cellW = metrics.cellWidth
    let cellH = metrics.cellHeight

    // NOT a `defer`: a `defer` registered here would fire at function exit and
    // swallow part of the teardown, destroying the very separation `pane.cells`
    // exists to expose. No timer goes inside a loop body.
    let glyphLoopStart = FrameMetrics.begin()
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
    FrameMetrics.end(.paneGlyphLoop, glyphLoopStart)
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

  /// Sets the active pane (the one with the bold statusbar filename) and requests a
  /// redraw.
  ///
  /// Also records the most-recently-active EDITOR pane (a live tree leaf, never the
  /// drawer) into `lastEditorPane`, which the `<C-w>k` drawer -> up bridge targets.
  /// All focus changes funnel through here, so this one update covers them.
  private func setActivePane(_ pane: PaneId?) {
    if let pane, pane != drawerPaneId, tree.leaves().contains(pane) {
      lastEditorPane = pane
    }
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
