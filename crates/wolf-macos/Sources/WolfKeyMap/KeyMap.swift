//
//  KeyMap.swift
//  Pure, AppKit-free key translation for the macOS front-end.
//
//  `WolfKeyMap.translate(...)` is the macOS analog of wolf-gnome's `gdk_key`
//  seam: it takes exactly the data an `NSView.keyDown(_:)` pulls off an
//  `NSEvent` — the virtual `keyCode`, `characters`, `charactersIgnoringModifiers`,
//  and the four modifier booleans — and returns a generated `FfiKeyEvent?` ready
//  to hand to the Rust bridge (`WolfCoreHandle.send_key`). It imports ONLY
//  Foundation and the generated FFI struct, so it is unit-testable headlessly
//  (no `NSEvent`, no window, no event loop).
//
//  ## The Shift contract (must match wolf-core/src/input.rs)
//
//  * Named keys (Return, Tab, arrows, F-keys, …) are recognized from `keyCode`
//    first — their `characters` is often a control scalar — and KEEP the real
//    `shift` flag, so wolf-core can emit `<S-Tab>`, `<S-Left>`, etc.
//  * Printable characters carry the ALREADY-SHIFTED glyph in `ch` with
//    `shift = false` (wolf-core ignores `shift` for `Key::Char` and expects the
//    shifted glyph). Shift+'a' → 'A', Shift+'1' → '!', both `shift = false`.
//  * When ctrl/alt/meta are held, the glyph is taken from
//    `charactersIgnoringModifiers` (lowercased), never the modifier-mangled
//    `characters` (so Ctrl+w → 'w', not U+0017).
//  * Bare modifier keys and unmodified control characters map to `nil`.

import Foundation

/// `FfiKeyEvent.kind` discriminants. These MUST match the Rust `FfiKeyEvent` kind
/// scheme in `wolf-ffi` (`key_event_from_ffi`) and mirror the variant order of
/// `wolf_core::input::Key`.
private enum Kind {
    static let char: UInt8 = 0
    static let enter: UInt8 = 1
    static let esc: UInt8 = 2
    static let tab: UInt8 = 3
    static let backspace: UInt8 = 4
    static let delete: UInt8 = 5
    static let left: UInt8 = 6
    static let right: UInt8 = 7
    static let up: UInt8 = 8
    static let down: UInt8 = 9
    static let home: UInt8 = 10
    static let end: UInt8 = 11
    static let pageUp: UInt8 = 12
    static let pageDown: UInt8 = 13
    static let function: UInt8 = 14
}

/// Carbon `kVK_*` virtual key codes used for named-key recognition. On macOS the
/// BACKSPACE key is `kVK_Delete` (0x33) and FORWARD-DELETE is `kVK_ForwardDelete`
/// (0x75) — do not transpose them.
private enum VK {
    static let returnKey: UInt16 = 0x24
    static let tab: UInt16 = 0x30
    static let backspace: UInt16 = 0x33  // kVK_Delete (the Backspace key)
    static let escape: UInt16 = 0x35
    static let forwardDelete: UInt16 = 0x75
    static let home: UInt16 = 0x73
    static let end: UInt16 = 0x77
    static let pageUp: UInt16 = 0x74
    static let pageDown: UInt16 = 0x79
    static let leftArrow: UInt16 = 0x7B
    static let rightArrow: UInt16 = 0x7C
    static let downArrow: UInt16 = 0x7D
    static let upArrow: UInt16 = 0x7E
}

/// The DEL control scalar (`0x7F`), the upper bound of the C0 control range.
private let delScalar: UInt32 = 0x7F
/// One past the last C0 control scalar (`0x20` = space, the first printable).
private let firstPrintableScalar: UInt32 = 0x20

