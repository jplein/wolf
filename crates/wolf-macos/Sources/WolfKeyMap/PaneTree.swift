//
//  PaneTree.swift
//  Pure, AppKit-free pane-layout tree for the macOS split-panes UI.
//
//  A port of `crates/wolf-gnome/src/layout.rs::PaneTree`, the macOS copy of that
//  pure seam (like `KeyMap.swift` / `Cursor.swift` / `PickerSeam.swift`). It
//  imports ONLY Foundation (uses `Double`, never `CGFloat`/CoreGraphics) so it
//  stays in the headless `WolfKeyMap` module and is unit-tested without a window
//  (see `PaneTreeTests.swift` / `PaneTreeWeightsTests.swift`). The AppKit glue
//  (`PaneContainerView`) consumes `paneRects()` (and `layout()`) to paint each
//  pane into a computed rectangle in a single flat view, and `dividers()` +
//  `setPairFraction()` to drag interior dividers to resize.
//
//  ## N-ary frame model (matches Neovim's window layout)
//
//  A `Frame` is either a leaf (one pane) or an n-ary split: an `Orientation` (a ROW
//  of side-by-side children for `.vertical`, a COLUMN of stacked children for
//  `.horizontal`) plus per-child `weights` (summing to 1) driving the divider
//  positions. Invariants (upheld by `PaneTree`'s operations): a split has `>= 2`
//  children; `weights.count == children.count`; and no split sits directly under a
//  same-orientation split (those are FLATTENED), so each axis is a single flat row/
//  column.
//
//  ## Intentional divergence from the gnome 1:1 port (macOS-only)
//
//  The TOPOLOGY contract still matches `wolf-gnome/src/layout.rs` exactly: a
//  same-orientation split inserts a FLAT sibling AFTER the target; a perpendicular
//  split NESTS `[target, newPane]`; remove never empties the tree; neighbor uses
//  STRICTLY positive cross-axis overlap; `leaves()`/`paneRects()` are a stable DFS
//  order; `.vertical` divides WIDTH, `.horizontal` divides HEIGHT.
//
//  The WEIGHT POLICY and the DIVIDER API are macOS-only additions. A
//  same-orientation split RE-EQUALIZES the row/column (each child gets `1/n` — this
//  MATCHES the gnome port: `:vsplit`/`:split` redistributes to equal shares); a
//  fresh two-child split is 0.5/0.5. The ONE remaining divergence from gnome is
//  REMOVE: it RE-NORMALIZES survivors PROPORTIONALLY (relative sizes preserved, NOT
//  re-equalized), so a user-resized layout survives an adjacent pane closing.
//  Together with `dividers()` / `setPairFraction()` / `rebuilt(from:firstLeafId:)`
//  these drive divider drag-to-resize and full layout persistence.
//

import Foundation

/// A pane identifier. A plain `UInt64` alias (not a newtype) so the tree's
/// `leaves()` / `paneRects()` results compare directly against literal ids.
public typealias PaneId = UInt64

/// The default split weight for a fresh two-child split: each child gets half the
/// bounds. A user drag of a divider is applied back into these weights via
/// `setPairFraction(path:boundary:leftFraction:)`.
private let defaultRatio: Double = 0.5

/// The clamp margin `setPairFraction` keeps between a divider fraction and the
/// `0` / `1` extremes so neither adjacent pane can fully collapse. The pixel-aware
/// minimum-pane clamp lives in `DividerDragSeam`; this is only the pure-tree floor.
private let pairFractionEps: Double = 1e-6

/// Tolerance for the geometric edge/overlap comparisons in `PaneTree.neighbor`. It
/// makes the cross-axis overlap STRICTLY positive (a shared edge does not count) so
/// a clean 2x2 grid has unambiguous neighbours.
private let neighborEps: Double = 1e-9

/// The axis along which an internal node splits its bounds.
///
/// Discriminants align with the FFI table (`wolf-ffi`'s `mod orientation`):
/// `horizontal == 0`, `vertical == 1`. A `.horizontal` split stacks its panes
/// top/bottom (dividing HEIGHT); a `.vertical` split places them side by side
/// (dividing WIDTH).
public enum Orientation {
  /// Panes stacked top/bottom; the divider runs horizontally and splits HEIGHT.
  case horizontal
  /// Panes side by side; the divider runs vertically and splits WIDTH.
  case vertical

