# wolf-gnome/tests

Tests for the seams and the windowed application. They split into **headless** checks (no display, no nvim), one **real-Neovim** end-to-end round-trip (no mocks, per project policy), and nine **headless-compositor** cage smoke tests that launch the real window (one window-render smoke, two file-picker smokes, one file-tree-sidebar smoke, and five split-pane smokes: vertical split, horizontal split, a three-pane same-orientation split, focus move, and close pane) plus one pure guard on the smokes' own fixture, which needs no compositor. Everything runs through `nix develop --command` so the GTK toolchain, the pinned font, and `WOLF_NVIM_BIN` are present:

```sh
nix develop --command cargo test -p wolf-gnome
```

The default run **does** include all nine cage smokes, and they are the slowest part of the suite — see [the smoke test section](#smoke_headlessrs--windowed-app--picker--tree--panes-real-compositor). They are part of the Linux verification gate; see [`build.md`](../../../build.md), "The Linux verification gate", for the authoritative full check.

## `grid_size_for` unit tests (in `src/geometry.rs`, headless)

The pure pixel-to-grid geometry is unit-tested **inside its own module** (a `#[cfg(test)] mod tests` in [`src/geometry.rs`](../src/geometry.rs)), needing no GTK, display, or nvim. A table-driven `grid_size_for_table` covers the typical exact-division case (`800×592` with `8×16` cells → `100×37`), flooring of partial cells (`17×17` → `2×1`), the min-1 clamp for tiny/zero/negative dims, the whole-result collapse to `(1, 1)` for degenerate cell sizes (zero/negative/NaN/ infinite, including a single-axis NaN), and the upper clamp of `i32::MAX` dims to `u16::MAX`. A second test, `grid_size_for_independent_axes`, proves cols and rows are computed separately (wide-but-short and narrow-but-tall grids).

## `cursor.rs` unit tests (in `src/cursor.rs`, headless)

The pure cursor geometry and color logic is unit-tested **inside its own module** (a `#[cfg(test)] mod tests` in [`src/cursor.rs`](../src/cursor.rs)), needing no GTK, display, or nvim. A table-driven `cursor_rect_table` covers the geometry per shape: `Block` always returns the full cell (ignoring `cell_percentage`); `Vertical` is a left bar of full height whose width is `cell_percentage` of `cell_w` (default `0.25`), floored to `MIN_BAR_PX` for tiny percentages and capped at `cell_w` for out-of-range ones; `Horizontal` is the analogous bottom bar against `cell_h` (anchored to the bottom edge). Deliberately non-square cells and a non-zero origin keep width/height math and offsets distinguishable. `named_constants_match_contract` pins `MIN_BAR_PX` / `DEFAULT_CELL_PCT` / `PCT_DENOM`.

A second group covers `cursor_colors` over hand-built `Snapshot`s: the **honor** path (a mode cursor highlight with distinct fg + bg → `(bg, fg)`, including with editor defaults set); the **inversion** path (bg-only, fg-only, `attr_id` `None`, out-of-range `mode_idx`, and empty `mode_infos` all fall through to `(cell_fg, cell_bg)` without panicking); the **guard** cases where a degenerate honored pair must NOT be honored (fill equal to the underlying cell background, or fill equal to glyph) and instead inverts; and the **fallback** case where unset cell colors reach the white/black `FALLBACK_FG` / `FALLBACK_BG`.

## `picker.rs` unit tests (in `src/picker.rs`, headless)

The picker's pure seams are unit-tested **inside their own module** (a `#[cfg(test)] mod tests` in [`src/picker.rs`](../src/picker.rs)), needing no GTK, display, or nvim — `gdk::Key` constants are compile-time and the keyvals are built with the same `key_from_char` helper used by `gdk_key.rs`:

- `is_picker_trigger_table` — the Ctrl+Shift+P chord is matched case-insensitively on the letter (`'p'` and `'P'`), an extra modifier (ALT) does not disqualify it, and a missing CONTROL/SHIFT or the wrong letter yields `false`.
- `next_selection_table` — down/up within range, clamp at both ends with NO wrap, empty list → `None`, single row, and out-of-range/negative `current` clamped.
- `restored_index_table` — empty list → `None` regardless of reset/prev; reset → `Some(0)`; preserve in range; preserve clamped when `prev` exceeds the new len; preserve with no prev defaults to `0`.

## `tree.rs` projection (a re-export; tests live in `wolf-core`)

[`src/tree.rs`](../src/tree.rs) is a ten-line shim whose entire body is `pub use wolf_core::tree::*;`. It carries no `#[cfg(test)] mod tests` of its own. The pure hierarchical projection — `TreeNode`, `children_of`, `ancestors_to_reveal`, `node_rel_path`, `rel_path_for`, plus the `TreeIndex` / `TreeIndexCache` incremental layer (`TreeIndex::build`, `TreeIndex::children_of`, `TreeIndexCache::index_for`, `TreeIndexCache::build_count`) — and **all of its unit tests** live in `wolf-core`'s `tree` module, so that a single FFI-expressible source of truth is shared by every front-end. The shim exists only so the GTK glue in `tree_view.rs` (and `lib.rs`'s re-exports) keeps compiling against the `crate::tree::…` paths unchanged.

