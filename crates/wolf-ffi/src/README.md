# wolf-ffi/src

`lib.rs` is the crate root: one `#[swift_bridge::bridge] mod ffi` plus the hand-written Rust glue behind it. It builds as a `staticlib` (the archive the `.app` links through the generated Swift) and as an `rlib`, so the Rust integration tests link and drive the same surface headlessly with no Swift runtime present. `bin/gen_bindings.rs` regenerates the committed bindings.

Read [`../DECISIONS.md`](../DECISIONS.md) first. It holds the durable constraints behind this file — the registered C function pointer that carries the wake, the per-project wake tokens, the four wake channels, the append-only discriminant rule, and the generated-bindings workflow — and none of that is repeated here.

**Shapes that repeat across the whole surface.** Knowing these makes the entries below read as signatures rather than as a list of exceptions.

- Anything returned inside a `Vec`, or carrying an owned `String`, is an opaque `extern "Rust"` type read back through accessor methods. swift-bridge emits `Vec` support only for opaque types, and a transparent `swift_repr = "struct"` cannot hold a `String`. `FfiColor`, `FfiKeyEvent`, `FfiMouseEvent`, and `FfiScrollEvent` are the only transparent structs; they are re-exported at the crate root so rlib consumers can name them.
- Every entry point is infallible across the boundary. A shut-down core yields an empty-backed value — a `0x0` frame, an empty `Vec`, `""`, `false`, `0` — instead of an error, so the Swift side needs no error channel.
- Colors reach Swift already resolved in Rust against the snapshot's `HighlightTable`. Swift never re-resolves a highlight id.
- A session handle (`FfiFileSearch`, `FfiSearch`, `FfiCommandPalette`, `FfiBufferPicker`) is main-thread-only and uses a `RefCell` for interior mutability behind the bridge's `&self`. Dropping an `FfiSearch` cancels its ripgrep subprocess.
- The bridge module carries a scoped `#[allow(clippy::all, missing_docs, unused)]`, and the crate carries `#![allow(clippy::unnecessary_cast)]` because the macro expansion emits same-type raw-pointer casts into sibling items that a module-scoped allow cannot reach.

## The bridge surface

### Lifecycle

- `wolf_ffi_start(init_lua: String, cwd: String, cols: u16, rows: u16) -> Option<WolfCoreHandle>` — `init_lua` and `cwd` are filesystem paths. Logs and returns `None` on any startup error. Mints this core's wake token.
- `WolfCoreHandle` — Opaque and main-thread-only: `WolfCore` is not `Send`. Its `RefCell<Option<WolfCore>>` lets `shutdown` take the core by value while the bridge only ever hands Swift a `&self`.
- `wake_token(&self) -> u64` — Routing identity for this core's wakes. Tokens start at 1, leaving 0 free as the front-end's "no core" sentinel.
- `shutdown(&self)` — Takes the core out of the handle and drops it; every method afterwards returns its empty-backed value.
- `set_ui_focus(&self, gained: bool)` — `nvim_ui_set_focus`. Both edges are reported so each refocus is a real transition, which is what makes `FocusGained` fire the autoread `:checktime`.

The four `extern "C"` wake registrations (`wolf_ffi_set_wake`, `wolf_ffi_set_filesearch_wake`, `wolf_ffi_set_tree_wake`, `wolf_ffi_set_search_wake`) live in `generated/wolf_ffi_wake.h` rather than in `mod ffi`; see [`../DECISIONS.md`](../DECISIONS.md).

### Liveness and restart

- `liveness(&self) -> u8` — `mod liveness`. A lock-free snapshot read, no RPC, so the front-end can call it on every wake to drive the crash and stall overlay.
- `restart(&self) -> bool` — Force-kills and respawns the captive nvim in place, re-running the setup chain. Capture each view's `open_files_for_tabpage` before calling: a respawn wipes the mirror.
- `open_files_for_tabpage(&self, tab: i64) -> FfiOpenFiles` — The Tier-1 open-file mirror; a snapshot read, no RPC. Only `active_path` is per-tabpage; `files` is not (below).
- `FfiOpenFiles::active_path(&self) -> String` — `""` for an unnamed buffer.
- `FfiOpenFiles::files(&self) -> Vec<String>` — Absolute paths. Reopen the active one last so it lands focused. This is the instance-global buflisted set, so a multi-view project over-reopens.