  /// Maps an FFI orientation discriminant (`0` → `.horizontal`, `1` →
  /// `.vertical`, else `nil`), guarding the contract shared with `wolf-ffi`.
  public init?(ffi: UInt8) {
    switch ffi {
    case 0: self = .horizontal
    case 1: self = .vertical
    default: return nil
    }
  }
}

/// A direction for a focus move (`<C-w>h/j/k/l` and the arrow variants).
///
/// Discriminants align with the FFI table (`wolf-ffi`'s `mod focus_dir`):
/// `left == 0`, `right == 1`, `up == 2`, `down == 3`.
public enum FocusDir {
  /// Move focus to the pane on the left.
  case left
  /// Move focus to the pane on the right.
  case right
  /// Move focus to the pane above.
  case up
  /// Move focus to the pane below.
  case down

  /// Maps an FFI focus-direction discriminant (`0` → `.left`, `1` → `.right`,
  /// `2` → `.up`, `3` → `.down`, else `nil`).
  public init?(ffi: UInt8) {
    switch ffi {
    case 0: self = .left
    case 1: self = .right
    case 2: self = .up
    case 3: self = .down
    default: return nil
    }
  }
}

/// An axis-aligned pixel rectangle within a pane-area's bounds.
public struct Rect: Equatable {
  /// Left edge, in device pixels.
  public let x: Double
  /// Top edge, in device pixels.
  public let y: Double
  /// Width, in device pixels.
  public let w: Double
  /// Height, in device pixels.
  public let h: Double

  /// Creates a rectangle from its components.
  public init(x: Double, y: Double, w: Double, h: Double) {
    self.x = x
    self.y = y
    self.w = w
    self.h = h
  }
}

/// A read-only mirror of the `PaneTree` structure, produced by `PaneTree.layout()`.
/// The widget layer (`PaneContainerView`) consumes `paneRects()` (and this mirror)
/// to paint each pane into a computed rectangle in a single flat view.
public indirect enum PaneLayout: Equatable {
  /// A single pane.
  case leaf(PaneId)
  /// An n-ary split of child layouts along `orientation`, with parallel per-child
  /// `weights` (summing to 1).
  case split(orientation: Orientation, children: [PaneLayout], weights: [Double])
}

/// One node of the n-ary pane-layout tree: either a leaf (a single pane) or a split
/// of two-or-more child frames along an `Orientation`, with a parallel per-child
/// `weights` vector (summing to 1). Private to this file — the public mirror is
/// `PaneLayout`.
private indirect enum Frame {
  case leaf(PaneId)
  case split(orientation: Orientation, children: [Frame], weights: [Double])
}

/// An n-ary tree of editor panes (Neovim's frame model).
///
/// Leaves are panes; internal nodes are flat rows/columns of child frames. The tree
/// is NEVER empty (it always has at least its root leaf). A reference type (mutated
/// in place), mirroring the GNOME `PaneTree` held behind interior mutability.
public final class PaneTree {
  private var root: Frame

  /// One interior divider between two adjacent children of a split node, produced
  /// by `dividers(width:height:)` for the widget layer to hit-test + drag.
  ///
  /// `path` is the child-index path root → owning split; `boundary` is the index
  /// `i` between child `i` and child `i + 1`. `lineRect` is a ZERO-thickness
  /// segment (a `.vertical` divider has `w == 0` and spans the split's height at
  /// the boundary x; a `.horizontal` divider has `h == 0` and spans the split's
  /// width at the boundary y) — the hit tolerance supplies the clickable width.
  /// `pairStart` / `pairSpan` are the pixel extent of ONLY the two adjacent
  /// children the divider governs (the same pair `setPairFraction` reweights);
  /// `fraction == weights[boundary] / (weights[boundary] + weights[boundary + 1])`.
  /// `before` / `after` are the first-DFS leaf of each of those two children (the
  /// two leaf ids themselves when both children are leaves).
  public struct Divider: Equatable {
    /// The owning split's orientation (`.vertical` → a vertical divider line).
    public let orientation: Orientation
    /// The child-index path from the root to the owning split.
    public let path: [Int]
    /// The boundary index `i` (between child `i` and child `i + 1`).
    public let boundary: Int
    /// The zero-thickness divider line rect (see the type doc).
    public let lineRect: Rect
    /// The pixel start (x for `.vertical`, y for `.horizontal`) of the governed pair.
    public let pairStart: Double
    /// The pixel span of the governed pair (both children combined).
    public let pairSpan: Double
    /// The first-DFS leaf id of child `boundary`.
    public let before: PaneId
    /// The first-DFS leaf id of child `boundary + 1`.
    public let after: PaneId
    /// `weights[boundary] / (weights[boundary] + weights[boundary + 1])`.
    public let fraction: Double

