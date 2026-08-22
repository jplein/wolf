# Durable user spell dictionary (`zg` / `zw`)

Status: all review findings fixed, 98/98 green. Final test coverage + docs in flight.

## Round 4: orchestrator decisions on the coder's new recommendations

All three review findings are fixed and verified. Finding 2 was confirmed a LIVE bug, not a
theoretical one: with the path unescaped, `swapname('%')` resolved to nvim's own default
swap dir — durable swap was silently disabled whenever the state dir contained a comma, and
`spell_durable`'s hostile dir was hitting exactly that. Escaped, the swap lands in the Wolf
dir with the `//` full-path encoding intact.

1. **ACCEPTED (test-writer): lock in the `'directory'` comma escaping.** Nothing asserts it
   today — `swap_recovery` uses a comma-free tempdir. We just fixed a real silent bug with
   zero coverage, so it goes in: give `swap_recovery.rs` a comma-and-space-bearing state
   dir, mirroring `spell_durable`'s `HOSTILE_STATE_SUBDIR`, so the existing recovery
   assertions run through an escaped path.
2. **DECLINED for this change: the `redraw_grid` scroll flake.** Measured at 4/10 failures
   on committed `HEAD` with all of this work stashed, so it strictly predates it and is
   unrelated to spell or swap. `scroll_shifts_grid_content` and
   `scroll_up_shifts_grid_content_down` sample the grid without waiting for the post-scroll
   redraw to settle. A real bug worth fixing, but fixing it here would expand a
   spell-dictionary change into unrelated test-harness repair. Report it to the user as a
   separate item.

## Round 3: orchestrator decisions on the review

Reviewer verdict: approved. Plan conformance item for item, all three measured constraints
respected with reasons recorded at the site, `syntax on` ordering intact, the
global-not-per-project decision genuinely protected in `state.rs`, and `spell_durable`
cannot pass vacuously. Three notes, all accepted:

1. **(must fix, test-writer) Stale tests-first note**, `tests/state_unit.rs:538-539`. It
   claims `spell_dir` "does not exist until the coder adds it" — false, and exactly the
   history narration `CLAUDE.md` bans. Delete those two lines; keep the rest of the doc.
2. **(accepted, coder) `'directory'` has the same comma hazard and no escaping**,
   `init.lua:33-36`. `'directory'` is also a comma-separated list, so a comma in
   `state_dir()` splits it into two bogus entries and durable swap silently reverts to
   nvim's default. This is live in our own suite: `spell_durable`'s `HOSTILE_STATE_SUBDIR`
   contains a comma, so that test runs with swap effectively disabled — a situation THIS
   change introduced. One line, and leaving one block escaped-and-documented beside an
   identical unescaped one is precisely how the reason gets lost. Mind the trailing `//`:
   escape the path, then append.
3. **(accepted, coder) Comment contradicts code**, `nvim.rs:1724-1726`. The swap
   `create_dir_all` comment says "best-effort (a failure just means nvim falls back)" but
   `?` propagates and fails the spawn. The BEHAVIOR is deliberate and mirrors the XDG
   `create_dir_all`s above it — fix the COMMENT, not the code. Pre-existing, but the
   correct new spell comment directly below makes it newly conspicuous.

Reviewer observations accepted without action: no test covers a regression to a
post-`syntax on` `FileType` autocmd for `'spellfile'` (that failure mode needs a markdown
startup file argument, which neither the tests nor production ever use — the `init.lua`
comment is the protection); the empty-`WOLF_SPELL_DIR` no-op branch is untested and
unreachable from Wolf.

Round 2 closed: both test bugs fixed, `spell_durable` runs 1 test and passes (0.4s, `small`
stands), `spawn` runs 4 (3 new). Full sweep `98 out of 98 tests pass`, no flakes this run.
The hostile `Application Support,Wolf` state dir passed with no application change, which is
the real proof of the comma escaping. Lint 5/5, clippy log clean. Docs (plan step 8) are the
only remaining work.

