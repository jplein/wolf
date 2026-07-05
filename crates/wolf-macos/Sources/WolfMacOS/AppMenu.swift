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
        static let closeFile = 8
    }

    /// Installs the standard three-menu bar (App / File / Window) onto
    /// `NSApp.mainMenu`, wiring every Wolf item to `target`'s
    /// `handleCommandMenuItem(_:)`.
    static func install(target: AppDelegate) {
        let mainMenu = NSMenu()

        mainMenu.addItem(appMenuItem(target: target))
        mainMenu.addItem(fileMenuItem(target: target))
        mainMenu.addItem(windowMenuItem(target: target))

        NSApp.mainMenu = mainMenu
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

    /// The File menu (New Window `n` / Go to File `o` / Open File / Open Folder
    /// `⇧⌘o` / Close File `w` / Close Window `⇧⌘w`).
    ///
    /// The item titles mirror `wolf_core::COMMAND_SPECS` (the canonical registry);
    /// the ellipsis characters are single U+2026, matching those specs.
    private static func fileMenuItem(target: AppDelegate) -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "File")

        menu.addItem(
            commandItem(
                title: "New Window", tag: Tag.newWindow, keyEquivalent: "n", target: target))
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
                title: "Close File", tag: Tag.closeFile, keyEquivalent: "w", target: target))
        menu.addItem(
            commandItem(
                title: "Close Window", tag: Tag.closeWindow, keyEquivalent: "w",
                target: target, modifiers: [.command, .shift]))

        item.submenu = menu
        return item
    }

    /// The Window menu (Minimize `m`).
    private static func windowMenuItem(target: AppDelegate) -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu(title: "Window")

        menu.addItem(
            commandItem(
                title: "Minimize", tag: Tag.minimizeWindow, keyEquivalent: "m", target: target))

        item.submenu = menu
        return item
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
