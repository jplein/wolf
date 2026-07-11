//
//  TreeWakeBridge.swift
//  Marshals wolf-core's file-TREE wake (fired on its tokio worker thread,
//  carrying the firing core's per-window token) onto the AppKit main thread and
//  asks the RIGHT window's sidebar for a COALESCED refresh — the tree analog of
//  `PickerWakeBridge`.
//

import AppKit
import WolfFFI

/// Bridges wolf-core's file-tree traversal wake to AppKit. Core invokes the
/// registered `extern "C"` callback (see `wolf_ffi_set_tree_wake`) from its
/// worker thread on each traversal batch with its per-window `token`; the thunk
/// below hops onto the main thread, looks the token up in `WindowRegistry`, and
/// asks that window's `FileTreeView` to reload/rebuild its rows. The wake asks
/// for a COALESCED refresh (`scheduleRefresh()`) so a large load's many batch
/// wakes collapse to at most one reload/rebuild per frame; the render-wake reveal
/// path (`onRenderWake()`) still refreshes immediately. A late wake after the
/// tree has closed is harmless — `refresh()` is a no-op when closed.
enum TreeWakeBridge {
    /// Registers the C file-tree wake callback with the Rust core once at launch.
    /// The closure captures no state (it only touches the global
    /// `WindowRegistry`), so it converts to a `@convention(c)` function pointer
    /// taking the token.
    static func install() {
        wolf_ffi_set_tree_wake({ token in
            DispatchQueue.main.async {
                WindowRegistry.shared.project(for: token)?.onTreeWake()
            }
        })
    }
}
