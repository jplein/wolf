//
//  WindowRegistry.swift
//  Routes a wolf-core wake (carrying its per-window token) to the owning
//  `WolfWindowController` (plan §1).
//

import Foundation

/// The process-global map from a core's per-window wake token to the
/// `WolfWindowController` that owns it.
///
/// The wake fires on a tokio worker thread with only a plain `Send` `u64` token;
/// the three `*WakeBridge` thunks hop onto the AppKit main thread and then look
/// the token up HERE to reach the right window's controller. Because every
/// access happens on the main thread (registration at `start()`, lookup inside
/// the dispatched-to-main wake, removal at teardown), no lock is needed.
final class WindowRegistry {
    /// The shared, main-thread-only registry.
    static let shared = WindowRegistry()

    /// Token → controller. Weak values are unnecessary: the controller
    /// unregisters itself at teardown, so an entry never outlives its window.
    private var controllers: [UInt64: WolfWindowController] = [:]

    private init() {}

    /// Registers `controller` under its core's wake `token`. Main thread only.
    func register(_ controller: WolfWindowController, token: UInt64) {
        controllers[token] = controller
    }

    /// Removes the controller registered under `token` (idempotent). Main thread
    /// only.
    func unregister(token: UInt64) {
        controllers.removeValue(forKey: token)
    }

    /// The controller registered under `token`, or `nil` when none is (a late
    /// wake after teardown). Main thread only.
    func controller(for token: UInt64) -> WolfWindowController? {
        controllers[token]
    }
}
