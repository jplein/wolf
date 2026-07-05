//
//  WindowTitleSeam.swift
//  Pure, AppKit-free seam that computes a window's title string (plan §1).
//
//  A pure, display-free seam exercised with scalar string inputs only — NO
//  AppKit, NO window. It maps the live working directory (plus whether a folder
//  is open and the user's home directory) to the string shown in the OS window
//  title bar: "Wolf" for an empty-state window, "~"/"~/…" for a cwd at or under
//  home, else the cwd unchanged.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap`
//  module alongside `WindowIndexSeam.swift` / `PickerSeam.swift`.
//

import Foundation

/// The pure seam picking a window's title. A namespace (`enum` with only static
/// members) mirroring `WindowIndexSeam` / `PickerSeam` / `ShortcutSeam`.
public enum WindowTitleSeam {
    /// The title for a window given its live `cwd`, whether a folder is open, and
    /// the user's `homeDir`. Semantics:
    ///  1. `hasFolder == false` -> "Wolf".
    ///  2. Normalize `cwd`: strip trailing "/" down to a non-empty remainder;
    ///     keep a bare "/" intact.
    ///  3. Normalized cwd empty -> "Wolf".
    ///  4. Normalize `homeDir` the same way; empty normalized home -> the
    ///     normalized cwd unchanged (no "~").
    ///  5. normalized cwd == normalized home -> "~".
    ///  6. cwd under home on a COMPONENT BOUNDARY (normalized cwd `hasPrefix`
    ///     normalized home AND the char one past the prefix is "/") ->
    ///     "~" + `cwd.dropFirst(home.count)` (yields "~/" + remainder). Because
    ///     the boundary char must be "/", `/Users/janet` does NOT match home
    ///     `/Users/jane`.
    ///  7. otherwise -> the normalized cwd unchanged.
    public static func title(cwd: String, hasFolder: Bool, homeDir: String) -> String {
        guard hasFolder else { return "Wolf" }

        let normalizedCwd = normalize(cwd)
        guard !normalizedCwd.isEmpty else { return "Wolf" }

        let normalizedHome = normalize(homeDir)
        guard !normalizedHome.isEmpty else { return normalizedCwd }

        if normalizedCwd == normalizedHome {
            return "~"
        }

        if normalizedCwd.hasPrefix(normalizedHome) {
            let boundary = normalizedCwd.index(
                normalizedCwd.startIndex, offsetBy: normalizedHome.count)
            if boundary < normalizedCwd.endIndex, normalizedCwd[boundary] == "/" {
                return "~" + normalizedCwd.dropFirst(normalizedHome.count)
            }
        }

        return normalizedCwd
    }

    /// Strips trailing "/" from `path` down to a non-empty remainder, keeping a
    /// bare "/" (root) intact. Empty stays empty.
    private static func normalize(_ path: String) -> String {
        var result = Substring(path)
        while result.count > 1, result.hasSuffix("/") {
            result = result.dropLast()
        }
        return String(result)
    }
}
