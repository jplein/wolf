# wolf-core decisions

Last verified at commit `1abfb83b013b22dd42b5e13ae533b4f0346c99cd`.

The durable constraints behind this crate. Read this before changing anything here. Per-file detail lives in [`src/README.md`](../crates/wolf-core/src/README.md).

## The one hard rule

> `wolf-core` must compile with no GUI toolkit in its dependency tree.

This is what lets one core serve both front-ends, and it is checked, not assumed. The `wolf-core-deps` gate of [`gates.sh`](../gates.sh) lists core's dependency graph on each platform, and fails on any entry that matches a pattern in [`tools/lint/wolf_core_forbidden_deps.txt`](../tools/lint/wolf_core_forbidden_deps.txt). How and why each platform lists the graph is in the gates notes, for [Linux](gates.sh.md#linux-gates) and [macOS](gates.sh.md#macos-gates).

The gate lists the graph of [`wolf-test-support`](../crates/wolf-test-support/README.md) too, though core does not depend on it. Every wolf-core and wolf-ffi Bazel test links it, so a GUI crate in its graph would enter the Bazel graph, which the GTK stack is kept out of.

The gate does not repin. After any dependency or feature change, repin `crate_universe`, because a feature change moves the Bazel lockfile even when no crate node is added.

`wolf-gnome` is given no Bazel target at all, which is how the GTK stack stays out of the Bazel graph.

Every dependency here was chosen partly because it is GUI-free:

