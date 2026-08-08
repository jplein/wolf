# wolf-ffi/tests

Integration tests for the [`wolf-ffi`](../README.md) bridge surface. Most drive a real, pinned Neovim rather than a mock, because what is under test is a live RPC round-trip across the FFI boundary.

```sh
cargo test -p wolf-ffi                     # all of them, on the Linux and Nix path
bazel test //crates/wolf-ffi:bridge_smoke  # one target, on the macOS Bazel host
```

## Adding a test file

Cargo discovers every `tests/*.rs`. **Bazel does not.** A new file needs a matching `rust_test` target in [`../BUILD.bazel`](../BUILD.bazel) or it runs under `cargo test` alone, which makes it a silent macOS-only gap. Mirror an existing sibling of the same kind:

- **Real-nvim** — stages `//bazel/nvim:nvim_tree` and `//runtime/nvim:init.lua` as `data`, wires `WOLF_TEST_NVIM_DIR` / `WOLF_TEST_INIT_LUA` via `$(rootpath)`, lists `tests/common/mod.rs` in `srcs`, and carries `tags = ["no-sandbox"]` because spawning a child nvim does not work under the macOS build sandbox. Results are still cached.
- **Headless** — none of that: no `data`, no env, no tag, no `common/`.

Some targets stage more: `search` adds `//bazel/rg:rg_tree` and wires `WOLF_RG_BIN`; `orlando_load` and `lighthouse_load` add their vendored plugin trees; the `treesitter_*` targets add the parser and `nvim-treesitter` trees. Each entry below says which kind it is and what it needs.

## The shared harness — `common/mod.rs`

The shared harness for every `wolf-ffi` bridge test. It mirrors `crates/wolf-core/tests/common/mod.rs` with one deliberate policy difference: where the wolf-core suite panics without a real nvim, these tests skip loudly, so a host that cannot provision the pinned Neovim is a no-op pass rather than a failure.

- `nvim_bin_or_skip()` — the skip gate. An explicit `WOLF_NVIM_BIN` wins, then the staged pinned tree via `WOLF_TEST_NVIM_DIR`. A caller that gets `None` must return from the test; panicking or asserting success instead defeats the policy, and the stderr banner is what keeps a misconfigured environment from reading as real coverage.
- Publishing the resolved path back into `WOLF_NVIM_BIN` is part of that helper's contract, not a side effect to tidy away: under Bazel the binary comes from the staged tree while `wolf_ffi_start` reads only that variable, so without the publish the whole Bazel path launches nothing. The write is conditional, so the devShell path performs none at all, and it happens once per test binary behind a `OnceLock` that every reader is ordered after — `setenv` is not synchronised against a C-side `getenv`, and `cargo test` runs a binary's tests in parallel.
- Because the helper publishes, a test body needs no `std::env::set_var("WOLF_NVIM_BIN", …)` of its own — do not copy one from a sibling. Most real-nvim binaries here still carry that redundant write at the top of each test body, which the helper makes a no-op and which races its neighbours' environment reads in any binary holding more than one test. Removing them is pure plumbing that touches no assertion, and nobody has done it.
- `init_lua_path()` — resolves the captive `runtime/nvim/init.lua`, preferring the staged runfile over the `CARGO_MANIFEST_DIR` fallback. Under Bazel that manifest path is an ephemeral sandbox directory absent at test runtime, and nvim launched with a missing `-u` file blocks forever on its `Press ENTER` prompt, hanging startup.
- `orlando_dir_or_none()`, `lighthouse_dir_or_none()`, `parsers_dir_or_none()`, `nvim_treesitter_dir_or_none()` — staged vendored-tree resolvers that return `None` off Bazel so the dependent test skips instead of failing.
- `copy_dir_recursive()` — stages a sibling tree next to a temp `init.lua` for the tests whose Lua self-locates its siblings. It classifies entries with `std::fs::metadata`, which follows symlinks, because Bazel runfiles entries are symlinks that `DirEntry::file_type` would skip.
- `fresh_cwd()` — a unique temp dir that is deliberately not auto-deleted: it is nvim's working directory and file-search root, so it must outlive `wolf_ffi_start`.
- `DEFAULT_COLS` / `DEFAULT_ROWS` — the starting grid size the smoke-style tests attach with.

