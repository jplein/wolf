//
//  PickerSeam.swift
//  Pure, AppKit-free decision seams for the macOS file picker.
//
//  These are the macOS analog of wolf-gnome's pure picker seams in
//  `crates/wolf-gnome/src/picker.rs` (`next_selection`, `restored_index`). They
//  take scalar inputs only — NO NSEvent, NO window, NO AppKit — so they are
//  unit-testable headlessly (see `PickerSeamTests.swift`). The semantics of
//  those two MUST stay identical to the Rust seams they mirror.
//
//  `isPickerTrigger` below is the exception: it has no Rust counterpart to
//  mirror any more. wolf-gnome deleted its `is_picker_trigger` predicate when
//  the file-picker chord moved to Ctrl+Shift+O, and the Linux chord is now a
//  table-driven `wolf_gnome::actions::action_for_key` lookup against
//  `ACTION_TABLE` that resolves to the `win.go-to-file` action.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap`
//  module alongside `KeyMap.swift`.
//

import Foundation

/// The pure decision functions backing the macOS file picker. A namespace
/// (`enum` with only static members) mirroring `WolfKeyMap`.
public enum PickerSeam {
  /// Returns `true` iff the keypress is the picker trigger chord
  /// **Command+O**.
  ///
  /// The rule is `command && firstScalar.lowercased == "o"`, matching the letter
  /// case-insensitively. `charactersIgnoringModifiers` is layout independent and,
  /// unlike when Ctrl is held, is NOT mangled into a control scalar when Command
  /// is held — so it still carries the plain `"o"`/`"O"`. Shift and other
  /// modifiers (ctrl/alt) are not part of the signature and so are neither
  /// required nor disqualifying. An empty string (no first scalar) is `false`.
  ///
  /// Both platforms now agree on the LETTER: wolf-gnome's file picker is
  /// Ctrl+Shift+O (it was Ctrl+Shift+P until the command-drain unit moved it and
  /// reserved P for the command palette). What differs is the MODIFIER and the
  /// MECHANISM.
  ///
  /// The modifier difference is deliberate and still load-bearing: Command+O is
  /// the idiomatic macOS "open" chord and, unlike Ctrl-modified keys, is not a
  /// Neovim keybinding — so macOS gets a whole free modifier and needs no Shift.
  /// Linux has no such modifier: there is effectively no `Ctrl+<letter>` free in
  /// all Neovim modes, so wolf-gnome is forced onto the GNOME-Terminal-style
  /// `Ctrl+Shift+<letter>` namespace, which gives up only the Ctrl+Shift alias
  /// for `<C-o>` (`<C-o>` itself is still typed as plain Ctrl+O, which Wolf
  /// never binds).
  ///
  /// The mechanism difference: this is a Swift predicate, whereas the Linux side
  /// is a table lookup (`actions::action_for_key` -> `win.go-to-file`), so there
  /// is no longer a Rust predicate for this function to be a parity port of. See
  /// `crates/wolf-gnome/README.md`, "Keyboard accelerators".
  public static func isPickerTrigger(
    charactersIgnoringModifiers: String,
    command: Bool
  ) -> Bool {
    guard command else { return false }
    guard let first = charactersIgnoringModifiers.unicodeScalars.first else {
      return false
    }
    return String(first).lowercased() == "o"
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
