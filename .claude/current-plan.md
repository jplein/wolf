# Plan: hermetic wolf-gnome real-nvim tests (Linux Wolf state dir)

## Status

COMPLETE (uncommitted). Reviewer approved; docs updated; all lints and tests green.

## Request

The previous commit (b8ca389, made on macOS) made wolf-ffi's real-nvim tests hermetic. The Linux tests still use `~/.local/state/wolf`. Make wolf-gnome's real-Neovim integration tests hermetic with respect to Wolf's state directory. Do not commit.

## Verified facts (orchestrator, measured on this Linux host)

- `nix develop --command env -u XDG_STATE_HOME -u XDG_DATA_HOME -u XDG_CONFIG_HOME HOME=<scratch>/home CARGO_HOME=/home/jplein/.cargo cargo test --workspace --no-fail-fast`: all green; created `<scratch>/home/.local/state/wolf/{folder-mru.sqlite,command-mru.sqlite,file-mru.sqlite,swap/<8 dirs>,spell,parsers_user,runtime_user/init_user.lua}`, first appearing during `crates/wolf-gnome/tests/im_commit_nvim.rs`.
- Cause: `crates/wolf-gnome/tests/common/mod.rs` `config_in` sets no `WOLF_STATE_DIR`. Affected: `im_commit_nvim.rs`, `mouse_nvim.rs`, `roundtrip_nvim.rs`.
- No other crate leaked. wolf-ffi's `state_isolation.rs` passes on Linux.
- `notes/macos-followups.md` row at line 70 records this as `Open`.

Scratchpad: `/tmp/claude-1000/-home-jplein-dev-jplein-wolf--worktrees-feat-documentation-checks-and-updates/c4a78c07-e57e-4977-a288-662a2a1c9cef/scratchpad`

## Decisions

### D1. Temp dir made with `std`, not `tempfile`

`tempfile` is in `Cargo.lock` (wolf-core dep, wolf-ffi dev-dep), but adding it as a wolf-gnome dev-dependency adds a `"tempfile"` line to the `wolf-gnome` entry in `Cargo.lock` (dev-deps are listed; see wolf-ffi's entry). `//:Cargo.lock` is crate_universe's `cargo_lockfile` (MODULE.bazel:55), so that creates a `CARGO_BAZEL_REPIN` obligation a Linux host cannot discharge — the reason in `release_profile.rs` lines 21–26. `crates/wolf-gnome/Cargo.toml` and `Cargo.lock` must not change. wolf-ffi's static `TempDir` is never dropped anyway, so a hand-made dir behaves the same.

### D2. Helper shape in `crates/wolf-gnome/tests/common/mod.rs` (coder)

Mirror wolf-ffi's `OnceLock<Option<…>>` variant (not wolf-core's check/init/re-check).

- Rename `CWD_COUNTER` → `TEMP_DIR_COUNTER`.
- Add `pub fn unique_temp_dir(prefix: &str) -> PathBuf`: `std::env::temp_dir().join(format!("{prefix}-{pid}-{nanos}-{n}"))`, created with `std::fs::create_dir` (exclusive), returned.
- `fresh_cwd()` = `unique_temp_dir("wolf-test-cwd")` (name format unchanged).
- `static HERMETIC_STATE_DIR: OnceLock<Option<PathBuf>>`; `None` = caller had set `WOLF_STATE_DIR`, left alone.
- private `fn ensure_hermetic_state_dir()`: inside `get_or_init`, if `WOLF_STATE_DIR` is set return `None`; else `unique_temp_dir("wolf-gnome-test-state")`, `std::env::set_var("WOLF_STATE_DIR", &dir)`, `Some(dir)`. Edition 2021: no `unsafe`.
- `config_in`: call `ensure_hermetic_state_dir()` right after `require_nvim_bin()`, before the `CoreConfig` literal. It is the only funnel (all three `WolfCore::start` calls use `config`; `smoke_headless.rs` doesn't use `mod common`).
- Doc comments on `HERMETIC_STATE_DIR`, `ensure_hermetic_state_dir`, `config_in`, adapted from wolf-ffi's: why (`state_dir()` would be `$XDG_STATE_HOME/wolf` or `$HOME/.local/state/wolf`; tests would source the user's `runtime_user/init_user.lua` and write swap/MRU/spell/parsers_user there), `OnceLock` because `setenv` races `getenv` on other test threads (libtest runs tests in parallel; `get_or_init` blocks other callers until the write finishes), set-if-absent, a test removing the variable must be alone in its binary, `state_isolation.rs` is the guard. One sentence in the `//!` header naming `config_in` as the state-dir funnel.
- Plain `//` comments: ≤2 lines, ≤100 columns.

### D3. Guard: new `crates/wolf-gnome/tests/state_isolation.rs` (test-writer)

Full port of `crates/wolf-ffi/tests/state_isolation.rs`, both payloads.

Differences from wolf-ffi:

