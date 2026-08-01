# Plan: View-menu entries to show Project Tree (⇧⌘E) and Search (⇧⌘F)

## Status: COMPLETE ✓ (uncommitted — no commit requested)

Two View-menu entries added below Toggle Sidebar: **Show Project Tree** (⇧⌘E) and **Show Search** (⇧⌘F). Checkmark tracks the visible panel (none when hidden); choosing either while hidden reveals the sidebar in that panel. Pure AppKit (no Rust). Reviewer APPROVED (all 4 correctness questions clean: checkmark logic, no false/missing checks, hidden→reveal ordering safe with setMode early-return, multi-window targeting). Docs synced (both macOS READMEs, count 36→37, prettier-clean). Green under bazelisk: app builds+signs, `WolfKeyMapTests` (incl. `SidebarMenuSeamTests`) PASS, `swift_format` PASS. Left uncommitted for review.

## Feature request
In the View menu, below "Toggle Sidebar" (⌃⌘S) and in the SAME section (above the separator), add two entries:
- **Show Project Tree** — key equivalent ⇧⌘E — reveals the sidebar showing the file tree.
- **Show Search** — key equivalent ⇧⌘F — reveals the sidebar showing the search UI.

Checkmark behavior (the crux):
- The currently-VISIBLE sidebar panel's entry has a checkmark (at most one).
- If the sidebar is HIDDEN, NEITHER new entry is checked.
- If the sidebar is hidden, choosing EITHER entry shows the sidebar (in that panel).

## Design (pure AppKit — NO Rust/CommandKind/FFI changes)
These are view-state actions (reveal sidebar + switch panel), like `raiseWindowMenuItem`/`openRecentMenuItem` — AppDelegate `@objc` selectors, NOT the Tag→CommandKind→Rust pipeline. Confirmed anchors below.

### New headless seam — `crates/wolf-macos/Sources/WolfKeyMap/SidebarMenuSeam.swift` (Foundation only, tested)
```swift
public enum SidebarPanel: Equatable { case tree, search }
public enum SidebarMenuSeam {
  /// A panel's menu entry is checked iff the sidebar is visible AND that panel is the active one.
  /// Hidden sidebar ⇒ `sidebarVisible == false` ⇒ never checked (regardless of `current`).
  public static func isChecked(sidebarVisible: Bool, current: SidebarPanel?, item: SidebarPanel) -> Bool {
    sidebarVisible && current == item
  }
}
```
This directly encodes the three spec bullets. Add the file to `wolf_key_map` `srcs` in `crates/wolf-macos/BUILD.bazel`.

### `crates/wolf-macos/Sources/WolfMacOS/AppMenu.swift` — `viewMenuItem(target:)` (~L173-186)
After `menu.addItem(toggleSidebar)` (L183) and BEFORE the separator, insert two items TARGETING the AppDelegate:
```swift
let showTree = NSMenuItem(title: "Show Project Tree",
  action: #selector(AppDelegate.showProjectTreeMenuItem(_:)), keyEquivalent: "e")
showTree.keyEquivalentModifierMask = [.command, .shift]
showTree.target = target
menu.addItem(showTree)

let showSearch = NSMenuItem(title: "Show Search",
  action: #selector(AppDelegate.showSearchMenuItem(_:)), keyEquivalent: "f")
showSearch.keyEquivalentModifierMask = [.command, .shift]
showSearch.target = target
menu.addItem(showSearch)
```
(Then the existing separator + Reset Font Size etc.) Update the `viewMenuItem` doc comment (L161-172) to mention the two new items. ⇧⌘E is free (⌘E = Buffer Picker, no shift). ⇧⌘F is free (Full Screen is ⌃⌘F).

### `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift`
Two selectors (mirror `raiseWindowMenuItem` @objc pattern, ~L490):
```swift
@objc func showProjectTreeMenuItem(_ sender: NSMenuItem) {
  controllers.first { $0.owns(effectiveKeyWindow) }?.showSidebar(panel: .tree)
}
@objc func showSearchMenuItem(_ sender: NSMenuItem) {
  controllers.first { $0.owns(effectiveKeyWindow) }?.showSidebar(panel: .search)
}
```
In `validateMenuItem(_:)` (L639), handle the two BEFORE the existing `guard item.action == #selector(handleCommandMenuItem(_:))` (L640) — because that guard `return true`s (without setting state) for any other action:
```swift
if item.action == #selector(showProjectTreeMenuItem(_:)) || item.action == #selector(showSearchMenuItem(_:)) {
  let c = controllers.first { $0.owns(effectiveKeyWindow) }
  let panel: SidebarPanel = item.action == #selector(showProjectTreeMenuItem(_:)) ? .tree : .search
  item.state = SidebarMenuSeam.isChecked(sidebarVisible: c?.sidebarVisible ?? false, current: c?.sidebarPanel, item: panel) ? .on : .off
  return true  // always enabled — choosing when hidden reveals the sidebar
}
```
(`effectiveKeyWindow` + `owns(_:)` already used at L642. `keyController` local pattern reused.)

