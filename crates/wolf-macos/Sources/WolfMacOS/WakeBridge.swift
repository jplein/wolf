// Marshals the render wake onto the main thread and repaints the right window.

import AppKit
import WolfFFI

/// Bridges wolf-core's redraw wake to AppKit. Core invokes the registered
/// `extern "C"` wake (see `wolf_ffi_set_wake`) from its worker thread with its
/// per-window `token`; the thunk below hops onto the main thread, looks the
/// token up in `WindowRegistry`, and drives that window's `WolfWindowController`
/// render-wake orchestration (which applies layout intents + redraws every pane,
/// the cmdline strip, and the file-tree sidebar).
enum WakeBridge {
  /// Registers the C wake callback with the Rust core once at launch. The
  /// closure captures no state (it only touches the global `WindowRegistry`),
  /// so it converts to a `@convention(c)` function pointer taking the token.
  static func install() {
    wolf_ffi_set_wake({ token in
      DispatchQueue.main.async {
        WindowRegistry.shared.project(for: token)?.onWake()
      }
    })
  }
}