    /// Creates a divider from its components (the widget layer + tests build these).
    public init(
      orientation: Orientation, path: [Int], boundary: Int, lineRect: Rect,
      pairStart: Double, pairSpan: Double, before: PaneId, after: PaneId, fraction: Double
    ) {
      self.orientation = orientation
      self.path = path
      self.boundary = boundary
      self.lineRect = lineRect
      self.pairStart = pairStart
      self.pairSpan = pairSpan
      self.before = before
      self.after = after
      self.fraction = fraction
    }
  }

  /// Creates a tree of a single leaf pane `root`.
  public init(root: PaneId) {
    self.root = .leaf(root)
  }

  /// Splits the `target` leaf with `newPane` along `orientation`.
  ///
  /// If the split's `orientation` MATCHES the frame that owns `target`, `newPane`
  /// is inserted as a FLAT sibling immediately AFTER `target` and that row/column
  /// RE-EQUALIZES (each child `1/n`). Otherwise `target` is wrapped in a new two-child sub-frame
  /// `[target, newPane]` (NEST), with `target` first. The root special case (a
  /// single leaf) becomes a two-child split. No-op if `target` is not a leaf.
  public func split(_ target: PaneId, _ newPane: PaneId, _ orientation: Orientation) {
    if case .leaf(let id) = root {
      if id == target {
        root = .split(
          orientation: orientation,
          children: [.leaf(target), .leaf(newPane)],
          weights: [defaultRatio, defaultRatio]
        )
      }
      return
    }
    _ = splitIn(&root, target, newPane, orientation)
  }

  /// Removes the leaf `pane`, then NORMALIZES the tree (flatten same-orientation
  /// children, collapse a 1-child split into its child, re-equalize). Removing the
  /// last remaining pane is a no-op (the tree is never empty).
  public func remove(_ pane: PaneId) {
    if leaves().count <= 1 {
      return
    }
    if removeIn(&root, pane) {
      normalize(&root)
    }
  }

  /// Returns every leaf pane id in a deterministic DFS order (child0 before
  /// child1, i.e. left-to-right / top-to-bottom).
  public func leaves() -> [PaneId] {
    var out: [PaneId] = []
    collectLeaves(root, &out)
    return out
  }

  /// Returns each leaf's pixel rectangle within bounds `(0, 0, width, height)`, in
  /// the same DFS order as `leaves()`. The rects tile the bounds exactly (no
  /// overlap; union equals the full area).
  public func paneRects(width: Double, height: Double) -> [(PaneId, Rect)] {
    var out: [(PaneId, Rect)] = []
    let bounds = Rect(x: 0, y: 0, w: width, h: height)
    collectRects(root, bounds, &out)
    return out
  }

  /// Returns the geometric neighbour of `from` in `dir`: the nearest pane in that
  /// direction whose rect STRICTLY overlaps `from` on the cross axis (Left/Right
  /// overlap on Y; Up/Down overlap on X). Returns `nil` at an edge or for an
  /// unknown pane.
  public func neighbor(
    _ from: PaneId, _ dir: FocusDir, width: Double, height: Double
  ) -> PaneId? {
    let rects = paneRects(width: width, height: height)
    guard let fromRect = rects.first(where: { $0.0 == from })?.1 else {
      return nil
    }

    var best: (PaneId, Double)?
    for (id, r) in rects {
      if id == from {
        continue
      }
      guard let distance = directionalDistance(fromRect, r, dir) else {
        continue
      }
      if best == nil || distance < best!.1 {
        best = (id, distance)
      }
    }
    return best?.0
  }

  /// Returns a read-only description of the tree's structure for the widget layer
  /// to paint each pane from (the internal `Frame` stays private).
  public func layout() -> PaneLayout {
    frameLayout(root)
  }

  /// Returns one `Divider` per interior boundary within bounds
  /// `(0, 0, width, height)`, mirroring `paneRects()`'s recursion so each divider's
  /// `lineRect` / `pairStart` / `pairSpan` are in the same pixel space.
  public func dividers(width: Double, height: Double) -> [Divider] {
    var out: [Divider] = []
    collectDividers(root, Rect(x: 0, y: 0, w: width, h: height), [], &out)
    return out
  }

