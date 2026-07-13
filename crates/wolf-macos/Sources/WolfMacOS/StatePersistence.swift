//
//  StatePersistence.swift
//  Observes ONE window's geometry / sidebar / tab-group changes and fires a
//  coalesced whole-tree state snapshot on the coordinator whenever any of them
//  change, so `state.json` tracks the live UI. Owned by a `WolfWindowController`.
//
//  This is TRIGGER-ONLY glue: it performs NO file IO and no `state_set_*` calls
//  (those per-index bridge setters are gone). Persistence is now a whole-tree
//  snapshot the coordinator walks (`AppDelegate.snapshotNow`); this object's job
//  is only to notice this window changed and ask for a (coalesced) re-snapshot.
//

import AppKit

/// Observes a window's geometry, its sidebar's collapsed state, and its tab
/// group's membership/selection, calling the injected `onChange` on each change
/// (the coordinator's coalesced `scheduleStateSnapshot`).
final class StatePersistence: NSObject {
  private weak var window: NSWindow?
  private weak var sidebarItem: NSSplitViewItem?
  private let onChange: () -> Void

  /// The NotificationCenter observation tokens (removed on deinit).
  private var notificationObservers: [NSObjectProtocol] = []
  /// The KVO observation of the sidebar split item's `isCollapsed`.
  private var sidebarObservation: NSKeyValueObservation?
  /// The tab-group membership/selection observer (best-effort; the coalesced
  /// snapshot backstops any KVO it misses).
  private var tabGroupObserver: TabGroupObserver?

  /// Builds the trigger glue over the launched UI. All UI references are weak:
  /// the controller owns their lifetimes, this object only observes.
  init(
    window: NSWindow,
    sidebarItem: NSSplitViewItem,
    onChange: @escaping () -> Void
  ) {
    self.window = window
    self.sidebarItem = sidebarItem
    self.onChange = onChange
    super.init()
  }

  deinit {
    let center = NotificationCenter.default
    for observer in notificationObservers {
      center.removeObserver(observer)
    }
    sidebarObservation?.invalidate()
    tabGroupObserver?.invalidate()
  }

  /// Installs the observers: window move, end-of-live-resize, non-live resize
  /// (zoom / programmatic setFrame), sidebar `isCollapsed` KVO, and tab-group
  /// membership/selection (tear-off / merge / reorder / tab switch).
  func install() {
    let center = NotificationCenter.default
    notificationObservers.append(
      center.addObserver(
        forName: NSWindow.didMoveNotification, object: window, queue: .main
      ) { [weak self] _ in
        self?.onChange()
      }
    )
    notificationObservers.append(
      center.addObserver(
        forName: NSWindow.didEndLiveResizeNotification, object: window, queue: .main
      ) { [weak self] _ in
        self?.onChange()
      }
    )
    notificationObservers.append(
      center.addObserver(
        forName: NSWindow.didResizeNotification, object: window, queue: .main
      ) { [weak self] _ in
        // Live-resize spams didResize per frame; skip those (the
        // end-of-live-resize notification above catches the final size)
        // and fire only on discrete resizes (zoom, programmatic setFrame).
        guard let self, let window = self.window, !window.inLiveResize else {
          return
        }
        self.onChange()
      }
    )
    sidebarObservation = sidebarItem?.observe(\.isCollapsed, options: [.new]) {
      [weak self] _, _ in
      self?.onChange()
    }
    if let window {
      let observer = TabGroupObserver(window: window, onChange: onChange)
      observer.install()
      tabGroupObserver = observer
    }
  }
}
