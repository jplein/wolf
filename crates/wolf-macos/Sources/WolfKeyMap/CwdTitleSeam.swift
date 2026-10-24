// Pure window-title computation from a working directory, with `~` for home.
// The exact contract: crates/wolf-macos/Sources/WolfKeyMap/README.md#cwdtitleseamswift

import Foundation

/// Computes the window/tab title text from a working-directory path, abbreviating
/// the home directory to `~` (see the file header for the exact contract). A
/// namespace (`enum` with only static members).
public enum CwdTitleSeam {
  /// Returns the display title for `cwd`, abbreviating the `home` prefix to `~`.
  public static func displayTitle(cwd: String, home: String) -> String {
    // Empty / all-whitespace cwd => empty (caller substitutes an app-name).
    if cwd.trimmingCharacters(in: .whitespaces).isEmpty {
      return ""
    }

    // An empty home disables abbreviation; strip ONE trailing "/" otherwise.
    guard !home.isEmpty else { return cwd }
    let normalizedHome = home.hasSuffix("/") ? String(home.dropLast()) : home
    guard !normalizedHome.isEmpty else { return cwd }

    // cwd exactly equals home => "~".
    if cwd == normalizedHome {
      return "~"
    }

    // cwd under home (segment-boundary match only) => "~/" + remainder.
    let homePrefix = normalizedHome + "/"
    if cwd.hasPrefix(homePrefix) {
      return "~/" + cwd.dropFirst(homePrefix.count)
    }

    // Otherwise: verbatim full absolute path.
    return cwd
  }
}
