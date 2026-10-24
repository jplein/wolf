// Best-effort KVO on one window's native tab group.

import AppKit

/// Observes a window's tab-group membership + selection, calling `onChange` on
/// each change. Owned by `StatePersistence`.
final class TabGroupObserver: NSObject {
  private weak var window: NSWindow?
  private let onChange: () -> Void

  private var selectedObservation: NSKeyValueObservation?
  private var windowsObservation: NSKeyValueObservation?
  private var reattachObserver: NSObjectProtocol?

  init(window: NSWindow, onChange: @escaping () -> Void) {
    self.window = window
    self.onChange = onChange
    super.init()
  }

  /// Attaches to the current tab group and arms the `didBecomeKey` re-attach.
  func install() {
    attach()
    reattachObserver = NotificationCenter.default.addObserver(
      forName: NSWindow.didBecomeKeyNotification, object: window, queue: .main
    ) { [weak self] _ in
      // Tear-off and drag-merge often move a window into a new tab group and make it key, so
      // re-attach to that group and re-emit a snapshot.
      self?.attach()
      self?.onChange()
    }
  }

  /// (Re)binds the KVO observations to the window's CURRENT tab group.
  private func attach() {
    guard let group = window?.tabGroup else {
      selectedObservation = nil
      windowsObservation = nil
      return
    }
    selectedObservation = group.observe(\.selectedWindow, options: [.new]) {
      [weak self] _, _ in
      self?.onChange()
    }
    windowsObservation = group.observe(\.windows, options: [.new]) {
      [weak self] _, _ in
      self?.onChange()
    }
  }

  /// Invalidates every observation (called from `StatePersistence.deinit`).
  func invalidate() {
    selectedObservation?.invalidate()
    windowsObservation?.invalidate()
    if let reattachObserver {
      NotificationCenter.default.removeObserver(reattachObserver)
    }
  }
}
