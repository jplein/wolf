# wolf-core/src

The modules of [`wolf-core`](../README.md), one entry each. Everything the crate re-exports is listed in the crate README's public API table; this file says what each module is and why it is shaped the way it is.

**Read [wolf-core-decisions.md](../../../notes/wolf-core-decisions.md) first.** The GUI-free dependency rule, the threading and locking model, liveness and restart, the snapshot-and-wake session pattern, and the browser-preview security requirements all live there and are not repeated here.

### The core loop

#### `lib.rs`

Crate root. Declares every module and flattens the public surface into root re-exports, so consumers write `wolf_core::Snapshot` rather than `wolf_core::render::Snapshot`. The [`app_state`](#app_staters) model is the exception, reached as `wolf_core::app_state::…`, because its `WindowState` and `Cursor` would collide with the root's `state::WindowState` and `render::Cursor`.

#### `api.rs`

`WolfCore`, the FFI-shaped surface both front-ends share, owning the tokio runtime, captive Neovim and snapshot.

**Public API**

- `CoreConfig.cwd` is nvim's launch directory and the search/MRU root until nvim reports its own cwd (`live_cwd`).
- `SnapshotReadGuard` owns an `Arc` of the lock, so it outlives `&self`. Hold it for one paint only.
- The blocking queries (`list_buffers` and siblings) return an empty or zero fallback on any RPC error or timeout, indistinguishable from an empty answer. The rest is fire-and-forget on the ordered input channel: `split_pane` splits the window an earlier `focus_win` made current.
- `restart_nvim` returns `Ok(false)` when no respawn happened (already `Alive` or in flight) and reopens nothing: capture `open_files_for_tabpage` first. After `Ok(true)` the snapshot, intents and history start over ([what carries](../../../notes/wolf-core-decisions.md#liveness-and-restart)). Typing sent before the swap never reaches the fresh nvim, so no half-typed Ex command lands as keys; state input still applies.
- `start` ignores `SIGPIPE` process-wide ([why](../../../notes/wolf-core-decisions.md#liveness-and-restart)).
- `recent_dirs` omits non-directories (a stat each, on the calling thread) and never prunes the store, so a returning folder reappears.

**Decisions**

- `block_on_rpc` detaches a call that outlives `WOLF_RPC_TIMEOUT_MS` rather than dropping it: dropping an `nvim-rs` response receiver mid-flight kills the io loop, which reads as a crash.
- `WolfCore` is `!Send` and keeps the process in a `RefCell`, so a restart can swap one in through `&self`. Never hold a borrow across an `.await`.
- Stale typing is cut by `send_input`'s generation stamp, not liveness: input sent while `Alive` can still be queued at the swap.

#### `pump.rs`

The input pump: the one consumer of `send_input`'s channel. While a batch's call is unanswered it keeps taking input, so a key can answer a prompt the call raised ([why](../../../notes/wolf-core-decisions.md#threading-and-locking)).

**Decisions**

- A batch ends at its first event that is not a fast input (`is_fast_input`), so everything behind a call that may wait stays queued in order.
- Held fast input is released only on a `nvim_get_mode` probe answered before the call, one per answer: Neovim runs typeahead before queued requests, so a key behind a busy call would land first.
- The first poll writes the call before any probe, unless nvim-rs's writer lock is held. nvim-rs signals no write, so then a released key can land first. The answer waits out any release in flight, so no later key overtakes it.
- A paste is never released: `nvim_paste` stays unanswered behind the call and would hold back every key. Nor does it stop a release, or the key ending the wait would wait on it. So a key lands ahead of an earlier paste ([spec](../../../specs/command-lifecycle.md#7-the-editor-affecting-path-back-into-nvim)), even in a wait that ends by itself, such as a pending mapping's.
- The call is pinned, never dropped ([why](#apirs)); probes and releases are spawned tasks, which dropping only detaches.
- A prompt showing at the budget's end means the user is late, not Neovim: the stall is cleared, not marked.

**Gotchas**

- Every RPC an `InputEvent` issues in `input.rs` must go through `Rpc::request`, never `Neovim::call`, or input stops behind it.

#### `nvim.rs`

Spawns and supervises the captive Neovim child, and wraps every RPC core issues against it. The launch contract and the supervisor's role in liveness are in [`wolf-core-decisions.md`](../../../notes/wolf-core-decisions.md).

**Public API**

- `NvimConfig::from_env` resolves the binary and creates the private state root. Its `Arc<TempDir>` guard keeps those directories only while a clone of the config lives.
- `NvimProcess::handle` yields a cloned RPC handle usable without borrowing the process; the browser-preview server re-renders through one.
- The `setup_*` methods install the nvim-side wiring behind every out-of-band `wolf_*` notification. A respawn re-runs the whole chain; a fresh nvim emits nothing until it does.
- `NvimError::Timeout` is never constructed here; it comes from `api`'s RPC chokepoint.

**Decisions**

- Every method reachable from that chokepoint returns `impl Future<..> + Send + 'static` (a cloned handle plus owned arguments) never `async fn(&self)`: the timeout path spawns the future as a task and detaches it, and a detached task cannot hold a borrow.
- Channel ids, modes, and paths reach `nvim_exec_lua` as varargs, never interpolated into the code string.
- `yank_visual_selection` checks the mode before yanking: a bare `y` outside visual mode leaves nvim in operator-pending and the call never returns.
- `shutdown` and `kill_in_place` both set `shutting_down` before the process exits, so the supervisor reads the stdout EOF as intentional rather than as a crash.
- `kill_in_place` then waits, bounded, for the supervisor ([why](../../../notes/wolf-core-decisions.md#liveness-and-restart)). It skips the wait once the supervisor has finished: a failed respawn leaves the same process, and polling a completed `JoinHandle` again panics.

#### `redraw.rs`

The `nvim-rs` handler: folds each `redraw` batch into `Snapshot` and routes the out-of-band `wolf_*` notifications into its sibling cells, queues and registries. It answers one request, `wolf_barrier` ([why](../../../notes/wolf-core-decisions.md#threading-and-locking)).

**Decisions**

- Only `flush` bumps `seq`; out-of-band notifications wake without bumping it, so a repaint gated on `seq` alone misses them. Which notifications wake at all is the condition in [the core↔front-end contract](../../../specs/architecture.md#the-corefront-end-contract).
- In `grid_line`, `repeat` counts columns including the first, so an explicit `0` is a zero-width no-op; clamping it to 1 paints past the span, a wrong-background cell Neovim's shadow grid never repaints. A double-width glyph's trailing half arrives as its own empty-text cell; synthesizing a placeholder shifts the row right.
- `win_hide` removes the window entry but keeps its grid: Neovim reuses the grid id and may re-`win_pos` it unresized.
- A resolved `confirm()` and an invalid-key re-prompt produce byte-identical `cmdline_hide` batches, so the toast is stashed with its prompt and restored only on a matching `cmdline_show`.

**Gotchas**

- `RedrawHandler` is `Clone`; a respawn's clone shares the snapshot, queues and history, which the restart starts over but for the commands queue ([why](../../../notes/wolf-core-decisions.md#liveness-and-restart)).
- Lock order: the nav-history fold takes the snapshot write lock then the nav mutex, with no `.await` between; `WolfCore` takes the nav mutex alone. Reversing one cycles.

#### `render.rs`

The toolkit-agnostic render-state model: `Snapshot` is a complete paint-ready frame, and `SharedSnapshot` is the one lock guarding it. The locking model is in [`wolf-core-decisions.md`](../../../notes/wolf-core-decisions.md).

**Public API**

- A `Cell` with empty `text` is either never-painted or the trailing half of a double-width glyph; `Grid::row_text` renders both as one space to keep columns aligned.
- `active_grid` falls back to a shared empty `0x0` grid, so a caller never dereferences a missing grid.
- `Toast::is_prompt` is the dismiss-on-input gate: a `confirm` toast must survive a passing keystroke.
- `Snapshot.markdown_cursors` is keyed by source buffer number, like `markdown_ticks` and `PreviewInfo.source_bufnr`. `MarkdownCursor.line` is 1-based and `col` a 0-based character column; an absent entry or `line == 0` means unknown.

**Decisions**

- `Grid::resize` preserves the overlap rather than clearing: Neovim's `grid_resize` is not a clear and re-sends no `grid_line` for rows it considers unchanged, so clearing blanks the surviving pane after a split or a close.
- `WinInfo.hidden` is always `false`, kept only for FFI and Swift stability: a `win_hide` removes the window from `windows` outright.
- `ToastChunk` carries its `attr_id` across the FFI while `CmdlineChunk` is flattened to plain text, because message color is load-bearing.
- `Snapshot::pending_confirm` is internal fold bookkeeping, never rendered or crossed over the FFI.
- `SharedSnapshot::reset_for_respawn` never blocks on the lock, since a caller may hold a read guard across a restart on the same thread: past `RESET_LOCK_WAIT` it leaves the reset pending, for the next `write` or `write_async` to apply first ([what it carries](../../../notes/wolf-core-decisions.md#liveness-and-restart)).

#### `liveness.rs`

The captive nvim's liveness cell: a lock-free `Alive` / `NotResponding` / `Dead` state over an `AtomicU8`, plus the transition helpers that mutate it. The state machine's meaning and who drives each edge are in [`wolf-core-decisions.md`](../../../notes/wolf-core-decisions.md).

**Public API**

- The `Liveness` discriminants `0 / 1 / 2` are stable — `wolf-ffi` mirrors them as its own constants. `from_u8` clamps any unrecognized byte to `Dead`, so a garbled value can never read as `Alive`.
- `get`, `mark_dead`, `mark_not_responding`, `clear_not_responding`, and `reset_alive` take `&AtomicU8` and `&WakeFn` directly rather than a wrapper type, so the cell can live in `WolfCore` and be shared with the supervisor by `Arc`.

**Decisions**

- Each helper fires the wake only on an actual state change, so a repeated call is silent. `mark_not_responding` is a compare-exchange from `Alive` only, so a late stall signal cannot downgrade `Dead`.
- `reset_alive` is an unconditional store and the only exit from `Dead`. The auto-clear after an answered call or batch is `clear_not_responding`, a compare-exchange from `NotResponding` only, so a late answer from a dying process can never resurrect it.

#### `logging.rs`

`init_tracing` installs the process-global stderr subscriber. It is idempotent and never panics: the first call wins and every later one discards `try_init`'s error, so each new project or window may call it freely.

**Decisions**

- Verbosity comes from `WOLF_LOG` using `EnvFilter` syntax, defaulting to `warn` when unset or unparseable. That default is chosen so the RPC-timeout warning is visible with no configuration while normal runs stay quiet.

### Configuration, state, and input

#### `config.rs`

Reads the user-preferences file `wolf-config.yaml` and resolves the per-platform preferences directory. Standalone: it touches neither `WolfCore`, the snapshot, nor nvim.

**Public API**

- `preferences_dir() -> Option<PathBuf>` — `WOLF_CONFIG_DIR` overrides everything (a test/determinism hook); otherwise macOS `~/Library/Preferences/Wolf`, Linux `~/.config/wolf`. `None` means no override and no home directory.
- `load_or_create(dir) -> WolfConfig` — infallible; see the decisions below for what each degraded input yields.
- `WolfConfig` — `line_height` is a percentage of the font size, not points. `default_config()` supplies a platform-specific font size.
- `Appearance` (`Auto`/`Dark`/`Light`), `CONFIG_FILE_NAME`.

**Decisions**

- The three font fields are required and the section deserializes all-or-nothing: a partial, malformed, or wrong-shape file falls back to the complete defaults rather than merging field by field, so a half-written config can never produce a half-applied one.
- A user file that fails to parse is left byte-for-byte untouched — the write path fires only when the file is absent — so a broken hand-edit survives for its author to fix instead of being silently overwritten.
- Keys added after the font fields (`appearance`, `macos-option-as-alt`) carry `#[serde(default)]` so a file predating them still parses and keeps its font settings; a present-but-invalid value still fails the whole section.
- `macos_option_as_alt` defaults to false, so Option composes accented characters rather than sending `<M-…>` mappings, mirroring Ghostty's default for the same key.

#### `state.rs`

Persists the `state.json` tree and renders a saved layout into the spec Neovim's rebuild helper decodes.

**Public API**

- `state_dir()` — `WOLF_STATE_DIR` overrides the platform default; `swap_dir_for(cwd)` derives a stable per-project swap dir under it, so a fresh nvim finds a dead one's swaps.
- `spell_dir()` / `runtime_user_dir()` / `parsers_user_dir()` — global subdirs of `state_dir()`, not per-project, exported as matching `WOLF_*` env vars; `None` when unresolvable. `seed_runtime_user_dir(dir)` seeds the `INIT_USER_FILE_NAME` template into a missing or empty dir only; a non-empty dir is never touched, even without the template ([wolf-core-decisions.md](../../../notes/wolf-core-decisions.md)).
- `load_state(dir)` — read-only and infallible: an absent file is a silent `None`; any other failure warns, leaving the bytes intact. An editor or preview leaf whose file is gone loads unzoomed ([why](../../../notes/wolf-core-decisions.md#the-persisted-zoom-mark)).
- `StateStore::open/state/set_snapshot/flush` — an unchanged `set_snapshot` writes nothing.
- `rebuild_layout_spec(&PaneNode) -> String` — the spec `WolfCore::rebuild_layout` takes; total: a non-finite weight renders `0.0`, a serialization error `"{}"`.

**Decisions**

- The spec is deliberately not `PaneNode`'s serde shape; leaves emit depth-first, through `app_state`'s builders, to zip onto `rebuild_layout`'s returned handles ([why](../../../notes/wolf-core-decisions.md#the-rebuild-layout-wire-format); shape in [`runtime/nvim/README.md`](../../../runtime/nvim/README.md)).
- One `wolf-state-writer` thread owns every write: coalesce, sibling-tmp, rename — no partial file after a crash. A failed write still advances the generation; `flush` cannot hang.
- `WindowState.tabs` has no `#[serde(default)]`: a file without it fails deserialization and is ignored rather than half-migrated.

**Gotchas**

- Opening one directory twice in-process races two writers on the tmp name — clone one store. Across processes, last writer wins. Geometry is front-end `f64` points; non-positive means unset.

#### `app_state.rs`

Purpose: the model of [§2](../../../specs/application-state.md#2-the-state-model), the rule that picks a directory's project state, and the restore helpers of [§5](../../../specs/application-state.md#5-restoring-at-launch). Its types serialize to the spec's JSON.

**Public API**

- `Frame::x`/`y`, `root` and `zoomed` are always written, `null` when unset. The fields a front-end may not capture (`cursor`, `top_line`, `scroll`, `focused`, `drawer`, the sidebar's mode, query and toggles) are skipped when `None`, and absent means nothing to restore. `Window::minimized` is written only when `true`, and absent reads `false`.
- `project_tab(state, recency, directory)` — [§4's rule](../../../specs/application-state.md#which-tab-becomes-a-directorys-project-state), as `(window, tab)`; `None` for `""`. `recency[w]` lists window `w`'s tabs most recent first. Unlisted tabs, or a missing list, rank after the listed ones in tab order, so empty recency is correct at launch.
- `project_states(state, recency)` — one entry per distinct non-empty directory: the chosen tab and its window's frame.
- `restored_tab(content)` — a missing edit file or preview source becomes an empty edit pane in place. A zoom is dropped on such a pane, out of range, on a lone pane with no open drawer, or on a closed drawer. A replaced pane keeps the focus.
- `tab_rebuild_spec(content)` — the `wolf_rebuild_layout` document, `None` without a root. It checks no file, so pass it `restored_tab`'s output.

**Decisions**

- `PaneRef` is a depth-first index, not a per-leaf mark ([why](../../../notes/wolf-core-decisions.md#the-persisted-zoom-mark)).
- The crate-private `leaf_spec` and `layout_spec` build the rebuild document for both models ([why](../../../notes/wolf-core-decisions.md#the-rebuild-layout-wire-format)).

#### `app_state_store.rs`

Purpose: `AppStateStore`, over the `window_state` and `project_state` state tables of `wolf.sqlite`, both opened on one connection through [`db.rs`](#dbrs) and written by a `wolf-app-state-writer` thread.

**Public API**

- `open(dir)` — infallible. Clones share the writer.
- `window_state()` for launch, and `project_state(directory)` for Open Folder, which marks the row most recently used. Each reads `None` for no row, a row that does not parse (left as it is), or a table from a newer build.
- `submit(state, recency)` — the whole window state plus each window's tab recency, which feeds `project_tab` and is never stored. Only a changed tree wakes the writer; a changed recency alone waits for the next write.
- `flush(timeout)` — as `StateStore::flush`. `PROJECT_STATE_LIMIT` is the spec's 200.

**Decisions**

- Each write is one immediate transaction: the window state, every chosen project state, then the prune ([§4](../../../specs/application-state.md#one-transaction-per-write)). The JSON is serialized first, because `with_conn` may run its closure twice.
- `last_used` is a counter (`MAX + 1`), not a clock, as `folder_mru.seq` is, so eviction is deterministic within a millisecond and tests can seed it.

**Gotchas**

- Two handles on one directory are safe in the database, but the unchanged-tree baseline and the newer-table flags are per handle, so a front-end opens one per process ([why](../../../notes/wolf-core-decisions.md#the-application-state-store)).

#### `layout.rs`

The Neovim-to-shell intent channel. `runtime/nvim/init.lua` overrides the split and focus chords to `rpcnotify` a `wolf_layout` payload; [`redraw`](#redrawrs) parses each into a `LayoutIntent` for the front-end to drain.

**Public API**

- `LayoutIntent::from_payload(&rmpv::Value) -> Option<Self>` — any malformed or partial payload yields `None` rather than an error. Window handles are accepted both as plain integers and as a msgpack `Ext`.
- `LayoutIntent` (`Split`, `FocusDir`, `DrawerOpen`), `SplitOrientation`, `FocusDir`.

**Decisions**

- A split and a drawer-open are performed by nvim and also reported, so the window and its grid genuinely exist by the time the shell reacts. A focus move is a pure intent: nvim does not change focus, and the shell resolves the neighbouring pane in its own layout. Making focus performed, or splits pure, breaks the side that depends on the other behaviour.
- Closing has no intent in either direction. nvim performs a guarded close and the shell learns of it from the `win_close` redraw event.

#### `command.rs`

The canonical, platform-agnostic registry of Wolf commands. A command is a no-parameter, no-return user action with two faces derived from one canonical snake_case id: the Ex-command `:Wolf<Pascal>` and the Lua function `require('wolf').<snake>()`. Pure data and enums.

**Public API**

- `Command` — one variant per user action.
- `COMMAND_SPECS` — one `CommandSpec { command, id, ex_command, title, menu }` per command, in variant order. All fields are public so a caller can build the nvim registration list or a menu from it.
- `Command::from_id(id) -> Option<Command>` (a linear scan) and `Command::spec()`.
- `MenuGroup` — menu-grouping metadata; its own variant order is cosmetic.

**Decisions**

- The `Command` variant order is a discriminant contract shared with `wolf-ffi`'s `command_kind` and the Swift `CommandKind` port, and the `COMMAND_SPECS` slice index equals that discriminant. Adding a command means appending a variant and a row; reordering or removing one breaks every consumer across the FFI.
- `Command` is declared through `declare_commands!`, which re-emits it unchanged and gives the tests its variant count. Stable Rust cannot count variants, so a plain `enum` would hide a variant with no row from the table test.
- The id-to-`ex_command` mapping is stored explicitly as data rather than computed by a snake-to-Pascal transform, so the registry remains the single source of truth.

**Gotchas**

- Commands are only ever user-triggered; nothing is emitted on attach, so a freshly attached core drains an empty queue.
- [`palette`](#paletters) skips `CommandPalette` when building its candidate list, since the palette cannot dispatch itself.

#### `input.rs`

The toolkit-neutral input vocabulary and its translation into Neovim keystroke notation and RPC.

**Public API**

- `InputEvent` — the events a front-end sends, with `Key`, `KeyEvent`, `MouseButton`, `MouseAction`, `MouseEvent`, `ScrollDirection` and `ScrollEvent`.
- `key_to_notation(&KeyEvent) -> String` — modifier order `C-`, `S-`, `A-`, `D-`; `<` becomes `<lt>`.
- `InputEvent::is_toast_dismissing()` — whether the event is Neovim input that dismisses a showing message toast. The whole rule is in [Input: events](../../../specs/architecture.md#input-events-front-end--core).

**Decisions**

- Shift contract: for `Key::Char` the front-end delivers the already-shifted glyph and leaves `shift` false; `S-` is emitted only for named keys, where Neovim gives it meaning.
- `MouseEvent` and `ScrollEvent` target a grid, with grid-local row and column: under `ext_multigrid` a pane has its own grid, and grid 0 is the global one.
- `Chdir` applies `:tcd`, not `:cd`, which would move every tab of a front-end whose tabs are tabpages of one Neovim ([Input: events](../../../specs/architecture.md#input-events-front-end--core)).
- `Resize` never resizes a local grid: the authoritative size arrives back as `grid_resize`.
- `is_typing` is an exhaustive match, so each new variant must be classed as typing or state, deciding whether it survives a restart ([`api.rs`](#apirs)). So is `is_fast_input` ([`pump.rs`](#pumprs)).
- `SaveAs` carries `force`, applied as `saveas!`, for a caller whose dialog already confirmed the overwrite: plain `saveas` fails `E13: File exists`, and the input pump degrades that to a warning nobody sees. Forcing unconditionally would clobber for a front-end that confirms nothing.

**Gotchas**

- Several variants delegate to `runtime/nvim/init.lua` helpers over `nvim_exec_lua`, which define their behaviour.

#### `markdown.rs`

Pure Markdown to themed-HTML rendering for the preview pane: a `&str` to `String` transform with no GUI toolkit and no filesystem access. Its CSS, themes, and fonts live in [`markdown/README.md`](markdown/README.md).

**Public API**

- `render_html(source, theme, width_mode) -> String` — a complete HTML document with CSS and fonts inlined, loadable directly by a web view. Headings get ids (explicit `{#custom}`, else a deduplicated auto-slug) for anchor links.
- `render_html_with_highlights(…, code_highlights)` — the same render with treesitter spans applied per block, keyed by document order; `None` or a past-the-end index leaves a block plain.
- `code_blocks(source)` — the blocks in document order; the highlight data comes from [`nvim`](#nvimrs).
- `CodeBlockInfo`, `BlockHighlight`, `HighlightSpan`, `HighlightStyle`, `PreviewTheme`, `theme_css`, `WidthMode`, `MARKER_COLUMN_STRIDE`.

**Decisions**

- Alignment contract: `code_blocks` and the highlighter concatenate the same `Event::Text` bodies, including pulldown-cmark's trailing newline, so Neovim's `(row, byte column)` indices match `text.split('\n')`; changing either side silently misplaces every span.
- Marker placement, its byte-identity guard, its exclusions, and the document-order guarantee consumers depend on are one contract with a long half-life: see [the preview position-marker contract](../../../notes/wolf-core-decisions.md).
- Math and highlighted code are `Event::InlineHtml`, so the writer passes them through verbatim and the injector never reads them as a block start.
- `CAPPED_WIDTH_CSS` is horizontal-only: the baseline grid depends on the body's vertical box.
- The head carries [`licenses.rs`](#licensesrs)'s notices as a constant HTML comment — how the inlined faces' attribution reaches a reader. It may not move into the body; see [`markdown/README.md`](markdown/README.md).

#### `licenses.rs`

Two compiled-in registries of verbatim licence texts: every bundled font and statically linked library owes attribution.

**Public API**

- `FontLicense` — `font`, `license`, `provenance`, `text`. `provenance` names the archive, the path inside it and its pinned sha256, so a bumped text is re-derived rather than trusted.
- `FONT_LICENSES` — one row per bundled font.
- `FONT_LICENSE_NOTICES` — those rows as one plain-text block, which [`markdown.rs`](#markdownrs)'s document head and both About surfaces carry.
- `LibraryLicense` — `library`, `license`, `provenance` (as above), `text`. For a library offered under a choice, `license` names the licence Wolf takes.
- `LIBRARY_LICENSES` — libghostty-vt, then the code its static archive compiles in.
- `LIBRARY_LICENSE_NOTICES` — those rows as one block, which both About surfaces carry after the font block and the preview document does not, embedding none of that code.

**Decisions**

- The blocks are `concat!` `const`s, not functions, because the preview head must be byte-identical on every render.
- Every row carries an explicit trailing newline: three texts end without one, which the others would hide.
- Symbols Nerd Font Mono's text appends its release README's icon-set table, since its MIT `LICENSE` names no icon set's licence.
- The Unicode Character Database row is there because uucode compiles its data in. uucode ships no copy of that licence, so the text is aro's.
- The texts under `licenses/` are committed as `.txt`, so prettier's Markdown gate cannot reformat a third party's licence. This entry documents that directory, which has no README of its own.

### Search, traversal, and ranking

#### `files.rs`

Purpose: the streaming file-list traversal behind the picker's `FileSearch` session — a synchronous MRU seed, then a background `ignore`-crate walk — plus the `SearchRegistry`, which re-walks live sessions on `wolf_fs_changed` and on its own `notify` watcher's events. The traversal rules are in the module doc, the rescan's why in [wolf-core-decisions.md](../../../notes/wolf-core-decisions.md).

**Public API**

- `FileEntry` — the canonical definition, re-exported from `mru` and the crate root.
- `FileSearch::read` — returns an owned guard, so a reader's borrow may outlive the `FileSearch`.

**Decisions**

- The walk is bounded by `SearchBounds` (the `WOLF_SEARCH_*` caps, each rejecting zero) because the root can fall back to `$HOME`. A capped walk ends `truncated` yet `complete`; only a real `cancel` leaves it incomplete.
- Both walks share the one `walk_files` helper so their rules cannot drift; a rescan reuses the session's original `SearchBounds` and re-seeds from `MruStore::recent`, which prunes rows for deleted files.
- A session's root is watched unless `WOLF_FS_WATCH=0`, read per search like the caps. `WatchFilter` drops each event path `walk_files` would not list ([the gitignore chain](../../../notes/crates/wolf-core/src/files.rs.md)).
- A rename, delete, `.gitignore` edit or refocus edge forces a re-walk; for `BufWritePost` and a watcher create or modify, `needs_rescan` rescans only for a path under the root and absent from `seen`. Hence `append_batch` folds streamed paths into `seen`, or every `:w` of a walk-discovered file would walk.

**Gotchas**

- The session shape is the shared snapshot-and-wake pattern. The synchronous MRU seed also seeds the dedup set, so a rediscovered recent file is not listed twice.

#### `search.rs`

Purpose: the ripgrep-backed content search behind the sidebar panel's `SearchSession`, streaming `rg --json` match events into a versioned snapshot.

**Public API**

- `SearchOptions` — `case_sensitive` selects `rg -s` or `-i` outright; there is no smart-case. `all_files` off restricts the search to Markdown globs.
- `SearchMatch` — `column` and `match_ranges` are byte offsets into `line_text`; `rel_path` falls back to the absolute path outside the root.
- `SearchState::matches` — accumulates in rg emission order; display order is imposed on read by [`search_tree::project`](#search_treers).
- `SearchSession::cancel` — also kills the rg child, as does `Drop`.

**Decisions**

- `--no-config` keeps results independent of the user's ripgrep config, `-e` protects a query starting with a dash, and `--` separates the root.
- `--hidden` keeps dot-directories such as `.github/` searchable, while one `--glob '!<name>'` per [`files.rs`](#filesrs)'s `EXCLUDED_DIR_NAMES` keeps rg out of `.git/` and `.wolf/`, which the picker walk prunes too.
- rg's exit code 1 means "no matches", not failure; only `>= 2` becomes `SearchState::error`.
- Only the `text` form of rg's data wrapper is deserialized. A non-UTF-8 path or line arrives as a `bytes` payload and is skipped — the deliberate alternative to taking a base64 dependency.
- A malformed JSON line is skipped rather than ending the stream.

**Gotchas**

- The snapshot-and-wake shape is shared with `files.rs`; see [wolf-core-decisions.md](../../../notes/wolf-core-decisions.md). What differs here: batches also flush on each per-file `end` event so a file's matches surface as a group, and the child is published into a shared slot so a concurrent `cancel` can kill it.

#### `search_tree.rs`

Purpose: the pure projection of a flat `SearchMatch` list into the panel's two-level file/line tree, plus the per-line context window that trims a long line to the sidebar's width.

**Public API**

- `project` — groups by `abs_path`, sorts groups by `rel_path` case-insensitively with the raw `rel_path` as tie-break, and stable-sorts each group's lines by line number.
- `SearchFileGroup::match_indices` — indices into the exact slice passed to `project`; they mean nothing against any other snapshot.
- `context_window(line, ranges, budget)` — `budget` counts characters, never pixels. The returned `ranges` are byte offsets re-based into the returned `text`.

**Decisions**

- Width is a character budget so the math needs no display server and stays headlessly testable. The front-end converts its own pixel width and draws the ellipsis glyphs; keeping those out of `text` is what keeps the re-based byte offsets correct.
- The window is sliced on character boundaries, so a multibyte scalar or emoji is never split.
- Context is balanced around the first match but greedy at the edges: when the match sits near one end, the budget that side cannot spend goes to the other, so a match near the start still fills the full width.

#### `tree.rs`

Purpose: the pure projection of the picker's flat `FileEntry` list into the per-directory children a lazy tree view needs, plus the helpers that reveal a file in that tree.

**Public API**

- `children_of(entries, dir)` — the immediate children of `dir`; `""` is the cwd root.
- `TreeIndex` — the same projection built in one pass, with `children_of` returning a borrowed slice.
- `TreeIndexCache::index_for(version, entries)` — rebuilds only when the file-search snapshot version changes; `build_count` reports actual rebuilds.
- `rel_path_for(entries, abs)` — exact path match only, so a parent directory never resolves to its child.

**Decisions**

- Siblings sort directories first, then case-insensitively by `name` with `rel_path` as a deterministic tie-break — the same rule `search_tree::project` applies to its groups.
- Directories are derived from file paths, so a directory appears only when it transitively contains a listed file, and phantom directories are impossible.
- The free `children_of` builds a throwaway index per call. A tree view whose per-child callbacks fire many times at one snapshot must go through `TreeIndexCache`, or every callback pays a full rescan of the flat list.
- Paths are split on `/` only, matching `FileEntry::display`'s separator.

#### `rank.rs`

Purpose: the pure, IO-free fuzzy match and ordering behind the file picker — given a query and the entries collected so far, which ones match and in what order.

**Public API**

- `rank_matches(query, entries)` — an empty or all-whitespace query returns every entry MRU-first. Otherwise the query splits on whitespace into terms, and an entry survives only if every term is a case-insensitive subsequence of `display`, each term matched independently, so term order does not matter.
- `rank_matches_top_n(query, entries, n)` — exactly `rank_matches(...).take(n)` for all inputs, selected with a bounded heap. This is what the picker FFI calls.
- `Match::index` — points into the input slice; map results back by index rather than reading `MatchKey`.

**Decisions**

- `MatchKey`'s field order is the sort priority and every field is "lower is better". MRU-first is the locked top level.
- `COMPONENT_START_BONUS > WORD_START_BONUS > CONTIGUOUS_BONUS` by margins wide enough to make the single-term tiers strict: a component-start always beats a word-start, which always beats a bare substring, which always beats a scattered subsequence. Narrowing the gaps silently reorders results.
- The internal borrowed key adds the entry `index` as a final tie-break to make its order total; without it the bounded path's unstable sort could disagree with `rank_matches`.
- Scoring folds ASCII case in place and allocates nothing per entry. Rewriting it around `to_lowercase()` costs an allocation per entry and switches matching to Unicode folding.

#### `db.rs`

Purpose: the crate-private open, versioning and recovery of `wolf.sqlite`, by [§1 of the application-state spec](../../../specs/application-state.md#1-storage). A store reaches its tables only through a `Table`.

**Public API** (crate-private)

- `Table::open(dir, spec)`, and `open_all(dir, specs)` for several tables on one connection — infallible and eager, so the file exists once it returns. `Table::disabled(spec)` never connects.
- `with_conn(f)` — `None` for a disabled handle and on any failure; a missing connection is reopened lazily.
- `is_newer(spec)` — lock-free, so a `with_conn` closure may call it.

**Decisions**

- `connect` installs `busy_timeout` before `journal_mode=WAL`, the one statement that can need an exclusive lock; without it, opening an unconverted file beside others fails at once with `SQLITE_BUSY`.
- `ensure_table` checks, recreates or migrates a table in one `BEGIN IMMEDIATE` transaction, or of two handles opening at once the later would drop the table the earlier recreated and wrote.
- `TableSpec::version` is the only shape check: bump it with `columns`, plus a state table's migration (`version` is `migrations.len() + 1`). A table with no `schema` row is recreated.
- A newer state table's flag is sticky across reopens and heals ([why](../../../notes/wolf-core-decisions.md#the-application-state-store)).
- Only `SQLITE_CORRUPT` or `SQLITE_NOTADB` deletes the file and its sidecars, destroying every table's rows; a busy, locked or schema error never may.
- The legacy files are removed on every `Table::open_all`.

**Gotchas**

- A heal unlinks `wolf.sqlite` under the other live connections, which keep the unlinked file until they meet the fault, and their heal then deletes the file the first one recreated.

#### `mru.rs`

Purpose: the recent-files cache — the global `file_mru` table of `wolf.sqlite`, opened through [`db.rs`](#dbrs) — letting the picker offer recent files synchronously and rank them first.

**Public API**

- `MruStore::open(dir)` — infallible. A failed open leaves the store inert and retries on the next call.
- `MruStore::disabled()` — every method a no-op, for an unresolvable state directory.
- `record(directory, file)` / `recent(directory)` — `mru_rank` is the 0-based position in the returned list; `recent` omits and deletes rows whose backing file is gone.

**Decisions**

- One global table with a `directory` column, not one per project: isolation is a `WHERE directory = ?` filter. `record` and `recent` must apply identical normalization (canonicalize, else as-given), or macOS resolving `/var` to `/private/var` on one side alone makes rows unfindable.
- Every operation is fire-and-forget: nothing panics or propagates, because a cache hiccup must never block opening a file.

#### `recent_dirs.rs`

Purpose: the recent-directories cache — the global `folder_mru` table of `wolf.sqlite`, opened through [`db.rs`](#dbrs) — recording every directory the captive Neovim changes into, whether through the open-folder command or a typed `:cd`/`:lcd`/`:tcd`.

**Public API**

- `RecentDirs::open` / `disabled` / `record` / `recent` — the same infallible, silently self-healing shape as [`mru.rs`](#mrurs), newest-first and capped at `RECENT_DIRS_LIMIT`.

**Decisions**

- Ordering is a monotonic `seq` column (`MAX(seq)+1` per insert), not a timestamp, so move-to-top and the cap stay deterministic for inserts landing in the same millisecond. That is what lets the tests avoid sleeps.
- `record` stores the path as given: it arrives already canonical from `apply_cwd`, so canonicalizing again would only add a filesystem hit.
- `recent` neither prunes nor filters vanished directories, unlike `MruStore::recent`: [`WolfCore::recent_dirs`](#apirs) filters them on read, so the store keeps a returning folder's row.

### Pickers, panes, and preview

#### `buffer_picker.rs`

Purpose: Pure candidate assembly and ranking for the buffer picker, over a snapshot of the captive Neovim's open buffers rather than the file tree.

**Public API**

- `BufInfo` — one buffer as decoded from `getbufinfo({'buflisted': 1})`.
- `BufferRow` — one ranked row; `display` is both the label shown and the string ranked against the query.
- `rank_buffers(buffers, cwd, query, max)` — orders by `lastused` descending (stable, tie-broken by `bufnr` ascending), so the current buffer sorts first and is included rather than filtered out. An empty query returns everything MRU-first.

**Decisions**

- Ranking delegates to `rank::rank_matches` by projecting each buffer into a `FileEntry` whose `mru_rank` is its position in the `lastused` order. One matcher serves the file picker, the palette and this, so their query semantics cannot drift apart.
- No IO at all, which is what keeps the module headlessly unit-testable. Candidates are fixed at picker-open by one blocking `WolfCore::list_buffers` and re-filtered synchronously — no streaming session and no wake.
- Display strings are cwd-relative or the absolute name verbatim; there is deliberately no `~`-home fold here, unlike `pane_status`.

#### `palette.rs`

Purpose: Pure candidate assembly and ranking for the command palette — static registry commands, "switch to window" rows, and "open recent folder" rows — ranked against the command MRU.

**Public API**

- `PaletteRowKind` — which of the three actions a row performs; it also determines which of `command` / `window_index` / `path` is `Some`.
- `PaletteWindow` — one _other_ open window. The caller excludes the current one.
- `PaletteRow` — `mru_key` is the opaque key the caller records into `CommandMru` on activation: a `COMMAND_SPECS` id, `switch_window:<cwd>`, or `open_recent:<path>`.
- `rank_palette(recent_dirs, windows, command_mru_recent, query, max)` — assembles in a fixed order (static commands, then windows, then recent dirs) that acts as the tie-break for equally ranked rows.

**Decisions**

- `Command::CommandPalette` is excluded from its own candidate list; the palette cannot dispatch itself.
- Ranking delegates to `rank::rank_matches` by projecting rows into `FileEntry` candidates, the same path `buffer_picker` and the file picker take, so query semantics stay identical across all three. The module does no IO, which keeps it headlessly unit-testable.

**Gotchas**

- `PaletteRowKind`'s variant order is a wire contract: wolf-ffi's `palette_row_kind` discriminants and the Swift `PaletteSeam` port are numbered from it. Append, never reorder.

#### `command_mru.rs`

Purpose: The global most-recently-used cache of command palette activations — the `command_mru` table of `wolf.sqlite`, opened through [`db.rs`](#dbrs) — keyed on an opaque command key string so it holds static commands and dynamic window/folder rows alike.

**Public API**

- `CommandMru::open(dir)` — infallible. A failed open is silent, and is retried on the next call.
- `CommandMru::disabled()` — every method a silent no-op; used when `state::state_dir` cannot be resolved.
- `record(key)` — fire-and-forget move-or-add-to-top, pruned to `COMMAND_MRU_LIMIT`.
- `recent()` — keys newest-first, empty on any failure.

**Decisions**

- Ordering is a monotonic `seq` column, not a timestamp, so move-to-top and the cap stay deterministic for same-millisecond inserts and tests need no sleeps.
- Nothing panics or propagates, and the only log is at `debug`: the store is disposable, and a palette activation must never be blocked or made noisy by it.

**Gotchas**

- Nothing auto-records here — only an explicit `record` writes — so a fresh store reads back empty.

#### `pane_status.rs`

Purpose: Pure display-string math for a pane's statusbar: the base filename and the path shown beneath it.

**Public API**

- `NO_NAME` — the placeholder filename for an unnamed buffer, also reused by `buffer_picker`.
- `PaneStatus` — `display_path` is relativized (cwd-relative wins, else the `~`-prefixed home form, else absolute), while `abs_path` is the raw buffer path and is never relativized, because the macOS front-end persists it as a pane-layout leaf path.
- `pane_status(buf_path, cwd, home)` — for a buffer only. A terminal pane's status comes from its session ([`terminal.rs`](#terminalrs)).

**Decisions**

- The inputs must already be canonical, and the caller canonicalizes buffer path, cwd and home before calling. Relativization is `strip_prefix`, which is textual, so an uncanonical path silently falls through to the absolute form instead of failing.
- Touches no filesystem, which keeps it headlessly unit-testable.

**Gotchas**

- `buf_path == cwd` strips to an empty remainder and falls through to the home or absolute form rather than yielding an empty `display_path`.

#### `nav_history.rs`

Purpose: Per-pane back/forward navigation history, the single source of truth behind the History menu and its toolbar buttons. Keyed by nvim window handle and not persisted. The stack is a literal visit log, deduplicated only when rendering rows.

**Public API**

- `NavTarget` — the module's dedup key. `Edit(X)` and `Preview(X)` are deliberately distinct entries.
- `derive_target` — `None` for every terminal pane, the drawer included ([why](../../../specs/terminal-panes.md#what-goes-away)).
- `NavHistory` / `PaneHistory` — `observe` folds one target; `back`/`forward`/`jump` return a `NavMove`, and a failed move is undone with `revert(from: mv.to, to: mv.from)`.
- `menu_rows(limit, home)` — newest-first distinct targets with minimal-growth prefix labels; the checkmarked row's `index` is the cursor's own, so clicking it is a no-op.

**Decisions**

- `back`/`forward`/`jump` arm a per-pane `expected` in the same mutation that moves the cursor, before the caller issues the RPC, and it never gates pushing. A stale `expected` cannot wedge history: the next genuine navigation self-heals it in one step, so no timer is needed.
- `NavHistory::observe` drops a target identical to the last one folded for that window. Neovim fires the driving autocmds several times per navigation, across ticks the Lua side cannot coalesce; a late repeat of the state just left would otherwise push right after a Back, truncating the entry being moved to.
- A window in neither map yields no entry, no push and no prune, making the two notifications' arrival order irrelevant during a preview transition. Do not add an "absent for N emits" reaper.

#### `browser_preview.rs`

Purpose: The lazily-started loopback HTTP server behind `WolfCore::open_in_browser` — one per core, serving a tiny static site rooted at the project so relative links in rendered Markdown resolve in URL space.

**Routes** (`relpath` is the pinned file's root-relative path)

- `GET /` — 302 to `/<relpath>`, or renders the pinned buffer in place when it is unnamed.
- `GET /<relpath>` — the pinned buffer rendered live, carrying an injected script that polls `/tick`. The only live page.
- `GET /tick` — the pinned buffer's `changedtick` as plain text.
- `GET /<other.md>` — rendered from disk, with no reload script.
- `GET /<asset>` — raw bytes, content type guessed from the extension.
- Anything else, and anything resolving outside the root — 404.

**Public API**

- `BrowserPreview::port()` — the bound loopback port. `abort()` — idempotent accept-loop cancellation; dropping the core's runtime also cancels.

**Decisions**

- The loopback-only bind and the path-traversal guard are hard security requirements — see [wolf-core-decisions.md](../../../notes/wolf-core-decisions.md).
- Responses are framed as `Vec<u8>` end to end so binary assets are never routed through `from_utf8_lossy`.
- The injected script's baseline is the render-time tick, which is what keeps the first poll from reloading immediately.
- `resolve_path` enforces containment with `starts_with`, so `root` must itself be canonical (on macOS `/var` resolves to `/private/var`).

**Gotchas**

- Each `open_in_browser` stores a fresh target, so the server re-points per invocation but does not follow the user's buffer between them.

### Terminal sessions

#### `terminal.rs`

Purpose: the terminal sessions core owns, each a pty, a shell, a `wolf_vt::Terminal` and a published `TerminalFrame`, shown in Neovim placeholder windows, and the registry behind `WolfCore`'s session methods. The pty, the router and the session are in [`terminal/`](terminal/README.md); [Terminal panes](../../../specs/terminal-panes.md) owns the contract.

**Public API**

- `open_terminal` and `bind_terminal` block through `block_on_rpc` and return once the binding is in `win_buffers`. The other session methods never wait on Neovim.
- `TerminalId` counts from 1 per core and is never reused, so a stale id names no session rather than another one.
- `terminfo_dir_from_env()` reads `WOLF_TERMINFO_DIR` for `CoreConfig.terminfo_dir`, which reaches a shell verbatim as `TERMINFO` and must be absolute ([why](../../../notes/wolf-core-decisions.md#terminal-sessions)).
- `live_terminals` lists running sessions only, while an exited session's frame stays readable until its pane closes ([why](../../../notes/wolf-core-decisions.md#terminal-sessions)).
- Mouse and scroll positions are pixels from the content area's top-left, turned into cells at the last `resize_terminal`'s cell size.
- `WolfCore::pane_status` of a terminal pane reports the session's shell name, or `NO_NAME` once the session is gone, with empty paths.
- The drawer, restart and restore methods, `toggle_terminal`, `drawer_terminal`, `queue_bind_terminal` and `terminal_cwd`, are specified in [Terminal panes](../../../specs/terminal-panes.md#the-drawer).

**Decisions**

- The registry reaches Neovim through `nvim.rs` free functions over the pump's current handle, never a captured one: a restart replaces the handle while the sessions live on.
- A router chord or a split command acts on the window showing the session: the active window when it does, else one on the current tabpage, else any, and with none it is dropped.