### Render and frame

- `read_frame(&self) -> FfiFrame` — The active grid.
- `read_frame_for_grid(&self, grid: u64) -> FfiFrame` — One per pane, plus grid 1 for the command-line strip. An absent grid yields a `0x0` frame, which Swift reads as "grid gone".
- `resize(&self, cols: u16, rows: u16)` — The outer UI size.
- `resize_grid(&self, grid: u64, cols: u16, rows: u16)` — One window grid, in cells. nvim echoes the authoritative size back via `grid_resize`.

`FfiFrame` accessors:

- `cols() -> u16`, `rows() -> u16` — Both 0 marks an absent grid.
- `cursor_row() -> u16`, `cursor_col() -> u16` — Grid-local.
- `cursor_shape() -> u8` — `mod cursor_shape`.
- `cursor_cell_percentage() -> u32` — The mode's cursor width as a percentage of the cell.
- `cursor_visible() -> bool` — Per-grid: true only when the cursor sits on this frame's grid.
- `grid_id() -> u64` — The grid this frame was packed from.
- `cursor_grid() -> u64` — The grid the cursor lives on at pack time, so a pane can tell a foreign cursor from its own.
- `default_fg() -> FfiColor`, `default_bg() -> FfiColor` — The theme defaults.
- `cursor_fill() -> FfiColor`, `cursor_glyph() -> FfiColor` — Reverse-video cursor colors, honoring the `WolfCursor` theme group first. Two scalar accessors rather than a `Vec<FfiColor>`, which swift-bridge cannot emit for a transparent struct.
- `cells() -> Vec<FfiCell>` — Flat and row-major, `cols * rows` entries.
- `topline()`, `botline()`, `curline()`, `linecount() -> u64` — The viewport, as nvim's `win_viewport` reports it; `botline` is one past the last visible line. All 0 when the grid has no window.

`FfiCell` accessors:

- `text() -> String` — One grapheme cluster; empty for a wide-char trailing placeholder or a never-painted cell.
- `fg()`, `bg()`, `special() -> FfiColor` — Already resolved; `special` is the underline color.
- `flags() -> u8` — The `mod flag` bitset.

`FfiColor { rgb: u32, set: bool }` is a packed `0x00RRGGBB` plus an is-set flag, standing in for `Option<Rgb>` across a boundary with no `Option`. `FfiFrame::from_snapshot` and `from_snapshot_grid` are `pub` on the rlib so the headless `tests/frame_pack.rs` can pack a snapshot with no live core behind it.

### Input

- `send_key(&self, ev: FfiKeyEvent)` — Mapped into a `wolf_core::KeyEvent` here, so nvim notation stays core's business.
- `send_mouse(&self, ev: FfiMouseEvent)` — `nvim_input_mouse` against `ev.grid`. An unrecognized button or action discriminant drops the event silently.
- `send_scroll(&self, ev: FfiScrollEvent)` — `nvim_input_mouse("wheel", …)`. Same drop-on-unknown-discriminant contract.
- `FfiKeyEvent { kind, ch, fnum, ctrl, shift, alt, meta }` — Transparent. `kind` per `mod kind`; for a char key, `ch` is the already-shifted Unicode scalar, and Swift did the shifting.
- `FfiMouseEvent { button, action, grid, row, col, ctrl, shift, alt }` — Transparent. `grid` is the target window grid under `ext_multigrid` and `row`/`col` are grid-local.
- `FfiScrollEvent { direction, grid, row, col }` — Transparent. Same grid-local convention.
- `copy_selection(&self) -> String` — The visual selection yanked to text. Blocks briefly; `""` when not in visual mode or the selection is empty, and Swift writes the pasteboard only for a non-empty result.
- `paste(&self, text: String)` — `nvim_paste`.
- `is_visual_mode(&self) -> bool` — Gates the Neovim source of Edit > Copy; a selection in a Markdown preview enables that item without core's involvement.

