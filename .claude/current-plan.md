# Unit: the Linux optimized release build target

Branch: `linux-feat/release-build` (cut from `linux` in the main working directory
`/home/jplein/dev/jplein/wolf`). The `linux` branch itself is checked out in the worktree
`/home/jplein/dev/jplein/wolf/.worktrees/linux`, so never `git checkout linux` here.

## Problem statement

Item 1 of "Suggested order of work for Linux" in `feat-list.md`. Linux builds only the
default `cargo` profiles today — the workspace `Cargo.toml` declares no `[profile]` block
at all — so every binary a human runs and every number anyone measures carries debug
codegen. The row to tick is "Optimized release build target" in the "Reliability and
performance" table (`feat-list.md:234`), Linux column.

The macOS counterpart is `//crates/wolf-macos:WolfRelease`, a Bazel configuration
transition documented in `build.md`. Linux builds with `cargo` inside the Nix devShell, so
the mechanism does not transfer. The reasoning does: an optimized build must not sweep into
ordinary test runs, and the artifact's honest evidence is that it *runs*, not that it
compiles.

## Orchestrator decisions on the architect's open questions

1. **`lto = "thin"`** — adopt. Coder measures the cold build wall clock; if it exceeds
   roughly 25 minutes on this 32-core host, stop and report rather than proceeding.
2. **`debug = "line-tables-only"`** — adopt. The named downstream consumer is item 2's
   `perf`-style profiling, which needs line tables, not variable inspection. `debug = true`
   costs size for nobody named.
3. **Run both cage smokes** as release evidence. The preview smoke additionally proves the
   WebKit helper processes spawn from the optimized binary, which is the heaviest runtime
   dependency in the tree.
4. **No new prose sentence** in `feat-list.md`'s "Reliability and performance" section.
   `build.md` owns the mechanism; restating it there would create a second owner.
5. **Ship all three guards** in `release_profile.rs`. The `panic = "abort"` failure is
   silent, expensive and has no other observer; the ELF check is not a tautology because it
   reads the artifact rather than the promise.

Verified by the orchestrator before adopting the plan: `tools/wolf-pointer-inject` is a C
derivation with `src = ./tools/wolf-pointer-inject`, so editing the workspace `Cargo.toml`
does not move its hash and forces no devShell rebuild.

## The profile block

Goes in `/home/jplein/dev/jplein/wolf/Cargo.toml`, after `[workspace.package]`, with a
comment block carrying the rationale below plus the deliberately-unset list.

```toml
[profile.release]
opt-level = 3
lto = "thin"
panic = "unwind"
debug = "line-tables-only"
strip = "none"
```

| key       | buys                                                                                                                | costs                                                       |
| --------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| `opt-level = 3` | Full LLVM optimization; also reaches `rusqlite`'s bundled SQLite, since `cc` reads `OPT_LEVEL`. Explicit so the manifest states the contract. | Compile time. |
| `lto = "thin"` | Cross-crate inlining across the `wolf-gnome` → `wolf-core` paint/snapshot path; parallel and bounded, unlike fat LTO. | Link wall clock and peak RSS; `bench` inherits it, so `cargo test --release` pays too. |
| `panic = "unwind"` | Declared at the point of temptation. Cargo silently ignores `panic` for `test`/`bench`, so `"abort"` forks `target/release/` between `cargo build --release` and `cargo test --release` and rebuilds the whole graph on every alternation. `"abort"` would also turn app panics into `SIGABRT`, changing the exit-code evidence the smokes rest on. | Nothing; it is the default. |
| `debug = "line-tables-only"` | Function names and file:line for `perf` and for release panic backtraces — what item 2 needs. | Binary size and link time; not enough for `gdb` variable inspection. |
| `strip = "none"` | Explicit because cargo's `strip` default is version-dependent; the contract is that symbols survive. | A larger artifact. |

Deliberately unset, and the manifest comment says so: `codegen-units` (release default 16;
thin LTO recovers most of what CGU splitting gives up, and `1` serializes final codegen on
a host whose parallelism is the point), `incremental`, `overflow-checks`,
`debug-assertions`, `split-debuginfo` (embedded default keeps a copied binary's symbols —
a trap the packaging unit would otherwise inherit), `[profile.release.package."*"]` and
`[profile.release.build-override]`.

### What the block does and does not change

