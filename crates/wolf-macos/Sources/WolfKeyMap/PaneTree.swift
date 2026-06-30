//
//  PaneTree.swift
//  Pure, AppKit-free pane-layout tree for the macOS split-panes UI.
//
//  A FAITHFUL port of `crates/wolf-gnome/src/layout.rs::PaneTree`, the macOS copy
//  of that pure seam (like `KeyMap.swift` / `Cursor.swift` / `PickerSeam.swift`).
//  It imports ONLY Foundation (uses `Double`, never `CGFloat`/CoreGraphics) so it
//  stays in the headless `WolfKeyMap` module and is unit-tested without a window
//  (see `PaneTreeTests.swift`). The AppKit glue (`PaneContainerView`) consumes
//  `paneRects()` (and `layout()`) to paint each pane into a computed rectangle in a
//  single flat view.
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
//  CONTRACT (must match wolf-gnome/src/layout.rs exactly): a same-orientation split
//  inserts a FLAT sibling AFTER the target then re-equalizes; a perpendicular split
//  NESTS `[target, newPane]`; remove never empties the tree; neighbor uses STRICTLY
//  positive cross-axis overlap; `leaves()`/`paneRects()` are a stable DFS order;
//  `.vertical` divides WIDTH, `.horizontal` divides HEIGHT.
//

import Foundation

/// A pane identifier. A plain `UInt64` alias (not a newtype) so the tree's
/// `leaves()` / `paneRects()` results compare directly against literal ids.
public typealias PaneId = UInt64

/// The default split weight for a fresh two-child split: each child gets half the
/// bounds. A user drag of a divider is owned by the widget layer (and not written
/// back to these weights in v1), not this pure tree.
private let defaultRatio: Double = 0.5

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

    /// Creates a tree of a single leaf pane `root`.
    public init(root: PaneId) {
        self.root = .leaf(root)
    }

    /// Splits the `target` leaf with `newPane` along `orientation`.
    ///
    /// If the split's `orientation` MATCHES the frame that owns `target`, `newPane`
    /// is inserted as a FLAT sibling immediately AFTER `target` and that row/column
    /// re-equalizes. Otherwise `target` is wrapped in a new two-child sub-frame
    /// `[target, newPane]` (NEST), with `target` first. The root special case (a
    /// single leaf) becomes a two-child split. No-op if `target` is not a leaf.
    public func split(_ target: PaneId, _ newPane: PaneId, _ orientation: Orientation) {
        if case let .leaf(id) = root {
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
}

// MARK: - Free helpers (ported 1:1 from layout.rs)

/// Recursively mirrors `frame` into a `PaneLayout`.
private func frameLayout(_ frame: Frame) -> PaneLayout {
    switch frame {
    case let .leaf(id):
        return .leaf(id)
    case let .split(orientation, children, weights):
        return .split(
            orientation: orientation,
            children: children.map(frameLayout),
            weights: weights
        )
    }
}

/// Equalizes `weights` to `1/n` each, where `n = weights.count`.
private func equalize(_ weights: inout [Double]) {
    let n = weights.count
    if n == 0 {
        return
    }
    let each = 1.0 / Double(n)
    for i in weights.indices {
        weights[i] = each
    }
}

/// Recursively splits the leaf matching `target` within a split `frame`. A
/// same-orientation split inserts a flat sibling after `target` and re-equalizes; a
/// perpendicular split wraps `target` in a new two-child sub-frame. Returns `true`
/// if a matching leaf was found and split.
private func splitIn(
    _ frame: inout Frame, _ target: PaneId, _ newPane: PaneId, _ orientation: Orientation
) -> Bool {
    guard case .split(let own, var children, var weights) = frame else {
        return false
    }

    if let i = children.firstIndex(where: { isLeaf($0, target) }) {
        if own == orientation {
            // FLAT: insert the new pane immediately after the target, re-equalize.
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
        if case let .split(childOrient, grandchildren, grandWeights) = child,
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

    // COLLAPSE a 1-child split into its sole child; else re-equalize the row/column.
    if children.count == 1 {
        frame = children[0]
    } else {
        equalize(&weights)
        frame = .split(orientation: orientation, children: children, weights: weights)
    }
}

/// Appends every leaf id under `frame` to `out` in DFS (child-order) order.
private func collectLeaves(_ frame: Frame, _ out: inout [PaneId]) {
    switch frame {
    case let .leaf(id):
        out.append(id)
    case let .split(_, children, _):
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
    case let .leaf(id):
        out.append((id, rect))
    case let .split(orientation, children, weights):
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

/// Whether `frame` is exactly the leaf `id`.
private func isLeaf(_ frame: Frame, _ id: PaneId) -> Bool {
    if case let .leaf(leafId) = frame {
        return leafId == id
    }
    return false
}
