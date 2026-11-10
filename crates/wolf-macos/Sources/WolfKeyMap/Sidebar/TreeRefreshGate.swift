// Pure visibility and version gate for a sidebar panel's refresh.

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
