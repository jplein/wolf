//
//  PickerSeam.swift
//  Pure, AppKit-free decision seams for the macOS file picker.
//
//  These are the macOS analog of wolf-gnome's pure picker seams in
//  `crates/wolf-gnome/src/picker.rs` (`is_picker_trigger`, `next_selection`,
//  `restored_index`). They take scalar inputs only — NO NSEvent, NO window, NO
//  AppKit — so they are unit-testable headlessly (see `PickerSeamTests.swift`).
//  The semantics MUST stay identical to the Rust seams they mirror.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap`
//  module alongside `KeyMap.swift`.
//

import Foundation

/// The pure decision functions backing the macOS file picker. A namespace
/// (`enum` with only static members) mirroring `WolfKeyMap`.
public enum PickerSeam {
    /// Returns `true` iff the keypress is the picker trigger chord
    /// **Ctrl+Shift+P**.
    ///
    /// The rule is `ctrl && shift && firstScalar.lowercased == "p"`, matching the
    /// letter case-insensitively (with Shift held, `charactersIgnoringModifiers`
    /// carries the uppercase `P`). `charactersIgnoringModifiers` is layout
    /// independent and, unlike `characters`, is not mangled into a control scalar
    /// when Ctrl is held. Other modifiers (alt/meta) are not part of the signature
    /// and so are neither required nor disqualifying. An empty string (no first
    /// scalar) is `false`. The exact analog of Rust `is_picker_trigger`.
    public static func isPickerTrigger(
        charactersIgnoringModifiers: String,
        ctrl: Bool,
        shift: Bool
    ) -> Bool {
        guard ctrl, shift else { return false }
        guard let first = charactersIgnoringModifiers.unicodeScalars.first else {
            return false
        }
        return String(first).lowercased() == "p"
    }

    /// Computes the next list selection index after moving by `delta` rows from
    /// `current`, clamped into `[0, nRows - 1]` with NO wrap. Returns `nil` when
    /// the list is empty (`nRows <= 0`).
    ///
    /// The formula is exactly `clamp(current + delta, 0, nRows - 1)`: an
    /// out-of-range or negative `current` is handled by the same clamp on the sum.
    /// The exact analog of Rust `next_selection` (`Int?` for `Option<usize>`).
    public static func nextSelection(current: Int, delta: Int, nRows: Int) -> Int? {
        guard nRows > 0 else { return nil }
        return min(max(current + delta, 0), nRows - 1)
    }

    /// Chooses which row index to select after a refresh rebuild. Returns `nil`
    /// for an empty list (`newLen == 0`); otherwise `0` when resetting to the top
    /// (query changed / first open), else the preserved `prev` index (defaulting
    /// to `0`) clamped into the new range. The exact analog of Rust
    /// `restored_index`.
    public static func restoredIndex(resetToTop: Bool, prev: Int?, newLen: Int) -> Int? {
        guard newLen > 0 else { return nil }
        if resetToTop { return 0 }
        return min(prev ?? 0, newLen - 1)
    }

    // MARK: - macOS-only presentation seams (no Rust/wolf-gnome parity)
    //
    // Unlike the three trigger/nav/restore ports above, the following seams have
    // NO wolf-gnome analog: they back macOS-specific picker presentation choices
    // (hide results until the user types; split a cwd-relative display path into a
    // filename title and a parent-directory subtitle). They stay here — pure and
    // Foundation-only — so they remain headlessly unit-testable.

    /// Returns `true` iff the picker should show result rows for `query`, i.e.
    /// the query has at least one non-whitespace character. An empty or
    /// whitespace-only query hides all results (the picker opens showing just the
    /// search field).
    public static func shouldShowResults(query: String) -> Bool {
        !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// Derives a row's primary title — the base filename — from a cwd-relative
    /// display path, via `NSString.lastPathComponent` (which also trims a trailing
    /// slash). An empty path yields `""`.
    public static func baseName(fromRelativePath relativePath: String) -> String {
        (relativePath as NSString).lastPathComponent
    }

    /// Derives a row's subtitle — the relative parent directory — from a
    /// cwd-relative display path, via `NSString.deletingLastPathComponent`. A
    /// cwd-root file (no parent) and an empty path both yield `""`.
    public static func parentRelativePath(fromRelativePath relativePath: String) -> String {
        (relativePath as NSString).deletingLastPathComponent
    }
}
