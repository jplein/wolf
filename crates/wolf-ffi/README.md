# wolf-ffi

The swift-bridge FFI surface over [`wolf-core`](../wolf-core). This crate is the seam between the platform-agnostic Rust core and the macOS (AppKit) front-end in [`wolf-macos`](../wolf-macos). It holds the single `#[swift_bridge::bridge]` module and the hand-written Rust glue behind it, and is built as both a `staticlib` (the archive the `.app` links through the generated Swift) and an `rlib` (so the Rust integration test can link the same surface headlessly).

swift-bridge is confined to this crate. `wolf-core` gains no new dependency and no GUI-toolkit dependency; the Linux/GTK front-end keeps consuming `wolf-core` directly.

## Bridge surface

All types are re-exported at the crate root (`wolf_ffi::…`).

- `wolf_ffi_start(init_lua: String, cwd: String, cols: u16, rows: u16) -> Option<WolfCoreHandle>` — starts the core against the pinned Neovim. `init_lua` and `cwd` are filesystem **paths** (mirroring `CoreConfig`). Returns `None` (after logging) on error.

- `WolfCoreHandle` — an opaque, **main-thread-only** handle wrapping a `RefCell<Option<WolfCore>>` (`WolfCore` is not `Send`). Methods:
  - `read_frame(&self) -> FfiFrame`
  - `send_key(&self, ev: FfiKeyEvent)`
  - `resize(&self, cols: u16, rows: u16)`
  - `start_file_search(&self) -> FfiFileSearch` — starts a streaming file-search session over the cwd (registering the picker wake) and wraps it as an `FfiFileSearch`. Infallible across the bridge: when no core is present the result simply yields no entries.
  - `open(&self, path: String)` — issues `InputEvent::Open { path }` to open the file; the MRU is recorded by core's input pump.
  - `shutdown(&self)` — takes the core out and shuts it down (idempotent; infallible across the bridge — any error is logged).

- `FfiFileSearch` — an opaque handle wrapping core's `FileSearch` session (`Option`, so `start_file_search` stays infallible). `ranked(&self, query: String, max: usize) -> Vec<FfiFileEntry>` runs the ranking **in Rust** (`wolf_core::rank_matches` over the current results snapshot) and returns the top-`max` entries; it yields an empty `Vec` when the session is absent. Dropping the handle cancels the background traversal.

- `FfiFileEntry` — opaque; `display() -> String` (the relative display name) and `path() -> String` (the absolute path). Opaque because swift-bridge only emits `Vec<T>` support for opaque `T` — the same reason `FfiCell` is opaque.