## Files

#### `appearance.rs`

**Covers**

`WolfCoreHandle::set_background(dark)` flipping the captive Neovim's global `&background`. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:appearance` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- The bridge exposes no `&background` accessor. A case observes it indirectly, by setting `WolfCursor guibg` to a marker chosen by the live option and reading `FfiFrame::cursor_fill()` back.
- A plain `:hi` does not fire `ColorScheme`, so the marker must be followed by `:doautocmd ColorScheme` or the theme tracker never re-emits.
- The captive `init.lua` launches with `&background` dark, so that is the baseline a new case starts from.

#### `attribute_highlight.rs`

**Covers**

Per-cell text attributes (`FfiCell::flags()`) and the special/underline color (`FfiCell::special()`) crossing the bridge, driven by an `nvim_set_hl` group applied with `matchadd`. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:attribute_highlight` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- The bit values are copied from `mod flag` in `src/lib.rs`; a new attribute case must mirror that layout rather than inventing bits.
- The styled repaint lags the `:lua` command by a frame or two, so the cell is located by its glyph text and polled.

#### `bridge_smoke.rs`

**Covers**

The baseline handle lifecycle: `wolf_ffi_start`, `read_frame` (dense `cols * rows` cells, cursor in bounds), `send_key` with hand-built `FfiKeyEvent`s, `resize`, `shutdown` — no Swift involved. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:bridge_smoke` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- `FfiFrame` carries no snapshot `seq`, so every assertion polls `read_frame()` to a timeout rather than keying on a flush counter. Prefer a sequence accessor if one is ever added.
- `resize` drives the global UI size while `read_frame()` reports the active-window grid: the width matches the request exactly, the height is the request minus the cmdline row(s), so height assertions need a small tolerance.

#### `buffer_picker.rs`

**Covers**

`buffer_picker()` and its `ranked(query, max)` (buflisted-only, most-recently-used first), the window-scoped `switch_buffer(bufnr)`, and the per-`(winid, bufnr)` view round-trip. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:buffer_picker` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- The picker snapshots `list_buffers()` at construction, so a case that creates a buffer must call `buffer_picker()` again to see it.
- `getbufinfo().lastused` has one-second resolution. The fixture sleeps past a second boundary between opening two buffers and awaits each becoming current on either side, because `send_ex_command` is fire-and-forget and the keystrokes would otherwise drain together and tie on `lastused`.

#### `close_pane.rs`

**Covers**

`close_pane()` dropping one split, counted through `windows()` (non-hidden grids at or above the first editor grid). Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:close_pane` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- Drive the handle method, not a typed `:close<CR>`: the captive `init.lua` intercepts the interactive Ex command, while the method issues `nvim_command("close")` over RPC and bypasses it. Only the setup `:vsplit` is typed.

#### `cmdline_ui.rs`

**Covers**

The native command line, completion popup and message toast — `cmdline()`, `pum()` and `toast()`, including per-chunk colors resolved in Rust against the highlight table. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:cmdline_ui` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- `emsg_toast_keeps_styled_color` asserts only that an `ErrorMsg` chunk's `fg()` differs from a plain echo's, never a fixed RGB. It would need softening if a future pinned colorscheme made `ErrorMsg` equal the default foreground.
- Completion words can come back shell-escaped, so a filename comparison de-escapes first — the seeded fixture files deliberately contain spaces.

#### `code_block_highlight.rs`

**Covers**

End-to-end treesitter highlighting of fenced code blocks in `render_preview` output: a `go` fence yields inline color-styled spans, an unknown-language fence stays plain `<pre><code>`. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:code_block_highlight` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- It also needs a queries tree and a parsers tree, from either provider: the Bazel-staged `WOLF_TEST_NVIM_TREESITTER_DIR` / `WOLF_TEST_PARSERS_DIR`, or the Linux devShell's `WOLF_NVIM_TREESITTER_DIR` / `WOLF_PARSERS_DIR`, Bazel first. It therefore asserts on both platforms and loud-skips only on a host carrying neither. The skip is about this fixture's language, not about the highlighter: `wolf_highlight_code_blocks` sits at module level in `runtime/nvim/init.lua`, outside the vendored-tree gate, so it runs everywhere and gates per language.
- The fixture rebuilds the bundle layout in a temp dir (`init.lua` beside sibling `nvim-treesitter/` and `parsers/`) because the captive block self-locates relative to the init file's own directory, and sanity-checks the staged `go.so` and injections query before paying for an nvim launch.
- The styled-span probe omits the space in `color:#`, which the emitted CSS rules (`color: #`) do not match.

