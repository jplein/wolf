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

## Browser preview security

Two hard requirements in the `browser_preview` server, both of which look removable and are not:

- The listener binds `127.0.0.1:0`, loopback only. Never `0.0.0.0`.
- Every request path is checked against the canonical root, and anything escaping it is a 404. The bind being loopback-only is not a reason to drop the traversal guard.

Each connection clones the current target under a short lock dropped before any `.await`, so no lock is held across an await. The `browser_preview` lock is never held across an nvim RPC.

## Present limitations

- A browser-preview server spawned before a `restart_nvim` keeps the RPC handle it captured at spawn time, so its live render goes blank after a restart until the command is run again.
- Content search skips a non-UTF-8 path or line, because `rg --json` sends those as a `bytes` payload and decoding it would add a base64 dependency.
- Linked Markdown pages in the browser preview are static snapshots rendered from disk. The tick endpoint tracks only the pinned buffer, so only that one page live-reloads.
- The Tier-1 open-file mirror reports the instance-global buflisted file set per tabpage rather than a genuinely per-tabpage set, which over-reopens on restore. The active path is genuinely per-tabpage.

## Deferred

Full `wrapNeovim` plugin baking (treesitter grammars, `marksman` LSP, formatter), and the non-layout intents (open and save dialogs, window title).