| command                                     | profile                                | inherits `[profile.release]`? | output      |
| --------------------------------------------- | ---------------------------------------- | ------------------------------- | ------------- |
| `cargo build`                               | `dev`                                  | no                            | `debug/`    |
| `cargo build --release`                     | `release`                              | yes                           | `release/`  |
| `cargo run --release -p wolf-gnome`         | `release`                              | yes                           | `release/`  |
| `cargo test`                                | `test` (targets) / `dev` (lib and bin) | **no**                        | `debug/`    |
| `cargo test --release`                      | `bench` (targets) / `release`          | yes                           | `release/`  |
| `cargo clippy --workspace --all-targets`    | `dev` / `test`                         | no                            | `debug/`    |
| `cargo build --workspace --all-targets`     | `dev` / `test`                         | no                            | `debug/`    |

No step of the Linux verification gate selects `release` or `bench`, so the gate is
untouched. That is the Linux analogue of `WolfRelease`'s `manual` tag: the optimized build
never sweeps in because no ordinary command asks for it.

## How a human builds and runs it — documented commands, no wrapper script

`build.md` owns build and run commands per `CLAUDE.md`; a committed script would either
duplicate them or force `build.md` to point at a second owner. The macOS side sets the
precedent — `:WolfRelease` needs a genuinely awkward `cquery` dance and `build.md` still
documents commands rather than shipping a script.

From the repository root, which is not optional (the shellHook generates `FONTCONFIG_FILE`
from `$PWD/crates/wolf-gnome/testdata/fonts.conf`):

```sh
nix develop --command cargo build --release
nix develop --command cargo run --release -p wolf-gnome
nix develop --command ./target/release/wolf-gnome   # no cargo in the process tree
```

Outside the devShell it fails in a known order: `WOLF_NVIM_BIN` unset makes
`WolfCore::start` return `NvimError::BinNotSet` and the process exits after one stderr
line; then `GSETTINGS_SCHEMA_DIR`/`XDG_DATA_DIRS` (`adw::Application` aborts on activate);
then `FONTCONFIG_FILE` (substituted fonts, so cell metrics differ from every golden); then
`WOLF_RG_BIN` and the two treesitter dirs.

## Files

Create:

- `crates/wolf-gnome/tests/release_profile.rs` — this crate because it has no
  `BUILD.bazel` (deliberate, see `crates/wolf-gnome/DECISIONS.md`), so the file needs no
  `rust_test` wiring and cannot become a silent macOS-only gap; and because the manifest
  half reads `<CARGO_MANIFEST_DIR>/../../Cargo.toml`, which is not a Bazel runfile, while
  the artifact half needs `CARGO_BIN_EXE_wolf-gnome`, which only this crate exports.

Modify:

- `Cargo.toml` — the profile block plus its comment.
- `build.md` — four edits, two of them rewrites of existing sentences (see doc plan).
- `crates/wolf-gnome/tests/README.md` — rewrite line 11's "three kinds of file" framing,
  add a `#### release_profile.rs` entry.
- `feat-list.md` — tick line 234's Linux box and extend its Branch column to the
  two-platform form neighbouring rows use; delete ordered-list item 1 and renumber 2→1 …
  6→5 **without rewording any survivor**.
- `macos-followups.md` — one ledger row: `//:Cargo.toml` is one of `crate_universe`'s
  `manifests` in `MODULE.bazel`, so the repository rule re-runs; profiles resolve no
  dependency and do not appear in `cargo metadata`, so no repin is expected and neither
  lockfile should move — unconfirmable from Linux, so macOS runs
  `bazel build //crates/wolf-core:wolf_core` and checks `Cargo.Bazel.lock` is unmoved.

Explicitly unchanged — do not invent work: `crates/wolf-gnome/README.md`,
`crates/wolf-gnome/DECISIONS.md` (the profile is workspace-scoped and survives deleting
the crate, so by `CLAUDE.md`'s test it is not a crate document's business),
`architecture.md`, `flake.nix`, the `wolf-core`/`wolf-ffi` test READMEs, every
`BUILD.bazel`, `MODULE.bazel`, `Cargo.lock`, `Cargo.Bazel.lock`.

## Test plan

`crates/wolf-gnome/tests/release_profile.rs`, no new dependency. The ELF walk is
hand-rolled specifically so no `object`/`toml` dev-dependency enters `Cargo.lock`, which
**is** a `crate_universe` input and would create a repin obligation undischargeable from
Linux. Shelling out to `readelf` is rejected: it is not in the devShell `packages`, so it
would mean a `flake.nix` edit and a stale direnv for every developer.