## Round 2: orchestrator decisions on the coder's report

Implementation is complete and verified by hand (the coder confirmed the full chain
end to end against the pinned nvim, including a spell dir containing both a comma and a
space). `:spell`, `:state_unit`, `:swap_recovery`, `:spawn` all pass; `state_unit` runs the
new `spell_dir_is_global_and_under_state_dir`. Only `spell_durable` is red, and both causes
are bugs in the TEST file, not the implementation.

**Must fix (test-writer):**

1. **`drop(core)` after `core.shutdown()` — compile error.** `WolfCore::shutdown` takes
   `self` by value, so the explicit `drop(core);` is a use-after-move (E0382). Delete that
   line; keep `drop(waiter);`. Shutdown already consumes and drops the core.
2. **The `<none>` sentinel can never fire.** Measured: `vim.fn.spellbadword()` returns an
   empty STRING at index 1 for a good word, not `nil`, and `""` is truthy in Lua — so
   `(… [1] or "<none>")` yields `""` and every `assert_eq!(…, NONE)` fails with a confusing
   empty string even when the feature works. Bind the call once in Lua and map empty to the
   sentinel explicitly. Note this bug also makes the "before" assertion pass by accident.

**Also adopt (test-writer):**

3. **Fold a comma AND a space into `spell_durable`'s own state dir.** The comma escaping is
   the one guard whose failure is completely silent (`zg` falls back to the rtp default and
   looks like success), and the test already owns its `WOLF_STATE_DIR` tempdir, so this
   costs nothing — create a subdirectory whose name contains both, and point
   `WOLF_STATE_DIR` at it. This locks in exactly what the coder verified by hand. Rejected
   the alternative of a separate comma-only target: a whole new Bazel target for one
   assertion, when the existing test can carry it for free.

**Also adopt (test-writer):**

4. **Cover the `from_env(…, None, …)` spell-dir fallback in `crates/wolf-core/tests/spawn.rs`.**
   Nothing exercises the `None` → `state_root/spell` branch or `spawn`'s `create_dir_all`
   for it, and a regression there is invisible until `zg` silently fails `E484`. Cheap and
   headless; `spawn.rs` is already wired.

**Declined:**

5. A separate assertion that `WOLF_SPELL_DIR` is present in the child's environment.
   `spell_durable` proves it end to end, and asserting it in `spawn.rs` would mean
   duplicating the toast/echo probe machinery a third time. The redundancy is not worth the
   maintenance cost.

**Noted, not a blocker:** the full sweep shows one flaky real-nvim target per run
(`tabpages`, then `save_open` — a different one each time), each passing in isolation, each
log ending at `running N tests` with no assertion output. Pre-existing, load-related, and
matching flakiness already seen on `autoread` and `spell` before this change. Not caused by
this work; do not chase it here.

## Problem

`zg` (add word to the user spell dictionary) and `zw` currently write to a path that does
not survive the session, so a word the user teaches Wolf is forgotten on restart. The user
reported this directly.

Cause: `runtime/nvim/init.lua` leaves `'spellfile'` unset. With no explicit value nvim
derives the target from the first writable directory on `runtimepath` plus
`/spell/en.utf-8.add`. `crates/wolf-core/src/nvim.rs`'s `spawn` pins `XDG_DATA_HOME` (and
the other `XDG_*` dirs) under a private temp dir for hermeticity, so the derived path lands
in that temp dir and dies with the process.

## Desired behavior

`zg` / `zw` write under `state_dir()` — macOS `~/Library/Application Support/Wolf/spell`,
Linux `~/.local/state/wolf/spell` — so taught words persist across restarts.

The dictionary is GLOBAL, not per-project: a word list is a property of the language, not
of the open folder. Deliberately UNLIKE `swap_dir_for(cwd)`, which hashes the cwd. Use
`state_dir()/spell` with no cwd component.

## Empirical findings (measured against the pinned nvim 0.12.3; binding)

