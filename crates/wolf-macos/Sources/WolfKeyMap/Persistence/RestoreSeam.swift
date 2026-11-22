// Pure restore planner: one planned tab, with its own Neovim, per saved tab.

import Foundation

/// The pure restore planner. A namespace (`enum` with only static members),
/// mirroring `StateSeam`.
public enum RestoreSeam {
  /// One saved tab's state: its working directory and sidebar visibility, in
  /// the shape the AppKit caller reads back from `FfiTabState`, and the shape a
  /// live view contributes to a commit (`SessionSeam`).
  public struct SavedTab: Equatable {
    /// The saved (already-canonical) working directory, or `""` for the
    /// empty state.
    public let cwd: String
    /// Whether the tab's file-tree sidebar was visible.
    public let sidebarVisible: Bool
    /// The tab's saved pane-split layout, `nil` ONLY for an empty-state (unnamed,
    /// non-preview) lone pane; a lone pane showing a file or preview persists a
    /// single leaf. A PASSTHROUGH: it rides into the matching `PlannedTab.layout`
    /// verbatim.
    public let layout: SavedPaneNode?

    public init(cwd: String, sidebarVisible: Bool, layout: SavedPaneNode?) {
      self.cwd = cwd
      self.sidebarVisible = sidebarVisible
      self.layout = layout
    }
  }

  /// One saved OS window (a native tab group), carrying its tabs in tab order.
  public struct SavedWindow {
    /// The window's saved tabs, in tab order.
    public let tabs: [SavedTab]

    public init(tabs: [SavedTab]) {
      self.tabs = tabs
    }
  }

  /// One planned tab: the directory its own Neovim launches at, and its ORIGINAL
  /// saved placement + metadata so Phase B can drop it back into the right OS
  /// window/tab-group.
  public struct PlannedTab: Hashable, Equatable {
    /// Index of the saved window (tab group) this tab belonged to.
    public let groupIndex: Int
    /// Position within that window's tab order.
    public let positionInGroup: Int
    /// The tab's ORIGINAL saved cwd string, retained even for a deleted-dir tab
    /// that launches at home.
    public let cwd: String
    /// Whether the tab's file-tree sidebar was visible.
    public let sidebarVisible: Bool
    /// The tab's saved pane-split layout, carried verbatim from `SavedTab.layout`.
    public let layout: SavedPaneNode?
    /// The directory the tab's Neovim launches at: the saved cwd when it is
    /// non-empty and still a directory, else `home` (`StateSeam.launchDecision`).
    public let launchCwd: String
    /// Whether the tab launches on a folder (`false` for the empty state).
    public let hasFolder: Bool

    public init(
      groupIndex: Int, positionInGroup: Int, cwd: String, sidebarVisible: Bool,
      layout: SavedPaneNode?, launchCwd: String, hasFolder: Bool
    ) {
      self.groupIndex = groupIndex
      self.positionInGroup = positionInGroup
      self.cwd = cwd
      self.sidebarVisible = sidebarVisible
      self.layout = layout
      self.launchCwd = launchCwd
      self.hasFolder = hasFolder
    }
  }

  /// Plans one tab per saved tab, in window then tab order. Each starts its own
  /// Neovim, so tabs on one directory are never merged.
  public static func plan(
    windows: [SavedWindow], home: String, isDirectory: (String) -> Bool
  ) -> [PlannedTab] {
    var tabs: [PlannedTab] = []
    for (groupIndex, window) in windows.enumerated() {
      for (position, tab) in window.tabs.enumerated() {
        let launchCwd: String
        let hasFolder: Bool
        switch StateSeam.launchDecision(restored: tab.cwd, home: home, isDirectory: isDirectory) {
        case .folder(let cwd):
          launchCwd = cwd
          hasFolder = true
        case .emptyState(let nvimCwd):
          launchCwd = nvimCwd
          hasFolder = false
        }
        tabs.append(
          PlannedTab(
            groupIndex: groupIndex,
            positionInGroup: position,
            cwd: tab.cwd,
            sidebarVisible: tab.sidebarVisible,
            layout: tab.layout,
            launchCwd: launchCwd,
            hasFolder: hasFolder))
      }
    }
    return tabs
  }
}
