// Pure codec between the `PaneTree` layout and its persisted and FFI shapes.

/// The per-pane view values a leaf persists: an edit pane's `cursor` and `topLine`, and a
/// preview's `scroll`, each `nil` when not captured (`specs/application-state.md#pane`).
public struct SavedPaneView: Hashable {
  /// A cursor position: `line` 1-based, `col` a 0-based byte column.
  public struct Cursor: Hashable {
    public let line: UInt32
    public let col: UInt32

    public init(line: UInt32, col: UInt32) {
      self.line = line
      self.col = col
    }
  }

  /// An edit pane's cursor.
  public var cursor: Cursor?
  /// An edit pane's 1-based first visible line.
  public var topLine: UInt32?
  /// A preview's scroll, the fraction of the page's scrollable height above the viewport.
  public var scroll: Double?

  public init(cursor: Cursor? = nil, topLine: UInt32? = nil, scroll: Double? = nil) {
    self.cursor = cursor
    self.topLine = topLine
    self.scroll = scroll
  }
}

/// A persistable, serializable mirror of a pane-split tree keyed by per-leaf
/// buffer path (the shape the window state is gathered into). Either a leaf bound to a buffer
/// path (`""` for an unnamed buffer) or an n-ary split of child nodes along
/// `orientation` with a parallel per-child `weights` vector.
public enum SavedPaneNode: Hashable {
  /// A single pane showing the buffer at `path` (`""` for an unnamed buffer). When
  /// `preview` is `true` the pane is a Markdown preview and `path` is the SOURCE
  /// Markdown file's ABS path (restored via `wolf_preview_file`, not `:edit`). When
  /// `terminal` is `true` the pane is a terminal pane and `path` is the directory its
  /// session starts in (`""` for the tabpage cwd). When `zoomed` is `true` the pane is its
  /// tab's zoomed pane, which the restore re-zooms. `view` is the pane's last captured
  /// cursor, top line or scroll.
  case leaf(
    path: String, preview: Bool, terminal: Bool = false, zoomed: Bool = false,
    view: SavedPaneView = SavedPaneView())
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

  /// The depth-first index of the first leaf marked `zoomed`, or `nil` when none is: the
  /// index into the leaf ids `PaneTree.rebuilt(from:firstLeafId:)` returns.
  public var zoomedLeafIndex: Int? {
    var index = 0
    func find(_ node: SavedPaneNode) -> Int? {
      switch node {
      case .leaf(_, _, _, let zoomed, _):
        defer { index += 1 }
        return zoomed ? index : nil
      case .split(_, _, let children):
        for child in children {
          if let found = find(child) { return found }
        }
        return nil
      }
    }
    return find(self)
  }

  /// Each leaf's `view`, in depth-first order.
  public var leafViews: [SavedPaneView] {
    switch self {
    case .leaf(_, _, _, _, let view):
      return [view]
    case .split(_, _, let children):
      return children.flatMap { $0.leafViews }
    }
  }

  /// The "suppress from persistence" predicate for a LONE pane: `true` iff this
  /// node is `.leaf(path: "", preview: false, terminal: false)`, whatever its `zoomed` and
  /// `view` —
  /// an empty (unnamed), non-preview lone buffer. A preview or terminal leaf, a leaf
  /// with a non-empty path, and any `.split(...)` all return `false`, so multi-pane
  /// layouts and lone panes bound to a file, a preview or a terminal are never suppressed.
  public var isEmptyLeaf: Bool {
    switch self {
    case .leaf(let path, let preview, let terminal, _, _):
      return path.isEmpty && !preview && !terminal
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
    /// A leaf bound to `path` (a preview of `path` when `preview` is `true`, a terminal
    /// pane starting in `path` when `terminal` is `true`, the tab's zoomed pane when
    /// `zoomed` is `true`) with its captured `view`, weighing `weight` within its parent split.
    case leaf(
      path: String, preview: Bool, terminal: Bool = false, zoomed: Bool = false,
      view: SavedPaneView = SavedPaneView(), weight: Double)
    /// A split along `orientation` of the top `childCount` stack nodes, weighing
    /// `weight` within its parent split.
    case split(orientation: Orientation, childCount: Int, weight: Double)
  }

  /// Maps a `PaneLayout` (leaf ids) into a `SavedPaneNode` (leaf paths), looking
  /// each leaf id up in `paths` (an unmapped leaf becomes an empty path) and marking
  /// it a `preview` leaf when its id is in `previews`, a `terminal` leaf when it is
  /// in `terminals`, and `zoomed` when it equals `zoomed`. A `zoomed` id that is no leaf
  /// of `layout`, such as the drawer's, marks nothing. Each leaf carries its id's entry in
  /// `views`, and an empty view when it has none.
  public static func encode(
    _ layout: PaneLayout, paths: [PaneId: String], previews: Set<PaneId>,
    terminals: Set<PaneId> = [], zoomed: PaneId? = nil, views: [PaneId: SavedPaneView] = [:]
  ) -> SavedPaneNode {
    switch layout {
    case .leaf(let id):
      return .leaf(
        path: paths[id] ?? "", preview: previews.contains(id),
        terminal: terminals.contains(id), zoomed: id == zoomed,
        view: views[id] ?? SavedPaneView())
    case .split(let orientation, let children, let weights):
      return .split(
        orientation: orientation, weights: weights,
        children: children.map {
          encode(
            $0, paths: paths, previews: previews, terminals: terminals, zoomed: zoomed,
            views: views)
        })
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
      case .leaf(let path, let preview, let terminal, let zoomed, let view, let weight):
        stack.append(
          (
            .leaf(path: path, preview: preview, terminal: terminal, zoomed: zoomed, view: view),
            weight
          ))
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

  // MARK: - Helpers

  /// Emits `node`'s post-order tokens into `out`, tagging it with `weight`.
  private static func emit(
    _ node: SavedPaneNode, weight: Double, into out: inout [LayoutToken]
  ) {
    switch node {
    case .leaf(let path, let preview, let terminal, let zoomed, let view):
      out.append(
        .leaf(
          path: path, preview: preview, terminal: terminal, zoomed: zoomed, view: view,
          weight: weight))
    case .split(let orientation, let weights, let children):
      for (child, childWeight) in zip(children, weights) {
        emit(child, weight: childWeight, into: &out)
      }
      out.append(
        .split(orientation: orientation, childCount: children.count, weight: weight))
    }
  }
}
