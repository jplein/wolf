# Plan: user-writeable state directories (spell, runtime_user, parsers_user)

## Status

COMPLETE (pending final markdown lint confirmation) — implementation, tests, BUILD wiring, and docs landed; full wolf-core suite 51/51 uncached (incl. swap_recovery with `env_inherit = ["HOME"]`, and the sweep covered the final rewritten test files — checksum-verified); wolf-ffi 38/38; Bazel lint targets pass; cargo-side gate (fmt/clippy --all-targets/doctests/cargo test) is Linux-only on this host and unexercised. Falsifiability evidence below. Reviewer's blocking items fixed and verified: orlando/parsers_user `--clean` comments corrected to measured behavior (init.lua, tests/parsers_user.rs, durable-user-directory-plan.md), query_predicates one-liner added, pre-existing `-n` launch-line comment drift fixed in nvim.rs + init.lua headers. Technical writer verified all docs against source (two doc-ward fixes: the plan doc's `--clean` clause; tests/README.md spell.rs precondition overclaim); architecture.md / crate README / build.md confirmed unaffected. Known unrelated follow-ups (out of scope, not addressed): `redraw_grid::scroll_emits_viewport` flakes under machine load; pre-existing advisory markdown findings in wolf-gnome docs; five stray untracked root .md files predating this work.

## Falsifiability evidence (coder, step 6 — every falsifier applied to app code, captured, restored; restores verified by cmp against snapshots)

| Falsifier | Test | Failure (verbatim core) |
|---|---|---|
| Delete WolfSpell autocmd | spell.rs (all 3) | `opening a .md file must turn window-local 'spell' ON … left: "0" right: "1"` |
| Delete `opt.spellfile` assignment | spell.rs 1 | `'spellfile' must point at the durable <state_dir>/spell/en.utf-8.add … left: ""` |
| Drop text/gitcommit from prose set | spell.rs 2 only | `'spell' must be ON for filetype=text … left: "0"` |
| Always-on `"*"` callback | spell.rs 3 only | `…must turn 'spell' OFF … left: "1" right: "0"` |
| Remove WOLF_SPELL_DIR export | spawn; spell_durable 1 | `WOLF_SPELL_DIR must be exported … left: ""`; `timed out … zg to append the word to the durable en.utf-8.add` |
| Remove spell `create_dir_all` | spawn; spell_durable 1 | `spawn must create <state_dir>/spell before launch…`; same zg timeout |
| Drop gsub from 'directory' | spell_durable 2 | `timed out … a .swp entry in the comma'd durable swap dir` |
| Drop gsub from 'spellfile' | spell_durable 2 | startup aborts: unescaped comma raises E474 in init.lua |
| Remove WOLF_RUNTIME_USER_DIR export | spawn | `WOLF_RUNTIME_USER_DIR must be exported … left: ""` |
| Remove WOLF_PARSERS_USER_DIR export | spawn; parsers_user (rtp assertion) | `WOLF_PARSERS_USER_DIR must be exported … left: ""`; `<state_dir>/parsers_user must be ON 'runtimepath'` |
| Delete seed call in api.rs | spawn; runtime_user 1 | `WolfCore::start must seed <state_dir>/runtime_user with init_user.lua` |
| Remove parsers_user `create_dir_all` | spawn; parsers_user 1 | `spawn must create <state_dir>/parsers_user before launch` |
| Delete EOF dofile block | runtime_user 2 | `a pre-existing init_user.lua must be dofile'd at startup … left: "MISSING"` |
| Remove vim.notify branch | runtime_user 4 | `timed out … the init_user.lua warning to surface via :messages` |
| Seed when init_user.lua absent | state_unit 3 + runtime_user 3 | `a NON-EMPTY dir must report Ok(false)…`; `…must never be seeded, even when init_user.lua is absent` |
| Resolver leaf changed | state_unit 1 | `spell_dir() must resolve to <state_dir>/spell … left: Some("…/spellx")` |
| Seed returns Ok(false) w/o write | state_unit 2 | `seeding a MISSING dir must report Ok(true)…` |
| Remove from_env spell fallback | state_unit 4 | `…must fall back to the ephemeral state_root/spell … left: ""` |
| Delete setup+prepend (19b) | parsers_user 2 | `<state_dir>/parsers_user must be ON 'runtimepath' …` |
| Drop install_dir from setup() | parsers_user 4 | `install dir must be <state_dir>/parsers_user/parser … left: "…/data/wolf/site/parser"` |

