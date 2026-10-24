# Plan: one nixpkgs input for the whole flake

## Status

DONE (uncommitted). Reviewer approved; review fixes applied (build.md split, "back" dropped, wolf-ffi common/mod.rs comment fixed, measured claims labelled "the Linux flake's nvim 0.12.6"); fmt/clippy/build/doc/prettier/markdown_check green. No test-writer pass (no new/changed test warranted; see Test plan).

Left out of scope (pre-existing): `bazel/nvim-treesitter/README.md:9`, `treesitter-decision-report.md:38–39`, `crates/wolf-ffi/tests/README.md:27`, `architecture.md:87`, "pinned nvim 0.12.3" labels in wolf-core/wolf-ffi tests that also run on Linux.

## Request (from the user)

"The flake.nix in this repo has two different pinned commits for nixpkgs. Let's use a single nixpkgs commit, set to nixos-unstable. `nix flake update` will be the mechanism to update, and it will update all at the same time."

This deliberately reverses the `nixpkgs-runtime` design. The user's decision replaces the old rationale; do not argue for keeping a second input.

## Measured facts (`nix eval` against each locked rev)

| | `nixpkgs` lock `55bfb4d7…` (kept) | `nixpkgs-runtime` `9ae611a4…` (removed) |
|---|---|---|
| neovim | 0.12.6 | 0.12.2 |
| ripgrep | 15.2.0 | 15.1.0 |
| nvim-treesitter src.rev | `e289100ff98969e118c702199d88b764ce9e7fdf` | `4916d6592ede8c07973490d9322f187e07dfefac` |