See [`crates/wolf-core/src/README.md`](../../wolf-core/src/README.md) for the projection's contract and its test coverage; do not duplicate that enumeration here.

## `layout.rs` unit tests (in `src/layout.rs`, headless)

The pure **N-ary frame** pane-layout tree (`PaneTree`, matching Neovim's window model) is unit-tested **inside its own module** (a `#[cfg(test)] mod tests` in [`src/layout.rs`](../src/layout.rs)), needing no GTK, display, or nvim — twenty tests covering:

- **construction / split** — `new` is a single leaf; a `Vertical` split yields two leaves side by side and a `Horizontal` split stacks them; `pane_rects` tiles the bounds for both; a perpendicular nested split produces the expected nested rectangles.
- **flatten vs. nest (the N-ary frame model)** — a split whose orientation MATCHES the containing frame FLATTENS into an equal-sized sibling row/column (a row `[P1, P2]` vsplit on `P1` becomes equal thirds `[P1, P3, P2]`, not a nested 25/25/50; repeated same-orientation vsplits stay flat at four equal columns), while a PERPENDICULAR split (an hsplit of a row member) NESTS into a sub-frame.
- **remove / collapse / re-equalize** — removing a leaf collapses its sibling (and a sibling _subtree_) into the parent's slot; removing a flat-row member re-equalizes the survivors (nvim `equalalways`); a now-1-child split collapses into its sole child; a surviving same-orientation split merges upward into the parent; removing the last pane is a no-op.
- **neighbor** — basic Left/Right (`Vertical`) and Up/Down (`Horizontal`) neighbours, a nested 2x2 case, a flat row of three, and `None` for an unknown pane.
- **ordering** — `leaves` and `pane_rects` enumerate in a stable DFS order.

## `gdk_key.rs` — input translation (headless, table-driven)

Table-driven unit tests for `wolf_gnome::gdk_key`, layout-independent: assertions cover only letter casing and named keys, never shifted-punctuation unicode (which is keyboard-layout dependent and would be flaky). These need no display server and no nvim — `gdk::Key` constants are compile-time and printable keyvals are built via the local `key_from_char` helper.

Cases cover: the Enter/Return family (incl. `KP_Enter`), `ISO_Left_Tab` + SHIFT → `Tab { shift: true }`, arrows, `Home`/`End`, `PageUp`/`PageDown`, `F1`/`F5`/`F12`, printable chars with the shift contract (`'a'` and `'A'` both → `Char(..)` with `shift: false`, `'<'`), modifiers on named keys (`Ctrl+Left`, `Ctrl+Shift+Left`), modifiers on printables (`Super+s`, `Alt+x`, `Ctrl+a` carries the base letter + ctrl, never a raw control char), and the `None` cases (`Shift_L`, `Control_L`, `Super_L`, `Caps_Lock`).

## `paint_golden.rs` — paint (headless, offscreen)

Builds a deterministic 4×2 `Snapshot` via core's public constructors (`Snapshot::new`, `HighlightTable::set_defaults`/`define`, `Grid::set_cell`), including one defined `hl_id` with distinct fg/bg and a wide-trailing empty-text cell. It measures `CellMetrics` against a throwaway 1×1 probe surface, sizes a real offscreen `ImageSurface` (ARGB32) to exactly the grid, paints, and samples pixels.

**Primary gate — structural ARGB center-sampling** (always on; deliberately _not_ a whole-image hash, which is too brittle across freetype/fontconfig):

- Known background fills are rasterizer-independent, so the center of the defined-bg empty cell and a default-bg cell are asserted for **exact** equality.
- The empty wide-trailing cell's interior is sampled at several inset points and must be background-only (verifying the "skip the glyph" path).
- The glyph cell's rect must contain at least one non-background pixel (the glyph was actually drawn).

Pixel reads assume little-endian ARGB32 (in-memory byte order `B, G, R, A`), which holds on the x86_64 Linux target.

**Dev aid (not part of the gate)** — `regenerate_png_golden` is `#[ignore]`d; it writes `tests/testdata/paint_golden.png` via `ImageSurface::write_to_png` (which needs the `cairo-rs` `png` feature). Run it explicitly:

```sh
nix develop --command cargo test -p wolf-gnome --test paint_golden \
    -- --ignored regenerate_png_golden
```

It is now the **only** `#[ignore]`d test in the whole repo, so a bare `cargo test -p wolf-gnome -- --ignored` runs nothing but this golden rewrite.

Font determinism relies on the pinned `DejaVu Sans Mono` exposed via `FONTCONFIG_FILE` (set by the flake devShell from [`testdata/fonts.conf`](../testdata/fonts.conf)).

## `roundtrip_nvim.rs` — end-to-end (real Neovim)

Exercises the full input seam against a real, hermetic Neovim (no mocks): `gdk_key` translates GDK keyvals → `InputEvent::Key` → `WolfCore` → nvim, then the snapshot is asserted. It enters insert mode, types `"Hello"` (the leading `H` is fed as the already-uppercased keyval _with_ the SHIFT mask, exercising the shift-glyph contract end-to-end), leaves insert mode with `Escape`, waits for and drains flushes, and asserts row 0 contains the word. Requires `WOLF_NVIM_BIN` (`common::require_nvim_bin` fails loud if unset).

## `smoke_headless.rs` — windowed app + picker + tree + panes (real compositor)

Nine `#[test]` cage smokes that prove the whole stack actually draws a window, drives the file picker, populates the file-tree sidebar, and creates / focuses / closes native split panes. They run in the **default** suite: the compositor and OCR toolchain they need comes from the flake devShell, so a normal `cargo test` run _inside `nix develop`_ does have it, and `require_smoke_env` fails loud if it does not. To run them:

```sh
nix develop --command cargo test -p wolf-gnome --test smoke_headless
```

That target reports **`10 passed`**, not `9`: the file also carries one pure, compositor-free guard, `sidebar_filler_names_cannot_satisfy_any_smoke_assertion` (last bullet below), which does **not** call `require_smoke_env` and needs none of the toolchain.

**Serialization (`CAGE_LOCK`).** Each smoke launches its own `cage` compositor, and two compositors started concurrently contend for the same Wayland socket under `XDG_RUNTIME_DIR` (lockfile error), which would fail both under the default parallel harness. So all nine smokes acquire a shared `static CAGE_LOCK: Mutex<()>` for their whole body, forcing serial execution; poisoning is recovered from (`into_inner`) so one panicking test does not cascade into the others. This serialization is why the smokes are the slowest part of the suite.

**Mechanism (common to all nine).** Each locates the built binary via `CARGO_BIN_EXE_wolf-gnome` and runs its interactive scenario as one `sh -c` string **inside `cage`** (the headless wlroots compositor) so that `wtype` and `grim` inherit cage's `WAYLAND_DISPLAY` (cage exports it only to its child process tree). The scenario launches the app in the background, polls (not a fixed sleep) until the window maps, drives it via `wtype`, captures the live window with `grim` to a PNG under `XDG_RUNTIME_DIR`, then OCRs it on the host with `tesseract … stdout --psm 11` ("sparse text" mode — the default page-segmentation mode returns empty on the sparse light-on-dark editor grid). The app is launched with `WOLF_FONT_PT=28` so glyphs are large enough to OCR, and a stale screenshot is removed first so a previous run cannot make a test pass.

**Every one of the nine launches in a FRESH temp working directory** (`make_temp_cwd()`, passed to the `cage` command as its `current_dir`), not in the repo. That isolation is mandatory, not tidiness. The app derives `CoreConfig.cwd` from `std::env::current_dir()`, and the captive Neovim keeps a **durable, per-project swap directory keyed on that cwd** (`wolf_core::state::swap_dir_for` → `state_dir()/swap/<hash of cwd>`), which deliberately outlives the process so a fresh nvim can recover a crashed one's unsaved edits. Every scenario ends by `kill`ing the app, so its nvim never cleans up. At a **fixed** cwd those two facts compose into a self-poisoning test: run _N_ leaves a stale `%…%Untitled.md.swp` behind and run _N+1_ finds it at startup, where naming the startup buffer `<cwd>/Untitled.md` collides with it inside `nvim_buf_set_name` and raises a blocking ATTENTION dialog that eats the first keystroke — so the smoke goes red for whatever change happens to be in flight, and points at the wrong cause. A fresh cwd makes each run idempotent. (The product-level guard for that swap collision is deliberate, headless and lives in [`crates/wolf-core/tests/swap_recovery.rs`](../../wolf-core/tests/swap_recovery.rs), `stale_untitled_swap_does_not_block_a_fresh_start`.) A side benefit: the app no longer traverses the entire repository through `FileSearch` on launch, and no longer writes into the working tree. That last point has one deliberate exception, `window_renders_typed_text_under_cage`, which writes nine tiny marker `.md` files into its own temp cwd purely so the tree sidebar has rows of text to draw — the repo listing used to supply that page density accidentally, and OCR has always depended on it (see that test's bullet below for the measurement). The temp dirs are **not** auto-deleted, to aid post-mortem inspection; each run therefore also leaves one new `state_dir()/swap/<hash>/` holding at most one `.swp`, which is harmless because no future run resolves to that hash.

