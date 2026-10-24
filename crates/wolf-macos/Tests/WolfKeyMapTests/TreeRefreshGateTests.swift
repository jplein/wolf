// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#treerefreshgatetestsswift

import XCTest

@testable import WolfKeyMap

final class TreeRefreshGateTests: XCTestCase {

  /// Number of repeated `requestRefresh()` calls while hidden.
  private let kHiddenRequestCount = 5

  /// While hidden, `requestRefresh()` defers (false) every time; the first
  /// `setVisible(true)` catches up EXACTLY once (true), and an immediate
  /// second `setVisible(true)` with no new deferred work returns false.
  func testHiddenDefersAndVisibleCatchesUpOnce() {
    let gate = TreeRefreshGate(isVisible: false)

    for _ in 0..<kHiddenRequestCount {
      XCTAssertFalse(
        gate.requestRefresh(),
        "requestRefresh() must return false (defer) while hidden")
    }

    XCTAssertTrue(
      gate.setVisible(true),
      "the first hidden->visible transition WITH deferred work must catch up (true)")
    XCTAssertFalse(
      gate.setVisible(true),
      "a second setVisible(true) with no new deferred work must return false")
  }

  /// While visible, `requestRefresh()` proceeds (true); a no-op
  /// `setVisible(true)` (already visible, no deferred work) returns false.
  func testVisibleRequestProceeds() {
    let gate = TreeRefreshGate(isVisible: true)

    XCTAssertTrue(
      gate.requestRefresh(),
      "requestRefresh() must return true (proceed) while visible")
    XCTAssertFalse(
      gate.setVisible(true),
      "setVisible(true) while already visible (no deferred work) must return false")
  }

  /// `shouldReload(version:)` gates on version change: a new version reloads
  /// (true) and records itself, an unchanged version does not (false), and
  /// `resetReloadTracking()` clears the record so the same version reloads
  /// again.
  func testShouldReloadGatesOnVersion() {
    let gate = TreeRefreshGate(isVisible: true)

    XCTAssertTrue(gate.shouldReload(version: 1), "first version 1 must reload")
    XCTAssertFalse(gate.shouldReload(version: 1), "unchanged version 1 must not reload")
    XCTAssertTrue(gate.shouldReload(version: 2), "advanced version 2 must reload")

    gate.resetReloadTracking()
    XCTAssertTrue(
      gate.shouldReload(version: 2),
      "after resetReloadTracking() the same version 2 must reload again")
  }

  /// While hidden the gate never advances reload tracking; after the
  /// visibility catch-up, a subsequent request + shouldReload reloads.
  func testHiddenNeverReloadsUntilVisible() {
    let hiddenVersion: UInt64 = 3
    let gate = TreeRefreshGate(isVisible: false)

    // Hidden: requests defer and reload tracking must not advance, so the version is still fresh
    // for shouldReload once visible.
    for _ in 0..<kHiddenRequestCount {
      XCTAssertFalse(gate.requestRefresh(), "hidden request must defer")
    }

    // Become visible (catch-up).
    XCTAssertTrue(
      gate.setVisible(true),
      "hidden->visible with deferred work must catch up once")

    // Now visible: request proceeds and the version reloads (tracking was
    // never advanced while hidden).
    XCTAssertTrue(gate.requestRefresh(), "visible request proceeds")
    XCTAssertTrue(
      gate.shouldReload(version: hiddenVersion),
      "the version must still reload after catch-up (hidden never consumed it)")
  }
}