| Need | Dependency | Note |
| --- | --- | --- |
| MRU cache | `rusqlite` (`bundled`) | Vendors SQLite via `cc`, so no system SQLite to provision |
| File traversal | `ignore` | ripgrep's pure-Rust walker |
| File watching | `notify` (`=8.2.0`) | FSEvents through `fsevent-sys` on macOS, inotify on Linux. Pinned exactly because 8.2.1 swaps in `objc2-core-*` bindings, which the `wolf-core-deps` gate rejects. `notify-types` is held at 2.1.0 in `Cargo.lock`, because 2.2.0 needs a newer rustc than Bazel's |
| Config | `serde` (`derive`), `serde_yaml`, `dirs` |  |
| State | `serde_json` |  |
| Markdown preview | `pulldown-cmark`, `pulldown-cmark-escape` |  |
| Math preview | `pulldown-latex` | Adds only `bumpalo` |
| Terminal emulation | `wolf-vt` | The workspace binding over libghostty-vt, linked statically; [Building libghostty-vt](../specs/terminal-panes.md#building-libghostty-vt) owns the pin |
| Ptys and processes | `libc` | `openpty`, `setsid`, `TIOCSCTTY`, `TIOCSWINSZ`, signals, `getpwuid_r` and `proc_pidinfo`, hand-rolled in place of a pty crate |
| Content search | none | Shells out to a pinned `rg` subprocess located via `WOLF_RG_BIN` |
| Browser preview | none | Hand-rolled HTTP/1.1 on the existing tokio runtime, `net` feature |

The last two are the pattern to follow when adding a capability: prefer a subprocess or a hand-rolled implementation over a crate that might drag a toolkit in.

## How core embeds Neovim

- **Generic untyped RPC.** All calls go through `Neovim::call` with `rmpv::Value` arguments, and the `redraw` stream is consumed as raw `Vec<rmpv::Value>`. This insulates core from the mismatch between `nvim-rs` 0.9.2's older typed API and Neovim 0.12. Do not reach for the typed API.
- **UI attach options.** `nvim_ui_attach` sets `ext_linegrid`, `ext_multigrid`, `ext_popupmenu`, `ext_messages`, and `rgb`. `ext_multigrid` is what gives each Neovim window its own grid, so a shell can own a native split-pane layout. `ext_messages` implicitly enables `ext_cmdline`.
- **Binary discovery.** The nvim binary is located only via `WOLF_NVIM_BIN`, never `PATH` and never by scanning the Nix store.
- **Hermetic launch.** `nvim --embed --clean -u runtime/nvim/init.lua -i NONE`, with `NVIM_APPNAME=wolf` and isolated `XDG_{CONFIG,DATA,STATE,CACHE}_HOME` under a private temp directory, so the captive editor cannot read the user's config or state.
- **stderr is captured, not discarded.** It is piped and drained to the log by a background `BufReader::lines()` task. Redirecting it to `/dev/null` would hide a crashing nvim's diagnostics, and not draining it would deadlock on a full pipe buffer.
- **Swap files stay enabled.** The launch deliberately does not pass `-n`, which would zero `updatecount` and disable swap. Swap files are what recover unsaved edits after a crash restart. The one exception, a path too long for Neovim's swap name, is in [the runtime README](../runtime/nvim/README.md#decisions).

## The bundle is Wolf's; `state_dir()` is the user's

The embedded runtime — nvim, `init.lua`, the vendored plugins and parsers — is Wolf's and is replaced wholesale on upgrade; nothing under it is user-writeable. Everything under `state_dir()` is the user's, and Wolf never modifies it, with one exception: `seed_runtime_user_dir` writes the `init_user.lua` template into a missing or empty `runtime_user/`, and never again once the directory has anything in it — even when the template file itself is absent, because a non-empty directory is the user's and a deleted template may be a decision.

Core resolves the durable directories (`swap`, `spell`, `runtime_user`, `parsers_user`) and hands them to `init.lua` as `WOLF_*` environment variables, so `init.lua` needs no platform knowledge. Seeding happens in `WolfCore::start`, not in `spawn`, so the crash-respawn path carries no new failure surface; a seed failure warns and startup continues.

## Liveness and restart

Core keeps a lock-free `Arc<AtomicU8>` liveness cell (`Alive` / `NotResponding` / `Dead`) alongside a clone of the redraw wake, mutated only through [`liveness`](../crates/wolf-core/src/README.md)'s transition helpers so each real change fires the wake exactly once.

A supervisor task owns the io-loop join handle and marks the process `Dead` on stdout EOF, whether clean or a crash — unless the per-process `shutting_down` flag is set, so an intentional shutdown does not report a spurious crash.

`WolfCore::start` ignores `SIGPIPE` for the whole process. nvim-rs writes each call straight to Neovim's stdin, so the first write after Neovim dies meets a closed pipe: a pump batch, a blocking query, or a resize still queued at shutdown. Rust's runtime ignores `SIGPIPE` before `main`, so a Rust host gets `EPIPE` and the supervisor's `Dead`, but a Swift host keeps the default action, under which the same write kills Wolf.app or the XCTest runner.

Core sets the disposition rather than each host, because core is what writes the pipe, and the app and the test host would each need the call. `std::process::Command` restores the default in every child it spawns, so Neovim, `rg` and the terminal sessions' shells keep the usual `SIGPIPE`.

Stall detection has two sites, `block_on_rpc` and the input pump, and both judge one call at a time. A call that outlasts the RPC budget escalates `Alive → NotResponding` and an answer clears it, both by `compare_exchange`, so neither can downgrade `Dead`. The pump keeps awaiting a stalled call rather than dropping it ([why](../crates/wolf-core/src/README.md#pumprs)). It marks and clears only while its batch's handle is still current, so a call sent to a replaced Neovim cannot report on the fresh one.

The pump's budget re-arms each time it elapses, and when a Neovim prompt is showing then, the pump clears rather than marks, because the user is late, not Neovim, and the overlay would cover the prompt. A probe that Neovim answers while the call waits clears it too.

A crashed core stays `Dead` until `restart_nvim`, which force-kills and reaps the dead child, respawns in the same working directory and config, and re-runs the full setup chain.

A restart starts the render state over. A fresh Neovim numbers its windows from 1000 and its tabpages from 1 again, so anything keyed by the dead process's handles would be read as the fresh one's: the snapshot is reset, and the layout intents and the navigation history are emptied. The commands queue is kept, because front-ends drain it in every liveness state. `kill_in_place` first waits for the killed process's supervisor, which resolves only once its io loop and handler loop are gone, so no notification from it lands after the reset.

The snapshot reset carries four fields. `seq` never goes back, because flush waiters key on it advancing. `open_files`, `tab_cwds` and `current_tabpage` are what a front-end captures before restarting, and a failed respawn must leave them for the next attempt; the fresh Neovim replaces each one wholesale during setup.

Terminal sessions live beside the Neovim process rather than in the snapshot, so neither a crash nor a restart touches them. A front-end puts them back after a respawn through `queue_bind_terminal`, which goes through the input queue ([When Neovim dies](../specs/terminal-panes.md#when-neovim-dies) says why).

The front-end reads liveness on each wake to drive its crash and stall overlay.

## Threading and locking

- Core owns a multi-thread tokio runtime on its own threads; the `nvim-rs` handler receives `redraw` notifications there.
- A single `parking_lot::RwLock` guards the snapshot. The redraw handler takes the write lock once per whole batch, never holds it across an `.await`, and fires the wake only after the lock is dropped, on `flush`.
- A flush can arrive with no input: Neovim's markdown ftplugin starts treesitter on every markdown buffer, and a parse that outlasts its budget finishes through `vim.schedule` and redraws later, so a test polls rather than reading once after a flush ([the shared harness](../crates/wolf-core/tests/README.md#the-shared-harness--commonmodrs)).
- That write lock is acquired cooperatively (a `try_write` retry loop that yields to the scheduler), so a front-end holding a read guard cannot pin a worker thread.
- `rebuild_layout` returns only once its windows are in the snapshot, and the barrier that guarantees it is an nvim→core request, `wolf_barrier`. nvim-rs answers an RPC response straight from its io loop but applies notifications in its handler loop, so a `wolf_buffers` emit sent before the response can still land after the caller wakes. The handler loop awaits each notification inline and spawns a request only after it, so the request's answer proves every earlier emit is applied. Folding the RPC's own copy of the list instead would overwrite a newer emit already applied.
- nvim-rs runs those two loops under one `try_join`, so when the io loop fails at a dying Neovim's stdout EOF, the handler loop is dropped with every notification still queued. The last redraw batches and emits before a death may never land, and nothing may rely on them. The restart's reset covers everything they feed but the commands queue, so a `wolf_command` sent in Neovim's last moment can be lost.
- Input is delivered through an ordered FIFO channel to a single consumer task, which fuses a run of consecutive `Key` events into one `nvim_input` call. This keeps keystrokes in order and makes a burst produce a single redraw flush.
- A pump call can wait on the user. Neovim answers a request only when its handler returns, and a prompt raised inside one holds it open: `:write` over a file changed on disk asks y/n, and `nvim_ui_set_focus` runs the `FocusGained` file check, whose W12 asks about a buffer changed on both sides. A request also waits in Neovim's queue behind a half-typed chord such as `<C-w>`. A pump that only awaited the call would hold back the key that answers it, a deadlock.
- So while a call is unanswered the pump keeps receiving, holds what arrives, and probes `nvim_get_mode`. Neovim answers the probe at once while it waits on a keystroke, and in order behind a call that is merely busy, so an answer before the call's means Neovim waits for input; the pump then releases the oldest held key, mouse or scroll event through Neovim's fast input calls ([`pump.rs`](../crates/wolf-core/src/README.md#pumprs)).
- W12 inside the focus call is intermittent: Neovim throttles its focus check to once per 2 s and defers a throttled one, like `WolfAutoRead`'s autocmd `:checktime`, to its main loop, where keys always flowed.
- Each terminal session owns two threads, a reader `wolf-term-<id>` and a writer `wolf-term-<id>-w`. Everything a session waits on happens on its reader: the pty, the publish deadlines, the exit, and the hang-up's grace and reap. Every pty write happens on its writer, so no front-end or tokio thread waits on a shell, but for `shutdown`, which joins the readers.
- A session's terminal lock is never held across a pty write, an RPC, a wake or the frame lock. Neither the registry's map lock nor the snapshot lock is held while a session lock is taken: the registry clones a session's `Arc` out of its map first, and the redraw handler pushes the editor's colours to the sessions only after its write lock drops.
- A session's threads hold the session's `Arc` and only a `Weak` to the registry, so dropping a `WolfCore` without `shutdown` still drops the registry, whose `Drop` hangs every session up without waiting.
- Each `SearchRegistry` owns a `wolf-fs-watch` thread, which makes every `notify` watch and unwatch call. A recursive inotify watch walks the whole tree, so it never runs on a front-end's thread or under the registry's locks. notify's `watch` waits on the backend thread that runs the event callback, so the callback only forwards into the thread's channel and never blocks or locks.

## Terminal sessions

[Terminal panes](../specs/terminal-panes.md) owns the contract. These are the decisions behind core's half of it.

- `KeyEvent` carries no physical key. libghostty-vt's key encoder derives control and modifier sequences from the key code, the modifiers and the unshifted text, and core maps a character to the US-layout key that types it. A new field would change every `KeyEvent` literal in both front-ends for nothing the encoder reads.
- A router split or close runs as a task on core's runtime, so `terminal_key`, called on a front-end thread, never waits on a wedged Neovim. A focus chord pushes its `FocusDir` intent directly. A split runs the origin session's program and arguments again rather than the default shell.
- A typed `:terminal` reaches core as the `wolf_terminal_open` notification, whose handler only hands the window to a runtime task, as a router split does. The bind ends with `wolf_sync_buffers`, whose `wolf_barrier` request nvim-rs answers only after the running notification handler returns, so awaiting the bind inline would deadlock.
- A shell's exit is published at once, and the reader then asks Neovim, through `wolf_terminal_exited`, to close every window showing the session. The resulting wipe removes it, so the exited frame stays readable while a pane still shows it. Core removes the session itself only when no window showed it or the call failed. A hidden drawer's placeholder has no window, so `wolf_terminal_exited` also deletes every buffer still tagged with the session, and that wipe removes it; the next toggle then starts a new shell.
- `close_terminal` hangs a running session up rather than removing it, so its panes close by that same exit path.
- A hang-up sends `SIGHUP` to the shell's group, closes the master, polls up to `TERMINAL_HANGUP_GRACE`, then sends `SIGKILL` and reaps. `shutdown` hangs every session up before joining any reader, so it takes one grace for all of them rather than one each.
- libghostty-vt's callbacks only push onto an effect queue the `wolf_vt::Terminal` owns. They run inside a library call under the session lock, where a pty write or a lock would stall or deadlock, so whoever holds the lock drains the queue after the call and hands any reply to the writer.
- A publish is coalesced to one per `TERMINAL_PUBLISH_INTERVAL`, the exit's excepted. A synchronized update defers it for at most `TERMINAL_RENDER_HOLD_MAX`, after which the hold counts as released until the program sets mode 2026 again, so a program that never ends its update cannot freeze its pane.
- The `BufWipeout` notifier is silent while Neovim quits (`v:dying`, `v:exiting`), so quitting Neovim hangs up no session; core's own shutdown does.
- Core learns Wolf's terminfo directory from `CoreConfig.terminfo_dir`, never from its own environment or a path relative to the executable. A per-core field lets one test process run a core with the directory and another without it, where changing the environment per test would race `getenv` on libtest's other threads. Core assumes no install layout, which differs per platform and is a test binary's under `cargo test` and `bazel test`. The front-ends fill the field from `WOLF_TERMINFO_DIR`, which packaging sets only as a default, following the `WOLF_NVIM_BIN` rule, so a developer's export wins.

## The snapshot-and-wake session pattern

The file picker, the content search, the redraw stream and the terminal frames all share one shape, and a new streaming capability should adopt it rather than inventing another: a worker streams results into shared state in batches, bumps a version, and fires the same wake callback the front-end already marshals onto its UI thread. The front-end then reads through an owned guard. No callback ever crosses the FFI boundary mid-stream.

Both search sessions resolve their root from the tracked live working directory cell rather than an RPC, so opening a picker never blocks on a wedged nvim. Dropping or cancelling a session stops its traversal, and for content search kills the `rg` subprocess, so no zombie lingers.

## The file-search rescan: a watcher and nvim events, walk-then-swap

A live `FileSearch` session rescans on three sources: a `notify` filesystem watcher, which is the immediate path for any change on disk, the `wolf_fs_changed` notification from nvim, and a core terminal pane losing focus. All three feed `SearchRegistry::notify_fs_changed`, and the registry is the seam the watcher plugs into without touching the sessions. Its targeted invalidation spares the rescan both a refocus-edge dependency and a walk per refocus.

The watcher watches a root recursively while a live registry entry has it. The watched set is re-derived from the live entries on every register and every prune rather than counted separately, for the same reason the registry has no unregister handshake (below). When a root's watch goes live it forwards that root once, because the session's walk may have started before the watch existed and a change in that gap would be lost.

A watcher remove or rename forces a rescan, since it may take a listed file away. A create or modify relies on `needs_rescan`'s `seen` check, and a `.gitignore` edit forces, since it changes what the walk lists. Every event is first judged by the walk's own rules — the excluded names, the depth cap, `skip_dirs` and the gitignore chain ([how the filter replays it](crates/wolf-core/src/files.rs.md#the-gitignore-chain)). A `cargo build` under an ignored `target/` would otherwise cause a stream of re-walks, and judging by the walk's rules means a dropped event is never one the walk would list.

The nvim triggers are two classes. Path-carrying buffer events (`BufWritePost` conditional, `BufFilePost`/`BufDelete` force) cover changes nvim makes itself; path-less refocus-edge force events (`ShellCmdPost`/`FocusGained`) catch mutations that fire no buffer event — a `:!git mv`, an external terminal or Finder while the app is unfocused. Core adds a path-less force of its own, a terminal pane losing focus (`terminal_focus(id, false)`), which covers a command run in a terminal pane, the drawer's included: every exit from one passes a focus loss.

The refocus-edge triggers stay beside the watcher because the edges are human-paced and the debounce plus the equality skip bound each to at most one no-op walk, and because they are the only path when the watcher cannot run (see the limitations below).

A rescan is walk-then-swap, never streaming: a delete can only be expressed by wholesale replacement, so the re-walk collects into a local vec and replaces the session's list under one short write lock, bumping `version`. A result equal to the current listing skips both the swap and the version bump, so a no-op event — a `:bdelete` of a still-existing file — causes no UI churn.

The registry holds strong handles to each session's state; its liveness signal is the session's existing `cancel` flag, which `Drop` sets. Dead entries are pruned on every register and every notify, so a closed picker or sidebar stops costing walks after at most one more event. Do not add a separate unregister handshake — the flag is already the single cancellation truth, and a second signal could disagree with it.

## Browser preview security

Two hard requirements in the `browser_preview` server, both of which look removable and are not:

- The listener binds `127.0.0.1:0`, loopback only. Never `0.0.0.0`.
- Every request path is checked against the canonical root, and anything escaping it is a 404. The bind being loopback-only is not a reason to drop the traversal guard.

Each connection clones the current target under a short lock dropped before any `.await`, so no lock is held across an await. The `browser_preview` lock is never held across an nvim RPC.

## The preview position-marker contract

`render_html` seeds every rendered document with position markers — `<span class="wolf-src" id="wl-{L}-{C}" data-line="{L}" data-col="{C}"></span>` — one per hard source line at column zero, plus one every `MARKER_COLUMN_STRIDE` characters along a longer line. `L` is a 1-based hard buffer line and `C` a 0-based character column, matching what `wolf_markdown_cursor` reports, so nothing along the path converts between bytes and characters. pulldown-cmark is chosen for `Parser::into_offset_iter`, which pairs each event with its source byte range; the placement rules are built on those ranges, so a plain event iterator is not a substitute.

Placement has two tiers. A column-zero marker is flushed as a raw span before the opening tag of the block that claims it, which for a list is the outermost container rather than the item. A `col != 0` marker inside a paragraph, heading, blockquote, list or footnote definition is deferred to its true position in the rendered inline flow: it splits the `Event::Text` that contains it when there is one, and otherwise lands before the next `Text` or at the block's end rather than collapsing to the block top.

Three properties are load-bearing on consumers and must survive any rewrite. Every marker the rule predicts exists exactly once, because a marker is deferred rather than dropped. Only the column-zero markers are guaranteed to appear in ascending document order — a block's inline markers follow a later line's column-zero marker — so a consumer sweeping the document must filter on `data-col="0"`. And `MARKER_COLUMN_STRIDE` is mirrored in Swift as `PreviewScrollSeam.markerColumnStride`; neither side reads the other, so the Swift assertion pins only its own literal and a change here must be mirrored by hand.

A `Text` is split only when `source[range]` matches the payload byte for byte: a length check passes a same-length entity such as `&nvap;` and then corrupts it, and a failed match defers the marker instead of dropping it. The split point snaps to the nearest word boundary within `MARKER_SNAP_MAX_BYTES`, because an inline-box edge breaks glyph shaping across it and a mid-word cut loses a ligature or kern pair; with no boundary in the window it splits at the exact offset. The id and `data-*` attributes always name the exact `(L, C)`.

Injection is suppressed inside four regions: code blocks, HTML blocks, tables and image alt text. The three block-level regions keep the before-the-tag placement. An image is inline, so its alt-text marker stays deferred across the element and lands at the block's next placement point, after the `<img>` — the exclusion promises only that no marker enters the element, never a before-the-tag position.

A raw span between `<tbody>` and `<tr>` is foster-parented out of the table by the HTML parser. An `alt` attribute is built by pulldown-cmark's `raw_text`, which drops an `Event::Html` outright — which is also why a marker is emitted as `Event::Html` and never as `Event::InlineHtml`, so a leak loses one anchor instead of corrupting the attribute.

What the markers cost the stylesheet is in [`src/markdown/README.md`](../crates/wolf-core/src/markdown/README.md); the granularity that remains out of reach is in [`architecture.md`](../specs/architecture.md).

## The rebuild-layout wire format

`state::rebuild_layout_spec` renders a saved `PaneNode` into the JSON `_G.wolf_rebuild_layout` decodes, and it is that document's one renderer, as the shared `runtime/nvim/init.lua` is its one decoder. GNOME calls it directly; macOS hands the `FfiPaneNode` it loaded back across the FFI to `WolfCoreHandle::rebuild_layout`, which renders it in Rust. So a change to the shape lands in `spec_value` and `init.lua` only, and no front-end can drift from the other. The shape is documented once, in [`runtime/nvim/README.md`](../runtime/nvim/README.md).

The wire document is deliberately not `PaneNode`'s serde shape, so nothing can be shared by serializing the type directly. A leaf path is arbitrary filesystem text, and one unescaped quote makes the Lua side's `pcall(vim.json.decode, …)` fail and abort the whole restore with no window handles at all — which is why the renderer does not hand-roll the JSON, and why the Rust renderer lives beside the type rather than once per front-end.

Leaf order is the other half of the contract. Children are emitted depth-first, the order `WolfCore::rebuild_layout` returns nvim window handles in, so a front-end zips those handles onto its own depth-first leaf list. Reordering either side transposes every restored pane while every count-based assertion stays green.

## Present limitations

- A browser-preview server spawned before a `restart_nvim` keeps the RPC handle it captured at spawn time, so its live render goes blank after a restart until the command is run again.
- Content search skips a non-UTF-8 path or line, because `rg --json` sends those as a `bytes` payload and decoding it would add a base64 dependency.
- Linked Markdown pages in the browser preview are static snapshots rendered from disk. The tick endpoint tracks only the pinned buffer, so only that one page live-reloads.
- The Tier-1 open-file mirror reports the instance-global buflisted file set per tabpage rather than a genuinely per-tabpage set, which over-reopens on restore. The active path is genuinely per-tabpage.
- The file-search watcher does not cover a root it cannot watch, a session started with `WOLF_FS_WATCH=0` whose root no watched session shares, or any root when the watcher cannot start. A recursive inotify watch adds one watch per directory, ignored and skipped trees included, so a root as large as `$HOME` can exhaust `max_user_watches`. An FSEvents stream that fails to start is silent, because notify 8.2.0 ignores `FSEventStreamStart`'s result. An external change under such a root waits for an nvim trigger, and GNOME forwards no OS focus, so `FocusGained` never fires there.
- An edit to `.git/info/exclude`, to the global excludes file, or to a `.gitignore` above the session root changes what the walk lists without forcing a rescan. The watcher's filter can also go on judging events by the rules it read before such an edit until the root stops being watched.

## Deferred

Full `wrapNeovim` plugin baking (`marksman` LSP, formatter), and the non-layout intents (open and save dialogs, window title).
