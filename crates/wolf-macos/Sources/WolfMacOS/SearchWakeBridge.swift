// Marshals the content-search wake onto the main thread for the tab whose core fired it.

import AppKit
import WolfFFI

/// Bridges wolf-core's rg-streaming search wake to AppKit. Core invokes the
/// registered `extern "C"` callback (see `wolf_ffi_set_search_wake`) from its
/// worker thread on each streamed batch with its `token`, which names one tab's
/// core; the thunk below hops onto the main thread, looks the token up in
/// `WindowRegistry`, and asks that tab's `TabCore.onSearchWake()` for a COALESCED
/// refresh of its `SearchView`. `scheduleRefresh()` is version-gated, so an idle
/// panel no-ops.
enum SearchWakeBridge {
  /// Registers the C content-search wake callback with the Rust core once at
  /// launch. The closure captures no state (only the global `WindowRegistry`), so
  /// it converts to a `@convention(c)` function pointer taking the token.
  static func install() {
    wolf_ffi_set_search_wake({ token in
      DispatchQueue.main.async {
        WindowRegistry.shared.tabCore(for: token)?.onSearchWake()
      }
    })
  }
}