/// Pure key translation: AppKit `NSEvent` fields → a generated `FfiKeyEvent?`.
public enum WolfKeyMap {
    /// Translates the data an `NSView.keyDown(_:)` pulls off an `NSEvent` into an
    /// `FfiKeyEvent`, or `nil` when the press produces no input (a bare modifier
    /// key, or an unmodified control character that is not a recognized named
    /// key). See the file header for the Shift contract.
    public static func translate(
        keyCode: UInt16,
        characters: String,
        charactersIgnoringModifiers: String,
        ctrl: Bool,
        shift: Bool,
        alt: Bool,
        meta: Bool
    ) -> FfiKeyEvent? {
        // 1. Named keys first (keyCode is authoritative); shift is preserved.
        if let named = namedKey(keyCode: keyCode) {
            return FfiKeyEvent(
                kind: named.kind,
                ch: 0,
                fnum: named.fnum,
                ctrl: ctrl,
                shift: shift,
                alt: alt,
                meta: meta
            )
        }

        // 2. Printable fallback. When ctrl/alt/meta are held, the unmodified glyph
        //    (lowercased) is authoritative; otherwise the shifted glyph in
        //    `characters` carries the case and `shift` is dropped for the Char.
        let hasComboModifier = ctrl || alt || meta
        let source = hasComboModifier ? charactersIgnoringModifiers.lowercased() : characters

        guard let scalar = source.unicodeScalars.first else {
            return nil  // bare modifier key: no character produced.
        }
        if scalar.value < firstPrintableScalar || scalar.value == delScalar {
            return nil  // an unmodified/unhandled control character: not input.
        }

        return FfiKeyEvent(
            kind: Kind.char,
            ch: scalar.value,
            fnum: 0,
            ctrl: ctrl,
            shift: false,  // never set shift on a Char (the glyph carries case).
            alt: alt,
            meta: meta
        )
    }

    /// A recognized named key: its `FfiKeyEvent.kind` and (for F-keys) its number.
    private struct Named {
        let kind: UInt8
        let fnum: UInt8
    }

    /// Maps a virtual `keyCode` to a named key, or `nil` if it is not one (it may
    /// still be a printable character).
    private static func namedKey(keyCode: UInt16) -> Named? {
        switch keyCode {
        case VK.returnKey: return Named(kind: Kind.enter, fnum: 0)
        case VK.escape: return Named(kind: Kind.esc, fnum: 0)
        case VK.tab: return Named(kind: Kind.tab, fnum: 0)
        case VK.backspace: return Named(kind: Kind.backspace, fnum: 0)
        case VK.forwardDelete: return Named(kind: Kind.delete, fnum: 0)
        case VK.leftArrow: return Named(kind: Kind.left, fnum: 0)
        case VK.rightArrow: return Named(kind: Kind.right, fnum: 0)
        case VK.upArrow: return Named(kind: Kind.up, fnum: 0)
        case VK.downArrow: return Named(kind: Kind.down, fnum: 0)
        case VK.home: return Named(kind: Kind.home, fnum: 0)
        case VK.end: return Named(kind: Kind.end, fnum: 0)
        case VK.pageUp: return Named(kind: Kind.pageUp, fnum: 0)
        case VK.pageDown: return Named(kind: Kind.pageDown, fnum: 0)
        default:
            if let n = functionKeyNumber(keyCode: keyCode) {
                return Named(kind: Kind.function, fnum: n)
            }
            return nil
        }
    }

    /// Maps an `F1`..`F12` virtual key code to its function-key number, or `nil`.
    /// The Carbon `kVK_F*` codes are not contiguous, so this is a direct table.
    private static func functionKeyNumber(keyCode: UInt16) -> UInt8? {
        switch keyCode {
        case 0x7A: return 1  // kVK_F1
        case 0x78: return 2  // kVK_F2
        case 0x63: return 3  // kVK_F3
        case 0x76: return 4  // kVK_F4
        case 0x60: return 5  // kVK_F5
        case 0x61: return 6  // kVK_F6
        case 0x62: return 7  // kVK_F7
        case 0x64: return 8  // kVK_F8
        case 0x65: return 9  // kVK_F9
        case 0x6D: return 10  // kVK_F10
        case 0x67: return 11  // kVK_F11
        case 0x6F: return 12  // kVK_F12
        default: return nil
        }
    }
}
