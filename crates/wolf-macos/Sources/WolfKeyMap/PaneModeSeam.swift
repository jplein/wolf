//
//  PaneModeSeam.swift
//  Pure, AppKit-free per-pane mode decision.
//
//  The sibling of `CommandSeam` / `PreviewSeam`: a pure, display-free decision
//  exercised with scalar inputs only — NO AppKit, NO WKWebView, NO live Neovim.
//  Each pane is in PREVIEW mode or EDIT mode, derived — never stored — from
//  whether the pane's nvim window appears in the `previews()` snapshot. The
//  Preview and Edit commands always act IN PLACE on the active pane, so the only
//  front-end question left is which action a picker/open-dialog activation routes
//  to (preview mode → `preview_file` / `preview_buffer`; edit mode → `open` /
//  `switch_buffer`).
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap`
//  module alongside `PreviewSeam.swift` / `CommandSeam.swift`.
//

import Foundation

/// A pane's mode: editing a buffer, or previewing a Markdown source.
public enum PaneMode {
  case edit
  case preview
}

/// The pure per-pane mode decision.
public enum PaneModeSeam {
  /// The mode of the active pane. `activeWin` is the active pane's nvim window
  /// handle (`0` when unknown / no pane is bound yet); `previewWins` is the set
  /// of window handles currently carrying a preview (the keys of the
  /// `previews()` snapshot). The active pane is in preview mode iff its OWN
  /// window is in the set; `activeWin == 0` is always edit mode.
  public static func mode(activeWin: Int64, previewWins: Set<Int64>) -> PaneMode {
    if activeWin != 0, previewWins.contains(activeWin) {
      return .preview
    }
    return .edit
  }
}