### Panes and windows

- `drain_intents(&self) -> Vec<FfiLayoutIntent>` — Splits, focus-direction requests, and drawer opens, in arrival order.
- `windows(&self) -> Vec<FfiWinInfo>` — Every window nvim reports; Swift filters for live editor windows.
- `grid_for_win(&self, win: i64) -> u64` — 0 when no window maps to it.
- `active_win(&self) -> i64` — 0 until nvim reports one.
- `focus_win(&self, win: i64)` — Click-to-focus and resolved focus-direction moves.
- `close_pane(&self)` — `:close`, which closes the split rather than the buffer.
- `rebuild_layout(&self, spec_json: String) -> Vec<i64>` — Realizes a saved split tree in the current tabpage's lone window, returning the window handles in leaf order. nvim supplies only count, topology, and per-window buffer; sizes come from the front-end's weights.
- `pane_status_for_grid(&self, grid: u64) -> FfiPaneStatus` — The statusbar strings for the window drawing into `grid`.
- `toggle_full_width(&self)` — Flips `b:wolf_full_width` on the effective buffer, which for a preview scratch is the source buffer.
- `is_full_width_active(&self) -> bool` — Drives the View > Use full width for content checkmark.
- `full_width_for_win(&self, win: i64) -> bool` — The per-window flag the front-end threads into a pane's geometry.
- `FfiLayoutIntent::kind/orientation/dir/from/win` — A tagged flattening of `wolf_core::LayoutIntent`. Fields a given kind does not use read as 0; a drawer open reuses the `win` field for the new drawer window rather than adding a bridged field.
- `FfiWinInfo::grid/win/hidden` — `grid` is `>= 2` for editor windows.
- `FfiPaneStatus::filename/display_path/abs_path` — `filename` is `[No Name]` for an unnamed buffer. `abs_path` is never relativized — the front-end persists it as a pane-layout leaf path.

### Tabpages

One captive nvim is one project; each of its views binds to a distinct tabpage.

- `create_tabpage(&self, cwd: String) -> i64` — Optionally roots the tabpage at `cwd` via `:tcd`; an empty string skips it. Blocks briefly. 0 when the core is gone, and the caller then drops the would-be view.
- `current_tabpage(&self) -> i64` — The authoritative first-view bind, via a blocking RPC.
- `active_tabpage(&self) -> i64` — The cheap refocus gate, read from the snapshot with no RPC.
- `cwd_for_tabpage(&self, tab: i64) -> String` — A tabpage's canonicalized `:tcd` cwd, authoritative for a view that is not current. `""` for a closed or unknown handle.
- `set_current_tabpage(&self, tab: i64)` — The front-end drives every tabpage switch when a view becomes key.
- `close_tabpage(&self, tab: i64)` — A non-last view close. The shared nvim survives until the last view goes, which the front-end handles by shutting the core down instead.

### Documents and directories

- `open(&self, path: String)` — Absolute path. The core's input pump records the file MRU.
- `open_at(&self, path: String, line: u64, col: u64)` — Open and jump. `line` is 1-based, `col` a 0-based byte offset.
- `chdir(&self, path: String)` — `:tcd` — the tab-local cwd, since each view owns its own working directory.
- `cwd(&self) -> String` — nvim's live canonicalized cwd, tracking `:cd` and `DirChanged`.
- `current_file(&self) -> String` — `""` for an unnamed buffer.
- `new_buffer(&self)` — An empty markdown-typed buffer named the next free `Untitled N.md`, not written to disk.
- `recent_dirs(&self) -> Vec<String>` — The global directory MRU, newest first, capped at 20. Drives File > Open Recent.
- `wolf_ffi_ancestors_to_reveal(rel_path: String) -> Vec<String>` — A free function, no session needed: the ancestor directories the sidebar expands before revealing a file.

### Commands and the command palette

