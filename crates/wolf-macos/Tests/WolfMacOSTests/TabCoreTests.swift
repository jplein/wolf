// Starts tab cores through TabCore.start: each tab's Neovim runs, closes and restarts alone.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// Stands in for a tab's window controller, which cannot be built offscreen, and records what
/// its `TabCore` asks of it.
private final class RecordingTabCoreView: TabCoreView {
  /// Every overlay shown, in order.
  private(set) var overlays: [NvimStatusMode] = []
  /// How many times the view was reset after a restart.
  private(set) var resets = 0
  /// How many times the restart notice was flashed.
  private(set) var flashes = 0

  func onRenderWake() {}
  func drainAndDispatchCommands() {}
  func onPickerWake() {}
  func onTreeWake() {}
  func onSearchWake() {}
  func showNvimOverlay(_ mode: NvimStatusMode) { overlays.append(mode) }
  func hideNvimOverlay() {}
  func flashRestartNotice() { flashes += 1 }
  func capturedTerminals() -> (splits: [UInt64], drawerOpen: Bool) { ([], false) }
  func resetAfterRestart() { resets += 1 }
  func requestTerminalRestore(_ steps: [LivenessSeam.TerminalRestoreStep]) {}

  /// Whether any overlay shown was the crash overlay.
  var showedCrash: Bool {
    overlays.contains { mode in
      if case .dead = mode { return true }
      return false
    }
  }
}

/// Each tab owns its own captive Neovim: two tabs on one directory run two processes, closing
/// one leaves the other running, and a crash restarts only the tab whose Neovim died.
///
/// The cores come from `TabCore.start`, the production entry, over the staged Neovim and
/// `init.lua` that `LiveCore`'s environment points it at. No wake callback is installed, so
/// each test drives `onWake()` itself while it spins the main run loop, which runs the
/// automatic restart's deferred block.
@MainActor
final class TabCoreTests: XCTestCase {
  /// How long a core gets to attach, open a file, answer a probe or restart.
  private static let timeout: TimeInterval = 30

  /// `NvimLiveness.alive`'s raw value, as `liveness()` reports it.
  private let alive = NvimLiveness.alive.rawValue

  /// `FfiKeyEvent.kind` of a printable character.
  private static let kindChar: UInt8 = 0
  /// `FfiKeyEvent.kind` of the Return key.
  private static let kindEnter: UInt8 = 1
  /// `FfiKeyEvent.kind` of the Escape key.
  private static let kindEsc: UInt8 = 2

  /// The unsaved text typed into the surviving tab of the crash test. Absent from the fixture.
  private static let marker = "WOLFSURVIVOR"

  /// K1: two tabs on one directory each start their own core and Neovim process.
  func testTwoTabsOnOneDirectoryRunTwoNeovims() {
    guard let dir = LiveCore.scratchDirectory() else { return }
    var cores: [TabCore] = []
    defer { shutDown(cores) }
    guard let a = start(at: dir, into: &cores), let b = start(at: dir, into: &cores) else {
      return
    }

    XCTAssertFalse(
      a === b,
      "K1: a second tab on the same directory must start its own TabCore; the same object"
        + " means tabs on one directory share one Neovim")
    XCTAssertNotEqual(
      a.wakeToken, b.wakeToken,
      "K1: each tab's core must have its own wake token, or a wake reaches the wrong tab")
    XCTAssertTrue(
      WindowRegistry.shared.tabCore(for: a.wakeToken) === a
        && WindowRegistry.shared.tabCore(for: b.wakeToken) === b,
      "K1: each token must resolve to its own core in WindowRegistry, or its wakes are lost")
    for (name, core) in [("a", a), ("b", b)] {
      let rooted = LiveCore.spin(timeout: Self.timeout) { core.handle.cwd().toString() == dir }
      XCTAssertTrue(
        rooted,
        "K1: tab \(name)'s Neovim must run in its launch directory \(dir); it reports"
          + " \(core.handle.cwd().toString())")
    }

    guard let pidA = probe(a, in: dir)?.pid, let pidB = probe(b, in: dir)?.pid else { return }
    XCTAssertNotEqual(
      pidA, pidB,
      "K1: two tabs on one directory must run two Neovim processes; one pid \(pidA) means"
        + " both cores drive the same Neovim")
  }

