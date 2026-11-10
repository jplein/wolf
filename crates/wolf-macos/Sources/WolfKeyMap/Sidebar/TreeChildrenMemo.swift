// Pure per-directory memo of the sidebar file tree's child projection.

import Foundation

/// Memoizes a directory's projected children so `project` runs at most once per
/// directory per invalidation window.
///
/// The first `children(ofDir:)` for a directory computes via `project` and stores
/// the result; subsequent lookups of the same directory return the cached array
/// with no recomputation. `invalidate()` drops all cached arrays (keeping the
/// dictionary's capacity) so the next lookups recompute against the new snapshot.
public final class TreeChildrenMemo<Item> {
  /// Projects a directory rel-path into its immediate children. Runs at most once
  /// per directory between invalidations.
  private let project: (String) -> [Item]
  /// The per-directory cache of already-projected children.
  private var cache: [String: [Item]] = [:]

  /// Builds the memo with its injected projection.
  public init(project: @escaping (String) -> [Item]) {
    self.project = project
  }

  /// The immediate children of `dir`, computed once via `project` and cached; a
  /// repeated lookup of the same `dir` returns the cached array untouched.
  public func children(ofDir dir: String) -> [Item] {
    if let cached = cache[dir] {
      return cached
    }
    let computed = project(dir)
    cache[dir] = computed
    return computed
  }

  /// Drops every cached projection (keeping capacity), so the next lookups
  /// recompute against the current snapshot. Called on reload and reroot.
  public func invalidate() {
    cache.removeAll(keepingCapacity: true)
  }
}
