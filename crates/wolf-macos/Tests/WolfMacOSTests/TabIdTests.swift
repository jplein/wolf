// Each tab's WOLF_TAB_ID: minted by TabCore.start, seen by its sessions, kept across a restart.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// Stands in for a tab's window controller, which cannot be built offscreen, and counts the
/// restart notices its `TabCore` flashes.
private final class RestartCountingView: TabCoreView {
  private(set) var flashes = 0

  func onRenderWake() {}
  func drainAndDispatchCommands() {}
  func onPickerWake() {}
  func onTreeWake() {}
  func onSearchWake() {}
  func showNvimOverlay(_ mode: NvimStatusMode) {}
  func hideNvimOverlay() {}
  func flashRestartNotice() { flashes += 1 }
  func capturedTerminals() -> (splits: [UInt64], drawerOpen: Bool) { ([], false) }
  func resetAfterRestart() {}
  func requestTerminalRestore(_ steps: [LivenessSeam.TerminalRestoreStep]) {}
}

/// The tab IDs of `specs/cli-and-file-open.md` §3 over real cores from `TabCore.start`, as
/// `TabCoreTests` starts them: each tab's `wolfTabId` is a fresh version-4 UUID, a terminal
/// session the tab starts prints it from `WOLF_TAB_ID`, and a crash restart of the tab's
/// Neovim keeps it.
///
/// A session is a `/bin/sh -c` that prints `WOLFTAB<…>` around the variable and then sleeps,
/// so its frame holds the value until the core shuts down, and an unset variable prints
/// `WOLFTAB<>`. No wake callback is installed, so the restart test drives `onWake()` itself.
@MainActor
final class TabIdTests: XCTestCase {
  /// How long a core gets to attach, print or restart.
  private static let timeout: TimeInterval = 30

  /// The session command: the tab ID between markers, then a sleep that outlives the test.
  private static let printTabId = "printf 'WOLFTAB<%s>\\n' \"$WOLF_TAB_ID\"; exec sleep 600"

  /// `FfiKeyEvent.kind` of a printable character.
  private static let kindChar: UInt8 = 0
  /// `FfiKeyEvent.kind` of the Return key.
  private static let kindEnter: UInt8 = 1

  // MARK: - T1: two tabs, two IDs, each seen by its own sessions

  func testEachTabHasItsOwnIdWhichItsSessionsSee() {
    guard let dir = LiveCore.scratchDirectory() else { return }
    var cores: [TabCore] = []
    defer { shutDown(cores) }
    guard let a = start(at: dir, into: &cores), let b = start(at: dir, into: &cores) else {
      return
    }

    for (name, core) in [("a", a), ("b", b)] {
      XCTAssertTrue(
        isVersion4(core.wolfTabId),
        "tab \(name)'s wolfTabId must be a lowercase version-4 UUID; it is"
          + " \"\(core.wolfTabId)\". Empty means TabCore.start never asked core for one")
    }
    XCTAssertNotEqual(
      a.wolfTabId, b.wolfTabId,
      "two tabs must have distinct IDs; one ID means assign_tab_id reuses a process-wide ID,"
        + " so a file sent from one tab's terminal could land in the other")

    for (name, core, other) in [("a", a, b), ("b", b, a)] {
      guard let printed = printedTabId(core) else { continue }
      XCTAssertEqual(
        printed, core.wolfTabId,
        "a session in tab \(name) must see its own tab's ID in WOLF_TAB_ID."
          + (printed == other.wolfTabId
            ? " It sees the other tab's, so the ID is shared across cores."
            : printed.isEmpty ? " It sees none, so the core was never given the ID." : ""))
    }
  }

  // MARK: - T2: a crash restart keeps the ID

  func testACrashRestartKeepsTheTabsId() {
    guard let dir = LiveCore.scratchDirectory() else { return }
    var cores: [TabCore] = []
    defer { shutDown(cores) }
    guard let core = start(at: dir, into: &cores) else { return }
    let view = RestartCountingView()
    core.view = view
    let before = core.wolfTabId
    guard let first = printedTabId(core) else { return }
    XCTAssertEqual(
      first, before, "premise: a session before the crash must see the tab's ID \(before)")

    typeEx("lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)", into: core)
    let restarted = LiveCore.spin(timeout: Self.timeout) {
      core.onWake()
      return view.flashes > 0 && core.handle.liveness() == NvimLiveness.alive.rawValue
    }
    XCTAssertTrue(
      restarted,
      "premise: the SIGKILLed Neovim must restart automatically: liveness"
        + " \(core.handle.liveness()), flashes \(view.flashes)")
    guard restarted else { return }
    let attached = LiveCore.spin(timeout: Self.timeout) { core.handle.read_frame().cols() > 0 }
    XCTAssertTrue(attached, "premise: the restarted Neovim must attach")
    guard attached else { return }

    XCTAssertEqual(
      core.wolfTabId, before,
      "a crash restart is the same tab, so wolfTabId must stay \(before)")

    // The restore reopens files as queued input, which a blocking open_terminal would overtake;
    // an open queued behind it shows only once the restore has landed.
    let barrier = (dir as NSString).appendingPathComponent("barrier.md")
    do {
      try "# Barrier\n".write(toFile: barrier, atomically: true, encoding: .utf8)
    } catch {
      XCTFail("premise: could not write \(barrier): \(error)")
      return
    }
    core.handle.open(barrier)
    var shown = ""
    let drained = LiveCore.spin(timeout: Self.timeout) {
      shown = core.handle.win_buffer(core.handle.active_win()).abs_path().toString()
      return shown == barrier
    }
    XCTAssertTrue(
      drained,
      "premise: an open queued after the restart must show \(barrier); the active window shows"
        + " \"\(shown)\", so the restart's queued restore never drained")
    guard drained else { return }
    guard let after = printedTabId(core) else { return }
    XCTAssertEqual(
      after, before,
      "a session started after the restart must see the tab's ID \(before); \(after) means the"
        + " restart gave core a fresh ID, so a terminal opened before the crash and one opened"
        + " after name different tabs")
  }

