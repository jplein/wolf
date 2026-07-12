//
//  PreviewSeam.swift
//  Pure, AppKit-free Markdown-preview placement seam.
//
//  The sibling of `CommandSeam` / `FontSizeSeam`: a pure, display-free decision
//  exercised with scalar inputs only — NO AppKit, NO WKWebView, NO live Neovim. It
//  decides where a Preview command opens its pane from the window's pane count, and
//  which preview WINDOWS need their web view reloaded by comparing each window's
//  last-loaded `(sourceBufnr, sourceTick)` against the live one.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap` module
//  alongside `CommandSeam.swift` / `PaneTree.swift`.
//

import Foundation

/// Pure Markdown-preview placement + reload arithmetic.
public enum PreviewSeam {
    /// The `mode` string threaded into core's `wolf_open_preview(mode)` for a
    /// window with `paneCount` panes (requirement 4 / OD3). Three modes:
    ///
    ///  - `"split_right"` (when `paneCount <= 1`, unconditional): open the preview
    ///    in a NEW pane to the right (a vertical split, reusing the `:vsplit`
    ///    `wolf_layout` intent path). A degenerate/empty count (`0`) falls here.
    ///  - `"reuse_preview_pane"` (when `paneCount == 2`, the active pane is NOT
    ///    itself a preview, and EXACTLY one OTHER pane is a preview): bind the
    ///    active source's preview buffer into that OTHER (preview) window rather
    ///    than clobbering the active editor pane.
    ///  - `"active"` (every other case — `paneCount == 2` with the reuse condition
    ///    unmet, and every `paneCount > 2`): replace the ACTIVE pane's buffer with
    ///    the preview (a bare `nvim_win_set_buf`).
    ///
    /// - Parameters:
    ///   - paneCount: The window's live pane count.
    ///   - activePaneIsPreview: Whether the ACTIVE pane is itself a preview pane;
    ///     when true the reuse case is declined (defensive) so a preview is never
    ///     opened from within a preview pane. Defaults to `false`.
    ///   - otherPreviewPaneCount: How many panes OTHER than the active one are
    ///     preview panes. The reuse case fires only when this is exactly `1`.
    ///     Defaults to `0`.
    public static func placementMode(
        paneCount: Int,
        activePaneIsPreview: Bool = false,
        otherPreviewPaneCount: Int = 0
    ) -> String {
        if paneCount <= 1 {
            return "split_right"
        }
        if paneCount == 2, !activePaneIsPreview, otherPreviewPaneCount == 1 {
            return "reuse_preview_pane"
        }
        return "active"
    }

    /// The `(sourceBufnr, sourceTick)` a preview WINDOW's web view was last loaded
    /// at. A struct (not a tuple) so `Optional<PreviewLoad>` inequality compiles,
    /// letting `winsNeedingReload` compare a window's live load against its last one.
    public struct PreviewLoad: Equatable {
        public let sourceBufnr: Int64
        public let sourceTick: UInt64
        public init(sourceBufnr: Int64, sourceTick: UInt64) {
            self.sourceBufnr = sourceBufnr
            self.sourceTick = sourceTick
        }
    }

    /// The preview WINDOW handles whose bound source buffer OR tick differs from what
    /// was last loaded: those in `current` (win → live `PreviewLoad`) whose entry is
    /// absent from — or unequal to — `loaded` (win → last-loaded `PreviewLoad`). A
    /// window is returned on:
    ///
    ///  - first bind (no `loaded` entry for the win),
    ///  - a changed `sourceBufnr` — a REUSED preview pane switching sources, even at
    ///    an equal `sourceTick` (the bug this seam fixes: two fresh files share a
    ///    `changedtick`, so a tick-only gate would suppress the reload),
    ///  - an advanced `sourceTick` (a live edit of the same source).
    ///
    /// A window whose live `PreviewLoad` equals its last one is NOT returned (no
    /// redundant blocking `render_preview`). Windows in `loaded` but absent from
    /// `current` (dropped previews) are never returned. Order is unspecified.
    ///
    /// A pure helper so the front-end's debounced reload gate is headlessly testable;
    /// the AppKit `PaneContainerView` keeps the per-window `loaded` map and calls
    /// `render_preview` for each returned window's source buffer.
    public static func winsNeedingReload(
        current: [Int64: PreviewLoad],
        loaded: [Int64: PreviewLoad]
    ) -> [Int64] {
        current
            .filter { win, load in loaded[win] != load }
            .map(\.key)
    }
}
