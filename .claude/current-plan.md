# Plan: External-file-reload for the captive Neovim runtime

## Problem statement

Wolf is a Markdown editor built on a captive, hermetic Neovim spawned by
`wolf-core` and configured by `runtime/nvim/init.lua`. Today, if the file shown
in the active buffer is changed on disk by something OUTSIDE the editor — the
common cases being `prettier --write` (which the repo runs after every Markdown
edit) and `git checkout` — Neovim does not automatically pick up the new
contents. The buffer keeps showing stale text until the user manually reloads
(`:e`).

## Goal

Make the captive editor reload a buffer whose file was edited externally, as
long as the buffer has no unsaved in-editor modifications (never clobber the
user's unsaved work).

## Approach (from the request)

Use Neovim's built-in `autoread` + `:checktime` mechanism, implemented in
`runtime/nvim/init.lua`:

- Set `autoread = true` (reload an unmodified buffer when its file changes on
  disk, on the next `checktime`).
- Register autocmds that run `:checktime` on the events that should trigger a
  freshness check (focus gained / buffer enter / cursor hold), so an external
  change is picked up promptly rather than only on the next shell-out.
- Rely on `autoread`'s default guard: a buffer with unsaved modifications is
  NOT auto-reloaded.

## Deliverables

1. `runtime/nvim/init.lua` — the implementation.
2. `runtime/nvim/README.md` — document the new behavior (per repo convention:
   the README is the source of truth for this directory).
3. An integration test that starts a REAL Neovim process with this `init.lua`,
   opens a file, mutates it on disk externally, and asserts the buffer reloads
   (and, ideally, that an unsaved-modified buffer is NOT clobbered).

## Status

- [x] Architect plan
- [x] Tests written — `crates/wolf-core/tests/autoread.rs` (2 tests: load-bearing
      reload + modified-buffer guard). No API substitutions. BUILD wiring left to coder.
- [~] Implementation + tests pass — implementation SETTLED; tests being made faithful.
      **Decisive experiment result (coder):** production is CORRECT. On the natural
      event-loop path (FocusGained on refocus, or any interaction → CursorHold), an
      external change reloads AND repaints reliably (6/6, both callback forms). The
      ONLY non-repaint case is external change during continued idle with zero
      interaction — repaints instantly on any interaction (standard editor behavior;
      `:help CursorHold` is one-shot). No wolf-core redraw gap. **Final init.lua =
      architect's exact BARE form** (`pcall(vim.cmd, "silent! checktime")`, NO
      `vim.schedule` — that was only masking the synthetic `:doautocmd` cmdline
      context). Committed; `swap_exists` + 4 other real-nvim suites pass; scratch
      artifacts removed.

      **Faithful test recipe (test-writer to apply):** the original tests failed only
      because `:doautocmd CursorHold` fires a synthetic CursorHold from cmdline
      context and the test polled with NO interaction — unfaithful. Fix:
      - Test 1: open `# Original`; `:set updatetime=200<CR>` (test-only, settle ~300ms,
        drain); sleep ~1100ms; rewrite file to `# Reloaded externally...` (diff byte
        len); send ONE non-modifying interaction `ch('j')` then `ch('k')` (resets idle
        timer → a fresh CursorHold fires AFTER the change; NOT a post-reload nudge);
        poll (drain + ~50ms sleep, bounded by FLUSH_TIMEOUT), NO further input, until
        `row_text(0)` has `# Reloaded externally`; assert `!is_prompt()`. Header note:
        pure continued idle doesn't repaint (CursorHold one-shot); the cursor move is
        the faithful proxy for the FocusGained/interaction path the headless harness
        can't emit. Do NOT use `:doautocmd CursorHold`.
      - Test 2 (guard): same low-updatetime + `j`/`k` interaction to fire a natural
        CursorHold; poll asserting the guard (`ZZUNSAVED` still present AND `never
        applied` never appears); re-send a dismissing `key(Enter)` EACH poll iteration
        to clear the async W12 both-changed prompt so it can't block (Enter in normal
        mode is non-modifying/harmless). Load-bearing = the guard (unsaved edit not
        clobbered); the W12 confirm is expected/acceptable for modified+externally-
        changed, so drop `!is_prompt()` as load-bearing (best-effort only).
      NOTE (scope): the modified+externally-changed collision raising nvim's W12 prompt
      is ACCEPTED — the architect explicitly scoped out a FileChangedShell policy. The
      feature is unmodified-reload; the modified case preserves user edits. A
      non-blocking policy for the modified case (à la SwapExists) is a possible
      follow-up, not this change.