- `drain_commands(&self) -> Vec<FfiCommand>` — User-triggered `:Wolf…` Ex-commands and `require('wolf')` Lua calls, in arrival order.
- `FfiCommand::kind() -> u8` — `mod command_kind`. The packing match is exhaustive, so a new `wolf_core::Command` variant is a compile error here.
- `command_palette(&self) -> FfiCommandPalette` — Seeded with the recent directories and the command MRU.
- `FfiCommandPalette::add_window(&self, index: u16, cwd: String)` — One other open window; the caller excludes the current one. An empty `cwd` renders as "Untitled".
- `FfiCommandPalette::ranked(&self, query: String, max: usize) -> Vec<FfiCommandRow>` — Assembles the static commands, the added windows, and the recent directories, ranked MRU-first. There is no wake: candidates are fixed when the palette opens, and this re-filters synchronously per keystroke.
- `FfiCommandPalette::record(&self, key: String)` — Writes a row's `mru_key` into the global command MRU on activation.
- `FfiCommandRow::display/row_kind/command_kind/window_index/path/mru_key` — `row_kind` per `mod palette_row_kind`. `command_kind` is meaningful only for a static row, `window_index` only for a switch-window row, `path` only for an open-recent row.

### File picker

- `start_file_search(&self) -> FfiFileSearch` — A streaming traversal of the core's cwd, fed by the file-search wake.
- `FfiFileSearch::ranked(&self, query: String, max: usize) -> Vec<FfiFileEntry>` — Ranks in Rust with the bounded top-N variant, which heaps over the match set instead of sorting it. An empty query lists everything MRU-first.
- `FfiFileSearch::version(&self) -> u64` — Live snapshots start at 1, so 0 uniquely means no session backs this handle. The sidebar reloads only when it advances.
- `FfiFileEntry::display/path` — `display` is relative to the search root and is the string ranked; `path` is absolute and goes back to `open`.

### File tree

- `start_tree_search(&self) -> FfiFileSearch` — A separate, long-lived session for the sidebar, fed by its own tree wake.
- `FfiFileSearch::children_of(&self, dir: String) -> Vec<FfiTreeNode>` — Immediate children of `dir` (`""` is the root), directories first then files. Served from a version-keyed index cache, because AppKit's outline reload issues many calls at a single snapshot version.
- `FfiFileSearch::rel_path_for(&self, abs: String) -> String` — Exact entry-path match; `""` when no streamed entry matches.
- `FfiTreeNode::name/rel_path/abs_path/is_dir` — `abs_path` is `""` for a directory node.

### Content search

- `start_search(&self, query: String, use_regex: bool, case_sensitive: bool, all_files: bool) -> FfiSearch` — A ripgrep-backed session fed by the search wake. The three booleans are the panel's toggle buttons. An unset `WOLF_RG_BIN` degrades to no results rather than an error.
- `ignorecase(&self) -> bool` — Read once to seed the panel's case-sensitivity toggle as its inverse. Blocks briefly.
- `FfiSearch::version(&self) -> u64` — Same zero-means-absent convention as `FfiFileSearch::version`.
- `FfiSearch::files(&self, budget: u32) -> Vec<FfiSearchFile>` — `budget` is a character count for the context window around each match. Memoized on `(version, budget)`, for the same reload-storm reason as the tree index cache.
- `FfiSearchFile::label/rel_path/abs_path/matches` — `label` is `"<basename> - <parent dir>"`, bare basename at the root. `rel_path` is the sort key.
- `FfiSearchMatch::line_number/column` — `line_number` is 1-based. `column` is a byte offset into the original line, not into `display_text`, because it feeds `open_at`.
- `FfiSearchMatch::display_text/truncated_left/truncated_right` — The windowed line plus whether context was dropped on each side, which is what the front-end's ellipses mean.
- `FfiSearchMatch::range_starts/range_ends` — The bold ranges as parallel byte-offset vectors, avoiding a third opaque type. Swift maps them to UTF-16 spans.

### Buffer picker

