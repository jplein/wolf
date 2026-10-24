// Pure, AppKit-free decode of the FFI appearance-mode discriminant.

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
