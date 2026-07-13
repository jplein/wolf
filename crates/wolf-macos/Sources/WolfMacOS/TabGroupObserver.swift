//
//  TabGroupObserver.swift
//  Best-effort observation of ONE window's native tab group (membership +
//  selection), firing `onChange` so the coordinator re-snapshots the whole tree
//  after a tear-off / merge / reorder / tab switch.
//
//  The native tab bar mutates `NSWindowTabGroup.windows` (tear-off, drag-merge,
//  reorder) and `.selectedWindow` (tab switch) without any explicit call site of
//  ours, so a KVO backstop keeps `state.json` current. Because a window's
//  `tabGroup` can change identity (a torn-off window gets a fresh group), the
//  observer re-attaches on `didBecomeKey` — which reliably fires around those
//  transitions — and re-emits a snapshot then too. The coordinator's coalesced
//  ~150ms timer absorbs the resulting KVO storm.
//

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
      // Membership changes (tear-off / drag-merge) frequently move a
      // window into a NEW tab group and make it key; re-attach to that
      // group and re-emit a snapshot.
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
