# wolf-core

The platform-agnostic Rust core of Wolf, a Markdown editor that embeds Neovim as
its editing engine. Core contains everything Neovim-related and draws no pixels:
it spawns a hermetic, pinned `nvim --embed`, attaches as an `ext_linegrid` UI,
folds the entire `redraw` event stream into a render-state snapshot, accepts
abstract input events, and signals the front-end (via a single wake callback)
when a new frame is ready.

See the workspace docs for context: [`architecture.md`](../../architecture.md)
(esp. §8 and the core↔front-end contract) and [`build.md`](../../build.md).

## The one hard rule

> **`wolf-core` must compile with no GUI toolkit in its dependency tree.**

Enforced by a `cargo tree` denylist check:

```sh
cargo tree -e no-dev -p wolf-core
```

No occurrence of any of these is permitted:

```
gtk gtk4 glib gdk4 cairo cairo-rs pango libadwaita
objc objc2 cocoa core-graphics winit wgpu skia-safe softbuffer
```

## Public API surface

Re-exported from the crate root (`wolf_core::…`):

### Lifecycle — `WolfCore` (from [`api`](src/api.rs))

- `WolfCore::start(cfg: CoreConfig, wake: WakeFn) -> Result<WolfCore, CoreError>`
  — builds a multi-thread tokio runtime, spawns the captive Neovim, wires `wake`
  into the redraw handler, and attaches as an `ext_linegrid` UI.
- `read_snapshot(&self) -> SnapshotReadGuard` — owned read guard over the current
  render-state `Snapshot` (the front-end holds it for a paint, then drops it).
  `Deref`s to `Snapshot`; its lifetime is independent of `&self`.
- `send_input(&self, event: InputEvent)` — non-blocking; enqueues the event on an
  ordered FIFO channel applied by a single consumer task. Errors are logged, not
  returned (FFI-shaped fire-and-forget).
- `resize(&self, cols: u16, rows: u16)` — enqueues an `InputEvent::Resize` on the
  same ordered channel, so it never races pending keystrokes.
- `shutdown(self) -> Result<(), CoreError>` — cleanly quits the captive Neovim
  and tears down the runtime.

`CoreConfig { init_lua: PathBuf, cols: u16, rows: u16 }`. `CoreError` is
`Runtime(io::Error)` or `Nvim(NvimError)`.

### Wake callback — `WakeFn` (from [`redraw`](src/redraw.rs))

`type WakeFn = Arc<dyn Fn() + Send + Sync>`. Invoked once per `flush`, after the
snapshot lock is dropped. May run on a tokio worker thread, so the front-end is
responsible for marshalling onto its UI thread.

### Input — the `InputEvent` family (from [`input`](src/input.rs))

`InputEvent` = `Key(KeyEvent) | Mouse(MouseEvent) | Scroll(ScrollEvent) |
Resize { cols, rows } | Open { path } | Save | SaveAs { path }`. Supporting
types: `KeyEvent`, `Key`, `MouseEvent`, `MouseButton`, `MouseAction`,
`ScrollEvent`, `ScrollDirection`.

`key_to_notation(&KeyEvent) -> String` translates a key event into Neovim
notation (`<CR>`, `<Esc>`, `<C-w>`, …). Modifier order is `C-`, `S-`, `A-`, `D-`.
**Shift contract:** for `Key::Char`, deliver the already-shifted glyph in the
char and leave `shift = false`; `S-` is emitted only for named keys.

### Render state (from [`render`](src/render.rs))

`Snapshot` (grid + highlights + cursor + mode_infos + viewport + seq) plus
`Grid`, `Cell`, `HighlightTable`, `HlAttr`, `ResolvedStyle`, `DefaultColors`,
`Rgb`, `Cursor`, `CursorShape`, `ModeInfo`, `Viewport`, and the lock-guarded
`SharedSnapshot`. See [`src/README.md`](src/README.md) for per-type detail.

## How core embeds Neovim

- **Generic untyped RPC.** All calls go through `Neovim::call` with `rmpv::Value`
  arguments, and the `redraw` stream is consumed as raw `Vec<rmpv::Value>`. This
  insulates core from `nvim-rs` 0.9.2's older typed API vs. Neovim 0.12.
- **`ext_linegrid` only.** `nvim_ui_attach` is called with `ext_linegrid = true`
  and `rgb = true`; no other UI extensions in Phase 1.
- **`WOLF_NVIM_BIN` discovery.** The binary is located *only* via the
  `WOLF_NVIM_BIN` env var (exported by the flake devShell), never `PATH` and
  never `/nix/store` scanning.
- **Hermetic launch.** `nvim --embed --clean -u runtime/nvim/init.lua -i NONE -n`
  with `NVIM_APPNAME=wolf` and isolated `XDG_{CONFIG,DATA,STATE,CACHE}_HOME` under
  a private temp dir, so the captive editor cannot read the user's config/state.

## Threading and locking model

- Core owns a **multi-thread tokio runtime on its own thread(s)**; the `nvim-rs`
  handler receives `redraw` notifications there.
- A **single `parking_lot::RwLock`** guards the snapshot. The redraw handler takes
  the write lock **once per whole batch**, never holds it across an `.await`, and
  fires the wake callback only **after** the lock is dropped (on `flush`).
- The write lock is acquired cooperatively (`write_async`, a `try_write` retry
  loop yielding to the scheduler) so a front-end holding a read guard cannot pin a
  worker thread.
- Input is delivered through an **ordered FIFO channel** to a single consumer
  task, which fuses a run of consecutive `Key` events into one `nvim_input` call.
  This keeps keystrokes in order and makes a burst produce a single redraw flush.

## Running the example and tests

```sh
cargo run -p wolf-core --example grid_dump   # headless smoke dump
cargo test -p wolf-core                       # spawns real, pinned nvim
```

Both require an active flake devShell (`direnv allow`) so `WOLF_NVIM_BIN` is set.
See [`tests/README.md`](tests/README.md).

## Phase 1 scope and what is deferred

In scope: hermetic pinned Neovim, `ext_linegrid` redraw → snapshot, abstract
input translation, the FFI-shaped surface, and headless verification.

Deferred to later phases: full `wrapNeovim` plugin baking (treesitter grammars,
`marksman` LSP, formatter), the GTK (`wolf-gnome`) and macOS (`wolf-macos`)
front-ends, swift-bridge bindings, native cmdline/messages/popupmenu, theming,
and intents.
