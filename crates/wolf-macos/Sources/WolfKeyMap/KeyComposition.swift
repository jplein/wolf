//
//  KeyComposition.swift
//  Pure, AppKit-free decision seam for the macOS option-as-option feature.
//
//  This brackets the untestable Carbon/`UCKeyTranslate` glue (`KeyComposer`, in
//  the WolfMacOS module) with two pure decisions that CAN be tested headlessly:
//
//    * `routesThroughComposer(keyCode:ctrl:meta:optionAsAlt:)` — whether a key
//      press should be routed through the dead-key composer (option-as-option)
//      rather than the existing `WolfKeyMap.translate` path (option-as-alt / any
//      ctrl/meta combo / a named key).
//    * `charEvents(for:)` — turns a composed string into one modifier-free
//      `FfiKeyEvent` per Unicode scalar, filtering C0/DEL control scalars.
//
//  It imports ONLY Foundation and the generated `FfiKeyEvent`, so it is
//  unit-testable with scalar inputs (no `NSEvent`, no Carbon, no window).

import Foundation

/// `FfiKeyEvent.kind` discriminant for a printable Char (mirrors the Rust
/// `FfiKeyEvent` kind scheme, where `Char` is 0). Matches `KeyMap.swift`.
private let kindChar: UInt8 = 0

/// The DEL control scalar (`0x7F`), the upper bound of the C0 control range.
/// Mirrors `KeyMap.swift`'s `delScalar`.
private let delScalar: UInt32 = 0x7F
/// One past the last C0 control scalar (`0x20` = space, the first printable).
/// Mirrors `KeyMap.swift`'s `firstPrintableScalar`.
private let firstPrintableScalar: UInt32 = 0x20

/// Pure decision seam bracketing the AppKit/Carbon dead-key composer.
public enum KeyComposition {
  /// Whether a key press should be routed through the dead-key composer rather
  /// than the existing `WolfKeyMap.translate` path.
  ///
  /// True IFF option-as-option is active (`!optionAsAlt`), neither ctrl nor meta
  /// is held, and the key is not a NAMED key. In option-as-option mode EVERY
  /// printable-candidate key routes through the composer (so the committing
  /// keystroke of a dead-key pair composes); when option-as-alt is on, ctrl/meta
  /// is held, or the key is named (Return, arrows, F-keys), `translate` owns it.
  public static func routesThroughComposer(
    keyCode: UInt16,
    ctrl: Bool,
    meta: Bool,
    optionAsAlt: Bool
  ) -> Bool {
    !optionAsAlt && !ctrl && !meta && !WolfKeyMap.isNamedKey(keyCode: keyCode)
  }

  /// Turns a composed string into one modifier-free Char `FfiKeyEvent` per
  /// Unicode scalar, filtering C0 (`< 0x20`) and DEL (`0x7F`) control scalars.
  /// All four modifiers are false and `kind` is Char; an empty or control-only
  /// string yields no events (e.g. a dead key still pending → `""`).
  public static func charEvents(for composed: String) -> [FfiKeyEvent] {
    composed.unicodeScalars.compactMap { scalar in
      if scalar.value < firstPrintableScalar || scalar.value == delScalar {
        return nil  // a control scalar: not input.
      }
      return FfiKeyEvent(
        kind: kindChar,
        ch: scalar.value,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false
      )
    }
  }
}
