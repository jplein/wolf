//
//  PreviewSeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.PreviewSeam`.
//
//  Like `FontSizeSeamTests` / `CommandSeamTests`, they import ONLY
//  Foundation-level test machinery (no AppKit, no WKWebView, no live Neovim) and
//  stay in the headless `WolfKeyMap` module. The Preview command's mechanism
//  itself (an IN-PLACE `nvim_win_set_buf` on the active pane) is core + AppKit
//  glue, covered by the wolf-ffi real-nvim tests; what remains pure here is the
//  per-window reload gate and the click-to-focus channel. The per-pane
//  preview/edit MODE decision lives in `PaneModeSeamTests`.
//
//  ## CONTRACT UNDER TEST — preview reload gate (winsNeedingReload)
//
//    public enum PreviewSeam {
//        // A per-preview-WINDOW load key. Two freshly-opened files can share a
//        // `sourceTick`, so the gate must react to the bound SOURCE BUFFER
//        // identity changing as well as the tick advancing.
//        public struct PreviewLoad: Equatable {
//            public let sourceBufnr: Int64
//            public let sourceTick: UInt64
//            public init(sourceBufnr: Int64, sourceTick: UInt64)
//        }
//
//        // Returns the preview WINDOW handles (keys of `current`) whose
//        // `PreviewLoad` differs from the `loaded` entry for that same win.
//        static func winsNeedingReload(
//            current: [Int64: PreviewLoad],
//            loaded: [Int64: PreviewLoad]
//        ) -> [Int64]
//    }
//
//  A win is returned iff `loaded[win] != current[win]`, which fires on:
//    * FIRST BIND      — no `loaded` entry for the win yet.
//    * SOURCE CHANGED  — a different `sourceBufnr` even at an EQUAL `sourceTick`
//                        (the reused-preview-pane regression: a single window
//                        switching which source it previews must reload).
//    * TICK ADVANCED   — the same `sourceBufnr` but a newer `sourceTick`.
//  It does NOT fire when all of (win, sourceBufnr, sourceTick) match. Only
//  `current` is iterated, so a win present only in `loaded` is never returned.
//  The result ORDER is unspecified (dictionary iteration): assert membership via
//  `Set<Int64>`, never array equality.
//

import XCTest

@testable import WolfKeyMap

final class PreviewSeamTests: XCTestCase {

  // MARK: - winsNeedingReload: first bind always reloads

  func testFirstBindNeedsReload() {
    let current: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: current, loaded: [:])),
      Set<Int64>([10]),
      "a preview win with no prior loaded entry must reload on first bind")
  }

  // MARK: - winsNeedingReload: an EMPTY loaded map reloads EVERY current win

  func testEmptyLoadedReloadsEveryCurrentWin() {
    // The exact mechanism the appearance re-pull relies on: clearing the per-window
    // `loaded` map (an empty `[:]`) must make `winsNeedingReload` return EVERY
    // currently-bound preview window, so a dark/light flip forces a full re-render
    // of the freshly re-sourced nvim colorscheme colors.
    let current: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
      11: PreviewSeam.PreviewLoad(sourceBufnr: 2, sourceTick: 6),
      12: PreviewSeam.PreviewLoad(sourceBufnr: 3, sourceTick: 7),
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: current, loaded: [:])),
      Set<Int64>([10, 11, 12]),
      "with an empty loaded map every current preview window must reload (the appearance re-pull)")
  }

  // MARK: - winsNeedingReload: source buffer changed at an EQUAL tick (THE regression)

  func testSourceBufnrChangedSameTickNeedsReload() {
    let loaded: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
    ]
    let current: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 2, sourceTick: 5)
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
      Set<Int64>([10]),
      "a reused preview pane switching sources at an equal changedtick must still reload")
  }

  // MARK: - winsNeedingReload: tick advanced for the same source buffer

  func testTickAdvancedSameBufnrNeedsReload() {
    let loaded: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
    ]
    let current: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 6)
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
      Set<Int64>([10]),
      "an edit to the same source (advanced tick) must reload the preview")
  }

  // MARK: - winsNeedingReload: identical (win, bufnr, tick) is a no-op

  func testIdenticalIsNoReload() {
    let loaded: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
    ]
    let current: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
      Set<Int64>(),
      "an unchanged (win, sourceBufnr, sourceTick) must NOT reload (avoid redundant render)")
  }

  // MARK: - winsNeedingReload: wins are judged independently

  func testMultipleWinsIndependent() {
    let loaded: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
      11: PreviewSeam.PreviewLoad(sourceBufnr: 2, sourceTick: 6),
    ]
    let current: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
      11: PreviewSeam.PreviewLoad(sourceBufnr: 2, sourceTick: 7),
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
      Set<Int64>([11]),
      "only the win whose load changed reloads; an unchanged win is left alone")
  }

  // MARK: - winsNeedingReload: two wins previewing the SAME source both reload

  func testTwoWinsSameSourceBothReload() {
    let loaded: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
      11: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
    ]
    let current: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 6),
      11: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 6),
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
      Set<Int64>([10, 11]),
      "two panes previewing the same source must EACH reload on a source edit (keyed by win)")
  }

  // MARK: - winsNeedingReload: a win present only in `loaded` is never returned

  func testDroppedWinNotReturned() {
    let loaded: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5),
      99: PreviewSeam.PreviewLoad(sourceBufnr: 2, sourceTick: 3),
    ]
    let current: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: current, loaded: loaded)),
      Set<Int64>(),
      "only `current` is iterated; a win dropped from `current` is never returned")
  }

  // MARK: - winsNeedingReload: an empty `current` reloads nothing

  func testEmptyCurrentIsNoReload() {
    let loaded: [Int64: PreviewSeam.PreviewLoad] = [
      10: PreviewSeam.PreviewLoad(sourceBufnr: 1, sourceTick: 5)
    ]
    XCTAssertEqual(
      Set(PreviewSeam.winsNeedingReload(current: [:], loaded: loaded)),
      Set<Int64>(),
      "no current previews means nothing to reload regardless of prior loads")
  }

  // MARK: - clickFocus: the message-handler name is the pane-focus channel

  func testClickFocusHandlerName() {
    XCTAssertEqual(
      PreviewSeam.clickFocusHandlerName, "wolfPaneFocus",
      "the click-to-focus message handler must be named `wolfPaneFocus`")
  }

  // MARK: - clickFocus: the injected JS is a capture-phase mousedown that posts to the handler

  func testClickFocusJSCaptureMousedownPostsToHandler() {
    let js = PreviewSeam.clickFocusJS()
    XCTAssertTrue(
      js.contains("mousedown"),
      "the click-to-focus JS must listen for `mousedown`: \(js)")
    XCTAssertTrue(
      js.contains("true"),
      "the listener must register in the CAPTURE phase (the `true` argument): \(js)")
    XCTAssertTrue(
      js.contains("window.webkit.messageHandlers.wolfPaneFocus"),
      "the JS must post to the `wolfPaneFocus` script message handler: \(js)")
  }
}
