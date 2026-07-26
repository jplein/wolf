//
//  PaneLayoutCodecTests.swift
//  Pure, headless unit tests for the NEW `WolfKeyMap.PaneLayoutCodec` +
//  `SavedPaneNode` seam and `PaneTree.rebuilt(from:firstLeafId:)` (feature plan
//  §2 / §3.1). Foundation-only — no AppKit, no live Neovim.
//
//  The codec bridges the pure `PaneTree` layout to the persistable + FFI shapes:
//    * `encode(_:paths:)` maps a `PaneLayout` (whose leaves are `PaneId`s) into a
//      `SavedPaneNode` tree keyed by per-leaf buffer PATH (an unmapped leaf -> "").
//    * `postOrderTokens(_:)` flattens a `SavedPaneNode` to the post-order token
//      stream the FFI push-builder replays (leaf / split), and `decodeTokens(_:)`
//      is its inverse.
//    * `specJSON(_:)` renders a `SavedPaneNode` to the single JSON string the nvim
//      `wolf_rebuild_layout` Lua helper decodes (`{leaf,path}` /
//      `{orientation:"h"|"v",weights,children}`).
//    * `PaneTree.rebuilt(from:firstLeafId:)` rebuilds a live tree (topology +
//      weights) and hands back the DFS-ordered leaf ids.
//
//  CONTRACT / SIGNATURE ASSUMPTIONS (test-writer report; coder implements to match):
//    * `enum SavedPaneNode: Equatable { case leaf(path: String);
//        case split(orientation: Orientation, weights: [Double],
//                   children: [SavedPaneNode]) }`.
//    * `enum PaneLayoutCodec` with
//      `static func encode(_ layout: PaneLayout, paths: [PaneId: String]) -> SavedPaneNode`,
//      `static func postOrderTokens(_ node: SavedPaneNode) -> [LayoutToken]`,
//      `static func decodeTokens(_ tokens: [LayoutToken]) -> SavedPaneNode?`,
//      `static func specJSON(_ node: SavedPaneNode) -> String`.
//    * `PaneTree.rebuilt(from: SavedPaneNode, firstLeafId: PaneId)
//        -> (tree: PaneTree, leafIds: [PaneId])`.
//    * `specJSON` uses `"v"` for `.vertical` and `"h"` for `.horizontal` and the
//      JSON keys `leaf` / `path` / `orientation` / `weights` / `children`
//      (matching `runtime/nvim/init.lua`'s `wolf_rebuild_layout`). Only the parsed
//      object graph is asserted, so key ordering / float spelling is not pinned.
//

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
      children: [.leaf(path: "/a.md"), .leaf(path: "/b.md")])
  }

  private func nary() -> SavedPaneNode {
    .split(orientation: .horizontal, weights: [0.5, 0.25, 0.25],
      children: [.leaf(path: "/a.md"), .leaf(path: "/b.md"), .leaf(path: "/c.md")])
  }

  private func perpendicular() -> SavedPaneNode {
    .split(orientation: .vertical, weights: [0.6, 0.4],
      children: [
        .leaf(path: "/a.md"),
        .split(orientation: .horizontal, weights: [0.5, 0.5],
          children: [.leaf(path: "/b.md"), .leaf(path: "/c.md")]),
      ])
  }

  // MARK: - encode: leaf ids -> paths (unmapped leaf -> "")

  func testEncodeMapsLeafIdsToPaths() {
    let tree = PaneTree(root: p1)
    tree.split(p1, p2, .vertical)

    // Map P1, leave P2 unmapped so it must encode to "".
    let saved = PaneLayoutCodec.encode(tree.layout(), paths: [p1: "/notes/a.md"])
    XCTAssertEqual(
      saved,
      .split(orientation: .vertical, weights: [0.5, 0.5],
        children: [.leaf(path: "/notes/a.md"), .leaf(path: "")]),
      "encode substitutes each leaf id's path; an unmapped leaf becomes an empty path")
  }

  func testEncodeSingleLeaf() {
    let tree = PaneTree(root: p1)
    let saved = PaneLayoutCodec.encode(tree.layout(), paths: [p1: "/only.md"])
    XCTAssertEqual(saved, .leaf(path: "/only.md"), "a single-pane tree encodes to a leaf")
  }

  // MARK: - postOrderTokens <-> decodeTokens round-trips

  func testTokenRoundTripLeaf() {
    let node: SavedPaneNode = .leaf(path: "/x.md")
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
      PaneLayoutCodec.encode(tree.layout(), paths: paths), saved,
      "the rebuilt tree round-trips back to the saved topology + weights")
  }

  func testRebuiltSingleLeaf() {
    let saved: SavedPaneNode = .leaf(path: "/solo.md")
    let (tree, leafIds) = PaneTree.rebuilt(from: saved, firstLeafId: firstLeaf)
    XCTAssertEqual(leafIds, [firstLeaf], "a single-leaf saved tree yields one leaf id")
    XCTAssertEqual(tree.leaves(), [firstLeaf], "the rebuilt single-leaf tree has just that leaf")
  }

  // MARK: - specJSON: stable, parseable object graph

  func testSpecJsonLeaf() {
    let json = PaneLayoutCodec.specJSON(.leaf(path: "/tmp/only.md"))
    let obj = parseObject(json)
    XCTAssertEqual(obj["leaf"] as? Bool, true, "a leaf spec carries leaf: true")
    XCTAssertEqual(obj["path"] as? String, "/tmp/only.md", "a leaf spec carries its path")
  }

  func testSpecJsonVerticalSplit() {
    let saved: SavedPaneNode = .split(orientation: .vertical, weights: [0.5, 0.5],
      children: [.leaf(path: "/tmp/a.md"), .leaf(path: "/tmp/b.md")])
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
      children: [.leaf(path: "/a"), .leaf(path: "/b")])
    let obj = parseObject(PaneLayoutCodec.specJSON(saved))
    XCTAssertEqual(obj["orientation"] as? String, "h", ".horizontal serializes as \"h\"")
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
