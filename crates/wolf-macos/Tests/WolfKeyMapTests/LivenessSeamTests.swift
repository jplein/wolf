// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import XCTest

@testable import WolfKeyMap

final class LivenessSeamTests: XCTestCase {

  /// Every liveness the FFI reports, listed by hand: `NvimLiveness` need not be
  /// `CaseIterable`, and a new case should be added here deliberately.
  private let allLiveness: [NvimLiveness] = [.alive, .notResponding, .dead]

  /// Attempt counts spanning "budget left" (0), "budget just spent" (1) and
  /// "past the budget" (2).
  private let attemptCounts = [0, 1, 2]

  /// The whole decision table: each liveness at 0 attempts (one automatic attempt
  /// still permitted) and at 1 (the one attempt already made).
  func testDecisionTable() {
    let rows: [(liveness: NvimLiveness, attempts: Int, expected: LivenessSeam.Decision)] = [
      (
        .alive, 0,
        LivenessSeam.Decision(
          overlay: .hidden, autoRestart: false, render: true, drain: true, persist: true)
      ),
      (
        .alive, 1,
        LivenessSeam.Decision(
          overlay: .hidden, autoRestart: false, render: true, drain: true, persist: true)
      ),
      (
        .notResponding, 0,
        LivenessSeam.Decision(
          overlay: .stalled, autoRestart: false, render: false, drain: true, persist: false)
      ),
      (
        .notResponding, 1,
        LivenessSeam.Decision(
          overlay: .stalled, autoRestart: false, render: false, drain: true, persist: false)
      ),
      (
        .dead, 0,
        LivenessSeam.Decision(
          overlay: .crashed(retrying: true), autoRestart: true, render: false, drain: true,
          persist: false)
      ),
      (
        .dead, 1,
        LivenessSeam.Decision(
          overlay: .crashed(retrying: false), autoRestart: false, render: false, drain: true,
          persist: false)
      ),
    ]
    XCTAssertEqual(
      rows.count, 3 * 2,
      "the table must cover every liveness at attempts 0 and 1; a missing row leaves a cell unpinned"
    )
    for row in rows {
      XCTAssertEqual(
        LivenessSeam.decision(liveness: row.liveness, autoRestartAttempts: row.attempts),
        row.expected,
        "decision(\(row.liveness), attempts: \(row.attempts)) differs from the plan's table: "
          + "a wrong overlay misreports the core, a wrong autoRestart loops or never recovers, "
          + "a wrong render paints a dead core, a false drain strands a queued close or quit, "
          + "a wrong persist loses files opened since the last commit or saves a dead core")
    }
  }

  /// The cap: with one automatic attempt already made, a second is refused and
  /// the overlay stops promising a restart.
  func testSecondAutomaticAttemptIsRefused() {
    XCTAssertEqual(
      LivenessSeam.maxAutoRestartAttempts, 1,
      "the plan caps automatic restarts at exactly one per crash, as wolf-gnome's MAX_AUTO_RESTART_ATTEMPTS"
    )
    XCTAssertTrue(
      LivenessSeam.decision(liveness: .dead, autoRestartAttempts: 0).autoRestart,
      "a dead core with no attempt made yet must auto-restart; false means crash recovery never "
        + "starts, and the refusals below would hold vacuously")
    for attempts in [1, 2] {
      let decision = LivenessSeam.decision(liveness: .dead, autoRestartAttempts: attempts)
      XCTAssertFalse(
        decision.autoRestart,
        "with \(attempts) automatic attempt(s) already made, another must be refused: true means "
          + "a respawn that keeps failing re-marks the core Dead and retries forever")
      XCTAssertEqual(
        decision.overlay, .crashed(retrying: false),
        "with \(attempts) attempt(s) made the overlay must say the automatic restart failed; "
          + "retrying: true promises a restart that will never come")
    }
  }

