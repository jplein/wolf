//
//  PickerWakeBridge.swift
//  Marshals wolf-core's file-search wake (fired on its tokio worker thread,
//  carrying the firing core's per-window token) onto the AppKit main thread and
//  refreshes the RIGHT window's picker — the picker analog of `WakeBridge`.
//

import AppKit
import WolfFFI

/// Bridges wolf-core's file-search traversal wake to AppKit. Core invokes the
/// registered `extern "C"` callback (see `wolf_ffi_set_filesearch_wake`) from its
/// worker thread on each traversal batch with its per-window `token`; the thunk
/// below hops onto the main thread, looks the token up in `WindowRegistry`, and
/// asks that window's `PickerView` to re-rank/redraw. A late wake after the
/// picker has closed is harmless — `refresh()` is a no-op when closed. The wake
/// asks for a COALESCED refresh (`scheduleRefresh()`) so a large load's many
/// batch wakes collapse to at most one re-rank per frame; keystrokes still
/// refresh immediately.
enum PickerWakeBridge {
    /// Registers the C file-search wake callback with the Rust core once at
    /// launch. The closure captures no state (it only touches the global
    /// `WindowRegistry`), so it converts to a `@convention(c)` function pointer
    /// taking the token.
    static func install() {
        wolf_ffi_set_filesearch_wake({ token in
            DispatchQueue.main.async {
                WindowRegistry.shared.project(for: token)?.onPickerWake()
            }
        })
    }
}
