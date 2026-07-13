//
//  PickerRow.swift
//  A file-agnostic value type describing one row of the picker list.
//
//  The picker's row/cell rendering is deliberately GENERAL so the same panel can
//  back other selectors (e.g. a future buffer selector): a `PickerRow` carries a
//  medium icon, a single-line title, and an OPAQUE activation `payload`. Only
//  `PickerView.activateSelection()` interprets the payload (for files it is the
//  absolute path passed to `WolfCoreHandle.open`); a buffer selector would set
//  title = the buffer's path, payload = a buffer id/token. AppKit-only (holds an
//  `NSImage`), so manual-smoke only.
//

import AppKit

/// One row in the picker list, independent of what is being picked.
struct PickerRow {
  /// The row's leading icon (a medium Finder icon for files); `nil` draws none.
  let icon: NSImage?
  /// The single line of text (for files, the path relative to the working
  /// directory root — Neovim users expect to see paths, so the whole relative
  /// path is shown, not just the base filename).
  let title: String
  /// The opaque activation token consumed only by `activateSelection()` (the
  /// absolute path for files).
  let payload: String
}