  /// K2: closing one tab shuts its core down and leaves the other tab's Neovim running.
  func testClosingOneTabLeavesTheOtherNeovimAlive() {
    guard let dir = LiveCore.scratchDirectory() else { return }
    var cores: [TabCore] = []
    defer { shutDown(cores) }
    guard let a = start(at: dir, into: &cores), let b = start(at: dir, into: &cores) else {
      return
    }
    let file = (dir as NSString).appendingPathComponent("after.md")
    guard write("# After\n", to: file) else { return }

    a.shutdown()
    XCTAssertNil(
      WindowRegistry.shared.tabCore(for: a.wakeToken),
      "K2: a closed tab's core must leave WindowRegistry, or late wakes reach a dead core")
    XCTAssertTrue(
      WindowRegistry.shared.tabCore(for: b.wakeToken) === b,
      "K2: closing tab a must leave tab b registered; nil means the close removed every"
        + " tab's entry, so b's wakes are dropped")
    XCTAssertEqual(
      b.handle.liveness(), alive,
      "K2: tab b's Neovim must still be alive after tab a closes")

    b.handle.open(file)
    let opened = LiveCore.spin(timeout: Self.timeout) {
      b.handle.current_file().toString() == file
    }
    XCTAssertTrue(
      opened,
      "K2: tab b's Neovim must still open files after tab a closes; it reports"
        + " \(b.handle.current_file().toString()), liveness \(b.handle.liveness())")
  }

  /// K3: killing one tab's Neovim restarts that tab alone. The other tab, on the same file
  /// with an unsaved edit, keeps its process and its edit, and its view sees no crash.
  func testKillingOneTabsNeovimRestartsOnlyThatTab() {
    guard let dir = LiveCore.scratchDirectory() else { return }
    var cores: [TabCore] = []
    defer { shutDown(cores) }
    let file = (dir as NSString).appendingPathComponent("shared.md")
    guard write("# Shared\n", to: file) else { return }
    guard let a = start(at: dir, into: &cores), let b = start(at: dir, into: &cores) else {
      return
    }
    let viewA = RecordingTabCoreView()
    let viewB = RecordingTabCoreView()
    a.view = viewA
    b.view = viewB

    for (name, core) in [("a", a), ("b", b)] {
      core.handle.open(file)
      let opened = LiveCore.spin(timeout: Self.timeout) {
        core.handle.current_file().toString() == file
      }
      XCTAssertTrue(
        opened,
        "premise: tab \(name) must open \(file); it reports"
          + " \(core.handle.current_file().toString())")
      guard opened else { return }
    }
    typeText("i" + Self.marker, into: b)
    press(Self.kindEsc, into: b)
    guard let pidA = probe(a, in: dir)?.pid, let before = probe(b, in: dir) else { return }
    XCTAssertTrue(
      before.line1.contains(Self.marker),
      "premise: tab b's buffer must hold the typed unsaved edit; line 1 is \(before.line1)")

    typeEx("lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)", into: a)
    var bDropped = false
    let restarted = LiveCore.spin(timeout: Self.timeout) {
      a.onWake()
      b.onWake()
      if b.handle.liveness() != alive { bDropped = true }
      return viewA.flashes > 0 && a.handle.liveness() == alive
    }
    XCTAssertTrue(
      restarted,
      "K3: tab a must restart its own Neovim after the SIGKILL: liveness"
        + " \(a.handle.liveness()), overlays \(viewA.overlays), resets \(viewA.resets), flashes"
        + " \(viewA.flashes). No crash overlay means onWake never saw it dead; an overlay with"
        + " no reset means no automatic restart was scheduled or it failed")
    XCTAssertTrue(viewA.showedCrash, "K3: tab a's view must show the crash overlay")
    XCTAssertGreaterThan(viewA.resets, 0, "K3: tab a's view must be reset after its restart")
    guard restarted else { return }

    let reopened = LiveCore.spin(timeout: Self.timeout) {
      a.onWake()
      return a.handle.current_file().toString() == file
    }
    XCTAssertTrue(
      reopened,
      "K3: tab a's restarted Neovim must reopen its file \(file); it reports"
        + " \(a.handle.current_file().toString())")
    if let after = probe(a, in: dir) {
      XCTAssertNotEqual(
        after.pid, pidA,
        "K3: tab a must run a new Neovim process after the restart, not the killed \(pidA)")
    }

    XCTAssertFalse(bDropped, "K3: tab b's Neovim must stay alive while tab a restarts")
    XCTAssertTrue(
      viewB.overlays.isEmpty,
      "K3: tab b's view must show no overlay, since its Neovim never died; it showed"
        + " \(viewB.overlays). A crash overlay here means a's overlay reached every tab")
    XCTAssertEqual(viewB.resets, 0, "K3: tab b's view must not be reset by a's restart")
    XCTAssertEqual(viewB.flashes, 0, "K3: tab b must not flash a restart notice")
    guard let survivor = probe(b, in: dir) else { return }
    XCTAssertEqual(
      survivor.pid, before.pid,
      "K3: tab b must keep its Neovim process \(before.pid) through a's crash; \(survivor.pid)"
        + " means b's Neovim was restarted too")
    XCTAssertTrue(
      survivor.line1.contains(Self.marker),
      "K3: tab b's unsaved edit must survive a's crash; line 1 is \(survivor.line1)")
  }

