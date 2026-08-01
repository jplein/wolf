# Plan: Rename sidebar menu items + focus search field on activation

## Status: COMPLETE ✓ (uncommitted — no commit requested)

Menu items renamed "Show Project Tree"→**Project** / "Show Search"→**Search** (⇧⌘E/⇧⌘F, selectors, checkmarks unchanged). Search field now gains keyboard focus when the search UI is activated via all three paths: menu item, ⇧⌘F shortcut (both via `WolfWindowController.showSidebar(panel:.search)`), and the magnifier button (`SidebarContainerView.showSearch()` — which also replaced a focus-clearing `makeFirstResponder(nil)`), through new `SearchView.focusSearchField()`. Reviewer APPROVED (all 3 paths focus, tree path doesn't steal focus, the nil-resign replacement is safe). Docs + stale source comments synced (0 old-title refs remain), prettier-clean. Green under bazelisk: app builds+signs, WolfKeyMapTests PASS, swift_format PASS. Left uncommitted for review.

## Feature request (two tweaks)
1. Rename the two View-menu items: **"Show Project Tree" → "Project"**, **"Show Search" → "Search"** (keep ⇧⌘E / ⇧⌘F, same actions/checkmarks).
2. When the search UI is invoked via ANY of: the menu item, the ⇧⌘F keyboard shortcut, or clicking the magnifying-glass header button — put keyboard focus in the search UI's text field.

## Design (pure AppKit — no Rust, no new headless test surface)
This is a string rename + NSResponder first-responder wiring. No new seam/unit test (focus is imperative AppKit, manual-smoke only, like the rest of SearchView/SidebarContainerView).

### Anchors (verified)
- `SearchView.swift:84` — `private let searchField = NSTextField()` (the search text field; action `runSearch` on Return, L347-348).
- `SidebarContainerView.swift`: `let search: SearchView` (L26); magnifier button action `@objc private func showSearch()` (L76) currently `setMode(.search); search.seedToggles()`. `folderButton`/`searchButton` header (L97-99).
- `WolfWindowController.swift:707-709` — `func showSidebar(panel:)` = `window?.sidebarItem.isCollapsed = false` then `sidebarContainer?.setMode(...)`. Container exposed via `sidebarContainer` (private var); `sidebarContainer?.search` is reachable (`search` is a public `let`).
- `AppMenu.swift:192` title "Show Project Tree", `:200` title "Show Search"; doc comment `:161-168` names them.

### Changes
1. **`AppMenu.swift`**: line 192 title `"Show Project Tree"` → `"Project"`; line 200 title `"Show Search"` → `"Search"`. Update the `viewMenuItem` doc comment (L161-168) to the new names.
2. **`SearchView.swift`**: add
   ```swift
   /// Make the search text field the window's first responder (called when the
   /// search panel is activated via menu / ⇧⌘F / the magnifier button).
   func focusSearchField() { window?.makeFirstResponder(searchField) }
   ```
3. **`SidebarContainerView.swift`** `showSearch()` (magnifier button, L76): after `setMode(.search)` + `search.seedToggles()`, add `search.focusSearchField()` — focuses even if already in search mode (an explicit "invoke search" gesture).
4. **`WolfWindowController.swift`** `showSidebar(panel:)` (L707-709): after the reveal + `setMode`, if `panel == .search`, focus the field. Because the sidebar may have just transitioned collapsed→expanded, do the focus AFTER `setMode`; if a same-tick `makeFirstResponder` on a just-uncollapsed split item proves unreliable in manual smoke, wrap that ONE call in `DispatchQueue.main.async { … }`. Recommended:
   ```swift
   func showSidebar(panel: SidebarPanel) {
     window?.sidebarItem.isCollapsed = false
     sidebarContainer?.setMode(panel == .tree ? .tree : .search)
     if panel == .search { sidebarContainer?.search.focusSearchField() }
   }
   ```
   (Coder: prefer the synchronous form; only add async if needed. The menu path and the ⇧⌘F shortcut both funnel through here, so this one site covers both.)

Note: the magnifier button is only visible/clickable when the sidebar is already shown, so its path (change 3) needs no reveal. The menu/shortcut path (change 4) both reveals and focuses.

## Test plan
No new headless tests (nothing pure to add — first-responder focus + a title string are AppKit/manual-smoke). Existing `SidebarMenuSeamTests` still valid (titles/selectors unchanged; only display titles changed, which the seam does not touch). Verify by build + existing suite + linter.

## Verification
- `bazelisk build //crates/wolf-macos:Wolf` (compiles + signs).
- `bazelisk test //crates/wolf-macos:WolfKeyMapTests` (still green — no seam behavior changed).
- `bazelisk test //tools/lint:swift_format`.

## Docs to update (technical-writer)
- `crates/wolf-macos/Sources/README.md` + `crates/wolf-macos/README.md`: rename the menu-item references "Show Project Tree"/"Show Search" → "Project"/"Search"; note the new focus-on-activation behavior in the `SearchView`/`SidebarContainerView`/`WolfWindowController`/AppMenu prose. `prettier --write`.

## Progress log
(updated as agents report)
