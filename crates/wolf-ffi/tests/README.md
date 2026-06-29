# wolf-ffi/tests

Integration tests for the `wolf-ffi` bridge surface. They drive the **Rust half**
of the swift-bridge boundary directly — no Swift, no AppKit, no window — exactly as
the generated Swift glue eventually will. There are two test binaries plus a shared
`common/` helper.

## `frame_pack.rs`

Headless, deterministic coverage of `FfiFrame`'s snapshot → frame packing. It
builds a `wolf_core::Snapshot` **by hand** (no nvim, no `WolfCore`) and asserts
exactly what `FfiFrame::from_snapshot` produces: explicit-highlight color packing
to `0x00RRGGBB` with `set = true`; unset colors packing to `set = false, rgb = 0`;
the style-flag bitset (bold/italic/underline/undercurl/strikethrough) packing to
exact bits; that `reverse` is applied **once** by `HighlightTable::resolve` and not
re-swapped by the bridge; cursor shape → `u8`; `cursor_visible` true only on the
active grid; the reverse-video cursor `fill`/`glyph` (plain inversion, and the
mode-cursor `attr_id` override when distinct); the default fg/bg; and that
`cells()` is a dense row-major `cols * rows` grid. It needs no `WOLF_NVIM_BIN` and
runs **regardless** — real, non-mock coverage of the resolution/packing logic that
otherwise only runs when a live nvim is present.

## `bridge_smoke.rs`

The real-nvim smoke test. It drives a hermetic, pinned Neovim through the bridge —
**no mocks**, per the project testing policy — constructing `FfiKeyEvent`s by hand
and calling `wolf_ffi_start` → `read_frame` (dims/cursor sane, `cells()` ==
`cols * rows`) → `send_key` (`i` then `x`, polling `read_frame` until the typed `x`
appears) → `resize` (polling until the dims update) → `shutdown`. Because the
`FfiFrame` surface exposes no `seq`, it **polls** `read_frame` on a timeout rather
than keying on a flush counter (the same pattern wolf-core's `current_file` /
multigrid tests use).

Unlike the wolf-core suite (which panics when `WOLF_NVIM_BIN` is unset), this test
**loudly SKIPs** (a no-op pass with a stderr banner) when `WOLF_NVIM_BIN` is unset
or empty: the macOS Bazel host cannot yet provision `nvim`, so the test must be a
no-op there while still running for real on Linux/Cargo devShells. It never calls
the Swift wake — the Swift-free `extern "C"` wake path means `wolf_ffi_start` links
and runs with no Swift runtime present.

## `common/mod.rs`

Shared helpers mirroring `crates/wolf-core/tests/common/mod.rs`, with the one
deliberate policy difference (skip, not panic, when nvim is absent):

- `nvim_bin_or_skip()` — the skip gate: returns the `WOLF_NVIM_BIN` path, or `None`
  after printing a loud stderr banner (the caller then `return`s — a no-op pass).
- `init_lua_path()` — resolves the in-repo `runtime/nvim/init.lua` from
  `CARGO_MANIFEST_DIR` (`crates/wolf-ffi`).
- `fresh_cwd()` — a fresh, unique, **non-auto-deleted** temp dir for a core's
  `cwd` (the MRU cache under `<cwd>/.wolf/` must outlive `wolf_ffi_start`).
- `DEFAULT_COLS` / `DEFAULT_ROWS` (80 × 24) — the smoke test's starting grid size.
