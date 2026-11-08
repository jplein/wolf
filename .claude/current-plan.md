# Plan: preview survives pane rebuild without a portal; hermetic smoke bus; picker tolerance

## Status

COMPLETE. Final: macOS ./gates.sh all 0; Linux host ./gates.sh all 0 (smoke_headless 75 passed); Podman smoke_headless 75 passed; packaged smokes pass; pane-origin constants bit-identical; text-scaling risk measured absent (portal dpi present at activate). Earlier: Test-writer done (smoke_headless.rs only): hermetic bus (smoke_bus_config, no servicedir/standard_session_servicedirs/include), GSETTINGS_BACKEND=memory, guard checks 10-13, T1 preview_survives_a_layout_rebuild_under_cage (assertion 8 discriminator; bus offers only org.freedesktop.DBus), T2 row 10, T4 rgb_within + headless test; expected 75 passed after fix. Orchestrator accepts the stricter <include ban and activatable-names assertion. Product fix done: app.rs FALLBACK_XFT_DPI + pin_unset_xft_dpi() (sha fc89be78…); mirror notes/crates/wolf-gnome/src/app.rs.md. RED before fix (new-view smoke a7 ink 0; preview smoke a10). GREEN: host hermetic 75 passed (×3 preview smokes), host with portal preview ok, Podman (--shm-size 0) 3 smokes + full ./gates.sh 75 passed; falsifiers checks 12/14/15/16 + pin removed all red; Linux host + macOS ./gates.sh all 0. Extents: control 787–1260, sample 959–1260; ink is the discriminator. Docs done (lint clean). Reviewer: product fix + harness correct; needs text fixes in smoke_headless.rs (failure-shape text, measured readings, false 'go red' claim, stale check 13 msg, packaged doc list, schema wording) + recommended host-independent precondition (app prints `wolf-xft-dpi-pinned:` stderr line; smokes assert it). Main risk to measure: portal text-scaling arriving after activate (APPLICATION source outranks) — coder measuring in a private env. Then: falsifier evidence (rgb exact, check 13, assertion 2 w/ servicedirs, F3), packaged smokes, pane-origin constants; tech-writer: 'Five details' count + late-session-value gotcha. Doc follow-ups: tests README counts/conventions, notes/wolf-gnome-decisions.md:343, flake.nix:314-315 comment.

## Plan

Full spec: `/private/tmp/claude-501/-Users-jplein-dev-jplein-wolf--worktrees-feat-documentation-checks-and-updates/2dea3bae-3b77-4aca-b134-5ba6d490bcba/scratchpad/portal-plan.md`. Orchestrator decisions: accept; product fix chosen by experiment (A > B > D); hermetic service-less bus + GSETTINGS_BACKEND=memory; picker ±1 tolerance; no forced renderer; no hermetic portal in this unit.

---

# Plan: long-path swap names (wolf bug found by the regina trial)

## Status

COMPLETE. Fix implemented (init.lua); macOS + Linux ./gates.sh exit 0; RED before fix (T1-T4) and all falsifiers red; deep TMPDIR (241B) reproduced 3 failures before, green after. Deviation: wolf_name_untitled calls sync_long_path_swap directly (VimEnter not nested); covered by startup_untitled_in_a_long_cwd_gets_no_swap (red without the call). Docs done (lint clean). Final: macOS + Linux ./gates.sh exit 0 (init.lua sha 3116292…). Reviewer approved w/ notes; follow-ups done: root budget 222 (derived; verified with /tmp read-only + 200B TMPDIR, falsifier cap=100 red), re-enable guard test (red w/o guard: ATTENTION as wmsg toast, save never written; message reworded), README precision. COMPLETE pending the combined final gates run (re-run the wmsg falsifier then). Side finding: restart_recovers_unsaved_untitled_buffer fails under unshare -r (uid 0); regina Podman uses keep-id, so not hit.

## Plan

