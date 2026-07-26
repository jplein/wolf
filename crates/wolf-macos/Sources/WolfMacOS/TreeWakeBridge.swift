//
//  TreeWakeBridge.swift
//  Marshals wolf-core's file-TREE wake (fired on its tokio worker thread,
//  carrying the firing core's per-PROJECT token) onto the AppKit main thread and
//  asks that project to fan a COALESCED refresh out to ALL of its attached views'
//  sidebars — the tree analog of `PickerWakeBridge`.
//

import AppKit
import WolfFFI

/// Bridges wolf-core's file-tree traversal wake to AppKit. Core invokes the
/// registered `extern "C"` callback (see `wolf_ffi_set_tree_wake`) from its
/// worker thread on each traversal batch with its per-PROJECT `token`; the thunk
/// below hops onto the main thread, looks the token up in `WindowRegistry` to
/// resolve the owning PROJECT, and asks it (`onTreeWake()`) to fan a refresh out
/// to ALL of its attached views' `FileTreeView`s — not just one window — because
/// the per-project token carries no view discriminator and every view runs its
/// own independent `start_tree_search()` traversal that must reload/rebuild its
/// rows. Each view's wake asks for a COALESCED refresh (`scheduleRefresh()`) so a
/// large load's many batch wakes collapse to at most one reload/rebuild per frame,
/// and the fan-out is cheap/idempotent (each `refresh()` is version-gated); the
/// render-wake reveal path (`onRenderWake()`) still refreshes immediately. A late
/// wake after a tree has closed is harmless — `refresh()` is a no-op when closed.
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
