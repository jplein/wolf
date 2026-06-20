# Current Plan: Start wolf-core (Phase 1)

## Problem statement

Build the foundational `wolf-core` Rust library for Wolf, a Markdown editor that
embeds Neovim as its editing engine. Phase 1 stands up the platform-agnostic core
that launches a hermetic, pinned `nvim --embed`, attaches as an `ext_linegrid`
UI, folds the `redraw` event stream into a render-state snapshot, accepts abstract
input events, and is verified headlessly (no GUI front-end yet).

Source of truth: `README.md`, `architecture.md`, `build.md` (docs override impl).

### The one hard rule
`wolf-core` must compile with **no GUI toolkit in its dependency tree**.

### Environment constraints (NixOS)
- Toolchain (Rust + pinned `nvim`) comes from a NixOS-only `flake.nix` devShell,
  activated by a Linux-guarded `.envrc` (`use flake`).
- Do NOT use `rustc`/`cargo`/`nvim` from the system PATH; do NOT search `/nix/store`.
  All builds/tests run inside the devShell.

### Scope (in)
1. `flake.nix` (NixOS-only): Rust toolchain + pinned hermetic `nvim`; no GUI deps.
2. `.envrc`: Linux-guarded `use flake`.
3. Fill in `build.md` (flake/direnv workflow); note in `architecture.md` §8 that
   binary pinning is day-one, plugin baking (treesitter/marksman) is later.
4. Cargo workspace + `crates/wolf-core` skeleton (compiles, GUI-free).
5. `runtime/nvim/init.lua`: minimal isolated markdown config.
6. `src/nvim`: spawn + supervise captive `nvim --embed --clean -u init.lua` via
   nvim-rs over stdio, pinned XDG_*/NVIM_APPNAME.
7. `src/render`: render-state model (grid cells char+hl_id, hl table from
   hl_attr_define/default_colors_set, cursor from mode_info_set/mode_change,
   win_viewport).
8. `src/redraw`: handler for grid_line, grid_scroll, grid_resize, grid_clear,
   flush -> snapshot behind a lock; flush triggers wake callback.
9. `src/input`: key -> nvim notation + mouse/scroll/resize/open/save.
10. `src/api`: FFI-shaped front-end surface + wake callback.
11. Headless verification: `examples/grid_dump.rs` + integration tests against the
    pinned nvim.
12. `README.md` in each source dir.

### Scope (out — later phases)
Full `wrapNeovim` plugin baking, swift-bridge/macOS, GTK/libadwaita front-end,
native cmdline/messages/popupmenu, theming, LSP/treesitter bundling.

## Progress log
- Stage A DONE: `flake.nix` + `.envrc` created. User restarted; devShell active
  (cargo/rustc 1.96.0, WOLF_NVIM_BIN -> nvim 0.12.2). Resuming Stage B onward.

## Progress log (cont.)
- Stages C/D/E (code) DONE by coder. Workspace + wolf-core compile clean (no
  warnings, clippy clean). GUI-free `cargo tree` check: clean. All 9 integration
  tests pass (real nvim via WOLF_NVIM_BIN), stable over 5x + 10x flaky reruns.
  Two real bugs fixed: input-burst reordering (ordered input pump + key fusing)
  and shutdown deadlock (cooperative write_async + bounded shutdown). Deviations:
  parking_lot arc_lock owned read guard, tempfile promoted to normal dep,
  write_async added. Still TODO: per-dir READMEs + build.md + architecture.md §8
  (technical-writer), reviewer pass, and 3 approved test additions below.
- DECISION: approved all 3 coder-recommended test additions (high-risk coverage):
  (a) wide-char grid_line (double-width glyph -> trailing empty cell + alignment),
  (b) grid_scroll content-shift assertion, (c) key_to_notation F-key + Alt/Meta.

