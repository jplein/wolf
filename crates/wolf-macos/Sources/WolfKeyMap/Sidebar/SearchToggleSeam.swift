// Pure initial states for the sidebar search panel's three toggle buttons.

import Foundation

/// Display-free seeds for the search panel's toggle buttons.
public enum SearchToggleSeam {
  /// The initial "case sensitive" state: the INVERSE of nvim's `:ignorecase`
  /// (ignorecase ON => case-sensitive OFF, and vice versa).
  public static func initialCaseSensitive(ignorecase: Bool) -> Bool {
    !ignorecase
  }

  /// The initial "use regex" state: ON by default.
  public static let defaultUseRegex = true

  /// The initial "search all files" state: OFF by default (Markdown-only).
  public static let defaultAllFiles = false
}
