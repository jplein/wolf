// Pure seams for the per-pane navigation history menu and toolbar buttons.

import Foundation

/// The pure decision functions + constants backing the macOS History menu and its
/// two toolbar buttons. A namespace (`enum` with only static members) mirroring
/// `PickerSeam` / `RestoreSeam`.
public enum HistorySeam {

  /// One History-menu row: a finished display label, the STACK index to hand back
  /// to `history_jump`, and whether it draws the checkmark.
  public struct Row: Equatable {
    /// The disambiguated label, drawn verbatim (core already home-abbreviates).
    public let title: String
    /// The STACK index this row addresses. For the checkmarked row this is the
    /// cursor's own index, so activating it is a true no-op.
    public let index: Int
    /// Whether this row draws the checkmark.
    public let isCurrent: Bool

    /// Memberwise initializer (made explicit so it is `public`).
    public init(title: String, index: Int, isCurrent: Bool) {
      self.title = title
      self.index = index
      self.isCurrent = isCurrent
    }
  }

  /// The disabled placeholder title used so the History menu is never EMPTY —
  /// before the first rebuild, and for a pane with no history.
  public static let emptyPlaceholderTitle = "No History"

  /// The Back toolbar item's `NSToolbarItem.Identifier` raw value.
  public static let backIdentifier = "com.wolf.editor.toolbar.back"

  /// The Forward toolbar item's `NSToolbarItem.Identifier` raw value.
  public static let forwardIdentifier = "com.wolf.editor.toolbar.forward"

  /// The Back button's SF Symbol. `chevron.backward` (NOT `chevron.left`) because
  /// it is RTL-aware, and an SF Symbol rather than `NSImage.Name.goBackTemplate`
  /// because the toolbar is already SF-Symbol-styled `.iconOnly` unified.
  public static let backSymbolName = "chevron.backward"

  /// The Forward button's SF Symbol. RTL-aware counterpart of
  /// ``backSymbolName``.
  public static let forwardSymbolName = "chevron.forward"

  /// Zips the three parallel FFI-derived arrays into menu rows.
  ///
  /// Core hands them over NEWEST-FIRST and this preserves that order verbatim — it
  /// never re-sorts, and it never renumbers: a non-negative `index` rides through
  /// unchanged because it is a STACK index handed straight back to `history_jump`.
  ///
  /// - Mismatched lengths TRUNCATE to the shortest array.
  /// - A NEGATIVE index is DROPPED (it can never address a stack slot).
  /// - At most ONE row keeps `isCurrent`; if several arrive, the FIRST wins.
  /// - Empty input yields `[]` (the caller substitutes
  ///   ``emptyPlaceholderTitle``).
  public static func rows(
    displays: [String],
    indices: [Int],
    currentFlags: [Bool]
  ) -> [Row] {
    let count = min(displays.count, indices.count, currentFlags.count)
    var result: [Row] = []
    result.reserveCapacity(count)
    var currentTaken = false
    for i in 0..<count {
      let index = indices[i]
      guard index >= 0 else { continue }
      let isCurrent = currentFlags[i] && !currentTaken
      if isCurrent { currentTaken = true }
      result.append(Row(title: displays[i], index: index, isCurrent: isCurrent))
    }
    return result
  }

  /// The LOCKED toolbar ordering, after the traffic lights: the sidebar toggle,
  /// the sidebar tracking separator, then Back and Forward.
  ///
  /// Returned as raw identifier strings so this stays AppKit-free; the caller maps
  /// them back to `NSToolbarItem.Identifier`.
  public static func itemIdentifiers(
    toggleSidebar: String,
    trackingSeparator: String
  ) -> [String] {
    [toggleSidebar, trackingSeparator, backIdentifier, forwardIdentifier]
  }
}