  // MARK: - Helpers

  /// Whether `id` is a lowercase version-4 UUID in the 8-4-4-4-12 form: hex digits, hyphens
  /// at 8, 13, 18 and 23, the version digit `4`, and a variant digit of `8` to `b`.
  private func isVersion4(_ id: String) -> Bool {
    let chars = Array(id)
    guard chars.count == 36 else { return false }
    for (i, char) in chars.enumerated() {
      if [8, 13, 18, 23].contains(i) {
        guard char == "-" else { return false }
      } else if !("0123456789abcdef".contains(char)) {
        return false
      }
    }
    return chars[14] == "4" && "89ab".contains(chars[19])
  }

  /// Starts a core at `dir` through `TabCore.start`, appends it to `cores` and waits for it
  /// to attach; `nil`, with the test failed, when it does not start or attach.
  private func start(
    at dir: String, into cores: inout [TabCore], file: StaticString = #filePath,
    line: UInt = #line
  ) -> TabCore? {
    guard let core = TabCore.start(launchCwd: dir, coordinator: nil) else {
      XCTFail("TabCore.start returned nil at \(dir)", file: file, line: line)
      return nil
    }
    cores.append(core)
    let attached = LiveCore.spin(timeout: Self.timeout) { core.handle.read_frame().cols() > 0 }
    XCTAssertTrue(attached, "premise: the core at \(dir) must attach", file: file, line: line)
    return attached ? core : nil
  }

  /// Shuts down every core in `cores` that is still registered.
  private func shutDown(_ cores: [TabCore]) {
    for core in cores where WindowRegistry.shared.tabCore(for: core.wakeToken) === core {
      core.shutdown()
    }
  }

  /// Starts a session in `core`'s current window that prints its `WOLF_TAB_ID`, and returns
  /// what it printed; `nil`, with the test failed, when no session starts or prints.
  private func printedTabId(
    _ core: TabCore, file: StaticString = #filePath, line: UInt = #line
  ) -> String? {
    let session = core.handle.open_terminal(0, 0, Self.printTabId)
    guard session != 0 else {
      XCTFail("premise: open_terminal must start a session", file: file, line: line)
      return nil
    }
    var text = ""
    var printed: String?
    let landed = LiveCore.spin(timeout: Self.timeout) {
      text = frameText(core, session)
      printed = between("WOLFTAB<", ">", in: text)
      return printed != nil
    }
    guard landed, let printed else {
      XCTFail(
        "premise: session \(session) must print WOLFTAB<…>; its frame reads \(text)",
        file: file, line: line)
      return nil
    }
    return printed
  }

  /// Every row of `session`'s frame, concatenated, so a wrapped line reads whole.
  private func frameText(_ core: TabCore, _ session: UInt64) -> String {
    let frame = core.handle.terminal_frame(session)
    return (0..<frame.rows()).map { row in
      frame.row_cells(row).map { cell -> String in
        let text = cell.text().toString()
        return text.isEmpty ? " " : text
      }.joined()
    }.joined()
  }

  /// The text between the first `open` and the next `close` after it in `text`.
  private func between(_ open: String, _ close: String, in text: String) -> String? {
    guard let start = text.range(of: open),
      let end = text.range(of: close, range: start.upperBound..<text.endIndex)
    else { return nil }
    return String(text[start.upperBound..<end.lowerBound])
  }

  /// Types `:command` and Return into `core`'s Neovim.
  private func typeEx(_ command: String, into core: TabCore) {
    for scalar in (":" + command).unicodeScalars {
      core.handle.send_key(
        FfiKeyEvent(
          kind: Self.kindChar, ch: scalar.value, fnum: 0, ctrl: false, shift: false, alt: false,
          meta: false))
    }
    core.handle.send_key(
      FfiKeyEvent(
        kind: Self.kindEnter, ch: 0, fnum: 0, ctrl: false, shift: false, alt: false, meta: false))
  }
}