  // MARK: - Helpers

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

  /// Shuts down every core in `cores` that is still registered, so a failed test leaks none
  /// and a core the test closed is not shut down twice.
  private func shutDown(_ cores: [TabCore]) {
    for core in cores where WindowRegistry.shared.tabCore(for: core.wakeToken) === core {
      core.shutdown()
    }
  }

  /// Writes `text` to `path`; `false`, with the test failed, when it cannot.
  private func write(
    _ text: String, to path: String, file: StaticString = #filePath, line: UInt = #line
  ) -> Bool {
    do {
      try text.write(toFile: path, atomically: true, encoding: .utf8)
      return true
    } catch {
      XCTFail("could not write \(path): \(error)", file: file, line: line)
      return false
    }
  }

  /// Sends one key of `kind`, carrying `scalar` for a character, to `core`'s Neovim.
  private func press(_ kind: UInt8, scalar: UInt32 = 0, into core: TabCore) {
    core.handle.send_key(
      FfiKeyEvent(
        kind: kind, ch: scalar, fnum: 0, ctrl: false, shift: false, alt: false, meta: false))
  }

  /// Types `text` into `core`'s Neovim one character at a time.
  private func typeText(_ text: String, into core: TabCore) {
    for scalar in text.unicodeScalars { press(Self.kindChar, scalar: scalar.value, into: core) }
  }

  /// Types `:command` and Return into `core`'s Neovim.
  private func typeEx(_ command: String, into core: TabCore) {
    typeText(":" + command, into: core)
    press(Self.kindEnter, into: core)
  }

  /// Has `core`'s Neovim write its pid and its current buffer's first line to a fresh probe
  /// file in `dir`, and returns them; `nil`, with the test failed, when no probe lands.
  /// The command queues behind earlier keys, so it sees their effect.
  private func probe(
    _ core: TabCore, in dir: String, file: StaticString = #filePath, line: UInt = #line
  ) -> (pid: Int, line1: String)? {
    let path = (dir as NSString).appendingPathComponent("probe-\(UUID().uuidString).txt")
    typeEx(
      "lua vim.fn.writefile({tostring(vim.fn.getpid()), vim.fn.getline(1)}, '\(path)')",
      into: core)
    var lines: [String] = []
    // Two lines and the final newline: a shorter read is writefile still writing.
    let landed = LiveCore.spin(timeout: Self.timeout) {
      let text = (try? String(contentsOfFile: path, encoding: .utf8)) ?? ""
      lines = text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
      return lines.count == 3 && lines[2].isEmpty && Int(lines[0]) != nil
    }
    guard landed, let pid = Int(lines[0]) else {
      XCTFail(
        "the probe at \(path) never landed; read \(lines). The core's Neovim is dead,"
          + " wedged, or not in normal mode", file: file, line: line)
      return nil
    }
    return (pid, lines[1])
  }
}
