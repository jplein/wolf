# wolf-core/tests

Integration tests for `wolf-core`. They drive a **real, hermetic Neovim** child process — no mocks, per the project's testing policy (the whole point of core is to talk to a Neovim process, so the tests actually start one).

## How they run

Neovim is never taken from `PATH`. It is located one of two ways, matching how the suite is invoked:

- **Cargo / Linux devShell:** Neovim is located via **`WOLF_NVIM_BIN`** (exported by the flake devShell), and `init.lua` is resolved from `CARGO_MANIFEST_DIR`. Enter the devShell (`direnv allow`) before running, then `cargo test -p wolf-core`. An explicit, absolute, existing `WOLF_NVIM_BIN` always wins here.
- **Bazel (macOS host):** `bazel test //crates/wolf-core:file_search` runs the real-nvim file-search integration tests. The `rust_test` stages the pinned `//bazel/nvim:nvim_tree` and the captive `//runtime/nvim:init.lua` as runfiles and injects their (runfiles-relative) paths via `WOLF_TEST_NVIM_DIR` / `WOLF_TEST_INIT_LUA`; `common/mod.rs` resolves those to ABSOLUTE paths before the captive nvim is spawned (it is launched with `current_dir(cfg.cwd)`, so a relative binary path would not resolve). The target is tagged `["no-sandbox"]` (a real nvim child cannot spawn under the macOS build sandbox). The harness mirrors `crates/wolf-ffi/tests/common/mod.rs`'s runfiles resolution.

Other properties:

- They are fully **headless**: `nvim --embed` speaks msgpack-RPC over stdio and needs no TTY, so the suite is CI-safe.
- They **fail loud** if no Neovim can be resolved: `require_nvim_bin` panics with an actionable message rather than silently skipping, so a misconfigured env surfaces immediately. (On both real environments resolution always succeeds.)
- Each test waits for redraw `flush`es with a generous timeout (`FLUSH_TIMEOUT = 10s`) to absorb nvim `--embed` startup and the inherently asynchronous redraw.

## `common/mod.rs`

Shared helpers used across the test binaries:

