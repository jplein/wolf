// Pure, AppKit-free decode of the FFI command-kind discriminant.

import Foundation

/// A no-parameter, no-return user action, decoded from the FFI discriminant.
///
/// Each case is one `wolf_core::Command`, named the lowerCamelCase of its registry id
/// (`go_to_file` is `goToFile`). The arms of `init?(ffi:)` are the Swift side's record of
/// each discriminant, which `CommandSeamTests` checks over the FFI against the registry's
/// count and `wolf-ffi`'s `mod command_kind`.
public enum CommandKind: CaseIterable {
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
  /// Toggle the terminal drawer.
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
  /// Split the active pane into two stacked panes, as Vim's `:split`. The new pane is of
  /// the active pane's kind; the terminal drawer refuses it.
  case splitHorizontal
  /// Split the active pane into two side-by-side panes, as Vim's `:vsplit`. The new pane
  /// is of the active pane's kind; the terminal drawer refuses it.
  case splitVertical
  /// Toggle the active pane, the drawer included, between zoomed over the whole pane area
  /// and not; front-end geometry that never reaches Neovim.
  case zoomPane
  /// Close Tab: close the tab, and its window when that is the window's only tab, once the
  /// tab's Neovim finds nothing unsaved. It is also what the `:q` and `:qa` families send.
  case closeTab

  /// Maps an FFI command-kind discriminant to its case, or `nil` for any value no case
  /// holds, guarding the contract shared with `wolf-ffi`'s `mod command_kind`.
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
    case 28: self = .splitHorizontal
    case 29: self = .splitVertical
    case 30: self = .zoomPane
    case 31: self = .closeTab
    default: return nil
    }
  }
}
