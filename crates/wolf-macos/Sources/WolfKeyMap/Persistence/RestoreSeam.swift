// Pure restore planner: one planned tab, with its own Neovim, per saved tab.

import Foundation

/// The pure restore planner. A namespace (`enum` with only static members),
/// mirroring `StateSeam`.
public enum RestoreSeam {
  /// A tab's reference to one of its panes: a depth-first leaf index, or the drawer.
  public enum SavedPaneRef: Hashable {
    case pane(Int)
    case drawer
  }

  /// A tab's saved sidebar mode and search panel, each field `nil` when not saved. The
  /// toggles mirror `SearchOptions`.
  public struct SavedSidebar: Hashable {
    public var mode: SidebarPanel?
    public var query: String?
    public var caseSensitive: Bool?
    public var useRegex: Bool?
    public var allFiles: Bool?

    public init(
      mode: SidebarPanel? = nil, query: String? = nil, caseSensitive: Bool? = nil,
      useRegex: Bool? = nil, allFiles: Bool? = nil
    ) {
      self.mode = mode
      self.query = query
      self.caseSensitive = caseSensitive
      self.useRegex = useRegex
      self.allFiles = allFiles
    }
  }

  /// A tab's saved terminal drawer: whether it is open, its share of the pane area's height,
  /// and whether it is the tab's zoomed pane.
  public struct SavedDrawer: Hashable {
    public let open: Bool
    public let share: Double
    public let zoomed: Bool

    public init(open: Bool, share: Double, zoomed: Bool = false) {
      self.open = open
      self.share = share
      self.zoomed = zoomed
    }
  }

  /// One saved tab's state: its working directory and sidebar visibility, in
  /// the shape the AppKit caller reads back from `FfiAppTab`, and the shape a
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
    /// The tab's focused pane, `nil` when none is saved.
    public let focused: SavedPaneRef?
    /// The sidebar's mode, query and toggles.
    public let sidebar: SavedSidebar
    /// The terminal drawer, `nil` when none is saved.
    public let drawer: SavedDrawer?

    public init(
      cwd: String, sidebarVisible: Bool, layout: SavedPaneNode?, focused: SavedPaneRef? = nil,
      sidebar: SavedSidebar = SavedSidebar(), drawer: SavedDrawer? = nil
    ) {
      self.cwd = cwd
      self.sidebarVisible = sidebarVisible
      self.layout = layout
      self.focused = focused
      self.sidebar = sidebar
      self.drawer = drawer
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

  /// The position to select in a restored window: the saved `active` one when it restored,
  /// else the first restored position, `nil` when none did.
  public static func selectedPosition(active: Int, materialized: [Int]) -> Int? {
    materialized.contains(active) ? active : materialized.first
  }
}