#### `command_palette.rs`

**Covers**

A `command_palette()` session: static registry rows, `add_window` and recent-folder rows, and the `record` MRU round-trip observed through `ranked("")`. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:command_palette` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- One `#[test]` per binary by design: the case points the process-global `WOLF_STATE_DIR` at a tempdir before `wolf_ffi_start`, and the MRU stores open under it exactly once at core start.
- A session fixes its candidate rows and MRU order at construction, so observing a new window, recent dir or recorded key means opening a fresh `command_palette()`.
- The palette excludes its own invoking command, so the highest static `command_kind` that appears as a row is one below `CommandPalette`.

#### `commands.rs`

**Covers**

The command channel: typing each `:Wolf…` Ex command and draining an `FfiCommand` of the matching `kind()` from `drain_commands()`. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:commands` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- The drains are inert — no front-end is present to act on them, so nothing closes, quits or opens. Only add commands whose observable effect stays confined to the queue; the editor-affecting halves are covered in their own files.
- The file re-declares every `command_kind` discriminant locally and asserts the set is contiguous from zero, as a drift guard. A new `wolf_core::Command` variant means adding the const, extending both sides of that guard, and appending a drain case.

#### `config.rs`

**Covers**

`wolf_ffi_load_config()` and the `FfiConfig` accessors: font family/size/line-height, the `appearance` discriminant, `macos_option_as_alt`, back-compat for absent keys, and default-file creation. Headless — it spawns no Neovim, and the `:config` target stages no runfiles and carries no `no-sandbox` tag.

**Gotchas**

- Every case points the process-global `WOLF_CONFIG_DIR` at its own tempdir and holds `ENV_LOCK` across the whole set/load/remove window. Cargo runs these concurrently in one process, so a new case must take the lock as well.
- Expected defaults are duplicated from `wolf_core::config` and the font size is platform-split, so a default change has to be mirrored here.

#### `copy_paste.rs`

**Covers**

`is_visual_mode()`, `copy_selection()` and `paste(text)` across a full editing round-trip: type a word, select it, copy, leave visual mode, paste. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:copy_paste` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- The selection is built from keystrokes and the result checked by flattening the frame's cell texts, so cases stay free of exact coordinates.
- The drained `:WolfCopy` / `:WolfPaste` command discriminants live in `commands.rs`; this file covers only the handle methods.

#### `edit.rs`

**Covers**

The Edit surface: the `is_preview_active()` gate and `edit_source(mode)` for `split_left`, `reuse_editor_pane` and `active`, plus the reuse fallback with no other editor pane, the not-a-preview no-op, and neutral values after `shutdown()`. Spawns a real Neovim: needs `WOLF_NVIM_BIN`, and the `:edit` target stages the nvim runfiles and is tagged `no-sandbox`.

**Gotchas**

- After `:e alpha.md` a case must wait on `current_file()`, not just `is_markdown_active()`: the startup buffer is already a markdown `Untitled.md`, so the markdown gate returns immediately and a synchronous `open_preview` or `edit_source` would race ahead of the buffered `:e`.
- `edit_source` is gated on the active pane being a preview, so a case must cycle focus onto the preview window first.

#### `file_actions.rs`

Real-nvim coverage of the native file-action methods on `WolfCoreHandle`, all driven against one session in a single `#[test]`. Target `//crates/wolf-ffi:file_actions` stages the nvim runfiles and runs unsandboxed; without a pinned nvim (`WOLF_NVIM_BIN` or the staged tree) it loud-skips.

**Covers**

