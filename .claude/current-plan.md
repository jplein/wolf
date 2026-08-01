# Current plan — Linux verification gate

## Status

**PHASE 2: product-bug fix.** Pass 1 of the coder is COMPLETE and landed almost everything (see "Pass 1 outcome" below). Four of the five gate steps are green. One cage smoke fails on a REAL product bug, which triggered the §6.3 carve-out. Architect is now diagnosing that bug; a test-writer pass follows if the fix admits a headless guard.

Test-writer step was SKIPPED for pass 1 by explicit orchestrator decision (see §7 of the plan below — that pass was infrastructure + docs; the nine tests already existed).

---

## Pass 1 outcome (coder, complete)

### Gate status

```
STEP1 cargo fmt --all -- --check                             PASS
STEP2 prettier --check "**/*.md"                             PASS
STEP3 cargo clippy --workspace --all-targets -- -D warnings  PASS
STEP4 cargo build --workspace --all-targets                  PASS
STEP5 cargo test --workspace                                 FAIL (one smoke — see below)
```

`prettier` now resolves to `/nix/store/hvhfy3az7yy18jx14p19vpvqmc04mrlq-prettier-3.8.3/bin/prettier` (was `/run/current-system/sw/bin/prettier`). `pkgs.prettier` resolved — no fallback needed.

Smoke suite: `test result: FAILED. 8 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out`. The **`0 ignored` is the proof the un-ignoring worked**; it passed 9/9 once, then became a repeatable 8/9.

### Landed in pass 1

- `flake.nix`: `pkgs.prettier` added; three stale comments rewritten. cage/wtype/grim/tesseract confirmed already present, not duplicated.
- `.prettierignore` created.
- `smoke_headless.rs`: nine `#[ignore]` lines deleted, module header rewritten, stale "FAILS today" TRI note corrected to past tense (n-ary redesign HAS landed; the three-pane smoke passes), both "relax the assertion" escape hatches deleted, `NOTE to coder` scaffolding converted to prose. **No assertion weakened.**
- Timing knobs: `WAIT_TRIES` 25→75 in all nine scenarios, `TREE_SETTLE` 1.0→1.5, `SETTLE` left at 0.5.
- Clippy green at `-D warnings` (35 findings fixed: 28 `doc_lazy_continuation` + 7 others; wolf-core 10, wolf-ffi 25, wolf-gnome 0). One `#[allow(..., reason = ...)]` in `frame_pack.rs` where clippy's suggestion cannot compile (E0451, crate-private field). No crate-level allows, no `[lints]` table, no `clippy.toml`.
- Docs: `build.md` (new gate section + four drift fixes), `crates/wolf-gnome/tests/README.md` (incl. `tree.rs` shim drift), `crates/wolf-gnome/README.md` (four→five split smokes), `tools/lint/README.md` (new `## Linux`), root `README.md`, `architecture.md`, `crates/wolf-gnome/src/README.md`, `macos-followups.md` (new, 5 seeded rows).

### Two PRE-EXISTING bugs the gate caught and fixed

1. **`wolf-gnome` did not compile at all.** `crates/wolf-gnome/src/pane_view.rs:162` — `E0004: non-exhaustive patterns: LayoutIntent::DrawerOpen { .. } not covered`. `wolf-core` gained `DrawerOpen` in the merged `feat/terminal-drawer` work (`e949f71`); `wolf-gnome` has zero drawer support and was never updated. Fixed with an explicit documented no-op arm (deliberately NOT a `_` wildcard).
2. **Broken doctest in `crates/wolf-core/src/rank.rs`.** A 4-space-indented list under `### Sort key…` was parsed by rustdoc as Rust. Plan §10's claim of "zero doctests in any lib target" was FALSE. Never caught because `cargo test --workspace` fail-fast never reached the doctest phase, and Bazel `rust_test` targets do not run doctests.

### Plan corrections found by the coder

- §5.2 item 8 is wrong: `window_renders_typed_text_under_cage` does NOT launch with the repo root as cwd. Cargo runs integration binaries with cwd = the **package** root, `crates/wolf-gnome`.
- §10's "zero doctests" claim is wrong (see above).

---

## OPEN ITEM A — the product bug (blocks the gate)

**`window_renders_typed_text_under_cage` fails. Exactly one keystroke — the first one delivered after the app is ready — is swallowed.**

User-visible impact: open Wolf, start typing immediately, your first character vanishes.

Evidence from a standalone probe driving the real binary under cage:

