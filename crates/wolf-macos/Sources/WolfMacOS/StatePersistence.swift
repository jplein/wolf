//
//  StatePersistence.swift
//  Pushes window geometry / sidebar visibility into the core's state store
//  (over the wolf-ffi `state_set_*` bridge) whenever they change, so
//  `state.json` tracks the live UI. Owned by `AppDelegate`.
//
//  This is WRITE-ONLY glue: it performs NO file IO itself (the store's writer
//  thread in wolf-core owns the file) and no restore (AppDelegate applies the
//  restored state before the window is shown). The embedded Neovim's cwd is
//  persisted core-side (the `wolf_cwd` hook) with no involvement here.
//

import AppKit
import WolfKeyMap

/// Observes the main window's geometry and the sidebar's collapsed state,
/// pushing each change over the `state_set_*` bridge methods; `pushSnapshot()`
/// pushes both facets at once (called once after launch and from
/// `applicationWillTerminate`, backstopping any observation that did not fire).
final class StatePersistence: NSObject {
    private let handle: WolfCoreHandle
    private weak var window: NSWindow?
    private weak var sidebarItem: NSSplitViewItem?

    /// The NotificationCenter observation tokens (removed on deinit).
    private var notificationObservers: [NSObjectProtocol] = []
    /// The KVO observation of the sidebar split item's `isCollapsed`.
    private var sidebarObservation: NSKeyValueObservation?

    /// Builds the persistence glue over the launched UI. All UI references are
    /// weak: the delegate owns their lifetimes, this object only observes.
    init(
        handle: WolfCoreHandle,
        window: NSWindow,
        sidebarItem: NSSplitViewItem
    ) {
        self.handle = handle
        self.window = window
        self.sidebarItem = sidebarItem
        super.init()
    }

    deinit {
        let center = NotificationCenter.default
        for observer in notificationObservers {
            center.removeObserver(observer)
        }
        sidebarObservation?.invalidate()
    }

    /// Installs the observers: window move, end-of-live-resize, non-live resize
    /// (zoom / programmatic setFrame), and sidebar `isCollapsed` KVO (the
    /// terminate-time `pushSnapshot()` is the correctness backstop).
    func install() {
        let center = NotificationCenter.default
        notificationObservers.append(
            center.addObserver(
                forName: NSWindow.didMoveNotification, object: window, queue: .main
            ) { [weak self] _ in
                self?.pushGeometry()
            }
        )
        notificationObservers.append(
            center.addObserver(
                forName: NSWindow.didEndLiveResizeNotification, object: window, queue: .main
            ) { [weak self] _ in
                self?.pushGeometry()
            }
        )
        notificationObservers.append(
            center.addObserver(
                forName: NSWindow.didResizeNotification, object: window, queue: .main
            ) { [weak self] _ in
                // Live-resize spams didResize per frame; skip those (the
                // end-of-live-resize notification above catches the final size)
                // and push only discrete resizes (zoom, programmatic setFrame).
                guard let self, let window = self.window, !window.inLiveResize else {
                    return
                }
                self.pushGeometry()
            }
        )
        sidebarObservation = sidebarItem?.observe(\.isCollapsed, options: [.new]) {
            [weak self] _, _ in
            self?.pushSidebar()
        }
    }

    /// Pushes ALL persisted facets (geometry + sidebar visibility) in one go.
    /// Called once right after the window is shown at launch (so a fresh run
    /// persists its initial layout) and from `applicationWillTerminate` (the
    /// backstop for any missed observation).
    func pushSnapshot() {
        pushGeometry()
        pushSidebar()
    }

    /// Pushes the window's current frame (AppKit bottom-left global points).
    private func pushGeometry() {
        guard let window else { return }
        let frame = window.frame
        handle.state_set_geometry(
            frame.origin.x, frame.origin.y, frame.size.width, frame.size.height
        )
    }

    /// Pushes the sidebar's current visibility (the inverse of `isCollapsed`).
    private func pushSidebar() {
        guard let sidebarItem else { return }
        handle.state_set_sidebar_visible(!sidebarItem.isCollapsed)
    }
}