  /// Reweights ONLY the two adjacent children at `boundary` within the split
  /// reached by the child-index `path`, so child `boundary` takes `leftFraction`
  /// of their combined weight and child `boundary + 1` takes the rest.
  ///
  /// The pair's combined weight (and every other child's weight, and the global
  /// sum) is unchanged. `leftFraction` is clamped to `[pairFractionEps,
  /// 1 - pairFractionEps]` so neither pane collapses. A no-op when `path` does not
  /// reach a split or `boundary` is out of range.
  public func setPairFraction(path: [Int], boundary: Int, leftFraction: Double) {
    setPairFractionIn(&root, path[...], boundary, leftFraction)
  }

  /// Rebuilds a live tree from a saved layout (topology + weights), assigning leaf
  /// ids sequentially in DFS order starting at `firstLeafId`.
  ///
  /// Returns the tree plus the ordered leaf-id list — the SAME DFS order as
  /// `leaves()` — so the caller can zip each id to its restored window handle.
  public static func rebuilt(
    from saved: SavedPaneNode, firstLeafId: PaneId
  ) -> (tree: PaneTree, leafIds: [PaneId]) {
    var nextId = firstLeafId
    var leafIds: [PaneId] = []
    func build(_ node: SavedPaneNode) -> Frame {
      switch node {
      case .leaf:
        let id = nextId
        nextId += 1
        leafIds.append(id)
        return .leaf(id)
      case .split(let orientation, let weights, let children):
        return .split(
          orientation: orientation, children: children.map(build), weights: weights)
      }
    }
    let root = build(saved)
    let tree = PaneTree(root: firstLeafId)
    tree.root = root
    return (tree, leafIds)
  }
}

// MARK: - Free helpers (ported 1:1 from layout.rs)

/// Recursively mirrors `frame` into a `PaneLayout`.
private func frameLayout(_ frame: Frame) -> PaneLayout {
  switch frame {
  case .leaf(let id):
    return .leaf(id)
  case .split(let orientation, let children, let weights):
    return .split(
      orientation: orientation,
      children: children.map(frameLayout),
      weights: weights
    )
  }
}

/// Recursively splits the leaf matching `target` within a split `frame`. A
/// same-orientation split inserts a flat sibling after `target` and re-equalizes the
/// row/column (each child `1/n`); a perpendicular split wraps `target` in a new
/// two-child sub-frame. Returns `true` if a matching leaf was found and split.
private func splitIn(
  _ frame: inout Frame, _ target: PaneId, _ newPane: PaneId, _ orientation: Orientation
) -> Bool {
  guard case .split(let own, var children, var weights) = frame else {
    return false
  }

  if let i = children.firstIndex(where: { isLeaf($0, target) }) {
    if own == orientation {
      // FLAT: insert the new pane immediately after the target, then RE-EQUALIZE the
      // whole row/column to `1/n` each — `:vsplit`/`:split` redistributes to equal
      // shares (matches the gnome port).
      children.insert(.leaf(newPane), at: i + 1)
      weights.insert(0.0, at: i + 1)
      equalize(&weights)
    } else {
      // NEST: wrap the target leaf in a new perpendicular two-child frame.
      children[i] = .split(
        orientation: orientation,
        children: [.leaf(target), .leaf(newPane)],
        weights: [defaultRatio, defaultRatio]
      )
    }
    frame = .split(orientation: own, children: children, weights: weights)
    return true
  }

  for idx in children.indices where splitIn(&children[idx], target, newPane, orientation) {
    frame = .split(orientation: own, children: children, weights: weights)
    return true
  }
  return false
}

/// Recursively removes the leaf `pane` (and its weight) from the split that owns it.
/// Returns `true` if `pane` was found and removed. Caller normalizes after.
private func removeIn(_ frame: inout Frame, _ pane: PaneId) -> Bool {
  guard case .split(let orientation, var children, var weights) = frame else {
    return false
  }

  if let i = children.firstIndex(where: { isLeaf($0, pane) }) {
    children.remove(at: i)
    weights.remove(at: i)
    frame = .split(orientation: orientation, children: children, weights: weights)
    return true
  }

  for idx in children.indices where removeIn(&children[idx], pane) {
    frame = .split(orientation: orientation, children: children, weights: weights)
    return true
  }
  return false
}