- **`window_renders_typed_text_under_cage`** — types `ihello` (insert mode + the word) and `Escape`, then moves the cursor off row 0 (a normal-mode `o` + `Escape` parks the block cursor on an empty row 1 so it does not OCR-distort the asserted word), and asserts the OCR loosely `contains("hello")`. Before launching, and unlike the other eight, it writes `SIDEBAR_FILLER_COUNT` (nine) tiny marker `.md` files (`SIDEBAR_FILLER_FILES`) into its temp cwd. Those are **fixture, not assertion** — nothing in the test looks at them — and they exist because `tesseract` classifies a word by the whole page, not just its own pixels: on an otherwise-empty page the isolated leading `h` is read as the ligature `ﬁ`, so the OCR comes back `ﬁello` even though the editor drew `hello` correctly (confirmed pixel by pixel, and reproduced by cropping a _passing_ repo-cwd screenshot down to the editor region). The measured floor on this host is 5 files; 3 and fewer fail, so nine leaves margin. The other eight smokes get the same density incidentally from the fixture files their scenarios drop, and before the temp cwd landed this test got it accidentally from the ~35-entry repo listing — the fillers make a dependency the assertion always had explicit and deliberate. If it ever regresses, add filler files; do not touch the assertion. Their names are also kept clear of every smoke's OCR constant, so a sidebar row can never satisfy an assertion on its own — enforced mechanically by `sidebar_filler_names_cannot_satisfy_any_smoke_assertion` (last bullet), not by comment. Its failure message is worth reading in full before diagnosing: an OCR miss where the window, sidebar, tildes and cursor all drew but row 0 is empty and the cursor sits two rows down means the leading `i` was consumed by the swap modal above, not that rendering failed.
- **`picker_opens_and_loads_selected_file_under_cage`** — drops a known `pickertarget.md` (heading `# PICKERTARGET`) into a fresh temp dir launched as the app's working directory; opens the picker with **Ctrl+Shift+P** (`wtype -M ctrl -M shift -k p -m shift -m ctrl`), types the query `picker`, presses `Return`, and asserts the OCR contains `pickertarget` — proving the picker opened, matched, selected, and nvim `:e`'d the file.
- **`picker_arrow_navigation_selects_second_match_under_cage`** — drops two matching files (`pick1.md` → row 0, `pick2.md` → row 1 by the rank tiebreaks, neither in the MRU) into the temp cwd; opens the picker, types `pick`, presses `Down` once to move the **list** selection to row 1, then `Return`, and asserts the OCR contains `picktwo` (row 1's heading) and does NOT contain `pickone` (row 0's) — proving `Down` moved the selection off the top row and `Enter` opened the selected row.
- **`tree_sidebar_shows_files_under_cage`** — drops a distinctively-named `treenode.md` into a fresh temp cwd launched as the app's working directory, then (with **no keyboard interaction**) waits an extra settle for the long-lived `FileSearch` to stream the cwd listing into the sidebar, screenshots, and asserts the OCR contains `treenode` — proving the tree **populated** from the same FileSearch mechanism as the picker (`children_of` projection + the `ListView` drew the row). It verifies population only; single-click-open and the highlight/reveal of the current file are covered by the S1 `current_file` core tests and the `tree.rs` / `rel_path_for` pure unit tests (OCR cannot reliably confirm a highlight color, and pixel-accurate row clicking is deliberately avoided as too flaky).
- **`vertical_split_shows_two_panes_under_cage`** — drops two files into a fresh temp cwd, opens file A, fires `<C-w>v` and `:e`s file B into the new pane, and asserts the OCR contains BOTH headings at once — proving the vertical split created a second pane bound to the new nvim window/grid and painted it beside the first (and that the first pane kept rendering, the `Grid::resize` content-preserving regression guard).
- **`horizontal_split_shows_two_panes_under_cage`** — the same, with `<C-w>s`, so the two files render stacked top/bottom.
- **`three_panes_keep_all_files_visible_under_cage`** — the N-ary flatten regression guard: opens file A, `<C-w>v` + `:e` file B (a side-by-side `A | B`), focuses the left pane (`<C-w>h`), then `<C-w>v` + `:e` file C in it, producing a flat row of THREE equal columns. It asserts the OCR contains ALL THREE headings at once — the load-bearing assertion being the pre-existing RIGHT pane (B), which the old binary always-nest model collapsed/blanked when a same-orientation third split nested into 25/25/50 instead of flattening into equal thirds.
- **`focus_move_returns_to_first_pane_under_cage`** — vsplits (A left, B right), inserts a marker, then `<C-w>h` back to the left pane and inserts again; asserts the OCR contains the marker AND both headings (so the focus-move plumbing reached a focused pane and the split survived). It proves the plumbing end to end but does NOT pixel-verify which pane got the marker — the pure `layout::neighbor` unit tests cover neighbour selection.
- **`close_pane_leaves_one_under_cage`** — vsplits (a survivor file + a to-close file), `<C-w>c` on the focused pane, and asserts the OCR contains the survivor's heading but NOT the closed pane's — proving `<C-w>c` removed the focused pane and the survivor reclaimed the width.
- **`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`** — the one test in this file that is **not** a cage smoke. It is pure: no compositor, no `require_smoke_env`, no `CAGE_LOCK`, no subprocess, microseconds to run, so it still guards the fixture on a machine without the smoke toolchain. It reads two const tables and asserts that no `SIDEBAR_FILLER_FILES` name contains any term in `ASSERTED_OCR_TERMS` — the table of every substring the nine smokes assert OCR must or must not contain, built from the OCR consts themselves rather than re-typed literals so it cannot drift. Were a filler ever named `fillhello.md`, the sidebar would render `hello` on a row of its own, `window_renders_typed_text_under_cage`'s assertion would be satisfied by that row, and the smoke would pass green forever with an editor pane that drew nothing — the silent false positive `CLAUDE.md` warns about, and one that is invisible in review because the fixture and the assertion it can defeat sit a thousand lines apart. It also pins that every asserted term is lowercase (OCR output is lowercased before every `contains`, so an uppercase const could never match, which would make the filename check vacuous too) and that the filler names are distinct (duplicates collapse on disk and would silently thin the page density). A fixture that can disarm the assertion it exists to support needs an executable guard, not prose.

