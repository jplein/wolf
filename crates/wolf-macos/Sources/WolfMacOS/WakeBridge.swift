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
/// below hops onto the main thread and marks the registered `GridView` for
/// redisplay (analogous to wolf-gnome's async-channel + `queue_draw`).
enum WakeBridge {
    /// The view repainted on each wake. Weak so the bridge never keeps the window
    /// alive; set by `AppDelegate` once the `GridView` exists.
    weak static var registeredView: GridView?

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
                WakeBridge.registeredView?.needsDisplay = true
                WakeBridge.registeredTree?.onRenderWake()
            }
        })
    }
}
