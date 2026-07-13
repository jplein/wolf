//
//  RestoreSeam.swift
//  Pure, AppKit-free "group-by-cwd" restore planner for the project/tabpage
//  model (plan §4.2 + OD5).
//
//  The macOS front-end maps each view (an OS window / native tab) to one nvim
//  TABPAGE within a captive-Neovim PROJECT; project identity is NOT persisted.
//  On restore, this seam rebuilds projects from the saved (window, tab, cwd)
//  tree: tabs whose saved cwd is a REAL existing directory group by that cwd
//  STRING (merging across windows) into one project; every empty-state (`""`)
//  or since-deleted / non-directory tab becomes its OWN single-tab project
//  (OD5 — never collapse them, matching "new window => new project").
//
//  Purity: the ONLY injected impurity is `isDirectory` (the AppKit caller passes
//  a FileManager-backed check; tests pass a set lookup). The seam does NOT
//  canonicalize — the AppKit caller persists an already-canonical cwd via
//  `cwd_for_tabpage`, so grouping is on the cwd string as-saved. Imports only
//  Foundation, so it lives in the Foundation-only `WolfKeyMap` module alongside
//  `StateSeam.swift` / `TabTitleSeam.swift`.
//

import Foundation

/// The pure restore planner. A namespace (`enum` with only static members),
/// mirroring `StateSeam`.
public enum RestoreSeam {
  /// One saved tab's state: its working directory and sidebar visibility, in
  /// the shape the AppKit caller reads back from `FfiTabState`.
  public struct SavedTab {
    /// The saved (already-canonical) working directory, or `""` for the
    /// empty state.
    public let cwd: String
    /// Whether the tab's file-tree sidebar was visible.
    public let sidebarVisible: Bool

    public init(cwd: String, sidebarVisible: Bool) {
      self.cwd = cwd
      self.sidebarVisible = sidebarVisible
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

  /// One planned tab: which project it belongs to is implicit (it lives in a
  /// `ProjectPlan.tabs`), but it retains its ORIGINAL saved placement +
  /// metadata so Phase B can drop it back into the right OS window/tab-group.
  ///
  /// `Hashable` so tests can compare an order-independent `Set` of tabs.
  public struct PlannedTab: Hashable, Equatable {
    /// Index of the saved window (tab group) this tab belonged to.
    public let groupIndex: Int
    /// Position within that window's tab order.
    public let positionInGroup: Int
    /// The tab's ORIGINAL saved cwd string (retained even for a deleted-dir
    /// tab whose GROUPING key differs — see `plan`).
    public let cwd: String
    /// Whether the tab's file-tree sidebar was visible.
    public let sidebarVisible: Bool

    public init(
      groupIndex: Int, positionInGroup: Int, cwd: String, sidebarVisible: Bool
    ) {
      self.groupIndex = groupIndex
      self.positionInGroup = positionInGroup
      self.cwd = cwd
      self.sidebarVisible = sidebarVisible
    }
  }

  /// One planned project: the shared captive nvim (one `Project`) plus the
  /// tabs (views) it owns.
  public struct ProjectPlan: Equatable {
    /// The directory nvim launches at: the real directory for a folder
    /// project, or `home` for an empty-state / deleted-dir project.
    public let launchCwd: String
    /// The project's tabs, each retaining its saved placement.
    public let tabs: [PlannedTab]

    public init(launchCwd: String, tabs: [PlannedTab]) {
      self.launchCwd = launchCwd
      self.tabs = tabs
    }
  }

  /// The whole restore plan: the projects to materialize (Phase A) before the
  /// saved OS windows/tab-groups are re-assembled (Phase B).
  public struct RestorePlan: Equatable {
    /// The projects to materialize, in first-encounter order.
    public let projects: [ProjectPlan]

    public init(projects: [ProjectPlan]) {
      self.projects = projects
    }
  }

  /// Groups the saved `windows` tree into projects (see the type doc).
  ///
  /// A tab whose saved `cwd` is non-empty AND `isDirectory(cwd)` is a REAL
  /// directory: it joins (or starts) the folder project keyed by that cwd
  /// STRING, merging same-dir tabs across windows. Anything else — an empty
  /// `""` cwd, or a since-deleted / non-directory path — is an empty-state tab
  /// and gets a UNIQUE project of its own (`launchCwd == home`), while its
  /// `PlannedTab.cwd` still retains the original saved string. Projects are
  /// returned in first-encounter order.
  public static func plan(
    windows: [SavedWindow], home: String, isDirectory: (String) -> Bool
  ) -> RestorePlan {
    // Ordered builders; `folderIndex` maps a real-dir cwd to its builder so
    // same-dir tabs (even across windows) merge into ONE project.
    var builders: [(launchCwd: String, tabs: [PlannedTab])] = []
    var folderIndex: [String: Int] = [:]

    for (groupIndex, window) in windows.enumerated() {
      for (position, tab) in window.tabs.enumerated() {
        let planned = PlannedTab(
          groupIndex: groupIndex,
          positionInGroup: position,
          cwd: tab.cwd,
          sidebarVisible: tab.sidebarVisible)

        let isRealDir = !tab.cwd.isEmpty && isDirectory(tab.cwd)
        if isRealDir {
          if let idx = folderIndex[tab.cwd] {
            builders[idx].tabs.append(planned)
          } else {
            folderIndex[tab.cwd] = builders.count
            builders.append((launchCwd: tab.cwd, tabs: [planned]))
          }
        } else {
          // OD5: an empty-state / deleted-dir tab is its OWN project.
          builders.append((launchCwd: home, tabs: [planned]))
        }
      }
    }

    let projects = builders.map { ProjectPlan(launchCwd: $0.launchCwd, tabs: $0.tabs) }
    return RestorePlan(projects: projects)
  }
}
