# Architecture

Wolf is a Markdown editor that embeds Neovim as its editing engine. The GUI is a front-end for Neovim, not an editor in its own right: Neovim owns all editor state — buffer, cursor, modes, syntax, undo, file I/O — and Wolf renders what Neovim tells it to render and forwards input back.

This document is the cross-crate view: what the components are, what crosses the boundaries between them, and the decisions that only make sense at the system level. Each crate's own constraints live in its `DECISIONS.md`, linked below, and are not repeated here.

## Guiding principle

There is one hard rule everything else follows from:

> `wolf-core` must compile with no GUI toolkit in its dependency tree.

All the Neovim-specific work — and it is the large majority of the work — lives in `wolf-core` and is shared verbatim across platforms. Each platform front-end is a thin shell: native window chrome, native file dialogs, a paint backend, and OS-event translation. If a type from GTK or AppKit ever appears in core, the boundary has leaked.

The rule is checked rather than assumed. Both verification commands, and the list of dependencies chosen because they are toolkit-free, are in [`crates/wolf-core/DECISIONS.md`](crates/wolf-core/DECISIONS.md).

## Components

| Crate                                       | Role                                                                                                                                                     | Built with            |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------- |
| [`wolf-core`](crates/wolf-core/README.md)   | Everything Neovim-related, drawing no pixels. Spawns the captive `nvim --embed`, folds `redraw` into a snapshot, accepts abstract input, signals a wake. | Cargo and Bazel       |
| [`wolf-ffi`](crates/wolf-ffi/README.md)     | The swift-bridge surface over core, and the only crate swift-bridge touches.                                                                             | Bazel                 |
| [`wolf-macos`](crates/wolf-macos/README.md) | The AppKit front-end, over the bridge.                                                                                                                   | Bazel                 |
| [`wolf-gnome`](crates/wolf-gnome/README.md) | The GTK 4 front-end, consuming core as an ordinary crate dependency.                                                                                     | Cargo, Linux only     |
| [`runtime/nvim`](runtime/nvim/README.md)    | The captive Neovim configuration, and the `_G.wolf_*` RPC contract core drives it through.                                                               | Bundled as a resource |

Both front-ends do the same four things: paint the render state core produces, provide native chrome and dialogs, translate OS input into core's abstract input events, and bridge their native event loop to core's async runtime. They differ in how they reach core — see decision 1 — and in what they have built, which each crate's README states.

`wolf-gnome` has no Bazel target at all. That absence is deliberate and load-bearing: it is what keeps the GTK stack out of `wolf-core`'s Bazel dependency graph, so the guiding principle holds under the macOS build as well as under `cargo tree`.

## Architectural decisions

### 1. Core↔front-end boundary: swift-bridge on macOS, direct crate on Linux