- `buffer_picker(&self) -> FfiBufferPicker` — Seeded from a blocking snapshot of the listed buffers plus the live cwd.
- `FfiBufferPicker::ranked(&self, query: String, max: usize) -> Vec<FfiBufferRow>` — Ordered by `lastused` among matches; an empty query puts the current buffer at row 0. No wake, and no activation hook — activation happens on the handle.
- `switch_buffer(&self, bufnr: i64)` — A window-scoped `nvim_win_set_buf` plus a per-pane view restore, against the live active window.
- `FfiBufferRow::bufnr/display/modified` — `display` is cwd-relative, absolute, or `[No Name]`, and is also the ranked string.

### Preview

- `is_markdown_active(&self) -> bool` — Gates View > Preview and its palette row.
- `is_preview_active(&self) -> bool` — Gates View > Edit.
- `open_preview(&self) -> i64` — Previews the active markdown buffer in place: the active pane's buffer is replaced with the preview scratch. Returns the scratch bufnr, or 0 when the active buffer is not markdown.
- `edit_source(&self) -> i64` — The inverse: replaces the active pane's preview with its source, in place. Returns the source bufnr, or 0 off a preview pane.
- `render_preview(&self, source_bufnr: i64) -> String` — The themed HTML document the web view loads; `""` on error.
- `open_in_browser(&self) -> String` — Starts or reuses the loopback-only preview server pinned at the active markdown buffer and returns a `http://127.0.0.1:<port>/<relpath>` URL. The path is root-relative so links between markdown files resolve.
- `preview_file(&self, win: i64, path: String) -> i64` — Previews the markdown file at `path` in window `win` — the link-follow entry, and where the file picker and open dialog route when the active pane is in preview mode. `path` is absolute — no relative resolution happens here. Returns the file's source bufnr, left buflisted so a preview-only file keeps its picker row, or 0, and the front-end has its own fallback for 0.
- `preview_buffer(&self, win: i64, bufnr: i64) -> i64` — Previews an already-open buffer in window `win` — where the buffer picker routes in preview mode. Returns the source bufnr, or 0 when the buffer is invalid, unnamed, or not markdown; on 0 the front-end falls back to `switch_buffer`, converting the pane to edit mode.
- `preview_in_other(&self, target_win: i64) -> i64` — Previews the active markdown buffer in another pane. `target_win` is the front-end-resolved target window, `0` to split a new pane on the right; the Lua helper gates before any split, so a rejected call never changes the layout, and a split surfaces as an ordinary vertical Split intent. Focus stays in the source editor. Blocks briefly; returns the source bufnr, or 0 off a markdown buffer or after shutdown.
- `edit_in_other(&self, target_win: i64) -> i64` — The other-pane inverse: binds the active preview's source into `target_win` (or splits, with the source landing left) and focuses the pane editing it. Returns the bound window's bufnr — non-zero even on the stale-bufnr `:edit` fallback — or 0 off a preview pane or after shutdown.
- `previews(&self) -> Vec<FfiPreview>` — One entry per window showing a preview scratch buffer.
- `FfiPreview::win/source_bufnr/source_name/source_path/source_display_path` — `win` is the pane the web view overlays. `source_name` is the basename, `source_display_path` the cwd-relative or `~`-prefixed form.
- `FfiPreview::source_tick() -> u64` — The source buffer's `changedtick`; the front-end reloads the web view when it advances. 0 until the first dirty notification.
- `FfiPreview::source_full_width() -> bool` — The source buffer's full-width flag, which decides whether the web view fills the pane or gets the capped, centered rect.
- `FfiPreview::source_cursor_line() -> u32` / `source_cursor_col() -> u32` — The source buffer's editing cursor, joined in `previews()` from `Snapshot.markdown_cursors` by `source_bufnr`. `line` is 1-based to match the `wl-{L}-{C}` markers and `col` is a 0-based character column, which the front-end rounds down to a marker stride. A `line` of `0` means the cursor is unknown, and the front-end must not scroll for it. Both are opaque-type accessors, so they are additive and `wolf_ffi_wake.h` is unaffected.

### Per-pane navigation history