- Linux only: `const PLATFORM_STATE_REL: &str = ".local/state/wolf";`, no macOS arm.
- No `tempfile`: scratch home from `common::unique_temp_dir("wolf-gnome-test-home")`, wrapped in file-local `struct ScratchDir(PathBuf)` whose `Drop` does `let _ = std::fs::remove_dir_all(&self.0);`. `plant_home() -> (ScratchDir, PathBuf)`. Payload cwd comes from `common::config(..)`.
- No skip: parent calls `common::require_nvim_bin()` (panics).
- Drive `wolf_core` directly: `WolfCore::ignorecase(&self) -> bool` (`crates/wolf-core/src/api.rs:488`), `wolf_core::state_dir`.

Kept from wolf-ffi: consts `PLANT_ENV = "WOLF_TEST_PLANTED_HOME"`, `ISOLATED_PAYLOAD = "isolated_payload"`, `CONTROL_PAYLOAD = "control_payload"`, `PLANT_BODY = "vim.o.ignorecase = true\n"`; helpers `run_child`, `assert_child_passed` (success and stdout contains `test result: ok. 1 passed;`), `list_tree`, `planted_home_or_return`. `run_child(name, home, extra_env)` re-execs `current_exe()` with `[name, "--exact", "--ignored", "--nocapture", "--test-threads=1"]`, sets `HOME` and `PLANT_ENV` to the scratch home, `env_remove`s `WOLF_STATE_DIR`, `XDG_STATE_HOME`, `XDG_DATA_HOME`.

Visible `a_planted_user_init_lua_never_reaches_a_test_core`: `require_nvim_bin()`; plant home A, run `ISOLATED_PAYLOAD`; plant home B, run `CONTROL_PAYLOAD` with `("WOLF_STATE_DIR", root_b)`; `assert_child_passed` each. Never calls `config_in` itself.

`isolated_payload` (`#[ignore = "child payload; run only by a_planted_user_init_lua_never_reaches_a_test_core"]`):

1. `planted_home_or_return`.
2. Preconditions: `WOLF_STATE_DIR` unset; `state_dir() == Some(home/.local/state/wolf)`; plant `is_file()`.
3. `let cfg = common::config(common::DEFAULT_COLS, common::DEFAULT_ROWS);`
4. `state_dir()` is `Some(d)` with `d != planted_root && !d.starts_with(&home)`; message "the wolf-gnome harness left state_dir() at {resolved:?} … see common::config_in".
5. `wake_channel`, `WolfCore::start(cfg, wake)`, `wait_for_flush(FLUSH_TIMEOUT)`, `core.ignorecase()`, `core.shutdown()`.
6. assert `!ic`: "captive nvim sourced the planted …".
7. `list_tree(planted_root) == {"runtime_user", "runtime_user/init_user.lua"}`.
8. Planted body byte-identical.

`control_payload` (same ignore): assert `WOLF_STATE_DIR == planted_root`; `common::config(..)`; `state_dir() == Some(planted_root)` ("the wolf-gnome harness overwrote a WOLF_STATE_DIR the caller had set"); start, flush, `ic`, shutdown; assert `ic` ("the planted init_user.lua did not take effect … proves nothing").

`//!` header: condensed wolf-ffi header; says it ports wolf-ffi's guard and uses std instead of `tempfile` for the lock-file reason in `release_profile.rs`.

Wiring: Cargo-only crate, auto-discovered. No Bazel/build.md change.

### D4. Out of scope

`preferences_dir()`/`.config/wolf` (nothing leaked there); `fresh_cwd` leaks (documented); the developer's already-polluted real `~/.local/state/wolf` — do not touch it.

## Files

| File | Owner | Change |
| --- | --- | --- |
| `crates/wolf-gnome/tests/common/mod.rs` | coder | D2 |
| `crates/wolf-gnome/tests/state_isolation.rs` | test-writer | D3 (new) |
| `crates/wolf-gnome/tests/README.md` | technical-writer | `common/mod.rs` entry in place; new `state_isolation.rs` entry between `smoke_headless.rs` and `vendored_pins.rs` |
| `notes/macos-followups.md` | technical-writer | rewrite row 70 in place, Status `Verified 2026-10-25` |

Not touched: `crates/wolf-gnome/Cargo.toml`, `Cargo.lock`, the three real-nvim test files, `notes/wolf-gnome-decisions.md`. Don't bump `notes/macos-followups.md`'s `Last verified` header.

## Test plan (falsifiers — run only with `--test state_isolation`; revert each)

While falsifier 1 is applied, any other real-nvim test would write into the real `~/.local/state/wolf`, so run only `--test state_isolation`.

1. Harness moves `state_dir()` off the default — delete `ensure_hermetic_state_dir();` in `config_in` → "the wolf-gnome harness left state_dir() at …". (Required revert check.)
2. nvim never sources the plant — edit 1 + comment out step-4 assert → "captive nvim sourced the planted …".
3. Nothing written into the plant — edits 1, 2 + comment out step-6 assert → tree diff lists swap/mru/spell/parsers_user.
4. Non-vacuity — `PLANT_BODY = "vim.o.ignorecase = false\n"` → control fails "…proves nothing".
5. Set-if-absent — delete the `is_some() → return None` branch → "overwrote a WOLF_STATE_DIR the caller had set".
6. Renamed payload can't pass on zero tests — `ISOLATED_PAYLOAD = "isolated_payloadx"` → parent fails "no longer names an ignored test".
7. Regression: full `cargo test -p wolf-gnome`.

