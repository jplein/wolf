// Install and Uninstall `wolf` Command: the link of `specs/cli-and-file-open.md` §1.

import Foundation

/// Creates and removes the `wolf` link, Foundation only. A path is touched only when nothing is
/// there or it is a link to some `Wolf.app`'s bundled binary: `/usr/local/bin` is Homebrew's on
/// Intel Macs, so anything else is refused and left alone. A step the user lacks the rights for
/// is handed to `runPrivileged` as an AppleScript run with administrator privileges.
public enum CommandInstallSeam {
  /// The end of every bundled binary's path, which marks a link as Wolf's.
  public static let bundledSuffix = "/Contents/Resources/bin/wolf"

  /// What an install or an uninstall did.
  public enum Outcome {
    /// The link now names the bundled binary.
    case installed
    /// The link already named the bundled binary.
    case alreadyInstalled
    /// Wolf's link is gone.
    case removed
    /// Nothing was at the link's path.
    case nothingToRemove
    /// Something that is not Wolf's is at the path, described, and was left alone.
    case refused(String)
    /// The step failed, for the reason given.
    case failed(String)
  }

  /// What is at a link path.
  private enum Existing {
    case nothing
    /// A symbolic link, with its destination as written.
    case link(String)
    case other
  }

  /// Makes `linkPath` a symbolic link to `bundled`. A missing directory, or one the user cannot
  /// write, hands the step to `runPrivileged`, which answers whether the script succeeded.
  public static func install(
    linkPath: String, bundled: String, runPrivileged: (String) -> Bool
  ) -> Outcome {
    switch existing(linkPath) {
    case .link(let destination) where destination == bundled:
      return .alreadyInstalled
    case .link(let destination) where !destination.hasSuffix(bundledSuffix):
      return .refused("\(linkPath) is a link to \(destination)")
    case .other:
      return .refused("\(linkPath) is not a link to Wolf")
    case .nothing, .link:
      break
    }
    let directory = (linkPath as NSString).deletingLastPathComponent
    func installed() -> Outcome {
      guard case .link(let destination) = existing(linkPath), destination == bundled else {
        return .failed("\(linkPath) does not link to \(bundled)")
      }
      return .installed
    }
    func privileged() -> Outcome {
      let command =
        "/bin/mkdir -p \(shellQuoted(directory)) && "
        + "/bin/ln -sfn \(shellQuoted(bundled)) \(shellQuoted(linkPath))"
      return runPrivileged(script(command)) ? installed() : .failed("the administrator step failed")
    }
    guard FileManager.default.fileExists(atPath: directory) else { return privileged() }
    do {
      if case .link = existing(linkPath) {
        try FileManager.default.removeItem(atPath: linkPath)
      }
      try FileManager.default.createSymbolicLink(atPath: linkPath, withDestinationPath: bundled)
      return installed()
    } catch {
      return needsRights(error) ? privileged() : .failed(error.localizedDescription)
    }
  }

  /// Removes `linkPath` when it is Wolf's link. A directory the user cannot write hands the step
  /// to `runPrivileged`, which answers whether the script succeeded.
  public static func uninstall(linkPath: String, runPrivileged: (String) -> Bool) -> Outcome {
    switch existing(linkPath) {
    case .nothing:
      return .nothingToRemove
    case .link(let destination) where !destination.hasSuffix(bundledSuffix):
      return .refused("\(linkPath) is a link to \(destination)")
    case .other:
      return .refused("\(linkPath) is not a link to Wolf")
    case .link:
      break
    }
    func removed() -> Outcome {
      guard case .nothing = existing(linkPath) else {
        return .failed("\(linkPath) is still there")
      }
      return .removed
    }
    do {
      try FileManager.default.removeItem(atPath: linkPath)
      return removed()
    } catch {
      guard needsRights(error) else { return .failed(error.localizedDescription) }
      let command = "/bin/rm -f \(shellQuoted(linkPath))"
      return runPrivileged(script(command)) ? removed() : .failed("the administrator step failed")
    }
  }

  /// What is at `path`, without following a final link.
  private static func existing(_ path: String) -> Existing {
    guard let attributes = try? FileManager.default.attributesOfItem(atPath: path) else {
      return .nothing
    }
    guard attributes[.type] as? FileAttributeType == .typeSymbolicLink,
      let destination = try? FileManager.default.destinationOfSymbolicLink(atPath: path)
    else { return .other }
    return .link(destination)
  }

  /// Whether `error` is one administrator rights can get past: a permission error, or a
  /// directory that vanished.
  private static func needsRights(_ error: Error) -> Bool {
    let error = error as NSError
    if error.domain == NSCocoaErrorDomain,
      [NSFileWriteNoPermissionError, NSFileNoSuchFileError, NSFileReadNoPermissionError]
        .contains(error.code)
    {
      return true
    }
    guard let posix = error.userInfo[NSUnderlyingErrorKey] as? NSError,
      posix.domain == NSPOSIXErrorDomain
    else { return false }
    return [EACCES, EPERM, ENOENT].contains(Int32(posix.code))
  }

  /// `text` as one POSIX shell word, single-quoted.
  private static func shellQuoted(_ text: String) -> String {
    "'" + text.replacingOccurrences(of: "'", with: "'\\''") + "'"
  }

  /// The AppleScript that runs `command` with administrator privileges; inside its string
  /// literal only `\` and `"` are escaped.
  private static func script(_ command: String) -> String {
    let literal = command.replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
    return "do shell script \"\(literal)\" with administrator privileges"
  }
}