- `chdir` / `open` / `new_buffer`, each observed by polling `cwd()` or `current_file()` until it settles.

**Gotchas**

- The startup buffer is a named `Untitled.md`, which `chdir` re-homes into the target directory, so `new_buffer()`'s collision walk lands on `Untitled 2.md`; a case that changes the `chdir` sequence changes that name.
- Path assertions canonicalize both sides — macOS temp dirs resolve `/var` to `/private/var`.

#### `file_search.rs`

Real-nvim coverage of the file-picker bridge: `start_file_search`, `FfiFileSearch::ranked`, and the `FfiFileEntry` accessors, seeded from a temp cwd. Target `//crates/wolf-ffi:file_search` stages the nvim runfiles and runs unsandboxed; loud-skips without a pinned nvim.

**Covers**

- Ranking and per-entry shape (`display()` relative, `path()` absolute and under the cwd), plus `ranked`'s bounding to the requested `max`.

**Gotchas**

- The traversal runs on a background worker, so every assertion polls `ranked` until the seeded files stream in.
- `open` is only checked at the core-stays-responsive level: `FfiFrame` exposes no `current_file` accessor.
- The bounding case seeds a few hundred matching files on purpose, so the top-N select is observably narrower than the match set.

#### `frame_pack.rs`

Headless, deterministic coverage of `FfiFrame::from_snapshot`: it builds a `wolf_core::Snapshot` by hand — no nvim, no `WolfCore`, no `tests/common/`. Target `//crates/wolf-ffi:frame_pack` needs no runfiles, no env, and no `no-sandbox` tag, so this is the packing coverage that still runs on a host with no nvim.

**Covers**

- Color packing and set/unset flags, the style-flag bitset, reverse-video resolution, cursor shape and visibility, cursor fill/glyph precedence (theme over mode attr over plain reverse video), and the dense row-major `cells()` grid.

**Gotchas**

- `from_snapshot` is public solely so this test can drive it without a live core.
- The flag and cursor-shape discriminants are copied from `src/lib.rs`; they are the values under test, so keep them in sync by hand.
- Build snapshots from `Snapshot::default()` plus field assignment — `..Snapshot::default()` fails with E0451 on the crate-private field.

#### `full_width.rs`

Real-nvim coverage of the per-buffer content-width toggle: `toggle_full_width`, `is_full_width_active`, `full_width_for_win`, and `FfiPreview::source_full_width`, plus `:WolfFullWidth` reaching the command channel. Target `//crates/wolf-ffi:full_width` stages the nvim runfiles and runs unsandboxed; loud-skips without a pinned nvim.

**Gotchas**

- Drive the handle method, not the Ex command: `:WolfFullWidth` only enqueues an `FfiCommand`, and no front-end dispatcher exists in-test, so it never flips the flag on its own.
- After `:e alpha.md`, wait on `current_file()` rather than `is_markdown_active()`, which stays true across a markdown-to-markdown switch and returns immediately.
- A split leaves the preview scratch pane active, so focus the markdown source before toggling or the flag lands on the wrong buffer.

#### `history.rs`

Real-nvim coverage of the per-pane navigation-history bridge — the six `WolfCoreHandle::history_*` methods and the opaque `FfiHistoryEntry` rows behind the History menu. Core owns the history and covers it in depth; these tests pin only what the bridge adds. Target `//crates/wolf-ffi:history` stages the nvim runfiles and runs unsandboxed; loud-skips without a pinned nvim.

**Covers**

- Rows follow core (newest-first, one `is_current()`, descending stack indices), `index()` round-trips through `history_jump`, and unknown windows, the terminal drawer, and a shut-down core all fall back to `false` / empty.

**Gotchas**

- The startup buffer is a named `Untitled.md`, so every pane's history carries an extra row; assert relative order and prefixes, never an absolute depth.
- Each move is followed by a settle pause so the self-caused emit is absorbed before the next assertion.

#### `lighthouse_load.rs`

