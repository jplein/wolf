# wolf-macos/Sources

The Swift sources for the macOS front-end, split into two modules. See the crate
[`README.md`](../README.md) for the design decisions and the build workflow.

## `WolfKeyMap/` — the pure key seam (no AppKit)

A single file, deliberately AppKit-free so it is unit-testable headlessly (the
macOS analog of wolf-gnome's `gdk_key` seam). The Bazel `wolf_key_map` module
compiles it together with the swift-bridge-generated Swift glue, so the rest of the
app gets the generated types (`FfiKeyEvent`, `WolfCoreHandle`, `FfiFrame`, …) via
`import WolfKeyMap`.

- **`KeyMap.swift`** — `WolfKeyMap.translate(keyCode:characters:charactersIgnoringModifiers:ctrl:shift:alt:meta:) -> FfiKeyEvent?`.
  It imports only Foundation and the generated FFI struct. Named keys (Return, Tab,
  arrows, F-keys, …) are recognized from the virtual `keyCode` first and **keep**
  the real `shift` flag; printable characters carry the already-shifted glyph in
  `ch` with `shift = false`; when ctrl/alt/meta are held the glyph comes from
  `charactersIgnoringModifiers` (lowercased); bare modifiers and unmodified control
  characters return `nil`. The `Kind` discriminants mirror `wolf_core::Key`'s
  variant order (and the Rust `key_event_from_ffi`).

## `WolfMacOS/` — the AppKit application

The `wolf_macos_lib` module: the native shell that links AppKit / CoreText /
CoreGraphics and depends on `:wolf_key_map` and the `wolf-ffi` archive.

- **`main.swift`** — the entry point: `NSApplication.shared`, install the
  `AppDelegate`, `.regular` activation policy, `app.run()`.
- **`AppDelegate.swift`** — resolves `init_lua` (a `WOLF_INIT_LUA` override, else
  the bundled `Contents/Resources/init.lua`) and the cwd, calls `wolf_ffi_start`,
  installs the `WakeBridge`, builds the window + `GridView`, registers the view with
  `WakeBridge`, terminates after the last window, and `shutdown`s the core on
  terminate (showing a fatal alert if the core fails to start).
- **`MainWindow.swift`** — the single "Wolf" `NSWindow`, sized to the initial
  cols/rows × the measured cell metrics, with the `GridView` as its content view and
  first responder.
- **`GridView.swift`** — the editor `NSView` (flipped, first responder): `draw(_:)`
  reads an `FfiFrame`, fills each cell's background, draws its glyph via CoreText,
  and overlays the mode-aware cursor when `cursor_visible`; `keyDown(_:)` runs
  `WolfKeyMap.translate` → `send_key`; `setFrameSize` / `viewDidEndLiveResize` map
  the bounds to a `(cols, rows)` grid size → `resize`.
- **`CellMetrics.swift`** — measures a monospaced `CTFont` (advance width, ascent +
  descent + leading, baseline) for SF Mono with a Menlo fallback, `WOLF_FONT_PT`
  overridable. (Per-cell text attributes are packed by `wolf-ffi` but not yet
  painted, so only the base font is used.)
- **`Cursor.swift`** — pure cursor-rectangle geometry (block / horizontal /
  vertical bar from `cursor_shape` + `cursor_cell_percentage`), ported from
  wolf-gnome's `cursor.rs`; the reverse-video fill/glyph colors are resolved in Rust.
- **`Color.swift`** — converts a packed `FfiColor` (`0x00RRGGBB` + `set`) to a
  `CGColor`, applying the white/black fallbacks that mirror wolf-gnome's `paint.rs`.
- **`WakeBridge.swift`** — registers the `extern "C"` wake thunk with the core
  (`wolf_ffi_set_wake`); on each wake it `DispatchQueue.main.async`es onto the
  AppKit main thread and sets the registered `GridView`'s `needsDisplay` (the
  wake → dispatch-to-main → `setNeedsDisplay` realization).
