// Marshals the render wake onto the main thread for the tab whose core fired it.

import AppKit
import WolfFFI

/// Bridges wolf-core's redraw wake to AppKit. Core invokes the registered
/// `extern "C"` wake (see `wolf_ffi_set_wake`) from its worker thread with its
/// `token`, which names one tab's core; the thunk below hops onto the main
/// thread, looks the token up in `WindowRegistry`, and hands the wake to that
/// tab's `TabCore.onWake()` (liveness overlay, render, state commit, command drain).
enum WakeBridge {
  /// Registers the C wake callback with the Rust core once at launch. The
  /// closure captures no state (it only touches the global `WindowRegistry`),
  /// so it converts to a `@convention(c)` function pointer taking the token.
  static func install() {
    wolf_ffi_set_wake({ token in
      DispatchQueue.main.async {
        WindowRegistry.shared.tabCore(for: token)?.onWake()
      }
    })
  }
}
