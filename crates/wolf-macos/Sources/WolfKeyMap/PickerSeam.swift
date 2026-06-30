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
}