### `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift`
Expose current panel + a reveal-and-switch method (`sidebarContainer` is `private var` at L118; `sidebarVisible` getter at L687 = `!window.sidebarItem.isCollapsed`; reveal precedent `window.sidebarItem.isCollapsed = false` at L454):
```swift
/// The active sidebar panel, or nil when there is no sidebar container yet.
var sidebarPanel: SidebarPanel? {
  guard let c = sidebarContainer else { return nil }
  switch c.mode { case .tree: return .tree; case .search: return .search }
}
/// Reveal the sidebar (if collapsed) and switch it to `panel`.
func showSidebar(panel: SidebarPanel) {
  window?.sidebarItem.isCollapsed = false   // KVO relays setSidebarVisible(true)
  sidebarContainer?.setMode(panel == .tree ? .tree : .search)
}
```
(`SidebarContainerView.Mode` is AppKit-nested; map to/from the Foundation `SidebarPanel`. `setMode` already calls `setSidebarVisible(isSidebarVisible)` internally so the newly-shown child refreshes.)

## Test plan
- **Headless Swift seam test** `crates/wolf-macos/Tests/WolfKeyMapTests/SidebarMenuSeamTests.swift` (Foundation only) — the isChecked truth table = the three spec bullets:
  - visible + current==item ⇒ true (tree/tree, search/search).
  - visible + current!=item ⇒ false (tree/search, search/tree).
  - hidden (sidebarVisible=false) ⇒ false for every (current,item), incl. current=nil.
  - current=nil + visible ⇒ false (defensive).
  Wire the file into `WolfKeyMapTests` `srcs` in `crates/wolf-macos/BUILD.bazel`.
- The AppKit menu wiring (AppMenu items, AppDelegate selectors, WolfWindowController.showSidebar/sidebarPanel) is compile-verified + manual-smoke (needs a window/menu server), consistent with the rest of the macOS UI.

## Verification
- `bazelisk build //crates/wolf-macos:Wolf` (compiles + signs).
- `bazelisk test //crates/wolf-macos:WolfKeyMapTests` (fresh; incl. new SidebarMenuSeamTests) + `//tools/lint:swift_format`. (No Rust changed; rustfmt/clippy unaffected but harmless to run.)

## Docs to update (technical-writer)
- `crates/wolf-macos/Sources/README.md` + `crates/wolf-macos/README.md`: document `SidebarMenuSeam` (+ `SidebarPanel`); bump AppKit-free WolfKeyMap file count 36→37; update the AppMenu/View-menu prose (two new items + shortcuts + checkmark rule), AppDelegate (two selectors + validateMenuItem checkmark branch), and WolfWindowController (`showSidebar(panel:)`/`sidebarPanel`). Run `prettier --write` on edited Markdown.

## Test ownership split
- test-writer owns `SidebarMenuSeamTests.swift` + its BUILD `srcs` wiring.
- coder owns all production code (seam, AppMenu, AppDelegate, WolfWindowController) + adding the seam source to `wolf_key_map` `srcs`, builds, runs tests + linters.

## Progress log
- **test-writer** (done): `SidebarMenuSeamTests.swift` (4 tests = the 3 spec bullets + defensive nil), wired into `WolfKeyMapTests` srcs.
- **coder** (done, all green): new `SidebarMenuSeam.swift` (+`SidebarPanel`, in `wolf_key_map` srcs); AppMenu "Show Project Tree" ⇧⌘E + "Show Search" ⇧⌘F (AppDelegate-targeted, before the View separator); AppDelegate two `@objc` selectors + `validateMenuItem` checkmark branch (before the handleCommand guard) via `SidebarMenuSeam.isChecked`; WolfWindowController `sidebarPanel`/`showSidebar(panel:)`. Verified fresh: `//crates/wolf-macos:Wolf` builds+signs, `WolfKeyMapTests` PASS (incl. new suite), `swift_format` PASS. No Rust changed. No plan deviations. Not committed.
