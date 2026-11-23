// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#panelayoutcodectestsswift

import Foundation
import XCTest

@testable import WolfKeyMap

final class PaneLayoutCodecTests: XCTestCase {

  private let p1: PaneId = 1
  private let p2: PaneId = 2
  private let p3: PaneId = 3

  /// The first leaf id used for `rebuilt` (distinct from the encode ids so a
  /// mistaken reuse would show).
  private let firstLeaf: PaneId = 10

  /// Counts every node (leaves + splits) in a `SavedPaneNode` tree.
  private func nodeCount(_ node: SavedPaneNode) -> Int {
    switch node {
    case .leaf:
      return 1
    case .split(_, _, let children):
      return 1 + children.reduce(0) { $0 + nodeCount($1) }
    }
  }

  // MARK: - Sample saved trees (topology + weights)

  private func twoChild() -> SavedPaneNode {
    .split(
      orientation: .vertical, weights: [0.5, 0.5],
      children: [.leaf(path: "/a.md", preview: false), .leaf(path: "/b.md", preview: false)])
  }

  private func nary() -> SavedPaneNode {
    .split(
      orientation: .horizontal, weights: [0.5, 0.25, 0.25],
      children: [
        .leaf(path: "/a.md", preview: false),
        .leaf(path: "/b.md", preview: false),
        .leaf(path: "/c.md", preview: false),
      ])
  }

  private func perpendicular() -> SavedPaneNode {
    .split(
      orientation: .vertical, weights: [0.6, 0.4],
      children: [
        .leaf(path: "/a.md", preview: false),
        .split(
          orientation: .horizontal, weights: [0.5, 0.5],
          children: [.leaf(path: "/b.md", preview: false), .leaf(path: "/c.md", preview: false)]),
      ])
  }

  /// A split whose FIRST child is a PREVIEW leaf (path = its source markdown file)
  /// and whose second child is a normal editor leaf.
  private func withPreview() -> SavedPaneNode {
    .split(
      orientation: .vertical, weights: [0.5, 0.5],
      children: [.leaf(path: "/src.md", preview: true), .leaf(path: "/b.md", preview: false)])
  }

  // MARK: - isEmptyLeaf: only a lone empty NON-preview leaf is "empty"

  func testIsEmptyLeafTrueForEmptyNonPreviewLeaf() {
    XCTAssertTrue(
      SavedPaneNode.leaf(path: "", preview: false).isEmptyLeaf,
      "an empty-path, non-preview leaf is the empty-state leaf")
  }

  func testIsEmptyLeafFalseForNamedLeaf() {
    XCTAssertFalse(
      SavedPaneNode.leaf(path: "/a.md", preview: false).isEmptyLeaf,
      "a leaf with a real path is NOT empty (its file must persist)")
  }

  func testIsEmptyLeafFalseForEmptyPreviewLeaf() {
    XCTAssertFalse(
      SavedPaneNode.leaf(path: "", preview: true).isEmptyLeaf,
      "an empty-path PREVIEW leaf is NOT empty (preview-ness must persist)")
  }

  func testIsEmptyLeafFalseForEmptyTerminalLeaf() {
    XCTAssertFalse(
      SavedPaneNode.leaf(path: "", preview: false, terminal: true).isEmptyLeaf,
      "a lone terminal leaf with no known cwd is NOT empty (the terminal pane must persist); "
        + "true means isEmptyLeaf ignores `terminal`")
  }

  func testIsEmptyLeafFalseForSplit() {
    XCTAssertFalse(
      twoChild().isEmptyLeaf,
      "a split is never the empty-state leaf")
  }

  // MARK: - encode: leaf ids -> paths (unmapped leaf -> "")