Real-nvim proof that the captive `runtime/nvim/init.lua` both finds and activates a sibling vendored `lighthouse/` colorscheme. Target `//crates/wolf-ffi:lighthouse_load` stages the nvim runfiles plus `//bazel/lighthouse:lighthouse_tree` (`WOLF_TEST_LIGHTHOUSE_DIR`) and runs unsandboxed; it loud-skips when either the pinned nvim or the lighthouse tree is missing, the normal cargo/Linux case.

**Covers**

- `vim.g.colors_name` / `vim.o.background`, and the startup pickup of the scheme's `WolfTitlebar` / `WolfCursor` groups through the theme and frame accessors.

**Gotchas**

- `init.lua` self-locates the colorscheme relative to its own file, so the test rebuilds the bundle layout (`init.lua` beside `lighthouse/`) in a temp dir; pointing at the staged tree would skip the real code path.
- The bridge has no Lua-eval accessor, so activation is read back from a sentinel file written by a typed `:lua vim.fn.writefile(...)`.
- The expected colors are pinned to the vendored commit; a lighthouse bump means updating those constants.

#### `mouse_scroll.rs`

Real-nvim coverage of `send_mouse` / `send_scroll` and the `FfiFrame` viewport accessors (`topline` / `botline` / `curline` / `linecount`). Target `//crates/wolf-ffi:mouse_scroll` stages the nvim runfiles and runs unsandboxed; loud-skips without a pinned nvim.

**Covers**

- Grid targeting: events carry a window grid (`>= 2`) with grid-local coordinates, and a press lands the cursor on exactly that cell; wheel down raises `topline`, wheel up lowers it.

**Gotchas**

- `topline` is 0-based, so it settles to 0 after `:1`.
- The fixture fills the buffer with zero-padded five-char lines, quoting the format with doubled single quotes: narrow lines would clamp the click column, and a literal double quote under `:put =` is parsed as an Ex comment.
- The drag check is deliberately soft (log, not fail) to avoid headless visual-mode flakiness.

#### `multi_window.rs`

Real-nvim coverage of multi-core identity and wake routing, running two cores at distinct cwds. Target `//crates/wolf-ffi:multi_window` stages the nvim runfiles and runs unsandboxed; loud-skips without a pinned nvim.

**Covers**

- Distinct `wake_token()` per core, per-core snapshot independence, and a process-global `wolf_ffi_set_wake` callback that attributes each wake to its own token without clobbering the other core.

**Gotchas**

- Deliberately one `#[test]`: it installs the process-global wake callback and sets `WOLF_STATE_DIR`, so new cases belong inside that test rather than beside it.
- Baselines are captured after a settle pause, so a late startup redraw wake is not mistaken for a misrouted one.

#### `orlando_bg_stress.rs`

Real-nvim regression test for cells drawing with the wrong background inside an orlando code block: it stages the vendored plugin like `orlando_load` and hammers a fenced-code document with seeded-random scrolling, motion, edit+undo, and fence churn. Target `//crates/wolf-ffi:orlando_bg_stress` stages the nvim runfiles, `//bazel/orlando:orlando_tree`, and `//bazel/lighthouse:lighthouse_tree`, runs unsandboxed, and is `size = "medium"`; it skips without a pinned nvim or the orlando tree, but lighthouse is optional.

**Covers**

- After every op, at idle: each visible row's text equals its buffer line, and every cell of a code-block row shares one background.

**Gotchas**

- It exists because wolf-core's `grid_line` clamped a zero-width clear-to-EOL `repeat` up to 1, writing a stray default-background cell that Neovim's shadow grid never repaints. Checks run only at idle, so a failure means persistent divergence, not a transient frame.
- The block background must differ from the default background, so the check cannot pass vacuously.
- The document's block line ranges are pinned and asserted; edit `DOC` and you must update them. Seed and iteration count come from `WOLF_STRESS_SEED` / `WOLF_STRESS_ITERS`.

#### `orlando_load.rs`

Real-nvim proof that the captive `runtime/nvim/init.lua` finds a sibling vendored `orlando/` tree, puts it on `runtimepath`, and sources it — the only automated coverage of the plugin-present branch. Target `//crates/wolf-ffi:orlando_load` stages the nvim runfiles plus `//bazel/orlando:orlando_tree` (`WOLF_TEST_ORLANDO_DIR`) and runs unsandboxed; it loud-skips when either the pinned nvim or the orlando tree is missing.

