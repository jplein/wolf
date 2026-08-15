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
/// `openFolder == 7`, `closePane == 8`, `newBuffer == 9`, `resetFontSize == 10`,
/// `increaseFontSize == 11`, `decreaseFontSize == 12`, `commandPalette == 13`,
/// `copy == 14`, `paste == 15`, `bufferPicker == 16`, `newTab == 17`,
/// `preview == 18`, `edit == 19`, `terminal == 20`, `previewInBrowser == 21`,
/// `fullWidth == 22`, `toggleSidebar == 23`, `sidebarShowProject == 24`,
/// `searchInProject == 25`, `previewInOtherPane == 26`, `editInOtherPane == 27`.
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
  /// Close the current split (the pane), not the buffer.
  case closePane
  /// Open a new empty buffer in the current window.
  case newBuffer
  /// Reset the font size to the configured default.
  case resetFontSize
  /// Increase the font size by one point.
  case increaseFontSize
  /// Decrease the font size by one point.
  case decreaseFontSize
  /// Open the command-palette overlay.
  case commandPalette
  /// Copy the visual-mode selection to the system clipboard.
  case copy
  /// Paste from the system clipboard into the active document.
  case paste
  /// Open the buffer-picker overlay to fuzzy-switch among open buffers.
  case bufferPicker
  /// Open a new tab in the current window.
  case newTab
  /// Render the active Markdown buffer as HTML in a preview pane.
  case preview
  /// Navigate from a preview pane back to its source file (inverse of preview).
  case edit
  /// Toggle the per-project terminal drawer.
  case terminal
  /// Render the active Markdown buffer and open it in the system web browser.
  case previewInBrowser
  /// Toggle the active buffer between the ~90-char default content width and full
  /// pane width (`b:wolf_full_width`).
  case fullWidth
  /// Toggle the sidebar (collapse/expand).
  case toggleSidebar
  /// Reveal the sidebar showing the file/project tree.
  case sidebarShowProject
  /// Reveal the sidebar showing the search UI (and focus the search field).
  case searchInProject
  /// Preview the active pane's Markdown file in another pane (reuse or split
  /// right).
  case previewInOtherPane
  /// Edit the active preview pane's source file in another pane (reuse or
  /// split, source lands left).
  case editInOtherPane

  /// Maps an FFI command-kind discriminant (`0` → `.newWindow` .. `27` →
  /// `.editInOtherPane`, else `nil`), guarding the contract shared with
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
    case 8: self = .closePane
    case 9: self = .newBuffer
    case 10: self = .resetFontSize
    case 11: self = .increaseFontSize
    case 12: self = .decreaseFontSize
    case 13: self = .commandPalette
    case 14: self = .copy
    case 15: self = .paste
    case 16: self = .bufferPicker
    case 17: self = .newTab
    case 18: self = .preview
    case 19: self = .edit
    case 20: self = .terminal
    case 21: self = .previewInBrowser
    case 22: self = .fullWidth
    case 23: self = .toggleSidebar
    case 24: self = .sidebarShowProject
    case 25: self = .searchInProject
    case 26: self = .previewInOtherPane
    case 27: self = .editInOtherPane
    default: return nil
    }
  }
}
