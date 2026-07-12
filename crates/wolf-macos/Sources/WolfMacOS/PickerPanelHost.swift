//
//  PickerPanelHost.swift
//  Drives one picker's `PickerPanel`: presents it as a child window in front of the
//  owning main window, sizes + positions it top-center via `PickerPanelSeam`, and
//  funnels every "resign key" (click-outside / Cmd-Tab / Mission Control / another
//  window) into the picker's `close()` (plan §2c).
//
//  One host per picker, owned strongly by `WolfWindowController`; the host owns the
//  panel strongly, and the panel owns the picker view (its `contentView`) strongly.
//  The host adds the panel via `addChildWindow(ordered:.above)` so the window server
//  composites it in front of the parent's content (including the preview
//  `WKWebView`) and auto-follows the parent on move. Window-server-bound, so
//  exercised by manual smoke only; the pure positioning is `PickerPanelSeam`.
//

import AppKit
import WolfKeyMap

/// The AppKit-facing seam each picker view calls to present / dismiss / refit its
/// floating panel. The view holds this WEAKLY (`weak var host`); the concrete
/// `PickerPanelHost` is the strong owner in the reverse direction.
protocol PickerPanelHosting: AnyObject {
    /// Orders the panel in front of the parent + makes it key (so the search field
    /// types). Sizes/positions it first.
    func presentPanel()
    /// Orders the panel out + returns first responder to the editor pane container.
    func dismissPanel()
    /// Resizes the panel to the view's fitting height and repositions it top-center
    /// over the parent's content.
    func refitPanel()
}

/// Hosts one picker view in a borderless non-activating child `NSPanel` composited
/// in front of the owning main window. Its `windowDidResignKey` is the SINGLE
/// dismissal funnel for both "click outside" and "another window/app activated"
/// (reqs 3 & 4), routed into `onResignKey` (the picker's `close()`).
final class PickerPanelHost: NSObject, PickerPanelHosting, NSWindowDelegate {
    /// The picker panel's fixed width, in points (parity with the picker views).
    private static let width: CGFloat = 600
    /// The panel's fixed top margin below the parent content's top edge, in points.
    private static let topMargin: CGFloat = 24

    /// The child panel hosting the picker (strong; the panel owns its `contentView`,
    /// the picker view, strongly in turn).
    private let panel: PickerPanel
    /// The owning main window the panel is a child of. Weak — the controller owns
    /// the window's lifetime.
    private weak var parentWindow: NSWindow?
    /// The editor pane container: used both to map screen coordinates for
    /// positioning and to receive first responder on dismiss. Weak — the controller
    /// owns it.
    private weak var paneContainer: PaneContainerView?
    /// The picker view (the panel's content), measured for its fitting height.
    private let contentView: NSView
    /// The dismissal callback (the picker's `close()`), invoked on resign-key. Must
    /// capture the picker WEAKLY at the call site (see the load-bearing note).
    private let onResignKey: () -> Void
    /// Whether the panel is currently presented. Guards present/dismiss/refit and
    /// the resign-key funnel against re-entrancy.
    private var isPresented = false

    /// Builds a host for `contentView` (the picker) over `parentWindow`, mapping
    /// coordinates + refocusing through `paneContainer`, dismissing via
    /// `onResignKey`. Assigns the panel's content view + delegate; does NOT present.
    init(
        parentWindow: NSWindow,
        paneContainer: PaneContainerView,
        contentView: NSView,
        onResignKey: @escaping () -> Void
    ) {
        self.panel = PickerPanel()
        self.parentWindow = parentWindow
        self.paneContainer = paneContainer
        self.contentView = contentView
        self.onResignKey = onResignKey
        super.init()
        panel.contentView = contentView
        panel.delegate = self
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("PickerPanelHost is not constructed from a nib")
    }

    // MARK: - PickerPanelHosting

    func presentPanel() {
        guard !isPresented else { return }
        isPresented = true
        parentWindow?.addChildWindow(panel, ordered: .above)
        refitPanel()
        panel.makeKeyAndOrderFront(nil)
        // Reposition top-center when the parent window resizes.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(parentDidResize),
            name: NSWindow.didResizeNotification,
            object: parentWindow)
    }

    func dismissPanel() {
        guard isPresented else { return }
        isPresented = false
        NotificationCenter.default.removeObserver(
            self, name: NSWindow.didResizeNotification, object: parentWindow)
        parentWindow?.removeChildWindow(panel)
        panel.orderOut(nil)
        // Deliberately NO makeKeyAndOrderFront / NSApp.activate: if the user
        // Cmd-Tabbed away, do not yank them back — just hand the editor first
        // responder for when the parent is next key.
        parentWindow?.makeFirstResponder(paneContainer)
    }

    func refitPanel() {
        guard isPresented, let parentWindow, let paneContainer else { return }
        let width = Self.width
        // Measure the picker's content height at the fixed 600pt width: AppKit's
        // `fittingSize` computes the size satisfying the view's constraints, so pin
        // the width (its vertical constraint chain is a determinate equality chain,
        // so the returned height is exact) for the duration of the measurement.
        let widthConstraint = contentView.widthAnchor.constraint(equalToConstant: width)
        widthConstraint.isActive = true
        contentView.layoutSubtreeIfNeeded()
        let fittingHeight = contentView.fittingSize.height
        widthConstraint.isActive = false

        // The pane container's bounds in the parent window's coordinates, then in
        // screen coordinates (bottom-left origin).
        let contentInWindow = paneContainer.convert(paneContainer.bounds, to: nil)
        let contentInScreen = parentWindow.convertToScreen(contentInWindow)

        let r = PickerPanelSeam.frame(
            parentContentWidth: Double(contentInScreen.width),
            parentContentHeight: Double(contentInScreen.height),
            panelWidth: Double(width),
            panelHeight: Double(fittingHeight),
            topMargin: Double(Self.topMargin))

        // Map the top-left content rect to a bottom-left screen frame: `r.y` is a
        // distance DOWN from the content top, so the panel's bottom edge sits at
        // `maxY - r.y - r.h`.
        let originX = contentInScreen.minX + CGFloat(r.x)
        let originY = contentInScreen.maxY - CGFloat(r.y) - CGFloat(r.h)
        panel.setFrame(
            NSRect(x: originX, y: originY, width: CGFloat(r.w), height: CGFloat(r.h)),
            display: true)
    }

    // MARK: - NSWindowDelegate

    /// The single dismissal funnel for reqs 3 & 4: the panel resigned key (a click
    /// outside it, Cmd-Tab, Mission Control, or another window activated), so close
    /// the picker.
    func windowDidResignKey(_ notification: Notification) {
        guard isPresented else { return }
        onResignKey()
    }

    // MARK: - Private

    @objc private func parentDidResize(_ notification: Notification) {
        refitPanel()
    }
}