Helpers: `workspace_manifest_path()`; `release_profile_keys(&str) -> BTreeMap` (a small
line scanner, not a TOML parser — track the current `[section]`, collect `key = value`
only under exactly `[profile.release]`, stop at the next `[` so a future
`[profile.release.package."*"]` is not folded in, trim quotes); `dwarf_line_section_size()`
(minimal ELF64 section-header walk; assert `\x7fELF`, `EI_CLASS == 2`, `EI_DATA == 1`,
`e_shentsize == 64`, refuse `e_shnum == 0` or `e_shstrndx >= 0xFF00`, walk with
`chunks_exact(64)` not an index loop, resolve names via `e_shstrndx`).

**A substring scan for `.debug_line` is unsound and must not be substituted.** Every Rust
binary links std's backtrace machinery (`object`/`gimli`), which carries DWARF section
names as string literals in `.rodata`, so a fully stripped binary still contains the bytes.

| # | Test                                             | Asserts                                                                                | Falsifying edit                                                             | Command                                                                            |
| - | -------------------------------------------------- | ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| 1 | `the_release_profile_never_aborts_on_panic`      | `[profile.release]` exists and `panic` is exactly `unwind`                             | `panic = "unwind"` → `"abort"`                                              | `cargo test -p wolf-gnome --test release_profile`                                  |
| 2 | `the_release_profile_keeps_its_debug_info`       | `debug` present and not `0`/`false`/`none`; `strip` is `none` or absent                | `strip = "none"` → `"symbols"`                                             | same as 1                                                                          |
| 3 | `the_binary_under_test_carries_line_tables`      | `CARGO_BIN_EXE_wolf-gnome` has a non-empty `.debug_line`; prints path and size          | dev: add `[profile.dev] debug = false`. release: `debug = false` in release  | 1's command, and `cargo test --release -p wolf-gnome --test release_profile -- --nocapture` |

Assert nothing about `opt-level`, `lto` or `codegen-units`: those have no silent failure
mode, and pinning them turns a future measured retune into a test edit.

### Defect found in test 3 by the failure-evidence step

Test 3 as written **does not detect `debug = false`**, the setting it is named and
documented for. Measured: with `debug = false` the binary still carries a 271,586-byte
`.debug_line`, because Rust's precompiled standard-library rlibs (`std::backtrace_rs`,
`addr2line`, `gimli`, `alloc`, `core`) ship debug info that `[profile.release] debug` does
not govern. With `debug = "line-tables-only"` the section is 5,980,598 bytes. The only
falsifier found is `strip = "debuginfo"`, which removes the section entirely — so the test
is attached to the `strip` contract, not the `debug` contract, and its rustdoc "Falsifiers"
paragraph is factually wrong.

The ELF walk itself is sound: its 271,586 matched `readelf`'s `0x424e2` exactly.

### The repair, chosen from measurement

**Decision: a semantic source-path check inside `.debug_str`, read by section header**, in
addition to the existing `.debug_line` size check. Not a size floor — a threshold is a
magic number that ages with std and the dependency graph, and the semantic check attaches
the test to the `debug` contract directly rather than by proxy.

Measured cross-product on this host (`rustc 1.96.0`, DWARF version 4 uniformly, so
`.debug_line_str` does not exist — it is a DWARF 5 section):

| variant                     | `.debug_line`   | `.debug_str` contains `crates/wolf-gnome/src` | whole-file scan finds `crates/wolf-gnome` |
| ----------------------------- | ----------------- | ----------------------------------------------- | ------------------------------------------- |
| `debug = "line-tables-only"` | 5,981,172 bytes | yes — 17 hits                                 | yes                                       |
| `debug = false`              | 271,586 bytes   | **no** — only `/rustc/<hash>/library/` and `/rust/deps/` | yes — **18 hits**             |
| `strip = "debuginfo"`        | absent          | absent (every `.debug*` gone but a 34-byte `.debug_gdb_scripts`) | yes — 18 hits          |

Full `.debug*` inventory, `readelf -S -W`, whole-workspace `cargo build --release`:

| section          | `line-tables-only` | `debug = false` |
| ------------------ | -------------------- | ----------------- |
| `.debug_info`    | 12,623,398         | 738,640         |
| `.debug_str`     | 5,477,789          | 1,326,607       |
| `.debug_line`    | 5,981,172          | 271,586         |
| `.debug_loc`     | 6,104,795          | absent          |
| `.debug_line_str` | absent             | absent          |