- `history_can_back(&self, win: i64) -> bool` — A snapshot read, no RPC. False for an unknown window, a never-navigated pane, or the terminal drawer.
- `history_can_forward(&self, win: i64) -> bool` — Same contract.
- `history_back(&self, win: i64) -> bool` — Blocks briefly. Returns whether the pane actually moved, not whether the call was accepted.
- `history_forward(&self, win: i64) -> bool` — Same contract.
- `history_jump(&self, win: i64, index: u32) -> bool` — False for an out-of-range index and for the cursor's own index, so activating the checkmarked row is a true no-op.
- `history_entries(&self, win: i64) -> Vec<FfiHistoryEntry>` — Newest first, capped at 10 distinct targets. Core has already ordered, capped, disambiguated, and home-abbreviated them, so Swift only draws.
- `FfiHistoryEntry::display/index/is_current` — `display` is finished text — draw it verbatim. `index` goes back to `history_jump`. At most one row is current, and none is when the cursor's target fell outside the cap.

### Command line, completion popup, and toasts

- `cmdline(&self) -> FfiCmdline` — `ext_cmdline`. Inactive when nothing is showing.
- `FfiCmdline::active/firstc/prompt/text/pos/indent/level` — The content chunks are flattened into one `text`: the command line is deliberately unstyled across the boundary. `pos` is a byte offset, `level` is 1 at top level.
- `pum(&self) -> FfiPum` — `ext_popupmenu`.
- `FfiPum::visible/selected/anchor_grid/anchor_row/anchor_col/items` — `selected` is -1 for none. An `anchor_grid` of -1 means anchored to the command line, and `anchor_col` is then a byte offset into the command-line text.
- `FfiPumItem::word/kind/menu/info` — `kind` is the single-letter hint.
- `toast(&self) -> FfiToast` — `ext_messages`.
- `FfiToast::visible/kind/lines` — `kind` is the last message's kind. Every message's lines are flattened in order.
- `FfiToastLine::chunks`, `FfiToastChunk::text/fg/bg/special/flags` — Unlike the command line, message color is load-bearing, so each chunk's colors are resolved in Rust against the same highlight table the frame uses.

### Appearance and theme

- `set_background(&self, dark: bool)` — `:set background=dark|light`. The macOS front-end pushes the resolved appearance at launch and on every system change while in auto mode.
- `theme_titlebar_bg(&self) -> FfiColor` — The `WolfTitlebar` group's background. Unset when the group is absent or cleared.
- `theme_titlebar_fg(&self) -> FfiColor` — The same group's foreground, carried for title-text coloring. Same unset convention.

### Config

- `wolf_ffi_load_config() -> FfiConfig` — A free function, no live core needed. Reads — or creates with defaults — `wolf-config.yaml` in the platform preferences directory, falling back to the built-in defaults on any error.
- `FfiConfig::font_family/font_size/line_height` — `line_height` is a percentage of the font size, so 130 means 130%.
- `FfiConfig::appearance() -> u8` — `mod appearance`.
- `FfiConfig::macos_option_as_alt() -> bool` — Whether Option produces `<M-…>` rather than composing accented characters. Defaults to false.

### Saved state

The read side loads a saved tree; the write side pushes a fresh one through free functions, since a recursive tree cannot cross a scalar bridge as one value.