1. **Set `'spellfile'` GLOBALLY (`vim.opt.spellfile`), once, at init.** It is local-to-buffer
   but the global value seeds every new buffer; verified on all three open paths (startup
   file argument, `:edit`, `:enew` + `set ft=markdown`). A buffer-local set from a NEW
   `FileType` autocmd appended at the end of `init.lua` FAILS for a markdown startup file
   argument — the same `syntax on` hazard the `WolfSpell` comment documents. If buffer-local
   is ever wanted it must go inside the EXISTING `WolfSpell` callback.
2. **`vim.fn.mkdir(dir, "p")` is MANDATORY, not defensive.** nvim does not create the parent
   of an explicitly-set `'spellfile'`. Missing parent ⇒ `zg` fails `E484` and the word is
   silently dropped; no dir, no file. nvim auto-creates only for the runtimepath-derived
   DEFAULT, which is why the premise misleads.
3. **A comma anywhere in the path silently defeats the feature.** `'spellfile'` is a
   comma-separated list; a raw comma raises `E5113`/`E474` and leaves the option EMPTY, so
   `zg` falls back to the default rtp spellfile and looks like success. Escape with
   `(path:gsub(",", "\\,"))`. Spaces need no escaping through the Lua option API, so a macOS
   `Application Support` path is fine.
4. **`zg` writes two files:** `en.utf-8.add` (plain text, one word per line) and a compiled
   `en.utf-8.add.spl`. No `:mkspell` needed — `zg` recompiles in-process and
   `spellbadword()` reports the word good immediately. A fresh nvim with the same
   `'spellfile'` also sees it good (verified end to end).
5. **The compiled `.add.spl` is the load-time source of truth, not the `.add`.** Externally
   appending to `.add` is NOT picked up; nvim does not auto-recompile and emits no warning.
   TESTING IMPLICATION: never hand-write the `.add` from Rust. Drive the real `zg`.

## Orchestrator decisions on the architect's open questions

- **`SPELL_LANG` shared local: ADOPTED.** Introduce it and use it in both the filename and
  the existing `WolfSpell` callback. Only a literal inside the callback body changes; the
  registration order — the load-bearing part — is untouched, and `//crates/wolf-core:spell`
  asserts `spelllang == "en"`, so a mistake fails loudly.
- **`size = "small"` for the new target: ADOPTED as the starting point.** If the first real
  run exceeds ~30s wall, the coder bumps that one target to `medium` and says so. Do not
  weaken poll assertions to fit a timeout.
- **`pcall` around `mkdir`: ADOPTED.** An init.lua that raises during startup is worse than
  a `zg` that fails.

---

## File-by-file plan

### `crates/wolf-core/src/state.rs`

Add `pub fn spell_dir() -> Option<PathBuf>`, `#[must_use]`, directly below `swap_dir_for`.
Returns `state_dir().map(|d| d.join("spell"))`. No re-export in `lib.rs` (`swap_dir_for`
is not re-exported either; tests reach it as `wolf_core::state::…`).

Doc comment must state: it is `state_dir()/spell`; it is deliberately NOT per-project and
NOT cwd-hashed, UNLIKE `swap_dir_for` — name `swap_dir_for` explicitly so a later agent does
not "make it consistent" by hashing the cwd. A swap file belongs to one project's files; a
word list belongs to the language. `None` only when `state_dir()` cannot resolve, and
`NvimConfig::from_env` then falls back to an ephemeral dir under the private state root
(documented, no panic).

### `crates/wolf-core/src/nvim.rs`

- **Field** `pub spell_dir: PathBuf` immediately after `swap_dir`, before `cwd`.
  `_state_guard` stays last and private. Doc: durable, Wolf-owned, GLOBAL user-dictionary
  dir, exported as `WOLF_SPELL_DIR`; global unlike `swap_dir`; distinct from the ephemeral
  `state_root`, which is exactly what made the default spellfile ephemeral.
