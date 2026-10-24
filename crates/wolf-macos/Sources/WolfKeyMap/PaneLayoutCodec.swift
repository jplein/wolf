// Pure codec between the `PaneTree` layout and its persisted, FFI and Lua shapes.

import Foundation

/// A persistable, serializable mirror of a pane-split tree keyed by per-leaf
/// buffer path (the FFI + `state.json` shape). Either a leaf bound to a buffer
/// path (`""` for an unnamed buffer) or an n-ary split of child nodes along
/// `orientation` with a parallel per-child `weights` vector.
public enum SavedPaneNode: Hashable {
  /// A single pane showing the buffer at `path` (`""` for an unnamed buffer). When
  /// `preview` is `true` the pane is a Markdown preview and `path` is the SOURCE
  /// Markdown file's ABS path (restored via `wolf_preview_file`, not `:edit`).
  case leaf(path: String, preview: Bool)
  /// An n-ary split of `children` along `orientation`, with parallel per-child
  /// `weights`.
  case split(orientation: Orientation, weights: [Double], children: [SavedPaneNode])

  /// The number of leaves under this node (`1` for a leaf). The rebuild path zips
  /// exactly this many nvim window handles to the rebuilt leaves.
  public var leafCount: Int {
    switch self {
    case .leaf:
      return 1
    case .split(_, _, let children):
      return children.reduce(0) { $0 + $1.leafCount }
    }
  }

  /// The "suppress from persistence" predicate for a LONE pane: `true` iff this
  /// node is `.leaf(path: "", preview: false)` — an empty (unnamed), non-preview
  /// lone buffer. A preview leaf (`preview: true`), a leaf with a non-empty path,
  /// and any `.split(...)` all return `false`, so multi-pane layouts and lone
  /// panes bound to a file/preview are never suppressed.
  public var isEmptyLeaf: Bool {
    switch self {
    case .leaf(let path, let preview):
      return path.isEmpty && !preview
    case .split:
      return false
    }
  }
}

/// The pure layout codec. A namespace (`enum` with only static members),
/// mirroring the other `WolfKeyMap` seams.
public enum PaneLayoutCodec {
  /// One post-order token in the flattened stream the FFI push-builder replays.
  /// Each token carries the node's own weight within its parent split (the root's
  /// is `1.0`).
  public enum LayoutToken: Equatable {
    /// A leaf bound to `path` (a preview of `path` when `preview` is `true`),
    /// weighing `weight` within its parent split.
    case leaf(path: String, preview: Bool, weight: Double)
    /// A split along `orientation` of the top `childCount` stack nodes, weighing
    /// `weight` within its parent split.
    case split(orientation: Orientation, childCount: Int, weight: Double)
  }

  /// Maps a `PaneLayout` (leaf ids) into a `SavedPaneNode` (leaf paths), looking
  /// each leaf id up in `paths` (an unmapped leaf becomes an empty path) and marking
  /// it a `preview` leaf when its id is in `previews`.
  public static func encode(
    _ layout: PaneLayout, paths: [PaneId: String], previews: Set<PaneId>
  ) -> SavedPaneNode {
    switch layout {
    case .leaf(let id):
      return .leaf(path: paths[id] ?? "", preview: previews.contains(id))
    case .split(let orientation, let children, let weights):
      return .split(
        orientation: orientation, weights: weights,
        children: children.map { encode($0, paths: paths, previews: previews) })
    }
  }

  /// Flattens `node` into a post-order token stream (children before their split);
  /// the root carries weight `1.0`. `tokens.count == node count`.
  public static func postOrderTokens(_ node: SavedPaneNode) -> [LayoutToken] {
    var out: [LayoutToken] = []
    emit(node, weight: 1.0, into: &out)
    return out
  }

  /// Rebuilds a `SavedPaneNode` from a post-order token stream (the inverse of
  /// `postOrderTokens`), or `nil` when the stream is malformed.
  public static func decodeTokens(_ tokens: [LayoutToken]) -> SavedPaneNode? {
    var stack: [(node: SavedPaneNode, weight: Double)] = []
    for token in tokens {
      switch token {
      case .leaf(let path, let preview, let weight):
        stack.append((.leaf(path: path, preview: preview), weight))
      case .split(let orientation, let childCount, let weight):
        guard childCount >= 0, stack.count >= childCount else {
          return nil
        }
        let popped = Array(stack.suffix(childCount))
        stack.removeLast(childCount)
        stack.append(
          (
            .split(
              orientation: orientation, weights: popped.map { $0.weight },
              children: popped.map { $0.node }), weight
          ))
      }
    }
    guard stack.count == 1 else {
      return nil
    }
    return stack[0].node
  }

  /// Renders `node` to the JSON string `runtime/nvim/init.lua`'s
  /// `wolf_rebuild_layout` decodes: a leaf is
  /// `{ "leaf": true, "path": …, "preview": true|false }`; a split is
  /// `{ "orientation": "v"|"h", "weights": [...], "children": [...] }`.
  public static func specJSON(_ node: SavedPaneNode) -> String {
    let object = jsonObject(node)
    guard let data = try? JSONSerialization.data(withJSONObject: object),
      let string = String(data: data, encoding: .utf8)
    else {
      return "{}"
    }
    return string
  }

  // MARK: - Helpers

  /// Emits `node`'s post-order tokens into `out`, tagging it with `weight`.
  private static func emit(
    _ node: SavedPaneNode, weight: Double, into out: inout [LayoutToken]
  ) {
    switch node {
    case .leaf(let path, let preview):
      out.append(.leaf(path: path, preview: preview, weight: weight))
    case .split(let orientation, let weights, let children):
      for (child, childWeight) in zip(children, weights) {
        emit(child, weight: childWeight, into: &out)
      }
      out.append(
        .split(orientation: orientation, childCount: children.count, weight: weight))
    }
  }

  /// Builds the `JSONSerialization` object graph for `specJSON`.
  private static func jsonObject(_ node: SavedPaneNode) -> Any {
    switch node {
    case .leaf(let path, let preview):
      return ["leaf": true, "path": path, "preview": preview]
    case .split(let orientation, let weights, let children):
      let tag = orientation == .vertical ? "v" : "h"
      return [
        "orientation": tag,
        "weights": weights,
        "children": children.map(jsonObject),
      ]
    }
  }
}