- `wolf_ffi_load_state() -> FfiState` — The pre-start restore read. It never creates or modifies the file and never opens the live store; an absent, corrupt, or unresolvable state all yield an empty window list.
- `FfiState::windows() -> Vec<FfiWindowState>` — Empty when nothing was saved.
- `FfiWindowState::x/y/width/height/tabs` — Points. A width or height at or below 0 means unset — do not restore it.
- `FfiTabState::cwd/sidebar_visible/has_layout/layout` — `has_layout` is false only for an empty-state lone pane; a lone pane showing a file or a preview still persists a single leaf.
- `FfiPaneNode::is_leaf/is_preview/leaf_path/orientation/child_count/weight/child` — Recursion happens through repeated opaque returns from `child`, which keeps the bridge on scalar, String, and opaque returns. A preview leaf's `leaf_path` is the source file's absolute path.
- `wolf_ffi_state_snapshot_begin()` — Clears the write-side scratch tree.
- `wolf_ffi_state_snapshot_add_window(x, y, width, height)` — Pushes a window with an empty tab list; later `add_tab` calls attach to it.
- `wolf_ffi_state_snapshot_add_tab(cwd, sidebar_visible)` — Attaches to the most recently added window. A no-op when there is none.
- `wolf_ffi_state_layout_push_leaf(path, preview, weight)` — Part of a post-order token replay that assembles one tab's split tree across the scalar bridge.
- `wolf_ffi_state_layout_push_split(orientation, child_count, weight)` — Pops `child_count` nodes, wraps them into a split, and pushes it back. The root split's weight is 1.0.
- `wolf_ffi_state_layout_commit_to_tab()` — Attaches the single remaining node to the last window's last tab. A no-op unless exactly one node remains.
- `wolf_ffi_state_snapshot_commit()` — Commits the whole tree at once; an identical tree is a no-op in the store.
- `wolf_ffi_state_flush() -> bool` — Blocks until every pending write has been attempted, bounded by an internal timeout. True when no store exists, since there is nothing to flush.

All windows share one process-wide store, opened lazily over the platform state directory. Opening the same directory twice would race the per-store writer threads, which the store does not support. An unresolvable state directory disables persistence process-wide, logged and never fatal.

## Discriminant tables

These `u8` values are the ABI: Swift decodes raw integers, so the numbers are content, not magic. They are append-only — see [`../DECISIONS.md`](../DECISIONS.md).

| Module             | Mirrors                       | Values                                                                                                                                                                    |
| ------------------ | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `kind`             | `wolf_core::Key`              | `CHAR` 0, `ENTER` 1, `ESC` 2, `TAB` 3, `BACKSPACE` 4, `DELETE` 5, `LEFT` 6, `RIGHT` 7, `UP` 8, `DOWN` 9, `HOME` 10, `END` 11, `PAGE_UP` 12, `PAGE_DOWN` 13, `FUNCTION` 14 |
| `cursor_shape`     | `wolf_core::CursorShape`      | `BLOCK` 0, `HORIZONTAL` 1, `VERTICAL` 2                                                                                                                                   |
| `appearance`       | `wolf_core::Appearance`       | `AUTO` 0, `DARK` 1, `LIGHT` 2                                                                                                                                             |
| `intent_kind`      | `wolf_core::LayoutIntent`     | `SPLIT` 0, `FOCUS_DIR` 1, `DRAWER_OPEN` 2                                                                                                                                 |
| `orientation`      | `wolf_core::SplitOrientation` | `HORIZONTAL` 0, `VERTICAL` 1                                                                                                                                              |
| `focus_dir`        | `wolf_core::FocusDir`         | `LEFT` 0, `RIGHT` 1, `UP` 2, `DOWN` 3                                                                                                                                     |
| `mouse_button`     | `wolf_core::MouseButton`      | `LEFT` 0, `RIGHT` 1, `MIDDLE` 2                                                                                                                                           |
| `mouse_action`     | `wolf_core::MouseAction`      | `PRESS` 0, `RELEASE` 1, `DRAG` 2                                                                                                                                          |
| `scroll_dir`       | `wolf_core::ScrollDirection`  | `UP` 0, `DOWN` 1, `LEFT` 2, `RIGHT` 3                                                                                                                                     |
| `liveness`         | `wolf_core::Liveness`         | `ALIVE` 0, `NOT_RESPONDING` 1, `DEAD` 2                                                                                                                                   |
| `palette_row_kind` | `wolf_core::PaletteRowKind`   | `STATIC` 0, `SWITCH_WINDOW` 1, `OPEN_RECENT` 2                                                                                                                            |

`mod flag` is a bitset rather than an enum: `BOLD` `1 << 0`, `ITALIC` `1 << 1`, `UNDERLINE` `1 << 2`, `UNDERCURL` `1 << 3`, `STRIKETHROUGH` `1 << 4`, packed into `FfiCell::flags` and `FfiToastChunk::flags`.

