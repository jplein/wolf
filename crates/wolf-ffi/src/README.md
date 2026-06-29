# wolf-ffi/src

The `wolf-ffi` source. Two files: `lib.rs` (the swift-bridge module + the
hand-written Rust glue behind it) and `bin/gen_bindings.rs` (the committed-binding
codegen). See the crate [`README.md`](../README.md) for the surface overview and
the build/regen workflow.

## `lib.rs`

The crate root: the single `#[swift_bridge::bridge] mod ffi` plus the glue that
backs it. swift-bridge is confined here, so `wolf-core` stays GUI-free and gains
no new dependency.

- **The bridge module (`mod ffi`).** Declares the transparent structs `FfiColor`
  (`{ rgb: u32, set: bool }` — a packed `0x00RRGGBB` plus an "is set" flag) and
  `FfiKeyEvent` (`{ kind, ch, fnum, ctrl, shift, alt, meta }`, Swift fills it from
  an `NSEvent`), and the opaque `extern "Rust"` types `WolfCoreHandle`, `FfiFrame`,
  and `FfiCell` with their methods. `FfiColor` / `FfiKeyEvent` are re-exported at
  the crate root (`pub use ffi::{FfiColor, FfiKeyEvent}`) so the rlib integration
  tests can name them. A scoped `#[allow(clippy::all, missing_docs, unused)]` keeps
  the macro expansion off this crate's stricter lint bar (with a crate-level
  `#![allow(clippy::unnecessary_cast)]` for the same-type pointer casts the
  expansion emits in sibling code).

- **`WolfCoreHandle`** — the opaque, **main-thread-only** handle wrapping a
  `RefCell<Option<WolfCore>>` (`WolfCore` is not `Send`). Methods: `read_frame`
  (reads the snapshot and packs an `FfiFrame`), `send_key` (maps the event and
  forwards it), `resize`, and `shutdown` (takes the core out via `.take()` so a
  second call is a no-op, then `WolfCore::shutdown`; any error is logged, not
  returned — the bridge is infallible). `wolf_ffi_start(init_lua, cwd, cols, rows)`
  builds a `CoreConfig` (the two strings are filesystem **paths**), wires the wake,
  calls `WolfCore::start`, and returns `Option<WolfCoreHandle>` (`None` after
  logging on error).

- **`FfiFrame` + `FfiCell`** — the opaque per-frame snapshot of the **active
  grid**. `FfiFrame::from_snapshot(&wolf_core::Snapshot)` (public, so the headless
  `frame_pack` test can drive it without a live `WolfCore`) resolves every cell
  against the snapshot's `HighlightTable` **in Rust** — reusing
  `HighlightTable::resolve` / `ResolvedStyle`, never re-resolving in Swift — into a
  flat row-major `Vec<FfiCell>`, computes the cursor's reverse-video fill/glyph
  colors (`cursor_colors`, ported from wolf-gnome's `cursor.rs`), and sets
  `cursor_visible = (cursor.grid == active_grid_id)`. Scalar accessors expose the
  dims, cursor position/shape/percentage, and the default + cursor colors.
  `FfiCell` is opaque (swift-bridge does not emit `Vec` support for transparent
  structs) with `text()` / `fg()` / `bg()` / `flags()`; its `text` field is `pub`
  so the Rust tests read it directly.

- **The wake (Swift-free).** `#[no_mangle] pub extern "C" fn wolf_ffi_set_wake`
  registers (or clears) the front-end wake callback in a global `AtomicPtr`;
  `registered_wake()` builds the `WakeFn` core fires on flush, which loads and
  calls the registered pointer (a no-op if none is registered). Declared via a
  hand-written C header (`generated/wolf_ffi_wake.h`) rather than an `extern
"Swift"` symbol, so the rlib links with no Swift present. The callback captures
  nothing and touches no `WolfCore` state, so it is `Send + Sync` and the only
  thing that crosses threads.

- **Helpers.** `key_event_from_ffi` maps an `FfiKeyEvent` into a
  `wolf_core::KeyEvent` (the `kind` discriminants — `mod kind`, `CHAR = 0` … `F =
14` — mirror `wolf_core::Key`'s variant order and are the single source of truth
  shared with the Swift `WolfKeyMap` and the test suites); `pack_flags` packs the
  style bitset (`mod flag`); `pack_cursor_shape` maps `CursorShape` to its `u8`
  (`mod cursor_shape`); `FfiColor::from_rgb` / `from_opt` do the `0x00RRGGBB`
  packing (with `FALLBACK_FG` white / `FALLBACK_BG` black mirroring wolf-gnome's
  `render`).

## `bin/gen_bindings.rs`

The standalone codegen binary (`//crates/wolf-ffi:gen_bindings`, run via `bazel
run`). It invokes `swift-bridge-build`'s parser over `lib.rs` and writes the Swift

- C output into `$BUILD_WORKSPACE_DIRECTORY/crates/wolf-ffi/generated/`, prepending
  the `import WolfFFI` line the Swift glue needs. It writes **in-tree** (only `bazel
run` sets `BUILD_WORKSPACE_DIRECTORY` and may write into the source tree — a
  genrule cannot), so the generated files stay **committed** and are refreshed
  explicitly after a bridge change.
