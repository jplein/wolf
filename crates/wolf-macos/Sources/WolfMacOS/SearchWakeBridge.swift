// Marshals the content-search wake onto the main thread for the owning project.

import AppKit
import WolfFFI

/// Bridges wolf-core's rg-streaming search wake to AppKit. Core invokes the
/// registered `extern "C"` callback (see `wolf_ffi_set_search_wake`) from its
/// worker thread on each streamed batch with its per-PROJECT `token`; the thunk
/// below hops onto the main thread, looks the token up in `WindowRegistry` to
/// resolve the owning PROJECT, and asks it (`onSearchWake()`) to fan a COALESCED
/// refresh out to ALL of its attached views' `SearchView`s. Each view's
/// `scheduleRefresh()` is version-gated, so idle panels no-op.
enum SearchWakeBridge {
  /// Registers the C content-search wake callback with the Rust core once at
  /// launch. The closure captures no state (only the global `WindowRegistry`), so
  /// it converts to a `@convention(c)` function pointer taking the token.
  static func install() {
    wolf_ffi_set_search_wake({ token in
      DispatchQueue.main.async {
        WindowRegistry.shared.project(for: token)?.onSearchWake()
      }
    })
  }
}
