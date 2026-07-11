# wolf-ffi/tests

Integration tests for the `wolf-ffi` bridge surface. They drive the **Rust half** of the swift-bridge boundary directly — no Swift, no AppKit, no window — exactly as the generated Swift glue eventually will. Each test is its own binary (see `BUILD.bazel` for the full list; not every binary is documented here yet), sharing the `common/` helper.

## `frame_pack.rs`

Headless, deterministic coverage of `FfiFrame`'s snapshot → frame packing. It builds a `wolf_core::Snapshot` **by hand** (no nvim, no `WolfCore`) and asserts exactly what `FfiFrame::from_snapshot` produces: explicit-highlight color packing to `0x00RRGGBB` with `set = true`; unset colors packing to `set = false, rgb = 0`; the style-flag bitset (bold/italic/underline/undercurl/strikethrough) packing to exact bits; that `reverse` is applied **once** by `HighlightTable::resolve` and not re-swapped by the bridge; cursor shape → `u8`; `cursor_visible` true only on the active grid; the reverse-video cursor `fill`/`glyph` (plain inversion, the mode-cursor `attr_id` override when distinct, and the `WolfCursor` theme override — a set `theme.cursor.bg` wins as the fill, a bg-only theme keeps the reverse-video glyph as the cell bg, and the theme beats an otherwise-honorable mode cursor attr); the default fg/bg; and that `cells()` is a dense row-major `cols * rows` grid. It needs no `WOLF_NVIM_BIN` and runs **regardless** — real, non-mock coverage of the resolution/packing logic that otherwise only runs when a live nvim is present.

## `bridge_smoke.rs`

