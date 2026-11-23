// Pure stacking order and tab recency, kept from focus changes rather than asked of AppKit.

import Foundation

/// The window stacking order and per-window tab recency of `specs/application-state.md` §3.
///
/// Kept over tab ids, not tab groups, since a macOS tab group can change identity on a tear-off
/// or merge. A window, passed as its tab ids in tab-bar order, ranks by its best-ranked tab, and
/// its tab recency is `tabs` filtered to its own tabs.
public struct StackingOrder: Equatable {
  /// One restored window, for `init(seed:)`.
  public struct SeedWindow: Equatable {
    /// The window's tab ids, in tab-bar order.
    public let tabs: [Int]
    /// The index into `tabs` of the active tab.
    public let activeTab: Int
    /// Whether the window restored minimized.
    public let minimized: Bool

    public init(tabs: [Int], activeTab: Int, minimized: Bool) {
      self.tabs = tabs
      self.activeTab = activeTab
      self.minimized = minimized
    }
  }

  /// Tab ids, most recently focused first.
  public private(set) var tabs: [Int]
  /// The tabs of minimized windows. A window is minimized when every one of its tabs is here.
  public private(set) var minimized: Set<Int>

  public init() {
    tabs = []
    minimized = []
  }

  /// The order a launch restores: the windows in seed order, each with its active tab first and
  /// the rest in tab order. An out-of-range `activeTab` lists the window in tab order.
  public init(seed: [SeedWindow]) {
    tabs = []
    minimized = []
    for window in seed {
      if window.tabs.indices.contains(window.activeTab) {
        tabs.append(window.tabs[window.activeTab])
        tabs += window.tabs.enumerated().filter { $0.offset != window.activeTab }.map(\.element)
      } else {
        tabs += window.tabs
      }
      if window.minimized { minimized.formUnion(window.tabs) }
    }
  }

  /// Moves `tab` to the front, and unmarks it as minimized, so a missed deminiaturize cannot keep
  /// a focused window behind.
  public mutating func focus(_ tab: Int) {
    tabs.removeAll { $0 == tab }
    tabs.insert(tab, at: 0)
    minimized.remove(tab)
  }

  /// Marks every tab of `window` minimized, leaving its rank alone.
  public mutating func minimize(_ window: [Int]) {
    minimized.formUnion(window)
  }

  /// Unmarks every tab of `window`, leaving its rank alone.
  public mutating func deminimize(_ window: [Int]) {
    minimized.subtract(window)
  }

  /// Forgets a closed tab.
  public mutating func remove(_ tab: Int) {
    tabs.removeAll { $0 == tab }
    minimized.remove(tab)
  }

  /// Whether `window` is minimized: it has tabs, and all of them are marked.
  public func isMinimized(_ window: [Int]) -> Bool {
    !window.isEmpty && window.allSatisfy(minimized.contains)
  }

  /// The indices of `window`, most recently focused first. Tabs never focused follow, in index
  /// order.
  public func recency(_ window: [Int]) -> [Int] {
    let rank = ranks()
    return window.indices.sorted { a, b in
      switch (rank[window[a]], rank[window[b]]) {
      case (let ra?, let rb?): return ra != rb ? ra < rb : a < b
      case (.some, nil): return true
      case (nil, .some): return false
      case (nil, nil): return a < b
      }
    }
  }

  /// The indices of `windows`, frontmost first: windows that are not minimized before those that
  /// are, then by their best-ranked tab, a window with no ranked tab last of its class, then by
  /// input index.
  public func order(_ windows: [[Int]]) -> [Int] {
    let rank = ranks()
    let keys = windows.map { window -> (Bool, Int) in
      (isMinimized(window), window.compactMap { rank[$0] }.min() ?? Int.max)
    }
    return windows.indices.sorted { a, b in
      let (ka, kb) = (keys[a], keys[b])
      if ka.0 != kb.0 { return !ka.0 }
      if ka.1 != kb.1 { return ka.1 < kb.1 }
      return a < b
    }
  }

  /// Each ranked tab's position in `tabs`.
  private func ranks() -> [Int: Int] {
    Dictionary(tabs.enumerated().map { ($0.element, $0.offset) }, uniquingKeysWith: min)
  }
}
