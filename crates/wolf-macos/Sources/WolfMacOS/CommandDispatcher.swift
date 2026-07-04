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
    /// four commands.
    static func dispatch(
        _ kind: CommandKind, window: NSWindow?, coordinator: AppDelegate?, originatingCwd: String
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
        }
    }
}
