# wolf-core decisions

The durable constraints behind this crate. Read this before changing anything here. Per-file detail lives in [`src/README.md`](src/README.md).

## The one hard rule

> `wolf-core` must compile with no GUI toolkit in its dependency tree.

This is what lets one core serve both front-ends, and it is checked, not assumed. Re-run both checks after any dependency change, and repin `crate_universe` — a feature change moves the Bazel lockfile even when no crate node is added.

```sh
cargo tree -e no-dev -p wolf-core

bazel query 'kind(rule, deps(//crates/wolf-core:wolf_core))' | grep -iE \
  'gtk|gdk|glib|cairo|pango|adwaita|objc|cocoa|core-graphics|winit|wgpu|skia|softbuffer' \
  || echo clean
```

The `kind(rule, …)` wrapper is load-bearing. A bare `deps()` also returns every source file in the graph, and `libc` vendors its bindings under `src/new/glibc/…`, whose paths match the `glib` pattern — so the guard reports seven `libc` files on every run, `|| echo clean` never fires, and a check that always cries wolf is a check nobody reads. The `cargo tree` check matches on crate names and is unaffected.

No occurrence of any of these is permitted:

```
gtk gtk4 glib gdk4 cairo cairo-rs pango libadwaita
objc objc2 cocoa core-graphics winit wgpu skia-safe softbuffer
```

`wolf-gnome` is given no Bazel target at all, which is how the GTK stack stays out of the Bazel graph.

Every dependency here was chosen partly because it is GUI-free:

| Need             | Dependency                                | Note                                                              |
| ---------------- | ----------------------------------------- | ----------------------------------------------------------------- |
| MRU cache        | `rusqlite` (`bundled`)                    | Vendors SQLite via `cc`, so no system SQLite to provision         |
| File traversal   | `ignore`                                  | ripgrep's pure-Rust walker                                        |
| Config           | `serde` (`derive`), `serde_yaml`, `dirs`  |                                                                   |
| State            | `serde_json`                              |                                                                   |
| Markdown preview | `pulldown-cmark`, `pulldown-cmark-escape` |                                                                   |
| Math preview     | `pulldown-latex`                          | Adds only `bumpalo`                                               |
| Content search   | none                                      | Shells out to a pinned `rg` subprocess located via `WOLF_RG_BIN`  |
| Browser preview  | none                                      | Hand-rolled HTTP/1.1 on the existing tokio runtime, `net` feature |

The last two are the pattern to follow when adding a capability: prefer a subprocess or a hand-rolled implementation over a crate that might drag a toolkit in.

## How core embeds Neovim

- **Generic untyped RPC.** All calls go through `Neovim::call` with `rmpv::Value` arguments, and the `redraw` stream is consumed as raw `Vec<rmpv::Value>`. This insulates core from the mismatch between `nvim-rs` 0.9.2's older typed API and Neovim 0.12. Do not reach for the typed API.
- **UI attach options.** `nvim_ui_attach` sets `ext_linegrid`, `ext_multigrid`, `ext_popupmenu`, `ext_messages`, and `rgb`. `ext_multigrid` is what gives each Neovim window its own grid, so a shell can own a native split-pane layout. `ext_messages` implicitly enables `ext_cmdline`.
- **Binary discovery.** The nvim binary is located only via `WOLF_NVIM_BIN`, never `PATH` and never by scanning the Nix store.
- **Hermetic launch.** `nvim --embed --clean -u runtime/nvim/init.lua -i NONE`, with `NVIM_APPNAME=wolf` and isolated `XDG_{CONFIG,DATA,STATE,CACHE}_HOME` under a private temp directory, so the captive editor cannot read the user's config or state.
- **stderr is captured, not discarded.** It is piped and drained to the log by a background `BufReader::lines()` task. Redirecting it to `/dev/null` would hide a crashing nvim's diagnostics, and not draining it would deadlock on a full pipe buffer.
- **Swap files stay enabled.** The launch deliberately does not pass `-n`, which would zero `updatecount` and disable swap. Swap files are what recover unsaved edits after a crash restart.

## The bundle is Wolf's; `state_dir()` is the user's

The embedded runtime — nvim, `init.lua`, the vendored plugins and parsers — is Wolf's and is replaced wholesale on upgrade; nothing under it is user-writeable. Everything under `state_dir()` is the user's, and Wolf never modifies it, with one exception: `seed_runtime_user_dir` writes the `init_user.lua` template into a missing or empty `runtime_user/`, and never again once the directory has anything in it — even when the template file itself is absent, because a non-empty directory is the user's and a deleted template may be a decision.

Core resolves the durable directories (`swap`, `spell`, `runtime_user`, `parsers_user`) and hands them to `init.lua` as `WOLF_*` environment variables, so `init.lua` needs no platform knowledge. Seeding happens in `WolfCore::start`, not in `spawn`, so the crash-respawn path carries no new failure surface; a seed failure warns and startup continues.

## Liveness and restart

Core keeps a lock-free `Arc<AtomicU8>` liveness cell (`Alive` / `NotResponding` / `Dead`) alongside a clone of the redraw wake, mutated only through [`liveness`](src/README.md)'s transition helpers so each real change fires the wake exactly once.

