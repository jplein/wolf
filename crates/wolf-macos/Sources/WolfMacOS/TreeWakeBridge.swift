//
//  TreeWakeBridge.swift
//  Marshals wolf-core's file-TREE wake (fired on its tokio worker thread) onto
//  the AppKit main thread and refreshes the sidebar — the tree analog of
//  `PickerWakeBridge`.
//

import AppKit
import WolfFFI

/// Bridges wolf-core's file-tree traversal wake to AppKit. Core invokes the
/// registered `extern "C"` callback (see `wolf_ffi_set_tree_wake`) from its
/// worker thread on each traversal batch; the thunk below hops onto the main
/// thread and asks the registered `FileTreeView` to rebuild its rows. A late wake
/// after the tree has closed is harmless — `refresh()` is a no-op when closed.
enum TreeWakeBridge {
    /// The sidebar refreshed on each file-tree wake. Weak so the bridge never
    /// keeps the window alive; set by `AppDelegate` once the `FileTreeView` exists.
    weak static var registeredTree: FileTreeView?

    /// Registers the C file-tree wake callback with the Rust core. The closure
    /// captures no state (it only touches the static above), so it converts to a
    /// `@convention(c)` function pointer.
    static func install() {
        wolf_ffi_set_tree_wake({
            DispatchQueue.main.async {
                TreeWakeBridge.registeredTree?.refresh()
            }
        })
    }
}
