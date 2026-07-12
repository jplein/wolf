//
//  EditSeam.swift
//  Pure, AppKit-free source-editor placement seam.
//
//  The INVERSE of `PreviewSeam`: a pure, display-free decision exercised with
//  scalar inputs only — NO AppKit, NO WKWebView, NO live Neovim. It decides where
//  the Edit command lands the SOURCE editor when fired from a preview pane, from the
//  window's pane count and whether the OTHER pane is an editor.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap` module
//  alongside `PreviewSeam.swift` / `CommandSeam.swift`.
//

import Foundation

/// Pure source-editor placement arithmetic for the Edit command.
public enum EditSeam {
    /// The `mode` string threaded into core's `wolf_edit_source(mode)` for a window
    /// with `paneCount` panes. Three modes (the inverse of `PreviewSeam`):
    ///
    ///  - `"split_left"` (when `paneCount <= 1`, unconditional): the preview is the
    ///    ONLY pane, so open the source editor in a NEW pane to the LEFT (a vertical
    ///    split, reusing the `:vsplit` `wolf_layout` intent path; the editor lands
    ///    left, the preview stays right). A degenerate/empty count (`0`) falls here.
    ///  - `"reuse_editor_pane"` (when `paneCount == 2`, the active pane IS a preview,
    ///    and EXACTLY one OTHER pane is an editor): bind the source into that OTHER
    ///    (editor) window and focus it rather than clobbering the preview pane.
    ///  - `"active"` (every other case — `paneCount == 2` with the reuse condition
    ///    unmet, the active pane NOT a preview, and every `paneCount > 2`): replace
    ///    the preview in the ACTIVE pane with the source editor (a bare
    ///    `nvim_win_set_buf`).
    ///
    /// - Parameters:
    ///   - paneCount: The window's live pane count.
    ///   - activePaneIsPreview: Whether the ACTIVE pane is itself a preview pane.
    ///     Edit is only meaningful FROM a preview pane, so this defaults to `true`;
    ///     when false the reuse case is declined (defensive) → `"active"`.
    ///   - otherEditorPaneCount: How many panes OTHER than the active one are
    ///     editors (non-preview). The reuse case fires only when this is exactly `1`.
    ///     Defaults to `0`.
    public static func placementMode(
        paneCount: Int,
        activePaneIsPreview: Bool = true,
        otherEditorPaneCount: Int = 0
    ) -> String {
        if paneCount <= 1 {
            return "split_left"
        }
        if paneCount == 2, activePaneIsPreview, otherEditorPaneCount == 1 {
            return "reuse_editor_pane"
        }
        return "active"
    }
}
