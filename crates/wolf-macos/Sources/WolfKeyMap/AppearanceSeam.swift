// Pure, AppKit-free decode of the FFI appearance-mode discriminant.

import Foundation

/// The user's configured appearance mode, decoded from the FFI discriminant.
///
/// A `UInt8` raw-value enum so the raw values ARE the contract shared with
/// `wolf-ffi`'s `mod appearance` and `wolf_core::config::Appearance`:
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

  /// The appearance this mode forces on the whole app (`true` == dark), which
  /// every window and its title inherit; `nil` for `auto`, which follows the OS.
  ///
  /// The window title is drawn by AppKit in the title colours of this resolved
  /// appearance, not the OS's: an OS in light mode under `appearance: dark` must
  /// not draw the light-mode title, which is nearly invisible on Wolf's dark
  /// titlebar. The colorscheme never chooses it.
  public var forcedDark: Bool? {
    switch self {
    case .auto: return nil
    case .dark: return true
    case .light: return false
    }
  }

  /// The resolved appearance (`true` == dark) given the OS's: the forced one, or
  /// `systemDark` under `auto`. Defined as `forcedDark ?? systemDark`, so the
  /// appearance pushed to Neovim and the one every title draws in cannot diverge.
  public func resolvesDark(systemDark: Bool) -> Bool {
    forcedDark ?? systemDark
  }
}
