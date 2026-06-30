//
//  PickerWakeBridge.swift
//  Marshals wolf-core's file-search wake (fired on its tokio worker thread) onto
//  the AppKit main thread and refreshes the picker — the picker analog of
//  `WakeBridge`.
//

import AppKit
import WolfFFI

/// Bridges wolf-core's file-search traversal wake to AppKit. Core invokes the
/// registered `extern "C"` callback (see `wolf_ffi_set_filesearch_wake`) from its
/// worker thread on each traversal batch; the thunk below hops onto the main
/// thread and asks the registered `PickerView` to re-rank/redraw. A late wake
/// after the picker has closed is harmless — `refresh()` is a no-op when closed.
enum PickerWakeBridge {
    /// The picker refreshed on each file-search wake. Weak so the bridge never
    /// keeps the window alive; set by `AppDelegate` once the `PickerView` exists.
    weak static var registeredPicker: PickerView?

    /// Registers the C file-search wake callback with the Rust core. The closure
    /// captures no state (it only touches the static above), so it converts to a
    /// `@convention(c)` function pointer.
    static func install() {
        wolf_ffi_set_filesearch_wake({
            DispatchQueue.main.async {
                PickerWakeBridge.registeredPicker?.refresh()
            }
        })
    }
}
