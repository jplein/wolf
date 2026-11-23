// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import Foundation
import XCTest

@testable import WolfKeyMap

/// `CommandInstallSeam` installs and uninstalls the `wolf` link of `specs/cli-and-file-open.md`
/// §1, in a fresh directory under `TEST_TMPDIR` that stands in for `/usr/local/bin`. It touches a
/// path only when nothing is there or the path is a link to some `Wolf.app`'s bundled binary.
///
/// The privileged runner is a recorder. Where a test lets the script run, the recorder decodes
/// the AppleScript string literal and runs the command it holds under `/bin/sh` as the test's
/// user, after making a locked directory writable, so the POSIX quoting is checked by a real
/// shell and the AppleScript escaping by the decode. The premise under that decode is
/// AppleScript's: inside a string literal only `\` and `"` need escaping, each with `\`.
final class CommandInstallSeamTests: XCTestCase {

  /// The bundle suffix that marks a link as Wolf's.
  private let suffix = "Contents/Resources/bin/wolf"

  /// What the recorder saw: each script it was handed.
  private final class Runner {
    private(set) var scripts: [String] = []
    /// Run before each script, as the user's password prompt would precede it.
    var before: () -> Void = {}
    /// Whether to run the decoded command; when `false` the runner answers failure.
    var execute = true

    func run(_ script: String) -> Bool {
      scripts.append(script)
      before()
      guard execute, let command = CommandInstallSeamTests.shellCommand(of: script) else {
        return false
      }
      let process = Process()
      process.executableURL = URL(fileURLWithPath: "/bin/sh")
      process.arguments = ["-c", command]
      do {
        try process.run()
      } catch {
        return false
      }
      process.waitUntilExit()
      return process.terminationStatus == 0
    }
  }

  // MARK: - I1: a fresh install

  func testInstallWithNothingThereCreatesTheLink() throws {
    let dir = try scratch()
    let bundled = try makeBundle(in: dir, named: "Wolf.app")
    let link = (dir as NSString).appendingPathComponent("wolf")
    let runner = Runner()

    let outcome = CommandInstallSeam.install(
      linkPath: link, bundled: bundled, runPrivileged: runner.run)
    XCTAssertTrue(
      isCase(outcome, .installed), "install over nothing must report installed: \(outcome)")
    XCTAssertEqual(
      destination(of: link), bundled,
      "install must create \(link) as a symbolic link to \(bundled); nil means nothing was "
        + "created though it reported \(outcome)")
    XCTAssertEqual(
      runner.scripts, [], "a writable directory needs no administrator rights")
  }

  // MARK: - I2: over Wolf's own link

  func testInstallOverWolfsLinkRepointsItOrReportsItCurrent() throws {
    let dir = try scratch()
    let bundled = try makeBundle(in: dir, named: "Wolf.app")
    let older = try makeBundle(in: dir, named: "Old Wolf.app")
    let link = (dir as NSString).appendingPathComponent("wolf")
    try FileManager.default.createSymbolicLink(atPath: link, withDestinationPath: older)
    let runner = Runner()

    let moved = CommandInstallSeam.install(
      linkPath: link, bundled: bundled, runPrivileged: runner.run)
    XCTAssertTrue(
      isCase(moved, .installed),
      "a link to another Wolf.app's binary is Wolf's, so install must re-point it and report "
        + "installed: \(moved). refused means any existing link counts as foreign")
    XCTAssertEqual(
      destination(of: link), bundled, "the re-pointed link must name this bundle's binary")

    let again = CommandInstallSeam.install(
      linkPath: link, bundled: bundled, runPrivileged: runner.run)
    XCTAssertTrue(
      isCase(again, .alreadyInstalled),
      "a link already to this bundle's binary must report already installed: \(again)")
    XCTAssertEqual(destination(of: link), bundled, "an install that changes nothing keeps it")
    XCTAssertEqual(runner.scripts, [], "a writable directory needs no administrator rights")
  }

  // MARK: - I3: a foreign file

