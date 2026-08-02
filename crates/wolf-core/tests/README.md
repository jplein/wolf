# wolf-core/tests

Integration tests for [`wolf-core`](../README.md). Most of them drive a real, pinned Neovim rather than a mock, because what is under test is a live RPC round-trip — see the testing note in [DECISIONS.md](../DECISIONS.md).

```sh
cargo test -p wolf-core                     # all of them, on the Linux and Nix path
bazel test //crates/wolf-core:file_search   # one target, on the macOS Bazel host
```

## Adding a test file

Cargo discovers every `tests/*.rs`. **Bazel does not.** A new file needs a matching `rust_test` target in [`../BUILD.bazel`](../BUILD.bazel) or it runs under `cargo test` alone, which makes it a silent macOS-only gap. Mirror an existing sibling of the same kind:

- **Real-nvim** — stages `//bazel/nvim:nvim_tree` and `//runtime/nvim:init.lua` as `data`, wires the `WOLF_*` env vars via `$(rootpath)`, and carries `tags = ["no-sandbox"]`, because spawning a child process does not work under the macOS build sandbox. Results are still cached.
- **Headless** — needs none of that: no `data`, no env, no tag.

Each entry below says which kind it is. Under Cargo, a real-nvim test needs `WOLF_NVIM_BIN`, which the flake devShell exports.

## The shared harness — `common/mod.rs`

Not a test — the shared harness compiled into every real-nvim target in this directory (each such target lists it in `srcs`). Headless targets do not include it.

It offers:

- `require_nvim_bin()` and `init_lua_path()` resolve the pinned nvim and the captive `init.lua` to absolute paths, preferring an existing absolute `WOLF_NVIM_BIN` over the Bazel-staged `WOLF_TEST_NVIM_DIR`.
- `config(cols, rows)` / `config_in(cwd, cols, rows)` build a `CoreConfig`. `config_in` is the single funnel that points `WOLF_STATE_DIR` at the process-global hermetic temp dir `hermetic_state_dir()` returns, set-if-absent, so a test managing its own state dir wins.
- `wake_channel() -> (WakeFn, FlushWaiter)`, with `wait_for_flush`, `wait_for_initial_flush`, `wait_for_wake` and `drain`.
- `start_core_sized`, `start_core_with_temp_md[_sized]`, `temp_md`, `temp_md_path`, `fresh_cwd`, and the input constructors.

**Gotchas**

- Use `wait_for_initial_flush` for the attach wait. That flush lands during `WolfCore::start`, so `wait_for_flush` baselines after it and blocks forever.
- The relative `WOLF_NVIM_BIN` Bazel injects is deliberately not trusted: nvim is spawned with `current_dir(cfg.cwd)`, a temp dir, so the path must be absolutized first. Likewise `init.lua`, since under Bazel `CARGO_MANIFEST_DIR` is an ephemeral sandbox path and nvim launched with `-u <missing>` hangs forever on a `Press ENTER` prompt.
- Out-of-band notifications (`wolf_current_file`, liveness) fire a wake without bumping `seq`. The flush waits consume and ignore them; `wait_for_wake` is the way to observe that seam.
- `fresh_cwd()` leaks its directory by design: `config` returns only a `CoreConfig`, so there is no guard to keep a `TempDir` alive under a running core.
- `require_nvim_bin()` panics rather than skipping, unlike the wolf-ffi twin, because the test bodies here do not handle a skip return.

## Files

#### `appearance.rs`