Full spec: `/private/tmp/claude-501/-Users-jplein-dev-jplein-wolf--worktrees-feat-documentation-checks-and-updates/2dea3bae-3b77-4aca-b134-5ba6d490bcba/scratchpad/swap-plan.md`. Orchestrator decisions: accept as written (noswapfile past 250 bytes is a documented limitation; recovery for those paths is already broken today). specs/architecture.md exception edit is in scope (spec must match code).

---

# Plan: regina harness for wolf (`.claude/skills/regina`)

## Status

READY (blockers fixed: regina 7b72b1e grants + shm, wolf swap + dpi fixes, harness review fixes). Next: commit, Trial re-run (Stage 1+2) with regina 7b72b1e, then backlog. Harness implemented (untracked .claude/skills/regina + ignores + doc-lint .workspace skip, RED/GREEN shown). Trial Stage 1 found blockers: M3 macOS Seatbelt (actool plugin load; 7 real-nvim targets with :terminal/save/swap time out), M4 Linux Podman (dbus-run-session: no /etc/dbus-1/session.conf; 60/73 smoke_headless fail).

## Request

Step 2 of the user's 2026-11-07 request: run every open item through regina (`~/dev/jplein/regina`) with the ssh worker `nixos.local`. Needs a wolf harness (flow, config template, run script, regina-protocol agent definitions, SKILL.md, README) and a trial that proves `gates.sh` under regina's real sandboxes.

## Plan

The full architect plan is at `/private/tmp/claude-501/-Users-jplein-dev-jplein-wolf--worktrees-feat-documentation-checks-and-updates/2dea3bae-3b77-4aca-b134-5ba6d490bcba/scratchpad/harness-plan.md` (sections 3–9: files with full flow.yaml/template, agents, files to modify, test plan, order, trial procedure, risks). Accepted with these orchestrator decisions:

- Implement sections 3, 4 (items 1–7; CLAUDE.md item 8 is proposal-only, for the user), and 5.
- Implementation order step 4 ("commit on the feature branch") is NOT done: committing waits for the user. The trial uses a scratch clone with a trial commit (section 7.2), which needs no commit on the user's branch.
- Trial Stage 1 required; Stage 2 (scripted five-agent flow) required too if Stage 1 passes, since the write sets and docs gate are otherwise first exercised on paid tasks.
- Record M3–M6 in the progress log; if M3 (actool/codesign under Seatbelt) or M4 (WebKit/EGL in Podman) blocks, stop and report — no workaround without orchestrator sign-off.

## Progress log

