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

  private let eps: Double = 1e-9

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
    .split(orientation: .vertical, weights: [0.5, 0.5],
      children: [.leaf(path: "/a.md", preview: false), .leaf(path: "/b.md", preview: false)])
  }

  private func nary() -> SavedPaneNode {
    .split(orientation: .horizontal, weights: [0.5, 0.25, 0.25],
      children: [
        .leaf(path: "/a.md", preview: false),
        .leaf(path: "/b.md", preview: false),
        .leaf(path: "/c.md", preview: false),
      ])
  }

  private func perpendicular() -> SavedPaneNode {
    .split(orientation: .vertical, weights: [0.6, 0.4],
      children: [
        .leaf(path: "/a.md", preview: false),
        .split(orientation: .horizontal, weights: [0.5, 0.5],
          children: [.leaf(path: "/b.md", preview: false), .leaf(path: "/c.md", preview: false)]),
      ])
  }

  /// A split whose FIRST child is a PREVIEW leaf (path = its source markdown file)
  /// and whose second child is a normal editor leaf.
  private func withPreview() -> SavedPaneNode {
    .split(orientation: .vertical, weights: [0.5, 0.5],
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
      .split(orientation: .vertical, weights: [0.5, 0.5],
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
      .split(orientation: .vertical, weights: [0.5, 0.5],
        children: [
          .leaf(path: "/notes/source.md", preview: true),
          .leaf(path: "/notes/b.md", preview: false),
        ]),
      "the leaf whose id is in `previews` encodes preview: true (path = source); the other false")
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

  // MARK: - specJSON: stable, parseable object graph

  func testSpecJsonLeaf() {
    let json = PaneLayoutCodec.specJSON(.leaf(path: "/tmp/only.md", preview: false))
    let obj = parseObject(json)
    XCTAssertEqual(obj["leaf"] as? Bool, true, "a leaf spec carries leaf: true")
    XCTAssertEqual(obj["path"] as? String, "/tmp/only.md", "a leaf spec carries its path")
    XCTAssertEqual(obj["preview"] as? Bool, false, "a non-preview leaf spec carries preview: false")
  }

  func testSpecJsonVerticalSplit() {
    let saved: SavedPaneNode = .split(orientation: .vertical, weights: [0.5, 0.5],
      children: [.leaf(path: "/tmp/a.md", preview: false), .leaf(path: "/tmp/b.md", preview: false)])
    let obj = parseObject(PaneLayoutCodec.specJSON(saved))

    XCTAssertEqual(obj["orientation"] as? String, "v", ".vertical serializes as \"v\"")
    let weights = obj["weights"] as? [Double] ?? []
    XCTAssertEqual(weights.count, 2, "two weights")
    XCTAssertTrue(approxEqual(weights, [0.5, 0.5]), "weights are 0.5 / 0.5")

    let children = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(children.count, 2, "two children")
    XCTAssertEqual(children[0]["leaf"] as? Bool, true, "child 0 is a leaf")
    XCTAssertEqual(children[0]["path"] as? String, "/tmp/a.md", "child 0 path")
    XCTAssertEqual(children[1]["path"] as? String, "/tmp/b.md", "child 1 path")
  }

  func testSpecJsonHorizontalOrientation() {
    let saved: SavedPaneNode = .split(orientation: .horizontal, weights: [0.5, 0.5],
      children: [.leaf(path: "/a", preview: false), .leaf(path: "/b", preview: false)])
    let obj = parseObject(PaneLayoutCodec.specJSON(saved))
    XCTAssertEqual(obj["orientation"] as? String, "h", ".horizontal serializes as \"h\"")
  }

  // MARK: - specJSON: the preview flag reaches the nvim rebuild spec (ROUND 2)

  func testSpecJsonCarriesPreviewFlag() {
    // Each leaf serializes `"preview"` beside the "v"/"h" and leaf/path keys, so Lua's
    // `wolf_rebuild_layout` recreates a preview of the source instead of editing a blank buffer.
    let obj = parseObject(PaneLayoutCodec.specJSON(withPreview()))

    XCTAssertEqual(obj["orientation"] as? String, "v", ".vertical serializes as \"v\"")
    let weights = obj["weights"] as? [Double] ?? []
    XCTAssertTrue(approxEqual(weights, [0.5, 0.5]), "weights are 0.5 / 0.5")

    let children = obj["children"] as? [[String: Any]] ?? []
    XCTAssertEqual(children.count, 2, "two children")

    // Child 0: the preview leaf.
    XCTAssertEqual(children[0]["leaf"] as? Bool, true, "child 0 is a leaf")
    XCTAssertEqual(children[0]["path"] as? String, "/src.md", "child 0 carries the SOURCE path")
    XCTAssertEqual(
      children[0]["preview"] as? Bool, true, "the preview leaf serializes preview: true")

    // Child 1: a normal editor leaf.
    XCTAssertEqual(children[1]["path"] as? String, "/b.md", "child 1 path")
    XCTAssertEqual(
      children[1]["preview"] as? Bool, false, "a normal leaf serializes preview: false")
  }

  // MARK: - JSON helpers

  /// Parses `json` into a `[String: Any]`, failing the test on any error.
  private func parseObject(
    _ json: String, file: StaticString = #filePath, line: UInt = #line
  ) -> [String: Any] {
    guard let data = json.data(using: .utf8),
      let obj = try? JSONSerialization.jsonObject(with: data),
      let dict = obj as? [String: Any]
    else {
      XCTFail("specJSON did not produce a JSON object: \(json)", file: file, line: line)
      return [:]
    }
    return dict
  }

  private func approxEqual(_ a: [Double], _ b: [Double]) -> Bool {
    a.count == b.count && zip(a, b).allSatisfy { abs($0 - $1) < eps }
  }
}