  func testEncodeMapsLeafIdsToPaths() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)

    // Map P1, leave P2 unmapped so it must encode to "". No previews here.
    let saved = PaneLayoutCodec.encode(
      tree.layout(), paths: [p1: "/notes/a.md"], previews: [])
    XCTAssertEqual(
      saved,
      .split(
        orientation: .vertical, weights: [0.5, 0.5],
        children: [.leaf(path: "/notes/a.md", preview: false), .leaf(path: "", preview: false)]),
      "encode substitutes each leaf id's path; an unmapped leaf becomes an empty path")
  }

  func testEncodeSingleLeaf() {
    let tree = PaneTree(root: p1)
    let saved = PaneLayoutCodec.encode(
      tree.layout(), paths: [p1: "/only.md"], previews: [])
    XCTAssertEqual(
      saved, .leaf(path: "/only.md", preview: false), "a single-pane tree encodes to a leaf")
  }

  // MARK: - encode: a leaf id in `previews` is marked preview: true (ROUND 2)

  func testEncodeMarksPreviewLeaves() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)

    // P1 previews its source Markdown and P2 is a normal editor leaf. The preview leaf's path is
    // the source file's, matching how the save path maps a preview window to its `source_path`.
    let saved = PaneLayoutCodec.encode(
      tree.layout(),
      paths: [p1: "/notes/source.md", p2: "/notes/b.md"],
      previews: [p1])
    XCTAssertEqual(
      saved,
      .split(
        orientation: .vertical, weights: [0.5, 0.5],
        children: [
          .leaf(path: "/notes/source.md", preview: true),
          .leaf(path: "/notes/b.md", preview: false),
        ]),
      "the leaf whose id is in `previews` encodes preview: true (path = source); the other false")
  }

  // MARK: - encode: a leaf id in `terminals` is marked terminal: true

  func testEncodeMarksTerminalLeaves() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)

    // P1 is a terminal pane whose path is the directory its session starts in.
    let saved = PaneLayoutCodec.encode(
      tree.layout(),
      paths: [p1: "/notes/sub", p2: "/notes/b.md"],
      previews: [],
      terminals: [p1])
    XCTAssertEqual(
      saved,
      .split(
        orientation: .vertical, weights: [0.5, 0.5],
        children: [
          .leaf(path: "/notes/sub", preview: false, terminal: true),
          .leaf(path: "/notes/b.md", preview: false),
        ]),
      "the leaf whose id is in `terminals` encodes terminal: true with its cwd; the other "
        + "false. terminal: false on the first means encode ignores `terminals`")
  }

  // MARK: - postOrderTokens <-> decodeTokens round-trips

  func testTokenRoundTripLeaf() {
    let node: SavedPaneNode = .leaf(path: "/x.md", preview: false)
    let tokens = PaneLayoutCodec.postOrderTokens(node)
    XCTAssertEqual(tokens.count, nodeCount(node), "one token for a single leaf")
    XCTAssertEqual(PaneLayoutCodec.decodeTokens(tokens), node, "leaf round-trips")
  }

  func testTokenRoundTripTwoChild() {
    let node = twoChild()
    let tokens = PaneLayoutCodec.postOrderTokens(node)
    XCTAssertEqual(tokens.count, nodeCount(node), "2 leaves + 1 split = 3 tokens")
    XCTAssertEqual(PaneLayoutCodec.decodeTokens(tokens), node, "2-child split round-trips")
  }

  func testTokenRoundTripNary() {
    let node = nary()
    let tokens = PaneLayoutCodec.postOrderTokens(node)
    XCTAssertEqual(tokens.count, nodeCount(node), "3 leaves + 1 split = 4 tokens")
    XCTAssertEqual(PaneLayoutCodec.decodeTokens(tokens), node, "n-ary split round-trips")
  }

  func testTokenRoundTripPerpendicularNested() {
    let node = perpendicular()
    let tokens = PaneLayoutCodec.postOrderTokens(node)
    XCTAssertEqual(tokens.count, nodeCount(node), "outer + inner splits + 3 leaves = 5 tokens")
    XCTAssertEqual(
      PaneLayoutCodec.decodeTokens(tokens), node,
      "a perpendicular nested tree round-trips (topology + weights)")
  }

  func testTokenRoundTripPreservesPreviewFlag() {
    // A preview leaf's `preview: true` must survive the post-order token round-trip
    // (the FFI push-builder replays these tokens, so the marker rides through).
    let node = withPreview()
    let tokens = PaneLayoutCodec.postOrderTokens(node)
    XCTAssertEqual(tokens.count, nodeCount(node), "2 leaves + 1 split = 3 tokens")
    XCTAssertEqual(
      PaneLayoutCodec.decodeTokens(tokens), node,
      "the preview leaf's preview: true survives the token round-trip")
  }

  func testTokenRoundTripKeepsATerminalLeaf() {
    let node: SavedPaneNode = .split(
      orientation: .vertical, weights: [0.5, 0.5],
      children: [
        .leaf(path: "/a.md", preview: false),
        .leaf(path: "/notes/sub", preview: false, terminal: true),
      ])
    let tokens = PaneLayoutCodec.postOrderTokens(node)
    XCTAssertEqual(tokens.count, nodeCount(node), "one token per node")
    XCTAssertEqual(
      PaneLayoutCodec.decodeTokens(tokens), node,
      "a terminal leaf must round-trip through the tokens with terminal: true; false means the "
        + "token drops the flag, so the pane restores as an editor of its directory")
  }

  // MARK: - rebuilt: topology + weights + DFS leaf ids

  func testRebuiltReproducesTopologyWeightsAndDfsIds() {
    let saved = perpendicular()
    let (tree, leafIds) = PaneTree.rebuilt(from: saved, firstLeafId: firstLeaf)

    // Leaf ids are assigned sequentially in DFS order from firstLeafId.
    XCTAssertEqual(
      leafIds, [firstLeaf, firstLeaf + 1, firstLeaf + 2],
      "rebuilt assigns leaf ids sequentially in DFS order from firstLeafId")
    // The rebuilt tree's own DFS leaf order matches the returned list.
    XCTAssertEqual(tree.leaves(), leafIds, "tree.leaves() equals the returned DFS leaf-id list")

    // Re-encoding the rebuilt tree (mapping each DFS leaf id back to its original
    // path) reproduces the ORIGINAL saved node — proving topology AND weights.
    let paths = Dictionary(uniqueKeysWithValues: zip(leafIds, ["/a.md", "/b.md", "/c.md"]))
    XCTAssertEqual(
      PaneLayoutCodec.encode(tree.layout(), paths: paths, previews: []), saved,
      "the rebuilt tree round-trips back to the saved topology + weights")
  }

  func testRebuiltSingleLeaf() {
    let saved: SavedPaneNode = .leaf(path: "/solo.md", preview: false)
    let (tree, leafIds) = PaneTree.rebuilt(from: saved, firstLeafId: firstLeaf)
    XCTAssertEqual(leafIds, [firstLeaf], "a single-leaf saved tree yields one leaf id")
    XCTAssertEqual(tree.leaves(), [firstLeaf], "the rebuilt single-leaf tree has just that leaf")
  }

  // MARK: - The zoomed leaf (P1 to P3)

  /// The drawer's id: never a layout leaf, as in the container.
  private let drawerId: PaneId = 9

  /// P1: `encode` marks the leaf whose id is `zoomed` and no other; a `zoomed` id that is not a
  /// leaf of the layout, the drawer's, marks none.
  func testEncodeMarksOnlyTheZoomedLeaf() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p2, p3, .vertical)
    let paths: [PaneId: String] = [p1: "/notes/a.md", p2: "/notes/b.md", p3: "/notes/c.md"]
    let unmarked: [SavedPaneNode] = [
      .leaf(path: "/notes/a.md", preview: false),
      .leaf(path: "/notes/b.md", preview: false),
      .leaf(path: "/notes/c.md", preview: false),
    ]
    guard case .split(_, _, let thirds) = tree.layout() else {
      return XCTFail("premise: splitting 1 and then 2 vertically must give a split")
    }

    XCTAssertEqual(
      PaneLayoutCodec.encode(tree.layout(), paths: paths, previews: [], zoomed: p2),
      .split(
        orientation: .vertical, weights: thirds,
        children: [
          unmarked[0], .leaf(path: "/notes/b.md", preview: false, zoomed: true), unmarked[2],
        ]),
      "zoomed on 2, only the middle leaf may carry zoomed: true. No mark means encode ignores"
        + " `zoomed` and the zoom is never saved; a mark on 1 or 3 too means it marks every leaf")
    XCTAssertEqual(
      PaneLayoutCodec.encode(tree.layout(), paths: paths, previews: [], zoomed: drawerId),
      .split(orientation: .vertical, weights: thirds, children: unmarked),
      "zoomed on the drawer, which is not a layout leaf, no leaf may be marked: the drawer's"
        + " zoom persists as SavedDrawer.zoomed, never as a leaf's mark")
  }

  /// P2: `zoomedLeafIndex` is the depth-first index of the first marked leaf, which the restore
  /// zips onto its rebuilt leaf ids.
  func testZoomedLeafIndexIsTheDepthFirstIndex() {
    func node(_ marked: Set<String>) -> SavedPaneNode {
      func leaf(_ path: String) -> SavedPaneNode {
        .leaf(path: path, preview: false, zoomed: marked.contains(path))
      }
      let inner: SavedPaneNode = .split(
        orientation: .horizontal, weights: [0.5, 0.5], children: [leaf("/b.md"), leaf("/c.md")])
      return .split(
        orientation: .vertical, weights: [0.5, 0.5], children: [leaf("/a.md"), inner])
    }
    XCTAssertEqual(
      node(["/b.md"]).zoomedLeafIndex, 1,
      "b, the first leaf of the nested split, is depth-first leaf 1; 0 means the index counts"
        + " within its split, nil that the mark is not found")
    XCTAssertEqual(
      node(["/c.md"]).zoomedLeafIndex, 2,
      "c is depth-first leaf 2; 1 means the index is the top-level child's")
    XCTAssertEqual(
      node(["/b.md", "/c.md"]).zoomedLeafIndex, 1,
      "with two marks, from a hand edit, the first in depth-first order wins, as init.lua's")
    XCTAssertNil(node([]).zoomedLeafIndex, "an unmarked layout has no zoomed leaf")
  }

  /// P3: the zoom mark survives the post-order tokens the FFI push-builder replays.
  func testTheZoomedFlagRoundTripsThroughTokens() {
    let node: SavedPaneNode = .split(
      orientation: .vertical, weights: [0.5, 0.5],
      children: [
        .leaf(path: "/a.md", preview: false),
        .leaf(path: "/notes/sub", preview: false, terminal: true, zoomed: true),
      ])
    let tokens = PaneLayoutCodec.postOrderTokens(node)
    XCTAssertEqual(
      tokens[1],
      .leaf(path: "/notes/sub", preview: false, terminal: true, zoomed: true, weight: 0.5),
      "the zoomed leaf's token must carry zoomed: true; false loses the zoom on its way to the"
        + " FFI push-builder")
    XCTAssertEqual(
      PaneLayoutCodec.decodeTokens(tokens), node,
      "a zoomed leaf must round-trip through the tokens with its mark")
  }

  // MARK: - Per-leaf views (S5)

  /// S5: `encode(views:)` puts each pane's view on its own leaf, and the view survives the
  /// post-order tokens the FFI push-builder replays.
  func testViewsRideEncodeAndTheTokenRoundTrip() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)
    tree.split(p2, p3, .horizontal)
    let paths: [PaneId: String] = [p1: "/notes/a.md", p2: "/notes/a.md", p3: "/notes/c.md"]
    let edited = SavedPaneView(cursor: .init(line: 12, col: 4), topLine: 7)
    let scrolled = SavedPaneView(scroll: 0.25)

    let node = PaneLayoutCodec.encode(
      tree.layout(), paths: paths, previews: [p2], views: [p1: edited, p2: scrolled])
    XCTAssertEqual(
      node.leafViews, [edited, scrolled, SavedPaneView()],
      "each leaf must carry its own pane's view, and pane 3, which has none, an empty one."
        + " Empty views throughout mean encode ignores `views`; swapped ones that it keys them"
        + " by something other than the leaf's id")
    XCTAssertTrue(
      SavedPaneNode.leaf(path: "", preview: false, view: edited).isEmptyLeaf,
      "a lone unnamed edit leaf is empty whatever its view, so a cursor on the startup buffer"
        + " does not make an empty tab persist a layout")

    XCTAssertEqual(
      PaneLayoutCodec.decodeTokens(PaneLayoutCodec.postOrderTokens(node)), node,
      "every leaf must round-trip through the tokens with its view; an empty view on leaf 1 or"
        + " 2 means a token's view is dropped on emit or on decode")
  }
}
