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
import WolfKeyMap

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
    static let commandPalette = 13
    static let copy = 14
    static let paste = 15
    static let bufferPicker = 16
    static let newTab = 17
    static let preview = 18
    static let edit = 19
    static let terminal = 20
    static let previewInBrowser = 21
    static let fullWidth = 22
  }

  /// The dynamic submenus `install` hands back to the coordinator: the Window
  /// submenu (rebuilt with the per-window list) and the Open Recent submenu
  /// (rebuilt lazily via `NSMenuDelegate.menuNeedsUpdate` from the recent-dirs
  /// MRU).
  struct InstalledMenus {
    let window: NSMenu
    let openRecent: NSMenu
    /// The dynamic History menu, rebuilt from core on every `menuNeedsUpdate`.
    let history: NSMenu
  }

  /// Installs the standard six-menu bar (App / File / Edit / View / History /
  /// Window) onto `NSApp.mainMenu`, wiring every Wolf item to `target`'s
  /// `handleCommandMenuItem(_:)`. Returns the Window, Open Recent and History
  /// submenus so the coordinator can rebuild their dynamic lists.
  static func install(target: AppDelegate) -> InstalledMenus {
    let mainMenu = NSMenu()

    mainMenu.addItem(appMenuItem(target: target))
    let openRecent = NSMenu(title: "Open Recent")
    mainMenu.addItem(fileMenuItem(target: target, openRecent: openRecent))
    mainMenu.addItem(editMenuItem(target: target))
    mainMenu.addItem(viewMenuItem(target: target))
    // History sits BETWEEN View and Window, the conventional place for a
    // navigation menu.
    let historyItem = historyMenuItem(target: target)
    mainMenu.addItem(historyItem)
    let windowItem = windowMenuItem(target: target)
    mainMenu.addItem(windowItem)

    NSApp.mainMenu = mainMenu
    // Force-unwrap: `windowMenuItem` / `historyMenuItem` always set a submenu.
    return InstalledMenus(
      window: windowItem.submenu!, openRecent: openRecent, history: historyItem.submenu!)
  }

  /// The History menu: Back `⌘[` / Forward `⌘]` / separator / the active pane's
  /// recently-visited rows (rebuilt at menu-open time by
  /// ``rebuildHistoryMenu(_:rows:target:)``), seeded with the disabled placeholder
  /// so it is never empty before the first rebuild.
  ///
  /// These are FRONT-END-ONLY items, like Toggle Sidebar: no `wolf_core::Command`
  /// variant exists for them, so they do NOT route through
  /// `handleCommandMenuItem` / `CommandDispatcher`.
  private static func historyMenuItem(target: AppDelegate) -> NSMenuItem {
    let item = NSMenuItem()
    let menu = NSMenu(title: "History")
    addHistoryFixedItems(to: menu, target: target)
    menu.addItem(historyPlaceholderItem())
    item.submenu = menu
    return item
  }

  /// Adds the History menu's FIXED head — Back, Forward, separator — to `menu`.
  /// Shared by the initial build and every rebuild so the two never drift.
  private static func addHistoryFixedItems(to menu: NSMenu, target: AppDelegate) {
    let back = NSMenuItem(
      title: "Back",
      action: #selector(AppDelegate.historyBackMenuItem(_:)),
      keyEquivalent: "[")
    back.keyEquivalentModifierMask = .command
    back.target = target
    menu.addItem(back)

    let forward = NSMenuItem(
      title: "Forward",
      action: #selector(AppDelegate.historyForwardMenuItem(_:)),
      keyEquivalent: "]")
    forward.keyEquivalentModifierMask = .command
    forward.target = target
    menu.addItem(forward)

    menu.addItem(.separator())
  }

  /// The disabled "No History" placeholder shown when the active pane has no
  /// history (standard macOS empty-state; `action: nil` auto-disables).
  private static func historyPlaceholderItem() -> NSMenuItem {
    let item = NSMenuItem(
      title: HistorySeam.emptyPlaceholderTitle, action: nil, keyEquivalent: "")
    item.isEnabled = false
    return item
  }

  /// Rebuilds the dynamic History `menu` from the active pane's `rows`
  /// (NEWEST-FIRST, already capped at 10 distinct rows and disambiguated by core).
  /// Empty -> a single disabled placeholder; otherwise one clickable item per row,
  /// checkmarked on the row core marks `isCurrent`. Called from `menuNeedsUpdate`
  /// at menu-open time.
  ///
  /// NOTE: a row item's `tag` is a STACK INDEX, **not** a `command_kind`
  /// discriminant like every `commandItem` tag in this file. The distinct
  /// `historyJumpMenuItem(_:)` selector is what keeps the two unambiguous.
  static func rebuildHistoryMenu(
    _ menu: NSMenu, rows: [HistorySeam.Row], target: AppDelegate
  ) {
    menu.removeAllItems()
    addHistoryFixedItems(to: menu, target: target)
    if rows.isEmpty {
      menu.addItem(historyPlaceholderItem())
      return
    }
    for row in rows {
      let item = NSMenuItem(
        title: row.title,
        action: #selector(AppDelegate.historyJumpMenuItem(_:)),
        keyEquivalent: "")
      item.target = target
      item.tag = row.index
      item.state = row.isCurrent ? .on : .off
      menu.addItem(item)
    }
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

  /// The File menu (New `⌘n` / New Window `⇧⌘n` / New Tab `⌘t` / Go to File `⌘o`
  /// / Open File / Open Folder `⇧⌘o` / Close `⌘w` / Close Window `⇧⌘w`).
  ///
  /// The item titles mirror `wolf_core::COMMAND_SPECS` (the canonical registry);
  /// the ellipsis characters are single U+2026, matching those specs.
  private static func fileMenuItem(target: AppDelegate, openRecent: NSMenu) -> NSMenuItem {
    let item = NSMenuItem()
    let menu = NSMenu(title: "File")

    menu.addItem(
      commandItem(
        title: "New", tag: Tag.newBuffer, keyEquivalent: "n", target: target))
    menu.addItem(
      commandItem(
        title: "New Window", tag: Tag.newWindow, keyEquivalent: "n",
        target: target, modifiers: [.command, .shift]))
    menu.addItem(
      commandItem(
        title: "New Tab", tag: Tag.newTab, keyEquivalent: "t", target: target))
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
    menu.addItem(openRecentItem(submenu: openRecent))
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

  /// The Edit menu (Copy `⌘c` / Paste `⌘v`). Both titles mirror
  /// `wolf_core::COMMAND_SPECS`. Copy is enabled only in visual mode (gated by
  /// `AppDelegate.validateMenuItem`); Paste is always enabled.
  private static func editMenuItem(target: AppDelegate) -> NSMenuItem {
    let item = NSMenuItem()
    let menu = NSMenu(title: "Edit")

    menu.addItem(
      commandItem(
        title: "Copy", tag: Tag.copy, keyEquivalent: "c", target: target))
    menu.addItem(
      commandItem(
        title: "Paste", tag: Tag.paste, keyEquivalent: "v", target: target))

    item.submenu = menu
    return item
  }

  /// The View menu (Toggle Sidebar `⌃⌘s` / a separator / Reset Font Size `⌘0` /
  /// Increase Font Size `⌘=` / Decrease Font Size `⌘-` / a separator / Command
  /// Palette… `⌘p` / Buffer Picker… `⌘e` / Preview / Edit Source / Terminal
  /// `` ⌃` `` / a separator / Enter Full Screen `⌃⌘f`). The font-size, palette,
  /// preview and terminal titles mirror `wolf_core::COMMAND_SPECS`.
  ///
  /// Toggle Sidebar and Enter Full Screen are STANDARD AppKit items (NOT Wolf
  /// commands / registry entries): their actions are
  /// `NSSplitViewController.toggleSidebar(_:)` and `NSWindow.toggleFullScreen(_:)`,
  /// routed via a `nil` target up the responder chain (to the split VC / key
  /// window), and AppKit owns their validation. This is net-new (no such items
  /// existed before); see the READMEs.
  private static func viewMenuItem(target: AppDelegate) -> NSMenuItem {
    let item = NSMenuItem()
    let menu = NSMenu(title: "View")

    let toggleSidebar = NSMenuItem(
      title: "Toggle Sidebar",
      action: #selector(NSSplitViewController.toggleSidebar(_:)),
      keyEquivalent: "s")
    toggleSidebar.keyEquivalentModifierMask = [.control, .command]
    toggleSidebar.target = nil
    menu.addItem(toggleSidebar)
    menu.addItem(.separator())

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
    menu.addItem(.separator())
    menu.addItem(
      commandItem(
        title: "Command Palette…", tag: Tag.commandPalette, keyEquivalent: "p",
        target: target))
    menu.addItem(
      commandItem(
        title: "Buffer Picker…", tag: Tag.bufferPicker, keyEquivalent: "e",
        target: target))
    // View > Preview: enabled only on a Markdown buffer (gated in
    // AppDelegate.validateMenuItem). NO key equivalent this cut (OD2 — ⌘P is
    // the Command Palette; a shortcut can be assigned later).
    menu.addItem(
      commandItem(
        title: "Preview", tag: Tag.preview, keyEquivalent: "", target: target))
    // View > Edit Source: the inverse of Preview, enabled only from a preview
    // pane (gated in AppDelegate.validateMenuItem). NO key equivalent.
    menu.addItem(
      commandItem(
        title: "Edit Source", tag: Tag.edit, keyEquivalent: "", target: target))
    // View > Terminal: toggles the per-project terminal drawer. Always enabled
    // (unlike Preview / Edit Source it is not gated in
    // AppDelegate.validateMenuItem).
    menu.addItem(
      commandItem(
        title: "Terminal", tag: Tag.terminal, keyEquivalent: "`",
        target: target, modifiers: .control))
    // View > Preview in Browser: renders the active Markdown buffer and opens it
    // in the system default browser. Enabled only on a Markdown buffer (gated in
    // AppDelegate.validateMenuItem, mirroring Preview). NO key equivalent.
    menu.addItem(
      commandItem(
        title: "Preview in Browser", tag: Tag.previewInBrowser, keyEquivalent: "",
        target: target))
    // View > Use full width for content: toggles the active buffer between the
    // ~90-char default content width and full pane width. ALWAYS enabled (OQ-2);
    // its checkmark tracks isFullWidthActive (gated in AppDelegate.validateMenuItem).
    // NO key equivalent.
    menu.addItem(
      commandItem(
        title: "Use full width for content", tag: Tag.fullWidth, keyEquivalent: "",
        target: target))
    menu.addItem(.separator())
    let fullScreen = NSMenuItem(
      title: "Enter Full Screen",
      action: #selector(NSWindow.toggleFullScreen(_:)),
      keyEquivalent: "f")
    fullScreen.keyEquivalentModifierMask = [.control, .command]
    fullScreen.target = nil
    menu.addItem(fullScreen)

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
      let cwd = controller.persistedCwd
      let windowItem = NSMenuItem(
        title: cwd.isEmpty ? "Wolf" : cwd,
        action: #selector(AppDelegate.raiseWindowMenuItem(_:)),
        keyEquivalent: "")
      windowItem.target = target
      windowItem.tag = controller.tabId
      menu.addItem(windowItem)
    }
  }

  /// The File > Open Recent parent item: a plain (no-action) item carrying the
  /// dynamic `submenu`, seeded with the disabled placeholder so it is never
  /// empty before the first `menuNeedsUpdate` rebuild.
  private static func openRecentItem(submenu: NSMenu) -> NSMenuItem {
    let item = NSMenuItem(title: "Open Recent", action: nil, keyEquivalent: "")
    submenu.addItem(placeholderItem())
    item.submenu = submenu
    return item
  }

  /// The disabled "No Recent Folders" placeholder shown when the recent-dirs
  /// MRU is empty (standard macOS empty-state; `action: nil` auto-disables).
  private static func placeholderItem() -> NSMenuItem {
    let item = NSMenuItem(title: "No Recent Folders", action: nil, keyEquivalent: "")
    item.isEnabled = false
    return item
  }

  /// Rebuilds the dynamic Open Recent `menu` from the recent-directories MRU
  /// (newest-first). Empty -> a single disabled "No Recent Folders" placeholder;
  /// otherwise one item per path, titled with the full path, whose action
  /// (`AppDelegate.openRecentMenuItem(_:)`) chdirs the key window, carrying the
  /// list index in its `tag`. Called from `menuNeedsUpdate` at menu-open time.
  static func rebuildOpenRecentMenu(
    _ menu: NSMenu, recentDirs: [String], target: AppDelegate
  ) {
    menu.removeAllItems()
    if recentDirs.isEmpty {
      menu.addItem(placeholderItem())
      return
    }
    for (index, path) in recentDirs.enumerated() {
      let item = NSMenuItem(
        title: path,
        action: #selector(AppDelegate.openRecentMenuItem(_:)),
        keyEquivalent: "")
      item.target = target
      item.tag = index
      menu.addItem(item)
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
