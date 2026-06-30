# wolf-macos

The macOS front-end: a native **AppKit** application (Swift) that renders the embedded Neovim character grid and forwards keyboard input, over the generated [swift-bridge](../wolf-ffi) surface. It is a thin native shell — native window chrome and a CoreText/CoreGraphics paint backend — over the unchanged, platform-agnostic [`wolf-core`](../wolf-core).

## Design decisions (locked)

- **Tahoe (macOS 26) and newer only** (`LSMinimumSystemVersion` / `minimum_os_version` = `26.0`).
- **AppKit, not SwiftUI.** Implementation language **Swift**; **Xcode 26+**.
- Build is **all-Bazel and authoritative**: `rules_swift` + `rules_apple` build the `.app`; `rules_xcodeproj` generates an **uncommitted** `.xcodeproj` for debugging/LLDB only (`.gitignore`d).
- Rust ↔ Swift via swift-bridge, confined to `wolf-ffi`; the generated bindings are committed.
- **Key translation lives in Rust.** Swift sends a structured `FfiKeyEvent`; `wolf-core` owns nvim notation.

## Modules and sources

### `Sources/WolfKeyMap/` — the pure key seam (no AppKit)

`WolfKeyMap.translate(keyCode:characters:charactersIgnoringModifiers:ctrl:shift:alt:meta:) -> FfiKeyEvent?` is the macOS analog of wolf-gnome's `gdk_key` seam: it takes exactly the data an `NSView.keyDown(_:)` pulls off an `NSEvent` and returns a generated `FfiKeyEvent?`. It imports only Foundation and the generated FFI struct, so it is unit-testable headlessly. It honors the **Shift contract**:

- Named keys (Return, Tab, arrows, F-keys, …) are recognized from `keyCode` first and KEEP the real `shift` flag (so wolf-core can emit `<S-Tab>`, …).
- Printable characters carry the already-shifted glyph in `ch` with `shift = false` (Shift+'a' → 'A', Shift+'1' → '!').
- When ctrl/alt/meta are held, the glyph comes from `charactersIgnoringModifiers` (lowercased), never the modifier-mangled `characters`.
- Bare modifier keys and unmodified control characters map to `nil`.

The Bazel `wolf_key_map` module compiles `KeyMap.swift` together with the swift-bridge-generated Swift glue (which provides `FfiKeyEvent` and the `WolfCoreHandle` / `FfiFrame` wrappers), so the rest of the app gets those types via `import WolfKeyMap`.

### `Sources/WolfMacOS/` — the AppKit application

- `main.swift` — entry point: `NSApplication.shared`, `.regular` policy, run.
- `AppDelegate.swift` — points wolf-core at the **bundled** Neovim (sets `WOLF_NVIM_BIN` to `Contents/Resources/nvim/bin/nvim` only if a developer hasn't already exported it, via `setenv(..., 0)`), resolves `init_lua` (honoring a `WOLF_INIT_LUA` override, else the bundled `Contents/Resources/init.lua`) + cwd, calls `wolf_ffi_start`, builds the window + `GridView`, registers the view with `WakeBridge`, terminates after the last window, and `shutdown`s the core on terminate.
- `MainWindow.swift` — the single "Wolf" `NSWindow`, sized from cell metrics × initial cols/rows.
- `GridView.swift` — the editor `NSView` (`isFlipped`): `draw(_:)` reads a frame and paints cell backgrounds + glyphs (CoreText) + a mode-aware cursor; `keyDown(_:)` runs `WolfKeyMap.translate` → `send_key`; resize maps points to a (cols, rows) grid size → `resize`.
- `CellMetrics.swift` — measures a monospaced `CTFont` (SF Mono → Menlo fallback; `WOLF_FONT_PT` override).
- `Cursor.swift` — cursor-shape rectangle geometry (ported from wolf-gnome's `cursor.rs`); reverse-video colors are resolved in Rust.
- `Color.swift` — packed `FfiColor` → `CGColor`, with white/black fallbacks mirroring wolf-gnome's `paint.rs`.
- `WakeBridge.swift` — registers the `extern "C"` wake thunk (`DispatchQueue.main.async { registeredView?.needsDisplay = true }`).

## Building

```sh
bazel build //crates/wolf-macos:Wolf            # the .app bundle (pinned nvim bundled in)
bazel test  //crates/wolf-macos:WolfKeyMapTests
bazel test  //crates/wolf-macos:bundle_layout_test  # asserts the .app resource layout
bazel run   //crates/wolf-macos:xcodeproj       # generate the (uncommitted) Xcode project
```

The `.app` bundles a pinned Neovim 0.12.3 tree at `Contents/Resources/nvim/` and the captive `init.lua` at `Contents/Resources/init.lua` (downloaded hermetically by the `//bazel/nvim` module extension — see [`bazel/nvim/README.md`](../../bazel/nvim/README.md) and [Building Wolf](../../build.md)), so a double-clicked `.app` runs with no shell environment. `bundle_layout_test` asserts that resource layout over the built `.app`.

## Threading

`wolf-core` runs tokio on its own thread(s) and fires a wake on `flush`. The wake is the registered `extern "C"` callback (see `wolf-ffi`); `WakeBridge` hops it onto the AppKit main thread and calls `setNeedsDisplay`, which triggers a `draw(_:)` that reads the current frame. The `WolfCoreHandle` is used only on the main thread (it is not `Send`).

## Known limitations (this increment)

Single window only; no native command line / messages / popup menu, no panes, no file picker / tree, no mouse or scroll input, and no theming. The app now **bundles its own pinned Neovim 0.12.3** + the captive `init.lua` in `Contents/Resources/`, so double-clicking `Wolf.app` works with no shell environment; the `WOLF_NVIM_BIN` and `WOLF_INIT_LUA` development overrides are still honored (an exported value wins). The view paints only the **active grid** with no cmdline strip, so entering Neovim's cmdline mode (`:`/`/`) swaps the whole view to the outer grid (grid 1) until the prompt closes — unlike wolf-gnome, which keeps the editor panes visible and paints the command line in a dedicated bottom strip. Per-cell glyph **text attributes** (bold/italic/underline/undercurl/strikethrough) are resolved and packed by `wolf-ffi` but are NOT yet consumed in painting (only per-cell fg/bg + the mode-aware cursor are painted this increment). Live `draw(_:)` and `keyDown:` are not yet exercised by an automated test (no headless window server); the testable logic is isolated in `WolfKeyMap` and the `wolf-ffi` bridge test. The live nvim→FFI round-trip **is** covered on the macOS Bazel host: the `//crates/wolf-ffi:bridge_smoke` test passes under `bazel test`, exercising the full round-trip (start, type, observe the grid, resize, shut down) against the bundled/pinned Neovim 0.12.3. The GUI window rendering itself (and Finder double-click) is still not headlessly asserted.
