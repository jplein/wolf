// Pure write-side decisions for state persistence, ported from wolf-gnome's state_seam.

import Foundation

/// What each live view contributes to `state.json`, and when a commit runs. A
/// namespace (`enum` with only static members), the write-side counterpart of
/// `RestoreSeam`.
public enum SessionSeam {
  /// What one leaf's Neovim window shows, read by window handle from core's
  /// tabpage-global maps.
  public enum LeafContent: Equatable {
    /// A Markdown preview of the file at `source`.
    case preview(source: String)
    /// A buffer at `path`; `""` is an unnamed buffer.
    case buffer(path: String)
    /// A terminal pane whose shell is in `cwd`; `""` when core cannot read it.
    case terminal(cwd: String)
    /// The window is in neither map yet, or the leaf has no window yet.
    case unreported
  }

  /// The outcome of reading one view's live state.
  public enum TabGather: Equatable {
    /// Every leaf and the cwd are reported: this is the tab to persist.
    case ready(RestoreSeam.SavedTab)
    /// Something is not reported yet; persisting now would write a partial tab.
    case refuse
  }

  /// Reads one view into the tab it persists, or refuses when any part of it is
  /// not reported yet.
  ///
  /// It refuses when `hasFolder` and `cwd` is empty, and when any leaf of `layout`
  /// is `.unreported` or missing from `content`. A view without a folder persists
  /// `cwd: ""`, whatever Neovim reports, and a lone empty leaf persists no layout.
  public static func gatherTab(
    hasFolder: Bool, cwd: String, sidebarVisible: Bool, layout: PaneLayout,
    content: [PaneId: LeafContent]
  ) -> TabGather {
    if hasFolder && cwd.isEmpty { return .refuse }
    var paths: [PaneId: String] = [:]
    var previews: Set<PaneId> = []
    var terminals: Set<PaneId> = []
    for id in leafIds(layout) {
      switch content[id] {
      case .preview(let source):
        paths[id] = source
        previews.insert(id)
      case .buffer(let path):
        paths[id] = path
      case .terminal(let cwd):
        paths[id] = cwd
        terminals.insert(id)
      case .unreported, nil:
        return .refuse
      }
    }
    let node = PaneLayoutCodec.encode(
      layout, paths: paths, previews: previews, terminals: terminals)
    return .ready(
      RestoreSeam.SavedTab(
        cwd: hasFolder ? cwd : "", sidebarVisible: sidebarVisible,
        layout: node.isEmptyLeaf ? nil : node))
  }

  /// Whether a view's live state may replace its cached contribution at all.
  ///
  /// False when its restore rebuild failed, so it shows a fallback pane, and when
  /// it has no folder although it was restored from one, which means the saved
  /// folder was missing at launch. Either way the cache, seeded from the saved
  /// tab, is what persists.
  public static func mayGather(hasFolder: Bool, restoredCwd: String?, restoreFailed: Bool)
    -> Bool
  {
    if restoreFailed { return false }
    if !hasFolder, let restoredCwd, !restoredCwd.isEmpty { return false }
    return true
  }

  /// The tab a view contributes to a commit: the gathered tab when the gate is open
  /// and the gather is ready, otherwise `cache`. `gather` is not evaluated when the
  /// gate is shut. The caller stores the result as its next cache.
  public static func contribution(
    mayGather: Bool, gather: @autoclosure () -> TabGather, cache: RestoreSeam.SavedTab?
  ) -> RestoreSeam.SavedTab? {
    guard mayGather, case .ready(let tab) = gather() else { return cache }
    return tab
  }

  /// What closing one view commits.
  public enum CloseCommit: Equatable {
    /// Commit nothing.
    case skip
    /// Commit now, with the closing view and every view closed in the same burst
    /// still in the tree.
    case keepSubtree
    /// Commit later, without the closing view.
    case dropSubtree
  }

  /// What closing a view commits. While terminating, nothing: the terminate
  /// commit already wrote the whole tree. Closing the last live view quits the
  /// app, so it keeps its subtree. Any other close drops it.
  public static func closeCommit(terminating: Bool, otherLiveViews: Int) -> CloseCommit {
    if terminating { return .skip }
    return otherLiveViews == 0 ? .keepSubtree : .dropSubtree
  }