- Coder: harness files created; RED/GREEN for doc-lint `.workspace`; `regina flow validate` exit 0; registration + refusal falsifiers OK; Trial Stage 1 evidence under scratchpad/trial/. M5: remote target/ reused within a task (build 33s -> 0s). M6: Linux image build ~2m first gate; bazelisk + Bazel output root (5.2G) per task; remote cargo registry per task.
- User decisions (2026-11-07): M3 -> widen regina's Seatbelt profile via a regina run on the regina repo (with the earlier FSEvents grant), each grant measured first on the host; M4 -> fix in wolf (smoke harness passes the Nix dbus session.conf); commit permission granted on feat/documentation-checks-and-updates once blockers are fixed, one commit per finished piece; CLAUDE.md edits approved (plan item 8: line 7 regina exception + lines 15/16/108/118 gates.sh wording).
- (A) done: orchestrator applied the user-approved CLAUDE.md edits directly (technical-writer declined relayed approval); lines 7, 15, 16, 108, 118. Needs prettier + markdown_check in the next coder run.
- D-Bus fix verified: NixOS host smoke_headless 74 passed; falsifiers A (check 11) and B (new guard) red; Podman: 127s gone. Linux `fmt 1` on smoke_headless.rs (rustfmt reflow, 2 spots) -> formatting fix pending.
- Podman residuals: /dev/shm 63M -> SIGBUS in 3-6 smokes (2G fixes all); WebKit with no GL: `preview_pane_renders_and_edit_source_returns_under_cage` (blank WebView after split/reparent) and `file_picker_covers_the_live_preview_under_cage` (highlight off by 1/channel) fail in every container run, pass on host.
- User decisions: WebKit -> hermetic software GL (Mesa llvmpipe) in linuxShell, re-measure; if blank-after-split persists under software GL it is a real wolf bug -> a task. shm -> regina change: a Podman `--shm-size` setting (worker key), folded into the regina sandbox task; wolf's template sets 2 GB.
- Seatbelt diagnosis (host, real rendered profile): grants measured per failure + ablation; full wolf gate 114/114 warm in a reproduced worker. save_open/swap_recovery failures are NOT sandbox: Neovim swap-name truncation >255 bytes with `//` directory -> collisions (wolf bug; architect planning fix). wolf-ffi:history ~8.3s vs 10s wait (flake risk under load).
- Regina run started 2026-11-07 (user "Go"): regina repo, branch tmp/main @8750493, label sandbox-widen, tasks 1 seatbelt-grants, 2 podman-shm (task files in regina/.workspace/tmp/main/tasks/sandbox-widen). Log: regina/.workspace/tmp/main/run-sandbox-widen.log. After landing: user reviews/merges tmp/main into regina main; rebuild regina on both hosts (remote rebuild needs user OK).
- smoke_headless.rs rustfmt applied (Linux cargo fmt --check rc 0; sha 57c46f6c…; only reflow + one closure brace wrap).
- Software GL (Mesa llvmpipe, env only) does NOT fix the 2 container WebKit failures (host with forced llvmpipe passes; container with llvmpipe still blank-after-split). Ruled out: GL renderer, DMA-BUF, bwrap sandbox, render node, shm. Not adopting hermetic llvmpipe for now (no fix; +6s host; extra container flake). Time-boxed container diagnosis running (passwd, /etc files, sched caps, WebKit knobs, wolf reparent path).
- WebKit-in-container root variable found: xdg-desktop-portal. Host passes only because the private test bus can activate the host's portal (via inherited XDG_DATA_DIRS /run/current-system/sw/share); host + GDK_DEBUG=no-portals reproduces the blank-after-split exactly. Real wolf bug (preview dead after reparent without a portal). Picker: cairo renderer draws (53,73,100) vs (54,74,101) — test precision. Architect planning fix (product + hermetic smokes + picker tolerance).
- Harness follow-up (from swap review): regina's per-task Bazel TEST_TMPDIR for this branch ≈ 215 bytes vs short_tempdir's 222-byte root budget — too tight. Harness change needed: regina.conf.template `state` at a short absolute path (e.g. $HOME/.cache/wolf-regina/<8-hex of branch>) instead of `.workspace/<branch>/state`; saves ~60 bytes. Do it with the harness review fixes.
- Regina run sandbox-widen: both tasks complete in 1 attempt; landed 8 commits on regina tmp/main @7b72b1e (finish done); regina ./gates.sh on tmp/main all 0. Mechanism: seatbelt grants per task file; Podman `--shm-size` 0 (unlimited) for host, 64 MiB for light; no protocol change. Live tests (pty, shm) being run outside the sandbox by a coder. After that: user merges tmp/main → regina main; rebuild regina on both hosts.
- Regina live tests verified outside sandbox (pty deny/allow red/green on macOS; shm host unlimited (128 MiB dd ok) / light 64 MiB red/green on Linux). Pre-existing regina issue: live test `a_worker_of_another_state_root_survives_a_sweep` fails on Podman 6.1.4 (`.Labels` index template); doc nit: Podman default shm is 64000k not 64 MiB.
- User chose "merge locally, push, rebuild both": regina main ff 8750493→7b72b1e, pushed to GitHub; nixos.local pulled + rebuilt (result → rcsrfcf7…, worker image ysjn5vrc…); local regina rebuilt (wm2q9y57…).
- Remaining before backlog: portal fix (coder), harness review + state-root shortening, commits, Trial re-run (Stage 1+2) with new regina, backlog task files + user go-ahead.
- Harness review: Needs revision — blocking (1) state root length (fix: state at $HOME/.local/state/wolf-regina/<8-hex of main+branch>, ~175 B TEST_TMPDIR) and (2) test-file notes mirrors not writable by any agent (add test-notes set to test-writer); plus minors (retrospective shell rule, tech-writer contradiction, REGINA_SOURCE checks incl. ancestor 7b72b1e, README measurements, SKILL wrapper usage, fixtures.rs rewrap, .prettierignore comment). Coder applying. NOTE: during a wolf regina run the starting checkout must not change — do not edit tracked .claude/current-plan.md then; keep orchestration notes in the scratchpad.
- Harness review fixes applied (coder): state root $HOME/.local/state/wolf-regina/<8hex> (47 B; est TEST_TMPDIR ~178 B; cap 60), $run/state symlink; test-notes set (test-writer only; coder loses it — accepted); REGINA_SOURCE checks incl. ancestor 7b72b1e; retrospective shell rule; tech-writer rejected wording; README measurements; SKILL wrapper usage, one run at a time, remote state step; fixtures.rs rewrap; .gitignore/.prettierignore comments. Checks: shellcheck clean, flow validate 0, trial start/finish refusals all as expected, doc-lint tests 4/4, macOS ./gates.sh 0. Measure real TEST_TMPDIR in the next trial.
- Next: (B) test-writer dbus fix in smoke_headless.rs, then coder verifies on Linux Podman (trial Linux-only) and measures WebKit/preview; (C) host-side diagnosis of minimal Seatbelt grants (actool, pty, save/swap, FSEvents) -> regina task.