The real-nvim smoke test. It drives a hermetic, pinned Neovim through the bridge — **no mocks**, per the project testing policy — constructing `FfiKeyEvent`s by hand and calling `wolf_ffi_start` → `read_frame` (dims/cursor sane, `cells()` == `cols * rows`) → `send_key` (`i` then `x`, polling `read_frame` until the typed `x` appears) → `resize` (polling until the dims update) → `shutdown`. Because the `FfiFrame` surface exposes no `seq`, it **polls** `read_frame` on a timeout rather than keying on a flush counter (the same pattern wolf-core's `current_file` / multigrid tests use).

It runs for real on Linux/Cargo devShells and **passes under `bazel test` on the macOS Bazel host** too: the `bridge_smoke` target stages the captive `init.lua` and the pinned Neovim tree (`//bazel/nvim:nvim_tree`) as runfiles (via `data` + the `$(rootpath)` env vars `WOLF_TEST_INIT_LUA` / `WOLF_TEST_NVIM_DIR`), so a real nvim is always available to drive hermetically; an explicit `WOLF_NVIM_BIN` still wins. Because spawning a real nvim child does not work under the macOS build sandbox, the test runs `tags = ["no-sandbox"]` (unsandboxed, but still test-result-cached). Unlike the wolf-core suite (which panics when no nvim is available), this test **loudly SKIPs** (a no-op pass with a stderr banner) only when NEITHER an explicit `WOLF_NVIM_BIN` NOR the staged tree is available. It never calls the Swift wake — the Swift-free `extern "C"` wake path means `wolf_ffi_start` links and runs with no Swift runtime present.

## `file_search.rs`

The real-nvim coverage of the file-picker surface — **no mocks**, modeled on `bridge_smoke.rs` (same `common/` helpers, same `data` / `$(rootpath)` env runfile staging, same `tags = ["no-sandbox"]` unsandboxed run, same loud-SKIP gate via `nvim_bin_or_skip`). It seeds a temp cwd with a couple of `.md` files, calls `wolf_ffi_start` there, opens a session with `start_file_search()`, and **polls** `ranked("", N)` until the background traversal streams the created files in; it then asserts the displays are present, that `path()` is absolute (under the cwd) while `display()` is the relative name, and that a focused query (e.g. `ranked("alpha")`) ranks the matching file first. Finally it issues `open(path())` and confirms the core survives the round-trip (it cannot assert the exact opened buffer name — `FfiFrame` exposes no `current_file` accessor, and the test does not add one — so `open` is checked at the round-trip-survives level, `ranked` correctness strongly).

A second test, `file_search_bounds_large_tree`, is the real-traversal guard for the BOUNDED ranking (`FfiFileSearch::ranked` now delegating to `wolf_core::rank_matches_top_n`): it seeds a large temp tree (thousands of files, many sharing an `alpha`-matching name plus one root `alpha.md`), polls `ranked("", MAX_RESULTS)` until the walk streams in, then asserts `ranked("alpha", MAX_RESULTS)` returns EXACTLY `MAX_RESULTS` rows (bounded — the top-N select never returns the whole match set), ranks the root `alpha.md` first, and that every returned `display()` still contains the query as a subsequence.

## `file_actions.rs`

The real-nvim coverage of the file-action handle methods (`file_actions_chdir_and_new_buffer`) — **no mocks**, modeled on `commands.rs` (same `common/` helpers, same `data` / `$(rootpath)` env runfile staging, same `tags = ["no-sandbox"]` unsandboxed run, same loud-SKIP gate, plus `tempfile`). After attach it `chdir`s to a fresh temp directory and polls `cwd()` until it equals the canonicalized target, then `open`s a seeded file (polling `current_file()` until it matches) and `new_buffer()`s (`:enew`), polling `current_file()` back to the empty string, before `shutdown()`. (This test formerly exercised `close_buffer()`/`:bdelete`; the `:close` split-drop path moved to `close_pane.rs`.)

## `close_pane.rs`

The real-nvim coverage of the `close_pane()` handle method (`:close`) — **no mocks**, modeled on `panes.rs` (same `common/` helpers, same runfile staging, same loud-SKIP gate). After attach it types `:vsplit` to reach two live editor grids (`windows()`), then calls `handle.close_pane()` **directly over RPC** (which bypasses init.lua's interactive `:close` override — `nvim_command` is not the typed Ex-command), and polls `windows()` back to a single live grid.

```sh
bazel test //crates/wolf-ffi:close_pane
```

## `copy_paste.rs`

The real-nvim coverage of the copy/paste + visual-mode handle surface (`is_visual_mode()` / `copy_selection()` / `paste(text)`, Bazel target `//crates/wolf-ffi:copy_paste`) — **no mocks**, modeled on `close_pane.rs` (same `common/` helpers, same runfile staging, same loud-SKIP gate, same `read_frame` live-grid polling). A single `#[test]` (`copy_selection_and_paste_round_trip`): after attach it confirms a fresh buffer is NOT visual (`is_visual_mode() == false`), types a known word (`i` + `WOLF` + `<Esc>`), enters a character-wise selection (`0 v e`), polls `is_visual_mode()` true and asserts `copy_selection()` contains the word, then `<Esc>` back to normal (`is_visual_mode() == false`), and finally `paste("Z")` polling `read_frame()` until the glyph appears in the grid. It exercises the discriminant-free handle methods (the `:WolfCopy`/`:WolfPaste` drained-command discriminants `14`/`15` are covered by `commands.rs`).

```sh
bazel test //crates/wolf-ffi:copy_paste
```

## `buffer_picker.rs`

The real-nvim coverage of the buffer-picker session + view round-trip (Bazel target `//crates/wolf-ffi:buffer_picker`) — **no mocks**, modeled on `command_palette.rs` (same `common/` helpers, same runfile staging, same loud-SKIP gate, same `read_frame` live-grid polling). A single `#[test]` (`buffer_picker_lists_switches_and_restores_view`): it seeds a temp cwd with a few `.md` files, opens buffers A then B a strict second apart (a `sleep(1100ms)` straddled by `wait_for_active_filename` before/after, so B gets a later `lastused` second than A — `getbufinfo().lastused` has 1-second resolution, and equal `lastused` correctly tie-breaks by `bufnr` ASC per the headless `buffer_rank` spec), plus a `:badd` buffer C that is listed-but-unloaded (proving `list_buffers` filters by `buflisted`, not loadedness) and an unlisted buffer that must NOT appear. It asserts `buffer_picker().ranked("", N)` lists the `buflisted` buffers MRU-first (B before A before C), switches the active pane via `switch_buffer(bufnr)` (observing the change through `FfiFrame`), and checks the per-`(winid, bufnr)` VIEW round-trip: scroll buffer A (capture `topline() > 0`), switch to B, switch back to A, and assert the topline is RESTORED; a buffer the pane never showed opens at the top (`topline() == 0`).

```sh
bazel test //crates/wolf-ffi:buffer_picker
```

## `pane_status.rs`

The real-nvim coverage of the per-pane statusbar bridge surface (Bazel target `//crates/wolf-ffi:pane_status`) — **no mocks**, modeled on `panes.rs` (same `common/` helpers, same runfile staging, same loud-SKIP gate via `nvim_bin_or_skip`). It seeds a fresh temp cwd with `alpha.md` / `beta.md`, calls `wolf_ffi_start` there, polls `read_frame()` until attached and `windows()` down to one editor grid, then `:e alpha.md` and polls `pane_status_for_grid(grid).filename()` until it reports `alpha.md` with a non-empty `display_path()`. After a `:vsplit` (polling `windows()` to two grids) and `:e beta.md` in the new window, it polls until the two window grids report DISTINCT filenames (`alpha.md` vs `beta.md`), proving the status is per-WINDOW rather than a single global value.

## `state.rs`

The HEADLESS coverage of the state-persistence bridge surface (Bazel target `//crates/wolf-ffi:state`) — **NO nvim** (the write side is a pure builder over the process-wide `SHARED_STORE` + a `Mutex` scratch tree, so no captive Neovim is needed), driven purely through `WOLF_STATE_DIR` like `config.rs`. It is deliberately a **single `#[test]`** (`snapshot_builder_round_trips_the_whole_tree`) because it mutates the process-global `WOLF_STATE_DIR` AND opens the process-wide `SHARED_STORE` `OnceLock`. The flow: with `WOLF_STATE_DIR` pointed at a tempdir, `wolf_ffi_load_state()` on the empty dir yields zero windows (and the file is never created); it builds a 2-window / 3-tab tree via `wolf_ffi_state_snapshot_begin` / `add_window(x,y,w,h)` / `add_tab(cwd, sidebar)` / `commit`, `wolf_ffi_state_flush()`es, and polls `wolf_ffi_load_state()` until the tree lands — asserting geometry + per-tab cwd/sidebar/order through the `FfiWindowState` / `FfiTabState` accessors and a pretty-printed version-1 `state.json`; finally an OLD flat (pre-tabs, no `tabs`) file overwritten in place loads back as zero windows and is preserved byte-for-byte.

## `multi_window.rs`

The real-nvim coverage of the MULTI-WINDOW wake-routing bridge surface (Bazel target `//crates/wolf-ffi:multi_window`) — **no mocks**, the sibling of `panes.rs` (same `common/` helpers, same runfile staging, same loud-SKIP gate). Deliberately a **single `#[test]`** because it registers the process-global wake callback (and sets `WOLF_STATE_DIR` for shutdown-flush hygiene). It starts TWO real cores at distinct cwds — `wolf_ffi_start(init_lua, cwd, cols, rows)` (4-arg; no `window_index`) — then asserts: **distinct identity** (`hA.wake_token() != hB.wake_token()`); **independent snapshots** (a marker typed into A appears in `hA.read_frame()` but never in B's); and **wake routing not clobbered** (a process-global `extern "C" fn(u64)` records per-token wake counts into a `Mutex<HashMap<u64,u32>>`, and driving input into A advances only A's token, driving B only B's — neither bumps the other). The former per-index state section is gone — the whole-tree snapshot round-trip is covered headlessly by `state.rs`.

## `orlando_bg_stress.rs`

Regression test for the "occasionally cells draw with the wrong background" bug (single default-black cells inside an orlando `OrlandoCodeBlock` gray region, persisting on screen). Root cause: Neovim ends many `grid_line` events with a zero-width clear-to-EOL cell `[" ", hl 0, repeat 0]`, and wolf-core's parser clamped `repeat` with `.max(1)`, writing a spurious default-highlight space one column past the event's real span; Neovim's shadow grid never has that cell, so it is never re-sent and the corruption persists until unrelated churn rewrites it. The test stages the vendored orlando (and lighthouse, when present) trees like `orlando_load`, opens a Markdown document with two fenced code blocks, then drives a seeded-random mix of scrolling, cursor motion, in-block edit+undo, and fence insert/delete churn; after every op it waits for the frame to go idle and asserts (1) every visible row's text equals the corresponding buffer line and (2) every cell of every code-block row shares one background, which must differ from the default bg so the check cannot pass vacuously. This reproduced the corruption deterministically before the fix. Seed and iteration count are overridable via `WOLF_STRESS_SEED` / `WOLF_STRESS_ITERS`.

## `common/mod.rs`

Shared helpers mirroring `crates/wolf-core/tests/common/mod.rs`, with the one deliberate policy difference (skip, not panic, when nvim is absent):

- `nvim_bin_or_skip()` — the skip gate: returns the nvim binary path, preferring an explicit `WOLF_NVIM_BIN`, else the binary inside the staged pinned tree (`WOLF_TEST_NVIM_DIR`, the `//bazel/nvim:nvim_tree` runfiles); returns `None` after printing a loud stderr banner only when neither is available (the caller then `return`s — a no-op pass).
- `init_lua_path()` — resolves the captive `runtime/nvim/init.lua`, preferring the staged runfiles copy (`WOLF_TEST_INIT_LUA`) and falling back to the in-repo path under `CARGO_MANIFEST_DIR` (`crates/wolf-ffi`) for the cargo/devShell path. Using the staged runfiles under Bazel is what fixed the prior hang — `CARGO_MANIFEST_DIR` points at an ephemeral sandbox path that does not exist at test runtime, so nvim could not read its init and blocked on the "Press ENTER" prompt forever.
- `fresh_cwd()` — a fresh, unique, **non-auto-deleted** temp dir for a core's `cwd` (the MRU cache under `<cwd>/.wolf/` must outlive `wolf_ffi_start`).
- `DEFAULT_COLS` / `DEFAULT_ROWS` (80 × 24) — the smoke test's starting grid size.
