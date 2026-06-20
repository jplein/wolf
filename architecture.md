# Architecture

Wolf is a Markdown editor that embeds Neovim as its editing engine. The GUI is a
**front-end for Neovim**, not an editor in its own right: Neovim owns all editor
state (buffer, cursor, modes, syntax, undo, file I/O), and Wolf renders what
Neovim tells it to render and forwards input back.

## Guiding principle

There is one hard rule everything else follows from:

> **`wolf-core` must compile with no GUI toolkit in its dependency tree.**

All the Neovim-specific work — and it is the large majority of the work — lives
in `wolf-core` and is shared verbatim across platforms. Each platform front-end
is a thin shell: native window chrome, native file dialogs, a paint backend, and
OS-event translation. If a type from GTK or AppKit ever appears in core, the
boundary has leaked.

## Components

### wolf-core

Platform-agnostic Rust. Contains everything Neovim-related and all app logic that
is not inherently platform-specific.

Responsibilities:

- Spawn and supervise the captive `nvim --embed` process; speak msgpack-RPC to it
  via [`nvim-rs`](https://github.com/KillTheMule/nvim-rs).
- Attach as a UI (`nvim_ui_attach` with `ext_linegrid`) and handle the entire
  `redraw` event stream.
- Maintain the **render-state model** — the in-memory description of what should
  be on screen (see "The core↔front-end contract").
- Translate abstract input events (key, mouse, scroll, resize) into Neovim input
  (`nvim_input`, `nvim_input_mouse`) and Ex commands.
- Own app-level semantics: open/save (delegated to Neovim), recent files, and the
  hermetic launch/isolation of the captive Neovim.

Responsibilities it explicitly does **not** have: deciding what text or colors
appear (Neovim does), drawing pixels (the front-ends do), or knowing about
windows, menus, or dialogs (only that it may *request* them via intents).

### Platform front-ends: wolf-gnome, wolf-macos

The UI layer. Each one:

- Paints the render-state model produced by core.
- Provides native window chrome: title bar / header bar / menus, scrolling,
  file dialogs.
- Translates OS input events into core's abstract input events.
- Queries the OS theme/accent color and pushes it into Neovim's highlight groups
  (so selection, etc., match the native look — see "Theming").
- Bridges its native event loop to core's async runtime.

#### wolf-gnome

Rust, using GTK 4 and libadwaita. Consumes `wolf-core` as a **normal Rust crate
dependency**. Should feel like a GNOME application.

#### wolf-macos

Swift, using AppKit. Consumes `wolf-core` through generated **swift-bridge**
bindings. Should feel like a macOS application.

## Architectural decisions

### 1. Core↔front-end boundary: swift-bridge on macOS, direct crate on Linux

Swift cannot import a Rust crate directly, so the macOS side needs a generated
binding regardless. We use [`swift-bridge`](https://github.com/chinedufn/swift-bridge),
which produces idiomatic Swift from an annotated bridge module in `wolf-core`.
Linux/GTK, being Rust, links `wolf-core` directly with no FFI.

Consequence — and a design constraint: the **front-end-facing API of `wolf-core`
must be expressible over swift-bridge** (FFI-friendly: plain structs, slices,
handles, and callbacks — no Rust-only generics or trait objects across the line).
The GTK front-end consumes that same API directly. This keeps a single
front-end-facing surface rather than two divergent ones.

We rejected a uniform C-ABI on both platforms: it would cost Linux its type
safety and async ergonomics for symmetry we don't need, since the boundary is
defined once in Rust either way.

### 2. Rendering: native paint per platform

The render-state model is toolkit-agnostic; each front-end paints it natively:
Cairo/Pango in GTK on Linux, CoreText/CoreGraphics in AppKit on macOS. This gives
the best native text rendering on each OS. The cell-painting loop is written
twice, but it is small once the render-state model exists.

We rejected a shared portable canvas (skia/wgpu): it would force surface handles
(GL/Metal) across the FFI and sacrifice native text quality. The render-state
model is kept behind a clean interface, so a shared renderer could be introduced
later without touching core's logic if the duplication ever justifies it.

### 3. Neovim is embedded as a child process over stdio

Core launches `nvim --embed` and communicates over its stdin/stdout via
msgpack-RPC. No sockets, no shared user Neovim instance.

### 4. Neovim owns the document model

There is no separate document representation in Wolf. Open is
`nvim.command("edit <path>")`; save is `"write"` / `"saveas"`. Because Markdown is
plain text, Neovim's buffer *is* the document model. The grid is a projection of
the visible viewport, never the whole file.

### 5. Scrolling belongs to Neovim; the scrollbar is synthesized

The grid is sized to the **visible viewport** only (resized via
`nvim_ui_try_resize` when the widget resizes). Scroll gestures are forwarded to
Neovim as input; Neovim scrolls itself and emits `grid_scroll`. A native
scrollbar's position and range are driven by the `win_viewport` event
(`topline`/`botline`/`curline`/`linecount`). The editor widget is **not** placed
in a scrolled-window that owns a tall child.

### 6. Syntax highlighting is whatever Neovim produces

The front-ends are semantically dumb: a cell carries an `hl_id`, which resolves
(via the `hl_attr_define` / `default_colors_set` table) to RGB + attributes, which
get painted. All highlighting — treesitter, LSP semantic tokens, diagnostics,
`Visual`/`Search` — is decided inside Neovim and arrives pre-resolved. To change
or extend highlighting you configure Neovim, never the renderer.

### 7. UI extensions: grid-drawn for v1

For v1 we enable only `ext_linegrid`. The command line, messages, and completion
menu render into the grid, terminal-style. This is the least work, is fully
functional, and is consistent across platforms.

Going native later (`ext_cmdline` + `ext_messages` + `ext_popupmenu`, rendered as
Adw/AppKit widgets) is an additive change: it touches the front-ends and adds
events to core's render-state model, but does not change the core architecture.

### 8. The captive Neovim is hermetic, pinned, and Markdown-only

Wolf ships its own Neovim, tuned for Markdown and nothing else (markdown +
markdown_inline treesitter grammars, a Markdown LSP such as `marksman`, an
optional formatter — and nothing else). It must not read the user's
`~/.config/nvim`, plugins, or shada.

Launch isolation (both platforms): `nvim --embed --clean -u <bundled init.lua>`,
with `NVIM_APPNAME` and the `XDG_*`/state dirs pinned away from the user's, and
shada/swap disabled. The `init.lua` lives **in this repo** and is referenced by
the build, so editor behavior is version-controlled alongside the code that
depends on it.

Delivery:

- **Linux (NixOS):** `wrapNeovim` / `neovimUtils.makeNeovimConfig` builds a single
  wrapped `nvim` with the plugin set baked into the runtimepath and the LSP/
  formatter binaries on its `PATH` (`makeWrapper --prefix PATH`). The whole closure
  ships together.
- **macOS:** a pinned Neovim plus its tools is bundled inside the `.app`. The GUI
  launches that absolute bundled path, never `nvim` from the user's `PATH`.

**Phase 1 status.** The Neovim binary is pinned and hermetic from day one, via
the flake devShell: core locates it only through `WOLF_NVIM_BIN` (never `PATH`,
never `/nix/store` scanning) and launches it `--embed --clean -u
runtime/nvim/init.lua` with `-i NONE -n`, pinning `NVIM_APPNAME=wolf` and the
`XDG_*` dirs under a private temp dir. Full `wrapNeovim` plugin baking
(markdown / markdown_inline treesitter grammars, the `marksman` LSP, a formatter)
is **deferred to a later phase**; Phase 1 runs the bare pinned binary with the
repo's `init.lua` and relies on Neovim's built-in markdown syntax highlighting.

## The core↔front-end contract

The boundary is intentionally narrow and FFI-shaped (decision 1). Two directions:

### Output: render state (core → front-end)

Core maintains a snapshot of everything needed to draw a frame:

- The cell grid (each cell: char(s) + `hl_id`).
- The highlight table: `hl_id → {fg, bg, special, bold, italic, underline…}`, plus
  default colors.
- Cursor: grid position + shape/mode (from cached `mode_info_set`, updated on
  `mode_change`).
- Viewport info for the scrollbar (`win_viewport`).

Core does not push pixels or per-event callbacks across the boundary. Instead it
applies `redraw` batches to the snapshot and, on `flush`, signals the front-end
that a new frame is ready (a "redraw needed" wake — see threading). The front-end
then reads the current snapshot on its UI thread and paints.

### Output: intents (core → front-end)

Things only the platform can do, requested abstractly: "show an open dialog,"
"show a save dialog for an unnamed buffer," "set the window title to X,"
"the buffer is modified / clean." The front-end fulfills these with native widgets
and feeds results back in as input.

### Input: events (front-end → core)

`key`, `mouse`, `scroll`, `resize(cols, rows)`, `open(path)`, `save`,
`save_as(path)`, and theme-color updates. Core translates these into Neovim RPC
calls. Key events are translated to Neovim notation (`<Esc>`, `<C-w>`, `<S-…>`,
`<CR>`, modifiers) on the way in.

## Threading and event-loop model

`nvim-rs` is async (tokio); the platform UIs run their own event loops (glib on
Linux, the AppKit/CFRunLoop on macOS). They must not block each other.

- Core owns a tokio runtime on its own thread(s). The `nvim-rs` handler receives
  `redraw` notifications there and updates the render-state snapshot (behind a
  lock or via an internal channel).
- On `flush`, core invokes a front-end-registered **wake callback**. The callback
  is responsible for marshalling onto the UI thread — `glib` idle/channel source
  on Linux, dispatch-to-main on macOS — and there triggering a paint
  (`queue_draw` / `setNeedsDisplay`) that reads the snapshot.
- Input flows the other way as plain calls into core, which hands them to the
  tokio side to issue RPC.

The wake mechanism is the one piece each front-end implements; the snapshot and
the RPC are entirely in core.

## Theming

To make native selection/colors match the OS, the front-end reads the live theme
(Adwaita named colors / accent on Linux; `NSColor`/appearance on macOS), converts
to RGB, and pushes it into Neovim's highlight groups via `nvim_set_hl` (e.g.
`Visual`, default fg/bg). It re-applies on theme/accent change. The renderer stays
dumb; Neovim re-emits `hl_attr_define` and the grid repaints.

## Build

See [Building Wolf](build.md). In brief: `wolf-core` + `wolf-gnome` build with
Cargo under Nix on NixOS; `wolf-macos` builds with Xcode/SwiftPM, linking
`wolf-core` via swift-bridge-generated bindings, with the pinned Neovim bundled
into the `.app`.

## Deferred / open

- Native command line, messages, and completion (decision 7) — when, and exactly
  which Adw/AppKit widgets.
- Whether a shared portable renderer (decision 2) ever earns its keep.
- macOS Neovim update/pinning workflow (how the bundled build is produced and
  refreshed).
