# Current Plan: Sidebar UI — Toolbar + Working-Directory Title

## Problem statement

We are changing the macOS window chrome (`crates/wolf-macos`) in two ways:

1. **Show a toolbar.** The window currently has NO `NSToolbar` (it uses a
   transparent `.fullSizeContentView` titlebar). Add an `NSToolbar`. For now its
   ONLY item is a "show sidebar" button (toggle the file-tree sidebar).

2. **Show the working directory instead of the filename in the title.** Today the
   window `title` shows the active buffer's filename (basename) via
   `TabTitleSeam` + `AppDelegate.refreshTabTitles`. Change it so the title shows
   the **working directory**, with a **file proxy** (`representedURL` = the cwd)
   so the user can command-click the title to see its parent directories.

## Decisions (confirmed with the user)

- **Title text form:** the FULL working-directory path, abbreviating `$HOME` → `~`
  per macOS convention (e.g. `~/Git/jplein/wolf`).
- **Tab labels:** `NSWindow.title` drives BOTH the titlebar text AND the native
  tab-group label. Both become the working directory. The filename-based
  `TabTitleSeam` disambiguation is RETIRED (tabs in the same folder share a
  label — acceptable).
- **File proxy:** set `window.representedURL` to the cwd directory URL so the
  proxy icon + command-click parent-hierarchy popup works.
- **Toolbar item:** a single toggle-sidebar button.

## Key existing code (for reference)

- `crates/wolf-macos/Sources/WolfMacOS/MainWindow.swift` — the `NSWindow`
  subclass; transparent titlebar, computes chrome height to keep the grid at
  exactly cols×rows.
- `crates/wolf-macos/Sources/WolfMacOS/EditorSplitViewController.swift` — the
  `NSSplitViewController`; `sidebarItem` is the collapsible `.sidebar` split item.
- `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift` —
  `refreshTabTitles(forGroupOf:)` sets each tab's `NSWindow.title` from
  `TabTitleSeam.labels(...)`.
- `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift` — per-window
  controller; `persistedCwd` / `currentCwd` expose the cwd; `onRenderWake()`
  already calls `coordinator?.refreshTabTitles(forGroupOf:)` each wake and
  reacts to cwd changes.
- `crates/wolf-macos/Sources/WolfKeyMap/TabTitleSeam.swift` — pure filename-title
  seam (Foundation-only, tested by `TabTitleSeamTests`).

## Status: COMPLETE ✓

Docs updated (README ×3 + architecture.md), prettier-conformant, no orphan
`TabTitleSeam` refs in code/BUILD. Tests pass (273, 0 failures; 9 new
CwdTitleSeamTests run). Build clean. Manual-smoke items remain for the user (see
below).

## Status (historical): review PASSED — updating docs

### Reviewer verdict: Approved, no code defects
- All 6 focus areas PASS. Notes: (1) stale `TabTitleSeam` reference at
  `architecture.md:66` — route to tech-writer (NOT in the original §F list);
  (2) `.whitespaces` vs `.whitespacesAndNewlines` in CwdTitleSeam.swift:35 —
  theoretical only, LEFT as-is.

### Coder result
- Implemented all steps A–F. `bazelisk test //crates/wolf-macos:WolfKeyMapTests`
  → 273 tests, 0 failures; all 9 `CwdTitleSeamTests` ran; `TabTitleSeamTests`
  gone. `bazelisk build //crates/wolf-macos:wolf_macos_lib //crates/wolf-macos:wolf_key_map`
  → success. `//tools/lint:swift_format` → passed. No deviations.
- SourceKit diagnostics (WolfFFI/WolfKeyMap/XCTest "No such module",
  SavedPaneNode) are IDE-only false positives — Bazel build is authoritative.
- Manual-smoke items still pending (non-headless): initial grid stays cols×rows
  with toolbar band; toggle-sidebar button works; cwd title + proxy command-click;
  empty-state "Wolf" + no proxy.

## Architect's implementation plan

### A. New title seam + test
- CREATE `Sources/WolfKeyMap/CwdTitleSeam.swift`: `public enum CwdTitleSeam` with
  `public static func displayTitle(cwd: String, home: String) -> String`.
  Contract:
  - empty/whitespace `cwd` → `""` (caller substitutes app-name fallback).
  - normalize `home` by stripping one trailing `/`; empty `home` disables
    abbreviation (return `cwd` unchanged).
  - `cwd == home` → `"~"`.
  - `cwd` begins with `home + "/"` → `"~/" + remainder` (segment-boundary match
    only — guard the `/Users/jane` vs `/Users/janet/x` prefix-collision bug).
  - otherwise → `cwd` verbatim (full absolute path).
  - Foundation-only, NO implicit `$HOME`/`FileManager` — `home` always injected
    (load-bearing purity / determinism constraint).
- CREATE `Tests/WolfKeyMapTests/CwdTitleSeamTests.swift`: `@testable import
  WolfKeyMap`. Cases: under-home→abbreviated; cwd==home→`~`; not-under-home→
  verbatim; prefix-collision guard; trailing-slash home normalizes; empty cwd→
  `""`; empty home→verbatim; deep nested path keeps all trailing segments.

### B. Toolbar (`Sources/WolfMacOS/MainWindow.swift`)
- Make `MainWindow` conform to `NSToolbarDelegate` (extension; no new stored
  property — `NSWindow.toolbar` strong-holds it, delegate is weak).