Swift cannot import a Rust crate, so the macOS side needs a generated binding regardless. Wolf generates it with [`swift-bridge`](https://github.com/chinedufn/swift-bridge). Linux and GTK, being Rust, link `wolf-core` directly with no FFI at all.

The annotated bridge module lives in [`wolf-ffi`](crates/wolf-ffi/README.md) and never in core. That containment is what keeps core's GUI-free rule enforceable, and it is the bridge crate's first decision — see [`crates/wolf-ffi/DECISIONS.md`](crates/wolf-ffi/DECISIONS.md).

The design constraint that follows: everything crossing to Swift has to be expressible over swift-bridge — plain structs, slices, opaque handles, and registered callbacks, with no Rust-only generics or trait objects on the line. `wolf-ffi` adapts core's API into that shape rather than distorting core to fit, and the GTK front-end consumes that same core API directly. The result is one front-end-facing surface rather than two divergent ones.

A uniform C ABI on both platforms is rejected. It would cost Linux its type safety and async ergonomics to buy a symmetry Wolf does not need, since the boundary is defined once in Rust either way.

### 2. Rendering: native paint per platform

The render-state model is toolkit-agnostic, and each front-end paints it natively: Cairo and Pango under GTK, CoreText and CoreGraphics under AppKit. This buys the best text rendering each OS offers, at the cost of writing the cell-painting loop twice — which is cheap once the render-state model exists.

The corollary is itself a contract: what to paint is a front-end decision and is not part of the core↔front-end contract. Core always publishes the full render state for every grid, and a front-end may decline to paint any of it. Core is never told, and the render state does not change.

A skip has to be provably invisible. macOS is the one exercise of this: a pane whose text grid is fully covered by an opaque Markdown-preview web view paints no grid and does not read that grid's frame across the FFI, decided by the zero-tolerance containment seam in [`PaneOcclusion.swift`](crates/wolf-macos/Sources/WolfKeyMap/README.md).

A shared portable canvas such as Skia or wgpu is rejected: it would force surface handles across the FFI and give up native text quality. The render-state model sits behind a clean interface, so a shared renderer could still be introduced later without touching core's logic.

### 3. Neovim is embedded as a child process over stdio

Core launches `nvim --embed` and speaks msgpack-RPC over its stdin and stdout. There are no sockets, and Wolf never attaches to a shared user Neovim instance. The launch flags, the isolation, and the supervision that owning a child process implies are core's own constraints — see [`crates/wolf-core/DECISIONS.md`](crates/wolf-core/DECISIONS.md).

### 4. Neovim owns the document model

Wolf has no document representation of its own. Opening a file is `edit <path>` and saving is `write` or `saveas`; because Markdown is plain text, Neovim's buffer is the document model. The grid is a projection of the visible viewport, never of the whole file.

### 5. Scrolling belongs to Neovim; the scrollbar is synthesized

The grid is sized to the visible viewport only, resized through `nvim_ui_try_resize` when the widget resizes. Scroll gestures are forwarded to Neovim as ordinary input; Neovim scrolls itself and emits `grid_scroll`. The editor widget is never placed inside a scrolled window that owns a tall child.

A scrollbar is therefore a projection, not a control. Its position and range come from the `win_viewport` event (`topline`, `botline`, `curline`, `linecount` — 0-based buffer line indices, with `botline` one past the last visible line). Whether a front-end draws one at all is its own decision, per decision 2.

### 6. Syntax highlighting is whatever Neovim produces

The front-ends are semantically dumb. A cell carries an `hl_id`, which resolves through the `hl_attr_define` and `default_colors_set` table into RGB plus attributes, which get painted. Treesitter, LSP semantic tokens, diagnostics, `Visual`, and `Search` are all decided inside Neovim and arrive pre-resolved. To change or extend highlighting you configure Neovim, never the renderer.

### 7. UI extensions: native command line / messages / completion (with `ext_multigrid` for native panes)

Core attaches with `ext_linegrid`, `ext_multigrid`, `ext_popupmenu`, `ext_messages`, and `rgb`. `ext_messages` implicitly enables `ext_cmdline` and forces `cmdheight=0`, so the outer grid 1 carries no command line: Neovim routes the command line, the completion menu, and messages off the grid into the `cmdline_*`, `popupmenu_*`, and `msg_*` events. Core folds those into `Snapshot.cmdline`, `Snapshot.pum`, and `Snapshot.toast` — see "The core↔front-end contract".

`ext_multigrid` moves window content onto per-window grids (id 2 and up), which is what lets a shell own a native split-pane layout instead of receiving Neovim's own splits drawn into a single grid.

How the three surfaces are presented is a front-end decision, per decision 2: each paints them from the snapshot fields with whatever native widget suits it. The captive `init.lua` sets `wildoptions+=pum`, so cmdline completion arrives as a vertical popup rather than the horizontal wildmenu.

### 8. The captive Neovim is hermetic, pinned, and Markdown-only

Wolf ships its own Neovim, tuned for Markdown. It must not read the user's `~/.config/nvim`, plugins, or shada, and core locates the binary only through `WOLF_NVIM_BIN`, never through `PATH`. The isolation flags and the private state directories that enforce this belong to core — see [`crates/wolf-core/DECISIONS.md`](crates/wolf-core/DECISIONS.md).

The captive `init.lua` lives in this repo at `runtime/nvim/init.lua` and is referenced by the build, so editor behavior is version-controlled alongside the code that depends on it. Swap files are deliberately left enabled, because they are what recovers unsaved edits after a crash restart.

Each platform pins the binary its own way: the Nix devShell on Linux, and a sha256-pinned Neovim tree bundled into the `.app` on macOS, alongside the vendored plugin, colorscheme, treesitter parsers, and fonts. [Building Wolf](build.md) holds the pinned versions and how each asset is downloaded and assembled.

### 9. The macOS UI: AppKit, Swift, Tahoe-only, all-Bazel — locked

The macOS front-end's foundations are fixed, and they are load-bearing rather than incidental: Tahoe (macOS 26) and newer only, AppKit rather than SwiftUI, Swift built with Xcode 26 or newer, and a Bazel build that is authoritative with the generated Xcode project used for debugging alone. Each of them, with its reason, is in [`crates/wolf-macos/DECISIONS.md`](crates/wolf-macos/DECISIONS.md).

The workspace-level half of that lock is the resolved `rules_swift`, `rules_apple`, `rules_xcodeproj`, and `apple_support` pins in `MODULE.bazel` — the highest releases that declare Bazel 8 compatibility and support Xcode 26 and the Tahoe SDK. Those live with the build they constrain, in [Building Wolf](build.md).

### 10. Commands: one canonical id → two faces; the captive nvim stays hermetic

A command is a no-parameter, no-return user action, invocable from inside Neovim, from the native menu, and from the command palette. It is user-triggered only: there is no emit-on-attach, so a freshly attached core has an empty command queue.

One canonical snake_case id derives both faces. `wolf-core`'s `COMMAND_SPECS` maps each id to a PascalCase Ex-command such as `:WolfCloseWindow` (Neovim forces the uppercase first letter) and to an idiomatic Lua function `require('wolf').close_window()`. The mapping is stored explicitly as data rather than computed by a transform, so the registry is the single source of truth.

The registry stays canonical in core, and the front-end maps a discriminant to an action. Only the `command_kind` byte crosses the FFI — the front-end never learns the ids — and the `Command` enum's variant order is that byte, append-only for the reason every discriminant table here is. See [`crates/wolf-ffi/DECISIONS.md`](crates/wolf-ffi/DECISIONS.md), with the table itself in [`crates/wolf-ffi/src/README.md`](crates/wolf-ffi/src/README.md).

The captive Neovim stays hermetic, and that shapes what a command can be. The `wolf` Lua module and the `:Wolf*` Ex-commands exist for Wolf's own bundled configuration, not as a user keybinding hook; no user-config source path is added. Because `--clean` means no clipboard integration, the front-end owns `NSPasteboard`: Copy yanks the visual-mode selection over a blocking RPC so the shell can write it out, and Paste inserts through `nvim_paste`.

Not every user-visible menu item is a command. A path-parameterized action such as Open Recent does not fit the no-argument model and is not a `Command` at all: macOS realizes it as a native dynamic submenu fed by a data accessor over the FFI, and `wolf-gnome` as a command-palette row carrying the path. A standard AppKit item such as Enter Full Screen carries no discriminant at all.

See [Command lifecycle](command-lifecycle.md) for the per-command inventory, the invocation-to-dispatch path, the three command flavors, and the checklist for adding one.

## The core↔front-end contract

The boundary is narrow and FFI-shaped (decision 1): only plain data crosses it, and no callback crosses it mid-stream. Everything below is delivered in one of three shapes.

- **A snapshot read under a wake.** Core folds state into a lock-guarded value and fires the front-end's wake callback; the front-end reads it on its UI thread and paints. Render state and the streaming sessions all use this shape.
- **A drainable queue.** Core enqueues discrete requests — layout intents, commands — in arrival order, and the front-end drains them on each wake.
- **A blocking query.** A few calls need an answer from Neovim before the front-end can act. Each returns a neutral fallback (`""`, `0`, `None`, an empty `Vec`) on any error or timeout, so no error channel crosses the boundary and a wedged nvim is indistinguishable from an empty answer.

One rule governs every wake: only a redraw `flush` bumps `Snapshot.seq`. The out-of-band `wolf_*` notifications fire the same wake without bumping it, so a front-end that gates its repaint on `seq` alone misses every cwd, buffer, theme and preview update ([`redraw.rs`](crates/wolf-core/src/README.md#redrawrs)).

The FFI-level shape of each contract — the Swift-facing signatures, the opaque handle types, and the discriminant tables that are the ABI — is in [`crates/wolf-ffi/src/README.md`](crates/wolf-ffi/src/README.md). What follows is the language-neutral form both front-ends consume.

### Output: render state (core → front-end)

`Snapshot` is a complete, paint-ready frame. Its painting fields:

- `grids: HashMap<u64, Grid>` — one cell grid per Neovim grid id, row-major; each `Cell` is one grapheme cluster plus an `hl_id`. Editor window content lives on grids `>= 2`; grid 1 is the outer grid.
- `highlights: HighlightTable` — the `hl_id` table plus the editor defaults. `resolve` applies defaults and reverse video, so a front-end paints `fg`/`bg` verbatim.
- `cursor: Cursor` — `grid`, `row`, `col`, and `mode_idx` into `mode_infos: Vec<ModeInfo>` (`name`, `shape`, `cell_percentage`, `attr_id`).
- `windows: HashMap<u64, WinInfo>` — keyed by grid id: the window handle `win`, `start_row`/`start_col`/`width`/`height` in outer-grid cells, and a `viewport` (`topline`, `botline`, `curline`, `linecount`) for a synthesized scrollbar.
- `seq: u64` — the frame counter.

The native widget surfaces are folded inside the redraw batch, so they advance `seq` with it:

- `cmdline: Cmdline` — `active`, `firstc`, `prompt`, `content` chunks, `pos` (a byte offset), `indent`, `level`.
- `pum: Pum` — `visible`, `items` (`word`/`kind`/`menu`/`info`), `selected`, and the anchor `grid`/`row`/`col`, where `grid == -1` anchors to the command line.
- `toast: Toast` — `visible` plus `messages`, each a `kind`, an `id` and styled `lines`.

The remaining facets arrive out-of-band, each from its own `wolf_*` notification, each firing the wake without bumping `seq`:

- `current_file: Option<PathBuf>`, `current_filetype: String`, `current_is_preview: bool`, `current_is_full_width: bool` — the active buffer, all four from one notification. `is_markdown_active`, `is_preview_active` and `is_full_width_active` read them as command gates.
- `active_win: Option<i64>`, `current_tabpage: i64` — the focused window and tabpage.
- `buffers: HashMap<i64, PathBuf>`, `win_buffers: HashMap<i64, WinBuffer>` — per-window buffer state keyed by window handle. `buffers` omits unnamed windows; the parallel `win_buffers` keeps them and adds `bufnr` plus the two terminal flags.
- `full_width: HashMap<i64, bool>`, `previews: HashMap<i64, PreviewInfo>` — per-window, keyed by window handle.
- `markdown_ticks: HashMap<i64, u64>` — a `changedtick` per source buffer number.
- `tab_cwds: HashMap<i64, PathBuf>`, `open_files: HashMap<i64, TabFiles>` — per tabpage handle.
- `theme: ThemeColors` — the optional `WolfCursor` / `WolfTitlebar` colors (see "Theming"); every channel is `Option`.

Two rules bind a consumer of those maps. Each is replaced wholesale on its notification — `markdown_ticks` alone upserts — so a closed window or tabpage vanishes rather than going stale. Neovim renders only its current tabpage, so `windows` and every per-window map describe that tabpage alone; a non-current view's working directory comes from `tab_cwds` through `cwd_for_tabpage`.

Field-level detail — why `Grid::resize` preserves rather than clears, why `WinInfo.hidden` is inert, why a toast keeps its colors while the command line does not — is in [`render.rs`](crates/wolf-core/src/README.md#renderrs).

### Output: file-search session (core → front-end)

`start_file_search(wake)` returns a `FileSearch` whose `read()` yields an owned guard over `entries: Vec<FileEntry>`, `complete`, `truncated`, and a monotonic `version`. The recent-files seed is present synchronously; a background walk streams the rest in, bumping `version` and firing the same wake after each batch. Dropping or cancelling the handle stops the traversal. A `FileEntry` carries an absolute `path`, a root-relative `display`, and an optional `mru_rank`.

Ranking belongs to core, not the front-end: `rank_matches` decides which entries match a query and in what order ([`rank.rs`](crates/wolf-core/src/README.md#rankrs)), so both front-ends and the tests agree on the result. Selection feeds back as an `Open` input event, which core also records in the per-project recent-files cache.

Content search and the sidebar's file tree use this same session shape over their own wake channels, and a new streaming capability should adopt it rather than inventing another — see [the snapshot-and-wake session pattern](crates/wolf-core/DECISIONS.md#the-snapshot-and-wake-session-pattern). Both traversals root at Neovim's live working directory read from a tracked cell rather than an RPC, so opening a picker never blocks on a wedged nvim.

### Output: intents (core → front-end)

An intent is something only the platform can do, requested abstractly. Layout is the realized channel: core parses the captive nvim's `wolf_layout` notifications into `LayoutIntent::Split { orientation, from, win }`, `FocusDir { dir, from }`, and `DrawerOpen { win }`, queued for `drain_intents()`. The shell owns the resulting pane geometry and binds each pane to a Neovim window and its grid.

The asymmetry between the three is the load-bearing part. A split and a drawer-open are performed by nvim and also reported, so the window and its grid genuinely exist by the time the shell reacts; a focus move is a pure intent that nvim does not act on, leaving the shell to resolve the neighbour in its own layout; a close has no intent in either direction and is observed through the `win_close` redraw event. Reversing any of the three breaks the side that depends on the other behaviour ([`layout.rs`](crates/wolf-core/src/README.md#layoutrs)).

The non-layout intents — open and save dialogs, the window title, modified-versus-clean state — are designed and deferred.

Rebuilding a saved layout runs the other way and is a three-party contract rather than a two-party one. A front-end hands core a `PaneNode`, core renders it to a JSON document, and `init.lua` decodes that document and recreates the windows, returning their handles in the same depth-first order the leaves were emitted in. The document deliberately differs from `PaneNode`'s own serde shape, so the renderer, the Lua decoder, and the independent Swift renderer on macOS must all agree; changing it on one side silently breaks the others.

### Output: commands (core → front-end)

A `Command` is a no-parameter, no-return user action, and its registry is canonical in core: one `CommandSpec { command, id, ex_command, title, menu }` per row of `COMMAND_SPECS`. A single snake_case id derives both faces — the Ex-command `:Wolf<Pascal>` and the Lua function `require('wolf').<snake>()` — and invoking either `rpcnotify`s `wolf_command`, which core folds onto the queue behind `drain_commands()` (decision 10). Commands are user-triggered only: nothing is emitted on attach, so a freshly attached core drains empty.

The `Command` variant order is the FFI `command_kind` discriminant and is append-only; the value table lives in [`crates/wolf-ffi/src/README.md`](crates/wolf-ffi/src/README.md#discriminant-tables). Only that discriminant crosses the FFI, so a native menu keeps its own item list while the command palette reads titles and groups from the registry through its own session.

Where a command lands is a front-end decision, and there are three answers: entirely in the shell (the font-size and sidebar commands issue no `InputEvent` at all), in a shell overlay (the pickers and the palette), or back into Neovim as an input event (Terminal, Use full width for content, Close Pane). [Command lifecycle](command-lifecycle.md) walks one end to end and lists what adding a command touches.

### Output: preview (core → front-end)

A preview pane is a real Neovim scratch buffer carrying `b:wolf_preview_source`, so the shell invents no document type of its own. Core reports one `PreviewInfo { source_bufnr, source_path, source_full_width }` per preview window in `Snapshot.previews`, and each source buffer's `changedtick` in `Snapshot.markdown_ticks`. The front-end joins the two by `source_bufnr`: when a tick advances it re-pulls through the blocking `render_preview(source_bufnr)`, which returns a complete HTML document with CSS and fonts inlined. Because the in-app render pins one theme and full width, that document's `<head>` is byte-identical on every render, including across a dark/light flip, so a front-end may replace only the body.

Placement is the front-end's choice, passed as a mode string. `open_preview(mode)` accepts `split_right`, `active`, or `reuse_preview_pane`; its inverse `edit_source(mode)` accepts `split_left`, `active`, or `reuse_editor_pane`. Both return the resulting buffer number, or `0`, and the front-end learns the new pane on the next wake rather than from the return value. `preview_file(win, path)` repoints an existing preview window at a linked file, and `open_in_browser()` returns a loopback URL for the same rendered document (decision 10).

Two gotchas fall out of the scratch-buffer design. The path-based trackers skip preview scratch buffers, so a scratch name never surfaces as a real path in `current_file` or `buffers`. A preview scratch reports its source buffer's flags rather than its own, which is what keeps a menu gate or checkmark tracking the document instead of the pane.

**Content width.** A buffer's content area is capped near 90 columns and centered by default, in both the editor and its preview, and the Use full width for content command flips the buffer-local `b:wolf_full_width` for that one buffer. There is no global default-to-full-width setting; the per-buffer toggle is the only control.

The contract is three fields and one event: `full_width` keyed by window handle, `PreviewInfo.source_full_width` for a preview pane, `current_is_full_width` for the menu checkmark, and `InputEvent::ToggleFullWidth`. Framing a pane to that flag is the front-end's job — see [`crates/wolf-macos/DECISIONS.md`](crates/wolf-macos/DECISIONS.md) and [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md). The terminal drawer defaults to full width because nvim seeds the flag on its buffer, and the browser preview is always capped and carries no toggle.

**Cursor position.** A third out-of-band notification closes the loop from the editor cursor to the preview's scroll position. `wolf_markdown_cursor` carries three flat scalars `(source_bufnr, line, col)`, which core upserts into `Snapshot.markdown_cursors` keyed by the source buffer number — the same key space as `markdown_ticks` and `PreviewInfo.source_bufnr`, so a front-end joins all three on one key. An absent entry, or a `line` of `0`, is the cursor-unknown sentinel, and a front-end must not scroll for it.

The column semantics are the contract: `line` is a 1-based hard buffer line and `col` a 0-based character column, not a byte offset, so nothing on the path converts. They pair with the `wl-{L}-{C}` position markers `render_html` emits into the preview HTML. Whether a front-end scrolls at all is its own decision, per decision 2.

### Input: events (front-end → core)

The front-end pushes `InputEvent`s onto an ordered FIFO channel that a single consumer task translates into Neovim RPC. Delivery is fire-and-forget: the channel keeps keystrokes in order and fuses a run of consecutive `Key` events into one `nvim_input` call, so a burst produces one redraw flush.

- Input proper — `Key`, `Mouse`, `Scroll`. A mouse or scroll event carries its target grid with grid-local row and column, because under `ext_multigrid` each pane has its own window grid.
- Geometry — `Resize { cols, rows }` for the outer UI, `ResizeGrid { grid, cols, rows }` per pane, and `FocusWin { win }` for click-to-focus and resolved focus moves.
- Documents — `Open { path }`, `OpenAt { path, line, col }`, `Save`, `SaveAs { path }`, `NewBuffer`, `Paste { text }`, `SwitchBuffer { win, bufnr }`, `ClosePane`.
- Views — `Chdir { path }`, `SetTabpage { handle }`, `CloseTabpage { handle }`. Each view is one nvim tabpage owning its own working directory, so `Chdir` applies `:tcd`; a global `:cd` would move every view of the shared nvim.
- Toggles — `SetBackground { dark }`, `ToggleTerminal`, `ToggleFullWidth`, `SetUiFocus { gained }`.

Key translation happens in core, not the front-end: `key_to_notation` is the single source of Neovim notation, and a front-end delivers the already-shifted glyph for a printable character. A `Key`, `Mouse` or `Scroll` also dismisses a showing message toast unless that toast is a `confirm` prompt; the exemption for a scroll gesture landing over the toast lives in the front-end, which never forwards that gesture. Several variants delegate to helpers in `runtime/nvim/init.lua`, so their behaviour is defined there rather than in core ([`input.rs`](crates/wolf-core/src/README.md#inputrs)).

## Threading and event-loop model

`nvim-rs` is async on tokio; the platform UIs run their own event loops — glib on Linux, the AppKit run loop on macOS. Neither may block the other.

Core owns a tokio runtime on its own threads and updates the render-state snapshot there. On `flush` it invokes a front-end-registered wake callback. The callback marshals onto the UI thread — a glib idle source on Linux, dispatch-to-main on macOS — and triggers a paint that reads the snapshot. Input flows the other way as plain calls into core, which hands them to the tokio side to issue RPC.

The wake is the one piece each front-end implements. The snapshot and the RPC are entirely in core. Core's locking discipline around that snapshot is in [`crates/wolf-core/DECISIONS.md`](crates/wolf-core/DECISIONS.md).

On macOS the wake is routed rather than global, because one captive Neovim is shared by several windows. Each wake carries a per-project token that the front-end resolves to the owning project before fanning out. The mechanism, and why identity crosses the boundary as a plain scalar, is in [`crates/wolf-ffi/DECISIONS.md`](crates/wolf-ffi/DECISIONS.md); how a project fans a wake out to its views is in [`crates/wolf-macos/DECISIONS.md`](crates/wolf-macos/DECISIONS.md).

## Theming

Theming runs in both directions, and both directions go through Neovim's highlight groups rather than through a parallel styling channel. Both are realized in full by the macOS front-end alone. `wolf-gnome` pushes nothing into Neovim and reads no OS theme — a configured `appearance` other than `auto` is reported once at launch there and then ignored — while the `WolfCursor` group does reach its cursor paint; the limitation is recorded in [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md).

**Front-end to Neovim.** The macOS front-end reads the live OS theme — `NSColor` and appearance — converts to RGB, and pushes it into Neovim's highlight groups with `nvim_set_hl`. It re-applies on theme or accent change. The renderer stays dumb: Neovim re-emits `hl_attr_define` and the grid repaints.

**Neovim to app chrome.** A colorscheme drives two app-level colors through the optional highlight groups `WolfCursor` (background is the cursor fill, foreground the glyph color) and `WolfTitlebar` (background is the titlebar). These are never painted on the grid, so the redraw stream never carries them. Core installs a `ColorScheme` autocmd that notifies `wolf_theme` with the groups' effective colors, folded into the snapshot's theme field. An unset `WolfCursor` falls back to the reverse-video logic; an unset `WolfTitlebar` keeps the system titlebar. The vendored lighthouse colorscheme defines both.

**OS appearance to Neovim.** The user-level `appearance` config key (`auto`, `dark`, `light`; default `auto`) is read in core's GUI-free config module. The macOS front-end resolves it at launch and pushes the result as `:set background=dark|light`, which re-sources the colorscheme, so the existing theme notification repaints the grid and the chrome with no new surface. In `auto` mode the front-end also observes live OS appearance changes and re-pushes; `dark` and `light` force the value and also set the app's own appearance so the chrome matches. The config value is read at startup, so only `auto`'s response is live.

## Build

See [Building Wolf](build.md) for every command and workflow. In outline: on Linux, `wolf-core` and `wolf-gnome` build with Cargo under Nix. On macOS, `wolf-core`, `wolf-ffi` and the `wolf-macos` app all build with Bazel, with `rules_rust` and `crate_universe` consuming the same `Cargo.toml` and `Cargo.lock`.

One cross-cutting choice worth stating here: the C and link toolchain uses the host Xcode SDK rather than the older SDK inside Nix. That matters because `rusqlite` compiles a bundled SQLite through `cc`, and the two SDKs are not interchangeable for it.

The `.app` bundles its own pinned Neovim, the captive `init.lua`, the vendored plugin and colorscheme trees, and both font families, so a packaged app runs with no shell environment. The inventory and what consumes each item is in [`crates/wolf-macos/README.md`](crates/wolf-macos/README.md).

## Deferred and open

- The GTK native completion popup and multi-line scrollable toast. Both exist on macOS.
- Whether a shared portable renderer, rather than a native paint backend per platform, ever earns its keep. See decision 2.
- **Marker granularity below the block.** A source line longer than `MARKER_COLUMN_STRIDE` is anchored inside its block, at the marker's true position in the rendered inline flow. A paragraph hard-wrapped into several shorter source lines is not: all of its `wl-{L}-0` markers resolve to the block's opening tag. Injection is suppressed entirely in a fenced code block, an HTML block and a table, which anchor to their opening tag, and in an image's alt text, which anchors just after the image. Generalizing is rejected rather than pending — an anchor between `<tbody>` and `<tr>` is foster-parented out of the table by the HTML parser, and one between `<ul>` and `<li>` is invalid.
- **Preview scroll-sync is macOS-only.** The GTK shell has no Markdown preview pane, and the browser preview reloads on a poll with no cursor coupling.
- **One preview theme ships.** The `PreviewTheme` enum, `theme_css`, and the embedded stylesheets are structured for more, with a config key as the intended selector. The preview follows the app's resolved light or dark appearance through a `prefers-color-scheme` block, and is not wired to the live Neovim colorscheme.

### Split panes — present limitations

These are scoped deliberately, and each has a clear direction.

- **Only windows opened through the overridden split commands become panes.** Panes come from `Split` intents, which only the rebound chords and Ex-commands emit. A window created another way — `:help`, quickfix, a location list, a preview window, `:diffsplit` — still gets a grid and a `WinInfo` but no pane, so its content is not shown. The direction is to auto-pane any unbound window grid.
- **Floating and external windows are ignored**, so they never appear as panes.

The macOS project-and-tabpage model adds two more, both detailed in [`crates/wolf-macos/DECISIONS.md`](crates/wolf-macos/DECISIONS.md): the lockout on Neovim-internal tabpage navigation is best-effort and a determined user can still desync a view from its tabpage, and a visible non-current window of a project can blank during a live resize until refocused.
