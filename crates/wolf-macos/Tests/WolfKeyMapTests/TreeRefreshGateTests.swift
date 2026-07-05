//
//  TreeRefreshGateTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.TreeRefreshGate` seam.
//
//  This is the redundant-work gate behind the file-tree sidebar (plan Layer 3):
//  a VISIBILITY gate (defer all outline/children work while the sidebar is
//  collapsed/hidden; run ONE catch-up refresh when it reappears) plus a VERSION
//  gate (reloadData only when the snapshot version actually advanced). Together
//  they stop the render-wake storm from doing full reload/reveal work on every
//  wake — and stop it entirely while the sidebar is hidden (which is why hiding
//  the sidebar did not previously help).
//
//  These tests exercise the pure seam directly — NO AppKit, NO outline view —
//  exactly the headless style of the other WolfKeyMap seam tests
//  (PickerRefreshCoalescerTests etc.).
//
//  NOTE to coder: this references the pure `WolfKeyMap.TreeRefreshGate` the coder
//  adds in `Sources/WolfKeyMap/TreeRefreshGate.swift` (plan Layer 3). The
//  contract asserted below is:
//    * `init(isVisible: Bool)`
//    * `func requestRefresh() -> Bool` — true when visible; when hidden returns
//      false AND records deferred work.
//    * `func setVisible(_ visible: Bool) -> Bool` — true iff this is a
//      hidden->visible transition WITH deferred work pending (clearing the
//      deferred flag); false otherwise.
//    * `func shouldReload(version: UInt64) -> Bool` — true iff `version` differs
//      from the last reloaded version (recording it); false when unchanged.
//    * `func resetReloadTracking()` — clears the last-reloaded version so the
//      next `shouldReload` reloads again.
//

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

        // Hidden: requests defer, and reload tracking must NOT advance — a
        // shouldReload check here must still see the version as fresh once
        // visible (i.e. hidden work never consumed it).
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