## Progress log (cont. 2)
- 3 added tests run: wide_char_grid_line exposed a REAL bug (impl synthesized a
  trailing wide-cell that nvim already emits explicitly -> misalignment after
  double-width glyph). Fixed in src/redraw.rs (store nvim's empty cell verbatim).
  Meta->`<D-s>` and Alt->`<A-x>` already correct. All 12 tests pass, stable; tree
  GUI-free; clippy clean.
- DECISION: approve ONE final test addition — direct grid_line `[text,hl,repeat]`
  expansion assertion (covers freshly-modified column-advance logic). Then proceed
  to reviewer + technical-writer.

## Progress log (cont. 3)
- Reviewer: APPROVED, no blockers. All load-bearing reqs confirmed. 4 minor
  Should-fix notes triaged:
  - S3 (fix): key_to_notation should NOT emit `<S-x>` for printable chars; deliver
    shifted glyph, drop S- prefix for printables; document the contract.
  - S4 (fix): de-dup the two key-translation paths (apply vs apply_batch).
  - S1 (test): add down-direction grid_scroll content-shift assertion.
  - S2 (test): assert win_viewport.linecount equals buffer line count.
  - N1-N3 + fnameescape fidelity: ACCEPTED as-is for Phase 1.
- 13 tests confirmed passing by coder (reviewer's inability to read tests 2-13 was
  broken Glob/Grep tooling, not a real gap).

## Progress log (cont. 4)
- S3/S4 src fixes landed (key_to_notation no S- for printable chars + documented
  contract; single key-translation path via shared nvim_input helper). Test type
  mismatch (linecount u64 vs usize) fixed in test. ALL 15 TESTS PASS, stable;
  clippy -D warnings clean; tree GUI-free. viewport linecount==50 assumption holds.
- REMAINING: Stage B docs (build.md, architecture.md §8 note) + Stage E per-dir
  READMEs (technical-writer). Then report completion.

## Orchestration note (amended)
After `flake.nix` and `.envrc` are created, the orchestrator PAUSES and asks the
user to restart so the devShell/direnv environment is active before any build or
test work proceeds.

## Architect's plan

### Key cross-cutting decisions
- **Use generic untyped nvim-rs RPC** (`Neovim::call`/`input`/`command`) for all
  version-sensitive calls; consume the `redraw` stream as raw `Vec<rmpv::Value>`.
  This insulates us from nvim-rs 0.9.2's older typed API vs nvim 0.12.
- **nvim located only via `WOLF_NVIM_BIN`** (exported by the flake devShell). Never
  PATH, never `/nix/store` scanning.
- **GUI-free enforcement** = documented `cargo tree` denylist check (Wolf's analog
  of depguard): deny `gtk4 gtk glib gdk4 cairo cairo-rs pango libadwaita objc objc2
  cocoa core-graphics winit wgpu skia-safe softbuffer`.
- The Go/tarjeta/golangci conventions that leaked into context DO NOT APPLY (Rust
  project). Use clippy + cargo-tree denylist + named consts + no mutable globals.

### BUILD ORDER
- **Stage A (then HARD PAUSE):** `flake.nix`, `.envrc`. User runs `direnv allow` /
  restarts so toolchain + `WOLF_NVIM_BIN` are live. No cargo/nvim before this.
- **Stage B (docs):** `build.md` (replace TODO), `architecture.md` §8 phase note.
- **Stage C (skeleton, compiles empty):** workspace `Cargo.toml`,
  `crates/wolf-core/Cargo.toml`, `src/lib.rs` stubs, `runtime/nvim/init.lua`.
- **Stage D (modules, in dep order):** render → input → redraw → nvim → api →
  finalize lib.rs re-exports.
- **Stage E (verification):** `examples/grid_dump.rs`, integration tests, per-dir
  READMEs.

### flake.nix (Stage A)
- NixOS-only (systems: x86_64-linux, aarch64-linux; no darwin outputs).
- Inputs: pinned `nixpkgs` (rev shipping nvim ≥ 0.12), `rust-overlay` (oxalica;
  chosen over nixpkgs rustPlatform because we want a devShell toolchain with
  rust-analyzer/clippy/rustfmt, not a build derivation), `flake-utils` optional.
- devShell (`pkgs.mkShell`) exposes: rustToolchain (stable pinned, extensions
  rust-src/rust-analyzer/clippy/rustfmt), `pkgs.neovim`, `pkgs.pkg-config`
  (defensive). shellHook exports `WOLF_NVIM_BIN="${lib.getExe pkgs.neovim}"`.
- No GUI libs in the shell.

### .envrc (Stage A)
```
if [ "$(uname -s)" = "Linux" ]; then
  use flake
fi
```
No-op on non-Linux. No other-platform branches.

### build.md (Stage B)
Prereqs (NixOS, Nix flakes, direnv+nix-direnv); `direnv allow` to enter devShell;
`WOLF_NVIM_BIN` points at pinned hermetic nvim; `cargo build` / `cargo test` (tests
spawn pinned nvim via WOLF_NVIM_BIN, headless since `nvim --embed` needs no TTY);
toolchain pinned by flake — don't use system cargo/nvim; note wrapNeovim plugin
baking deferred.

### architecture.md §8 (Stage B)
Append a "Phase 1 status" note: nvim binary pinned/hermetic from day one via the
flake devShell (located via WOLF_NVIM_BIN, launched `--embed --clean -u
runtime/nvim/init.lua` with isolated XDG_*/NVIM_APPNAME); full wrapNeovim plugin
baking (treesitter grammars, marksman) deferred. Keep existing §8 text intact.

### Cargo.toml workspace + crates/wolf-core/Cargo.toml (Stage C)
- Workspace: resolver "2", members ["crates/wolf-core"], `[workspace.package]`
  edition 2021, optional `[workspace.dependencies]`.
- wolf-core deps: `nvim-rs 0.9` (feature use-tokio), `tokio 1` (rt-multi-thread,
  macros, io-util, process, sync, time), `rmpv 1.3`, `parking_lot 0.12`,
  `thiserror`, `tracing`, `async-trait 0.1` (Handler is async_trait-shaped).
- dev-deps: `tokio` (test-util, macros, rt-multi-thread), `tempfile 3`,
  `tracing-subscriber 0.3`.
- None pull a GUI toolkit (verify with cargo tree denylist).

### runtime/nvim/init.lua (Stage C)
`--clean`-compatible markdown-only isolated config. Set: swapfile=false,
shadafile=NONE, undofile=false, backup/writebackup=false, shortmess+=I,
termguicolors=true (so hl_attr_define/default_colors_set carry RGB),
number=false, signcolumn=no, ruler=false, showcmd=false, laststatus=0,
mouse=a, wrap=false (deterministic test columns). No treesitter/LSP (deferred;
comment saying so). Header comment: hermetic, loaded via --embed --clean -u.

### src/render (Stage D, first — pure data)
Types: `HlAttr{fg,bg,special:Option<u32>, bold/italic/underline/undercurl/
strikethrough/reverse:bool}`, `DefaultColors{fg,bg,special}`,
`HighlightTable{defaults, attrs:HashMap<u64,HlAttr>}` with `resolve(hl_id)->
ResolvedStyle`, `define`, `set_defaults`; `Cell{text:String, hl_id:u64}` (text =
one grapheme cluster, "" for trailing wide cell); `Grid{cols,rows,cells:Vec<Cell>}`
with resize/clear/cell/set_cell/scroll/row_text; `CursorShape`,
`ModeInfo{shape,cell_percentage,attr_id,name}`, `Cursor{row,col,mode_idx}`;
`Viewport{topline,botline,curline,linecount}`; `Snapshot{grid,highlights,cursor,
mode_infos,viewport,seq}`; `SharedSnapshot(Arc<parking_lot::RwLock<Snapshot>>)`
with new/read/write/clone_handle.
LOCKING INVARIANT: one RwLock; redraw takes the write lock once per whole batch,
NEVER holds it across `.await`; wake fired after unlock. Named consts for color
shifts/masks (no bare magic numbers).

### src/input (Stage D)
`InputEvent{Key,Mouse,Scroll,Resize{cols,rows},Open{path},Save,SaveAs{path}}`;
`KeyEvent{key:Key, ctrl/shift/alt/meta}`, `Key{Char,Enter,Esc,Tab,Backspace,
arrows,Home,End,PageUp,PageDown,F(u8),...}`; Mouse/Scroll enums.
`key_to_notation(&KeyEvent)->String` (modifier order C-,S-,A-,D-; Enter=<CR>,
Esc=<Esc>, Tab=<Tab>, BS=<BS>, '<'=<lt>; plain 'a'->"a").
`apply(nvim,&InputEvent)`: Key->nvim_input(notation); Mouse->nvim_input_mouse;
Scroll->wheel; Resize->nvim_ui_try_resize (grid corrected by resulting
grid_resize, don't resize locally); Open->`edit {fnameescape path}`;
Save->`write`; SaveAs->`saveas {path}`. Provide fnameescape helper.

### src/redraw (Stage D)
`WakeFn=Arc<dyn Fn()+Send+Sync>`. `RedrawHandler{snapshot:SharedSnapshot,
wake:WakeFn}` impl `nvim_rs::Handler` (`type Writer=Compat<ChildStdin>`,
handle_notify routes name=="redraw" to private `apply_redraw(Vec<Value>)`).
apply_redraw: take write lock ONCE per batch; match event:
grid_resize/grid_clear/grid_line (walk cells: [text],[text,hl],[text,hl,repeat],
carry-forward hl_id, expand repeat, wide-char trailing empty cell)/grid_scroll/
hl_attr_define/default_colors_set/mode_info_set/mode_change/grid_cursor_goto/
win_viewport/flush(bump seq, DROP lock, then call wake)/unknown=ignore.
Wake may run on a tokio worker thread (front-end marshals to UI thread).

### src/nvim (Stage D)
`NvimConfig{nvim_bin,init_lua,appname,state_root,initial_cols,initial_rows}` with
`from_env()` (reads WOLF_NVIM_BIN, builds isolated XDG dirs under a private temp).
`NvimError` (BinNotSet/Spawn/Rpc/Attach). `spawn(&cfg, RedrawHandler)->NvimProcess`:
tokio Command for nvim_bin with args `--embed --clean -u <init.lua> -i NONE -n`;
env-pin NVIM_APPNAME=wolf + XDG_{CONFIG,DATA,STATE,CACHE}_HOME under state_root;
`new_child_cmd` -> (neovim, io_handle); ui_attach via generic RPC
`nvim_ui_attach(cols,rows,{ext_linegrid=true,rgb=true})` (only ext_linegrid).
`NvimProcess`: handle() (clone for RPC), ui_attach, try_resize, shutdown
(`qa!` + await io_handle w/ timeout, kill on timeout). Generic RPC only.

### src/api (Stage D)
FFI-shaped (swift-bridge-expressible later; none added now). `WolfCore{runtime:
tokio::runtime::Runtime, nvim:NvimProcess, snapshot:SharedSnapshot}`,
`CoreConfig{init_lua,cols,rows}`. `start(cfg, WakeFn)->Result<WolfCore>` builds
multi-thread rt, block_on(nvim::spawn), wires wake into RedrawHandler, ui_attach.
`read_snapshot()->SnapshotReadGuard` (GTK path); `send_input(InputEvent)`
(non-blocking: runtime.spawn(apply)); `resize(cols,rows)`; `shutdown()`.
One wake callback (on flush), no per-event callbacks across boundary.

### examples/grid_dump.rs (Stage E)
Reads WOLF_NVIM_BIN + init.lua path; WolfCore::start with wake setting a channel;
open/type a few keys; wait for flush; print grid.row_text per row + cursor + a
sampled cell's resolved fg/bg; shutdown. `cargo run -p wolf-core --example grid_dump`.

### Integration tests — crates/wolf-core/tests/ (Stage E)
Real nvim via WOLF_NVIM_BIN (no mocks, per CLAUDE.md), headless. `common/mod.rs`
helpers (start_core_with_temp_md, wait_for_flush(timeout), temp .md). Tests must
FAIL (not silently pass) if WOLF_NVIM_BIN unset. Generous flush timeout (5–10s).
Cases:
1. spawn_and_attach: grid size matches; defaults populated; mode_infos non-empty; shutdown Ok.
2. type_text_appears_in_grid: i + "Hello" + Esc -> row 0 starts "Hello"; H cell hl resolves non-None fg.
3. open_existing_markdown: "# Title" -> row 0 contains it; heading hl_id differs from body.
4. cursor_moves: j/l advance cursor.row/col; i selects insert ModeInfo (vertical bar).
5. resize_updates_grid: resize -> grid cols/rows match (nvim_ui_try_resize -> grid_resize).
6. save_writes_file: type + Save -> file on disk contains text.
7. save_as_new_path: type + SaveAs -> new path exists with content.
8. scroll_emits_viewport: tall buffer + Scroll Down -> viewport.topline increases.
9. key_notation_unit: pure asserts on key_to_notation.

### Per-directory READMEs (Stage E, CLAUDE.md requirement)
crates/wolf-core/README.md (overview + GUI-free rule + cargo tree denylist cmd +
WOLF_NVIM_BIN dep), and a README.md in each of src/{nvim,render,redraw,input,api}
and runtime/nvim describing interfaces/functions.

### Risks
nvim-rs 0.9.2 typed API vs nvim 0.12 (use generic RPC); Handler needs async-trait;
Child ownership/Writer type (verify at code time, supervise via io_handle);
lock latency/deadlock (whole-batch lock, no await held, wake after unlock); wake on
worker thread (Send+Sync); grid_line parsing (wide/repeat/carried hl — highest bug
risk, covered by tests 2/3); GUI-free regression (cargo tree denylist); test
flakiness from nvim startup (channel wait + generous timeout).
