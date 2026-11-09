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
}