**Gotchas**

- `init.lua` self-locates the plugin relative to its own file, so the test rebuilds the bundle layout (`init.lua` beside `orlando/`) in a temp dir rather than pointing at the staged tree.
- The bridge has no Lua-eval accessor, so `vim.g.loaded_orlando` and `require("orlando")` are read back from a sentinel file written by a typed `:lua vim.fn.writefile(...)`.

#### `pane_status.rs`

Real-nvim coverage of the per-pane statusbar bridge: `pane_status_for_grid` and the opaque `FfiPaneStatus` accessors. It opens one file, splits, opens a second, and requires the two grids to report distinct names — the assertion that makes the status per-window rather than global. Target `//crates/wolf-ffi:pane_status` stages the nvim runfiles and runs unsandboxed; loud-skips without a pinned nvim.

**Gotchas**

- `abs_path()` is the raw, never-relativized buffer path, which core canonicalizes when it applies buffers, so compare against a canonicalized expectation.

#### `panes.rs`

Real-nvim coverage of the split-panes bridge — `windows()`, `active_win`, `grid_for_win`, `read_frame_for_grid`, `drain_intents`, `resize_grid`, `focus_win` — over a split/resize/focus/close sequence. Target `//crates/wolf-ffi:panes` stages the nvim runfiles and runs unsandboxed; loud-skips without a pinned nvim.

**Gotchas**

- `drain_intents` drains, so accumulate intents across polls and keep the scalar fields; the opaque intent cannot be retained.
- Drive layout changes with Ex commands rather than `<C-w>` chords: both emit the same intent, but the Ex path is deterministic headless.
- Closing a window emits no intent — it is observed by the window vanishing from `windows()` and `grid_for_win` returning 0.
- Grid 1 is the outer cmdline strip, so pane assertions filter to grids `>= 2`.

#### `preview.rs`

**Covers** — the Markdown-preview bridge: the `is_markdown_active` gate, `open_preview(mode)` placements plus the reuse-pane fallback, the `previews()` rows, `render_preview` HTML (headings, slugs, LaTeX), and `preview_file(win, path)` link-follow, plus the scroll-sync cursor join — a row's `source_cursor_line()` / `source_cursor_col()` matching `Snapshot.markdown_cursors` for its own source buffer, and the `line == 0` sentinel a background-loaded source produces. Real nvim; target `//crates/wolf-ffi:preview` (staged nvim runfiles or `WOLF_NVIM_BIN`, `tags = ["no-sandbox"]`). Each case seeds its own temp cwd and polls the local `wait_for_*` helpers.

**Gotchas** — the startup buffer is already a markdown `Untitled.md`, so `wait_for_markdown_active(true)` returns instantly across a markdown-to-markdown `:e`; use `wait_for_current_file` as the switch barrier before a synchronous `open_preview`. The temp cwd is nvim's working dir and search root, so keep it alive until after `shutdown()`.

#### `recent_dirs.rs`

**Covers** — `WolfCoreHandle::recent_dirs()`, the global folder MRU read back newest-first after driving `:cd`. Real nvim; target `//crates/wolf-ffi:recent_dirs` (staged nvim runfiles or `WOLF_NVIM_BIN`, `tags = ["no-sandbox"]`).

**Gotchas** — deliberately a single `#[test]`: it sets the process-global `WOLF_STATE_DIR` before start, and the store opens `folder-mru.sqlite` once at core start, so new cases belong inside that test. The MRU is global and records every cwd change, so nvim's launch cwd is in the list alongside the dirs the test chdirs into.

#### `search.rs`

**Covers** — the content-search bridge: `start_search(query, use_regex, case_sensitive, all_files) -> FfiSearch`, the `files(budget)` file and match rows with their truncation flags and highlight ranges, `ignorecase()`, and `open_at(path, line, col)`. Real nvim plus a real `rg` child; target `//crates/wolf-ffi:search`, which additionally stages `//bazel/rg:rg_tree` and wires `WOLF_RG_BIN`, and skips loudly when either binary is missing.