---

# Previous task (complete)

# Plan: root `gates.sh` for regina (both platforms)

## Status

COMPLETE (uncommitted). gates.sh green on macOS (all 5 gates) and Linux nixos.local cold clone (all 11 gates, 3m51s). Reviewer approved with minor notes; doc fixes applied (build.md Under regina, notes/gates.sh.md), message reworded, shellcheck clean. Pending user: commit; CLAUDE.md edits (lines 16, 118 -> gates.sh). Next: regina harness (architect planning).

## Request (from the user, 2026-11-07)

"ok let's go through all of the open items. We'll use a tool to orchestrate it: ~/dev/jplein/regina/, and you'll use its ssh workers feature to fix it on Linux, too. The Linux host is nixos.local and on that machine the regina repo is in /home/jplein/dev/jplein/regina. Regina expects a script named 'gates.sh' to exist at the root of the repo which includes all of the deterministic quality gates, so build this script so that it detects the platform and runs the appropriate commands to build, test, lint, etc., and verify that it runs both locally and on the remote host."

This plan covers step 1 only: `gates.sh`. The open-items sweep through regina comes after.

## Facts (orchestrator)

- Regina (`/Users/jplein/dev/jplein/regina`, docs `usage.md`, `crates/regina-cli/README.md`): with no `gate-default`, a gate runs `./gates.sh` from the repo root. Under `workflow = nix` regina runs the gate inside the flake's dev shell already. A remote secondary (`host = nixos.local`) runs the gate in a Podman worker on a snapshot of the workspace (uncommitted files included, git-ignored excluded); the agent sees only that run's gate log (stdout+stderr), never the remote's files. On macOS the primary runs under Seatbelt (`sandbox-exec`) with HOME = the task's home; the dev shell sets its own `DEVELOPER_DIR` (wolf's darwinShell exports `/Applications/Xcode.app/Contents/Developer`).
- Regina's own `gates.sh` (reference): runs each gate in turn (no fail-fast), logs to `target/gates/<gate>.log`, prints `<gate> <exit>` per gate then `logs: <dir>`, exits 1 if any failed, 2 on setup error; TMPDIR is a mktemp dir removed on exit.
- Wolf gates: Linux = build.md "The verification gate" list (cargo fmt x2, prettier, markdown_all.sh, markdown_check_test.sh, doc-lint clippy/test/--root ., workspace clippy, build, test). macOS = `bazel test //...` (every non-manual target, including all `//tools/lint` checks and the `:Wolf` build via `bundle_layout_test`). prettier is only in linuxShell.
- Branch: work on `feat/documentation-checks-and-updates` (HEAD 9247127); local `main` is 7 behind; nixos.local's checkout `/home/jplein/dev/jplein/wolf` is on main@24af640 — do not touch it.

## Draft design (orchestrator; architect to confirm or correct)

1. `uname -s`: Darwin → Bazel gates; Linux → build.md's list; else exit 2.
2. If not inside the dev shell (`IN_NIX_SHELL` unset), re-exec under `nix develop --command`; inside regina it already is.
3. One gate per step, run all (no fail-fast), log per gate under `target/gates/`, `<gate> <exit>` lines, then for each failed gate print the tail of its log to stdout (remote failures must be diagnosable from the gate log alone), exit 0/1/2.
4. TMPDIR = mktemp dir removed on exit.
5. Docs: build.md owns the commands; the Linux "verification gate" section should point at `gates.sh` instead of duplicating the list. Root `comment_srcs` must include `gates.sh` (doc-lint comment limits).

## Decisions (orchestrator, after architect)

- Architect plan accepted in full (D1 gate lists, D2 `WOLF_DEV_SHELL=1` marker in both shellHooks + one-shot `nix develop` re-entry with loop guard, D3 Linux `/tmp/wolf-gates.XXXXXX` TMPDIR handoff (cage 71-byte budget), none on macOS, D4 output contract with failure excerpt + tail, exit 0/1/2, bash 3.2 safe).
- D5 adopted: `pkgs.prettier` in darwinShell; macOS gets a `prettier` gate.
- `WolfRelease` stays out of the gate.
- No test-writer pass: gates.sh gets no automated test (it would recurse into the suite); the falsifiers in the architect's section 4 are the evidence.
- CLAUDE.md: one minimal edit so the lint-script wiring rule names `gates.sh`'s Linux list instead of "the Linux gate in build.md" (it would otherwise be false). Report to user.
- Files: new `gates.sh` (100755), `notes/gates.sh.md`; edit `flake.nix`, root `BUILD.bazel` (`"*.sh"` in comment_srcs), `build.md` (top-level `## The verification gate`, `### Under regina`), `notes/flake.nix.md`, CLAUDE.md line ~16, optionally `crates/wolf-gnome/tests/smoke_headless.rs:410` comment.
- Verification order: macOS local (outer + inner paths, falsifiers 1-5, 7, 8; M1, M2) -> nixos.local scratch dir (`/home/jplein/scratch/wolf-gates.*`, never `/home/jplein/dev/jplein/wolf`; outer cold run, behavior 6, inner falsifier run) -> regina trial with custom `/bin/sh` worker (later step).

## Architect plan (accepted; condensed by orchestrator, specifics kept)

### Findings
- Regina wraps the gate itself: `crates/regina-cli/src/config.rs:155` `NIX_GATE_WRAPPER` runs `exec nix develop --command "$@"` around `sh -c './gates.sh'` (marks an untracked flake.nix intent-to-add). So under regina gates.sh starts inside the shell; it must not wrap again.
- `IN_NIX_SHELL` is not wolf-specific; use a marker `WOLF_DEV_SHELL=1` exported by both shellHooks.
- Bazel under Seatbelt: darwin-sandbox can't nest; Bazel falls back to processwrapper-sandbox on its own (measured by regina on 8.4.2) — no flag. Regina writes `$HOME/.bazelrc` with `startup --output_user_root=<home>/.cache/bazel` (per task: cold first build) and `common --repository_cache=<cache root>/bazel-repository-cache` (shared). Regina kills the Bazel server at worker end: no `bazel shutdown`/`--max_idle_secs`. Unmeasured by regina: `actool` (`:Wolf` compiles `Wolf.icon`), xcodeproj. Seatbelt reads only regina's chosen Xcode; wolf hardcodes `/Applications/Xcode.app` (matches `xcode-select -p` here). Seatbelt grants only the worker's own TMPDIR; `/tmp` denied.
- Podman profiles: `light` 4 GiB/2 CPU/512 pids, `host` no limits; no device nodes (no /dev/dri). WebKit dlopens host Mesa from `/run/opengl-driver` (`notes/flake.nix.md`) — preview smokes may fail in Podman (measurement M4).
- Linux TMPDIR budget: `crates/wolf-gnome/tests/smoke_headless.rs` asserts `$TMPDIR/wolf-smoke-rt-<pid>-<seq>` <= `RUNTIME_DIR_MAX_BYTES` (71). `nix develop` nests TMPDIR under `nix-shell.XXXXXX` (+17). So base `/tmp/wolf-gates.XXXXXX`, never nested.
- `bazel test //...` builds every non-manual target (`:Wolf`, all `//tools/lint` checks incl. clippy, rustfmt_*, swift_format, markdown, markdown_check_test, comments; doc-lint rust_tests). Misses: doc-lint `--root`-only rules (md-link, md-anchor, notes-orphan, notes-unlinked, bazel-coverage), prettier, manual targets.
- Outside the shell on macOS `#!/usr/bin/env bash` is bash 3.2: no mapfile, assoc arrays, `${x,,}`, `wait -n`, `|&`; guard empty arrays under `set -u`. `markdown_all.sh` uses mapfile → Linux only.

### D1 gate lists
Linux (order fast→slow; name = log name):
fmt `cargo fmt --all -- --check`; fmt-doc-lint `cargo fmt --manifest-path tools/doc-lint/Cargo.toml -- --check`; prettier `prettier --check "**/*.md"`; markdown `./tools/lint/markdown_all.sh`; markdown-check-test `./tools/lint/markdown_check_test.sh`; clippy-doc-lint `cargo clippy --manifest-path tools/doc-lint/Cargo.toml --all-targets -- -D warnings`; test-doc-lint `cargo test --manifest-path tools/doc-lint/Cargo.toml`; doc-lint `cargo run --quiet --manifest-path tools/doc-lint/Cargo.toml -- --root .`; clippy `cargo clippy --workspace --all-targets -- -D warnings`; build `cargo build --workspace --all-targets`; test `cargo test --workspace --no-fail-fast` (no --all-targets: it drops doctests).
macOS: prettier `prettier --check "**/*.md"`; doc-lint `bazel run --color=no --curses=no //tools/doc-lint:doc_lint -- --root "$PWD"`; lint `bazel test <B> <T> //tools/lint:check`; build `bazel build <B> //...`; test `bazel test <B> <T> //...`. <B> = `--keep_going --color=no --curses=no --show_progress_rate_limit=30`; <T> = `--test_output=errors --test_summary=terse`. WolfRelease excluded.

### D2 dev shell
`WOLF_DEV_SHELL=1` → run gates directly. Else (needs `nix`, else exit 2) run `nix --extra-experimental-features 'nix-command flakes' develop --command ./gates.sh` as a child (not exec), with `WOLF_GATES_REENTERED=1`; inner run seeing REENTERED without the marker → exit 2 "nix develop did not export WOLF_DEV_SHELL; check flake.nix's shellHook". Outer removes `target/gates` first; inner creates it first thing; if missing afterwards → `gates.sh: nix develop failed (exit N) before any gate ran`, exit 2. Print `gates.sh: entering the dev shell` on stderr when entering. Stale-direnv caveat documented.

### D3 TMPDIR
Linux outer: `tmp=$(mktemp -d /tmp/wolf-gates.XXXXXX)`, trap rm on EXIT, export `TMPDIR=$tmp WOLF_GATES_TMPDIR=$tmp`. Inner: if `WOLF_GATES_TMPDIR` set, `TMPDIR=$WOLF_GATES_TMPDIR`; else create its own `/tmp/wolf-gates.XXXXXX` + trap. macOS: no override.

### D4 output
Each gate `</dev/null >"$logs/<gate>.log" 2>&1`; log header `gates.sh: <gate>: <cmd>` and footer `gates.sh: <gate> exited <rc> after <n>s`. stdout: `<gate> <rc>` as each finishes; then per failed gate a block `--- <gate> (exit <rc>): <log> ---` … `--- end <gate> ---` holding the whole log if <=150 lines, else an excerpt `grep -n -E -A 40 '<pats>' | head -n 400` (pats: `^---- .* stdout ----$`, `^error(\[E[0-9]+\])?: `, `^(ERROR|FAIL): `, `^==================== Test output for `, `^Diff in `, `^\[warn\] `) then `tail -n 150`; last line `logs: <dir>`. Exit 0 all pass / 1 any fail / 2 setup (unknown platform, any argument, nix missing outside shell, mkdir/mktemp fail, loop guard, nix develop failed, Darwin precheck `xcrun --find swift-format` failing → message naming DEVELOPER_DIR and regina's `xcode`). `set -uo pipefail` (no -e); `trap 'exit 130' INT`, `trap 'exit 143' TERM`; `cd "$(dirname "${BASH_SOURCE[0]}")" || exit 2` first; failures recorded in a space-separated string.

### Files
- `gates.sh` (100755): 2-line header comment; comments ≤2 lines ≤100 cols, rationale links `See notes/gates.sh.md#<anchor>`; avoid doc-lint workflow-ref words ("the coder", "the architect", "test-writer", "per plan").
- `notes/gates.sh.md`: H1, blank, `Last verified at commit \`<git rev-parse HEAD>\`.`; headings `## Dev shell entry`, `## Temporary directory`, `## Gate order`, `## Linux gates`, `## macOS gates`, `## Failure output` — each linked from a gates.sh comment (else notes-unlinked). Content moved from build.md's verification-gate rationale.
- Root `BUILD.bazel`: add `"*.sh"` to `comment_srcs` include list after `"*.nix"`.
- `flake.nix`: `export WOLF_DEV_SHELL=1` in both shellHooks, comment `# gates.sh reads this to skip entering the shell again.` / `# See notes/gates.sh.md#dev-shell-entry`; `pkgs.prettier` in darwinShell packages.
- `notes/flake.nix.md`: Darwin shell sentence listing contents gets prettier; bump SHA only if whole file re-checked.
- `build.md`: drop Linux `### The verification gate`; add top-level `## The verification gate` after "Linting and formatting" (same heading → anchors survive): gates.sh is the definition of green on both platforms; runs in or out of the shell (outside needs only nix); after a flake.nix edit use `nix develop --command ./gates.sh`; output contract + exit codes; logs in `target/gates/`; keep "roughly 10–25 minutes cold" for Linux; no copied gate table. `### Under regina`: `workflow = nix`, no `gate-default`, regina already wraps in nix develop; `[queue] network = unrestricted`; macOS: `xcode` must resolve to `/Applications/Xcode.app`, per-task output root (cold first build), shared repository cache, Bazel sandbox falls back by itself; Linux secondary: `[worker <project> <name>]` with `host`, `start = <path>/result/bin/regina serve`, `workflow = nix`, `profile = host`, a `timeout` (`light` too small); base branch must contain gates.sh and the flake marker before enqueueing; no unmeasured numbers. One pointer sentence each in "Running the tests" and "Linting and formatting". Keep line ~78 and ~138 as they are.
- CLAUDE.md: minimal edit of the lint-script bullet ("a line in the Linux gate in `build.md`" → gates.sh's Linux gate list). Optional: `crates/wolf-gnome/tests/smoke_headless.rs:410` comment citation.

### Falsifiers (section 4)
1. Untracked `notes/gates-falsifier.md` = `# Falsifier\n\nNo header line.\n` → Linux `markdown 1`; macOS `lint 1`, `test 1`; others 0; stdout has `[notes-header]` and the filename; exit 1. Falsify: drop `status=1` in run() (exit 0); remove print_failure loop (no finding on stdout).
2. macOS long `test` log: excerpt shows `==================== Test output for //tools/lint:markdown`; deleting that pattern loses it.
3. `env -i HOME="$HOME" PATH=/usr/bin:/bin ./gates.sh` → "nix not found", exit 2.
4. Inside the shell no `entering the dev shell` on stderr; clean-env run prints it. Falsify: invert the marker test.
5. `WOLF_GATES_REENTERED=1 env -u WOLF_DEV_SHELL ./gates.sh` → exit 2 shellHook message.
6. Linux: no `/tmp/wolf-gates.*` and no new `/tmp/nix-shell.*` left after an outer run; cage smokes pass. Falsify: inner mktemp under `${TMPDIR}` inside a nix develop → `RUNTIME_DIR_MAX_BYTES` panic.
7. Third consecutive `#` line in a gates.sh comment → `bazel test //tools/lint:comments` fails (and doc-lint `--files gates.sh`); with the `"*.sh"` BUILD line reverted the same bad comment passes `:comments`.
8. Rename a `##` heading in notes/gates.sh.md → `doc-lint` gate reports notes-unlinked / doc-anchor.
Measurements: M1 prettier glob vs `bazel-*` symlinks on macOS (wall time; if it walks the output base, feed prettier `git ls-files -z --cached --others --exclude-standard -- '*.md'` on both platforms); M2 `command -v cc strip xcrun` still /usr/bin in darwinShell after adding prettier; M3–M7 belong to the later regina trial.

### Verification
macOS: `env -i HOME="$HOME" USER="$USER" TERM=dumb PATH=/nix/var/nix/profiles/default/bin:/usr/bin:/bin:/usr/sbin:/sbin ./gates.sh` → all 0; `git status --porcelain` unchanged besides intended files; falsifiers via `nix develop --command ./gates.sh`.
Linux (nixos.local, BatchMode ssh): `git bundle create $S/wolf.bundle feat/documentation-checks-and-updates`; `git ls-files -z -m -o --exclude-standard | tar --null -T - -czf $S/wip.tgz`; check no cargo running (`pgrep -a cargo`); `D=/home/jplein/scratch/wolf-gates.<ts>`; clone bundle + untar wip there; run detached (`setsid nohup … ./gates.sh > ../run1.out 2>&1; echo $? > ../run1.rc`), poll; behavior 6; then falsifier run via `nix develop --command ./gates.sh`; remove `$D` at the end. Never touch `/home/jplein/dev/jplein/wolf`.

## Progress log (gates.sh)

- Coder: gates.sh, notes/gates.sh.md, flake.nix marker + darwin prettier, BUILD.bazel `*.sh`, build.md, notes/flake.nix.md. macOS clean-env run: prettier 0, doc-lint 0, lint 0, build 0, test 0 (31s warm). Falsifiers F1 (lint 3/test 3, rc 1), F1a, F1b, F2/F2a/F2b, F3 (rc 2), F4, F5, F7 (+control), F8 all as expected; Bazel test failure exits 3, not 1. M1: glob does not follow symlinked dirs, 1.15s. M2: cc/strip/xcrun/ld still /usr/bin.
- Linux nixos.local cold run 4m10s: all gates 0 except `test 101` from `smoke_headless.rs` guard: WOLF_DEV_SHELL not in PACKAGED_SCRUB_ALWAYS/keep-list. Cage smokes pass; behavior 6 holds.
- Orchestrator decisions: classify WOLF_DEV_SHELL in PACKAGED_SCRUB_ALWAYS (test-writer; also fix smoke_headless.rs:410 citation "The Linux verification gate" -> build.md's "The verification gate"). CLAUDE.md lint-script bullet edit -> technical-writer (orchestrator authorizes: it is in scope of the user's gates.sh request; report to user). macOS leaves one /tmp/nix-shell.* per outer run: accepted, documented (wrapping TMPDIR on macOS would pull it from under a surviving Bazel server).

## User decisions for the open-items sweep (2026-11-07)

D0 prune the ledger (a). D1+D4 ship on test evidence, give the user an eyeball checklist (a). D7 add com.apple.FSEvents to regina's Seatbelt MACH_SERVICES (a). D2(a) one Rust layout-spec renderer, D3(a) discard queued input across restart, D5(a) drawer status bar shows shell name, D6(a) About-panel credits via FFI accessor, D8 close five rows as accepted divergences — all accepted.

## Verification

- Local (macOS): `./gates.sh` from outside the dev shell, all gates 0; falsifier (scratch copy) shows a failing gate named, its log tail printed, exit 1.
- Remote: ship the branch to a scratch dir on nixos.local (git bundle over ssh; never the user's checkout), `./gates.sh` there all 0; same falsifier.

---

# Previous task (complete)

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
