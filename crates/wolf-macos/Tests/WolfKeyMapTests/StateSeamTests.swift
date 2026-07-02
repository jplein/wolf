//
//  StateSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.StateSeam` namespace.
//
//  These exercise the AppKit-FREE seam of the macOS state-restore path
//  (plan §2, `Sources/WolfKeyMap/StateSeam.swift`) with scalar inputs only —
//  NO NSWindow, NO NSScreen, NO NSToolbar, NO AppKit. Foundation/CoreGraphics
//  types (CGRect) only, exactly like PaneGeometry.
//
//  CONTRACT UNDER TEST (the signatures the implementation must satisfy):
//    * StateSeam.ToolbarMode — a UInt8-raw enum mirroring wolf-ffi's
//      `mod toolbar_mode` discriminant table EXACTLY (the single source of
//      truth, in wolf_core::ToolbarMode variant order):
//        .iconAndText = 0, .iconOnly = 1, .textOnly = 2
//      Both directions of the discriminant mapping ride on RawRepresentable:
//      `ToolbarMode(rawValue:)` (discriminant -> case; nil for an unknown
//      discriminant) and `.rawValue` (case -> discriminant).
//    * StateSeam.restoredWindowFrame(x:y:width:height:screenVisibleFrames:)
//      -> CGRect? — nil when width or height is non-positive (the "unset"
//      sentinel, plan §1) and nil when the rect intersects NO screen visible
//      frame (never restore a window fully offscreen); otherwise the rect
//      CGRect(x:y:width:height).
//    * StateSeam.launchCwd(restored:fallback:isDirectory:) -> String —
//      `restored` only when it is non-empty AND `isDirectory(restored)`;
//      otherwise `fallback`. The injected closure keeps the seam pure (no
//      FileManager in tests).
//
//  NOTE to coder: this test references the pure `WolfKeyMap.StateSeam` enum the
//  coder will add in `Sources/WolfKeyMap/StateSeam.swift` (and list in the
//  `wolf_key_map` srcs in BUILD.bazel). The signatures asserted above are the
//  contract the implementation must satisfy; if the discriminant mapping is
//  spelled differently (e.g. static funcs instead of a raw enum), adapt these
//  call sites — the 0/1/2 table itself is NOT negotiable (it must mirror
//  wolf-ffi's `mod toolbar_mode`).
//

import CoreGraphics
import XCTest

@testable import WolfKeyMap

/// A typical main-screen visible frame (menu bar + Dock already subtracted).
private let kMainScreen = CGRect(x: 0, y: 0, width: 1920, height: 1055)
/// A second display arranged to the right of the main screen.
private let kSecondScreen = CGRect(x: 1920, y: 0, width: 1440, height: 900)

final class StateSeamTests: XCTestCase {

    // MARK: - ToolbarMode discriminants

    /// One discriminant-mapping table row: (description, discriminant,
    /// expected case; nil == unknown discriminant).
    private struct ModeCase {
        let desc: String
        let discriminant: UInt8
        let expected: StateSeam.ToolbarMode?
    }

    func testToolbarModeDiscriminantTable() {
        // MUST mirror wolf-ffi's `mod toolbar_mode` (ICON_AND_TEXT = 0,
        // ICON_ONLY = 1, TEXT_ONLY = 2) — the cross-language contract guard.
        let cases: [ModeCase] = [
            ModeCase(desc: "0 => iconAndText", discriminant: 0, expected: .iconAndText),
            ModeCase(desc: "1 => iconOnly", discriminant: 1, expected: .iconOnly),
            ModeCase(desc: "2 => textOnly", discriminant: 2, expected: .textOnly),
            ModeCase(desc: "3 (out of table) => nil", discriminant: 3, expected: nil),
            ModeCase(desc: "99 (out of table) => nil", discriminant: 99, expected: nil),
        ]
        for c in cases {
            let got = StateSeam.ToolbarMode(rawValue: c.discriminant)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }

        // The reverse direction: case -> discriminant.
        XCTAssertEqual(StateSeam.ToolbarMode.iconAndText.rawValue, 0, "iconAndText => 0")
        XCTAssertEqual(StateSeam.ToolbarMode.iconOnly.rawValue, 1, "iconOnly => 1")
        XCTAssertEqual(StateSeam.ToolbarMode.textOnly.rawValue, 2, "textOnly => 2")
    }

    // MARK: - restoredWindowFrame