  func testInstallRefusesAForeignLinkAndAFileAndLeavesThem() throws {
    let dir = try scratch()
    let bundled = try makeBundle(in: dir, named: "Wolf.app")
    let runner = Runner()

    let foreignLink = (dir as NSString).appendingPathComponent("linked/wolf")
    let homebrew = (dir as NSString).appendingPathComponent("Cellar/wolf/1.0/bin/wolf")
    try FileManager.default.createDirectory(
      atPath: (foreignLink as NSString).deletingLastPathComponent,
      withIntermediateDirectories: true)
    try FileManager.default.createSymbolicLink(atPath: foreignLink, withDestinationPath: homebrew)
    let linked = CommandInstallSeam.install(
      linkPath: foreignLink, bundled: bundled, runPrivileged: runner.run)
    XCTAssertTrue(
      isCase(linked, .refused), "a link to another program must be refused: \(linked)")
    XCTAssertEqual(
      destination(of: foreignLink), homebrew,
      "the refused link must still name \(homebrew); \(bundled) means install replaced it")

    let plain = (dir as NSString).appendingPathComponent("plain/wolf")
    try FileManager.default.createDirectory(
      atPath: (plain as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
    try "#!/bin/sh\necho another wolf\n".write(toFile: plain, atomically: true, encoding: .utf8)
    let filed = CommandInstallSeam.install(
      linkPath: plain, bundled: bundled, runPrivileged: runner.run)
    XCTAssertTrue(isCase(filed, .refused), "a regular file must be refused: \(filed)")
    XCTAssertNil(destination(of: plain), "the refused regular file must not become a link")
    XCTAssertEqual(
      try String(contentsOfFile: plain, encoding: .utf8), "#!/bin/sh\necho another wolf\n",
      "the refused regular file must keep its contents")
    XCTAssertEqual(runner.scripts, [], "a refusal must ask for no administrator rights")
  }

  // MARK: - I4, I5: uninstall

  func testUninstallRemovesWolfsLinkOnly() throws {
    let dir = try scratch()
    let bundled = try makeBundle(in: dir, named: "Wolf.app")
    let link = (dir as NSString).appendingPathComponent("wolf")
    try FileManager.default.createSymbolicLink(atPath: link, withDestinationPath: bundled)
    let runner = Runner()

    let outcome = CommandInstallSeam.uninstall(linkPath: link, runPrivileged: runner.run)
    XCTAssertTrue(isCase(outcome, .removed), "uninstall of Wolf's link must report removed")
    XCTAssertFalse(exists(link), "uninstall must remove the link \(link)")
    XCTAssertTrue(
      FileManager.default.fileExists(atPath: bundled),
      "uninstall must leave the link's destination \(bundled); gone means it removed through "
        + "the link")
    XCTAssertTrue(
      FileManager.default.fileExists(atPath: dir), "uninstall must leave the link's directory")

    let none = CommandInstallSeam.uninstall(linkPath: link, runPrivileged: runner.run)
    XCTAssertTrue(
      isCase(none, .nothingToRemove), "uninstall with nothing there must say so: \(none)")
    XCTAssertEqual(runner.scripts, [], "a writable directory needs no administrator rights")
  }

  func testUninstallLeavesAForeignLinkAndAFile() throws {
    let dir = try scratch()
    let runner = Runner()
    let foreignLink = (dir as NSString).appendingPathComponent("linked/wolf")
    let homebrew = (dir as NSString).appendingPathComponent("Cellar/wolf/1.0/bin/wolf")
    try FileManager.default.createDirectory(
      atPath: (foreignLink as NSString).deletingLastPathComponent,
      withIntermediateDirectories: true)
    try FileManager.default.createSymbolicLink(atPath: foreignLink, withDestinationPath: homebrew)
    let plain = (dir as NSString).appendingPathComponent("plain/wolf")
    try FileManager.default.createDirectory(
      atPath: (plain as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
    try "#!/bin/sh\n".write(toFile: plain, atomically: true, encoding: .utf8)

    let linked = CommandInstallSeam.uninstall(linkPath: foreignLink, runPrivileged: runner.run)
    XCTAssertTrue(isCase(linked, .refused), "a link to another program must be refused")
    XCTAssertEqual(
      destination(of: foreignLink), homebrew, "uninstall must leave a foreign link in place")
    let filed = CommandInstallSeam.uninstall(linkPath: plain, runPrivileged: runner.run)
    XCTAssertTrue(isCase(filed, .refused), "a regular file must be refused")
    XCTAssertTrue(exists(plain), "uninstall must leave a regular file in place")
    XCTAssertEqual(runner.scripts, [], "a refusal must ask for no administrator rights")
  }

  // MARK: - I6: a directory the user cannot write

  func testALockedDirectoryHandsTheStepToTheAdministratorScript() throws {
    let dir = try scratch()
    let bundled = try makeBundle(in: dir, named: "My \"Wolf's\" Copy.app")
    let locked = (dir as NSString).appendingPathComponent("locked")
    try FileManager.default.createDirectory(atPath: locked, withIntermediateDirectories: true)
    let link = (locked as NSString).appendingPathComponent("wolf")
    try lock(locked)
    defer { try? unlock(locked) }
    guard try premiseLocked(locked) else { return }

    let runner = Runner()
    var createdFirst = false
    runner.before = {
      createdFirst = self.exists(link)
      try? self.unlock(locked)
    }
    let outcome = CommandInstallSeam.install(
      linkPath: link, bundled: bundled, runPrivileged: runner.run)
    XCTAssertEqual(
      runner.scripts.count, 1,
      "a permission error must hand the install to one administrator script; none means the "
        + "seam gave up or reported success without the link")
    XCTAssertFalse(createdFirst, "the unprivileged attempt must create nothing in \(locked)")
    XCTAssertTrue(isCase(outcome, .installed), "the script succeeded, so installed: \(outcome)")
    XCTAssertEqual(
      destination(of: link), bundled,
      "the script, run by a real shell after its AppleScript literal is decoded, must link "
        + "\(link) to \(bundled), a path holding a space, ' and \". Another destination or none "
        + "means a quote broke the command: scripts \(runner.scripts)")

    try lock(locked)
    runner.before = { try? self.unlock(locked) }
    let removed = CommandInstallSeam.uninstall(linkPath: link, runPrivileged: runner.run)
    XCTAssertEqual(
      runner.scripts.count, 2, "uninstall in a locked directory must ask for rights too")
    XCTAssertTrue(isCase(removed, .removed), "the script succeeded, so removed: \(removed)")
    XCTAssertFalse(exists(link), "the uninstall script must remove \(link)")
    XCTAssertTrue(
      FileManager.default.fileExists(atPath: bundled), "the destination must survive uninstall")
  }

  func testAFailedAdministratorScriptIsReportedFailed() throws {
    let dir = try scratch()
    let bundled = try makeBundle(in: dir, named: "Wolf.app")
    let locked = (dir as NSString).appendingPathComponent("locked")
    try FileManager.default.createDirectory(atPath: locked, withIntermediateDirectories: true)
    let link = (locked as NSString).appendingPathComponent("wolf")
    try lock(locked)
    defer { try? unlock(locked) }
    guard try premiseLocked(locked) else { return }

    let runner = Runner()
    runner.execute = false
    let outcome = CommandInstallSeam.install(
      linkPath: link, bundled: bundled, runPrivileged: runner.run)
    XCTAssertTrue(
      isCase(outcome, .failed),
      "a script that fails, as a cancelled password prompt does, must report failed: \(outcome)")
    XCTAssertFalse(exists(link), "a failed install must leave nothing at \(link)")
  }

  // MARK: - I7: a missing directory

  func testAMissingDirectoryIsMadeByTheScriptBeforeTheLink() throws {
    let dir = try scratch()
    let bundled = try makeBundle(in: dir, named: "Wolf.app")
    let missing = (dir as NSString).appendingPathComponent("usr/local/bin")
    let link = (missing as NSString).appendingPathComponent("wolf")
    let runner = Runner()

    let outcome = CommandInstallSeam.install(
      linkPath: link, bundled: bundled, runPrivileged: runner.run)
    XCTAssertEqual(
      runner.scripts.count, 1,
      "a missing directory must hand the install to one administrator script")
    XCTAssertTrue(isCase(outcome, .installed), "the script succeeded, so installed: \(outcome)")
    XCTAssertEqual(
      destination(of: link), bundled,
      "the script must create \(missing) before linking; no link means it ran ln into a "
        + "directory that does not exist: scripts \(runner.scripts)")
  }

  // MARK: - Helpers

  /// The outcomes compared by case alone, so a payload never decides a check.
  private enum Kind {
    case installed, alreadyInstalled, removed, nothingToRemove, refused, failed
  }

  private func isCase(_ outcome: CommandInstallSeam.Outcome, _ kind: Kind) -> Bool {
    switch (outcome, kind) {
    case (.installed, .installed), (.alreadyInstalled, .alreadyInstalled),
      (.removed, .removed), (.nothingToRemove, .nothingToRemove), (.refused, .refused),
      (.failed, .failed):
      return true
    default:
      return false
    }
  }

  /// A fresh directory under `TEST_TMPDIR`, canonical.
  private func scratch() throws -> String {
    let env = ProcessInfo.processInfo.environment
    let tmp = env["TEST_TMPDIR"].flatMap { $0.isEmpty ? nil : $0 } ?? NSTemporaryDirectory()
    let made = (tmp as NSString).appendingPathComponent("install-\(UUID().uuidString)")
    try FileManager.default.createDirectory(atPath: made, withIntermediateDirectories: true)
    return OpenFolderSeam.canonicalDirectory(made)
  }

  /// Makes `<dir>/<name>/Contents/Resources/bin/wolf`, an executable, and returns its path.
  private func makeBundle(in dir: String, named name: String) throws -> String {
    let bundle = (dir as NSString).appendingPathComponent(name)
    let binary = (bundle as NSString).appendingPathComponent(suffix)
    try FileManager.default.createDirectory(
      atPath: (binary as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
    try "#!/bin/sh\n".write(toFile: binary, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: binary)
    return binary
  }

  /// The destination of the symbolic link at `path`, or `nil` when it is no link.
  private func destination(of path: String) -> String? {
    try? FileManager.default.destinationOfSymbolicLink(atPath: path)
  }

  /// Whether anything, a dangling link included, is at `path`.
  private func exists(_ path: String) -> Bool {
    (try? FileManager.default.attributesOfItem(atPath: path)) != nil
  }

  private func lock(_ dir: String) throws {
    try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: dir)
  }

  private func unlock(_ dir: String) throws {
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: dir)
  }

  /// Fails the test, returning `false`, when the user can still write in the locked `dir`, as
  /// root can.
  private func premiseLocked(_ dir: String) throws -> Bool {
    let probe = (dir as NSString).appendingPathComponent("probe")
    let writable = FileManager.default.createFile(atPath: probe, contents: Data())
    if writable {
      try? FileManager.default.removeItem(atPath: probe)
      XCTFail("premise: \(dir) at mode 0555 must refuse the test's writes; is it run as root?")
    }
    return !writable
  }

  /// The shell command an administrator script `do shell script "…" with administrator
  /// privileges` runs, its string literal decoded; `nil` when the script has another shape.
  static func shellCommand(of script: String) -> String? {
    let head = "do shell script \""
    let tail = "\" with administrator privileges"
    guard script.hasPrefix(head) else { return nil }
    var command = ""
    var escaped = false
    var rest = script.dropFirst(head.count)
    while let char = rest.first {
      rest = rest.dropFirst()
      if escaped {
        command.append(char)
        escaped = false
      } else if char == "\\" {
        escaped = true
      } else if char == "\"" {
        // The literal ends here, so everything after it must be the privileges clause.
        let after = "\"" + String(rest)
        return after == tail ? command : nil
      } else {
        command.append(char)
      }
    }
    return nil
  }
}