- **`from_env`** gains a `spell_dir: Option<PathBuf>` parameter immediately after
  `swap_dir` (six params, under clippy's threshold of 8 — do NOT bundle them into a struct).
  Body: `let spell_dir = spell_dir.unwrap_or_else(|| state_root.join("spell"));`.
  Extend the EXISTING swap-fallback doc paragraph in place to cover both; state the
  consequence of the fallback: `zg` still works within the session (real writable dir, so
  no `E484`), the words simply do not outlive the process — the pre-change behavior, no
  panic.
- **`spawn`**: `create_dir_all(&cfg.spell_dir).map_err(NvimError::StateDir)?` immediately
  after the `swap_dir` one, commented with finding 2 as the reason. Add
  `.env("WOLF_SPELL_DIR", &cfg.spell_dir)` on the line after the `WOLF_SWAP_DIR` export.
  Rewrite `spawn`'s doc clause in place to name both vars; do not append a second sentence.

### `crates/wolf-core/src/api.rs`

In `WolfCore::start` (~308-317): extend `use crate::state::{state_dir, swap_dir_for};` with
`spell_dir`; add `let spell_dir = spell_dir();` after the `swap_dir_for` line; pass it as the
new fourth argument. Extend the existing comment in place with one clause: the spell dir
takes no cwd — it is global by design. Do not restate the why (that lives in `state.rs`).
`restart_nvim` reuses the stored config and needs no change.

### `runtime/nvim/init.lua`

Insert immediately BEFORE the `local spell_group = vim.api.nvim_create_augroup("WolfSpell",
…)` line and AFTER the existing comment block. A plain option assignment before the augroup
creation does not move the `WolfSpell` registration relative to `vim.cmd("syntax on")`, so
the load-bearing ordering is preserved. Do NOT move, wrap, or re-register `WolfSpell`.

- `local SPELL_LANG = "en"` above the block, commenting both consumers. Do not restate why
  `en` is pinned — the existing comment already carries it; reference it.
- Change `vim.opt_local.spelllang = "en"` inside the `WolfSpell` callback to
  `vim.opt_local.spelllang = SPELL_LANG`. This is the ONLY edit inside the existing block.
- Block behavior, in order: read `vim.env.WOLF_SPELL_DIR`; guard
  `if dir ~= nil and dir ~= "" then` (strict no-op otherwise, matching the swap block's
  shape); `pcall(vim.fn.mkdir, dir, "p")`; build `dir .. "/" .. SPELL_LANG .. ".utf-8.add"`;
  escape commas over the whole joined path with `(path:gsub(",", "\\,"))` (parenthesized so
  the substitution count is discarded); `vim.opt.spellfile = <escaped>`.
- Comments must carry the WHY for each non-obvious step: global not buffer-local (finding 1,
  and name it as the simplification a later agent would attempt); mkdir mandatory (finding
  2); comma escaping (finding 3, including that spaces are fine so nobody "fixes" that);
  the `spelllang` ↔ filename coupling (nvim derives the name from `'spelllang'` and
  `'encoding'`, so the filename must track `SPELL_LANG`).
- Tab-indented, matching the file. No Lua linter exists.

### `crates/wolf-core/tests/spell_durable.rs` (new)

A SIBLING file, not an extension of `spell.rs`, for three reasons the module doc must state:
(1) it sets `WOLF_STATE_DIR` process-wide before any core starts, and a peer `#[test]`
thread in the same binary calling the same set-if-absent helper is a real data race — a
binary with exactly ONE `#[test]` has no peer (the lever `recent_dirs_records.rs` uses);
(2) the spell dir is global, so every test in one binary shares one dictionary — isolation
by target is the only isolation that holds; (3) it starts two cores sequentially and should
not slow the existing fast target.

Local helpers duplicated from `spell.rs` (deliberate; `poll_until` already exists in two
files): `send_ex`, `toast_text`, `poll_until`, `echo`, `write_fixture`, `open`. Do NOT hoist
them into `tests/common/mod.rs` — that recompiles ~40 targets for two consumers.
The `echo` unique-tag discipline is not optional: ext_messages leaves the previous message
in the toast, so a fixed marker matches stale residue. Copy the monotonic-nonce scheme.

`BAD_WORD`: a fixed lowercase-ASCII nonsense word. Lowercase matters — unusual
capitalization triggers separate `spellbadword` rules. Step 2 below is the resolver for
whether the chosen literal is suitable.

Single test `zg_persists_the_user_dictionary_across_cores`:

1. `common::require_nvim_bin()`.
2. `tempfile::tempdir()` + `set_var("WOLF_STATE_DIR", …)` BEFORE anything touches the
   harness; hold the guard for the whole test.
3. cwd A: canonicalized tempdir (macOS `/tmp` → `/private/tmp`), like `swap_recovery.rs`.
   Write `note.md` whose FIRST LINE IS EXACTLY `BAD_WORD`, so `gg0` lands on its first char.
4. Core 1 via `config_in(cwd_a, …)` + `wake_channel()`; `wait_for_initial_flush`; open
   `note.md` via `InputEvent::Open`; `wait_for_flush`.
5. Precondition probe: filetype/`spell`/`spelllang` = `markdown`/`true`/`en`. Without it the
   rest can pass vacuously.
6. **Before** assertion: `spellbadword(BAD_WORD)[1]` == `BAD_WORD`. Load-bearing — guards
   against a word already in the dictionary.
7. Drive the REAL `zg` as keystrokes: `type_str(&core, "gg0zg")`. Not `:normal! zg`, not
   `:spellgood` — the point is the user's gesture. Never write the `.add` from Rust.
8. Poll until `<state>/spell/en.utf-8.add` exists; assert its contents contain a line equal
   to `BAD_WORD` after trimming. Poll, don't sleep — keystrokes arrive asynchronously.
9. Assert the compiled `en.utf-8.add.spl` sibling also exists (finding 4/5).
10. Same-session assertion: re-probe `spellbadword(BAD_WORD)` in core 1 ⇒ `<none>`.
    Separates "`zg` failed" from "persistence failed" when it goes red.
11. `core.shutdown()`, drop core + waiter so the child is gone.
12. cwd B: a DIFFERENT canonicalized tempdir + a markdown fixture (content need not contain
    `BAD_WORD`; it only has to open as markdown). Core 2 via `config_in(cwd_b, …)`.
13. Probe filetype/spell, then `spellbadword(BAD_WORD)` ⇒ `<none>`. The different cwd proves
    the dictionary is global; say so in the assertion message.
14. `core2.shutdown()`.

Finding 3's failure mode is caught implicitly: a comma-emptied `'spellfile'` sends `zg` to
the rtp default and step 8 times out.

### `crates/wolf-core/tests/state_unit.rs` (existing, already wired)

Add one headless `#[test]` `spell_dir_is_global_and_under_state_dir` after
`swap_dir_for_is_stable_per_cwd_and_distinct_across_cwds`, with a matching `// ====` banner.
Take `ENV_LOCK` first (this file's convention), set `WOLF_STATE_DIR` to a tempdir, call
`spell_dir()` twice, `remove_var`, assert: both `Some`, equal to each other, byte-equal to
`state.path().join("spell")`, and carrying NO cwd-derived component. Assert exact equality
rather than only `starts_with`, so a future cwd hash fails.

### `crates/wolf-core/BUILD.bazel`

New `rust_test` `spell_durable` immediately after the `spell` target, copying it field for
field: `srcs = ["tests/common/mod.rs", "tests/spell_durable.rs"]`, matching `crate_root`,
`tags = ["no-sandbox"]` with the sibling's verbatim comment, `size = "small"`,
`edition = "2021"`, the same `data` and `env`, the same `deps`. Do NOT put `WOLF_STATE_DIR`
in `env` — the test owns its own. Preceding comment in the house style.

### Docs (edit in place per CLAUDE.md; never append beside stale prose)

- `runtime/nvim/README.md` — line ~11: name both `WOLF_SWAP_DIR` and `WOLF_SPELL_DIR` in the
  launch-env sentence (edit, don't add). After the existing spell Decisions paragraph, ONE
  sibling paragraph (the swap decision already uses two adjacent paragraphs): points at
  `WOLF_SPELL_DIR/<spelllang>.utf-8.add` so `zg` survives a restart; global rather than
  buffer-local because a buffer-local set hits the `syntax on` hazard the paragraph above
  describes (refer, don't restate); `mkdir -p` first because nvim does not create the parent
  and `zg` then fails `E484` silently; commas escaped because a raw comma empties the option
  and makes `zg` look like it worked; absent/empty env var leaves the option alone. Do not
  restate the core-side resolution. Four sentences max, under 600 chars.
- `crates/wolf-core/src/README.md` — `state.rs` Public API: one bullet after `swap_dir_for`.
  `nvim.rs` Public API: edit the `NvimConfig::from_env` bullet in place to name both durable
  dirs and their env vars plus the ephemeral fallback. One bullet each; do not append.
- `crates/wolf-core/DECISIONS.md` — edit the "Hermetic launch" bullet in place: append a
  clause naming the two directories that deliberately escape the temp root and the single
  reason (their contents must outlive the process). Do not describe nvim-side option
  mechanics.
- `crates/wolf-core/tests/README.md` — new `#### spell_durable.rs` entry between `spell.rs`
  and `state_layout.rs` (kind line, **Covers**, **Gotchas** per the house template; the
  gotchas are the one-`#[test]` rule and the never-hand-write-the-`.add` rule). Edit the
  `state_unit.rs` entry's trailing clause in place to name `spell_dir`.
- `prettier --write` then `./tools/lint/markdown_check.sh` on every touched file. Present
  tense only — `markdown_check.sh` bans "now", "no longer", "previously", "the old", and
  this is a bug fix, so that temptation is strong.

---

## Implementation order

1. `state.rs` → 2. `nvim.rs` → 3. `api.rs` (workspace compiles again) → 4. `init.lua`
(feature live) → 5. `state_unit.rs` test → 6. `spell_durable.rs` → 7. `BUILD.bazel`, then
CONFIRM the target actually runs → 8. docs + prettier + markdown lint → 9. full run.

Full run: `bazelisk test //crates/... //bazel/... //runtime/...` — NOT `//...`, a stale
`.worktrees/` checkout breaks package loading (pre-existing, unrelated).

Regression targets to watch: `//crates/wolf-core:spell` (the `spelllang` literal became a
local), `:state_unit`, `:swap_recovery`, `:spawn` (the `from_env` signature and the `spawn`
env block moved).

## Risks (settled)

- **`from_env` callers**: exactly ONE in the repo (`api.rs`). `NvimConfig` is unconstructible
  outside the crate (`_state_guard` is private). Neither `wolf-ffi` nor `wolf-macos` names
  it. No Swift/FFI change, no `swift_test` `srcs` edit.
- **`WOLF_SPELL_DIR` collision**: zero existing occurrences in the repo. `WOLF_TEST_*` /
  `WOLF_NVIM_BIN` are Bazel→test-process; `WOLF_STATE_DIR` / `WOLF_RPC_TIMEOUT_MS` /
  `WOLF_LOG` are read by the core itself. No shadowing.
- **macOS `.app`**: nothing to stage. `state_dir()` resolves to the same
  `~/Library/Application Support/Wolf` the app already uses for `state.json` and the sqlite
  MRUs; the app is not sandboxed; `spawn` creates `spell/` on first launch. The block touches
  `'spellfile'` only, never `runtimepath`, so the vendored bundle siblings are unaffected.
- **Restart path**: `restart_nvim` re-spawns against the stored config, re-creates the dir
  and re-exports the var. A crash restart keeps the dictionary.
