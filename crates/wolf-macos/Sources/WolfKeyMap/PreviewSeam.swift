//
//  PreviewSeam.swift
//  Pure, AppKit-free Markdown-preview placement seam.
//
//  The sibling of `CommandSeam` / `FontSizeSeam`: a pure, display-free decision
//  exercised with scalar inputs only — NO AppKit, NO WKWebView, NO live Neovim. It
//  decides where a Preview command opens its pane from the window's pane count, and
//  (optionally) which preview panes need their web view reloaded from a tick map.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap` module
//  alongside `CommandSeam.swift` / `PaneTree.swift`.
//

import Foundation

/// Pure Markdown-preview placement + reload arithmetic.
public enum PreviewSeam {
    /// The `mode` string threaded into core's `wolf_open_preview(mode)` for a
    /// window with `paneCount` panes (requirement 4 / OD3):
    ///
    ///  - `<= 1` pane → `"split_right"`: open the preview in a NEW pane to the
    ///    right (a vertical split, reusing the `:vsplit` `wolf_layout` intent path).
    ///  - `> 1` pane → `"active"`: replace the ACTIVE pane's buffer with the
    ///    preview (a bare `nvim_win_set_buf`).
    ///
    /// A degenerate/empty count (`0`) falls to the split-right branch.
    public static func placementMode(paneCount: Int) -> String {
        paneCount <= 1 ? "split_right" : "active"
    }

    /// The source buffer numbers whose preview web view needs reloading: those in
    /// `current` (source-bufnr → live `source_tick`) whose tick differs from the
    /// `lastTicks` the front-end last loaded (a new source, or an advanced tick).
    ///
    /// A pure helper so the front-end's debounced "reload when the tick advanced"
    /// gate is headlessly testable; the AppKit `PaneContainerView` keeps the
    /// `lastTicks` map and calls `render_preview` for each returned bufnr.
    public static func previewsNeedingReload(
        current: [Int64: UInt64],
        lastTicks: [Int64: UInt64]
    ) -> [Int64] {
        current
            .filter { bufnr, tick in lastTicks[bufnr] != tick }
            .map(\.key)
    }
}
