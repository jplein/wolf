//
//  StatePersistence.swift
//  Pushes window geometry / sidebar visibility / toolbar display mode into the
//  core's state store (over the wolf-ffi `state_set_*` bridge) whenever they
//  change, so `state.json` tracks the live UI. Owned by `AppDelegate`.
//
//  This is WRITE-ONLY glue: it performs NO file IO itself (the store's writer
//  thread in wolf-core owns the file) and no restore (AppDelegate applies the
//  restored state before the window is shown). The embedded Neovim's cwd is
//  persisted core-side (the `wolf_cwd` hook) with no involvement here.
//

import AppKit
import WolfKeyMap

/// Observes the main window's geometry, the sidebar's collapsed state, and the
/// toolbar's display mode, pushing each change over the `state_set_*` bridge
/// methods; `pushSnapshot()` pushes all three facets at once (called once after
/// launch and from `applicationWillTerminate`, backstopping any observation
/// that did not fire — `NSToolbar.displayMode` KVO is not documented-guaranteed).
final class StatePersistence: NSObject {
    private let handle: WolfCoreHandle
    private weak var window: NSWindow?
    private weak var sidebarItem: NSSplitViewItem?
    private weak var toolbar: NSToolbar?

    /// The NotificationCenter observation tokens (removed on deinit).
    private var notificationObservers: [NSObjectProtocol] = []
    /// The KVO observation of the sidebar split item's `isCollapsed`.
    private var sidebarObservation: NSKeyValueObservation?
    /// The (best-effort) KVO observation of the toolbar's `displayMode`.
    private var toolbarObservation: NSKeyValueObservation?

    /// Builds the persistence glue over the launched UI. All UI references are
    /// weak: the delegate owns their lifetimes, this object only observes.
    init(
        handle: WolfCoreHandle,
        window: NSWindow,
        sidebarItem: NSSplitViewItem,
        toolbar: NSToolbar?
    ) {
        self.handle = handle
        self.window = window
        self.sidebarItem = sidebarItem
        self.toolbar = toolbar
        super.init()
    }

    deinit {
        let center = NotificationCenter.default
        for observer in notificationObservers {
            center.removeObserver(observer)
        }
        sidebarObservation?.invalidate()
        toolbarObservation?.invalidate()
    }

    /// Installs the observers: window move, end-of-live-resize, non-live resize
    /// (zoom / programmatic setFrame), sidebar `isCollapsed` KVO, and toolbar
    /// `displayMode` KVO (best-effort; the terminate-time `pushSnapshot()` is
    /// the correctness backstop).
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
        toolbarObservation = toolbar?.observe(\.displayMode, options: [.new]) {
            [weak self] _, _ in
            self?.pushToolbar()
        }
    }

    /// Pushes ALL persisted facets (geometry + sidebar visibility + toolbar
    /// display mode) in one go. Called once right after the window is shown at
    /// launch (so a fresh run persists its initial layout) and from
    /// `applicationWillTerminate` (the backstop for any missed observation).
    func pushSnapshot() {
        pushGeometry()
        pushSidebar()
        pushToolbar()
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

    /// Pushes the toolbar's current display mode, mapped onto the shared
    /// `StateSeam.ToolbarMode` discriminant table. `.default` (the user never
    /// chose an explicit mode) — and a missing toolbar — clear the persisted
    /// mode (`set == false`), so restore leaves the system default alone.
    private func pushToolbar() {
        let mode: StateSeam.ToolbarMode?
        switch toolbar?.displayMode {
        case .iconAndLabel:
            mode = .iconAndText
        case .iconOnly:
            mode = .iconOnly
        case .labelOnly:
            mode = .textOnly
        default:
            mode = nil
        }
        if let mode {
            handle.state_set_toolbar_mode(mode.rawValue, true)
        } else {
            handle.state_set_toolbar_mode(0, false)
        }
    }
}