- `FfiFrame` — an opaque, owned, paint-ready snapshot of the **active grid**, with every cell already resolved against the highlight table **in Rust** (reusing `HighlightTable::resolve` / `ResolvedStyle` — never re-resolved in Swift). Accessors: `cols`, `rows`, `cursor_row`, `cursor_col`, `cursor_shape` (0 = Block, 1 = Horizontal, 2 = Vertical), `cursor_cell_percentage`, `cursor_visible`, `default_fg`, `default_bg`, `cursor_fill`, `cursor_glyph` (reverse-video cursor colors, ported from wolf-gnome's `cursor.rs`), and `cells() -> Vec<FfiCell>` (one flat, row-major handoff per frame).

- `FfiCell` — opaque; `text()`, `fg()`, `bg()`, `flags()`. (Opaque because swift-bridge does not emit `Vec` support for transparent structs.) Its `text` field is `pub` so the Rust integration test reads it directly.

- `FfiColor` — transparent `{ rgb: u32, set: bool }` (a packed `0x00RRGGBB` plus an "is set" flag; `set == false` means a front-end fallback applies).

- `FfiKeyEvent` — transparent `{ kind: u8, ch: u32, fnum: u8, ctrl, shift, alt, meta: bool }`. Swift fills it from an `NSEvent`; Rust maps it into a `wolf_core::KeyEvent` (so nvim notation stays the single source of truth in core). `kind` mirrors the variant order of `wolf_core::input::Key`:

  | kind | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 |
  | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
  | key | Char | Enter | Esc | Tab | Backspace | Delete | Left | Right | Up | Down | Home | End | PageUp | PageDown | F(fnum) |

  These discriminants are the single source of truth shared with the Swift `WolfKeyMap` and the test suites.

## The wake (Swift-free link path)

`wolf-core` fires a `WakeFn` on each `flush`; the front-end must marshal it onto the AppKit main thread and trigger a repaint. Rather than an `extern "Swift"` symbol — which a Rust-only integration test could never resolve at link time — the wake is a globally **registered** `extern "C"` function pointer:

```c
void wolf_ffi_set_wake(void (*wake)(void)); // generated/wolf_ffi_wake.h
```

Swift's `WakeBridge` registers a `DispatchQueue.main.async { … setNeedsDisplay }` thunk once at startup. The Rust integration test never registers, so the wake is a harmless no-op there and `wolf_ffi_start` links and runs with no Swift runtime present. The wake callback touches no `WolfCore` state, so the only thing that crosses threads is stateless.

There is a **second**, parallel registered wake for the file picker:

```c
void wolf_ffi_set_filesearch_wake(void (*wake)(void)); // generated/wolf_ffi_wake.h
```

It is a separate global `extern "C"` callback (its own `registered_filesearch_wake`), fired after each streaming traversal batch of a `start_file_search` session, so the picker can refresh its results independently of editor repaints (Swift's `PickerWakeBridge` registers a dispatch-to-main `PickerView.refresh()` thunk). It is otherwise identical in shape to the editor wake — stateless, no-op when unregistered.

## Generated bindings

The committed `generated/` tree is the swift-bridge output plus two hand-written files:

- `SwiftBridgeCore.swift` / `SwiftBridgeCore.h` — the shared swift-bridge runtime.
- `wolf-ffi.swift` / `wolf-ffi.h` — this crate's bridge.
- `wolf_ffi_wake.h` — the hand-written wake-registration declaration.
- `module.modulemap` — exposes the C headers to Swift as the `WolfFFI` module (matched by the `import WolfFFI` line `gen_bindings` prepends to the generated Swift, and by the `swift_interop_hint` in `BUILD.bazel`).

Regenerate after changing the bridge:

```sh
bazel run //crates/wolf-ffi:gen_bindings   # rewrites generated/, then commit
```

`gen_bindings` writes in-tree (it reads `BUILD_WORKSPACE_DIRECTORY`, which only `bazel run` provides — a genrule cannot write into the source tree). After changing Cargo dependencies, repin:

```sh
CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-ffi:wolf_ffi   # then commit lockfiles
```

## Bazel targets

- `:wolf_ffi` — the `rlib` (used by the Rust integration test).
- `:wolf_ffi_staticlib` — `libwolf_ffi.a` (linked into the app).
- `:wolf_ffi_cc` — a `cc_library` exposing the generated headers + the wake header as the `WolfFFI` Clang module and linking the archive; consumed by the Swift targets.
- `:generated_swift` — a filegroup of the generated Swift glue, compiled into `wolf-macos`'s `wolf_key_map` module.
- `:gen_bindings` — the codegen binary (run via `bazel run`).
- `:bridge_smoke` — the Rust integration test for the editor round-trip.
- `:file_search` — the Rust integration test for the file-search surface (real pinned nvim, modeled on `bridge_smoke`).

## Testing

`tests/bridge_smoke.rs` drives the Rust half against a **real** pinned Neovim (no mocks), exercising the full live round-trip (start → type → the grid reflects the typed char → resize → shutdown). It runs for real on Linux/Cargo devShells **and passes under `bazel test` on the macOS Bazel host**: the `bridge_smoke` `rust_test` stages the captive `init.lua` and the pinned Neovim tree (`//bazel/nvim:nvim_tree`) as runfiles (`data` + `$(rootpath)` env vars `WOLF_TEST_INIT_LUA` / `WOLF_TEST_NVIM_DIR`, resolved by `common::{init_lua_path, nvim_bin_or_skip}`), so it has a real nvim to drive hermetically on every run; an explicit `WOLF_NVIM_BIN` still wins for the cargo/devShell path. It runs `tags = ["local"]` (unsandboxed) because spawning a real nvim child does not work under the macOS build sandbox. The test only **loudly skips** if NEITHER an explicit `WOLF_NVIM_BIN` NOR the staged tree is available:

```sh
bazel test //crates/wolf-ffi:bridge_smoke
```

`tests/file_search.rs` covers the file-search surface against the same real pinned Neovim (modeled on `bridge_smoke`, reusing `tests/common/mod.rs` and the same `data` / `env` / `no-sandbox` runfile staging): it seeds a temp cwd with a couple of files, calls `start_file_search()`, polls `ranked(query, max)` until the traversal streams them in, and asserts the displays are present (`path()` absolute, `display()` relative, ranking honored) and that `open(path())` round-trips without panicking.

```sh
bazel test //crates/wolf-ffi:file_search
```