macOS (Bazel) pins Neovim 0.12.3 and nvim-treesitter `4916d659…`. Bazel pins are OUT OF SCOPE (Linux host can't verify); the divergence is recorded in `macos-followups.md`.

Out of scope (pre-existing, unrelated): `bazel/nvim-treesitter/README.md:9` "Linux/Nix vendoring path is deferred", `treesitter-decision-report.md:38–39` "still deferred".

## flake.nix

1. Lines 36–63: delete the `nixpkgs-runtime` comment block and `.url` line. Short comment above `nixpkgs.url`: one input supplies toolchain, GTK, and the runtime assets the package ships (Neovim, ripgrep, nvim-treesitter); `flake.lock` pins it and `nix flake update` moves all together; one input is deliberate — don't split runtime assets into their own input; an update that moves Neovim owes the full verification gate (`build.md`) because `runtime/nvim/init.lua` and test comments state measured claims per Neovim version; macOS pins its own Neovim/nvim-treesitter under `bazel/`, divergence in `macos-followups.md`. Keep the Mesa/glibc paragraph (45–52) but drop its last sentence ("Neovim runs in a process of its own … stay pinned here").
2. Line 75: remove `nixpkgs-runtime,` from outputs args.
3. Lines 88–92: delete `pkgsRuntime` comment + binding.
4. NOTE 94–97: "`pkgsRuntime` and everything taken from it (`wolfNeovim`, `wolfRipgrep`)" → "`wolfNeovim`, `wolfRipgrep`".
5. 204–209: `wolfNeovim = pkgs.neovim; wolfRipgrep = pkgs.ripgrep;`. Comment: devShell export and package wrapper both read these bindings, so tested and shipped binary are one store path by construction.
6. 446–450: delete the "The plugin comes from `pkgsRuntime`…" paragraph.
7. 456: `pkgs.vimPlugins.nvim-treesitter.parsers.${lang}`.
8. 467: keep warning, retarget to `${pkgs.vimPlugins.nvim-treesitter}/runtime`.
9. 479–484: rewrite — queries unfiltered on purpose, matching macOS package which globs them unfiltered; parsers curated because each is a compile. Add: the revision is nixpkgs', independent of `bazel/nvim-treesitter/extensions.bzl`'s, so the platforms' queries can differ (see `macos-followups.md`). Remove `4916d65…` literal and "byte-identical".
10. 486–488: delete ("That parity claim is about a REVISION…").
11. 490: `{ src = pkgs.vimPlugins.nvim-treesitter; }`.
12. 1040: "the NOTE beside `pkgsRuntime` above" → "the NOTE above `rustToolchain`".
13. Header 1–31: no change.

Afterwards `grep -n 'pkgsRuntime\|nixpkgs-runtime\|4916d65' flake.nix` prints nothing.

## flake.lock

`nix flake lock` (NOT `nix flake update`). Verify diff removes only the `nixpkgs-runtime` node and its `root.inputs` line; `nix flake metadata --json | jq -r .locks.nodes.nixpkgs.locked.rev` = `55bfb4d7437bd4e09ab7cba58a43bd68eb310044` before and after.

## Docs and comments

(a) reword; (b) re-measure on 0.12.6 first (probes below); (c) leave.

| Location | Class | Change |
|---|---|---|
| `build.md:136` last sentence | (a) | Replace the `nixpkgs-runtime` sentence: `nix flake update nixpkgs` moves Neovim, ripgrep, nvim-treesitter too, so run the verification gate after it; and a consumer's `follows` line makes the package take all three from the host's nixpkgs, not the revision this repo tested. |
| `build.md:31` "66 paths fetched, 22.2 MiB" | (b) | Probe M7; on failure cut the figures, keep the qualitative claim. |
| `feat-list.md:27` | (a) | "Neovim held in a second, rev-locked flake input…" → both routes read one Neovim from the flake's single `nixpkgs` input, so `nix flake update` moves tested and shipped Neovim together. |
| `feat-list.md:182`, `runtime/nvim/README.md:324`, `crates/wolf-core/src/nvim.rs:1569`, `smoke_headless.rs:~25304` ("seven parsers") | (b) | Confirm with M2; no text change if it matches. |
| `runtime/nvim/init.lua:279`, `:369` (0.12.2) | (b) | M1, then relabel 0.12.6. |
| `runtime/nvim/init.lua` 0.12.3 mentions | (c) | macOS pin. |
| `crates/wolf-core/src/redraw.rs:226`, `:2181` (0.12.2) | (b) | M3, relabel. `///` prose only — no indented/fenced lines (doctests). |
| `crates/wolf-core/tests/nvim_restart.rs:292`, pid example ~297 | (b) | M3, relabel, replace pid pair with observed pair. |
| `crates/wolf-core/src/markdown.rs` 0.12.2 | (c) | pulldown-cmark version. |
| `smoke_headless.rs:2464, 5251, 12422, 19824, 19986, 20043` | (b) | M4, relabel; keep `\` continuations in assert strings. |
| `smoke_headless.rs:5055` | (b) | M5, relabel. |
| `smoke_headless.rs:6893` (+ samples 6898–6899) | (b) | M6, relabel/update samples. |
| `smoke_headless.rs:~38064–38067` falsifier recipe | (a) | Point the package wrapper's `WOLF_NVIM_BIN` default (flake.nix ~1014) at a Neovim build other than `wolfNeovim` (not `neovim-unwrapped` — same binary). |
| `smoke_headless.rs:~38266–38267` assert message | (a) | → "The named cause is the package wrapper's `WOLF_NVIM_BIN` default and `linuxShell`'s export not both reading `wolfNeovim`." |
| `crates/wolf-gnome/tests/README.md:365` | (a) | Red run needs the wrapper pointed at a Neovim build other than `wolfNeovim`; none was taken. Keep remaining sentences. |
| `crates/wolf-gnome/tests/README.md:366` | (a) | "**What that equality does not say is that both platforms run one Neovim.** Linux takes Neovim from the flake's `nixpkgs`, which `nix flake update` moves, while Bazel stages its own pin; [`macos-followups.md`](…) carries the row." Net-negative. |
| `bazel/treesitter-parsers/README.md:11` | (b) | M2, "against Neovim 0.12.6 from the Linux flake". |
| `bazel/treesitter-parsers/README.md:13` | (a) | No stale Linux version: Bazel pins 0.12.3, flake takes `pkgs.neovim` from nixos-unstable via `flake.lock`, they move independently. Keep "recorded rather than fixed…" tail. |
| `bazel/treesitter-parsers/extensions.bzl:14–16` | (a)+(b) | Docstring only → "Neovim 0.12.6 from the Linux flake". Moves `bzlTransitiveDigest`; record in the new ledger row. |
| `bazel/nvim-treesitter/README.md`, `bazel/jetbrains-mono/README.md` | (c) | |
| `macos-followups.md` row 63 | (a) | See below. |
| `macos-followups.md` rows 33, 57, 61 | (c) | Dated measurement records. |

### macos-followups.md

1. Row 63 item (a): keep bold lead; replace rest with `**Amended 2026-10-24 (documentation-checks-and-updates):**` Linux takes Neovim from the flake's single `nixpkgs` input; the 2026-10-24 row owns the divergence. Delete item (c)'s last sentence ("…against 0.12.2 while Bazel stages 0.12.3…").
2. New row 2026-10-24, `documentation-checks-and-updates`, Status Open. What changed: drops `nixpkgs-runtime`; versions at this lock (0.12.6 / 15.2.0 / `e289100…`); `flake.lock` loses the node, `nixpkgs` stays `55bfb4d7…`; comment edits incl. `bazel/treesitter-parsers/extensions.bzl` docstring. Why macOS re-verification: (1) Neovim Linux 0.12.6 vs Bazel 0.12.3 and Linux moves each `nix flake update` — decide whether `bazel/nvim` tracks the lock; (2) nvim-treesitter `e289100…` vs `4916d65…` — decide whether to bump `bazel/nvim-treesitter` with `bazel/treesitter-parsers`; (3) docstring edit moves `bzlTransitiveDigest` — first macOS build rewrites `MODULE.bazel.lock`, commit it, run `bazel test //...`; (4) Darwin drvPath before/after + `nix flake check --all-systems --no-build` (evaluation evidence only). No changelog words.

## Re-measurement (inside `nix develop`, `$WOLF_NVIM_BIN` = 0.12.6)

**Stop rule:** if a probe disagrees with the claim it backs, stop and report — don't edit the comment.

- M1: `"$WOLF_NVIM_BIN" --clean -u NONE --headless <tmp.md containing "# h"> -c 'lua io.stdout:write(tostring(vim.treesitter.highlighter.active[vim.api.nvim_get_current_buf()] ~= nil).."\n")' -c 'qa!'` → `true`.
- M2: list `parser/*.so` with highlights queries under `--clean` → `c lua markdown markdown_inline query vim vimdoc`.
- M3: `term://` replay via `:edit fnameescape(name)` spawns a second shell: names differ only in `//<pid>:`, children `1->2`, empty errmsg.
- M4/M5: `nvim_input_mouse` probes (paced drag → `v`, marks `3 4 8 13`; click → empty; rapid → empty, cursor at release; paced → `v`). Fallback: temporarily edit `POINTER_DRAG_BODY` and run `drag_selects_a_range_under_cage`, revert.
- M6: temporarily emit `nvim_win_get_width` in `DIVIDER_RELAUNCH_SCENARIO` read_widths, run `dragged_divider_weights_survive_relaunch_under_cage`, confirm `winwidth` changes and `nvim_win_get_width` doesn't; revert.
- M7: before first `nix develop`, dry-run the two treesitter drvs into an empty scratch store to read fetched count / MiB / will-be-built. On failure use fallback.

## Test plan

No new or changed tests. `vendored_pins.rs` reads only `wolfVendoredPins`; `smoke_headless.rs` `linux_shell_hook_exports` reads only `shellHook` exports; nothing reads `flake.lock`. Existing real-Neovim suites exercise the new versions. Cheap optional falsifier: drop `"go"` from `wolfTreesitterLanguages` → `treesitter_highlight.rs` reddens; revert.

## Implementation order

1. Baseline: Darwin drvPath (`nix eval --raw .#devShells.aarch64-darwin.default.drvPath`), nixpkgs locked rev.
2. Edit flake.nix. 3. `nix flake lock`, verify. 4. M7. 5. `nix develop --command true`; versions: nvim `v0.12.6`, rg `15.2.0`, treesitter rev `e289100…`.
6. M1–M6 (stop rule). 7. Comment/doc edits. 8. macos-followups.md.
9. `prettier --write` edited `.md`; `./tools/lint/markdown_check.sh` — no new findings vs `git show HEAD:<file>` copies (never `git stash`).
10. Verification:
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
    plus `nix flake check --all-systems --no-build`, `nix flake show --all-systems` (no `packages.*-darwin`), Darwin drvPath unchanged, `nix build .#wolf-gnome --no-link --print-out-paths`, `./tools/wolf-package-smoke.sh` (never concurrently with another cargo test), `git grep -n 'nixpkgs-runtime\|pkgsRuntime'` hits only macos-followups.md rows.

## Lint

Comments + string literals only in Rust. Markdown checker flags "now", "no longer", "previously", "the old", ALL-CAPS outside backticks.

## Results

Coder pass 1 — DONE, all green (uncommitted, branch `feat/single-nixpkgs-pin`; ledger row uses that name).

- Darwin drvPath unchanged `/nix/store/aq3d22bxfa3cyfg558d9br8hg319jvvp-nix-shell.drv`; nixpkgs rev unchanged `55bfb4d7…`; flake.lock −17 lines (node + root input only).
- Versions: NVIM v0.12.6, ripgrep 15.2.0, treesitter `e289100…`. Wrapper `--set-default` paths identical to devShell exports.
- Probes M1–M7 all agree. M7: 66 paths, 23.4 MiB (was 22.2) → build.md updated.
- Gate: fmt, prettier, clippy, build pass; `cargo test --workspace` 1178 passed / 0 failed / 10 ignored. `nix flake check --all-systems --no-build` passes; no darwin packages; `nix build .#wolf-gnome` OK; package smoke 6+3 passed.
- Falsifier: dropping `"go"` from `wolfTreesitterLanguages` → `treesitter_highlight.rs:234` panics "staged compiled go parser should exist…"; reverted, green.
- Leftover stale claim (for technical writer): `crates/wolf-ffi/tests/common/mod.rs:251–255` says the Linux tree is the plugin's own `runtime` dir and "the same upstream revision Bazel pins, so both platforms get identical queries" — both false.
