//
//  MainWindow.swift
//  The single editor window. Its content is an `EditorSplitViewController`
//  (an `NSSplitViewController`) with a `.sidebar` source-list split item for the
//  file-tree sidebar and a content split item for the editor grid, under a
//  transparent full-size-content-view titlebar. The sidebar's vibrancy extends
//  full-height under the titlebar; the editor pane is pinned below the titlebar
//  so it keeps its exact `cols` × `rows` cell size.
//

import AppKit
import WolfKeyMap

/// The main "Wolf" window. Its `contentViewController` is an
/// `EditorSplitViewController`; the window uses a transparent
/// `.fullSizeContentView` titlebar.
final class MainWindow: NSWindow {
    /// Fixed initial width of the file-tree sidebar pane in points.
    static let treePaneWidth: CGFloat = 240

    /// The window's pristine `backgroundColor`, captured on the FIRST themed
    /// titlebar application so an unset `WolfTitlebar` can restore the exact
    /// system default. `nil` until a theme color has ever been applied.
    private var defaultBackgroundColor: NSColor?

    /// The content split-view controller, retained so the sidebar split item is
    /// reachable for state restore/persistence.
    private let splitViewController: EditorSplitViewController

    /// The sidebar `NSSplitViewItem`, exposed so state restore can set — and
    /// `StatePersistence` can observe — `isCollapsed`.
    var sidebarItem: NSSplitViewItem {
        splitViewController.sidebarItem
    }

    /// Builds the window around `treeView` (sidebar) and `paneContainer` (editor),
    /// sizing the editor pane to `cols` × `rows` cells with the cmdline strip as
    /// extra height below it.
    convenience init(
        treeView: FileTreeView, paneContainer: PaneContainerView, metrics: CellMetrics,
        cols: Int, rows: Int
    ) {
        let splitVC = EditorSplitViewController(treeView: treeView, paneContainer: paneContainer)
        // The editor content is inset by a uniform 20pt margin on all four sides
        // (`EditorInset.padding`), so the initial content must grow by 2*padding in
        // each dimension to keep the pane area at exactly cols × rows cells (the
        // 80-col OCR width is preserved) inside that margin.
        let padding = CGFloat(EditorInset.padding)
        let editorWidth = CGFloat(cols) * metrics.cellWidth + 2 * padding
        // The pane area stays exactly cols × rows cells; the one-row cmdline strip is
        // extra height beneath it, and the 20pt margin surrounds the whole region.
        let cmdlineStripHeight = ceil(metrics.cellHeight)
        let editorHeight = CGFloat(rows) * metrics.cellHeight + cmdlineStripHeight + 2 * padding
        let divider = splitVC.splitView.dividerThickness

        // An initial content rect; the exact size (with chrome) is set below once
        // the content VC is attached and the titlebar height is known.
        let contentRect = NSRect(
            x: 0, y: 0,
            width: Self.treePaneWidth + divider + editorWidth,
            height: editorHeight
        )

        self.init(
            splitVC: splitVC,
            contentRect: contentRect,
            editorWidth: editorWidth,
            editorHeight: editorHeight,
            divider: divider,
            paneContainer: paneContainer
        )
    }

    /// Designated initializer: wires the split VC and transparent titlebar, then
    /// sizes the content so the editor pane keeps its exact cell size below the
    /// titlebar chrome.
    private init(
        splitVC: EditorSplitViewController,
        contentRect: NSRect,
        editorWidth: CGFloat,
        editorHeight: CGFloat,
        divider: CGFloat,
        paneContainer: PaneContainerView
    ) {
        splitViewController = splitVC

        super.init(
            contentRect: contentRect,
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )

        // ARC owns this window (the `WolfWindowController` holds the strong
        // reference). AppKit's default `isReleasedWhenClosed == true` would add an
        // unbalanced release on close, over-releasing the window — the deferred
        // close-animation dealloc then crashes. Let ARC be the sole owner.
        isReleasedWhenClosed = false

        title = "Wolf"
        titleVisibility = .visible
        titlebarAppearsTransparent = true

        contentViewController = splitVC

        // Size the content so the editor pane is exactly `editorWidth` ×
        // `editorHeight` BELOW the titlebar chrome. `contentLayoutRect` is
        // the area not obscured by the (now-attached) titlebar, so the
        // chrome height is the difference from the full content height.
        let fullHeight = contentView?.bounds.height ?? editorHeight
        let chrome = fullHeight - contentLayoutRect.height
        setContentSize(
            NSSize(
                width: Self.treePaneWidth + divider + editorWidth,
                height: editorHeight + chrome
            )
        )

        // Pin the divider so the sidebar pane is exactly `treePaneWidth` wide.
        splitVC.pinDivider()

        initialFirstResponder = paneContainer
    }

    /// Applies (or reverts) the `WolfTitlebar` theme background.
    ///
    /// The window uses `.fullSizeContentView` + `titlebarAppearsTransparent`,
    /// so `backgroundColor` shows through the titlebar over the editor pane
    /// (the sidebar keeps its own vibrancy material) — setting it is the
    /// minimal, reversible lever for a themed titlebar. On the first themed
    /// application the pristine `backgroundColor` is captured; `set == false`
    /// (group absent or cleared) restores that captured default. Idempotent:
    /// re-applying the same state is harmless.
    func applyTitlebarBackground(rgb: UInt32, set: Bool) {
        if set {
            if defaultBackgroundColor == nil {
                defaultBackgroundColor = backgroundColor
            }
            backgroundColor = NSColor(
                srgbRed: CGFloat((rgb >> 16) & 0xFF) / 255,
                green: CGFloat((rgb >> 8) & 0xFF) / 255,
                blue: CGFloat(rgb & 0xFF) / 255,
                alpha: 1
            )
        } else if let defaultBackgroundColor {
            backgroundColor = defaultBackgroundColor
        }
    }
}