| cwd | wait before typing | traversal state | `hello` rendered? |
| --- | --- | --- | --- |
| repo root | 0.5 s | still streaming | **yes** |
| repo root | 8 s | complete | **no** |
| `crates/wolf-gnome` (the test's real cwd) | 0.5 s | complete (tiny dir) | **no** |
| `crates/wolf-gnome` | 0.5 s + one throwaway `Escape` first | complete | **yes** |

**It is not timing — more settle makes it deterministically WORSE.** The correlation is with the file-search traversal COMPLETING, not with elapsed time.

From the retained PNG (`$XDG_RUNTIME_DIR/wolf_gnome_smoke.png`): window, sidebar, editor pane, tildes and cursor all render correctly. Row 0 is empty and the cursor sits **two** rows down — the leading `i` was dropped and `hello` executed in NORMAL mode (its trailing `o` opened a line), then the scripted `o` opened another. So keys DO reach nvim; precisely one is lost.

**Disproved hypothesis (already reverted, do not repeat):** deferring `focus_area.grab_focus()` from immediately-after-`present()` into `window.connect_map()` does NOT fix it.

## OPEN ITEM B — pre-existing flaky test

`crates/wolf-core/tests/file_search.rs::mru_seed_follows_nvim_cd` failed once under full-workspace parallelism (`the B-relative entry must carry an mru_rank (Some)`), passed in another workspace run and 3/3 in isolation. The 11 tests in that binary share one sqlite MRU at the process-global `WOLF_STATE_DIR`; under contention `MruStore` degrades **silently** to an empty seed (`start_file_search` documents "a disabled/unreadable store yields an empty seed"). A test that races its siblings through a process-global is genuinely wrong and must be fixed — a flaky test in a gate is a broken gate.

## Branch

`linux-test-fixes` (branched off `linux`).

> NOTE: the requested branch name `linux/test-fixes` is **impossible** in git — `refs/heads/linux` already exists as a file, so `refs/heads/linux/test-fixes` cannot be created. Using `linux-test-fixes`. Report this to the user.

## Standing constraints (from the request — these override the plan where they conflict)

- `wolf-core` signature changes ALLOWED, but any change to a `wolf-core` public API consumed by `wolf-ffi` / `wolf-macos` MUST be (a) reflected in the relevant README / `architecture.md`, and (b) appended to `macos-followups.md`.
- New `crates/wolf-core/tests/*.rs` must be wired into `crates/wolf-core/BUILD.bazel` with a matching `rust_test`, mirroring a sibling exactly. Bazel cannot be run here.
- `crates/wolf-gnome` has no `BUILD.bazel`; gnome-only tests need no Bazel wiring.
- Markdown: no hard line breaks inside a paragraph; `prettier --write` every `.md` edited.
- Do NOT modify `feat-list.md`. Do NOT touch `feat-list-workflow.md`.
- Do NOT weaken a test to make it pass. If a test is genuinely wrong, fix the test and say so explicitly.

---

# Implementation Plan — Linux Verification Gate (`linux-test-fixes`)

## 1. Summary

This change closes the "no documented way to lint on Linux" gap and makes the GTK front-end's only real end-to-end proof actually run. It (a) adds a single, copy-pasteable **Linux verification gate** to `/home/jplein/Git/jplein/wolf/build.md` — rustfmt check, prettier check, clippy at `-D warnings`, workspace build, workspace test — all through `nix develop --command` so a stale direnv env can never be the thing under test; (b) pins `prettier` into `flake.nix`'s `linuxShell` (it currently resolves to the host binary at `/run/current-system/sw/bin/prettier`, so the "gate" would otherwise depend on an unpinned system tool); (c) removes the nine `#[ignore]` attributes from `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/tests/smoke_headless.rs` so the cage smokes run in the standard suite; (d) makes the whole gate pass without weakening any assertion; (e) fixes the documentation that is falsified by (c) plus the pre-existing `tree.rs` drift; and (f) creates `/home/jplein/Git/jplein/wolf/macos-followups.md` as the ledger for the later macOS merge pass. It is overwhelmingly infrastructure + documentation; the only code that will change beyond attribute/doc-comment edits is whatever clippy and the newly-running smokes surface.

---

## 2. The exact Linux gate command set (Deliverable 1)

### 2.1 The canonical form (this is what goes in `build.md`)

Five commands, run from the repo root. Every one is prefixed with `nix develop --command` — **not** optional. After a `flake.nix` edit the already-loaded direnv environment is stale (it will not have `prettier` on `PATH`), and `nix develop` re-evaluates the flake. This also matches the existing repo idiom (`crates/wolf-gnome/README.md` lines 23-27 and 116-121 already document exactly this reasoning).

```sh
nix develop --command cargo fmt --all -- --check
nix develop --command prettier --check "**/*.md"
nix develop --command cargo clippy --workspace --all-targets -- -D warnings
nix develop --command cargo build --workspace --all-targets
nix develop --command cargo test --workspace
```

### 2.2 The one-shot form (also goes in `build.md`, immediately after)

One flake evaluation, fail-fast, no new file to maintain:

```sh
nix develop --command bash -c '
  set -euo pipefail
  cargo fmt --all -- --check
  prettier --check "**/*.md"
  cargo clippy --workspace --all-targets -- -D warnings
  cargo build --workspace --all-targets
  cargo test --workspace
'
```

### 2.3 Per-step justification (the coder must not "simplify" these away)

| Step | Flag rationale |
|---|---|
| `cargo fmt --all -- --check` | `--all` = every workspace member. `-- --check` is rustfmt's non-writing mode (exit 1 + diff). Costs seconds, compiles nothing. |
| `prettier --check "**/*.md"` | Quote the glob so the shell does not expand it (zsh `**` expansion would behave differently and would not respect `.prettierignore`); prettier expands it internally and applies `.prettierignore`. See §2.5. |
| `cargo clippy --workspace --all-targets -- -D warnings` | **`--all-targets` is load-bearing.** Without it clippy lints only `--lib` and `--bins`; `tests/*.rs` and every `#[cfg(test)] mod tests` go unlinted. Since the point of this change is that `smoke_headless.rs` is now a first-class gating test, it must be clippy-clean — and `layout.rs` / `cursor.rs` / `picker.rs` / `geometry.rs` carry large in-module test modules that have likely never been linted. `-D warnings` mirrors the macOS `--config=lint` semantics (`.bazelrc` line 28 sets `clippy_flags=-Dwarnings`) so the two hosts enforce the same bar. |
| `cargo build --workspace --all-targets` | Plain `cargo build --workspace` does **not** build `tests/` or `examples/`. `--all-targets` does, so a compile error in `smoke_headless.rs` (or in `crates/wolf-core/examples/grid_dump.rs`) surfaces here as a clean build failure rather than mid-test-run. |
| `cargo test --workspace` | Deliberately **no** `--all-targets`: `cargo test --all-targets` silently drops doctests. Plain `cargo test --workspace` runs unit + integration + doctests. |

### 2.4 Ordering rationale (asked explicitly)

Cheap checks first, then clippy, then build, then test:

1. **fmt + prettier first.** Sub-second to seconds, zero compilation. Failing on a stray blank line after a 12-minute compile is a bad gate.
2. **clippy before build.** Clippy compiles the *entire* dependency graph. Because `clippy-driver` is installed as `RUSTC_WORKSPACE_WRAPPER`, only the **workspace member** units get a distinct fingerprint; the (much larger) third-party dependency graph is shared with the subsequent `cargo build`. So clippy-then-build costs one dep-graph build + two workspace-member builds; build-then-clippy costs exactly the same, but reports the high-volume clippy findings later. Clippy first also means the coder gets the biggest work item's size early.
3. **build before test.** `--all-targets` links every test binary, so the compile/run failure boundary is crisp and `cargo test --workspace` afterwards is close to pure execution.
4. **test last.** It is by far the longest step and the only one with external side effects (spawns real `nvim` processes and nine `cage` compositors, writes `.wolf/` into the repo cwd — gitignored — plus `$XDG_RUNTIME_DIR/wolf_gnome_*.png` and `/tmp/wolf-gnome-picker-smoke-*` dirs, which are deliberately not cleaned up for post-mortem).

### 2.5 The prettier glob, `.prettierignore`, and the `feat-list.md` constraint

There is **no `.prettierignore` today** (verified: the only prettier config at the repo root is `.prettierrc`). Prettier's built-in ignores are only `**/.git`, `**/.svn`, `**/.hg`, `**/node_modules`. `target/` and `.direnv/` are **not** ignored, and `.direnv/` contains symlinks into `/nix/store` that prettier's globber (fast-glob, `followSymbolicLinks: true` by default) will happily descend into. **A `.prettierignore` is therefore mandatory**, not a nicety — without it `prettier --check "**/*.md"` may try to check read-only Nix store documentation.

Create `/home/jplein/Git/jplein/wolf/.prettierignore`:

```
# Build + environment output — never source.
target/
.direnv/
.worktrees/
bazel-*/
node_modules/

# Orchestration scratch, not project documentation. `.claude/current-plan.md`
# is rewritten every task; formatting it is churn, and a dirty scratch file
# must never be able to fail the Linux verification gate.
.claude/
```

**The `feat-list.md` tension.** The plan's standing constraints forbid modifying `feat-list.md` and forbid touching `feat-list-workflow.md`. Both are in scope of `**/*.md`. Handling:

- The coder runs `nix develop --command prettier --check "**/*.md"` **as a discovery step before changing anything** and reports the list of dirty files.
- If `feat-list.md` and `feat-list-workflow.md` are already clean (likely — commits `dcf9c58` "Changing prettier printWidth, applying to Markdown files" and `3e7003a` "Table reformatting in the prettier config, applied to feat-list.md" both ran prettier over them), nothing more is needed and they stay in scope.
- If either is dirty, **do not reformat it**. Append to `.prettierignore`:
  ```
  # Owned by the feat-list workflow (feat-list-workflow.md); this branch is
  # forbidden from modifying them, so they are out of the format gate's scope.
  feat-list.md
  feat-list-workflow.md
  ```
  and report the exclusion to the user.

Fix command to document alongside: `nix develop --command prettier --write "**/*.md"`.

### 2.6 Do we still need a separate wolf-gnome line? — **No.** (asked explicitly)

`/home/jplein/Git/jplein/wolf/Cargo.toml` line 3: `members = ["crates/wolf-core", "crates/wolf-ffi", "crates/wolf-gnome"]`. `wolf-gnome` **is** a workspace member, so `cargo test --workspace` runs its four test binaries (`gdk_key`, `paint_golden`, `roundtrip_nvim`, `smoke_headless`) including — once the attributes are removed — all nine cage smokes.

Decision: **one `cargo test --workspace` line in the gate.** A duplicate `cargo test -p wolf-gnome` line would re-run the slowest, most side-effectful part of the suite for zero added coverage, and would re-launch nine cage compositors. CLAUDE.md's concern is "does the command that runs the suite actually run the test?" — `--workspace` does.

Document `nix develop --command cargo test -p wolf-gnome` separately as a **narrow iteration loop**, explicitly labelled as a convenience and *not* an extra gate step.

### 2.7 Required proof artifact (CLAUDE.md "confirm it actually runs")

The coder must run this once and paste the output into their report:

```sh
nix develop --command cargo test -p wolf-gnome --test smoke_headless
```

Acceptance: the summary line must read `test result: ok. 9 passed; 0 failed; 0 ignored; ...`. The `0 ignored` is the evidence; `--list` is **not** sufficient because libtest lists ignored tests too.

### 2.8 Timeouts — the single most important operational note

Rough budget on this host:

- Cold `cargo build --workspace --all-targets` (full GTK4 + libadwaita + tokio graph, ~90 integration-test binaries): **3-8 min**.
- `cargo clippy --workspace --all-targets`: similar order, **3-8 min**.
- `cargo test --workspace`: ~50 `wolf-core` + ~40 `wolf-ffi` integration binaries, most spawning a real `nvim`; plus the nine serialized smokes (see §5.3, ~60-100 s). **5-15 min**.

**Total cold gate: 10-25 minutes.** The coder MUST set the Bash tool timeout to at least **1800000 ms (30 min)** — ideally 45 min — on the clippy, build, and test commands. The default 2-minute Bash timeout will kill them and produce a misleading "failure".

### 2.9 Wrapper script? — **No.** (asked explicitly)

Recommendation: **do not add a Linux lint script.** Reasons:

- The repo has **no** `Makefile`, `justfile`, or `scripts/` directory (verified). Documented `nix develop --command …` lines are the established Linux idiom throughout `build.md` and `crates/wolf-gnome/README.md`.
- The one-shot `nix develop --command bash -c '…'` form in §2.2 already gives single-invocation ergonomics with one flake eval and `set -e` fail-fast, at zero maintenance cost.
- A script under `tools/lint/` would sit next to Bazel-only content whose README opens "for the macOS Bazel host", inviting exactly the confusion this change is trying to remove; and it would need its own README entry per CLAUDE.md, plus it is itself an untested artifact.
- The macOS `bazel test //tools/lint:check` aggregate exists because Bazel *requires* a target to be a thing you can name. Cargo does not.

Document the plain commands in `build.md` and cross-reference from `tools/lint/README.md`.

---

## 3. Files to create

### 3.1 `/home/jplein/Git/jplein/wolf/.prettierignore` (new)

Contents as specified in §2.5. No README entry needed for a dotfile, but `build.md`'s gate section must state that the Markdown check is scoped by `.prettierignore` and list what it excludes and why.

### 3.2 `/home/jplein/Git/jplein/wolf/macos-followups.md` (new) — Deliverable 6

**Exact structure to write:**

- **H1:** `# macOS follow-ups`
- **Intro paragraph (one paragraph, no hard wraps):** state that Wolf's macOS side (`wolf-ffi`'s Swift bindings and the `wolf-macos` AppKit app) builds and tests **only** under Bazel/bazelisk on a macOS Tahoe (26) host with Xcode 26+ (see `build.md`, "Building on macOS (Bazel)"), and that this development machine is Linux/NixOS with no Bazel and no Xcode — so nothing on a Linux branch can be verified against macOS here. This file is the ledger the later macOS merge pass works through.
- **Second paragraph — the standing rule (quote it from the plan's constraints so it survives):** any change to a `wolf-core` public API consumed by `wolf-ffi` / `wolf-macos` must be (a) reflected in the relevant README / `architecture.md`, and (b) appended here. Likewise any new `crates/wolf-core/tests/*.rs` needs a matching `rust_test` in `crates/wolf-core/BUILD.bazel` (which cannot be validated here), and any `BUILD.bazel` / `MODULE.bazel` edit.
- **H2:** `## Ledger`
- **Table with exactly these five columns:**

  `| Date | Branch | What changed | Why macOS needs re-verification | Status |`

  `Status` is `Open` / `Verified <date>` / `N/A`. Add the fifth column deliberately: a ledger with no closure column can never be cleaned up, and the macOS pass needs to tick rows off.
- **H2:** `## How to close a row` — one short paragraph: on the macOS host run `bazel test //...` (plus `bazel build //crates/wolf-macos:Wolf` when the Swift/`.app` graph is implicated), then set `Status` to `Verified <date>` or delete the row if it turned out to be a no-op.

**Predicted seed rows for this branch** (date `2026-08-01`, branch `linux-test-fixes`):

1. **Nine cage smokes un-ignored** (`crates/wolf-gnome/tests/smoke_headless.rs`). Why: **no macOS impact.** `crates/wolf-gnome` deliberately has **no `BUILD.bazel`** (`build.md` line 409: "`wolf-gnome` is deliberately given no Bazel target, so GTK never enters `wolf-core`'s Bazel dependency graph"), so `bazel test //...` never sees this file. Logged for completeness. Status: `N/A`.
2. **`flake.nix` `linuxShell` gains `pkgs.prettier`; two stale comments updated.** Why: `devShells.default` on Darwin resolves to `darwinShell`, which is a separate `mkShell` and references none of the Linux packages (see the laziness note at `flake.nix` lines 44-47) — so no Darwin eval-path change is expected. Still worth a one-line spot check that `nix develop` still evaluates on the Darwin host. Status: `Open` (spot-check).
3. **`tools/lint/README.md` line 7 asserts "The pre-existing default suite stays exactly 47/47."** Why: that number is a frozen historical claim that cannot be verified from Linux, and the repo has grown many test targets since. Re-run `bazel test //...` on the macOS host and refresh or delete the number. Status: `Open`. (This is a genuine finding surfaced by this preflight, not busywork.)
4. **Conditional — clippy fixes that change a `wolf-core` public signature.** **Prediction: none.** At `-D warnings` on clippy's *default* lint set (correctness/style/complexity/perf/suspicious — pedantic is off), findings are overwhelmingly local: `needless_borrow`, `redundant_clone`, `uninlined_format_args`, `useless_conversion`, `&Vec<T>` params on private fns. If a public signature *does* change, add one row per API, and also update `crates/wolf-core/src/README.md` + `architecture.md`.
5. **Conditional — new `crates/wolf-core/tests/*.rs`.** **Prediction: none.** This task adds no wolf-core test files (see §7).

**"No entries yet" wording** (use only if, after implementation, the table would genuinely be empty — put it *under* the table heading, not as an empty table):

> **Nothing outstanding.** As of `<date>` every change on this branch is Linux-only — documentation, the Nix devShell, and `crates/wolf-gnome`, which has no Bazel target. No `wolf-core` or `wolf-ffi` public API changed, no `BUILD.bazel` or `MODULE.bazel` changed, and no `crates/wolf-core/tests/*.rs` was added, so there is nothing here for the macOS merge pass to re-verify beyond a routine `bazel test //...`.

Given predictions 1-3, the file will ship with rows; the wording above is the fallback.

**Linking:** add one bullet to the root `README.md` (see §4.6) so the file is reachable from the documented source of truth. Do not link it from `feat-list.md`.

---

## 4. Files to modify

### 4.1 `/home/jplein/Git/jplein/wolf/flake.nix` — Deliverable 2

**Confirmed by inspection (do NOT add duplicates):** `pkgs.cage` (line 131), `pkgs.wtype` (132), `pkgs.grim` (133), `pkgs.tesseract` (134) are **already present** in `linuxShell.packages`. `rustToolchain` (lines 48-55) already carries the `clippy` and `rustfmt` extensions, so `cargo clippy` / `cargo fmt` already resolve. The **only** missing tool is `prettier`.

**Change A — add the package.** Insert a new group in `linuxShell.packages` (after the smoke-toolchain block ending line 134, before the closing `];` on line 135):

```
            # LINT/FORMAT GATE: prettier backs the Markdown format check in the
            # Linux verification gate (see build.md, "The Linux verification
            # gate"). Pinned here so the gate can never silently run a host
            # prettier of a different version against `.prettierrc`.
            pkgs.prettier
```

**Which attribute:** use **`pkgs.prettier`** (the top-level `pkgs/by-name/pr/prettier/package.nix`), *not* `pkgs.nodePackages.prettier`. Prettier was promoted out of `nodePackages` into a top-level by-name package; the flake's pinned nixpkgs (`flake.lock`, `lastModified: 1781074563` ≈ June 2026, rev `9ae611a455b90cf061d8f332b977e387bda8e1ca`, `nixos-unstable`) is well after that migration. `nodePackages.prettier` may still resolve but is legacy and has known profile-collision issues.

**The coder MUST verify it resolves before relying on it**, e.g.:

```sh
nix eval --raw nixpkgs#prettier.version   # or: nix develop --command prettier --version
```

If `pkgs.prettier` does not resolve against this pinned nixpkgs, fall back to `pkgs.nodePackages.prettier` and **say so in the report** — do not silently swap.

**Change B — the stale package-list comment, lines 125-130.** It currently reads "SMOKE-ONLY: the opt-in headless end-to-end smoke test (tests/smoke_headless.rs, run with `--ignored`) launches the real window … Not needed to build or to run the app interactively." After this change that is false twice over: the smokes are not opt-in, and the tools *are* needed for the standard `cargo test`. Rewrite to say these four are **gate-required**: the nine cage smokes in `crates/wolf-gnome/tests/smoke_headless.rs` run as part of a plain `cargo test` (no `--ignored`), so `cage` (headless wlroots compositor), `wtype` (synthetic keystrokes), `grim` (screenshot), `tesseract` (OCR) must be present or the suite fails loud. Keep the "not needed to run the app interactively" clause only if reworded to "not needed to *build*".

**Change C — the stale shellHook comment, lines 169-174.** "Used only by the opt-in `--ignored` smoke test (cage/wtype/grim)." → "Used by the cage smoke tests in `crates/wolf-gnome/tests/smoke_headless.rs`, which run as part of the default `cargo test`." Keep the rest of that block verbatim (the `XDG_RUNTIME_DIR` default, `WLR_BACKENDS=headless`, `WLR_LIBINPUT_NO_DEVICES=1`, and the "we do NOT export `WAYLAND_DISPLAY` — cage creates it" note are all still exactly right and load-bearing).

**Change D — the file-header comment, lines 5-13.** It enumerates what the Linux shell provides ("a pinned Rust toolchain …, a pinned hermetic Neovim, and the GTK/Cairo/Pango GUI toolkit"). Append: "…plus the headless-compositor smoke toolchain and the Markdown formatter that the Linux verification gate (`build.md`) requires."

**Explicit non-changes:** do not add `pkgs.git` to `linuxShell` (the Darwin shell has it; the gate does not need it — out of scope). Do not add `pkgs.nodejs` (the `prettier` package wraps its own node). Do not add a `package.json` or `node_modules` — prettier comes from Nix, not npm. `flake.lock` does not need regenerating.

### 4.2 `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/tests/smoke_headless.rs` — Deliverable 3

**Required edits, exact:**

**(a) Delete these nine lines** — each is byte-identical:

`#[ignore = "headless-compositor smoke test; run explicitly with --ignored (needs flake smoke toolchain)"]`

at lines **1194, 1270, 1351, 1449, 1532, 1629, 1728, 1833, 1927** (immediately below each `#[test]`, above `fn window_renders_typed_text_under_cage`, `picker_opens_and_loads_selected_file_under_cage`, `picker_arrow_navigation_selects_second_match_under_cage`, `tree_sidebar_shows_files_under_cage`, `vertical_split_shows_two_panes_under_cage`, `horizontal_split_shows_two_panes_under_cage`, `focus_move_returns_to_first_pane_under_cage`, `close_pane_leaves_one_under_cage`, `three_panes_keep_all_files_visible_under_cage`). Delete the attribute line only; leave `#[test]` and the function untouched.

**(b) Module header, line 1.** `//! Opt-in, headless-compositor end-to-end smoke test for the real wolf-gnome window.` → drop "Opt-in,". Also make it plural ("smoke tests"), since there are nine.

**(c) Module header, lines 13-21 — the section `## Why it is `#[ignore]` (opt-in)`.** This whole section is now false, including its ```` ```text ```` block containing `nix develop --command cargo test -p wolf-gnome -- --ignored`. **Replace** with a section titled `## When it runs` saying, in substance:

- These nine tests run as part of the **standard** suite — `cargo test -p wolf-gnome` and `cargo test --workspace` — inside the flake devShell. They are the Linux verification gate's only end-to-end proof that the GTK front-end draws a window a human could see (see `build.md`, "The Linux verification gate").
- They are **not** `#[ignore]`d. "They are slow" is not a reason to skip the only real end-to-end test; per `CLAUDE.md` a test that the suite command does not run is a silently-skipped test.
- They are the slowest part of the suite: each drives a real compositor, and all nine serialize behind `CAGE_LOCK` (see below).
- Show the command as:
  ```text
  nix develop --command cargo test -p wolf-gnome --test smoke_headless
  ```
  **Keep the ```` ```text ```` annotation.** A bare ```` ``` ```` fence in a doc comment is parsed as Rust. (This is a `tests/` target so `cargo test --doc` will not collect it, but keep it correct anyway.)

**(d) Module header, lines 23-35 — "Requirements".** The requirement list (`cage`/`wtype`/`grim`/`tesseract` on `PATH`, `WOLF_NVIM_BIN`, writable `XDG_RUNTIME_DIR`) is still exactly right — **keep it**. The fail-loud paragraph is also still right, and is now *more* right: **keep `require_smoke_env`'s hard fail.** Now that these run by default, a missing tool means the gate was invoked outside the devShell, which is precisely the setup bug that must be surfaced, not hidden behind a green suite. Rewrite the trailing "(NOTE to coder: … do not soften this to a skip without revisiting the plan.)" into shipped-behavior prose: state that softening this to a skip would make the gate silently pass on an unprovisioned machine, and is forbidden.

**(e) Line 46** — `/// Serializes the nine `#[ignore]` cage smoke tests.` → `/// Serializes the nine cage smoke tests.` (rest of the doc comment is still correct and valuable).

**(f) Lines 442-445 — `require_smoke_env`'s panic message.** It currently ends: ``Run via `nix develop --command cargo test -p wolf-gnome -- --ignored`.`` That command is now wrong and would send a reader down a dead end. Change to ``Run via `nix develop --command cargo test -p wolf-gnome`.``

**(g) Lines 376-378 — the stale TRI note. This is a factual falsehood and must be fixed.**

> `// NOTE to coder: this FAILS today (B collapses to ~0 width / gray) and PASSES after`
> `// the n-ary frame redesign + divider (install_paned_position) fix.`

**Finding: the n-ary frame redesign HAS landed.** See §5.1. Rewrite to past tense, e.g.: "This is the regression guard for the n-ary frame model (landed). Before it, the always-nest binary model collapsed the pre-existing right pane B to ~0 width and blanked file B; `PaneTree::split`'s same-orientation flatten plus `pane_view::install_paned_position` fixed it. It must PASS." Keep the following sentence about the chords mirroring the vsplit/focus smokes.

**(h) Lines 336-338 and 1906-1909 — remove the "relax the assertion" escape hatches.** Both say, of `close_pane_leaves_one_under_cage`: "if nvim leaves the closed buffer listed elsewhere, or OCR noise makes `closegone` linger in the capture, relax to ONLY asserting `closekeep` is present." That is a standing licence to drop a gating assertion, which directly contradicts Deliverable 4 ("Do NOT weaken a test to make it pass"). Delete both. If the NOT-contains assertion genuinely fails, that is a finding to report, not a knob to turn.

**Optional but recommended (flag as a separate reviewable commit):** the remaining `NOTE to coder:` blocks (lines ~101-105, 145-156, 185-203, 238-249, 286-293, 380-385, 596-603, 686-690, 698-700, 1806-1809) are architect-to-coder construction scaffolding embedded in a now-shipping gating test. They are not *false* — the width/height notes at 193-198, 244-249 and 380-385 offer window-size / `WOLF_FONT_PT` adjustments, which are legitimate **robustness** knobs and not assertion weakening, so keep their substance. Convert the framing from imperative "NOTE to coder: verify this against your wtype build…" to descriptive "Note: …" rationale.

**Do NOT touch:** the `CAGE_LOCK` mutex and its poisoning recovery; `require_smoke_env`'s checks; any `assert!`; any OCR constant; the scenario shell strings (except see §5.4 for the one permitted timing knob).

### 4.3 `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/tests/paint_golden.rs` — no code change

Keep `#[ignore]` at line 297 on `regenerate_png_golden`. It is a **generator**, not a test: it rewrites `tests/testdata/paint_golden.png` in the source tree. Ignoring it is correct. After this change it is the **only** `#[ignore]` in the whole repo (grep confirmed: the only other matches are prose in `.md` files and a comment in `crates/wolf-gnome/Cargo.toml` line 37). Call this out in the docs (§4.4) because `cargo test -p wolf-gnome -- --ignored` now does nothing *but* rewrite a golden — a real footgun for anyone with the old muscle memory.

**Answer to "are there OTHER `#[ignore]`d tests anywhere in the repo?"** — No. `crates/wolf-core` (50 test files) and `crates/wolf-ffi` (40 test files) contain zero `#[ignore]` attributes. After this change the repo has exactly one, and it is a generator.

### 4.4 `/home/jplein/Git/jplein/wolf/build.md` — Deliverables 1 + 5

**(a) NEW section: `## The Linux verification gate`** — place it inside the Linux/NixOS part, i.e. **after `### Testing` (ends line 48) and before `## Building on macOS (Bazel)` (line 50)**. It is the counterpart to `## Linting and formatting (macOS)` (line 368). Contents:

- One-paragraph framing: this is the complete definition of "Linux is green" — build, tests (including the nine cage smokes), clippy, rustfmt, prettier. Unlike the macOS `//tools/lint` targets, which are advisory and `manual`-tagged, **this gate is gating**: `-D warnings` is on, and every step must exit 0.
- The five canonical commands (§2.1) in one `sh` block.
- The one-shot `bash -c` block (§2.2).
- A small table mirroring the macOS section's shape:

  | Check | Command | Fix |
  |---|---|---|
  | rustfmt | `nix develop --command cargo fmt --all -- --check` | `nix develop --command cargo fmt --all` |
  | Markdown format | `nix develop --command prettier --check "**/*.md"` | `nix develop --command prettier --write "**/*.md"` |
  | clippy | `nix develop --command cargo clippy --workspace --all-targets -- -D warnings` | fix the finding; `#[allow(…, reason = "…")]` only when the lint is wrong |
  | build | `nix develop --command cargo build --workspace --all-targets` | — |
  | tests | `nix develop --command cargo test --workspace` | — |

- **Notes subsection**, each a bullet:
  - **Everything runs through `nix develop --command`.** After a `flake.nix` edit the loaded direnv env is stale; `nix develop` re-evaluates the flake. The toolchain (`cargo`/`rustc`/`clippy`/`rustfmt`), `nvim` (`WOLF_NVIM_BIN`), `rg` (`WOLF_RG_BIN`), the GTK stack, the pinned DejaVu `FONTCONFIG_FILE`, the cage/wtype/grim/tesseract smoke toolchain, and `prettier` all come from the devShell. Never a system binary.
  - **`--all-targets` on clippy is load-bearing.** Without it `tests/*.rs` and every `#[cfg(test)] mod tests` go unlinted.
  - **`--all-targets` on build** compiles the integration-test binaries and `crates/wolf-core/examples/grid_dump.rs`, so a test-file compile error surfaces as a build failure.
  - **`cargo test --workspace` includes the nine cage smokes.** They are no longer `#[ignore]`d, they need a compositor + OCR toolchain, and they serialize behind a shared mutex, so they are the slowest part. Expect 10-25 minutes for a cold full gate.
  - **Prettier scope** is `**/*.md` minus `.prettierignore` (`target/`, `.direnv/`, `.worktrees/`, `bazel-*/`, `node_modules/`, `.claude/` — plus `feat-list*.md` if excluded per §2.5). `.direnv/` in particular *must* stay excluded: it symlinks into `/nix/store`.
  - **`--ignored` now means only one thing.** `cargo test -p wolf-gnome -- --ignored` no longer runs the smokes; the only remaining `#[ignore]` in the repo is `regenerate_png_golden` in `crates/wolf-gnome/tests/paint_golden.rs`, which **rewrites** `tests/testdata/paint_golden.png`.
  - **Narrow loops** (convenience, not gate steps): `cargo test -p wolf-gnome`, `cargo test -p wolf-gnome --test smoke_headless`, `cargo clippy -p wolf-gnome --all-targets -- -D warnings`.
  - **macOS is not covered here.** Cross-link to `macos-followups.md` and to `## Linting and formatting (macOS)`.

**(b) Fix drift at line 39.** `cargo test         # unit + integration tests for wolf-core` — false: at the workspace root (virtual manifest) `cargo test` runs **all** members, i.e. `wolf-core`, `wolf-ffi` and `wolf-gnome`. Change the comment to "unit + integration tests for the whole workspace" and point at the gate section.

**(c) Fix drift at line 42.** "The integration tests spawn the **pinned** Neovim … They are fully **headless**: `nvim --embed` speaks msgpack-RPC over stdio and needs no TTY, so the suite is CI-safe." Under a `### Testing` heading that describes plain `cargo test`, this now reads as a claim about the whole workspace suite, and it is false: the `wolf-gnome` cage smokes need a Wayland compositor, `wtype`/`grim`/`tesseract`, and a writable `XDG_RUNTIME_DIR`. Add a qualifying sentence: the `wolf-core` / `wolf-ffi` integration tests are fully headless in the `nvim --embed` sense; the nine `wolf-gnome` cage smokes additionally need the devShell's compositor + OCR toolchain (they *are* headless in the display sense — `cage` is a headless compositor — but they are not toolchain-free), and they fail loud rather than skipping when it is absent.

**(d) Line 33** `cargo build        # builds the whole workspace` — true but incomplete for gate purposes. Add a one-line note that the gate uses `--workspace --all-targets` so test and example targets are compiled too.

**(e) Line 372 (macOS lint section).** Add a sentence cross-linking to the new Linux gate section, so a reader who lands on "Linting and formatting (macOS)" learns that Linux has its own, *gating*, path. Also note the asymmetry explicitly: macOS clippy is advisory + `manual`-tagged with `-Dwarnings` opt-in via `--config=lint`; the Linux gate is `-D warnings` unconditionally.

**(f)** Add a pointer to `macos-followups.md` at the top of `## Building on macOS (Bazel)` (around line 52): work done on Linux-only branches is logged there for the macOS merge pass.

### 4.5 `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/tests/README.md` — Deliverable 5

Drift list with line + what it says + what is true:

| Line | Says | Truth after this change |
|---|---|---|
| 3 | "…and nine **opt-in headless-compositor** cage smoke tests…" | Not opt-in. Drop "opt-in"; they run in the default suite. |
| 9 | "The default run does **not** include the smoke test (it is `#[ignore]`d)" | **False.** Replace with: the default run *does* include all nine cage smokes, and they are the slowest part of the suite. Link to `build.md` → "The Linux verification gate". |
| 29-37 | Section `## `tree.rs` unit tests (in `src/tree.rs`, headless)` describing `children_of` / `ancestors_to_reveal_cases` / `node_rel_path_both_variants` / `rel_path_for` / `empty_entries_yield_empty` as living in `crates/wolf-gnome/src/tree.rs` | **False today, pre-existing drift.** `crates/wolf-gnome/src/tree.rs` is a 10-line shim: its entire body is `pub use wolf_core::tree::*;` and it has no `#[cfg(test)] mod tests`. The projection **and its tests** live in `crates/wolf-core/src/tree.rs` (documented at `crates/wolf-core/src/README.md` line 416-418; `crates/wolf-gnome/src/README.md` line 178 is already correct). **Fix:** retitle to something like `## `tree.rs` projection (a re-export; tests live in `wolf-core`)`, replace the body with one paragraph explaining the shim and cross-linking to `crates/wolf-core/src/README.md`, and note that the wolf-core module also carries `TreeIndex` / `TreeIndexCache` (`TreeIndex::build`, `TreeIndex::children_of`, `TreeIndexCache::index_for`, `TreeIndexCache::build_count`) that this section never mentioned. Do not re-enumerate the test cases here — they are wolf-core's to document. |
| 67-72 | `regenerate_png_golden` `#[ignore]` dev aid + the `-- --ignored regenerate_png_golden` recipe | Still true — **keep**. Add one sentence: it is now the **only** `#[ignore]`d test in the repo, so a bare `cargo test -p wolf-gnome -- --ignored` runs *nothing but* the golden rewrite. |
| 80 | Heading "…(opt-in, real compositor)" | Drop "opt-in". Note: the heading text is also the anchor target of the line-9 link — update both together, or drop that link when rewriting line 9. |
| 82-86 | "Nine `#[test] #[ignore]`d cage smokes … They are opt-in because they need a compositor and an OCR toolchain a normal `cargo test` will not have. Run them explicitly: `nix develop --command cargo test -p wolf-gnome -- --ignored`" | **False.** Replace: nine `#[test]` cage smokes that run in the default suite; the compositor + OCR toolchain comes from the flake devShell, so a normal `cargo test` *inside `nix develop`* does have it, and `require_smoke_env` fails loud if it does not. |
| 88 | "So all nine tests acquire a shared `static CAGE_LOCK: Mutex<()>`…" | True — **keep verbatim**, and add that this is why the suite is slow. |
| 98 | Tri-pane bullet, "…which the old binary always-nest model collapsed/blanked…" | Already past tense and **correct**. Keep. |
| 102 | "Required toolchain + env (all from the flake devShell smoke toolchain)… `require_smoke_env` **fails loud**" | True — keep, and add that these are now **gate-required**, not optional. |

### 4.6 `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/README.md` — Deliverable 5

| Line | Says | Truth |
|---|---|---|
| 62 | "…covered by the opt-in picker cage smokes" | Not opt-in. Drop "opt-in". |
| 129 | "A suite of **opt-in, `#[ignore]`d** end-to-end tests… Run them explicitly:" | **False.** Rewrite: they run as part of `cargo test -p wolf-gnome` / the Linux verification gate. |
| 131-133 | ```` ```sh nix develop --command cargo test -p wolf-gnome -- --ignored ``` ```` | **False command.** Replace with `nix develop --command cargo test -p wolf-gnome` (or `--test smoke_headless` for just the smokes). |
| 135 | "…further smokes cover the file picker, the file-tree sidebar, and **the four split-pane operations** (vertical / horizontal split, focus move, close pane)" | **Undercount.** There are **five** split-pane smokes: vsplit, hsplit, **three-pane (n-ary flatten regression)**, focus move, close pane. `tests/README.md` line 3 has it right. Fix to five and name the three-pane one. |
| 116-121 | `## Building and testing` two-command block | Still valid. Add a sentence pointing at `build.md` → "The Linux verification gate" as the authoritative full gate, and note that `cargo test -p wolf-gnome` now includes the nine smokes. |
| 139 | "`cairo-rs` enables the `png` feature for the `#[ignore]`d golden-regeneration dev-aid test." | Still true. Keep. |

### 4.7 `/home/jplein/Git/jplein/wolf/tools/lint/README.md` — Deliverable 5

- Line 3 ("for the macOS Bazel host") is **true** — keep, but sharpen the first sentence to say **macOS-only**, so a Linux reader is not left thinking this is the repo's whole lint story.
- **Add a new short section `## Linux`** (place it right after `## Policy: advisory, not gating`): these Bazel targets do not and cannot run on Linux (no Bazel, no Xcode in the Linux devShell). On Linux the equivalents are `cargo clippy`, `cargo fmt` and `prettier` invoked directly inside the Nix devShell, and — unlike these advisory targets — they are **gating**. Link to `build.md` → "The Linux verification gate". Note the asymmetry: `-Dwarnings` is opt-in via `--config=lint` on macOS and unconditional on Linux; `swift-format` has no Linux counterpart (no Swift in the Linux shell); prettier has no macOS/Bazel counterpart today.
- Line 7: "The pre-existing default suite stays exactly 47/47." — **suspected stale**, unverifiable from Linux. Do **not** guess a new number. Either soften it (drop the specific count, keep the "no wildcard sweeps them in" claim, which is what actually matters) or leave it and log it. Either way it gets a `macos-followups.md` row (§3.2 row 3).

### 4.8 `/home/jplein/Git/jplein/wolf/README.md` — Deliverable 5/6

Swept: line 11 ("Linux: Only NixOS is supported…") and lines 15-22 are all accurate; **no drift found**. One additive change only — under `## Building Wolf`, add a bullet:

- `- See [macOS follow-ups](macos-followups.md) for work done on Linux that still needs re-verification on a macOS host.`

### 4.9 Files swept and found clean (record, do not edit)

- `/home/jplein/Git/jplein/wolf/architecture.md` — no mention of the smokes, `--ignored`, cage, or Linux linting. Clean.
- `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/src/README.md` — lines 90-118 and 136 accurately describe the landed n-ary frame model and `install_paned_position`. Clean.
- `/home/jplein/Git/jplein/wolf/crates/wolf-core/src/README.md` lines 416-418 — correctly states the tree projection + tests live in wolf-core. Clean.
- `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/Cargo.toml` line 37 — the comment "used by the `#[ignore]`d golden-regeneration dev-aid test" remains true. Clean.
- `/home/jplein/Git/jplein/wolf/feat-list.md`, `/home/jplein/Git/jplein/wolf/feat-list-workflow.md` — **out of bounds by constraint.** Note that `feat-list-workflow.md` line 7 says "Use the /orchestrate skill to verify that all of the Linux tests and linters pass" — the new `build.md` gate section is precisely the missing definition that workflow depends on. Do not edit the file; mention the connection in the report.
- `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/tests/common/` has no `README.md` (pre-existing, `mod.rs` is documented in `tests/README.md` line 106-108). Out of scope; mention only if the reviewer raises the CLAUDE.md per-directory-README rule.

---

## 5. Risk assessment for the nine smokes (Deliverable 4)

### 5.1 Has the n-ary frame redesign landed? — **YES. Finding: the "FAILS today" comment is stale.**

Evidence read directly from source:

- `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/src/layout.rs`: `enum Frame` (line 87) is `Leaf(PaneId)` **or** `Split { orientation, children: Vec<Frame>, weights: Vec<f64> }` (lines 90-99) — the n-ary form, not a binary pair. `split_in` (line 278) explicitly flattens: when the requested orientation matches the containing frame it does `children.insert(i + 1, Frame::Leaf(new_pane)); weights.insert(i + 1, 0.0); equalize(weights);` (lines 294-296), and only nests for a perpendicular split (lines 299-302). `normalize` (line ~335) flattens same-orientation child splits, collapses 1-child splits, and re-equalizes.
- `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/src/pane_view.rs`: `build_chain` (line 732) renders an n-ary split as a right-leaning chain of binary `gtk::Paned`s with `set_shrink_start_child(false)` / `set_shrink_end_child(false)` and a per-paned `ratio = weights[0] / total`; **`install_paned_position` exists** (line 774) and re-applies the proportional divider on **every** `max-position` notify rather than once — which is exactly the divider fix the stale comment says is missing.
- `/home/jplein/Git/jplein/wolf/crates/wolf-gnome/src/README.md` line 136 states it outright: "`install_paned_position` re-applies the proportional divider on EVERY `max-position` notify (not one-shot) … so a 3rd same-orientation split no longer starves the pre-existing pane to a blank/gray zero-width strip." Line 111 documents the flatten: "a row `[P1, P2]` vsplit on `P1` becomes equal thirds `[P1, P3, P2]`, NOT a nested 25/25/50 that blanks `P2`".

**Conclusion:** `three_panes_keep_all_files_visible_under_cage` is expected to **pass**. The lines 376-378 comment is documentation drift (fix per §4.2g), not a live bug. If it nevertheless fails, the failure is almost certainly **OCR legibility at third-width panes**, not layout — the coder must diagnose by inspecting the retained PNG at `$XDG_RUNTIME_DIR/wolf_gnome_trisplit_smoke.png` before touching any code.

### 5.2 Which smokes are most likely to fail, and why (ranked)

1. **`three_panes_keep_all_files_visible_under_cage` — highest OCR risk, low layout risk.** Under `cage`'s headless wlroots output the window is effectively fullscreen; minus `TREE_PANE_WIDTH = 250` (`crates/wolf-gnome/src/app.rs` line 79) the editor area is roughly 1000-1650 px, split three ways → ~330-550 px per pane. At `WOLF_FONT_PT = 28` a DejaVu Sans Mono advance is ≈ 22 px, so ~15-25 columns per pane — enough for `# TRIBTWO` (9 chars), but with no margin for tesseract error. **Diagnose from the PNG first.** Permitted fixes if it is a width problem: a wider window or a lower `WOLF_FONT_PT` for this one test (the file's own note at 380-385 sanctions this, and it is a robustness knob, not an assertion change). **Not permitted:** dropping the `tribtwo` assertion — that is the load-bearing one.
2. **`tree_sidebar_shows_files_under_cage` — underrated risk.** It asserts OCR of `treenode` in the **sidebar**, which is a `gtk::ListView` label rendered at the **GTK default font size**, *not* at `WOLF_FONT_PT = 28`. In a 250 px sidebar at ~11 pt that is small type for tesseract `--psm 11`. Also it has no keyboard interaction, so it depends entirely on the long-lived `FileSearch` streaming within `SETTLE + TREE_SETTLE = 1.5 s`. If it fails, the likely fixes are a longer `TREE_SETTLE` (robustness, permitted) — not a weaker assertion.
3. **`close_pane_leaves_one_under_cage` — the NOT-contains assertion.** `!ocr.contains("closegone")`. Analysis says it should hold: after `:e cgone.md` nvim's toast (painted in the cmdline strip) shows the *filename* `cgone.md`, whose lowercase does not contain `closegone` (the *heading*). So the assertion is well-designed. Residual risk is `<C-w>c` not firing, or `E444: Cannot close last window` if the preceding split failed. Its two "relax the assertion" comments are being deleted (§4.2h) precisely so this is diagnosed rather than papered over.
4. **`picker_arrow_navigation_selects_second_match_under_cage`.** Depends on (a) `Down` routing to the ListBox and not the SearchEntry — `crates/wolf-gnome/README.md` line 59 confirms this is the implemented behavior ("`Up`/`Down` move the list selection (focus stays in the textbox…)"), and (b) the rank tiebreak putting `pick1.md` at row 0. The MRU is global (`state_dir()/file-mru.sqlite`) but keyed by absolute path, and `make_temp_cwd` generates a fresh pid+nanos dir per run, so prior runs cannot contaminate the ordering. Medium risk.
5. **`focus_move_returns_to_first_pane_under_cage`.** Longest chord chain (two `<C-w>` chords, two `:e`, an insert, a cursor park) and seven `SETTLE`s — most exposed to timing under load. Three assertions, all `contains`.
6. **`vertical_split_…` / `horizontal_split_…`.** Two panes, moderate. hsplit halves height rather than width, which is the friendlier direction for OCR.
7. **`picker_opens_and_loads_selected_file_under_cage`.** Depends on `wtype -M ctrl -M shift -k p -m shift -m ctrl` delivering Ctrl+Shift+P through cage's `zwp_virtual_keyboard_v1`. If it works for the other chords it works here.
8. **`window_renders_typed_text_under_cage` — lowest risk.** Note it is the **only** smoke that does not set `.current_dir()`, so the app launches with the **repo root** as cwd; the sidebar's `FileSearch` will traverse the repo (the `ignore` crate honors `.gitignore`, so `target/` and `.direnv/` are pruned) and the run will create/refresh `.wolf/` in the repo root (gitignored). Harmless, but worth stating.