Required toolchain + env for the **nine cage smokes** (all from the flake devShell smoke toolchain, and now **gate-required** rather than optional): `cage`, `wtype`, `grim`, `tesseract` on `PATH`; `WOLF_NVIM_BIN` set; and a set, writable `XDG_RUNTIME_DIR`. Per project policy `require_smoke_env` **fails loud** (not a silent skip) on any missing precondition, since absence means the smoke toolchain was not provisioned. A stale screenshot is removed first so a previous run cannot make the test pass. The tenth test needs none of this.

**NOTE:** the smoke test asserts only the typed `hello`, not the cursor; cursor geometry and color are covered by the pure `cursor.rs` unit tests above plus visual confirmation (OCR is too fragile to couple to the cursor).

## `common/mod.rs`

Local test helpers (Cargo cannot share a `tests/common` module across crates, so these mirror `wolf-core`'s): `require_nvim_bin()` (fail-loud `WOLF_NVIM_BIN` check), `init_lua_path()` (resolves `../../runtime/nvim/init.lua` from this crate's `CARGO_MANIFEST_DIR`), `config(cols, rows)`, and `wake_channel()` → `(WakeFn, FlushWaiter)` with `wait_for_flush(timeout)` / `drain()` and `FLUSH_TIMEOUT = 10s`.

## The `key_from_char` helper

Both `gdk_key.rs` and `roundtrip_nvim.rs` define a small `key_from_char(c: char) -> gdk::Key` helper, because gdk4-rs has no `Key::from_unicode`. It builds the keyval exactly as GDK would from a Unicode scalar:

```rust
use gdk::glib::translate::from_glib;
fn key_from_char(c: char) -> gdk::Key { unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) } }
```

This is test-side only and does not change `gdk_key`'s contract: printables still carry the exact glyph, and `gdk_key` reads `keyval.to_unicode()` and sets `shift: false` for `Key::Char`.