## Verification

From repo root, inside `nix develop --command …`; never overlap two `cargo test` runs.

```sh
nix develop --command cargo fmt --all -- --check
nix develop --command cargo clippy --workspace --all-targets -- -D warnings
nix develop --command cargo test -p wolf-gnome --test state_isolation -- --nocapture
nix develop --command cargo test -p wolf-gnome
S=/tmp/claude-1000/-home-jplein-dev-jplein-wolf--worktrees-feat-documentation-checks-and-updates/c4a78c07-e57e-4977-a288-662a2a1c9cef/scratchpad
mkdir "$S/home2"
nix develop --command env -u XDG_STATE_HOME -u XDG_DATA_HOME -u XDG_CONFIG_HOME \
  HOME="$S/home2" CARGO_HOME=/home/jplein/.cargo cargo test --workspace --no-fail-fast
test ! -e "$S/home2/.local/state/wolf" && echo "state dir absent: OK"
find "$S/home2" -maxdepth 4
git diff --stat -- Cargo.lock crates/wolf-gnome/Cargo.toml   # must be empty
nix develop --command cargo run --quiet --manifest-path tools/doc-lint/Cargo.toml -- --root .
nix develop --command ./tools/lint/markdown_all.sh
```

(Check `build.md`'s Linux gate for the exact doc-lint invocation if the above differs.)

## Docs (technical-writer)

Present tense, no history words, ≤~200 words/entry, ≤4 sentences/paragraph, prettier afterwards.

- `common/mod.rs` entry: helper list gains `unique_temp_dir`; `config_in` is the state-dir funnel (throwaway `wolf-gnome-test-state-*`, once per binary, set-if-absent; without it `state_dir()` is `~/.local/state/wolf` and tests source the user's `init_user.lua` and write swap/MRU there; link `#state_isolationrs`). Decisions: std not `tempfile` (link `#release_profilers`); `OnceLock` because `setenv` races `getenv`. Gotchas: keep "kept in sync by hand"; not-auto-deleted extends to the state dir; a test setting its own `WOLF_STATE_DIR` before `config_in` keeps it, one removing it must be alone in its binary.
- New `state_isolation.rs` entry: Covers (re-exec under planted scratch HOME; isolated/control payload assertions); Decisions (link wolf-ffi's guard entry `../../wolf-ffi/tests/README.md#state_isolationrs` rather than restating); Gotchas (requires `WOLF_NVIM_BIN`, panics; payloads show as ignored, return without `WOLF_TEST_PLANTED_HOME`).
- `notes/macos-followups.md` row 70: defect + fix, Linux half; **No macOS impact** (no Bazel target, Cargo.toml/Cargo.lock untouched, no repin); measured Linux results with falsifier lines; Status `Verified 2026-10-25`.

## Progress log

- test-writer: created `crates/wolf-gnome/tests/state_isolation.rs` per D3 (adds shared helper `ignorecase_of_a_fresh_core`; drops wolf-ffi's separate "child cannot resolve nvim" assert since `config` panics via `require_nvim_bin`).
- coder: D2 done in `common/mod.rs` (also dropped "now" from `fresh_cwd` doc). Guard `1 passed; 2 ignored`. Falsifiers 1–6 all failed with the planned messages (1: "harness left state_dir() at Some(\"…/wolf-gnome-test-home-…/.local/state/wolf\")"; 3: tree diff lists command/file/folder-mru.sqlite, parsers_user, spell, swap). fmt, clippy -D warnings, full `cargo test -p wolf-gnome` (smoke_headless 73 passed), and every build.md Linux gate lint exit 0. Scratch-HOME `--workspace` run: exit 0, 1180 passed / 0 failed, `home2` left empty. Cargo.toml/Cargo.lock unchanged.
- reviewer: Approved with minor notes, none blocking (ignorecase() returns false on RPC error — covered by control/tree asserts, same as wolf-ffi; `WOLF_STATE_DIR=""` treated as set, matches siblings; runtime string "no longer names" copied from wolf-ffi, left as is). Orchestrator holds verbatim failure text for all six falsifiers from the coder's report.
- technical-writer: rewrote `common/mod.rs` entry, added `state_isolation.rs` entry, added a Decisions bullet to the `release_profile.rs` entry (the lock-file reason other entries link to lived only in the source header), closed `notes/macos-followups.md` row 70 as `Verified 2026-10-25`.
- coder: reworded stale "round-trip test"/"REAL" in `common/mod.rs` header, panic text and doc comment. prettier, markdown_check, markdown_all, doc-lint `--root .`, `prettier --check`, `cargo fmt --check` all exit 0; guard still `1 passed; 2 ignored`.
