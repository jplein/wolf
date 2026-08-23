//
//  SidebarRowStyle.swift
//  Pure, AppKit-free choices behind the file-tree sidebar's current-file mark:
//  which file is marked, and the color one row's label is drawn in.
//
//  The sidebar marks the current buffer with a COLOR, and that color must not be
//  a side effect of the row's SELECTION. AppKit tints a selected source-list
//  label with the accent color only while the row view is emphasized — a key
//  window whose first responder is the outline — and the sidebar outline never
//  becomes first responder, so a selection tint holds only until the first app
//  switch and never comes back. Resolving the color here, from the current-buffer
//  flag alone, makes the mark independent of window focus.
//
//  The one case that still defers to AppKit is an emphasized selection, whose
//  fill is the accent color itself: accent text on an accent fill is unreadable.
//

import Foundation

/// The color one sidebar row's label is drawn in.
public enum SidebarLabelColor: Equatable {
  /// The ordinary label color.
  case normal
  /// The accent color, marking the row whose file is the current buffer.
  case current
  /// The contrasting color AppKit's emphasized selection fill demands.
  case selected
}

/// Resolves a sidebar row's label color from the row's state.
public enum SidebarRowStyle {
  /// Picks the label color for a row: an emphasized (accent-filled) selection
  /// wins over the current-buffer mark, because accent text on an accent fill is
  /// unreadable; otherwise the current buffer is accented and every other row is
  /// plain.
  public static func labelColor(isCurrent: Bool, isEmphasized: Bool) -> SidebarLabelColor {
    if isEmphasized {
      return .selected
    }
    return isCurrent ? .current : .normal
  }

  /// The absolute path the sidebar marks, or "" for none: the file the ACTIVE
  /// pane is previewing when it is previewing one, else the edited buffer.
  ///
  /// A preview pane's scratch buffer is unnamed, and nvim's `wolf_current_file`
  /// reports "" for it, so `currentFile` alone cannot name what a preview shows.
  /// `previewSource` is the front-end's per-pane answer, and an empty one falls
  /// back rather than clearing the mark.
  public static func markedFile(previewSource: String?, currentFile: String) -> String {
    if let previewSource, !previewSource.isEmpty {
      return previewSource
    }
    return currentFile
  }
}
