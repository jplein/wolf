//
//  SearchToggleSeam.swift
//  Pure, headless seeds for the sidebar search panel's three toggle buttons.
//
//  Toolkit-free (Foundation only), so it is unit-testable without AppKit: the
//  `SearchView` reads these when it builds the toggles.
//    - "case sensitive" starts as the INVERSE of nvim's `:ignorecase` (seeded once
//      from `handle.ignorecase()`; toggling it never writes back to nvim).
//    - "use regex" is ON by default.
//    - "search all files" is OFF by default (Markdown-only).
//

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