- `require_nvim_bin()` — resolves the pinned nvim to an ABSOLUTE path and returns it (also re-exporting it into `WOLF_NVIM_BIN` so the core spawns the right binary regardless of cwd). Resolution order: an explicit absolute-and-existing `WOLF_NVIM_BIN` (cargo / devShell), then the Bazel-staged tree via `WOLF_TEST_NVIM_DIR`. Panics with an actionable message if neither resolves.
- `init_lua_path()` — resolves the captive `runtime/nvim/init.lua`: the Bazel-staged runfile (`WOLF_TEST_INIT_LUA`, resolved to an absolute path) when present, else `<CARGO_MANIFEST_DIR>/../../runtime/nvim/init.lua` (cargo).
- `resolve_runfile(env_key)` — joins a Bazel-injected runfiles-relative path against the known bases (`TEST_SRCDIR` / `RUNFILES_DIR`, …) to an absolute path; a byte-for-byte port of wolf-ffi's helper.
- `fresh_cwd()` — a fresh, persistent temp directory (NOT a dropped guard) to serve as a test core's `cwd`, so the MRU cache `.wolf/` and the picker's search root are isolated per run.
- `config(cols, rows)` — builds a `CoreConfig` pointing at that `init.lua`, using a `fresh_cwd()` as the `cwd`.
- `config_in(cwd, cols, rows)` — same, but with an explicit `cwd` (used by the MRU and file-search tests that assert against a known directory layout). It is also the single funnel that keeps the always-on global recent-directories store hermetic: before building the `CoreConfig`, if `WOLF_STATE_DIR` is UNSET it points it at a process-global temp dir (a `OnceLock`, one per test process), so a test that does not manage its own state dir never writes `folder-mru.sqlite` into the real `~/Library/Application Support/Wolf`. Set-if-absent, so a test that sets its own `WOLF_STATE_DIR` first (the recent-dirs real-nvim test) is not clobbered, and `state_save.rs` (which injects an explicit `StateStore::open(dir)`) is unaffected.
- `wake_channel()` → `(WakeFn, FlushWaiter)`. The `WakeFn` is an `mpsc::Sender<()>` closure (`Send + Sync`, since the wake may fire on a tokio worker thread) that signals on **every** wake — both real redraw flushes and out-of-band wakes such as the `wolf_current_file` notification. The `FlushWaiter` is therefore **seq-based** (it keys on the snapshot's `seq`, the redraw-frame counter, not on the raw wake count, so a `current_file` wake — which does NOT bump `seq` — is consumed and ignored rather than mistaken for a flush):
  - `wait_for_flush(&core, timeout)` blocks until `core`'s snapshot `seq` advances beyond its value at the call (panics on timeout/disconnect). Used for post-input waits, where the input pump's coalesce delay guarantees the flush is still in the future when the baseline is captured.
  - `wait_for_initial_flush(&core, timeout)` blocks until `seq >= 1` (the attach flush). It keys on that ABSOLUTE threshold rather than an advance because the attach redraw (`seq` 0 → 1) lands DURING `WolfCore::start`, so it usually returns immediately; an advance-based wait would baseline at 1 and block forever. Used for the startup/attach wait in `start_core_sized` and the first wait in `start_core_with_temp_md_sized`. Robust to out-of-band current_file wakes.
  - `drain()` discards queued wakes without blocking.
- `temp_md(content)` / `temp_md_path()` — a `.md` temp file with content, or a not-yet-created path for `SaveAs`.
- `start_core_with_temp_md[_sized](content[, cols, rows])` — starts a core, opens the temp `.md`, and waits for the open to land. `start_core_sized(cols, rows)` starts a bare core (no file).
- Input constructors: `key(Key)`, `ch(char)`, `ctrl(Key)`, `type_str(core, s)`.

## Coverage

Integration tests across the `tests/*.rs` binaries (driving a real pinned Neovim and/or real SQLite/filesystem — plus the fully headless `state_unit.rs`, `recent_dirs.rs`, `command_mru.rs`, `palette_rank.rs`, `buffer_rank.rs`, `rank_bounded.rs`, and `pane_status_unit.rs`, which need neither Neovim nor a filesystem), plus the pure in-module `#[cfg(test)]` unit tests (eighteen for the ranking function in `rank.rs`, ten for layout-intent parsing in `layout.rs`, and the `config.rs` / `tree.rs` suites documented in [`src/README.md`](../src/README.md)).

### File picker (real SQLite / filesystem / git / Neovim — no mocks)

- **MRU cache** (`mru.rs`, real SQLite + fs, six tests) — `open` creates `.wolf/mru.sqlite` and `recent()` starts empty; three records come back most-recent-first with 0/1/2 ranks; re-recording an older path moves it to the front without duplicating; deleting a file omits it from `recent()` AND prunes its row from the cache; `recent()` is capped at `MRU_LIMIT`; `display` is relative to the cwd while `path` is absolute.
- **Open records MRU** (`mru_open_records.rs`, real Neovim, two tests) — `open_records_file_in_mru`: after `start_core_with_temp_md`, `mru_recent()` contains the opened path (the input pump recorded the `Open`). `open_after_cd_records_in_nvim_cwd_mru`: the WRITE path follows nvim's live cwd — start the core in dir A, drive `:cd <B>` as keystrokes, then open `B/written.md` via `InputEvent::Open`; the pump must record it into B's store (`<B>/.wolf/mru.sqlite`, B-relative `display`), `mru_recent()` reports B's store, and A's store must NOT have leaked it.
- **Streaming traversal** (`file_search.rs`, real fs + real git via shelled `git init`, eleven tests, polling until `complete`) — a non-repo lists plain files + dotfiles but no directories; symlinks are included only when they target a plain file; a symlink loop does not hang; a git repo respects `.gitignore` (cwd and nested/ancestor) and excludes `.git/` while including untracked files; the `.wolf/` cache is excluded; the MRU seed is present synchronously and not duplicated when rediscovered; `cancel` stops streaming.
- **Search root follows Neovim's cwd** (`file_search.rs`, real Neovim + fs) — `file_search_root_follows_nvim_cd` starts the core in dir A, drives `:cd <B>` into a second dir as keystrokes, waits for the redraw flush, then asserts the search lists B's files and NOT A's sentinel (the root follows nvim's live `getcwd()`); `file_search_root_defaults_to_cfg_cwd_without_cd` starts the core in A with no `:cd` and asserts A's file is listed (proving the captive nvim now spawns in `cfg.cwd`, so `getcwd() == cfg.cwd` is the fallback root).
- **MRU seed follows Neovim's cwd** (`file_search.rs`, real Neovim + fs, `mru_seed_follows_nvim_cd`) — the headline regression guard for the screenshot bug: record a nested recent file into B's store and a different recent file into A's store, start the core in A, `:cd <B>`, then open the picker. The seed must come from `<B>/.wolf` and show RELATIVE to B (`sub/recent_b.md`, carrying an `mru_rank`), A's MRU must NOT leak into B's seed, and the store must physically live at `<B>/.wolf/mru.sqlite`.