  /// Groups every view close of one runloop turn into a burst, and makes a keep
  /// commit in that burst write the tree read at the burst's first close.
  ///
  /// The tree is read at the first close because a tabbed window's close button
  /// closes each tab's window in turn: by the last close, which is the keep, the
  /// earlier tabs are torn down and have left the group, so a fresh read would
  /// keep only the last tab. A tab closed alone ends its burst on the next turn,
  /// so a later close reads afresh and the closed tab stays dropped.
  ///
  /// The collaborators are injected: `scheduleEnd` runs the burst's end on the next
  /// turn (production: `DispatchQueue.main.async`), `read` walks the live tree, and
  /// `commit` writes a tree.
  public final class CloseBurst<Tree> {
    private let scheduleEnd: (@escaping () -> Void) -> Void
    private let read: () -> Tree
    private let commit: (Tree) -> Void
    /// Whether a burst is open. Its own flag, since `Tree` may be optional and a read
    /// may return `nil`.
    private var isOpen = false
    /// The tree read at the open burst's first close.
    private var tree: Tree?

    public init(
      scheduleEnd: @escaping (@escaping () -> Void) -> Void, read: @escaping () -> Tree,
      commit: @escaping (Tree) -> Void
    ) {
      self.scheduleEnd = scheduleEnd
      self.read = read
      self.commit = commit
    }

    /// What closing one view commits, as `SessionSeam.closeCommit(terminating:
    /// otherLiveViews:)` answers. A terminating close does nothing else. Any other
    /// close opens a burst when none is open, and on `.keepSubtree` commits the
    /// burst's tree before returning.
    public func close(terminating: Bool, otherLiveViews: Int) -> CloseCommit {
      if terminating { return .skip }
      if !isOpen {
        isOpen = true
        tree = read()
        scheduleEnd { [weak self] in
          self?.isOpen = false
          self?.tree = nil
        }
      }
      let decision = SessionSeam.closeCommit(terminating: false, otherLiveViews: otherLiveViews)
      if decision == .keepSubtree, let tree {
        commit(tree)
      }
      return decision
    }
  }

  /// The windows a commit writes, or `nil` to commit nothing: with no live view
  /// the walk read only torn-down views, and with no window every core failed,
  /// and either would erase the saved session.
  public static func committedWindows<Window>(liveViews: Int, windows: [Window]) -> [Window]? {
    if liveViews == 0 || windows.isEmpty { return nil }
    return windows
  }

  /// One saved tab's place: its saved window and its position in that window.
  public struct TabSlot: Hashable {
    public let groupIndex: Int
    public let positionInGroup: Int

    public init(groupIndex: Int, positionInGroup: Int) {
      self.groupIndex = groupIndex
      self.positionInGroup = positionInGroup
    }
  }

  /// The saved tabs of one saved window that no live view took.
  public struct OrphanGroup: Equatable {
    public let groupIndex: Int
    public let positions: [Int]

    public init(groupIndex: Int, positions: [Int]) {
      self.groupIndex = groupIndex
      self.positions = positions
    }
  }

  /// Every saved slot without a live view, grouped by saved window, with groups and
  /// positions in ascending saved order. A window with no orphan yields no group.
  public static func orphanedSlots(savedTabCounts: [Int], materialized: Set<TabSlot>)
    -> [OrphanGroup]
  {
    savedTabCounts.enumerated().compactMap { groupIndex, count in
      let positions = (0..<count).filter {
        !materialized.contains(TabSlot(groupIndex: groupIndex, positionInGroup: $0))
      }
      return positions.isEmpty ? nil : OrphanGroup(groupIndex: groupIndex, positions: positions)
    }
  }

  /// The leaf ids of `layout`, in DFS order.
  private static func leafIds(_ layout: PaneLayout) -> [PaneId] {
    switch layout {
    case .leaf(let id):
      return [id]
    case .split(_, let children, _):
      return children.flatMap(leafIds)
    }
  }
}
