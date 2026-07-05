# Feature: Window titlebar reflects cwd (home → ~), "Wolf" for empty window

## Problem statement

Change the Wolf macOS window title from the hardcoded literal `"Wolf"` to the
window's live cwd, home-abbreviated (e.g. `~/Git/jplein/wolf`, or exactly `~`
when at home). Empty-state windows (no folder open, `hasFolder == false`)
continue to show `"Wolf"`. Title logic goes into a new pure, Foundation-only,
unit-testable seam in `WolfKeyMap`; the `window.title` assignment is thin AppKit
glue applied from `WolfWindowController.onRenderWake()`.

## Status: COMPLETE ✅ — implemented, tested (WolfKeyMapTests PASS, 15 rows), reviewed (code APPROVED), docs synced + prettier-clean

### Final outcome
- New pure seam `WindowTitleSeam.title(cwd:hasFolder:homeDir:)` in `wolf_key_map`; wired into
  `WolfWindowController.onRenderWake()` (unconditional per-wake `window?.title` set, home from
  `FileManager.default.homeDirectoryForCurrentUser.path`); `MainWindow.swift:94` kept as the
  initial "Wolf" placeholder.
- Tests: `//crates/wolf-macos:WolfKeyMapTests` PASS (15 table rows incl. component-boundary
  safety, trailing-slash normalization, root edge, empty-home/empty-cwd fallbacks).
  `wolf_macos_lib` builds (compile-verifies the controller change).
- Reviewer: code + tests APPROVED (no correctness bugs); only blocker was missing docs.
- Docs: `Sources/README.md` (count 17→18, WindowTitleSeam bullet, MainWindow + WolfWindowController
  entries), `Tests/README.md` (seam enumeration + WindowTitleSeamTests subsection),
  `README.md` (seam inventory + compile list + controller/window entries) — all updated,
  `prettier --write` clean.
- REMAINING (not automatable): manual GUI smoke on a window server — launch the app, confirm the
  titlebar shows the home-abbreviated cwd (e.g. `~/Git/jplein/wolf`), `~` at home, an unchanged
  absolute path outside home, and "Wolf" for an empty (no-folder) window; `:cd` updates it live.

---

## 1. New pure seam

### Create `crates/wolf-macos/Sources/WolfKeyMap/WindowTitleSeam.swift`
Namespace mirroring `WindowIndexSeam`/`StatusbarLayout` (header comment,
`import Foundation`, `public enum` with static members only).

- Type: `public enum WindowTitleSeam`
- Func: `public static func title(cwd: String, hasFolder: Bool, homeDir: String) -> String`

Semantics (document + enforce):
1. `hasFolder == false` -> `"Wolf"`.
2. Normalize cwd: strip trailing `/` down to a non-empty remainder; keep bare `/` intact.
3. Normalized cwd empty -> `"Wolf"` (safe fallback; folder window should never have empty cwd).
4. Normalize `homeDir` the same way. Empty normalized home -> return normalized cwd unchanged (no `~`).
5. Normalized `cwd == normalized homeDir` -> `"~"`.
6. cwd under home on a COMPONENT BOUNDARY — `cwd.hasPrefix(homeDir)` AND the char one past
   the prefix is `/` -> `"~" + cwd.dropFirst(homeDir.count)` (yields `"~/" + remainder`).
   Because home is trailing-slash-normalized and boundary char must be `/`, `/Users/jane`
   does NOT match `/Users/janet`.
7. Otherwise -> normalized cwd unchanged.

## 2. Call site: `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift`
- Add `private let homeDir: String`, set in `init(...)` to
  `FileManager.default.homeDirectoryForCurrentUser.path` (matches
  `AppDelegate.swift:284`, same home value used by `StateSeam.launchDecision`).
- Add `private func updateWindowTitle(cwd: String)` doing
  `window?.title = WindowTitleSeam.title(cwd: cwd, hasFolder: hasFolder, homeDir: homeDir)`.
- In `onRenderWake()`: after the `if !hasFolder, !cwd.isEmpty { ... }` transition block
  (after ~line 297, so `hasFolder` reflects any `didOpenFolder()` flip) and NOT gated by
  the `lastKnownCwd != cwd` guard, call `updateWindowTitle(cwd: cwd)` UNCONDITIONALLY each
  wake. Rationale: title depends on BOTH cwd and hasFolder; an unconditional idempotent set
  is provably correct across initial show, empty→folder flip, and typed `:cd`. Do NOT fold
  it inside the `lastKnownCwd` guard (a hasFolder flip with unchanged cwd string would miss).
- `window` is `private var window: MainWindow?` — assign via `window?.title`.

## 3. `crates/wolf-macos/Sources/WolfMacOS/MainWindow.swift`
- KEEP `title = "Wolf"` at line 94 as the initial placeholder (shown until the first render
  wake sets the real title). No code change required (optional comment note only).

