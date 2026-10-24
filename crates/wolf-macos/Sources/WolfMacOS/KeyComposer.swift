// Dead-key and Option-character composition over Carbon's `UCKeyTranslate`.

import AppKit
import Carbon.HIToolbox

/// Composes dead-key / Option characters for the current keyboard layout via
/// `UCKeyTranslate`, persisting the dead-key state across calls.
final class KeyComposer {
  /// The persisted Carbon dead-key state. Non-zero while a dead key (e.g. the
  /// first press of Option-u) is pending; threaded through each `UCKeyTranslate`
  /// call so the committing keystroke composes the accented glyph.
  private var deadKeyState: UInt32 = 0

  /// Composes the key at `keyCode` (with the given `shift`/`option`/`capsLock`
  /// modifiers) against the current keyboard layout, returning the produced
  /// string.
  ///
  /// Returns "" while a dead key is pending (the first press of an Option
  /// dead-key sequence produced no committed character yet) — the caller treats
  /// an empty result as "consume, do not send". The persisted `deadKeyState`
  /// carries the pending composition into the next call. `capsLock` maps to the
  /// Carbon `alphaLock` bit so letters uppercase with Caps Lock on (matching the
  /// old `event.characters` path this replaces in option-as-option mode).
  func compose(keyCode: UInt16, shift: Bool, option: Bool, capsLock: Bool) -> String {
    guard let source = TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue(),
      let layoutDataRef = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData)
    else {
      return ""
    }
    // The property is a CFData wrapping a `UCKeyboardLayout`.
    let layoutData = unsafeBitCast(layoutDataRef, to: CFData.self)
    // `CFDataGetBytePtr` can return null for empty CFData — bail rather than crash.
    guard let layoutBytes = CFDataGetBytePtr(layoutData) else {
      return ""
    }

    // Pack the Carbon modifier bitfield into the `modifierKeyState` byte
    // `UCKeyTranslate` expects (the high byte of the EventRecord modifiers).
    var modifierKeyState: UInt32 = 0
    if shift { modifierKeyState |= UInt32(shiftKey >> 8) }
    if option { modifierKeyState |= UInt32(optionKey >> 8) }
    if capsLock { modifierKeyState |= UInt32(alphaLock >> 8) }

    let maxLength = 4
    var chars = [UniChar](repeating: 0, count: maxLength)
    var actualLength = 0

    let status = layoutBytes.withMemoryRebound(
      to: UCKeyboardLayout.self, capacity: 1
    ) { layoutPtr in
      UCKeyTranslate(
        layoutPtr,
        keyCode,
        UInt16(kUCKeyActionDown),
        modifierKeyState,
        UInt32(LMGetKbdType()),
        // keyTranslateOptions = 0: do NOT set the no-dead-keys bit, so dead keys
        // (Option-u, …) ARE processed and their state persists in `deadKeyState`.
        0,
        &deadKeyState,
        maxLength,
        &actualLength,
        &chars)
    }
    guard status == noErr else {
      return ""
    }
    return String(utf16CodeUnits: chars, count: actualLength)
  }

  /// Clears any pending dead-key state (e.g. on focus loss), so a later keystroke
  /// starts a fresh composition rather than committing a stale accent.
  func reset() {
    deadKeyState = 0
  }
}
