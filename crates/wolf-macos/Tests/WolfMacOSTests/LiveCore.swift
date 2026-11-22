// Starts a real core over the Bazel-staged Neovim for view tests, with its state and config fenced.

import Foundation
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// The process-wide environment a live core needs, prepared once before any core or
/// config read: `WOLF_NVIM_BIN` from the staged Neovim, `WOLF_INIT_LUA` from the staged
/// `init.lua`, so `TabCore.start` launches it through its production path, fresh
/// `WOLF_STATE_DIR` and `WOLF_CONFIG_DIR`, so no test reads or writes the user's state or
/// preferences, and `SHELL=/bin/sh`, so a session started with no command runs a known shell.
///
/// It also registers the staged font trees the app bundles (iA Writer Mono S, JetBrains Mono
/// and Symbols Nerd Font Mono) with CoreText, before any metrics are measured, so every test
/// measures the families the app ships rather than the Menlo fallback.
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
    setenv("WOLF_INIT_LUA", initLua, 1)
    if let problem = registerStagedFonts(env: env) {
      return LiveCoreEnvironment(initLua: nil, problem: problem, scratch: scratch)
    }
    return LiveCoreEnvironment(initLua: initLua, problem: nil, scratch: scratch)
  }

  /// The font trees the app bundles: each variable and the target that stages its tree.
  private static let fontTrees = [
    ("WOLF_TEST_IA_WRITER_MONO_DIR", "//bazel/ia-writer-mono:ia_writer_mono_tree"),
    ("WOLF_TEST_JETBRAINS_MONO_DIR", "//bazel/jetbrains-mono:jetbrains_mono_tree"),
    ("WOLF_TEST_NERD_FONTS_SYMBOLS_DIR", "//bazel/nerd-fonts-symbols:nerd_fonts_symbols_tree"),
  ]

  /// Registers every staged font tree's inner `fonts/` directory with CoreText, as the app
  /// registers its bundle's; returns what went wrong, naming the variable, or `nil`.
  private static func registerStagedFonts(env: [String: String]) -> String? {
    for (key, target) in fontTrees {
      guard let tree = resolveRunfile(key, env: env) else {
        return "\(key) (\(env[key] ?? "unset")) resolves to no staged font tree: the"
          + " WolfMacOSTests target must stage \(target)"
      }
      let fonts = URL(fileURLWithPath: tree).appendingPathComponent("fonts")
      var isDir: ObjCBool = false
      guard FileManager.default.fileExists(atPath: fonts.path, isDirectory: &isDir),
        isDir.boolValue
      else {
        return "\(key) resolved to \(tree), which holds no fonts/ directory"
      }
      FontRegistration.registerFonts(in: fonts)
    }
    return nil
  }

  /// Resolves the runfiles-relative path in `key` against the bases Bazel may use, as
  /// `resolve_runfile_value` in `crates/wolf-test-support/src/resolve.rs` does.
  fileprivate static func resolveRunfile(_ key: String, env: [String: String]) -> String? {
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
  ///
  /// `initLua`, when given, replaces the staged `init.lua`, such as a copy staged beside a
  /// vendored tree it self-locates.
  static func start(
    initLua override: String? = nil, file: StaticString = #filePath, line: UInt = #line
  ) -> LiveCore? {
    let env = LiveCoreEnvironment.shared
    guard let staged = env.initLua else {
      XCTFail(
        "LiveCore: \(env.problem ?? "the environment was not prepared")", file: file, line: line)
      return nil
    }
    let initLua = override ?? staged
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

  /// The staged `init.lua`, or `nil` after failing the calling test when it does not resolve.
  static func stagedInitLua(file: StaticString = #filePath, line: UInt = #line) -> String? {
    let env = LiveCoreEnvironment.shared
    guard let initLua = env.initLua else {
      XCTFail(
        "LiveCore: \(env.problem ?? "the environment was not prepared")", file: file, line: line)
      return nil
    }
    return initLua
  }

  /// Prepares the shared environment, which registers the staged fonts, for a test that
  /// starts no core; `false`, with the calling test failed, when it could not be prepared.
  @discardableResult
  static func stageFonts(file: StaticString = #filePath, line: UInt = #line) -> Bool {
    let env = LiveCoreEnvironment.shared
    guard env.problem == nil else {
      XCTFail("LiveCore: \(env.problem ?? "")", file: file, line: line)
      return false
    }
    return true
  }

  /// Resolves the runfiles-relative path in the variable `key` to an existing path, after the
  /// shared environment is prepared; `nil` when the variable is unset or names nothing.
  static func runfile(_ key: String) -> String? {
    _ = LiveCoreEnvironment.shared
    return LiveCoreEnvironment.resolveRunfile(key, env: ProcessInfo.processInfo.environment)
  }

  /// Opens a session running `command` in Neovim's current window, returning its id or 0.
  func openTerminal(_ command: String) -> UInt64 {
    handle.open_terminal(0, 0, command)
  }

  /// Spins the main run loop in short steps until `condition` holds or `timeout` passes,
  /// and returns whether it held.
  @discardableResult
  func waitFor(timeout: TimeInterval, _ condition: () -> Bool) -> Bool {
    Self.spin(timeout: timeout, until: condition)
  }

  /// `waitFor(timeout:_:)` for a test that holds no `LiveCore`, such as one whose cores
  /// `TabCore.start` makes.
  @discardableResult
  static func spin(timeout: TimeInterval, until condition: () -> Bool) -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
      if condition() { return true }
      RunLoop.main.run(until: Date().addingTimeInterval(pollStep))
    }
    return condition()
  }

  /// Prepares the shared environment and makes a fresh directory in its scratch area,
  /// returned canonical, as Neovim reports its cwd; `nil`, with the calling test failed,
  /// when the environment or the directory cannot be made.
  static func scratchDirectory(file: StaticString = #filePath, line: UInt = #line) -> String? {
    let env = LiveCoreEnvironment.shared
    guard env.problem == nil else {
      XCTFail("LiveCore: \(env.problem ?? "")", file: file, line: line)
      return nil
    }
    let dir = (env.scratch as NSString).appendingPathComponent("dir-\(UUID().uuidString)")
    do {
      try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    } catch {
      XCTFail("LiveCore: could not create \(dir): \(error)", file: file, line: line)
      return nil
    }
    guard let resolved = realpath(dir, nil) else {
      XCTFail("LiveCore: could not resolve \(dir)", file: file, line: line)
      return nil
    }
    defer { free(resolved) }
    return String(cString: resolved)
  }

  /// Shuts the core and its sessions down.
  func shutdown() {
    handle.shutdown()
  }
}