**Gotchas** — the local `rg_bin_or_skip` re-exports `WOLF_RG_BIN` as an absolute path because Bazel injects it runfiles-relative, which the spawned `rg` could not resolve from a different cwd. The seeded `notes.txt` is there to prove the non-`all_files` filter drops it.

#### `state.rs`

**Covers** — the state-persistence bridge end to end: the snapshot-builder free functions, the pane-layout push-builder, the read side (`wolf_ffi_load_state` through the `FfiState` / `FfiWindowState` / `FfiTabState` / `FfiPaneNode` accessors), and rejection of an old flat file. Headless — the write side is a pure builder over the shared store, so target `//crates/wolf-ffi:state` stages no nvim and carries no `no-sandbox` tag; everything is driven through `WOLF_STATE_DIR` pointed at a tempdir.

**Gotchas** — one `#[test]` only: it mutates the process-global `WOLF_STATE_DIR` and opens the process-wide `SHARED_STORE` `OnceLock`, so extend that test rather than adding a sibling.

#### `tabpages.rs`

**Covers** — the tabpage handle methods the macOS front-end drives (`current_tabpage`, `create_tabpage`, `active_tabpage`, `set_current_tabpage`, `cwd_for_tabpage`, `close_tabpage`) plus their neutral values after `shutdown()`. Real nvim; target `//crates/wolf-ffi:tabpages` (staged nvim runfiles or `WOLF_NVIM_BIN`, `tags = ["no-sandbox"]`).

**Gotchas** — one `#[test]`, which also sets the process-global `WOLF_STATE_DIR` for shutdown-flush hygiene. The `:vsplit`-then-switch-away-and-back step is the point of the test, not setup: it proves each window handle re-resolves to a live, paintable grid after a tabpage switch.

#### `terminal.rs`

**Covers** — the terminal-drawer surface through `WolfCoreHandle::toggle_terminal()`: open, hide with the buffer surviving, reopen reusing that buffer, recreate after the buffer is wiped, the `DRAWER_OPEN` layout intent, and the drawer's full-width default. Real nvim; target `//crates/wolf-ffi:terminal` (staged nvim runfiles or `WOLF_NVIM_BIN`, `tags = ["no-sandbox"]`).

**Gotchas** — drive the handle method, not `:WolfTerminal`; the Ex command only enqueues an `FfiCommand` and there is no front-end dispatcher here to call the method back. Drawer buffers are identified by the `term://` prefix on buffer-picker rows, and because nothing drains layout intents they accumulate, so the test polls `drain_intents()` for the kind it wants.

#### `theme.rs`

**Covers** — `theme_titlebar_bg()` / `theme_titlebar_fg()` and the frame's pre-resolved `cursor_fill()` / `cursor_glyph()`, across defining and then clearing the `WolfTitlebar` and `WolfCursor` highlight groups. Real nvim; target `//crates/wolf-ffi:theme` (staged nvim runfiles or `WOLF_NVIM_BIN`, `tags = ["no-sandbox"]`).

**Gotchas** — follow every `:hi` mutation with an explicit `:doautocmd ColorScheme`: theme detection rides that autocmd and a plain `:hi` does not fire it. The target stages the repo `init.lua` with no lighthouse sibling, so no colorscheme defines the Wolf groups at startup; startup pickup from a real scheme is `lighthouse_load.rs`'s job.

#### `tree_search.rs`

**Covers** — the file-tree sidebar surface: `start_tree_search()`, `children_of(dir)` with the `FfiTreeNode` accessors, `rel_path_for(abs)`, the free function `wolf_ffi_ancestors_to_reveal`, and `cwd()` / `current_file()` round-tripped through `open`. Real nvim; target `//crates/wolf-ffi:tree_search` (staged nvim runfiles or `WOLF_NVIM_BIN`, `tags = ["no-sandbox"]`).

**Gotchas** — the seeded cwd carries a dotfile and a `.gitignore` plus the file it ignores on purpose; directories-before-files case-insensitive ordering, dotfile inclusion, and gitignore exclusion are all asserted against that exact layout. Path comparisons canonicalize, so the macOS `/var` to `/private/var` symlink does not break them.

