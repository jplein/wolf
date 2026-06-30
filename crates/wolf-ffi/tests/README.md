# wolf-ffi/tests

Integration tests for the `wolf-ffi` bridge surface. They drive the **Rust half** of the swift-bridge boundary directly — no Swift, no AppKit, no window — exactly as the generated Swift glue eventually will. There are three test binaries plus a shared `common/` helper.

## `frame_pack.rs`

Headless, deterministic coverage of `FfiFrame`'s snapshot → frame packing. It builds a `wolf_core::Snapshot` **by hand** (no nvim, no `WolfCore`) and asserts exactly what `FfiFrame::from_snapshot` produces: explicit-highlight color packing to `0x00RRGGBB` with `set = true`; unset colors packing to `set = false, rgb = 0`; the style-flag bitset (bold/italic/underline/undercurl/strikethrough) packing to exact bits; that `reverse` is applied **once** by `HighlightTable::resolve` and not re-swapped by the bridge; cursor shape → `u8`; `cursor_visible` true only on the active grid; the reverse-video cursor `fill`/`glyph` (plain inversion, and the mode-cursor `attr_id` override when distinct); the default fg/bg; and that `cells()` is a dense row-major `cols * rows` grid. It needs no `WOLF_NVIM_BIN` and runs **regardless** — real, non-mock coverage of the resolution/packing logic that otherwise only runs when a live nvim is present.

## `bridge_smoke.rs`

The real-nvim smoke test. It drives a hermetic, pinned Neovim through the bridge — **no mocks**, per the project testing policy — constructing `FfiKeyEvent`s by hand and calling `wolf_ffi_start` → `read_frame` (dims/cursor sane, `cells()` == `cols * rows`) → `send_key` (`i` then `x`, polling `read_frame` until the typed `x` appears) → `resize` (polling until the dims update) → `shutdown`. Because the `FfiFrame` surface exposes no `seq`, it **polls** `read_frame` on a timeout rather than keying on a flush counter (the same pattern wolf-core's `current_file` / multigrid tests use).

It runs for real on Linux/Cargo devShells and **passes under `bazel test` on the macOS Bazel host** too: the `bridge_smoke` target stages the captive `init.lua` and the pinned Neovim tree (`//bazel/nvim:nvim_tree`) as runfiles (via `data` + the `$(rootpath)` env vars `WOLF_TEST_INIT_LUA` / `WOLF_TEST_NVIM_DIR`), so a real nvim is always available to drive hermetically; an explicit `WOLF_NVIM_BIN` still wins. Because spawning a real nvim child does not work under the macOS build sandbox, the test runs `tags = ["local"]` (unsandboxed). Unlike the wolf-core suite (which panics when no nvim is available), this test **loudly SKIPs** (a no-op pass with a stderr banner) only when NEITHER an explicit `WOLF_NVIM_BIN` NOR the staged tree is available. It never calls the Swift wake — the Swift-free `extern "C"` wake path means `wolf_ffi_start` links and runs with no Swift runtime present.

## `file_search.rs`

The real-nvim coverage of the file-picker surface — **no mocks**, modeled on `bridge_smoke.rs` (same `common/` helpers, same `data` / `$(rootpath)` env runfile staging, same `tags = ["local"]` unsandboxed run, same loud-SKIP gate via `nvim_bin_or_skip`). It seeds a temp cwd with a couple of `.md` files, calls `wolf_ffi_start` there, opens a session with `start_file_search()`, and **polls** `ranked("", N)` until the background traversal streams the created files in; it then asserts the displays are present, that `path()` is absolute (under the cwd) while `display()` is the relative name, and that a focused query (e.g. `ranked("alpha")`) ranks the matching file first. Finally it issues `open(path())` and confirms the core survives the round-trip (it cannot assert the exact opened buffer name — `FfiFrame` exposes no `current_file` accessor, and the test does not add one — so `open` is checked at the round-trip-survives level, `ranked` correctness strongly).

## `common/mod.rs`

Shared helpers mirroring `crates/wolf-core/tests/common/mod.rs`, with the one deliberate policy difference (skip, not panic, when nvim is absent):

- `nvim_bin_or_skip()` — the skip gate: returns the nvim binary path, preferring an explicit `WOLF_NVIM_BIN`, else the binary inside the staged pinned tree (`WOLF_TEST_NVIM_DIR`, the `//bazel/nvim:nvim_tree` runfiles); returns `None` after printing a loud stderr banner only when neither is available (the caller then `return`s — a no-op pass).
- `init_lua_path()` — resolves the captive `runtime/nvim/init.lua`, preferring the staged runfiles copy (`WOLF_TEST_INIT_LUA`) and falling back to the in-repo path under `CARGO_MANIFEST_DIR` (`crates/wolf-ffi`) for the cargo/devShell path. Using the staged runfiles under Bazel is what fixed the prior hang — `CARGO_MANIFEST_DIR` points at an ephemeral sandbox path that does not exist at test runtime, so nvim could not read its init and blocked on the "Press ENTER" prompt forever.
- `fresh_cwd()` — a fresh, unique, **non-auto-deleted** temp dir for a core's `cwd` (the MRU cache under `<cwd>/.wolf/` must outlive `wolf_ffi_start`).
- `DEFAULT_COLS` / `DEFAULT_ROWS` (80 × 24) — the smoke test's starting grid size.
