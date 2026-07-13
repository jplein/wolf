//
//  PickerRefreshCoalescerTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.PickerRefreshCoalescer` seam.
//
//  This is the debounce/coalescing seam behind the file picker's streaming
//  refreshes (plan Fix #1): the traversal wakes fire many times during a large
//  load, and each bare `refresh()` re-ranks the whole accumulated list. The
//  coalescer collapses a BURST of `request()` calls into a single trailing
//  `perform()`, while still guaranteeing a later wake reschedules (the final,
//  complete snapshot is never dropped).
//
//  These tests inject the seam's two collaborators so NO real timer / run loop is
//  needed — exactly the headless style of the other WolfKeyMap seam tests
//  (PickerSeamTests etc.):
//    * `schedule`: a closure that would normally arm a `DispatchQueue.main`
//      timer. Here it CAPTURES the pending work closure instead of firing it, so
//      the test drives the "timer fired" edge deterministically by invoking the
//      captured closure by hand.
//    * `perform`: the work to run once per coalesced burst. Here it just bumps a
//      counter so the tests can assert the exact number of fires.
//
//  NOTE to coder: this references the pure `WolfKeyMap.PickerRefreshCoalescer`
//  the coder adds in `Sources/WolfKeyMap/PickerRefreshCoalescer.swift` (plan
//  Fix #1). The contract asserted below is:
//    * `init(schedule: (@escaping () -> Void) -> Void, perform: () -> Void)`
//    * `func request()` — coalesce: if a fire is already scheduled, do nothing;
//      otherwise mark scheduled and call `schedule { <clear-flag>; perform() }`.
//    * `static let refreshInterval: TimeInterval` — the debounce window (~30fps).
//

import XCTest

@testable import WolfKeyMap

final class PickerRefreshCoalescerTests: XCTestCase {

  /// Number of rapid `request()` calls in the burst (a named constant rather
  /// than a bare literal, mirroring the seam tests' `kNRows` convention).
  private let kBurstCount = 5

  /// A burst of N rapid `request()` calls arms EXACTLY ONE trailing fire, and
  /// invoking that captured fire runs `perform` EXACTLY ONCE (coalescing +
  /// trailing fire).
  func testBurstCoalescesToSingleTrailingFire() {
    var pending: (() -> Void)?
    var scheduleCount = 0
    var performCount = 0

    let coalescer = PickerRefreshCoalescer(
      schedule: { work in
        scheduleCount += 1
        pending = work
      },
      perform: { performCount += 1 })

    for _ in 0..<kBurstCount {
      coalescer.request()
    }

    // The whole burst armed a SINGLE fire; nothing has performed yet.
    XCTAssertEqual(
      scheduleCount, 1,
      "a burst of \(kBurstCount) requests must schedule exactly one trailing fire")
    XCTAssertEqual(
      performCount, 0,
      "perform must not run until the scheduled fire is invoked")

    // The trailing fire runs perform exactly once for the whole burst.
    pending?()
    XCTAssertEqual(
      performCount, 1,
      "the coalesced burst must perform exactly once when the fire runs")
  }

  /// After a fire CLEARS the scheduled flag, a further `request()` schedules and
  /// fires AGAIN — the final/complete wake is never permanently dropped.
  func testRequestAfterFireReschedulesAndFiresAgain() {
    var pending: (() -> Void)?
    var scheduleCount = 0
    var performCount = 0

    let coalescer = PickerRefreshCoalescer(
      schedule: { work in
        scheduleCount += 1
        pending = work
      },
      perform: { performCount += 1 })

    // First burst -> one schedule, then fire clears the flag.
    for _ in 0..<kBurstCount {
      coalescer.request()
    }
    pending?()
    XCTAssertEqual(scheduleCount, 1, "first burst schedules once")
    XCTAssertEqual(performCount, 1, "first fire performs once")

    // A later wake after the flag cleared must reschedule and fire again.
    coalescer.request()
    XCTAssertEqual(
      scheduleCount, 2,
      "a request after the fire cleared the flag must schedule again")
    pending?()
    XCTAssertEqual(
      performCount, 2,
      "the rescheduled request must perform again (final wake not dropped)")
  }

  /// A single `request()` with no burst schedules once and fires exactly once.
  func testSingleRequestFiresOnce() {
    var pending: (() -> Void)?
    var scheduleCount = 0
    var performCount = 0

    let coalescer = PickerRefreshCoalescer(
      schedule: { work in
        scheduleCount += 1
        pending = work
      },
      perform: { performCount += 1 })

    coalescer.request()
    XCTAssertEqual(scheduleCount, 1, "a single request schedules exactly one fire")
    XCTAssertEqual(performCount, 0, "perform waits for the fire")

    pending?()
    XCTAssertEqual(performCount, 1, "a single request performs exactly once")
  }

  /// The debounce window is in the ~30–60fps band (16–33 ms). The plan sets it
  /// to 0.033 s; this pins it can't drift outside a sane range.
  func testRefreshIntervalIsInFrameBand() {
    let lowerBound: TimeInterval = 0.016
    let upperBound: TimeInterval = 0.033
    XCTAssertGreaterThanOrEqual(
      PickerRefreshCoalescer.refreshInterval, lowerBound,
      "refreshInterval must be at least \(lowerBound)s (<= ~60fps)")
    XCTAssertLessThanOrEqual(
      PickerRefreshCoalescer.refreshInterval, upperBound,
      "refreshInterval must be at most \(upperBound)s (>= ~30fps)")
  }
}
