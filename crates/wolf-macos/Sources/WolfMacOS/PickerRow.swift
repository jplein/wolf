//
//  PickerRow.swift
//  A file-agnostic value type describing one row of the picker list.
//
//  The picker's row/cell rendering is deliberately GENERAL so the same panel can
//  back other selectors (e.g. a future buffer selector): a `PickerRow` carries a
//  medium icon, a primary title, a secondary subtitle, and an OPAQUE activation
//  `payload`. Only `PickerView.activateSelection()` interprets the payload (for
//  files it is the absolute path passed to `WolfCoreHandle.open`); a buffer
//  selector would set title = buffer name, subtitle = detail, payload = a buffer
//  id/token. AppKit-only (holds an `NSImage`), so manual-smoke only.
//

import AppKit

/// One row in the picker list, independent of what is being picked.
struct PickerRow {
    /// The row's leading icon (a medium Finder icon for files); `nil` draws none.
    let icon: NSImage?
    /// The primary line (the base filename for files).
    let title: String
    /// The secondary line (the relative parent directory for files); may be "".
    let subtitle: String
    /// The opaque activation token consumed only by `activateSelection()` (the
    /// absolute path for files).
    let payload: String
}