- [!] BLOCKED — pivotal findings (awaiting user scope decision):
      1. **No front-end forwards OS focus to nvim.** Repo-wide, `nvim_ui_set_focus`/
         focus-forwarding exists NOWHERE (Explore). macOS `windowDidBecomeKey`
         (`WolfWindowController.swift:775`) only marks the view current + rebinds panes;
         GNOME wires no window-active signal to core. So in the REAL product
         **`FocusGained` never fires** — the autocmd's headline trigger is dead. Reload
         would only fire on `BufEnter` (buffer switch) or a 4s `CursorHold` idle.
      2. **Background-reload repaint is genuinely unreliable.** `grid_line` is visible to
         `read_snapshot()` immediately, but wake/`seq` fire ONLY when nvim emits a `flush`
         in the batch (`redraw.rs:316-344`, `791`). A `:checktime` reload isn't guaranteed
         to ride a flush → coder measured 0/8–6/6 raciness; buffer reloads reliably, grid
         repaint doesn't. Forced `nvim__redraw` in the callback = 0/8.
      Net: the init.lua-only change (committed, `autoread` + `WolfAutoRead` bare form)
      does NOT deliver the requested UX on its own. Test 2 (guard) passes reliably; Test 1
      (reload→repaint) is flaky by nature. Real fix needs front-end focus forwarding
      (macOS + GNOME + a core/FFI focus method) AND resolving the multigrid-embed repaint
      reliability — both beyond the original "small init.lua change". ESCALATED to user.
- [x] **Repaint investigation — RESOLVED (repaint is FINE; it was a TEST BUG).**
      RAW protocol capture (instrumented `apply_redraw`, then reverted): after a
      background reload nvim emits `grid_line grid=2 row=0 text="Reloaded..."` FOLLOWED
      BY `flush`, **12/12**, identically for natural CursorHold / interactive
      `:checktime` / RPC `nvim_command("checktime")`. Attach-flag bisection: repaint
      works with ext_multigrid dropped, ext_messages dropped, or both — **no flag is at
      fault**. wolf-core applies it correctly. So NO init.lua change, NO core change, NO
      attach change.
      **Root cause of the earlier flakiness = TEST BUG:** Test 1's poll loop bound
      `let snap = core.read_snapshot();` and held that `parking_lot` read-guard across
      `waiter.drain()` + `sleep(50ms)` each iteration. `write_async()` spins on
      `try_write()` (`render.rs:1046`), which FAILS while any reader is held → the
      redraw handler is starved exactly when the reload's `grid_line` would apply → poll
      reads stale grid → timeout. Controlled interleaved proof (only variable = hold vs
      drop the guard): hold = 11/20 flaky; drop (read row into owned `String` first) =
      **20/20 interleaved, 25/25 standalone**.
      **FIX = test change only:** never hold a `SnapshotReadGuard` across `drain()`/
      `sleep`; read `row_text(0)` (and `toast.is_prompt()`) into owned values, dropping
      the guard before the drain/sleep. Apply to BOTH tests (Test 2 too, for robustness).
      init.lua stays the committed bare form. (Optional, NOT doing now: make
      `write_async` writer-preferring so a misbehaving reader can't starve it — the
      correct fix is callers not holding the lock across blocking ops.)
      NOTE: escalation concern #2 (unreliable repaint) is DISSOLVED. Concern #1 (no
      front-end OS-focus forwarding → `FocusGained` never fires in-app) REMAINS but is a
      SEPARATE optional enhancement — the feature works reliably via `BufEnter` (buffer
      switch / focus editor pane from the terminal drawer) and `CursorHold` (idle). The
      `FocusGained` entry in the autocmd list is harmless + standard and goes live for
      free if focus forwarding is added later.
