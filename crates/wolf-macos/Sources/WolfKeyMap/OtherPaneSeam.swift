//
//  OtherPaneSeam.swift
//  Pure, AppKit-free "which other pane?" resolution seam.
//
//  The sibling of `CommandSeam` / `DrawerFocusSeam`: a pure, display-free
//  decision exercised with a `PaneTree` and scalar bounds only — NO AppKit, NO
//  WKWebView, NO live Neovim. It answers the one front-end question behind
//  "Preview in Another Pane" / "Edit in Another Pane": given the active pane and
//  a preferred side, which OTHER pane should receive the bind — an existing
//  pane, or a fresh split? The split+bind mechanics themselves are core + Lua.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap`
//  module alongside `PaneTree.swift` / `CommandSeam.swift`.
//

import Foundation

/// Resolves the OTHER pane that should receive an other-pane bind. A namespace
/// (`enum` with only static members).
public enum OtherPaneSeam {
  /// The side the command prefers: Preview in Another Pane prefers `.right`,
  /// Edit in Another Pane prefers `.left`.
  public enum Side {
    /// Prefer the pane to the left of the active pane.
    case left
    /// Prefer the pane to the right of the active pane.
    case right
  }

  /// Where the bind should land: a fresh split (the Lua helper splits and binds
  /// atomically) or an existing pane.
  public enum Target: Equatable {
    /// No existing pane can take the bind — ask core to split.
    case split
    /// Reuse this existing pane.
    case existing(PaneId)
  }

  /// Resolves the target pane for an other-pane bind. Purely geometric — no
  /// buffer-content preference.
  ///
  /// Rules, in order:
  /// 1. `active` is not a leaf of `tree` (unknown pane / the drawer) → `nil`
  ///    (the command is a no-op, never a stray split).
  /// 2. The tree is a single leaf → `.split` (no other pane exists).
  /// 3. `tree.neighbor(active, dir, width:height:)` resolves for the preferred
  ///    side (`.right` → `FocusDir.right`, `.left` → `FocusDir.left`) →
  ///    `.existing` of that pane.
  /// 4. Deterministic DFS-adjacent fallback WITH WRAP over `tree.leaves()`:
  ///    `.right` → the next leaf after `active` (wrapping to the first);
  ///    `.left` → the previous leaf (wrapping to the last).
  ///
  /// `width` / `height` must be the SAME bounds pair the caller's focus
  /// resolution uses (the drawer-reduced editor height), so rule 3's geometry
  /// matches what the user sees.
  public static func target(
    active: PaneId, prefer: Side, tree: PaneTree, width: Double, height: Double
  ) -> Target? {
    let leaves = tree.leaves()
    guard let activeIndex = leaves.firstIndex(of: active) else { return nil }
    if leaves.count == 1 {
      return .split
    }

    let dir: FocusDir = prefer == .right ? .right : .left
    if let neighbor = tree.neighbor(active, dir, width: width, height: height) {
      return .existing(neighbor)
    }

    let offset = prefer == .right ? 1 : leaves.count - 1
    let fallback = leaves[(activeIndex + offset) % leaves.count]
    return .existing(fallback)
  }
}
