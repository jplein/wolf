// Marshals the file-tree wake onto the main thread for the tab whose core fired it.

import AppKit
import WolfFFI

/// Bridges wolf-core's file-tree traversal wake to AppKit. Core invokes the
/// registered `extern "C"` callback (see `wolf_ffi_set_tree_wake`) from its
/// worker thread on each traversal batch with its `token`, which names one tab's
/// core; the thunk below hops onto the main thread, looks the token up in
/// `WindowRegistry`, and asks that tab's `TabCore.onTreeWake()` to refresh its
/// `FileTreeView`. The wake asks for a COALESCED refresh (`scheduleRefresh()`) so a
/// large load's many batch wakes collapse to at most one reload/rebuild per frame;
/// the render-wake reveal path (`onRenderWake()`) still refreshes immediately. A late
/// wake after a tree has closed is harmless — `refresh()` is a no-op when closed.
enum TreeWakeBridge {
  /// Registers the C file-tree wake callback with the Rust core once at launch.
  /// The closure captures no state (it only touches the global
  /// `WindowRegistry`), so it converts to a `@convention(c)` function pointer
  /// taking the token.
  static func install() {
    wolf_ffi_set_tree_wake({ token in
      DispatchQueue.main.async {
        WindowRegistry.shared.tabCore(for: token)?.onTreeWake()
      }
    })
  }
}