/// Bottom-up normalization after a remove: recurse children first, then FLATTEN any
/// same-orientation child split into this one (scaling its weights), COLLAPSE a
/// single remaining child into this frame, else re-EQUALIZE the weights.
private func normalize(_ frame: inout Frame) {
  guard case .split(let orientation, var children, var weights) = frame else {
    return
  }

  for idx in children.indices {
    normalize(&children[idx])
  }

  // FLATTEN: splice any same-orientation child's children + scaled weights in.
  var newChildren: [Frame] = []
  var newWeights: [Double] = []
  for (child, w) in zip(children, weights) {
    if case .split(let childOrient, let grandchildren, let grandWeights) = child,
      childOrient == orientation
    {
      for (gc, gw) in zip(grandchildren, grandWeights) {
        newChildren.append(gc)
        newWeights.append(gw * w)
      }
    } else {
      newChildren.append(child)
      newWeights.append(w)
    }
  }
  children = newChildren
  weights = newWeights

  // COLLAPSE a 1-child split into its sole child; else RE-NORMALIZE the survivors
  // PROPORTIONALLY (sum back to 1 while preserving relative sizes) — the macOS
  // weight-preserving divergence from the gnome re-equalize.
  if children.count == 1 {
    frame = children[0]
  } else {
    let sum = weights.reduce(0, +)
    if sum > 0 {
      for i in weights.indices {
        weights[i] /= sum
      }
    }
    frame = .split(orientation: orientation, children: children, weights: weights)
  }
}

/// Appends every leaf id under `frame` to `out` in DFS (child-order) order.
private func collectLeaves(_ frame: Frame, _ out: inout [PaneId]) {
  switch frame {
  case .leaf(let id):
    out.append(id)
  case .split(_, let children, _):
    for child in children {
      collectLeaves(child, &out)
    }
  }
}

/// Appends each leaf's `(id, rect)` under `frame` (occupying `rect`) to `out`,
/// dividing `rect` among the children by cumulative normalized weight fractions
/// (`.vertical` divides WIDTH; `.horizontal` divides HEIGHT), in DFS order.
private func collectRects(_ frame: Frame, _ rect: Rect, _ out: inout [(PaneId, Rect)]) {
  switch frame {
  case .leaf(let id):
    out.append((id, rect))
  case .split(let orientation, let children, let weights):
    let total = weights.reduce(0, +)
    var offset = 0.0
    for (child, w) in zip(children, weights) {
      let frac = total > 0 ? w / total : 0
      let childRect: Rect
      switch orientation {
      case .vertical:
        let cw = rect.w * frac
        childRect = Rect(x: rect.x + offset, y: rect.y, w: cw, h: rect.h)
        offset += cw
      case .horizontal:
        let ch = rect.h * frac
        childRect = Rect(x: rect.x, y: rect.y + offset, w: rect.w, h: ch)
        offset += ch
      }
      collectRects(child, childRect, &out)
    }
  }
}

/// Returns the non-negative gap distance from `from` to candidate `cand` in `dir`
/// if `cand` lies in that direction AND strictly overlaps `from` on the cross axis;
/// otherwise `nil`. A smaller distance means a nearer neighbour.
private func directionalDistance(_ from: Rect, _ cand: Rect, _ dir: FocusDir) -> Double? {
  switch dir {
  case .left:
    guard cand.x + cand.w <= from.x + neighborEps, yOverlap(from, cand) else {
      return nil
    }
    return from.x - (cand.x + cand.w)
  case .right:
    guard cand.x + neighborEps >= from.x + from.w, yOverlap(from, cand) else {
      return nil
    }
    return cand.x - (from.x + from.w)
  case .up:
    guard cand.y + cand.h <= from.y + neighborEps, xOverlap(from, cand) else {
      return nil
    }
    return from.y - (cand.y + cand.h)
  case .down:
    guard cand.y + neighborEps >= from.y + from.h, xOverlap(from, cand) else {
      return nil
    }
    return cand.y - (from.y + from.h)
  }
}

/// Whether `a` and `b` overlap with strictly positive extent on the Y axis.
private func yOverlap(_ a: Rect, _ b: Rect) -> Bool {
  max(a.y, b.y) < min(a.y + a.h, b.y + b.h) - neighborEps
}

/// Whether `a` and `b` overlap with strictly positive extent on the X axis.
private func xOverlap(_ a: Rect, _ b: Rect) -> Bool {
  max(a.x, b.x) < min(a.x + a.w, b.x + b.w) - neighborEps
}