### Application state (headless + real Neovim)

- **State module unit coverage** (`state_unit.rs`, Bazel target `//crates/wolf-core:state_unit`, twelve tests, NO nvim — everything drives the public `state` API through explicit tempdirs, the pure analog of `//crates/wolf-ffi:config`) — store round-trip through `StateStore` setters → `flush` → `load_state` (pretty-printed, `"version": 1`); the unset-geometry sentinel (`set_cwd` alone writes zero geometry); the silently-ignore matrix (absent file → `None` and `open` yields defaults; junk bytes / wrong shape / missing required field / `version: 2` → `None` with the on-disk bytes preserved byte-for-byte — `load_state` never creates or modifies the file); unknown extra fields still parse; `legacy_toolbar_display_mode_key_is_ignored` (a backward-compat guard: an old `state.json` carrying the removed `toolbar_display_mode` key still deserializes, its other fields surviving verbatim); rewriting window 0 preserves a hand-written window 1 verbatim; `set_cwd` beyond the list length grows it with defaults; concurrent setters from many threads produce a clean parseable file with no `.tmp` residue; rapid mutations coalesce to the last value; `mutate_then_immediate_drop_is_persisted` (mutate, drop the store WITHOUT flushing, poll `state.json` until the mutation appears — pinning the drop-without-flush persistence contract the GNOME front-end will rely on); and `state_dir` honoring `WOLF_STATE_DIR` (env-mutating, under a local env lock).
- **Save-on-cwd-change flow** (`state_save.rs`, Bazel target `//crates/wolf-core:state_save`, real Neovim, one test, mirroring `mru_open_records`'s runfile staging) — `cwd_changes_persist_to_state_json`: start the core in tempdir A, `StateStore::open` an explicit tempdir B (so `WOLF_STATE_DIR` is never touched), `attach_state_store`, then poll `state.json` until window 0's cwd equals canonicalized A (the attach-time push must cover the setup-time `wolf_cwd` emission that can race ahead of the attach); type `:cd <C>` through the normal input path and poll until the persisted cwd equals canonicalized C (`/var` → `/private/var` robust); assert the file is pretty-printed.

### Recent directories (headless + real Neovim)

- **Recent-dirs store unit coverage** (`recent_dirs.rs`, Bazel target `//crates/wolf-core:recent_dirs`, seven tests, NO nvim — everything drives `RecentDirs::open(tempdir)` directly, so it needs neither `WOLF_STATE_DIR` nor an env lock, the pure analog of `state_unit.rs`) — `open` creates `folder-mru.sqlite` and `recent()` starts empty; records come back newest-first; re-recording a path moves it to the front without duplicating (monotonic `seq`, no sleeps); `recent()` is capped at `RECENT_DIRS_LIMIT` and the oldest rows are physically pruned (a second `RecentDirs` on the same dir still sees them gone, proving a `DELETE` not just a `LIMIT`); a garbage DB and a wrong-schema DB (a `recent_dirs` table missing the `seq` column, which the probe `SELECT path, seq … LIMIT 0` forces to surface) are BOTH silently recreated with no panic/error; and a `disabled()` store is a no-op.
- **Record-on-cd flow** (`recent_dirs_records.rs`, Bazel target `//crates/wolf-core:recent_dirs_records`, real Neovim, one test, mirroring `state_save`'s runfile staging) — `cd_records_directory_at_top_of_recent_dirs`: with an explicit `WOLF_STATE_DIR` set to a fresh tempdir BEFORE the core starts, drive `:cd <B>` then `:cd <C>` through the normal input path and poll until the typed target sits at the top of the MRU, asserted via BOTH read paths — `core.recent_dirs()` and a fresh `RecentDirs::open(state_dir).recent()` (confirming the on-disk row) — accounting for the setup-time initial `wolf_cwd` emission that can be recorded before the typed `:cd`.

### Current-file tracking (real Neovim)

`current_file.rs` (four tests) verifies `Snapshot.current_file` against a real, hermetic nvim. Because the `wolf_current_file` notification is SEPARATE from redraw flushes, these tests **poll** `read_snapshot().current_file` (draining wakes) with a generous timeout, canonicalizing both sides (`/tmp` is often symlinked):

- opening a file via `InputEvent::Open` sets `current_file` to its canonical path;
- opening a file by **typing `:e <path><CR>`** as keystrokes (not via `Open`) also sets it — exercising the autocmd reveal path for nvim-driven buffer changes;
- a core started with no file open (unnamed buffer) reports `None` (the initial emit's empty `expand('%:p')` maps to `None`);
- `current_file` follows a buffer switch (open A, then open B → it moves to B).

### Per-pane buffer tracking (real Neovim)

`pane_buffers.rs` (Bazel target `//crates/wolf-core:pane_buffers`, four tests) verifies the per-pane statusbar CORE surface: `Snapshot.buffers` (the per-window buffer-path map fed by the `wolf_buffers` notification) and `WolfCore::pane_status(grid)` (which joins grid → window → buffer path and formats it via the pure `pane_status`). Like `current_file.rs`, the notification is separate from redraw flushes, so the tests **poll** `pane_status(grid)` / `read_snapshot().windows` with a generous timeout, draining wakes, and set an explicit temp `cwd` (opening files INSIDE it) so the cwd-relative `display_path` is deterministic (canonicalizing both sides, since macOS `/var`/`/tmp` are symlinks):

- opening a file nested under cwd reports its base name as `filename` and the cwd-relative remainder as `display_path`;
- after a `:vsplit` + `:e` of a DIFFERENT file B in the new window, the two window grids report DISTINCT filenames (proving the map is per-WINDOW, not global);
- an unnamed startup buffer reports `[No Name]` with an empty `display_path` (it is absent from the map);
- closing one split drops THAT window's grid from `windows`, and the survivor keeps reporting its own file.

### Pure per-pane statusbar strings (headless)

`pane_status_unit.rs` (Bazel target `//crates/wolf-core:pane_status_unit`, nine tests, NO nvim / fs / GUI — the pure analog of `palette_rank.rs`) exercises the IO-free `pane_status` display-string function directly: the empty→`([No Name], "")` case, cwd-relative remainder (nested + direct), the `~`/home-relative and path==home cases, the absolute passthrough, the path==cwd edge (must NOT emit an empty relative string), cwd-over-home precedence, and the `cwd=None,home=None` absolute passthrough.

### Theme tracking (real Neovim)

`theme.rs` (Bazel target `//crates/wolf-core:theme`, four tests) verifies `Snapshot.theme` — the `WolfCursor` / `WolfTitlebar` app-theme colors delivered by the `wolf_theme` notification (`ThemeColors { cursor, titlebar }`, each channel an `Option<Rgb>`) — against a real, hermetic nvim started with the plain repo `init.lua` (no lighthouse staged, so no colorscheme defines the Wolf groups at startup). Like `current_file.rs`, the notification is separate from redraw flushes (it wakes but does not bump `seq`), so the tests **poll** `read_snapshot().theme` with a generous timeout, draining wakes; every `:hi` mutation is followed by an explicit `:doautocmd ColorScheme`, because a plain `:hi` does NOT fire the `ColorScheme` autocmd the tracker rides (only `:colorscheme` / a `:set background=` re-source do):

- startup with no Wolf groups reports the all-`None` default (the helper's emit-once arrived and parsed as unset);
- a runtime `:hi WolfCursor` / `:hi WolfTitlebar` + `:doautocmd ColorScheme` surfaces the exact packed colors, with any channel the `:hi` left unset staying `None`;
- a linked group (`:hi link WolfCursor Visual`) resolves to its target's effective attributes (the helper reads with `link = false`);
- clearing both groups (`:hi clear …` + `:doautocmd ColorScheme`) falls back to all-`None`.

### Copy / paste + visual mode (real Neovim)

`copy_paste.rs` (Bazel target `//crates/wolf-core:copy_paste`, four tests) verifies the Edit-menu copy/paste surface against a real, hermetic nvim, reusing `start_core_with_temp_md` and polling `is_visual_mode()` across a few flushes (a stray startup-tail redraw can satisfy a single `wait_for_flush` before the mode_change lands):

- `is_visual_mode_tracks_v_bar_and_ctrl_v` — a freshly opened buffer is normal mode (`is_visual_mode() == false`); each of `v`, `V`, and `<C-v>` reports visual, and `<Esc>` returns to normal;
- `copy_visual_selection_yields_the_selected_text` — `gg0` then `v e` selects the fixture's distinctive first word, and `WolfCore::copy_visual_selection()` returns `Some(text)` containing it (pinned to nvim's visual-`y` unnamed-register semantics, synchronous);
- `copy_outside_visual_mode_is_none` — in normal mode `copy_visual_selection()` returns `None` (the mode guard that also prevents a `y` operator-hang);
- `paste_inserts_text_into_the_active_grid` — `WolfCore::paste("WOLFPASTE")` into an empty buffer surfaces the text on row 0 of the active grid (via `nvim_paste`).

The `command.rs` unit tests (`copy`/`paste`/`buffer_picker`/`close_tab` `from_id` round-trips, spec-field pins, and the append-only `COMMAND_SPECS[14] == Copy` / fixed-index `[15] == Paste` / `[16] == BufferPicker` / last-row `[17] == CloseTab` index guards, `EXPECTED_COMMAND_COUNT == 18`) are the pure in-module coverage of the registry additions (documented in [`src/README.md`](../src/README.md)).

### Ranking (pure, in-module)

`rank.rs` has eighteen table-driven `#[cfg(test)]` unit tests (no fs/Neovim): empty query returns all entries MRU-first; case-insensitive matching; subsequence match and non-match exclusion; MRU outranks a stronger non-MRU match; strict-prefix beats a scattered subsequence; word splitting on separators; full-word match tops the prefix group; deterministic length/lexicographic tie-breaks; the decisive `match_start` (key 5) and `span` (key 6) tie-breaks; the multi-term AND-matching cases (space-separated terms are all required, order-independent, and extra whitespace is harmless — an entry missing any term is dropped); and the scoring-tier cases (MRU dominates a stronger non-MRU match, component-start beats scattered, contiguity beats scattered among non-component matches, and more/stronger per-term hits score higher).

### Bounded ranking equivalence (headless)

`rank_bounded.rs` (Bazel target `//crates/wolf-core:rank_bounded`, nine tests, NO nvim / fs / GUI — the pure analog of `palette_rank.rs`) is the headless equivalence guard for the picker's BOUNDED top-N ranking. Its one property, asserted structurally with `assert_eq!` across every case, is that `rank_matches_top_n(query, &entries, n)` equals `rank_matches(query, &entries).into_iter().take(n).collect()` for all inputs — never re-deriving the expected order by hand, so the bounded heap's `index` final-tiebreak is pinned to the unbounded stable-sort `take(n)`. Cases span the empty and all-whitespace query (MRU-mixed set), a single term, multi-term AND, order-independence, no-match, a scoring tie group straddling the `n` boundary, an MRU entry weakly beating a stronger non-MRU match, `n` larger than the match count, `n == 0`, `n == 1`, a large synthetic in-memory set at `n = 200`, and an ASCII-fold parity check with a non-ASCII display/query char.

### Buffer ranking (headless)

`buffer_rank.rs` (Bazel target `//crates/wolf-core:buffer_rank`, five tests, NO nvim / fs / GUI — the pure analog of `palette_rank.rs`) exercises the IO-free `rank_buffers` directly: an empty query lists all buffers MRU-first (`lastused` DESC, current buffer at index 0); a fuzzy query filters + keeps the MRU-first order among matches; the display projection covers the cwd-relative, absolute-fallback, and `[No Name]` cases; the result truncates to `max`; and equal `lastused` values tie-break by `bufnr` ASCending (the invariant the FFI test's 1-second-resolution timing must respect).

### Layout-intent parsing (pure, in-module)

`layout.rs` has ten table-driven `#[cfg(test)]` unit tests (no fs/Neovim) for `LayoutIntent::from_payload`, the `wolf_layout` payload parser: a valid vertical and horizontal `split` (preserving `from`/`win`); a valid `focus_dir` and that every `dir` string (`left`/`right`/`up`/`down`) maps to the right `FocusDir`; and the malformed cases that must all yield `None` — a missing `action`, an unknown `action`, a `split` missing `from`/`win`, a `split` with a bad `orientation`, a `focus_dir` with a bad `dir`, and a non-map payload (a bare integer).

### Redraw / input pipeline (real Neovim)

The original fifteen tests cover the redraw → snapshot and input pipelines against the real pinned Neovim:

- **Spawn / attach** — grid size matches the requested cols/rows; default colors populated; `mode_infos` non-empty; clean `shutdown`.
- **Typing** — `i` + text + `Esc` lands the text on the grid; the typed cell's `hl_id` resolves to a non-`None` foreground.
- **Open existing Markdown** — an opened `# Title` shows on row 0; a heading's `hl_id` differs from body text (built-in markdown syntax).
- **Cursor movement** — `j`/`l` advance `cursor.row`/`cursor.col`; entering insert mode selects the insert `ModeInfo` (vertical-bar shape).
- **Resize** — a resize request updates the grid cols/rows (`nvim_ui_try_resize` → `grid_resize`).
- **Save / SaveAs** — typed text written with `Save` appears in the file on disk; `SaveAs` creates a new path with the content.
- **Scroll / viewport** — scrolling a tall buffer increases `viewport.topline`; `win_viewport.linecount` matches the buffer line count.
- **`grid_line` decoding** — direct `[text, hl, repeat]` expansion (column advance) and wide-character handling (a double-width glyph followed by Neovim's explicit empty trailing cell, with following cells correctly aligned); `grid_scroll` content-shift assertions (both directions).
- **`key_to_notation` unit asserts** — special keys, function keys, and Alt/Meta modifiers (`<A-x>`, `<D-s>`), plus the printable-char shift contract.

### Multigrid foundation (real Neovim)

`redraw_multigrid.rs` (six tests) drives a real nvim attached with `ext_multigrid` and asserts the multigrid render model. Because multigrid notifications (`win_pos` / grid creation / `grid_destroy`) can arrive across several flushes, these tests **poll** with a generous timeout (draining wakes), assert on grid/window MAP sizes, MEMBERSHIP, and active-grid behavior — never literal `WinInfo.win` handle values or HashMap iteration order:

- attaching creates a window grid (`grids` has grid 1 + a window grid `>= 2`, `windows` has the active window, `active_grid_id() != 1`);
- a raw `:split<CR>` (real nvim op — no init.lua override in this file) adds a grid + window, the new `win` round-tripping through `grid_for_win`;
- `grid_line` lands in the active grid only;
- `:close<CR>` removes the gone grid AND its `windows` entry;
- `:split` + `<C-w>w` moves `cursor.grid` so `active_grid` follows focus;
- `resize_grid` changes only the targeted grid (grid 1 unchanged).

### Layout intents + active-window (real Neovim)

`layout_intents.rs` (eight tests) exercises the Neovim→shell intent channel with the `init.lua` overrides installed (`setup_layout`). They drain intents / poll the snapshot with a timeout and use membership/relative assertions (never literal handle values):

- `<C-w>s` performs a real horizontal split AND emits `Split { orientation: Horizontal, .. }` (window count grows; new `win` round-trips through `grid_for_win`);
- `<C-w>v` → `Split { Vertical, .. }`;
- typing `:vsplit<CR>` (the fragile `cnoreabbrev` path) → a vertical split intent;
- `<C-w>l` emits `FocusDir { Right, .. }` but, because focus moves are PURE intents, nvim focus and `active_win` do NOT change (the load-bearing assertion);
- `focus_win(other)` changes `active_win` and `current_file` follows;
- `<C-w>c` with multiple windows drops the window count (via `win_close`);
- `<C-w>c` on the LAST window is a no-op (the Lua guard) and nvim stays alive;
- a `WinEnter` (split, then `focus_win`) updates `active_win`, always naming a window present in `windows`.
