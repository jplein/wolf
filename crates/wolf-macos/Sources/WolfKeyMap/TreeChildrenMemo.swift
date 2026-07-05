//
//  TreeChildrenMemo.swift
//  A pure, headless per-directory memo for the sidebar file tree's child
//  projection (perf plan Layer 1).
//
//  AppKit's `NSOutlineView` asks for a directory's children in a storm: it calls
//  `numberOfChildrenOfItem` and then `child:index:` once per child, each of which
//  otherwise re-invokes the FFI `children_of(dir)` from scratch (an O(N) scan of
//  the flat file list). This seam caches the projected `[Item]` per directory so a
//  reload's callback storm hits memory: `project` runs at most ONCE per directory
//  between invalidations.
//
//  It is toolkit-free (Foundation only): the caller injects `project` (which in
//  production maps `search.children_of(dir)` into the outline's `TreeItem`s,
//  reusing stable identities), so the seam is unit-testable without AppKit or a
//  live search. `invalidate()` is called on each reload (new snapshot) and reroot.
//

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
