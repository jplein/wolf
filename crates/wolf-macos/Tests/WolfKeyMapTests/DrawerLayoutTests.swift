//
//  DrawerLayoutTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.DrawerLayout`.
//
//  These exercise the terminal-drawer GEOMETRY seam the coder adds in
//  `Sources/WolfKeyMap/DrawerLayout.swift` (terminal-drawer FIX plan §4/§7). Like
//  `EditorInsetTests` / `PaneGeometryTests`, they import ONLY Foundation-level test
//  machinery (no AppKit / CoreGraphics, no live Neovim) and stay in the headless
//  `WolfKeyMap` module. `PaneContainerView`'s wiring of this seam is window-server
//  AppKit glue and is compile + manual-smoke only; all headless coverage lands here.
//  They reuse the `eps` / `approx` + `assertRectEqual` style from `EditorInsetTests`.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor, plan §4)
//
//    public struct DrawerLayout {
//        public static let defaultWeight: Double = 1.0 / 3.0
//        public let editorHeight: Double   // hasDrawer ? max(0, paneAreaHeight - drawerHeight)
//                                          //           : paneAreaHeight
//        public let drawerHeight: Double   // hasDrawer ? (paneAreaHeight*weight).rounded() : 0
//        public init(paneAreaHeight: Double, hasDrawer: Bool, weight: Double)
//        public func drawerRect(width: Double) -> Rect?  // full-width strip at y=editorHeight,
//                                                        //   h=drawerHeight; nil when !hasDrawer
//    }
//
//  `Rect` is the EXISTING value type from `PaneTree.swift` (`Rect(x:y:w:h:)`,
//  `Double` fields). The drawer is a FULL-WIDTH root-bottom strip: the editor tree
//  tiles over `editorHeight`, the drawer occupies `[editorHeight, paneAreaHeight]`.
//
//  NOTE (coder): `DrawerLayout` does NOT exist yet — this file will not compile
//  until the coder adds it (by design). Test-writer references it as the contract.
//

import XCTest

@testable import WolfKeyMap

final class DrawerLayoutTests: XCTestCase {

  /// Tolerance for comparing the `Double` rect / scalar components (ported from
  /// `EditorInsetTests.swift`).
  private let eps: Double = 1e-9

  /// A representative pane-area height (px) evenly divisible by 3, so the
  /// one-third drawer weight lands on an exact integer with no rounding ambiguity.
  private let evenPaneAreaHeight: Double = 600.0

  /// A pane-area height (px) NOT divisible by 3, to exercise the `.rounded()` in
  /// `drawerHeight`: 601/3 == 200.333… -> 200.
  private let oddPaneAreaHeight: Double = 601.0

  /// A full canvas width (px) for the drawer strip.
  private let width: Double = 1000.0

  /// A deliberately tiny pane-area height (px) to pin the non-negative floor.
  private let tinyPaneAreaHeight: Double = 2.0

  // MARK: - Helpers (ported from EditorInsetTests' `approx` / `assertRectEqual`)

  /// Returns whether two doubles are within `eps`.
  private func approx(_ a: Double, _ b: Double) -> Bool {
    abs(a - b) < eps
  }

  // MARK: - defaultWeight constant pinned

  func testDefaultWeightIsOneThird() {
    XCTAssertEqual(
      DrawerLayout.defaultWeight, 1.0 / 3.0, accuracy: eps,
      "DrawerLayout.defaultWeight is pinned at 1/3")
  }

  // MARK: - No drawer: editor fills the whole pane area, no drawer rect

  func testNoDrawerFillsEditorHeightAndNilRect() {
    let dl = DrawerLayout(
      paneAreaHeight: evenPaneAreaHeight, hasDrawer: false,
      weight: DrawerLayout.defaultWeight)

    XCTAssertEqual(
      dl.editorHeight, evenPaneAreaHeight, accuracy: eps,
      "without a drawer the editor fills the full pane-area height")
    XCTAssertEqual(
      dl.drawerHeight, 0.0, accuracy: eps,
      "without a drawer the drawer height is 0")
    XCTAssertNil(
      dl.drawerRect(width: width),
      "without a drawer there is no drawer rect")
  }

  // MARK: - With drawer: one-third strip at the bottom, editor above

  func testWithDrawerOneThirdBottomStrip() {
    let dl = DrawerLayout(
      paneAreaHeight: evenPaneAreaHeight, hasDrawer: true,
      weight: DrawerLayout.defaultWeight)

    // drawerHeight == (600 * 1/3).rounded() == 200; editor takes the rest.
    let expectedDrawerHeight = (evenPaneAreaHeight * DrawerLayout.defaultWeight).rounded()
    XCTAssertEqual(
      dl.drawerHeight, expectedDrawerHeight, accuracy: eps,
      "drawerHeight == round(paneAreaHeight * weight)")
    XCTAssertEqual(
      dl.editorHeight, evenPaneAreaHeight - dl.drawerHeight, accuracy: eps,
      "editorHeight == paneAreaHeight - drawerHeight")

    // The drawer rect is a full-width strip sitting directly below the editor.
    guard let rect = dl.drawerRect(width: width) else {
      XCTFail("a drawer layout must produce a drawer rect")
      return
    }
    XCTAssertEqual(rect.x, 0.0, accuracy: eps, "drawer rect is left-aligned (x == 0)")
    XCTAssertEqual(rect.w, width, accuracy: eps, "drawer rect is full-width (w == width)")
    XCTAssertEqual(
      rect.y, dl.editorHeight, accuracy: eps,
      "drawer rect sits at y == editorHeight (below the editor)")
    XCTAssertEqual(
      rect.h, dl.drawerHeight, accuracy: eps,
      "drawer rect height == drawerHeight")

    // The editor + drawer heights tile the whole pane area exactly.
    XCTAssertEqual(
      dl.editorHeight + dl.drawerHeight, evenPaneAreaHeight, accuracy: eps,
      "editor + drawer heights tile the full pane area")
  }

  // MARK: - Rounding: an odd height still rounds the one-third strip

  func testWithDrawerRoundsHeight() {
    let dl = DrawerLayout(
      paneAreaHeight: oddPaneAreaHeight, hasDrawer: true,
      weight: DrawerLayout.defaultWeight)

    XCTAssertEqual(
      dl.drawerHeight, (oddPaneAreaHeight * DrawerLayout.defaultWeight).rounded(),
      accuracy: eps, "drawerHeight rounds paneAreaHeight * weight to a whole pixel")
    XCTAssertEqual(
      dl.editorHeight + dl.drawerHeight, oddPaneAreaHeight, accuracy: eps,
      "editor + drawer still tile the full (odd) pane area")
  }

  // MARK: - Edge: a tiny pane area keeps both heights non-negative

  func testTinyPaneAreaStaysNonNegative() {
    let dl = DrawerLayout(
      paneAreaHeight: tinyPaneAreaHeight, hasDrawer: true,
      weight: DrawerLayout.defaultWeight)

    XCTAssertGreaterThanOrEqual(
      dl.editorHeight, 0.0, "editorHeight is never negative even for a tiny pane area")
    XCTAssertGreaterThanOrEqual(
      dl.drawerHeight, 0.0, "drawerHeight is never negative even for a tiny pane area")
  }
}
