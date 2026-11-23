// Marshals the open channel's wake onto the main thread, where the coordinator drains it.

import AppKit
import WolfFFI

/// Bridges the open listener's wake to AppKit. The listener calls the registered `extern "C"`
/// callback (see `wolf_ffi_set_open_wake`) from a connection's thread after queueing each
/// request; the thunk hops onto the main thread and asks the app delegate to handle the
/// queued requests. The listener is process-wide, so the token carries nothing.
enum OpenWakeBridge {
  /// Registers the C open wake callback with the Rust side once at launch. The closure
  /// captures no state, so it converts to a `@convention(c)` function pointer.
  static func install() {
    wolf_ffi_set_open_wake({ _ in
      DispatchQueue.main.async {
        (NSApp.delegate as? AppDelegate)?.handleOpenRequests()
      }
    })
  }
}