#### `treesitter_highlight.rs`

**Covers** — the captive `init.lua` treesitter block, in two tests. The first opens a markdown buffer and reads back two sentinels: that treesitter is attached to it, and, the discriminating one, that a fenced `go` block yields an injected `go` langtree, which proves nvim-treesitter's `injections.scm` and the vendored grammar both reached `runtimepath`. The second covers the resolver in front of the gate: with decoy siblings staged and the environment also valid, the siblings win; a stale environment path with no siblings degrades to the bare-checkout no-op, observed as the absent augroup.

Real nvim plus queries and parsers trees from either provider, so both platforms run it; target `//crates/wolf-ffi:treesitter_highlight` adds `//bazel/nvim-treesitter:nvim_treesitter_tree` and `//bazel/treesitter-parsers:parsers_tree` to the nvim runfiles (`tags = ["no-sandbox"]`), and the first test skips loudly only where neither resolves.

**Gotchas** — the attached-to-the-buffer sentinel is a precondition, not evidence for the gate: the pinned Neovim starts treesitter on a markdown buffer by itself, so it reads true with the gate absent, and only the langtree sentinel discriminates. The `init.lua` block self-locates its siblings relative to its own directory, so the first test copies the real one and both trees into one temp dir; pointing nvim at the staged paths would not exercise the gate. The resolver test spawns nvim directly, because the bridge offers no per-spawn environment and mutating the process-wide one would turn its neighbour into a silent skip. Facts come back through a sentinel file nvim writes via `:lua vim.fn.writefile(...)`, since the bridge has no Lua-eval accessor.

#### `treesitter_parser_abi.rs`

**Covers** — that a compiled parser `.so` from a vendored parsers tree is ABI-compatible with the pinned Neovim: `vim.treesitter.language.add` by absolute path succeeds, and parsing yields an error-free `source_file` root. Real nvim plus a parsers tree from either provider — the Bazel-staged `//bazel/treesitter-parsers:parsers_tree` through `WOLF_TEST_PARSERS_DIR`, or the devShell's `WOLF_PARSERS_DIR`; target `//crates/wolf-ffi:treesitter_parser_abi` wires the first alongside the nvim runfiles (`tags = ["no-sandbox"]`). It skips loudly only when neither resolves, or when the go parser is missing from the one that did.

**Gotchas** — the two providers lay the tree out differently, nested `parsers/parser/<lang>.so` under Bazel and flat `parser/<lang>.so` from the devShell's `symlinkJoin`, and the probe accepts either. Scope is ABI only: it uses the captive `init.lua` unchanged, stages no sibling bundle, and never enables highlighting, which is `treesitter_highlight.rs`'s job. Results come back through a sentinel file, as with that sibling.

#### `unicode_key.rs`

**Covers** — the composed-character delivery path: a modifier-free `FfiKeyEvent { kind: char }` per Unicode scalar, the exact shape Swift's `KeyComposition.charEvents` emits, lands verbatim in insert mode both singly and as a two-scalar run that wolf-core fuses into one `nvim_input`. Real nvim; target `//crates/wolf-ffi:unicode_key` (staged nvim runfiles or `WOLF_NVIM_BIN`, `tags = ["no-sandbox"]`).

**Gotchas** — `FfiFrame` carries no sequence number, so a new case polls `read_frame()` and searches the flattened grid text rather than asserting once.

#### `untitled.rs`

**Covers** — anonymous-buffer naming over the existing `new_buffer` / `chdir` / `open` / `current_file` / `is_markdown_active` surface: the un-numbered first `Untitled.md`, the collision walk over both live buffers and files on disk, the absence of any write before an explicit `:w`, and chdir re-homing an untouched anonymous buffer while leaving a real file alone. Real nvim; target `//crates/wolf-ffi:untitled` (staged nvim runfiles or `WOLF_NVIM_BIN`, `tags = ["no-sandbox"]`).

**Gotchas** — each `#[test]` starts its own core through the local `start_core` on a fresh temp cwd, and the on-disk-collision case depends on that cwd being empty except for the file it seeds.