### 5.3 Wall-clock estimate (asked explicitly)

Counting the `sleep`s in each scenario string (all `SETTLE = 0.5`, plus the poll loop `WAIT_TRIES=25 × WAIT_INTERVAL=0.2` = 5 s ceiling):

| Test | scenario sleeps | typical total (incl. cage start, window map, `grim`, `tesseract`, teardown) |
|---|---|---|
| `window_renders_typed_text` | 3 × 0.5 = 1.5 s | ~5-8 s |
| `picker_opens_and_loads_selected_file` | 4 × 0.5 = 2.0 s | ~6-9 s |
| `picker_arrow_navigation_selects_second_match` | 5 × 0.5 = 2.5 s | ~6-10 s |
| `tree_sidebar_shows_files` | 0.5 + 1.0 = 1.5 s | ~5-8 s |
| `vertical_split_shows_two_panes` | 4 × 0.5 = 2.0 s | ~6-9 s |
| `horizontal_split_shows_two_panes` | 4 × 0.5 = 2.0 s | ~6-9 s |
| `focus_move_returns_to_first_pane` | 7 × 0.5 = 3.5 s | ~8-12 s |
| `close_pane_leaves_one` | 5 × 0.5 = 2.5 s | ~7-10 s |
| `three_panes_keep_all_files_visible` | 8 × 0.5 = 4.0 s | ~9-13 s |

Fully serialized behind `CAGE_LOCK`: **~60-90 s typical, up to ~2.5 min under load.** The other eight blocked test threads cost nothing. See §2.8 for the overall gate budget and the required 30-45 minute command timeout.

### 5.4 The one predicted systemic failure mode, and the permitted fix

`cargo test --workspace` runs ~90 integration-test binaries in parallel, most spawning a real `nvim`. Under that load, launching a GTK window inside `cage` may not map within the smoke's `WAIT_TRIES=25 × WAIT_INTERVAL=0.2` = **5-second budget**, and the scenario then `exit 11`s with "app process exited early" / a non-success cage status. This is my top predicted cause of a *mass* smoke failure, and it is an artifact of the tests never having run concurrently with the full workspace suite before.

**Permitted fix (not weakening):** raise `WAIT_TRIES` in the nine scenario strings from `25` to e.g. `75` (a 15-second budget) and/or `SETTLE` from `0.5` to `0.75`. These are timing-robustness knobs; **no assertion changes.** Additionally: kill any stale `cage` process holding a Wayland socket under `$XDG_RUNTIME_DIR` before a rerun.

**Not permitted:** re-adding `#[ignore]`, adding `#[cfg]` gates, converting `require_smoke_env`'s panics into skips, deleting or loosening any `assert!`, or introducing a `WOLF_SKIP_SMOKE`-style env escape hatch. If a smoke fails for a real product reason, **fix the product**, say so explicitly, and (per §7) consider a pure headless unit test pinning the fix.

**Other environment checks the coder should confirm once, up front:** `WLR_BACKENDS=headless` and `WLR_LIBINPUT_NO_DEVICES=1` are exported by the shellHook (lines 178-179) and inherited by `cage`; `XDG_RUNTIME_DIR` on NixOS is `/run/user/<uid>` mode 0700, which passes `require_smoke_env`'s `!meta.permissions().readonly()` check (Unix `readonly()` is `mode & 0o222 == 0`); `tesseract` from nixpkgs ships the `eng` traineddata `--psm 11` needs.

---

## 6. Test plan (Deliverable: what to test / which test files)

**No new test files. No modified assertions.** The "test plan" for this change is a verification protocol, not authored tests.

### 6.1 Behaviors that must be demonstrated (with the evidence for each)

| Behavior | Evidence |
|---|---|
| The nine cage smokes actually execute under the documented suite command | `nix develop --command cargo test -p wolf-gnome --test smoke_headless` prints `test result: ok. 9 passed; 0 failed; 0 ignored`. Paste it. |
| …and under the *gate* command, not just the narrow one | `nix develop --command cargo test --workspace` completes green; grep its output for the nine `..._under_cage` test names. |
| `prettier` comes from the flake, not the host | Inside `nix develop`, `command -v prettier` resolves under `/nix/store/…`, **not** `/run/current-system/sw/bin/prettier`. Paste it. This is the literal thing Deliverable 2 asks to be proven. |
| `cage`/`wtype`/`grim`/`tesseract` are not duplicated in `flake.nix` | Show the single occurrence of each at lines ~131-134. |
| Clippy is clean at the gate's bar | `cargo clippy --workspace --all-targets -- -D warnings` exits 0. |
| rustfmt + prettier are clean | Both `--check` forms exit 0. |
| `--all-targets` really does reach the test sources | Sanity check during the discovery pass: confirm clippy's output mentions `smoke_headless.rs` / test-module paths (or that a deliberate temporary lint in a test file is caught). Do not commit the deliberate lint. |
| Exactly one `#[ignore]` remains repo-wide, and it is a generator | `rg '#\[ignore' --type rust` returns only `crates/wolf-gnome/tests/paint_golden.rs:297`. |

### 6.2 Edge cases explicitly *not* to cover with new tests

- Do **not** write a test that shells out to `nix develop` / `cargo` / `prettier` from inside `cargo test`. It is recursive, non-hermetic, and would multiply the gate's runtime.
- Do **not** write a "no `#[ignore]` anywhere" meta-test. It would immediately conflict with `paint_golden.rs`'s legitimate generator, and lint-by-test is the wrong mechanism.
- Do **not** add a Bazel target for anything here. `crates/wolf-gnome` has no `BUILD.bazel` by design, and nothing in this change touches `crates/wolf-core/tests/` or `crates/wolf-ffi/tests/`.

### 6.3 Conditional test work

**Only if** making the gate pass reveals a genuine product bug in `wolf-gnome` or `wolf-core` (e.g. a pane-layout, focus-routing, or picker-ranking defect that makes a smoke legitimately fail), then a test-writer **is** needed for exactly one thing: a pure, headless unit test pinning the fixed behavior in the relevant `#[cfg(test)] mod tests` — most likely `crates/wolf-gnome/src/layout.rs` (which already has twenty such tests) or `crates/wolf-gnome/src/picker.rs`. Rationale: a 10-second OCR smoke is a terrible regression guard for pure logic. If such a test lands in `crates/wolf-core/src/*.rs` it needs no Bazel wiring (in-module unit tests ride the existing `rust_library`); if it lands as a new `crates/wolf-core/tests/*.rs` it **must** get a matching `rust_test` in `crates/wolf-core/BUILD.bazel` mirroring a sibling, plus a `macos-followups.md` row (Bazel cannot be run here to confirm it).

---

## 7. Should the orchestrator run a test-writer? — **No. Skip it.** (asked explicitly)

Being blunt, as requested: **this task is infrastructure and documentation. There is nothing meaningful for a test-writer to write.**

- The nine end-to-end tests already exist and are well-constructed. The work is deleting nine attribute lines, not authoring assertions.
- The gate is a documented command sequence. "Testing the gate" would mean invoking the gate from inside the gate — recursive and hermetically broken.
- The two plausible-sounding meta-tests (a "no `#[ignore]`" scan; a "prettier is on PATH" assertion) are lint-by-test, trivially circumventable, and would fight legitimate existing behavior.

**Skip the test-writer step**, with the one conditional carve-out in §6.3: if the coder's gate run surfaces a real product bug and fixes it, re-invoke the test-writer *then*, scoped to a pure headless unit test for that specific fix. Do not pre-schedule it.

---

## 8. Implementation order

Follow literally. Steps 1-2 first because everything after them must run through the edited flake.

