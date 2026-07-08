//
//  CommandDispatcher.swift
//  The single funnel every Wolf command flows through.
//
//  Both the native menu-item path (`AppDelegate.handleCommandMenuItem` decodes the
//  item's `.tag` into a `CommandKind`) and the nvim-drained path
//  (`WolfWindowController.drainAndDispatchCommands` decodes each `FfiCommand.kind()`)
//  route here, so a command has ONE native implementation regardless of its face.
//

import AppKit
import WolfKeyMap

/// Maps a decoded `CommandKind` to its native AppKit action. A namespace (`enum`
/// with only static members).
enum CommandDispatcher {
    /// Performs `kind`'s native action.
    ///
    /// `window` is the command's target window (the menu path passes
    /// `NSApp.keyWindow`; the nvim-drained path passes the originating controller's
    /// own window, so a command from window A acts on window A). `coordinator`
    /// opens new windows. `originatingCwd` is the originating window's LIVE working
    /// directory, inherited by `.newWindow` so a drained `:WolfNewWindow` / the New
    /// Window menu item root the new window at the same place Cmd-N does (empty
    /// falls back to the home dir in `makeNewWindow`). It is ignored by the other
    /// commands. `controller` is the command's target window controller (the menu
    /// path finds the key window's controller; the nvim-drained path passes the
    /// originating controller itself), through which the file commands reach that
    /// window's picker / core handle; `nil` when no controller owns the window.
    static func dispatch(
        _ kind: CommandKind, window: NSWindow?, coordinator: AppDelegate?, originatingCwd: String,
        controller: WolfWindowController?
    ) {
        switch kind {
        case .newWindow:
            coordinator?.makeNewWindow(originatingCwd: originatingCwd)
        case .closeWindow:
            window?.performClose(nil)
        case .quitApplication:
            NSApp.terminate(nil)
        case .hideApplication:
            NSApp.hide(nil)
        case .minimizeWindow:
            window?.miniaturize(nil)
        case .goToFile:
            controller?.openFilePicker()
        case .openFile:
            controller?.presentOpenFileDialog()
        case .openFolder:
            controller?.presentOpenFolderDialog()
        case .closePane:
            controller?.closePane()
        case .newBuffer:
            controller?.newBuffer()
        case .resetFontSize:
            controller?.resetFontSize()
        case .increaseFontSize:
            controller?.increaseFontSize()
        case .decreaseFontSize:
            controller?.decreaseFontSize()
        case .commandPalette:
            controller?.openCommandPalette()
        case .copy:
            controller?.copy()
        case .paste:
            controller?.paste()
        case .bufferPicker:
            controller?.openBufferPicker()
        }
    }
}