- Add `private static let toolbarIdentifier = NSToolbar.Identifier("com.wolf.editor.toolbar")`.
- Use the SYSTEM `.toggleSidebar` item (auto-wires to
  `NSSplitViewController.toggleSidebar(_:)` up the responder chain — same
  mechanism as the existing View-menu Toggle Sidebar item). Delegate methods:
  - `toolbarDefaultItemIdentifiers` → `[.toggleSidebar]`
  - `toolbarAllowedItemIdentifiers` → `[.toggleSidebar]`
  - `toolbar(_:itemForItemIdentifier:willBeInsertedIntoToolbar:)` → return `nil`
    for `.toggleSidebar` so AppKit supplies the built-in item; FALLBACK: if the
    slot renders empty, construct an `NSToolbarItem` for that identifier.
- `toolbarStyle = .unified`; `toolbar.displayMode = .iconOnly`; keep
  `titleVisibility = .visible`.
- **CRITICAL ordering:** install the toolbar (create, set delegate, assign
  `self.toolbar`, set `toolbarStyle`) AFTER `contentViewController = splitVC`
  (line 112) but BEFORE the `let fullHeight = ...` / `chrome` measurement (lines
  118-119), so the measured chrome includes the toolbar band and `setContentSize`
  keeps the grid at exactly cols×rows. If manual smoke shows a wrong initial
  grid, add `contentView?.layoutSubtreeIfNeeded()` after assigning the toolbar,
  before reading `contentLayoutRect`.

### C. Title / representedURL wiring (`Sources/WolfMacOS/AppDelegate.swift`)
- Rewrite `refreshTabTitles(forGroupOf:)` (lines 587-600):
  - resolve `home` via existing `homeDir()` helper (line 258).
  - for each `tabWindow` in `window.tabGroup?.windows ?? [window]`, find its
    controller, read its **`persistedCwd`** (correct for non-current tabs; `""`
    for empty-state — NOT `currentCwd`, NOT `activeBufferPath`).
  - `title = CwdTitleSeam.displayTitle(cwd: cwd, home: home)`.
  - `tabWindow.title = title.isEmpty ? "Wolf" : title`.
  - `tabWindow.representedURL = cwd.isEmpty ? nil : URL(fileURLWithPath: cwd, isDirectory: true)`.
  - remove the `TabTitleSeam.labels` / `paths` / `zip` machinery; update the doc
    comment.

### D. Retire filename title plumbing (`Sources/WolfMacOS/WolfWindowController.swift`)
- Remove `cachedActiveBufferPath` stored property (lines 160-161) + its refresh
  at top of `onRenderWake()` (line 364); remove `activeBufferPath` accessor
  (lines 691-697). KEEP the unconditional `refreshTabTitles(forGroupOf:)` call in
  `onRenderWake()` (now reads cwd; keeps titles fresh after `:cd`). Update the
  `onRenderWake` comment (lines 394-397).
- DELETE `Sources/WolfKeyMap/TabTitleSeam.swift` and
  `Tests/WolfKeyMapTests/TabTitleSeamTests.swift` (only AFTER step C removes the
  last app reference).
- Fix stray comments referencing `TabTitleSeam` in `RestoreSeam.swift` (line 19)
  and `RestoreSeamTests.swift` (line 9).

### E. BUILD.bazel wiring (`crates/wolf-macos/BUILD.bazel`)
- `wolf_key_map` `srcs`: ADD `Sources/WolfKeyMap/CwdTitleSeam.swift`, REMOVE
  `Sources/WolfKeyMap/TabTitleSeam.swift` (alphabetical).
- `WolfKeyMapTests` `srcs`: ADD `Tests/WolfKeyMapTests/CwdTitleSeamTests.swift`,
  REMOVE `Tests/WolfKeyMapTests/TabTitleSeamTests.swift`. (Hand-maintained list,
  NOT a glob — unlisted files silently never run.)
- No `deps` changes. `swift_format_srcs` glob needs no edit.

### F. Docs (source of truth)
- `README.md`: line 43 (TabTitleSeam para → CwdTitleSeam), line 80 (compiled
  files list), line 87 (AppDelegate/refreshTabTitles), line 88
  (WolfWindowController), line 91 (MainWindow — now HAS a `.unified` toolbar +
  cwd title + proxy), line 92 (EditorSplitViewController — toolbar toggle).
- `Sources/README.md`: line 17 (TabTitleSeam bullet → CwdTitleSeam); verify the
  "Thirty-three … files" count is unchanged (one added, one removed = 33).
- `Tests/README.md`: line 3 roll-call, lines 117-119 section, line 135 removed-
  seams note.
- `prettier --write` all edited Markdown; no hard line breaks inside paragraphs.

### Implementation order (avoid compile/test breakage)
1. Create `CwdTitleSeam.swift`. 2. Add it to `wolf_key_map` srcs. 3. Update
`AppDelegate.refreshTabTitles`. 4. Update `WolfWindowController` cleanup. 5.
Install toolbar in `MainWindow`. 6. Delete `TabTitleSeam.swift` +
`TabTitleSeamTests.swift`. 7. Update BUILD.bazel (drop deleted, add new test). 8.
Create `CwdTitleSeamTests.swift`. 9. Update READMEs + stray comments; prettier.
10. `bazelisk test //crates/wolf-macos:WolfKeyMapTests`; confirm new test runs,
old gone; build app for smoke.

### Testability note
Only `CwdTitleSeam` is headlessly testable. The AppKit glue (toolbar, chrome
ordering, title/representedURL) is compile-verify + manual-smoke — do NOT invent
mock-heavy AppKit tests (consistent with how these files self-document).

### Risks
- Chrome measurement synchrony (grid must stay 80×24) — highest risk; mitigate
  with `layoutSubtreeIfNeeded()` if needed; verify visually.
- System `.toggleSidebar` provisioning — fall back to a constructed item if empty.
- KVO on animated `isCollapsed` still fires (both observers) — verify in smoke.
- Empty-state title = "Wolf" + no proxy (product decision).