1. **`flake.nix`** — add `pkgs.prettier`; update the three stale comments (package-list 125-130, shellHook 169-174, file header 5-13). Verify immediately: `nix develop --command prettier --version` and `nix develop --command command -v prettier` (must be a `/nix/store` path).
2. **`.prettierignore`** — create it (§2.5). Nothing downstream is safe until `.direnv/` is excluded.
3. **DISCOVERY PASS — read-only, fix nothing yet.** Run and capture:
   - `nix develop --command prettier --check "**/*.md"` → list of dirty files; decide `feat-list*.md` handling per §2.5.
   - `nix develop --command cargo fmt --all -- --check` → size of the format diff.
   - `nix develop --command cargo clippy --workspace --all-targets -- -D warnings` (**30-45 min timeout**) → count findings per crate/lint.
   **Report these counts back before proceeding.** If clippy findings are large (say >50), that is a scope signal the orchestrator needs.
4. **`cargo fmt --all`** (the writer) — run before any hand edit so later diffs are legible and rustfmt does not churn lines the coder is about to touch. Expect it to rewrite several hand-wrapped `.unwrap_or_else(|e| panic!(…))` lines in `smoke_headless.rs` (e.g. around lines 1551, 1554, 1648, 1746, 1851, 1853); that is fine and expected.
5. **`crates/wolf-gnome/tests/smoke_headless.rs`** — all edits in §4.2: delete the nine `#[ignore]` lines; fix the module header (lines 1, 13-21, 23-35); fix line 46; fix the `require_smoke_env` panic message (442-445); fix the stale TRI note (376-378); delete the two "relax the assertion" escape hatches (336-338, 1906-1909); optional `NOTE to coder` editorial pass.
6. **Fix clippy findings** (source only, per §9's policy). Re-run clippy to green.
7. **Build then test.** `cargo build --workspace --all-targets`, then `cargo test --workspace` (**30-45 min timeout each**). Fix real failures per §5.4's rules. Use `-- --nocapture` and `RUST_BACKTRACE=1` while diagnosing; inspect the retained PNGs under `$XDG_RUNTIME_DIR/wolf_gnome_*.png` before concluding anything about an OCR miss.
8. **Capture the proof artifact** (§2.7) — `cargo test -p wolf-gnome --test smoke_headless` showing `9 passed; 0 ignored`.
9. **Documentation**, in this order (each depends on what actually happened above): `build.md` (new gate section + the four drift fixes) → `crates/wolf-gnome/tests/README.md` (incl. the `tree.rs` drift) → `crates/wolf-gnome/README.md` → `tools/lint/README.md` → root `README.md`.
10. **`macos-followups.md`** — create it last, seeded from what actually changed (§3.2).
11. **`nix develop --command prettier --write "**/*.md"`** over everything touched (CLAUDE.md rule), then **re-run the full gate end to end in one shot** (§2.2) and paste the complete output. A gate that has never been run start-to-finish clean is not a gate.

---

## 9. Dependencies

- **New external package: `prettier`**, via `pkgs.prettier` in `flake.nix`'s `linuxShell` (§4.1). That is the **only** new dependency.
- **No new Cargo dependencies.** No `Cargo.toml` / `Cargo.lock` change, therefore no `CARGO_BAZEL_REPIN` and no `Cargo.Bazel.lock` / `MODULE.bazel.lock` churn.
- **No npm / `package.json` / `node_modules`.** Prettier comes from Nix. Adding a `package.json` would put a `node_modules/` in the tree that prettier's own default ignores would then have to handle.
- **No `flake.lock` regeneration.** `pkgs.prettier` exists in the already-pinned nixpkgs (rev `9ae611a…`, `nixos-unstable`, ~June 2026).
- **No new Bazel deps.** Nothing in this change enters the Bazel graph.

---

## 10. Lint considerations

**Expected fmt churn.** `cargo fmt --all` will reflow several hand-wrapped `panic!` / `unwrap_or_else` chains in `smoke_headless.rs`. Run it in step 4, before hand edits.

**Expected clippy volume — plan for it.** `cargo clippy --workspace --all-targets -- -D warnings` has plausibly **never** been run against this tree: macOS clippy is advisory, `manual`-tagged, `-Dwarnings` is opt-in, and it is scoped to `-p wolf-core -p wolf-ffi` (`tools/lint/README.md` line 40: "wolf-gnome's GTK stack does not build on macOS"). So `wolf-gnome`'s ~15 source modules *and* every crate's `#[cfg(test)] mod tests` are being linted at this bar for the first time. Expect a non-trivial first run.

**Policy the coder must follow (in priority order):**

1. **Fix the code.** Default answer for `needless_borrow`, `redundant_clone`, `useless_conversion`, `uninlined_format_args`, `needless_return`, `manual_let_else`, `&Vec<T>` params, etc.
2. **Narrowly-scoped `#[allow]` with a `reason`** — only when the lint is genuinely wrong at that site. Match the existing house style already in the tree, e.g. `crates/wolf-gnome/src/pane_view.rs::ratio_position` (lines 786-789) and `cell_floor` (799-802):
   ```
   #[allow(clippy::cast_possible_truncation, reason = "…why this is provably safe…")]
   ```
   Item- or block-level. **Never** crate-level.
3. **Forbidden:** a crate-level `#![allow(...)]`, a `[lints]` table in any `Cargo.toml`, a `clippy.toml` that lowers thresholds, or dropping `-D warnings` from the documented command. Any of these hollows out the gate on the day it ships. There is no `[lints]` table and no `clippy.toml` in the repo today — keep it that way.
4. **If a clippy fix changes a `wolf-core` public signature**, it also needs: `crates/wolf-core/src/README.md` + `architecture.md` updates *and* a `macos-followups.md` row (standing constraint). Prefer a non-signature-changing fix where one exists.

**Lints specifically checked against `smoke_headless.rs` (no action expected):**
- `clippy::len_zero` will **not** fire on `png_meta.len() > 0` — `std::fs::Metadata` has no `is_empty()`, and `len_zero` only fires when one exists.
- The file already uses inline format args (`{missing:?}`, `{e}`, `{ocr_text}`), so `uninlined_format_args` should be quiet.
- `REQUIRED_TOOLS: [&str; 4]`, the `static CAGE_LOCK: Mutex<()>`, and the `r#"…"#` scenario strings are all idiomatic.

**Doctest note (avoid creating a new failure):** when rewriting the module header of `smoke_headless.rs`, **keep the ```` ```text ```` annotation** on every command block. A bare ```` ``` ```` fence in a doc comment is parsed as Rust. (Verified: there are currently zero doctests in any *lib* target — every fenced doc block lives in `tests/` or `examples/`, which `cargo test --doc` does not collect — so the workspace has no doctest exposure today. Don't be the one to introduce it.)

**Markdown:** per CLAUDE.md, no hard line breaks inside a paragraph, and `prettier --write` every `.md` touched. `.prettierrc` sets `proseWrap: never` and `printWidth: 300` for `*.md`, so long single-line paragraphs are the house style — match it.

---
---

# PHASE 2 PLAN — OPEN ITEM A (swallowed first keystroke) + OPEN ITEM B (flaky file_search.rs)

## P2.1 Summary

Two independent defects. **A** is a `wolf-gnome` input-routing defect: the window-level key controller is installed in the **bubble** phase, so Neovim only receives a key that no other widget wanted, and the app compensates by parking focus on a dummy `DrawingArea` — an invariant that GTK, the sidebar `ListView`, and the pane rebuild path can all break. The first key after any focus disturbance is spent re-establishing focus and never reaches `WolfCore::send_input`. The fix is to make key delivery **focus-independent** by moving the controller to `PropagationPhase::Capture`, plus making the initial focus grab actually take effect and stop failing silently. **B** is a genuine `wolf-core` production data-loss bug that manifests as a test flake: all three SQLite stores (`mru.rs`, `recent_dirs.rs`, `command_mru.rs`) set `busy_timeout` **after** `journal_mode=WAL` (so the one pragma that needs a lock has no back-off), and then **delete the entire database file** on *any* error — including a transient `SQLITE_BUSY`. Two Wolf windows contending on the global store can silently wipe the user's whole recent-files history; eleven parallel tests sharing one `WOLF_STATE_DIR` do exactly that, which is the flake.

---

# TASK A — the swallowed first keystroke

## A.1 Facts established from the source (not hypotheses)

| # | Fact | Evidence |
| --- | --- | --- |
| F1 | The window key controller is installed in the **default (BUBBLE)** phase. | `crates/wolf-gnome/src/app.rs:240-262` — `gtk::EventControllerKey::new()` then `window.add_controller(controller)`; `set_propagation_phase` is never called. |
| F2 | Therefore Neovim is the **last-resort** key sink, not the default one. A bubble-phase controller on the window only fires if the focus widget and every widget between it and the window declined the key. | GTK4 `gtk_propagate_event` runs CAPTURE root→target, then TARGET, then BUBBLE target→root. |
| F3 | The app compensates for F2 by parking focus on a **dummy focusable `DrawingArea`** (the cmdline strip) so keys have something to bubble *from*. | `app.rs:160-162`, `app.rs:224-226`; documented at `crates/wolf-gnome/src/README.md:18`. |
| F4 | `focus_area.grab_focus()` is called **immediately after `window.present()`**, in the same `activate` callback, and its `bool` return value is **discarded**. | `app.rs:223-226`. |
| F5 | Under Wayland `present()` does not map synchronously (it needs an `xdg_surface.configure` round-trip), and GTK4's `gtk_widget_grab_focus_self` returns `FALSE` for an unmapped widget. So F4 is a **silent no-op** at that point, and the window is left with no focus widget. | GTK4 semantics + F4. |
| F6 | Deferring the grab into `window.connect_map()` did not help — `map` is `G_SIGNAL_RUN_FIRST`, and `adw::ApplicationWindow`'s internal content wrapping plus the still-pending first allocation mean this is not a reliable "the strip is mapped now" hook. | Pass-1 experiment (already reverted — do NOT re-propose). |
| F7 | Every widget in the editor region and the sidebar is **focusable**, and several are destroyed/recreated on ordinary render wakes. Unparenting a widget that holds the focus makes GTK4 clear the root focus. | `pane_view.rs:400` (`.focusable(true)` on every pane area), `pane_view.rs:630` (cmdline strip), `pane_view.rs:567-588` (`rebuild_pane_area` unparents **every** pane area then clears children), called from `ensure_initial_pane` (`:148`), `apply_split` (`:216`), `detect_closes` (`:274`), `resolve_pending_grids` (`:323`). |
| F8 | The sidebar's `refresh()` **destroys and recreates every `GtkListItemWidget`** on *every* traversal batch, including the final completion wake. `GtkListItemWidget` is focusable. | `tree_view.rs:110-113` — `root_store.remove_all()` then re-append; driven by `tree_view.rs:419-425`. |
| F9 | Nothing in the startup path re-grabs focus after F5. The three `grab_focus()` calls that exist all require a user action. | `picker.rs:141`, `picker.rs:158`, `tree_view.rs:196`, `pane_view.rs:496`. |
| F10 | The sidebar does **not** auto-select row 0 and does **not** focus on scroll. | `tree_view.rs:370` `selection.set_autoselect(false)`; `tree_view.rs:167-168` `scroll_to(pos, ListScrollFlags::NONE, None)`. |
| F11 | `wolf-core` cannot drop a key. `send_input` uses an **unbounded** channel; the pump batches and fuses but never discards. | `api.rs:331`, `api.rs:425-428`, `input.rs:375-400`. |
| F12 | The file search is **pure Rust on tokio** and never touches Neovim, so traversal completion cannot by itself make nvim print a message or enter a prompt. | `api.rs:1241-1270` → `files::spawn_traversal`. |
| F13 | The picker's own key controller is already **Capture** on `picker_box`, a descendant of the window — so a window-level capture controller runs *before* it, and the existing `picker.is_open()` early-`Proceed` at `app.rs:244-246` is exactly what keeps the picker working. | `picker.rs:396-418`, `app.rs:244-246`. |
| F14 | The failing smoke is the **only** one of the nine that does not call `.current_dir()`, so it runs with cwd = `crates/wolf-gnome`; the other eight use fresh temp dirs holding 1-3 flat `.md` files. | `smoke_headless.rs:1264-1275`. |
| F15 | Losing the first key is *harmless* for the two picker smokes (their first key event is a bare `Control_L` press, which `gdk_key` maps to `None`) and for the tree smoke (no keys). It is **not** harmless for the five split smokes, whose first key is `:`. Those five pass. | `smoke_headless.rs:619, 692` vs `:830, 904, 977, 1077, 1157`; `input.rs:113-133`. |

## A.2 Who holds keyboard focus, moment by moment

**Inside `build_ui`:** tree built and attached (`app.rs:196-202`); `panes.on_render_wake()` at `:218` runs synchronously (usually a no-op at this point); `window.present()` (`:223`); `focus_area.grab_focus()` (`:226`) → **fails silently (F5)**. Window focus widget: **`None`**.

**After the main loop starts:** nothing in wolf-gnome grabs focus (F9). Focus is whatever GTK's own initial-focus machinery picks — plausibly the header-bar **Open** button, or nothing.

**On the first core wake (`app.rs:210-215`):** `ensure_initial_pane` → `make_pane_area` → `rebuild_pane_area()` **unparents every pane area and clears children** (F7); `resolve_pending_grids` may call it a second time. Each is a focus-disturbing operation.

**On every file-search batch wake (`tree_view.rs:419-425`):** `refresh()` destroys and rebuilds every `GtkListItemWidget` (F8).

**Net:** at the moment the first key arrives, the window's focus widget is *not* the widget the design assumes, and it is being churned by two independent wake-driven paths. **The key path depends on a focus invariant that is never actually established and is repeatedly disturbed.**

## A.3 What consumes the first key

- **Hypothesis (a) "the tree ListView takes focus when populated" — REFUTED as stated.** `tree_view.rs:370` disables autoselect, `reveal` uses `ListScrollFlags::NONE`, no `grab_focus` on the list view or selection model. What it *does* do is **destroy focusable widgets** (F8) — a focus *disturbance*, not a focus *grab*.
- **Hypothesis (b) "phase/position" — CORRECT in substance and actionable.** F1/F2: the controller is bubble-phase, hence last in the chain. Any focus state that is unsettled, `NULL`, or parked on a widget that handles the key means the key never arrives.

**Mechanism:** the app's contract is "every key belongs to Neovim unless the picker is open" (`crates/wolf-gnome/src/README.md:21`), but the implementation encodes the *opposite* — "Neovim gets whatever no other widget wanted" — held together by a `grab_focus()` that never succeeds. "Throwaway `Escape` fixes it" is the signature of exactly this: the first key causes GTK to settle a focus target and is consumed doing so.

**Honest limitation:** the source proves the key path is focus-dependent and the grab is a silent no-op. It does NOT prove *which* GTK internal consumed the byte. Hence the mandatory experiments in A.4/A.5 — the recommended fix is correct under every candidate, because they all sit strictly *below* the window in the capture chain.

## A.4 Why an incomplete traversal masks it — the probe rows are CONFOUNDED

Rows 1 and 2 differ only in *elapsed time*; rows 1 and 3 differ only in *cwd*. "Traversal completion" is merely the quantity that moves with both.

1. **The completion refresh is not special.** `tree_view::refresh` is byte-identical on a mid-stream batch and on the completion wake — it does not read `complete`. A refresh destroys every row widget (F8); the **last** refresh is the one whose disturbance is still unpaid when a later key arrives. Under a still-streaming traversal the disturbances are continuous and the first key can fall in a settled window between them — a race, not a latch.
2. **The eight passing smokes contradict a pure "completion ⇒ next key eaten" latch** (F14/F15): five run in temp dirs whose traversal completes in microseconds and whose first key is `:`. They pass. So the discriminator is *either* the cwd's **content** *or* **elapsed time**, and the probe cannot tell which.

**Two control runs, MANDATORY before the fix:**

- **C1 (isolates time):** probe with cwd = a fresh temp dir containing exactly two `.md` files (what the eight passing smokes use) and wait **8 s** before typing. First key lost there too ⇒ discriminator is *time*. Not lost ⇒ discriminator is cwd content.
- **C2 (isolates the sidebar):** cwd = `crates/wolf-gnome` with `WOLF_SEARCH_MAX_ENTRIES=1` (see `api.rs:1237-1239`, `files::SearchBounds::from_env`) so the sidebar stays empty. Loss disappears ⇒ `tree_view::refresh` implicated. Loss persists ⇒ sidebar innocent, defect is purely focus/phase.

Report both results.

## A.5 Diagnostic D1 (run FIRST; temporary scaffolding, removed before commit)

In `app.rs`, temporarily:
- inside `connect_key_pressed` (`:241`) print `keyval.name()`, `state`, `picker.is_open()`, the branch taken (`picker-open` / `trigger` / `sent` / `unmapped`), and `window.focus().map(|w| w.type_().name())`;
- right after `focus_area.grab_focus()` (`:226`) print the **returned `bool`** and `window.focus().is_some()`;
- in a temporary `focus_area.connect_map` handler print the same two again.

Run under `cage` with stderr captured, cwd = `crates/wolf-gnome`, 0.5 s wait, typing `ihello`.

- **Branch A — `i` never appears, or appears with focus `None`/an unexpected widget while `h`…`o` appear with settled focus.** GTK is the consumer; the A.6 fix is correct and complete. This is what F1-F9 predict.
- **Branch B — `i` appears with branch `sent`.** GTK is innocent; loss is downstream. Then run **D2**: temporary `eprintln!` in `wolf-core::input::apply_batch`'s `flush_keys` (`crates/wolf-core/src/input.rs:378-386`) printing the exact `notation`. If it prints `"ihello"`, nvim received all six and discarded the `i` — nvim was in a state that consumes one key (a `msg_show` `return_prompt` / hit-enter). Collect `Snapshot::toast` / `Snapshot::cmdline` (`snap.toast.is_prompt()` exists — `api.rs:360-366`) and the bottom cmdline strip in the PNG. **STOP AND REPORT if you land in Branch B.**

## A.6 The fix (Branch A)

### Fix A1 (THE fix, required) — move the window key controller to the CAPTURE phase

**File:** `crates/wolf-gnome/src/app.rs`, `wire_keys` (lines 233-263). **Change:** before `window.add_controller(controller)` at line 262, set the controller's propagation phase to `gtk::PropagationPhase::Capture`. Nothing else in the closure changes — the `picker.is_open()` early-`Proceed`, the `is_picker_trigger` branch, and the `gdk_key` branch keep their exact current semantics and return values.

**Why this is root-cause, not a workaround:**
- GTK4's capture chain runs **root → target**. The window *is* the root, so a capture-phase controller observes **every** key event delivered to the window, regardless of which descendant is the focus target and regardless of whether the window has a focus widget at all. The entire class of "some widget or GTK's focus machinery ate it" disappears.
- It makes the implementation match the documented contract (`crates/wolf-gnome/src/README.md:21`). Bubble implements "Neovim gets the leftovers"; capture implements the stated contract directly.
- It removes the fragile invariant from the critical path. Focus becomes a nicety (somewhere for the picker/tree to return focus to), not a correctness requirement.
- Minimal: one line, no new types, no new dependency, no picker behavior change (F13 — the picker's own capture controller is on a descendant, so it still runs after ours, and the `is_open()` guard still yields to it).

**Expected behavior changes (improvements, document them):** `<Tab>`, `<S-Tab>` and arrows now reach Neovim instead of driving GTK focus navigation — unambiguously correct for a modal editor. Verify `<Tab>` reaches nvim; if GtkWindow's own capture controller still intercepts it, report it — a separate smaller follow-up, **not** a reason to revert A1. The header-bar **Open** button is no longer keyboard-activatable (still mouse-clickable; Ctrl+Shift+P remains the keyboard path). Acceptable.

### Fix A2 (required, small) — make the initial focus grab real and non-silent

**File:** `app.rs`, `build_ui`, lines 223-227.
1. Stop discarding the result of `focus_area.grab_focus()`; bind the `bool`.
2. Add a **one-shot** fallback on **`focus_area`'s own** `map` signal (NOT the window's — F6; pass 1 hooked the wrong widget), guarded by an `Rc<Cell<bool>>` "already focused" flag so it runs at most once.
3. If both attempts fail, log once via `eprintln!` in the style of `app.rs:146` / `:286` so this class of silent failure is visible next time.

**Framing:** A2 is NOT the fix — A1 is. A2 exists so the picker/tree focus-return target documented at `crates/wolf-gnome/src/README.md:18` is actually established. Land both.

### Ranked alternatives / rejected

| Rank | Option | Verdict |
| --- | --- | --- |
| 1 | **Fix A1 + A2.** | **Recommended.** |
| 2 | Attach the controller to `focus_area` in TARGET phase. | Rejected: makes the focus invariant *more* load-bearing. |
| 3 | Make pane areas + sidebar rows non-focusable. | Rejected as primary: breaks click-to-focus (`pane_view.rs:487-498`) and sidebar keyboard use; only removes *some* stealers. |
| X | Synthetic throwaway key at startup / `InputEvent::Key(Esc)` from `build_ui`. | **PAPER-OVER.** Institutionalizes the bug; injects a real `<Esc>` into the user's editor. |
| X | `glib::timeout_add` / idle re-grab after N ms. | **PAPER-OVER.** Timing workaround for a non-timing bug. |
| X | Changing the smoke's typed sequence / leading throwaway key / more settle / relaxed OCR. | **FORBIDDEN.** The smoke asserts the correct thing and is right to be red. |
| X | Re-`#[ignore]`ing the smoke or an env escape hatch. | **FORBIDDEN.** |

### Files to modify for Task A

1. `crates/wolf-gnome/src/app.rs` — A1 (`wire_keys` ~:262) and A2 (`build_ui` ~:223-227). Update the **module doc** at lines 22-24 to say the controller is installed in the **capture** phase so key delivery does not depend on which widget holds focus, and that the cmdline-strip focus grab exists only as the picker/tree focus-return target.
2. `crates/wolf-gnome/src/README.md` — rewrite **line 18** and **line 21**. Line 18's claim that keys "keep bubbling to the window-level key controller" becomes false. Add that `grab_focus()` before the widget is mapped is a no-op in GTK4, so the grab is hooked to `focus_area`'s `map`.
3. `crates/wolf-gnome/README.md` — grep for `bubbl`, `focus`, and the picker key-routing paragraph (~line 59). Correct anything describing keys as *bubbling*. The picker paragraph itself stays true (F13).
4. `architecture.md` — check ~line 87 and any GTK input-path description; correct if it states the bubble/leftover model.
5. `macos-followups.md` — one row, `Status = N/A`, mirroring the existing `pane_view.rs` row at line 14: wolf-gnome-only, no `wolf-core` API touched, no Bazel target for this crate.

## A.7 Headless guard? — NO, honestly

The defect is a GTK **event-propagation phase** plus **focus-timing** property. Observing it requires a live `GtkWindow`, a mapped surface, and a real key event. Every existing `#[cfg(test)] mod tests` in `crates/wolf-gnome` is a *pure* seam that runs with no display precisely because it touches no widget. `wire_keys` has no return value and no state. **Do not invent a headless test — it would be test theater.**

The *policy* inside the closure IS pure and could be extracted (`fn key_action(picker_open, keyval, state) -> KeyAction`), but **it would not guard this defect** — the bug is that the callback is never *reached*, not that its policy is wrong. If wanted, land it as an explicitly-labelled separate refactor; do NOT present it as the regression guard.

**The actual guard is the existing smoke.** It was designed to catch exactly this and it did.

### The coder's suggested second smoke — REJECTED

1. **No covered behavior added** — same end state via a strictly weaker path.
2. **Institutionalizes the workaround** — a permanently-green "throwaway key first" smoke next to a red strict smoke invites "fixing" the product by tolerating the loss.
3. **Pure cost** — all nine serialize behind `CAGE_LOCK`; a tenth adds ~8-10 s for zero coverage.
4. **Triage value is free elsewhere** — obtained here from the retained PNG plus a one-off probe.

**ACCEPT INSTEAD (zero cost):** improve the *failure message* of the existing assertion in `window_renders_typed_text_under_cage` — not the assertion. State the retained PNG path and add: *"if row 0 is empty and the cursor sits two rows below it, the leading `i` was dropped — suspect a lost FIRST keystroke (input routing/focus), not a rendering failure."* Message change only; weakens nothing.

## A.8 Implementation order for Task A

1. Run **C1**, **C2**, **D1**. Report outputs. If D1 lands in **Branch B**, STOP and report — do not touch `app.rs`.
2. Apply **A1**. Rebuild; `cargo test -p wolf-gnome --test smoke_headless` must give `9 passed; 0 failed; 0 ignored`.
3. Apply **A2**. Rerun.
4. Manually verify: (a) Ctrl+Shift+P still opens the picker and typing/`Up`/`Down`/`Esc`/`Enter` inside it still work (the one thing capture could plausibly break — F13 says it should not); (b) `<Tab>` reaches nvim. Report (b) either way.
5. Remove all D1/D2 scaffolding. Re-run clippy + fmt.
6. Documentation (A.6 list), then `prettier --write`.

---

# TASK B — the flaky `crates/wolf-core/tests/file_search.rs`

## B.1 What exactly races

**Not `WOLF_STATE_DIR` per se, not an ordering assumption. SQLite contention amplified by a destructive self-heal — and the destructive path is a PRODUCTION bug.**

1. `common::ensure_hermetic_state_dir` (`crates/wolf-core/tests/common/mod.rs:234-253`) sets `WOLF_STATE_DIR` **once per test process**. All 11 tests share one dir and one `file-mru.sqlite`.
2. Every `WolfCore::start` opens **three** stores against it (`api.rs:257-275`: `RecentDirs`, `CommandMru`, `MruStore`), each eagerly `open_or_heal` → `init`. libtest runs the 11 in parallel threads ⇒ up to ~11 concurrent `Connection`s per DB, plus writes from the input pump (`api.rs:376-384`).
3. **`init` sets `busy_timeout` AFTER `journal_mode=WAL`** — `crates/wolf-core/src/mru.rs:241-243`. `PRAGMA journal_mode=WAL` is the one statement in `init` that can need an exclusive lock (the rollback→WAL conversion), and at that moment the connection has **no busy timeout at all**. Concurrent first-opens return `SQLITE_BUSY` immediately instead of backing off. *Identical bug in `crates/wolf-core/src/recent_dirs.rs:197-198` and `crates/wolf-core/src/command_mru.rs:200-201`.*
4. On **any** `init` error, `open_or_heal` (`mru.rs:264-284`) calls `remove_db_files` — **deletes `file-mru.sqlite`, `-wal`, `-shm`** — and re-inits. On **any** query error, `with_conn` (`mru.rs:195-225`) does the same. **No error classification at all.**
5. Deleting the file unlinks it from under every other live connection in the process. On Linux those connections keep writing to the now-unlinked inode; their rows vanish from the visible DB.

**The observed failure follows exactly.** `mru_seed_follows_nvim_cd` (`file_search.rs:679`) records B's row at `:694`, then a sibling test's `MruStore::open`/`RecentDirs::open` hits a busy `journal_mode` pragma and deletes the DB. `start_file_search` → `self.mru.recent(&root)` (`api.rs:1244`) returns empty. `sub/recent_b.md` is still found *by the traversal*, so `assert_eq!(seeded.len(), 1)` at `:727` **passes**, and the failure surfaces one line later at `assert!(seeded[0].mru_rank.is_some())` (`:733-736`) — precisely the reported message. `mru_seed_is_synchronous_and_not_duplicated` (`:468`) has the same shape and exposure.

## B.2 Is the silent degradation correct?

- **Silent degradation on READ is correct — KEEP IT.** `MruStore::recent` returning `Vec::new()` on failure and `record` being fire-and-forget are right for a disposable cache. `start_file_search`'s documented "a disabled/unreadable store yields an empty seed" (`api.rs:1230-1231`) is a fine contract.
- **Silently DELETING the whole database on any error is NOT correct and is a real production defect.** It conflates "corrupt or schema-incompatible" — which warrants recreation, and which `init`'s schema probe at `mru.rs:255` already detects — with "this statement transiently failed", where deletion is catastrophic. In production: **two Wolf windows contending on the global store can silently wipe the user's entire recent-files history across every project**, with only a `tracing::debug!`. `record`'s "never block opening a file" rationale (`mru.rs:100-102`) justifies *ignoring* the error, not destroying the data.
- **Should the TEST distinguish?** Yes, and today it half-can, which is why it is flaky rather than vacuous. **Do not weaken either assertion.** Add an assertion that fires at the *true* point of breakage (B3).

## B.3 The fix

**`WOLF_STATE_DIR` per-test override is NOT viable — do not attempt.** `state_dir()` (`crates/wolf-core/src/state.rs:194-197`) reads the process-global env var at `WolfCore::start` time. In a multi-threaded test binary a per-test `set_var` is a data race against every other running test, and `std::env::set_var` is `unsafe` in recent editions. The existing helper is deliberately set-if-absent. Leave it alone.

### Fix B1 (required) — pragma ordering

**Files:** `crates/wolf-core/src/mru.rs::init` (240-257), `recent_dirs.rs::init` (~195-205), `command_mru.rs::init` (~198-208). Move the `busy_timeout` `pragma_update` to **immediately after `Connection::open`**, before `journal_mode=WAL`. Update each `init` doc comment to state the ordering is load-bearing: the WAL conversion is the one statement that can block, so the back-off must already be installed.

### Fix B2 (required) — never destroy the DB for a non-corruption error

**Files:** same three modules, in `open_or_heal` and `with_conn`. Add one private classifier per module, `fn warrants_recreate(err: &rusqlite::Error) -> bool`, matching on `rusqlite::Error::SqliteFailure(ffi_err, _)`:
- `true` for `ErrorCode::DatabaseCorrupt`, `NotADatabase`, `SchemaChanged`; plus the wrong-schema case, which surfaces as `ErrorCode::Unknown` (`SQLITE_ERROR`) with a message containing `no such table` / `no such column` — that is what `init`'s probe at `mru.rs:255` triggers, and it MUST keep working.
- `false` for `DatabaseBusy`, `DatabaseLocked`, `SystemIoFailure`, `ReadOnly`, `DiskFull`, and every non-`SqliteFailure` variant.

Then: `open_or_heal` only calls `remove_db_files` + retry when `warrants_recreate(&err)`; otherwise `tracing::debug!` and return `None` (degrade to "no connection", retried next call) **without touching the file**. `with_conn` identically. Keep the existing named-local guard so `clippy::significant_drop_in_scrutinee` stays quiet, and keep the "retry at most once" shape.

**Do NOT refactor the three modules into a shared abstraction on this branch** — bigger than the gate warrants. Duplicate the small classifier (or `pub(crate)` one copy) and keep the diff mechanical. **Fix all three** — the hazard is identical (`recent_dirs` = Open-Recent menu, `command_mru` = palette ordering), and shipping one fixed would falsify `crates/wolf-core/src/README.md:324`.

### Fix B3 (required) — strengthen the two MRU-dependent tests

**File:** `crates/wolf-core/tests/file_search.rs` (already Bazel-wired as `rust_test(name = "file_search")`, `crates/wolf-core/BUILD.bazel:55-66` — **no wiring change, no new file**).
- In `mru_seed_is_synchronous_and_not_duplicated` (:468), inside the block at :478-481: after `store.record(&cwd, &cwd.join("seeded.md"))`, read back through the *same* `store` — `store.recent(&cwd)` — and assert it contains `seeded.md`. Message must name the cause: the MRU write did not land, so any later seed assertion would be testing nothing.
- In `mru_seed_follows_nvim_cd` (:679), inside the block at :692-696: after both `record` calls, assert `store.recent(&cwd_b)` contains `sub/recent_b.md` and `store.recent(&cwd_a)` contains `recent_a.md`.

These are **strengthenings** — they convert a silent, downstream-manifesting degradation into a loud failure at the true point of breakage, and make it impossible for either test to pass vacuously against an empty store.

### Fix B4 (new tests, headless) — pin the fixed behavior

**File:** `crates/wolf-core/tests/mru.rs` — an existing headless (no-nvim) binary against real SQLite on a real tempfs, already Bazel-wired as `rust_test(name = "mru")` at `crates/wolf-core/BUILD.bazel:1139-1146` with `srcs = ["tests/mru.rs"]`. **Modifying it needs NO BUILD.bazel change.** It has 8 tests today; none covers concurrency or corruption. Add:

- **T1 `a_second_store_open_does_not_destroy_existing_rows`** — open store A on a fresh `state_tempdir()`, `record` a file, then open a *second* `MruStore` on the same dir (exactly what a second `WolfCore` does), and assert the row is still visible through **both** handles. The precise production hazard and the precise flake mechanism, fully deterministic. **Load-bearing.**
- **T2 `concurrent_opens_preserve_rows`** — record N (say 20) files, spawn K (say 8) `std::thread`s that each `MruStore::open(dir)` and `recent(dir)`, join, assert every reader saw all N; then assert the main store still sees all N. Deterministic after the fix, reliably red before it under load. Keep K/N small so the test stays `size = "small"`.
- **T3 `a_corrupt_database_is_still_healed`** — write garbage bytes over `<state_dir>/file-mru.sqlite`, then `MruStore::open` + `record` + `recent` and assert it works. **Guard against over-narrowing Fix B2** — proves the legitimate recreation path still fires. Check whether `tests/mru.rs` already covers this before adding.

Reuse the file's existing helpers (`touch`, `canon_tempdir`, `state_tempdir`, `count_named`, `ends_with`); match its doc-comment style.

### Fix B5 (HOLD IN RESERVE — do NOT land first)

A file-local `static MRU_DB_LOCK: std::sync::Mutex<()>` in `file_search.rs`, taken for the whole body of **all eleven** tests (poison-recovered via `into_inner`, mirroring `smoke_headless.rs:61`). Scoping it to only the two MRU tests would be useless — every test opens all three stores via `WolfCore::start`.

**Do not land this first.** It serializes the entire binary (11 × real-nvim startup, ~max → ~sum) and *hides* the production bug B1+B2 actually fix. Land B1-B4, then re-run `cargo test -p wolf-core --test file_search` **× 10** and once inside a full `cargo test --workspace`. Only if a flake survives should B5 be added — with a comment saying it is defence-in-depth on top of B1/B2, not a substitute.

## B.4 In scope? YES

The standing instruction is "fix every failure the gate surfaces", and a flaky test in a gate is a broken gate. More importantly the investigation turned up a genuine **production data-loss defect** in a store that ships on both front-ends. Deferring would leave a known "concurrent Wolf windows can silently wipe your recent files" bug in `main`.

## B.5 Public-API and macOS impact — READ THIS

**LOUD NOTE: `wolf-core` BEHAVIOR consumed by `wolf-ffi` / `wolf-macos` changes, even though no signature does.**

- **No public signature changes.** `MruStore::open` / `record` / `recent`, `RecentDirs`, `CommandMru` keep exact signatures; `with_conn`, `init`, `open_or_heal`, `remove_db_files` are private. `wolf-ffi` and the Swift bindings need **no** source change; `Cargo.toml` / `Cargo.lock` untouched (no `CARGO_BAZEL_REPIN`).
- **Documented behavior DOES change**, and three docs assert the behavior being removed:
  - `crates/wolf-core/src/mru.rs` module doc **lines 20-22**: "Any open/schema/query failure is handled SILENTLY: the sqlite file (plus its `-wal`/`-shm` sidecars) is deleted and the schema recreated." Rewrite: only a *corrupt or schema-incompatible* database is recreated; a transient failure (busy/locked/IO) degrades to an empty result or a dropped record and **never** deletes the store.
  - The same paragraph in `recent_dirs.rs` and `command_mru.rs` module docs, plus the `with_conn` / `open_or_heal` / `init` doc comments in all three (`mru.rs:185-194`, `:259-263`, `:233-239` and siblings).
  - `crates/wolf-core/src/README.md` **line 324** and the corresponding `RecentDirs` (~:333) and `CommandMru` entries. **Line 317**'s mention of `MRU_BUSY_TIMEOUT_MS` should note the pragma ordering is load-bearing.
- `architecture.md` — grep for `file-mru`, `recent_dirs`, `command_mru`, `self-heal`; correct any claim of unconditional recreation.
- **`macos-followups.md` — add one row** (`2026-08-01`, `linux-test-fixes`, `Status = Open`), following the table shape at line 9: all three SQLite stores now set `busy_timeout` before `journal_mode=WAL` and only recreate the DB for genuine corruption/schema incompatibility; the previous code deleted the whole store on any transient `SQLITE_BUSY`, which could wipe a macOS user's `~/Library/Application Support/Wolf/{file-mru,folder-mru,command-mru}.sqlite` when two windows contended. macOS needs a spot check that recents/Open-Recent/palette ordering survive a two-window session, plus `bazel test //...` (the `//crates/wolf-core:mru` and `//crates/wolf-core:file_search` targets gain/modify tests but **no `BUILD.bazel` change**).

## B.6 Implementation order for Task B

1. **B1** (pragma ordering) in all three modules. Build.
2. **B2** (classifier + conditional heal) in all three. Build; clippy.
3. **B4** (`tests/mru.rs`: T1, T2, T3). `cargo test -p wolf-core --test mru`. **T1/T2 MUST be verified RED before B1/B2 and GREEN after** — stash the fix, run, unstash. Paste both results; a new test never seen failing is not a proven guard.
4. **B3** (`tests/file_search.rs` read-back assertions). `cargo test -p wolf-core --test file_search`.
5. Soak: `cargo test -p wolf-core --test file_search` **× 10** consecutively, then once inside `cargo test --workspace`. Paste counts.
6. Only if a flake survives: **B5**, with the mandated comment.
7. Docs (B.5 list) + `macos-followups.md` row; `prettier --write`.

---

## P2 Combined implementation order

1. Task A diagnostics: **C1, C2, D1** → report. (If Branch B, STOP before touching `app.rs`.)
2. **A1** → `cargo test -p wolf-gnome --test smoke_headless`, expect `9 passed; 0 failed; 0 ignored`.
3. **A2** → rerun; manual picker + `<Tab>` check; remove all scaffolding.
4. **B1** → **B2** → **B4** (red-then-green proof) → **B3** → soak.
5. `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`.
6. All documentation edits (A.6 + B.5 lists), then `prettier --write "**/*.md"`.
7. Full gate end-to-end in one shot (§2.2), output pasted.

## P2 Dependencies

**None.** No new crates, no `Cargo.toml` / `Cargo.lock` change (no `CARGO_BAZEL_REPIN`), no new Nix packages, no `BUILD.bazel` change (both test files touched are already wired: `crates/wolf-core/BUILD.bazel:55-66` and `:1139-1146`; `crates/wolf-gnome` has no Bazel target by design). `gtk::PropagationPhase` and `EventController::set_propagation_phase` are already available through the `gtk4` dependency in use.

## P2 Lint considerations

- **No new `#[allow]` expected.** If unavoidable: item- or block-level with `reason = "…"`, matching `pane_view.rs:789-792` / `:802-805`. Crate-level `#![allow]`, a `[lints]` table, or a `clippy.toml` remain forbidden.
- **B2's classifier** matches on `rusqlite::ErrorCode`. Expect `clippy::match_same_arms` if several codes map to the same boolean — collapse into one `|`-joined pattern rather than allowing the lint. `matches!` is the idiomatic shape.
- **A2** binds the `bool` from `grab_focus()`. Do NOT write `let _ = focus_area.grab_focus();` — that reintroduces the exact silent failure being fixed. Use the value.
- Keep the `with_conn` mutex guard in a **named local** in all three modules (`mru.rs:198`) so `clippy::significant_drop_in_scrutinee` stays quiet.
- **B4 T2** spawns threads; `expect` the `JoinHandle::join` results, do not `let _` them.
- Doc-comment edits in the three store modules: watch `clippy::doc_lazy_continuation` and keep any fenced block annotated ```` ```text ```` — `crates/wolf-core` **is** a lib target, so a bad fence becomes a failing doctest under `cargo test --workspace` (see the `rank.rs` regression pass 1 found).

---

## P2 test-writer pass — DONE (orchestrator decisions recorded)

Files modified: `crates/wolf-core/tests/mru.rs` (T1/T2/T3 added), `crates/wolf-core/tests/file_search.rs` (B3 read-back assertions), `crates/wolf-gnome/tests/smoke_headless.rs` (§A.7 message-only change). No `BUILD.bazel` change — both Rust targets already wired.

**ACCEPTED DEVIATION (orchestrator decision).** §B.4 said "record N files, THEN spawn K threads". The test-writer showed that shape cannot be red before B1/B2: on a DB already converted to WAL, `PRAGMA journal_mode=WAL` is a no-op needing no lock and `CREATE TABLE IF NOT EXISTS` takes no write lock, so sequential later opens have nothing to contend for. T2 was therefore reshaped so all K+1 opens rendezvous on a `Barrier` and race the **first** open — precisely the mechanism §B.1 item 3 names. The asserted contract is unchanged (every reader sees all N; the original handle still sees all N afterwards). **T2 is the red-then-green proof.**

**T1 is expected GREEN both before and after** the fix, for the same reason. It is a contract guard for the §B.5 production hazard, not the red-then-green proof. Do NOT treat T1-green-before as a failed proof, and do NOT contort it to be red.

Two watch items flagged for the coder:
- Post-fix T2 requires all 9 concurrent opens to succeed. B2 makes a non-corruption `init` failure return `None` (disabled handle → empty `recent()` → T2 red). With B1's `busy_timeout` installed before `journal_mode=WAL` this should not happen. If T2 is flaky-green after the fix, that is a signal about the back-off — **not** a reason to relax the test.
- T3 depends on B2's classifier returning `true` for `ErrorCode::NotADatabase` (a garbage header surfaces as `SQLITE_NOTADB` when the `journal_mode` pragma reads the schema). That is exactly the over-narrowing concern T3 guards.

---
---

# PASS 2 OUTCOME + PHASE 3 PROBLEM STATEMENT

## Task B — LANDED AND GREEN

`mru.rs` / `recent_dirs.rs` / `command_mru.rs` all fixed (B1 pragma order + B2 `warrants_recreate` classifier). `cargo test -p wolf-core --test mru` 11/11, `--test file_search` 11/11 (soak x10 = 10/10), `--test command_mru` 7/7, `--test recent_dirs` 7/7. **B5 was NOT needed.** Docs updated: `crates/wolf-core/src/README.md`, `architecture.md` line 30, `macos-followups.md` (two rows).

### Plan corrections from pass 2 (both important)

1. **§B.1 item 3's premise was FALSE.** `rusqlite` 0.31 already calls `sqlite3_busy_timeout(db, 5000)` inside `Connection::open_with_flags` (`inner_connection.rs:121`). The busy timeout was already 5000 ms when `journal_mode=WAL` ran; the store's own pragma only *lowered* it to 2000 ms afterwards. And the WAL conversion fails anyway — SQLite does not run the busy handler for that lock:
   ```text
   PROBE open_or_heal err: SqliteFailure(Error { code: DatabaseBusy, extended_code: 5 }, Some("database is locked"))
   ```
   **So B1 is not what stops the data loss — B2 is.** B1 still landed (the invariant must not rest on a dependency's implementation detail, and only the explicit pragma makes `*_BUSY_TIMEOUT_MS` the value actually in force); the doc comments now say exactly that.

2. **A real bug in the first `warrants_recreate`, caught by the gate.** Matching only `rusqlite::Error::SqliteFailure` silently disabled the legitimate recreation path, because `rusqlite-0.31.0/src/error.rs:430-448` (`error_with_offset`, `modern_sqlite` feature) converts an `ErrorCode::Unknown` with a valid `sqlite3_error_offset` into **`Error::SqlInputError`**, not `SqliteFailure`. The schema probe's `prepare` raises `SqlInputError`. Gate step 5 caught it as `wrong_schema_db_is_silently_recreated` failing in `command_mru`. The classifier now destructures **both** variants in all three modules.

### T2 red-then-green — NOT ACHIEVED, and the test was correctly left alone

T2 passed 20/20 both before and after. The coder proved the hazard was live pre-fix by instrumenting `remove_db_files`: **9 destructive-heal invocations across 20 pre-fix runs (all `DatabaseBusy`), 0 after.** But every one lands during the simultaneous open burst, which by T2's construction happens *before* the main thread records any row — so the destruction is benign and the assertion never trips. T2 is a sound contract guard (it DID catch the `SqlInputError` classifier regression) but is not a red-then-green proof.

## OPEN ITEM C — test gaps identified by the coder (Phase 3 test work)

1. **`crates/wolf-core/tests/mru.rs` has NO wrong-schema test.** `a_corrupt_database_is_still_healed` goes through `NotADatabase`, a `SqliteFailure` — so the identical `SqlInputError` bug in `mru.rs` would have shipped undetected. Only `command_mru.rs` and `recent_dirs.rs` have `wrong_schema_db_is_silently_recreated`. Mirror it into `tests/mru.rs`.
2. **A deterministic red-then-green guard for B2.** Record N rows, then open a second raw `rusqlite::Connection` on `<state_dir>/file-mru.sqlite` and hold `BEGIN EXCLUSIVE` while a fresh `MruStore::open(dir)` runs; drop the lock and assert the N rows are still readable. Pre-fix that deletes the file (RED); post-fix it backs off (GREEN). No threads, no barrier, fully deterministic, targets B2 directly.
3. **`crates/wolf-core/tests/swap_recovery.rs` does not cover the startup `Untitled.md` path** — only dead/live-owner `SwapExists` for a real *file* fixture. That gap is why OPEN ITEM D shipped.

## OPEN ITEM D — the REAL Task A root cause (the architect's GTK hypothesis was REFUTED)

**Facts F4 and F5 of the Phase 2 plan are refuted. A1 (capture phase) and A2 would have fixed NOTHING.** `app.rs` was not touched; all scaffolding removed; `git diff crates/wolf-gnome/src/app.rs` is net zero.

### Evidence

**C1 (isolates time)** — fresh temp dir, two `.md` files, 8 s wait: first key **SURVIVED**. (The probe's auto-verdict said MISSING, but the PNG shows `hello` on row 0 with the cursor one row below — OCR read the leading `h` as `ﬁ` because it is clipped by the pane's left edge.) **The discriminator is NOT elapsed time.**

**C2 (isolates the sidebar)** — cwd `crates/wolf-gnome`, `WOLF_SEARCH_MAX_ENTRIES=1`, 0.5 s: first key **LOST**, row 0 empty, cursor two rows down. **The sidebar is innocent.**

**D1 — BRANCH B.** Every key including the `i` reached `send_input`, with focus a settled `GtkDrawingArea` throughout, and `grab_focus()` returned **`true`**:
```text
D1: post-present grab_focus() -> true; window.focus() = Some("GtkDrawingArea")
D1: key Some("i") state=ModifierType(0x0) focus=GtkDrawingArea branch=sent KeyEvent { key: Char('i'), ... }
D1: key Some("h") ... branch=sent      (… e, l, l, o, Escape all branch=sent)
```

**D2 — nvim was in a modal prompt.**
```text
D2: BEFORE "ihe" -> mode = { blocking = false, mode = "r?" }, lines = { "" }, msgs = "", bufname = ".../crates/wolf-gnome/Untitled.md"
D2: nvim_input("ihe") -> Ok(Ok(Integer(PosInt(3))))
D2: AFTER  "ihe" -> mode = { mode = "n" }, lines = { "" }
D2: nvim_input("llo") -> Ok(3);  AFTER -> mode = "i", lines = { "", "" }
```
`mode() == "r?"` is Neovim's **`MODE_CONFIRM`** — a `do_dialog` modal prompt is up. The `i` is spent answering it; `h`/`e` then run as normal-mode motions.

### ROOT CAUSE: a stale swap file for `<cwd>/Untitled.md`

`~/.local/state/wolf/swap/ba8b974071464706/` held **24** accumulated `%home%…%crates%wolf-gnome%Untitled.md.sw*` files. Deterministic in both directions:

| stale swaps for `<cwd>/Untitled.md` | `mode()` at first key | result |
| --- | --- | --- |
| 0 (dir removed) | `"n"` | `hello` RENDERED |
| 1 | `"r?"` | first key LOST |

**Every run leaves exactly one more behind**, because the smoke scenario `kill`s the app. `window_renders_typed_text_under_cage` is the only smoke that does not set `.current_dir()`, so it runs in `crates/wolf-gnome` and poisons itself. **That is exactly the pass-1 "passed 9/9 once, then repeatable 8/9".**

### Two defences in `runtime/nvim/init.lua` BOTH fail

1. **`_G.wolf_untitled_swap_exists` (~line 1607) can never match.** It compares `vim.fn.swapinfo(...).fname` to the absolute `abspath`. Measured against the pinned nvim:
   ```text
   fname = "~jplein/Git/jplein/wolf/crates/wolf-gnome/Untitled.md"
   host = "nixos", pid = 0, mtime = 0, inode = 0, dirty = 1
   ```
   nvim returns `fname` **home-relative with a `~user` prefix** (`home_replace`), so `info.fname == abspath` is never true for any project under `$HOME`. The startup naming therefore never advances to `Untitled 2.md`. This is `home_replace`, **not** Linux-specific — **macOS is affected identically.**
2. **The `SwapExists` autocmd (`init.lua:45`) never fires.** Proven by temporarily wrapping its callback to append to a log: with `mode()` at `r?` the file was **never created**. So `do_swapexists()` returns 0 and nvim falls through to `do_dialog`. The coder did NOT diagnose why — that is the fix decision.

Note `pid = 0` above: even if the autocmd ran, it would take init.lua's "no usable owner pid → dead → `swapchoice = "r"`" branch.

### §A.8 step 4 manual checks (both moot, but recorded)

- **(a) Ctrl+Shift+P picker** — both picker smokes pass, but this check existed to validate A1; A1 was not applied, so it proves nothing new.
- **(b) `<Tab>` reaches nvim — YES, already, in the BUBBLE phase.** GTK focus navigation does not intercept Tab today. The plan's predicted A1 "improvement" is already the status quo:
  ```text
  TABPROBE: controller got Some("Tab") state=ModifierType(0x0)        -> gdk_key=Some(KeyEvent { key: Tab, shift: false, ... })
  TABPROBE: controller got Some("Tab") state=ModifierType(SHIFT_MASK) -> gdk_key=Some(KeyEvent { key: Tab, shift: true, ... })
  ```

**The §A.6 doc edits were deliberately NOT made** — they all assert capture-phase behaviour that does not exist. `crates/wolf-gnome/src/README.md:17` and `:20` remain accurate for the shipped code.

## GATE STATUS: green ONCE, but NON-IDEMPOTENT

```text
STEP1 cargo fmt --all -- --check                             PASS
STEP2 prettier --check "**/*.md"                             PASS
STEP3 cargo clippy --workspace --all-targets -- -D warnings  PASS
STEP4 cargo build --workspace --all-targets                  PASS
STEP5 cargo test --workspace                                 PASS
GATE EXIT: 0
```
91 `test result: ok` lines, zero `FAILED`, all nine smokes ok.

**BUT step 5 only passes because the swap dir was manually wiped first.** With the exact binary the gate just built and no code change:
```text
### swap files left by the gate run:
%home%jplein%Git%jplein%wolf%crates%wolf-gnome%Untitled.md.swp
### immediately re-running the SAME gate step 5 smoke target:
test window_renders_typed_text_under_cage ... FAILED
test result: FAILED. 8 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```
**The gate passes once per manual swap-dir wipe. It is NOT green until OPEN ITEM D is fixed.**

---
---

# PHASE 3 PLAN — Task A3 (the swap modal) + Task C3 (three test gaps)

## P3.1 Summary

The lost first keystroke is a **product bug in `runtime/nvim/init.lua`'s anonymous-buffer naming**, not a GTK bug. When the `VimEnter` naming calls `nvim_buf_set_name(buf, "<cwd>/Untitled.md")`, Neovim creates the buffer's swap file *as part of the rename* — and on that code path Neovim does **not** offer the `SwapExists` hook, so a pre-existing swap for that exact name goes straight to the blocking `do_dialog` ATTENTION/E325 modal (`mode() == "r?"`). The designed defence (`_G.wolf_untitled_swap_exists`, which should have made the startup buffer take `Untitled 2.md`) never fires for any project under `$HOME`, because it compares `swapinfo().fname` — which Neovim stores `home_replace`d as `~<username>/…` — against an absolute path.

## P3.2 WHY the `SwapExists` autocmd does not fire

### The exact site where the startup `Untitled.md` buffer gets its name and its swap file

| Step | File : line | What happens |
| --- | --- | --- |
| 1 | `crates/wolf-core/src/nvim.rs:1530-1541` | nvim is launched **without `-n`** (deliberate), so `'updatecount'` keeps its default `200` — swap-file writing is enabled. |
| 2 | `crates/wolf-core/src/nvim.rs:1547` | `WOLF_SWAP_DIR` = `swap_dir_for(canonical cwd)` = `state_dir()/swap/<16-hex-hash>` (`state.rs:223-229`, `api.rs:293-301`). **Stable across respawns of the same cwd.** |
| 3 | `runtime/nvim/init.lua:32-36` | `opt.swapfile = true`; `opt.directory = swap_dir .. "//"`. The trailing `//` makes the swap name the buffer's **full path with `/` → `%`** plus `.swp` — matching the observed `%home%jplein%…%Untitled.md.swp`. |
| 4 | `runtime/nvim/init.lua:1747-1754` | The `WolfUntitled` **`VimEnter`** autocmd → `wolf_name_untitled_current(_G.wolf_untitled_swap_exists)`. |
| 5 | `runtime/nvim/init.lua:1707-1731` → `:1673-1686` | → `wolf_name_untitled(buf, extra_collision)`. |
| 6 | **`runtime/nvim/init.lua:1677`** | **`pcall(vim.api.nvim_buf_set_name, buf, abspath)` — THE CALL SITE.** |

Nothing in `wolf-core` names or opens this buffer over RPC; the whole naming is `init.lua`'s `VimEnter`.

### The mechanism

`nvim_buf_set_name` → `rename_buffer` → `setfname` → `buf_name_changed` → `ml_setname`. `ml_setname`'s first branch, verbatim from `neovim/src/nvim/memline.c`:

```text
if (mfp->mf_fd < 0) {             // there is no swapfile yet
    // When 'updatecount' is 0 and 'noswapfile' there is no swapfile.
    // For help files we will make a swapfile now.
    if (p_uc != 0 && (cmdmod.cmod_flags & CMOD_NOSWAPFILE) == 0) {
      ml_open_file(buf);  // create a swapfile
    }
    return;
  }
```

`p_uc` is `'updatecount'` = 200 (step 1) and the buffer is `swapfile`-enabled (step 3), so **`ml_open_file()` runs inside the rename** → `findswapname()` → the stale `%…%Untitled.md.swp` is found → the modal.

`SwapExists` does not fire there because the event is raised **only where Neovim has armed the choice machinery** (`:help SwapExists`: "Only when it is possible to select a way to handle the situation, when Vim would ask the user what to do"). That arming happens on the *file-load* path (`do_ecmd` → `open_buffer` → `readfile` → `check_need_swap`), which is what `:edit` goes through — the internal gate is `swap_exists_action != SEA_NONE` in `findswapname`, and `nvim_buf_set_name` never sets it. Known upstream issue: **neovim/neovim#23145, "There appears to be no way to handle response of swap check on `nvim_buf_set_name`"**.

### Ruled out — do NOT re-investigate

- **Ordering (buffer created before the autocmd registers)?** NO. `init.lua` is sourced by `-u` during startup; `VimEnter` fires at the *end* of startup. The `WolfSwap` autocmd at `init.lua:44-79` is registered long before line 1677 runs.
- **`SwapExists` only for `:e`-style loads?** Effectively YES — that is the answer. `:e` and the Tier-1 reopen *do* fire it; that is why `swap_recovery.rs`'s two file-fixture tests pass.
- **Created by wolf-core over RPC?** NO. Called from Lua inside `init.lua`.
- **`eventignore`?** NO — appears nowhere in the repo.
- **Neovim's default `nvim_swapfile` `SwapExists` handler (0.11+)?** Present but irrelevant: only auto-chooses `(E)dit` for a **running** owner, and is behind the same gate.

### Two consequences

1. The `pcall` at `init.lua:1677` gives **zero protection**. The dialog is not an error; it blocks and consumes input.
2. `'shortmess'` flag `A` suppresses the ATTENTION *message*, not the *dialog*. **Do not try it.**

## P3.3 The fix, ranked

### R1 (REQUIRED) — fix the identity comparison in `_G.wolf_untitled_swap_exists`

`runtime/nvim/init.lua:1623` does `info.fname == abspath`. Neovim writes `b0_fname` through `home_replace`, and on Unix additionally rewrites a leading `~/` to `~<username>/`. Measured: `fname = "~jplein/Git/jplein/wolf/crates/wolf-gnome/Untitled.md"`.

**Normalize the STORED value FORWARD to absolute; never normalize the query backward.** Add a file-local helper `swap_fname_matches(fname, abspath) -> boolean` immediately above `wolf_untitled_swap_exists`:

1. Return `false` for a non-string or empty `fname`.
2. Return `true` if `fname == abspath` **(keep this branch)** — `home_replace` only rewrites under `$HOME`, so for a project *outside* `$HOME` (`/tmp/…`, `/var/folders/…`) `b0_fname` is already plain absolute. This is the only branch every existing test exercises.
3. Otherwise `pcall(vim.fn.fnamemodify, fname, ":p")` and return `true` iff the result is a string equal to `abspath`.

**Why `fnamemodify(…, ":p")`:** `:p` performs `~` / `~user` / `$VAR` expansion via `expand_env` then makes the name a full path; for an already-absolute path it is identity, so branch 3 subsumes branch 2 — which is why it is robust for a path **not** under `$HOME`. **Reject `vim.fn.expand()`** — it also does *wildcard* expansion, so a filename containing `*`, `?`, `[`, `{` or `` ` `` would be mangled. **Reject normalizing `abspath` with `fnamemodify(abspath, ":~")`** — that produces `~/…`, never the `~<username>/…` form Neovim stores, so it would *never* match. `pcall` is required because `~someuser` for a nonexistent user (a swap copied from another machine) can raise.

### R2 (REQUIRED) — add the exact, `fname`-independent criterion

Add `untitled_swap_file(abspath) -> string|nil`, returning the swap path Neovim itself would choose: `WOLF_SWAP_DIR .. "/" .. <abspath with every "/" replaced by "%"> .. ".swp"`, or `nil` when `WOLF_SWAP_DIR` is unset/empty.

`_G.wolf_untitled_swap_exists(abspath)` becomes **`true` if EITHER** (a) `uv.fs_stat(untitled_swap_file(abspath)) ~= nil`, **OR** (b) the existing `fs_scandir` loop with `swap_fname_matches(info.fname, abspath)` substituted for the broken equality.

- **(a) is O(1) and is precisely the test Neovim performs.** Because `'directory'` ends in `//`, `makeswapname` builds the name from the full path with separators replaced by `%`, and `findswapname` only raises the dialog on the **first** candidate, the `.swp` one. It cannot be defeated by an unreadable/short/`b0`-less swap file, which the `swapinfo` scan can.
- **(b) is kept** because the documented contract is broader — a `.swo`/`.swn` leftover still holds recoverable edits though it would not raise a dialog — and it is the fallback if the `%`-mangling ever drifts (e.g. a symlinked cwd where `fix_fname`/`resolve_symlink` differs from `getcwd()`).

**Gotchas:** Lua `abspath:gsub("/", "%%")` returns **two** values and `%` is the escape char in the *replacement*, so `"%%"` is a literal `%` — bind it wrapped in parentheses. Defensively collapse repeated `/` before mangling (guards `cwd == "/"`, where `cwd .. "/" .. name` yields `//Untitled.md`). Guard a non-absolute `abspath` through `fnamemodify(abspath, ":p")` first.

### R3 (STRONGLY RECOMMENDED) — apply the check at ALL THREE naming call sites

Today `extra_collision` is passed **only** by the `VimEnter` autocmd (`:1751`); `_G.wolf_new_untitled` (Cmd-N, `:1693-1696`) and the `Chdir` arm's `wolf_name_untitled_current()` pass `nil`. The identical modal bug is reachable mid-session: crash → relaunch (startup correctly takes `Untitled 2.md`) → user presses **Cmd-N** → `wolf_next_untitled` finds `Untitled.md` free → `nvim_buf_set_name` → stale swap → **same blocking modal, eating the next keystroke**.

**Safe and behaviour-preserving in-session:** every named `Untitled N.md` buffer owns its own swap, so the swap check only adds a collision for a name that *already* collides via the buffer-name check; a `:bwipeout`ed buffer has its swap deleted by Neovim. Net effect is only against swaps left by a **previous, dead** nvim. Verified: `crates/wolf-ffi/tests/untitled.rs` all use fresh `tempfile::tempdir()` cwds, so `Untitled.md → Untitled 2.md → Untitled 3.md` is unchanged.

Implement by making `_G.wolf_name_untitled_current` and `_G.wolf_new_untitled` **default** `extra_collision` to `_G.wolf_untitled_swap_exists` when `nil`. Covers Cmd-N and `Chdir` with **no Rust change**. *If scope must be cut, cut this LAST.*

### R4 — `setlocal noswapfile` is the WRONG fix. REJECTED.

1. **Loses a product promise the suite pins.** `crates/wolf-core/tests/swap_recovery.rs:271-398` (`restart_recovers_unsaved_untitled_buffer`) asserts an unsaved `Untitled.md` edit survives SIGKILL and is recovered *via its swap file*. `crates/wolf-core/tests/nvim_restart.rs:162-181` asserts the same buffer is Tier-1 precisely so the reopen `:e` can drive swap recovery. Would require weakening those tests — forbidden.
2. **Would not even fix the bug.** Toggling swap off at naming and back on at first change defers `ml_open_file` to `changed()` — moving the modal to **the user's first keystroke**, the exact symptom.
3. **Eager swap creation at naming time is a feature**: it *claims* the name atomically, closing the race where a sibling nvim grabs the same `Untitled.md`. Say so in the doc so nobody "optimizes" it later.

### R5 — reaping stale swaps: NOT IN THIS CHANGE

A reaper deletes the **only** copy of a user's unsaved edits; a wrong staleness predicate loses data silently. The observed 24-file pile is fully explained by the smoke re-running in a fixed cwd and `kill`ing the app — fixing isolation removes the symptom. With R1-R3 landed, accumulated swaps only bump the Untitled index. The one targeted reap that exists is correct and stays (the `SwapExists` `"r"` branch's `vim.schedule(pcall(vim.fn.delete, swapname))` at `init.lua:69-74`). **Do** record the residual index-creep as a documented known limitation.

### Explicitly rejected quick fixes (do not retry)

- `shortmess+=A` — suppresses the message, not the modal.
- Replacing `nvim_buf_set_name` with `:edit <path>` so `SwapExists` fires — creates a second buffer, breaks the "exactly one listed buffer" `VimEnter` guard, and (with the measured `pid = 0`) takes the `swapchoice = "r"` branch, silently recovering the crashed session's edits into the *startup* buffer, contradicting the deliberate restart-shadow design.
- Relying on Neovim's default `nvim_swapfile` handler — only helps for a **live** owner, behind the same gate.

| | Verdict |
| --- | --- |
| R1 + R2 (name avoidance) | **The real fix.** |
| R3 (all call sites) | **Real fix**, completes R1/R2. |
| `noswapfile` | Papering-over **and** a regression. Reject. |
| Reaping swaps | Orthogonal, data-loss-shaped. Defer. |
| Temp cwd for the smoke | Necessary and non-weakening — **only valid paired with the P3.5 test**. |

## P3.4 Cross-platform — YES

**`runtime/nvim/init.lua` is the single captive init used on BOTH hosts — verified:** Linux/cargo via `CoreConfig.init_lua` / `common::init_lua_path()` (`crates/wolf-core/tests/common/mod.rs:174`); macOS via `crates/wolf-macos/BUILD.bazel:190` putting `//runtime/nvim:init.lua` into the `.app` at `Contents/Resources/init.lua`; Bazel `rust_test` targets stage it as `WOLF_TEST_INIT_LUA`.

The `home_replace` / `~user` bug is **not Linux-specific** — on macOS it is arguably *worse*, because essentially every real user project lives under `/Users/<name>`, so the defence is dead 100% of the time in production, while every test fixture (`/var/folders/…`) sits outside `$HOME` and accidentally works.

**No `wolf-core` public API changes anywhere in this plan.** The new test reaches `swap_dir_for` via the already-`pub mod state` (`crates/wolf-core/src/lib.rs:74`); it is *not* in the `pub use state::{…}` list at `:112-114` and does **not** need adding.

## P3.5 Test isolation for `window_renders_typed_text_under_cage`

**Decision: give it a temp cwd. NECESSARY and non-weakening — but it must land TOGETHER with the deterministic guard in P3.6.1, never alone.** Alone it would be papering-over; paired, it is strictly better coverage.

- **It does not weaken the test.** Assertion, scenario string, timing knobs and failure surface are byte-identical; only *which directory* the app opens changes — and the other eight smokes already do exactly this.
- **It does not hide the bug — it stops the test hiding it.** This smoke never *deliberately* caught the swap regression; it caught it by **self-poisoning**. A test whose fixture is its own previous run's leftovers is order-dependent and non-idempotent — it goes red for whatever change happens to be in flight, which is exactly what happened (Phase 2 spent a whole pass on a refuted GTK hypothesis, partly because the test's own failure message asserts the cause is "input routing / focus").
- **Coverage moves from accidental to deliberate and gets stronger.** P3.6.1 plants the stale swap explicitly, in a cwd chosen so the *real* (`$HOME`) bug reproduces, asserts the specific mechanism, runs headless in seconds, and runs under **both** `cargo test` and `bazelisk test`. The cage smoke never ran on macOS at all.
- **Side benefits:** today the app launches with the repo root as cwd, so `FileSearch` traverses the whole repository (a multi-second, load-dependent traversal that itself perturbs this smoke's timing) and the run writes `.wolf/` into the working tree.

Residual cost, not to fix: `make_temp_cwd()` deliberately does not delete its dirs, so each run leaves one new `state_dir()/swap/<hash>/` holding at most one `.swp`. Harmless — no future run collides with it.

## P3.6 Test plan

### P3.6.1 The startup-swap guard

**Where: modify `crates/wolf-core/tests/swap_recovery.rs`. Do NOT create a new file.** Already wired: `rust_test(name = "swap_recovery")` at `crates/wolf-core/BUILD.bazel:1051-1073`, `srcs = ["tests/common/mod.rs", "tests/swap_recovery.rs"]`, `tags = ["no-sandbox"]`, staged `//bazel/nvim:nvim_tree` + `//runtime/nvim:init.lua`, three wired `WOLF_*` env vars. **No `BUILD.bazel` change.**

#### The fixture MUST live under `$HOME` — load-bearing

The existing three tests use `owned_cwd()` (`swap_recovery.rs:135-139`), a `tempfile::tempdir()`. **That is the exact blind spot:** `home_replace` leaves a `/tmp` or `/private/var/folders` path untouched, so the *buggy* `info.fname == abspath` already matches there, `restart_recovers_unsaved_untitled_buffer` passes, and the bug shipped invisibly. A new test at a temp cwd would be green with **or** without the fix.

Add a sibling helper `fn home_cwd() -> (tempfile::TempDir, PathBuf)` next to `owned_cwd`:
- Read `HOME`; if unset or empty, **panic with a loud explanatory message** (no skip — fail-loud convention).
- `tempfile::Builder::new().prefix("wolf-swap-home-").tempdir_in(&home)`.
- `std::fs::canonicalize` the result; return `(guard, canonical_path)` — same shape as `owned_cwd`.
- **Doc comment must state, in full, why it cannot be `owned_cwd()`**, quoting the measured `fname = "~jplein/Git/jplein/wolf/crates/wolf-gnome/Untitled.md"` and noting `home_replace` (plus Unix's `~/` → `~<user>/` rewrite) fires only under `$HOME`. This comment is what stops a future maintainer "simplifying" the test into uselessness.

#### The test

Name `stale_untitled_swap_does_not_block_a_fresh_start`, placed after `restart_recovers_unsaved_untitled_buffer`.

New const, chosen so no character is `Z` (would `ZZ`-write-and-quit), `q` (starts macro recording) or `:` — `const STARTUP_MARKER: &str = "WOLFSTALESWAP";`

New helper `fn untitled_swap_files(cwd: &Path) -> Vec<PathBuf>`: `wolf_core::state::swap_dir_for(cwd)` (call **after** the first `start_core_in`, so `common::config_in` has already set the process-global `WOLF_STATE_DIR`), `.expect(...)`; read the dir (missing ⇒ empty vec); keep entries whose file name **contains `"Untitled.md"`** and **ends with `".swp"`**. Note `Untitled 2.md.swp` does not contain `Untitled.md`, so this discriminates correctly.

Sequence:
1. `let (_home_guard, cwd) = home_cwd();`
2. `let (core_a, waiter_a) = start_core_in(&cwd);`
3. Poll (`poll_until` + `core_a.open_files_for_tabpage(tab)`, mirroring `:287-297`) until the Tier-1 mirror carries basename `Untitled.md`. **Precondition:** the startup buffer really did take the un-numbered name in a fresh cwd.
4. Type a marker into it (`ch('i')`, `type_str`, `key(Key::Esc)`, `wait_for_flush`), then `ex_command(&core_a, "preserve")` + `sleep(PRESERVE_SETTLE)` so the swap is deterministically on disk.
5. **Assert the fixture exists**: `!untitled_swap_files(&cwd).is_empty()`, message naming the resolved swap dir and its actual contents. Without this the test could silently prove nothing.
6. `self_sigkill(&core_a)` + `poll_until(… Liveness::Dead)`.
7. **Assert the fixture survived the kill**: `!untitled_swap_files(&cwd).is_empty()` again — "a SIGKILLed nvim must leave its swap behind; that stale swap IS this test's fixture".
8. `let _ = core_a.shutdown();` (reaps the child; returns `Ok` for an already-dead core — `api.rs:1340-1350`).
9. `let (core_b, waiter_b) = start_core_in(&cwd);` — a **cold** start in the same cwd, therefore the same durable swap dir. *This is the shape that actually shipped broken; `restart_recovers_unsaved_untitled_buffer` already covers the in-process `restart_nvim` shape.*
10. **Assertion A — the mechanism.** Poll `core_b.open_files_for_tabpage(core_b.current_tabpage())` until it carries a file whose basename is `"Untitled 2.md"`; assert that, and assert it does **NOT** carry `"Untitled.md"`. Message: "`_G.wolf_untitled_swap_exists` did not match the stale swap for `<cwd>/Untitled.md`; the startup naming must advance to `Untitled 2.md`. Under `$HOME` this is the `swapinfo().fname` home-relative `~user` rewrite."
11. **Assertion B — the user-visible symptom (primary).** `core_b.send_input(ch('i'))`, `type_str(&core_b, STARTUP_MARKER)`, `key(Key::Esc)`, then `poll_until` row 0 of `active_grid()` contains `STARTUP_MARKER`, then assert it. Message must state: "the FIRST keystroke was swallowed — nvim was in `MODE_CONFIRM` (`mode() == \"r?\"`), a blocking `do_dialog` ATTENTION prompt raised by `nvim_buf_set_name` finding a stale swap; see `runtime/nvim/init.lua`'s anonymous-buffer naming section." **Cannot be faked:** the leading `i` is consumed answering the dialog, so at most `len-1` characters can be inserted, and `contains(full marker)` requires all of them contiguously.
12. **Assertion C — diagnostic.** `assert!(!snap.toast.is_prompt(), …)`, mirroring the sibling tests. `Toast::is_prompt` is `last().kind == "confirm"` (`render.rs:669-674`), and `do_dialog` under `ext_messages` emits `msg_show` with kind `confirm` — a genuine detector, not decoration.
13. `core_b.shutdown().expect(...)`.

#### Red-then-green proof (coder must perform and report)

Temporarily make `_G.wolf_untitled_swap_exists` `return false` unconditionally, run `cargo test -p wolf-core --test swap_recovery`, confirm the new test **FAILS at Assertion A and/or B**, paste the message, revert. **Reverting only the R1 comparison is NOT sufficient for a red**, because R2's `%`-mangled stat would still catch it — that is defence in depth working as designed, not the test being weak.

#### Module-doc update

Extend `swap_recovery.rs`'s header (lines 1-38): add a fourth bullet for the startup-`Untitled.md` case, and a short "why `$HOME`" note under the existing "How the swap dir is controlled (read before trusting green)" section.

#### No regressions expected

Every existing real-nvim test uses `common::fresh_cwd()` or `owned_cwd()` — a temp dir with an empty, freshly-hashed swap dir — so `wolf_untitled_swap_exists` returns `false` and the name stays `Untitled.md`.

### P3.6.2 The gnome smoke

Modify `crates/wolf-gnome/tests/smoke_headless.rs`:
1. In `window_renders_typed_text_under_cage` (`:1235`): add `let cwd = make_temp_cwd();` and `.current_dir(&cwd)` to the `Command::new("cage")` builder at `:1264-1275`. Nothing else changes.
2. Rewrite `make_temp_cwd`'s doc comment (`:1217-1222`): it currently frames itself as the picker's helper. State that **all nine** smokes use it, and that isolation is mandatory because the app's captive nvim keeps a **durable per-project swap dir keyed on the cwd** (`swap_dir_for`) while the scenario `kill`s the app — so a fixed cwd accumulates one stale swap per run and each run's leftovers become the next run's fixture. Cross-reference `crates/wolf-core/tests/swap_recovery.rs::stale_untitled_swap_does_not_block_a_fresh_start`.
3. **Rewrite the failure message at `:1301-1312`.** Its "(b)" branch attributes an empty row 0 to "input routing / focus" — now known WRONG, and it cost an entire diagnostic pass. New (b) must name the swap modal **first** (empty row 0 + cursor two rows down ⇒ the first key was spent on a `do_dialog` ATTENTION prompt; check `state_dir()/swap/<hash>/` for a `%…%Untitled.md.swp`) and only then mention focus/input routing as secondary.

### P3.6.3 Mirror `wrong_schema_db_is_silently_recreated` into `tests/mru.rs`

**Sibling to copy: `crates/wolf-core/tests/command_mru.rs:200-229`** (`recent_dirs.rs` is identical). Keep the **same test name** so it is greppable across all three stores.

Adaptations for the `file-mru` schema:
- Table is **`mru`**, not `command_mru`. The probe is `SELECT directory, file, opened_at FROM mru LIMIT 0` (`crates/wolf-core/src/mru.rs:294`).
- The fixture must survive `CREATE TABLE IF NOT EXISTS mru (…)` yet fail that probe. Use the **historically real** wrong schema documented at `mru.rs:273-275`: a raw `rusqlite::Connection` executing `CREATE TABLE mru (path TEXT PRIMARY KEY);` — missing all three probed columns.
- Use the existing `MRU_DB_NAME` const (`tests/mru.rs:75`), not a local `DB_NAME`.
- `MruStore`'s API is directory-scoped and `recent()` prunes files not on disk, so use the existing `state_tempdir()`, `canon_tempdir()`, `touch()` and `ends_with()` helpers exactly as `a_corrupt_database_is_still_healed` (`tests/mru.rs:555-585`) does. Post-heal file: `dir_a.join("healed.md")`, `touch`ed.
- Assertions in order: `MruStore::open` does not panic → `store.recent(&dir_a).is_empty()` right after the heal → `record` + `recent().len() == 1` + `ends_with(path, "healed.md")` → `db.exists()`.
- **Doc comment must state why it exists**: `a_corrupt_database_is_still_healed` goes through `SQLITE_NOTADB`, which rusqlite surfaces as `Error::SqliteFailure`; the schema probe's `prepare` raises `Error::SqlInputError` instead (rusqlite converts an `ErrorCode::Unknown` carrying a valid `sqlite3_error_offset` into `SqlInputError`, `rusqlite-0.31.0/src/error.rs:430-448`). Only the second shape catches the classifier over-narrowing pass 2 found in `command_mru` — the identical bug in `mru.rs` would have shipped undetected.

Also update the module doc at `tests/mru.rs:44-62` ("the last three tests" / "the guards come in a matched pair") for the new count and pair.

### P3.6.4 The deterministic B2 red-then-green guard

**`rusqlite` availability — DECIDED: it is NOT in `[dev-dependencies]` but does NOT need to be.** Cargo makes ordinary `[dependencies]` available to *every* target including integration tests. **Proof in this repo:** `crates/wolf-core/tests/command_mru.rs:210` and `tests/recent_dirs.rs` already call `rusqlite::Connection::open` and both compile and pass; both Bazel targets use `deps = [":wolf_core"] + all_crate_deps(normal = True, normal_dev = True)` and `rusqlite` is in the `normal` map. **No `Cargo.toml` change, no `Cargo.lock` change, no `CARGO_BAZEL_REPIN`, no `macos-followups.md` row on that account. It is free — do it.**

**Where:** `crates/wolf-core/tests/mru.rs`, immediately after `concurrent_opens_preserve_rows`. **Name:** `an_open_that_loses_the_lock_race_preserves_rows`.

**The subtlety that makes or breaks it.** A naive `BEGIN EXCLUSIVE` against a **WAL** database does nothing useful: in WAL mode SQLite treats `EXCLUSIVE` as `IMMEDIATE`, readers are not blocked, and a no-op `CREATE TABLE IF NOT EXISTS` takes no write lock — so `init()` would simply succeed and the test would be green with or without the fix. The pass-2 measurement is explicit that the destructive path is reached via the **rollback-journal → WAL conversion**, which needs an exclusive lock and does **not** run the busy handler. **So the fixture must present a rollback-journal-mode database.**

New const `const LOCKED_FILES: usize = 12;` (comfortably under `MRU_LIMIT`).

1. `let state = state_tempdir(); let (_dir, dir_a) = canon_tempdir();` Create and `touch` `LOCKED_FILES` paths under `dir_a`.
2. In an inner scope: `let store = MruStore::open(state.path());` record all of them; assert `store.recent(&dir_a).len() == LOCKED_FILES` as a precondition; **drop the store** so no Wolf-owned handle is live. (Rows written entirely through the public API — the test never hand-rolls the schema, the `directory` normalization or the `opened_at` units.)
3. Convert the file **back** to rollback-journal mode with a raw connection: `rusqlite::Connection::open(&db)` then `execute_batch("PRAGMA journal_mode=DELETE;")`, then drop it. Comment must explain *why* (the WAL-mode `BEGIN EXCLUSIVE` no-op above).
4. `let blocker = rusqlite::Connection::open(&db).expect(…); blocker.execute_batch("BEGIN EXCLUSIVE").expect(…);` — in rollback mode this blocks readers **and** writers.
5. `let contended = MruStore::open(state.path());` while the lock is held.
6. **Assert immediately**: `db.exists()`, message spelling out that the pre-fix code deleted `file-mru.sqlite` plus its `-wal`/`-shm` sidecars on a transient `SQLITE_BUSY`, and that in production that is one Wolf window wiping another's recent-files history.
7. `blocker.execute_batch("ROLLBACK").expect(…); drop(blocker);`
8. Assert `contended.recent(&dir_a).len() == LOCKED_FILES` — proves the handle self-heals by lazily re-opening (`with_conn` re-opens when the guard holds `None`, `mru.rs:212-219`).
9. Assert a **fresh** `MruStore::open(state.path()).recent(&dir_a).len() == LOCKED_FILES` — proves the on-disk file survived, not just a cached handle.

**Why it is a genuine red-then-green:** pre-fix, `warrants_recreate` matched everything, so the contended `init()`'s `SQLITE_BUSY` triggered `remove_db_files`, and the retry `init()` succeeded (the blocker's lock is on the now-unlinked inode) leaving an **empty** database — steps 8 and 9 both read 0 rows. Post-fix, `warrants_recreate(DatabaseBusy) == false`, the handle backs off with `conn = None`, and the file is untouched.

**Coder must verify red-capability by mutation:** temporarily make `warrants_recreate` in `crates/wolf-core/src/mru.rs` return `true` unconditionally, confirm this test fails, paste the message, revert.

**Runtime note (not a flake):** post-fix, `MRU_BUSY_TIMEOUT_MS = 2000` (`mru.rs:63`) is installed *before* `journal_mode`. Pass 2 measured that SQLite does not run the busy handler for the WAL-conversion lock, so the open fails immediately; if it ever does engage, worst case is a 2 s test, well inside `size = "small"`. Say so in the doc comment so nobody "fixes" it later.

### P3.6.5 Bazel confirmation — NEITHER file needs a `BUILD.bazel` change

| File | Target | Location | Why no change |
| --- | --- | --- | --- |
| `crates/wolf-core/tests/mru.rs` | `rust_test(name = "mru")` | `BUILD.bazel:1139-1146` | `srcs = ["tests/mru.rs"]` already lists it; new tests are added *inside* it; `all_crate_deps(normal = True, normal_dev = True)` already supplies `rusqlite`. Headless — no `data`/`env`/`no-sandbox` needed and none should be added. |
| `crates/wolf-core/tests/swap_recovery.rs` | `rust_test(name = "swap_recovery")` | `BUILD.bazel:1051-1073` | `srcs` already lists it; staged nvim tree + `init.lua`, three `WOLF_*` env vars and `tags = ["no-sandbox"]` all already present. |

`crates/wolf-gnome` has no `BUILD.bazel`, so P3.6.2 needs no wiring.

## P3.7 Files to modify (none to create)

1. **`runtime/nvim/init.lua`** — add file-local `swap_fname_matches` and `untitled_swap_file` above `_G.wolf_untitled_swap_exists` (~:1607) (keep them local — no test can call them, so they are not new documented surface); rewrite `_G.wolf_untitled_swap_exists` (R1+R2); default `extra_collision` in `_G.wolf_name_untitled_current` (`:1707`) and `_G.wolf_new_untitled` (`:1693`) (R3). **Correct the FALSE doc comments:**
   - `:1598-1606` — "matches on `swapinfo().fname` — the SAME identity nvim's own `SwapExists` machinery uses" is misleading. Neovim's machinery matches on the swap **file name** derived from `b_ffname` (`%`-mangled because `'directory'` ends in `//`), not on `b0_fname`; and `b0_fname` is `home_replace`d with a `~<user>` prefix. State both criteria and why each exists.
   - `:1739-1746` — "**the fresh nvim's startup buffer is named WITHOUT loading it (`nvim_buf_set_name` skips the swap check)**" is **FALSE and IS the bug**. Replace with the P3.2 mechanism, and note the `pcall` gives no protection.
   - `:38-43` (the `WolfSwap` policy comment) — add the LIMIT: `SwapExists` fires only on the file-load path (`:edit` / `do_ecmd` / `open_buffer` / the Tier-1 reopen). It does **not** cover `nvim_buf_set_name` or `changed()`'s first-modification `ml_open_file`; those are covered by name avoidance.
   - Add a note that eager swap creation at name time is **deliberate** (claims the name atomically) and that `noswapfile` on this buffer is forbidden because `swap_recovery.rs::restart_recovers_unsaved_untitled_buffer` and `nvim_restart.rs` pin the recovery promise.
   - Style: **tab** indentation, `--` comments. No Lua linter/formatter exists in this repo (no `.stylua.toml`, no `luacheck`, nothing in `tools/lint`).
2. **`runtime/nvim/README.md`** — line 18 (`SwapExists` policy bullet): add the "only on the file-load path" limit. Line 221: replace "the fresh nvim names its startup buffer WITHOUT loading it (so `nvim_buf_set_name` never triggers the swap check)" with the true mechanism; describe the two-criteria check; state it now applies to **all** naming call sites and why that is a no-op in-session. Lines 211/213/215/217/219: update the `extra_collision` "nil for Cmd-N / Chdir" statements, which R3 invalidates. Add a **Known limitation** paragraph on residual index-creep and why no reaper ships.
3. **`crates/wolf-core/tests/swap_recovery.rs`** — P3.6.1.
4. **`crates/wolf-core/tests/mru.rs`** — P3.6.3 + P3.6.4 + module doc.
5. **`crates/wolf-gnome/tests/smoke_headless.rs`** — P3.6.2.
6. **`crates/wolf-gnome/tests/README.md`** — line 90 and the "(real compositor)" section: state that **every** smoke launches in a fresh temp cwd, and why.
7. **`macos-followups.md`** — P3.8.

## P3.8 `macos-followups.md` rows to append (date `2026-08-01`, branch `linux-test-fixes`)

1. **`runtime/nvim/init.lua`: `_G.wolf_untitled_swap_exists` fixed + applied to all naming call sites.** Why: `init.lua` is the SHARED captive config bundled into the `.app` at `Contents/Resources/init.lua` (`crates/wolf-macos/BUILD.bazel:190`). The bug is a `home_replace` `~<user>` rewrite, so **macOS is affected identically and in practice more often** — every project under `/Users/<name>` had the defence dead, while every test fixture (`/var/folders/…`) sat outside `$HOME` and accidentally passed. Symptom: after any crash/force-quit with an unsaved `Untitled.md`, the next launch raises a blocking ATTENTION modal that silently eats the first keystroke. macOS needs `bazel test //...` plus a **manual spot check** — open a project under `$HOME`, type into `Untitled.md`, force-quit, relaunch, confirm no modal and the first keystroke lands. Status `Open`.
2. **`crates/wolf-core/tests/swap_recovery.rs` gained `stale_untitled_swap_does_not_block_a_fresh_start`.** Why: no `BUILD.bazel` change, but the test **requires a writable `$HOME`** for its `tempdir_in($HOME)` fixture and runs `no-sandbox`; confirm it passes on the macOS host. Status `Open`.
3. **`crates/wolf-core/tests/mru.rs` gained `wrong_schema_db_is_silently_recreated` and `an_open_that_loses_the_lock_race_preserves_rows`.** Why: no `BUILD.bazel` and no dependency change (`rusqlite` is already a `normal` dep; **no `CARGO_BAZEL_REPIN`**). The second is the deterministic red-then-green guard for the pass-2 `warrants_recreate` data-loss fix that ships on both front-ends. Status `Open`.
4. **`crates/wolf-gnome/tests/smoke_headless.rs`: `window_renders_typed_text_under_cage` now launches in a temp cwd.** Why: **no macOS impact** — `wolf-gnome` has no Bazel target. Logged for completeness. Status `N/A`.
5. **`runtime/nvim/README.md` corrections.** Documentation-only, but it documents the shared captive init and two claims were false. Status `N/A` (or fold into row 1).

## P3.9 Implementation order

1. **`runtime/nvim/init.lua`** — R1, R2, R3 + all comment corrections. Pure Lua; nothing compiles against it, and everything downstream depends on it being right.
2. **`runtime/nvim/README.md`** + `prettier --write`.
3. **`crates/wolf-core/tests/swap_recovery.rs`** — new helper, const and test. `cargo test -p wolf-core --test swap_recovery` (expect 4/4).
4. **Red-proof for step 3**: mutate `_G.wolf_untitled_swap_exists` to `return false`, re-run, capture the failure, revert, re-run green.
5. **`crates/wolf-core/tests/mru.rs`** — two new tests + module doc. `cargo test -p wolf-core --test mru` (expect 13/13).
6. **Red-proof for step 5**: mutate `warrants_recreate` in `crates/wolf-core/src/mru.rs` to `true`, confirm `an_open_that_loses_the_lock_race_preserves_rows` fails, revert.
7. **`crates/wolf-gnome/tests/smoke_headless.rs`** — temp cwd + `make_temp_cwd` doc + failure-message rewrite.
8. **`crates/wolf-gnome/tests/README.md`** + `prettier --write`.
9. **`macos-followups.md`** + `prettier --write`.
10. **IDEMPOTENCY PROOF (the acceptance criterion for OPEN ITEM D).** Wipe `~/.local/state/wolf/swap/` **once**, then run `nix develop --command cargo test -p wolf-gnome --test smoke_headless` **twice back-to-back with no wipe in between**; both runs must report `9 passed; 0 failed; 0 ignored`. **A single green run is not acceptance.**
11. **Full gate**, all five steps, Bash timeouts >= 30 min on clippy/build/test.

## P3.10 Dependencies

**None.** No new crates, no `Cargo.toml` edit, no `Cargo.lock` change, no `MODULE.bazel` change, no `CARGO_BAZEL_REPIN`, no `BUILD.bazel` change.

## P3.11 Lint considerations

- **No `#[allow(...)]` should be needed.** If one becomes necessary it must carry `reason = "…"`.
- `cargo clippy --workspace --all-targets -- -D warnings` lints `tests/*.rs`. In the new test code: use **inline format args** (`{cwd:?}`, `{dir}`); bind tempdir guards as `let (_home_guard, cwd) = home_cwd();`; `let _ = core_a.shutdown();` is the accepted way to discard the `Result` on a deliberately-dead core; `.expect("…")` each `rusqlite` call; `drop(blocker)` after `ROLLBACK` is intentional and clippy-clean; take `&Path` not `&PathBuf` (`clippy::ptr_arg`).
- **Lua**: no linter or formatter in this repo. Match `init.lua`'s style: **tab** indentation, `--` comments.
- **Markdown**: `prettier --write` every `.md` touched; no hard line breaks inside a paragraph.

---

## P3 test-writer pass — DONE (caveats ACCEPTED by the orchestrator; coder must honor them)

Files modified: `crates/wolf-core/tests/swap_recovery.rs`, `crates/wolf-gnome/tests/smoke_headless.rs`, `crates/wolf-core/tests/mru.rs`. No `Cargo.toml` / `Cargo.lock` / `BUILD.bazel` change. `rusqlite` reached via full paths as `command_mru.rs:210` already does.

**Improvement on the plan (accepted):** `STARTUP_MARKER = "WOLFSTALESWAP"` also avoids `Q` and **`D`** — the ATTENTION dialog's "(Q)uit" and "(D)elete it" hotkeys; `D` would delete the test's own fixture.

**Extra helpers added beyond the plan (accepted):** `swap_dir_of`, `swap_dir_report` (steps 5/7 require messages naming the resolved swap dir and its contents), `mirror_basenames`.

### Caveats the coder MUST honor

1. **§P3.6.4 step 6 (`assert!(db.exists())`) is NOT red-capable.** Pre-fix, `open_or_heal` calls `remove_db_files` then immediately re-runs `init`; `Connection::open` re-creates the file at a **new inode** (the blocker's EXCLUSIVE lock is stranded on the unlinked one), so the retry succeeds and `db.exists()` is `true` both pre- and post-fix. **The genuinely red assertions are steps 8 and 9** (row counts read 0 pre-fix). Under the mutation proof, expect step 6 to PASS and only 8/9 to fail — that is NOT evidence the mutation failed to take.
2. **§P3.6.4's "fails immediately" runtime claim is too optimistic (not a correctness problem).** `PRAGMA journal_mode=WAL` on a DELETE-mode DB reaches `sqlite3BtreeSetVersion`, whose first action is `sqlite3BtreeBeginTrans(pBtree, 0, 0)` — an ordinary read transaction, and that path **does** invoke the busy handler. Realistic cost is up to `MRU_BUSY_TIMEOUT_MS` = 2 s. Bounded, not flaky. The doc comment says "worst case is one 2 s wait" so nobody later reads a 2 s test as a bug.
3. **§P3.6.3 is a regression guard, NOT a red-then-green.** `warrants_recreate` (`crates/wolf-core/src/mru.rs:328-332`) **already** matches `rusqlite::Error::SqlInputError` — that landed in pass 2. `wrong_schema_db_is_silently_recreated` should be green on its first run. **Do not hunt for a red here.**
4. **§P3.6.1's "exactly the leading `i` was swallowed" is an approximation.** The ATTENTION dialog's hotkeys are O/E/R/Q/A/D; `i` is not one, so `do_dialog` re-prompts. With `STARTUP_MARKER = "WOLFSTALESWAP"` the consumed sequence is `i` (invalid), `W` (invalid), then **`O` → "Open Read-Only"** — more than one key is eaten and the buffer opens read-only. The test is still solidly red (row 0 cannot hold the contiguous marker) and no destructive answer (`Q`, `D`) appears in the marker, but the message's "at most len-1 characters" reasoning is a lower bound, not exact.
5. **§P3.6.1 Assertion C is effectively unreachable in the red case** — the `poll_until` before Assertion B panics at `FLUSH_TIMEOUT` first. C is diagnostic and carries no red-proof weight. The `poll_until` descriptions for steps 10/11 were loaded with the diagnosis, since the timeout panic is what a failing run actually prints.
6. **`home_cwd()` writes into the developer's real `$HOME` under cargo.** `TempDir::drop` cleans up on normal exit and on unwind, but a SIGKILLed test binary leaks one `wolf-swap-home-*` dir in `$HOME`. Under Bazel `HOME` is a sandbox dir. Cargo-only cosmetic risk; the fixture location is the point.
7. **Left for the coder:** `crates/wolf-gnome/tests/README.md` (§P3.7 item 6) — needs `prettier --write`, which the test-writer cannot run.
8. **rustfmt:** the test-writer could not run `cargo fmt`. Hand-checked against clean neighbours, but `cargo fmt --all` may still adjust a line or two.

---

## P3 coder pass — the product bug is FIXED; one OCR-fixture item remains

**OPEN ITEM D is RESOLVED.** R1+R2+R3 landed in `runtime/nvim/init.lua`. Mutation proof (a) — `_G.wolf_untitled_swap_exists` → `return false` — went honestly RED, and *earlier* than caveat 5 predicted (core B never emits a first flush at all, because nvim is stuck in the ATTENTION dialog before the window lands):
```text
test stale_untitled_swap_does_not_block_a_fresh_start ... FAILED
test restart_recovers_unsaved_untitled_buffer ... FAILED
  timed out after 10s waiting for: B's initial editor window after attach
test result: FAILED. 2 passed; 2 failed; 0 ignored
```
Mutation proof (b) — `warrants_recreate` → `true` — went RED exactly as caveat 1 predicted (step 6 `db.exists()` PASSED; step 8 red at 0 vs 12). Both reverted and re-run green.

Suites: `swap_recovery` 4/4, `mru` 13/13, `wolf-ffi --test untitled` 7/7 (R3 is a no-op in a fresh cwd), `nvim_restart` 2/2. Gate steps 1-4 all OK.

### The IDEMPOTENCY half of the acceptance criterion IS met

Two back-to-back runs with no wipe between are byte-identical (`8 passed; 1 failed` both times), and no swap accumulates in any shared project dir — 18 per-cwd dirs, 9 per run, every hash unique to its run, the two `Untitled.md.swp` files in `73f41811fff50c83/` and `93ecae17d8f7462e/`, never colliding.

## OPEN ITEM E — `window_renders_typed_text_under_cage` fails on an OCR classifier artifact, NOT a product defect

Failure is `OCR output was:\nwolf\n\nﬁello` — U+FB01 `ﬁ` + `ello`. The PNG shows window, sidebar, editor pane, tildes and cursor all drawn, **row 0 reads `hello` and the cursor sits exactly ONE row below it** — the leading `i` was NOT eaten. An ASCII pixel dump confirms `h e l l o` rendered with correct ascenders and no clipping.

Isolated by running the scenario by hand against different cwds:

| cwd (sidebar contents) | OCR of row 0 |
| --- | --- |
| repo root (~35 entries) | `hello` PASS |
| temp dir, 12 files | `hello` PASS |
| temp dir, 5 files (3 separate runs) | `hello` PASS x3 |
| temp dir, 3 files | `ﬁello` FAIL |
| temp dir, 1 file | `ﬁello` FAIL |
| temp dir, empty (what the test now does) | `ﬁello` FAIL |

**Decisive control:** cropping **the repo-root PNG** down to just the editor region also yields `ﬁello`, and the crop with the pane border removed yields `nello`. So the `hello` pixels are byte-identical in the pass and fail cases — what changes is how much text is elsewhere on the page for tesseract's classifier. `--psm 3/4/6/11/12` all fail on the sparse page.

**The assertion has been silently depending on the repo's file listing populating the sidebar**, which §P3.6.2's temp cwd removes. Evidence PNGs: `scratchpad/{FAILING_ocr_empty_sidebar,repo_cwd,temp_n5,crop_repo}.png`.

**ORCHESTRATOR DECISION — this is test-fixture work, dispatched to the test-writer.** Write ~8-10 tiny marker `.md` files into the temp cwd after `let cwd = make_temp_cwd();` (measured floor is 5; 8-10 gives margin), mirroring what the other eight smokes already do incidentally by dropping fixture files. **This weakens nothing** — the assertion condition, the scenario string, the OCR invocation and the timing knobs are all untouched; it restores the page density the assertion always implicitly relied on, and makes that reliance explicit and deliberate instead of accidental.

## OPEN ITEM F — unrelated observation, NOT acted on

The devShell `shellHook` sets `FONTCONFIG_FILE` from a **cwd-relative** `crates/wolf-gnome/testdata/fonts.conf`, so `nix develop` invoked from anywhere but the repo root prints a `sed: can't read …` error and leaves font rendering unpinned. Harmless for the documented gate (always run from the repo root), but it would make an OCR smoke non-deterministic for anyone running from a subdirectory. Logged, not fixed.
