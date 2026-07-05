//
//  AppMenu.swift
//  The native NSMenu app menu bar (net-new; there is no menu bar otherwise).
//
//  Wolf-macos hardcodes its own menu description for this cut (it does NOT
//  read the core registry over FFI yet — only the `command_kind` discriminant
//  crosses the bridge). Every Wolf item carries the FFI `command_kind` u8 in its
//  `NSMenuItem.tag`, targets the `AppDelegate`, and fires
//  `handleCommandMenuItem(_:)`, which decodes the tag and funnels through
//  `CommandDispatcher` — the SAME dispatcher the nvim-drained command path uses.
//

import AppKit

/// Builds and installs `NSApp.mainMenu` once. A namespace (`enum` with only static
/// members).
enum AppMenu {
    /// The FFI `command_kind` discriminants (mirroring `wolf-ffi`'s `mod
    /// command_kind` / `wolf_core::Command` variant order). Held in the item `tag`
    /// so `handleCommandMenuItem` can decode them with `CommandKind(ffi:)`.
    private enum Tag {
        static let newWindow = 0
        static let closeWindow = 1
        static let quitApplication = 2
        static let hideApplication = 3
        static let minimizeWindow = 4
        static let goToFile = 5
        static let openFile = 6
        static let openFolder = 7
        static let closePane = 8
        static let newBuffer = 9
        static let resetFontSize = 10
        static let increaseFontSize = 11
        static let decreaseFontSize = 12
    }

    /// Installs the standard four-menu bar (App / File / View / Window) onto
    /// `NSApp.mainMenu`, wiring every Wolf item to `target`'s
    /// `handleCommandMenuItem(_:)`. Returns the Window submenu `NSMenu` so the
    /// coordinator can rebuild its dynamic per-window list.
    static func install(target: AppDelegate) -> NSMenu {
        let mainMenu = NSMenu()

        mainMenu.addItem(appMenuItem(target: target))
        mainMenu.addItem(fileMenuItem(target: target))
        mainMenu.addItem(viewMenuItem(target: target))
        let windowItem = windowMenuItem(target: target)
        mainMenu.addItem(windowItem)

        NSApp.mainMenu = mainMenu
        // Force-unwrap: `windowMenuItem` always sets a submenu.
        return windowItem.submenu!
    }

    /// The application menu (About / Hide `h` / Quit `q`).
    private static func appMenuItem(target: AppDelegate) -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "Wolf")

        menu.addItem(
            NSMenuItem(
                title: "About Wolf",
                action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
                keyEquivalent: ""))
        menu.addItem(.separator())
        menu.addItem(
            commandItem(
                title: "Hide Wolf", tag: Tag.hideApplication, keyEquivalent: "h", target: target))
        menu.addItem(.separator())
        menu.addItem(
            commandItem(
                title: "Quit Wolf", tag: Tag.quitApplication, keyEquivalent: "q", target: target))

        item.submenu = menu
        return item
    }

    /// The File menu (New `⌘n` / New Window `⇧⌘n` / Go to File `⌘o` / Open File /
    /// Open Folder `⇧⌘o` / Close `⌘w` / Close Window `⇧⌘w`).
    ///
    /// The item titles mirror `wolf_core::COMMAND_SPECS` (the canonical registry);
    /// the ellipsis characters are single U+2026, matching those specs.
    private static func fileMenuItem(target: AppDelegate) -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "File")

        menu.addItem(
            commandItem(
                title: "New", tag: Tag.newBuffer, keyEquivalent: "n", target: target))
        menu.addItem(
            commandItem(
                title: "New Window", tag: Tag.newWindow, keyEquivalent: "n",
                target: target, modifiers: [.command, .shift]))
        menu.addItem(.separator())
        menu.addItem(
            commandItem(
                title: "Go to File…", tag: Tag.goToFile, keyEquivalent: "o", target: target))
        menu.addItem(
            commandItem(
                title: "Open File…", tag: Tag.openFile, keyEquivalent: "", target: target))
        menu.addItem(
            commandItem(
                title: "Open Folder…", tag: Tag.openFolder, keyEquivalent: "o",
                target: target, modifiers: [.command, .shift]))
        menu.addItem(.separator())
        menu.addItem(
            commandItem(
                title: "Close", tag: Tag.closePane, keyEquivalent: "w", target: target))
        menu.addItem(
            commandItem(
                title: "Close Window", tag: Tag.closeWindow, keyEquivalent: "w",
                target: target, modifiers: [.command, .shift]))

        item.submenu = menu
        return item
    }

    /// The View menu (Reset Font Size `⌘0` / Increase Font Size `⌘=` / Decrease
    /// Font Size `⌘-`). Titles mirror `wolf_core::COMMAND_SPECS`.
    private static func viewMenuItem(target: AppDelegate) -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "View")

        menu.addItem(
            commandItem(
                title: "Reset Font Size", tag: Tag.resetFontSize, keyEquivalent: "0",
                target: target))
        menu.addItem(
            commandItem(
                title: "Increase Font Size", tag: Tag.increaseFontSize, keyEquivalent: "=",
                target: target))
        menu.addItem(
            commandItem(
                title: "Decrease Font Size", tag: Tag.decreaseFontSize, keyEquivalent: "-",
                target: target))

        item.submenu = menu
        return item
    }

    /// The Window menu (Minimize `m`, a separator, then the dynamic per-window
    /// list built by `rebuildWindowMenu`).
    private static func windowMenuItem(target: AppDelegate) -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "Window")

        menu.addItem(
            commandItem(
                title: "Minimize", tag: Tag.minimizeWindow, keyEquivalent: "m", target: target))

        item.submenu = menu
        return item
    }

    /// Rebuilds the dynamic Window `menu`: Minimize (`⌘m`), a separator, then one
    /// item per open window titled with that window's full cwd path (fallback
    /// "Wolf"). Each dynamic item's action raises its window
    /// (`AppDelegate.raiseWindowMenuItem(_:)`), carrying the window's
    /// `windowIndex` in its `tag`. Called by the coordinator on window
    /// open/close/cwd-change.
    static func rebuildWindowMenu(
        _ menu: NSMenu, controllers: [WolfWindowController], target: AppDelegate
    ) {
        menu.removeAllItems()
        menu.addItem(
            commandItem(
                title: "Minimize", tag: Tag.minimizeWindow, keyEquivalent: "m", target: target))
        menu.addItem(.separator())
        for controller in controllers {
            let cwd = controller.currentCwd
            let windowItem = NSMenuItem(
                title: cwd.isEmpty ? "Wolf" : cwd,
                action: #selector(AppDelegate.raiseWindowMenuItem(_:)),
                keyEquivalent: "")
            windowItem.target = target
            windowItem.tag = controller.windowIndex
            menu.addItem(windowItem)
        }
    }

    /// Builds a Wolf command menu item: its `tag` carries the FFI discriminant, its
    /// action funnels through `handleCommandMenuItem(_:)`.
    private static func commandItem(
        title: String, tag: Int, keyEquivalent: String, target: AppDelegate,
        modifiers: NSEvent.ModifierFlags = .command
    ) -> NSMenuItem {
        let item = NSMenuItem(
            title: title,
            action: #selector(AppDelegate.handleCommandMenuItem(_:)),
            keyEquivalent: keyEquivalent)
        item.keyEquivalentModifierMask = modifiers
        item.tag = tag
        item.target = target
        return item
    }
}
