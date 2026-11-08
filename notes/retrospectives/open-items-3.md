# Retrospective: check the tree-sitter language parity mechanically, and close three vendoring ledger rows

Last verified at commit `ce933380faffdd393244fe3e7002c2271cf9df12`.

The task added `crates/wolf-gnome/tests/treesitter_languages.rs`, which fails on Linux when `flake.nix`'s `wolfTreesitterLanguages` and `bazel/treesitter-parsers/BUILD.bazel`'s `parsers_tree` `srcs` differ or fall out of alphabetical order. It also gave the orlando and lighthouse `extensions.bzl` the "bump both together" note, timed `//crates/wolf-ffi:treesitter_highlight` (it stays `small`), and closed three rows of `notes/macos-followups.md`. The work is commits `7d13bb3` and `ce93338`.

It ran once through the flow with no rework: `architect-1`, `test-writer-1`, `coder-1`, the implementation gate at exit 0 on both workers, `technical-writer-1`, the documentation gate at exit 0 on both workers, and `reviewer-1`, which approved. The five invocations cost about $6.83: `architect-1` $1.84 (33 turns), `reviewer-1` $1.61, `technical-writer-1` $1.47, `coder-1` $1.18 (632 s, the longest) and `test-writer-1` $0.73 (12 turns).

## What went well

- A precedent in the same directory made the new test cheap. `crates/wolf-gnome/tests/vendored_pins.rs` already parsed `flake.nix` and Bazel files with std-only line scanners, and the instructions named it. Plan 1 modelled the new file on it, and `test-writer-1` wrote its 352 lines in 12 turns, with `cargo fmt --check` and `clippy -D warnings` clean at the first run on the Linux worker.
- `crates/wolf-gnome` has no `BUILD.bazel`, so Cargo found the new test file and there was no wiring to get wrong (plan 1, `review-1` §3).
- `bazel/ia-writer-mono/extensions.bzl` already carried the two-line `#` note, so `coder-1` copied it word for word into the other two extensions, and `review-1` checked it by comparison alone.
- One Bazel target per test file kept the timing to one command. `coder-1` ran `//crates/wolf-ffi:treesitter_highlight` three times with `--nocache_test_results`, and the slowest run was 1.1 s. `regina on linux` let the macOS coder run all three falsifiers on Linux, where the test runs.

## What did not go well

No step was redone: no rejection, no failed gate, no discrepancy. These things cost effort or left something behind.

- **Plan 1 again asked the test writer to run tests.** Its step 1 said to confirm the test passes with `cargo test`, which the test writer's definition forbids ("Run no test"). `test-writer-1` skipped it and left it to the coder. The architect's definition under which plan 1 was written still says nothing about this.
- **The ledger needed its own parser again.** `architect-1` read rows of `notes/macos-followups.md` by line number (`sed -n 45p`, `65p`), `technical-writer-1` wrote a Python script to rewrite the three cells, and `reviewer-1` wrote another to compare them cell by cell. The file is 90,207 bytes, still flagged `big-file` by the documentation gate.
- **The workspace-path helper now exists three times.** `treesitter_languages.rs` holds private copies of `workspace_path` and `read_workspace_file`, which `vendored_pins.rs` also defines, and `release_profile.rs` has the same logic as `workspace_manifest_path`. Plan 1 chose the copies because `crates/wolf-gnome/tests/common/mod.rs` has no such helper.
- **Plan 1 said no lockfile would move, and `MODULE.bazel.lock` did.** The comment and docstring edits to three `extensions.bzl` files changed their three `bzlTransitiveDigest` values. `coder-1` ran `git diff MODULE.bazel.lock` to prove no pin or `usagesDigest` moved, and `review-1` checked it again. The bump paragraph of `build.md` says a bump rewrites the lockfile, but not that any edit to an extension file does.
- **A stale comment was left in a file the task put off limits.** The comment above `wolfTreesitterLanguages` in `flake.nix` (around line 209) says "edit both sides" and does not name the test that enforces it. `coder-1`, `technical-writer-1` and `review-1` each flagged it.
- **A test README had drifted from the code.** The resolver bullet of `crates/wolf-ffi/tests/README.md` said each resolver reads only its own `WOLF_TEST_*_DIR`. `parsers_dir_or_none` and `nvim_treesitter_dir_or_none` in `crates/wolf-ffi/tests/common/mod.rs` fall back to the devShell's `WOLF_PARSERS_DIR` and `WOLF_NVIM_TREESITTER_DIR`. Plan 1 asked for a sentence in that bullet without noticing, and `technical-writer-1` found and fixed it.
- **The falsifiers that edit `flake.nix` had to wait.** `nix develop` re-reads `flake.nix` on every command, so `coder-1` held F1 and F3 until its background macOS timing runs had finished, so that no scratch edit could leak into them. This is inherent to a test whose subject is `flake.nix`, so there is no recommendation for it.
- In the landed ledger, the 2026-08-04 typography row says the test passes "at this task's commit" without naming one, and the rows tell the falsifier evidence in the past tense (`review-1` §5). The pruning task at the end of the request removes these rows.

## Recommendations

1. **Say in the architect's definition that the test writer runs no test.** This came up again: it is recommendation 3 of [open-items-1](open-items-1.md). Plan 1 in this task repeated the conflict, and `test-writer-1` had to notice and work around it.
2. **Change the ledger's format when it is pruned.** This came up again: it is recommendation 2 of [open-items-2](open-items-2.md). Three agents in this task wrote or ran their own row parsers for three rows.
3. **Name the parity test in `flake.nix`'s comment.** Change the comment above `wolfTreesitterLanguages` so that it names `crates/wolf-gnome/tests/treesitter_languages.rs` in place of "edit both sides". It stays within two lines, since its second line already links `notes/flake.nix.md#treesitter-grammar-set`. Three agents flagged this, and a later editor of the list would see the check at the place they edit.
4. **Move the workspace-path helper into the crate's shared test module.** Put one `workspace_path` and `read_workspace_file` in `crates/wolf-gnome/tests/common/mod.rs`, and have `vendored_pins.rs`, `treesitter_languages.rs` and `release_profile.rs` use it. The next pure parity test would then not add a fourth copy, and the "crate moved relative to the workspace root" message would live in one place.
5. **Say that any edit to an `extensions.bzl` moves `MODULE.bazel.lock`.** Add one sentence to the bump paragraph of `build.md`: a change to a module extension file, a comment included, changes its `bzlTransitiveDigest`, and the change is committed with the edit. Plan 1 predicted no lockfile change, and `coder-1` and `review-1` each had to prove the move harmless.