## 4. BUILD wiring: `crates/wolf-macos/BUILD.bazel`
Both `wolf_key_map.srcs` (~lines 29-47) and `WolfKeyMapTests.srcs` (~lines 172-190) are
EXPLICIT lists (no glob), so add manually, alphabetically:
- `"Sources/WolfKeyMap/WindowTitleSeam.swift"` (after `WindowIndexSeam.swift`).
- `"Tests/WolfKeyMapTests/WindowTitleSeamTests.swift"` (after `WindowIndexSeamTests.swift`).

## 5. Tests
Pure seam only; the `window.title` assignment is AppKit glue = compile + manual smoke
(consistent with repo conventions). No integration/e2e / real-nvim test warranted.

### Create `crates/wolf-macos/Tests/WolfKeyMapTests/WindowTitleSeamTests.swift`
Table-driven `XCTestCase`, `@testable import WolfKeyMap`, mirroring `WindowIndexSeamTests`
(`struct Case { desc; cwd; hasFolder; homeDir; expected }`). Cover:
- `hasFolder == false` -> `"Wolf"` (with non-empty AND empty cwd).
- Folder, `cwd == homeDir` -> `"~"`.
- Folder, `cwd == homeDir + "/"` -> `"~"`.
- Folder, cwd under home (`/Users/jane/Git/jplein/wolf`, home `/Users/jane`) -> `"~/Git/jplein/wolf"`.
- Folder, home trailing slash (`/Users/jane/`), cwd `/Users/jane/x` -> `"~/x"`.
- Boundary: home `/Users/jane`, cwd `/Users/janet` -> `"/Users/janet"` (NOT `~t`).
- Boundary: home `/Users/jane`, cwd `/Users/janet/x` -> unchanged.
- Folder, cwd not under home (`/tmp/project`) -> `"/tmp/project"`.
- Folder, cwd trailing slash not under home (`/tmp/project/`) -> `"/tmp/project"`.
- Folder, empty cwd -> `"Wolf"`.
- Folder, empty homeDir, cwd `/Users/jane/x` -> `/Users/jane/x` unchanged.
- Root edge: folder, cwd `/`, home `/Users/jane` -> `"/"`.

## 6. Docs to update (source of truth; `prettier --write`, no mid-paragraph hard breaks)
- `crates/wolf-macos/Sources/README.md`:
  - WolfKeyMap count "Seventeen ... files" (line ~6) -> "Eighteen".
  - Add `WindowTitleSeam.swift` bullet near `WindowIndexSeam` (line ~14): enum, Foundation-only,
    `title(cwd:hasFolder:homeDir:)` semantics + tested by `WindowTitleSeamTests`.
  - `MainWindow.swift` entry (line ~35): `title = "Wolf"` is only the initial placeholder,
    overridden per render wake by `WolfWindowController` to the home-abbreviated live cwd
    (or "Wolf" for an empty-state window).
  - `WolfWindowController.swift` entry (line ~33): `onRenderWake()` also sets `window.title`
    from `WindowTitleSeam.title(...)` (home from `FileManager...homeDirectoryForCurrentUser.path`)
    each wake, following the live cwd and the empty→folder transition.
- `crates/wolf-macos/Tests/README.md`:
  - Intro seam enumeration (line ~3): add the window-title seam.
  - Add a `## WolfKeyMapTests/WindowTitleSeamTests.swift` subsection (mirror `WindowIndexSeamTests`).
- `crates/wolf-macos/README.md`:
  - Read first; add `WindowTitleSeam` to any seam list/inventory; correct any "title is Wolf"
    statement; bump a seam count if it states one.

## 7. Implementation order (never un-buildable)
1. Create `WindowTitleSeam.swift`.
2. Add it to `wolf_key_map.srcs` in BUILD.bazel.
3. Modify `WolfWindowController.swift` (homeDir prop + init + `updateWindowTitle` + onRenderWake call).
4. Leave `MainWindow.swift` as-is.
5. Create `WindowTitleSeamTests.swift`.
6. Add test file to `WolfKeyMapTests.srcs` in BUILD.bazel.
7. Update docs + `prettier --write`.

## Files
Create:
- `crates/wolf-macos/Sources/WolfKeyMap/WindowTitleSeam.swift`
- `crates/wolf-macos/Tests/WolfKeyMapTests/WindowTitleSeamTests.swift`
Modify:
- `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift`
- `crates/wolf-macos/BUILD.bazel`
- `crates/wolf-macos/Sources/README.md`
- `crates/wolf-macos/Tests/README.md`
- `crates/wolf-macos/README.md`

## Deps / lint
None. Foundation-only seam. Keep seam AppKit-free so it stays in `wolf_key_map`.

## Test target to run
`bazelisk test //crates/wolf-macos:WolfKeyMapTests` (plus a `wolf_macos_lib`/app build to
compile-verify the WolfWindowController change).
