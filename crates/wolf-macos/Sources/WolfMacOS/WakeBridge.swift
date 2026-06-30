//
//  WakeBridge.swift
//  Marshals wolf-core's wake (fired on its tokio thread) onto the AppKit main
//  thread and triggers a repaint — the macOS realization of the
//  wake → dispatch-to-main → setNeedsDisplay path.
//

import AppKit
import WolfFFI

/// Bridges wolf-core's redraw wake to AppKit. Core invokes the registered
/// `extern "C"` wake (see `wolf_ffi_set_wake`) from its worker thread; the thunk
/// below hops onto the main thread and drives the registered `PaneContainerView`'s
/// render-wake orchestration (which applies layout intents + redraws every pane and
/// the cmdline strip), then the file-tree sidebar (analogous to wolf-gnome's
/// async-channel + `queue_draw`).
enum WakeBridge {
    /// The pane container driven on each wake (its `onRenderWake()` applies layout
    /// intents and marks every pane + the cmdline strip for redisplay). Weak so the
    /// bridge never keeps the window alive; set by `AppDelegate` once the
    /// `PaneContainerView` exists.
    weak static var registeredPaneView: PaneContainerView?

    /// The file-tree sidebar nudged on each render wake to reveal + bold the
    /// current buffer (`current_file` rides the same render wake, not a redraw
    /// frame). Weak so the bridge never keeps the window alive; set by
    /// `AppDelegate` once the `FileTreeView` exists.
    weak static var registeredTree: FileTreeView?

    /// Registers the C wake callback with the Rust core. The closure captures no
    /// state (it only touches the statics above), so it converts to a
    /// `@convention(c)` function pointer.
    static func install() {
        wolf_ffi_set_wake({
            DispatchQueue.main.async {
                WakeBridge.registeredPaneView?.onRenderWake()
                WakeBridge.registeredTree?.onRenderWake()
            }
        })
    }
}