Three named falsifiers measured non-falsifying (rulings above): reference on-only autocmd (spell.rs 3), prepend-alone deletion, runtime-call deletion.

## Problem statement

Implement the work described in `durable-user-directory-plan.md` (repo root — authoritative scope). The embedded Neovim runtime stays immutable in the app bundle / nix store. Wolf adds user-writeable directories under the platform state dir (`state_dir()` in `crates/wolf-core/src/state.rs`), all resolved core-side and handed to `runtime/nvim/init.lua` via `WOLF_*` env vars following the existing `WOLF_SWAP_DIR` pattern. No front-end (Swift/GTK/FFI) changes.

1. **Spell**: enable spell checking for `markdown`/`text`/`gitcommit` only, plus a durable user dictionary (`state_dir()/spell`, `WOLF_SPELL_DIR`, `'spellfile'` at `<dir>/en.utf-8.add`). Reference: `git diff main...feat/spell-check` (reapply/adapt; do not merge).
2. **runtime_user/**: seeded once (only when missing or empty) with an `init_user.lua` template; never touched when non-empty; `WOLF_RUNTIME_USER_DIR`; sourced via `pcall(dofile, …)` at the very end of `init.lua`; user-file errors are `vim.notify` warnings.
3. **parsers_user/**: created in `spawn`; `WOLF_PARSERS_USER_DIR`; `init.lua` activates the vendored nvim-treesitter (main branch) — runtime-source its plugin file, `require("nvim-treesitter").setup({ install_dir = … })`, user parsers shadow bundled `parsers/`.

## Falsifiability findings + orchestrator rulings (post-coder step 6)

Three named falsifiers did not falsify; measurements accepted, rulings:

1. **spell.rs test 3 passes under the reference on-only autocmd** — `vim.opt_local` spell is NOT sticky on this pinned nvim (window-local value resets from window-global on buffer switch). RULING: keep the `"*"`-with-explicit-off design as determinism hardening (it guarantees spell off for non-prose regardless of window-global state and survives future `vim.o.spell`-touching code), but every comment/doc stating the stickiness premise must be corrected to this measured rationale. Test 3 stays (it falsifies an always-on callback — evidence 4b).
2. **Deleting the parsers_user rtp prepend alone is invisible** — `setup()` eagerly prepends the identical string; the explicit prepend is load-bearing only when `pcall(setup)` fails. RULING: keep the prepend; combined falsifier 19b (setup + prepend deleted) is the accepted evidence; no test change.
3. **Deleting `vim.cmd.runtime("plugin/nvim-treesitter.lua")` is invisible** — measured: `--clean` keeps `'loadplugins'` on, so rtp-prepended `plugin/*.lua` load post-init anyway. RULING: keep the explicit call (registers `:TSInstall` deterministically during init); assertion 3 is a regression guard, not a falsifiable edge; coder already corrected the false premise in init.lua comments and the README's orlando decision.

Also landed by orchestrator approval: `env_inherit = ["HOME"]` on the `swap_recovery` Bazel target (pre-existing environmental failure, proven unrelated).

## Orchestrator decisions

- The architect's deviation from the reference diff is ACCEPTED: the `WolfSpell` autocmd uses `pattern = "*"` setting `spell` on for prose filetypes and OFF otherwise (window-local `'spell'` is sticky; the reference's on-only autocmd leaves spell enabled when a prose window switches to a non-prose file). `tests/spell.rs` test 3 pins this.
- Seeding lives in `api.rs::WolfCore::start` (not `spawn`) so the respawn path stays free of a new failure surface.
- `swap_recovery.rs` is NOT modified; comma-escaping coverage lives in `spell_durable.rs`.

# Architect plan (execute as written)

## 1. Summary

Three new directories resolved core-side, exported as env vars: `spell/` (durable `zg`/`zw` dictionary + spell on for prose filetypes), `runtime_user/` (one-time seed of `init_user.lua`, sourced last), `parsers_user/` (`:TSInstall` target; vendored nvim-treesitter activated as a real plugin). Also carries the reference branch's swap hardening not yet landed: comma-escaping of `'directory'` in `init.lua` (the fatal `create_dir_all` half already landed at `nvim.rs:1728`, but its comment still says "best-effort" — fix the comment). `CoreConfig` untouched.

## 2. Pre-flight verification (coder, read-only, before treesitter wiring)

The vendored nvim-treesitter is the **main branch** (pinned in `bazel/nvim-treesitter/extensions.bzl`, commit `4916d659…`; tree: `nvim-treesitter/{lua,plugin,queries,LICENSE}`). It is a Bazel external, not checked in. Verify against the fetched tree (`$(bazel info output_base)/external/*nvim_treesitter*/…`, or download the pinned tarball):

1. **`:TSInstall` is registered by `plugin/nvim-treesitter.lua`**, not by `setup()`. Under `--clean`, plugin dirs are never scanned, so `init.lua` must `vim.cmd.runtime("plugin/nvim-treesitter.lua")` after prepending the tree to `runtimepath` (orlando pattern). Verify the exact plugin file name.
2. **`require("nvim-treesitter").setup({ install_dir = … })` is a lightweight config merge** — no install jobs, no network/compilation.
3. **`install_dir` lands on `runtimepath` only lazily** (expected accessor: `require('nvim-treesitter.config').get_install_dir('parser')` — verify name; the `parsers_user.rs` probe depends on it). `init.lua` therefore **prepends** `WOLF_PARSERS_USER_DIR` itself, unconditionally — which also gives user parsers precedence over bundled `parsers/`.

If any assumption fails: keep the manual rtp prepend (load-bearing regardless); fallback for a missing/misnamed plugin file is sourcing whatever `plugin/*.lua` the tree ships; fallback for a heavy `setup()` is the `pcall` wrap (already specified). Report the verified accessor + plugin file name to the orchestrator.

## 3. Files to create (test-writer)

- `crates/wolf-core/tests/spell.rs`
- `crates/wolf-core/tests/spell_durable.rs`
- `crates/wolf-core/tests/runtime_user.rs`
- `crates/wolf-core/tests/parsers_user.rs`

No new source files.

## 4. Files to modify

### 4.1 `crates/wolf-core/src/state.rs` — coder

Add, directly after `swap_dir_for` (~line 243):

- `pub fn spell_dir() -> Option<PathBuf>` — `state_dir().map(|d| d.join("spell"))`. `#[must_use]`. Doc: global, deliberately not per-project (a word list belongs to the language, not the open folder); `None` only when `state_dir()` unresolvable → `NvimConfig::from_env` falls back to ephemeral `state_root/spell`.
- `pub fn runtime_user_dir() -> Option<PathBuf>` — `state_dir().map(|d| d.join("runtime_user"))`. `#[must_use]`.
- `pub fn parsers_user_dir() -> Option<PathBuf>` — `state_dir().map(|d| d.join("parsers_user"))`. `#[must_use]`.
- `pub const INIT_USER_FILE_NAME: &str = "init_user.lua";`
- `const INIT_USER_TEMPLATE: &str` — Lua comments only: this file is the user's, loaded after every Wolf default at the end of Wolf's `init.lua`, never overwritten once the directory is non-empty, errors reported as warnings. Must include the exact greppable sentence `-- Wolf loads this file last, so anything here overrides Wolf's defaults.`
- `pub fn seed_runtime_user_dir(dir: &Path) -> std::io::Result<bool>` — dir missing: `create_dir_all` + write template, `Ok(true)`. Exists with zero entries: write template, `Ok(true)`. Non-empty: touch nothing — **even when `init_user.lua` is absent** — `Ok(false)`. Doc the one-time-seed contract (sole exception to "Wolf never writes into the user's state-dir subtrees").

### 4.2 `crates/wolf-core/src/lib.rs` — coder

Extend the `pub use state::{…}` re-export (line 118) with `spell_dir`, `runtime_user_dir`, `parsers_user_dir`, `seed_runtime_user_dir`, `INIT_USER_FILE_NAME`.

### 4.3 `crates/wolf-core/src/nvim.rs` — coder

- New `pub struct DurableDirs` (near `NvimConfig`, ~line 60), derive `Clone, Debug, Default`: `pub swap_dir: Option<PathBuf>`, `pub spell_dir: Option<PathBuf>`, `pub runtime_user_dir: Option<PathBuf>`, `pub parsers_user_dir: Option<PathBuf>`. (Keeps `from_env` under clippy `too_many_arguments`.)
- `NvimConfig` gains: `pub spell_dir: PathBuf` (resolved, fallback applied; doc like `swap_dir`), `pub runtime_user_dir: Option<PathBuf>`, `pub parsers_user_dir: Option<PathBuf>`.
- `NvimConfig::from_env(init_lua: PathBuf, cwd: PathBuf, dirs: DurableDirs, cols: u16, rows: u16)`. Body: `swap_dir = dirs.swap_dir.unwrap_or_else(|| state_root.join("swap"))`; `spell_dir = dirs.spell_dir.unwrap_or_else(|| state_root.join("spell"))` (ephemeral fallback: `zg` works, words die with process); `runtime_user_dir`/`parsers_user_dir` carried as `Option` (`None` = skip: no creation, no export, init.lua no-ops).
- `spawn` (~1720-1751):
  - Fix stale comment above `create_dir_all(&cfg.swap_dir)` (1726-1727): the call IS fatal; say why (missing `'directory'` silently degrades crash recovery).
  - Add fatal `create_dir_all(&cfg.spell_dir)` with comment: nvim never creates the parent of an explicit `'spellfile'`; missing parent → `zg` fails E484 silently.
  - `if let Some(d) = &cfg.parsers_user_dir` → fatal `create_dir_all(d)`.
  - Env exports: unconditional `.env("WOLF_SPELL_DIR", &cfg.spell_dir)`; conditional `WOLF_RUNTIME_USER_DIR` / `WOLF_PARSERS_USER_DIR` when `Some`.
  - Update `spawn` doc comment's env-var list.

### 4.4 `crates/wolf-core/src/api.rs` — coder

In `WolfCore::start` (~308-317), after computing `swap_dir`: resolve `state::spell_dir()`, `state::runtime_user_dir()`, `state::parsers_user_dir()`. If `runtime_user` is `Some`, call `state::seed_runtime_user_dir(dir)`; on `Err`, `tracing::warn!` and continue (never fatal). Build `DurableDirs { swap_dir, spell_dir, runtime_user_dir, parsers_user_dir }`, pass to `from_env`. Update imports (line 33 gains `nvim::DurableDirs`; line 38 gains the new resolvers).

### 4.5 `runtime/nvim/init.lua` — coder (positions load-bearing)

1. **Comma-escape helper + swap hardening** (before the swap block, ~line 38): `local function escape_option_path(p) return (p:gsub(",", "\\,")) end` with comment (`'directory'`/`'spellfile'` are comma-separated lists; a literal comma silently splits the value). Change line 41 to `opt.directory = escape_option_path(swap_dir) .. "//"`.
2. **Spell block** — after `vim.filetype.add` (line 170), **strictly before `vim.cmd("syntax on")`** (line 191; ordering load-bearing — after `syntax on` the FileType autocmd never fires for a markdown startup file argument; carry the reference diff's comment):
   - Augroup `WolfSpell` (`clear = true`), one `FileType` autocmd, `pattern = "*"`, callback: `local prose = { markdown = true, text = true, gitcommit = true }` (file-local table); set `vim.opt_local.spell = (prose[args.match] == true)`; set `vim.opt_local.spelllang = "en"` when turning on. Comment the deviation: `'spell'` is window-local and sticky — startup buffer `Untitled.md` has `filetype=markdown` at VimEnter, so an on-only autocmd leaves spell on for every non-prose file later `:e`d in that window; the `"*"` pattern turns it off on the way out. `en` pinned: the only language whose `.spl` the pinned nvim ships (a missing `.spl` opens a blocking download prompt that has nowhere to draw under `ext_messages`).
   - Global `'spellfile'`: `local spell_dir = vim.env.WOLF_SPELL_DIR; if spell_dir ~= nil and spell_dir ~= "" then vim.opt.spellfile = escape_option_path(spell_dir) .. "/en.utf-8.add" end`. Comment: core created the dir pre-launch (nvim never creates a spellfile's parent; missing parent → E484 silent drop).
3. **parsers_user / nvim-treesitter activation** — inside the `if nvim_treesitter_dir then` gate (318-353), after the textobjects prepend (~328), before the `WolfTreesitter` autocmd:
   ```lua
   local parsers_user_dir = vim.env.WOLF_PARSERS_USER_DIR
   if parsers_user_dir ~= nil and parsers_user_dir ~= "" and uv.fs_stat(parsers_user_dir) then
       vim.cmd.runtime("plugin/nvim-treesitter.lua")
       pcall(function()
           require("nvim-treesitter").setup({ install_dir = parsers_user_dir })
       end)
       vim.opt.runtimepath:prepend(parsers_user_dir)
   end
   ```
   PRE-FLIGHT VERIFIED (coder): `plugin/nvim-treesitter.lua` is the exact plugin file (registers `:TSInstall`/`TSUpdate`/`TSUninstall`/`TSLog`, guard `vim.g.loaded_nvim_treesitter`, all requires lazy — cheap to source); `setup()` is a pure config merge BUT eagerly prepends `install_dir` to rtp via raw string concat (`vim.o.rtp = dir .. ',' .. vim.o.rtp`). Hence the ORDER above: setup first, then `vim.opt.runtimepath:prepend` — option-list prepend dedupes, so the entry lands exactly once and at the front whether or not the `pcall(setup)` succeeded. Comments to carry: user dir lands ahead of bundled `parsers_dir` (user parsers deliberately shadow bundled); `--clean` never sources plugin dirs so the explicit `runtime` call registers `:TSInstall` (orlando pattern); `pcall` degrades incompatibility to "no :TSInstall"; only `plugin/nvim-treesitter.lua` is sourced — the sibling `query_predicates.lua` predicates are unused by every bundled language (one-line comment). Placement inside the gate deliberate (`require` needs the tree's `lua/` on rtp, prepended by the gate). **Rewrite the stale gate comments in place** (295-298 "We do NOT call … .setup{}" and the "purely an on-rtp query provider" framing) to the new contract — present tense, no changelog words.
4. **End-of-file user override load point** — appended after `wolf_setup_preview` (line 2629), stays the last thing in the file:
   ```lua
   local runtime_user_dir = vim.env.WOLF_RUNTIME_USER_DIR
   if runtime_user_dir ~= nil and runtime_user_dir ~= "" then
       local user_init = runtime_user_dir .. "/init_user.lua"
       if uv and uv.fs_stat(user_init) then
           local ok, err = pcall(dofile, user_init)
           if not ok then
               vim.notify("Wolf: error in init_user.lua: " .. tostring(err), vim.log.levels.WARN)
           end
       end
   end
   ```
   Comment: must remain last — user code overrides every Wolf default; user-file errors warn, never fail startup. The notify text must contain the literal `init_user.lua` (a test greps `:messages` for it). File-level `uv` local (line 208) is in scope.

### 4.6 `crates/wolf-core/BUILD.bazel` — coder

Four new `rust_test` targets mirroring the `swap_recovery` stanza (1143-1165) in shape — `tests/common/mod.rs` in `srcs`, `crate_root` on the test file, `tags = ["no-sandbox"]`, `size = "small"`, `edition = "2021"`, `data = ["//bazel/nvim:nvim_tree", "//runtime/nvim:init.lua"]`, `env = { "WOLF_NVIM_BIN": "$(rootpath //bazel/nvim:nvim_tree)/nvim/bin/nvim", "WOLF_TEST_INIT_LUA": "$(rootpath //runtime/nvim:init.lua)", "WOLF_TEST_NVIM_DIR": "$(rootpath //bazel/nvim:nvim_tree)" }`, `deps = [":wolf_core"] + all_crate_deps(normal = True, normal_dev = True)`:

- `spell`, `spell_durable`, `runtime_user`.
- `parsers_user` — additionally `"//bazel/nvim-treesitter:nvim_treesitter_tree"` and `"//bazel/treesitter-parsers:parsers_tree"` in `data`, and `"WOLF_TEST_NVIM_TREESITTER_DIR": "$(rootpath //bazel/nvim-treesitter:nvim_treesitter_tree)"`, `"WOLF_TEST_PARSERS_DIR": "$(rootpath //bazel/treesitter-parsers:parsers_tree)"` in `env` (mirror `//crates/wolf-ffi:treesitter_highlight`, wolf-ffi BUILD.bazel 848-873).

Short leading comments in house style. `state_unit`/`spawn` targets already glob their files — no change. Confirm each new target actually runs under `bazelisk test`.

### 4.7 Documentation — coder

- `runtime/nvim/README.md`: env list (line 11) names all exported `WOLF_*` vars; rewrite in place the stale "purely an on-runtimepath query provider" decision to the new contract; add Decisions entries for the spell block (before-`syntax on`; `"*"` pattern with explicit off — window-local stickiness; comma-escaping; `en` pinned) and the end-of-file `init_user.lua` load point (last-wins, pcall + notify); note `:TSInstall` compiling depends on the user's toolchain — Wolf only guarantees a durable install target.
- `crates/wolf-core/src/README.md` `#### state.rs` entry (line 124): extend Public API with the three resolvers + `seed_runtime_user_dir` (one-time-seed semantics), within the ~200-word budget.
- `crates/wolf-core/DECISIONS.md`: new short section (after "How core embeds Neovim"): bundle is Wolf's, replaced wholesale on upgrade; everything under `state_dir()` is the user's and Wolf never modifies it, sole exception the one-time seeding of an empty `runtime_user/`; core resolves dirs → `WOLF_*` env vars.
- `architecture.md`: no change.
- `prettier --write` + `./tools/lint/markdown_check.sh` on touched Markdown.

## 5. Test plan (test-writer)

Shared conventions: real nvim via `common::require_nvim_bin`; observation channel is the **file probe** — `:call writefile([<expr1>, …], '<probe path>')` via `common::ch(':')` + `type_str` + `Key::Enter`, then poll the probe file (50 ms cadence, 10 s budget). Add to `tests/common/mod.rs`:

- `pub fn send_ex(core: &WolfCore, cmd: &str)` (promote the `messages.rs` helper).
- `pub fn probe_values(core: &WolfCore, exprs: &[&str], probe: &Path) -> Vec<String>` — writes the `:call writefile([…], '…')` command, polls, returns lines. Keep expressions cmdline-safe (no single quotes inside `luaeval` strings).
- `pub fn resolve_test_runfile(env_key: &str) -> Option<PathBuf>` — `pub` wrapper over the private `resolve_runfile`.

`WOLF_STATE_DIR` strategy per binary:

| binary | strategy |
|---|---|
| `spell.rs`, `spawn.rs` | shared hermetic (`common::hermetic_state_dir()`), no env mutation, concurrent-safe |
| `spell_durable.rs`, `runtime_user.rs`, `parsers_user.rs` | file-local `static ENV_LOCK: Mutex<()>` (the `state_unit.rs` precedent); each test holds the lock for its whole body, `set_var`s its own fresh tempdir, never `remove_var` |
| `state_unit.rs` | existing `ENV_LOCK` for env-mutating additions |

### `tests/spell.rs` (shared hermetic)

1. `spell_enables_for_markdown_on_open` — Open a temp `.md`; probe `string(&l:spell)` == `"1"`, `&spelllang` == `"en"`, `&spellfile` == `hermetic_state_dir()/spell/en.utf-8.add`. Falsifiers: delete `WolfSpell` block; delete `opt.spellfile` assignment.
2. `spell_enables_for_text_and_gitcommit` — `:e notes.txt` → `&l:spell == 1`; `:e COMMIT_EDITMSG` → `&l:spell == 1` and `&filetype == "gitcommit"`. Falsifier: drop `text`/`gitcommit` from the prose set.
3. `spell_disabled_for_non_prose_after_prose` — open `.md` (spell on), then `:e main.lua` in the same window; probe `&l:spell == 0`, `&filetype == "lua"`. Pins the `"*"`-pattern design. Falsifier: revert autocmd to on-only three-pattern form.

### `tests/spell_durable.rs` (per-test `set_var` under `ENV_LOCK`)

1. `zg_word_survives_core_restart` — fresh tempdir state dir; open temp `.md` whose first word is `zzqxwolf`; send `zg`; poll `<state>/spell/en.utf-8.add` until it contains the token. `core.shutdown()`. Second core, same env; probe `&spellfile` (same path) and `join(spellbadword('zzqxwolf'), '|')` → first element empty (word good ⇒ durable dictionary loaded). Falsifiers: remove `WOLF_SPELL_DIR` export in `spawn`; remove `create_dir_all(&cfg.spell_dir)`.
2. `comma_state_dir_swap_and_spellfile_escaped` — state tempdir whose leaf contains a comma (`wolf,state`). Swap half: `:e <cwd>/probe.md`, type text, `:preserve`, poll `swap_dir_for(&canonical_cwd)` for a `*.swp` entry. Spell half: `zg` a token, poll `<state,dir>/spell/en.utf-8.add`. Falsifiers: drop the gsub from `'directory'`; from `'spellfile'`.

### `tests/runtime_user.rs` (per-test `set_var` under `ENV_LOCK`)

1. `empty_state_dir_is_seeded_and_template_sourced` — fresh tempdir; start core; `<state>/runtime_user/init_user.lua` exists and contains the greppable sentence; core flushes normally. Falsifier: delete the `seed_runtime_user_dir` call in `api.rs`.
2. `user_file_is_sourced_last_and_overrides_defaults` — pre-create `init_user.lua` with `vim.g.wolf_user_marker = "loaded"` and `vim.o.linebreak = false` (init.lua sets `linebreak = true` at line 162 → observing `0` proves execution AND after-defaults position). Probe `get(g:, 'wolf_user_marker', 'MISSING')` == `"loaded"`, `string(&linebreak)` == `"0"`. Also byte-identical before/after check on the user file. Falsifier: delete the EOF dofile block; (position half) moving the block above the wrap settings flips `&linebreak` to `1`.
3. `non_empty_dir_without_init_user_is_untouched` — pre-create only `notes.txt`. Assert `init_user.lua` absent, `notes.txt` byte-identical, core works. Falsifier: seed whenever `init_user.lua` absent.
4. `erroring_user_file_warns_but_startup_succeeds` — `init_user.lua` = `error("wolf-test-boom")`. Core starts, initial flush arrives; open `.md`, `&l:spell == 1` (defaults survived); `send_ex("messages")`, poll `Snapshot.toast` text for a line containing `init_user.lua`. Falsifier (warning half): remove the `vim.notify` branch.

### `tests/parsers_user.rs` (per-test `set_var` under `ENV_LOCK`; one test fn)

Setup: `set_var("WOLF_STATE_DIR", tempdir)`; resolve staged trees Bazel-first, devShell-second (wolf-ffi common precedent, `crates/wolf-ffi/tests/common/mod.rs` 216-270): `resolve_test_runfile("WOLF_TEST_NVIM_TREESITTER_DIR")` → `set_var("WOLF_NVIM_TREESITTER_DIR", root.join("nvim-treesitter"))`; likewise `WOLF_TEST_PARSERS_DIR` → `WOLF_PARSERS_DIR` = `root.join("parsers")`. If neither resolves, panic loudly.

`parsers_user_dir_created_activated_and_first_on_rtp` — start core; assert:

1. `<state>/parsers_user` exists. Falsifier: remove the `WOLF_PARSERS_USER_DIR` export (sharper than removing create_dir_all).
2. Probe `&runtimepath`: contains parsers_user path AND its index strictly before the bundled parsers dir's (precedence). Probe rtp before any plugin-internal assertion (the accessor appends to rtp itself and would mask a missing prepend). Falsifier: delete the `prepend` line.
3. Probe `exists(':TSInstall')` == `"2"`. Falsifier: delete the `vim.cmd.runtime(...)` line.
4. Probe `luaeval(...)` calling `require('nvim-treesitter.config').get_install_dir('parser')` (PRE-FLIGHT CONFIRMED accessor, singular `'parser'`; parsers land at `<install_dir>/parser/<lang>.so`) == `<state>/parsers_user/parser`. CAVEATS (pre-flight): the accessor has a mkdir side effect (creates `<install_dir>/parser/`), so the on-disk assertion 1 and the rtp assertion 2 MUST run before this probe (they already do — keep that order); the returned path is `vim.fs.normalize`d, so normalize/canonicalize both sides before comparing (macOS tempdirs may differ in trailing-slash or `/var` vs `/private/var` spelling — canonicalize the Rust side, compare suffix-tolerantly if needed). Falsifier: drop `install_dir` from `setup()` (accessor then returns the stdpath default under the hermetic XDG root).

### `tests/spawn.rs` addition (shared hermetic)

`durable_dirs_created_and_exported` — start core; `state = common::hermetic_state_dir()`; on disk: `state/spell`, `state/parsers_user`, `state/runtime_user/init_user.lua` exist; probe `$WOLF_SPELL_DIR`, `$WOLF_RUNTIME_USER_DIR`, `$WOLF_PARSERS_USER_DIR` inside nvim == the `state/…` paths. Falsifiers: each `.env(…)` export and each `create_dir_all`, one assertion apiece.

### `tests/state_unit.rs` additions (headless)

1. `spell_runtime_parsers_dirs_resolve_under_state_dir` (ENV_LOCK, `WOLF_STATE_DIR` override): resolvers == `<override>/{spell,runtime_user,parsers_user}`. Falsifier: change a `join` leaf.
2. `seed_runtime_user_seeds_missing_and_empty_dirs` (no env): missing dir → `Ok(true)` + file with greppable sentence; fresh empty dir → `Ok(true)`. Falsifier: return `Ok(false)` without writing.
3. `seed_runtime_user_never_touches_non_empty_dir` (no env): dir with only `notes.txt` → `Ok(false)`, no `init_user.lua`, `notes.txt` unchanged; dir with custom `init_user.lua` → `Ok(false)`, byte-identical. Falsifier: seed whenever `init_user.lua` absent.
4. `from_env_falls_back_to_ephemeral_spell_dir` (ENV_LOCK; save/restore `WOLF_NVIM_BIN`, set dummy non-empty): `from_env(any_init, any_cwd, DurableDirs::default(), 80, 24)` → `spell_dir == state_root.join("spell")`, `swap_dir == state_root.join("swap")`, `runtime_user_dir.is_none()`, `parsers_user_dir.is_none()`. Falsifier: remove the spell `unwrap_or_else` fallback.

### `swap_recovery.rs`

No changes (comma coverage deliberately lives in `spell_durable.rs`; it needs exclusive `WOLF_STATE_DIR` mutation).

## 6. Implementation order

1. **Coder pre-flight**: verify §2 assumptions; report accessor + plugin file name.
2. **Test-writer**: common helpers → `state_unit.rs` additions → `spawn.rs` addition → `spell.rs` → `spell_durable.rs` → `runtime_user.rs` → `parsers_user.rs`. Reference the planned API exactly: `wolf_core::{spell_dir, runtime_user_dir, parsers_user_dir, seed_runtime_user_dir, INIT_USER_FILE_NAME}`, `wolf_core::nvim::{NvimConfig, DurableDirs}`.
3. **Coder**: `state.rs` → `lib.rs` → `nvim.rs` → `api.rs` (compiles at every step; `from_env` signature change + its sole caller land together).
4. **Coder**: `init.lua` in §4.5 order (helper first).
5. **Coder**: `BUILD.bazel` targets; run `bazelisk test` on the six affected targets + pre-existing real-nvim suite (regression: every core start now creates/seeds dirs); `cargo test` where available.
6. **Coder**: falsifiability passes — apply each named revert, observe the mapped test fail, restore.
7. **Coder**: docs (§4.7) + prettier + markdown lint.

## 7. Dependencies

None new. Bazel trees `//bazel/nvim-treesitter:nvim_treesitter_tree` and `//bazel/treesitter-parsers:parsers_tree` already exist (consumed by wolf-ffi).

## 8. Lint considerations

- `DurableDirs` exists to stay under `clippy::too_many_arguments`; do not flatten.
- New `pub` items need doc comments; `#[must_use]` on pure resolvers (house style).
- `tests/common/mod.rs` carries `#![allow(dead_code)]` already. Env-mutating tests use `std::env::set_var` per existing practice (edition 2021).
- `init.lua`: tabs, match comment register.
- Markdown: `proseWrap: never`; no changelog words ("now", "no longer") when rewriting the stale nvim-treesitter decision.

Key files: `crates/wolf-core/src/state.rs`, `crates/wolf-core/src/nvim.rs` (struct ~60-131, spawn ~1700-1799), `crates/wolf-core/src/api.rs` (~308-317), `runtime/nvim/init.lua` (38-42, 170-191, 305-353, EOF 2629), `crates/wolf-core/BUILD.bazel` (template 1143-1165; ffi template `crates/wolf-ffi/BUILD.bazel` 848-873), `crates/wolf-core/tests/common/mod.rs`.