A supervisor task owns the io-loop join handle and marks the process `Dead` on stdout EOF, whether clean or a crash — unless the per-process `shutting_down` flag is set, so an intentional shutdown does not report a spurious crash.

Stall detection lives at the `block_on_rpc` chokepoint. A timeout escalates `Alive → NotResponding` and a successful RPC clears it, both by `compare_exchange` so neither can downgrade `Dead`. A crashed core stays `Dead` until `restart_nvim`, which force-kills and reaps the dead child, respawns in the same working directory and config, and re-runs the full setup chain.

The front-end reads liveness on each wake to drive its crash and stall overlay.

## Threading and locking

- Core owns a multi-thread tokio runtime on its own threads; the `nvim-rs` handler receives `redraw` notifications there.
- A single `parking_lot::RwLock` guards the snapshot. The redraw handler takes the write lock once per whole batch, never holds it across an `.await`, and fires the wake only after the lock is dropped, on `flush`.
- That write lock is acquired cooperatively (a `try_write` retry loop that yields to the scheduler), so a front-end holding a read guard cannot pin a worker thread.
- Input is delivered through an ordered FIFO channel to a single consumer task, which fuses a run of consecutive `Key` events into one `nvim_input` call. This keeps keystrokes in order and makes a burst produce a single redraw flush.

## The snapshot-and-wake session pattern

The file picker, the content search, and the redraw stream all share one shape, and a new streaming capability should adopt it rather than inventing another: a worker streams results into shared state in batches, bumps a version, and fires the same wake callback the front-end already marshals onto its UI thread. The front-end then reads through an owned guard. No callback ever crosses the FFI boundary mid-stream.

Both search sessions resolve their root from the tracked live working directory cell rather than an RPC, so opening a picker never blocks on a wedged nvim. Dropping or cancelling a session stops its traversal, and for content search kills the `rg` subprocess, so no zombie lingers.

## The file-search rescan: nvim events, walk-then-swap

A live `FileSearch` session rescans on the `wolf_fs_changed` notification rather than on a filesystem watcher: a watcher crate (`notify`) would add a dependency, Bazel lockfile churn, and a watcher thread — see the external-edit limitation below. The `SearchRegistry` is the seam such a watcher plugs into without touching the sessions, and it supersedes both the refocus-edge dependency and the per-refocus walk cost with targeted invalidation.

The triggers are two classes. Path-carrying buffer events (`BufWritePost` conditional, `BufFilePost`/`BufDelete` force) cover changes nvim makes itself; path-less refocus-edge force events (`ShellCmdPost`/`FocusGained`/`TermLeave`) catch mutations that fire no buffer event — a `:!git mv`, an external terminal or Finder while the app is unfocused, a command run in the drawer's shell. The edges are human-paced, and the debounce plus the equality skip bound each to at most one no-op walk. `TermLeave` is chosen over `WinLeave` or `TermClose` because the drawer auto-`startinsert`s on `WinEnter`, so every drawer-exit path passes a terminal-to-normal mode change and the one unguarded event covers them all.

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

What the markers cost the stylesheet is in [`src/markdown/README.md`](src/markdown/README.md); the granularity that remains out of reach is in [`architecture.md`](../../architecture.md).

## The rebuild-layout wire format

`state::rebuild_layout_spec` renders a saved `PaneNode` into the JSON `_G.wolf_rebuild_layout` decodes, and the macOS front-end renders the same document independently in Swift as `PaneLayoutCodec.specJSON`. Two renderers, one decoder: the shared `runtime/nvim/init.lua` is all either of them talks to, and neither reads the other, so a change to the shape has to land on both sides or one front-end silently restores a wrong layout. The shape is documented once, in [`runtime/nvim/README.md`](../../runtime/nvim/README.md).

The wire document is deliberately not `PaneNode`'s serde shape, so nothing can be shared by serializing the type directly. A leaf path is arbitrary filesystem text, and one unescaped quote makes the Lua side's `pcall(vim.json.decode, …)` fail and abort the whole restore with no window handles at all — which is why neither side hand-rolls the JSON, and why the Rust renderer lives beside the type rather than once per front-end.

Leaf order is the other half of the contract. Children are emitted depth-first, the order `WolfCore::rebuild_layout` returns nvim window handles in, so a front-end zips those handles onto its own depth-first leaf list. Reordering either side transposes every restored pane while every count-based assertion stays green.

## Present limitations

- A browser-preview server spawned before a `restart_nvim` keeps the RPC handle it captured at spawn time, so its live render goes blank after a restart until the command is run again.
- Content search skips a non-UTF-8 path or line, because `rg --json` sends those as a `bytes` payload and decoding it would add a base64 dependency.
- Linked Markdown pages in the browser preview are static snapshots rendered from disk. The tick endpoint tracks only the pinned buffer, so only that one page live-reloads.
- The Tier-1 open-file mirror reports the instance-global buflisted file set per tabpage rather than a genuinely per-tabpage set, which over-reopens on restore. The active path is genuinely per-tabpage.
- A file created, renamed, or deleted by an external process is invisible to a live file-search session until a refocus edge or another trigger fires; there is no immediate watcher-driven update, and GNOME forwards no OS focus, so `FocusGained` never fires there.

## Deferred

Full `wrapNeovim` plugin baking (`marksman` LSP, formatter), and the non-layout intents (open and save dialogs, window title).