`.debug_str` is populated in both — the discriminator is the needle, not the section's
presence. `.debug_line_str` is absent because rustc emits **DWARF 4** uniformly here
(`rustc 1.96.0`, LLVM 22.1.2, `x86_64-unknown-linux-gnu`); it is a DWARF 5 section.

The test therefore searches `.debug_str`, then `.debug_line_str`, then `.debug_line`, and
passes if any present section carries the needle. Not `.debug_line` alone: LLVM emits
DWARF 5 file and directory names as `DW_FORM_line_strp` into `.debug_line_str`, so a rustc
default bump to DWARF 5 would move the strings and produce a false red.

**Do not formulate the check as "some `.debug*` section exists".** Under
`strip = "debuginfo"` every `.debug*` section vanishes except `.debug_gdb_scripts`, which
survives at 34 bytes.

Binary sizes for the two variants: 45,656,208 vs 12,791,936. Note also that a
`-p wolf-gnome` build differs from a whole-workspace one by 7,424 bytes of feature
unification, so quote which build a size came from.

The 18 `.rodata` hits in the `debug = false` binary come from `init_lua_path`'s
`env!("CARGO_MANIFEST_DIR")` and from `panic!` location strings. **This is the second and
sharper proof that a whole-file substring scan is unsound**: it would pass on exactly the
binary the test must reject. Only a section-scoped read discriminates.

So the repaired test carries two assertions with two distinct falsifiers: `.debug_line`
non-empty (falsified by `strip = "debuginfo"`) and a workspace source path in `.debug_str`
(falsified by `debug = false`). Its rustdoc must claim only these two verified falsifiers.

