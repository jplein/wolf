//
//  WindowRegistry.swift
//  Routes a wolf-core wake (carrying its per-PROJECT token) to the owning
//  `Project`, which fans it out to its current view.
//

import Foundation

/// The process-global map from a core's per-PROJECT wake token to the `Project`
/// that owns it.
///
/// The wake fires on a tokio worker thread with only a plain `Send` `u64` token;
/// the three `*WakeBridge` thunks hop onto the AppKit main thread and then look
/// the token up HERE to reach the right project (which routes to its current
/// view). Because every access happens on the main thread (registration at
/// `Project.start()`, lookup inside the dispatched-to-main wake, removal at last-
/// view shutdown), no lock is needed.
final class WindowRegistry {
  /// The shared, main-thread-only registry.
  static let shared = WindowRegistry()

  /// Token → project. The project unregisters itself when its last view closes,
  /// so an entry never outlives its captive Neovim.
  private var projects: [UInt64: Project] = [:]

  private init() {}

  /// Registers `project` under its core's wake `token`. Main thread only.
  func register(_ project: Project, token: UInt64) {
    projects[token] = project
  }

  /// Removes the project registered under `token` (idempotent). Main thread only.
  func unregister(token: UInt64) {
    projects.removeValue(forKey: token)
  }

  /// The project registered under `token`, or `nil` when none is (a late wake
  /// after the project's last view closed). Main thread only.
  func project(for token: UInt64) -> Project? {
    projects[token]
  }
}
