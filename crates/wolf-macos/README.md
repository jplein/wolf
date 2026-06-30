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

The module also holds `PickerSeam.swift` — a second AppKit-free seam (`public enum PickerSeam`, Foundation only, headlessly tested) that ports wolf-gnome's pure picker helpers:

- `isPickerTrigger(charactersIgnoringModifiers:ctrl:shift:) -> Bool` — the **Shift+Control+P** trigger predicate (`ctrl && shift && first scalar lowercased == "p"`; alt/meta ignored). Deliberately non-conflicting so every Neovim key, including `<C-p>`, is preserved.
- `nextSelection(current:delta:nRows:) -> Int?` — list navigation clamp (a port of wolf-gnome's `next_selection`; `nil` when there are no rows).
- `restoredIndex(resetToTop:prev:newLen:) -> Int?` — selection restore across streaming result batches (a port of `restored_index`).

The Bazel `wolf_key_map` module compiles `KeyMap.swift` and `PickerSeam.swift` together with the swift-bridge-generated Swift glue (which provides `FfiKeyEvent`, the `WolfCoreHandle` / `FfiFrame` wrappers, and the `FfiFileSearch` / `FfiFileEntry` picker types), so the rest of the app gets those types via `import WolfKeyMap`.

### `Sources/WolfMacOS/` — the AppKit application

- `main.swift` — entry point: `NSApplication.shared`, `.regular` policy, run.
- `AppDelegate.swift` — points wolf-core at the **bundled** Neovim (sets `WOLF_NVIM_BIN` to `Contents/Resources/nvim/bin/nvim` only if a developer hasn't already exported it, via `setenv(..., 0)`), resolves `init_lua` (honoring a `WOLF_INIT_LUA` override, else the bundled `Contents/Resources/init.lua`) + cwd, calls `wolf_ffi_start`, installs `WakeBridge` / `PickerWakeBridge` / `TreeWakeBridge`, builds the window + `GridView`, builds/starts/holds the sidebar `FileTreeView` (registering it with `WakeBridge.registeredTree` and `TreeWakeBridge.registeredTree`), builds/holds the overlay `PickerView` (wiring `GridView.onPickerTrigger` to open it), terminates after the last window, and on terminate `close`s the tree + picker sessions then `shutdown`s the core.
- `MainWindow.swift` — the single "Wolf" `NSWindow`, whose content view is a horizontal `NSSplitView` of the `FileTreeView` sidebar (left, fixed `treePaneWidth`) and the `GridView` editor (right). The content width is widened by `treePaneWidth + dividerThickness` so the editor pane keeps its full initial cols × rows cell size; the `GridView` stays first responder.
- `FileTreeView.swift` — the file-tree sidebar `NSView` (an `NSOutlineView` in an `NSScrollView`) fed by its OWN long-lived `FfiFileSearch` session (`start_tree_search`). The cwd is shown as a single **synthetic root node** (its basename, sentinel rel-path `""`), **expanded on init** with `children_of("")` beneath it and their subdirectories collapsed — the VS Code / Snacks model, **divergent from the GNOME tree** (macOS-only). The sidebar **follows nvim's live cwd**: `start()` roots from `handle.cwd()`, and `onRenderWake()` (editor wake) reads `cwd()` first and **re-roots** (rebuild the root + restart `start_tree_search()`) when it changed (`:cd`/`DirChanged`), mirroring how the picker re-queries `getcwd()`. A single click opens a file (`open(absPath)`, then refocus the `GridView`) or toggles a directory. The current buffer's row is bolded; `onRenderWake()` reveals it on change — expanding the synthetic root then its ancestor dirs (`wolf_ffi_ancestors_to_reveal`) top-down before locating the row, leaving `pendingReveal` set if its file hasn't streamed in; `refresh()` (tree wake) reloads rows preserving expansion and retries the pending reveal.
- `GridView.swift` — the editor `NSView` (`isFlipped`): `draw(_:)` reads a frame and paints cell backgrounds + glyphs (CoreText) + a mode-aware cursor; `keyDown(_:)` runs `WolfKeyMap.translate` → `send_key`; resize maps points to a (cols, rows) grid size → `resize`. It also holds an `onPickerTrigger` closure: before translating a key, `keyDown(_:)` checks `PickerSeam.isPickerTrigger(...)` and, on a match, invokes the closure (to open the picker) instead of forwarding the key.
- `PickerView.swift` — the floating overlay `NSView` panel (an `NSSearchField` + an `NSTableView` in an `NSScrollView`) shown over the editor. `open()` starts an `FfiFileSearch` session and makes the search field first responder; typing filters via `ranked` (ranking in Rust); Up/Down navigate (using `PickerSeam.nextSelection`), selection is restored across streaming batches (`PickerSeam.restoredIndex`); Enter or a double-click calls `open(path)` and closes; Esc closes. While open, the search field holds first-responder status, so `GridView.keyDown` does not fire and keys are not forwarded to Neovim.
- `PickerWakeBridge.swift` — registers the picker's `extern "C"` wake (`wolf_ffi_set_filesearch_wake`) and, on each wake, `DispatchQueue.main.async`es onto the main thread to call `PickerView.refresh()` (a late wake after the picker closes is a harmless no-op).
- `TreeWakeBridge.swift` — registers the sidebar's `extern "C"` wake (`wolf_ffi_set_tree_wake`) and, on each wake, `DispatchQueue.main.async`es onto the main thread to call `FileTreeView.refresh()` (a late wake after the tree closes is a harmless no-op).
- `CellMetrics.swift` — measures a monospaced `CTFont` (SF Mono → Menlo fallback; `WOLF_FONT_PT` override).
- `Cursor.swift` — cursor-shape rectangle geometry (ported from wolf-gnome's `cursor.rs`); reverse-video colors are resolved in Rust.
- `Color.swift` — packed `FfiColor` → `CGColor`, with white/black fallbacks mirroring wolf-gnome's `paint.rs`.
- `WakeBridge.swift` — registers the `extern "C"` wake thunk (`DispatchQueue.main.async { registeredView?.needsDisplay = true; registeredTree?.onRenderWake() }`).

## Building

```sh
bazel build //crates/wolf-macos:Wolf            # the .app bundle (pinned nvim bundled in)
bazel test  //crates/wolf-macos:WolfKeyMapTests  # the pure key seam + the pure picker seam
bazel test  //crates/wolf-macos:bundle_layout_test  # asserts the .app resource layout
bazel run   //crates/wolf-macos:xcodeproj       # generate the (uncommitted) Xcode project
```

The `.app` bundles a pinned Neovim 0.12.3 tree at `Contents/Resources/nvim/` and the captive `init.lua` at `Contents/Resources/init.lua` (downloaded hermetically by the `//bazel/nvim` module extension — see [`bazel/nvim/README.md`](../../bazel/nvim/README.md) and [Building Wolf](../../build.md)), so a double-clicked `.app` runs with no shell environment. `bundle_layout_test` asserts that resource layout over the built `.app`.

## Threading

`wolf-core` runs tokio on its own thread(s) and fires a wake on `flush`. The wake is the registered `extern "C"` callback (see `wolf-ffi`); `WakeBridge` hops it onto the AppKit main thread and calls `setNeedsDisplay`, which triggers a `draw(_:)` that reads the current frame. The `WolfCoreHandle` is used only on the main thread (it is not `Send`).

## Known limitations (this increment)

Single window only; no native command line / messages / popup menu, no panes, no mouse or scroll input, and no theming. (The **file picker** — Shift+Control+P — and the **file-tree sidebar** have both landed.) The app now **bundles its own pinned Neovim 0.12.3** + the captive `init.lua` in `Contents/Resources/`, so double-clicking `Wolf.app` works with no shell environment; the `WOLF_NVIM_BIN` and `WOLF_INIT_LUA` development overrides are still honored (an exported value wins). The view paints only the **active grid** with no cmdline strip, so entering Neovim's cmdline mode (`:`/`/`) swaps the whole view to the outer grid (grid 1) until the prompt closes — unlike wolf-gnome, which keeps the editor panes visible and paints the command line in a dedicated bottom strip. Per-cell glyph **text attributes** (bold/italic/underline/undercurl/strikethrough) are resolved and packed by `wolf-ffi` but are NOT yet consumed in painting (only per-cell fg/bg + the mode-aware cursor are painted this increment). Live `draw(_:)` and `keyDown:` are not yet exercised by an automated test (no headless window server); the testable logic is isolated in `WolfKeyMap` and the `wolf-ffi` bridge tests. The AppKit picker UI (the panel rendering, the `NSSearchField` first-responder transfer, the `doCommandBy` routing, table selection, and the dispatched picker-wake reload) is **compile-verified and manual-smoke only**; its pure seam (`PickerSeam`) is headlessly unit-tested and its FFI surface (`FfiFileSearch` / `FfiFileEntry` / `open`) is covered by `//crates/wolf-ffi:file_search`. The AppKit **file-tree sidebar** UI (the `NSOutlineView` data source / delegate, the synthetic cwd-root node expanded-on-init, the live-cwd re-root on each render wake, single-click open / toggle, and the reveal + bold of the current buffer) is likewise **compile-verified and manual-smoke only** (it needs a window server); note its presentation **diverges from GNOME** — macOS shows the cwd as a single root node (VS Code / Snacks model) and follows nvim's live cwd, whereas the GNOME tree shows `children_of("")` directly and is unchanged. Its pure projection lives in `wolf_core::tree` (headlessly unit-tested under `//crates/wolf-core:wolf_core_lib_test`) and its FFI surface (`start_tree_search` / `children_of` / `rel_path_for` / `FfiTreeNode` / `current_file` / `cwd` / `wolf_ffi_ancestors_to_reveal`) is covered by `//crates/wolf-ffi:tree_search`. The live nvim→FFI round-trip **is** covered on the macOS Bazel host: the `//crates/wolf-ffi:bridge_smoke` test passes under `bazel test`, exercising the full round-trip (start, type, observe the grid, resize, shut down) against the bundled/pinned Neovim 0.12.3. The GUI window rendering itself (and Finder double-click) is still not headlessly asserted.
