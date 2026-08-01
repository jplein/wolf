//
//  CwdTitleSeam.swift
//  Pure, AppKit-free window-title computation seam.
//
//  Computes the window/tab title text from a working-directory path,
//  abbreviating the user's home directory to `~` per the macOS convention (plan
//  §A). A pure, display-free seam exercised with scalar string inputs only — NO
//  AppKit, NO window, NO FileManager, NO implicit `$HOME` (the `home` argument
//  is always injected, which is load-bearing for determinism) — so it lives in
//  the Foundation-only `WolfKeyMap` module alongside `CommandSeam.swift` /
//  `PaneTree.swift`.
//
//  CONTRACT:
//    * empty or all-whitespace `cwd` => "" (caller substitutes an app-name
//      fallback).
//    * `home` is normalized by stripping ONE trailing "/" before comparison; an
//      empty `home` disables abbreviation and returns `cwd` unchanged.
//    * `cwd == normalizedHome` => "~".
//    * `cwd` begins with `normalizedHome + "/"` => "~/" + <remainder after the
//      "home/">. Abbreviation happens ONLY on a full path-segment boundary, so
//      "/Users/janet/x" must NOT be abbreviated against home "/Users/jane".
//    * otherwise (cwd not under home) => `cwd` returned verbatim (full absolute
//      path).
//

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