- [x] Tests fixed (guard-drop) + all green — both tests read into owned locals, no
      `SnapshotReadGuard` held across drain/sleep. Coder verified: **40/40 each** (20
      native bazel `--runs_per_test` + 20 explicit), `swap_exists` regression PASSES,
      `//tools/lint:rustfmt` + `//tools/lint:clippy` PASS. init.lua unchanged (bare
      form); BUILD `autoread` target intact; `src/**` zero diffs (all instrumentation
      reverted). Flakiness GONE.
- [x] Review — reviewer APPROVED all 4 areas: init.lua bare form (no vim.schedule,
      placed after WolfSwap), both test poll loops correctly avoid holding the read-guard
      across drain/sleep, BUILD `autoread` target is an exact swap_exists analog, no stray
      instrumentation. Nothing that would reintroduce flakiness or make the feature wrong.
- [x] Docs verified — `runtime/nvim/README.md` bullet corrected (honest `FocusGained`-
      not-forwarded caveat added; no overclaim of instant-on-refocus). `tests/README.md`
      autoread entry corrected to the real faithful trigger (was stale `:doautocmd`).
      `architecture.md` verified — no init.lua/reload staleness. `prettier --write` run on
      both edited docs (unchanged — already conformant). Docs now match implementation.

## COMPLETE ✅ (not git-committed — working-tree changes only)

External-file reload landed: the captive Neovim now reloads a buffer whose file was
edited from OUTSIDE the editor (`prettier --write`, `git checkout`) as long as the
in-editor copy is unmodified. Implementation = `runtime/nvim/init.lua` `autoread = true`
+ a `WolfAutoRead` augroup running `pcall(vim.cmd, "silent! checktime")` on
FocusGained/BufEnter/CursorHold/CursorHoldI. Proven by `crates/wolf-core/tests/autoread.rs`
(real hermetic nvim): unmodified-reload (load-bearing) + modified-buffer guard, both
40/40 non-flaky; `swap_exists` regression + rustfmt + clippy green. Docs synced.

Change set (working tree, uncommitted): `runtime/nvim/init.lua`,
`crates/wolf-core/tests/autoread.rs` (new), `crates/wolf-core/BUILD.bazel`,
`runtime/nvim/README.md`, `crates/wolf-core/tests/README.md`.

Notable outcome of investigation: the "unreliable repaint" scare was a TEST lock-
starvation bug, NOT a product defect — nvim emits `grid_line`+`flush` on the reload
reliably (12/12, any attach config). One honest residual (OPTIONAL follow-up, not this
change): no front-end forwards OS window focus to nvim, so `FocusGained` doesn't fire
in-app — refocus-after-external-edit only reloads via `BufEnter` (buffer switch / focus
editor pane from terminal drawer) or after a `CursorHold` idle. Making refocus instant
needs front-end focus forwarding (macOS `windowDidBecomeKey` + GNOME window-active →
new wolf-core/FFI method → `nvim_ui_set_focus`).

---

## Architect's plan (authoritative)

### init.lua change

**Insertion point:** immediately after the `WolfSwap` `SwapExists` autocmd block
(after the closing `})` on line 37, before `opt.shadafile = "NONE"` on line 39).
Groups the two always-on augroup behaviors together; keeps it out of the
`wolf_setup_*` RPC helpers (it is top-level, not channel-driven).

Exact code:

```lua
-- External-file reload: when the file shown in a buffer is changed on disk from
-- OUTSIDE the editor (e.g. `prettier --write`, `git checkout`), reload it into the
-- buffer. `autoread` is Neovim's built-in mechanism, but on its own it only
-- re-reads on certain triggers, so a `WolfAutoRead` augroup runs `:checktime` on
-- focus regain, buffer entry, and after `updatetime` idle (`CursorHold`/`CursorHoldI`).
-- `:checktime` only re-reads a buffer whose in-editor copy is UNMODIFIED (autoread's
-- own guard — an unsaved edit is never clobbered) and is a NO-OP on buffers with no
-- associated file (the terminal `buftype=terminal` drawer and the `buftype=nofile`
-- preview scratch), so it is harmless there. Wrapped in pcall + `silent!` so a
-- special buffer can neither raise nor pollute the grid.
opt.autoread = true
local autoread_group = vim.api.nvim_create_augroup("WolfAutoRead", { clear = true })
vim.api.nvim_create_autocmd({ "FocusGained", "BufEnter", "CursorHold", "CursorHoldI" }, {
	group = autoread_group,
	pattern = "*",
	callback = function()
		pcall(vim.cmd, "silent! checktime")
	end,
})
```