    /// One `restoredWindowFrame` table row: (description, saved geometry,
    /// screen visible frames, expected; nil == "do not restore, use defaults").
    private struct FrameCase {
        let desc: String
        let x: Double
        let y: Double
        let width: Double
        let height: Double
        let screens: [CGRect]
        let expected: CGRect?
    }

    func testRestoredWindowFrameTable() {
        let cases: [FrameCase] = [
            // --- accepted: intersects a screen visible frame -----------------
            FrameCase(
                desc: "fully on the main screen => restored as-is",
                x: 128, y: 64, width: 1180, height: 760,
                screens: [kMainScreen],
                expected: CGRect(x: 128, y: 64, width: 1180, height: 760)),
            FrameCase(
                desc: "partially off the left edge but intersecting => restored",
                x: -600, y: 100, width: 800, height: 600,
                screens: [kMainScreen],
                expected: CGRect(x: -600, y: 100, width: 800, height: 600)),
            FrameCase(
                desc: "on the SECOND screen only => restored (any screen counts)",
                x: 2000, y: 50, width: 800, height: 600,
                screens: [kMainScreen, kSecondScreen],
                expected: CGRect(x: 2000, y: 50, width: 800, height: 600)),
            // --- rejected: non-positive size (the "unset" sentinel) ----------
            FrameCase(
                desc: "zero width (unset sentinel) => nil",
                x: 128, y: 64, width: 0, height: 760,
                screens: [kMainScreen],
                expected: nil),
            FrameCase(
                desc: "zero height (unset sentinel) => nil",
                x: 128, y: 64, width: 1180, height: 0,
                screens: [kMainScreen],
                expected: nil),
            FrameCase(
                desc: "zero-everything (fresh default state) => nil",
                x: 0, y: 0, width: 0, height: 0,
                screens: [kMainScreen],
                expected: nil),
            FrameCase(
                desc: "negative width => nil",
                x: 128, y: 64, width: -100, height: 760,
                screens: [kMainScreen],
                expected: nil),
            FrameCase(
                desc: "negative height => nil",
                x: 128, y: 64, width: 1180, height: -1,
                screens: [kMainScreen],
                expected: nil),
            // --- rejected: fully offscreen -----------------------------------
            FrameCase(
                desc: "fully right of every screen => nil",
                x: 5000, y: 100, width: 800, height: 600,
                screens: [kMainScreen, kSecondScreen],
                expected: nil),
            FrameCase(
                desc: "fully below every screen => nil",
                x: 100, y: -2000, width: 800, height: 600,
                screens: [kMainScreen],
                expected: nil),
            FrameCase(
                desc: "no screens at all => nil",
                x: 128, y: 64, width: 1180, height: 760,
                screens: [],
                expected: nil),
        ]

        for c in cases {
            let got = StateSeam.restoredWindowFrame(
                x: c.x, y: c.y, width: c.width, height: c.height,
                screenVisibleFrames: c.screens)
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }

    // MARK: - launchCwd

    /// One `launchCwd` table row: (description, restored, fallback, the set of
    /// paths the injected `isDirectory` closure reports as directories,
    /// expected result).
    private struct CwdCase {
        let desc: String
        let restored: String
        let fallback: String
        let directories: Set<String>
        let expected: String
    }

    func testLaunchCwdTable() {
        let restoredDir = "/Users/alice/projects/notes"
        let fallbackDir = "/Users/alice"
        let cases: [CwdCase] = [
            CwdCase(
                desc: "non-empty restored that IS a directory => restored",
                restored: restoredDir, fallback: fallbackDir,
                directories: [restoredDir, fallbackDir],
                expected: restoredDir),
            CwdCase(
                desc: "empty restored (never reported) => fallback",
                restored: "", fallback: fallbackDir,
                directories: [fallbackDir],
                expected: fallbackDir),
            CwdCase(
                desc: "restored no longer a directory (deleted since) => fallback",
                restored: restoredDir, fallback: fallbackDir,
                directories: [fallbackDir],
                expected: fallbackDir),
            CwdCase(
                desc: "restored exists but is a plain FILE (not a dir) => fallback",
                restored: "/Users/alice/notes.md", fallback: fallbackDir,
                directories: [fallbackDir],
                expected: fallbackDir),
        ]

        for c in cases {
            let got = StateSeam.launchCwd(
                restored: c.restored,
                fallback: c.fallback,
                isDirectory: { c.directories.contains($0) })
            XCTAssertEqual(got, c.expected, "case: \(c.desc)")
        }
    }
}
