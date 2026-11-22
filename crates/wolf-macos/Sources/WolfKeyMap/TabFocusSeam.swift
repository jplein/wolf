// Pure, AppKit-free tab focus rule: which tab is focused and the edges a change reports.

import Foundation

/// Which tab is focused, and the focus edges a change reports to the tabs' Neovims. A namespace
/// (`enum` with only static members), mirroring the other seams.
public enum TabFocusSeam {
  /// One focus change for one tab: `gained` is `true` for a gain, `false` for a loss.
  public struct Edge: Equatable {
    /// The tab's identifier.
    public let tab: Int
    /// Whether the tab gained focus (`true`) or lost it (`false`).
    public let gained: Bool

    public init(tab: Int, gained: Bool) {
      self.tab = tab
      self.gained = gained
    }
  }

  /// The focused tab: the main window's tab while the app is active, else none.
  ///
  /// Main rather than key, so a picker panel taking key changes no tab's focus.
  public static func focusedTab(appActive: Bool, mainTab: Int?) -> Int? {
    appActive ? mainTab : nil
  }

  /// The edges a move of focus from `previous` to `next` reports: none when they are equal, else
  /// the loss of `previous`, if any, then the gain of `next`, if any.
  public static func edges(from previous: Int?, to next: Int?) -> [Edge] {
    if previous == next {
      return []
    }
    var result: [Edge] = []
    if let previous {
      result.append(Edge(tab: previous, gained: false))
    }
    if let next {
      result.append(Edge(tab: next, gained: true))
    }
    return result
  }
}
