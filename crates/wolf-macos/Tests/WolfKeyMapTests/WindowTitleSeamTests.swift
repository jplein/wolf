//
//  WindowTitleSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.WindowTitleSeam` namespace.
//
//  A pure, display-free seam (plan §5) that computes a window's title string from
//  the current working directory, whether a folder is open, and the user's home
//  directory. Exercised with scalar string inputs only — NO AppKit, NO window.
//
//  CONTRACT UNDER TEST:
//    * WindowTitleSeam.title(cwd: String, hasFolder: Bool, homeDir: String)
//      -> String
//      1. hasFolder == false -> "Wolf".
//      2. Normalize cwd: strip trailing "/" down to a non-empty remainder; keep
//         bare "/" intact.
//      3. Normalized cwd empty -> "Wolf".
//      4. Normalize homeDir the same way; empty normalized home -> return
//         normalized cwd unchanged (no ~).
//      5. normalized cwd == normalized homeDir -> "~".
//      6. cwd under home on a COMPONENT BOUNDARY (cwd hasPrefix homeDir AND next
//         char after prefix is "/") -> "~" + cwd.dropFirst(homeDir.count).
//      7. otherwise -> normalized cwd unchanged.
//
//  NOTE to coder: this test references the pure `WolfKeyMap.WindowTitleSeam` enum
//  the coder adds in `Sources/WolfKeyMap/WindowTitleSeam.swift`. The single
//  function signature asserted below is the contract the implementation must
//  satisfy.
//

import XCTest

@testable import WolfKeyMap

final class WindowTitleSeamTests: XCTestCase {

    /// One `title` table row: (description, cwd, hasFolder, homeDir, expected).
    private struct Case {
        let desc: String
        let cwd: String
        let hasFolder: Bool
        let homeDir: String
        let expected: String
    }

    func testTitleTable() {
        let cases: [Case] = [
            // --- no folder open => the app name ------------------------------
            Case(
                desc: "no folder, non-empty cwd => Wolf",
                cwd: "/Users/jane/proj", hasFolder: false,
                homeDir: "/Users/jane", expected: "Wolf"),
            Case(
                desc: "no folder, empty cwd => Wolf",
                cwd: "", hasFolder: false,
                homeDir: "/Users/jane", expected: "Wolf"),
            // --- cwd equals home => tilde ------------------------------------
            Case(
                desc: "folder, cwd == home => ~",
                cwd: "/Users/jane", hasFolder: true,
                homeDir: "/Users/jane", expected: "~"),
            Case(
                desc: "folder, cwd == home + trailing slash => ~",
                cwd: "/Users/jane/", hasFolder: true,
                homeDir: "/Users/jane", expected: "~"),
            // --- cwd under home => ~/remainder -------------------------------
            Case(
                desc: "folder, cwd under home => ~/Git/jplein/wolf",
                cwd: "/Users/jane/Git/jplein/wolf", hasFolder: true,
                homeDir: "/Users/jane", expected: "~/Git/jplein/wolf"),
            Case(
                desc: "folder, home trailing slash, cwd under home => ~/x",
                cwd: "/Users/jane/x", hasFolder: true,
                homeDir: "/Users/jane/", expected: "~/x"),
            Case(
                desc: "folder, deeper cwd under home => ~/a/b/c",
                cwd: "/Users/jane/a/b/c", hasFolder: true,
                homeDir: "/Users/jane", expected: "~/a/b/c"),
            Case(
                desc: "folder, cwd trailing slash under home => ~/x",
                cwd: "/Users/jane/x/", hasFolder: true,
                homeDir: "/Users/jane", expected: "~/x"),
            // --- component boundary: NOT a prefix match ----------------------
            Case(
                desc: "boundary: /Users/janet not under /Users/jane",
                cwd: "/Users/janet", hasFolder: true,
                homeDir: "/Users/jane", expected: "/Users/janet"),
            Case(
                desc: "boundary: /Users/janet/x not under /Users/jane",
                cwd: "/Users/janet/x", hasFolder: true,
                homeDir: "/Users/jane", expected: "/Users/janet/x"),
            // --- cwd not under home => unchanged -----------------------------
            Case(
                desc: "folder, cwd not under home => /tmp/project",
                cwd: "/tmp/project", hasFolder: true,
                homeDir: "/Users/jane", expected: "/tmp/project"),
            Case(
                desc: "folder, cwd trailing slash not under home => /tmp/project",
                cwd: "/tmp/project/", hasFolder: true,
                homeDir: "/Users/jane", expected: "/tmp/project"),
            // --- normalized cwd empty => the app name ------------------------
            Case(
                desc: "folder, empty cwd => Wolf",
                cwd: "", hasFolder: true,
                homeDir: "/Users/jane", expected: "Wolf"),
            // --- empty home => no tilde substitution -------------------------
            Case(
                desc: "folder, empty home => cwd unchanged",
                cwd: "/Users/jane/x", hasFolder: true,
                homeDir: "", expected: "/Users/jane/x"),
            // --- root edge => bare slash kept intact -------------------------
            Case(
                desc: "folder, cwd is root => /",
                cwd: "/", hasFolder: true,
                homeDir: "/Users/jane", expected: "/"),
        ]

        for c in cases {
            let got = WindowTitleSeam.title(
                cwd: c.cwd, hasFolder: c.hasFolder, homeDir: c.homeDir)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }
}
