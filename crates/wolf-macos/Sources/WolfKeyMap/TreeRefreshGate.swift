//
//  TreeRefreshGate.swift
//  A pure, headless visibility + version gate for the sidebar file tree's refresh
//  (perf plan Layer 3).
//
//  Two redundant-work problems it removes:
//    - The outline reload/reveal ran even while the sidebar was collapsed/hidden
//      (which is why hiding the sidebar did not help the freeze). The VISIBILITY
//      gate defers all refresh work while hidden and runs exactly ONE catch-up
//      refresh when the sidebar reappears with deferred work pending.
//    - A full `reloadData()` ran on every render wake even when nothing structural
//      changed. The VERSION gate reloads only when the file-search snapshot version
//      has advanced since the last reload.
//
//  It is toolkit-free (Foundation only), so it is unit-testable without AppKit: the
//  `FileTreeView` relays the sidebar's real `isCollapsed` KVO into `setVisible`,
//  and passes `search.version()` into `shouldReload`.
//

import Foundation

/// Gates the sidebar tree's refresh work on visibility and snapshot version.
public final class TreeRefreshGate {
  /// Whether the sidebar is currently visible; refresh work is deferred while
  /// this is false.
  private var isVisible: Bool
  /// Whether a refresh was requested (and dropped) while hidden, so a catch-up is
  /// owed when the sidebar reappears.
  private var deferred = false
  /// The snapshot version last actually reloaded, so `shouldReload` skips
  /// no-op reloads at an unchanged version.
  private var lastReloadedVersion: UInt64?

  /// Builds the gate with the sidebar's initial visibility.
  public init(isVisible: Bool) {
    self.isVisible = isVisible
  }

  /// Requests a refresh. Returns true when the sidebar is visible (the caller
  /// should proceed); when hidden it records that a catch-up is owed and returns
  /// false (the caller does nothing now).
  public func requestRefresh() -> Bool {
    if isVisible {
      return true
    }
    deferred = true
    return false
  }

  /// Updates the sidebar's visibility. Returns true iff this transition was
  /// hidden->visible AND a refresh was deferred while hidden (clearing the
  /// deferred flag) — the caller then runs the one catch-up refresh. Returns
  /// false otherwise.
  public func setVisible(_ visible: Bool) -> Bool {
    let wasHidden = !isVisible
    isVisible = visible
    if visible, wasHidden, deferred {
      deferred = false
      return true
    }
    return false
  }

  /// Whether a reload is warranted for `version`: true iff it differs from the
  /// last reloaded version, recording it as reloaded when so. A repeated call at
  /// an unchanged version returns false (skip the redundant `reloadData`).
  public func shouldReload(version: UInt64) -> Bool {
    if version != lastReloadedVersion {
      lastReloadedVersion = version
      return true
    }
    return false
  }

  /// Forgets the last-reloaded version, so the next `shouldReload` reloads
  /// unconditionally. Called on reroot, where the outline is rebuilt wholesale.
  public func resetReloadTracking() {
    lastReloadedVersion = nil
  }
}
