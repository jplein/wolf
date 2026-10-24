// A file-agnostic value type describing one row of a picker list.

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
