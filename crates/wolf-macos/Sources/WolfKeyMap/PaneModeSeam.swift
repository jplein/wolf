// Pure per-pane preview-or-edit mode decision.

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
