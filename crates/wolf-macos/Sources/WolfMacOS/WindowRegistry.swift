// Routes a wolf-core wake by its core's token to the owning tab's `TabCore`.

import Foundation

/// The process-global map from a core's wake token to the `TabCore` that owns it.
///
/// The wake fires on a tokio worker thread with only a plain `Send` `u64` token;
/// the four `*WakeBridge` thunks hop onto the AppKit main thread and then look
/// the token up HERE to reach the tab whose core fired. Because every access
/// happens on the main thread (registration at `TabCore.start()`, lookup inside
/// the dispatched-to-main wake, removal at `TabCore.shutdown()`), no lock is needed.
final class WindowRegistry {
  /// The shared, main-thread-only registry.
  static let shared = WindowRegistry()

  /// Token → tab core. A core unregisters itself when it shuts down, so an entry
  /// never outlives its captive Neovim.
  private var tabCores: [UInt64: TabCore] = [:]

  private init() {}

  /// Registers `tabCore` under its core's wake `token`. Main thread only.
  func register(_ tabCore: TabCore, token: UInt64) {
    tabCores[token] = tabCore
  }

  /// Removes the core registered under `token` (idempotent). Main thread only.
  func unregister(token: UInt64) {
    tabCores.removeValue(forKey: token)
  }

  /// The core registered under `token`, or `nil` when none is (a late wake after
  /// its tab closed). Main thread only.
  func tabCore(for token: UInt64) -> TabCore? {
    tabCores[token]
  }
}