/// Re-equalizes `weights` so each of the `n` children gets `1/n` (a no-op on an
/// empty vector). Used by the same-orientation `splitIn` branch to redistribute a
/// row/column to equal shares on `:vsplit`/`:split`.
private func equalize(_ weights: inout [Double]) {
  guard !weights.isEmpty else {
    return
  }
  let share = 1.0 / Double(weights.count)
  for i in weights.indices {
    weights[i] = share
  }
}

/// Whether `frame` is exactly the leaf `id`.
private func isLeaf(_ frame: Frame, _ id: PaneId) -> Bool {
  if case .leaf(let leafId) = frame {
    return leafId == id
  }
  return false
}

/// The first leaf id encountered under `frame` in DFS (child-order) order.
private func firstLeafOf(_ frame: Frame) -> PaneId {
  switch frame {
  case .leaf(let id):
    return id
  case .split(_, let children, _):
    return children.first.map(firstLeafOf) ?? 0
  }
}

/// Appends one `Divider` per interior boundary under `frame` (occupying `rect` at
/// child-index `path`) to `out`, then recurses, mirroring `collectRects`.
private func collectDividers(
  _ frame: Frame, _ rect: Rect, _ path: [Int], _ out: inout [PaneTree.Divider]
) {
  guard case .split(let orientation, let children, let weights) = frame else {
    return
  }

  // Child rects, exactly as collectRects divides them (cumulative weight fractions).
  let total = weights.reduce(0, +)
  var childRects: [Rect] = []
  var offset = 0.0
  for w in weights {
    let frac = total > 0 ? w / total : 0
    let childRect: Rect
    switch orientation {
    case .vertical:
      let cw = rect.w * frac
      childRect = Rect(x: rect.x + offset, y: rect.y, w: cw, h: rect.h)
      offset += cw
    case .horizontal:
      let ch = rect.h * frac
      childRect = Rect(x: rect.x, y: rect.y + offset, w: rect.w, h: ch)
      offset += ch
    }
    childRects.append(childRect)
  }

  // One divider per interior boundary i (between child i and child i + 1).
  if children.count >= 2 {
    for i in 0..<(children.count - 1) {
      let a = childRects[i]
      let b = childRects[i + 1]
      let pairSum = weights[i] + weights[i + 1]
      let fraction = pairSum > 0 ? weights[i] / pairSum : defaultRatio
      let lineRect: Rect
      let pairStart: Double
      let pairSpan: Double
      switch orientation {
      case .vertical:
        lineRect = Rect(x: a.x + a.w, y: rect.y, w: 0, h: rect.h)
        pairStart = a.x
        pairSpan = a.w + b.w
      case .horizontal:
        lineRect = Rect(x: rect.x, y: a.y + a.h, w: rect.w, h: 0)
        pairStart = a.y
        pairSpan = a.h + b.h
      }
      out.append(
        PaneTree.Divider(
          orientation: orientation, path: path, boundary: i, lineRect: lineRect,
          pairStart: pairStart, pairSpan: pairSpan,
          before: firstLeafOf(children[i]), after: firstLeafOf(children[i + 1]),
          fraction: fraction))
    }
  }

  for (idx, child) in children.enumerated() {
    collectDividers(child, childRects[idx], path + [idx], &out)
  }
}

/// Navigates `frame` by the child-index `path` to a split, then reweights the two
/// adjacent children at `boundary` (see `PaneTree.setPairFraction`). A no-op when
/// the path does not reach a split or the boundary is out of range.
private func setPairFractionIn(
  _ frame: inout Frame, _ path: ArraySlice<Int>, _ boundary: Int, _ leftFraction: Double
) {
  guard case .split(let orientation, var children, var weights) = frame else {
    return
  }

  if let idx = path.first {
    guard idx >= 0, idx < children.count else {
      return
    }
    setPairFractionIn(&children[idx], path.dropFirst(), boundary, leftFraction)
    frame = .split(orientation: orientation, children: children, weights: weights)
    return
  }

  guard boundary >= 0, boundary + 1 < weights.count else {
    return
  }
  let pairSum = weights[boundary] + weights[boundary + 1]
  let clamped = min(max(leftFraction, pairFractionEps), 1.0 - pairFractionEps)
  weights[boundary] = clamped * pairSum
  weights[boundary + 1] = (1.0 - clamped) * pairSum
  frame = .split(orientation: orientation, children: children, weights: weights)
}