  /// `drain` is always true; see crates/wolf-macos/Sources/WolfKeyMap/README.md#livenessseamswift
  func testDrainRunsInEveryLivenessState() {
    for liveness in allLiveness {
      for attempts in attemptCounts {
        XCTAssertTrue(
          LivenessSeam.decision(liveness: liveness, autoRestartAttempts: attempts).drain,
          "drain must be true for \(liveness) at \(attempts) attempt(s): false strands a close "
            + "or quit queued just before the crash, and the window stays behind the overlay")
      }
    }
  }

  /// Only a dead core with budget left restarts by itself; a stalled core is
  /// never force-killed, since a restart loses what is not in a swap file.
  func testOnlyADeadCoreWithAttemptsLeftAutoRestarts() {
    for liveness in allLiveness {
      for attempts in attemptCounts {
        let expected = liveness == .dead && attempts == 0
        let reason: String
        if liveness == .notResponding {
          reason = "a stalled core must never auto-restart: the restart force-kills Neovim"
        } else if liveness == .alive {
          reason = "an alive core has nothing to recover"
        } else if expected {
          reason = "a dead core with its one attempt unspent must auto-restart"
        } else {
          reason = "a dead core whose attempt is spent must wait for the button"
        }
        XCTAssertEqual(
          LivenessSeam.decision(liveness: liveness, autoRestartAttempts: attempts).autoRestart,
          expected,
          "autoRestart for \(liveness) at \(attempts) attempt(s): \(reason)")
      }
    }
  }

  /// The render wake runs only for an alive core: the overlay hides a frozen
  /// frame, and a spent budget must never stop a live editor painting.
  func testRenderOnlyWhenAlive() {
    for liveness in allLiveness {
      for attempts in attemptCounts {
        XCTAssertEqual(
          LivenessSeam.decision(liveness: liveness, autoRestartAttempts: attempts).render,
          liveness == .alive,
          liveness == .alive
            ? "an alive core must render at \(attempts) attempt(s): the count must not freeze it"
            : "\(liveness) must not render: the frame sits behind the overlay")
      }
    }
  }

  /// Only an alive wake commits the session, so a file opened or a pane split is saved before
  /// a forced quit; a stalled or dead core's maps are stale and must not overwrite it.
  func testOnlyAnAliveWakePersists() {
    for attempts in [0, LivenessSeam.maxAutoRestartAttempts] {
      XCTAssertTrue(
        LivenessSeam.decision(liveness: .alive, autoRestartAttempts: attempts).persist,
        "an alive wake at \(attempts) attempt(s) must persist: false leaves every file opened "
          + "since the last unrelated commit to applicationWillTerminate, which a kill skips")
      XCTAssertFalse(
        LivenessSeam.decision(liveness: .notResponding, autoRestartAttempts: attempts).persist,
        "a stalled wake at \(attempts) attempt(s) must not persist: its maps are frozen")
      XCTAssertFalse(
        LivenessSeam.decision(liveness: .dead, autoRestartAttempts: attempts).persist,
        "a dead wake at \(attempts) attempt(s) must not persist: a dead core reports no "
          + "windows, and the commit would write every tab without its files")
    }
  }

  /// The raw values are the discriminants of wolf-ffi's `mod liveness`
  /// (`crates/wolf-ffi/src/lib.rs`), which `WolfHandle.liveness()` returns.
  func testNvimLivenessRawValues() {
    XCTAssertEqual(
      NvimLiveness(rawValue: 0), .alive, "FFI ALIVE is 0; a mismatch reads a live core as broken")
    XCTAssertEqual(
      NvimLiveness(rawValue: 1), .notResponding,
      "FFI NOT_RESPONDING is 1; a mismatch shows the wrong overlay for a stall")
    XCTAssertEqual(
      NvimLiveness(rawValue: 2), .dead,
      "FFI DEAD is 2; a mismatch leaves a crash unrecovered or restarts a live core")
    XCTAssertNil(
      NvimLiveness(rawValue: 3),
      "3 is no FFI liveness; decoding it to a case would hide an FFI change")
  }
}
