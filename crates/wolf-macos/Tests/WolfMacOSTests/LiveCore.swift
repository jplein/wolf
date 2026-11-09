// Starts a real core over the Bazel-staged Neovim for view tests, with its state and config fenced.

import Foundation
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// The process-wide environment a live core needs, prepared once before any core or
/// config read: `WOLF_NVIM_BIN` from the staged Neovim, fresh `WOLF_STATE_DIR` and
/// `WOLF_CONFIG_DIR`, so no test reads or writes the user's state or preferences, and
/// `SHELL=/bin/sh`, so a session started with no command runs a known shell.
private struct LiveCoreEnvironment: Sendable {
  /// The staged `init.lua`, or `nil` with `problem` set.
  let initLua: String?
  /// Why the environment could not be prepared, naming the variable to fix.
  let problem: String?
  /// The directory fresh working directories are made in.
  let scratch: String

  /// Prepared on first use, once per process: `setenv` races a concurrent `getenv`.
  static let shared = prepare()

  private static func prepare() -> LiveCoreEnvironment {
    let env = ProcessInfo.processInfo.environment
    let base = env["TEST_TMPDIR"].flatMap { $0.isEmpty ? nil : $0 } ?? NSTemporaryDirectory()
    let scratch = (base as NSString).appendingPathComponent("wolf-macos-live-\(UUID().uuidString)")
    let state = (scratch as NSString).appendingPathComponent("state")
    let config = (scratch as NSString).appendingPathComponent("config")
    for dir in [state, config] {
      do {
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
      } catch {
        return LiveCoreEnvironment(
          initLua: nil, problem: "could not create \(dir): \(error)", scratch: scratch)
      }
    }
    setenv("WOLF_STATE_DIR", state, 1)
    setenv("WOLF_CONFIG_DIR", config, 1)
    // The drawer and a typed `:terminal` run the default shell; tests type POSIX `sh` into it.
    setenv("SHELL", "/bin/sh", 1)

    guard let nvimDir = resolveRunfile("WOLF_TEST_NVIM_DIR", env: env) else {
      return LiveCoreEnvironment(
        initLua: nil,
        problem: "WOLF_TEST_NVIM_DIR (\(env["WOLF_TEST_NVIM_DIR"] ?? "unset")) resolves to no"
          + " staged Neovim tree: the WolfMacOSTests target must stage //bazel/nvim:nvim_tree",
        scratch: scratch)
    }
    let candidates = ["nvim/bin/nvim", "bin/nvim"].map {
      (nvimDir as NSString).appendingPathComponent($0)
    }
    guard let nvim = candidates.first(where: { FileManager.default.fileExists(atPath: $0) })
    else {
      return LiveCoreEnvironment(
        initLua: nil,
        problem: "WOLF_TEST_NVIM_DIR resolved to \(nvimDir), which holds neither nvim/bin/nvim"
          + " nor bin/nvim", scratch: scratch)
    }
    setenv("WOLF_NVIM_BIN", nvim, 1)

    guard let initLua = resolveRunfile("WOLF_TEST_INIT_LUA", env: env) else {
      return LiveCoreEnvironment(
        initLua: nil,
        problem: "WOLF_TEST_INIT_LUA (\(env["WOLF_TEST_INIT_LUA"] ?? "unset")) resolves to no"
          + " staged init.lua: the target must stage //runtime/nvim:init.lua", scratch: scratch)
    }
    return LiveCoreEnvironment(initLua: initLua, problem: nil, scratch: scratch)
  }

  /// Resolves the runfiles-relative path in `key` against the bases Bazel may use, as
  /// `resolve_runfile` in `crates/wolf-ffi/tests/common/mod.rs` does.
  private static func resolveRunfile(_ key: String, env: [String: String]) -> String? {
    guard let rel = env[key], !rel.isEmpty else { return nil }
    let fm = FileManager.default
    if rel.hasPrefix("/") { return fm.fileExists(atPath: rel) ? rel : nil }
    var bases: [String] = []
    if let srcdir = env["TEST_SRCDIR"] {
      if let workspace = env["TEST_WORKSPACE"] {
        bases.append((srcdir as NSString).appendingPathComponent(workspace))
      }
      bases.append((srcdir as NSString).appendingPathComponent("_main"))
      bases.append(srcdir)
    }
    if let runfiles = env["RUNFILES_DIR"] {
      bases.append((runfiles as NSString).appendingPathComponent("_main"))
      bases.append(runfiles)
    }
    bases.append(fm.currentDirectoryPath)
    return bases.map { ($0 as NSString).appendingPathComponent(rel) }
      .first { fm.fileExists(atPath: $0) }
  }
}

/// A started core with its measured cell metrics. Call `shutdown()` when the test ends.
@MainActor
final class LiveCore {
  /// The default run-loop step `waitFor(timeout:_:)` spins in.
  private static let pollStep: TimeInterval = 0.02
  /// How long the core gets to attach its first grid.
  private static let attachTimeout: TimeInterval = 15

  let handle: WolfCoreHandle
  let metrics: CellMetrics

  private init(handle: WolfCoreHandle, metrics: CellMetrics) {
    self.handle = handle
    self.metrics = metrics
  }

  /// Starts a core in a fresh working directory at 80 x 24 and waits for it to attach.
  /// Fails the calling test and returns `nil` when the staged Neovim or `init.lua` does not
  /// resolve, the core does not start, or it does not attach.
  static func start(file: StaticString = #filePath, line: UInt = #line) -> LiveCore? {
    let env = LiveCoreEnvironment.shared
    guard let initLua = env.initLua else {
      XCTFail(
        "LiveCore: \(env.problem ?? "the environment was not prepared")", file: file, line: line)
      return nil
    }
    let cwd = (env.scratch as NSString).appendingPathComponent("cwd-\(UUID().uuidString)")
    do {
      try FileManager.default.createDirectory(atPath: cwd, withIntermediateDirectories: true)
    } catch {
      XCTFail("LiveCore: could not create the cwd \(cwd): \(error)", file: file, line: line)
      return nil
    }
    guard let handle = wolf_ffi_start(initLua, cwd, 80, 24) else {
      XCTFail(
        "LiveCore: wolf_ffi_start returned nil for the staged Neovim at"
          + " \(ProcessInfo.processInfo.environment["WOLF_NVIM_BIN"] ?? "?")",
        file: file, line: line)
      return nil
    }
    let core = LiveCore(
      handle: handle, metrics: CellMetrics.measured(config: wolf_ffi_load_config()))
    guard core.waitFor(timeout: attachTimeout, { handle.read_frame().cols() > 0 }) else {
      XCTFail("LiveCore: the core did not attach within \(attachTimeout) s", file: file, line: line)
      handle.shutdown()
      return nil
    }
    return core
  }

  /// Opens a session running `command` in Neovim's current window, returning its id or 0.
  func openTerminal(_ command: String) -> UInt64 {
    handle.open_terminal(0, 0, command)
  }

  /// Spins the main run loop in short steps until `condition` holds or `timeout` passes,
  /// and returns whether it held.
  @discardableResult
  func waitFor(timeout: TimeInterval, _ condition: () -> Bool) -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
      if condition() { return true }
      RunLoop.main.run(until: Date().addingTimeInterval(Self.pollStep))
    }
    return condition()
  }

  /// Shuts the core and its sessions down.
  func shutdown() {
    handle.shutdown()
  }
}