End-to-end evidence, existing smokes, no new smoke (so `flake.nix`'s `GATE-REQUIRED` count
and `tests/README.md`'s pass count are untouched):

```sh
nix develop --command cargo test --release -p wolf-gnome --test smoke_headless -- \
  --exact window_renders_typed_text_under_cage \
          preview_pane_renders_and_edit_source_returns_under_cage
```

The first proves the window maps, the captive Neovim is alive and receiving keys, and
Cairo/Pango really drew glyphs. The second proves the WebKit helpers spawn and render.
`CAGE_LOCK` serializes them, so one invocation is safe; check no stray `cage` holds the
Wayland socket first.

## Measurement — the honesty rule

**No honest runtime measurement exists on this host.** The GTK front-end carries no
frame-time instrumentation (that is item 2), and every proxy is dominated by something
else: `grid_dump` and the `wolf-core` suite by Neovim startup and RPC waits, a cage smoke's
wall clock by fixed settle sleeps. So this unit makes **no speed claim**, and `build.md`
says that in as many words.

### What was actually measured

Toolchain: `cargo 1.96.0 (30a34c682 2026-05-25)`, `rustc 1.96.0 (ac68faa20 2026-05-25)`.

Cold `cargo build --release` (`target/release` did not exist; 167 crates from scratch):
`real 0m52.610s`, `user 6m19.665s`, `sys 0m26.134s` on the 32-core dev host. `target/release`
came to 1.4 GB. Far under the 25-minute threshold, so the `lto = "thin"` decision stands
without revisiting.

Binary sizes for `target/release/wolf-gnome`:

| build                                          | bytes      |
| ------------------------------------------------ | ------------ |
| `debug = "line-tables-only"`, `strip = "none"` | 45,656,208 |
| `debug = false`                                | 12,791,936 |
| `strip = "debuginfo"`                          | 10,165,744 |

Retained line tables cost 32,864,272 bytes (~31.3 MiB). Not comparable to macOS's
documented 8.26 MB — different linker, different debug policy, dynamically linked GTK
against a bundled `.app`.

Gate: exit 0, 1036 tests passed. `cargo test --workspace` took `real 10m8.964s`,
`user 1m17.190s` — the ratio confirms it is dominated by waiting on Neovim and the
compositor, not compilation, and sits at the low end of `build.md`'s documented 10-25
minutes. **Every `Finished` line in the gate log reads `dev` or `test`, never `release` or
`bench`**, `release_profile.rs` ran from `target/debug/deps/`, and `target/release/wolf-gnome`
kept its pre-gate timestamp. The named hazard did not materialise.

Both cage smokes passed under `--release` in 21.31 s, first try, no flake.

### Corrections to this plan, measured rather than predicted

The "Outside the devShell it fails in a known order" paragraph above was **wrong** and must
not be copied into `build.md`. Measured under `env -i` with `LD_LIBRARY_PATH`,
`IN_NIX_SHELL` and `WOLF_NVIM_BIN` all confirmed unset:

- `ldd target/release/wolf-gnome | grep 'not found'` produces no output; all 188 entries
  resolve through store RPATHs written by nixpkgs' linker wrapper.
- With no display at all: `Gtk-WARNING: Failed to open display`, exit 1.
- With a display but no devShell exports, in this order: about 40 `Fontconfig warning`
  lines from the host fontconfig; then
  `GLib-GIO-CRITICAL: g_settings_schema_source_lookup: assertion 'source != NULL' failed`,
  which is a warning and **does not abort** — the plan's "`adw::Application` aborts on
  activate" claim is false; then
  `wolf-gnome: failed to start core: WOLF_NVIM_BIN is not set; the pinned nvim binary must
  be provided by the flake devShell`; then
  `wolf-gnome: no window could be opened for any of the 1 restored plans; exiting` — and
  **exit code 0** despite opening no window.

The exit-0-with-no-window behaviour is pre-existing `wolf-gnome` behaviour, unrelated to
this unit. Record it; do not fix it here. Confirmed three times.

**Whatever `build.md` says about the exit code must name which path it describes**: the
zero is specific to the has-display, no-devShell path. With no display at all it fails
earlier and differently — `Gtk-WARNING: Failed to open display`, exit 1.

## Doc plan

`build.md`, Linux section only:

1. Line 23, **rewrite in place**: the `FONTCONFIG_FILE` sentence gains that it is generated
   from a repo-relative template, so `nix develop` commands run from the repository root.
2. Lines 35-41, edit the code block's last comment to mark it unoptimized, and one sentence
   after the block saying these select cargo's default profiles, linking to the new
   subsection. Do not repeat the release commands here.
3. New `### The optimized build` between `### Building and testing` and
   `### The verification gate`, carrying: the commands and the repo-root requirement; the
   profile by contract (optimized, unwinding, line-table-bearing) with a pointer to the
   manifest comment as the single owner of the per-key reasons; that no gate step builds
   this profile, so alternating costs a second full graph; that `cargo test --release`
   reuses those artifacts and that this is what `panic = "unwind"` buys; the guard
   commands; the measured build time and size with provenance; and one flat sentence that
   no runtime measurement exists on this platform. **No table modelled on "The measured
   penalty" — there is no number to put in it.**
4. **No change to `### The verification gate`.**

`crates/wolf-gnome/tests/README.md`: rewrite line 11's framing to admit a fourth kind of
file (the built artifact and its manifest) rather than appending beside the stale sentence;
add a `#### release_profile.rs` entry, ~200 words, whose Gotchas cover that the artifact
half only discriminates under `--release`, that the manifest half is a tripwire of the same
shape as `the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes`, that
`opt-level`/`lto`/`codegen-units` are deliberately unpinned, why the file lives in this
crate, and that `.symtab` is not asserted because no revert isolates it.

Then `prettier --write` every edited Markdown file and run `./tools/lint/markdown_check.sh`
on them (the workspace documents are outside the Bazel filegroup, so the script is their
only check).

## Deferred to the packaging unit (item 3) — record, do not act

1. `crates/wolf-gnome/src/app.rs::init_lua_path` bakes `env!("CARGO_MANIFEST_DIR")`, so a
   Nix-built binary would carry an absolute path into an ephemeral build sandbox. The
   derivation needs a real resolution path (a `WOLF_INIT_LUA`-style override, which
   `wolf-gnome` does not read today, or a store path baked at build time) — that is a
   **source change in `wolf-gnome`**, not packaging-only.
2. The whole runtime environment is shellHook-supplied and must be re-supplied by a wrapper
   (`wrapGAppsHook4`/`makeWrapper`), plus whatever WebKit needs to find its helper
   processes.
3. `FONTCONFIG_FILE` is a render-determinism device pointing at a per-shell `mktemp` file,
   not a shipping strategy.
4. `buildRustPackage` defaults to `--release`, so the derivation inherits
   `debug = "line-tables-only"` and must decide whether to split it into a `debug` output
   or drop it. `split-debuginfo` stays embedded here specifically to keep that open.
5. The `ldd` result above.

## Implementation order

1. `df -h`; confirm room beside the existing 28 GB `target/`.
2. Edit `Cargo.toml`.
3. Add `crates/wolf-gnome/tests/release_profile.rs`.
4. `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings`.
5. `cargo test -p wolf-gnome --test release_profile` — green in the default suite.
6. Failure evidence for tests 1 and 2 (two one-line manifest reverts, captured, reverted).
7. `time cargo build --release`; record wall clock and `ls -l`.
8. `cargo test --release -p wolf-gnome --test release_profile -- --nocapture` — the printed
   path is the evidence `--release` retargeted the binary.
9. Failure evidence for test 3 under `--release` (`debug = false`, rebuild, capture, second
   `ls -l`, restore, rebuild).
10. The two smokes under `--release`.
11. `ldd` from a plain shell.
12. Docs; `prettier --write`; `markdown_check.sh`.
13. The full Linux verification gate, verbatim from `build.md`.

## Verified evidence

Toolchain `cargo`/`rustc 1.96.0`, LLVM 22.1.2, `x86_64-unknown-linux-gnu`.

| claim                                    | command                                                                | result                                                                       |
| ------------------------------------------ | ------------------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| Tests 1 and 2 fail with no profile block | `cargo test -p wolf-gnome --test release_profile`                      | 2 FAILED, test 3 ok (pre-implementation red)                                 |
| Test 1 falsifier                         | `panic = "abort"`                                                      | FAILED, `left: Some("abort")`                                                |
| Test 2 falsifier                         | `strip = "symbols"`                                                    | FAILED                                                                       |
| Test 3 falsifier A                       | `debug = false`, release                                               | assertion 1 green (271,586), assertion 2 FAILED at the needle                 |
| Test 3 falsifier B                       | `strip = "debuginfo"`, release                                         | assertion 1 FAILED first (`.debug_line` absent)                              |
| Optimized artifact is the one tested     | `cargo test --release … --nocapture`                                   | path `…/target/release/wolf-gnome`, needle hit in `.debug_str`               |
| **The optimized binary actually runs**   | `cargo test --release … --test smoke_headless` (2 cage smokes)         | both ok, 21.31 s, first try, no flake                                        |
| Gate unaffected                          | the full verification gate                                             | exit 0, 1036 passed; every `Finished` reads `dev`/`test`; 10m9.048s vs 10m8.964s |
| Docs formatted                           | `prettier --check "**/*.md"`                                           | exit 0                                                                       |

The smoke row is the load-bearing one: it launches the optimized binary under `cage`,
types into the live captive Neovim, screenshots and OCRs the text back, and the second
smoke additionally spawns the WebKit helper processes. That is the artifact demonstrably
working, not merely compiling.

**What stays unproven.** No runtime measurement exists on this platform — the GTK
front-end carries no frame-time instrumentation, so no speed claim is made anywhere. The
macOS half is unverifiable from this host: `//:Cargo.toml` is a `crate_universe` manifest
input, so the repository rule re-runs, and `macos-followups.md` carries the row asking a
macOS pass to confirm `Cargo.Bazel.lock` does not move.

## Status

- [x] Branch cut
- [x] Architect plan
- [x] Tests written, then repaired after the failure-evidence step exposed a hollow test
- [x] Implementation + tests green
- [x] Failure evidence captured for all three tests, both falsifiers per test where two exist
- [x] Docs
- [x] Review — approved with minor notes, all four acted on
- [x] feat-list row ticked

## What the review changed

The reviewer caught one real accuracy defect and three smaller ones, all fixed:

- `build.md` labelled 31.3 MiB as the line tables. It is the whole retained debug-info
  delta; the line tables are about 5.7 MiB. Relabelled, paragraph split to stay inside the
  length ceilings.
- `build.md` asserted that a profiler and a release backtrace "resolve to a `file:line`",
  which nobody observed. It states the requirement instead.
- `build.md` called the two release commands "guards"; the cage smokes guard nothing, they
  are the evidence the binary runs.
- `build.md` restated the `panic` mechanism four lines after naming `Cargo.toml` its owner.
  The mechanism is cut; the workflow fact stays.
- `release_profile.rs` claimed `strip = "symbols"` as a falsifier where `"debuginfo"` was
  measured, and `DEBUG_OFF_VALUES` omitted `"line-directives-only"`, which cargo accepts
  and which would have passed test 2. Both fixed, the latter with its own captured red.

Two durability additions the review prompted: `crates/wolf-gnome/DECISIONS.md` records that
`init_lua_path` bakes `env!("CARGO_MANIFEST_DIR")`, and `feat-list.md` states the resulting
limit so the ticked row cannot be read as a shippable artifact.