Event rationale: `FocusGained` = primary GUI path (fires when the front-end UI
reports focus regain — the "edit externally, come back" flow); `BufEnter` =
switching back to a backgrounded buffer that changed; `CursorHold`/`CursorHoldI`
= eventual catch-all after `updatetime` idle. **Leave `updatetime` at default
(4000ms)** — lowering it globally has side effects; the focus/enter paths cover
the common flow. Do NOT add a `FileChangedShell` autocmd or touch the modified
path — autoread's default guard is exactly what we want. `pcall` + `silent!`
keep special buffers from raising or polluting the deterministic grid.

### README.md change (`runtime/nvim/README.md`)

Add ONE new top-level bullet to the "What it sets and why" list (lines 16-28),
immediately AFTER the **Hermetic / no on-disk side effects** bullet (line 17) —
it is the read-side complement to the hermetic write-side settings. Substance
(match the doc's "what + why" detailed style):

- **External-file reload (`autoread`)** — `autoread = true` + a `WolfAutoRead`
  augroup running `:checktime` on `FocusGained`/`BufEnter`/`CursorHold`/`CursorHoldI`.
- Why: a file edited OUTSIDE the editor (`prettier --write`, `git checkout`) is
  reloaded; autoread alone only re-reads on some triggers, so the augroup drives
  `:checktime` on focus regain, buffer entry, and idle.
- Guard: `:checktime` re-reads ONLY an UNMODIFIED buffer (unsaved edits never
  clobbered; a both-sides-changed buffer falls through to Neovim's normal warning).
- Harmless on special buffers: `:checktime` is a no-op with no associated file, so
  the `buftype=terminal` drawer and `buftype=nofile` preview scratch are unaffected;
  callback is `pcall`-wrapped + `silent!` (no raise, no grid pollution).
- `updatetime` left at default.

Run `prettier --write`; no hard line breaks inside the bullet.

### Integration test

**Create:** `crates/wolf-core/tests/autoread.rs`. **Mirror:**
`crates/wolf-core/tests/swap_exists.rs` (same crate, real-nvim policy, local
`start_core_in(cwd)` helper — copy VERBATIM, it is not in `common`; grid
assertions via `snap.active_grid().row_text(row)`; `!snap.toast.is_prompt()`
guard). Borrow keystroke Ex-command pattern from `current_file.rs`
(`ch(':')` + `type_str` + `key(Key::Enter)`) and the `:doautocmd` fire technique
from `theme.rs`.

Harness API (all in `tests/common/mod.rs`): `require_nvim_bin()`,
`config_in(cwd, cols, rows)`, `wake_channel()`, `WolfCore::start(cfg, wake)`,
`waiter.wait_for_initial_flush/.wait_for_flush(&core, FLUSH_TIMEOUT)`,
`waiter.drain()`, `core.send_input(InputEvent::Open { path })`, `common::ch`,
`common::key`, `common::type_str`, `common::key(Key::Enter)`,
`core.read_snapshot()`, `snap.active_grid().row_text(n)`, `snap.toast.is_prompt()`,
`core.shutdown()`.

**Test 1 (load-bearing) `external_change_reloads_unmodified_buffer`:**
1. `tempfile::tempdir()`; `cwd = canonicalize(dir.path())` (macOS `/tmp`→`/private/tmp`).
2. Write `note.md` = `# Original\nbody\n`.
3. `start_core_in(&cwd)`.
4. `Open { path: fixture }`; `wait_for_flush`; assert `row_text(0)` has `# Original`.
5. Sleep ~1100ms, then `std::fs::write(&fixture, "# Reloaded externally by the test\nbody\n")`
   — DIFFERENT byte length so size-change detection fires regardless of mtime granularity.
6. Fire trigger: type `:doautocmd CursorHold<CR>` (`ch(':')`, `type_str(&core,"doautocmd CursorHold")`,
   `key(Key::Enter)`). Exercises autocmd wiring + `autoread`. Fallback: `:checktime<CR>`.
7. Poll (don't assume single flush): loop until `row_text(0)` has `# Reloaded externally`,
   draining wakes + ~25ms sleep per iter, bounded by `FLUSH_TIMEOUT`; panic w/ last row on timeout.
   Then assert `!snap.toast.is_prompt()` (reload is silent).
8. `core.shutdown()`.

**Test 2 (guard) `modified_buffer_is_not_reloaded_by_checktime`:**
1-4. Same setup; assert `# Original`.
5. Unsaved in-editor edit: `ch('i')`, `type_str(&core,"ZZUNSAVED ")`, `key(Key::Esc)`;
   `wait_for_flush`; assert `ZZUNSAVED` on grid (buffer now modified).
6. Sleep ~1100ms, rewrite file with distinct different-size content
   (e.g. `# External change never applied\nbody\n`).
7. Fire `:doautocmd CursorHold<CR>` (or `:checktime<CR>`), then IMMEDIATELY queue a
   dismissing `key(Key::Enter)` (FIFO pump answers default `[O]K`, keeps modified buffer)
   in case the W12 both-changed prompt fires.
8. Poll/settle; assert `ZZUNSAVED` STILL on grid and `never applied` NOT present.
   Assert `!snap.toast.is_prompt()` after dismissal.
9. `core.shutdown()`.

Header comment in `swap_exists.rs` style; document load-bearing vs guard split and
W12 handling.

**BUILD wiring** (`crates/wolf-core/BUILD.bazel`): add a `rust_test` target `autoread`,
EXACT copy of the `swap_exists` target (lines 708-730) with
`srcs = ["tests/common/mod.rs", "tests/autoread.rs"]`, `crate_root = "tests/autoread.rs"`,
same `tags = ["no-sandbox"]`, `size`, `edition`,
`data = ["//bazel/nvim:nvim_tree", "//runtime/nvim:init.lua"]`, same three `env` entries
(`WOLF_NVIM_BIN`/`WOLF_TEST_INIT_LUA`/`WOLF_TEST_NVIM_DIR`),
`deps = [":wolf_core"] + all_crate_deps(normal = True, normal_dev = True)`. Preceding
comment block matching neighbors.

**Test README** (`crates/wolf-core/tests/README.md`): add a short subsection for the
`autoread` binary (unmodified-reload load-bearing case + modified-buffer guard, real
hermetic nvim w/ repo init.lua). `prettier --write`.

### Verification commands (coder runs)

- `bazel test //crates/wolf-core:autoread`
- Regression: `bazel test //crates/wolf-core:swap_exists //crates/wolf-core:current_file`
- Cargo alt (Linux devShell, `direnv allow` first): `cargo test -p wolf-core --test autoread`

### Risks / edge cases

- **W12 both-changed prompt (Test 2):** modified + file-changed → `FileChangedShell`
  → warning/prompt that can block the flush wait (like swap E325). Mitigate w/ a
  dismissing `Enter`; make the load-bearing assertion "unsaved marker still on grid".
  If flaky, keep Test 2 best-effort or drop it (Test 1 is load-bearing). Test 1
  (unmodified) is prompt-free by design.
- **Change detection:** `:checktime` compares mtime/mode/size. Make the rewrite a
  DIFFERENT byte length (size mismatch always triggers) AND sleep ~1.1s first.
- **`:doautocmd` vs `:checktime`:** prefer `:doautocmd CursorHold` (tests full feature);
  fall back to `:checktime` if the embedded nvim doesn't run the callback deterministically.
- **Special buffers:** `:checktime` skips no-file buffers → terminal + preview scratch
  are no-ops; `pcall`/`silent!` defensive. No interaction w/ `swapfile=false`. A reload
  advancing `changedtick` is a benign, correct preview-refresh signal.
- **`FocusGained` in embedded nvim:** fires only when the front-end reports focus; a
  headless test can't easily emit it — hence firing `CursorHold` via `:doautocmd`. Test
  limitation, not a runtime one.
- **Lua/lint:** init.lua not clippy-linted. `tests/autoread.rs` needs no clippy allows
  (swap_exists.rs needs none; common/mod.rs already `#![allow(dead_code)]`). Markdown:
  no hard line breaks; `prettier --write` both READMEs.