`mod command_kind` mirrors `wolf_core::Command` and is contiguous from 0.

| Value | Constant                | Meaning                                                                         |
| ----- | ----------------------- | ------------------------------------------------------------------------------- |
| 0     | `NEW_WINDOW`            | Open a new editor window.                                                       |
| 1     | `CLOSE_WINDOW`          | Close the focused window.                                                       |
| 2     | `QUIT_APPLICATION`      | Quit the application.                                                           |
| 3     | `HIDE_APPLICATION`      | Hide the application.                                                           |
| 4     | `MINIMIZE_WINDOW`       | Minimize the focused window.                                                    |
| 5     | `GO_TO_FILE`            | Open the file-picker overlay.                                                   |
| 6     | `OPEN_FILE`             | Native open-file dialog.                                                        |
| 7     | `OPEN_FOLDER`           | Native dialog to change the working directory.                                  |
| 8     | `CLOSE_PANE`            | Close the current split, not the buffer.                                        |
| 9     | `NEW_BUFFER`            | New empty buffer in the current window.                                         |
| 10    | `RESET_FONT_SIZE`       | Back to the configured default size.                                            |
| 11    | `INCREASE_FONT_SIZE`    | One point larger.                                                               |
| 12    | `DECREASE_FONT_SIZE`    | One point smaller.                                                              |
| 13    | `COMMAND_PALETTE`       | Open the palette; excluded from the palette's own rows.                         |
| 14    | `COPY`                  | Visual selection to the system clipboard.                                       |
| 15    | `PASTE`                 | System clipboard into the active document.                                      |
| 16    | `BUFFER_PICKER`         | Open the buffer-picker overlay.                                                 |
| 17    | `NEW_TAB`               | New tab in the current window.                                                  |
| 18    | `PREVIEW`               | Render the active markdown buffer into a preview pane.                          |
| 19    | `EDIT`                  | From a preview pane back to its source file.                                    |
| 20    | `TERMINAL`              | Toggle the per-project terminal drawer.                                         |
| 21    | `PREVIEW_IN_BROWSER`    | Render the active markdown buffer and open it in the system browser.            |
| 22    | `FULL_WIDTH`            | Toggle the active buffer between the default content width and full pane width. |
| 23    | `TOGGLE_SIDEBAR`        | Collapse or expand the sidebar; a native-only action.                           |
| 24    | `SIDEBAR_SHOW_PROJECT`  | Reveal the sidebar's file tree; a native-only action.                           |
| 25    | `SEARCH_IN_PROJECT`     | Reveal and focus the sidebar's search panel; a native-only action.              |
| 26    | `PREVIEW_IN_OTHER_PANE` | Preview the active pane's markdown file in another pane.                        |
| 27    | `EDIT_IN_OTHER_PANE`    | Edit the active preview's source file in another pane.                          |

#### `bin/gen_bindings.rs`

Purpose: the standalone codegen behind `bazel run //crates/wolf-ffi:gen_bindings`. It parses the `#[swift_bridge::bridge]` module in `lib.rs` and rewrites the committed `generated/` tree in place.

**Public API**

- `main()` — takes no arguments; reads `BUILD_WORKSPACE_DIRECTORY` and exits 1 with a usage message when it is unset.

**Decisions**

- It flattens swift-bridge's nested per-crate output directory up into `generated/`, so the committed layout matches what `BUILD.bazel` and the modulemap name.
- It prepends `import WolfFFI` to each generated `.swift` file. The generated Swift references bare `__swift_bridge__$…` C symbols, which are in scope only once the C interop module named in `generated/module.modulemap` is imported, because the Swift sources compile as their own module.

**Gotchas**

- `generated/module.modulemap` and `generated/wolf_ffi_wake.h` are hand-written and are never overwritten here, so re-running this binary cannot restore them.
- Writing in-tree requires `bazel run`; see [`../DECISIONS.md`](../DECISIONS.md).
