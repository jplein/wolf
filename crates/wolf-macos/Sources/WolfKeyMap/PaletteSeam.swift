//
//  PaletteSeam.swift
//  Pure, AppKit-free decision seams for the macOS command palette.
//
//  The command-palette sibling of `PickerSeam.swift`: pure, display-free seams
//  taking scalar inputs only — NO NSEvent, NO window, NO AppKit — so they are
//  unit-testable headlessly (see `PaletteSeamTests.swift`).
//
//  NOTE (plan §10): `isPaletteTrigger` is DORMANT/UNWIRED. ⌘P is a menu
//  key-equivalent that preempts `keyDown` (exactly like ⌘N), so the menu item is
//  the real trigger and this predicate is NOT called from
//  `PaneContainerView.keyDown`. It is kept for consistency with `PickerSeam` (and
//  pinned by a test) so a future reader has the palette chord in the same shape as
//  the picker chord.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap`
//  module alongside `PickerSeam.swift`.
//

import Foundation

/// The pure decision functions backing the macOS command palette. A namespace
/// (`enum` with only static members) mirroring `PickerSeam`.
public enum PaletteSeam {
    /// The FFI palette-row-kind discriminant, decoded from `wolf-ffi`'s
    /// `mod palette_row_kind` (STATIC=0, SWITCH_WINDOW=1, OPEN_RECENT=2).
    ///
    /// Structurally the sibling of `CommandKind(ffi:)`: a `UInt8`-backed enum
    /// with a guarded `init?(ffi:)` that returns `nil` for any out-of-range
    /// discriminant.
    public enum PaletteRowKind: UInt8 {
        /// A static registry command row (dispatched via the command channel).
        case staticCommand = 0
        /// A "Switch to Window" row (raises the window at its window index).
        case switchWindow = 1
        /// An "Open Recent Folder" row (opens-or-raises the recent dir at its path).
        case openRecent = 2

        /// Maps an FFI palette-row-kind discriminant (`0` → `.staticCommand`,
        /// `1` → `.switchWindow`, `2` → `.openRecent`, else `nil`), guarding the
        /// contract shared with `wolf-ffi`'s `mod palette_row_kind`.
        public init?(ffi: UInt8) {
            self.init(rawValue: ffi)
        }
    }

    /// Returns `true` iff the keypress is the palette trigger chord **Command+P**.
    ///
    /// The rule is `command && firstScalar.lowercased == "p"`, matching the letter
    /// case-insensitively — the palette sibling of `PickerSeam.isPickerTrigger`'s
    /// Command+O ("o" → "p"). `charactersIgnoringModifiers` is layout independent
    /// and, unlike when Ctrl is held, is NOT mangled into a control scalar when
    /// Command is held — so it still carries the plain `"p"`/`"P"`. Shift and other
    /// modifiers (ctrl/alt) are not part of the signature and so are neither
    /// required nor disqualifying. An empty string (no first scalar) is `false`.
    ///
    /// DORMANT: see the file header — ⌘P is a menu key-equivalent, so this is not
    /// called from `keyDown`.
    public static func isPaletteTrigger(
        charactersIgnoringModifiers: String,
        command: Bool
    ) -> Bool {
        guard command else { return false }
        guard let first = charactersIgnoringModifiers.unicodeScalars.first else {
            return false
        }
        return String(first).lowercased() == "p"
    }
}
