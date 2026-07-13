//
//  AppearanceSeam.swift
//  Pure, AppKit-free appearance-mode decode seam.
//
//  The sibling of `CommandSeam`'s `CommandKind(ffi:)` decode: a pure, display-free
//  mapping exercised with scalar inputs only — NO AppKit, NO NSApp, NO
//  NSAppearance, NO live Neovim. It mirrors the FFI appearance discriminant that
//  `FfiConfig::appearance() -> u8` delivers, which `AppDelegate` decodes to resolve
//  dark/light.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap` module
//  alongside `CommandSeam.swift` / `PaneTree.swift`.
//

import Foundation

/// The user's configured appearance mode, decoded from the FFI discriminant.
///
/// A `UInt8` raw-value enum so the raw values ARE the contract shared with
/// `wolf-ffi`'s `mod appearance` and `wolf_core::config::Appearance` (plan OD-a):
/// `auto == 0`, `dark == 1`, `light == 2`. `AppDelegate` reads
/// `FfiConfig.appearance()` (a `u8`) and maps it via `AppearanceMode(rawValue:)`;
/// an out-of-range discriminant decodes to `nil`.
public enum AppearanceMode: UInt8 {
    /// Follow the macOS system appearance, switching live when the OS changes.
    case auto = 0
    /// Force dark mode regardless of the OS setting.
    case dark = 1
    /// Force light mode regardless of the OS setting.
    case light = 2
}
