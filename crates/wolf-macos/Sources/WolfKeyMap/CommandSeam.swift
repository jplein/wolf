//
//  CommandSeam.swift
//  Pure, AppKit-free command-kind decode seam.
//
//  The sibling of `PaneTree`'s `Orientation(ffi:)` / `FocusDir(ffi:)` seams: a
//  pure, display-free decode exercised with scalar inputs only — NO AppKit, NO
//  NSMenu, NO live Neovim. It decodes the FFI command-kind `u8` (delivered in an
//  `NSMenuItem.tag` or by a drained `FfiCommand`) into a typed `CommandKind`.
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap` module
//  alongside `PaneTree.swift` / `ShortcutSeam.swift`.
//

import Foundation

/// A no-parameter, no-return user action, decoded from the FFI discriminant.
///
/// The cases align with `wolf-ffi`'s `mod command_kind` and the `wolf_core::Command`
/// variant order: `newWindow == 0`, `closeWindow == 1`, `quitApplication == 2`,
/// `hideApplication == 3`, `minimizeWindow == 4`, `goToFile == 5`, `openFile == 6`,
/// `openFolder == 7`, `closeFile == 8`.
public enum CommandKind {
    /// Open a new editor window.
    case newWindow
    /// Close the focused window.
    case closeWindow
    /// Quit the whole application.
    case quitApplication
    /// Hide the application.
    case hideApplication
    /// Minimize the focused window.
    case minimizeWindow
    /// Open the file-picker overlay to jump to a file.
    case goToFile
    /// Open a native dialog to open a file anywhere.
    case openFile
    /// Open a native dialog to change the working directory to a folder.
    case openFolder
    /// Close (delete) the current buffer.
    case closeFile

    /// Maps an FFI command-kind discriminant (`0` → `.newWindow` .. `8` →
    /// `.closeFile`, else `nil`), guarding the contract shared with
    /// `wolf-ffi`'s `mod command_kind`.
    public init?(ffi: UInt8) {
        switch ffi {
        case 0: self = .newWindow
        case 1: self = .closeWindow
        case 2: self = .quitApplication
        case 3: self = .hideApplication
        case 4: self = .minimizeWindow
        case 5: self = .goToFile
        case 6: self = .openFile
        case 7: self = .openFolder
        case 8: self = .closeFile
        default: return nil
        }
    }
}
