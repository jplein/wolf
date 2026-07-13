//
//  ShortcutSeam.swift
//  Pure, AppKit-free decision seams for macOS keyboard shortcuts.
//
//  The sibling of `PickerSeam`: a pure, display-free seam exercised with scalar
//  inputs only — NO NSEvent, NO window, NO AppKit. It decides whether a keystroke
//  is the Command-N "new window" chord (plan §5). It mirrors the shape of
//  `PickerSeam.isPickerTrigger` but matches the letter "n" instead of "o".
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap`
//  module alongside `PickerSeam.swift`.
//

import Foundation

/// The pure decision functions backing the macOS window shortcuts. A namespace
/// (`enum` with only static members) mirroring `PickerSeam`.
public enum ShortcutSeam {
  /// Returns `true` iff the keypress is the new-window trigger chord
  /// **Command+N**.
  ///
  /// The rule is `command && firstScalar.lowercased == "n"`, matching the
  /// letter case-insensitively. `charactersIgnoringModifiers` is layout
  /// independent and, unlike when Ctrl is held, is NOT mangled into a control
  /// scalar when Command is held — so it still carries the plain `"n"`/`"N"`
  /// (uppercase when Shift is also down). An empty string (no first scalar) is
  /// `false`. Guards against colliding with the Command+O picker chord by
  /// matching "n", not "o".
  public static func isNewWindowTrigger(
    charactersIgnoringModifiers: String,
    command: Bool
  ) -> Bool {
    guard command else { return false }
    guard let first = charactersIgnoringModifiers.unicodeScalars.first else {
      return false
    }
    return String(first).lowercased() == "n"
  }
}