Real Neovim (`//crates/wolf-core:appearance`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers**

- `WolfCore::set_background(dark)` flips nvim's global `&background` in both directions; the captive nvim launches `dark`.

**Gotchas**

- Core exposes no eval or get-option helper, so `&background` is read indirectly: set a `WolfCursor guibg` marker chosen by the live `&background`, fire `:doautocmd ColorScheme` (a bare `:hi` does not fire it), then poll `Snapshot.theme.cursor.bg`. New cases need the same marker trick.

#### `autoread.rs`

Real Neovim (`//crates/wolf-core:autoread`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers**

- init.lua's `WolfAutoRead` augroup: a file rewritten on disk from outside reloads into an unmodified buffer, and a buffer holding unsaved edits is never clobbered.

**Gotchas**

- The trigger is `set_ui_focus(false)` then `set_ui_focus(true)`; `FocusGained` is edge-triggered, so a lone `true` can be a no-op.
- An external rewrite must differ in byte length from the original and follow a ~1.1s sleep, so `:checktime`'s size and mtime checks both trip.
- Never hold a snapshot read guard across `drain()`/`sleep()` — the redraw handler's `write_async` spins on `try_write` and starves. Read owned values.

#### `browser_preview.rs`

Real Neovim plus a real loopback TCP socket (`//crates/wolf-core:browser_preview`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`, which the socket also needs).

**Covers**

- The lazily started loopback HTTP server behind `WolfCore::open_in_browser`: the pinned file rendered with the reload script, the `GET /` redirect, `/tick`, static sibling renders, raw asset bytes, 404s, path-traversal rejection, port reuse, and the empty URL for a non-markdown buffer.

**Gotchas**

- The HTTP client is hand-rolled on `std::net::TcpStream` because an HTTP-client crate (reqwest/hyper/ureq) is denylisted from the Bazel test deps. Use the local `http_get` and `http_get_bytes` (the latter splits head from body on raw bytes so asset bytes survive intact).

#### `buffer_rank.rs`

Headless (`//crates/wolf-core:buffer_rank`: no nvim, no staged runfiles, no `no-sandbox` tag).

**Covers**

- `rank_buffers` over `BufInfo` into `BufferRow`: `lastused`-descending order with the current buffer first, `bufnr`-ascending tie-break, the query filter preserving MRU order among survivors, `max` truncation, and the cwd-relative / absolute / `[No Name]` display projection.

**Gotchas**

- `rank_buffers` is IO-free and fixtures are plain path strings under a `/tmp/proj` prefix that never exists on disk. Keep new cases off the filesystem.

#### `cmdline_pum.rs`

Real Neovim (`//crates/wolf-core:cmdline_pum`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers**

- The `ext_cmdline` and `ext_popupmenu` folds: `:` then `e Fi` populating `Snapshot.cmdline`, `<Tab>` populating `Snapshot.pum` with the prefix-matching files alone, `<Esc>` tearing both down, and `cmdline.pos` as a byte offset (pinned with a multi-byte fixture).

**Gotchas**

- Fixtures are seeded into a temp dir pinned as nvim's cwd by a local `start_core_in(cwd)`; no `common` helper lets a test choose the cwd.
- Completion words can arrive shell-escaped (`File\ A.md`), so assertions compare through `deescape`.

#### `command_mru.rs`

Headless (`//crates/wolf-core:command_mru`: real SQLite on a tempdir, no nvim, no staged runfiles).

**Covers**

- `CommandMru::open`/`disabled`/`record`/`recent` and `COMMAND_MRU_LIMIT`: eager creation of `command-mru.sqlite`, newest-first order, re-record moving to the front without duplicating, the cap with rows physically pruned (proven across a re-open), and silent self-heal over a garbage or wrong-schema DB.

**Gotchas**

- Opens `CommandMru::open(<tempdir>)` directly, so it never reads `state_dir()` or `WOLF_STATE_DIR` and needs no env lock. Ordering comes from a monotonic `seq` column, so records need no sleeps.

#### `commands.rs`

Real Neovim (`//crates/wolf-core:commands`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`); the `COMMAND_SPECS` registry tests at the bottom start no core.

**Covers**

- The Neovim-to-shell command channel: the `:Wolf<Pascal>` Ex face and the `require('wolf').<snake>()` Lua face both enqueue the matching `Command`; the `:q`/`:q!`/`:wq`/`:qa`/`:qa!`/`:wqa` overrides and their modified-buffer refusals; and the registry invariants (`from_id` round-trip, unique ids and ex-commands, the `Wolf` prefix, menu groups).

**Gotchas**

- Commands are user-triggered with no emit-on-attach, so a freshly attached core draining an empty queue is load-bearing; a new case should discard setup-time commands first.
- Core never acts on a drained command, so typing `:WolfQuitApplication` or `:qa` destroys nothing.

#### `copy_paste.rs`

Real Neovim (`//crates/wolf-core:copy_paste`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers**

- `Snapshot::is_visual_mode` across `v`, `V`, `<C-v>`, and `<Esc>`; `WolfCore::copy_visual_selection` returning the yanked selection and `None` outside visual mode; and `WolfCore::paste` landing text on the active grid.

**Gotchas**

- A stray startup-tail redraw can satisfy a single `wait_for_flush` before the `mode_change` lands, so mode assertions go through `wait_until_visual_mode`, which polls across several flushes.

#### `current_file.rs`

Real Neovim (`//crates/wolf-core:current_file`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers**

- `Snapshot.current_file`: set by `InputEvent::Open`, by a typed `:e <path>` (the autocmd reveal path for nvim-driven buffer changes), moving on a second open, and `None` for a genuinely unnamed buffer.

**Gotchas**

- The startup buffer is a named `Untitled.md`, so the unnamed case needs a raw typed `:enew`; `InputEvent::NewBuffer` routes through `wolf_new_untitled` and would name the buffer.
- `wolf_current_file` arrives out of band from redraw flushes, so tests poll, and both sides are canonicalized because `/tmp` entries are often symlinks.

#### `empty_state_cwd.rs`

Real Neovim (`//crates/wolf-core:empty_state_cwd`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers**

- Folder-MRU hygiene at launch: the one-shot startup `wolf_cwd` emit updates the tracked-cwd cell alone and never records the launch directory, while a genuine later `:cd` lands at rank 0 — asserted through both `WolfCore::recent_dirs()` and the physical `folder-mru.sqlite`.

**Gotchas**

- `WOLF_STATE_DIR` is set to a fresh tempdir before the core starts, because the always-on recent-dirs store reads `state_dir()` eagerly. That is a process-global mutation, so a new case must keep the ordering and the cleanup.

#### `file_search.rs`

Real Neovim plus a real filesystem and the real `git` binary (`//crates/wolf-core:file_search`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers**

- `WolfCore::start_file_search` and the `FileSearch` session: which entries the traversal yields (plain files and dotfiles, no directories, symlinks only to plain files, loop-safe), `.gitignore` plus `.git/` and `.wolf/` exclusion, the synchronous non-duplicated MRU seed, `cancel`, and both the root and the seed following nvim's live cwd rather than `cfg.cwd`.

**Gotchas**

- Every temp cwd is canonicalized (macOS `/tmp` is a symlink) so the traversal root, the MRU rows, and the relative `display` strings line up.
- MRU cases go through `MruStore::open(&common::hermetic_state_dir())`, the single global `file-mru.sqlite`, not a per-project directory.
- Traversal-cap cases belong in the sibling `file_search_bounds.rs`, which owns a separate binary because it mutates the process-global `WOLF_SEARCH_*` env vars.

#### `file_search_bounds.rs`

Real Neovim (`WOLF_NVIM_BIN`, `no-sandbox` target with staged nvim runfiles).

**Covers**

- The bounded-traversal safety net: the entry cap and the wall-clock budget each end a walk both `complete` and `truncated`, while `WOLF_SEARCH_MAX_DEPTH` prunes silently, so the depth case asserts the absence of a too-deep file rather than the flag.

**Gotchas**

- A separate binary from `file_search.rs` because every case mutates the process-global `WOLF_SEARCH_*` caps, which `file_search.rs` assumes are default. Within this binary, hold `BOUNDS_ENV_LOCK` across `set_var` + `start_file_search` (the caps are read synchronously there) — a new case must do the same.

#### `input_keys.rs`

Real Neovim (`WOLF_NVIM_BIN`, `no-sandbox` target with staged nvim runfiles), though only the cursor/mode case uses it.

**Covers**

- Cursor row/column advance and insert-mode `ModeInfo` selection against a live nvim, plus pure `key_to_notation` asserts for special keys, F-keys, and the modifier order `C-`, `S-`, `A-`, `D-`.

**Gotchas**

- The insert-mode check accepts either a mode name containing "insert" or a `Vertical` cursor shape, so it stays uncoupled from nvim's `mode_info_set` ordering and labels.

#### `layout_intents.rs`

Real Neovim (`WOLF_NVIM_BIN`, `no-sandbox` target with staged nvim runfiles).

**Covers**

- The Neovim-to-shell layout intent channel with the init.lua overrides installed: `<C-w>` chords and the `cnoreabbrev` ex-command path emit `Split`, `FocusDir`, and close intents; `focus_win` moves `active_win` and `current_file`; `<C-w>c` on the last window is a no-op.

**Gotchas**

- Focus is a pure intent — the core never acts on `FocusDir`, so `<C-w>l` must leave nvim focus and `active_win` unchanged. That negative assertion needs the `SETTLE` delay to be meaningful.
- Assert on membership and relative change, never literal window or grid handle values, and never HashMap order.

#### `markdown_render.rs`

Headless: no Neovim, no filesystem, no staged runfiles.

**Covers**

- The public preview render surface (`render_html`, `render_html_with_highlights`, `code_blocks`, `theme_css`, `PreviewTheme`, `WidthMode`): `wl-{L}-{C}` scroll-sync anchor placement, MathML with inlined fonts, the paired dark/light palettes, and a sweep asserting every vertical dimension in the shipped `base.css` lands on the baseline grid.

**Gotchas**

- `COLUMN_STRIDE` and `BASELINE` are re-declared locally instead of imported from the crate, deliberately: an assertion must state its expectation independently, so changing the source const fails this test rather than moving with it. Follow that when adding cases.
- The selectors `GRID_SELECTORS` omits are argued in its doc comment. Adding a rule to `COMPENSATION_ALLOWLIST` instead of fixing its arithmetic converts the audit into a rubber stamp.

#### `messages.rs`

Real Neovim (`WOLF_NVIM_BIN`, `no-sandbox` target with staged nvim runfiles).

**Covers**

- The `ext_messages` fold into `Snapshot.toast`: `nvim_echo` and `:messages` populate it, consecutive messages replace rather than accumulate, `search_count` is suppressed, and the input pump clears the toast on the next key.

**Gotchas**

- A `confirm`-kind toast is exempt from the input dismiss (the `Toast::is_prompt` gate). The cases covering an answered prompt clearing, and a later message not resurrecting it, exist because of a real lingering-prompt bug.

#### `mru.rs`

Headless: real SQLite on a real tempfs, no Neovim.

**Covers**

- `MruStore` directly — `open` creates `<state_dir>/file-mru.sqlite`; `record`/`recent` are directory-scoped and mutually isolated, most-recent-first with ascending `mru_rank`; re-recording upserts; a deleted file is pruned from the database, not merely filtered; `recent` caps at `MRU_LIMIT`.

**Gotchas**

- The store canonicalizes its `directory` argument, so build directories with `canon_tempdir` or the relative `display` strings will not line up.
- Ordering cases need distinct `opened_at` values; keep the `REC_GAP` sleep between records. A flake there means the timestamp resolution or tiebreak needs fixing, not the test.

#### `mru_open_records.rs`

Real Neovim (`WOLF_NVIM_BIN`, `no-sandbox` target with staged nvim runfiles).

**Covers**

- The record-on-open path through the api.rs input pump: an `InputEvent::Open` lands in the MRU, and after a `:cd` the row is scoped to nvim's live cwd rather than the directory the core started in.

**Gotchas**

- The record can land after the open's redraw flush, so poll with `wait_for_mru_contains` instead of waiting on a flush.

#### `nav_history.rs`

Headless: pure path math, no Neovim and no filesystem.

**Covers**

- The `nav_history` data model — the per-pane stack and cursor, feedback-loop suppression via `expected`, the emit-level repeat filter, `menu_rows` selection and its single checkmark, and the `disambiguate` prefix-label pipeline.

**Gotchas**

- `disambiguate` is IO-free, which is why labels are asserted for paths that do not exist on disk.

#### `nav_history_live.rs`

Real Neovim (`WOLF_NVIM_BIN`, `no-sandbox` target with staged nvim runfiles).

**Covers**

- The whole nav-history path end to end: the `wolf_buffers` emit, the fold over `win_buffers` and previews, the `win_close` prune, and the `WolfCore::history_*` moves driving a live nvim.

**Gotchas**

- Never assert an absolute stack depth — the startup buffer is a named `<cwd>/Untitled.md`, so every pane's history opens with an extra `Edit`. Assert relative growth and newest-first row prefixes.
- The terminal-drawer exclusion cases guard three separate init.lua mechanisms (coalesced `wolf_buffers` emits, `wolf_suppress_buffers` around the drawer open, and the `is_drawer` buffer-or-window tag); a regression in any one shows up here.

#### `nvim_liveness.rs`

Real Neovim (`WOLF_NVIM_BIN`, `no-sandbox` target with staged nvim runfiles).

**Covers**

- Crash and stall detection: a self-SIGKILL fed through the input pump flips `liveness()` to `Dead`; a wedged event loop escalates repeated RPC timeouts to `NotResponding`; a successful RPC after recovery auto-clears back to `Alive`. Each transition fires a front-end wake.

**Gotchas**

- SIGKILL rather than `qa!` is load-bearing: a clean quit sets `shutting_down`, so the stdout EOF is suppressed instead of reported as `Dead`.
- The stall cases set the process-global `WOLF_RPC_TIMEOUT_MS` and wedge nvim with a finite `vim.uv.sleep`; teardown force-kills the wedged child, so widen (never tighten) the wedge if a case flakes.

#### `nvim_restart.rs`

Real Neovim (`WOLF_NVIM_BIN`, `no-sandbox` target with staged nvim runfiles).

**Covers**

- `restart_nvim()` and Tier-1 restore: from a `Dead` core it respawns (`Ok(true)`), carries none of the files that were open before the crash, and the open-file mirror captured before the crash drives the reopen. On a live core it is a no-op (`Ok(false)`) that leaves nvim undisturbed.

**Gotchas**

- The Tier-1 set includes the never-saved startup `Untitled.md`: reopening it via `:e` is what triggers Neovim's swap-based recovery of unsaved edits, so filtering it out would lose them permanently.
- The respawn wipes the mirror, so capture it before restarting, and canonicalize every path (macOS `/tmp` and `/var` are symlinks).

#### `palette_rank.rs`

Headless: `rank_palette` is IO-free, so the target needs no nvim, no runfiles, and no `no-sandbox` tag.

**Covers**

- Command-palette candidate assembly and ranking: the kind, label, `mru_key` and payload of static, switch-window and open-recent rows; the empty-cwd "Untitled" window; the `AND`-subsequence query filter; MRU float; and `max` truncation.

**Gotchas**

- `STATIC_COMMAND_COUNT` is a hand-maintained mirror of `COMMAND_SPECS.len() - 1` (the palette excludes only its own invoking command, and gates nothing else). Registering a new command fails two assertions here until the constant is bumped.

#### `pane_buffers.rs`

Real Neovim: needs `WOLF_NVIM_BIN`, the staged nvim runfiles, and `tags = ["no-sandbox"]`.

**Covers**

- `Snapshot.buffers` (per-window buffer paths fed by the `wolf_buffers` notification) and `WolfCore::pane_status(grid)` joining grid to window to buffer: distinct filenames across splits, unnamed and preview panes staying out of the map, and closed windows dropping out.

**Gotchas**

- `wolf_buffers` arrives separately from redraw flushes, so every assertion polls while draining wakes — never wait on a single flush.
- Cases pin an explicit temp cwd and canonicalize both sides so `display_path` survives the macOS `/var` and `/tmp` symlinks.
- The startup buffer is named `Untitled.md`; reaching the `[No Name]` path needs a raw `:enew`, which bypasses the `wolf_new_untitled` naming helper.

#### `pane_status_unit.rs`

Headless: `pane_status` is IO-free string and path math, so the target needs no nvim or runfiles.

**Covers**

- The pane statusbar display strings: cwd-relative remainder (nested and direct), the `~` home-relative form, absolute passthrough, `[No Name]` for an empty buffer path, and cwd-over-home precedence.

**Gotchas**

- Cases supply pre-aligned POSIX literals so `strip_prefix` lines up; the production caller canonicalizes cwd, home and buf_path itself.
- `path == cwd` and `path == home` are pinned deliberately: neither may collapse to an empty relative string.

#### `rank_bounded.rs`

Headless: ranking reads only each entry's `display` and `mru_rank`, so the target needs no nvim or runfiles.

**Covers**

- One property across every case — `rank_matches_top_n(query, entries, n)` equals `rank_matches(query, entries).take(n)` — which pins the bounded heap's `index` final tiebreak to the unbounded stable sort.

**Gotchas**

- Never hand-derive an expected order; compare against `rank_matches(..).take(n)` through the file's helpers, or the case stops proving equivalence.
- A useful new case picks an `n` that cuts through a scoring tie group; that boundary is where the two orders could diverge.

#### `rebuild_layout.rs`

Real Neovim: needs `WOLF_NVIM_BIN`, the staged nvim runfiles, and `tags = ["no-sandbox"]`.

**Covers**

- `WolfCore::rebuild_layout(spec_json)` realizing a saved pane-split tree: one handle per spec leaf, distinct, in DFS/leaf order, each bound to the requested buffer — across nested splits, lone leaves, empty-path leaves, and `preview: true` leaves reconstructed as real previews rather than `:edit`.

**Gotchas**

- Windows and the `wolf_buffers` emit settle across several flushes, so poll while draining wakes. Assert handle count, distinctness and the per-window buffer, never literal handle values.
- An empty-path leaf inherits the named `Untitled.md` startup buffer, matched by basename suffix so `/var` versus `/private/var` does not break it.

#### `recent_dirs.rs`

Headless: builds `RecentDirs::open(<tempdir>)` directly against real SQLite, so it needs no nvim, no runfiles, and no `WOLF_STATE_DIR` or env lock.

**Covers**

- The `RecentDirs` store: eager DB creation, newest-first order, move-to-top on re-record, the `RECENT_DIRS_LIMIT` cap with physical pruning, silent self-heal over a garbage or wrong-schema DB, and the no-op `disabled()` store.

**Gotchas**

- Ordering comes from a monotonic `seq` column rather than a timestamp, so no case needs a sleep between records.
- The cap case re-opens a second store on the same dir to prove rows were deleted, not merely capped by the read query's `LIMIT`.

#### `recent_dirs_records.rs`

Real Neovim: needs `WOLF_NVIM_BIN`, the staged nvim runfiles, and `tags = ["no-sandbox"]`.

**Covers**

- The record-on-`:cd` write path: a typed `:cd` lands its target at rank 0 of the always-on recent-dirs MRU, asserted through both `WolfCore::recent_dirs()` and a fresh `RecentDirs::open(state_dir)`; the launch cwd is never recorded.

**Gotchas**

- The store reads `state_dir()`, so a case must set `WOLF_STATE_DIR` to a fresh tempdir before `WolfCore::start` — the harness's set-if-absent hermetic default then leaves it alone.
- The recorded row trails the visible redraw flush, so poll rather than sleep.

#### `redraw_grid.rs`

Real Neovim: needs `WOLF_NVIM_BIN`, the staged nvim runfiles, and `tags = ["no-sandbox"]`.

**Covers**

- The single-grid (`ext_linegrid`) redraw fold: typed text and its resolved `hl_id` reaching the grid, `grid_line` wide-char and repeat-run decoding, `grid_scroll` shifting content in both directions, global resize, and `win_viewport`'s `linecount`.

**Gotchas**

- `core.resize` is a global resize and drives grid 1, not the active window grid; per-grid resize is covered in `redraw_multigrid.rs`.
- Assertions inspect cell text and column alignment only, never how nvim chunks a `grid_line` or `grid_scroll` region.
- The line-count fixture joins lines with no trailing newline; adding one makes nvim report an extra empty final line.

#### `redraw_multigrid.rs`

Real Neovim: needs `WOLF_NVIM_BIN`, the staged nvim runfiles, and `tags = ["no-sandbox"]`.

**Covers**

- The `ext_multigrid` model: window-grid creation on attach and `:split`, grid plus window removal on `:close`, `grid_line` landing only in the active grid, `cursor.grid` following focus, and per-grid resize leaving grid 1 untouched.

**Gotchas**

- Assert map sizes, membership, and that a `win` round-trips through `grid_for_win`; never literal `WinInfo.win` values or HashMap iteration order.
- `win_pos`, grid creation and `grid_destroy` can arrive across several flushes, so poll while draining wakes.
- The per-grid resize case polls for the exact requested dimensions. If nvim clamps a window grid to the surrounding layout, relax it to assert the target grid merely changed — the load-bearing claim is that only it changed.

#### `rpc_timeout.rs`

Real Neovim: needs `WOLF_NVIM_BIN`, the staged nvim runfiles, and `tags = ["no-sandbox"]`.

**Covers**

- The central `block_on_rpc` chokepoint: with a small `WOLF_RPC_TIMEOUT_MS` and nvim wedged inside a `vim.uv.sleep`, `current_tabpage` returns within a bound and degrades to its documented `0` fallback. `start_file_search` is timed alongside as a cheap guard that the picker root stays out-of-band.

**Gotchas**

- The wedge races the input pump's coalesce-and-apply hop. If this turns flaky, widen the wedge sleep or the return bound — never tighten them, because a tight bound reintroduces the exact race the test exists to pin.
- The wedge sleep is finite on purpose, so nvim recovers on its own and teardown does not block.

#### `save_open.rs`

Real Neovim: needs `WOLF_NVIM_BIN`, the staged nvim runfiles, and `tags = ["no-sandbox"]`.

**Covers**

- File semantics delegated to nvim: opening an existing Markdown file (with a heading-versus-body `hl_id` smoke check), `InputEvent::Save` writing the current buffer, and `InputEvent::SaveAs` creating a path that did not exist.

**Gotchas**

- Writes land asynchronously, so a case polls the file until it contains the expected text rather than reading once after the input.

#### `search.rs`

Spawns a real Neovim **and** a real pinned `rg`: its target stages `//bazel/rg:rg_tree` alongside the nvim runfiles and is tagged `no-sandbox`.

**Covers** — `start_search` streaming into a `SearchSession`, the `all_files` / `use_regex` / `case_sensitive` toggles, `match_ranges` and `column` on a multi-submatch line, and the `open_at(path, line, col)` round-trip.

**Gotchas** — Without a resolvable `WOLF_RG_BIN` the local `rg_bin_or_skip` turns every case into a no-op pass, so green does not by itself prove rg ran. Matches accumulate in rg emission order, so ordering assertions must go through `project`.

#### `search_no_rg.rs`

Spawns a real Neovim but deliberately no rg — the target omits `//bazel/rg:rg_tree` and never wires `WOLF_RG_BIN`.

**Covers** — the graceful-degrade contract: with rg absent, `start_search` returns an immediately-`complete` session carrying `error == Some("WOLF_RG_BIN unset")` and no matches, and fires the wake so a front-end repaints into the error state.

**Gotchas** — It owns its own process precisely so it can `remove_var("WOLF_RG_BIN")`; Rust runs a binary's tests on parallel threads, so folding any of this into `search.rs` would race that binary's rg lookup.

#### `search_projection.rs`

Headless — pure functions only, so its target needs no staged runfiles, no `WOLF_*` env, and no `no-sandbox` tag.

**Covers** — the public projection and display surface re-exported from `search_tree`: `file_label`, `project` (grouping by `abs_path`, case-insensitive rel-path order, per-group line order), and `context_window` truncation at the start, end and middle of a line plus multibyte and emoji boundaries.

**Gotchas** — Cases build `SearchMatch` and read `SearchFileGroup` / `ContextWindow` as struct literals, so those fields must stay public. The private rg-JSON parsers are covered by the inline `#[cfg(test)]` module in `src/search.rs`, not here.

#### `search_root_out_of_band.rs`

Spawns a real Neovim (staged nvim runfiles, wired `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers** — that `resolve_search_root` is an out-of-band `live_cwd()` read rather than a blocking `getcwd` RPC: `start_file_search` returns under `PROMPT_BOUND` while nvim is wedged, roots at the tracked cwd, and follows a `:cd`.

**Gotchas** — The wedge is a finite `vim.uv.sleep` fed through the input pump, so there is an inherent race between nvim actually sleeping and the timed call. If a case proves flaky, widen `WEDGE_SLEEP_MS` or `PROMPT_BOUND`; tightening them re-introduces the exact race the file exists to pin.

#### `spawn.rs`

Spawns a real Neovim (staged runfiles, wired `WOLF_NVIM_BIN`, `no-sandbox`). The suite's smoke test — if this fails, nothing else in the real-nvim set is meaningful.

**Covers** — attaching as an `ext_linegrid` UI: global grid 1 is sized to the requested attach, highlight defaults are populated by `default_colors_set`, `mode_infos` is non-empty after `mode_info_set`, and `shutdown` is clean.

**Gotchas** — Under `ext_multigrid` only grid 1 carries the requested attach size; window grids (id >= 2) take the window content area instead.

#### `state_layout.rs`

Headless — driven entirely through `tempfile` dirs handed to `StateStore::open` / `load_state`, so its target needs no runfiles and no `no-sandbox` tag.

**Covers** — the per-tab pane tree: `TabState.layout: Option<PaneNode>` round-tripping through `set_snapshot` -> `flush` -> `load_state` for leaf, two-child, n-ary, perpendicular-nested and empty-path shapes; the on-disk serde tags and orientation strings; and that a v1 file missing `layout`, or a leaf missing `preview`, loads with the `serde(default)` value rather than being rejected.

**Gotchas** — `STATE_VERSION` stays 1, so a new field means a new `serde(default)` back-compat case here, not a version bump.

#### `state_unit.rs`

Headless — explicit `tempfile` dirs passed to `StateStore::open` / `load_state`; no nvim, no runfiles, no `no-sandbox` tag.

**Covers** — the whole-tree `state` API: the `set_snapshot` -> `flush` -> `load_state` round-trip, an unchanged snapshot skipping a rewrite, a later snapshot fully replacing the prior tree, the silently-ignore matrix (absent, corrupt, wrong-shape, flat pre-tabs and unsupported-version files all load as `None` with the bytes left untouched), and `swap_dir_for` stability per canonical cwd.

**Gotchas** — Env-mutating cases take the file-local `ENV_LOCK`, since cargo runs this binary's tests concurrently in one process. `REWRITE_OBSERVE_DELAY` outlasts coarse filesystem mtime resolution — mtime is how the no-rewrite claim is observed.

#### `swap_recovery.rs`

Spawns a real Neovim (staged runfiles, wired `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers** — the recovery-aware `SwapExists` autocmd never raising a blocking prompt: a dead owner's unsaved edit is silently recovered (`v:swapchoice = "r"`), a live owner's is not (`"e"`, the clean on-disk copy), and an unsaved `Untitled.md` survives a crash-restart instead of being shadowed by the fresh nvim's own startup buffer.

**Gotchas** — The swap dir is never set directly: it derives from `swap_dir_for(cwd)`, so a case controls it by owning a canonicalized cwd, reused across a restart or shared by two cores. SIGKILL skips `preserve_exit`, so a case must `:preserve` and settle before killing or the edit is simply lost.

#### `tabpage_cwd.rs`

Spawns a real Neovim (staged runfiles, wired `WOLF_NVIM_BIN`, `no-sandbox`). One test.

**Covers** — that `cwd_for_tabpage` is a cheap `Snapshot.tab_cwds` read fed out-of-band by the `wolf_tab_cwds` notification, not a blocking `tab_cwd` RPC: while nvim is wedged, both a current and a non-current tabpage resolve correctly under `PROMPT_BOUND`.

**Gotchas** — The wedge is a finite `vim.uv.sleep` fed through the input pump, so the timing is inherently racy; widen `WEDGE_SLEEP_MS` or `PROMPT_BOUND` if flaky, never tighten them. Do not set `WOLF_RPC_TIMEOUT_MS` here — the point is that no RPC remains on this path.

#### `tabpages.rs`

Spawns a real Neovim (staged runfiles, wired `WOLF_NVIM_BIN`, `no-sandbox`).

**Covers** — the tabpage lifecycle: `create_tabpage` / `set_current_tabpage` / `close_tabpage` / `cwd_for_tabpage` / `current_tabpage` plus `Snapshot.current_tabpage`, per-tabpage `:tcd` cwd including a non-current read, the shared buffer list, handle stability across a renumber, and the keystone that window handles survive a tabpage switch.

**Gotchas** — A switch churns grid ids while window handles stay stable, so assert on handle membership and `grid_for_win` round-trips, never literal grid ids or map order. `win_hide` removes the window from `Snapshot.windows` but keeps its grid in `Snapshot.grids`. Changes land across several flushes, so poll.

#### `theme.rs`

Spawns a real Neovim (staged runfiles, wired `WOLF_NVIM_BIN`, `no-sandbox`), started with the plain repo `init.lua` so nothing defines the Wolf highlight groups at startup.

**Covers** — `Snapshot.theme` fed by the `wolf_theme` notification: the startup emit-once reporting all-`None`, runtime `:hi WolfCursor` / `:hi WolfTitlebar` values, a linked group resolving to its target's effective attributes, and `:hi clear` falling back to all-`None`.

**Gotchas** — A plain `:hi` does not fire `ColorScheme`, so every mutation must be followed by an explicit `:doautocmd ColorScheme`; this is required by design, not a test convenience. The notification wakes without bumping `seq`, so poll `read_snapshot().theme` rather than waiting for a flush.
