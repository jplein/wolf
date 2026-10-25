# Plan: hermetic real-nvim tests (no user HOME / Wolf state)

## Status

COMPLETE (uncommitted). Reviewer approved with minor notes (funnel is by convention; leaked per-binary tempdir; Linux arm of state_isolation unrun). wolf-gnome leak recorded Open in notes/macos-followups.md.

## Request (from the user)

"The test should not use my real configuration file in HOME. The failing test should be modified to be hermetic."

Failing test: `//crates/wolf-ffi:search` fails because `~/Library/Application Support/Wolf/runtime_user/init_user.lua` (the user's real file) sets `ignorecase`. Passes with `--test_env=HOME=<scratch>`. Logged Open in `notes/macos-followups.md` (row "Test-hermeticity defect").

## Known facts (orchestrator, read from code)

- `crates/wolf-core/src/state.rs:208` `state_dir()`: `WOLF_STATE_DIR` override first; else macOS `dirs::data_dir()/Wolf`, Linux `dirs::state_dir()/wolf`. `runtime_user_dir` etc. derive from it.
- Bazel strips `HOME` from test env unless `env_inherit`; `dirs` then falls back to getpwuid, i.e. the real home. Real-nvim tests are `no-sandbox`.
- `cargo test` on Linux inherits the user's real HOME, so a Bazel-only fix leaves Linux non-hermetic.
- `crates/wolf-core/tests/swap_recovery.rs` `home_cwd()` needs a cwd under `$HOME` (`env_inherit = ["HOME"]` in `crates/wolf-core/BUILD.bazel:920`).

## Scope

At minimum `//crates/wolf-ffi:search` must not read or write the user's real Wolf state. Preferred: every real-nvim test, both platforms (Bazel and cargo), via one shared mechanism. Close the hermeticity row in `notes/macos-followups.md` with measurements.

## Decisions (orchestrator)

- Accept the architect's mechanism: `crates/wolf-ffi/tests/common/mod.rs` gains `ensure_hermetic_state_dir()` (OnceLock, set `WOLF_STATE_DIR` to a `wolf-ffi-test-state-*` tempdir only if absent), called first in `nvim_bin_or_skip()`. No HOME change, no production API change, no Bazel `env`.
- New guard `crates/wolf-ffi/tests/state_isolation.rs` + `rust_test(name = "state_isolation")` in `crates/wolf-ffi/BUILD.bazel`: parent re-execs its own binary with a scratch HOME holding a planted `<home>/<platform default>/runtime_user/init_user.lua` (`vim.o.ignorecase = true`). `isolated_payload` (ignored, gated on `WOLF_TEST_PLANTED_HOME`) asserts preconditions, that `state_dir()` moved off the plant, `!ignorecase`, and no writes under the plant. `control_payload` points `WOLF_STATE_DIR` at a second plant and asserts `ignorecase` (non-vacuity) and set-if-absent. Parent checks each child status success and stdout `1 passed`.
- wolf-core tests are already hermetic (`ensure_hermetic_state_dir` in its common). swap_recovery's `$HOME` tempdir stays.
- wolf-gnome (`tests/common/mod.rs:92-100`, Linux cargo only) leaks too but cannot compile on this macOS host: deferred, recorded as an explicit Open item in the closing row.
- Docs: `crates/wolf-ffi/tests/README.md` (harness bullet + `state_isolation.rs` entry), `notes/wolf-ffi-decisions.md` Testing posture paragraph, rewrite `notes/macos-followups.md` hermeticity row in place. No `build.md` change. No SHA bumps unless whole file re-checked.
- Never write to the user's real `~/Library/Application Support/Wolf`. Don't run `:search` or `//...` with F1 applied.

## Falsifiers

- F1: delete the `ensure_hermetic_state_dir();` call -> `state_isolation` red with "the wolf-ffi harness left state_dir() at …".
- F1 + drop step-4 assert -> "captive nvim sourced the planted …"; + drop step 5 -> lists written entries.
- F4: `PLANT_BODY` ignorecase=false -> control payload red.
- F5: typo in `ISOLATED_PAYLOAD` -> parent's `1 passed` check red.
- search fix: F1 with `--test_env=HOME=<scratch plant>` -> `:search` red.

## Verification

- marker file, `bazel test //... --nocache_test_results` -> 114/114 with the user's real init_user.lua in place; `find "$HOME/Library/Application Support/Wolf" -newer marker` empty (caveat: running Wolf.app).
- `bazel test //tools/lint:markdown //tools/lint:comments`; prettier on edited md.

## Progress log

- Architect plan accepted (full text in the architect report; summary above).
- Test-writer: `state_isolation.rs` + BUILD target. Coder: RED before fix ("the wolf-ffi harness left state_dir() at …/Library/Application Support/Wolf"); fix in `crates/wolf-ffi/tests/common/mod.rs`; GREEN; F4, F5, F1b, F1c, search falsifier all red as expected; `//... --nocache_test_results` 114/114; real-dir `find -newer marker` empty.
- Done: rustfmt (2 reflows + trailing commas, aspect clean on :state_isolation, :search); docs (tests README, wolf-ffi-decisions, macos-followups rows 69/70); harness doc-comment fixes (code unchanged by checksum); lints + prettier clean. Reviewer approved.
- Was open: `state_isolation.rs` not rustfmt-clean (Bazel rustfmt covers libs only; Linux `cargo fmt --check` would fail) -> coder reformatting. Docs -> technical-writer.

---

# Previous task (complete)

# Plan: macOS (Bazel) build in line with the Linux merge

## Status

COMPLETE (uncommitted). `bazel build //crates/wolf-macos:Wolf` and `WolfRelease` build. `bazel test //...` is 112/113; the only failure is `//crates/wolf-ffi:search`, a pre-existing hermeticity defect caused by the user's `init_user.lua`, logged in `notes/macos-followups.md`. Reviewer issues are applied and the docs are updated. Pending user: replace `notes/.bazelrc.md`'s placeholder `Last verified` SHA at commit time.

## Request (from the user)

1. Match the macOS Neovim runtime version to Linux.
2. Fix `bazel build //crates/wolf-macos:Wolf` (`error[E0463]: can't find crate for thiserror_impl` compiling rules_rust's internal `thiserror` [for tool]).
3. Update any other dependency versions that changed on the Linux side.

## Diagnosis (measured by the orchestrator)

- The failing crate is rules_rust's internal `thiserror` (exec config), reached via `@crates//:swift-bridge` -> `cargo_toml_env_vars` -> `cargo_toml_variable_extractor`.
- `dlopen` of the built `libthiserror_impl-*.dylib` fails on macOS 27.0.1 / Xcode 27.0: `mis-aligned LINKEDIT string pool`. rustc reports that as E0463.
- Exec (opt) proc-macros use `-Cstrip=debuginfo`. rustc runs `strip` from `PATH`. With `PATH=/usr/bin:/bin` the stripped dylib loads; with the Nix shell's `strip` (llvm-strip from clang-wrapper 21.1.8) it does not. `/usr/bin/strip -S` and `xcrun strip -S` are fine; `rust-objcopy --strip-debug` is not. Same on rustc 1.85.0 and 1.96.0.
- `flake.nix` `darwinShell = pkgs.mkShell` brings the Darwin stdenv's cc-wrapper, cctools and an xcbuild `xcrun` onto `PATH`; Bazel actions inherit `PATH`. Exposed by `b172196`'s nixpkgs bump (`9ae611a` -> `55bfb4d`).

## Decisions

- **D1. `darwinShell` becomes `pkgs.mkShellNoCC`.** The shell needs only bazelisk, the shim, git, starpls; the C toolchain is Xcode via apple_support/`DEVELOPER_DIR`. Also protects `bazel run` fixers (client env) that call `xcrun`.
- **D2. `.bazelrc` gains `build --incompatible_strict_action_env`.** Actions get `/bin:/usr/bin:/usr/local/bin`, so no client PATH can reach rustc's strip again; action keys stop depending on the shell. Risk: `tools/lint/markdown_check.sh` runs under BSD `/usr/bin/awk` instead of gawk — `//tools/lint:markdown_check_test` will show it; if it breaks, make the awk portable, do not drop the flag. Rejected: `extra_exec_rustc_flags=-Cstrip=none` (covers only exec rustc), `--action_env=PATH=` (non-standard).
- **D3. Rust stays at 1.85.0.** It did not change in this merge; 1.96 has the same strip failure; a bump forces a `CARGO_BAZEL_REPIN`, new clippy lints at `-Dwarnings`, and may lack rules_rust 0.71.2 shas. Contingency: if Wolf code fails to compile on 1.85 because Linux-side code needs newer Rust, stop and report.
- **D4. Version labels become version-free** ("the pinned Neovim") except `_VERSION` in `bazel/nvim/extensions.bzl` and `nvim_version_test.sh`. Measured claims keep an explicit version ("measured on Neovim 0.12.6", or the original version if not re-measured).

## Upstream facts

**Neovim v0.12.6** — asset names unchanged. GitHub digests (cross-check only; coder must `shasum -a 256`):
- `nvim-macos-arm64.tar.gz` `1dbd148222b051ba6c307c4d17a318ed083df018a4f6a5a0ce46955ffc9ab87b`
- `nvim-macos-x86_64.tar.gz` `481424f1dbc85f637d57e47d27d042aa9c883e5a4fe9ee64fc4ee9573b221998`
- Bundled tree-sitter v0.26.13: grammar ABI range 13–15 (`grep -m1 'define LANGUAGE_VERSION' src/parser.c`).

**nvim-treesitter `e289100ff98969e118c702199d88b764ce9e7fdf`** — `runtime/queries/`, `lua/`, `plugin/`, `LICENSE` present; markdown injections use only `#set!`/`#offset!`; no `lockfile.json` (revisions in `lua/nvim-treesitter/parsers.lua`).

**Grammar revisions at `e289100`** (all commit `src/parser.c`; scanners C; scanner booleans unchanged):

| lang | repo | rev | change? |
| --- | --- | --- | --- |
| bash | tree-sitter/tree-sitter-bash | `a06c2e4415e9bc0346c6b86d401879ffb44058f7` | yes |
| c | tree-sitter/tree-sitter-c | `b780e47fc780ddc8da13afa35a3f4ed5c157823d` | yes |
| cpp | tree-sitter/tree-sitter-cpp | `c009222808634c1014f82438d4883753516a2c24` | yes |
| css | tree-sitter/tree-sitter-css | `dda5cfc5722c429eaba1c910ca32c2c0c5bb1a3f` | yes |
| go | tree-sitter/tree-sitter-go | `2346a3ab1bb3857b48b29d779a1ef9799a248cd7` | yes |
| html | tree-sitter/tree-sitter-html | `73a3947324f6efddf9e17c0ea58d454843590cc0` | yes |
| java | tree-sitter/tree-sitter-java | `e10607b45ff745f5f876bfa3e94fbcc6b44bdc11` | yes |
| javascript | tree-sitter/tree-sitter-javascript | `58404d8cf191d69f2674a8fd507bd5776f46cb11` | yes |
| json | tree-sitter/tree-sitter-json | `254c42a6476413b776221e03982ac8ae159eeb72` | yes |
| kotlin | fwcd/tree-sitter-kotlin | `1852ea17b7f60fb3f9d84e0b1555d56b46b39fb1` | yes |
| rust | tree-sitter/tree-sitter-rust | `77a3747266f4d621d0757825e6b11edcbf991ca5` | yes |
| swift | alex-pinkus/tree-sitter-swift | Linux `35245fb…` has no parser.c (`generate = true`); pin `82bb3a533e0801fd2bbaa11dc49676e10bf41948` (tag `0.7.4-with-generated-files`) | special |
| toml | tree-sitter-grammars/tree-sitter-toml | `64b56832c2cffe41758f28e05c756a3a98d16f41` | no |
| typescript, tsx | tree-sitter/tree-sitter-typescript | `75b3874edb2dc714fb1fd77a32013d0f8699989f` | no |
| yaml | tree-sitter-grammars/tree-sitter-yaml | `a1c4812a73ec5e089de8e441fdea3a921e8d5079` | yes |

Swift: the current pin `31d17fe` (`0.7.3-with-generated-files`) predates the `nil_literal` node that `e289100`'s `queries/swift/highlights.scm` uses, so it should fail the new queries. `82bb3a5`'s `src/` also holds `parser_abi13.c`/`parser_abi14.c`, which the overlay glob never compiles.

## Steps

### Step 0 — preflight (coder, no edits)

- `git diff --stat MODULE.bazel.lock` (a pre-existing modification from the earlier failing build is expected; commit with this work).
- Record `nix eval --raw .#devShells.aarch64-darwin.default.drvPath` before the change.

### Step 1 — build fix (coder)

Files: `flake.nix`, `.bazelrc`, `notes/flake.nix.md`.

- `flake.nix`: `darwinShell = pkgs.mkShellNoCC { … }` (packages and shellHook unchanged). Two-line comment above it, ending `See notes/flake.nix.md#darwin-shell`.
- `.bazelrc`: `build --incompatible_strict_action_env` after the macOS block, with a two-line comment (≤100 cols, no link) giving the reason.
- `notes/flake.nix.md`: add `## Darwin shell` in this same step (doc-lint checks the anchor): measured root cause, `mkShell` vs `mkShellNoCC`, and that `.bazelrc`'s strict action env is the Bazel half.
- Re-enter the shell and check `command -v strip cc clang ld xcrun`, `xcrun --find swift-format`, `SDKROOT`, `DEVELOPER_DIR`: nothing under `/nix/store`, `SDKROOT` unset. Report rather than adding shellHook PATH surgery if something still resolves to Nix.
- `bazel build //crates/wolf-macos:Wolf` (full rebuild expected).

### Step 2 — Neovim 0.12.6 (coder; test edit by test-writer)

Files: `bazel/nvim/extensions.bzl`, `bazel/nvim/nvim_version_test.sh`, `bazel/nvim/BUILD.bazel`, `MODULE.bazel`.

- `extensions.bzl`: `_VERSION = "0.12.6"`, both shas via `shasum -a 256` (cross-check above), confirm archive roots with `tar tzf | head -1`; docstring line 1 and `doc =` version-free.
- `nvim_version_test.sh` (test-writer): one `EXPECTED_VERSION="0.12.6"` used by the `grep -F "NVIM v$EXPECTED_VERSION"` and messages; comments and final echo version-free.
- `BUILD.bazel` lines 1 and 16, `MODULE.bazel` line 70: comments version-free.
- Run `bazel test //bazel/nvim:nvim_version_test`.

### Step 3 — nvim-treesitter `e289100` (coder)

Files: `bazel/nvim-treesitter/extensions.bzl`, `MODULE.bazel`.

- `_COMMIT = "e289100ff98969e118c702199d88b764ce9e7fdf"`, `_SHA256` from the archive tarball.
- Drop `lockfile.json` from the overlay exclusion comment.
- The `master` `#set-lang-from-info-string!` failure claim: reword as "measured on Neovim 0.12.3", or re-measure on 0.12.6 (optional). Never claim "every later release" unmeasured.
- `MODULE.bazel` line 95 comment version-free.
- Run `bazel test //bazel/nvim-treesitter:nvim_treesitter_tree_test`.

### Step 4 — new test `//bazel/treesitter-parsers:queries_compile_test` (test-writer; run by coder before Step 5)

Files: new `bazel/treesitter-parsers/queries_compile_test.sh`, new `bazel/treesitter-parsers/queries_compile_probe.lua`, `bazel/treesitter-parsers/BUILD.bazel`.

- Shell (`#!/bin/sh`, `set -eu`): find in runfiles the nvim binary (`*nvim_macos_*/nvim/bin/nvim`, `find -L` fallback), the nvim-treesitter tree (`*/nvim_treesitter_tree/nvim-treesitter`), the parser dir (`*/parsers_tree/parsers/parser`), reusing sibling tests' probing patterns. Enumerate `parser/*.so` (fail if none). XDG dirs under `$TEST_TMPDIR`. Run `"$NVIM" --clean -n -i NONE --headless -l <probe> <ts_dir> <parser_dir> <lang…>` (fallback: env vars + `-c 'luafile …'`).
- Lua probe (separate file — doc-lint lexes `.sh`): prepend `ts_dir` to `runtimepath`; per lang, `pcall(vim.treesitter.language.add, lang, {path=…})`, then `vim.treesitter.query.get(lang, "highlights")` must succeed and be non-nil; if `get_files(lang, "injections")` is non-empty, injections must compile. Print `PASS <lang>` / `FAIL <lang>: <err>`; `os.exit(1)` on any failure.
- `BUILD.bazel`: `sh_test(name = "queries_compile_test", srcs = ["queries_compile_test.sh"], args = ["$(rootpath :queries_compile_probe.lua)"], data = [":parsers_tree", ":queries_compile_probe.lua", "//bazel/nvim:nvim_tree", "//bazel/nvim-treesitter:nvim_treesitter_tree"])`. Add `tags = ["no-sandbox"]` only if needed.
- Coder runs it against old grammar pins + new queries and records which languages fail (swift expected): red-before-green evidence.

### Step 5 — re-pin grammars (coder)

File: `bazel/treesitter-parsers/extensions.bzl`.

- Each changed row: new rev + recomputed sha256; scanner booleans unchanged. yaml: update both the `_archive` rev and the `_yaml_build_file("tree-sitter-yaml-<rev>")` argument. toml and typescript unchanged.
- Swift: pin `82bb3a5…`; diff its `src/scanner.c` against `35245fb`'s / `171fa3b`'s to confirm the cwe-476 scanner fix and record the result; rewrite the line 184–185 comment.
- Docstring: lines 14–17 re-measure parsers-with-highlights in the 0.12.6 macOS tarball (seven expected) and state "measured with `--clean` against the pinned Neovim 0.12.6"; lines 46–50 provenance is `parsers.lua` at the `//bazel/nvim-treesitter` commit.
- Per grammar: `bazel build @ts_<lang>//:<lang>.so`, check `LANGUAGE_VERSION` 13–15.
- Fallbacks per grammar, never blocking the rest: (1) nearest later compiling commit that passes `queries_compile_test`; (2) keep the old pin if it passes; (3) drop+log (extensions, `MODULE.bazel` `use_repo`, `BUILD.bazel` srcs, README table, ledger note) — never drop go, rust, typescript, tsx, json; (4) swift only: keep `82bb3a5` with an explicit commented expected-failure list, recorded in the README.
- Run `bazel test //bazel/treesitter-parsers:parsers_tree_test //bazel/treesitter-parsers:queries_compile_test`.

### Step 6 — version labels and re-measured claims (coder)

Files: `runtime/nvim/init.lua`, `crates/wolf-ffi/tests/treesitter_parser_abi.rs`, `crates/wolf-core/tests/markdown_cursor.rs`, `crates/wolf-core/tests/nav_history_live.rs`, `runtime/nvim/README.md`.

- Relabel: `init.lua:143` -> `-- vim.uv since Neovim 0.10; vim.loop fallback`; `treesitter_parser_abi.rs` line 6 (doc) and line 215 (assertion string) -> "the pinned Neovim"; `nav_history_live.rs:1615` -> "(nvim >= 0.10, so present in the pinned Neovim)".
- Re-measure on 0.12.6 with the Bazel-built nvim (`$(bazel cquery --output=files //bazel/nvim:nvim_tree)/nvim/bin/nvim --clean --headless`): `runtime/nvim/README.md:303` (background `nvim_win_set_buf` fires BufEnter/BufWinEnter, focus restored); `markdown_cursor.rs:702` (`bufload()` fires BufEnter/BufWinEnter in an autocmd window at 1/1); `nav_history_live.rs:1299` point 1 (`:e link/doc.md` via symlinked dir gives the real path). Write "measured on Neovim 0.12.6"; any disagreement is a behaviour change — stop and report.

### Step 7 — full verification (coder)

1. `env -i HOME=$HOME PATH=/usr/bin:/bin:/nix/var/nix/profiles/default/bin nix develop --command bash -c 'command -v strip cc clang ld xcrun bazel; echo "SDKROOT=${SDKROOT-unset}"'`
2. Same wrapper: `bazel build //crates/wolf-macos:Wolf`.
3. Same wrapper: `bazel test //...`, then `bazel test //bazel/treesitter-parsers:queries_compile_test --test_output=all`.
4. Recommended once: `bazel build //crates/wolf-macos:WolfRelease`.
5. `nix eval --raw .#devShells.aarch64-darwin.default.drvPath` (expect a new path; record it), `nix develop --command true`.
6. `git diff --stat Cargo.lock Cargo.Bazel.lock` empty (if `Cargo.Bazel.lock` moved: `CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-core:wolf_core`); `MODULE.bazel.lock` rewritten — keep; `flake.lock` unchanged.
7. `prettier --write` each edited `.md`, then `./tools/lint/markdown_check.sh <files>`.

### Step 8 — docs (technical writer)

See "Docs to update".

## Test plan and falsifiers

1. `//bazel/nvim:nvim_version_test` (edited) — falsifier: revert `_VERSION` + shas to 0.12.3 while the test expects 0.12.6; expect "version mismatch".
2. `nvim_treesitter_tree_test` (unchanged) — falsifier: drop `"{archive_root}/lua/**"` from the overlay glob.
3. `parsers_tree_test` (unchanged) — falsifier: drop `@ts_go//:go_parser` from `parsers_tree` srcs.
4. `queries_compile_test` (new) — primary falsifier: re-pin `ts_swift` to the old `31d17fe…` pin; expect `FAIL swift … nil_literal`. Secondary: drop `"{archive_root}/runtime/queries/**"` from the nvim-treesitter overlay glob. Also record the Step 4 red run.
5. `//crates/wolf-ffi:treesitter_parser_abi`, `treesitter_highlight`, `code_block_highlight`, `//crates/wolf-core:parsers_user` (existing; message-string edit only).
6. `//crates/wolf-macos:bundle_layout_test` (existing).
7. Build-fix guard, no new test; demonstrate each half:
   - (a) `CCW=$(nix build --no-link --print-out-paths --inputs-from . nixpkgs#stdenv.cc)`. With the `.bazelrc` flag removed, `PATH="$CCW/bin:$PATH" bazel build //crates/wolf-ffi:wolf_ffi` -> E0463; restored -> succeeds.
   - (b) `darwinShell` temporarily `mkShell`: `nix develop --command command -v strip` prints a `/nix/store/…clang-wrapper…` path; with `mkShellNoCC` it does not. Use a temporary edit, never bare `git stash`.
8. `bazel test //...` including `//tools/lint:markdown_check_test`, `:markdown`, `:comments` (BSD awk risk from D2).

## Docs to update (technical writer)

Edit in place, prefer net-negative, keep labels version-free (D4).

- `bazel/nvim/README.md`: intro and Targets version-free; bump-steps example tag; decision that `_VERSION` tracks the Linux lock's Neovim, with check command `nix eval --raw --inputs-from . nixpkgs#legacyPackages.x86_64-linux.neovim-unwrapped.version`; only `_VERSION` and the test name the version.
- `bazel/nvim-treesitter/README.md`: pin section (commit, URL, sha, root); relabel the 0.12.3 claim as a measurement; delete the "not version-matched / future nicety" paragraph; rewrite the stale "never sourced" / "Linux vendoring deferred" lines to link `runtime/nvim/README.md`; drop `lockfile.json`; add `queries_compile_test` to bump steps.
- `bazel/treesitter-parsers/README.md`: lines 11, 13, 17, 81; provenance (`parsers.lua` at `e289100`); scanner claim; swift note and status row; "Adding a language" step 1; Targets gains `:queries_compile_test` and its probe.
- `build.md`: the Darwin shell paragraph (~line 196) says "no C toolchain" and links `notes/flake.nix.md#darwin-shell`; the bump re-run list (~line 352) gains `queries_compile_test`.
- `notes/flake.nix.md`: finish `## Darwin shell`; rewrite where "Single nixpkgs input" and "Treesitter plugin tree" say the platforms can differ (matched by hand at this lock; a `nix flake update` that moves them owes the `bazel/` bump). Bump `Last verified` only after re-checking the whole file, against a commit containing the code changes.
- `notes/macos-followups.md`: row for 2026-10-24 `feat/single-nixpkgs-pin` -> Verified if Step 7 passes; rows wanting the Darwin spot check -> Verified; 2026-08-12 `linux-feat/nixos-package` item (c) rewritten in place (row stays Open for (b), (d)); check the remaining 2026-10-24 row's items. Do not bump `Last verified` unless the whole file is re-checked.
- Leave `notes/treesitter-decision-report.md` and `notes/completions-decision-report.md` unchanged (reports).

## Lint considerations

- doc-lint: non-doc comments ≤2 lines, ≤100 cols from the marker (`flake.nix`, `.bazelrc`, `.bzl`, `BUILD.bazel`, `.sh`, `.lua`); every `notes/…#anchor` in a comment must exist in the same change.
- Markdown checker: no changelog words outside `notes/`, no ALL-CAPS runs, `notes/` header line, `prettier --write`.
- No lint-ignore directives. No bare `git stash`.

## Progress log

### Correction to the diagnosis (measured by the coder, Step 1)

- With `mkShellNoCC` + `--incompatible_strict_action_env` (action PATH `/bin:/usr/bin:/usr/local/bin`, confirmed by aquery), the build still failed with E0463 and the same `mis-aligned LINKEDIT string pool, fileOffset=0x0017C2EC`.
- A logging `strip` shim first on PATH was never invoked by the exec rustc action. Running rustc's bundled `rust-objcopy --strip-debug` on the unstripped dylib reproduces the identical offset. `/usr/bin/strip -S`, `xcrun strip -S` and `-Cstrip=none` all produce loadable dylibs.
- So the cause is rustc's own `rust-objcopy` strip of exec proc-macros, whose output macOS 27 dyld rejects. The PATH theory above is wrong. (The orchestrator's clean-PATH test also changed the default linker from the Nix cc-wrapper to `/usr/bin/cc`, which is the likely confound.)
- **Fix (supersedes D2's rejection):** `.bazelrc` `build --@rules_rust//rust/settings:extra_exec_rustc_flags=-Cstrip=none`. F7(a1): removing just that line reproduces E0463; restoring it builds.
- D1 (`mkShellNoCC`) is kept for its own reason, measured in F7(b): under `mkShell` the shell resolved `strip`/`cc`/`ld` to clang-wrapper 21, `xcrun` to xcbuild's (about 18 `unhandled Platform key` warnings on `xcrun --find swift-format`), and `SDKROOT` to a Nix apple-sdk-14.4. Under `mkShellNoCC` all are `/usr/bin`, and SDKROOT is unset.
- D2 (`--incompatible_strict_action_env`) is kept: it is green across `bazel test //...` (BSD awk is fine), and it makes action keys and tool resolution independent of the shell. It is not what fixes the E0463, and docs must not say it is.

### Coder Step 0–7 results

- Pins: Neovim 0.12.6 (shas match GitHub); nvim-treesitter `e289100` sha `f4b626bb…`; all grammars re-pinned per the table with no fallbacks; swift `82bb3a5` (scanner byte-identical to `35245fb`). ABI 14–15 everywhere.
- `queries_compile_test`: red on the old pins (`FAIL kotlin … "..<"`, `FAIL swift … "nil_literal"`, 2/16), green on the new ones (16/16). It needed `tags = ["no-sandbox"]`: under the sandbox, the probe's realpath prefix check never matches.
- Step 6 re-measurements all hold on 0.12.6. `init.lua:143` and `runtime/nvim/README.md:303` were relabeled by the coder; the test-file relabels are owed to the test-writer.
- `bazel test //...`: 112/113 pass. `//crates/wolf-ffi:search` fails because the user's real `~/Library/Application Support/Wolf/runtime_user/init_user.lua` sets `ignorecase`; with `--test_env=HOME=<scratch>` it passes. This test-hermeticity defect predates this work and is out of scope; report it to the user.
- Falsifiers F1, F2, F3, F4 (primary and secondary), F7(a1) and F7(b) were captured. F7(a2) does not fail, which is consistent with the correction.
- Lockfiles: `MODULE.bazel.lock` rewritten; `Cargo.lock`, `Cargo.Bazel.lock` and `flake.lock` unchanged.

### Round 2 (done)

- The test-writer relabeled the test files and made `any_from_ts_tree` compare unresolved, normalized paths; `no-sandbox` is removed.
- Sandboxed run: `queries_compile_test` PASSED, `SUMMARY 0/16 languages failed`, with paths under `…/sandbox/darwin-sandbox/2368/…`.
- Vendored-tree falsifier, excluding `{archive_root}/runtime/queries/c/**` from the overlay: `FAIL c: no highlights query under … (resolved: …/nvim_tree/nvim/share/nvim/runtime/queries/c/highlights.scm)`, `SUMMARY 1/16`. Restored, it PASSED.
- Re-ran `treesitter_parser_abi`, `nav_history_live`, `markdown_cursor`, `//tools/lint:comments` and `:clippy`: 5/5 PASS.
- `bazel test //...`: 112/113. The only failure is the known `//crates/wolf-ffi:search` (the user's `init_user.lua` sets `ignorecase`).
- `WolfRelease` builds (round 1).

### Review (round 1): verdict "needs revision", no build-breaking defects

M1 (missing evidence) is answered by Round 2. Fixes routed:

- Coder:
  - L1: `bazel/nvim-treesitter/extensions.bzl:15-17,63`, a stale "never sourced / highlighting-disabled" claim. `init.lua:189-191` sources `plugin/nvim-treesitter.lua` and calls `setup({install_dir=…})`.
  - L2: `bazel/treesitter-parsers/extensions.bzl:16` is a mis-wrapped line of about 150 columns.
  - L3: swift "matching tag" wording. Verify `grammar.js` identity or reword it to "nearest".
  - N1: `.bazelrc:21` wording.
  - N3: `bazel/nvim/extensions.bzl:20` should say "change `_VERSION`".
- Test-writer: N2 (`nav_history_live.rs` rewrap).
- Declined: N4 (harmless message on a probe usage error).

### Final round

- The tech writer moved the E0463 rationale to `notes/.bazelrc.md#exec-strip`, and `.bazelrc` links it.
- Bazel `*` skips dotfiles. The root `markdown_srcs` gains `notes/**/.*.md`, which brings the `notes/` label count from 44 to 45, matching the 45 files on disk.
- Final `bazel test //...` is 112/113, with only the known `search` failure. Prettier, `markdown_check.sh`, `:markdown`, `:comments` and the standalone `doc-lint --root .` are all clean.
